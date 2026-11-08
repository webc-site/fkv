//! io-09：写派发 / 忙分区读的读屏障按**目标分区**排空，而不是全 shard。
//!
//! 验收：
//! 1. A 表长 Scan 在飞（在飞数 > 16）期间向 B 表 Put，Put 在 Scan 结束前完成，
//!    且期间 `read_in_flight` 不归零（没有全量 drain）；
//! 2. 同分区「读不越过后到的写」：A 表读在飞时向 A 表 Put，Put 必须等 A 的读；
//!    同时 B 表在飞读不被这次等待排空（证明只等目标分区）；
//! 3. 忙分区读经窗口到达：B 表写序列在飞时 Get B，其它分区（A）的读进度
//!    不被排空阻塞——探针读的完成顺序不得落后于其提交前已发起的全部 scan，
//!    Get B 在写完成后按正确值回执。A 侧流量由补给泵维持（8 种错开的扫描
//!    范围轮转），消除负载下「固定批次 scan 自然跑完让 gauge 前提失效」的
//!    竞态（test-26）。
//!
//! 慢 IO 用真实 Local 后端 + 大表制造（`yield_budget_us = 1` 让 scan 在页链
//! 边界主动让出）。断言全部基于回执先后顺序 / gauge 采样，不依赖绝对时延；
//! 等待只受 [`WATCHDOG`] 约束，它用于让「完全不推进」的实现有界失败，
//! 不作为性能断言。

use std::{
  collections::VecDeque,
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
  },
  time::{Duration, Instant},
};

