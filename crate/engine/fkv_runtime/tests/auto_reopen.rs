//! 外部 `Reopen` 的失败语义：对齐 C++，manifest 暂不可见时**一次即回**
//! `ResourceMissing`（由调用方自行退避重试），不在 shard 内挂满
//! `auto_reopen_retry_times × pending_time`。
//!
//! 读请求的延迟 driver / waiter 语义在 `fkv_shard/tests/cloud_reopen.rs` 覆盖
//! （需要 `StoreMode::Cloud`；本文件的 `Runtime::start` 是 Local 模式，读缺失表
//! 走懒创建空表语义）。

use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  thread,
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use compio::runtime;
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io_compio::CompioStore;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_store::Store;
use fkv_types::TableIdent;

fn temp_root(tag: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_runtime_auto_reopen_{tag}_{}_{}_{}",
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

fn config(root: PathBuf, retry_times: u8, pending_us: u64) -> RuntimeConfig {
  RuntimeConfig {
    num_threads: 1,
    root,
    page_size: 512,
    restart_interval: 16,
    pages_per_file_shift: fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    segment_size: 4096,
    segments_per_file_shift: 2,
    large_value_threshold: 1024,
    fixed_buffer_size: 0,
    queue_size: 64,
    auto_reopen_retry_times: retry_times,
    auto_reopen_pending_time_us: pending_us,
    archive_interval_secs: 0,
    archive_num_retained: 100,
  }
}

/// `reopen` 缺失表：即使配置了多轮预算，也立即回 `ResourceMissing`。
#[test]
fn runtime_reopen_missing_manifest_fails_fast() {
  let root = temp_root("missing");
  pollster::block_on(async {
    // 多轮预算 × 长 pending：若被 park，调用方会挂 50 × 1s。
    let runtime = Runtime::start(config(root.clone(), 50, 1_000_000)).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("missing", 1).unwrap();
    let started = Instant::now();
    assert_eq!(
      runtime.reopen(&tbl).await.unwrap_err(),
      KvError::ResourceMissing
    );
    assert!(
      started.elapsed() < Duration::from_millis(200),
      "外部 Reopen 不应在 shard 内挂起：{:?}",
      started.elapsed()
    );
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}

/// 外部 Reopen 失败后由调用方决定重试：manifest 出现后再次 reopen 成功。
#[test]
fn runtime_reopen_succeeds_after_manifest_appears() {
  let root = temp_root("retry");
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.clone(), 50, 1_000_000)).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();

    let started = Instant::now();
    assert_eq!(
      runtime.reopen(&tbl).await.unwrap_err(),
      KvError::ResourceMissing
    );
    assert!(started.elapsed() < Duration::from_millis(200));

    // 另一个 writer 稍后落盘：模拟远端 manifest 迟到。
    let writer_tbl = tbl.clone();
    let writer_root = root.clone();
    let writer = thread::spawn(move || {
      thread::sleep(Duration::from_millis(100));
      let runtime = runtime::Runtime::new().unwrap();
      runtime.block_on(async move {
        let mut store = Store::new(
          CompioStore::new(writer_root, 512, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT),
          512,
          16,
        );
        store
          .put(
            &writer_tbl,
            &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
          )
          .await
          .unwrap();
      });
    });
    writer.join().unwrap();

    // 调用方自己退避后重试：manifest 已可见，直接成功。
    runtime.reopen(&tbl).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}
