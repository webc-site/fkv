//! 云 overlay：本地 cache 为第一层，对象存储为 durable 副本。
//!
//! data 以**整文件**为单位上传 / 下载；manifest 也是整对象。
//! 本地 cache 按字节预算做 LRU 淘汰，dirty 文件在上传完成前不淘汰。

use core::cell::RefCell;
use std::{str::from_utf8, sync::Arc};

use fkv_error::{KvError, Result};
use fkv_io::{
  DEFAULT_SEGMENT_SIZE, DEFAULT_SEGMENTS_PER_FILE_SHIFT, FileIo, ManifestIo, PageIo, SegmentIo,
  TermIo,
};
use fkv_metrics::ShardMetrics;
use fkv_object::ObjectStore;
use fkv_types::{FileId, FilePageId, TableIdent, current_term_file_name};
use gxhash::{HashMap, HashSet};

#[derive(Debug, Clone, Copy)]
struct FileState {
  size: u64,
  last_used: u64,
  dirty: bool,
}

#[derive(Debug, Default)]
struct CacheState {
  file_map: HashMap<(TableIdent, FileId), FileState>,
  clock: u64,
  resident_bytes: u64,
}

#[derive(Debug)]
pub struct CloudStore<I, O> {
  local: I,
  remote: O,
  prefix: String,
  pages_per_file_shift: u8,
  segment_size: usize,
  segments_per_file_shift: u8,
  cache_budget: u64,
  metrics: Option<Arc<ShardMetrics>>,
  state: RefCell<CacheState>,
}

impl<I, O> CloudStore<I, O> {
  pub fn new(local: I, remote: O, prefix: impl Into<String>, pages_per_file_shift: u8) -> Self {
    Self {
      local,
      remote,
      prefix: prefix.into(),
      pages_per_file_shift,
      segment_size: DEFAULT_SEGMENT_SIZE,
      segments_per_file_shift: DEFAULT_SEGMENTS_PER_FILE_SHIFT,
      cache_budget: 0,
      metrics: None,
      state: RefCell::new(CacheState::default()),
    }
  }

  pub fn with_segment_config(mut self, segment_size: usize, segments_per_file_shift: u8) -> Self {
    self.segment_size = segment_size;
    self.segments_per_file_shift = segments_per_file_shift;
    self
  }

  /// `0` 表示不限制预算。
  pub fn with_cache_budget(mut self, cache_budget: u64) -> Self {
    self.cache_budget = cache_budget;
    self
  }

  pub fn with_metrics(mut self, metrics: Arc<ShardMetrics>) -> Self {
    self.metrics = Some(metrics);
    self
  }

  fn record_cache_hit(&self) {
    if let Some(metrics) = &self.metrics {
      metrics.inc_cache_hit();
    }
  }

  fn record_cache_miss(&self) {
    if let Some(metrics) = &self.metrics {
      metrics.inc_cache_miss();
    }
  }

  fn sync_cache_bytes(&self) {
    if let Some(metrics) = &self.metrics {
      metrics.set_cache_bytes(self.resident_bytes());
    }
  }

  pub fn cache_budget(&self) -> u64 {
    self.cache_budget
  }

  pub fn resident_bytes(&self) -> u64 {
    self.state.borrow().resident_bytes
  }

  pub fn resident_file_count(&self) -> usize {
    self.state.borrow().file_map.len()
  }

  pub fn local(&self) -> &I {
    &self.local
  }

  pub fn remote(&self) -> &O {
    &self.remote
  }

  pub fn into_parts(self) -> (I, O) {
    (self.local, self.remote)
  }

  fn data_prefix(&self, tbl: &TableIdent) -> String {
    format!("{}/{}/data/", self.prefix.trim_end_matches('/'), tbl)
  }

  fn file_key(&self, tbl: &TableIdent, file_id: FileId) -> String {
    format!("{}{file_id}", self.data_prefix(tbl))
  }

