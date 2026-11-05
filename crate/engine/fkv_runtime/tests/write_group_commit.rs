use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_types::TableIdent;
use futures::future::join_all;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_group_commit_{}_{}_{}",
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
    queue_size: 512,
    auto_reopen_retry_times: 0,
    auto_reopen_pending_time_us: 0,
    archive_interval_secs: 0,
    archive_num_retained: 100,
  }
}

#[test]
fn concurrent_puts_are_group_committed() {
  let root = temp_root();
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.clone())).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("gc", 1).unwrap();
    let request_num = 256usize;
    let future_li = (0..request_num).map(|index| {
      let key = format!("k{index:04}").into_bytes();
      runtime.put(
        &tbl,
        vec![BatchEntry::upsert(key, vec![b'v'; 32], index as u64 + 1)],
      )
    });
    let result_li = join_all(future_li).await;
    assert!(result_li.iter().all(Result::is_ok));

    let fsync_num: u64 = runtime
      .metrics()
      .shard_li
      .iter()
      .map(|shard| shard.fsync_num)
      .sum();
    assert!(
      fsync_num < request_num as u64 / 2,
      "expected group commit, got {fsync_num} fsyncs for {request_num} puts"
    );
    assert_eq!(
      runtime.scan(&tbl, b"", b"", 0).await.unwrap().len(),
      request_num
    );
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}
