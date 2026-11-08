//! 低优先通道深度 gauge 的回滚验收。
//!
//! `low_pq_depth` 在入队侧先 +1，投递成功交给消费侧 -1；投递失败或 future 被
//! drop 必须由 `LowPqGuard` 回滚。否则关停 / 取消后 gauge 会残留非零，调度
//! 指标永久失真。
//!
//! # test-24 / io-32：已知的构造性风险（非本用例 / 引擎回归）
//!
//! [`dropped_pending_low_request_rolls_back_depth`] 刻意构造「满通道 + 大量真实
//! 等待发送者 + 一个以 noop waker poll 一次即 drop 的发送者」。极端负载改变
//! 单次 poll 与 drop 之间的时序后，会出现一个真实等待发送者被永久滞留的形态
//! （双侧 0% CPU 真挂起；第 33 轮 load 6~12.7 实测间歇，0b87909 同形复现 ⇒
//! 跨提交既有，**非本用例或引擎回归**；根因已由 io-32 定案并修复：事件循环 park 判据
//! 忽略写门（见 io-32 结论）。候选机理（**已被 io-32 根因取代**，见
//! `issue/new/io-32-cancelled-low-send-strands-sibling-waiter.md`）：crossfire
//! 3.1 对「已注册 waker 先被唤醒（`Waiting→Woken`）后才 drop」有显式交接
//! （`shared.rs::abandon_send_waker` 的 `Woken` 分支调 `on_recv()`），但该次
//! 重唤醒未送达某个真实等待者——缺口在 fire 游标 / 重注册状态复位 / timer
//! 变体三者之一。限时守卫把这种滞留变为快速失败 + 诊断，而不是 120s 硬超时
//! 悬挂门禁；引擎侧的等待上界加固与根因定案由 io-32 跟踪。

use std::{
  future::Future,
  path::PathBuf,
  pin::Pin,
  task::{Context, Poll},
  time::Duration,
};

