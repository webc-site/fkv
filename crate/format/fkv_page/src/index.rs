//! Index page：header(15B) + entries + restart array + restart_num(2B) + padding。

use std::cmp::Ordering;

use fkv_codec::{decode_i32_delta, encode_i32_delta, get_varint32, put_varint32, varint32_len};
use fkv_error::{KvError, Result};

use crate::{
  header::{
    INDEX_HEADER_SIZE, INDEX_LEFTMOST_PTR_OFFSET, PageType, content_len, new_page, page_type,
    restart_num, set_checksum, set_content_len, set_page_type,
  },
  restart::{self, cmp_split, common_prefix},
};

#[derive(Debug, Clone)]
pub struct IndexPageBuilder {
  page_size: usize,
  restart_interval: u16,
  page_type: PageType,
  buf: Vec<u8>,
  restart_li: Vec<u16>,
  counter: u16,
  cnt: u16,
  last_key: Vec<u8>,
  last_page_id: u32,
  finished: bool,
}

impl IndexPageBuilder {
  pub fn new(page_size: usize, restart_interval: u16, leaf: bool) -> Result<Self> {
    if restart_interval == 0 {
      return Err(KvError::InvalidArgs);
    }
    let page_type = if leaf {
      PageType::LeafIndex
    } else {
      PageType::NonLeafIndex
    };
    let buf = new_page(page_size, page_type, INDEX_HEADER_SIZE)?;
    Ok(Self {
      page_size,
      restart_interval,
      page_type,
      buf,
      restart_li: vec![INDEX_HEADER_SIZE as u16],
      counter: 0,
      cnt: 0,
      last_key: Vec::new(),
      last_page_id: 0,
      finished: false,
    })
  }

  pub fn is_empty(&self) -> bool {
    self.cnt == 0
  }

  pub fn current_size_estimate(&self) -> usize {
    self.buf.len() + self.restart_li.len() * 2 + 2
  }

  /// 复用缓冲重新开始一页（对齐 C++ `IndexPageBuilder::Reset`）：`buf` 截回 header
  /// 并清零、页类型复原，restart / run 状态全部重置（容量保留）。
  pub fn reset(&mut self) -> Result<()> {
    self.buf.truncate(INDEX_HEADER_SIZE);
    self.buf.fill(0);
    set_page_type(&mut self.buf, self.page_type)?;
    self.restart_li.clear();
    self.restart_li.push(INDEX_HEADER_SIZE as u16);
    self.counter = 0;
    self.cnt = 0;
    self.last_key.clear();
    self.last_page_id = 0;
    self.finished = false;
    Ok(())
  }

  pub fn add(&mut self, key: &[u8], page_id: u32) -> Result<bool> {
    if self.finished {
      return Err(KvError::InvalidArgs);
    }
    if self.is_empty() {
      let dst = self
        .buf
        .get_mut(INDEX_LEFTMOST_PTR_OFFSET..INDEX_LEFTMOST_PTR_OFFSET + 4)
        .ok_or(KvError::EndOfFile)?;
      dst.copy_from_slice(&page_id.to_le_bytes());
      self.cnt = 1;
      return Ok(true);
    }
    self.encode_entry(key, page_id)
  }

  /// 把一段自 restart 边界起、长度 ≤ `restart_interval` 的 entry 编码为**单个 region**
  /// 字节，返回 `buf` 的 entry 区。编码前 `reset()`，故缓冲可跨次复用；调用方取走字节后
  /// 须再 `reset()`（或不再使用）才可继续 `add`。首条 entry 也按 restart 编码
  /// （`shared == 0`、`page_id` 记绝对值），与整页重建时该 region 的字节逐字节一致。
  pub fn encode_region(&mut self, entries: &[(&[u8], u32)]) -> Result<&[u8]> {
    if entries.is_empty() || entries.len() > usize::from(self.restart_interval) {
      return Err(KvError::InvalidArgs);
    }
    self.reset()?;
    for (key, page_id) in entries {
      if !self.encode_entry(key, *page_id)? {
        return Err(KvError::OutOfSpace);
      }
    }
    self.buf.get(INDEX_HEADER_SIZE..).ok_or(KvError::Corrupted)
  }

