//! eng-32c 并发读验收：
//! 1. 32 并发 `Get` 全部返回正确值，读侧指标与并发度一致；
//! 2. 慢表 `Scan` 与另一表 `Get` 同窗口并发，`Get` 不被 `Scan` 阻塞；
//! 3. attach/detach 与在飞读线性化：detach 前的读全部回执，detach 后
//!    新读返回 `NotRunning`；
//! 4. in-flight 读峰值不超过读侧窗口上限（`MAX_READ_IN_FLIGHT`），且超过单轮
//!    收集上限（`MAX_READ_BATCH`）时不再触发全量 drain（io-04 滑动窗口）。
use std::{
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  time::Duration,
};

use compio::{
  runtime::spawn,
  time::{sleep, timeout},
};
use fkv_api::{Request, Response, ScanPageOption};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_shard::{Backend, MAX_READ_BATCH, MAX_READ_IN_FLIGHT, Shard, ShardConfig};
use fkv_store::AllocMode;
use fkv_testkit::TempRoot;
use fkv_types::{StoreMode, TableIdent};

/// eng-32c 的并发读规模：与 `MAX_READ_BATCH` 相等，恰好打满一个读窗口。
const GET_NUM: u32 = MAX_READ_BATCH as u32;

/// 超出单窗口的突发读数量：覆盖「窗口满 → drain → 再开新窗口」。
const BURST_NUM: u32 = 96;

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
    max_processing_time_us: 0,
    yield_budget_us: 0,
  }
}

fn key(index: u32) -> Vec<u8> {
  format!("key-{index:04}").into_bytes()
}

fn val(index: u32) -> Vec<u8> {
  format!("value-{index:04}").into_bytes()
}

fn put(tbl: &TableIdent, index_li: impl IntoIterator<Item = u32>) -> Request {
  Request::Put {
    tbl: tbl.clone(),
    batch_li: index_li
      .into_iter()
      .map(|index| BatchEntry::upsert(key(index), val(index), 1))
      .collect(),
    term: Some(1),
  }
}

fn get(tbl: &TableIdent, index: u32) -> Request {
  Request::Get {
    tbl: tbl.clone(),
    key: key(index),
  }
}

