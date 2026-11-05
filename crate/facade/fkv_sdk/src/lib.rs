//! 便捷 API：put / get / exists / delete / scan / floor。
//!
//! 当前基于 `Store<CompioStore>`，调用方需在 compio runtime 内 await。
//! SDK 只提供 async 形状；需要同步调用的一方在自己的 runtime 外层 `block_on`，
//! 避免在 thread-per-core 的 poll 栈内嵌套驱动调度器。
//!
//! **只装配 Local 模式**：Cloud / Standby 需要多 shard 与模式化 IO 后端，
//! 请用 `fkv` facade 的 `start_runtime`（内部 `fkv_runtime::Runtime`）；
//! 纯内存模式用 `fkv::start_mem_runtime`。SDK 对非 Local 配置显式报
//! `InvalidArgs`，不会静默按本地盘跑。
//!
//! **单根**：SDK 只持一个 `Store<CompioStore>`，`store_path` 多根配置显式报
//! `InvalidArgs`；多根 / 多 shard 用 facade 的 `start_runtime`（`RootedStore`）。
//! 首次 `get` / `put` 会自动 `reopen` 已有 manifest，未打开的表不会被当成空表
//! 覆盖。

use core::future::Future;
use std::path::{Path, PathBuf};

pub use fkv_btree::BatchEntry;
use fkv_config::KvOption;
use fkv_error::{KvError, Result};
use fkv_io_compio::CompioStore;
use fkv_large::SegmentLayout;
pub use fkv_store::PageOption;
use fkv_store::{FloorValue, KvEntry, KvEntryPage, LargeValue, ScanOption, ScanPageResult, Store};
use fkv_types::{MAIN_BRANCH_NAME, StoreMode, TableIdent, now_ms};

pub struct KvStore {
  inner: Store<CompioStore>,
  pages_per_file_shift: u8,
}

impl KvStore {
  /// `term` 是 store 的文件身份：data/segment 文件名带 `(branch, term)`；本地
  /// 活跃 manifest 固定名 `manifest_main_0`，term 不进 manifest 名（sto-70）。
  /// `term` 仍必须与同一数据根上的控制面（`Runtime::attach_shard`）一致：
  /// term 分叉时读侧有 `(branch, term)` 变体回退（当前身份优先、其余按 term
  /// 降序，io-31 候选语义 + `stale_read_num`），读不报错；但写只按当前
  /// `(branch, term)` 命名（不回退旧 identity 文件），分叉会把同一逻辑盘面
  /// 分裂成两个 term 文件族——归档名构造、term 文件 CAS（attach fencing）
  /// 与 GC 变体归属都以统一 term 为前提，SDK 不检查 term 文件、不做
  /// fencing，盘面归属与诊断口径会乱。
  pub fn new(root: impl Into<PathBuf>, option: &KvOption, term: u64) -> Result<Self> {
    let root = root.into();
    let mut option = option.clone();
    option.validate()?;
    // SDK 只装配本地 `CompioStore`。Cloud / Standby 配置必须显式报错，
    // 不能静默按本地盘跑（配了云路径却只写本地、零报错，比报错更糟）。
    // 需要这些模式的调用方走 `fkv::start_runtime`（facade 暴露的
    // `fkv_runtime::Runtime`）；纯内存走 `fkv::start_mem_runtime`。
    // `validate()` 通过不代表模式被尊重，故这里单独判 `derive_mode()`。
    if option.derive_mode() != StoreMode::Local {
      return Err(KvError::InvalidArgs);
    }
    // `store_path` 是配置驱动的数据根：SDK 只持有一个 `Store`，无法按
    // `store_path_lut` 把分区路由到多个根。与传入 `root` 不一致时直接拒绝，
    // 不能静默忽略配置写进 `root`（那会把数据写到调用方没预期的目录）。
    // TODO(sdk-multi-root)：需要多根时按 `RootedConfig` 装配 `RootedStore`。
    if !option.store_path.is_empty()
      && (option.store_path.len() != 1 || Path::new(option.store_path[0].as_str()) != root)
    {
      return Err(KvError::InvalidArgs);
    }
    let page_size = option.data_page_size as usize;
    let layout = SegmentLayout::new(option.segment_size, option.segments_per_file_shift)?;
    let io = CompioStore::new(root, page_size, option.pages_per_file_shift)
      .with_segment_config(option.segment_size as usize, option.segments_per_file_shift)
      .with_max_write_batch_pages(option.max_write_batch_pages as usize)
      .with_max_inflight_write(option.max_inflight_write as usize)
      .with_fd_cache_limit(option.fd_limit as usize);
    let mut store = Store::new(io, page_size, option.data_page_restart_interval)
      .with_index_restart_interval(option.index_page_restart_interval)
      .with_manifest_limit(option.manifest_limit as usize)
      .with_pages_per_file_shift(option.pages_per_file_shift)
      .with_index_cache_bytes(option.index_cache_bytes as usize)
      .with_index_cache_total_bytes(option.index_cache_total_bytes as usize)
      // 单 shard 不再均分 root meta 预算（对齐 `ShardConfig` 的注入口径）。
      .with_root_meta_cache_bytes(
        usize::try_from(option.root_meta_cache_size).unwrap_or(usize::MAX),
      )
      .with_yield_budget_us(option.yield_budget_us)
      .with_branch(MAIN_BRANCH_NAME, term)
      .with_large_value(layout, page_size / 2)
      .with_verify_checksum(!option.skip_verify_checksum)
      .with_overflow_pointers(option.overflow_pointers)
      .with_file_amplify_factor(option.file_amplify_factor)
      .with_segment_amplify_factor(option.segment_file_amplify_factor);
    if option.enable_compression {
      store = store.with_compression_level(option.zstd_compression_level);
    }
    Ok(Self {
      inner: store,
      pages_per_file_shift: option.pages_per_file_shift,
    })
  }

