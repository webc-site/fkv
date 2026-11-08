use fkv_btree::BatchEntry;
use fkv_error::KvError;
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

/// eng-50：同一批内重复 key 按 timestamp 取较新者，后出现的旧写入不能覆盖新值。
#[test]
fn duplicate_key_in_batch_keeps_newer_timestamp() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"k".to_vec(), b"new".to_vec(), 5),
          BatchEntry::upsert(b"k".to_vec(), b"old".to_vec(), 3),
        ],
      )
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, b"k").await.unwrap(), Some(b"new".to_vec()));
  });
}

/// rev-02：写入口整批校验 key 非空且不超过 `page_size / 2`（对齐 C++ `SetBatch`）。
#[test]
fn key_len_bounds_reject_whole_batch() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let max_len = 512 / 2;

    // 空 key：拒绝。
    assert_eq!(
      store
        .put(&tbl, &[BatchEntry::upsert(Vec::new(), b"v".to_vec(), 1)])
        .await,
      Err(KvError::InvalidArgs)
    );

    // 恰好 page_size/2：接受且可读回。
    let edge = vec![b'k'; max_len];
    store
      .put(&tbl, &[BatchEntry::upsert(edge.clone(), b"v".to_vec(), 1)])
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, &edge).await.unwrap(), Some(b"v".to_vec()));

    // 超长 key：整批拒绝，同批的合法 key 也不落盘。
    let too_long = vec![b'x'; max_len + 1];
    assert_eq!(
      store
        .put(
          &tbl,
          &[
            BatchEntry::upsert(b"ok".to_vec(), b"1".to_vec(), 2),
            BatchEntry::upsert(too_long.clone(), b"2".to_vec(), 2),
          ],
        )
        .await,
      Err(KvError::InvalidArgs)
    );
    assert_eq!(store.get(&tbl, b"ok").await.unwrap(), None);
    assert_eq!(store.get(&tbl, &too_long).await.unwrap(), None);
  });
}
