//! 单线程 Store：可接 `MemStore` / `CompioStore`，打通 put / get / scan / delete，
//! 并在写路径后追加 manifest snapshot / log。

use std::time::{SystemTime, UNIX_EPOCH};

use fkv_btree::BatchEntry;
use fkv_codec::{append_varint32, append_varint64};
use fkv_compress::DictCompression;
use fkv_error::{KvError, Result};
use fkv_io::{FileIo, FixedBuffer, FixedSegmentIo, ManifestIo, PageIo, SegmentIo, TermIo};
use fkv_large::{
  ManagedReadResult, SegmentLayout, free_value as free_large_value, read_value as read_large_value,
  read_value_fixed, write_value as write_large_value, write_value_fixed_with_len,
};
use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArgs};
use fkv_mapping::{AllocMode, PageMapper};
use fkv_task_read::{read_entry, read_floor_entry};
pub use fkv_task_scan::ScanOptions;
use fkv_task_scan::{PageOptions, ScanArgs, scan_entries, scan_entries_with, scan_page_with};
use fkv_task_write::{
  PreparedWriteBatch, WriteArgs, is_expired, parse_ttl_key, prepare_write_batch, write_batch,
};
use fkv_types::{
  BranchFileRange, FilePageId, PageId, TableIdent, WriteOp, archive_file_name, manifest_file_name,
  normalize_branch_name, parse_manifest_file_name,
};
use gxhash::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CleanExpiredStats {
  pub deleted: usize,
  pub ttl_deleted: usize,
  pub next_expire_ts: u64,
}

