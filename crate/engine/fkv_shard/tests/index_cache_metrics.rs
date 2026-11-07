//! eng-66：index cache 的命中 / 未命中 / 逐出 / 占用进入 shard 指标。
use std::{path::PathBuf, sync::Arc};

use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_io_mem::MemStore;
use fkv_metrics::ShardMetric;
use fkv_shard::{Backend, Shard, ShardConfig, metered::MeteredIo};
use fkv_store::{AllocMode, Store};
use fkv_testkit::TempRoot;
use fkv_types::{StoreMode, TableIdent};

fn config(root: PathBuf, index_cache_bytes: usize) -> ShardConfig {
  ShardConfig {
    alloc_mode: AllocMode::Append,
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
  let root = TempRoot::new("index_cache_hit");
  let shard = Shard::start(config(
    root.path().to_path_buf(),
    fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize,
  ))
  .unwrap();
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
  let root = TempRoot::new("index_cache_evict");
  // 预算约一页：第二条插入即超预算触发逐出。
  let shard = Shard::start(config(root.path().to_path_buf(), 512)).unwrap();
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

// 以下 3 个用例原属 `fkv_store/tests/index_cache.rs`；它们用 `MeteredIo`
// 统计页读次数，随装饰器一起迁到 shard 侧（`fkv_store` 不再依赖
// `fkv_metrics` 的 IO 部分）。

/// 索引页缓存：warm 后点读只产生 1 次 data page IO，不再逐层读 index。
#[compio::test]
async fn index_cache_eliminates_index_io_after_warmup() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  // 小页 + 多 entry，构造 3 层以上的树。
  let metrics = Arc::new(ShardMetric::new(0));
  let io = MeteredIo::new(MemStore::new(256), metrics.clone());
  let mut store = Store::new(io, 256, 4);

  let batch: Vec<BatchEntry> = (0..2000u32)
    .map(|i| BatchEntry::upsert(format!("k{i:06}").into_bytes(), vec![b'v'; 8], 1))
    .collect();
  store.put(&tbl, &batch).await.unwrap();

  // warm：填充 index cache。
  let _ = store.get(&tbl, b"k001000").await.unwrap().unwrap();

  let before = metrics.snapshot().page_reads;
  for i in 0..1000u32 {
    let key = format!("k{:06}", (i * 2) % 2000);
    assert!(store.get(&tbl, key.as_bytes()).await.unwrap().is_some());
  }
  let reads = metrics.snapshot().page_reads - before;
  // 每次点读只允许 1 次 data page IO（允许极小的边界抖动）。
  assert!(
    reads <= 1000 + 10,
    "expected ~1 page read/op after warm cache, got {reads}"
  );
}

/// 27b：写提交把未变更索引页的缓存条目重标到新映射代际，后续读仍能命中；
/// 只有被重写的路径（root 等）需要重新读 index 页。
#[compio::test]
async fn write_retags_unmodified_index_pages_for_next_reads() {
  let tbl = TableIdent::new("demo", 7).unwrap();
  let metrics = Arc::new(ShardMetric::new(0));
  let io = MeteredIo::new(MemStore::new(256), metrics.clone());
  let mut store = Store::new(io, 256, 4);
  let batch: Vec<BatchEntry> = (0..2000u32)
    .map(|i| BatchEntry::upsert(format!("k{i:06}").into_bytes(), vec![b'v'; 8], 1))
    .collect();
  store.put(&tbl, &batch).await.unwrap();

  // warm 左半区（与随后写的 "zzz" 路径无关）。
  let keys: Vec<Vec<u8>> = (0..1000u32)
    .step_by(7)
    .map(|i| format!("k{i:06}").into_bytes())
    .collect();
  for key in &keys {
    assert!(store.get(&tbl, key).await.unwrap().is_some());
  }
  assert!(
    store.index_cache_len(&tbl) >= 2,
    "need root + leaf index pages cached to detect generation regression"
  );
  store
    .put(
      &tbl,
      &[BatchEntry::upsert(b"zzz".to_vec(), b"v".to_vec(), 2)],
    )
    .await
    .unwrap();

  let before = metrics.snapshot().page_reads;
  for key in &keys {
    assert!(store.get(&tbl, key).await.unwrap().is_some());
  }
  let reads = metrics.snapshot().page_reads - before;
  // 写后 root 被重写需重读；叶子 index 命中缓存，故每次点读只有 root + data
  // 两次 IO。若未重标代际，叶子 index 也全部 miss（3 次/读）。
  let limit = keys.len() as u64 * 2 + 8;
  assert!(
    reads <= limit,
    "unmodified index pages must stay hittable after write: reads={reads} limit={limit}"
  );
}

/// eng-67：reopen 后逻辑页仍映射到同一物理页，索引缓存必须保留（等价 C++
/// `InstallExternalSnapshot` 的 swizzle 复用），读同一 key 只剩 data 页 IO。
#[compio::test]
async fn reopen_reuses_index_cache_when_file_page_ids_unchanged() {
  let tbl = TableIdent::new("demo", 5).unwrap();
  let metrics = Arc::new(ShardMetric::new(0));
  let io = MeteredIo::new(MemStore::new(256), metrics.clone());
  let mut store = Store::new(io, 256, 4);
  let batch: Vec<BatchEntry> = (0..2000u32)
    .map(|i| BatchEntry::upsert(format!("k{i:06}").into_bytes(), vec![b'v'; 8], 1))
    .collect();
  store.put(&tbl, &batch).await.unwrap();
  for i in (0..2000u32).step_by(11) {
    let key = format!("k{i:06}");
    assert!(store.get(&tbl, key.as_bytes()).await.unwrap().is_some());
  }
  let warm = store.index_cache_len(&tbl);
  assert!(warm > 1, "warm cache too small: {warm}");

  store.reopen(&tbl).await.unwrap();
  assert_eq!(
    store.index_cache_len(&tbl),
    warm,
    "reopen must keep index pages whose file_page_id is unchanged"
  );

  // 已 warm 的 key：复用后只允许 1 次 data page IO。
  let before = metrics.snapshot().page_reads;
  assert!(store.get(&tbl, b"k001001").await.unwrap().is_some());
  assert_eq!(
    metrics.snapshot().page_reads - before,
    1,
    "reused cache must cost only the data page read"
  );
}
