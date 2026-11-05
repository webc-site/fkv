use std::borrow::Cow;

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
  let entry_li = [with_ttl(b"a", b"A", 1, 100)];
  let batch = prepare_write_batch(&entry_li, 99);
  assert_eq!(batch.expired_num, 0);
  assert_eq!(batch.live_li.len(), 1);
  assert_eq!(batch.ttl_li.len(), 1);
  assert_eq!(batch.ttl_li[0].key, ttl_key(100, b"a"));
  assert_eq!(batch.ttl_li[0].op, WriteOp::Upsert);
}

#[test]
fn expired_upsert_is_dropped_and_ttl_index_deleted() {
  let entry_li = [with_ttl(b"a", b"A", 1, 100)];
  let batch = prepare_write_batch(&entry_li, 100);
  assert_eq!(batch.expired_num, 1);
  assert_eq!(batch.live_li.len(), 1);
  assert_eq!(batch.live_li[0].op, WriteOp::Delete);
  assert_eq!(batch.live_li[0].timestamp, 1);
  // TTL 索引删除由主树 merge 的 `free_replaced_values` 按 timestamp 胜负决定：
  // 本批胜者若更旧，旧 TTL 索引必须保留（对齐 C++ change_ts < base_ts）。
  assert!(batch.ttl_li.is_empty());
}

#[test]
fn expired_delete_still_reaches_data_tree() {
  let mut delete = BatchEntry::delete(b"a".to_vec(), 2);
  delete.expire_ts = 100;
  let entry_li = [delete];
  let batch = prepare_write_batch(&entry_li, 100);
  assert_eq!(batch.expired_num, 1);
  assert_eq!(batch.live_li.len(), 1);
  assert_eq!(batch.live_li[0].op, WriteOp::Delete);
  assert!(batch.ttl_li.is_empty());
}

/// 同批同 key：旧 live 值 + 更新的「已过期 upsert」→ 最新写等价于不存在，
/// 主树必须产出 delete；若沿用旧 live 条目，旧值会遮蔽这次写入。
#[test]
fn expired_upsert_shadows_older_live_value_in_same_batch() {
  let entry_li = [
    with_ttl(b"a", b"old", 1, 500),
    with_ttl(b"a", b"new", 2, 100),
  ];
  let batch = prepare_write_batch(&entry_li, 200);
  assert_eq!(batch.expired_num, 1);
  assert_eq!(batch.live_li.len(), 1);
  assert_eq!(batch.live_li[0].op, WriteOp::Delete);
  assert_eq!(batch.live_li[0].timestamp, 2);
  assert_eq!(batch.live_li[0].expire_ts, 0);
}

/// 反向：更旧的过期 upsert 不能压掉更新的 live 值（timestamp 折叠）。
#[test]
fn older_expired_upsert_does_not_shadow_newer_live_value() {
  let entry_li = [
    with_ttl(b"a", b"new", 2, 500),
    with_ttl(b"a", b"new", 1, 100),
  ];
  let batch = prepare_write_batch(&entry_li, 200);
  assert_eq!(batch.live_li.len(), 1);
  assert_eq!(batch.live_li[0].op, WriteOp::Upsert);
  assert_eq!(batch.live_li[0].timestamp, 2);
  assert_eq!(batch.live_li[0].value, b"new");
}

/// 无 TTL 且 key 严格升序：整批零拷贝借用。
#[test]
fn batch_without_ttl_is_borrowed() {
  let entry_li = [
    BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
    BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2),
  ];
  let batch = prepare_write_batch(&entry_li, 0);
  assert!(matches!(batch.live_li, Cow::Borrowed(_)));
  assert!(batch.ttl_li.is_empty());
}

#[test]
fn ttl_index_entries_are_deduplicated_and_sorted() {
  let entry_li = [
    with_ttl(b"b", b"B", 1, 200),
    with_ttl(b"a", b"A", 1, 100),
    with_ttl(b"a", b"A2", 2, 300),
  ];
  let batch = prepare_write_batch(&entry_li, 0);
  let key_li: Vec<Vec<u8>> = batch.ttl_li.iter().map(|entry| entry.key.clone()).collect();
  // 败者（ts=1, expire=100）不再生成 TTL 项：只有最终胜者 ts=2 的 expire 生效。
  assert_eq!(key_li, vec![ttl_key(200, b"b"), ttl_key(300, b"a")]);
}

/// 胜者为删除时不得留下同一 key 的 TTL upsert（否则到期会误删重建的 key）。
#[test]
fn delete_winner_drops_stale_ttl_upsert() {
  let entry_li = [
    with_ttl(b"a", b"old", 1, 500),
    with_ttl(b"a", b"expired", 2, 100),
  ];
  let batch = prepare_write_batch(&entry_li, 200);
  assert!(batch.ttl_li.is_empty());
}

/// 显式 delete 胜出：旧 TTL 索引删除后不再补 upsert。
#[test]
fn explicit_delete_winner_drops_ttl_upsert() {
  let mut delete = BatchEntry::delete(b"a".to_vec(), 3);
  delete.expire_ts = 400;
  let entry_li = [with_ttl(b"a", b"old", 2, 500), delete];
  let batch = prepare_write_batch(&entry_li, 200);
  assert_eq!(batch.live_li.len(), 1);
  assert_eq!(batch.live_li[0].op, WriteOp::Delete);
  assert!(batch.ttl_li.is_empty());
}