  pub fn inner(&self) -> &Store<CompioStore> {
    &self.inner
  }

  /// 预加载 table 目录的 file id -> path 索引，返回索引文件数。
  pub fn preload_path_li(&self, tbl: &TableIdent) -> usize {
    self.inner.preload_path_li(tbl)
  }

  /// 外部同步改动目录后主动失效路径索引。
  pub fn invalidate_path_cache(&self) {
    self.inner.invalidate_path_cache();
  }

  pub async fn reopen(&mut self, tbl: &TableIdent) -> Result<()> {
    self.inner.reopen(tbl).await
  }

  /// 确保表已加载：有 manifest 则 replay，没有则懒创建空表。
  ///
  /// SDK 是“无需 attach 的便捷入口”，不能要求调用方在每次重启后手工
  /// `reopen`；否则 `get` 会静默返回空、`put` 会以空状态覆盖已有 manifest。
  async fn ensure_open(&mut self, tbl: &TableIdent) -> Result<()> {
    if self.inner.is_open(tbl) {
      return Ok(());
    }
    match self.inner.reopen(tbl).await {
      Ok(()) => Ok(()),
      Err(KvError::ResourceMissing) => {
        self.inner.open_empty(tbl);
        Ok(())
      }
      Err(err) => Err(err),
    }
  }

  pub async fn put(&mut self, tbl: &TableIdent, batch_li: &[BatchEntry]) -> Result<()> {
    self.ensure_open(tbl).await?;
    self.inner.put(tbl, batch_li).await
  }

  /// 带时间注入的批量写：`now_ms` 决定 `expire_ts <= now_ms` 的条目是否落主树。
  pub async fn put_at(
    &mut self,
    tbl: &TableIdent,
    batch_li: &[BatchEntry],
    now_ms: u64,
  ) -> Result<()> {
    self.ensure_open(tbl).await?;
    self.inner.put_at(tbl, batch_li, now_ms).await
  }

  pub async fn put_kv(
    &mut self,
    tbl: &TableIdent,
    key: impl Into<Vec<u8>>,
    value: impl Into<Vec<u8>>,
  ) -> Result<()> {
    self.put_kv_at(tbl, key, value, now_ms()).await
  }

