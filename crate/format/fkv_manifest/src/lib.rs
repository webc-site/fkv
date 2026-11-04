//! Manifest record：snapshot / log / branch metadata。
//!
//! 字节布局对齐 eloqstore `ManifestBuilder`：
//! `checksum(8B) | root(4B) | ttl_root(4B) | payload_len(4B) | payload`。

use std::mem::take;

use fkv_codec::{
  checksum, get_fixed32, get_varint32, get_varint64, put_fixed32, put_varint32, put_varint64,
  varint32_len, varint64_len,
};
use fkv_error::{KvError, Result};
use fkv_types::{BranchFileRange, FilePageId, PageId};

pub const HEADER_BYTES: usize = 20;
pub const OFFSET_ROOT: usize = 8;
pub const OFFSET_TTL_ROOT: usize = 12;
pub const OFFSET_PAYLOAD_LEN: usize = 16;
pub const MANIFEST_ALIGN: usize = 4096;
pub const CHECKSUM_BATCH: usize = 1024 * 1024;
pub const CHECKSUM_MIXER: u64 = 0x9e37_79b9_7f4a_7c15;

pub const VAL_TYPE_BITS: u8 = 3;
pub const VAL_TYPE_MASK: u64 = (1 << VAL_TYPE_BITS) - 1;
pub const VAL_FILE_PAGE_ID: u64 = 1;
pub const VAL_PAGE_ID: u64 = 2;
pub const VAL_INVALID: u64 = VAL_TYPE_MASK;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchManifestMetadata {
  pub branch_name: Box<str>,
  pub term: u64,
  pub file_ranges: Vec<BranchFileRange>,
}

impl BranchManifestMetadata {
  pub fn new(branch_name: impl Into<Box<str>>, term: u64) -> Self {
    Self {
      branch_name: branch_name.into(),
      term,
      file_ranges: Vec::new(),
    }
  }
}

pub fn encode_file_page_id(file_page_id: FilePageId) -> u64 {
  (file_page_id << VAL_TYPE_BITS) | VAL_FILE_PAGE_ID
}

pub fn encode_page_id(page_id: PageId) -> u64 {
  (u64::from(page_id) << VAL_TYPE_BITS) | VAL_PAGE_ID
}

pub fn invalid_mapping_value() -> u64 {
  VAL_INVALID
}

/// eloqstore `ManifestBuilder::CalcChecksum`：1 MiB 分批，rotl(1) ^ XXH3，再乘 mixer。
pub fn manifest_checksum(content: &[u8]) -> u64 {
  if content.is_empty() {
    return 0;
  }
  let mut agg = 0u64;
  for batch in content.chunks(CHECKSUM_BATCH) {
    agg = agg.rotate_left(1) ^ checksum(batch);
    agg = agg.wrapping_mul(CHECKSUM_MIXER);
  }
  agg
}

pub fn serialize_branch_metadata(metadata: &BranchManifestMetadata) -> Vec<u8> {
  let mut out =
    Vec::with_capacity(16 + metadata.branch_name.len() + metadata.file_ranges.len() * 28);
  let name_len = u32::try_from(metadata.branch_name.len()).unwrap_or(u32::MAX);
  out.extend_from_slice(&name_len.to_le_bytes());
  out.extend_from_slice(metadata.branch_name.as_bytes());
  out.extend_from_slice(&metadata.term.to_le_bytes());
  out.extend_from_slice(&(metadata.file_ranges.len() as u64).to_le_bytes());
  for range in &metadata.file_ranges {
    let range_name_len = u32::try_from(range.branch_name.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&range_name_len.to_le_bytes());
    out.extend_from_slice(range.branch_name.as_bytes());
    out.extend_from_slice(&range.term.to_le_bytes());
    out.extend_from_slice(&range.max_file_id.to_le_bytes());
    out.extend_from_slice(&range.max_segment_file_id.to_le_bytes());
  }
  out
}

pub fn deserialize_branch_metadata(data: &[u8]) -> Result<BranchManifestMetadata> {
  let mut pos = 0usize;
  let name_len = read_u32(data, &mut pos)? as usize;
  let branch_name = read_bytes(data, &mut pos, name_len)?;
  let term = read_u64(data, &mut pos)?;
  let num_entries = read_u64(data, &mut pos)?;
  let mut file_ranges = Vec::new();
  for _ in 0..num_entries {
    let range_name_len = read_u32(data, &mut pos)? as usize;
    let range_name = read_bytes(data, &mut pos, range_name_len)?;
    file_ranges.push(BranchFileRange {
      branch_name: String::from_utf8_lossy(range_name)
        .into_owned()
        .into_boxed_str(),
      term: read_u64(data, &mut pos)?,
      max_file_id: read_u64(data, &mut pos)?,
      max_segment_file_id: read_u64(data, &mut pos)?,
    });
  }
  Ok(BranchManifestMetadata {
    branch_name: String::from_utf8_lossy(branch_name)
      .into_owned()
      .into_boxed_str(),
    term,
    file_ranges,
  })
}

