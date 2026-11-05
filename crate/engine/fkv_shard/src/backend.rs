//! IO 后端枚举：按运行模式在 shard 线程内构造 Local / Mem / Cloud 之一。
//!
//! 用 enum 分派而非 `dyn`：`Store` 对 IO 的 trait bound 是六件套叠加，
//! 枚举转发保持编译期单态化，热路径无虚调用。

use std::{path::PathBuf, rc::Rc, sync::Arc};

use fkv_cloud::{CacheGroup, CloudStore};
use fkv_error::{KvError, Result};
use fkv_io::{
  FileIo, FileVariant, FixedBuffer, FixedSegmentIo, ManifestIo, PageIo, SegmentIo, TermIo,
};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_metrics::ShardMetric;
use fkv_object::{ListPage, ObjectStore};
use fkv_object_s3::{S3Config, S3ObjectStore};
use fkv_task_prewarm::PrewarmStat;
use fkv_types::{BranchFileRange, FileId, FilePageId, TableIdent};

use super::roots::{RootedConfig, RootedStore};

/// 本地文件后端：按 `ShardConfig` 的盘面 / IO 参数装配一个 `CompioStore`。
///
/// 单根与多根（`RootedStore`）共用本函数，避免两处装配参数漂移。
pub(crate) fn build_compio_store(config: &super::ShardConfig, root: PathBuf) -> CompioStore {
  CompioStore::new(root, config.page_size, config.pages_per_file_shift)
    .with_segment_config(config.segment_size as usize, config.segments_per_file_shift)
    .with_managed_segment_read()
    .with_max_write_batch_pages(config.max_write_batch_pages)
    .with_max_inflight_write(config.max_inflight_write)
    .with_fd_cache_limit(config.fd_limit)
}

/// 本地 cache 根目录与云参数：只在 `Backend::Cloud` 下使用。
#[derive(Debug, Clone)]
pub struct CloudBackend {
  pub bucket: String,
  pub prefix: String,
  pub endpoint: String,
  pub region: String,
  pub access_key: String,
  pub secret_key: String,
  pub allow_http: bool,
  /// 本地 cache 预算组（12d）：同一运行时的全部 shard 与运行时侧预热线程
  /// 共享同一个 `local_space_limit`，不再是每个实例一份上界。
  /// `CacheGroup::new(0)` 表示不限预算。
  pub cache_group: Arc<CacheGroup>,
  /// 是否复用启动前已存在的本地 cache（对齐 C++ `allow_reuse_local_caches`
  /// 默认关闭）。由运行时从 `KvOption` 投影，shard 装配时显式传给
  /// [`CloudStore::with_reuse_local_caches`]；不在这里透传会让该配置成为死旋钮，
  /// 且默认值与 C++ 相反。
  pub allow_reuse_local_caches: bool,
  /// S3 飞行中请求上限（`KvOption::max_cloud_concurrency`；0 = 环境/默认）。
  pub max_cloud_concurrency: usize,
  /// 共享同一份 `max_cloud_concurrency` 的 `S3ObjectStore` 实例数。
  ///
  /// 每个 shard 各起一个 S3 后台线程（`CloudStore` 含 `RefCell`，不能跨线程
  /// 共享），运行时侧另有长驻全局列举、预热 crond、归档 crond 三类实例
  /// （后两者按配置启用），实例数 = `shard_num + 1 + 预热 + 归档`，与
  /// `fkv_runtime::cloud_s3_instance_num` 的四类计数一致。若每个实例都按完整
  /// 上限放行，实际并发会被放大到 `实例数 × 上限`；这里按实例数均分，保证整机
  /// 飞行中请求不超过配置值。1 = 不拆分（单实例）。
  pub s3_instance_num: usize,
  /// 是否校验 S3 服务端 TLS 证书（`KvOption::cloud_verify_ssl`）。
  pub verify_ssl: bool,
}

