//! Data page：header(19B) + entries + restart array + restart_num(2B) + padding。

use fkv_codec::{
  decode_i64_delta, encode_i64_delta, get_varint32, get_varint64, put_fixed32, put_varint32,
  put_varint64, varint32_len, varint64_len,
};
use fkv_error::{KvError, Result};

use crate::header::{
  DATA_HEADER_SIZE, DATA_NEXT_PAGE_OFFSET, DATA_PREV_PAGE_OFFSET, PageType, content_len,
  data_restart_offset, new_page, page_type, restart_num, set_checksum, set_content_len,
  set_page_type,
};

pub const VAL_LEN_BITS: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompressionType {
  None = 0,
  Dictionary = 1,
  Standalone = 2,
}

impl CompressionType {
  pub fn from_bits(bits: u8) -> Result<Self> {
    match bits {
      0 => Ok(Self::None),
      1 => Ok(Self::Dictionary),
      2 => Ok(Self::Standalone),
      _ => Err(KvError::Corrupted),
    }
  }
}

#[derive(Debug, Clone)]
pub struct DataPageBuilder {
  page_size: usize,
  restart_interval: u16,
  buf: Vec<u8>,
  restart_li: Vec<u16>,
  counter: u16,
  cnt: u16,
  last_key: Vec<u8>,
  last_ts: i64,
  finished: bool,
}

impl DataPageBuilder {
  pub fn new(page_size: usize, restart_interval: u16) -> Result<Self> {
    if restart_interval == 0 {
      return Err(KvError::InvalidArgs);
    }
    let mut buf = new_page(page_size, PageType::Data)?;
    buf.truncate(DATA_HEADER_SIZE);
    Ok(Self {
      page_size,
      restart_interval,
      buf,
      restart_li: vec![DATA_HEADER_SIZE as u16],
      counter: 0,
      cnt: 0,
      last_key: Vec::new(),
      last_ts: 0,
      finished: false,
    })
  }

  pub fn is_empty(&self) -> bool {
    self.cnt == 0
  }

  pub fn current_size_estimate(&self) -> usize {
    self.buf.len() + self.restart_li.len() * 2 + 2
  }

  #[allow(clippy::too_many_arguments)]
  pub fn add(
    &mut self,
    key: &[u8],
    value: &[u8],
    overflow: bool,
    ts: u64,
    expire_ts: u64,
    compression: CompressionType,
    large_value: bool,
  ) -> Result<bool> {
    if self.finished {
      return Err(KvError::InvalidArgs);
    }
    if !self.last_key.is_empty() && key <= self.last_key.as_slice() {
      return Ok(false);
    }
    if large_value && overflow {
      return Err(KvError::InvalidArgs);
    }

    let value_len = u64::try_from(value.len()).map_err(|_| KvError::InvalidArgs)?;
    if value_len > u64::from(u32::MAX >> VAL_LEN_BITS) {
      return Err(KvError::InvalidArgs);
    }
    let mut stored_val_len = value_len << VAL_LEN_BITS;
    if overflow {
      stored_val_len |= 1;
    }
    if large_value {
      stored_val_len |= 0b11 << 2;
    } else {
      stored_val_len |= u64::from(compression as u8) << 2;
    }
    if expire_ts != 0 {
      stored_val_len |= 1 << 1;
    }
    let stored_val_len = u32::try_from(stored_val_len).map_err(|_| KvError::InvalidArgs)?;

    let restart = self.counter >= self.restart_interval;
    let last_ts = if restart { 0 } else { self.last_ts };
    let shared = if restart {
      0
    } else {
      common_prefix(&self.last_key, key)
    };
    let non_shared = key.len() - shared;
    let ts_delta = encode_i64_delta(ts as i64 - last_ts);
    let addition = varint32_len(shared as u32)
      + varint32_len(non_shared as u32)
      + varint32_len(stored_val_len)
      + non_shared
      + value.len()
      + usize::from(expire_ts != 0) * varint64_len(expire_ts)
      + varint64_len(ts_delta)
      + if restart { 2 } else { 0 };

    if self.current_size_estimate() + addition > self.page_size {
      return Ok(false);
    }

    if restart {
      self
        .restart_li
        .push(u16::try_from(self.buf.len()).map_err(|_| KvError::InvalidArgs)?);
      self.counter = 0;
      self.last_ts = 0;
    }

    let mut scratch = [0u8; 10];
    let n = put_varint32(&mut scratch, shared as u32).ok_or(KvError::InvalidArgs)?;
    self.buf.extend_from_slice(&scratch[..n]);
    let n = put_varint32(&mut scratch, non_shared as u32).ok_or(KvError::InvalidArgs)?;
    self.buf.extend_from_slice(&scratch[..n]);
    let n = put_varint32(&mut scratch, stored_val_len).ok_or(KvError::InvalidArgs)?;
    self.buf.extend_from_slice(&scratch[..n]);
    self.buf.extend_from_slice(&key[shared..]);
    self.buf.extend_from_slice(value);
    if expire_ts != 0 {
      let n = put_varint64(&mut scratch, expire_ts).ok_or(KvError::InvalidArgs)?;
      self.buf.extend_from_slice(&scratch[..n]);
    }
    let n = put_varint64(&mut scratch, ts_delta).ok_or(KvError::InvalidArgs)?;
    self.buf.extend_from_slice(&scratch[..n]);

    self.last_key.clear();
    self.last_key.extend_from_slice(key);
    self.last_ts = ts as i64;
    self.counter += 1;
    self.cnt += 1;
    Ok(true)
  }

