//! 对齐 eloqstore Rust SDK 的便捷 API：put / get / delete / scan / floor。
//!
//! 当前基于 `Store<CompioStore>`，调用方需在 compio runtime 内 await。

use core::future::Future;
use std::{
  path::PathBuf,
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_config::KvOptions;
use fkv_error::Result;
use fkv_io_compio::CompioStore;
use fkv_large::SegmentLayout;
use fkv_store::{FloorValue, LargeValue, ScanOptions, ScanPageResult, Store};
use fkv_types::TableIdent;

pub struct KvStore {
  inner: Store<CompioStore>,
}

impl KvStore {
  pub fn new(root: impl Into<PathBuf>, options: &KvOptions) -> Result<Self> {
    let page_size = options.data_page_size as usize;
    let layout = SegmentLayout::new(options.segment_size, options.segments_per_file_shift)?;
    let io = CompioStore::new(root, page_size, options.pages_per_file_shift).with_segment_config(
      options.segment_size as usize,
      options.segments_per_file_shift,
    );
    let inner = Store::new(io, page_size, options.data_page_restart_interval)
      .with_manifest_limit(options.manifest_limit as usize)
      .with_pages_per_file_shift(options.pages_per_file_shift)
      .with_branch("main", 0)
      .with_large_value(layout, page_size / 2);
    Ok(Self { inner })
  }

  pub fn inner(&self) -> &Store<CompioStore> {
    &self.inner
  }

  pub fn inner_mut(&mut self) -> &mut Store<CompioStore> {
    &mut self.inner
  }

  /// 预加载 table 目录的 file id -> path 索引，返回索引文件数。
  pub fn preload_paths(&self, tbl: &TableIdent) -> usize {
    self.inner.preload_paths(tbl)
  }

  /// 外部同步改动目录后主动失效路径索引。
  pub fn invalidate_path_cache(&self) {
    self.inner.invalidate_path_cache();
  }

  pub async fn reopen(&mut self, tbl: &TableIdent) -> Result<()> {
    self.inner.reopen(tbl).await
  }

  pub async fn put(&mut self, tbl: &TableIdent, batch: &[BatchEntry]) -> Result<()> {
    self.inner.put(tbl, batch).await
  }

  pub async fn put_kv(
    &mut self,
    tbl: &TableIdent,
    key: impl Into<Vec<u8>>,
    value: impl Into<Vec<u8>>,
  ) -> Result<()> {
    self
      .inner
      .put(tbl, &[BatchEntry::upsert(key, value, now_ts())])
      .await
  }

  pub async fn get(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<Vec<u8>>> {
    self.inner.get(tbl, key).await
  }

  pub async fn read(&self, tbl: &TableIdent, key: &[u8]) -> Result<Vec<u8>> {
    self.inner.read(tbl, key).await
  }

  pub async fn delete(&mut self, tbl: &TableIdent, key: &[u8]) -> Result<()> {
    self.inner.delete(tbl, key, now_ts()).await
  }

  pub async fn scan(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    self.inner.scan(tbl, begin, end, max_entries).await
  }

  pub async fn scan_opts(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    options: ScanOptions,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    self
      .inner
      .scan_with_options(tbl, begin, end, max_entries, options)
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
      .inner
      .scan_page(tbl, begin, end, options, page_entries, page_size)
      .await
  }

  pub async fn floor(&self, tbl: &TableIdent, key: &[u8]) -> Result<FloorValue> {
    self.inner.floor(tbl, key).await
  }

  pub async fn truncate(&mut self, tbl: &TableIdent) -> Result<()> {
    self.inner.truncate(tbl).await
  }

  pub async fn truncate_from(&mut self, tbl: &TableIdent, position: &[u8]) -> Result<usize> {
    self.inner.truncate_from(tbl, position).await
  }

  pub async fn drop_table(&mut self, tbl: &TableIdent) -> Result<()> {
    self.inner.drop_table(tbl).await
  }

  pub async fn put_large(&mut self, tbl: &TableIdent, batch: &[BatchEntry]) -> Result<()> {
    self.inner.put_large(tbl, batch).await
  }

  pub async fn get_large(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<LargeValue>> {
    self.inner.get_large(tbl, key).await
  }
}

/// 便于泛型调用的最小 typed API。
pub trait KvApi {
  fn put<'a>(
    &'a mut self,
    tbl: &'a TableIdent,
    batch: &'a [BatchEntry],
  ) -> impl Future<Output = Result<()>> + 'a;

  fn get<'a>(
    &'a self,
    tbl: &'a TableIdent,
    key: &'a [u8],
  ) -> impl Future<Output = Result<Option<Vec<u8>>>> + 'a;

  fn delete<'a>(
    &'a mut self,
    tbl: &'a TableIdent,
    key: &'a [u8],
  ) -> impl Future<Output = Result<()>> + 'a;

  fn scan<'a>(
    &'a self,
    tbl: &'a TableIdent,
    begin: &'a [u8],
    end: &'a [u8],
    max_entries: usize,
  ) -> impl Future<Output = Result<Vec<(Vec<u8>, Vec<u8>)>>> + 'a;
}

impl KvApi for KvStore {
  fn put<'a>(
    &'a mut self,
    tbl: &'a TableIdent,
    batch: &'a [BatchEntry],
  ) -> impl Future<Output = Result<()>> + 'a {
    self.put(tbl, batch)
  }

  fn get<'a>(
    &'a self,
    tbl: &'a TableIdent,
    key: &'a [u8],
  ) -> impl Future<Output = Result<Option<Vec<u8>>>> + 'a {
    self.get(tbl, key)
  }

  fn delete<'a>(
    &'a mut self,
    tbl: &'a TableIdent,
    key: &'a [u8],
  ) -> impl Future<Output = Result<()>> + 'a {
    self.delete(tbl, key)
  }

  fn scan<'a>(
    &'a self,
    tbl: &'a TableIdent,
    begin: &'a [u8],
    end: &'a [u8],
    max_entries: usize,
  ) -> impl Future<Output = Result<Vec<(Vec<u8>, Vec<u8>)>>> + 'a {
    self.scan(tbl, begin, end, max_entries)
  }
}

pub fn now_ts() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
    .unwrap_or(0)
}