impl CloudBackend {
  /// 按实例数拆分后的 S3 配置；运行时侧复用同一规则，避免默认值分叉。
  pub fn s3_config(&self) -> S3Config {
    // 0 表示「用环境变量 / 默认值」，不做拆分；否则按实例数均分。
    let max_concurrency = match self.max_cloud_concurrency {
      0 => 0,
      n => s3_quota_share(n, self.s3_instance_num),
    };
    S3Config {
      bucket: self.bucket.clone(),
      endpoint: self.endpoint.clone(),
      region: self.region.clone(),
      access_key: self.access_key.clone(),
      secret_key: self.secret_key.clone(),
      allow_http: self.allow_http,
      max_concurrency,
      verify_ssl: self.verify_ssl,
    }
  }
}

/// 单实例的飞行中请求额度。
///
/// `instances` 必须是**真实 `CloudStore` 构造点数**（shard 数 + 运行时侧实际
/// 启用的列举 / 预热 / 归档实例），否则整机并发会被放大成
/// `实例数 × 单实例额度`。
///
/// 额度按 `n / instances` 向下取整；`n < instances` 时每个实例至少 1
/// （`S3Config::max_concurrency == 0` 表示「用环境默认」，不能表达 0 额度），
/// 此时整机上界是 `instances` 而不是 `instances × n`。
fn s3_quota_share(n: usize, instances: usize) -> usize {
  (n / instances.max(1)).max(1)
}

/// `ShardConfig.backend`：决定 shard 线程内装配哪种 IO。
#[derive(Debug, Clone, Default)]
pub enum Backend {
  /// 本地文件（`CompioStore`）。
  #[default]
  Local,
  /// 多根本地文件：分区按 LUT 选根（对齐 C++ `store_path` / `store_path_lut`）。
  LocalRouted(RootedConfig),
  /// 纯内存（`MemStore`），无 durability。
  Mem,
  /// 本地 cache + S3（`CloudStore<CompioStore, S3ObjectStore>`）。
  Cloud(CloudBackend),
}

#[derive(Clone)]
pub enum StoreBackend {
  // 各后端结构体内联 KB 级状态，统一装箱，避免枚举被最大变体撑大。
  //
  // `Rc` 让每分区 Store 共享同一后端（fd 缓存 / 路径索引 / 在飞写闸门），
  // 克隆只增引用计数，不复制缓存；单 shard 单线程，不引入跨线程共享。
  Local(Rc<RootedStore>),
  Mem(Rc<MemStore>),
  Cloud(Rc<CloudStore<CompioStore, S3ObjectStore>>),
}

impl StoreBackend {
  pub fn local(config: &super::ShardConfig) -> Self {
    Self::Local(Rc::new(RootedStore::new(
      config,
      &RootedConfig::single(config.root.clone()),
    )))
  }

  /// 多根本地后端：根列表与 LUT 来自 `Backend::LocalRouted`。
  pub fn local_routed(config: &super::ShardConfig, rooted: &RootedConfig) -> Self {
    Self::Local(Rc::new(RootedStore::new(config, rooted)))
  }

  pub fn mem(config: &super::ShardConfig) -> Self {
    Self::Mem(Rc::new(
      MemStore::with_pages_per_file(config.page_size, config.pages_per_file_shift)
        .with_segment_config(config.segment_size as usize, config.segments_per_file_shift),
    ))
  }

  /// `metrics` 必须是**本 shard** 的 `Arc<ShardMetric>`（cld-55）：不传的话
  /// cloud 本地 cache 的 hit / miss / evict / bytes 四项指标恒 0，且从外部
  /// 看不出「指标没接」和「cache 一直没命中」的区别。
  pub fn cloud(
    config: &super::ShardConfig,
    cloud: &CloudBackend,
    metrics: Arc<ShardMetric>,
  ) -> Result<Self> {
    let local = build_compio_store(config, config.root.clone());
    let remote = S3ObjectStore::start(cloud.s3_config())?;
    let store = CloudStore::new(
      local,
      remote,
      cloud.prefix.clone(),
      config.pages_per_file_shift,
    )
    .with_segment_config(config.segment_size as usize, config.segments_per_file_shift)
    .with_cache_group(cloud.cache_group.clone())
    .with_reuse_local_caches(cloud.allow_reuse_local_caches)
    // shard 是分区写实例：跨进程共用本地 root 时，只有它能启用 / 回填表级
    // segment scheme；运行时侧预热 / 列举 / 归档实例统一置只读。
    .with_read_only(false)
    .with_metrics(metrics);
    // 装配点自检：配置必须真的走到 CloudStore，禁止再回落默认值。
    debug_assert_eq!(
      store.reuse_local_caches(),
      cloud.allow_reuse_local_caches,
      "CloudStore 未按 KvOption 接线 allow_reuse_local_caches"
    );
    debug_assert!(
      !store.read_only(),
      "shard 写实例必须 read_only(false)，否则 scheme 回填无人负责"
    );
    Ok(Self::Cloud(Rc::new(store)))
  }
}