  pub fn finish(mut self) -> Result<Vec<u8>> {
    self.finished = true;
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
    set_checksum(&mut self.buf)?;
    Ok(self.buf)
  }
}

#[derive(Debug, Clone)]
pub struct DataPage<'a> {
  page: &'a [u8],
}

impl<'a> DataPage<'a> {
  pub fn new(page: &'a [u8]) -> Result<Self> {
    if page_type(page)? != PageType::Data {
      return Err(KvError::Corrupted);
    }
    Ok(Self { page })
  }

  pub fn content_len(&self) -> Result<u16> {
    content_len(self.page)
  }

  pub fn prev_page_id(&self) -> Result<u32> {
    fkv_codec::get_fixed32(
      self
        .page
        .get(DATA_PREV_PAGE_OFFSET..)
        .ok_or(KvError::EndOfFile)?,
    )
  }

  pub fn next_page_id(&self) -> Result<u32> {
    fkv_codec::get_fixed32(
      self
        .page
        .get(DATA_NEXT_PAGE_OFFSET..)
        .ok_or(KvError::EndOfFile)?,
    )
  }
}

#[derive(Debug)]
pub struct DataPageIter<'a> {
  page: &'a [u8],
  restart_num: u16,
  restart_offset: usize,
  curr_offset: usize,
  curr_restart_idx: u16,
  key: Vec<u8>,
  value_range: (usize, usize),
  timestamp: u64,
  expire_ts: u64,
  overflow: bool,
  large_value: bool,
  compression: CompressionType,
  valid: bool,
}

impl<'a> DataPageIter<'a> {
  pub fn new(page: &'a [u8]) -> Result<Self> {
    if page_type(page)? != PageType::Data {
      return Err(KvError::Corrupted);
    }
    let restart_offset = data_restart_offset(page)?;
    if restart_offset < DATA_HEADER_SIZE || restart_offset > page.len() {
      return Err(KvError::Corrupted);
    }
    Ok(Self {
      page,
      restart_num: restart_num(page)?,
      restart_offset,
      curr_offset: DATA_HEADER_SIZE,
      curr_restart_idx: 0,
      key: Vec::new(),
      value_range: (0, 0),
      timestamp: 0,
      expire_ts: 0,
      overflow: false,
      large_value: false,
      compression: CompressionType::None,
      valid: false,
    })
  }

  pub fn key(&self) -> &[u8] {
    &self.key
  }

  pub fn value(&self) -> &[u8] {
    &self.page[self.value_range.0..self.value_range.1]
  }

  pub fn timestamp(&self) -> u64 {
    self.timestamp
  }

  pub fn expire_ts(&self) -> u64 {
    self.expire_ts
  }

  pub fn is_overflow(&self) -> bool {
    self.overflow
  }

  pub fn is_large_value(&self) -> bool {
    self.large_value
  }

  pub fn compression(&self) -> CompressionType {
    self.compression
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
    let (shared, non_shared, value_len, flags, mut pos) =
      decode_entry(self.page, self.curr_offset)?;
    let key_start = pos;
    pos += non_shared;
    let value_start = pos;
    pos += value_len;
    if pos > self.restart_offset {
      return Err(KvError::Corrupted);
    }
    if shared > self.key.len() {
      return Err(KvError::Corrupted);
    }
    self.key.truncate(shared);
    self.key.extend_from_slice(
      self
        .page
        .get(key_start..key_start + non_shared)
        .ok_or(KvError::Corrupted)?,
    );
    self.value_range = (value_start, value_start + value_len);
    self.overflow = flags & 1 != 0;
    self.expire_ts = if flags & 2 != 0 {
      let (value, read) = get_varint64(&self.page[pos..self.restart_offset])?;
      pos += read;
      value
    } else {
      0
    };
    let (delta, read) = get_varint64(&self.page[pos..self.restart_offset])?;
    pos += read;
    let delta = decode_i64_delta(delta);
    self.timestamp = if is_restart {
      delta as u64
    } else {
      (self.timestamp as i64 + delta) as u64
    };
    self.compression = if flags >> 2 == 0b11 {
      self.large_value = true;
      CompressionType::None
    } else {
      self.large_value = false;
      CompressionType::from_bits(flags >> 2)?
    };
    self.curr_offset = pos;
    if self.curr_restart_idx + 1 < self.restart_num
      && pos >= self.restart_offset_at(self.curr_restart_idx + 1)?
    {
      self.curr_restart_idx += 1;
    }
    self.valid = true;
    Ok(true)
  }