fn read_u32(data: &[u8], pos: &mut usize) -> Result<u32> {
  let bytes = data.get(*pos..*pos + 4).ok_or(KvError::EndOfFile)?;
  *pos += 4;
  Ok(u32::from_le_bytes(
    bytes.try_into().map_err(|_| KvError::EndOfFile)?,
  ))
}

fn read_u64(data: &[u8], pos: &mut usize) -> Result<u64> {
  let bytes = data.get(*pos..*pos + 8).ok_or(KvError::EndOfFile)?;
  *pos += 8;
  Ok(u64::from_le_bytes(
    bytes.try_into().map_err(|_| KvError::EndOfFile)?,
  ))
}

fn read_bytes<'a>(data: &'a [u8], pos: &mut usize, len: usize) -> Result<&'a [u8]> {
  let bytes = data.get(*pos..*pos + len).ok_or(KvError::EndOfFile)?;
  *pos += len;
  Ok(bytes)
}

#[derive(Debug, Clone, Copy)]
pub struct SnapshotArgs<'a> {
  pub root_id: PageId,
  pub ttl_root: PageId,
  pub max_fp_id: FilePageId,
  pub dict_bytes: &'a [u8],
  pub mapping_value_li: &'a [u64],
  pub branch_metadata: &'a BranchManifestMetadata,
  pub segment_value_li: Option<&'a [u64]>,
  pub max_segment_fp_id: FilePageId,
}

#[derive(Debug, Clone)]
pub struct ManifestBuilder {
  buf: Vec<u8>,
  resized_for_mapping_len: bool,
}

impl Default for ManifestBuilder {
  fn default() -> Self {
    Self::new()
  }
}

impl ManifestBuilder {
  pub fn new() -> Self {
    Self {
      buf: vec![0u8; HEADER_BYTES],
      resized_for_mapping_len: false,
    }
  }

  pub fn reset(&mut self) {
    self.buf.resize(HEADER_BYTES, 0);
    self.resized_for_mapping_len = false;
  }

  pub fn is_empty(&self) -> bool {
    self.buf.len() <= HEADER_BYTES
  }

  pub fn current_size(&self) -> usize {
    self.buf.len()
  }

  pub fn update_mapping(&mut self, page_id: PageId, encoded_value: u64) {
    self.ensure_mapping_len();
    append_varint32(&mut self.buf, page_id);
    append_varint64(&mut self.buf, encoded_value);
  }

  pub fn delete_mapping(&mut self, page_id: PageId) {
    self.update_mapping(page_id, invalid_mapping_value());
  }

  pub fn append_branch_metadata(&mut self, metadata: &BranchManifestMetadata) {
    self.ensure_mapping_len();
    let mapping_len = self.buf.len() - HEADER_BYTES - 4;
    if let Some(dst) = self.buf.get_mut(HEADER_BYTES..HEADER_BYTES + 4) {
      dst.copy_from_slice(&(mapping_len as u32).to_le_bytes());
    }
    let branch = serialize_branch_metadata(metadata);
    let len = branch.len() as u32;
    self.buf.extend_from_slice(&len.to_le_bytes());
    self.buf.extend_from_slice(&branch);
  }

  pub fn append_segment_mapping(&mut self, segment_mapping: &[u8]) {
    self.buf.extend_from_slice(segment_mapping);
  }

