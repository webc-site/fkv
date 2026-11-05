//! Manifest replay：snapshot + log records → 内存状态。
//!
//! 只做字节解析，不碰 IO；冷启动 / reopen / snapshot install 由上层组合。

use std::mem::size_of;

use fkv_codec::{get_fixed32, get_varint32, get_varint64};
use fkv_error::{KvError, Result};
use fkv_manifest::{
  BranchManifestMetadata, deserialize_branch_metadata, parse_header, payload, validate_record,
};
use fkv_mapping::MappingTbl;
use fkv_types::{FilePageId, MAIN_BRANCH_NAME, NULL_PAGE, PageId, decode_id, is_file_page_id};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayState {
  pub root_id: PageId,
  pub ttl_root_id: PageId,
  pub mapping_tbl: MappingTbl,
  pub max_fp_id: FilePageId,
  pub dict_bytes: Vec<u8>,
  pub branch_metadata: BranchManifestMetadata,
  pub segment_mapping: MappingTbl,
  pub max_segment_fp_id: FilePageId,
}

impl Default for ReplayState {
  fn default() -> Self {
    Self {
      root_id: NULL_PAGE,
      ttl_root_id: NULL_PAGE,
      mapping_tbl: MappingTbl::new(),
      max_fp_id: 0,
      dict_bytes: Vec::new(),
      branch_metadata: BranchManifestMetadata::new(MAIN_BRANCH_NAME, 0),
      segment_mapping: MappingTbl::new(),
      max_segment_fp_id: 0,
    }
  }
}

struct Cursor<'a> {
  data: &'a [u8],
  pos: usize,
}

/// manifest 中 Fixed32 长度前缀的字节数。
const U32_BYTES: usize = size_of::<u32>();

impl<'a> Cursor<'a> {
  fn new(data: &'a [u8]) -> Self {
    Self { data, pos: 0 }
  }

  fn remaining(&self) -> usize {
    self.data.len().saturating_sub(self.pos)
  }

  fn take(&mut self, len: usize) -> Result<&'a [u8]> {
    let end = self.pos.checked_add(len).ok_or(KvError::Corrupted)?;
    let bytes = self.data.get(self.pos..end).ok_or(KvError::EndOfFile)?;
    self.pos = end;
    Ok(bytes)
  }

  fn u32(&mut self) -> Result<u32> {
    get_fixed32(self.take(4)?)
  }

  fn varint32(&mut self) -> Result<u32> {
    let (value, read) = get_varint32(&self.data[self.pos..])?;
    self.pos += read;
    Ok(value)
  }

  fn varint64(&mut self) -> Result<u64> {
    let (value, read) = get_varint64(&self.data[self.pos..])?;
    self.pos += read;
    Ok(value)
  }

  /// 可选 branch metadata 段：`u32` 长度前缀 + 负载。
  ///
  /// 对齐 C++ `Replayer::DeserializeSnapshot` / `ReplayLog`：
  /// 前缀不足 4B、`len == 0`、或声明长度超过剩余字节时都视作段缺失，
  /// 只消费长度前缀、保留调用方已有值；负载解析失败仍然报错（防损坏）。
  fn branch_metadata_opt(&mut self) -> Result<Option<BranchManifestMetadata>> {
    if self.remaining() < U32_BYTES {
      return Ok(None);
    }
    let len = self.u32()? as usize;
    if len == 0 || self.remaining() < len {
      return Ok(None);
    }
    deserialize_branch_metadata(self.take(len)?).map(Some)
  }
}

/// 快照落地后一次性扫描：持久化 `max_fp_id` 与表内最大值取 max。
fn max_file_page_id(mapping_tbl: &MappingTbl) -> FilePageId {
  mapping_tbl
    .iter_value_li()
    .filter(|value| is_file_page_id(*value))
    .map(|value| decode_id(value) + 1)
    .max()
    .unwrap_or(0)
}

