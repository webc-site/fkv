//! 单批重复 key 的折叠语义（sem-01）。
//!
//! C++ `BatchWriteTask::SetBatch` 在 debug 构建断言同批 key 严格升序（即唯一），
//! `DataPageBuilder::Add` 也断言 key 相对上一项严格递增；Rust `Store::put_at`
//! 则按 timestamp 折叠同 key：timestamp 大者胜，相等时后写胜。这里钉住该行为
//! （含 `expire_ts`），避免后续重构无意改成 C++ 的 debug 断言或无条件后写覆盖。

use fkv_btree::BatchEntry;
use fkv_io_mem::MemStore;
use fkv_store::{KvEntry, Store};
use fkv_types::TableIdent;

fn upsert(key: &[u8], value: &[u8], timestamp: u64, expire_ts: u64) -> BatchEntry {
  let mut entry = BatchEntry::upsert(key.to_vec(), value.to_vec(), timestamp);
  entry.expire_ts = expire_ts;
  entry
}

#[test]
fn same_batch_duplicate_key_newer_timestamp_wins() {
  pollster::block_on(async {
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let tbl = TableIdent::new("dup_ts", 1).unwrap();
    store
      .put_at(
        &tbl,
        &[upsert(b"a", b"v1", 1, 100), upsert(b"a", b"v2", 2, 200)],
        0,
      )
      .await
      .unwrap();

    assert_eq!(
      store.get_at(&tbl, b"a", 0).await.unwrap(),
      Some(b"v2".to_vec())
    );
    assert_eq!(
      store.scan_entry_at(&tbl, b"", b"", 0, 0).await.unwrap(),
      vec![KvEntry {
        key: b"a".to_vec(),
        value: b"v2".to_vec(),
        timestamp: 2,
        expire_ts: 200,
      }]
    );
  });
}

#[test]
fn same_batch_duplicate_key_equal_timestamp_last_wins() {
  pollster::block_on(async {
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let tbl = TableIdent::new("dup_eq_ts", 1).unwrap();
    store
      .put_at(
        &tbl,
        &[upsert(b"a", b"v1", 5, 100), upsert(b"a", b"v2", 5, 200)],
        0,
      )
      .await
      .unwrap();

    assert_eq!(
      store.get_at(&tbl, b"a", 0).await.unwrap(),
      Some(b"v2".to_vec())
    );
    assert_eq!(
      store.scan_entry_at(&tbl, b"", b"", 0, 0).await.unwrap(),
      vec![KvEntry {
        key: b"a".to_vec(),
        value: b"v2".to_vec(),
        timestamp: 5,
        expire_ts: 200,
      }]
    );
  });
}

#[test]
fn same_batch_duplicate_key_delete_and_upsert_fold_by_timestamp() {
  pollster::block_on(async {
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let tbl = TableIdent::new("dup_delete", 1).unwrap();
    // 同一批内两种先后顺序都按 timestamp 裁决，不看批内位置：
    // - a：更新的 delete 胜过旧 upsert；
    // - b：更新的 upsert 胜过旧 delete，且胜者的 expire_ts 生效。
    store
      .put_at(
        &tbl,
        &[
          upsert(b"a", b"old", 6, 300),
          BatchEntry::delete(b"a".to_vec(), 7),
          BatchEntry::delete(b"b".to_vec(), 6),
          upsert(b"b", b"live", 7, 400),
        ],
        0,
      )
      .await
      .unwrap();

    assert_eq!(store.get_at(&tbl, b"a", 0).await.unwrap(), None);
    assert_eq!(
      store.get_at(&tbl, b"b", 0).await.unwrap(),
      Some(b"live".to_vec())
    );
    assert_eq!(
      store.scan_entry_at(&tbl, b"", b"", 0, 0).await.unwrap(),
      vec![KvEntry {
        key: b"b".to_vec(),
        value: b"live".to_vec(),
        timestamp: 7,
        expire_ts: 400,
      }]
    );
  });
}

#[test]
fn cross_put_duplicate_key_stale_timestamp_does_not_rollback() {
  pollster::block_on(async {
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let tbl = TableIdent::new("dup_stale", 1).unwrap();
    store
      .put_at(
        &tbl,
        &[upsert(b"a", b"new", 5, 0), upsert(b"b", b"live", 5, 0)],
        0,
      )
      .await
      .unwrap();
    // 第二次 put 的 timestamp 更旧：旧值不能覆盖新值，旧 expire_ts 也不生效。
    store
      .put_at(
        &tbl,
        &[
          upsert(b"a", b"stale", 3, 999),
          upsert(b"b", b"stale", 4, 999),
        ],
        0,
      )
      .await
      .unwrap();

    assert_eq!(
      store.get_at(&tbl, b"a", 0).await.unwrap(),
      Some(b"new".to_vec())
    );
    assert_eq!(
      store.get_at(&tbl, b"b", 0).await.unwrap(),
      Some(b"live".to_vec())
    );
    assert_eq!(
      store.scan_entry_at(&tbl, b"", b"", 0, 0).await.unwrap(),
      vec![
        KvEntry {
          key: b"a".to_vec(),
          value: b"new".to_vec(),
          timestamp: 5,
          expire_ts: 0,
        },
        KvEntry {
          key: b"b".to_vec(),
          value: b"live".to_vec(),
          timestamp: 5,
          expire_ts: 0,
        },
      ]
    );
  });
}
