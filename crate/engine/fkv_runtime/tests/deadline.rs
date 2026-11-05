//! sto-62b：Runtime 数据面默认 deadline 与显式覆盖。

use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_types::TableIdent;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_runtime_deadline_{}_{}_{}",
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

fn config(root: PathBuf) -> RuntimeConfig {
  RuntimeConfig {
    num_threads: 1,
    root,
    page_size: 512,
    restart_interval: 16,
    pages_per_file_shift: 18,
    segment_size: 4096,
    segments_per_file_shift: 2,
    large_value_threshold: 1024,
    fixed_buffer_size: 0,
    auto_reopen_retry_times: 0,
    auto_reopen_pending_time_us: 0,
    archive_interval_secs: 0,
    archive_num_retained: 100,
    queue_size: 64,
  }
}

fn put(tbl: &TableIdent, key: &[u8], value: &[u8]) -> Request {
  Request::Put {
    tbl: tbl.clone(),
    batch_li: vec![BatchEntry::upsert(key.to_vec(), value.to_vec(), 1)],
    term: None,
  }
}

fn get(tbl: &TableIdent, key: &[u8]) -> Request {
  Request::Get {
    tbl: tbl.clone(),
    key: key.to_vec(),
  }
}

/// 默认 deadline 落到便捷方法：`Duration::ZERO` 的请求在 shard 开始执行前被拒，
/// 重启后数据不存在，证明未落盘。
#[test]
fn default_request_timeout_rejects_before_execution() {
  let root = temp_root();
  let tbl = TableIdent::new("deadline", 1).unwrap();
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.clone()))
      .unwrap()
      .with_request_timeout(Duration::ZERO)
      .with_queue_timeout(Duration::from_millis(50));
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let err = runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"late".to_vec(), b"x".to_vec(), 1)],
      )
      .await
      .unwrap_err();
    assert_eq!(err, KvError::Timeout);
    runtime.shutdown().await.unwrap();
  });

  pollster::block_on(async {
    let runtime = Runtime::start(config(root.clone())).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    assert_eq!(
      runtime.read(&tbl, b"late").await.unwrap_err(),
      KvError::NotFound
    );
    runtime.shutdown().await.unwrap();
  });

  let _ = remove_dir_all(root);
}

/// 显式 `request_deadline` 覆盖 Runtime 的默认值；未过期请求正常执行。
#[test]
fn explicit_request_deadline_overrides_runtime_default() {
  let root = temp_root();
  let tbl = TableIdent::new("deadline", 1).unwrap();
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.clone()))
      .unwrap()
      .with_request_timeout(Duration::ZERO);
    runtime.attach_shard(0, "main", 1).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"k").await.unwrap_err(), KvError::Timeout);
    let deadline = Duration::from_secs(30);
    runtime
      .request_deadline(&tbl, put(&tbl, b"k", b"v"), deadline)
      .await
      .unwrap();
    assert!(matches!(
      runtime.request_deadline(&tbl, get(&tbl, b"k"), deadline).await.unwrap(),
      Response::Get(Some(value)) if value == b"v"
    ));
    runtime.shutdown().await.unwrap();
  });
  let _ = remove_dir_all(root);
}
