//! Overflow page：header(11B) + value + pointers(N*4B) + num_pointers(1B)。

use fkv_codec::put_fixed32;
use fkv_error::{KvError, Result};

use crate::header::{
  OVERFLOW_HEADER_SIZE, OVERFLOW_VALUE_OFFSET, PageType, content_len, new_page, page_type,
  set_checksum, set_content_len, set_page_type,
};

/// 每页最多可放的 value 字节数。
pub fn capacity(page_size: usize, overflow_pointers: u8, end: bool) -> Result<usize> {
  let pointer_bytes = if end {
    usize::from(overflow_pointers) * 4
  } else {
    0
  };
  page_size
    .checked_sub(OVERFLOW_HEADER_SIZE + 1 + pointer_bytes)
    .ok_or(KvError::InvalidArgs)
}

pub fn build(
  page_size: usize,
  value: &[u8],
  pointer_li: &[u32],
  overflow_pointers: u8,
) -> Result<Vec<u8>> {
  if value.len() > capacity(page_size, overflow_pointers, !pointer_li.is_empty())? {
    return Err(KvError::InvalidArgs);
  }
  if pointer_li.len() > usize::from(overflow_pointers) || pointer_li.len() > 255 {
    return Err(KvError::InvalidArgs);
  }
  let mut page = new_page(page_size, PageType::Overflow)?;
  set_page_type(&mut page, PageType::Overflow)?;
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
  set_checksum(&mut page)?;
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

  pub fn value(&self) -> Result<&'a [u8]> {
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
    let count = usize::from(self.num_pointers()?);
    if count == 0 {
      return Ok(Vec::new());
    }
    let start = self
      .page
      .len()
      .checked_sub(1 + count * 4)
      .ok_or(KvError::Corrupted)?;
    let mut pointer_li = Vec::with_capacity(count);
    for idx in 0..count {
      let pos = start + idx * 4;
      pointer_li.push(fkv_codec::get_fixed32(
        self.page.get(pos..pos + 4).ok_or(KvError::Corrupted)?,
      )?);
    }
    Ok(pointer_li)
  }
}
