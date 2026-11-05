use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io_compio::CompioStore;
use fkv_large::SegmentLayout;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

#[compio::test]
async fn branch_files_survive_parent_gc_and_are_removed_after_delete_branch() {
  let root = TempRoot::new("store_branch_gc");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let mut main = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 2),
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
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 2),
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
  main
    .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .await
    .unwrap();
  let mut branch = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 2),
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
  let removed = main
    .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .await
    .unwrap();
  assert!(removed > 0);
  assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

  let mut missing = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 2),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  assert_eq!(
    missing.open_branch(&tbl, "feature", 0).await.unwrap_err(),
    KvError::ResourceMissing
  );
}
