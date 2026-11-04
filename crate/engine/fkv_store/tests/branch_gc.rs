use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io_compio::CompioStore;
use fkv_large::SegmentLayout;
use fkv_store::Store;
use fkv_types::TableIdent;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_branch_gc_{}_{}_{}",
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

fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

#[compio::test]
async fn branch_files_survive_parent_gc_and_are_removed_after_delete_branch() {
  let root = temp_root();
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let mut main = Store::new(
    CompioStore::new(root.clone(), 512, 18).with_segment_config(4096, 2),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  main
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  main.create_branch(&tbl, "feature").await.unwrap();

  let mut branch = Store::new(
    CompioStore::new(root.clone(), 512, 18).with_segment_config(4096, 2),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  branch.open_branch(&tbl, "feature", 0).await.unwrap();
  let branch_big = pattern(9000);
  branch
    .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)])
    .await
    .unwrap();
  branch
    .put_large(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), branch_big.clone(), 2)],
    )
    .await
    .unwrap();
  drop(branch);

  // main GC 读取 active + branch manifest，不能删除 branch 文件。
  main.gc(&tbl, 18).await.unwrap();
  let mut branch = Store::new(
    CompioStore::new(root.clone(), 512, 18).with_segment_config(4096, 2),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  branch.open_branch(&tbl, "feature", 0).await.unwrap();
  assert_eq!(branch.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
  assert_eq!(
    branch.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    branch_big
  );
  drop(branch);

  // 删除 branch 后，branch-only 文件应被 GC 回收。
  main.delete_branch(&tbl, "feature").await.unwrap();
  let removed = main.gc(&tbl, 18).await.unwrap();
  assert!(removed > 0);
  assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

  let mut missing = Store::new(
    CompioStore::new(root.clone(), 512, 18).with_segment_config(4096, 2),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  assert_eq!(
    missing.open_branch(&tbl, "feature", 0).await.unwrap_err(),
    KvError::ResourceMissing
  );

  remove_dir_all(root).unwrap();
}