  /// 编码一条 entry（无 leftmost 特例）：restart 点 `shared=0`、`page_id` 记绝对值。
  fn encode_entry(&mut self, key: &[u8], page_id: u32) -> Result<bool> {
    // 乱序 / 重复 / 非首条空 key 是调用方错误（C++ 侧只有 debug assert），显式报错。
    if self.cnt > 0 && (key.is_empty() || key <= self.last_key.as_slice()) {
      return Err(KvError::InvalidArgs);
    }
    // 对齐 C++ `assert(page_id <= INT32_MAX)`：超域会让 `as i32` 回绕并编出错误 delta。
    if page_id > i32::MAX as u32 {
      return Err(KvError::InvalidArgs);
    }
    let restart = self.counter >= self.restart_interval;
    let last_page_id = if restart { 0 } else { self.last_page_id };
    let shared = if restart {
      0
    } else {
      common_prefix(&self.last_key, key)
    };
    let non_shared = key.len() - shared;
    let delta = page_id as i32 - last_page_id as i32;
    let p_delta = encode_i32_delta(delta).ok_or(KvError::InvalidArgs)?;
    let addition = varint32_len(shared as u32)
      + varint32_len(non_shared as u32)
      + non_shared
      + varint32_len(p_delta)
      + if restart { 2 } else { 0 };
    if self.current_size_estimate() + addition > self.page_size {
      return Ok(false);
    }
    if restart {
      self
        .restart_li
        .push(u16::try_from(self.buf.len()).map_err(|_| KvError::InvalidArgs)?);
      self.counter = 0;
    }
    let mut scratch = [0u8; 10];
    let n = put_varint32(&mut scratch, shared as u32).ok_or(KvError::InvalidArgs)?;
    self.buf.extend_from_slice(&scratch[..n]);
    let n = put_varint32(&mut scratch, non_shared as u32).ok_or(KvError::InvalidArgs)?;
    self.buf.extend_from_slice(&scratch[..n]);
    self.buf.extend_from_slice(&key[shared..]);
    let n = put_varint32(&mut scratch, p_delta).ok_or(KvError::InvalidArgs)?;
    self.buf.extend_from_slice(&scratch[..n]);
    self.last_key.clear();
    self.last_key.extend_from_slice(key);
    self.last_page_id = page_id;
    self.counter += 1;
    self.cnt += 1;
    Ok(true)
  }

  /// 与 C++ `IndexPageBuilder::Finish` 一致：不写 checksum（前 8 字节为 0）。
  pub fn finish_without_checksum(self) -> Result<Vec<u8>> {
    self.finish_inner(false)
  }

  pub fn finish(self) -> Result<Vec<u8>> {
    self.finish_inner(true)
  }

  fn finish_inner(mut self, with_checksum: bool) -> Result<Vec<u8>> {
    self.finished = true;
    if self.cnt == 1 {
      self.restart_li.clear();
    }
    for offset in &self.restart_li {
      let mut dst = [0u8; 2];
      fkv_codec::put_fixed16(&mut dst, *offset);
      self.buf.extend_from_slice(&dst);
    }
    let restart_count = u16::try_from(self.restart_li.len()).map_err(|_| KvError::InvalidArgs)?;
    let mut dst = [0u8; 2];
    fkv_codec::put_fixed16(&mut dst, restart_count);
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

#[derive(Debug, Clone)]
pub struct IndexPage<'a> {
  page: &'a [u8],
}

impl<'a> IndexPage<'a> {
  pub fn new(page: &'a [u8]) -> Result<Self> {
    if !matches!(
      page_type(page)?,
      PageType::LeafIndex | PageType::NonLeafIndex
    ) {
      return Err(KvError::Corrupted);
    }
    Ok(Self { page })
  }

  pub fn is_leaf(&self) -> Result<bool> {
    Ok(page_type(self.page)? == PageType::LeafIndex)
  }

  pub fn leftmost_ptr(&self) -> Result<u32> {
    fkv_codec::get_fixed32(
      self
        .page
        .get(INDEX_LEFTMOST_PTR_OFFSET..)
        .ok_or(KvError::EndOfFile)?,
    )
  }

  pub fn content_len(&self) -> Result<u16> {
    content_len(self.page)
  }
}

#[derive(Debug)]
pub struct IndexPageIter<'a> {
  page: &'a [u8],
  restart_num: u16,
  restart_offset: usize,
  curr_offset: usize,
  curr_restart_idx: u16,
  key: Vec<u8>,
  page_id: u32,
  valid: bool,
}

impl<'a> IndexPageIter<'a> {
  /// 默认入口：结构校验 + restart key 升序校验。
  pub fn new(page: &'a [u8]) -> Result<Self> {
    let iter = Self::new_prevalidated(page)?;
    restart::validate_order(&iter)?;
    Ok(iter)
  }

  /// 结构校验，但**跳过** restart key 升序校验：仅供调用方已用 [`Self::new`]
  /// 校验过同一份页字节的场景（`fkv_btree::IndexNode` 缓存页），避免每次
  /// seek 重复 O(restart_num) 解码。其余头部 / 类型 / 边界校验照做。
  pub fn new_prevalidated(page: &'a [u8]) -> Result<Self> {
    let content = usize::from(content_len(page)?);
    let count = restart_num(page)?;
    let restart_offset = content
      .checked_sub((1 + usize::from(count)) * 2)
      .ok_or(KvError::Corrupted)?;
    if restart_offset < INDEX_HEADER_SIZE {
      return Err(KvError::Corrupted);
    }
    if !matches!(
      page_type(page)?,
      PageType::LeafIndex | PageType::NonLeafIndex
    ) {
      return Err(KvError::Corrupted);
    }
    restart::validate_offsets(page, INDEX_HEADER_SIZE, restart_offset, count)?;
    Ok(Self {
      page,
      restart_num: count,
      restart_offset,
      curr_offset: INDEX_HEADER_SIZE,
      curr_restart_idx: 0,
      key: Vec::new(),
      page_id: 0,
      valid: false,
    })
  }

  pub fn key(&self) -> &[u8] {
    &self.key
  }

