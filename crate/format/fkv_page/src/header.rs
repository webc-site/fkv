//! 页头、页类型与 checksum。

use fkv_codec::{CHECKSUM_BYTES, checksum_excluding_prefix, get_fixed16, put_fixed16, put_fixed64};
use fkv_error::{KvError, Result};

pub(crate) const PAGE_TYPE_OFFSET: usize = CHECKSUM_BYTES;
pub(crate) const CONTENT_LEN_OFFSET: usize = PAGE_TYPE_OFFSET + 1;

pub const DATA_HEADER_SIZE: usize = CONTENT_LEN_OFFSET + 2 + 4 + 4;
pub const INDEX_HEADER_SIZE: usize = CONTENT_LEN_OFFSET + 2 + 4;
pub const OVERFLOW_HEADER_SIZE: usize = CONTENT_LEN_OFFSET + 2;

pub(crate) const DATA_PREV_PAGE_OFFSET: usize = CONTENT_LEN_OFFSET + 2;
pub(crate) const DATA_NEXT_PAGE_OFFSET: usize = DATA_PREV_PAGE_OFFSET + 4;
pub(crate) const INDEX_LEFTMOST_PTR_OFFSET: usize = CONTENT_LEN_OFFSET + 2;
pub(crate) const OVERFLOW_VALUE_OFFSET: usize = OVERFLOW_HEADER_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PageType {
  NonLeafIndex = 0,
  LeafIndex = 1,
  Data = 2,
  Overflow = 3,
  Deleted = 255,
}

impl TryFrom<u8> for PageType {
  type Error = KvError;

  fn try_from(value: u8) -> Result<Self> {
    match value {
      0 => Ok(Self::NonLeafIndex),
      1 => Ok(Self::LeafIndex),
      2 => Ok(Self::Data),
      3 => Ok(Self::Overflow),
      255 => Ok(Self::Deleted),
      _ => Err(KvError::Corrupted),
    }
  }
}

pub fn page_type(page: &[u8]) -> Result<PageType> {
  let value = *page.get(PAGE_TYPE_OFFSET).ok_or(KvError::EndOfFile)?;
  PageType::try_from(value)
}

pub fn set_page_type(page: &mut [u8], page_type: PageType) -> Result<()> {
  *page.get_mut(PAGE_TYPE_OFFSET).ok_or(KvError::EndOfFile)? = page_type as u8;
  Ok(())
}

pub fn content_len(page: &[u8]) -> Result<u16> {
  get_fixed16(page.get(CONTENT_LEN_OFFSET..).ok_or(KvError::EndOfFile)?)
}

pub(crate) fn set_content_len(page: &mut [u8], len: u16) -> Result<()> {
  let dst = page
    .get_mut(CONTENT_LEN_OFFSET..)
    .ok_or(KvError::EndOfFile)?;
  put_fixed16(dst, len).ok_or(KvError::EndOfFile)?;
  Ok(())
}

pub fn restart_num(page: &[u8]) -> Result<u16> {
  let end = usize::from(content_len(page)?);
  let start = end.checked_sub(2).ok_or(KvError::Corrupted)?;
  get_fixed16(page.get(start..end).ok_or(KvError::Corrupted)?)
}

pub fn data_restart_offset(page: &[u8]) -> Result<usize> {
  let end = usize::from(content_len(page)?);
  let count = usize::from(restart_num(page)?);
  end.checked_sub((1 + count) * 2).ok_or(KvError::Corrupted)
}

pub fn set_checksum(page: &mut [u8]) -> Result<()> {
  if page.len() < CHECKSUM_BYTES {
    return Err(KvError::EndOfFile);
  }
  let sum = checksum_excluding_prefix(page);
  put_fixed64(page, sum).ok_or(KvError::EndOfFile)?;
  Ok(())
}

pub fn validate_checksum(page: &[u8]) -> Result<bool> {
  let stored = fkv_codec::get_fixed64(page).map_err(|_| KvError::EndOfFile)?;
  Ok(stored == checksum_excluding_prefix(page))
}

/// 分配页缓冲：容量 `page_size`，前 `len` 字节置零并设页类型。
/// 尾部零填充延迟到 `finish` 的 `resize(page_size, 0)`，避免构造时整页 memset。
pub fn new_page(page_size: usize, page_type: PageType, len: usize) -> Result<Vec<u8>> {
  if page_size < DATA_HEADER_SIZE || page_size > usize::from(u16::MAX) || len > page_size {
    return Err(KvError::InvalidArgs);
  }
  let mut page = Vec::with_capacity(page_size);
  page.resize(len, 0);
  set_page_type(&mut page, page_type)?;
  Ok(page)
}
