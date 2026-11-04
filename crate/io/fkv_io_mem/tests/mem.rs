use fkv_error::KvError;
use fkv_io::{ManifestIo, PageIo, TermIo};
use fkv_io_mem::MemStore;
use fkv_types::TableIdent;

#[test]
fn page_roundtrip() {
  pollster::block_on(async {
    let store = MemStore::new(8);
    let tbl = TableIdent::new("demo", 1).unwrap();
    store.write_page(&tbl, 7, b"12345678").await.unwrap();
    let mut dst = [0u8; 8];
    store.read_page(&tbl, 7, &mut dst).await.unwrap();
    assert_eq!(&dst, b"12345678");
    let mut missing = [0u8; 8];
    assert_eq!(
      store.read_page(&tbl, 8, &mut missing).await.unwrap_err(),
      KvError::ResourceMissing
    );
  });
}

#[test]
fn manifest_roundtrip() {
  pollster::block_on(async {
    let store = MemStore::new(8);
    let tbl = TableIdent::new("demo", 1).unwrap();
    store.append_manifest(&tbl, b"abc").await.unwrap();
    store.append_manifest(&tbl, b"def").await.unwrap();
    assert_eq!(store.read_manifest(&tbl).await.unwrap().unwrap(), b"abcdef");
    store.switch_manifest(&tbl, b"xyz").await.unwrap();
    assert_eq!(store.read_manifest(&tbl).await.unwrap().unwrap(), b"xyz");
    store.drop_manifest(&tbl).await.unwrap();
    assert!(store.read_manifest(&tbl).await.unwrap().is_none());
  });
}

#[test]
fn term_roundtrip() {
  pollster::block_on(async {
    let store = MemStore::new(8);
    assert_eq!(store.read_term("main", 1).await.unwrap(), None);
    store.upsert_term("main", 1, 5).await.unwrap();
    assert_eq!(store.read_term("main", 1).await.unwrap(), Some(5));
    assert_eq!(
      store.upsert_term("main", 1, 4).await.unwrap_err(),
      KvError::ExpiredTerm
    );
  });
}
