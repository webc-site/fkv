use std::sync::Arc;

use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_io::{FileIo, PageIo};
use fkv_io_mem::MemStore;
use fkv_metrics::ShardMetrics;
use fkv_object::{MemObjectStore, ObjectStore};
use fkv_store::Store;
use fkv_types::TableIdent;

#[test]
fn cloud_overlay_cache_miss_and_reopen() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut store = Store::new(
      CloudStore::new(MemStore::new(512), remote.clone(), "fkv", 18),
      512,
      16,
    );
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
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    let key_li = remote.list("fkv/demo.1/").await.unwrap();
    assert!(
      key_li
        .iter()
        .any(|meta| meta.key.starts_with("fkv/demo.1/data/"))
    );
    assert!(
      key_li
        .iter()
        .all(|meta| !meta.key.starts_with("fkv/demo.1/page/"))
    );
    drop(store);

    let mut cached = Store::new(
      CloudStore::new(MemStore::new(512), remote.clone(), "fkv", 18),
      512,
      16,
    );
    cached.reopen(&tbl).await.unwrap();
    assert_eq!(cached.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
    // 缓存未命中时按整文件下载：index + data 至少两个 page 同时驻留。
    assert!(cached.page_io().local().page_count() >= 2);
    assert_eq!(cached.scan(&tbl, b"", b"", 0).await.unwrap().len(), 2);
  });
}

use fkv_error::KvError;
use fkv_io::TermIo;

#[test]
fn term_cas() {
  pollster::block_on(async {
    let remote = MemObjectStore::new();
    let store = CloudStore::new(MemStore::new(512), remote.clone(), "fkv", 18);
    store.upsert_term("main", 1, 1).await.unwrap();
    store.upsert_term("main", 1, 2).await.unwrap();
    assert_eq!(store.read_term("main", 1).await.unwrap(), Some(2));
    assert_eq!(
      store.upsert_term("main", 1, 1).await.unwrap_err(),
      KvError::ExpiredTerm
    );
  });
}

#[test]
fn store_rejects_expired_term() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut first = Store::new(
      CloudStore::new(MemStore::new(512), remote.clone(), "fkv", 18),
      512,
      16,
    );
    first.claim_term("main", 1, 1).await.unwrap();
    first
      .put_with_term(
        &tbl,
        &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
        "main",
        1,
        1,
      )
      .await
      .unwrap();

    let second = Store::new(
      CloudStore::new(MemStore::new(512), remote.clone(), "fkv", 18),
      512,
      16,
    );
    second.claim_term("main", 1, 2).await.unwrap();
    assert_eq!(
      first
        .put_with_term(
          &tbl,
          &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)],
          "main",
          1,
          1,
        )
        .await
        .unwrap_err(),
      KvError::ExpiredTerm
    );
  });
}

#[test]
fn whole_file_upload_is_deferred_until_sync() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    );
    for file_page_id in 0..3u64 {
      let mut page = vec![0u8; 512];
      page[0] = file_page_id as u8 + 1;
      cloud.write_page(&tbl, file_page_id, &page).await.unwrap();
    }
    assert!(remote.list("fkv/demo.1/data/").await.unwrap().is_empty());
    cloud.sync_data(&tbl).await.unwrap();
    let meta_li = remote.list("fkv/demo.1/data/").await.unwrap();
    assert_eq!(meta_li.len(), 1);
    assert_eq!(meta_li[0].key, "fkv/demo.1/data/0");
    assert_eq!(meta_li[0].size, 3 * 512);
    assert!(remote.list("fkv/demo.1/page/").await.unwrap().is_empty());
  });
}

#[test]
fn cache_budget_evicts_and_redownloads_whole_files() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    // 4 pages / file = 2048 B；预算 3072 B，只容得下一个整文件。
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_budget(3072);
    for file_page_id in 0..8u64 {
      let mut page = vec![0u8; 512];
      page[0] = file_page_id as u8 + 1;
      cloud.write_page(&tbl, file_page_id, &page).await.unwrap();
    }
    cloud.sync_data(&tbl).await.unwrap();
    assert_eq!(remote.list("fkv/demo.1/data/").await.unwrap().len(), 2);
    assert_eq!(cloud.resident_file_count(), 1);
    assert_eq!(cloud.resident_bytes(), 2048);
    // 最久未使用的 file 0 被淘汰。
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_none());
    assert!(cloud.local().file_size(&tbl, 1).await.unwrap().is_some());

    let mut page = vec![0u8; 512];
    cloud.read_page(&tbl, 0, &mut page).await.unwrap();
    assert_eq!(page[0], 1);
    // 重新下载 file 0 后预算仍然生效：file 1 被淘汰。
    assert_eq!(cloud.resident_file_count(), 1);
    assert_eq!(cloud.resident_bytes(), 2048);
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_some());
    assert!(cloud.local().file_size(&tbl, 1).await.unwrap().is_none());
  });
}

#[test]
fn write_to_evicted_file_restores_before_modifying() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_budget(3072);
    for file_page_id in 0..8u64 {
      let mut page = vec![0u8; 512];
      page[0] = file_page_id as u8 + 1;
      cloud.write_page(&tbl, file_page_id, &page).await.unwrap();
    }
    cloud.sync_data(&tbl).await.unwrap();
    // file 0 已被淘汰，再写其中一页必须先恢复整文件。
    let page = vec![7u8; 512];
    cloud.write_page(&tbl, 1, &page).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    let Some(bytes) = remote.get("fkv/demo.1/data/0").await.unwrap() else {
      panic!("file 0 must be uploaded");
    };
    assert_eq!(bytes[0], 1);
    assert_eq!(bytes[512], 7);
    assert_eq!(bytes[2 * 512], 3);
  });
}

#[test]
fn drop_table_removes_local_and_remote_objects() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut store = Store::new(
      CloudStore::new(MemStore::new(512), remote.clone(), "fkv", 18),
      512,
      16,
    );
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
    assert!(!remote.list("fkv/demo.1/").await.unwrap().is_empty());

    store.drop_table(&tbl).await.unwrap();
    assert!(remote.list("fkv/demo.1/").await.unwrap().is_empty());
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), None);

    store
      .put(&tbl, &[BatchEntry::upsert(b"x".to_vec(), b"X".to_vec(), 2)])
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, b"x").await.unwrap(), Some(b"X".to_vec()));
    assert!(!remote.list("fkv/demo.1/").await.unwrap().is_empty());
  });
}

#[test]
fn cache_metrics_record_hit_miss_and_eviction() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let metrics = Arc::new(ShardMetrics::new(0));
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_budget(3072)
    .with_metrics(metrics.clone());
    for file_page_id in 0..8u64 {
      let mut page = vec![0u8; 512];
      page[0] = file_page_id as u8 + 1;
      cloud.write_page(&tbl, file_page_id, &page).await.unwrap();
    }
    cloud.sync_data(&tbl).await.unwrap();
    assert!(metrics.snapshot().cache_evict >= 1);

    let mut page = vec![0u8; 512];
    cloud.read_page(&tbl, 0, &mut page).await.unwrap();
    assert_eq!(page[0], 1);
    cloud.read_page(&tbl, 0, &mut page).await.unwrap();
    let snapshot = metrics.snapshot();
    assert!(snapshot.cache_miss >= 1);
    assert!(snapshot.cache_hit >= 1);
    assert!(snapshot.cache_bytes > 0);
    assert_eq!(snapshot.cache_bytes, cloud.resident_bytes());
  });
}