  fn manifest_key(&self, tbl: &TableIdent) -> String {
    format!("{}/{}/manifest", self.prefix.trim_end_matches('/'), tbl)
  }

  fn manifest_file_prefix(&self, tbl: &TableIdent) -> String {
    format!("{}/{}/manifest/", self.prefix.trim_end_matches('/'), tbl)
  }

  fn manifest_file_key(&self, tbl: &TableIdent, name: &str) -> String {
    format!("{}{name}", self.manifest_file_prefix(tbl))
  }

  fn segment_prefix(&self, tbl: &TableIdent) -> String {
    format!("{}/{}/segment/", self.prefix.trim_end_matches('/'), tbl)
  }

  fn segment_key(&self, tbl: &TableIdent, file_id: FileId) -> String {
    format!("{}{file_id}", self.segment_prefix(tbl))
  }

  fn segment_file_id(&self, segment_fp_id: FilePageId) -> FileId {
    segment_fp_id >> self.segments_per_file_shift
  }

  fn file_id_of(&self, file_page_id: FilePageId) -> FileId {
    file_page_id >> self.pages_per_file_shift
  }

  fn page_size_of(&self, file_page_id: FilePageId, src_len: usize) -> u64 {
    let index = file_page_id & ((1u64 << self.pages_per_file_shift) - 1);
    (index + 1) * src_len as u64
  }

  fn contains(&self, tbl: &TableIdent, file_id: FileId) -> bool {
    self
      .state
      .borrow()
      .file_map
      .contains_key(&(tbl.clone(), file_id))
  }

  fn touch(&self, tbl: &TableIdent, file_id: FileId) {
    let mut state = self.state.borrow_mut();
    state.clock += 1;
    let clock = state.clock;
    if let Some(entry) = state.file_map.get_mut(&(tbl.clone(), file_id)) {
      entry.last_used = clock;
    }
  }

  fn upsert_resident(&self, tbl: &TableIdent, file_id: FileId, size: u64, dirty: bool) {
    let mut state = self.state.borrow_mut();
    state.clock += 1;
    let clock = state.clock;
    let key = (tbl.clone(), file_id);
    let old_size = match state.file_map.get_mut(&key) {
      Some(entry) => {
        let old_size = entry.size;
        entry.size = size;
        entry.last_used = clock;
        entry.dirty |= dirty;
        Some(old_size)
      }
      None => None,
    };
    match old_size {
      Some(old_size) => state.resident_bytes = state.resident_bytes - old_size + size,
      None => {
        state.resident_bytes += size;
        state.file_map.insert(
          key,
          FileState {
            size,
            last_used: clock,
            dirty,
          },
        );
      }
    }
  }

  fn mark_clean(&self, tbl: &TableIdent, file_id: FileId, size: u64) {
    let mut state = self.state.borrow_mut();
    let key = (tbl.clone(), file_id);
    let Some(old_size) = state.file_map.get(&key).map(|entry| entry.size) else {
      return;
    };
    if let Some(entry) = state.file_map.get_mut(&key) {
      entry.size = size;
      entry.dirty = false;
    }
    state.resident_bytes = state.resident_bytes - old_size + size;
  }

  fn forget(&self, tbl: &TableIdent, file_id: FileId) {
    let mut state = self.state.borrow_mut();
    if let Some(entry) = state.file_map.remove(&(tbl.clone(), file_id)) {
      state.resident_bytes -= entry.size;
    }
  }

  fn dirty_file_li(&self, tbl: &TableIdent) -> Vec<FileId> {
    let mut file_id_li: Vec<FileId> = self
      .state
      .borrow()
      .file_map
      .iter()
      .filter(|(key, entry)| &key.0 == tbl && entry.dirty)
      .map(|(key, _)| key.1)
      .collect();
    file_id_li.sort_unstable();
    file_id_li
  }