  /// 显式指定版本时间戳的单条 upsert（对齐 C++ `put(tbl, key, value, ts)`）。
  pub async fn put_kv_at(
    &mut self,
    tbl: &TableIdent,
    key: impl Into<Vec<u8>>,
    value: impl Into<Vec<u8>>,
    timestamp: u64,
  ) -> Result<()> {
    self.ensure_open(tbl).await?;
    self
      .inner
      .put(tbl, &[BatchEntry::upsert(key, value, timestamp)])
      .await
  }

  pub async fn get(&mut self, tbl: &TableIdent, key: &[u8]) -> Result<Option<Vec<u8>>> {
    self.ensure_open(tbl).await?;
    self.inner.get(tbl, key).await
  }

  /// 判断 key 是否存在；TTL 已过期或未命中均返回 `false`。
  ///
  /// large value 的 `get` 会返回 `LargeValueUnsupported`，这里改用 `get_large`
  /// 判存在，保证存在性语义与 value 形态无关。
  pub async fn exists(&mut self, tbl: &TableIdent, key: &[u8]) -> Result<bool> {
    self.ensure_open(tbl).await?;
    match self.inner.get(tbl, key).await {
      Ok(value) => Ok(value.is_some()),
      Err(KvError::LargeValueUnsupported) => Ok(self.inner.get_large(tbl, key).await?.is_some()),
      Err(err) => Err(err),
    }
  }

  pub async fn read(&mut self, tbl: &TableIdent, key: &[u8]) -> Result<Vec<u8>> {
    self.ensure_open(tbl).await?;
    self.inner.read(tbl, key).await
  }

  pub async fn delete(&mut self, tbl: &TableIdent, key: &[u8]) -> Result<()> {
    self.delete_at(tbl, key, now_ms()).await
  }

  /// 显式指定版本时间戳的删除（对齐 C++ `delete(tbl, key, ts)`）。
  pub async fn delete_at(&mut self, tbl: &TableIdent, key: &[u8], timestamp: u64) -> Result<()> {
    self.ensure_open(tbl).await?;
    self.inner.delete(tbl, key, timestamp).await
  }

  pub async fn scan(
    &mut self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    self.ensure_open(tbl).await?;
    self.inner.scan(tbl, begin, end, max_entries).await
  }

  pub async fn scan_opt(
    &mut self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    option: ScanOption,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    self.ensure_open(tbl).await?;
    self
      .inner
      .scan_with_option(tbl, begin, end, max_entries, option)
      .await
  }

  pub async fn scan_page(
    &mut self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    option: ScanOption,
    page: PageOption,
  ) -> Result<ScanPageResult> {
    self.ensure_open(tbl).await?;
    self.inner.scan_page(tbl, begin, end, option, page).await
  }

  /// 范围扫描，结果保留 `expire_ts`；已过期条目按读过滤口径剔除。
  pub async fn scan_kv(
    &mut self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
  ) -> Result<Vec<KvEntry>> {
    self.ensure_open(tbl).await?;
    self.inner.scan_entry(tbl, begin, end, max_entries).await
  }

  pub async fn scan_kv_opt(
    &mut self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    option: ScanOption,
  ) -> Result<Vec<KvEntry>> {
    self.ensure_open(tbl).await?;
    self
      .inner
      .scan_entry_with_option(tbl, begin, end, max_entries, option)
      .await
  }

  pub async fn scan_kv_page(
    &mut self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    option: ScanOption,
    page: PageOption,
  ) -> Result<KvEntryPage> {
    self.ensure_open(tbl).await?;
    self
      .inner
      .scan_entry_page(tbl, begin, end, option, page)
      .await
  }

  pub async fn floor(&mut self, tbl: &TableIdent, key: &[u8]) -> Result<FloorValue> {
    self.ensure_open(tbl).await?;
    self.inner.floor(tbl, key).await
  }

  pub async fn truncate(&mut self, tbl: &TableIdent) -> Result<()> {
    self.ensure_open(tbl).await?;
    self.inner.truncate(tbl).await
  }

