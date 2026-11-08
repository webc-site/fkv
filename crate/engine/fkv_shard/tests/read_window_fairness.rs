//! io-08：读窗口常驻预算下的调度公平性。
//!
//! 持续高优先读会让读窗口一直有在飞读（io-04 的滑动窗口不再因满 32 条退出）。
//! 若窗口没有自己的时间预算，主循环顶部的低优先出队 / reopen 到期重试 / 写
//! 回收都会停摆，直到读流量停止。这里用持续 Get 压住窗口，断言：
//! 1. 低优先 `FileGc` 在读负载仍在推进时被出队并执行；
//! 2. 已到期的 reopen 挂起读在读负载仍在推进时被重试并拿到迟到的 manifest；
//! 3. 读回执恰好一次、`read_in_flight_max` 不越过在飞上限。
//!
//! 等待条件全部是**状态事件**（回执完成 / 单调计数增长），`WATCHDOG` 只用于
//! 让「完全不推进」的实现有界失败，不作为时延 / 性能断言。
use std::{
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
  },
  time::{Duration, Instant},
};

use compio::{
  runtime::{JoinHandle, spawn},
  time::sleep,
};
use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_shard::{Shard, ShardConfig};
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::{StoreMode, TableIdent};

/// 读压测 worker 数：每个 worker 同一时刻只有一条在飞读，因此在飞读上界
/// 就是本值（落在 io-08 验收要求的 32~64）。
const READ_WORKERS: u64 = 64;

/// 热表键数：每个 worker 轮转取键，避免所有读同时完成。
const HOT_KEYS: u64 = 64;

/// 挂死看门狗：实现完全不推进时让测试有界失败。
///
/// 不是性能断言——正常路径由状态事件（回执 / 单调计数）立即返回；只有窗口
/// 常驻不退、低优先永远轮不到这类回归才会走满这个时长。
const WATCHDOG: Duration = Duration::from_secs(30);

fn config(root: PathBuf, mode: StoreMode, retry_times: u8, pending_us: u64) -> ShardConfig {
  let mut cfg = ShardConfig::for_tests(root);
  cfg.large_value_threshold = 1 << 20;
  cfg.queue_size = 1024;
  cfg.auto_reopen_retry_times = retry_times;
  cfg.auto_reopen_pending_time_us = pending_us;
  cfg.overflow_pointers = 1;
  cfg.mode = mode;
  cfg.max_processing_time_us = 100;
  cfg.yield_budget_us = 1;
  cfg
}

fn key(index: u64) -> Vec<u8> {
  format!("key-{index:04}").into_bytes()
}

