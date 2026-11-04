//! Index page：header(15B) + entries + restart array + restart_num(2B) + padding。

use fkv_codec::{decode_i32_delta, encode_i32_delta, get_varint32, put_varint32, varint32_len};
use fkv_error::{KvError, Result};

use crate::header::{
  INDEX_HEADER_SIZE, INDEX_LEFTMOST_PTR_OFFSET, PageType, content_len, new_page, page_type,
  restart_num, set_checksum, set_content_len, set_page_type,
};

#[derive(Debug, Clone)]
pub struct IndexPageBuilder {
  page_size: usize,
  restart_interval: u16,
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
    let mut buf = new_page(
      page_size,
      if leaf {
        PageType::LeafIndex
      } else {
        PageType::NonLeafIndex
      },
    )?;
    set_page_type(
      &mut buf,
      if leaf {
        PageType::LeafIndex
      } else {
        PageType::NonLeafIndex
      },
    )?;
    buf.truncate(INDEX_HEADER_SIZE);
    Ok(Self {
      page_size,
      restart_interval,
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
    if !self.last_key.is_empty() && key <= self.last_key.as_slice() {
      return Ok(false);
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
    let p_delta = encode_i32_delta(delta);
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

  pub fn finish(mut self) -> Result<Vec<u8>> {
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
    set_checksum(&mut self.buf)?;
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
  pub fn new(page: &'a [u8]) -> Result<Self> {
    let content = usize::from(content_len(page)?);
    let count = usize::from(restart_num(page)?);
    let restart_offset = content
      .checked_sub((1 + count) * 2)
      .ok_or(KvError::Corrupted)?;
    Ok(Self {
      page,
      restart_num: count as u16,
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
    let bytes = self
      .page
      .get(self.curr_offset..self.restart_offset)
      .ok_or(KvError::EndOfFile)?;
    let (shared, r1) = get_varint32(bytes)?;
    let (non_shared, r2) = get_varint32(&bytes[r1..])?;
    let key_start = self.curr_offset + r1 + r2;
    let key_end = key_start + non_shared as usize;
    let (delta, r3) = get_varint32(
      self
        .page
        .get(key_end..self.restart_offset)
        .ok_or(KvError::EndOfFile)?,
    )?;
    if shared as usize > self.key.len() {
      return Err(KvError::Corrupted);
    }
    self.key.truncate(shared as usize);
    self.key.extend_from_slice(
      self
        .page
        .get(key_start..key_end)
        .ok_or(KvError::Corrupted)?,
    );
    let delta = decode_i32_delta(delta);
    self.page_id = if is_restart {
      delta as u32
    } else {
      (self.page_id as i32 + delta) as u32
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

  pub fn seek(&mut self, target: &[u8]) -> Result<bool> {
    self.reset();
    while self.advance()? {
      if self.key() >= target {
        return Ok(true);
      }
    }
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

fn common_prefix(a: &[u8], b: &[u8]) -> usize {
  a.iter().zip(b).take_while(|(x, y)| x == y).count()
}
