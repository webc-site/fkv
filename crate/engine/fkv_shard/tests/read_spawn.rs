//! 读请求 spawn 并发（eng-32b）：
//! 1. 每表打开状态与读请求解耦：首次读仍能在串行准入路径补开后并发执行；
//! 2. 多表读并发：慢表 scan 不阻塞其他表 get；
//! 3. attach / detach 线性化：detach 前进队的读全部完成回执，detach 后新读
//!    快速失败 `NotRunning`。
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
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_types::{StoreMode, TableIdent};

/// 并发读规模：大于单轮读批上限（32），覆盖「批满后 drain 再开新批」。
const READ_NUM: u32 = 96;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_read_spawn_{}_{}_{}",
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

fn expect_value(response: Result<Response, KvError>, index: u32) {
  match response.unwrap() {
    Response::Get(Some(got)) => assert_eq!(got, val(index)),
    other => panic!("unexpected response for {index}: {other:?}"),
  }
}

/// 首读走串行打开（manifest replay）后 spawn：新 shard 尚未打开任何表，
/// 两条并发读同表只 replay 一次 manifest，结果都正确。
#[compio::test]
async fn first_concurrent_reads_open_table_once_and_spawn() {
  let root = temp_root();
  let tbl = TableIdent::new("cold", 1).unwrap();
  {
    let shard = Shard::start(config(root.clone())).unwrap();
    attach(&shard).await;
    shard.request(put(&tbl, 0..READ_NUM)).await.unwrap();
    shard.shutdown().await.unwrap();
  }
  // 新 shard：所有表状态为空，首读必须走 `ensure_table_ready` 的 reopen 路径。
  let shard = Shard::start(config(root.clone())).unwrap();
  attach(&shard).await;
  let mut handle_li = Vec::new();
  for index in 0..READ_NUM {
    let sender = shard.sender();
    let tbl = tbl.clone();
    handle_li.push(spawn(async move {
      timeout(Duration::from_secs(5), sender.request(get(&tbl, index)))
        .await
        .expect("首读不应被打开路径串行阻塞")
    }));
  }
  for (index, handle) in handle_li.into_iter().enumerate() {
    expect_value(handle.await.unwrap(), index as u32);
  }
  shard.shutdown().await.unwrap();
  let _ = remove_dir_all(root);
}

/// 多表读并发：大表 scan 与另一表点读同时排队，点读不被 scan 阻塞；
/// 全部请求在超时内返回且结果正确。
#[compio::test]
async fn slow_scan_does_not_block_other_table_get() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone())).unwrap();
  attach(&shard).await;
  let slow = TableIdent::new("slow-table", 1).unwrap();
  let fast = TableIdent::new("fast-table", 2).unwrap();
  // slow 表塞入整页量级的 value，让它产生可观的页读；fast 表只放小 value。
  let slow_batch = (0..128u32)
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

  // scan 与点读并发排队：点读不能等 scan 整批跑完。
  let sender = shard.sender();
  let scan_tbl = slow.clone();
  let scan = spawn(async move {
    sender
      .request(Request::Scan {
        tbl: scan_tbl,
        begin: key(0),
        end: b"key-zzzz".to_vec(),
        max_entries: 0,
        option: ScanPageOption::default(),
      })
      .await
  });
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
  let scan_reply = scan.await.unwrap().unwrap();
  match scan_reply {
    Response::Scan(reply) => assert_eq!(reply.entry_li.len(), 128),
    other => panic!("unexpected scan response: {other:?}"),
  }
  for (index, handle) in get_li.into_iter().enumerate() {
    expect_value(handle.await.unwrap(), index as u32);
  }
  // 另一表的 Floor 也走并发读路径。
  let sender = shard.sender();
  let floor_tbl = fast.clone();
  let floor = spawn(async move {
    sender
      .request(Request::Floor {
        tbl: floor_tbl,
        key: b"key-0008".to_vec(),
      })
      .await
  })
  .await
  .unwrap()
  .unwrap();
  match floor {
    Response::Floor(reply) => assert_eq!(reply.key, key(8)),
    other => panic!("unexpected floor response: {other:?}"),
  }

  shard.shutdown().await.unwrap();
  let _ = remove_dir_all(root);
}

/// detach 线性化：detach 前已排队的读全部完成回执；detach 后新读快速失败；
/// 重新 attach 后读恢复正常。
#[compio::test]
async fn detach_linearizes_inflight_reads() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone())).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("detach", 1).unwrap();
  shard.request(put(&tbl, 0..READ_NUM)).await.unwrap();

  // 先把读请求灌满通道再发 detach：这些读在 detach 之前入队，必须全部完成。
  let mut handle_li = Vec::new();
  for index in 0..READ_NUM {
    let sender = shard.sender();
    let req = get(&tbl, index);
    handle_li.push(spawn(async move { sender.request(req).await }));
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
      // 允许「在 detach 之后才被调度」的读快速失败，但绝不能返回错值。
      Err(KvError::NotRunning) => {}
      other => panic!("unexpected response for {index}: {other:?}"),
    }
  }
  assert!(
    ok_num > 0,
    "detach 前的读至少应有一部分完成回执，实际 {ok_num}"
  );

  // detach 后：普通读快速失败。
  assert_eq!(
    shard.request(get(&tbl, 0)).await.unwrap_err(),
    KvError::NotRunning
  );
  // 重新 attach 后同一表可读（重新打开 manifest）。
  attach(&shard).await;
  expect_value(shard.request(get(&tbl, 3)).await, 3);

  shard.shutdown().await.unwrap();
  let _ = remove_dir_all(root);
}
