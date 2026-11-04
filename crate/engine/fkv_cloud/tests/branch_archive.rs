use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_io_mem::MemStore;
use fkv_object::{MemObjectStore, ObjectStore};
use fkv_store::Store;
use fkv_types::TableIdent;

#[test]
fn cloud_archive_and_branch_manifest_files() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut store = Store::new(
      CloudStore::new(MemStore::new(512), remote.clone(), "fkv", 18),
      512,
      16,
    )
    .with_branch("main", 2);
    store
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    let archive = store.create_archive(&tbl, "snap", 5).await.unwrap();
    assert_eq!(archive.name, "manifest_main_2_snap");
    assert!(
      remote
        .get("fkv/demo.1/manifest/manifest_main_2_snap")
        .await
        .unwrap()
        .is_some()
    );

    let branch = store.create_branch(&tbl, "feature").await.unwrap();
    assert_eq!(branch.name, "manifest_feature_0");
    assert!(
      remote
        .get("fkv/demo.1/manifest/manifest_feature_0")
        .await
        .unwrap()
        .is_some()
    );
    assert_eq!(store.delete_branch(&tbl, "feature").await.unwrap(), 1);
    assert!(
      remote
        .get("fkv/demo.1/manifest/manifest_feature_0")
        .await
        .unwrap()
        .is_none()
    );
    // active branch manifest 与数据不受影响。
    assert!(remote.get("fkv/demo.1/manifest").await.unwrap().is_some());
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  });
}
