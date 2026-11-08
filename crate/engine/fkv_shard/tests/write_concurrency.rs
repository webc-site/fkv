//! eng-33a 验收：跨分区写真正并发、同分区严格 FIFO、读不被其它分区写阻塞。
//!
//! 并发性用 `ShardMetric::write_running`（写门在跑数）由独立线程高频采样：
//! 派发时 gauge 立即 +1，任务完成回收入口才 -1；两个分区被同时派发时
//! 采样必然能观测到 2，而 `max_write_concurrency = 1` 时永不超过 1。
use std::{
  hint::spin_loop,
  path::PathBuf,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
  },
  thread,
};

use compio::runtime::spawn;
use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_shard::{Shard, ShardConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;
use futures::future::{join, join_all};

/// 两个分区各写这么多个唯一 key：足够多页 IO，保证两个写任务在采样窗口内
/// 都在飞；同时控制测试时长。
const BATCH_ENTRIES: u32 = 4_000;
/// 同分区 FIFO 的请求数，必须小于单轮 group commit 上限（64）。
const FIFO_REQUESTS: u32 = 32;

fn config(root: PathBuf, max_write_concurrency: usize) -> ShardConfig {
  let mut cfg = ShardConfig::for_tests(root);
  cfg.queue_size = 256;
  cfg.max_write_batch_pages = 1;
  cfg.max_inflight_write = 32;
  cfg.max_write_concurrency = max_write_concurrency;
  cfg
}

fn key(index: u32) -> Vec<u8> {
  format!("key-{index:06}").into_bytes()
}

fn value(_index: u32) -> Vec<u8> {
  // 每个 value 约 360B：512B 页下基本一 entry 一页，写路径产生大量 IO。
  vec![b'v'; 360]
}

fn big_put(tbl: TableIdent, round: u32) -> Request {
  let batch_li = (0..BATCH_ENTRIES)
    .map(|index| {
      BatchEntry::upsert(
        key(round.wrapping_mul(BATCH_ENTRIES) + index),
        value(index),
        1,
      )
    })
    .collect();
  Request::Put {
    tbl,
    batch_li,
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

/// 独立线程高频采样 `write_running` 峰值（与 write_gate 测试同口径）。
fn watch_running(
  metrics: Arc<fkv_metrics::ShardMetric>,
) -> (thread::JoinHandle<u64>, Arc<AtomicBool>) {
  let stop = Arc::new(AtomicBool::new(false));
  let stop_thread = stop.clone();
  let handle = thread::spawn(move || {
    let mut max_running = 0u64;
    while !stop_thread.load(Ordering::Relaxed) {
      max_running = max_running.max(metrics.snapshot().write_running);
      spin_loop();
    }
    max_running
  });
  (handle, stop)
}

/// 两个分区各一条慢写：派发窗口内 `write_running` 达到 2（真并发）。
#[compio::test]
async fn cross_partition_writes_overlap() {
  let root = TempRoot::new("write_concurrency");
  let shard = Shard::start(config(root.path().to_path_buf(), 0)).unwrap();
  attach(&shard).await;
  let tbl_a = TableIdent::new("overlap_a", 1).unwrap();
  let tbl_b = TableIdent::new("overlap_b", 1).unwrap();

  let (watcher, stop) = watch_running(shard.metrics().clone());
  let sender_a = shard.sender();
  let put_a = {
    let tbl = tbl_a.clone();
    spawn(async move { sender_a.request(big_put(tbl, 0)).await })
  };
  let sender_b = shard.sender();
  let put_b = {
    let tbl = tbl_b.clone();
    spawn(async move { sender_b.request(big_put(tbl, 1)).await })
  };
  put_a.await.unwrap().unwrap();
  put_b.await.unwrap().unwrap();
  stop.store(true, Ordering::Relaxed);
  let max_running = watcher.join().unwrap();

  assert!(
    max_running >= 2,
    "two partitions must be dispatched concurrently, max write_running={max_running}"
  );
  // 两个分区的数据都要落地，且互不覆盖。
  for (tbl, round) in [(tbl_a, 0u32), (tbl_b, 1u32)] {
    let key = key(round * BATCH_ENTRIES);
    match shard
      .request(Request::Get {
        tbl,
        key: key.clone(),
      })
      .await
      .unwrap()
    {
      Response::Get(Some(got)) => assert_eq!(got, value(0)),
      other => panic!("unexpected response: {other:?}"),
    }
  }
  assert_eq!(shard.metrics_snapshot().write_running, 0);
  shard.shutdown().await.unwrap();
}

/// `max_write_concurrency = 1`：跨分区也不重叠，峰值在跑写恒为 1。
#[compio::test]
async fn max_write_concurrency_one_serializes_partitions() {
  let root = TempRoot::new("write_concurrency");
  let shard = Shard::start(config(root.path().to_path_buf(), 1)).unwrap();
  attach(&shard).await;
  let tbl_a = TableIdent::new("serial_a", 1).unwrap();
  let tbl_b = TableIdent::new("serial_b", 1).unwrap();

  let (watcher, stop) = watch_running(shard.metrics().clone());
  let sender_a = shard.sender();
  let put_a = {
    let tbl = tbl_a.clone();
    spawn(async move { sender_a.request(big_put(tbl, 0)).await })
  };
  let sender_b = shard.sender();
  let put_b = {
    let tbl = tbl_b.clone();
    spawn(async move { sender_b.request(big_put(tbl, 1)).await })
  };
  put_a.await.unwrap().unwrap();
  put_b.await.unwrap().unwrap();
  stop.store(true, Ordering::Relaxed);
  let max_running = watcher.join().unwrap();

  assert_eq!(
    max_running, 1,
    "max_write_concurrency=1 must never run two partition writes at once"
  );
  shard.shutdown().await.unwrap();
}

/// 同分区 N 条写按入队顺序完成（回执顺序 = 完成顺序）。
#[compio::test]
async fn same_partition_writes_complete_in_fifo_order() {
  let root = TempRoot::new("write_concurrency");
  let shard = Shard::start(config(root.path().to_path_buf(), 0)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("fifo_order", 1).unwrap();
  let done_li = Arc::new(Mutex::new(Vec::new()));

  let sender = shard.sender();
  let mut fut_li = Vec::new();
  for seq in 0..FIFO_REQUESTS {
    let req = Request::Put {
      tbl: tbl.clone(),
      batch_li: vec![BatchEntry::upsert(key(seq), value(seq), u64::from(seq) + 1)],
      term: Some(1),
    };
    let sender = sender.clone();
    let done_li = done_li.clone();
    fut_li.push(async move {
      sender.request(req).await.unwrap();
      done_li.lock().unwrap().push(seq);
    });
  }
  join_all(fut_li).await;

  let done = done_li.lock().unwrap().clone();
  assert_eq!(
    done,
    (0..FIFO_REQUESTS).collect::<Vec<_>>(),
    "same partition writes must reply in enqueue order"
  );
  shard.shutdown().await.unwrap();
}

/// 一个分区的大写进行中，另一个分区的读仍能完成（读不被写完全阻塞）。
#[compio::test]
async fn read_on_other_partition_progresses_during_slow_write() {
  let root = TempRoot::new("write_concurrency");
  let shard = Shard::start(config(root.path().to_path_buf(), 0)).unwrap();
  attach(&shard).await;
  let tbl_write = TableIdent::new("slow_write", 1).unwrap();
  let tbl_read = TableIdent::new("fast_read", 1).unwrap();

  let order_li = Arc::new(Mutex::new(Vec::new()));
  let sender_put = shard.sender();
  let put = {
    let tbl = tbl_write.clone();
    let order_li = order_li.clone();
    async move {
      sender_put.request(big_put(tbl, 0)).await.unwrap();
      order_li.lock().unwrap().push("put");
    }
  };
  let sender_get = shard.sender();
  let get = {
    let tbl = tbl_read.clone();
    let order_li = order_li.clone();
    async move {
      let response = sender_get
        .request(Request::Get { tbl, key: key(0) })
        .await
        .unwrap();
      assert!(matches!(response, Response::Get(None)));
      order_li.lock().unwrap().push("get");
    }
  };
  join(put, get).await;

  let order = order_li.lock().unwrap().clone();
  assert_eq!(
    order,
    vec!["get", "put"],
    "read on another partition must finish before the slow write"
  );
  shard.shutdown().await.unwrap();
}
