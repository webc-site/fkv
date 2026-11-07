use fkv_io::{FileIo, ManifestIo, PageIo};
use fkv_io_compio::CompioStore;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

#[compio::test]
async fn page_roundtrip() {
  // `TempRoot` 析构时递归清理，无需手工 `remove_dir_all`。
  let root = TempRoot::new("compio_page");
  // 只验证 IO 通路，写入的不是合法 page，关闭 checksum 校验。
  let store = CompioStore::new(root.path(), 8, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .with_checksum_verify(false);
  let tbl = TableIdent::new("demo", 1).unwrap();
  store.write_page(&tbl, 3, b"12345678").await.unwrap();
  let mut dst = [0u8; 8];
  store.read_page(&tbl, 3, &mut dst).await.unwrap();
  assert_eq!(&dst, b"12345678");
}

#[compio::test]
async fn manifest_roundtrip() {
  let root = TempRoot::new("compio_manifest");
  let store = CompioStore::new(root.path(), 8, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT);
  let tbl = TableIdent::new("demo", 1).unwrap();
  store.append_manifest(&tbl, b"abc").await.unwrap();
  store.append_manifest(&tbl, b"def").await.unwrap();
  assert_eq!(store.read_manifest(&tbl).await.unwrap().unwrap(), b"abcdef");
  store.switch_manifest(&tbl, b"xyz").await.unwrap();
  assert_eq!(store.read_manifest(&tbl).await.unwrap().unwrap(), b"xyz");
  store.drop_manifest(&tbl).await.unwrap();
  assert!(store.read_manifest(&tbl).await.unwrap().is_none());
}

#[compio::test]
async fn file_roundtrip() {
  let root = TempRoot::new("compio_file");
  let store = CompioStore::new(root.path(), 8, 2).with_checksum_verify(false);
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
}