  fn evict_candidate(&self) -> Option<(TableIdent, FileId)> {
    let state = self.state.borrow();
    if self.cache_budget == 0 || state.resident_bytes <= self.cache_budget {
      return None;
    }
    state
      .file_map
      .iter()
      .filter(|(_, entry)| !entry.dirty)
      .min_by_key(|(key, entry)| {
        (
          entry.last_used,
          key.0.tbl_name.as_ref(),
          key.0.partition_id,
          key.1,
        )
      })
      .map(|(key, _)| key.clone())
  }
}

impl<I: FileIo, O: ObjectStore> CloudStore<I, O> {
  async fn evict_to_budget(&self) -> Result<()> {
    while let Some((tbl, file_id)) = self.evict_candidate() {
      self.local.remove_file(&tbl, file_id).await?;
      self.forget(&tbl, file_id);
      if let Some(metrics) = &self.metrics {
        metrics.inc_cache_evict();
      }
    }
    self.sync_cache_bytes();
    Ok(())
  }

  /// 写页前保证整文件已在本地：优先信任本地 cache，否则从远端拉整文件。
  async fn ensure_resident(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    if self.contains(tbl, file_id) {
      return Ok(());
    }
    if let Some(size) = self.local.file_size(tbl, file_id).await? {
      self.upsert_resident(tbl, file_id, size, false);
      return Ok(());
    }
    if let Some(bytes) = self.remote.get(&self.file_key(tbl, file_id)).await? {
      self.local.write_file(tbl, file_id, &bytes).await?;
      self.upsert_resident(tbl, file_id, bytes.len() as u64, false);
    }
    Ok(())
  }

  /// 把某个 table 的 dirty data 文件整文件上传。
  async fn flush_tbl(&self, tbl: &TableIdent) -> Result<()> {
    for file_id in self.dirty_file_li(tbl) {
      let Some(bytes) = self.local.read_file(tbl, file_id).await? else {
        return Err(KvError::ResourceMissing);
      };
      self
        .remote
        .put(&self.file_key(tbl, file_id), &bytes, None, false)
        .await?;
      self.mark_clean(tbl, file_id, bytes.len() as u64);
    }
    self.evict_to_budget().await
  }
}

impl<I: SegmentIo, O> CloudStore<I, O> {
  async fn assemble_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Vec<u8>> {
    if self.segment_size == 0 || self.segments_per_file_shift >= 63 {
      return Err(KvError::InvalidArgs);
    }
    let slot_num = 1usize
      .checked_shl(u32::from(self.segments_per_file_shift))
      .ok_or(KvError::InvalidArgs)?;
    let mut bytes = vec![0u8; slot_num * self.segment_size];
    let base = file_id << self.segments_per_file_shift;
    for index in 0..slot_num {
      let mut buf = vec![0u8; self.segment_size];
      match self
        .local
        .read_segment(tbl, base + index as u64, &mut buf)
        .await
      {
        Ok(()) => {
          let offset = index * self.segment_size;
          bytes[offset..offset + self.segment_size].copy_from_slice(&buf);
        }
        Err(KvError::ResourceMissing) => {}
        Err(err) => return Err(err),
      }
    }
    Ok(bytes)
  }

  async fn install_segment_file(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    bytes: &[u8],
  ) -> Result<()> {
    if self.segment_size == 0 {
      return Err(KvError::InvalidArgs);
    }
    let slot_num = 1usize
      .checked_shl(u32::from(self.segments_per_file_shift))
      .ok_or(KvError::InvalidArgs)?;
    if bytes.len() > slot_num * self.segment_size {
      return Err(KvError::Corrupted);
    }
    let base = file_id << self.segments_per_file_shift;
    for (index, chunk) in bytes.chunks(self.segment_size).enumerate() {
      self
        .local
        .write_segment(tbl, base + index as u64, chunk)
        .await?;
    }
    Ok(())
  }
}

