//! Overflow page：header(11B) + value + pointers(N*4B) + num_pointers(1B)。

use fkv_codec::{get_fixed32, put_fixed32};
use fkv_error::{KvError, Result};

use crate::header::{
  OVERFLOW_HEADER_SIZE, OVERFLOW_VALUE_OFFSET, PageType, content_len, new_page, page_type,
  set_checksum, set_content_len,
};

/// 对齐 C++ `KvOption::overflow_pointers` 默认值：一批最多 16 个页。
pub const DEFAULT_OVERFLOW_POINTERS: u8 = 16;

/// 每页最多可放的 value 字节数。
///
/// `with_pointers` 为 true 时为页尾指针数组预留空间（链中页带指针，尾页不带）。
pub fn capacity(page_size: usize, overflow_pointers: u8, with_pointers: bool) -> Result<usize> {
  let pointer_bytes = if with_pointers {
    usize::from(overflow_pointers) * 4
  } else {
    0
  };
  page_size
    .checked_sub(OVERFLOW_HEADER_SIZE + 1 + pointer_bytes)
    .ok_or(KvError::InvalidArgs)
}

/// 首批 overflow 页的指针个数：`min(ceil(value_len / page_cap), overflow_pointers)`。
///
/// leaf entry 里存的就是这批指针，长度决定 entry 的 value 字节数。
pub fn first_pointer_count(
  value_len: usize,
  page_size: usize,
  overflow_pointers: u8,
) -> Result<usize> {
  let page_cap = capacity(page_size, overflow_pointers, false)?;
  if page_cap == 0 || overflow_pointers == 0 {
    return Err(KvError::InvalidArgs);
  }
  Ok(
    value_len
      .div_ceil(page_cap)
      .min(usize::from(overflow_pointers)),
  )
}

/// leaf entry 的指针数组编码：每个 PageId 小端 4 字节（对齐 C++ `PutFixed32`）。
pub fn encode_pointers(pointer_li: &[u32]) -> Vec<u8> {
  let mut out = Vec::with_capacity(pointer_li.len() * 4);
  for pointer in pointer_li {
    out.extend_from_slice(&pointer.to_le_bytes());
  }
  out
}

/// 解码 leaf entry / overflow 页尾部指针数组；长度非 4 的倍数视为损坏。
pub fn decode_pointers(encoded: &[u8]) -> Result<Vec<u32>> {
  if encoded.len() % 4 != 0 {
    return Err(KvError::Corrupted);
  }
  let mut pointer_li = Vec::with_capacity(encoded.len() / 4);
  for chunk in encoded.as_chunks::<4>().0 {
    pointer_li.push(get_fixed32(chunk)?);
  }
  Ok(pointer_li)
}

pub fn build(
  page_size: usize,
  value: &[u8],
  pointer_li: &[u32],
  overflow_pointers: u8,
) -> Result<Vec<u8>> {
  build_inner(page_size, value, pointer_li, overflow_pointers, true)
}

/// 与 C++ `OverflowPage` 构造一致：不写 checksum，供格式差分。
pub fn build_without_checksum(
  page_size: usize,
  value: &[u8],
  pointer_li: &[u32],
  overflow_pointers: u8,
) -> Result<Vec<u8>> {
  build_inner(page_size, value, pointer_li, overflow_pointers, false)
}

fn build_inner(
  page_size: usize,
  value: &[u8],
  pointer_li: &[u32],
  overflow_pointers: u8,
  with_checksum: bool,
) -> Result<Vec<u8>> {
  if value.len() > capacity(page_size, overflow_pointers, !pointer_li.is_empty())? {
    return Err(KvError::InvalidArgs);
  }
  if pointer_li.len() > usize::from(overflow_pointers) || pointer_li.len() > 255 {
    return Err(KvError::InvalidArgs);
  }
  let mut page = new_page(page_size, PageType::Overflow, page_size)?;
  let value_len = u16::try_from(value.len()).map_err(|_| KvError::InvalidArgs)?;
  set_content_len(&mut page, value_len)?;
  page[OVERFLOW_VALUE_OFFSET..OVERFLOW_VALUE_OFFSET + value.len()].copy_from_slice(value);

  *page.last_mut().ok_or(KvError::EndOfFile)? = pointer_li.len() as u8;
  if !pointer_li.is_empty() {
    let mut pos = page_size - 1 - pointer_li.len() * 4;
    for pointer in pointer_li {
      let dst = page.get_mut(pos..pos + 4).ok_or(KvError::EndOfFile)?;
      put_fixed32(dst, *pointer).ok_or(KvError::EndOfFile)?;
      pos += 4;
    }
  }
  if with_checksum {
    set_checksum(&mut page)?;
  }
  Ok(page)
}

#[derive(Debug, Clone)]
pub struct OverflowPage<'a> {
  page: &'a [u8],
}

impl<'a> OverflowPage<'a> {
  pub fn new(page: &'a [u8]) -> Result<Self> {
    if page_type(page)? != PageType::Overflow {
      return Err(KvError::Corrupted);
    }
    Ok(Self { page })
  }

  /// 指针区起点；同时校验 value 区不侵入指针区。
  ///
  /// 损坏页可通过伪造 `content_len` / 指针个数把指针当 value 读出，或让
  /// 指针区反向覆盖 value 区。两者都以 `Corrupted` 拒绝。
  fn pointer_start(&self) -> Result<usize> {
    let count = usize::from(self.num_pointers()?);
    let start = self
      .page
      .len()
      .checked_sub(1 + count * 4)
      .ok_or(KvError::Corrupted)?;
    let value_end = OVERFLOW_VALUE_OFFSET + usize::from(content_len(self.page)?);
    if value_end > start {
      return Err(KvError::Corrupted);
    }
    Ok(start)
  }

  pub fn value(&self) -> Result<&'a [u8]> {
    self.pointer_start()?;
    let len = usize::from(content_len(self.page)?);
    self
      .page
      .get(OVERFLOW_VALUE_OFFSET..OVERFLOW_VALUE_OFFSET + len)
      .ok_or(KvError::Corrupted)
  }

  pub fn num_pointers(&self) -> Result<u8> {
    self.page.last().copied().ok_or(KvError::EndOfFile)
  }

  pub fn pointer_li(&self) -> Result<Vec<u32>> {
    let start = self.pointer_start()?;
    if start == self.page.len() - 1 {
      return Ok(Vec::new());
    }
    decode_pointers(
      self
        .page
        .get(start..self.page.len() - 1)
        .ok_or(KvError::Corrupted)?,
    )
  }
}