fn scan(tbl: &TableIdent, end: &[u8]) -> Request {
  Request::Scan {
    tbl: tbl.clone(),
    begin: key(0),
    end: end.to_vec(),
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

fn expect_value(response: Result<Response, KvError>, index: u32) {
  match response.unwrap() {
    Response::Get(Some(got)) => assert_eq!(got, val(index)),
    other => panic!("unexpected response for {index}: {other:?}"),
  }
}

/// 32 并发 `Get`：单 shard 一次收满读批，全部结果正确；读侧 gauge 在请求
/// 结束后归零，峰值与读批记录都不超过 `MAX_READ_BATCH`。
#[compio::test]
async fn thirty_two_concurrent_gets_are_served_and_gauged() {
  let root = TempRoot::new("read_concurrency");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("get32", 1).unwrap();
  shard.request(put(&tbl, 0..GET_NUM)).await.unwrap();

  let idle = shard.metrics_snapshot();
  assert_eq!(idle.read_in_flight, 0, "idle read gauge: {idle:?}");

  let mut handle_li = Vec::new();
  for index in 0..GET_NUM {
    let sender = shard.sender();
    let tbl = tbl.clone();
    handle_li.push(spawn(async move {
      timeout(Duration::from_secs(5), sender.request(get(&tbl, index)))
        .await
        .expect("32 并发读不应超时")
    }));
  }
  for (index, handle) in handle_li.into_iter().enumerate() {
    expect_value(handle.await.unwrap(), index as u32);
  }

  let snapshot = shard.metrics_snapshot();
  assert_eq!(snapshot.read_in_flight, 0, "读完成后 gauge 必须归零");
  assert_eq!(snapshot.io_in_flight, 0, "读完成后 IO gauge 必须归零");
  assert!(
    snapshot.read_in_flight_max <= GET_NUM as u64,
    "读在飞峰值 {} 超过上限 {GET_NUM}",
    snapshot.read_in_flight_max
  );
  assert!(snapshot.read_in_flight_max >= 1);
  assert!(snapshot.read_batch_size.count >= 1, "至少记录一次读批");
  assert!(
    snapshot.read_batch_size.max <= GET_NUM as u64,
    "读批 {} 超过上限 {GET_NUM}",
    snapshot.read_batch_size.max
  );

  shard.shutdown().await.unwrap();
}

/// 慢表 `Scan` 与另一表 `Get` 同窗口并发：点读在超时内完成，scan 结果完整；
/// in-flight 峰值 ≥ 2 证明两个读任务确实同时在飞。
#[compio::test]
async fn slow_scan_does_not_block_other_table_get() {
  let root = TempRoot::new("read_concurrency");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  attach(&shard).await;
  let slow = TableIdent::new("slow-table", 1).unwrap();
  let fast = TableIdent::new("fast-table", 2).unwrap();
  // 大表制造可观的页读，保证 scan 任务在读窗口里停留足够久。
  let slow_batch = (0..512u32)
    .map(|index| BatchEntry::upsert(key(index), vec![index as u8; 480], 1))
    .collect::<Vec<_>>();
  shard
    .request(Request::Put {
      tbl: slow.clone(),
      batch_li: slow_batch,
      term: Some(1),
    })
    .await
    .unwrap();
  shard.request(put(&fast, 0..16)).await.unwrap();

  let sender = shard.sender();
  let scan_tbl = slow.clone();
  let scan_task = spawn(async move { sender.request(scan(&scan_tbl, b"key-zzzz")).await });
  let mut get_li = Vec::new();
  for index in 0..16 {
    let sender = shard.sender();
    let tbl = fast.clone();
    get_li.push(spawn(async move {
      timeout(Duration::from_secs(5), sender.request(get(&tbl, index)))
        .await
        .expect("scan 期间点读应能并发完成")
    }));
  }
  match scan_task.await.unwrap().unwrap() {
    Response::Scan(reply) => assert_eq!(reply.entry_li.len(), 512),
    other => panic!("unexpected scan response: {other:?}"),
  }
  for (index, handle) in get_li.into_iter().enumerate() {
    expect_value(handle.await.unwrap(), index as u32);
  }

  let snapshot = shard.metrics_snapshot();
  assert!(
    snapshot.read_in_flight_max >= 2,
    "scan 与 get 应同时在飞，read_in_flight_max={}",
    snapshot.read_in_flight_max
  );
  assert!(snapshot.read_in_flight_max <= MAX_READ_IN_FLIGHT as u64);
  assert_eq!(snapshot.read_in_flight, 0);
  assert!(
    snapshot.io_in_flight_max >= 1,
    "本地后端应观测到在飞 page/segment IO"
  );

  shard.shutdown().await.unwrap();
}

/// attach/detach 线性化：detach 前入队的读全部回执（值正确或
/// `NotRunning`），detach 后的新读快速失败，重新 attach 后恢复。
#[compio::test]
async fn detach_linearizes_inflight_reads() {
  let root = TempRoot::new("read_concurrency");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("detach", 1).unwrap();
  shard.request(put(&tbl, 0..BURST_NUM)).await.unwrap();

  let mut handle_li = Vec::new();
  for index in 0..BURST_NUM {
    let sender = shard.sender();
    let tbl = tbl.clone();
    handle_li.push(spawn(async move { sender.request(get(&tbl, index)).await }));
  }
  // 让读请求先入队，再发 detach。
  sleep(Duration::from_millis(10)).await;
  let detach = shard
    .request(Request::DetachShard { shard_id: 0 })
    .await
    .unwrap();
  assert!(matches!(detach, Response::DetachShard));

  let mut ok_num = 0;
  for (index, handle) in handle_li.into_iter().enumerate() {
    match handle.await.unwrap() {
      Ok(Response::Get(Some(got))) => {
        assert_eq!(got, val(index as u32));
        ok_num += 1;
      }
      // detach 之后才被调度的读快速失败，绝不允许返回错值。
      Err(KvError::NotRunning) => {}
      other => panic!("unexpected response for {index}: {other:?}"),
    }
  }
  assert!(ok_num > 0, "detach 前应至少有一部分读完成回执");

  // detach 后新读快速失败，且没有遗留的在飞读。
  assert_eq!(
    shard.request(get(&tbl, 0)).await.unwrap_err(),
    KvError::NotRunning
  );
  assert_eq!(shard.metrics_snapshot().read_in_flight, 0);
  assert!(shard.metrics_snapshot().read_in_flight_max <= MAX_READ_IN_FLIGHT as u64);

  // 重新 attach 后同一表可读。
  attach(&shard).await;
  expect_value(shard.request(get(&tbl, 3)).await, 3);

  shard.shutdown().await.unwrap();
}

/// 突发 96 条读 + 慢 scan：在飞读峰值被读侧窗口上限
/// `MAX_READ_IN_FLIGHT` 截断（不再被单轮收集上限 32 截断），全部请求按原值
/// 回执；读批大小仍不超过 `MAX_READ_BATCH`。
#[compio::test]
async fn read_in_flight_peak_is_bounded_by_window_limit() {
  let root = TempRoot::new("read_concurrency");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  attach(&shard).await;
  let slow = TableIdent::new("burst-slow", 1).unwrap();
  let fast = TableIdent::new("burst-fast", 2).unwrap();
  let slow_batch = (0..512u32)
    .map(|index| BatchEntry::upsert(key(index), vec![index as u8; 480], 1))
    .collect::<Vec<_>>();
  shard
    .request(Request::Put {
      tbl: slow.clone(),
      batch_li: slow_batch,
      term: Some(1),
    })
    .await
    .unwrap();
  shard.request(put(&fast, 0..BURST_NUM)).await.unwrap();

  let sender = shard.sender();
  let scan_tbl = slow.clone();
  let scan_task = spawn(async move { sender.request(scan(&scan_tbl, b"key-zzzz")).await });
  let mut handle_li = Vec::new();
  for index in 0..BURST_NUM {
    let sender = shard.sender();
    let tbl = fast.clone();
    handle_li.push(spawn(async move {
      timeout(Duration::from_secs(5), sender.request(get(&tbl, index)))
        .await
        .expect("突发读不应超时")
    }));
  }
  match scan_task.await.unwrap().unwrap() {
    Response::Scan(reply) => assert_eq!(reply.entry_li.len(), 512),
    other => panic!("unexpected scan response: {other:?}"),
  }
  for (index, handle) in handle_li.into_iter().enumerate() {
    expect_value(handle.await.unwrap(), index as u32);
  }

  let snapshot = shard.metrics_snapshot();
  assert_eq!(snapshot.read_in_flight, 0);
  assert!(
    snapshot.read_in_flight_max <= MAX_READ_IN_FLIGHT as u64,
    "in-flight 峰值 {} 超过读窗口上限 {MAX_READ_IN_FLIGHT}",
    snapshot.read_in_flight_max
  );
  assert!(
    snapshot.read_batch_size.max <= MAX_READ_BATCH as u64,
    "读批 {} 超过单轮收集上限 {MAX_READ_BATCH}",
    snapshot.read_batch_size.max
  );

  shard.shutdown().await.unwrap();
}

/// io-23：主循环批提交路径同样遵守 `MAX_READ_IN_FLIGHT`。
///
/// 窗口按 `max_processing_time_us` 预算退休时在飞读可能已满 128 条；旧实现
/// 之后从高优先通道再批提交最多 `MAX_READ_BATCH` 条，直接 `push` 进
/// `ReadFlight`，峰值 128 + 32 = 160。现在批收集按剩余额度截断，溢出的读
/// 进入通用 deferred 队列，由读窗口等空位按到达顺序准入。
///
/// 规模取 issue 探针口径（Mem 后端、8 轮 × 2000 并发 Get），断言峰值严格
/// 不超过窗口上限，且确实超过单轮收集上限（证明用例走到了滑动窗口）。
#[compio::test]
async fn read_in_flight_peak_stays_under_window_limit_after_retirement() {
  const ROUNDS: u32 = 8;
  const NUM: u32 = 2000;
  const VALUE_LEN: usize = 64;
  let root = TempRoot::new("read_concurrency");
  let mut cfg = config(root.path().to_path_buf());
  cfg.backend = Backend::Mem;
  cfg.queue_size = 1024;
  // 100µs 窗口预算：窗口持续退休，主循环批提交路径在在飞读未排空时运行。
  cfg.max_processing_time_us = 100;
  let shard = Shard::start(cfg).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("cap", 1).unwrap();
  let batch_li = (0..NUM)
    .map(|index| BatchEntry::upsert(key(index), vec![b'v'; VALUE_LEN], 1))
    .collect();
  shard
    .request(Request::Put {
      tbl: tbl.clone(),
      batch_li,
      term: Some(1),
    })
    .await
    .unwrap();

  let sender = shard.sender();
  let mut handle_li = Vec::with_capacity((ROUNDS * NUM) as usize);
  for round in 0..ROUNDS {
    for index in 0..NUM {
      let sender = sender.clone();
      let tbl = tbl.clone();
      handle_li.push(spawn(async move {
        timeout(
          Duration::from_secs(20),
          sender.request(get(&tbl, (index + round * 7) % NUM)),
        )
        .await
        .expect("并发 Get 不应超时")
      }));
    }
  }
  for handle in handle_li {
    match handle.await.unwrap().unwrap() {
      Response::Get(Some(_)) => {}
      other => panic!("unexpected response: {other:?}"),
    }
  }

  let snapshot = shard.metrics_snapshot();
  assert_eq!(snapshot.read_in_flight, 0, "全部完成后 gauge 必须归零");
  assert!(
    snapshot.read_in_flight_max <= MAX_READ_IN_FLIGHT as u64,
    "读在飞峰值 {} 超过 MAX_READ_IN_FLIGHT={MAX_READ_IN_FLIGHT}：批提交路径绕过额度检查",
    snapshot.read_in_flight_max
  );
  assert!(
    snapshot.read_in_flight_max > MAX_READ_BATCH as u64,
    "读在飞峰值 {} 未超过单轮收集上限，用例未覆盖滑动窗口",
    snapshot.read_in_flight_max
  );

  shard.shutdown().await.unwrap();
}

/// io-24：读在飞满额时访问未打开冷表的读不能把事件循环锁进
/// 「窗口 Exit → deferred → 窗口 seed → 再次 Exit」的无 await 闭环。
///
/// 规模取 issue 探针口径：Mem 后端、`queue_size=1024`、hot 表 2000 key 并已
/// 打开、64 张 cold 表不预开；8 轮 × 2000 并发 Get，每 37 条换一张 cold 表。
/// 修复前，冷表读在 `window_admit` 中先因「表未 ready」Exit（早于 `is_full`
/// 检查），主循环在 `quota == 0` 时把它放进 `deferred_read_li`，下一轮又作
/// 窗口 seed 重入并再次 Exit：整条环路没有一次 `Pending`，shard 线程 100%
/// CPU 忙转，128 条在飞读永远不完成（本用例 30s 超时）。
/// 修复后全部请求完成，冷表未命中按 `Get(None)` 回执，峰值仍不突破上限。
#[compio::test]
async fn cold_table_read_at_flight_cap_does_not_livelock() {
  const ROUNDS: u32 = 8;
  const NUM: u32 = 2000;
  // 每 37 条插入一条冷表读（与 issue 探针一致）。
  const COLD_EVERY: u32 = 37;
  const COLD_NUM: u32 = 64;
  const VALUE_LEN: usize = 64;

  let root = TempRoot::new("read_concurrency");
  let mut cfg = config(root.path().to_path_buf());
  cfg.backend = Backend::Mem;
  cfg.queue_size = 1024;
  // 100µs 窗口预算：窗口按预算退休，主循环在在飞读未排空时继续批量提交。
  cfg.max_processing_time_us = 100;
  let shard = Shard::start(cfg).unwrap();
  attach(&shard).await;
  let hot = TableIdent::new("hot", 1).unwrap();
  let batch_li = (0..NUM)
    .map(|index| {
      BatchEntry::upsert(
        format!("k{index:05}").into_bytes(),
        vec![b'v'; VALUE_LEN],
        1,
      )
    })
    .collect();
  shard
    .request(Request::Put {
      tbl: hot.clone(),
      batch_li,
      term: Some(1),
    })
    .await
    .unwrap();

  let sender = shard.sender();
  let mut handle_li = Vec::with_capacity((ROUNDS * NUM) as usize);
  for round in 0..ROUNDS {
    for index in 0..NUM {
      let sender = sender.clone();
      let hot = hot.clone();
      handle_li.push(spawn(async move {
        if index % COLD_EVERY == 0 {
          // 从未打开过的表：首次访问由主循环串行补开（Mem：空表），
          // 未命中也必须正常回执。
          let tbl =
            TableIdent::new(format!("cold{}", (round * NUM + index) % COLD_NUM), 1).unwrap();
          match sender
            .request(Request::Get {
              tbl,
              key: b"k00000".to_vec(),
            })
            .await
          {
            Ok(Response::Get(None)) => {}
            other => panic!("冷表读回执异常: {other:?}"),
          }
        } else {
          let key = format!("k{:05}", (index + round * 7) % NUM).into_bytes();
          match sender.request(Request::Get { tbl: hot, key }).await {
            Ok(Response::Get(Some(got))) => assert_eq!(got, vec![b'v'; VALUE_LEN]),
            other => panic!("热表读回执异常: {other:?}"),
          }
        }
      }));
    }
  }
  // 单次全局超时：修复前事件循环忙转，客户端请求永远不返回。
  let drained = timeout(Duration::from_secs(30), async move {
    for handle in handle_li {
      handle.await.expect("冷表满额读任务异常退出");
    }
  })
  .await;
  assert!(
    drained.is_ok(),
    "读在飞满额 + 冷表读：事件循环忙转卡死（io-24 回归）"
  );

  let snapshot = shard.metrics_snapshot();
  assert_eq!(snapshot.read_in_flight, 0, "全部完成后 gauge 必须归零");
  assert!(
    snapshot.read_in_flight_max <= MAX_READ_IN_FLIGHT as u64,
    "读在飞峰值 {} 超过 MAX_READ_IN_FLIGHT={MAX_READ_IN_FLIGHT}：冷表读绕过额度检查",
    snapshot.read_in_flight_max
  );
  assert!(
    snapshot.read_in_flight_max > MAX_READ_BATCH as u64,
    "读在飞峰值 {} 未超过单轮收集上限，用例未覆盖满额场景",
    snapshot.read_in_flight_max
  );

  shard.shutdown().await.unwrap();
}

/// io-04：64 条并发 Get 的读窗口不再被 `MAX_READ_BATCH = 32` 的 drain 屏障
/// 卡死；第 33+ 条读不必等在飞 32 条全部完成。
///
/// 构造：Mem 后端 + `overflow_pointers = 1` + `yield_budget_us = 1`，前半批
/// 读做成大 overflow 链（每条 8 KiB）。每条慢读在 overflow 批次边界主动让出，
/// 窗口能持续准入，不会出现「32 条一波、波间全量等待」的 lockstep。
#[compio::test]
async fn read_window_slides_past_batch_limit() {
  const SLOW_NUM: u32 = 48;
  const FAST_NUM: u32 = 16;
  const NUM: u32 = SLOW_NUM + FAST_NUM;

  let root = TempRoot::new("read_concurrency");
  let mut cfg = config(root.path().to_path_buf());
  cfg.backend = Backend::Mem;
  cfg.queue_size = 256;
  cfg.overflow_pointers = 1;
  cfg.yield_budget_us = 1;
  // 8 KiB 值走 overflow 页链而不是 segment 大值，才能在读路径上产生让出点。
  cfg.large_value_threshold = 1 << 20;
  let shard = Shard::start(cfg).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("window", 1).unwrap();
  let slow_val = vec![b's'; 8192];
  let batch_li = (0..SLOW_NUM)
    .map(|index| BatchEntry::upsert(key(index), slow_val.clone(), 1))
    .chain((SLOW_NUM..NUM).map(|index| BatchEntry::upsert(key(index), val(index), 1)))
    .collect();
  shard
    .request(Request::Put {
      tbl: tbl.clone(),
      batch_li,
      term: Some(1),
    })
    .await
    .unwrap();

  // 回执顺序：`sender.request` 返回后立即取号，用先后关系断言快读与慢读交错。
  let order = Arc::new(AtomicU64::new(0));
  let mut handle_li = Vec::with_capacity(NUM as usize);
  for index in 0..NUM {
    let sender = shard.sender();
    let tbl = tbl.clone();
    let order = order.clone();
    handle_li.push(spawn(async move {
      let result = timeout(Duration::from_secs(10), sender.request(get(&tbl, index)))
        .await
        .expect("滑动窗口下读不应挂起");
      let pos = order.fetch_add(1, Ordering::SeqCst);
      (index, pos, result)
    }));
  }

  let mut pos_li = Vec::with_capacity(NUM as usize);
  for handle in handle_li {
    let (index, pos, result) = handle.await.unwrap();
    let expect = if index < SLOW_NUM {
      slow_val.clone()
    } else {
      val(index)
    };
    match result.unwrap() {
      Response::Get(Some(got)) => assert_eq!(got, expect, "read {index}"),
      other => panic!("unexpected response for {index}: {other:?}"),
    }
    pos_li.push((index, pos));
  }

  let snapshot = shard.metrics_snapshot();
  assert_eq!(snapshot.read_in_flight, 0, "全部完成后 gauge 必须归零");
  assert!(
    snapshot.read_in_flight_max > MAX_READ_BATCH as u64,
    "读在飞峰值 {} 仍被单轮收集上限 {MAX_READ_BATCH} 截断",
    snapshot.read_in_flight_max
  );
  assert!(snapshot.read_in_flight_max <= NUM as u64);
  // 交错：旧实现要等前 32 条全部完成才准入第 33 条，快读不可能先于最后一条
  // 慢读完成；现在快读会在慢读仍在飞时回执。
  let max_slow_pos = pos_li
    .iter()
    .filter(|(index, _)| *index < SLOW_NUM)
    .map(|(_, pos)| *pos)
    .max()
    .unwrap();
  let min_fast_pos = pos_li
    .iter()
    .filter(|(index, _)| *index >= SLOW_NUM)
    .map(|(_, pos)| *pos)
    .min()
    .unwrap();
  assert!(
    min_fast_pos < max_slow_pos,
    "快读与慢读没有交错：min_fast_pos={min_fast_pos} max_slow_pos={max_slow_pos}"
  );

  shard.shutdown().await.unwrap();
}
