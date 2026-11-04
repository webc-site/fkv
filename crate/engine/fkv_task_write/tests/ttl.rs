use fkv_btree::BatchEntry;
use fkv_task_write::{prepare_write_batch, ttl_key};
use fkv_types::WriteOp;

fn with_ttl(key: &[u8], value: &[u8], ts: u64, expire_ts: u64) -> BatchEntry {
  let mut entry = BatchEntry::upsert(key.to_vec(), value.to_vec(), ts);
  entry.expire_ts = expire_ts;
  entry
}

#[test]
fn live_upsert_writes_data_and_ttl_index() {
  let batch = prepare_write_batch(&[with_ttl(b"a", b"A", 1, 100)], 99);
  assert_eq!(batch.expired_num, 0);
  assert_eq!(batch.live_li.len(), 1);
  assert_eq!(batch.ttl_li.len(), 1);
  assert_eq!(batch.ttl_li[0].key, ttl_key(100, b"a"));
  assert_eq!(batch.ttl_li[0].op, WriteOp::Upsert);
}

#[test]
fn expired_upsert_is_dropped_and_ttl_index_deleted() {
  let batch = prepare_write_batch(&[with_ttl(b"a", b"A", 1, 100)], 100);
  assert_eq!(batch.expired_num, 1);
  assert!(batch.live_li.is_empty());
  assert_eq!(batch.ttl_li.len(), 1);
  assert_eq!(batch.ttl_li[0].key, ttl_key(100, b"a"));
  assert_eq!(batch.ttl_li[0].op, WriteOp::Delete);
}

#[test]
fn expired_delete_still_reaches_data_tree() {
  let mut delete = BatchEntry::delete(b"a".to_vec(), 2);
  delete.expire_ts = 100;
  let batch = prepare_write_batch(&[delete], 100);
  assert_eq!(batch.expired_num, 1);
  assert_eq!(batch.live_li.len(), 1);
  assert_eq!(batch.live_li[0].op, WriteOp::Delete);
  assert_eq!(batch.ttl_li.len(), 1);
  assert_eq!(batch.ttl_li[0].op, WriteOp::Delete);
}

#[test]
fn ttl_index_entries_are_deduplicated_and_sorted() {
  let batch = prepare_write_batch(
    &[
      with_ttl(b"b", b"B", 1, 200),
      with_ttl(b"a", b"A", 1, 100),
      with_ttl(b"a", b"A2", 2, 300),
    ],
    0,
  );
  let key_li: Vec<Vec<u8>> = batch.ttl_li.iter().map(|entry| entry.key.clone()).collect();
  assert_eq!(
    key_li,
    vec![ttl_key(100, b"a"), ttl_key(200, b"b"), ttl_key(300, b"a")]
  );
}
