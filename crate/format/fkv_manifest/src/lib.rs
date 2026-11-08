//! Manifest record：snapshot / log / branch metadata。
//!
//! 字节布局：
//! `checksum(8B) | root(4B) | ttl_root(4B) | payload_len(4B) | payload`。

use std::mem::take;

use fkv_codec::{
  append_varint32, append_varint64, checksum, get_fixed32, put_fixed32, put_fixed64,
};
use fkv_error::{KvError, Result};
use fkv_types::{BranchFileRange, FilePageId, INVALID_VALUE, PageId};

pub const HEADER_BYTES: usize = 20;
pub const OFFSET_ROOT: usize = 8;
pub const OFFSET_TTL_ROOT: usize = 12;
pub const OFFSET_PAYLOAD_LEN: usize = 16;
/// manifest record 的对齐粒度：OS 页大小（对齐 C++ `page_align`）。
///
/// Linux 通常 4 KiB、Apple Silicon 16 KiB。record 写入与读取推进必须使用同一
/// 值，否则跨平台/跨 C++ 语料会把 padding 当成下一条 record 的 header。
pub fn manifest_align() -> usize {
  fkv_types::os_page_align()
}
pub(crate) const CHECKSUM_BATCH: usize = 1024 * 1024;
pub(crate) const CHECKSUM_MIXER: u64 = 0x9e37_79b9_7f4a_7c15;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchManifestMetadata {
  pub branch_name: Box<str>,
  pub term: u64,
  pub file_range_li: Vec<BranchFileRange>,
}

impl BranchManifestMetadata {
  pub fn new(branch_name: impl Into<Box<str>>, term: u64) -> Self {
    Self {
      branch_name: branch_name.into(),
      term,
      file_range_li: Vec::new(),
    }
  }
}

/// manifest checksum：1 MiB 分批，rotl(1) ^ XXH3，再乘 mixer。
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

pub fn serialize_branch_metadata(metadata: &BranchManifestMetadata) -> Result<Vec<u8>> {
  let mut out =
    Vec::with_capacity(16 + metadata.branch_name.len() + metadata.file_range_li.len() * 28);
  let name_len = length_u32(metadata.branch_name.len())?;
  out.extend_from_slice(&name_len.to_le_bytes());
  out.extend_from_slice(metadata.branch_name.as_bytes());
  out.extend_from_slice(&metadata.term.to_le_bytes());
  out.extend_from_slice(&(metadata.file_range_li.len() as u64).to_le_bytes());
  for range in &metadata.file_range_li {
    let range_name_len = length_u32(range.branch_name.len())?;
    out.extend_from_slice(&range_name_len.to_le_bytes());
    out.extend_from_slice(range.branch_name.as_bytes());
    out.extend_from_slice(&range.term.to_le_bytes());
    out.extend_from_slice(&range.max_file_id.to_le_bytes());
    out.extend_from_slice(&range.max_segment_file_id.to_le_bytes());
  }
  Ok(out)
}

pub fn deserialize_branch_metadata(data: &[u8]) -> Result<BranchManifestMetadata> {
  let mut pos = 0usize;
  let name_len = read_u32(data, &mut pos)? as usize;
  let branch_name = read_bytes(data, &mut pos, name_len)?;
  let term = read_u64(data, &mut pos)?;
  let num_entries = read_u64(data, &mut pos)?;
  let mut file_range_li = Vec::new();
  for _ in 0..num_entries {
    let range_name_len = read_u32(data, &mut pos)? as usize;
    let range_name = read_bytes(data, &mut pos, range_name_len)?;
    file_range_li.push(BranchFileRange {
      // 非 UTF-8 名直接报错，不做 lossy 替换（避免与原始字节分叉、GC key 碰撞）。
      branch_name: String::from_utf8(range_name.to_vec())
        .map_err(|_| KvError::Corrupted)?
        .into_boxed_str(),
      term: read_u64(data, &mut pos)?,
      max_file_id: read_u64(data, &mut pos)?,
      max_segment_file_id: read_u64(data, &mut pos)?,
    });
  }
  Ok(BranchManifestMetadata {
    branch_name: String::from_utf8(branch_name.to_vec())
      .map_err(|_| KvError::Corrupted)?
      .into_boxed_str(),
    term,
    file_range_li,
  })
}

