use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_config::KvOptions;
use fkv_error::KvError;
use fkv_sdk::KvStore;
use fkv_types::TableIdent;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_sdk_{}_{}_{seq}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos()
  ));
  create_dir_all(&root).unwrap();
  root
}

#[compio::test]
async fn sdk_put_get_delete_scan_floor_and_reopen() {
  let root = temp_root();
  let tbl = TableIdent::new("demo", 1).unwrap();
  let options = KvOptions::default();
  let mut store = KvStore::new(root.clone(), &options).unwrap();
  store
    .put_kv(&tbl, b"a".to_vec(), b"A".to_vec())
    .await
    .unwrap();
  store
    .put_kv(&tbl, b"b".to_vec(), b"B".to_vec())
    .await
    .unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(store.read(&tbl, b"b").await.unwrap(), b"B");
  assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 2);
  assert_eq!(store.floor(&tbl, b"c").await.unwrap().key, b"b");
  assert_eq!(
    store.floor(&tbl, b"0").await.unwrap_err(),
    KvError::NotFound
  );
  store.delete(&tbl, b"a").await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), None);
  assert_eq!(store.read(&tbl, b"a").await.unwrap_err(), KvError::NotFound);
  drop(store);

  let mut store = KvStore::new(root.clone(), &options).unwrap();
  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
  remove_dir_all(root).unwrap();
}

#[compio::test]
async fn sdk_large_value_roundtrip() {
  let root = temp_root();
  let tbl = TableIdent::new("demo", 1).unwrap();
  let options = KvOptions::default();
  let mut store = KvStore::new(root.clone(), &options).unwrap();
  let value: Vec<u8> = (0..30_000).map(|index| (index % 251) as u8).collect();
  store
    .put_large(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), value.clone(), 1).with_metadata(b"meta")],
    )
    .await
    .unwrap();
  let read = store.get_large(&tbl, b"big").await.unwrap().unwrap();
  assert_eq!(read.value, value);
  assert_eq!(read.metadata, b"meta");
  drop(store);

  let mut store = KvStore::new(root.clone(), &options).unwrap();
  store.reopen(&tbl).await.unwrap();
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    value
  );
  remove_dir_all(root).unwrap();
}