  pub fn reset(&mut self) {
    self.curr_offset = DATA_HEADER_SIZE;
    self.curr_restart_idx = 0;
    self.key.clear();
    self.value_range = (0, 0);
    self.timestamp = 0;
    self.expire_ts = 0;
    self.overflow = false;
    self.large_value = false;
    self.compression = CompressionType::None;
    self.valid = false;
  }

  pub fn seek(&mut self, target: &[u8]) -> Result<bool> {
    self.reset();
    while self.advance()? {
      if self.key() >= target {
        return Ok(true);
      }
    }
    Ok(false)
  }

  pub fn seek_floor(&mut self, target: &[u8]) -> Result<bool> {
    self.reset();
    let mut last_key = None;
    while self.advance()? {
      if self.key() > target {
        break;
      }
      last_key = Some(self.key().to_vec());
    }
    let Some(last_key) = last_key else {
      self.valid = false;
      return Ok(false);
    };
    self.reset();
    while self.advance()? {
      if self.key() == last_key.as_slice() {
        return Ok(true);
      }
    }
    self.valid = false;
    Ok(false)
  }

  fn restart_offset_at(&self, index: u16) -> Result<usize> {
    let start = self
      .restart_offset
      .checked_add(usize::from(index) * 2)
      .ok_or(KvError::Corrupted)?;
    let value = fkv_codec::get_fixed16(self.page.get(start..start + 2).ok_or(KvError::Corrupted)?)?;
    Ok(usize::from(value))
  }
}

fn decode_entry(page: &[u8], offset: usize) -> Result<(usize, usize, usize, u8, usize)> {
  let limit = page.len();
  let bytes = page.get(offset..limit).ok_or(KvError::EndOfFile)?;
  if bytes.len() < 3 {
    return Err(KvError::EndOfFile);
  }
  let (shared, non_shared, stored_val_len, pos) =
    if bytes[0] < 0x80 && bytes[1] < 0x80 && bytes[2] < 0x80 {
      (
        usize::from(bytes[0]),
        usize::from(bytes[1]),
        u32::from(bytes[2]),
        offset + 3,
      )
    } else {
      let (shared, r1) = get_varint32(bytes)?;
      let (non_shared, r2) = get_varint32(&bytes[r1..])?;
      let (stored_val_len, r3) = get_varint32(&bytes[r1 + r2..])?;
      (
        shared as usize,
        non_shared as usize,
        stored_val_len,
        offset + r1 + r2 + r3,
      )
    };
  let flags = (stored_val_len & 0xf) as u8;
  let value_len = (stored_val_len >> VAL_LEN_BITS) as usize;
  Ok((shared, non_shared, value_len, flags, pos))
}

fn common_prefix(a: &[u8], b: &[u8]) -> usize {
  a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

pub fn put_data_header(page: &mut [u8], prev_page_id: u32, next_page_id: u32) -> Result<()> {
  set_page_type(page, PageType::Data)?;
  put_fixed32(
    page
      .get_mut(DATA_PREV_PAGE_OFFSET..)
      .ok_or(KvError::EndOfFile)?,
    prev_page_id,
  )
  .ok_or(KvError::EndOfFile)?;
  put_fixed32(
    page
      .get_mut(DATA_NEXT_PAGE_OFFSET..)
      .ok_or(KvError::EndOfFile)?,
    next_page_id,
  )
  .ok_or(KvError::EndOfFile)?;
  Ok(())
}

pub fn put_data_checksum(page: &mut [u8]) -> Result<()> {
  set_checksum(page)
}

pub fn set_prev_page_id(page: &mut [u8], page_id: u32) -> Result<()> {
  put_fixed32(
    page
      .get_mut(DATA_PREV_PAGE_OFFSET..)
      .ok_or(KvError::EndOfFile)?,
    page_id,
  )
  .ok_or(KvError::EndOfFile)?;
  Ok(())
}

pub fn set_next_page_id(page: &mut [u8], page_id: u32) -> Result<()> {
  put_fixed32(
    page
      .get_mut(DATA_NEXT_PAGE_OFFSET..)
      .ok_or(KvError::EndOfFile)?,
    page_id,
  )
  .ok_or(KvError::EndOfFile)?;
  Ok(())
}