impl<I: SegmentIo, O: ObjectStore> SegmentIo for CloudStore<I, O> {
  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    match self.local.read_segment(tbl, segment_fp_id, dst).await {
      Ok(()) => {
        self.record_cache_hit();
        return Ok(());
      }
      Err(KvError::ResourceMissing) => {
        self.record_cache_miss();
      }
      Err(err) => return Err(err),
    }
    let file_id = self.segment_file_id(segment_fp_id);
    let Some(bytes) = self.remote.get(&self.segment_key(tbl, file_id)).await? else {
      return Err(KvError::ResourceMissing);
    };
    self.install_segment_file(tbl, file_id, &bytes).await?;
    self.local.read_segment(tbl, segment_fp_id, dst).await
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> Result<()> {
    self.local.write_segment(tbl, segment_fp_id, src).await?;
    let file_id = self.segment_file_id(segment_fp_id);
    let bytes = self.assemble_segment_file(tbl, file_id).await?;
    self
      .remote
      .put(&self.segment_key(tbl, file_id), &bytes, None, false)
      .await?;
    Ok(())
  }
}

impl<I: PageIo + FileIo, O: ObjectStore> PageIo for CloudStore<I, O> {
  fn set_file_identity(&self, branch: &str, term: u64) {
    self.local.set_file_identity(branch, term);
  }

  fn preload_paths(&self, tbl: &TableIdent) -> usize {
    self.local.preload_paths(tbl)
  }

  fn invalidate_path_cache(&self) {
    self.local.invalidate_path_cache();
  }

  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    let file_id = self.file_id_of(file_page_id);
    match self.local.read_page(tbl, file_page_id, dst).await {
      Ok(()) => {
        self.touch(tbl, file_id);
        self.record_cache_hit();
        self.evict_to_budget().await?;
        return Ok(());
      }
      Err(KvError::ResourceMissing) => {
        self.record_cache_miss();
      }
      Err(err) => return Err(err),
    }
    // 文件只要在本地（包括上一进程遗留的 cache），就不要再整文件覆盖。
    if self.contains(tbl, file_id) || self.local.file_size(tbl, file_id).await?.is_some() {
      return Err(KvError::ResourceMissing);
    }
    let Some(bytes) = self.remote.get(&self.file_key(tbl, file_id)).await? else {
      return Err(KvError::ResourceMissing);
    };
    self.local.write_file(tbl, file_id, &bytes).await?;
    self.upsert_resident(tbl, file_id, bytes.len() as u64, false);
    let result = self.local.read_page(tbl, file_page_id, dst).await;
    // dst 已拿到数据后再淘汰，预算可以严格执行。
    self.evict_to_budget().await?;
    result
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    let file_id = self.file_id_of(file_page_id);
    self.ensure_resident(tbl, file_id).await?;
    self.local.write_page(tbl, file_page_id, src).await?;
    let size = self.page_size_of(file_page_id, src.len());
    self.upsert_resident(tbl, file_id, size, true);
    self.evict_to_budget().await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.flush_tbl(tbl).await?;
    self.local.sync_data(tbl).await
  }
}

