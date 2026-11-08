//! io-14：等大值读单飞名额的 GetLarge 不能把读窗口钉住。
//!
//! `large_read_out` 由一条在飞 GetLarge 独占（io-05 的 fixed buffer / 内存峰值
//! 约束）。第二条 GetLarge 在窗口里只能暂存；旧实现预算到点进入 `retiring`
//! 后仍要「把暂存读准入完才退出」，而单飞名额只有首条读完成才会释放——窗口
//! 因此驻留到首条大值读全程，期间的写派发 / 写完成回收 / 低优先出队 / reopen
//! 到期全部停摆（io-08 的「窗口常驻有界」不成立）。
//!
//! 本用例用一条大 value 的 overflow 链（约 1.6 万页 IO，`yield_budget_us=1`
//! 放大每次批读的调度间隔）构造长 GetLarge：GL1 在飞后发 GL2，再发一条不同
//! 分区的 Put 与一条低优先 FileGc。断言 Put / FileGc 的回执都早于 GL1 完成
//! （调用方可见顺序 + shard 侧 Get 完成计数双保险）。旧实现下窗口卡在
//! 「等 GL1 完成」，前两者只能在 GL1 结束后才被主循环看到。
use std::{
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  time::{Duration, Instant},
};

use compio::{runtime::spawn, time::sleep};
use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_metrics::Op;
use fkv_shard::{Shard, ShardConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

/// 慢读载体：远超单页的 overflow 链（512B 页，约 1.6 万个 payload 页）。
///
/// 不走 segment 大值存储（`large_value_threshold` 配得更高）：段 id 列表要
/// 塞进一页，值大小受页容量限制；overflow 链能稳定做到几十毫秒以上的读，
/// 远大于一次小 Put（~ms）的完成时间——否则机器空闲时 GL1 可能先于 Put
/// 结束，完成顺序断言就失去意义。
const SLOW_VALUE_LEN: usize = 8 * 1024 * 1024;

fn config(root: PathBuf) -> ShardConfig {
  let mut cfg = ShardConfig::for_tests(root);
  cfg.large_value_threshold = 16 * 1024 * 1024;
  cfg.queue_size = 1024;
  cfg.max_processing_time_us = 1;
  cfg.yield_budget_us = 1;
  cfg
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

fn get_large(tbl: &TableIdent, key: &[u8]) -> Request {
  Request::GetLarge {
    tbl: tbl.clone(),
    key: key.to_vec(),
  }
}

/// 已完成回执的 Get / GetLarge 条数（shard 侧完成记录，供完成顺序断言）。
///
/// 比调用方的 `done` 标志更贴近 shard 事实：`finish_read` 先记指标再发回执，
/// 调用方标志要等跨线程唤醒后才置位。
fn completed_gets(shard: &Shard) -> u64 {
  shard
    .metrics_snapshot()
    .op_li
    .iter()
    .find(|op| op.op == Op::Get)
    .map_or(0, |op| op.total)
}

/// 等条件成立；超时返回 `false`。
async fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
  let started = Instant::now();
  while !cond() {
    if started.elapsed() >= timeout {
      return false;
    }
    sleep(Duration::from_micros(200)).await;
  }
  true
}

/// GL1 在飞 → GL2 暂存（旧实现把窗口钉住）→ Put / FileGc 必须先在 GL1 前完成。
#[compio::test]
async fn put_and_low_priority_progress_while_get_large_waits() {
  let root = TempRoot::new("io14-getlarge-window");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  attach(&shard).await;

  let big_tbl = TableIdent::new("big", 1).unwrap();
  let put_tbl = TableIdent::new("put", 2).unwrap();
  let gc_tbl = TableIdent::new("gc", 3).unwrap();
  // 三个分区各铺一条数据；big 的值构成慢读链。
  shard
    .request(put(
      &big_tbl,
      vec![BatchEntry::upsert(
        b"big".to_vec(),
        vec![b'B'; SLOW_VALUE_LEN],
        1,
      )],
    ))
    .await
    .unwrap();
  for tbl in [&put_tbl, &gc_tbl] {
    shard
      .request(put(
        tbl,
        vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      ))
      .await
      .unwrap();
  }

  // GL1：长读数在飞，随后所有等待都必须让开。
  let gl1_done = Arc::new(AtomicBool::new(false));
  let gl1 = {
    let sender = shard.sender();
    let tbl = big_tbl.clone();
    let done = gl1_done.clone();
    spawn(async move {
      let result = sender.request(get_large(&tbl, b"big")).await;
      done.store(true, Ordering::SeqCst);
      result
    })
  };
  assert!(
    wait_until(Duration::from_secs(5), || {
      shard.metrics_snapshot().read_in_flight > 0
    })
    .await,
    "GL1 没有进入在飞读（场景无效）"
  );

  // GL2：单飞名额被 GL1 占着，只能在窗口里等。
  let gl2_done = Arc::new(AtomicBool::new(false));
  let gl2 = {
    let sender = shard.sender();
    let tbl = big_tbl.clone();
    let done = gl2_done.clone();
    spawn(async move {
      let result = sender.request(get_large(&tbl, b"big")).await;
      done.store(true, Ordering::SeqCst);
      result
    })
  };

  // 不同分区的 Put：旧实现下窗口钉在 GL1 上，它只能在 GL1 完成后才被看到。
  let put_done = Arc::new(AtomicBool::new(false));
  let put_task = {
    let sender = shard.sender();
    let tbl = put_tbl.clone();
    let done = put_done.clone();
    spawn(async move {
      let result = sender
        .request(put(
          &tbl,
          vec![BatchEntry::upsert(b"k2".to_vec(), b"v2".to_vec(), 2)],
        ))
        .await;
      done.store(true, Ordering::SeqCst);
      result
    })
  };
  assert!(
    wait_until(Duration::from_secs(5), || put_done.load(Ordering::SeqCst)).await,
    "GL1 在飞期间 Put 没有完成（读窗口被大值单飞钉住）"
  );
  // 旧实现下 Put 也能完成，但只会在 GL1 结束后才被主循环看到（窗口卡在
  // `flight.next().await`）。**回执完成顺序**是本用例的真正断言；不用
  // `read_in_flight` 判据：`finish_read` 先减 gauge 再发回执，两者之间有
  // 一个极窄的交接窗口，采样会误判成「GL1 已完成」。
  assert!(
    !gl1_done.load(Ordering::SeqCst),
    "Put 未能在 GL1 完成前回执（读窗口被大值单飞钉住）"
  );
  // 双保险：shard 侧 GL1 尚未记录完成（完成顺序的权威来源，不受回执投递
  // 延迟影响）。
  assert_eq!(
    completed_gets(&shard),
    0,
    "Put 完成时 GL1 已在 shard 侧完成（窗口没有让 Put 先推进）"
  );

  // 低优先 FileGc：窗口驻留期间主循环到不了，低优先通道不被消费。
  let gc_done = Arc::new(AtomicBool::new(false));
  let gc_task = {
    let sender = shard.sender();
    let tbl = gc_tbl.clone();
    let done = gc_done.clone();
    spawn(async move {
      let result = sender.request(Request::FileGc { tbl }).await;
      done.store(true, Ordering::SeqCst);
      result
    })
  };
  // 低优先通道不在读窗口的等待集里：只有窗口按预算超时退出回主循环，
  // FileGc 才会被出队。旧实现下它会等到 GL1 结束。
  assert!(
    wait_until(Duration::from_secs(5), || gc_done.load(Ordering::SeqCst)).await,
    "GL1 在飞期间低优先 FileGc 没有被出队执行"
  );
  assert!(
    !gl1_done.load(Ordering::SeqCst),
    "FileGc 未能在 GL1 完成前回执（窗口没有按预算退出）"
  );
  assert_eq!(
    completed_gets(&shard),
    0,
    "FileGc 完成时 GL1 已在 shard 侧完成（窗口没有按预算退出）"
  );

  put_task.await.unwrap().unwrap();
  gc_task.await.unwrap().unwrap();

  assert!(
    wait_until(Duration::from_secs(5), || gl2_done.load(Ordering::SeqCst)).await,
    "GL1 已结束但 GL2 仍未完成（大值等待队列没有重排）：{:?}",
    shard.metrics_snapshot()
  );

  // 两条大值读都拿到完整值，且单飞语义未破：期间任何时刻最多一条 GetLarge
  // 在飞（本用例没有其它读）。
  for handle in [gl1, gl2] {
    match handle.await.unwrap().unwrap() {
      Response::GetLarge(Some(large)) => {
        assert_eq!(large.value.len(), SLOW_VALUE_LEN);
        assert!(large.value.iter().all(|byte| *byte == b'B'));
      }
      other => panic!("unexpected get_large response: {other:?}"),
    }
  }

  let snapshot = shard.metrics_snapshot();
  assert_eq!(
    snapshot.read_in_flight, 0,
    "读完成后 gauge 必须归零：{snapshot:?}"
  );
  assert_eq!(
    snapshot.read_in_flight_max, 1,
    "GetLarge 单飞被破坏（同时有 {} 条在飞读）：{snapshot:?}",
    snapshot.read_in_flight_max
  );
  assert_eq!(
    snapshot.low_pq_depth, 0,
    "低优先请求完成后深度必须归零：{snapshot:?}"
  );

  shard.shutdown().await.unwrap();
}