use compio::{
  runtime::spawn,
  time::{sleep, timeout},
};
use fkv_api::{Request, ScanPageOption};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_shard::{Shard, ShardConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;
use futures::task::noop_waker;

/// 积压规模：远大于 `queue_size = 1`。用 `CreateArchive`（写唯一 tag 的归档文件
/// + prune）而不是 `Gc`（空表近似 no-op），保证低优先通道在观测窗口内保持满。
const BACKLOG_NUM: usize = 512;

/// test-24 场景限时：正常 <1s；唤醒/取消竞态滞留快速失败并给诊断，
/// 不让 120s 硬超时悬挂门禁。
const SCENE_DEADLINE: Duration = Duration::from_secs(15);

/// 全表扫描的数据规模：`SCAN_ROUNDS × SCAN_BATCH` 条 entry（512B 页下约一
/// entry 一页），让一条 Scan 要读数千页、耗时数十毫秒，把事件循环稳定留在
/// 读窗口里。单批 entry 数要低于本配置写路径的批量字节上限（400 条 300B
/// value 的批会被拒）。
const SCAN_ROUNDS: usize = 20;
const SCAN_BATCH: usize = 350;

fn config(root: PathBuf) -> ShardConfig {
  let mut cfg = ShardConfig::for_tests(root);
  cfg.queue_size = 1;
  cfg.fd_limit = 0;
  cfg
}

fn low_req(tbl: &TableIdent, tag: String) -> Request {
  Request::CreateArchive {
    tbl: tbl.clone(),
    tag,
    num_retained: 1,
  }
}

/// 只 poll 一次：让请求完成到「登记 send」这一步，但不推进到完成。
fn poll_once<F: Future>(fut: Pin<&mut F>) -> Poll<F::Output> {
  let waker = noop_waker();
  let mut cx = Context::from_waker(&waker);
  fut.poll(&mut cx)
}

async fn attach_and_put(shard: &Shard, tbl: &TableIdent) {
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
  shard
    .request(Request::Put {
      tbl: tbl.clone(),
      batch_li: vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      term: Some(1),
    })
    .await
    .unwrap();
}

/// 入队失败（shard 已停，通道断开）：深度必须回滚到 0。
#[compio::test]
async fn enqueue_failure_rolls_back_low_depth() {
  let root = TempRoot::new("shard-backpressure");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  let metrics = shard.metrics().clone();
  let sender = shard.sender();
  let tbl = TableIdent::new("enqueue_fail", 1).unwrap();
  shard.shutdown().await.unwrap();

  assert_eq!(
    sender
      .request(low_req(&tbl, "closed".to_owned()))
      .await
      .unwrap_err(),
    KvError::NotRunning,
    "shard 停止后入队必须失败"
  );
  assert_eq!(
    metrics.snapshot().low_pq_depth,
    0,
    "入队失败后低优先深度必须回滚"
  );
}

/// 请求已记账但还没入队时被 drop（调用方取消）：深度必须回滚。
///
/// 维护请求任务化（io-05）后事件循环消费低优先通道不再被内联 IO 挡住，
/// 通道不会因慢消费自然保持满；这里用一条全表扫描把循环留在读窗口
/// （窗口只消费高优先通道），低优先通道才能稳定塞满。
#[compio::test]
async fn dropped_pending_low_request_rolls_back_depth() {
  // test-24：限时守卫（见文件头风险节）。
  match timeout(SCENE_DEADLINE, drop_pending_scene()).await {
    Ok(()) => {}
    Err(_) => panic!(
      "backpressure 场景挂起（{SCENE_DEADLINE:?}）：疑似 crossfire 唤醒/取消竞态\
       滞留发送者（test-24 / io-32，非本用例或引擎回归；重跑 / 降载后可能通过，load 敏感）"
    ),
  }
}

async fn drop_pending_scene() {
  let root = TempRoot::new("shard-backpressure");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  let tbl = TableIdent::new("drop_depth", 1).unwrap();
  attach_and_put(&shard, &tbl).await;
  // 建一张有足够多页的表（512B 页 × 300B value ≈ 一 entry 一页）：
  // 一条大范围 Scan 要读数万页，读窗口足够长时间保持活跃。
  for round in 0..SCAN_ROUNDS {
    let batch_li = (0..SCAN_BATCH)
      .map(|index| {
        let key = format!("key-{:08}", round * SCAN_BATCH + index);
        BatchEntry::upsert(key.into_bytes(), vec![b'v'; 300], 1)
      })
      .collect();
    shard
      .request(Request::Put {
        tbl: tbl.clone(),
        batch_li,
        term: Some(1),
      })
      .await
      .unwrap();
  }
  let scan_sender = shard.sender();
  let scan_tbl = tbl.clone();
  let scan = spawn(async move {
    scan_sender
      .request(Request::Scan {
        tbl: scan_tbl,
        begin: b"key-".to_vec(),
        end: b"key~".to_vec(),
        max_entries: usize::MAX,
        option: ScanPageOption::default(),
      })
      .await
  });
  sleep(Duration::from_millis(5)).await;

  // 先塞满低优先通道，保证新请求的 send 一定处于 Pending。
  let mut backlog_li = Vec::with_capacity(BACKLOG_NUM);
  for index in 0..BACKLOG_NUM {
    let sender = shard.sender();
    let tbl = tbl.clone();
    let tag = format!("{index:020}");
    backlog_li.push(spawn(
      async move { sender.request(low_req(&tbl, tag)).await },
    ));
  }
  // 前提等待改为有界轮询：负载高时 512 个 spawn 可能在固定睡眠内未被调度
  // （io-32 取证中实测 load ≥50 下 24/30 在此假红），固定 10ms 睡眠会把它
  // 误判成「积压没进通道」。轮询到深度 >0 或超时（超时才判前提失败）。
  let mut waited = Duration::ZERO;
  while shard.metrics_snapshot().low_pq_depth == 0 {
    assert!(
      waited < Duration::from_secs(5),
      "积压没有进入低优先通道，测试前提不成立（等待 {waited:?}）"
    );
    sleep(Duration::from_millis(5)).await;
    waited += Duration::from_millis(5);
  }

  let sender = shard.sender();
  let mut pending = Box::pin(sender.request(low_req(&tbl, "pending".to_owned())));
  assert!(
    poll_once(pending.as_mut()).is_pending(),
    "通道满时第一次 poll 必须是 Pending"
  );
  // 只 poll 过一次就 drop：`LowPqGuard` 必须把入队侧的 +1 回滚。
  // 必须用 `Box::pin`：`pin_mut!` 把 future 藏在作用域内的临时变量里，
  // drop 掉 `Pin<&mut _>` 并不会 drop 真正的 future。
  drop(pending);

  scan.await.unwrap().unwrap();
  for handle in backlog_li {
    handle.await.unwrap().unwrap();
  }
  assert_eq!(
    shard.metrics_snapshot().low_pq_depth,
    0,
    "被 drop 的低优先请求必须回滚深度"
  );

  shard.shutdown().await.unwrap();
}
