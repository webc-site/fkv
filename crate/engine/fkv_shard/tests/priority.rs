//! 高/低优先调度（eng-34）：
//! 1. 后台维护请求积压时，用户读不被队头阻塞；
//! 2. 只有后台请求时，shard 仍能被唤醒并执行（双通道等待不丢低优先）。
use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  },
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use compio::{runtime::spawn, time::sleep};
use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_types::{StoreMode, TableIdent};

/// 积压规模：远大于 shard 单轮能跑完的后台请求数，保证读请求到达时仍有积压。
const LOW_NUM: usize = 4096;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_priority_{}_{}_{}",
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

fn config(root: PathBuf, max_processing_time_us: u64) -> ShardConfig {
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
    queue_size: LOW_NUM * 2,
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
    max_write_concurrency: 0,
    max_processing_time_us,
    yield_budget_us: 0,
  }
}

fn key(index: u32) -> Vec<u8> {
  format!("key-{index:03}").into_bytes()
}

fn val(index: u32) -> Vec<u8> {
  format!("value-{index:03}").into_bytes()
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

fn gc(tbl: &TableIdent) -> Request {
  Request::FileGc { tbl: tbl.clone() }
}

/// 后台 Gc 积压时，前台 Get 先于大多数积压完成：
/// 没有优先级时 Get 会排在 4096 条 Gc 之后。
#[compio::test]
async fn get_overtakes_queued_background_tasks() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), 0)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("priority", 1).unwrap();
  shard
    .request(Request::Put {
      tbl: tbl.clone(),
      batch_li: vec![BatchEntry::upsert(key(0), val(0), 1)],
      term: Some(1),
    })
    .await
    .unwrap();

  let done = Arc::new(AtomicUsize::new(0));
  let mut task_li = Vec::with_capacity(LOW_NUM);
  for _ in 0..LOW_NUM {
    let sender = shard.sender();
    let tbl = tbl.clone();
    let done = done.clone();
    task_li.push(spawn(async move {
      let result = sender.request(gc(&tbl)).await;
      done.fetch_add(1, Ordering::Relaxed);
      result
    }));
  }
  // 让后台请求先进入队列，形成真实积压。
  sleep(Duration::from_millis(8)).await;

  let get = shard.request(Request::Get { tbl, key: key(0) }).await;
  match get.unwrap() {
    Response::Get(Some(got)) => assert_eq!(got, val(0)),
    other => panic!("unexpected response: {other:?}"),
  }
  let done_at_get = done.load(Ordering::Relaxed);
  assert!(
    done_at_get < LOW_NUM / 2,
    "Get 完成时后台已跑 {done_at_get}/{LOW_NUM} 条，高优先未插队"
  );

  for task in task_li {
    task.await.unwrap().unwrap();
  }
  shard.shutdown().await.unwrap();
  remove_dir_all(root).ok();
}

/// 只有低优先请求时 shard 仍能唤醒执行（阻塞等待同时看两条通道）。
#[compio::test]
async fn background_only_request_is_served() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), 0)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("background", 1).unwrap();
  shard
    .request(Request::Put {
      tbl: tbl.clone(),
      batch_li: vec![BatchEntry::upsert(key(0), val(0), 1)],
      term: Some(1),
    })
    .await
    .unwrap();

  shard.request(gc(&tbl)).await.unwrap();
  shard.shutdown().await.unwrap();
  remove_dir_all(root).ok();
}

/// eng-34c：调度面三个指标真的在动 —— 时间片让路、超预算轮数、低优先深度。
///
/// 用 1µs 时间片：只要主循环连续服务高优先超过 1µs 且低优先通道里有货，
/// 就必须走「让一条低优先」分支。后台先积压 `LOW_NUM` 条 Gc，前台持续 Get，
/// 保证观测窗口内低优先通道非空。
#[compio::test]
async fn scheduler_metrics_track_yield_budget_and_low_pq_depth() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), 1)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("sched_metrics", 1).unwrap();
  shard
    .request(Request::Put {
      tbl: tbl.clone(),
      batch_li: vec![BatchEntry::upsert(key(0), val(0), 1)],
      term: Some(1),
    })
    .await
    .unwrap();

  // 基线：1µs 时间片下 attach / put 本身就会让「超预算」计数动起来，所以
  // 只能断言增量；深度是 gauge，此刻确实应为 0。
  let base = shard.metrics_snapshot();
  assert_eq!(
    base.low_pq_depth, 0,
    "还没有低优先请求，深度应为 0：{base:?}"
  );

  // 后台积压：保证前台 Get 期间低优先通道里始终有货。
  let mut low_task_li = Vec::with_capacity(LOW_NUM);
  for _ in 0..LOW_NUM {
    let sender = shard.sender();
    let tbl = tbl.clone();
    low_task_li.push(spawn(async move { sender.request(gc(&tbl)).await }));
  }

  // 前台读持续抢占；顺便采样深度 gauge（入队侧 +1、shard 取走侧 -1）。
  let mut depth_observed = false;
  for _ in 0..64 {
    shard
      .request(Request::Get {
        tbl: tbl.clone(),
        key: key(0),
      })
      .await
      .unwrap();
    depth_observed |= shard.metrics_snapshot().low_pq_depth > 0;
  }
  assert!(
    depth_observed,
    "低优先通道深度从未观测到 > 0：{:?}",
    shard.metrics_snapshot()
  );

  let snapshot = shard.metrics_snapshot();
  assert!(
    snapshot.round_over_budget_total > base.round_over_budget_total,
    "时间片超预算的轮数没有增长（基线 {}）：{snapshot:?}",
    base.round_over_budget_total
  );
  assert!(
    snapshot.yield_total > base.yield_total,
    "低优先从未被放行（时间片让路分支没走，基线 {}）：{snapshot:?}",
    base.yield_total
  );

  for task in low_task_li {
    task.await.unwrap().unwrap();
  }
  let snapshot = shard.metrics_snapshot();
  assert_eq!(
    snapshot.low_pq_depth, 0,
    "低优先全部完成后深度必须回到 0：{snapshot:?}"
  );
  shard.shutdown().await.unwrap();
  remove_dir_all(root).ok();
}