  /// 构造 snapshot payload 并 finalize。
  pub fn snapshot(&mut self, args: SnapshotArgs<'_>) -> Result<Vec<u8>> {
    let SnapshotArgs {
      root_id,
      ttl_root,
      max_fp_id,
      dict_bytes,
      mapping_value_li,
      branch_metadata,
      segment_value_li,
      max_segment_fp_id,
    } = args;
    self.reset();
    append_varint64(&mut self.buf, max_fp_id);
    append_varint32(&mut self.buf, dict_bytes.len() as u32);
    self.buf.extend_from_slice(dict_bytes);
    let mapping_len_offset = self.buf.len();
    self.buf.extend_from_slice(&[0u8; 4]);
    for value in mapping_value_li {
      append_varint64(&mut self.buf, *value);
    }
    let mapping_len = (self.buf.len() - mapping_len_offset - 4) as u32;
    self.buf[mapping_len_offset..mapping_len_offset + 4]
      .copy_from_slice(&mapping_len.to_le_bytes());
    let branch = serialize_branch_metadata(branch_metadata);
    self
      .buf
      .extend_from_slice(&(branch.len() as u32).to_le_bytes());
    self.buf.extend_from_slice(&branch);
    if let Some(segment_value_li) = segment_value_li {
      append_varint64(&mut self.buf, max_segment_fp_id);
      let seg_len_offset = self.buf.len();
      self.buf.extend_from_slice(&[0u8; 4]);
      for value in segment_value_li {
        append_varint64(&mut self.buf, *value);
      }
      let seg_len = (self.buf.len() - seg_len_offset - 4) as u32;
      self.buf[seg_len_offset..seg_len_offset + 4].copy_from_slice(&seg_len.to_le_bytes());
    }
    self.finalize(root_id, ttl_root)
  }

  pub fn finalize(&mut self, new_root: PageId, ttl_root: PageId) -> Result<Vec<u8>> {
    put_field(&mut self.buf, OFFSET_ROOT, new_root)?;
    put_field(&mut self.buf, OFFSET_TTL_ROOT, ttl_root)?;
    let payload_len = (self.buf.len() - HEADER_BYTES) as u32;
    self.buf[OFFSET_PAYLOAD_LEN..OFFSET_PAYLOAD_LEN + 4]
      .copy_from_slice(&payload_len.to_le_bytes());
    let sum = manifest_checksum(&self.buf[8..]);
    self.buf[..8].copy_from_slice(&sum.to_le_bytes());
    let aligned = (self.buf.len() + MANIFEST_ALIGN - 1) & !(MANIFEST_ALIGN - 1);
    self.buf.resize(aligned, 0);
    Ok(take(&mut self.buf))
  }

  fn ensure_mapping_len(&mut self) {
    if !self.resized_for_mapping_len {
      self.buf.resize(HEADER_BYTES + 4, 0);
      self.resized_for_mapping_len = true;
    }
  }
}

fn put_field(buf: &mut [u8], offset: usize, value: PageId) -> Result<()> {
  put_fixed32(buf.get_mut(offset..).ok_or(KvError::EndOfFile)?, value).ok_or(KvError::EndOfFile)?;
  Ok(())
}

fn append_varint32(buf: &mut Vec<u8>, value: u32) {
  let mut scratch = [0u8; 5];
  let len = put_varint32(&mut scratch, value).unwrap_or(0);
  buf.extend_from_slice(&scratch[..len]);
}

fn append_varint64(buf: &mut Vec<u8>, value: u64) {
  let mut scratch = [0u8; 10];
  let len = put_varint64(&mut scratch, value).unwrap_or(0);
  buf.extend_from_slice(&scratch[..len]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestHeader {
  pub root: PageId,
  pub ttl_root: PageId,
  pub payload_len: u32,
}

pub fn parse_header(record: &[u8]) -> Result<ManifestHeader> {
  if record.len() < HEADER_BYTES {
    return Err(KvError::EndOfFile);
  }
  Ok(ManifestHeader {
    root: get_fixed32(&record[OFFSET_ROOT..])?,
    ttl_root: get_fixed32(&record[OFFSET_TTL_ROOT..])?,
    payload_len: get_fixed32(&record[OFFSET_PAYLOAD_LEN..])?,
  })
}

pub fn validate_record(record: &[u8]) -> Result<bool> {
  let header = parse_header(record)?;
  let end = HEADER_BYTES
    .checked_add(header.payload_len as usize)
    .ok_or(KvError::Corrupted)?;
  let content = record.get(8..end).ok_or(KvError::EndOfFile)?;
  let stored = fkv_codec::get_fixed64(record)?;
  Ok(stored == manifest_checksum(content))
}

pub fn payload(record: &[u8]) -> Result<&[u8]> {
  let header = parse_header(record)?;
  let end = HEADER_BYTES
    .checked_add(header.payload_len as usize)
    .ok_or(KvError::Corrupted)?;
  record.get(HEADER_BYTES..end).ok_or(KvError::EndOfFile)
}

pub fn varint64_size(value: u64) -> usize {
  varint64_len(value)
}

pub fn varint32_size(value: u32) -> usize {
  varint32_len(value)
}

pub fn read_varint64(data: &[u8]) -> Result<(u64, usize)> {
  get_varint64(data)
}

pub fn read_varint32(data: &[u8]) -> Result<(u32, usize)> {
  get_varint32(data)
}
