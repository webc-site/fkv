//! Manifest replay：snapshot + log records → 内存状态。
//!
//! 只做字节解析，不碰 IO；冷启动 / reopen / snapshot install 由上层组合。

use fkv_codec::{get_fixed32, get_varint32, get_varint64};
use fkv_error::{KvError, Result};
use fkv_manifest::{
  BranchManifestMetadata, deserialize_branch_metadata, parse_header, payload, validate_record,
};
use fkv_mapping::{MappingTbl, decode_id, is_file_page_id};
use fkv_types::{FilePageId, PageId};

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
      root_id: PageId::MAX,
      ttl_root_id: PageId::MAX,
      mapping_tbl: MappingTbl::new(),
      max_fp_id: 0,
      dict_bytes: Vec::new(),
      branch_metadata: BranchManifestMetadata::new("main", 0),
      segment_mapping: MappingTbl::new(),
      max_segment_fp_id: 0,
    }
  }
}

struct Cursor<'a> {
  data: &'a [u8],
  pos: usize,
}

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

  fn branch_metadata(&mut self) -> Result<BranchManifestMetadata> {
    let len = self.u32()? as usize;
    deserialize_branch_metadata(self.take(len)?)
  }
}

fn max_file_page_id(mapping_tbl: &MappingTbl) -> FilePageId {
  mapping_tbl
    .value_li()
    .iter()
    .filter(|value| is_file_page_id(**value))
    .map(|value| decode_id(*value) + 1)
    .max()
    .unwrap_or(0)
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
    let branch_metadata = cur.branch_metadata()?;

    let (segment_mapping, max_segment_fp_id) = if cur.remaining() > 0 {
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
    while mapping_cur.remaining() > 0 {
      let page_id = mapping_cur.varint32()?;
      let value = mapping_cur.varint64()?;
      self.mapping_tbl.set(page_id, value)?;
    }
    self.branch_metadata = cur.branch_metadata()?;
    let seg_len = cur.u32()? as usize;
    let seg_bytes = cur.take(seg_len)?;
    let mut seg_cur = Cursor::new(seg_bytes);
    while seg_cur.remaining() > 0 {
      let page_id = seg_cur.varint32()?;
      let value = seg_cur.varint64()?;
      self.segment_mapping.set(page_id, value)?;
    }
    self.max_fp_id = self.max_fp_id.max(max_file_page_id(&self.mapping_tbl));
    self.max_segment_fp_id = self
      .max_segment_fp_id
      .max(max_file_page_id(&self.segment_mapping));
    self.root_id = header.root;
    self.ttl_root_id = header.ttl_root;
    Ok(())
  }
}
