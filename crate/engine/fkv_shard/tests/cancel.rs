//! oneshot 完成契约：调用方放弃等待（drop future / 超时）后，未开始的请求不再执行。
//!
//! 用 Cloud 模式（Local IO 后端）确定性地制造「请求挂起 → 调用方取消 → manifest 出现
//! 后重排」的时序：重排的取消请求必须被跳过，且 shard 仍能正常服务后续请求。

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
use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_metrics::Op;
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_store::Store;
use fkv_types::{StoreMode, TableIdent};

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_cancel_{}_{}_{}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos(),
    seq,
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
    pages_per_file_shift: fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    segment_size: 4096,
    segments_per_file_shift: 2,
    large_value_threshold: 1024,
    fixed_buffer_size: 0,
    queue_size: 128,
    // driver 只延迟尝试一次：`pending_time` 必须晚于调用方 30ms 超时，
    // 才能在挂起期间观察取消；manifest 在 100ms 出现、300ms 的 driver 会成功。
    auto_reopen_retry_times: 50,
    auto_reopen_pending_time_us: 300_000,
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
    mode: StoreMode::Cloud,
    standby: None,
    max_write_concurrency: 0,
    max_processing_time_us: 0,
    yield_budget_us: 0,
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

fn get_req(tbl: &TableIdent) -> Request {
  Request::Get {
    tbl: tbl.clone(),
    key: b"a".to_vec(),
  }
}

/// 另一个 store 稍后落盘：模拟远端 manifest 可见性延迟。
async fn write_later(root: PathBuf, tbl: TableIdent, delay: Duration) {
  sleep(delay).await;
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

/// 挂起的读请求被调用方超时取消：manifest 出现后重排时不再执行，
/// 指标计数不增长；shard 后续仍能正常服务。
#[compio::test]
async fn canceled_parked_read_is_skipped() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone())).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("cancel", 1).unwrap();
  let writer = spawn(write_later(
    root.clone(),
    tbl.clone(),
    Duration::from_millis(100),
  ));

  // 首次尝试即刻 `ResourceMissing` 并挂起（计一次 Get），随后调用方超时放弃。
  let sender = shard.sender();
  let timed = timeout(Duration::from_millis(30), sender.request(get_req(&tbl))).await;
  assert!(timed.is_err(), "请求应在挂起期间超时");
  let before = shard.metrics_snapshot().op(Op::Get).total;

  // 等 manifest 出现触发重排：取消标志已置位，重排后必须跳过执行。
  writer.await.unwrap();
  sleep(Duration::from_millis(200)).await;
  assert_eq!(
    shard.metrics_snapshot().op(Op::Get).total,
    before,
    "取消的请求在重排后仍被执行"
  );

  // shard 仍可服务新请求（取消不泄漏队列位）。
  let response = shard.request(get_req(&tbl)).await.unwrap();
  assert!(matches!(response, Response::Get(Some(ref value)) if value == b"A"));

  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}
