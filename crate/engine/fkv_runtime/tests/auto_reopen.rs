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
    pages_per_file_shift: 18,
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

#[test]
fn runtime_reopen_waits_for_manifest_from_another_writer() {
  let root = temp_root("wait");
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.clone(), 50, 20_000)).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    let writer_tbl = tbl.clone();
    let writer_root = root.clone();
    let writer = thread::spawn(move || {
      thread::sleep(Duration::from_millis(200));
      let runtime = runtime::Runtime::new().unwrap();
      runtime.block_on(async move {
        let mut store = Store::new(CompioStore::new(writer_root, 512, 18), 512, 16);
        store
          .put(
            &writer_tbl,
            &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
          )
          .await
          .unwrap();
      });
    });

    let started = Instant::now();
    runtime.reopen(&tbl).await.unwrap();
    assert!(started.elapsed() >= Duration::from_millis(150));
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    writer.join().unwrap();
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}

#[test]
fn runtime_reopen_exhausts_retries() {
  let root = temp_root("missing");
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.clone(), 2, 10_000)).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("missing", 1).unwrap();
    let started = Instant::now();
    assert_eq!(
      runtime.reopen(&tbl).await.unwrap_err(),
      KvError::ResourceMissing
    );
    assert!(started.elapsed() >= Duration::from_millis(20));
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}