impl StoreBackend {
  /// 按分区预热本地 cache（仅 Cloud）。
  ///
  /// 直接复用 `fkv_task_prewarm::prewarm_table`：先 manifest，再 data，最后
  /// segment；磁盘余量不足 / 列举失败 / 下载失败都由该函数返回统计或错误。
  /// Local / Mem / Standby 没有远端对象可拉，显式返回 `InvalidArgs`，不静默
  /// 降级为成功（否则调用方会误以为 cache 已就绪）。
  pub async fn prewarm_table(&self, tbl: &TableIdent) -> Result<PrewarmStat> {
    match self {
      Self::Cloud(cloud) => fkv_task_prewarm::prewarm_table(cloud, tbl).await,
      Self::Local(_) | Self::Mem(_) => Err(KvError::InvalidArgs),
    }
  }

  /// 列举远端对象（仅 Cloud），直接复用 `ObjectStore::list_page`。
  ///
  /// `prefix` 是远端 key 的原始前缀（对齐 C++ `ListObjectRequest::SetRemotePath`），
  /// 不做表目录拼接；`token` / `max_keys` 契约见 `ObjectStore::list_page`。
  /// 非 Cloud 后端返回 `InvalidArgs`：空列表会被误读成「远端没有对象」。
  pub async fn list_object(
    &self,
    prefix: &str,
    token: Option<&str>,
    max_keys: usize,
  ) -> Result<ListPage> {
    match self {
      Self::Cloud(cloud) => cloud.remote().list_page(prefix, token, max_keys).await,
      Self::Local(_) | Self::Mem(_) => Err(KvError::InvalidArgs),
    }
  }
}

impl PageIo for StoreBackend {
  fn set_file_identity(&self, branch: &str, term: u64) {
    match self {
      Self::Local(io) => io.set_file_identity(branch, term),
      Self::Mem(io) => io.set_file_identity(branch, term),
      Self::Cloud(io) => io.set_file_identity(branch, term),
    }
  }

  fn set_file_range_li(&self, tbl: &TableIdent, range_li: &[BranchFileRange]) {
    match self {
      Self::Local(io) => io.set_file_range_li(tbl, range_li),
      Self::Mem(io) => io.set_file_range_li(tbl, range_li),
      Self::Cloud(io) => io.set_file_range_li(tbl, range_li),
    }
  }

  fn preload_path_li(&self, tbl: &TableIdent) -> usize {
    match self {
      Self::Local(io) => io.preload_path_li(tbl),
      Self::Mem(io) => io.preload_path_li(tbl),
      Self::Cloud(io) => io.preload_path_li(tbl),
    }
  }

  fn invalidate_path_cache(&self) {
    match self {
      Self::Local(io) => io.invalidate_path_cache(),
      Self::Mem(io) => io.invalidate_path_cache(),
      Self::Cloud(io) => io.invalidate_path_cache(),
    }
  }

  fn set_verify_checksum(&self, enabled: bool) {
    match self {
      Self::Local(io) => io.set_verify_checksum(enabled),
      Self::Mem(io) => io.set_verify_checksum(enabled),
      Self::Cloud(io) => io.set_verify_checksum(enabled),
    }
  }

