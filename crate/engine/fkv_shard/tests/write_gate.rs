//! per-partition FIFO 写队列：同一分区串行、跨分区独立（eng-33）；
//! 队列深度 / 在跑写进入 shard 指标（eng-33b）。
use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  hint::spin_loop,
  path::PathBuf,
  process::id,
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
  },
  thread,
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use compio::runtime::spawn;
use fkv_api::{Request, Response, ScanPageOption};
use fkv_btree::BatchEntry;
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_types::{StoreMode, TableIdent};

const KEY_NUM: u32 = 16;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_write_gate_{}_{}_{}",
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

fn config(root: PathBuf, max_write_concurrency: usize) -> ShardConfig {
  ShardConfig {
    shard_id: 0,
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
    fd_limit: 10_000,
    index_cache_bytes: fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize,
    index_cache_total_bytes: fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize,
    root_meta_cache_bytes: 0,
    io_queue_size: 4096,
    backend: Backend::default(),
    mode: StoreMode::Local,
    standby: None,
    max_write_concurrency,
    // 关掉时间片，避免测试时序受轮预算影响；优先级本身由 priority 测试覆盖。
    max_processing_time_us: 0,
    yield_budget_us: 0,
  }
}

fn key(index: u32) -> Vec<u8> {
  format!("key-{index:03}").into_bytes()
}

fn val(index: u32) -> Vec<u8> {
  format!("value-{index:03}").into_bytes()
}

fn put(tbl: TableIdent, index: u32) -> Request {
  Request::Put {
    tbl,
    batch_li: vec![BatchEntry::upsert(key(index), val(index), 1)],
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

/// 同一分区并发写：全局并发限 1 时全部成功且无死锁，最终 16 条都在。
#[compio::test]
async fn same_table_concurrent_writes_all_land() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), 1)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("fifo", 1).unwrap();

  let mut handle_li = Vec::new();
  for index in 0..KEY_NUM {
    let sender = shard.sender();
    let tbl = tbl.clone();
    handle_li.push(spawn(async move { sender.request(put(tbl, index)).await }));
  }
  for handle in handle_li {
    handle.await.unwrap().unwrap();
  }

  for index in 0..KEY_NUM {
    match shard
      .request(Request::Get {
        tbl: tbl.clone(),
        key: key(index),
      })
      .await
      .unwrap()
    {
      Response::Get(Some(got)) => assert_eq!(got, val(index)),
      other => panic!("unexpected response for {index}: {other:?}"),
    }
  }
  shard.shutdown().await.unwrap();
  let _ = remove_dir_all(root);
}

/// 跨分区写互不阻塞：不同分区各自 FIFO，全部落盘。
#[compio::test]
async fn different_tables_write_independently() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), 0)).unwrap();
  attach(&shard).await;
  let tbl_a = TableIdent::new("table_a", 1).unwrap();
  let tbl_b = TableIdent::new("table_b", 1).unwrap();

  let mut handle_li = Vec::new();
  for index in 0..KEY_NUM {
    let sender = shard.sender();
    let tbl = tbl_a.clone();
    handle_li.push(spawn(async move { sender.request(put(tbl, index)).await }));
    let sender = shard.sender();
    let tbl = tbl_b.clone();
    handle_li.push(spawn(async move { sender.request(put(tbl, index)).await }));
  }
  for handle in handle_li {
    handle.await.unwrap().unwrap();
  }

  for tbl in [tbl_a, tbl_b] {
    match shard
      .request(Request::Scan {
        tbl,
        begin: key(0),
        end: b"key-zzz".to_vec(),
        max_entries: 0,
        option: ScanPageOption::default(),
      })
      .await
      .unwrap()
    {
      Response::Scan(reply) => assert_eq!(reply.entry_li.len(), KEY_NUM as usize),
      other => panic!("unexpected scan response: {other:?}"),
    }
  }
  shard.shutdown().await.unwrap();
  let _ = remove_dir_all(root);
}

/// 独立线程高频采样指标；置停止开关后 join 得到（在跑写峰值, 队列深度峰值）。
///
/// 采样放 OS 线程而非 compio 任务：shard 在独立线程写 gauge，采样不能占
/// 测试 runtime 的 poll 预算，否则观测不到短命写的在跑窗口。
fn watch_write_gauges(
  metrics: Arc<fkv_metrics::ShardMetric>,
) -> (thread::JoinHandle<(u64, u64)>, Arc<AtomicBool>) {
  let stop = Arc::new(AtomicBool::new(false));
  let stop_thread = stop.clone();
  let handle = thread::spawn(move || {
    let mut max_running = 0u64;
    let mut max_depth = 0u64;
    while !stop_thread.load(Ordering::Relaxed) {
      let snap = metrics.snapshot();
      max_running = max_running.max(snap.write_running);
      max_depth = max_depth.max(snap.write_queue_depth);
      spin_loop();
    }
    (max_running, max_depth)
  });
  (handle, stop)
}

/// eng-33b 验收：`max_write_concurrency = 1` + 同分区 N 条写时在跑写最大为 1；
/// 观测结束后队列与在跑数都回到 0（口径与 eng-32c 的 in-flight 一致）。
#[compio::test]
async fn metrics_track_running_write_and_queue_depth() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), 1)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("metrics", 1).unwrap();

  // 初始无写：深度与在跑数均为 0。
  let idle = shard.metrics_snapshot();
  assert_eq!(idle.write_queue_depth, 0, "idle depth must be 0: {idle:?}");
  assert_eq!(idle.write_running, 0, "idle running must be 0: {idle:?}");

  let (watcher, stop) = watch_write_gauges(shard.metrics().clone());

  let mut handle_li = Vec::new();
  for index in 0..KEY_NUM {
    let sender = shard.sender();
    let tbl = tbl.clone();
    handle_li.push(spawn(async move { sender.request(put(tbl, index)).await }));
  }
  for handle in handle_li {
    handle.await.unwrap().unwrap();
  }
  stop.store(true, Ordering::Relaxed);
  let (max_running, max_depth) = watcher.join().unwrap();

  assert_eq!(
    max_running, 1,
    "serial work loop must run exactly one write at a time (eng-33a will lift this)"
  );
  // 回执先于指标递减发出：等观测值归零，避免把收尾窗口当成泄漏。
  for _ in 0..100 {
    let snap = shard.metrics_snapshot();
    if snap.write_running == 0 && snap.write_queue_depth == 0 {
      break;
    }
    thread::sleep(Duration::from_millis(1));
  }
  assert_eq!(shard.metrics_snapshot().write_running, 0);
  assert_eq!(shard.metrics_snapshot().write_queue_depth, 0);
  assert!(max_depth <= u64::from(KEY_NUM));
  shard.shutdown().await.unwrap();
  let _ = remove_dir_all(root);
}
