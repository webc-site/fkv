//! test-11：Pooled（生产默认）下的并发读写一致性与重开。
//!
//! Pooled 的 COW 旧页会退役回池复用，回收安全依赖「在途快照代际」。本用例在
//! 同一 shard 上让多个读任务持续 `Get`、写任务轮转覆盖同一批 key：
//! - 每次读到的 value 必须完整属于某一轮：首 4 字节是本 key 序号，其余字节是
//!   同一轮次 tag。旧物理页被提前复用 / 跨页串读都会直接破坏这两个不变量；
//! - 写结束后最终值等于最后一轮，重开 shard 后仍一致（空闲池按映射空洞重建）。

use std::{
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
  time::Duration,
};

use compio::{runtime::spawn, time::sleep};
use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_store::AllocMode;
use fkv_testkit::TempRoot;
use fkv_types::{StoreMode, TableIdent};

/// key 数：覆盖多个 data page 与 index page。
const KEYS: u32 = 40;
/// 覆盖轮次：每轮全量重写，制造大量 COW retire。
const LAST_ROUND: u8 = 6;
/// 并发读任务数。
const READER_NUM: usize = 8;
/// value 约 360B：512B 页下基本一 entry 一页，串页 / 复用错页会直接可见。
const VALUE_LEN: usize = 360;

fn config(root: PathBuf) -> ShardConfig {
  ShardConfig {
    alloc_mode: AllocMode::Pooled,
    shard_id: 0,
    root,
    page_size: 512,
    restart_interval: 16,
    index_restart_interval: 16,
    // 16 KiB / file：40+ 页的负载会跨文件，覆盖跨文件分配与复用。
    pages_per_file_shift: 5,
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
    max_write_batch_pages: 8,
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
  format!("k{index:04}").into_bytes()
}

/// 首 4 字节存 key 序号，其余字节是轮次 tag。
fn value(round: u8, index: u32) -> Vec<u8> {
  let mut value = vec![round; VALUE_LEN];
  value[..4].copy_from_slice(&index.to_be_bytes());
  value
}

fn batch(round: u8) -> Vec<BatchEntry> {
  (0..KEYS)
    .map(|index| BatchEntry::upsert(key(index), value(round, index), u64::from(round) + 1))
    .collect()
}

fn put_req(tbl: &TableIdent, round: u8) -> Request {
  Request::Put {
    tbl: tbl.clone(),
    batch_li: batch(round),
    term: Some(1),
  }
}

fn get_req(tbl: &TableIdent, index: u32) -> Request {
  Request::Get {
    tbl: tbl.clone(),
    key: key(index),
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

/// 读到的值必须来自某一轮完整写入：key 序号正确、轮次 tag 一致。
fn check_read(index: u32, got: Option<Vec<u8>>) {
  let Some(value) = got else {
    panic!("并发覆盖期间 key {index} 不可见");
  };
  assert_eq!(value.len(), VALUE_LEN, "key {index} 读到的值长度被截断");
  assert_eq!(
    &value[..4],
    &index.to_be_bytes(),
    "key {index} 读到别的 key 的页（跨页串读）"
  );
  let round = value[4];
  assert!(round <= LAST_ROUND, "key {index} 读到未知轮次 tag={round}");
  assert!(
    value[4..].iter().all(|byte| *byte == round),
    "key {index} 的值内部混合了不同轮次：tag={round}"
  );
}

async fn expect_round(shard: &Shard, tbl: &TableIdent, round: u8) {
  for index in 0..KEYS {
    match shard.request(get_req(tbl, index)).await.unwrap() {
      Response::Get(got) => assert_eq!(got, Some(value(round, index)), "key={index}"),
      other => panic!("unexpected response: {other:?}"),
    }
  }
}

#[compio::test]
async fn pooled_concurrent_put_get_stays_whole_and_survives_reopen() {
  let root = TempRoot::new("pooled_concurrency");
  let cfg = config(root.path().to_path_buf());
  let shard = Shard::start(cfg.clone()).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("pooled_concurrency", 1).unwrap();
  shard.request(put_req(&tbl, 0)).await.unwrap();

  let done = Arc::new(AtomicBool::new(false));
  let started = Arc::new(AtomicUsize::new(0));
  let mut reader_li = Vec::with_capacity(READER_NUM);
  for _ in 0..READER_NUM {
    let sender = shard.sender();
    let tbl = tbl.clone();
    let done = done.clone();
    let started = started.clone();
    reader_li.push(spawn(async move {
      started.fetch_add(1, Ordering::SeqCst);
      let mut reads = 0u64;
      while !done.load(Ordering::Relaxed) {
        for index in 0..KEYS {
          match sender.request(get_req(&tbl, index)).await {
            Ok(Response::Get(got)) => check_read(index, got),
            Ok(other) => panic!("unexpected response: {other:?}"),
            Err(err) => panic!("并发读失败: {err}"),
          }
          reads += 1;
        }
      }
      reads
    }));
  }

  // 所有读任务进入循环后再启动覆盖写，保证读写窗口重叠。
  while started.load(Ordering::SeqCst) < READER_NUM {
    sleep(Duration::from_millis(1)).await;
  }

  let sender = shard.sender();
  let tbl_write = tbl.clone();
  let done_writer = done.clone();
  let writer = spawn(async move {
    for round in 1..=LAST_ROUND {
      if let Err(err) = sender.request(put_req(&tbl_write, round)).await {
        // 先放读任务退出，避免写失败时读者空转。
        done_writer.store(true, Ordering::Relaxed);
        panic!("并发覆盖写失败 round={round}: {err}");
      }
      // 轮间留出读窗口，让 Get 与 COW / reclaim 真正交错。
      sleep(Duration::from_millis(2)).await;
    }
    done_writer.store(true, Ordering::Relaxed);
  });
  writer.await.unwrap();

  let mut reads = 0u64;
  for handle in reader_li {
    reads += handle.await.unwrap();
  }
  assert!(
    reads > 0,
    "读任务必须在并发窗口内至少完成一次读，实际 {reads}"
  );

  // 写完后的可见状态必须是最后一轮。
  expect_round(&shard, &tbl, LAST_ROUND).await;

  // 重开：manifest 重放 + Pooled 空闲池按映射空洞重建后结果不变。
  shard.shutdown().await.unwrap();
  let reopened = Shard::start(cfg).unwrap();
  attach(&reopened).await;
  expect_round(&reopened, &tbl, LAST_ROUND).await;
  reopened.shutdown().await.unwrap();
}