  fn evict_table(&self, tbl: &TableIdent) {
    match self {
      Self::Local(io) => io.evict_table(tbl),
      Self::Mem(io) => io.evict_table(tbl),
      Self::Cloud(io) => io.evict_table(tbl),
    }
  }

  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    match self {
      Self::Local(io) => io.read_page(tbl, file_page_id, dst).await,
      Self::Mem(io) => io.read_page(tbl, file_page_id, dst).await,
      Self::Cloud(io) => io.read_page(tbl, file_page_id, dst).await,
    }
  }

  async fn read_page_li(
    &self,
    tbl: &TableIdent,
    file_page_id_li: &[FilePageId],
    page_buf_li: &mut [Vec<u8>],
  ) -> Result<()> {
    match self {
      Self::Local(io) => io.read_page_li(tbl, file_page_id_li, page_buf_li).await,
      Self::Mem(io) => io.read_page_li(tbl, file_page_id_li, page_buf_li).await,
      Self::Cloud(io) => io.read_page_li(tbl, file_page_id_li, page_buf_li).await,
    }
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    match self {
      Self::Local(io) => io.write_page(tbl, file_page_id, src).await,
      Self::Mem(io) => io.write_page(tbl, file_page_id, src).await,
      Self::Cloud(io) => io.write_page(tbl, file_page_id, src).await,
    }
  }

  async fn write_page_li(&self, tbl: &TableIdent, page_li: &[(FilePageId, Vec<u8>)]) -> Result<()> {
    match self {
      Self::Local(io) => io.write_page_li(tbl, page_li).await,
      Self::Mem(io) => io.write_page_li(tbl, page_li).await,
      Self::Cloud(io) => io.write_page_li(tbl, page_li).await,
    }
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    match self {
      Self::Local(io) => io.sync_data(tbl).await,
      Self::Mem(io) => io.sync_data(tbl).await,
      Self::Cloud(io) => io.sync_data(tbl).await,
    }
  }
}

impl ManifestIo for StoreBackend {
  async fn reset_manifest(&self, tbl: &TableIdent, size: u64) -> Result<()> {
    match self {
      Self::Local(io) => io.reset_manifest(tbl, size).await,
      Self::Mem(io) => io.reset_manifest(tbl, size).await,
      Self::Cloud(io) => io.reset_manifest(tbl, size).await,
    }
  }

  async fn reset_manifest_file(&self, tbl: &TableIdent, name: &str, size: u64) -> Result<()> {
    match self {
      Self::Local(io) => io.reset_manifest_file(tbl, name, size).await,
      Self::Mem(io) => io.reset_manifest_file(tbl, name, size).await,
      Self::Cloud(io) => io.reset_manifest_file(tbl, name, size).await,
    }
  }

  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    match self {
      Self::Local(io) => io.append_manifest(tbl, record).await,
      Self::Mem(io) => io.append_manifest(tbl, record).await,
      Self::Cloud(io) => io.append_manifest(tbl, record).await,
    }
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    match self {
      Self::Local(io) => io.switch_manifest(tbl, record).await,
      Self::Mem(io) => io.switch_manifest(tbl, record).await,
      Self::Cloud(io) => io.switch_manifest(tbl, record).await,
    }
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    match self {
      Self::Local(io) => io.read_manifest(tbl).await,
      Self::Mem(io) => io.read_manifest(tbl).await,
      Self::Cloud(io) => io.read_manifest(tbl).await,
    }
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
    match self {
      Self::Local(io) => io.drop_manifest(tbl).await,
      Self::Mem(io) => io.drop_manifest(tbl).await,
      Self::Cloud(io) => io.drop_manifest(tbl).await,
    }
  }

  async fn refresh_manifest(&self, tbl: &TableIdent) -> Result<()> {
    match self {
      Self::Local(io) => io.refresh_manifest(tbl).await,
      Self::Mem(io) => io.refresh_manifest(tbl).await,
      Self::Cloud(io) => io.refresh_manifest(tbl).await,
    }
  }

  async fn drop_local_manifest(&self, tbl: &TableIdent) -> Result<()> {
    match self {
      Self::Local(io) => io.drop_local_manifest(tbl).await,
      Self::Mem(io) => io.drop_local_manifest(tbl).await,
      Self::Cloud(io) => io.drop_local_manifest(tbl).await,
    }
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    match self {
      Self::Local(io) => io.write_manifest_file(tbl, name, record).await,
      Self::Mem(io) => io.write_manifest_file(tbl, name, record).await,
      Self::Cloud(io) => io.write_manifest_file(tbl, name, record).await,
    }
  }

  async fn append_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    match self {
      Self::Local(io) => io.append_manifest_file(tbl, name, record).await,
      Self::Mem(io) => io.append_manifest_file(tbl, name, record).await,
      Self::Cloud(io) => io.append_manifest_file(tbl, name, record).await,
    }
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    match self {
      Self::Local(io) => io.list_manifest_file_name_li(tbl).await,
      Self::Mem(io) => io.list_manifest_file_name_li(tbl).await,
      Self::Cloud(io) => io.list_manifest_file_name_li(tbl).await,
    }
  }

  /// 旁路文件（segment 完成标记）必须按当前后端显式转发，不做空实现兜底。
  async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    match self {
      Self::Local(io) => io.list_sidecar_file_name_li(tbl).await,
      Self::Mem(io) => io.list_sidecar_file_name_li(tbl).await,
      Self::Cloud(io) => io.list_sidecar_file_name_li(tbl).await,
    }
  }

  async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    match self {
      Self::Local(io) => io.remove_sidecar_file_name(tbl, name).await,
      Self::Mem(io) => io.remove_sidecar_file_name(tbl, name).await,
      Self::Cloud(io) => io.remove_sidecar_file_name(tbl, name).await,
    }
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    match self {
      Self::Local(io) => io.read_manifest_file(tbl, name).await,
      Self::Mem(io) => io.read_manifest_file(tbl, name).await,
      Self::Cloud(io) => io.read_manifest_file(tbl, name).await,
    }
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    match self {
      Self::Local(io) => io.remove_manifest_file(tbl, name).await,
      Self::Mem(io) => io.remove_manifest_file(tbl, name).await,
      Self::Cloud(io) => io.remove_manifest_file(tbl, name).await,
    }
  }

  async fn prune_superseded_manifest_file_li(
    &self,
    tbl: &TableIdent,
    branch: &str,
    term: u64,
  ) -> Result<usize> {
    match self {
      Self::Local(io) => {
        io.prune_superseded_manifest_file_li(tbl, branch, term)
          .await
      }
      Self::Mem(io) => {
        io.prune_superseded_manifest_file_li(tbl, branch, term)
          .await
      }
      Self::Cloud(io) => {
        io.prune_superseded_manifest_file_li(tbl, branch, term)
          .await
      }
    }
  }
}

