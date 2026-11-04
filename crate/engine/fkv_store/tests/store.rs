use fkv_btree::BatchEntry;
use fkv_io_mem::MemStore;
use fkv_store::Store;
use fkv_types::TableIdent;

#[test]
fn put_get_scan_delete() {
  pollster::block_on(async {
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let tbl = TableIdent::new("demo", 1).unwrap();
    let batch = [
      BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
      BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
      BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 1),
    ];
    store.put(&tbl, &batch).await.unwrap();
    assert_eq!(store.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 3);
    store.delete(&tbl, b"b", 2).await.unwrap();
    assert_eq!(store.get(&tbl, b"b").await.unwrap(), None);
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 2);
  });
}
