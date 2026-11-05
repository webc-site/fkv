//! Page region 原语：按 restart 数组把页切成 region（连续 entry 的原始字节段），
//! 供 index 页重建时直拷未变更的 region，替代逐条 varint 重编码。
//!
//! region 边界即 restart 点，该处 `shared == 0`、entry 编码自包含，故 region 直拷与
//! 逐条重编码逐字节一致。对齐 C++ `PageRegionIter` / `FastPageBuilder`。

use fkv_codec::{get_fixed16, get_fixed32, get_varint32, put_fixed16};
use fkv_error::{KvError, Result};

use crate::{
  header::{
    DATA_HEADER_SIZE, INDEX_HEADER_SIZE, INDEX_LEFTMOST_PTR_OFFSET, PageType, content_len,
    data_restart_offset, new_page, page_type, restart_num, set_checksum, set_content_len,
  },
  restart,
};

/// 页头大小（data / index）；其余页类型无 restart 数组，不支持 region 切分。
fn header_size(page: &[u8]) -> Result<usize> {
  match page_type(page)? {
    PageType::Data => Ok(DATA_HEADER_SIZE),
    PageType::LeafIndex | PageType::NonLeafIndex => Ok(INDEX_HEADER_SIZE),
    _ => Err(KvError::Corrupted),
  }
}

/// 按 restart 数组把页切成 region 的迭代器。
///
/// 构造时完成全部校验（页类型 / 边界 / offset 严格递增 / 首 offset 等于页头），
/// 之后每个 `next` 都是 O(1) 且不可能失败。
/// 空页（无 entry）region 数为 0，空 data 页的 restart 数组只含页头 offset 亦归一为 0。
#[derive(Debug)]
pub struct PageRegionIter<'a> {
  page: &'a [u8],
  /// restart 数组字节（长度恰为 `2 * restart_num`）。
  restart_li: &'a [u8],
  /// 末 region 的结束边界，即 restart 数组起始偏移。
  restart_offset: usize,
  region_cnt: u16,
  idx: u16,
}

impl<'a> PageRegionIter<'a> {
  pub fn new(page: &'a [u8]) -> Result<Self> {
    let header = header_size(page)?;
    let content = usize::from(content_len(page)?);
    if content > page.len() || content < header {
      return Err(KvError::Corrupted);
    }
    let count = restart_num(page)?;
    let restart_offset = data_restart_offset(page)?;
    restart::validate_offsets(page, header, restart_offset, count)?;
    let restart_li = page
      .get(restart_offset..content)
      .ok_or(KvError::Corrupted)?;
    // 无 entry 的页（restart 区紧贴页头）：index 空页 count=0，data 空页 count=1。
    let region_cnt = if restart_offset == header { 0 } else { count };
    Ok(Self {
      page,
      restart_li,
      restart_offset,
      region_cnt,
      idx: 0,
    })
  }

  pub fn region_cnt(&self) -> u16 {
    self.region_cnt
  }

  /// 第 `idx` 个 region 起始 offset；构造时已校验，越界只可能由内部调用错误触发。
  fn offset_at(&self, idx: u16) -> usize {
    offset_bytes(self.restart_li, idx)
      .ok()
      .and_then(|bytes| get_fixed16(bytes).ok().map(usize::from))
      .unwrap_or(self.restart_offset)
  }
}

/// restart 数组第 `idx` 个 offset 的 2 字节。
fn offset_bytes(restart_li: &[u8], idx: u16) -> Result<&[u8]> {
  let start = 2 * usize::from(idx);
  restart_li.get(start..start + 2).ok_or(KvError::Corrupted)
}

/// 走一遍 region 字节，返回其 entry 条数。
///
/// region 自 restart 边界起，每条 entry = `shared` varint + `non_shared` varint
/// + key(`non_shared` 字节) + `page_id` delta varint；只跳步，不解析 key/page_id。
///
/// 用于校验旧页 region 的 entry 跨度是否与当前 `restart_interval` 对齐。
pub fn region_entry_cnt(region: &[u8]) -> Result<u16> {
  let mut pos = 0usize;
  let mut cnt = 0u16;
  while pos < region.len() {
    let (_, shared_len) = get_varint32(region.get(pos..).ok_or(KvError::Corrupted)?)?;
    let (non_shared, non_shared_len) =
      get_varint32(region.get(pos + shared_len..).ok_or(KvError::Corrupted)?)?;
    pos = pos
      .checked_add(shared_len + non_shared_len + non_shared as usize)
      .ok_or(KvError::Corrupted)?;
    let (_, delta_len) = get_varint32(region.get(pos..).ok_or(KvError::Corrupted)?)?;
    pos = pos.checked_add(delta_len).ok_or(KvError::Corrupted)?;
    cnt = cnt.checked_add(1).ok_or(KvError::Corrupted)?;
  }
  Ok(cnt)
}