fn slow_val(index: u64) -> Vec<u8> {
  vec![(index % 251) as u8; 8192]
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

fn put(tbl: &TableIdent, entries: Vec<BatchEntry>) -> Request {
  Request::Put {
    tbl: tbl.clone(),
    batch_li: entries,
    term: Some(1),
  }
}

fn get(tbl: &TableIdent, index: u64) -> Request {
  Request::Get {
    tbl: tbl.clone(),
    key: key(index),
  }
}

/// 等异步负载进入目标状态；不用固定 sleep，CI 慢机也不会误判。
///
/// `timeout` 只作挂死看门狗：调用点传 [`WATCHDOG`]，正常路径由状态事件
/// 立即满足，不依赖机器速度。
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

/// 持续读负载：`READ_WORKERS` 个 worker 各自串行发 Get，`stop` 置位后退出。
///
/// 返回值是每条读的期望值（worker 用它校验恰好的回执内容）。
fn spawn_read_load(
  shard: &Shard,
  tbl: TableIdent,
  stop: Arc<AtomicBool>,
  served: Arc<AtomicU64>,
) -> Vec<JoinHandle<()>> {
  (0..READ_WORKERS)
    .map(|worker| {
      let sender = shard.sender();
      let tbl = tbl.clone();
      let stop = stop.clone();
      let served = served.clone();
      spawn(async move {
        let mut round = 0u64;
        while !stop.load(Ordering::Relaxed) {
          let index = (worker + round * READ_WORKERS) % HOT_KEYS;
          round += 1;
          match sender.request(get(&tbl, index)).await {
            Ok(Response::Get(Some(value))) if value == slow_val(index) => {
              served.fetch_add(1, Ordering::Relaxed);
            }
            other => panic!("read worker {worker} unexpected response: {other:?}"),
          }
        }
      })
    })
    .collect()
}

/// 低优先通道在持续读下不被饿死：`FileGc`（低优先写）必须在读流量停止前
/// 被主循环取出并执行完成。
#[compio::test]
async fn low_priority_write_runs_while_reads_are_sustained() {
  let root = TempRoot::new("read_window_fairness");
  let shard = Shard::start(config(root.path().to_path_buf(), StoreMode::Local, 0, 0)).unwrap();
  attach(&shard).await;

  let hot = TableIdent::new("hot", 1).unwrap();
  let gc_tbl = TableIdent::new("gc", 2).unwrap();
  let hot_batch = (0..HOT_KEYS)
    .map(|index| BatchEntry::upsert(key(index), slow_val(index), 1))
    .collect();
  shard.request(put(&hot, hot_batch)).await.unwrap();
  shard
    .request(put(
      &gc_tbl,
      vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
    ))
    .await
    .unwrap();

  let stop = Arc::new(AtomicBool::new(false));
  let served = Arc::new(AtomicU64::new(0));
  let load = spawn_read_load(&shard, hot, stop.clone(), served.clone());
  // 让读窗口进入稳定并发（≥ 32 条在飞）；正常路径一个读批内满足，慢机只受
  // 挂死看门狗约束。
  assert!(
    wait_until(|| shard.metrics_snapshot().read_in_flight >= 32, WATCHDOG).await,
    "持续读没有形成 32 条以上在飞：{:?}",
    shard.metrics_snapshot()
  );

  // 读负载必须先真正推进，否则后面的公平性结论没有意义。
  assert!(
    wait_until(|| served.load(Ordering::Relaxed) > 0, WATCHDOG).await,
    "读负载 30s 内没有任何回执：{:?}",
    shard.metrics_snapshot()
  );

  // 低优先 FileGc 从低优先通道入队；窗口不退出时主循环永远看不到它。
  let base = shard.metrics_snapshot();
  let base_served = served.load(Ordering::Relaxed);
  let gc_done = Arc::new(AtomicBool::new(false));
  let sender = shard.sender();
  let tbl = gc_tbl.clone();
  let done = gc_done.clone();
  let gc_task = spawn(async move {
    let result = sender.request(Request::FileGc { tbl }).await;
    done.store(true, Ordering::SeqCst);
    result
  });

  // 事件驱动：等 GC 的回执状态，而不是「2s 内必须完成」的墙钟断言。
  assert!(
    wait_until(|| gc_done.load(Ordering::SeqCst), WATCHDOG).await,
    "读负载持续期间低优先 FileGc 30s 内没有被调度（读窗口常驻未退出）：{:?}",
    shard.metrics_snapshot()
  );
  // 场景有效性：等待期间读负载仍在推进（不是读流量停止后 GC 才被跑到），
  // 且读窗口至少因时间片退出过一次（`yield_total` / `round_over_budget_total`
  // 是事件循环维护的单调计数，与机器速度无关）。
  let served_during = served.load(Ordering::Relaxed).saturating_sub(base_served);
  assert!(
    served_during > 0,
    "低优先请求等待期间读负载没有推进，场景无效：base={base_served} served={served_during}"
  );
  let after_gc = shard.metrics_snapshot();
  assert!(
    after_gc.yield_total > base.yield_total
      || after_gc.round_over_budget_total > base.round_over_budget_total,
    "等待低优先请求期间读窗口从未因时间片退出：base={base:?} now={after_gc:?}"
  );

  // 先停读负载，避免遗留任务影响后续断言与关停。
  stop.store(true, Ordering::Relaxed);
  for worker in load {
    worker.await.unwrap();
  }
  let gc_result = gc_task.await.unwrap();
  gc_result.unwrap();

  let snapshot = shard.metrics_snapshot();
  assert_eq!(
    snapshot.read_in_flight, 0,
    "读完成后 gauge 必须归零：{snapshot:?}"
  );
  assert_eq!(
    snapshot.low_pq_depth, 0,
    "低优先请求完成后深度必须归零：{snapshot:?}"
  );
  assert!(
    snapshot.read_in_flight_max >= 32,
    "持续读期间在飞峰值 {} 未达到并发场景要求",
    snapshot.read_in_flight_max
  );
  assert!(
    snapshot.read_in_flight_max <= READ_WORKERS,
    "在飞峰值 {} 超过 worker 数 {READ_WORKERS}",
    snapshot.read_in_flight_max
  );
  assert!(
    served.load(Ordering::Relaxed) >= READ_WORKERS,
    "读负载回执过少：{}",
    served.load(Ordering::Relaxed)
  );

  shard.shutdown().await.unwrap();
}

/// 已到期的 reopen 挂起读在持续读下被重试：manifest 在 `pending_time` 前落盘，
/// driver 到期时窗口必须已经退出过（预算到点），否则读请求要等读流量停止。
#[compio::test]
async fn due_reopen_retries_while_reads_are_sustained() {
  let root = TempRoot::new("read_window_fairness");
  // `pending_time` 放宽到 300ms：写 manifest 的 setup 延迟（20ms）与到期之间
  // 留出足够余量，慢机不会因「到期时 manifest 还没落盘」而误判。
  let shard = Shard::start(config(
    root.path().to_path_buf(),
    StoreMode::Cloud,
    1,
    300_000,
  ))
  .unwrap();
  attach(&shard).await;

  let hot = TableIdent::new("hot", 1).unwrap();
  let missing = TableIdent::new("late", 2).unwrap();
  let hot_batch = (0..HOT_KEYS)
    .map(|index| BatchEntry::upsert(key(index), slow_val(index), 1))
    .collect();
  shard.request(put(&hot, hot_batch)).await.unwrap();

  let stop = Arc::new(AtomicBool::new(false));
  let served = Arc::new(AtomicU64::new(0));
  let load = spawn_read_load(&shard, hot, stop.clone(), served.clone());
  assert!(
    wait_until(|| shard.metrics_snapshot().read_in_flight >= 32, WATCHDOG).await,
    "持续读没有形成 32 条以上在飞：{:?}",
    shard.metrics_snapshot()
  );

  let base = shard.metrics_snapshot();
  let base_served = served.load(Ordering::Relaxed);
  // 远端 manifest 在 driver 到期（300ms）之前落盘。
  let writer = spawn({
    let root = root.path().to_path_buf();
    let tbl = missing.clone();
    async move {
      sleep(Duration::from_millis(20)).await;
      let mut store = Store::new(
        CompioStore::new(root, 512, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT),
        512,
        16,
      );
      store
        .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
        .await
        .unwrap();
    }
  });

  let reopened = Arc::new(AtomicBool::new(false));
  let done = reopened.clone();
  let sender = shard.sender();
  let tbl = missing.clone();
  let read_task = spawn(async move {
    let result = sender
      .request(Request::Get {
        tbl,
        key: b"a".to_vec(),
      })
      .await;
    done.store(true, Ordering::SeqCst);
    result
  });

  // 事件驱动：等挂起读拿到回执，而不是「800ms 内完成」的墙钟断言。
  assert!(
    wait_until(|| reopened.load(Ordering::SeqCst), WATCHDOG).await,
    "持续读期间到期的 reopen 30s 内没有被重试（读窗口常驻未退出）：{:?}",
    shard.metrics_snapshot()
  );
  // 场景有效性：等待期间读负载仍在推进（不是读流量停止后 reopen 才被跑到），
  // 且读窗口至少因时间片退出过一次（单调事件计数，与机器速度无关）。
  let served_during = served.load(Ordering::Relaxed).saturating_sub(base_served);
  assert!(
    served_during > 0,
    "等待 reopen 重试期间读负载没有推进，场景无效：base={base_served} served={served_during}"
  );
  let after_wait = shard.metrics_snapshot();
  assert!(
    after_wait.yield_total > base.yield_total
      || after_wait.round_over_budget_total > base.round_over_budget_total,
    "等待 reopen 重试期间读窗口从未因时间片退出：base={base:?} now={after_wait:?}"
  );

  stop.store(true, Ordering::Relaxed);
  for worker in load {
    worker.await.unwrap();
  }
  writer.await.unwrap();
  let read_result = read_task.await.unwrap();

  match read_result.unwrap() {
    Response::Get(Some(value)) => assert_eq!(value, b"A"),
    other => panic!("unexpected reopen read response: {other:?}"),
  }

  let snapshot = shard.metrics_snapshot();
  assert_eq!(
    snapshot.read_in_flight, 0,
    "读完成后 gauge 必须归零：{snapshot:?}"
  );
  assert!(
    snapshot.read_in_flight_max >= 32,
    "持续读期间在飞峰值 {} 未达到并发场景要求",
    snapshot.read_in_flight_max
  );

  shard.shutdown().await.unwrap();
}
