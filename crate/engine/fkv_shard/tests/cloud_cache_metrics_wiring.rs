//! cld-55：Cloud 后端的本地 cache 指标必须挂到**本 shard** 的 `ShardMetric`。
//!
//! 只断言 `Arc::ptr_eq`：计数断言证明不了接线（「没接线」与「一直没命中」
//! 在计数上都表现为 0）。真正的计数行为在
//! `fkv_cloud/tests/cloud.rs::cache_metrics_record_hit_miss_and_eviction`。
//!
//! 反向验证：去掉 `StoreBackend::cloud` 里的 `.with_metrics(metrics)`，
//! 本测试立即失败（`metrics()` 返回 `None`）。

use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::Arc,
};

use fkv_cloud::CacheGroup;
use fkv_metrics::ShardMetric;
use fkv_shard::{Backend, CloudBackend, ShardConfig, StoreBackend};
use fkv_types::StoreMode;

fn config(root: PathBuf, backend: Backend) -> ShardConfig {
  ShardConfig {
    shard_id: 3,
    root,
    page_size: 512,
    restart_interval: 16,
    index_restart_interval: 16,
    pages_per_file_shift: 18,
    segment_size: 4096,
    segments_per_file_shift: 2,
    large_value_threshold: 1024,
    fixed_buffer_size: 0,
    queue_size: 128,
    auto_reopen_retry_times: 0,
    auto_reopen_pending_time_us: 0,
    oom_retry_times: 0,
    compression_level: None,
    verify_checksum: true,
    overflow_pointers: fkv_page::DEFAULT_OVERFLOW_POINTERS,
    file_amplify_factor: 0,
    segment_file_amplify_factor: 0,
    manifest_limit: 8 * 1024 * 1024,
    max_write_batch_pages: 32,
    max_inflight_write: 32 * 1024,
    fd_limit: 1024,
    index_cache_bytes: 1024 * 1024,
    index_cache_total_bytes: 1024 * 1024,
    root_meta_cache_bytes: 0,
    io_queue_size: 256,
    backend,
    mode: StoreMode::Cloud,
    standby: None,
    max_write_concurrency: 0,
    max_processing_time_us: 0,
    yield_budget_us: 0,
  }
}

#[compio::test]
async fn cloud_backend_attaches_shard_metrics() {
  let root = temp_dir().join(format!("fkv_shard_cloud_metrics_{}", id()));
  let _ = remove_dir_all(&root);
  create_dir_all(&root).unwrap();
  // 只建客户端不连网络（endpoint 故意不可达）：本测试不发起任何对象请求。
  let cloud = CloudBackend {
    bucket: "bucket".to_owned(),
    prefix: "fkv".to_owned(),
    endpoint: "http://127.0.0.1:1".to_owned(),
    region: "us-east-1".to_owned(),
    access_key: "k".to_owned(),
    secret_key: "s".to_owned(),
    allow_http: true,
    cache_group: Arc::new(CacheGroup::new(4096)),
    allow_reuse_local_caches: true,
    max_cloud_concurrency: 0,
    s3_instance_num: 1,
    verify_ssl: true,
  };
  let metrics = Arc::new(ShardMetric::new(3));
  let backend = StoreBackend::cloud(
    &config(root.clone(), Backend::Cloud(cloud.clone())),
    &cloud,
    metrics.clone(),
  )
  .unwrap();

  let StoreBackend::Cloud(store) = &backend else {
    panic!("Backend::Cloud 必须装配成 Cloud 后端");
  };
  let attached = store
    .metrics()
    .expect("cloud cache 指标未接线（cld-55 回归）：cache hit/miss/evict/bytes 会恒 0");
  assert!(
    Arc::ptr_eq(attached, &metrics),
    "挂的不是本 shard 的 ShardMetric 实例"
  );
  // 12d：shard 的 cache 预算必须挂在 `CloudBackend` 的共享组上，否则每个 shard
  // 各记一份 `local_space_limit`，整机 cache 上界会放大到 shard_num 倍。
  assert!(
    Arc::ptr_eq(store.cache_group(), &cloud.cache_group),
    "shard CloudStore 未共享 CloudBackend::cache_group"
  );
  // `allow_reuse_local_caches` 必须真的透传：否则配置是死旋钮，且默认值
  // （CloudStore 复用=true）与 C++ 默认（false）相反。
  assert!(
    store.reuse_local_caches(),
    "shard CloudStore 未接线 allow_reuse_local_caches"
  );
  // 装配自检：shard 是分区写实例，必须独占表级 segment scheme 的启用 /
  // 回填；运行时侧预热 / 列举 / 归档实例全部只读。
  assert!(
    !store.read_only(),
    "shard 写实例必须 read_only(false)，否则 scheme 回填无人负责"
  );

  let _ = remove_dir_all(&root);
}
