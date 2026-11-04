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
use fkv_io::ManifestIo;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_recovery::ReplayState;
use fkv_store::{BRANCH_FILE_RESERVE_FILE_NUM, Store};
use fkv_types::{PageId, TableIdent};

fn temp_root(tag: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_branch_{tag}_{}_{}_{}",
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

fn archive_tag_li(name_li: &[String]) -> Vec<String> {
  let mut tag_li: Vec<String> = name_li
    .iter()
    .filter_map(|name| {
      let (_, _, tag) = fkv_types::parse_manifest_file_name(name)?;
      tag.map(str::to_owned)
    })
    .collect();
  tag_li.sort();
  tag_li
}

#[test]
fn archive_creation_retention_and_branch_metadata() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16)
      .with_branch("main", 7)
      .with_pages_per_file_shift(18);
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    let parent_max_fp = store.mapper(&tbl).unwrap().max_fp_id();
    let parent_root = store.root_id(&tbl).unwrap();

    let first = store.create_archive(&tbl, "001", 2).await.unwrap();
    assert_eq!(first.name, "manifest_main_7_001");
    assert!(first.bytes > 0);
    assert_eq!(first.removed_num, 0);
    store.create_archive(&tbl, "002", 2).await.unwrap();
    let third = store.create_archive(&tbl, "003", 2).await.unwrap();
    assert_eq!(third.removed_num, 1, "oldest archive should be pruned");

    let name_li = store
      .page_io()
      .list_manifest_file_name_li(&tbl)
      .await
      .unwrap();
    assert_eq!(archive_tag_li(&name_li), vec!["002", "003"]);
    assert!(!name_li.contains(&"manifest_main_7_001".to_owned()));

    let bytes = store
      .page_io()
      .read_manifest_file(&tbl, "manifest_main_7_003")
      .await
      .unwrap()
      .unwrap();
    assert!(fkv_manifest::validate_record(&bytes).unwrap());
    let state = ReplayState::from_snapshot(&bytes).unwrap();
    assert_eq!(state.root_id, parent_root);
    assert_eq!(state.branch_metadata.branch_name.as_ref(), "main");
    assert_eq!(state.branch_metadata.term, 7);
    assert_eq!(state.branch_metadata.file_ranges.len(), 1);
    assert_eq!(
      state.branch_metadata.file_ranges[0].max_file_id,
      parent_max_fp >> 18
    );

    // N=0 表示不裁剪。
    let info = store.create_archive(&tbl, "004", 0).await.unwrap();
    assert_eq!(info.removed_num, 0);
    assert_eq!(
      archive_tag_li(
        &store
          .page_io()
          .list_manifest_file_name_li(&tbl)
          .await
          .unwrap()
      )
      .len(),
      3
    );

    let branch = store.create_branch(&tbl, "Feature-1").await.unwrap();
    assert_eq!(branch.name, "manifest_feature-1_0");
    let branch_bytes = store
      .page_io()
      .read_manifest_file(&tbl, &branch.name)
      .await
      .unwrap()
      .unwrap();
    let branch_state = ReplayState::from_snapshot(&branch_bytes).unwrap();
    assert_eq!(branch_state.root_id, parent_root);
    assert_eq!(
      branch_state.branch_metadata.branch_name.as_ref(),
      "feature-1"
    );
    assert_eq!(branch_state.branch_metadata.term, 0);
    let next_file = (parent_max_fp >> 18) + 1;
    let expected_file =
      next_file.div_ceil(BRANCH_FILE_RESERVE_FILE_NUM) * BRANCH_FILE_RESERVE_FILE_NUM;
    assert_eq!(branch_state.max_fp_id, expected_file << 18);

    assert_eq!(
      store.create_branch(&tbl, "main").await.unwrap_err(),
      KvError::InvalidArgs
    );
    assert_eq!(
      store.delete_branch(&tbl, "main").await.unwrap_err(),
      KvError::InvalidArgs
    );
    assert_eq!(store.delete_branch(&tbl, "feature-1").await.unwrap(), 1);
    assert!(
      store
        .page_io()
        .read_manifest_file(&tbl, "manifest_feature-1_0")
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(
      store.create_archive(&tbl, "bad/tag", 1).await.unwrap_err(),
      KvError::InvalidArgs
    );
  });
}

#[compio::test]
async fn compio_archive_files_and_branch_manifest() {
  let root = temp_root("compio");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store =
    Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16).with_branch("main", 3);
  store
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  store.create_archive(&tbl, "001", 2).await.unwrap();
  store.create_archive(&tbl, "002", 2).await.unwrap();
  store.create_archive(&tbl, "003", 2).await.unwrap();
  assert!(!root.join("demo.1").join("manifest_main_3_001").exists());
  assert!(root.join("demo.1").join("manifest_main_3_002").exists());
  assert!(root.join("demo.1").join("manifest_main_3_003").exists());

  let branch = store.create_branch(&tbl, "feature").await.unwrap();
  assert!(root.join("demo.1").join(&branch.name).exists());
  assert!(root.join("demo.1").join("manifest_main_0").exists());
  assert_eq!(store.delete_branch(&tbl, "feature").await.unwrap(), 1);
  assert!(!root.join("demo.1").join(&branch.name).exists());

  drop(store);
  let mut store = Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16);
  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_ne!(store.root_id(&tbl), Some(PageId::MAX));

  remove_dir_all(root).unwrap();
}
