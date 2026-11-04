use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_io::{FileIo, ManifestIo, PageIo};
use fkv_io_compio::CompioStore;
use fkv_types::TableIdent;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_compio_{}_{}_{}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos(),
    seq
  ));
  create_dir_all(&root).unwrap();
  root
}

#[compio::test]
async fn page_roundtrip() {
  let root = temp_root();
  let store = CompioStore::new(&root, 8, 18);
  let tbl = TableIdent::new("demo", 1).unwrap();
  store.write_page(&tbl, 3, b"12345678").await.unwrap();
  let mut dst = [0u8; 8];
  store.read_page(&tbl, 3, &mut dst).await.unwrap();
  assert_eq!(&dst, b"12345678");
  remove_dir_all(root).unwrap();
}

#[compio::test]
async fn manifest_roundtrip() {
  let root = temp_root();
  let store = CompioStore::new(&root, 8, 18);
  let tbl = TableIdent::new("demo", 1).unwrap();
  store.append_manifest(&tbl, b"abc").await.unwrap();
  store.append_manifest(&tbl, b"def").await.unwrap();
  assert_eq!(store.read_manifest(&tbl).await.unwrap().unwrap(), b"abcdef");
  store.switch_manifest(&tbl, b"xyz").await.unwrap();
  assert_eq!(store.read_manifest(&tbl).await.unwrap().unwrap(), b"xyz");
  store.drop_manifest(&tbl).await.unwrap();
  assert!(store.read_manifest(&tbl).await.unwrap().is_none());
  remove_dir_all(root).unwrap();
}

#[compio::test]
async fn file_roundtrip() {
  let root = temp_root();
  let store = CompioStore::new(&root, 8, 2);
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut bytes = vec![0u8; 4 * 8];
  bytes[..8].copy_from_slice(b"PAGE0000");
  bytes[2 * 8..3 * 8].copy_from_slice(b"PAGE0002");
  store.write_file(&tbl, 0, &bytes).await.unwrap();
  assert_eq!(store.file_size(&tbl, 0).await.unwrap(), Some(32));
  assert_eq!(store.read_file(&tbl, 0).await.unwrap().unwrap(), bytes);
  let mut page = [0u8; 8];
  store.read_page(&tbl, 1, &mut page).await.unwrap();
  assert_eq!(page, [0u8; 8]);
  assert!(store.read_file(&tbl, 1).await.unwrap().is_none());
  store.remove_file(&tbl, 0).await.unwrap();
  assert!(store.file_size(&tbl, 0).await.unwrap().is_none());
  remove_dir_all(root).unwrap();
}