  pub async fn truncate_from(&mut self, tbl: &TableIdent, position: &[u8]) -> Result<usize> {
    self.ensure_open(tbl).await?;
    self.inner.truncate_from(tbl, position).await
  }

  pub async fn drop_table(&mut self, tbl: &TableIdent) -> Result<()> {
    self.inner.drop_table(tbl).await?;
    // 对齐 C++ Drop 末尾的 TryAddFileGc：回收不再被任何 manifest 引用的文件。
    // GC 失败不回滚已完成的逻辑删除。
    let _ = self.inner.file_gc(tbl, self.pages_per_file_shift).await;
    Ok(())
  }

  pub async fn put_large(&mut self, tbl: &TableIdent, batch_li: &[BatchEntry]) -> Result<()> {
    self.ensure_open(tbl).await?;
    self.inner.put_large(tbl, batch_li).await
  }

  pub async fn get_large(&mut self, tbl: &TableIdent, key: &[u8]) -> Result<Option<LargeValue>> {
    self.ensure_open(tbl).await?;
    self.inner.get_large(tbl, key).await
  }
}

/// 便于泛型调用的最小 typed API。
pub trait KvApi {
  fn put<'a>(
    &'a mut self,
    tbl: &'a TableIdent,
    batch_li: &'a [BatchEntry],
  ) -> impl Future<Output = Result<()>> + 'a;

  fn get<'a>(
    &'a mut self,
    tbl: &'a TableIdent,
    key: &'a [u8],
  ) -> impl Future<Output = Result<Option<Vec<u8>>>> + 'a;

  fn exists<'a>(
    &'a mut self,
    tbl: &'a TableIdent,
    key: &'a [u8],
  ) -> impl Future<Output = Result<bool>> + 'a;

  fn delete<'a>(
    &'a mut self,
    tbl: &'a TableIdent,
    key: &'a [u8],
  ) -> impl Future<Output = Result<()>> + 'a;

  fn scan<'a>(
    &'a mut self,
    tbl: &'a TableIdent,
    begin: &'a [u8],
    end: &'a [u8],
    max_entries: usize,
  ) -> impl Future<Output = Result<Vec<(Vec<u8>, Vec<u8>)>>> + 'a;

  fn scan_kv<'a>(
    &'a mut self,
    tbl: &'a TableIdent,
    begin: &'a [u8],
    end: &'a [u8],
    max_entries: usize,
  ) -> impl Future<Output = Result<Vec<KvEntry>>> + 'a;
}

impl KvApi for KvStore {
  fn put<'a>(
    &'a mut self,
    tbl: &'a TableIdent,
    batch_li: &'a [BatchEntry],
  ) -> impl Future<Output = Result<()>> + 'a {
    self.put(tbl, batch_li)
  }

  fn get<'a>(
    &'a mut self,
    tbl: &'a TableIdent,
    key: &'a [u8],
  ) -> impl Future<Output = Result<Option<Vec<u8>>>> + 'a {
    self.get(tbl, key)
  }

  fn exists<'a>(
    &'a mut self,
    tbl: &'a TableIdent,
    key: &'a [u8],
  ) -> impl Future<Output = Result<bool>> + 'a {
    self.exists(tbl, key)
  }

  fn delete<'a>(
    &'a mut self,
    tbl: &'a TableIdent,
    key: &'a [u8],
  ) -> impl Future<Output = Result<()>> + 'a {
    self.delete(tbl, key)
  }

  fn scan<'a>(
    &'a mut self,
    tbl: &'a TableIdent,
    begin: &'a [u8],
    end: &'a [u8],
    max_entries: usize,
  ) -> impl Future<Output = Result<Vec<(Vec<u8>, Vec<u8>)>>> + 'a {
    self.scan(tbl, begin, end, max_entries)
  }

  fn scan_kv<'a>(
    &'a mut self,
    tbl: &'a TableIdent,
    begin: &'a [u8],
    end: &'a [u8],
    max_entries: usize,
  ) -> impl Future<Output = Result<Vec<KvEntry>>> + 'a {
    self.scan_kv(tbl, begin, end, max_entries)
  }
}