impl TermIo for StoreBackend {
  async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>> {
    match self {
      Self::Local(io) => io.read_term(branch, shard_id).await,
      Self::Mem(io) => io.read_term(branch, shard_id).await,
      Self::Cloud(io) => io.read_term(branch, shard_id).await,
    }
  }

  async fn upsert_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<u64> {
    match self {
      Self::Local(io) => io.upsert_term(branch, shard_id, term).await,
      Self::Mem(io) => io.upsert_term(branch, shard_id, term).await,
      Self::Cloud(io) => io.upsert_term(branch, shard_id, term).await,
    }
  }

  async fn remove_term(&self, branch: &str, shard_id: u32) -> Result<()> {
    match self {
      Self::Local(io) => io.remove_term(branch, shard_id).await,
      Self::Mem(io) => io.remove_term(branch, shard_id).await,
      Self::Cloud(io) => io.remove_term(branch, shard_id).await,
    }
  }
}

impl SegmentIo for StoreBackend {
  fn write_segment_len(&self, src_len: usize) -> u64 {
    match self {
      Self::Local(io) => io.write_segment_len(src_len),
      Self::Mem(io) => io.write_segment_len(src_len),
      Self::Cloud(io) => io.write_segment_len(src_len),
    }
  }

  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    match self {
      Self::Local(io) => io.read_segment(tbl, segment_fp_id, dst).await,
      Self::Mem(io) => io.read_segment(tbl, segment_fp_id, dst).await,
      Self::Cloud(io) => io.read_segment(tbl, segment_fp_id, dst).await,
    }
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> Result<()> {
    match self {
      Self::Local(io) => io.write_segment(tbl, segment_fp_id, src).await,
      Self::Mem(io) => io.write_segment(tbl, segment_fp_id, src).await,
      Self::Cloud(io) => io.write_segment(tbl, segment_fp_id, src).await,
    }
  }
}

