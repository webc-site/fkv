use std::{
  env::temp_dir,
  fs::{create_dir_all, read_dir, remove_dir_all},
  path::{Path, PathBuf},
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  thread,
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_types::TableIdent;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_archive_crond_{}_{}_{}",
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

fn archive_num(root: &Path) -> usize {
  let dir = root.join("demo.1");
  let Ok(entry_li) = read_dir(dir) else {
    return 0;
  };
  entry_li
    .flatten()
    .filter(|entry| {
      entry
        .file_name()
        .to_str()
        .is_some_and(|name| name.starts_with("manifest_main_1_"))
    })
    .count()
}

fn wait_until(mut predicate: impl FnMut() -> bool, timeout: Duration) -> bool {
  let deadline = Instant::now() + timeout;
  while Instant::now() < deadline {
    if predicate() {
      return true;
    }
    thread::sleep(Duration::from_millis(50));
  }
  predicate()
}

#[test]
fn archive_crond_archives_attached_shards_until_detach() {
  let root = temp_root();
  pollster::block_on(async {
    let runtime = Runtime::start(RuntimeConfig {
      num_threads: 1,
      root: root.clone(),
      page_size: 512,
      restart_interval: 16,
      pages_per_file_shift: 18,
      segment_size: 4096,
      segments_per_file_shift: 2,
      large_value_threshold: 1024,
      fixed_buffer_size: 0,
      queue_size: 64,
      auto_reopen_retry_times: 0,
      auto_reopen_pending_time_us: 0,
      archive_interval_secs: 1,
      archive_num_retained: 2,
    })
    .unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      )
      .await
      .unwrap();

    assert!(
      wait_until(|| archive_num(&root) >= 1, Duration::from_secs(6)),
      "first archive not created"
    );
    assert!(
      wait_until(|| archive_num(&root) >= 2, Duration::from_secs(5)),
      "second archive not created"
    );

    runtime.detach_shard(0).await.unwrap();
    thread::sleep(Duration::from_millis(500));
    let after_detach = archive_num(&root);
    thread::sleep(Duration::from_millis(1300));
    assert_eq!(
      archive_num(&root),
      after_detach,
      "detached shard must not be archived"
    );
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}