/// log delta 增量抬升高水位。非 `FilePageId`（`PageId` / `Invalid` /
/// `SwizzlingPointer`）不参与，对齐 `max_file_page_id` 的过滤条件。
///
/// `is_file_page_id` 保证低 3 位为 1，`decode_id(value) + 1` 不会溢出。
#[inline]
fn bump_max_fp_id(max_fp_id: &mut FilePageId, value: u64) {
  if is_file_page_id(value) {
    *max_fp_id = (*max_fp_id).max(decode_id(value) + 1);
  }
}

impl ReplayState {
  pub fn from_snapshot(record: &[u8]) -> Result<Self> {
    if !validate_record(record)? {
      return Err(KvError::Corrupted);
    }
    let header = parse_header(record)?;
    let mut cur = Cursor::new(payload(record)?);
    let max_fp_id = cur.varint64()?;
    let dict_len = cur.varint32()? as usize;
    let dict_bytes = cur.take(dict_len)?.to_vec();
    let mapping_len = cur.u32()? as usize;
    let mapping_tbl = MappingTbl::deserialize(cur.take(mapping_len)?)?;
    let max_fp_id = max_fp_id.max(max_file_page_id(&mapping_tbl));
    // 旧格式 snapshot 可能没有 branch 段；缺失时用 main/term=0 默认值继续。
    let branch_metadata = cur
      .branch_metadata_opt()?
      .unwrap_or_else(|| BranchManifestMetadata::new(MAIN_BRANCH_NAME, 0));

    // 对齐 C++ `DeserializeSnapshot`：segment 段需至少 4B 长度前缀，
    // 1-3 字节尾残留按缺失处理而不是报错。
    let (segment_mapping, max_segment_fp_id) = if cur.remaining() >= U32_BYTES {
      let max_segment_fp_id = cur.varint64()?;
      let seg_len = cur.u32()? as usize;
      let segment_mapping = MappingTbl::deserialize(cur.take(seg_len)?)?;
      let max_segment_fp_id = max_segment_fp_id.max(max_file_page_id(&segment_mapping));
      (segment_mapping, max_segment_fp_id)
    } else {
      (MappingTbl::new(), 0)
    };

    Ok(Self {
      root_id: header.root,
      ttl_root_id: header.ttl_root,
      mapping_tbl,
      max_fp_id,
      dict_bytes,
      branch_metadata,
      segment_mapping,
      max_segment_fp_id,
    })
  }

  pub fn apply_log(&mut self, record: &[u8]) -> Result<()> {
    if !validate_record(record)? {
      return Err(KvError::Corrupted);
    }
    let header = parse_header(record)?;
    let mut cur = Cursor::new(payload(record)?);
    let mapping_len = cur.u32()? as usize;
    let mapping_bytes = cur.take(mapping_len)?;
    let mut mapping_cur = Cursor::new(mapping_bytes);
    // 对齐 C++ `Replayer::ReplayLog`：解析 delta 时就地抬升 `max_fp_id`，
    // 避免每条 record 全表重扫（O(records × mapping) → O(records)）。
    while mapping_cur.remaining() > 0 {
      let page_id = mapping_cur.varint32()?;
      let value = mapping_cur.varint64()?;
      self.mapping_tbl.set(page_id, value);
      bump_max_fp_id(&mut self.max_fp_id, value);
    }
    // 对齐 C++ `ReplayLog`：branch / segment 段都是可选的（各至少 4B 长度前缀），
    // 缺失时保留已有状态而不是报错。
    if let Some(branch_metadata) = cur.branch_metadata_opt()? {
      self.branch_metadata = branch_metadata;
    }
    if cur.remaining() >= U32_BYTES {
      let seg_len = cur.u32()? as usize;
      let seg_bytes = cur.take(seg_len)?;
      let mut seg_cur = Cursor::new(seg_bytes);
      while seg_cur.remaining() > 0 {
        let page_id = seg_cur.varint32()?;
        let value = seg_cur.varint64()?;
        self.segment_mapping.set(page_id, value);
        bump_max_fp_id(&mut self.max_segment_fp_id, value);
      }
    }
    self.root_id = header.root;
    self.ttl_root_id = header.ttl_root;
    Ok(())
  }
}