impl FileIo for StoreBackend {
  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    match self {
      Self::Local(io) => io.list_file_id_li(tbl).await,
      Self::Mem(io) => io.list_file_id_li(tbl).await,
      Self::Cloud(io) => io.list_file_id_li(tbl).await,
    }
  }

  async fn list_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    match self {
      Self::Local(io) => io.list_file_variants(tbl).await,
      Self::Mem(io) => io.list_file_variants(tbl).await,
      Self::Cloud(io) => io.list_file_variants(tbl).await,
    }
  }

  async fn list_local_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    match self {
      Self::Local(io) => io.list_local_file_variants(tbl).await,
      Self::Mem(io) => io.list_local_file_variants(tbl).await,
      Self::Cloud(io) => io.list_local_file_variants(tbl).await,
    }
  }

  async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>> {
    match self {
      Self::Local(io) => io.read_file(tbl, file_id).await,
      Self::Mem(io) => io.read_file(tbl, file_id).await,
      Self::Cloud(io) => io.read_file(tbl, file_id).await,
    }
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()> {
    match self {
      Self::Local(io) => io.write_file(tbl, file_id, bytes).await,
      Self::Mem(io) => io.write_file(tbl, file_id, bytes).await,
      Self::Cloud(io) => io.write_file(tbl, file_id, bytes).await,
    }
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>> {
    match self {
      Self::Local(io) => io.file_size(tbl, file_id).await,
      Self::Mem(io) => io.file_size(tbl, file_id).await,
      Self::Cloud(io) => io.file_size(tbl, file_id).await,
    }
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    match self {
      Self::Local(io) => io.remove_file(tbl, file_id).await,
      Self::Mem(io) => io.remove_file(tbl, file_id).await,
      Self::Cloud(io) => io.remove_file(tbl, file_id).await,
    }
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    match self {
      Self::Local(io) => io.list_segment_file_id_li(tbl).await,
      Self::Mem(io) => io.list_segment_file_id_li(tbl).await,
      Self::Cloud(io) => io.list_segment_file_id_li(tbl).await,
    }
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    match self {
      Self::Local(io) => io.remove_segment_file(tbl, file_id).await,
      Self::Mem(io) => io.remove_segment_file(tbl, file_id).await,
      Self::Cloud(io) => io.remove_segment_file(tbl, file_id).await,
    }
  }

  async fn list_local_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    match self {
      Self::Local(io) => io.list_local_file_id_li(tbl).await,
      Self::Mem(io) => io.list_local_file_id_li(tbl).await,
      Self::Cloud(io) => io.list_local_file_id_li(tbl).await,
    }
  }

  async fn remove_local_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    match self {
      Self::Local(io) => io.remove_local_file(tbl, file_id).await,
      Self::Mem(io) => io.remove_local_file(tbl, file_id).await,
      Self::Cloud(io) => io.remove_local_file(tbl, file_id).await,
    }
  }

  async fn list_local_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    match self {
      Self::Local(io) => io.list_local_segment_file_id_li(tbl).await,
      Self::Mem(io) => io.list_local_segment_file_id_li(tbl).await,
      Self::Cloud(io) => io.list_local_segment_file_id_li(tbl).await,
    }
  }

  async fn remove_local_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    match self {
      Self::Local(io) => io.remove_local_segment_file(tbl, file_id).await,
      Self::Mem(io) => io.remove_local_segment_file(tbl, file_id).await,
      Self::Cloud(io) => io.remove_local_segment_file(tbl, file_id).await,
    }
  }

  async fn remove_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    match self {
      Self::Local(io) => io.remove_file_variant(tbl, file_id, branch, term).await,
      Self::Mem(io) => io.remove_file_variant(tbl, file_id, branch, term).await,
      Self::Cloud(io) => io.remove_file_variant(tbl, file_id, branch, term).await,
    }
  }

  async fn remove_local_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    match self {
      Self::Local(io) => {
        io.remove_local_file_variant(tbl, file_id, branch, term)
          .await
      }
      Self::Mem(io) => {
        io.remove_local_file_variant(tbl, file_id, branch, term)
          .await
      }
      Self::Cloud(io) => {
        io.remove_local_file_variant(tbl, file_id, branch, term)
          .await
      }
    }
  }

  async fn remove_segment_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    match self {
      Self::Local(io) => {
        io.remove_segment_file_variant(tbl, file_id, branch, term)
          .await
      }
      Self::Mem(io) => {
        io.remove_segment_file_variant(tbl, file_id, branch, term)
          .await
      }
      Self::Cloud(io) => {
        io.remove_segment_file_variant(tbl, file_id, branch, term)
          .await
      }
    }
  }

  async fn remove_local_segment_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    match self {
      Self::Local(io) => {
        io.remove_local_segment_file_variant(tbl, file_id, branch, term)
          .await
      }
      Self::Mem(io) => {
        io.remove_local_segment_file_variant(tbl, file_id, branch, term)
          .await
      }
      Self::Cloud(io) => {
        io.remove_local_segment_file_variant(tbl, file_id, branch, term)
          .await
      }
    }
  }

  async fn list_segment_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    match self {
      Self::Local(io) => io.list_segment_file_variants(tbl).await,
      Self::Mem(io) => io.list_segment_file_variants(tbl).await,
      Self::Cloud(io) => io.list_segment_file_variants(tbl).await,
    }
  }

  async fn list_local_segment_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    match self {
      Self::Local(io) => io.list_local_segment_file_variants(tbl).await,
      Self::Mem(io) => io.list_local_segment_file_variants(tbl).await,
      Self::Cloud(io) => io.list_local_segment_file_variants(tbl).await,
    }
  }

  async fn try_remove_empty_partition_dir(&self, tbl: &TableIdent) -> Result<bool> {
    match self {
      Self::Local(io) => io.try_remove_empty_partition_dir(tbl).await,
      Self::Mem(io) => io.try_remove_empty_partition_dir(tbl).await,
      Self::Cloud(io) => io.try_remove_empty_partition_dir(tbl).await,
    }
  }
}