impl<I: ManifestIo + FileIo, O: ObjectStore> ManifestIo for CloudStore<I, O> {
  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    // 先上传 data 文件，再让 manifest 指向它们。
    self.flush_tbl(tbl).await?;
    self.local.append_manifest(tbl, record).await?;
    let Some(bytes) = self.local.read_manifest(tbl).await? else {
      return Err(KvError::ResourceMissing);
    };
    self
      .remote
      .put(&self.manifest_key(tbl), &bytes, None, false)
      .await?;
    Ok(())
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.flush_tbl(tbl).await?;
    self.local.switch_manifest(tbl, record).await?;
    self
      .remote
      .put(&self.manifest_key(tbl), record, None, false)
      .await?;
    Ok(())
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    if let Some(bytes) = self.local.read_manifest(tbl).await? {
      return Ok(Some(bytes));
    }
    let key = self.manifest_key(tbl);
    let Some(bytes) = self.remote.get(&key).await? else {
      return Ok(None);
    };
    self.local.switch_manifest(tbl, &bytes).await?;
    Ok(Some(bytes))
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
    self.local.drop_manifest(tbl).await?;
    self.remote.delete(&self.manifest_key(tbl)).await?;
    Ok(())
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self.local.write_manifest_file(tbl, name, record).await?;
    self
      .remote
      .put(&self.manifest_file_key(tbl, name), record, None, false)
      .await?;
    Ok(())
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    let mut name_set = HashSet::default();
    name_set.extend(self.local.list_manifest_file_name_li(tbl).await?);
    for meta in self.remote.list(&self.manifest_file_prefix(tbl)).await? {
      let Some(name) = meta.key.rsplit('/').next() else {
        continue;
      };
      if fkv_types::parse_manifest_file_name(name).is_some() {
        name_set.insert(name.to_owned());
      }
    }
    let mut name_li: Vec<String> = name_set.into_iter().collect();
    name_li.sort();
    Ok(name_li)
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    if let Some(bytes) = self.local.read_manifest_file(tbl, name).await? {
      return Ok(Some(bytes));
    }
    let Some(bytes) = self.remote.get(&self.manifest_file_key(tbl, name)).await? else {
      return Ok(None);
    };
    self.local.write_manifest_file(tbl, name, &bytes).await?;
    Ok(Some(bytes))
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.local.remove_manifest_file(tbl, name).await?;
    self
      .remote
      .delete(&self.manifest_file_key(tbl, name))
      .await?;
    Ok(())
  }
}

impl<I, O: ObjectStore> CloudStore<I, O> {
  pub async fn remote_data_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    let prefix = self.data_prefix(tbl);
    let mut file_id_li = Vec::new();
    for meta in self.remote.list(&prefix).await? {
      let Some(raw) = meta.key.rsplit('/').next() else {
        continue;
      };
      if let Ok(file_id) = raw.parse::<FileId>() {
        file_id_li.push(file_id);
      }
    }
    file_id_li.sort_unstable();
    file_id_li.dedup();
    Ok(file_id_li)
  }

  pub async fn remote_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    let prefix = self.segment_prefix(tbl);
    let mut file_id_li = Vec::new();
    for meta in self.remote.list(&prefix).await? {
      let Some(raw) = meta.key.rsplit('/').next() else {
        continue;
      };
      if let Ok(file_id) = raw.parse::<FileId>() {
        file_id_li.push(file_id);
      }
    }
    file_id_li.sort_unstable();
    file_id_li.dedup();
    Ok(file_id_li)
  }
}

impl<I: ManifestIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 确保 manifest 已在本地；返回 manifest 是否存在。
  pub async fn prewarm_manifest(&self, tbl: &TableIdent) -> Result<bool> {
    Ok(self.read_manifest(tbl).await?.is_some())
  }
}

impl<I: FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 确保 data 文件已在本地 cache；返回文件是否存在。
  pub async fn prewarm_data_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<bool> {
    Ok(self.read_file(tbl, file_id).await?.is_some())
  }
}

impl<I: SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 确保 segment 文件已在本地 cache；返回文件是否存在。
  pub async fn prewarm_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<bool> {
    if self
      .local
      .list_segment_file_id_li(tbl)
      .await?
      .contains(&file_id)
    {
      return Ok(true);
    }
    let Some(bytes) = self.remote.get(&self.segment_key(tbl, file_id)).await? else {
      return Ok(false);
    };
    self.install_segment_file(tbl, file_id, &bytes).await?;
    Ok(true)
  }
}

