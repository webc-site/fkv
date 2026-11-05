//! eng-66：index cache 的命中 / 未命中 / 逐出 / 占用进入 shard 指标。
use std::{
  env::temp_dir,
  fs::create_dir_all,
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_types::{StoreMode, TableIdent};

fn temp_root(name: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_index_cache_{name}_{}_{}_{}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos(),
    seq
  ));
  create_dir_all(&root).unwrap();
  root
}

fn config(root: PathBuf, index_cache_bytes: usize) -> ShardConfig {
  ShardConfig {
    shard_id: 0,
    root,
    page_size: 256,
    restart_interval: 16,
    index_restart_interval: 16,
    pages_per_file_shift: 18,
    segment_size: 4096,
    segments_per_file_shift: 2,
    large_value_threshold: 1024,
    fixed_buffer_size: 0,
    queue_size: 32,
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
    fd_limit: 0,
    index_cache_bytes,
    index_cache_total_bytes: index_cache_bytes,
    root_meta_cache_bytes: 0,
    io_queue_size: 4096,
    backend: Backend::Mem,
    mode: StoreMode::Local,
    standby: None,
    max_write_concurrency: 0,
    max_processing_time_us: 0,
    yield_budget_us: 0,
  }
}

fn key(index: u32) -> Vec<u8> {
  format!("key-{index:06}").into_bytes()
}

fn put_batch(tbl: &TableIdent) -> Request {
  Request::Put {
    tbl: tbl.clone(),
    batch_li: (0..2000u32)
      .map(|index| BatchEntry::upsert(key(index), vec![b'v'; 8], 1))
      .collect(),
    term: Some(1),
  }
}

async fn attach(shard: &Shard) {
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
}

async fn get(shard: &Shard, tbl: &TableIdent, index: u32) {
  match shard
    .request(Request::Get {
      tbl: tbl.clone(),
      key: key(index),
    })
    .await
    .unwrap()
  {
    Response::Get(Some(value)) => assert_eq!(value, vec![b'v'; 8]),
    other => panic!("unexpected response: {other:?}"),
  }
}

/// 默认预算：冷读计数未命中，重复读命中，占用与容量可见。
#[compio::test]
async fn snapshot_reports_hits_misses_and_bytes() {
  let root = temp_root("hit");
  let shard = Shard::start(config(root, fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("cache", 1).unwrap();
  shard.request(put_batch(&tbl)).await.unwrap();

  get(&shard, &tbl, 1000).await;
  let cold = shard.metrics_snapshot();
  assert_eq!(cold.cache_hit, 0, "cold read must not hit: {cold:?}");
  assert!(cold.cache_miss > 0, "cold read must miss: {cold:?}");
  assert!(cold.cache_bytes > 0, "resident bytes must be reported");
  assert_eq!(cold.cache_evict, 0);

  get(&shard, &tbl, 1000).await;
  let warm = shard.metrics_snapshot();
  assert!(warm.cache_hit > 0, "second read must hit: {warm:?}");
  assert_eq!(warm.cache_miss, cold.cache_miss);
  shard.shutdown().await.unwrap();
}

/// 极小预算：逐出计数上报，容量旋钮真正限制常驻。
#[compio::test]
async fn snapshot_reports_evictions_under_tiny_budget() {
  let root = temp_root("evict");
  // 预算约一页：第二条插入即超预算触发逐出。
  let shard = Shard::start(config(root, 512)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("cache", 2).unwrap();
  shard.request(put_batch(&tbl)).await.unwrap();

  for index in (0..2000u32).step_by(7) {
    get(&shard, &tbl, index).await;
  }
  let snap = shard.metrics_snapshot();
  assert!(snap.cache_miss > 0);
  assert!(snap.cache_evict > 0, "tiny budget must evict: {snap:?}");
  // 一页能放进预算，Clock 保留最新一页。
  assert!(snap.cache_bytes > 0);
  shard.shutdown().await.unwrap();
}