impl FixedSegmentIo for StoreBackend {
  fn register_fixed_buffer(&self, len: usize) -> Result<FixedBuffer> {
    match self {
      Self::Local(io) => io.register_fixed_buffer(len),
      Self::Mem(io) => io.register_fixed_buffer(len),
      Self::Cloud(io) => io.register_fixed_buffer(len),
    }
  }

  async fn read_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &mut FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    match self {
      Self::Local(io) => {
        io.read_segment_fixed(tbl, segment_fp_id, buf, offset, len)
          .await
      }
      Self::Mem(io) => {
        io.read_segment_fixed(tbl, segment_fp_id, buf, offset, len)
          .await
      }
      Self::Cloud(io) => {
        io.read_segment_fixed(tbl, segment_fp_id, buf, offset, len)
          .await
      }
    }
  }

  async fn write_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    match self {
      Self::Local(io) => {
        io.write_segment_fixed(tbl, segment_fp_id, buf, offset, len)
          .await
      }
      Self::Mem(io) => {
        io.write_segment_fixed(tbl, segment_fp_id, buf, offset, len)
          .await
      }
      Self::Cloud(io) => {
        io.write_segment_fixed(tbl, segment_fp_id, buf, offset, len)
          .await
      }
    }
  }
}

/// 校验 `KvOption::cloud_store_path`（`bucket/prefix`）并拆成两段。
pub fn split_cloud_store_path(path: &str) -> Result<(String, String)> {
  let path = path.strip_suffix('/').unwrap_or(path);
  let (bucket, prefix) = path.split_once('/').unwrap_or((path, ""));
  if bucket.is_empty() {
    return Err(KvError::InvalidArgs);
  }
  Ok((bucket.to_owned(), prefix.to_owned()))
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Mem 后端没有远端对象：Prewarm / ListObject 必须显式回 `InvalidArgs`，
  /// 不能静默成功（否则调用方会把「没做」当成「已完成」）。
  #[compio::test]
  async fn mem_backend_rejects_cloud_only_operations() {
    let backend = StoreBackend::Mem(MemStore::new(512).into());
    let tbl = TableIdent::new("demo", 1).unwrap();
    assert_eq!(
      backend.prewarm_table(&tbl).await.unwrap_err(),
      KvError::InvalidArgs
    );
    assert_eq!(
      backend
        .list_object("fkv/demo.1/", None, 100)
        .await
        .unwrap_err(),
      KvError::InvalidArgs
    );
  }
}