impl<I: FileIo, O: ObjectStore> FileIo for CloudStore<I, O> {
  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    let mut file_id_set = HashSet::default();
    file_id_set.extend(self.local.list_file_id_li(tbl).await?);
    let prefix = self.data_prefix(tbl);
    for meta in self.remote.list(&prefix).await? {
      let Some(raw) = meta.key.rsplit('/').next() else {
        continue;
      };
      if let Ok(file_id) = raw.parse::<FileId>() {
        file_id_set.insert(file_id);
      }
    }
    Ok(file_id_set.into_iter().collect())
  }

  async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>> {
    if let Some(bytes) = self.local.read_file(tbl, file_id).await? {
      self.touch(tbl, file_id);
      self.record_cache_hit();
      self.evict_to_budget().await?;
      return Ok(Some(bytes));
    }
    self.record_cache_miss();
    let Some(bytes) = self.remote.get(&self.file_key(tbl, file_id)).await? else {
      return Ok(None);
    };
    self.local.write_file(tbl, file_id, &bytes).await?;
    self.upsert_resident(tbl, file_id, bytes.len() as u64, false);
    self.evict_to_budget().await?;
    Ok(Some(bytes))
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()> {
    self.local.write_file(tbl, file_id, bytes).await?;
    self.upsert_resident(tbl, file_id, bytes.len() as u64, true);
    self.evict_to_budget().await
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>> {
    self.local.file_size(tbl, file_id).await
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.local.remove_file(tbl, file_id).await?;
    self.forget(tbl, file_id);
    self.sync_cache_bytes();
    self.remote.delete(&self.file_key(tbl, file_id)).await?;
    Ok(())
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    let mut file_id_set = HashSet::default();
    file_id_set.extend(self.local.list_segment_file_id_li(tbl).await?);
    for meta in self.remote.list(&self.segment_prefix(tbl)).await? {
      let Some(raw) = meta.key.rsplit('/').next() else {
        continue;
      };
      if let Ok(file_id) = raw.parse::<FileId>() {
        file_id_set.insert(file_id);
      }
    }
    Ok(file_id_set.into_iter().collect())
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.local.remove_segment_file(tbl, file_id).await?;
    self.remote.delete(&self.segment_key(tbl, file_id)).await?;
    Ok(())
  }
}

impl<I, O: ObjectStore> TermIo for CloudStore<I, O> {
  async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>> {
    let key = format!(
      "{}/{}",
      self.prefix.trim_end_matches('/'),
      current_term_file_name(branch, shard_id)
    );
    let Some(bytes) = self.remote.get(&key).await? else {
      return Ok(None);
    };
    let text = from_utf8(&bytes).map_err(|_| KvError::Corrupted)?;
    text
      .trim()
      .parse()
      .map(Some)
      .map_err(|_| KvError::Corrupted)
  }

  async fn upsert_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<u64> {
    let key = format!(
      "{}/{}",
      self.prefix.trim_end_matches('/'),
      current_term_file_name(branch, shard_id)
    );
    let value = term.to_string();
    for _ in 0..10 {
      let meta = self.remote.head(&key).await?;
      if let Some(meta) = &meta {
        let Some(bytes) = self.remote.get(&key).await? else {
          continue;
        };
        let text = from_utf8(&bytes).map_err(|_| KvError::Corrupted)?;
        let current: u64 = text.trim().parse().map_err(|_| KvError::Corrupted)?;
        if current > term {
          return Err(KvError::ExpiredTerm);
        }
        match self
          .remote
          .put(&key, value.as_bytes(), Some(&meta.etag), false)
          .await
        {
          Ok(_) => return Ok(term),
          Err(KvError::Busy | KvError::AlreadyExists) => continue,
          Err(err) => return Err(err),
        }
      } else {
        match self.remote.put(&key, value.as_bytes(), None, true).await {
          Ok(_) => return Ok(term),
          Err(KvError::Busy | KvError::AlreadyExists) => continue,
          Err(err) => return Err(err),
        }
      }
    }
    Err(KvError::TryAgain)
  }
}
