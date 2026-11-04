use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_store::Store;
use fkv_types::TableIdent;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_manifest_{}_{}_{}",
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

#[compio::test]
async fn compio_reopen_after_snapshot_and_log() {
  let root = temp_root();
  let tbl = TableIdent::new("demo", 2).unwrap();
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
  drop(store);

  let mut reopened = Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16);
  reopened.reopen(&tbl).await.unwrap();
  assert_eq!(reopened.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  reopened
    .put(&tbl, &[BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 2)])
    .await
    .unwrap();
  drop(reopened);

  let mut second = Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16);
  second.reopen(&tbl).await.unwrap();
  assert_eq!(second.get(&tbl, b"c").await.unwrap(), Some(b"C".to_vec()));
  assert_eq!(second.scan(&tbl, b"", b"", 0).await.unwrap().len(), 3);
  remove_dir_all(root).unwrap();
}

#[compio::test]
async fn compio_manifest_rotation() {
  let root = temp_root();
  let tbl = TableIdent::new("demo", 3).unwrap();
  let mut store =
    Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16).with_manifest_limit(1);
  for (i, key) in [b"a", b"b", b"c"].into_iter().enumerate() {
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(
          key.to_vec(),
          vec![b'v'; 8],
          i as u64 + 1,
        )],
      )
      .await
      .unwrap();
  }
  drop(store);
  let mut reopened = Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16);
  reopened.reopen(&tbl).await.unwrap();
  assert_eq!(reopened.scan(&tbl, b"", b"", 0).await.unwrap().len(), 3);
  remove_dir_all(root).unwrap();
}