fn is_safe_tag(tag: &str) -> bool {
  !tag.is_empty()
    && tag
      .chars()
      .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn batch_timestamp_map(batch: &[BatchEntry]) -> HashMap<Vec<u8>, u64> {
  let mut timestamp_map: HashMap<Vec<u8>, u64> = HashMap::default();
  for entry in batch {
    let timestamp = timestamp_map.entry(entry.key.clone()).or_insert(0);
    *timestamp = (*timestamp).max(entry.timestamp);
  }
  timestamp_map
}

fn now_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
    .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeValue {
  pub value: Vec<u8>,
  pub metadata: Vec<u8>,
}

/// branch 预留的连续 file id 数；main 跳过该区间，避免与 branch 并发写碰撞。
pub const BRANCH_FILE_RESERVE_FILE_NUM: u64 = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveInfo {
  pub name: String,
  pub bytes: usize,
  pub removed_num: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInfo {
  pub name: String,
  pub bytes: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanPageResult {
  pub entry_li: Vec<(Vec<u8>, Vec<u8>)>,
  pub has_remaining: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloorValue {
  pub key: Vec<u8>,
  pub value: Vec<u8>,
  pub metadata: Vec<u8>,
  pub timestamp: u64,
  pub expire_ts: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeValueInto {
  pub value_len: usize,
  pub metadata: Vec<u8>,
}

#[derive(Debug)]
pub struct Store<I> {
  io: I,
  page_size: usize,
  restart_interval: u16,
  mapper_map: HashMap<TableIdent, PageMapper>,
  segment_mapper_map: HashMap<TableIdent, PageMapper>,
  root_map: HashMap<TableIdent, PageId>,
  ttl_root_map: HashMap<TableIdent, PageId>,
  large_value_layout: Option<SegmentLayout>,
  large_value_threshold: usize,
  branch_name: String,
  term: u64,
  manifest_file: Option<String>,
  branch_file_limit: Option<FilePageId>,
  branch_segment_limit: Option<FilePageId>,
  pages_per_file_shift: u8,
  manifest_started: HashSet<TableIdent>,
  manifest_size_map: HashMap<TableIdent, usize>,
  manifest_limit: usize,
  ttl_read_filter: bool,
  ttl_next_map: HashMap<TableIdent, u64>,
  compression_level: Option<i32>,
  compression_map: HashMap<TableIdent, DictCompression>,
}

impl<I: PageIo> Store<I> {
  pub fn new(io: I, page_size: usize, restart_interval: u16) -> Self {
    Self {
      io,
      page_size,
      restart_interval,
      mapper_map: HashMap::default(),
      segment_mapper_map: HashMap::default(),
      root_map: HashMap::default(),
      ttl_root_map: HashMap::default(),
      large_value_layout: None,
      large_value_threshold: 0,
      branch_name: "main".to_owned(),
      term: 0,
      manifest_file: None,
      branch_file_limit: None,
      branch_segment_limit: None,
      pages_per_file_shift: 18,
      manifest_started: HashSet::default(),
      manifest_size_map: HashMap::default(),
      manifest_limit: 8 * 1024 * 1024,
      ttl_read_filter: true,
      ttl_next_map: HashMap::default(),
      compression_level: None,
      compression_map: HashMap::default(),
    }
  }

  pub fn with_manifest_limit(mut self, manifest_limit: usize) -> Self {
    self.manifest_limit = manifest_limit;
    self
  }

  /// 读路径是否过滤 `expire_ts <= now` 的 entry（默认 true）。
  ///
  /// C++ eloqstore 的 Read / Scan / Floor 不过滤，过期项由 CleanExpired
  /// 物理删除后才消失；需要逐项对齐 oracle 行为时传 false。
  pub fn with_ttl_read_filter(mut self, enabled: bool) -> Self {
    self.ttl_read_filter = enabled;
    self
  }

  /// 启用写路径值压缩（zstd level）：`>= SKIP_COMPRESSION_THRESHOLD` 的值
  /// 压后更小才落盘，读路径按 entry 的 compression flag 自动解压。
  pub fn with_compression_level(mut self, level: i32) -> Self {
    self.compression_level = Some(level);
    self
  }

  /// 该表当前字典字节；未启用压缩或无字典返回空。
  fn dict_bytes(&self, tbl: &TableIdent) -> &[u8] {
    self
      .compression_map
      .get(tbl)
      .map_or(&[], DictCompression::dictionary_bytes)
  }

  /// 为表装载已有 zstd 字典；之后该表的写入按字典压缩，manifest 会持久化
  /// 字典字节，reopen 时自动恢复。
  pub fn set_dictionary(&mut self, tbl: &TableIdent, dict: Vec<u8>) -> Result<()> {
    let level = self.compression_level.unwrap_or_default();
    let mut compression = self
      .compression_map
      .remove(tbl)
      .unwrap_or_else(|| DictCompression::new(level));
    compression.load_dictionary(dict)?;
    self.compression_map.insert(tbl.clone(), compression);
    Ok(())
  }

  /// 该表是否已装载字典（写入会走 Dictionary 压缩）。
  pub fn has_dictionary(&self, tbl: &TableIdent) -> bool {
    self
      .compression_map
      .get(tbl)
      .is_some_and(DictCompression::has_dictionary)
  }

  /// 让后端预加载 table 目录的 file id -> path 索引，返回索引文件数。
  pub fn preload_paths(&self, tbl: &TableIdent) -> usize {
    self.io.preload_paths(tbl)
  }

  /// 外部同步工具改动目录后主动失效后端路径索引。
  pub fn invalidate_path_cache(&self) {
    self.io.invalidate_path_cache();
  }

  pub fn with_branch(mut self, branch_name: impl Into<String>, term: u64) -> Self {
    let branch_name = branch_name.into();
    let normalized = normalize_branch_name(&branch_name)
      .map(|name| name.into_string())
      .unwrap_or(branch_name);
    self.io.set_file_identity(&normalized, term);
    self.branch_name = normalized;
    self.term = term;
    self
  }

  pub fn with_pages_per_file_shift(mut self, pages_per_file_shift: u8) -> Self {
    self.pages_per_file_shift = pages_per_file_shift;
    self
  }

  /// attach 时更新 active term；后续 manifest / archive 记录使用该 term。
  pub fn set_term(&mut self, term: u64) {
    self.term = term;
    self.io.set_file_identity(&self.branch_name, term);
  }

  /// 启用 Store 级大 value：`value.len() >= threshold` 的 upsert 写入 segment 文件。
  pub fn with_large_value(mut self, layout: SegmentLayout, threshold: usize) -> Self {
    self.large_value_layout = Some(layout);
    self.large_value_threshold = threshold;
    self
  }

  pub fn page_size(&self) -> usize {
    self.page_size
  }

  pub async fn get(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<Vec<u8>>> {
    self.get_at(tbl, key, now_ms()).await
  }

  pub async fn get_at(&self, tbl: &TableIdent, key: &[u8], now_ms: u64) -> Result<Option<Vec<u8>>> {
    let Some(root_id) = self.root_map.get(tbl).copied() else {
      return Ok(None);
    };
    let Some(mapper) = self.mapper_map.get(tbl) else {
      return Ok(None);
    };
    let Some(entry) = read_entry(
      &self.io,
      tbl,
      root_id,
      mapper,
      key,
      self.page_size,
      self.compression_map.get(tbl),
    )
    .await?
    else {
      return Ok(None);
    };
    if self.ttl_read_filter && is_expired(entry.expire_ts, now_ms) {
      return Ok(None);
    }
    Ok(Some(entry.value))
  }

  pub async fn read(&self, tbl: &TableIdent, key: &[u8]) -> Result<Vec<u8>> {
    self.read_at(tbl, key, now_ms()).await
  }

  pub async fn read_at(&self, tbl: &TableIdent, key: &[u8], now_ms: u64) -> Result<Vec<u8>> {
    self
      .get_at(tbl, key, now_ms)
      .await?
      .ok_or(KvError::NotFound)
  }

  pub async fn scan(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    self.scan_at(tbl, begin, end, max_entries, now_ms()).await
  }

  pub async fn scan_at(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    now_ms: u64,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    self
      .scan_opts(tbl, begin, end, max_entries, now_ms, ScanOptions::default())
      .await
  }

  pub async fn scan_with_options(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    options: ScanOptions,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    self
      .scan_opts(tbl, begin, end, max_entries, now_ms(), options)
      .await
  }

  pub async fn scan_page(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    options: ScanOptions,
    page_entries: usize,
    page_size: usize,
  ) -> Result<ScanPageResult> {
    self
      .scan_page_at(
        tbl,
        begin,
        end,
        now_ms(),
        options,
        PageOptions {
          page_entries,
          page_size,
        },
      )
      .await
  }

  pub async fn scan_page_at(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    now_ms: u64,
    options: ScanOptions,
    page: PageOptions,
  ) -> Result<ScanPageResult> {
    let Some(root_id) = self.root_map.get(tbl).copied() else {
      return Ok(ScanPageResult::default());
    };
    let Some(mapper) = self.mapper_map.get(tbl) else {
      return Ok(ScanPageResult::default());
    };
    let mut entry_li = Vec::new();
    let mut result_size = 0usize;
    let mut has_remaining = false;
    let mut cursor = begin.to_vec();
    let mut cursor_inclusive = options.begin_inclusive;
    loop {
      let remaining_entries = if page.page_entries == 0 {
        0
      } else {
        page.page_entries - entry_li.len()
      };
      let task_page = scan_page_with(
        &self.io,
        ScanArgs {
          tbl,
          root_id,
          mapper,
          begin: &cursor,
          end,
          page_size: self.page_size,
          max_entries: 0,
          dict: self.compression_map.get(tbl),
        },
        fkv_task_scan::ScanOptions {
          begin_inclusive: cursor_inclusive,
          end_inclusive: options.end_inclusive,
          reject_large_value: options.reject_large_value,
        },
        PageOptions {
          page_entries: remaining_entries,
          page_size: 0,
        },
      )
      .await?;
      let next_cursor = task_page.entry_li.last().map(|entry| entry.key.clone());
      for entry in task_page.entry_li {
        if self.ttl_read_filter && is_expired(entry.expire_ts, now_ms) {
          continue;
        }
        let entry_size = entry.key.len() + entry.value.len() + 16;
        if page.page_size != 0
          && result_size > 0
          && result_size.saturating_add(entry_size) > page.page_size
        {
          has_remaining = true;
          return Ok(ScanPageResult {
            entry_li,
            has_remaining,
          });
        }
        result_size += entry_size;
        entry_li.push((entry.key, entry.value));
      }
      if !task_page.has_remaining {
        break;
      }
      if page.page_entries != 0 && entry_li.len() >= page.page_entries {
        has_remaining = true;
        break;
      }
      let Some(next_cursor) = next_cursor else {
        break;
      };
      cursor = next_cursor;
      cursor_inclusive = false;
    }
    Ok(ScanPageResult {
      entry_li,
      has_remaining,
    })
  }

  pub async fn scan_opts(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    now_ms: u64,
    options: ScanOptions,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    let Some(root_id) = self.root_map.get(tbl).copied() else {
      return Ok(Vec::new());
    };
    let Some(mapper) = self.mapper_map.get(tbl) else {
      return Ok(Vec::new());
    };
    let entry_li = scan_entries_with(
      &self.io,
      ScanArgs {
        tbl,
        root_id,
        mapper,
        begin,
        end,
        page_size: self.page_size,
        max_entries: 0,
        dict: self.compression_map.get(tbl),
      },
      options,
    )
    .await?;
    let mut result_li = Vec::new();
    for entry in entry_li {
      if self.ttl_read_filter && is_expired(entry.expire_ts, now_ms) {
        continue;
      }
      result_li.push((entry.key, entry.value));
      if max_entries != 0 && result_li.len() >= max_entries {
        break;
      }
    }
    Ok(result_li)
  }

  fn branch_metadata(&self, tbl: &TableIdent) -> BranchManifestMetadata {
    let mut metadata = BranchManifestMetadata::new(self.branch_name.clone(), self.term);
    let segment_shift = self
      .large_value_layout
      .map_or(7, |layout| layout.segments_per_file_shift());
    let max_file_id = self
      .mapper_map
      .get(tbl)
      .map_or(0, |mapper| mapper.max_fp_id() >> self.pages_per_file_shift);
    let max_segment_file_id = self
      .segment_mapper_map
      .get(tbl)
      .map_or(0, |mapper| mapper.max_fp_id() >> segment_shift);
    if self.mapper_map.contains_key(tbl) || self.segment_mapper_map.contains_key(tbl) {
      metadata.file_ranges.push(BranchFileRange {
        branch_name: self.branch_name.clone().into(),
        term: self.term,
        max_file_id,
        max_segment_file_id,
      });
    }
    metadata
  }

  fn snapshot_record(
    &self,
    tbl: &TableIdent,
    root_id: PageId,
    ttl_root_id: PageId,
    max_fp_id: Option<FilePageId>,
    max_segment_fp_id: Option<FilePageId>,
    branch_metadata: &BranchManifestMetadata,
  ) -> Result<Vec<u8>> {
    let (mapping_value_li, mapper_max_fp) =
      self.mapper_map.get(tbl).map_or((&[][..], 0), |mapper| {
        (mapper.tbl().value_li(), mapper.max_fp_id())
      });
    let (segment_value_li, mapper_max_segment_fp) =
      self
        .segment_mapper_map
        .get(tbl)
        .map_or((None, 0), |segment_mapper| {
          (
            Some(segment_mapper.tbl().value_li()),
            segment_mapper.max_fp_id(),
          )
        });
    ManifestBuilder::new().snapshot(SnapshotArgs {
      root_id,
      ttl_root: ttl_root_id,
      max_fp_id: max_fp_id.unwrap_or(mapper_max_fp),
      dict_bytes: self.dict_bytes(tbl),
      mapping_value_li,
      branch_metadata,
      segment_value_li,
      max_segment_fp_id: max_segment_fp_id.unwrap_or(mapper_max_segment_fp),
    })
  }

  fn check_branch_segment_limit(&self, tbl: &TableIdent, batch_len: usize) -> Result<()> {
    let Some(limit) = self.branch_segment_limit else {
      return Ok(());
    };
    let segment_budget = 64u64.saturating_add((batch_len as u64).saturating_mul(4));
    let mapper_max = self
      .segment_mapper_map
      .get(tbl)
      .map_or(0, PageMapper::max_fp_id);
    if mapper_max.saturating_add(segment_budget) > limit {
      return Err(KvError::OutOfSpace);
    }
    Ok(())
  }

  async fn capture_old_large(
    &self,
    tbl: &TableIdent,
    batch: &[BatchEntry],
  ) -> Result<Vec<(Vec<u8>, Vec<u8>, u64)>> {
    let mut old_large_li = Vec::new();
    let (Some(root_id), Some(mapper)) = (self.root_map.get(tbl).copied(), self.mapper_map.get(tbl))
    else {
      return Ok(old_large_li);
    };
    let mut seen = HashSet::default();
    for entry in batch {
      if !seen.insert(entry.key.clone()) {
        continue;
      }
      if let Some(old) = read_entry(
        &self.io,
        tbl,
        root_id,
        mapper,
        &entry.key,
        self.page_size,
        None,
      )
      .await?
        && old.is_large_value
      {
        old_large_li.push((entry.key.clone(), old.value, old.timestamp));
      }
    }
    Ok(old_large_li)
  }

  pub fn page_io(&self) -> &I {
    &self.io
  }

  pub fn mapper(&self, tbl: &TableIdent) -> Option<&PageMapper> {
    self.mapper_map.get(tbl)
  }

  pub fn segment_mapper(&self, tbl: &TableIdent) -> Option<&PageMapper> {
    self.segment_mapper_map.get(tbl)
  }

  pub fn root_id(&self, tbl: &TableIdent) -> Option<PageId> {
    self.root_map.get(tbl).copied()
  }

  pub fn ttl_root_id(&self, tbl: &TableIdent) -> Option<PageId> {
    self.ttl_root_map.get(tbl).copied()
  }

  pub fn branch_file_limit(&self) -> Option<FilePageId> {
    self.branch_file_limit
  }

  pub fn branch_segment_limit(&self) -> Option<FilePageId> {
    self.branch_segment_limit
  }

  pub fn into_io(self) -> I {
    self.io
  }
}

impl<I: PageIo + ManifestIo> Store<I> {
  fn note_next_expire(&mut self, tbl: &TableIdent, expire_ts: u64) {
    if expire_ts == 0 {
      return;
    }
    let next = self.ttl_next_map.entry(tbl.clone()).or_insert(expire_ts);
    if expire_ts < *next {
      *next = expire_ts;
    }
  }

  fn record_next_expire(&mut self, tbl: &TableIdent, next_expire_ts: u64) {
    if next_expire_ts == 0 {
      self.ttl_next_map.remove(tbl);
    } else {
      self.ttl_next_map.insert(tbl.clone(), next_expire_ts);
    }
  }

  fn note_ttl_batch(&mut self, tbl: &TableIdent, ttl_li: &[BatchEntry]) {
    for entry in ttl_li {
      if entry.op == WriteOp::Upsert
        && let Ok((expire_ts, _)) = parse_ttl_key(&entry.key)
      {
        self.note_next_expire(tbl, expire_ts);
      }
    }
  }

  /// 写后触发：只在已知最早过期时间已到时扫描 TTL 树，对齐 C++ TriggerTTL。
  async fn maybe_clean_expired(&mut self, tbl: &TableIdent, now_ms: u64) -> Result<()> {
    let next = self.ttl_next_map.get(tbl).copied().unwrap_or(0);
    if next == 0 || next > now_ms {
      return Ok(());
    }
    self.clean_expired(tbl, now_ms).await?;
    Ok(())
  }

  pub async fn put(&mut self, tbl: &TableIdent, batch: &[BatchEntry]) -> Result<()> {
    self.put_at(tbl, batch, now_ms()).await
  }

  /// 带时间注入的写入：对齐 C++ BatchWriteTask，`expire_ts <= now_ms` 的
  /// upsert 不写主树，只删除对应 TTL 索引项。
  pub async fn put_at(
    &mut self,
    tbl: &TableIdent,
    batch: &[BatchEntry],
    now_ms: u64,
  ) -> Result<()> {
    let PreparedWriteBatch {
      live_li,
      ttl_li,
      expired_num: _,
    } = prepare_write_batch(batch, now_ms);
    if live_li.is_empty() && ttl_li.is_empty() {
      return Ok(());
    }
    // 启用压缩时，首次写为该表建立 DictCompression（无字典，standalone 路径）。
    if let Some(level) = self.compression_level {
      self
        .compression_map
        .entry(tbl.clone())
        .or_insert_with(|| DictCompression::new(level));
    }
    let root_id = self.root_map.get(tbl).copied().unwrap_or(PageId::MAX);
    let ttl_root_id = self.ttl_root_map.get(tbl).copied().unwrap_or(PageId::MAX);
    let branch_mode = self.manifest_file.is_some();
    if let Some(limit) = self.branch_file_limit {
      let page_budget = 64u64.saturating_add((batch.len() as u64).saturating_mul(4));
      let mapper_max = self.mapper_map.get(tbl).map_or(0, PageMapper::max_fp_id);
      if mapper_max.saturating_add(page_budget) > limit {
        return Err(KvError::OutOfSpace);
      }
    }
    let first_manifest = !branch_mode && !self.manifest_started.contains(tbl);
    let (new_root, new_ttl_root) = {
      let mapper = self
        .mapper_map
        .entry(tbl.clone())
        .or_insert_with(|| PageMapper::new(AllocMode::Append, 0));
      let new_root = if live_li.is_empty() {
        root_id
      } else {
        write_batch(
          &self.io,
          WriteArgs {
            tbl,
            root_id,
            mapper,
            batch: &live_li,
            page_size: self.page_size,
            restart_interval: self.restart_interval,
            dict: self.compression_map.get(tbl),
          },
        )
        .await?
      };
      let new_ttl_root = if ttl_li.is_empty() {
        ttl_root_id
      } else {
        write_batch(
          &self.io,
          WriteArgs {
            tbl,
            root_id: ttl_root_id,
            mapper,
            batch: &ttl_li,
            page_size: self.page_size,
            restart_interval: self.restart_interval,
            dict: self.compression_map.get(tbl),
          },
        )
        .await?
      };
      (new_root, new_ttl_root)
    };

    if branch_mode {
      self
        .write_snapshot_manifest(tbl, new_root, new_ttl_root)
        .await?;
      self.root_map.insert(tbl.clone(), new_root);
      self.ttl_root_map.insert(tbl.clone(), new_ttl_root);
      self.note_ttl_batch(tbl, &ttl_li);
      // TTL 清理是提交后的维护动作；失败不影响写入，等下一次写或显式 clean。
      let _ = self.maybe_clean_expired(tbl, now_ms).await;
      return Ok(());
    }
    let branch = self.branch_metadata(tbl);
    if first_manifest {
      let record = {
        let mapper = self.mapper_map.get(tbl).ok_or(KvError::ResourceMissing)?;
        let (segment_value_li, max_segment_fp_id) =
          self
            .segment_mapper_map
            .get(tbl)
            .map_or((None, 0), |segment_mapper| {
              (
                Some(segment_mapper.tbl().value_li()),
                segment_mapper.max_fp_id(),
              )
            });
        ManifestBuilder::new().snapshot(SnapshotArgs {
          root_id: new_root,
          ttl_root: new_ttl_root,
          max_fp_id: mapper.max_fp_id(),
          dict_bytes: self.dict_bytes(tbl),
          mapping_value_li: mapper.tbl().value_li(),
          branch_metadata: &branch,
          segment_value_li,
          max_segment_fp_id,
        })?
      };
      self.io.switch_manifest(tbl, &record).await?;
      self.manifest_size_map.insert(tbl.clone(), record.len());
      self.manifest_started.insert(tbl.clone());
      if let Some(mapper) = self.mapper_map.get_mut(tbl) {
        let _ = mapper.take_dirty_page_li();
      }
    } else {
      self.append_mapping_log(tbl, new_root, new_ttl_root).await?;
    }
    self.root_map.insert(tbl.clone(), new_root);
    self.ttl_root_map.insert(tbl.clone(), new_ttl_root);
    self.note_ttl_batch(tbl, &ttl_li);
    // TTL 清理是提交后的维护动作；失败不影响写入，等下一次写或显式 clean。
    let _ = self.maybe_clean_expired(tbl, now_ms).await;
    Ok(())
  }

  async fn free_old_large(
    &mut self,
    tbl: &TableIdent,
    old_large_li: &[(Vec<u8>, Vec<u8>, u64)],
    timestamp_map: &HashMap<Vec<u8>, u64>,
    layout: SegmentLayout,
  ) -> Result<()> {
    if old_large_li.is_empty() {
      return Ok(());
    }
    let mut freed_any = false;
    if let Some(segment_mapper) = self.segment_mapper_map.get_mut(tbl) {
      for (key, content, old_ts) in old_large_li {
        if timestamp_map.get(key).is_none_or(|ts| *ts < *old_ts) {
          continue;
        }
        if free_large_value(segment_mapper, content, layout.segment_size())? > 0 {
          freed_any = true;
        }
      }
    }
    if freed_any {
      let root_id = self.root_map.get(tbl).copied().unwrap_or(PageId::MAX);
      let ttl_root_id = self.ttl_root_map.get(tbl).copied().unwrap_or(PageId::MAX);
      self.append_mapping_log(tbl, root_id, ttl_root_id).await?;
    }
    Ok(())
  }

  async fn append_mapping_log(
    &mut self,
    tbl: &TableIdent,
    root_id: PageId,
    ttl_root_id: PageId,
  ) -> Result<()> {
    if self.manifest_file.is_some() {
      return self
        .write_snapshot_manifest(tbl, root_id, ttl_root_id)
        .await;
    }
    let branch = self.branch_metadata(tbl);
    let record = {
      let mapper = self
        .mapper_map
        .get_mut(tbl)
        .ok_or(KvError::ResourceMissing)?;
      let dirty_li = mapper.take_dirty_page_li();
      let mut builder = ManifestBuilder::new();
      for page_id in dirty_li {
        builder.update_mapping(page_id, mapper.tbl().get(page_id)?);
      }
      builder.append_branch_metadata(&branch);
      let segment_entries = if let Some(segment_mapper) = self.segment_mapper_map.get_mut(tbl) {
        let mut bytes = Vec::new();
        for page_id in segment_mapper.take_dirty_page_li() {
          append_varint32(&mut bytes, page_id);
          append_varint64(&mut bytes, segment_mapper.tbl().get(page_id)?);
        }
        bytes
      } else {
        Vec::new()
      };
      let mut segment_delta = Vec::with_capacity(4 + segment_entries.len());
      segment_delta.extend_from_slice(&(segment_entries.len() as u32).to_le_bytes());
      segment_delta.extend_from_slice(&segment_entries);
      builder.append_segment_mapping(&segment_delta);
      builder.finalize(root_id, ttl_root_id)?
    };
    let current_size = self.manifest_size_map.get(tbl).copied().unwrap_or(0);
    if self.manifest_limit != 0 && current_size.saturating_add(record.len()) > self.manifest_limit {
      let snapshot = {
        let mapper = self.mapper_map.get(tbl).ok_or(KvError::ResourceMissing)?;
        let (segment_value_li, max_segment_fp_id) =
          self
            .segment_mapper_map
            .get(tbl)
            .map_or((None, 0), |segment_mapper| {
              (
                Some(segment_mapper.tbl().value_li()),
                segment_mapper.max_fp_id(),
              )
            });
        ManifestBuilder::new().snapshot(SnapshotArgs {
          root_id,
          ttl_root: ttl_root_id,
          max_fp_id: mapper.max_fp_id(),
          dict_bytes: self.dict_bytes(tbl),
          mapping_value_li: mapper.tbl().value_li(),
          branch_metadata: &branch,
          segment_value_li,
          max_segment_fp_id,
        })?
      };
      self.io.switch_manifest(tbl, &snapshot).await?;
      self.manifest_size_map.insert(tbl.clone(), snapshot.len());
      if let Some(mapper) = self.mapper_map.get_mut(tbl) {
        let _ = mapper.take_dirty_page_li();
      }
    } else {
      self.io.append_manifest(tbl, &record).await?;
      self
        .manifest_size_map
        .insert(tbl.clone(), current_size.saturating_add(record.len()));
    }
    Ok(())
  }

  pub async fn delete(&mut self, tbl: &TableIdent, key: &[u8], timestamp: u64) -> Result<()> {
    self
      .put(tbl, &[BatchEntry::delete(key.to_vec(), timestamp)])
      .await
  }

  /// 切换到一个 branch manifest 并打开独立 Store 状态。
  /// Attach 前重置 Store：清空所有 table 的缓存状态，并设置 branch manifest 模式。
  pub fn prepare_attach(&mut self, branch_name: &str, term: u64) -> Result<()> {
    let normalized = normalize_branch_name(branch_name).ok_or(KvError::InvalidArgs)?;
    self.reset_tables();
    if normalized.as_ref() == "main" {
      self.branch_name = "main".to_owned();
      self.manifest_file = None;
    } else {
      self.manifest_file = Some(manifest_file_name(&normalized, 0));
      self.branch_name = normalized.into_string();
    }
    self.branch_file_limit = None;
    self.branch_segment_limit = None;
    self.term = term;
    self.io.set_file_identity(&self.branch_name, term);
    Ok(())
  }

  fn reset_tables(&mut self) {
    self.mapper_map.clear();
    self.segment_mapper_map.clear();
    self.root_map.clear();
    self.ttl_root_map.clear();
    self.manifest_started.clear();
    self.manifest_size_map.clear();
  }

  pub async fn open_branch(
    &mut self,
    tbl: &TableIdent,
    branch_name: &str,
    term: u64,
  ) -> Result<()> {
    let normalized = normalize_branch_name(branch_name).ok_or(KvError::InvalidArgs)?;
    let manifest_file = manifest_file_name(&normalized, term);
    let bytes = self
      .io
      .read_manifest_file(tbl, &manifest_file)
      .await?
      .ok_or(KvError::ResourceMissing)?;
    let state = fkv_task_reopen::replay_manifest_bytes(&bytes)?;
    let size = bytes.len();
    let branch_base_file = (state.max_fp_id >> self.pages_per_file_shift)
      .div_euclid(BRANCH_FILE_RESERVE_FILE_NUM)
      * BRANCH_FILE_RESERVE_FILE_NUM;
    let branch_limit_file = branch_base_file
      .checked_add(BRANCH_FILE_RESERVE_FILE_NUM)
      .ok_or(KvError::InvalidArgs)?;
    let branch_file_limit = branch_limit_file
      .checked_shl(u32::from(self.pages_per_file_shift))
      .ok_or(KvError::InvalidArgs)?;
    let segment_shift = self
      .large_value_layout
      .map_or(7, |layout| layout.segments_per_file_shift());
    let branch_segment_limit = if state.max_segment_fp_id > 0 {
      let base_file = (state.max_segment_fp_id >> segment_shift)
        .div_euclid(BRANCH_FILE_RESERVE_FILE_NUM)
        * BRANCH_FILE_RESERVE_FILE_NUM;
      let limit_file = base_file
        .checked_add(BRANCH_FILE_RESERVE_FILE_NUM)
        .ok_or(KvError::InvalidArgs)?;
      Some(
        limit_file
          .checked_shl(u32::from(segment_shift))
          .ok_or(KvError::InvalidArgs)?,
      )
    } else {
      None
    };
    self.branch_name = normalized.clone().into_string();
    self.term = term;
    self.io.set_file_identity(normalized.as_ref(), term);
    self.manifest_file = Some(manifest_file);
    self.branch_file_limit = Some(branch_file_limit);
    self.branch_segment_limit = branch_segment_limit;
    self.install_replay_state(tbl, state, size);
    Ok(())
  }

  async fn write_snapshot_manifest(
    &mut self,
    tbl: &TableIdent,
    root_id: PageId,
    ttl_root_id: PageId,
  ) -> Result<()> {
    let branch = self.branch_metadata(tbl);
    let record = self.snapshot_record(tbl, root_id, ttl_root_id, None, None, &branch)?;
    match self.manifest_file.clone() {
      Some(name) => {
        self.io.write_manifest_file(tbl, &name, &record).await?;
      }
      None => {
        self.io.switch_manifest(tbl, &record).await?;
      }
    }
    self.manifest_size_map.insert(tbl.clone(), record.len());
    self.manifest_started.insert(tbl.clone());
    if let Some(mapper) = self.mapper_map.get_mut(tbl) {
      let _ = mapper.take_dirty_page_li();
    }
    if let Some(segment_mapper) = self.segment_mapper_map.get_mut(tbl) {
      let _ = segment_mapper.take_dirty_page_li();
    }
    Ok(())
  }

  /// 按 `ResourceMissing` 重试 reopen；branch 模式同样生效。
  pub async fn auto_reopen_with_sleeper<S: fkv_task_reopen::Sleeper>(
    &mut self,
    tbl: &TableIdent,
    options: fkv_task_reopen::AutoReopenOptions,
    sleeper: &S,
  ) -> Result<()> {
    let mut remaining = options.retry_times;
    loop {
      match self.reopen(tbl).await {
        Ok(()) => return Ok(()),
        Err(KvError::ResourceMissing) if remaining > 0 => {
          sleeper.sleep(options.pending_time).await;
          remaining -= 1;
        }
        Err(err) => return Err(err),
      }
    }
  }

  pub async fn reopen(&mut self, tbl: &TableIdent) -> Result<()> {
    let (state, size) = match self.manifest_file.clone() {
      Some(name) => {
        let bytes = self
          .io
          .read_manifest_file(tbl, &name)
          .await?
          .ok_or(KvError::ResourceMissing)?;
        (fkv_task_reopen::replay_manifest_bytes(&bytes)?, bytes.len())
      }
      None => fkv_task_reopen::reopen_with_size(&self.io, tbl).await?,
    };
    self.install_replay_state(tbl, state, size);
    Ok(())
  }

  fn install_replay_state(
    &mut self,
    tbl: &TableIdent,
    state: fkv_recovery::ReplayState,
    size: usize,
  ) {
    let mapper = PageMapper::with_tbl(state.mapping_tbl, AllocMode::Append, state.max_fp_id);
    self.mapper_map.insert(tbl.clone(), mapper);
    // 恢复字典：即便当前未启用压缩，也要能读回旧盘面里字典压缩的页。
    if !state.dict_bytes.is_empty() {
      let level = self.compression_level.unwrap_or_default();
      let mut dict = DictCompression::new(level);
      if dict.load_dictionary(state.dict_bytes.clone()).is_ok() {
        self.compression_map.insert(tbl.clone(), dict);
      }
    }
    if !state.segment_mapping.is_empty() || state.max_segment_fp_id > 0 {
      self.segment_mapper_map.insert(
        tbl.clone(),
        PageMapper::with_tbl(
          state.segment_mapping,
          AllocMode::Append,
          state.max_segment_fp_id,
        ),
      );
    }
    self.root_map.insert(tbl.clone(), state.root_id);
    self.ttl_root_map.insert(tbl.clone(), state.ttl_root_id);
    self.manifest_started.insert(tbl.clone());
    self.manifest_size_map.insert(tbl.clone(), size);
    // 对齐 C++ reopen：next_expire_ts 不从 manifest 恢复，等新 TTL 写入/清理重建。
    self.ttl_next_map.remove(tbl);
  }

  /// 整表清空：释放两棵树占用的物理页，root / ttl_root 归零，并重置 manifest snapshot。
  ///
  /// data 文件不立即删除，留给 `gc` 回收；逻辑 page id 表保留为 INVALID，
  /// 避免重启后逻辑 id 与旧文件冲突。
  pub async fn truncate(&mut self, tbl: &TableIdent) -> Result<()> {
    let branch = self.branch_metadata(tbl);
    let dict_bytes = self.dict_bytes(tbl).to_vec();
    let Some(mapper) = self.mapper_map.get_mut(tbl) else {
      self.ttl_next_map.remove(tbl);
      return Ok(());
    };
    let page_num = mapper.tbl().len();
    for page_id in 0..page_num as PageId {
      let _ = mapper.free_page(page_id)?;
    }
    let _ = mapper.take_dirty_page_li();
    let _ = mapper.take_garbage_file_page_li();
    if let Some(segment_mapper) = self.segment_mapper_map.get_mut(tbl) {
      let segment_num = segment_mapper.tbl().len();
      for page_id in 0..segment_num as PageId {
        let _ = segment_mapper.free_page(page_id)?;
      }
      let _ = segment_mapper.take_dirty_page_li();
      let _ = segment_mapper.take_garbage_file_page_li();
    }
    let record = {
      let (segment_value_li, max_segment_fp_id) =
        self
          .segment_mapper_map
          .get(tbl)
          .map_or((None, 0), |segment_mapper| {
            (
              Some(segment_mapper.tbl().value_li()),
              segment_mapper.max_fp_id(),
            )
          });
      ManifestBuilder::new().snapshot(SnapshotArgs {
        root_id: PageId::MAX,
        ttl_root: PageId::MAX,
        max_fp_id: mapper.max_fp_id(),
        dict_bytes: &dict_bytes,
        mapping_value_li: mapper.tbl().value_li(),
        branch_metadata: &branch,
        segment_value_li,
        max_segment_fp_id,
      })?
    };
    match self.manifest_file.clone() {
      Some(name) => {
        self.io.write_manifest_file(tbl, &name, &record).await?;
      }
      None => {
        self.io.switch_manifest(tbl, &record).await?;
      }
    }
    self.root_map.insert(tbl.clone(), PageId::MAX);
    self.ttl_root_map.insert(tbl.clone(), PageId::MAX);
    self.manifest_started.insert(tbl.clone());
    self.manifest_size_map.insert(tbl.clone(), record.len());
    self.ttl_next_map.remove(tbl);
    Ok(())
  }

  /// 删除所有 key >= position；position 为空等价整表 truncate。
  pub async fn truncate_from(&mut self, tbl: &TableIdent, position: &[u8]) -> Result<usize> {
    if position.is_empty() {
      self.truncate(tbl).await?;
      return Ok(0);
    }
    let Some(root_id) = self.root_map.get(tbl).copied() else {
      return Ok(0);
    };
    let Some(mapper) = self.mapper_map.get(tbl) else {
      return Ok(0);
    };
    let entry_li = scan_entries_with(
      &self.io,
      ScanArgs {
        tbl,
        root_id,
        mapper,
        begin: position,
        end: &[],
        page_size: self.page_size,
        max_entries: 0,
        dict: self.compression_map.get(tbl),
      },
      ScanOptions {
        reject_large_value: false,
        ..ScanOptions::default()
      },
    )
    .await?;
    if entry_li.is_empty() {
      return Ok(0);
    }
    let delete_li: Vec<BatchEntry> = entry_li
      .into_iter()
      .map(|entry| BatchEntry::delete(entry.key, u64::MAX))
      .collect();
    let deleted = delete_li.len();
    self.put(tbl, &delete_li).await?;
    Ok(deleted)
  }

  /// 清理 TTL tree 中 `expire_ts <= now_ms` 的条目，并删除对应主树 entry。
  ///
  /// 只有主树 entry 的 `expire_ts` 与 TTL key 一致时才删除，避免误删被覆盖的新值。
  pub async fn clean_expired(
    &mut self,
    tbl: &TableIdent,
    now_ms: u64,
  ) -> Result<CleanExpiredStats> {
    let (Some(&root_id), Some(&ttl_root_id)) = (self.root_map.get(tbl), self.ttl_root_map.get(tbl))
    else {
      self.record_next_expire(tbl, 0);
      return Ok(CleanExpiredStats::default());
    };
    if ttl_root_id == PageId::MAX {
      self.record_next_expire(tbl, 0);
      return Ok(CleanExpiredStats::default());
    }
    let entry_li = {
      let mapper = self.mapper_map.get(tbl).ok_or(KvError::ResourceMissing)?;
      scan_entries(
        &self.io,
        ScanArgs {
          tbl,
          root_id: ttl_root_id,
          mapper,
          begin: &[],
          end: &[],
          page_size: self.page_size,
          max_entries: 0,
          dict: self.compression_map.get(tbl),
        },
      )
      .await?
    };
    let mut data_delete_li = Vec::new();
    let mut ttl_delete_li = Vec::new();
    let mut next_expire_ts = 0;
    for entry in entry_li {
      let Ok((expire_ts, user_key)) = parse_ttl_key(&entry.key) else {
        // 坏 TTL key 直接清掉，避免永久占空间。
        ttl_delete_li.push(BatchEntry::delete(entry.key, now_ms));
        continue;
      };
      if expire_ts > now_ms {
        if next_expire_ts == 0 {
          next_expire_ts = expire_ts;
        }
        continue;
      }
      let mapper = self.mapper_map.get(tbl).ok_or(KvError::ResourceMissing)?;
      if let Some(data_entry) = read_entry(
        &self.io,
        tbl,
        root_id,
        mapper,
        user_key,
        self.page_size,
        None,
      )
      .await?
        && data_entry.expire_ts == expire_ts
      {
        data_delete_li.push(BatchEntry::delete(user_key.to_vec(), now_ms));
      }
      ttl_delete_li.push(BatchEntry::delete(entry.key, now_ms));
    }
    if ttl_delete_li.is_empty() {
      self.record_next_expire(tbl, next_expire_ts);
      return Ok(CleanExpiredStats {
        deleted: 0,
        ttl_deleted: 0,
        next_expire_ts,
      });
    }
    let (new_root, new_ttl_root) = {
      let mapper = self
        .mapper_map
        .get_mut(tbl)
        .ok_or(KvError::ResourceMissing)?;
      let new_root = if data_delete_li.is_empty() {
        root_id
      } else {
        write_batch(
          &self.io,
          WriteArgs {
            tbl,
            root_id,
            mapper,
            batch: &data_delete_li,
            page_size: self.page_size,
            restart_interval: self.restart_interval,
            dict: self.compression_map.get(tbl),
          },
        )
        .await?
      };
      let new_ttl_root = write_batch(
        &self.io,
        WriteArgs {
          tbl,
          root_id: ttl_root_id,
          mapper,
          batch: &ttl_delete_li,
          page_size: self.page_size,
          restart_interval: self.restart_interval,
          dict: self.compression_map.get(tbl),
        },
      )
      .await?;
      (new_root, new_ttl_root)
    };
    self.append_mapping_log(tbl, new_root, new_ttl_root).await?;
    self.root_map.insert(tbl.clone(), new_root);
    self.ttl_root_map.insert(tbl.clone(), new_ttl_root);
    self.record_next_expire(tbl, next_expire_ts);
    Ok(CleanExpiredStats {
      deleted: data_delete_li.len(),
      ttl_deleted: ttl_delete_li.len(),
      next_expire_ts,
    })
  }

  /// 生成一份 archive manifest，并按 tag 倒序保留最近 `num_retained` 份。
  pub async fn create_archive(
    &mut self,
    tbl: &TableIdent,
    tag: &str,
    num_retained: u32,
  ) -> Result<ArchiveInfo> {
    if !is_safe_tag(tag) {
      return Err(KvError::InvalidArgs);
    }
    let branch = normalize_branch_name(&self.branch_name).ok_or(KvError::InvalidArgs)?;
    let root_id = self.root_map.get(tbl).copied().unwrap_or(PageId::MAX);
    let ttl_root_id = self.ttl_root_map.get(tbl).copied().unwrap_or(PageId::MAX);
    let branch_metadata = self.branch_metadata(tbl);
    let record = self.snapshot_record(tbl, root_id, ttl_root_id, None, None, &branch_metadata)?;
    let name = archive_file_name(&branch, self.term, tag);
    self.io.write_manifest_file(tbl, &name, &record).await?;
    let removed_num = self.prune_archives(tbl, &branch, num_retained).await?;
    Ok(ArchiveInfo {
      name,
      bytes: record.len(),
      removed_num,
    })
  }

  /// 创建 branch manifest：继承当前 mapping，max_fp_id 跳到下一个文件避免与父 branch 冲突。
  pub async fn create_branch(&mut self, tbl: &TableIdent, branch_name: &str) -> Result<BranchInfo> {
    let normalized = normalize_branch_name(branch_name).ok_or(KvError::InvalidArgs)?;
    let active = normalize_branch_name(&self.branch_name).ok_or(KvError::InvalidArgs)?;
    if normalized == active || normalized.as_ref() == "main" {
      return Err(KvError::InvalidArgs);
    }
    let parent_max_fp_id = self.mapper_map.get(tbl).map_or(0, PageMapper::max_fp_id);
    let next_file_id = (parent_max_fp_id >> self.pages_per_file_shift)
      .checked_add(1)
      .ok_or(KvError::InvalidArgs)?;
    let branch_base_file = next_file_id
      .div_ceil(BRANCH_FILE_RESERVE_FILE_NUM)
      .checked_mul(BRANCH_FILE_RESERVE_FILE_NUM)
      .ok_or(KvError::InvalidArgs)?;
    let branch_limit_file = branch_base_file
      .checked_add(BRANCH_FILE_RESERVE_FILE_NUM)
      .ok_or(KvError::InvalidArgs)?;
    let new_max_fp_id = branch_base_file
      .checked_shl(u32::from(self.pages_per_file_shift))
      .ok_or(KvError::InvalidArgs)?;
    let branch_limit_fp_id = branch_limit_file
      .checked_shl(u32::from(self.pages_per_file_shift))
      .ok_or(KvError::InvalidArgs)?;
    let segment_shift = self
      .large_value_layout
      .map_or(7, |layout| layout.segments_per_file_shift());
    let parent_segment_max_fp = self
      .segment_mapper_map
      .get(tbl)
      .map_or(0, PageMapper::max_fp_id);
    let next_segment_file_id = (parent_segment_max_fp >> segment_shift)
      .checked_add(1)
      .ok_or(KvError::InvalidArgs)?;
    let branch_segment_base_file = next_segment_file_id
      .div_ceil(BRANCH_FILE_RESERVE_FILE_NUM)
      .checked_mul(BRANCH_FILE_RESERVE_FILE_NUM)
      .ok_or(KvError::InvalidArgs)?;
    let branch_segment_limit_file = branch_segment_base_file
      .checked_add(BRANCH_FILE_RESERVE_FILE_NUM)
      .ok_or(KvError::InvalidArgs)?;
    let branch_segment_fp_id = branch_segment_base_file
      .checked_shl(u32::from(segment_shift))
      .ok_or(KvError::InvalidArgs)?;
    let branch_segment_limit_fp_id = branch_segment_limit_file
      .checked_shl(u32::from(segment_shift))
      .ok_or(KvError::InvalidArgs)?;
    let mut metadata = BranchManifestMetadata::new(normalized.clone(), 0);
    metadata.file_ranges.push(BranchFileRange {
      branch_name: normalized.clone(),
      term: 0,
      max_file_id: new_max_fp_id >> self.pages_per_file_shift,
      max_segment_file_id: branch_segment_base_file,
    });
    // 先确保 segment mapper 存在，branch snapshot 才会写入 segment 区间。
    self
      .segment_mapper_map
      .entry(tbl.clone())
      .or_insert_with(|| PageMapper::new(AllocMode::Append, 0));
    let root_id = self.root_map.get(tbl).copied().unwrap_or(PageId::MAX);
    let ttl_root_id = self.ttl_root_map.get(tbl).copied().unwrap_or(PageId::MAX);
    let record = self.snapshot_record(
      tbl,
      root_id,
      ttl_root_id,
      Some(new_max_fp_id),
      Some(branch_segment_fp_id),
      &metadata,
    )?;
    let name = manifest_file_name(&normalized, 0);
    // 父 branch 跳过整个预留区间（data + segment），先把新 max 持久化；
    // 即使随后 branch manifest 写入失败，也只是浪费预留空间，不会重叠。
    if let Some(mapper) = self.mapper_map.get_mut(tbl) {
      mapper.reserve_until(branch_limit_fp_id);
    }
    let segment_mapper = self
      .segment_mapper_map
      .entry(tbl.clone())
      .or_insert_with(|| PageMapper::new(AllocMode::Append, 0));
    segment_mapper.reserve_until(branch_segment_limit_fp_id);
    self
      .write_snapshot_manifest(tbl, root_id, ttl_root_id)
      .await?;
    self.io.write_manifest_file(tbl, &name, &record).await?;
    Ok(BranchInfo {
      name,
      bytes: record.len(),
    })
  }

  /// 删除 branch 的所有 manifest / archive 文件（不允许 main 或 active branch）。
  pub async fn delete_branch(&mut self, tbl: &TableIdent, branch_name: &str) -> Result<usize> {
    let normalized = normalize_branch_name(branch_name).ok_or(KvError::InvalidArgs)?;
    let active = normalize_branch_name(&self.branch_name).ok_or(KvError::InvalidArgs)?;
    if normalized.as_ref() == "main" || normalized == active {
      return Err(KvError::InvalidArgs);
    }
    let mut removed = 0;
    for name in self.io.list_manifest_file_name_li(tbl).await? {
      let Some((branch, ..)) = parse_manifest_file_name(&name) else {
        continue;
      };
      if branch == normalized.as_ref() {
        self.io.remove_manifest_file(tbl, &name).await?;
        removed += 1;
      }
    }
    Ok(removed)
  }

  async fn prune_archives(
    &self,
    tbl: &TableIdent,
    branch: &str,
    num_retained: u32,
  ) -> Result<usize> {
    if num_retained == 0 {
      return Ok(0);
    }
    let mut archive_li: Vec<(String, String)> = self
      .io
      .list_manifest_file_name_li(tbl)
      .await?
      .into_iter()
      .filter_map(|name| {
        let (archive_branch, _, tag) = parse_manifest_file_name(&name)?;
        let tag = tag?.to_owned();
        if archive_branch == branch {
          Some((tag, name))
        } else {
          None
        }
      })
      .collect();
    archive_li.sort_by(|left, right| right.0.cmp(&left.0));
    let mut removed = 0;
    for (_, name) in archive_li.into_iter().skip(num_retained as usize) {
      self.io.remove_manifest_file(tbl, &name).await?;
      removed += 1;
    }
    Ok(removed)
  }

  pub async fn compact_file(
    &mut self,
    tbl: &TableIdent,
    file_id: fkv_types::FileId,
    pages_per_file_shift: u8,
  ) -> Result<usize> {
    let page_size = self.page_size;
    let moved = {
      let mapper = self
        .mapper_map
        .get_mut(tbl)
        .ok_or(KvError::ResourceMissing)?;
      fkv_task_compact::compact_file(
        &self.io,
        tbl,
        mapper,
        file_id,
        page_size,
        pages_per_file_shift,
      )
      .await?
    };
    let root_id = self.root_map.get(tbl).copied().unwrap_or(PageId::MAX);
    let ttl_root_id = self.ttl_root_map.get(tbl).copied().unwrap_or(PageId::MAX);
    self.append_mapping_log(tbl, root_id, ttl_root_id).await?;
    Ok(moved)
  }
}

impl<I: PageIo + ManifestIo + SegmentIo> Store<I> {
  /// Store 级大 value 写入：超过阈值的 upsert 先写 segment 文件，再写 data page。
  pub async fn put_large(&mut self, tbl: &TableIdent, batch: &[BatchEntry]) -> Result<()> {
    let Some(layout) = self.large_value_layout else {
      return Err(KvError::InvalidArgs);
    };
    self.check_branch_segment_limit(tbl, batch.len())?;
    let timestamp_map = batch_timestamp_map(batch);
    // 覆盖 / 删除前先记下旧大 value 的 encoded content，成功写入后再释放旧 segment。
    let old_large_li = self.capture_old_large(tbl, batch).await?;

    let mut write_li = Vec::with_capacity(batch.len());
    for entry in batch {
      let mut entry = entry.clone();
      if entry.op == WriteOp::Upsert
        && !entry.large_value
        && entry.value.len() >= self.large_value_threshold
      {
        let segment_mapper = self
          .segment_mapper_map
          .entry(tbl.clone())
          .or_insert_with(|| PageMapper::new(AllocMode::Append, 0));
        let written = write_large_value(
          &self.io,
          tbl,
          segment_mapper,
          layout,
          &entry.value,
          &entry.metadata,
        )
        .await?;
        entry.value = written.content;
        entry.large_value = true;
        entry.metadata.clear();
      }
      write_li.push(entry);
    }
    self.put(tbl, &write_li).await?;

    self
      .free_old_large(tbl, &old_large_li, &timestamp_map, layout)
      .await
  }
}

impl<I: PageIo + ManifestIo + FixedSegmentIo> Store<I> {
  /// 与 `put_large` 相同，但大 value 通过调用方注册的 fixed buffer 写出。
  pub async fn put_large_fixed(
    &mut self,
    tbl: &TableIdent,
    batch: &[BatchEntry],
    buf: &mut FixedBuffer,
  ) -> Result<()> {
    let Some(layout) = self.large_value_layout else {
      return Err(KvError::InvalidArgs);
    };
    self.check_branch_segment_limit(tbl, batch.len())?;
    let timestamp_map = batch_timestamp_map(batch);
    let old_large_li = self.capture_old_large(tbl, batch).await?;
    let mut write_li = Vec::with_capacity(batch.len());
    for entry in batch {
      let mut entry = entry.clone();
      if entry.op == WriteOp::Upsert
        && !entry.large_value
        && entry.value.len() >= self.large_value_threshold
      {
        if entry.value.len() > buf.len() {
          return Err(KvError::InvalidArgs);
        }
        buf.as_mut_slice()[..entry.value.len()].copy_from_slice(&entry.value);
        let segment_mapper = self
          .segment_mapper_map
          .entry(tbl.clone())
          .or_insert_with(|| PageMapper::new(AllocMode::Append, 0));
        let written = write_value_fixed_with_len(
          &self.io,
          tbl,
          segment_mapper,
          layout,
          buf,
          entry.value.len(),
          &entry.metadata,
        )
        .await?;
        entry.value = written.content;
        entry.large_value = true;
        entry.metadata.clear();
      }
      write_li.push(entry);
    }
    self.put(tbl, &write_li).await?;
    self
      .free_old_large(tbl, &old_large_li, &timestamp_map, layout)
      .await
  }
}

impl<I: PageIo + FixedSegmentIo> Store<I> {
  pub async fn get_large_into(
    &self,
    tbl: &TableIdent,
    key: &[u8],
    buf: &mut FixedBuffer,
  ) -> Result<Option<LargeValueInto>> {
    self.get_large_into_at(tbl, key, now_ms(), buf).await
  }

  /// 大 value 直接读入 fixed buffer；返回有效长度与 metadata。
  pub async fn get_large_into_at(
    &self,
    tbl: &TableIdent,
    key: &[u8],
    now_ms: u64,
    buf: &mut FixedBuffer,
  ) -> Result<Option<LargeValueInto>> {
    let Some(root_id) = self.root_map.get(tbl).copied() else {
      return Ok(None);
    };
    let Some(mapper) = self.mapper_map.get(tbl) else {
      return Ok(None);
    };
    let Some(entry) = read_entry(
      &self.io,
      tbl,
      root_id,
      mapper,
      key,
      self.page_size,
      self.compression_map.get(tbl),
    )
    .await?
    else {
      return Ok(None);
    };
    if self.ttl_read_filter && is_expired(entry.expire_ts, now_ms) {
      return Ok(None);
    }
    if !entry.is_large_value {
      if entry.value.len() > buf.len() {
        return Err(KvError::InvalidArgs);
      }
      buf.as_mut_slice()[..entry.value.len()].copy_from_slice(&entry.value);
      return Ok(Some(LargeValueInto {
        value_len: entry.value.len(),
        metadata: Vec::new(),
      }));
    }
    let Some(layout) = self.large_value_layout else {
      return Err(KvError::InvalidArgs);
    };
    let Some(segment_mapper) = self.segment_mapper_map.get(tbl) else {
      return Err(KvError::ResourceMissing);
    };
    let read: ManagedReadResult =
      read_value_fixed(&self.io, tbl, segment_mapper, layout, &entry.value, buf).await?;
    Ok(Some(LargeValueInto {
      value_len: read.header.actual_len as usize,
      metadata: read.metadata,
    }))
  }
}

impl<I: PageIo + SegmentIo> Store<I> {
  /// 最大且 <= key 的 entry；语义对齐 eloqstore：不存在返回 `NotFound`。
  pub async fn floor(&self, tbl: &TableIdent, key: &[u8]) -> Result<FloorValue> {
    let Some(root_id) = self.root_map.get(tbl).copied() else {
      return Err(KvError::NotFound);
    };
    let Some(mapper) = self.mapper_map.get(tbl) else {
      return Err(KvError::NotFound);
    };
    let Some(entry) = read_floor_entry(
      &self.io,
      tbl,
      root_id,
      mapper,
      key,
      self.page_size,
      self.compression_map.get(tbl),
    )
    .await?
    else {
      return Err(KvError::NotFound);
    };
    if !entry.is_large_value {
      return Ok(FloorValue {
        key: entry.key,
        value: entry.value,
        metadata: Vec::new(),
        timestamp: entry.timestamp,
        expire_ts: entry.expire_ts,
      });
    }
    let Some(layout) = self.large_value_layout else {
      return Err(KvError::InvalidArgs);
    };
    let Some(segment_mapper) = self.segment_mapper_map.get(tbl) else {
      return Err(KvError::ResourceMissing);
    };
    let read = read_large_value(&self.io, tbl, segment_mapper, layout, &entry.value).await?;
    Ok(FloorValue {
      key: entry.key,
      value: read.value,
      metadata: read.metadata,
      timestamp: entry.timestamp,
      expire_ts: entry.expire_ts,
    })
  }
  pub async fn get_large(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<LargeValue>> {
    self.get_large_at(tbl, key, now_ms()).await
  }

  pub async fn get_large_at(
    &self,
    tbl: &TableIdent,
    key: &[u8],
    now_ms: u64,
  ) -> Result<Option<LargeValue>> {
    let Some(root_id) = self.root_map.get(tbl).copied() else {
      return Ok(None);
    };
    let Some(mapper) = self.mapper_map.get(tbl) else {
      return Ok(None);
    };
    let Some(entry) = read_entry(
      &self.io,
      tbl,
      root_id,
      mapper,
      key,
      self.page_size,
      self.compression_map.get(tbl),
    )
    .await?
    else {
      return Ok(None);
    };
    if self.ttl_read_filter && is_expired(entry.expire_ts, now_ms) {
      return Ok(None);
    }
    if !entry.is_large_value {
      return Ok(Some(LargeValue {
        value: entry.value,
        metadata: Vec::new(),
      }));
    }
    let Some(layout) = self.large_value_layout else {
      return Err(KvError::InvalidArgs);
    };
    let Some(segment_mapper) = self.segment_mapper_map.get(tbl) else {
      return Err(KvError::ResourceMissing);
    };
    let read = read_large_value(&self.io, tbl, segment_mapper, layout, &entry.value).await?;
    Ok(Some(LargeValue {
      value: read.value,
      metadata: read.metadata,
    }))
  }
}

impl<I: PageIo + ManifestIo + FileIo> Store<I> {
  /// 删除整表：先删 manifest（逻辑删除），再删 data 文件，最后清空内存状态。
  pub async fn drop_table(&mut self, tbl: &TableIdent) -> Result<()> {
    self.io.drop_manifest(tbl).await?;
    for file_id in self.io.list_file_id_li(tbl).await? {
      self.io.remove_file(tbl, file_id).await?;
    }
    for file_id in self.io.list_segment_file_id_li(tbl).await? {
      self.io.remove_segment_file(tbl, file_id).await?;
    }
    self.mapper_map.remove(tbl);
    self.segment_mapper_map.remove(tbl);
    self.root_map.remove(tbl);
    self.ttl_root_map.remove(tbl);
    self.manifest_started.remove(tbl);
    self.manifest_size_map.remove(tbl);
    self.ttl_next_map.remove(tbl);
    Ok(())
  }
}

impl<I: PageIo + FileIo + ManifestIo> Store<I> {
  /// GC 以所有 manifest（active + branch + archive）的 retained set 为准，
  /// 再并上当前内存 mapping（含未 flush 的 page）。
  pub async fn gc(&self, tbl: &TableIdent, pages_per_file_shift: u8) -> Result<usize> {
    let (mut data_set, mut segment_set) =
      fkv_task_gc::manifest_retained_file_page_set(&self.io, tbl).await?;
    if let Some(mapper) = self.mapper_map.get(tbl) {
      fkv_task_gc::add_mapping_retained(mapper.tbl(), &mut data_set);
    }
    if let Some(segment_mapper) = self.segment_mapper_map.get(tbl) {
      fkv_task_gc::add_mapping_retained(segment_mapper.tbl(), &mut segment_set);
    }
    let mut removed =
      fkv_task_gc::gc_files_with_retained(&self.io, tbl, &data_set, pages_per_file_shift).await?;
    if let Some(layout) = self.large_value_layout {
      removed += fkv_task_gc::gc_segment_files_with_retained(
        &self.io,
        tbl,
        &segment_set,
        layout.segments_per_file_shift(),
      )
      .await?;
    }
    Ok(removed)
  }
}

impl<I: PageIo + ManifestIo + TermIo> Store<I> {
  pub async fn claim_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<()> {
    self.io.upsert_term(branch, shard_id, term).await?;
    Ok(())
  }

  pub async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>> {
    self.io.read_term(branch, shard_id).await
  }

  pub async fn put_with_term(
    &mut self,
    tbl: &TableIdent,
    batch: &[BatchEntry],
    branch: &str,
    shard_id: u32,
    term: u64,
  ) -> Result<()> {
    if self
      .io
      .read_term(branch, shard_id)
      .await?
      .is_some_and(|current| current > term)
    {
      return Err(KvError::ExpiredTerm);
    }
    self.put(tbl, batch).await
  }
}
