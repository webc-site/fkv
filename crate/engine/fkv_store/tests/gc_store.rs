use fkv_btree::BatchEntry;
use fkv_io::FileIo;
use fkv_io_mem::MemStore;
use fkv_store::Store;
use fkv_types::TableIdent;

#[test]
fn compact_gc_and_reopen() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::with_pages_per_file(128, 1), 128, 8);
    let mut batch = Vec::new();
    for i in 0..100u32 {
      batch.push(BatchEntry::upsert(
        format!("k{i:03}").into_bytes(),
        vec![b'v'; 30],
        1,
      ));
    }
    store.put(&tbl, &batch).await.unwrap();
    let mut file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
    file_li.sort_unstable();
    assert!(file_li.len() > 2);
    let mut target = None;
    let mut moved = 0;
    for file_id in &file_li {
      let count = store.compact_file(&tbl, *file_id, 1).await.unwrap();
      if count > 0 {
        target = Some(*file_id);
        moved = count;
        break;
      }
    }
    let target = target.expect("no file with live pages");
    assert!(moved > 0);
    let removed = store.gc(&tbl, 1).await.unwrap();
    assert!(removed >= 1);
    assert!(
      !store
        .page_io()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .contains(&target)
    );
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 100);

    let io = store.into_io();
    let mut reopened = Store::new(io, 128, 8);
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(reopened.scan(&tbl, b"", b"", 0).await.unwrap().len(), 100);
  });
}
