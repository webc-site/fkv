//! TTL 索引：key = `expire_ts(8B, big-endian) | user_key`，value 为空。
//!
//! TTL tree 与主树共用同一个 `PageMapper` / manifest mapping，只是 root 不同；
//! 这样 snapshot / log 重放天然同时恢复两棵树。

use std::collections::BTreeMap;

use fkv_btree::BatchEntry;
use fkv_error::{KvError, Result};
use fkv_types::WriteOp;

pub const TTL_TS_BYTES: usize = 8;

pub fn ttl_key(expire_ts: u64, key: &[u8]) -> Vec<u8> {
  let mut out = Vec::with_capacity(TTL_TS_BYTES + key.len());
  out.extend_from_slice(&expire_ts.to_be_bytes());
  out.extend_from_slice(key);
  out
}

pub fn parse_ttl_key(ttl_key: &[u8]) -> Result<(u64, &[u8])> {
  if ttl_key.len() < TTL_TS_BYTES {
    return Err(KvError::Corrupted);
  }
  let ts = u64::from_be_bytes(
    ttl_key[..TTL_TS_BYTES]
      .try_into()
      .map_err(|_| KvError::Corrupted)?,
  );
  Ok((ts, &ttl_key[TTL_TS_BYTES..]))
}

pub fn is_expired(expire_ts: u64, now_ms: u64) -> bool {
  expire_ts != 0 && expire_ts <= now_ms
}

/// 为 upsert 且带 TTL 的条目生成 TTL 索引写入 batch（按 key 去重排序）。
pub fn ttl_upsert_batch(batch: &[BatchEntry]) -> Vec<BatchEntry> {
  prepare_write_batch(batch, 0).ttl_li
}

/// 写前按 `now_ms` 拆分 batch：已过期的 upsert 不落主树，只生成对应 TTL
/// 索引删除；已过期的 delete 保留删除动作并清 TTL 索引。TTL 索引按 key
/// 去重排序，同一 key 后出现的操作覆盖先出现的。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreparedWriteBatch {
  pub live_li: Vec<BatchEntry>,
  pub ttl_li: Vec<BatchEntry>,
  pub expired_num: usize,
}

pub fn prepare_write_batch(batch: &[BatchEntry], now_ms: u64) -> PreparedWriteBatch {
  let mut ttl_map: BTreeMap<Vec<u8>, BatchEntry> = BTreeMap::new();
  let mut live_li = Vec::with_capacity(batch.len());
  let mut expired_num = 0;
  for entry in batch {
    if is_expired(entry.expire_ts, now_ms) {
      expired_num += 1;
      let key = ttl_key(entry.expire_ts, &entry.key);
      ttl_map.insert(key.clone(), BatchEntry::delete(key, now_ms));
      if entry.op == WriteOp::Upsert {
        continue;
      }
    } else if entry.op == WriteOp::Upsert && entry.expire_ts != 0 {
      let key = ttl_key(entry.expire_ts, &entry.key);
      ttl_map.insert(key.clone(), BatchEntry::upsert(key, Vec::new(), 0));
    }
    live_li.push(entry.clone());
  }
  PreparedWriteBatch {
    live_li,
    ttl_li: ttl_map.into_values().collect(),
    expired_num,
  }
}
