//! OutOfMem 请求重试 + PageMapper 回滚。
use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_failpoint::{reset, set_spec};
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_types::{StoreMode, TableIdent};

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_oom_{}_{}_{}",
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

fn config(root: PathBuf, oom_retry_times: u32) -> ShardConfig {
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
    queue_size: 16,
    auto_reopen_retry_times: 0,
    auto_reopen_pending_time_us: 0,
    oom_retry_times,
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

#[compio::test]
async fn out_of_mem_retries_request_after_rollback() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), 1)).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
  let tbl = TableIdent::new("demo", 1).unwrap();

  // 第一次写页注入 OutOfMem；put_at 回滚 mapping，shard 重试后成功。
  set_spec("WritePage@1!err=OutOfMem");
  shard
    .request(Request::Put {
      tbl: tbl.clone(),
      batch_li: vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      term: Some(1),
    })
    .await
    .unwrap();
  reset();

  let response = shard
    .request(Request::Get {
      tbl: tbl.clone(),
      key: b"a".to_vec(),
    })
    .await
    .unwrap();
  match response {
    Response::Get(Some(value)) => assert_eq!(value, b"A"),
    other => panic!("unexpected get response: {other:?}"),
  }
  shard.shutdown().await.unwrap();
  let _ = remove_dir_all(root);
}

#[compio::test]
async fn out_of_mem_retries_large_put_after_rollback() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), 1)).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
  let tbl = TableIdent::new("demo", 1).unwrap();
  // 超过 large_value_threshold：走 segment + data page 双写路径。
  let value = vec![b'L'; 2048];

  set_spec("WritePage@1!err=OutOfMem");
  shard
    .request(Request::PutLarge {
      tbl: tbl.clone(),
      batch_li: vec![BatchEntry::upsert(b"big".to_vec(), value.clone(), 1)],
      term: Some(1),
    })
    .await
    .unwrap();
  reset();

  let response = shard
    .request(Request::GetLarge {
      tbl: tbl.clone(),
      key: b"big".to_vec(),
    })
    .await
    .unwrap();
  match response {
    Response::GetLarge(Some(large)) => assert_eq!(large.value, value),
    other => panic!("unexpected get_large response: {other:?}"),
  }
  shard.shutdown().await.unwrap();
  let _ = remove_dir_all(root);
}

#[compio::test]
async fn out_of_mem_without_retry_fails() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), 0)).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
  let tbl = TableIdent::new("demo", 1).unwrap();
  set_spec("WritePage@1!err=OutOfMem");
  let err = shard
    .request(Request::Put {
      tbl: tbl.clone(),
      batch_li: vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      term: Some(1),
    })
    .await
    .unwrap_err();
  reset();
  assert_eq!(err, KvError::OutOfMem);
  shard.shutdown().await.unwrap();
  let _ = remove_dir_all(root);
}
