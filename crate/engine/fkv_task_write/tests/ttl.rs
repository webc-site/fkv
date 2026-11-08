use std::borrow::Cow;

use fkv_btree::BatchEntry;
use fkv_task_write::prepare_write_batch;
use fkv_types::WriteOp;

fn with_ttl(key: &[u8], value: &[u8], ts: u64, expire_ts: u64) -> BatchEntry {
  let mut entry = BatchEntry::upsert(key.to_vec(), value.to_vec(), ts);
  entry.expire_ts = expire_ts;
  entry
}

/// sem-55：写前折叠只保留胜者并保持其 expire；TTL upsert 由能看到基页的
/// merge 阶段生成（见 `write/ttl.rs` 的门控用例），不在这里预生成。
#[test]
fn live_upsert_is_kept_for_merge() {
  let entry_li = [with_ttl(b"a", b"A", 1, 100)];
  let batch = prepare_write_batch(&entry_li, 99);
  assert_eq!(batch.expired_num, 0);
  assert_eq!(batch.live_li.len(), 1);
  assert_eq!(batch.live_li[0].op, WriteOp::Upsert);
  assert_eq!(batch.live_li[0].expire_ts, 100);
}

#[test]
fn expired_upsert_is_dropped_and_turned_into_delete() {
  let entry_li = [with_ttl(b"a", b"A", 1, 100)];
  let batch = prepare_write_batch(&entry_li, 100);
  assert_eq!(batch.expired_num, 1);
  assert_eq!(batch.live_li.len(), 1);
  assert_eq!(batch.live_li[0].op, WriteOp::Delete);
  assert_eq!(batch.live_li[0].timestamp, 1);
  // 旧 TTL 索引的删除由主树 merge 按 timestamp 胜负决定：本批胜者若更旧，
  // 旧条目与旧索引必须保留（对齐 C++ change_ts < base_ts）。
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
  assert_eq!(batch.expired_num, 0);
}

/// 同 key 折叠只保留最终胜者：败者的 expire 不会生效，merge 只为胜者收集
/// TTL 索引项（sem-55）。
#[test]
fn same_key_folds_to_latest_ttl_winner() {
  let entry_li = [
    with_ttl(b"b", b"B", 1, 200),
    with_ttl(b"a", b"A", 1, 100),
    with_ttl(b"a", b"A2", 2, 300),
  ];
  let batch = prepare_write_batch(&entry_li, 0);
  let key_li: Vec<&[u8]> = batch
    .live_li
    .iter()
    .map(|entry| entry.key.as_slice())
    .collect();
  assert_eq!(key_li, vec![&b"a"[..], &b"b"[..]]);
  let winner_a = batch
    .live_li
    .iter()
    .find(|entry| entry.key.as_slice() == b"a")
    .unwrap();
  assert_eq!(winner_a.expire_ts, 300, "败者 expire=100 不得生效");
}

/// 胜者为删除时不得留下同一 key 的 TTL upsert（否则到期会误删重建的 key）。
#[test]
fn delete_winner_folds_expired_upsert_to_delete() {
  let entry_li = [
    with_ttl(b"a", b"old", 1, 500),
    with_ttl(b"a", b"expired", 2, 100),
  ];
  let batch = prepare_write_batch(&entry_li, 200);
  assert_eq!(batch.live_li.len(), 1);
  assert_eq!(batch.live_li[0].op, WriteOp::Delete);
}

/// 显式 delete 胜出：主树折叠为 delete，TTL upsert 不可能出现。
#[test]
fn explicit_delete_winner_folds_to_delete() {
  let mut delete = BatchEntry::delete(b"a".to_vec(), 3);
  delete.expire_ts = 400;
  let entry_li = [with_ttl(b"a", b"old", 2, 500), delete];
  let batch = prepare_write_batch(&entry_li, 200);
  assert_eq!(batch.live_li.len(), 1);
  assert_eq!(batch.live_li[0].op, WriteOp::Delete);
}
