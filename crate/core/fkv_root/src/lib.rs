//! 分区级持久状态缓存：`RootMeta` + LRU。

use core::mem::size_of;
use std::collections::VecDeque;

use fkv_compress::DictCompression;
use fkv_manifest::BranchManifestMetadata;
use fkv_mapping::{AllocMode, MappingTbl, PageMapper};
use fkv_types::{FilePageId, PageId, TableIdent};
use gxhash::HashMap;

pub struct ReplayArgs {
  pub mode: AllocMode,
  pub compression_level: i32,
  pub mapping_tbl: MappingTbl,
  pub max_fp_id: FilePageId,
  pub segment_mapping: MappingTbl,
  pub max_segment_fp_id: FilePageId,
  pub root_id: PageId,
  pub ttl_root_id: PageId,
  pub dict_bytes: Vec<u8>,
  pub branch_metadata: BranchManifestMetadata,
}

#[derive(Debug)]
pub struct RootMeta {
  pub root_id: PageId,
  pub ttl_root_id: PageId,
  pub mapper: PageMapper,
  pub segment_mapper: PageMapper,
  pub manifest_size: u64,
  pub next_expire_ts: u64,
  pub compression: DictCompression,
  pub branch_metadata: BranchManifestMetadata,
  pub first_unflushed_fp_id: FilePageId,
  pub first_unflushed_seg_fp_id: FilePageId,
  pub ref_cnt: u32,
  pub locked: bool,
}

impl RootMeta {
  pub fn new(mode: AllocMode, max_fp_id: FilePageId, compression_level: i32) -> Self {
    Self {
      root_id: PageId::MAX,
      ttl_root_id: PageId::MAX,
      mapper: PageMapper::new(mode, max_fp_id),
      segment_mapper: PageMapper::new(mode, 0),
      manifest_size: 0,
      next_expire_ts: 0,
      compression: DictCompression::new(compression_level),
      branch_metadata: BranchManifestMetadata::new("main", 0),
      first_unflushed_fp_id: 0,
      first_unflushed_seg_fp_id: 0,
      ref_cnt: 0,
      locked: false,
    }
  }

  pub fn from_replay(args: ReplayArgs) -> fkv_error::Result<Self> {
    let ReplayArgs {
      mode,
      compression_level,
      mapping_tbl,
      max_fp_id,
      segment_mapping,
      max_segment_fp_id,
      root_id,
      ttl_root_id,
      dict_bytes,
      branch_metadata,
    } = args;
    let mut compression = DictCompression::new(compression_level);
    if !dict_bytes.is_empty() {
      compression.load_dictionary(dict_bytes)?;
    }
    Ok(Self {
      root_id,
      ttl_root_id,
      mapper: PageMapper::with_tbl(mapping_tbl, mode, max_fp_id),
      segment_mapper: PageMapper::with_tbl(segment_mapping, mode, max_segment_fp_id),
      manifest_size: 0,
      next_expire_ts: 0,
      compression,
      branch_metadata,
      first_unflushed_fp_id: 0,
      first_unflushed_seg_fp_id: 0,
      ref_cnt: 0,
      locked: false,
    })
  }

  pub fn memory_bytes(&self) -> usize {
    self.mapper.tbl().len() * size_of::<u64>()
      + self.segment_mapper.tbl().len() * size_of::<u64>()
      + self.compression.dictionary_bytes().len()
  }

  pub fn max_fp_id(&self) -> FilePageId {
    self.mapper.max_fp_id()
  }

  pub fn max_segment_fp_id(&self) -> FilePageId {
    self.segment_mapper.max_fp_id()
  }
}

#[derive(Debug)]
struct Entry {
  meta: RootMeta,
  ref_cnt: u32,
}

#[derive(Debug)]
pub struct RootMetaMgr {
  capacity_bytes: usize,
  used_bytes: usize,
  map: HashMap<TableIdent, Entry>,
  lru: VecDeque<TableIdent>,
}

impl RootMetaMgr {
  pub fn new(capacity_bytes: usize, num_threads: u16) -> Self {
    let capacity_bytes = if num_threads == 0 {
      capacity_bytes
    } else {
      capacity_bytes / usize::from(num_threads)
    };
    Self {
      capacity_bytes,
      used_bytes: 0,
      map: HashMap::default(),
      lru: VecDeque::new(),
    }
  }

  pub fn len(&self) -> usize {
    self.map.len()
  }

  pub fn is_empty(&self) -> bool {
    self.map.is_empty()
  }

  pub fn get(&self, tbl: &TableIdent) -> Option<&RootMeta> {
    self.map.get(tbl).map(|entry| &entry.meta)
  }

  pub fn get_mut(&mut self, tbl: &TableIdent) -> Option<&mut RootMeta> {
    self.touch(tbl);
    self.map.get_mut(tbl).map(|entry| &mut entry.meta)
  }

  pub fn get_or_create(&mut self, tbl: &TableIdent, mode: AllocMode) -> Option<&mut RootMeta> {
    if !self.map.contains_key(tbl) {
      let meta = RootMeta::new(mode, 0, 3);
      self.used_bytes = self.used_bytes.saturating_add(meta.memory_bytes());
      self.map.insert(tbl.clone(), Entry { meta, ref_cnt: 0 });
      self.lru.push_back(tbl.clone());
    }
    self.touch(tbl);
    self.map.get_mut(tbl).map(|entry| &mut entry.meta)
  }

  pub fn pin(&mut self, tbl: &TableIdent) -> Option<&mut RootMeta> {
    self.touch(tbl);
    let entry = self.map.get_mut(tbl)?;
    entry.ref_cnt += 1;
    entry.meta.ref_cnt = entry.ref_cnt;
    Some(&mut entry.meta)
  }

  pub fn release(&mut self, tbl: &TableIdent) -> bool {
    let Some(entry) = self.map.get_mut(tbl) else {
      return false;
    };
    entry.ref_cnt = entry.ref_cnt.saturating_sub(1);
    entry.meta.ref_cnt = entry.ref_cnt;
    true
  }

  pub fn remove(&mut self, tbl: &TableIdent) -> Option<RootMeta> {
    let entry = self.map.remove(tbl)?;
    self.used_bytes = self.used_bytes.saturating_sub(entry.meta.memory_bytes());
    self.lru.retain(|item| item != tbl);
    Some(entry.meta)
  }

  pub fn evict_if_needed(&mut self, additional_bytes: usize) -> usize {
    if self.capacity_bytes == 0 {
      return 0;
    }
    self.used_bytes = self
      .map
      .values()
      .map(|entry| entry.meta.memory_bytes())
      .sum();
    let mut evicted = 0;
    while self.used_bytes.saturating_add(additional_bytes) > self.capacity_bytes
      && !self.lru.is_empty()
    {
      let Some(tbl) = self.lru.front().cloned() else {
        break;
      };
      let evictable = self
        .map
        .get(&tbl)
        .is_some_and(|entry| entry.ref_cnt == 0 && !entry.meta.locked);
      if evictable {
        self.remove(&tbl);
        evicted += 1;
      } else {
        self.lru.rotate_left(1);
        if self
          .lru
          .iter()
          .all(|item| self.map.get(item).is_none_or(|entry| entry.ref_cnt > 0))
        {
          break;
        }
      }
    }
    evicted
  }

  fn touch(&mut self, tbl: &TableIdent) {
    self.lru.retain(|item| item != tbl);
    if self.map.contains_key(tbl) {
      self.lru.push_back(tbl.clone());
    }
  }
}
