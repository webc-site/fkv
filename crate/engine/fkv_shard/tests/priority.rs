//! 高/低优先调度（eng-34）：
//! 1. 后台维护请求积压时，用户读不被队头阻塞；
//! 2. 只有后台请求时，shard 仍能被唤醒并执行（双通道等待不丢低优先）。
use std::{
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  },
  time::{Duration, Instant},
};

use compio::{runtime::spawn, time::sleep};
use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_shard::{Shard, ShardConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

/// 积压规模：远大于 shard 单轮能跑完的后台请求数，保证读请求到达时仍有积压。
const LOW_NUM: usize = 4096;

/// 等待后台请求全部入队 / 前台 Get 完成的上限。
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);

fn config(root: PathBuf, max_processing_time_us: u64) -> ShardConfig {
  let mut cfg = ShardConfig::for_tests(root);
  cfg.queue_size = LOW_NUM * 2;
  cfg.max_processing_time_us = max_processing_time_us;
  cfg
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

/// 轮询指标直到条件成立或超时。
///
/// `spawn` 只把任务排进 compio 执行器，不保证已执行到 `sender.request` 的
/// 入队点；等后台请求真正入队必须看单调计数，不能采样会被 shard 取走侧
/// 清零的瞬时深度 gauge。
async fn wait_until(mut pred: impl FnMut() -> bool, timeout: Duration) -> bool {
  let started = Instant::now();
  loop {
    if pred() {
      return true;
    }
    if started.elapsed() >= timeout {
      return false;
    }
    sleep(Duration::from_millis(1)).await;
  }
}

/// 后台 Gc 积压时，前台 Get 先于大多数积压完成：
/// 没有优先级时 Get 会排在 4096 条 Gc 之后。
#[compio::test]
async fn get_overtakes_queued_background_tasks() {
  let root = TempRoot::new("priority");
  let shard = Shard::start(config(root.path().to_path_buf(), 0)).unwrap();
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
}

/// 只有低优先请求时 shard 仍能唤醒执行（阻塞等待同时看两条通道）。
#[compio::test]
async fn background_only_request_is_served() {
  let root = TempRoot::new("priority");
  let shard = Shard::start(config(root.path().to_path_buf(), 0)).unwrap();
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
}

/// eng-34c：调度面三个指标真的在动 —— 时间片让路、超预算轮数、低优先积压。
///
/// 用 1µs 时间片：只要主循环连续服务高优先超过 1µs 且低优先通道里有货，
/// 就必须走「让一条低优先」分支。后台先积压 `LOW_NUM` 条 Gc，等它们全部
/// 进入低优先队列后再持续 Get，保证前台服务期间低优先确实有货；入队累计与
/// 深度峰值都是单调计数，不受外部采样时机影响。
#[compio::test]
async fn scheduler_metrics_track_yield_budget_and_low_pq_depth() {
  let root = TempRoot::new("priority");
  let shard = Shard::start(config(root.path().to_path_buf(), 1)).unwrap();
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
  // 只能断言增量；此刻还没有低优先请求，深度与两项单调计数都应为 0。
  let base = shard.metrics_snapshot();
  assert_eq!(
    base.low_pq_depth, 0,
    "还没有低优先请求，深度应为 0：{base:?}"
  );
  assert_eq!(
    base.low_pq_enqueued_total, 0,
    "还没有低优先请求，入队计数应为 0：{base:?}"
  );
  assert_eq!(
    base.low_pq_depth_max, 0,
    "还没有低优先请求，深度峰值应为 0：{base:?}"
  );

  // 后台积压：保证前台 Get 期间低优先通道里始终有货。
  let mut low_task_li = Vec::with_capacity(LOW_NUM);
  for _ in 0..LOW_NUM {
    let sender = shard.sender();
    let tbl = tbl.clone();
    low_task_li.push(spawn(async move { sender.request(gc(&tbl)).await }));
  }

  // spawn 只把任务排进执行器，不保证已执行到入队点：先等单调入队计数覆盖
  // 全部后台请求，再开始前台读，构造确定性的「低优先积压 + 高优先抢占」。
  assert!(
    wait_until(
      || {
        shard
          .metrics_snapshot()
          .low_pq_enqueued_total
          .saturating_sub(base.low_pq_enqueued_total)
          >= LOW_NUM as u64
      },
      WAIT_TIMEOUT,
    )
    .await,
    "后台请求没有全部进入低优先队列：{:?}",
    shard.metrics_snapshot()
  );
  let backlog = shard.metrics_snapshot();
  // 深度峰值由入队侧更新，单调递增：`> 1` 证明至少两条低优先请求同时排队，
  // 即确实形成过积压（而非「入队即被取走」的单条流水），与两个 Get 之间
  // 恰好采样到非零的瞬时窗口无关。
  assert!(
    backlog.low_pq_depth_max > 1,
    "低优先队列从未形成超过 1 条的积压（峰值 {}）：{backlog:?}",
    backlog.low_pq_depth_max
  );

  for _ in 0..64 {
    shard
      .request(Request::Get {
        tbl: tbl.clone(),
        key: key(0),
      })
      .await
      .unwrap();
  }

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
  // 全部 `LOW_NUM` 条低优先请求确实进过队列（单调计数，不受取出时机影响）。
  assert!(
    snapshot
      .low_pq_enqueued_total
      .saturating_sub(base.low_pq_enqueued_total)
      >= LOW_NUM as u64,
    "低优先入队计数少于派发数（基线 {}）：{snapshot:?}",
    base.low_pq_enqueued_total
  );

  for task in low_task_li {
    task.await.unwrap().unwrap();
  }
  let snapshot = shard.metrics_snapshot();
  assert_eq!(
    snapshot.low_pq_depth, 0,
    "低优先全部完成后深度必须回到 0：{snapshot:?}"
  );
  assert!(
    snapshot.low_pq_depth_max >= backlog.low_pq_depth_max,
    "深度峰值必须单调不回退：{snapshot:?}"
  );
  shard.shutdown().await.unwrap();
}
