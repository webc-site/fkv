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
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_store::Store;
use fkv_types::TableIdent;

fn temp_root(tag: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_branch_store_{tag}_{}_{}_{}",
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

#[test]
fn branch_store_reads_parent_and_writes_independently() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut main = Store::new(MemStore::new(512), 512, 16);
    main
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"main-a".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"main-b".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    main.create_branch(&tbl, "feature").await.unwrap();
    let io = main.into_io();

    let mut branch = Store::new(io, 512, 16);
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(
      branch.get(&tbl, b"a").await.unwrap(),
      Some(b"main-a".to_vec())
    );
    assert_eq!(
      branch.get(&tbl, b"b").await.unwrap(),
      Some(b"main-b".to_vec())
    );
    branch
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"branch-a".to_vec(), 2),
          BatchEntry::upsert(b"c".to_vec(), b"branch-c".to_vec(), 2),
        ],
      )
      .await
      .unwrap();
    assert_eq!(
      branch.get(&tbl, b"a").await.unwrap(),
      Some(b"branch-a".to_vec())
    );
    assert_eq!(
      branch.get(&tbl, b"c").await.unwrap(),
      Some(b"branch-c".to_vec())
    );

    // 重启后 branch 写仍然存在。
    let io = branch.into_io();
    let mut branch = Store::new(io, 512, 16);
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(
      branch.get(&tbl, b"a").await.unwrap(),
      Some(b"branch-a".to_vec())
    );
    assert_eq!(
      branch.get(&tbl, b"c").await.unwrap(),
      Some(b"branch-c".to_vec())
    );

    // main 分支保持原值，看不到 branch 的新 key。
    let io = branch.into_io();
    let mut main = Store::new(io, 512, 16);
    main.reopen(&tbl).await.unwrap();
    assert_eq!(
      main.get(&tbl, b"a").await.unwrap(),
      Some(b"main-a".to_vec())
    );
    assert_eq!(
      main.get(&tbl, b"b").await.unwrap(),
      Some(b"main-b".to_vec())
    );
    assert_eq!(main.get(&tbl, b"c").await.unwrap(), None);
  });
}

#[compio::test]
async fn compio_branch_store_with_large_values_and_missing_branch() {
  let root = temp_root("compio");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let mut main =
    Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16).with_large_value(layout, 1024);
  let big = pattern(9000);
  main
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  main
    .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 1)])
    .await
    .unwrap();
  main.create_branch(&tbl, "feature").await.unwrap();
  drop(main);

  let mut branch =
    Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16).with_large_value(layout, 1024);
  branch.open_branch(&tbl, "feature", 0).await.unwrap();
  assert_eq!(branch.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(
    branch.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big
  );
  let branch_big = pattern(5000);
  branch
    .put_large(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), branch_big.clone(), 2)],
    )
    .await
    .unwrap();
  assert_eq!(
    branch.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    branch_big
  );
  drop(branch);

  let mut branch =
    Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16).with_large_value(layout, 1024);
  branch.open_branch(&tbl, "feature", 0).await.unwrap();
  assert_eq!(
    branch.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    branch_big
  );
  drop(branch);

  let mut main =
    Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16).with_large_value(layout, 1024);
  main.reopen(&tbl).await.unwrap();
  assert_eq!(
    main.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big
  );

  assert_eq!(
    main.open_branch(&tbl, "missing", 0).await.unwrap_err(),
    KvError::ResourceMissing
  );
  assert_eq!(
    main.open_branch(&tbl, "bad name", 0).await.unwrap_err(),
    KvError::InvalidArgs
  );

  remove_dir_all(root).unwrap();
}
