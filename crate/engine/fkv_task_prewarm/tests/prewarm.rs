use std::slice::from_ref;

use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_io::{FileIo, ManifestIo};
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_object::MemObjectStore;
use fkv_store::Store;
use fkv_task_prewarm::{prewarm_table, prewarm_tables};
use fkv_types::TableIdent;

fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

fn cloud(local: MemStore, remote: MemObjectStore) -> CloudStore<MemStore, MemObjectStore> {
  CloudStore::new(local, remote, "fkv", 18).with_segment_config(4096, 1)
}

#[test]
fn prewarm_pulls_manifest_data_and_segments() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 1).unwrap();
    let remote = MemObjectStore::new();
    let mut store = Store::new(
      cloud(
        MemStore::new(512).with_segment_config(4096, 1),
        remote.clone(),
      ),
      512,
      16,
    )
    .with_large_value(layout, 1024);
    store
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    let big = pattern(9000);
    store
      .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 2)])
      .await
      .unwrap();
    drop(store);

    let fresh = cloud(MemStore::new(512).with_segment_config(4096, 1), remote);
    assert!(
      fresh
        .local()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty()
    );
    let stats = prewarm_table(&fresh, &tbl).await.unwrap();
    assert!(stats.manifest_present);
    assert!(stats.data_file_num >= 1, "{stats:?}");
    assert!(stats.segment_file_num >= 2, "{stats:?}");
    assert_eq!(stats.missing_num, 0);
    assert!(
      !fresh
        .local()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty()
    );
    assert!(
      !fresh
        .local()
        .list_segment_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty()
    );
    assert!(fresh.local().read_manifest(&tbl).await.unwrap().is_some());

    let mut store = Store::new(fresh, 512, 16).with_large_value(layout, 1024);
    store.reopen(&tbl).await.unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      big
    );
  });
}

#[test]
fn prewarm_missing_table_is_reported() {
  pollster::block_on(async {
    let tbl = TableIdent::new("missing", 1).unwrap();
    let fresh = cloud(MemStore::new(512), MemObjectStore::new());
    let stats = prewarm_table(&fresh, &tbl).await.unwrap();
    assert!(!stats.manifest_present);
    assert_eq!(stats.data_file_num, 0);
    assert_eq!(stats.segment_file_num, 0);
    assert_eq!(stats.missing_num, 1);

    let report = prewarm_tables(&fresh, from_ref(&tbl)).await.unwrap();
    assert_eq!(report.len(), 1);
    assert_eq!(report[0].0, tbl);
    assert_eq!(report[0].1.missing_num, 1);
  });
}
