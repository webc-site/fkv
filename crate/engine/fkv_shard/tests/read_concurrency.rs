//! eng-32c 并发读验收：
//! 1. 32 并发 `Get` 全部返回正确值，读侧指标与并发度一致；
//! 2. 慢表 `Scan` 与另一表 `Get` 同窗口并发，`Get` 不被 `Scan` 阻塞；
//! 3. attach/detach 与在飞读线性化：detach 前的读全部回执，detach 后
//!    新读返回 `NotRunning`；
//! 4. in-flight 读峰值不超过读批上限（`MAX_READ_BATCH`）。
use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use compio::{
  runtime::spawn,
  time::{sleep, timeout},
};
use fkv_api::{Request, Response, ScanPageOption};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_shard::{Backend, MAX_READ_BATCH, Shard, ShardConfig};
use fkv_types::{StoreMode, TableIdent};

/// eng-32c 的并发读规模：与 `MAX_READ_BATCH` 相等，恰好打满一个读窗口。
const GET_NUM: u32 = MAX_READ_BATCH as u32;

/// 超出单窗口的突发读数量：覆盖「窗口满 → drain → 再开新窗口」。
const BURST_NUM: u32 = 96;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_read_concurrency_{}_{}_{}",
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

fn config(root: PathBuf) -> ShardConfig {
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
  let root = temp_root();
  let shard = Shard::start(config(root.clone())).unwrap();
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
  let _ = remove_dir_all(root);
}

/// 慢表 `Scan` 与另一表 `Get` 同窗口并发：点读在超时内完成，scan 结果完整；
/// in-flight 峰值 ≥ 2 证明两个读任务确实同时在飞。
#[compio::test]
async fn slow_scan_does_not_block_other_table_get() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone())).unwrap();
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
  assert!(snapshot.read_in_flight_max <= MAX_READ_BATCH as u64);
  assert_eq!(snapshot.read_in_flight, 0);
  assert!(
    snapshot.io_in_flight_max >= 1,
    "本地后端应观测到在飞 page/segment IO"
  );

  shard.shutdown().await.unwrap();
  let _ = remove_dir_all(root);
}

/// attach/detach 线性化：detach 前入队的读全部回执（值正确或
/// `NotRunning`），detach 后的新读快速失败，重新 attach 后恢复。
#[compio::test]
async fn detach_linearizes_inflight_reads() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone())).unwrap();
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
  assert!(shard.metrics_snapshot().read_in_flight_max <= MAX_READ_BATCH as u64);

  // 重新 attach 后同一表可读。
  attach(&shard).await;
  expect_value(shard.request(get(&tbl, 3)).await, 3);

  shard.shutdown().await.unwrap();
  let _ = remove_dir_all(root);
}

/// 突发 96 条读 + 慢 scan：在飞读峰值始终被 `MAX_READ_BATCH` 截断，
/// 全部请求按原值回执。
#[compio::test]
async fn read_in_flight_peak_is_bounded_by_batch_limit() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone())).unwrap();
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
    snapshot.read_in_flight_max <= MAX_READ_BATCH as u64,
    "in-flight 峰值 {} 超过读批上限 {MAX_READ_BATCH}",
    snapshot.read_in_flight_max
  );
  assert!(
    snapshot.read_batch_size.max <= MAX_READ_BATCH as u64,
    "读批 {} 超过上限 {MAX_READ_BATCH}",
    snapshot.read_batch_size.max
  );

  shard.shutdown().await.unwrap();
  let _ = remove_dir_all(root);
}
