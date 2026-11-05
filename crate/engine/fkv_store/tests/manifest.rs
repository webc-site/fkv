use fkv_btree::BatchEntry;
use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_store::Store;
use fkv_types::TableIdent;

#[test]
fn mem_reopen_after_snapshot_and_log() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
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
    let io = store.into_io();
    let mut reopened = Store::new(io, 512, 16);
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(reopened.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

    reopened
      .put(&tbl, &[BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 2)])
      .await
      .unwrap();
    let io = reopened.into_io();
    let mut second = Store::new(io, 512, 16);
    second.reopen(&tbl).await.unwrap();
    assert_eq!(second.get(&tbl, b"c").await.unwrap(), Some(b"C".to_vec()));
    assert_eq!(second.scan(&tbl, b"", b"", 0).await.unwrap().len(), 3);
  });
}

#[test]
fn manifest_rotation() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 2).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16).with_manifest_limit(1);
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
    let size = store
      .page_io()
      .read_manifest(&tbl)
      .await
      .unwrap()
      .unwrap()
      .len();
    assert_eq!(size, fkv_types::os_page_align());
    let io = store.into_io();
    let mut reopened = Store::new(io, 512, 16);
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(reopened.scan(&tbl, b"", b"", 0).await.unwrap().len(), 3);
  });
}