  pub fn page_id(&self) -> u32 {
    self.page_id
  }

  pub fn valid(&self) -> bool {
    self.valid
  }

  pub fn advance(&mut self) -> Result<bool> {
    if self.curr_offset >= self.restart_offset {
      self.valid = false;
      return Ok(false);
    }
    let is_restart = self.curr_offset == self.restart_offset_at(self.curr_restart_idx)?;
    let (shared, key_start, key_end) = self.decode_head(self.curr_offset)?;
    let (delta, r3) = get_varint32(
      self
        .page
        .get(key_end..self.restart_offset)
        .ok_or(KvError::EndOfFile)?,
    )?;
    if shared > self.key.len() {
      return Err(KvError::Corrupted);
    }
    self.key.truncate(shared);
    self.key.extend_from_slice(&self.page[key_start..key_end]);
    let delta = decode_i32_delta(delta);
    self.page_id = if is_restart {
      delta as u32
    } else {
      (self.page_id as i32).wrapping_add(delta) as u32
    };
    self.curr_offset = key_end + r3;
    if self.curr_restart_idx + 1 < self.restart_num
      && self.curr_offset >= self.restart_offset_at(self.curr_restart_idx + 1)?
    {
      self.curr_restart_idx += 1;
    }
    self.valid = true;
    Ok(true)
  }

  pub fn reset(&mut self) {
    self.curr_offset = INDEX_HEADER_SIZE;
    self.curr_restart_idx = 0;
    self.key.clear();
    self.page_id = 0;
    self.valid = false;
  }

  /// 第一个 key `>= target` 的 entry（ceiling）：restart 二分 + 区间内线性。
  ///
  /// 注意：C++ `IndexPageIter::Seek` 是 **floor** 语义（最后一个 `key <= target`，
  /// 且 target 小于页内首 key 时定位到 leftmost 虚拟 entry）。Rust 的 index 下降
  /// 必须用 [`Self::seek_floor`] + [`IndexPage::leftmost_ptr`] 组合，不要用本方法
  /// 做树下降。
  pub fn seek(&mut self, target: &[u8]) -> Result<bool> {
    restart::seek(self, target)
  }

  /// 最后一个 key `<= target` 的 entry（floor）：restart 二分 + 单遍推进。
  ///
  /// 与 C++ `IndexPageIter::Seek` 的定位语义一致；但 target 小于页内首 key 时
  /// 返回 `false`（C++ 会给出 leftmost 虚拟 entry），调用方需自行回退到
  /// leftmost 指针。
  pub fn seek_floor(&mut self, target: &[u8]) -> Result<bool> {
    restart::seek_floor(self, target)
  }

  /// 解码 entry 头：`(shared, key_start, key_end)`，key 尾部已做越界检查。
  fn decode_head(&self, offset: usize) -> Result<(usize, usize, usize)> {
    let bytes = self
      .page
      .get(offset..self.restart_offset)
      .ok_or(KvError::EndOfFile)?;
    let (shared, r1) = get_varint32(bytes)?;
    let (non_shared, r2) = get_varint32(bytes.get(r1..).ok_or(KvError::Corrupted)?)?;
    let key_start = r1 + r2;
    let key_end = key_start + non_shared as usize;
    if key_end > bytes.len() {
      return Err(KvError::Corrupted);
    }
    Ok((shared as usize, offset + key_start, offset + key_end))
  }

  fn restart_offset_at(&self, index: u16) -> Result<usize> {
    restart::offset_at(self.page, self.restart_offset, index)
  }
}

impl restart::Cursor for IndexPageIter<'_> {
  fn restart_cnt(&self) -> u16 {
    self.restart_num
  }

  fn no_entry(&self) -> bool {
    self.restart_num == 0 || self.restart_offset <= INDEX_HEADER_SIZE
  }

  fn restart_key(&self, idx: u16) -> Result<&[u8]> {
    let (shared, start, end) = self.decode_head(self.restart_offset_at(idx)?)?;
    if shared != 0 {
      return Err(KvError::Corrupted);
    }
    Ok(&self.page[start..end])
  }

  fn seek_to_restart(&mut self, idx: u16) -> Result<()> {
    // restart 点 shared=0、page_id 为绝对值，清空前缀即可独立解码。
    self.curr_offset = self.restart_offset_at(idx)?;
    self.curr_restart_idx = idx;
    self.key.clear();
    self.page_id = 0;
    self.valid = false;
    Ok(())
  }

  fn step(&mut self) -> Result<bool> {
    self.advance()
  }

  fn cur_key(&self) -> &[u8] {
    &self.key
  }

  fn invalidate(&mut self) {
    self.curr_offset = self.restart_offset;
    self.valid = false;
  }

  fn peek_cmp(&self, target: &[u8]) -> Result<Option<Ordering>> {
    if self.curr_offset >= self.restart_offset {
      return Ok(None);
    }
    let (shared, start, end) = self.decode_head(self.curr_offset)?;
    let head = self.key.get(..shared).ok_or(KvError::Corrupted)?;
    Ok(Some(cmp_split(head, &self.page[start..end], target)))
  }
}
