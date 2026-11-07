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
    let segment_meta_li = remote
      .list("fkv/demo.1/")
      .await
      .unwrap()
      .into_iter()
      .filter(|m| {
        m.key
          .rsplit('/')
          .next()
          .is_some_and(|n| n.starts_with("segment_"))
      })
      .collect::<Vec<_>>();
    assert_eq!(segment_meta_li.len(), 2);
    assert_eq!(
      remote
        .get("fkv/demo.1/segment_0_main_0")
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
    let removed = cached
      .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
      .await
      .unwrap();
    assert!(removed >= 1);
    assert!(
      remote
        .get("fkv/demo.1/segment_0_main_0")
        .await
        .unwrap()
        .is_none()
    );
    assert!(
      remote
        .get("fkv/demo.1/segment_1_main_0")
        .await
        .unwrap()
        .is_some()
    );

    // drop 只删活跃 manifest；剩余 segment / data 由 FileGc 回收。
    cached.drop_table(&tbl).await.unwrap();
    cached
      .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
      .await
      .unwrap();
    assert!(remote.list("fkv/demo.1/").await.unwrap().is_empty());
  });
}

/// io-12（segment 侧同构）：跨 term 重开归档时，本地 cache 里同 file id 的
/// segment 副本同样不能按 file id 命中。
///
/// term 3 / term 5 的大 value 各自落在 `segment_0_main_3` / `segment_0_main_5`
/// （同一个 file id、不同 term）；同一实例先重开 `v1`（term 5）再重开
/// `only3`（term 3），后者必须回源 term 3 对象，不能读到本地残留的 term 5 槽位。
#[test]
fn reopen_archive_same_instance_validates_segment_cache_identity() {
  pollster::block_on(async {
    let tbl = TableIdent::new("archseg", 1).unwrap();
    let remote = MemObjectStore::new();
    let layout = SegmentLayout::new(4096, 1).unwrap();
    let old: Vec<u8> = pattern(9000);
    let new: Vec<u8> = old.iter().map(|byte| byte ^ 0x5A).collect();
    assert_ne!(old, new);

    let mut t3 = Store::new(cloud_store(remote.clone()), 512, 16)
      .with_branch("main", 3)
      .with_large_value(layout, 1024);
    t3.put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), old.clone(), 1)])
      .await
      .unwrap();
    t3.create_archive(&tbl, "only3", 0).await.unwrap();
    drop(t3);

    let mut t5 = Store::new(cloud_store(remote.clone()), 512, 16)
      .with_branch("main", 5)
      .with_large_value(layout, 1024);
    t5.put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), new.clone(), 2)])
      .await
      .unwrap();
    t5.create_archive(&tbl, "v1", 0).await.unwrap();
    drop(t5);

    let mut store = Store::new(cloud_store(remote), 512, 16)
      .with_branch("main", 6)
      .with_large_value(layout, 1024);
    store.reopen_archive(&tbl, "v1").await.unwrap();
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      new
    );

    store.reopen_archive(&tbl, "only3").await.unwrap();
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      old,
      "重开旧 term 归档必须读到该 term 的 segment 槽位"
    );

    store.reopen_archive(&tbl, "v1").await.unwrap();
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      new
    );
  });
}