impl<'a> Iterator for PageRegionIter<'a> {
  type Item = &'a [u8];

  fn next(&mut self) -> Option<&'a [u8]> {
    if self.idx >= self.region_cnt {
      return None;
    }
    let start = self.offset_at(self.idx);
    let end = if self.idx + 1 < self.region_cnt {
      self.offset_at(self.idx + 1)
    } else {
      self.restart_offset
    };
    self.idx += 1;
    self.page.get(start..end)
  }
}

/// index 页快速重建器：把 region 原始字节直接拼进页，避免逐条 varint 重编码。
///
/// 用法：`from_page(old)` 复用旧页 header，或 `new(page_size, leaf)` + `set_leftmost`，
/// 然后依次 `add_region`；单次放不下返回 `false`（对齐 C++ `FastPageBuilder::AddRegion`）。
#[derive(Debug)]
pub struct FastIndexPageBuilder {
  page_size: usize,
  buf: Vec<u8>,
  /// 各 region 的起始 offset（即新页 restart 数组）。
  region_offset_li: Vec<u16>,
}

impl FastIndexPageBuilder {
  pub fn new(page_size: usize, leaf: bool) -> Result<Self> {
    let page_type = if leaf {
      PageType::LeafIndex
    } else {
      PageType::NonLeafIndex
    };
    let buf = new_page(page_size, page_type, INDEX_HEADER_SIZE)?;
    Ok(Self {
      page_size,
      buf,
      region_offset_li: Vec::new(),
    })
  }

  /// 复用旧 index 页的页类型与 header 里的 leftmost 指针。`old` 须为整页缓冲。
  pub fn from_page(old: &[u8]) -> Result<Self> {
    let leaf = match page_type(old)? {
      PageType::LeafIndex => true,
      PageType::NonLeafIndex => false,
      _ => return Err(KvError::Corrupted),
    };
    let mut builder = Self::new(old.len(), leaf)?;
    let leftmost = get_fixed32(
      old
        .get(INDEX_LEFTMOST_PTR_OFFSET..)
        .ok_or(KvError::EndOfFile)?,
    )?;
    builder.set_leftmost(leftmost)?;
    Ok(builder)
  }

  pub fn set_leftmost(&mut self, page_id: u32) -> Result<()> {
    let dst = self
      .buf
      .get_mut(INDEX_LEFTMOST_PTR_OFFSET..INDEX_LEFTMOST_PTR_OFFSET + 4)
      .ok_or(KvError::EndOfFile)?;
    dst.copy_from_slice(&page_id.to_le_bytes());
    Ok(())
  }

  pub fn current_size(&self) -> usize {
    self.buf.len() + self.region_offset_li.len() * 2 + 2
  }

  /// 直拷一个 region。只有 restart 点（`shared == 0`）才能作为 region 边界，
  /// 否则新页 restart 语义会被破坏，返回 `Corrupted`。
  pub fn add_region(&mut self, region: &[u8]) -> Result<bool> {
    if region.is_empty() {
      return Err(KvError::InvalidArgs);
    }
    let (shared, _) = get_varint32(region)?;
    if shared != 0 {
      return Err(KvError::Corrupted);
    }
    if self.current_size() + region.len() + 2 > self.page_size {
      return Ok(false);
    }
    self
      .region_offset_li
      .push(u16::try_from(self.buf.len()).map_err(|_| KvError::InvalidArgs)?);
    self.buf.extend_from_slice(region);
    Ok(true)
  }

  /// 与 C++ `FastPageBuilder::Finish` 一致：不写 checksum，供逐字节差分。
  pub fn finish_without_checksum(self) -> Result<Vec<u8>> {
    self.finish_inner(false)
  }

  pub fn finish(self) -> Result<Vec<u8>> {
    self.finish_inner(true)
  }

  fn finish_inner(mut self, with_checksum: bool) -> Result<Vec<u8>> {
    for offset in &self.region_offset_li {
      let mut dst = [0u8; 2];
      put_fixed16(&mut dst, *offset);
      self.buf.extend_from_slice(&dst);
    }
    let count = u16::try_from(self.region_offset_li.len()).map_err(|_| KvError::InvalidArgs)?;
    let mut dst = [0u8; 2];
    put_fixed16(&mut dst, count);
    self.buf.extend_from_slice(&dst);
    let content_size = u16::try_from(self.buf.len()).map_err(|_| KvError::InvalidArgs)?;
    set_content_len(&mut self.buf, content_size)?;
    self.buf.resize(self.page_size, 0);
    if with_checksum {
      set_checksum(&mut self.buf)?;
    }
    Ok(self.buf)
  }
}
