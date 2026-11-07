//! 运行时侧 `CloudStore` 装配参数与构造（只读、共享预算组）。

use std::path::PathBuf;

use fkv_cloud::CloudStore;
use fkv_error::Result;
use fkv_io_compio::CompioStore;
use fkv_object_s3::S3ObjectStore;
use fkv_shard::{CloudBackend, ShardConfig};

/// 构造运行时侧 `CloudStore` 所需的装配参数（从 `ShardConfig` 投影）。
///
/// 运行时侧的预热线程 / 归档 crond / 全局列举都需要一个 `CloudStore`，但它们
/// 不该长期持有一整份 `ShardConfig`。
#[derive(Debug, Clone)]
pub(crate) struct CloudStoreConfig {
  pub(crate) root: PathBuf,
  pub(crate) page_size: usize,
  pub(crate) pages_per_file_shift: u8,
  pub(crate) segment_size: u32,
  pub(crate) segments_per_file_shift: u8,
  pub(crate) max_write_batch_pages: usize,
  pub(crate) max_inflight_write: usize,
  pub(crate) fd_limit: usize,
  pub(crate) allow_reuse_local_caches: bool,
}

impl CloudStoreConfig {
  pub(crate) fn from_shard(config: &ShardConfig, cloud: &CloudBackend) -> Self {
    Self {
      root: config.root.clone(),
      page_size: config.page_size,
      pages_per_file_shift: config.pages_per_file_shift,
      segment_size: config.segment_size,
      segments_per_file_shift: config.segments_per_file_shift,
      max_write_batch_pages: config.max_write_batch_pages,
      max_inflight_write: config.max_inflight_write,
      fd_limit: config.fd_limit,
      allow_reuse_local_caches: cloud.allow_reuse_local_caches,
    }
  }
}

/// 构造 Cloud 模式的运行时侧 `CloudStore`（本地 cache + S3）。
///
/// `CloudStore` 内含 `RefCell` 非 `Sync`，shard 与运行时侧的预热线程 / 归档 crond
/// 必须各持实例，因此这里按同一装配参数（同一 root / bucket / prefix / segment
/// 配置）复刻 `StoreBackend::cloud`。
///
/// **有意不接 `ShardMetric`（cld-55）**：这里的实例服务 prewarm crond /
/// 全局列举等运行时侧工作，不归属任何 shard，把某个 shard 的计数器挂上去会让
/// 「这个 cache 的命中率」指向错误的 shard。shard 自己的实例在
/// `fkv_shard::StoreBackend::cloud` 里接线。
///
/// **预算边界（12d）**：实例不再各记一份 `local_space_limit`，而是与全部 shard
/// 共用 `CloudBackend::cache_group`：预热下载、shard 读写都计入同一份全局驻留
/// 记账，淘汰也按全局用量触发，因此整机本地 cache 上界是 `local_space_limit`
/// 而不是 `(shard_num + 1) × local_space_limit`。归档 crond / 全局列举实例只做
/// 远端列举、不写本地 cache，共享预算组不产生额外占用。
///
/// **只读边界**：本函数产出的实例（预热 / 全局列举 / 归档 crond）都不是分区
/// 写实例：`read_only(true)` 只关闭表级 segment scheme 的落盘与回填，安装 /
/// 读 cache 照常进行。跨进程共用同一本地 root 时只有该分区的写实例（shard
/// 侧 `StoreBackend::cloud`）能启用 scheme，否则预热会把对方进程正在写的
/// 半装文件按旧尾槽规则盖成「完整」。
pub(crate) fn open_runtime_cloud_store(
  config: &CloudStoreConfig,
  cloud: &CloudBackend,
) -> Result<CloudStore<CompioStore, S3ObjectStore>> {
  let local = CompioStore::new(
    config.root.clone(),
    config.page_size,
    config.pages_per_file_shift,
  )
  .with_segment_config(config.segment_size as usize, config.segments_per_file_shift)
  .with_managed_segment_read()
  .with_max_write_batch_pages(config.max_write_batch_pages)
  .with_max_inflight_write(config.max_inflight_write)
  .with_fd_cache_limit(config.fd_limit)
  // C++ 同一 IouringMgr 管本地 cache 与本地 store：data / segment 均 O_DIRECT。
  .with_direct_io(true);
  // 与 shard 侧共用同一份 `s3_config()`：并发上限按实例数拆分，禁止两侧分叉。
  let remote = S3ObjectStore::start(cloud.s3_config())?;
  let store = CloudStore::new(
    local,
    remote,
    cloud.prefix.clone(),
    config.pages_per_file_shift,
  )
  .with_segment_config(config.segment_size as usize, config.segments_per_file_shift)
  .with_cache_group(cloud.cache_group.clone())
  .with_reuse_local_caches(config.allow_reuse_local_caches)
  // 运行时侧实例一律只读：scheme 回填权归该分区的 shard 写实例。
  .with_read_only(true);
  // 装配点自检：与 shard 侧共用同一份 KvOption 投影，禁止默认值分叉。
  debug_assert_eq!(
    store.reuse_local_caches(),
    config.allow_reuse_local_caches,
    "运行时侧 CloudStore 未接线 allow_reuse_local_caches"
  );
  debug_assert!(
    store.read_only(),
    "运行时侧 CloudStore 必须置只读，不得参与 segment scheme 回填"
  );
  debug_assert_eq!(
    config.allow_reuse_local_caches, cloud.allow_reuse_local_caches,
    "运行时侧与 shard 侧的 allow_reuse_local_caches 不一致"
  );
  Ok(store)
}
