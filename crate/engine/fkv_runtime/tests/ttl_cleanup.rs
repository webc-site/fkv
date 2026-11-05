use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  thread::sleep,
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_types::TableIdent;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_ttl_cleanup_{}_{}_{}",
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

fn now_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_millis() as u64
}

/// Runtime 暴露 CleanExpired 请求：到期后调用可物理删除主树与 TTL 树条目。
#[test]
fn runtime_clean_expired_deletes_due_entries() {
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
      archive_interval_secs: 0,
      archive_num_retained: 100,
    })
    .unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("ttl", 1).unwrap();

    let expire_ts = now_ms() + 1000;
    let mut entry = BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1);
    entry.expire_ts = expire_ts;
    runtime
      .put(
        &tbl,
        vec![
          entry,
          BatchEntry::upsert(b"keep".to_vec(), b"K".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"k").await.unwrap(), Some(b"v".to_vec()));

    // 到期前清理不删任何东西。
    let before = runtime.clean_expired(&tbl).await.unwrap();
    assert_eq!(before.deleted, 0);
    assert!(before.next_expire_ts > 0);

    // 等过期后再清理。
    sleep(Duration::from_millis(1300));
    let after = runtime.clean_expired(&tbl).await.unwrap();
    assert_eq!(after.deleted, 1);
    assert_eq!(after.ttl_deleted, 1);
    assert_eq!(after.next_expire_ts, 0);

    assert_eq!(runtime.get(&tbl, b"k").await.unwrap(), None);
    assert_eq!(
      runtime.get(&tbl, b"keep").await.unwrap(),
      Some(b"K".to_vec())
    );
  });
  let _ = remove_dir_all(&root);
}
