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
use fkv_io::{FileIo, ManifestIo};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_store::{CleanExpiredStats, Store};
use fkv_types::{PageId, TableIdent};

fn temp_root(tag: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_truncate_{tag}_{}_{}_{}",
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
fn truncate_clears_data_ttl_and_manifest() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let mut ttl_entry = BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 1);
    ttl_entry.expire_ts = 100;
    store
      .put_at(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
          ttl_entry,
        ],
        0,
      )
      .await
      .unwrap();
    assert_eq!(store.scan_at(&tbl, b"", b"", 0, 0).await.unwrap().len(), 3);
    let file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
    assert!(!file_li.is_empty());

    store.truncate(&tbl).await.unwrap();
    assert_eq!(store.root_id(&tbl), Some(PageId::MAX));
    assert_eq!(store.ttl_root_id(&tbl), Some(PageId::MAX));
    assert_eq!(store.get_at(&tbl, b"a", 0).await.unwrap(), None);
    assert!(
      store
        .scan_at(&tbl, b"", b"", 0, 0)
        .await
        .unwrap()
        .is_empty()
    );
    assert_eq!(
      store.clean_expired(&tbl, u64::MAX).await.unwrap(),
      CleanExpiredStats::default()
    );
    // truncate 只重置映射 / manifest，data 文件留给 gc。
    assert!(
      !store
        .page_io()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty()
    );

    let io = store.into_io();
    let mut reopened = Store::new(io, 512, 16);
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(reopened.root_id(&tbl), Some(PageId::MAX));
    assert!(reopened.scan(&tbl, b"", b"", 0).await.unwrap().is_empty());

    let removed = reopened.gc(&tbl, 18).await.unwrap();
    assert!(removed >= 1);
    assert!(
      reopened
        .page_io()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty()
    );

    reopened
      .put(
        &tbl,
        &[BatchEntry::upsert(b"new".to_vec(), b"N".to_vec(), 2)],
      )
      .await
      .unwrap();
    assert_eq!(
      reopened.get(&tbl, b"new").await.unwrap(),
      Some(b"N".to_vec())
    );
    assert_eq!(reopened.get(&tbl, b"a").await.unwrap(), None);
    assert_eq!(reopened.scan(&tbl, b"", b"", 0).await.unwrap().len(), 1);
  });
}

#[test]
fn drop_table_removes_manifest_files_and_state() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let mut ttl_entry = BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 1);
    ttl_entry.expire_ts = 100;
    store
      .put_at(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          ttl_entry,
        ],
        0,
      )
      .await
      .unwrap();
    assert!(store.root_id(&tbl).is_some());

    store.drop_table(&tbl).await.unwrap();
    assert_eq!(store.root_id(&tbl), None);
    assert_eq!(store.ttl_root_id(&tbl), None);
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), None);
    assert!(
      store
        .page_io()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty()
    );
    store.drop_table(&tbl).await.unwrap();

    let io = store.into_io();
    assert!(
      ManifestIo::read_manifest(&io, &tbl)
        .await
        .unwrap()
        .is_none()
    );
    let mut store = Store::new(io, 512, 16);
    store
      .put(&tbl, &[BatchEntry::upsert(b"x".to_vec(), b"X".to_vec(), 3)])
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, b"x").await.unwrap(), Some(b"X".to_vec()));
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), None);
  });
}

#[compio::test]
async fn compio_truncate_and_drop() {
  let root = temp_root("compio");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16);
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
  let data_file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
  assert!(!data_file_li.is_empty());
  let manifest_path = root.join("demo.1").join("manifest_main_0");
  assert!(manifest_path.exists());

  store.truncate(&tbl).await.unwrap();
  assert!(manifest_path.exists());
  assert_eq!(store.root_id(&tbl), Some(PageId::MAX));
  drop(store);

  let mut store = Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16);
  store.reopen(&tbl).await.unwrap();
  assert!(store.scan(&tbl, b"", b"", 0).await.unwrap().is_empty());
  assert!(
    !store
      .page_io()
      .list_file_id_li(&tbl)
      .await
      .unwrap()
      .is_empty()
  );
  let removed = store.gc(&tbl, 18).await.unwrap();
  assert!(removed >= 1);
  assert!(
    store
      .page_io()
      .list_file_id_li(&tbl)
      .await
      .unwrap()
      .is_empty()
  );

  store
    .put(
      &tbl,
      &[BatchEntry::upsert(b"new".to_vec(), b"N".to_vec(), 2)],
    )
    .await
    .unwrap();
  assert_eq!(store.get(&tbl, b"new").await.unwrap(), Some(b"N".to_vec()));

  store.drop_table(&tbl).await.unwrap();
  assert!(!manifest_path.exists());
  assert!(
    store
      .page_io()
      .list_file_id_li(&tbl)
      .await
      .unwrap()
      .is_empty()
  );
  let mut store = Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16);
  assert_eq!(
    store.reopen(&tbl).await.unwrap_err(),
    KvError::ResourceMissing
  );
  store
    .put(&tbl, &[BatchEntry::upsert(b"z".to_vec(), b"Z".to_vec(), 3)])
    .await
    .unwrap();
  assert_eq!(store.get(&tbl, b"z").await.unwrap(), Some(b"Z".to_vec()));

  remove_dir_all(root).unwrap();
}
