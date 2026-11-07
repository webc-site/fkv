//! io-09：写派发 / 忙分区读的读屏障按**目标分区**排空，而不是全 shard。
//!
//! 验收：
//! 1. A 表长 Scan 在飞（在飞数 > 16）期间向 B 表 Put，Put 在 Scan 结束前完成，
//!    且期间 `read_in_flight` 不归零（没有全量 drain）；
//! 2. 同分区「读不越过后到的写」：A 表读在飞时向 A 表 Put，Put 必须等 A 的读；
//!    同时 B 表在飞读不被这次等待排空（证明只等目标分区）；
//! 3. 忙分区读经窗口到达：B 表写任务在飞时 Get B，其它分区（A）的在飞读不被
//!    排空，Get B 在写完成后按正确值回执。
//!
//! 慢 IO 用真实 Local 后端 + 大表制造（`yield_budget_us = 1` 让 scan 在页链
//! 边界主动让出）。断言全部基于回执先后顺序 / gauge 采样，不依赖绝对时延；
//! 等待只受 [`WATCHDOG`] 约束，它用于让「完全不推进」的实现有界失败，
//! 不作为性能断言。

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
  time::{sleep, timeout},
};
use fkv_api::{Request, Response, ScanPageOption};
use fkv_btree::BatchEntry;
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_store::AllocMode;
use fkv_testkit::TempRoot;
use fkv_types::{StoreMode, TableIdent};

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
/// 忙分区读场景里 B 表写任务的规模。
///
/// 只需要覆盖「Get B 从提交到被窗口准入」的窗口（`wait_write_running` 观察到
/// 写任务后立即提交），远小于 A 表 scan 的读页数：B 写若和 A 读抢同一份 IO，
/// A 读可能在 Get B 返回前自然结束，让「其它分区读仍在飞」的采样前提失效。
const BUSY_WRITE_ENTRIES: u32 = 256;

fn config(root: PathBuf) -> ShardConfig {
  ShardConfig {
    alloc_mode: AllocMode::Append,
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
    queue_size: 256,
    auto_reopen_retry_times: 0,
    auto_reopen_pending_time_us: 0,
    oom_retry_times: 0,
    compression_level: None,
    verify_checksum: true,
    overflow_pointers: fkv_page::DEFAULT_OVERFLOW_POINTERS,
    file_amplify_factor: 0,
    segment_file_amplify_factor: 0,
    manifest_limit: 8 * 1024 * 1024,
    // 逐页提交：写任务在采样窗口内保持足够长的在飞时间。
    max_write_batch_pages: 1,
    max_inflight_write: 32,
    fd_limit: 10_000,
    index_cache_bytes: fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize,
    index_cache_total_bytes: fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize,
    root_meta_cache_bytes: 0,
    io_queue_size: 4096,
    backend: Backend::default(),
    mode: StoreMode::Local,
    standby: None,
    max_write_concurrency: 0,
    // 生产默认 400us：窗口周期性让出主循环，让「目标分区读已空」的写重新派发；
    // 同时保证暂存写不会因为窗口常驻而永久等全 shard 读空。
    max_processing_time_us: 100,
    yield_budget_us: 1,
  }
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

/// 忙分区读经窗口到达：B 写任务在飞时 Get B 挂 blocked，A 的在飞读不被排空。
#[compio::test]
async fn busy_partition_read_does_not_drain_other_partitions() {
  let root = TempRoot::new("partition_barrier");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  attach(&shard).await;
  let a = TableIdent::new("busy-a", 1).unwrap();
  let b = TableIdent::new("busy-b", 2).unwrap();
  shard.request(put_req(&a, LARGE_ENTRIES)).await.unwrap();
  shard.request(put_req(&b, 1)).await.unwrap();

  // A 表 8 条大 scan：Get B 在窗口里等待 B 写时，其它分区的读必须仍在飞。
  // 用 1ms 最小 gauge 采样判定「Get B 期间 read_in_flight 从未归零」；
  // 旧实现的全量屏障会在 Get B 准入时把 A 读全部排空，采样必然看到 0。
  let mut a_scan_li = Vec::with_capacity(8);
  for _ in 0..8 {
    let sender = shard.sender();
    let req = scan_req(&a);
    a_scan_li.push(spawn(async move { sender.request(req).await }));
  }
  wait_read_in_flight(&shard, 8).await;

  // B 表 Put：任务化写持有 B 分区 Store，让 Get B 在窗口里拿到 Busy。
  // 写规模只需覆盖 Get B 的提交窗口，远小于 A 读的页数：否则两者抢同一份
  // IO 时 A 读可能先自然结束，采样前提失效。
  let b_put = {
    let sender = shard.sender();
    let tbl = b.clone();
    spawn(async move {
      timeout(WATCHDOG, sender.request(put_req(&tbl, BUSY_WRITE_ENTRIES)))
        .await
        .expect("B 大 Put 不应超时")
    })
  };
  wait_write_running(&shard).await;
  // Get B 提交前确认 A 读仍在飞：这是「其它分区读不被排空」的采样前提。
  wait_read_in_flight(&shard, 1).await;

  let armed = Arc::new(AtomicBool::new(false));
  let stop = Arc::new(AtomicBool::new(false));
  let sampler = spawn_min_gauge_sampler(&shard, armed.clone(), stop.clone());
  armed.store(true, Ordering::Relaxed);

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
  let response = get_b.await.unwrap();
  match response.unwrap() {
    Response::Get(Some(got)) => assert_eq!(got, value(0)),
    other => panic!("unexpected Get B response: {other:?}"),
  }
  stop.store(true, Ordering::Relaxed);
  let min = sampler.await.unwrap();
  assert!(min > 0, "忙分区读到达时其它分区在飞读被排空：min={min}");

  b_put.await.unwrap().unwrap();
  for handle in a_scan_li {
    let response = handle.await.unwrap();
    match response.unwrap() {
      Response::Scan(reply) => assert_eq!(reply.entry_li.len(), LARGE_ENTRIES as usize),
      other => panic!("unexpected A scan response: {other:?}"),
    }
  }

  shard.shutdown().await.unwrap();
}