use compio::{
  runtime::{JoinHandle, spawn},
  time::{sleep, timeout},
};
use fkv_api::{Request, Response, ScanPageOption};
use fkv_btree::BatchEntry;
use fkv_error::Result;
use fkv_shard::{Shard, ShardConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;
use futures::{StreamExt, channel::mpsc};

/// 512B 页下单 entry 基本独占一页：scan 页数 ≈ 表行数，便于制造慢读。
const VALUE_LEN: usize = 480;
/// 小表：一两条 scan 就能在写等待期内读完。
const SMALL_ENTRIES: u32 = 1024;
/// 大表：scan 明显慢于小表，保证对照分区的读仍在飞。
const LARGE_ENTRIES: u32 = 4096;
/// 跨分区 Put 验收要求「在飞数 > 16」。
const SCAN_NUM: usize = 17;
/// 挂死看门狗：实现完全不推进时让测试有界失败，不作为性能断言。
const WATCHDOG: Duration = Duration::from_secs(30);
/// 忙分区读场景里 B 侧写序列的规模：256 条**顺序单条 Put**（每条回执后再发
/// 下一条）。
///
/// 逐条提交保证写门无同分区待合并条目（group commit 不生效），每条 Put 的
/// 派发都独立经过 `finish_read_partition(B)` —— 该用例锁定的两处潜在排空点
/// 之一（另一处是忙分区读自身的准入）。旧 bug 形态（排空退化为全 shard）
/// 会在**每条**写派发上把事件循环挂到其它分区全部在飞读结束，探针读由此
/// 逐条获得判别机会；顺序提交的节奏由回执驱动，不依赖任何时延。
const BUSY_WRITE_PUTS: u32 = 256;
/// 忙分区读场景的 A 表规模（独立于前两例的 [`LARGE_ENTRIES`]）。
///
/// 完成顺序断言的绿侧裕度 = 「提交前已发起 scan 的剩余工作量」相对「探针读
/// 的引擎侧全程」：8 条在飞 scan 的起点按 1/8 表宽轮转（见 [`spawn_traffic`]
/// 的 offset），剩余工作量近似均匀铺满 [0, 全表扫描]；全部在探针全程内跑完
/// 的概率 ~ (探针全程 / 全表扫描)^8。全表扫描在负载下为数十毫秒级，探针
/// （1 页 Get）的引擎侧全程为微秒~毫秒级，该幂次把假红压到可忽略；表宽
/// 不足时幂次底数上升，假红率随之上升——这是把表放大到 16384 的原因。
const BUSY_TABLE_ENTRIES: u32 = 16384;
/// 忙分区读场景的流量深度：泵维持的 A 表「已提交未回执」scan 数。
///
/// 固定批次的 scan 总会在长写期间自然跑完（原用例的假红根因，见 test-26）；
/// 泵按「未回执 < 深度即补发」持续补给，任意时刻在飞 / 在途的 8 条 scan
/// 覆盖 8 个错开的表宽档位。
const PUMP_DEPTH: usize = 8;
/// 忙分区读场景的探针读条数。
///
/// 排空回归（变异形态）下每条 Put 派发都把事件循环挂到最年轻在飞 scan
/// 结束（数十毫秒），写序列的大部分墙钟时间处于「读准入被排空阻塞」状态；
/// 12 条探针全部落在阻塞间隙之外的概率按间隙占比的 12 次方衰减。
const PROBE_NUM: u32 = 12;

fn config(root: PathBuf) -> ShardConfig {
  let mut cfg = ShardConfig::for_tests(root);
  cfg.queue_size = 256;
  cfg.max_write_batch_pages = 1;
  cfg.max_inflight_write = 32;
  cfg.max_processing_time_us = 100;
  cfg.yield_budget_us = 1;
  cfg
}

fn value(index: u32) -> Vec<u8> {
  vec![u8::try_from(index % 251).unwrap_or(0); VALUE_LEN]
}

fn key(index: u32) -> Vec<u8> {
  format!("k{index:08}").into_bytes()
}

fn put_req(tbl: &TableIdent, num: u32) -> Request {
  Request::Put {
    tbl: tbl.clone(),
    batch_li: (0..num)
      .map(|index| BatchEntry::upsert(key(index), value(index), 1))
      .collect(),
    term: Some(1),
  }
}

fn scan_req(tbl: &TableIdent) -> Request {
  Request::Scan {
    tbl: tbl.clone(),
    begin: Vec::new(),
    // 0xff 大于任何 ASCII key，扫到表尾。
    end: vec![0xff; 16],
    max_entries: 0,
    option: ScanPageOption::default(),
  }
}

/// 从第 `begin_index` 行起扫到表尾（`key(begin_index)` 起点的范围 scan，
/// 终点同 [`scan_req`]）：行数 = 表宽 - `begin_index`。
fn scan_req_from(tbl: &TableIdent, begin_index: u32) -> Request {
  Request::Scan {
    tbl: tbl.clone(),
    begin: key(begin_index),
    end: vec![0xff; 16],
    max_entries: 0,
    option: ScanPageOption::default(),
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

/// 等到读在飞数达到 `want`：证明目标读批已全部进入读窗口。
///
/// 只看状态（gauge 达到目标），`WATCHDOG` 只防「读永远不被准入」。
async fn wait_read_in_flight(shard: &Shard, want: u64) {
  let deadline = Instant::now() + WATCHDOG;
  loop {
    let now = shard.metrics_snapshot().read_in_flight;
    if now >= want {
      return;
    }
    assert!(
      Instant::now() < deadline,
      "read_in_flight 未在 {WATCHDOG:?} 内达到 {want}（当前 {now}）"
    );
    sleep(Duration::from_millis(1)).await;
  }
}

async fn wait_write_running(shard: &Shard) {
  let deadline = Instant::now() + WATCHDOG;
  loop {
    if shard.metrics_snapshot().write_running >= 1 {
      return;
    }
    assert!(
      Instant::now() < deadline,
      "写任务未在 {WATCHDOG:?} 内进入在跑状态"
    );
    sleep(Duration::from_millis(1)).await;
  }
}

/// 武装后采样 `read_in_flight` 的最小值；返回 `u64::MAX` 表示从未采样。
///
/// 采样间隔 1ms：判定是「是否曾归零」，采样密度不改变 `> 0` 语义，也不与
/// 机器速度挂钩（io-09 原 50µs 采样只是徒增唤醒开销）。
fn spawn_min_gauge_sampler(
  shard: &Shard,
  armed: Arc<AtomicBool>,
  stop: Arc<AtomicBool>,
) -> JoinHandle<u64> {
  let metrics = shard.metrics().clone();
  spawn(async move {
    let mut min = u64::MAX;
    while !stop.load(Ordering::Relaxed) {
      if armed.load(Ordering::Relaxed) {
        min = min.min(metrics.snapshot().read_in_flight);
      }
      sleep(Duration::from_millis(1)).await;
    }
    min
  })
}

/// 忙分区读场景的回执事件：A 侧全部读请求（scan 与探针）的回执都汇入同一
/// 条通道，**消费顺序 = 引擎侧完成顺序**（每条请求任务在完成瞬间就地发送）。
///
/// 顺序判定在引擎侧计时，测试侧任务何时被调度只影响事件何时被消费、不影响
/// 事件的先后——这是对负载下测试任务唤醒延迟（毫秒级）免疫的关键。
enum TrafficReply {
  /// 一条 A scan：`expect_entries` 为该范围应有的条数（发起时算好）。
  Scan {
    expect_entries: usize,
    result: Result<Response>,
  },
  /// 一条探针读（A 表单键 Get）：`pre_launched` 为探针**提交前**已发起的
  /// scan 总数（发起计数快照）。
  Probe {
    pre_launched: u64,
    result: Result<Response>,
  },
}

/// 校验一条 A scan 回执的范围条数。
fn expect_scan(result: Result<Response>, expect_entries: usize) {
  match result {
    Ok(Response::Scan(reply)) => assert_eq!(reply.entry_li.len(), expect_entries),
    other => panic!("unexpected A scan response: {other:?}"),
  }
}

/// 消费一条回执事件：scan 校验并计数；探针校验并执行**完成顺序断言**。
fn handle_traffic_event(event: TrafficReply, scans_seen: &mut u64, unreplied: &mut usize) {
  match event {
    TrafficReply::Scan {
      expect_entries,
      result,
    } => {
      *unreplied -= 1;
      expect_scan(result, expect_entries);
      *scans_seen += 1;
    }
    TrafficReply::Probe {
      pre_launched,
      result,
    } => {
      match &result {
        Ok(Response::Get(Some(got))) => assert_eq!(got, &value(0)),
        other => panic!("unexpected probe response: {other:?}"),
      }
      assert!(
        *scans_seen < pre_launched,
        "探针读排在其提交前已发起的全部 {pre_launched} 条 scan 之后完成\
         （完成顺序第 {scans_seen}）：其它分区在飞读的准入 / 进度被排空阻塞"
      );
    }
  }
}

/// A 侧流量任务：scan 补给泵 + 顺序判定的唯一消费者。
///
/// - **泵**：维持「已提交未回执」scan ≥ [`PUMP_DEPTH`]，起点按 1/8 表宽
///   轮转（第 i 条从 `key((i % 8) × 表宽/8)` 扫到表尾）——8 条在飞 scan 的
///   剩余工作量铺满 [0, 全表]，任一时刻总有远未跑完的长 scan 在飞；
/// - **消费者**：按完成顺序逐条处理事件（见 [`handle_traffic_event`]）；
/// - **停机**：置 stop 后先放掉自身持有的发送端，再消费完剩余事件——通道
///   在全部发起方任务结束后关闭，等待全程每步都有真实进展。
///
/// 返回 scan 总发起数。
fn spawn_traffic(
  shard: &Shard,
  a: &TableIdent,
  launched: Arc<AtomicU64>,
  mut reply_rx: mpsc::UnboundedReceiver<TrafficReply>,
  reply_tx: mpsc::UnboundedSender<TrafficReply>,
  stop: Arc<AtomicBool>,
) -> JoinHandle<u64> {
  let sender = shard.sender();
  let a = a.clone();
  spawn(async move {
    let mut scans_launched = 0u64;
    let mut scans_seen = 0u64;
    let mut unreplied = 0usize;
    // compio 的 JoinHandle 在 drop 时取消任务：子任务句柄必须持有到任务
    // 自然结束（通道关闭 ⟹ 全部发送端 drop ⟹ 子任务已跑完发送段）。
    let mut children: VecDeque<JoinHandle<()>> = VecDeque::new();
    while !stop.load(Ordering::Relaxed) {
      while unreplied < PUMP_DEPTH {
        // 第 i 条 scan 从表宽的第 (i % 8)/8 档起扫到表尾：长度按 1/8 表宽
        // 轮转铺开，避免同批 scan 同时跑完（簇状死亡会让任何「仍有在读」
        // 的外部观察失去前提）。
        let begin = (scans_launched % 8) as u32 * BUSY_TABLE_ENTRIES / 8;
        let expect_entries = (BUSY_TABLE_ENTRIES - begin) as usize;
        let sender = sender.clone();
        let req = scan_req_from(&a, begin);
        scans_launched += 1;
        unreplied += 1;
        launched.fetch_add(1, Ordering::Relaxed);
        let reply_tx = reply_tx.clone();
        children.push_back(spawn(async move {
          let result = sender.request(req).await;
          let _ = reply_tx.unbounded_send(TrafficReply::Scan {
            expect_entries,
            result,
          });
        }));
      }
      if let Some(event) = reply_rx.next().await {
        handle_traffic_event(event, &mut scans_seen, &mut unreplied);
      } else {
        break;
      }
    }
    // 通道关闭条件是全部发送端 drop：先放掉自身这份，剩余 scan 逐条回执
    // 后由各自的发起任务关闭。
    drop(reply_tx);
    while let Some(event) = reply_rx.next().await {
      handle_traffic_event(event, &mut scans_seen, &mut unreplied);
    }
    scans_launched
  })
}

/// 探针任务：逐条（上一条完成后再发下一条，节奏由回执驱动）向 A 表发起
/// 单键 Get，把 `pre_launched` 快照与回执一起汇入流量通道。
fn spawn_probes(
  shard: &Shard,
  a: &TableIdent,
  launched: Arc<AtomicU64>,
  reply_tx: mpsc::UnboundedSender<TrafficReply>,
) -> JoinHandle<()> {
  let sender = shard.sender();
  let a = a.clone();
  spawn(async move {
    for _ in 0..PROBE_NUM {
      let pre_launched = launched.load(Ordering::Relaxed);
      let sender = sender.clone();
      let req = Request::Get {
        tbl: a.clone(),
        key: key(0),
      };
      let reply_tx = reply_tx.clone();
      spawn(async move {
        let result = sender.request(req).await;
        let _ = reply_tx.unbounded_send(TrafficReply::Probe {
          pre_launched,
          result,
        });
      })
      .await
      .expect("探针任务未 panic");
    }
  })
}

/// A 表长 Scan 在飞时向 B 表 Put：Put 先于 Scan 完成，期间读 gauge 不归零。
#[compio::test]
async fn cross_partition_put_does_not_wait_for_slow_scan() {
  let root = TempRoot::new("partition_barrier");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  attach(&shard).await;
  let a = TableIdent::new("barrier-a", 1).unwrap();
  let b = TableIdent::new("barrier-b", 2).unwrap();
  shard.request(put_req(&a, LARGE_ENTRIES)).await.unwrap();
  shard.request(put_req(&b, 1)).await.unwrap();

  let armed = Arc::new(AtomicBool::new(false));
  let stop = Arc::new(AtomicBool::new(false));
  let sampler = spawn_min_gauge_sampler(&shard, armed.clone(), stop.clone());

  let order = Arc::new(AtomicU64::new(0));
  let mut scan_li = Vec::with_capacity(SCAN_NUM);
  for _ in 0..SCAN_NUM {
    let sender = shard.sender();
    let req = scan_req(&a);
    let order = order.clone();
    scan_li.push(spawn(async move {
      let result = sender.request(req).await.expect("A 表 scan 不应失败");
      (order.fetch_add(1, Ordering::SeqCst), result)
    }));
  }
  wait_read_in_flight(&shard, SCAN_NUM as u64).await;
  armed.store(true, Ordering::Relaxed);

  let put = {
    let sender = shard.sender();
    let order = order.clone();
    spawn(async move {
      let result = timeout(WATCHDOG, sender.request(put_req(&b, 1)))
        .await
        .expect("B 表 Put 不应等 A 的 scan 超时")
        .expect("B 表 Put 不应失败");
      (order.fetch_add(1, Ordering::SeqCst), result)
    })
  };
  let (put_pos, put_response) = put.await.unwrap();
  assert!(matches!(put_response, Response::Put));

  // 全量 drain 会把 gauge 打到 0；目标分区排空不会。
  let at_put_done = shard.metrics_snapshot().read_in_flight;
  assert!(
    at_put_done > 0,
    "B 表 Put 完成时 A 表 scan 已全部结束（发生了全量 drain）"
  );
  stop.store(true, Ordering::Relaxed);
  let min = sampler.await.unwrap();
  assert!(
    min > 0,
    "Put 期间 read_in_flight 曾归零（全量 drain）：min={min}"
  );

  let mut scan_pos_li = Vec::with_capacity(SCAN_NUM);
  for handle in scan_li {
    let (pos, response) = handle.await.unwrap();
    match response {
      Response::Scan(reply) => assert_eq!(reply.entry_li.len(), LARGE_ENTRIES as usize),
      other => panic!("unexpected scan response: {other:?}"),
    }
    scan_pos_li.push(pos);
  }
  assert!(
    scan_pos_li.iter().all(|pos| *pos > put_pos),
    "B 表 Put 被 A 表 scan 拖住：put_pos={put_pos} scan_pos={scan_pos_li:?}"
  );

  shard.shutdown().await.unwrap();
}

/// 同分区「读不越过后到的写」：Put A 等 A 的 scan，但不等仍在飞的 B 表读。
#[compio::test]
async fn same_partition_write_waits_only_for_its_own_reads() {
  let root = TempRoot::new("partition_barrier");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  attach(&shard).await;
  let a = TableIdent::new("own-a", 1).unwrap();
  let b = TableIdent::new("own-b", 2).unwrap();
  shard.request(put_req(&a, SMALL_ENTRIES)).await.unwrap();
  shard.request(put_req(&b, LARGE_ENTRIES)).await.unwrap();

  let order = Arc::new(AtomicU64::new(0));
  // A：一条小 scan（Put A 必须等它）。B：16 条大 scan（Put A 不应等它们）。
  let a_scan = {
    let sender = shard.sender();
    let req = scan_req(&a);
    let order = order.clone();
    spawn(async move {
      let result = sender.request(req).await.expect("A scan 不应失败");
      (order.fetch_add(1, Ordering::SeqCst), result)
    })
  };
  let mut b_scan_li = Vec::with_capacity(SCAN_NUM - 1);
  for _ in 0..SCAN_NUM - 1 {
    let sender = shard.sender();
    let req = scan_req(&b);
    let order = order.clone();
    b_scan_li.push(spawn(async move {
      let result = sender.request(req).await.expect("B scan 不应失败");
      (order.fetch_add(1, Ordering::SeqCst), result)
    }));
  }
  wait_read_in_flight(&shard, SCAN_NUM as u64).await;

  let put = {
    let sender = shard.sender();
    let order = order.clone();
    spawn(async move {
      let result = timeout(WATCHDOG, sender.request(put_req(&a, 1)))
        .await
        .expect("Put A 不应超时")
        .expect("Put A 不应失败");
      (order.fetch_add(1, Ordering::SeqCst), result)
    })
  };
  let (put_pos, put_response) = put.await.unwrap();
  assert!(matches!(put_response, Response::Put));

  let (a_pos, a_response) = a_scan.await.unwrap();
  match a_response {
    Response::Scan(reply) => assert_eq!(reply.entry_li.len(), SMALL_ENTRIES as usize),
    other => panic!("unexpected A scan response: {other:?}"),
  }
  assert!(
    put_pos > a_pos,
    "同分区写越过了先到的读：put_pos={put_pos} a_pos={a_pos}"
  );
  // Put A 完成时 B 的 scan 仍应在飞：排空范围必须仅是 A 分区。
  let at_put_done = shard.metrics_snapshot().read_in_flight;
  assert!(
    at_put_done > 0,
    "Put A 完成时 B 表读也被排空（发生了全量 drain）"
  );

  let mut b_pos_li = Vec::with_capacity(SCAN_NUM - 1);
  for handle in b_scan_li {
    let (pos, response) = handle.await.unwrap();
    match response {
      Response::Scan(reply) => assert_eq!(reply.entry_li.len(), LARGE_ENTRIES as usize),
      other => panic!("unexpected B scan response: {other:?}"),
    }
    b_pos_li.push(pos);
  }
  assert!(
    b_pos_li.iter().any(|pos| *pos > put_pos),
    "Put A 排空了 B 表读：put_pos={put_pos} b_pos={b_pos_li:?}"
  );

  shard.shutdown().await.unwrap();
}

/// 忙分区读经窗口到达：B 写序列在飞时 Get B 挂 blocked，A 侧读的准入与进度
/// 不被排空阻塞。
///
/// 与前两例的不同点：Get B 的回执被 B 写序列压住，窗口天然比一条写长得多，
/// 固定批次的 A scan 会在窗口内自然跑完（原实现因此负载下假红，test-26）。
/// 本用例的判定改为**完成顺序**（见 [`spawn_traffic`] / [`spawn_probes`]）：
/// 探针读（A 表 1 页 Get）不允许排到它提交前已发起的全部 scan 之后完成——
/// 目标分区排空退化为全 shard 排空的回归会把事件循环挂到全部在飞读结束，
/// 期间提交的探针只能在那之后完成，顺序断言确定性转红；绿引擎里探针走
/// 已打开分区 fast path，而铺满剩余工作量的 8 条 scan 中总有远未跑完的。
/// 顺序在引擎侧计时（回执任务完成瞬间入通道），对测试侧调度延迟免疫。
#[compio::test]
async fn busy_partition_read_does_not_drain_other_partitions() {
  let root = TempRoot::new("partition_barrier");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  attach(&shard).await;
  let a = TableIdent::new("busy-a", 1).unwrap();
  let b = TableIdent::new("busy-b", 2).unwrap();
  shard
    .request(put_req(&a, BUSY_TABLE_ENTRIES))
    .await
    .unwrap();
  shard.request(put_req(&b, 1)).await.unwrap();

  let stop = Arc::new(AtomicBool::new(false));
  let launched = Arc::new(AtomicU64::new(0));
  let (reply_tx, reply_rx) = mpsc::unbounded::<TrafficReply>();
  // A 侧流量先行：确认 8 条 A scan 已进入在飞（顺序判定的前提就位）后才
  // 起写序列。
  let traffic = spawn_traffic(
    &shard,
    &a,
    launched.clone(),
    reply_rx,
    reply_tx.clone(),
    stop.clone(),
  );
  wait_read_in_flight(&shard, PUMP_DEPTH as u64).await;

  // B 写序列：任务化写逐条持有 B 分区 Store，每条派发 = 一次潜在排空点。
  let b_put = {
    let sender = shard.sender();
    let tbl = b.clone();
    spawn(async move {
      for _ in 0..BUSY_WRITE_PUTS {
        let result = timeout(WATCHDOG, sender.request(put_req(&tbl, 1)))
          .await
          .expect("B 单条 Put 不应超时");
        assert!(matches!(result.unwrap(), Response::Put), "B 单条 Put 失败");
      }
    })
  };
  wait_write_running(&shard).await;

  // 忙分区读：写序列几乎全程占住 B Store，Get B 的准入必经窗口 !ready 路径
  // （`finish_read_partition(B)` 的第二个潜在排空点），随后挂 blocked 等写
  // 完成。它在写序列的间隙被重排回执，无需在此等待。
  let get_b = {
    let sender = shard.sender();
    let req = Request::Get {
      tbl: b.clone(),
      key: key(0),
    };
    spawn(async move {
      timeout(WATCHDOG, sender.request(req))
        .await
        .expect("忙分区读不应超时")
    })
  };

  // 探针在写序列进行中逐条发起：排空回归的阻塞窗占写序列大部分墙钟时间
  // （每条派发都挂到最年轻在飞 scan 结束），12 条探针逐条获得判别机会。
  let probes = spawn_probes(&shard, &a, launched.clone(), reply_tx.clone());
  timeout(WATCHDOG, probes)
    .await
    .expect("探针序列未在 WATCHDOG 内完成")
    .unwrap();

  // 写序列完成 → 探针已全部回执（顺序断言已在流量任务内逐条执行）→
  // 停泵收尾：消费完剩余 scan 回执后通道关闭，返回总发起数。
  b_put.await.unwrap();
  stop.store(true, Ordering::Relaxed);
  drop(reply_tx);
  let scans_launched = timeout(WATCHDOG, traffic)
    .await
    .expect("流量任务未在 WATCHDOG 内收尾")
    .unwrap();
  assert!(
    scans_launched >= PUMP_DEPTH as u64,
    "流量任务未发起任何 scan（用例空洞）"
  );

  let response = get_b.await.unwrap();
  match response.unwrap() {
    Response::Get(Some(got)) => assert_eq!(got, value(0)),
    other => panic!("unexpected Get B response: {other:?}"),
  }

  shard.shutdown().await.unwrap();
}
