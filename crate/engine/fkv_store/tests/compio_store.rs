use std::fs::{read, write};

use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

#[compio::test]
async fn compio_end_to_end() {
  let root = TempRoot::new("store_compio_e2e");
  let io = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  let mut store = Store::new(io, 512, 16);
  let tbl = TableIdent::new("demo", 1).unwrap();
  let batch = [
    BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
    BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
  ];
  store.put(&tbl, &batch).await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 2);
}

/// issue 48：`Store::with_verify_checksum(false)` 可关闭读 page 的 checksum 校验。
#[compio::test]
async fn store_can_toggle_page_checksum_verify() {
  let root = TempRoot::new("store_compio_checksum");
  let tbl = TableIdent::new("demo", 1).unwrap();
  {
    let io = CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 512, 16);
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
  }
  // 破坏 data page 前缀里的 checksum。
  let path = root.join("demo.1").join("data_0_main_0");
  let mut bytes = read(&path).unwrap();
  bytes[0] ^= 0xff;
  write(&path, &bytes).unwrap();

  {
    let io = CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 512, 16);
    store.reopen(&tbl).await.unwrap();
    assert!(
      store.get(&tbl, b"a").await.is_err(),
      "checksum must be verified"
    );
  }
  {
    let io = CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 512, 16).with_verify_checksum(false);
    store.reopen(&tbl).await.unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  }
}

/// eng-37：`evict_table` 清空内存状态，之后 reopen 能重新加载数据。
#[compio::test]
async fn evict_table_clears_state_and_reopen_reloads() {
  let root = TempRoot::new("store_compio_io");
  let tbl = TableIdent::new("demo", 2).unwrap();
  let io = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  let mut store = Store::new(io, 512, 16);
  store
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

  store.evict_table(&tbl);
  assert!(store.mapper(&tbl).is_none());
  assert!(store.root_id(&tbl).is_none());
  // 状态已卸载：读不到数据（不会误当成空表返回旧值）。
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), None);

  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
}

/// issue 13：`max_open_tables` 超限时按 FIFO 驱逐最久未写的表，重新访问可 reopen。
#[compio::test]
async fn max_open_tables_evicts_lru_and_reloads() {
  let root = TempRoot::new("store_compio_tables");
  let io = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  let mut store = Store::new(io, 512, 16).with_max_open_tables(1);
  let tbl_a = TableIdent::new("demo", 1).unwrap();
  let tbl_b = TableIdent::new("demo", 2).unwrap();
  store
    .put(
      &tbl_a,
      &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
    )
    .await
    .unwrap();
  store
    .put(
      &tbl_b,
      &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1)],
    )
    .await
    .unwrap();
  assert!(
    store.mapper(&tbl_a).is_none(),
    "oldest table must be evicted"
  );
  assert!(store.mapper(&tbl_b).is_some());

  store.reopen(&tbl_a).await.unwrap();
  assert_eq!(store.get(&tbl_a, b"a").await.unwrap(), Some(b"A".to_vec()));
}