fn read_u32(data: &[u8], pos: &mut usize) -> Result<u32> {
  let bytes = data
    .get(*pos..)
    .and_then(|rest| rest.first_chunk::<4>())
    .ok_or(KvError::EndOfFile)?;
  *pos += 4;
  Ok(u32::from_le_bytes(*bytes))
}

fn read_u64(data: &[u8], pos: &mut usize) -> Result<u64> {
  let bytes = data
    .get(*pos..)
    .and_then(|rest| rest.first_chunk::<8>())
    .ok_or(KvError::EndOfFile)?;
  *pos += 8;
  Ok(u64::from_le_bytes(*bytes))
}

fn read_bytes<'a>(data: &'a [u8], pos: &mut usize, len: usize) -> Result<&'a [u8]> {
  let bytes = data.get(*pos..*pos + len).ok_or(KvError::EndOfFile)?;
  *pos += len;
  Ok(bytes)
}

#[derive(Debug, Clone, Copy)]
pub struct SnapshotArg<'a> {
  pub root_id: PageId,
  pub ttl_root: PageId,
  pub max_fp_id: FilePageId,
  pub dict_bytes: &'a [u8],
  pub branch_metadata: &'a BranchManifestMetadata,
  pub max_segment_fp_id: FilePageId,
}

#[derive(Debug, Clone)]
pub struct ManifestBuilder {
  buf: Vec<u8>,
  resized_for_mapping_len: bool,
  mapping_len_written: bool,
  /// segment 段在 `buf` 中的起点；`None` 表示本次记录没有 segment 段。
  /// finalize 用它精确回填 mapping 段长度，而不是按 `len - HEADER_BYTES - 4`
  /// 反推（后者会把 segment 字节算进 mapping）。
  segment_start: Option<usize>,
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
      mapping_len_written: false,
      segment_start: None,
    }
  }

  pub fn reset(&mut self) {
    self.buf.resize(HEADER_BYTES, 0);
    self.resized_for_mapping_len = false;
    self.mapping_len_written = false;
    self.segment_start = None;
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
    self.update_mapping(page_id, INVALID_VALUE);
  }

  pub fn append_branch_metadata(&mut self, metadata: &BranchManifestMetadata) -> Result<()> {
    self.ensure_mapping_len();
    let mapping_len = length_u32(self.buf.len() - HEADER_BYTES - 4)?;
    if let Some(dst) = self.buf.get_mut(HEADER_BYTES..HEADER_BYTES + 4) {
      dst.copy_from_slice(&mapping_len.to_le_bytes());
    }
    self.mapping_len_written = true;
    let branch = serialize_branch_metadata(metadata)?;
    let len = length_u32(branch.len())?;
    self.buf.extend_from_slice(&len.to_le_bytes());
    self.buf.extend_from_slice(&branch);
    Ok(())
  }

  pub fn append_segment_mapping(&mut self, segment_mapping: &[u8]) {
    // 先补齐 mapping 段长度占位，再记录 segment 起点；否则「仅 segment」的
    // 记录会把 segment 长度前缀当成 mapping_len，并在 finalize 时被覆盖。
    self.ensure_mapping_len();
    self.segment_start = Some(self.buf.len());
    self.buf.extend_from_slice(segment_mapping);
  }

  /// 切片入参的 snapshot 便捷入口（测试/小表用）。
  pub fn snapshot(
    &mut self,
    args: SnapshotArg<'_>,
    mapping_value_li: &[u64],
    segment_value_li: Option<&[u64]>,
  ) -> Result<Vec<u8>> {
    self.snapshot_with(
      args,
      mapping_value_li.iter().copied(),
      segment_value_li.map(|value_li| value_li.iter().copied()),
    )
  }

  /// 构造 snapshot payload 并 finalize。
  ///
  /// mapping / segment 以迭代器逐值编码，避免整表摊平成 `Vec<u64>`；
  /// 泛型静态分派，无 `dyn`、无中间分配。
  pub fn snapshot_with<M, S>(
    &mut self,
    args: SnapshotArg<'_>,
    mapping_value_li: M,
    segment_value_li: Option<S>,
  ) -> Result<Vec<u8>>
  where
    M: Iterator<Item = u64>,
    S: Iterator<Item = u64>,
  {
    let SnapshotArg {
      root_id,
      ttl_root,
      max_fp_id,
      dict_bytes,
      branch_metadata,
      max_segment_fp_id,
    } = args;
    self.reset();
    append_varint64(&mut self.buf, max_fp_id);
    append_varint32(&mut self.buf, length_u32(dict_bytes.len())?);
    self.buf.extend_from_slice(dict_bytes);
    self.append_varint_section(mapping_value_li)?;
    let branch = serialize_branch_metadata(branch_metadata)?;
    self
      .buf
      .extend_from_slice(&length_u32(branch.len())?.to_le_bytes());
    self.buf.extend_from_slice(&branch);
    if let Some(segment_value_li) = segment_value_li {
      append_varint64(&mut self.buf, max_segment_fp_id);
      self.append_varint_section(segment_value_li)?;
    }
    self.mapping_len_written = true;
    self.finalize(root_id, ttl_root)
  }

  /// 写一段 varint 序列：4 字节 LE 长度前缀 + 逐值 LEB128。
  fn append_varint_section(&mut self, value_li: impl Iterator<Item = u64>) -> Result<()> {
    let len_offset = self.buf.len();
    self.buf.extend_from_slice(&[0u8; 4]);
    for value in value_li {
      append_varint64(&mut self.buf, value);
    }
    let len = length_u32(self.buf.len() - len_offset - 4)?;
    // 上一步刚写入 4 字节，区间必然存在。
    self.buf[len_offset..len_offset + 4].copy_from_slice(&len.to_le_bytes());
    Ok(())
  }

  pub fn finalize(&mut self, new_root: PageId, ttl_root: PageId) -> Result<Vec<u8>> {
    // 只有 mapping 段（无 branch/segment 可选段）的 log：补写 mapping 段长度。
    // 有 segment 段时以其起点为界，只统计 mapping 条目。
    // 未写过 mapping 段时 buf 可能不足 `HEADER_BYTES + 4`，按 0 记。
    if !self.mapping_len_written {
      let mapping_end = self.segment_start.unwrap_or(self.buf.len());
      let mapping_len = length_u32(mapping_end.saturating_sub(HEADER_BYTES + 4))?;
      if let Some(dst) = self.buf.get_mut(HEADER_BYTES..HEADER_BYTES + 4) {
        dst.copy_from_slice(&mapping_len.to_le_bytes());
      }
    }
    put_field(&mut self.buf, OFFSET_ROOT, new_root)?;
    put_field(&mut self.buf, OFFSET_TTL_ROOT, ttl_root)?;
    let payload_len = length_u32(
      self
        .buf
        .len()
        .checked_sub(HEADER_BYTES)
        .ok_or(KvError::EndOfFile)?,
    )?;
    self
      .buf
      .get_mut(OFFSET_PAYLOAD_LEN..OFFSET_PAYLOAD_LEN + 4)
      .ok_or(KvError::EndOfFile)?
      .copy_from_slice(&payload_len.to_le_bytes());
    let sum = manifest_checksum(&self.buf[8..]);
    put_fixed64(&mut self.buf, sum).ok_or(KvError::EndOfFile)?;
    let align = manifest_align();
    let aligned = (self.buf.len() + align - 1) & !(align - 1);
    self.buf.resize(aligned, 0);
    let out = take(&mut self.buf);
    // 复用 builder 时必须回到 `new` 的初始态：`buf` 已被取走，若保留
    // `resized_for_mapping_len` / `mapping_len_written`，后续 `append_branch_metadata`
    // 会跳过 4 字节 mapping 段长度占位，写出无 header 的记录或长度下溢。
    self.reset();
    Ok(out)
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

/// 长度字段显式收敛到 u32：manifest 单 record 超 4 GiB 视为空间不足。
fn length_u32(len: usize) -> Result<u32> {
  u32::try_from(len).map_err(|_| KvError::OutOfSpace)
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
