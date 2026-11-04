use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_io::FileIo;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_object::{MemObjectStore, ObjectStore};
use fkv_store::Store;
use fkv_types::TableIdent;

fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

fn cloud_store(remote: MemObjectStore) -> CloudStore<MemStore, MemObjectStore> {
  CloudStore::new(
    MemStore::new(512).with_segment_config(4096, 1),
    remote,
    "fkv",
    18,
  )
  .with_segment_config(4096, 1)
}

#[test]
fn cloud_segment_upload_download_and_gc() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 1).unwrap();
    let remote = MemObjectStore::new();
    let mut store = Store::new(cloud_store(remote.clone()), 512, 16).with_large_value(layout, 1024);
    let big = pattern(9000);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 1).with_metadata(b"meta")],
      )
      .await
      .unwrap();
    let segment_meta_li = remote.list("fkv/demo.1/segment/").await.unwrap();
    assert_eq!(segment_meta_li.len(), 2);
    assert_eq!(
      remote
        .get("fkv/demo.1/segment/0")
        .await
        .unwrap()
        .unwrap()
        .len(),
      2 * 4096
    );

    // 全新本地 cache：manifest + segment 都从远端拉取。
    let mut cached =
      Store::new(cloud_store(remote.clone()), 512, 16).with_large_value(layout, 1024);
    cached.reopen(&tbl).await.unwrap();
    assert!(
      cached
        .page_io()
        .local()
        .list_segment_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty()
    );
    let read = cached.get_large(&tbl, b"big").await.unwrap().unwrap();
    assert_eq!(read.value, big);
    assert_eq!(read.metadata, b"meta");
    assert_eq!(
      cached
        .page_io()
        .local()
        .list_segment_file_id_li(&tbl)
        .await
        .unwrap(),
      vec![0, 1]
    );

    // 覆盖为较小的大 value；旧 segment 文件应从本地 + 远端 GC。
    let big2 = pattern(3000);
    cached
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), big2.clone(), 2)],
      )
      .await
      .unwrap();
    assert_eq!(
      cached.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      big2
    );
    let removed = cached.gc(&tbl, 18).await.unwrap();
    assert!(removed >= 1);
    assert!(remote.get("fkv/demo.1/segment/0").await.unwrap().is_none());
    assert!(remote.get("fkv/demo.1/segment/1").await.unwrap().is_some());

    // drop 清理剩余远端 segment / data / manifest。
    cached.drop_table(&tbl).await.unwrap();
    assert!(remote.list("fkv/demo.1/").await.unwrap().is_empty());
  });
}
