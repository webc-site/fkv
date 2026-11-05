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

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_branch_attach_{}_{}_{}",
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

#[test]
fn attach_branch_opens_independent_state_and_returns_to_main() {
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
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"main-a".to_vec(), 1)],
      )
      .await
      .unwrap();
    runtime.create_branch(&tbl, "Feature").await.unwrap();

    runtime.detach_shard(0).await.unwrap();
    runtime.attach_shard(0, "feature", 2).await.unwrap();
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap(),
      Some(b"main-a".to_vec())
    );
    runtime
      .put(
        &tbl,
        vec![
          BatchEntry::upsert(b"a".to_vec(), b"branch-a".to_vec(), 2),
          BatchEntry::upsert(b"c".to_vec(), b"branch-c".to_vec(), 2),
        ],
      )
      .await
      .unwrap();
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap(),
      Some(b"branch-a".to_vec())
    );

    runtime.detach_shard(0).await.unwrap();
    runtime.attach_shard(0, "main", 3).await.unwrap();
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap(),
      Some(b"main-a".to_vec())
    );
    assert_eq!(runtime.get(&tbl, b"c").await.unwrap(), None);

    runtime.detach_shard(0).await.unwrap();
    runtime.attach_shard(0, "feature", 4).await.unwrap();
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap(),
      Some(b"branch-a".to_vec())
    );
    assert_eq!(
      runtime.get(&tbl, b"c").await.unwrap(),
      Some(b"branch-c".to_vec())
    );
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}
