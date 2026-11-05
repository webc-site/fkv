//! TTL 索引：key = `expire_ts(8B, big-endian) | user_key`，value 为空。
//!
//! TTL tree 与主树共用同一个 `PageMapper` / manifest mapping，只是 root 不同；
//! 这样 snapshot / log 重放天然同时恢复两棵树。

use std::{
  borrow::Cow,
  collections::{BTreeMap, btree_map::Entry},
};

use fkv_btree::BatchEntry;
use fkv_error::{KvError, Result};
use fkv_types::WriteOp;
pub use fkv_types::is_expired;

pub(crate) const TTL_TS_BYTES: usize = 8;

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

/// 写前按 `now_ms` 拆分写入批次：已过期的 upsert 不落主树，只生成对应 TTL
/// 索引删除；已过期的 delete 保留删除动作并清 TTL 索引。TTL 索引按 key
/// 去重排序，同一 key 后出现的操作覆盖先出现的。
///
/// 主树同 key 只在**折叠后**出现一次：与 `write_batch` 的 timestamp 折叠
/// 语义一致（相等时后者胜）。胜者若恰是「已过期 upsert」，说明最新写等价于
/// 不存在，产出 delete 覆盖旧值，否则旧的 live 条目会遮蔽这次写入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedWriteBatch<'a> {
  /// 主树写入批次：无 TTL / 无重复 key 时直接借用调用方切片（零拷贝）。
  pub live_li: Cow<'a, [BatchEntry]>,
  pub ttl_li: Vec<BatchEntry>,
  pub expired_num: usize,
}

impl Default for PreparedWriteBatch<'_> {
  fn default() -> Self {
    Self {
      live_li: Cow::Borrowed(&[]),
      ttl_li: Vec::new(),
      expired_num: 0,
    }
  }
}

pub fn prepare_write_batch(batch_li: &[BatchEntry], now_ms: u64) -> PreparedWriteBatch<'_> {
  // 快路径：无 TTL（也就没有过期项）且 key 严格升序（无同 key 折叠）时整批借用。
  if batch_li.iter().all(|entry| entry.expire_ts == 0)
    && batch_li.windows(2).all(|pair| pair[0].key < pair[1].key)
  {
    return PreparedWriteBatch {
      live_li: Cow::Borrowed(batch_li),
      ttl_li: Vec::new(),
      expired_num: 0,
    };
  }
  let mut ttl_map: BTreeMap<Vec<u8>, BatchEntry> = BTreeMap::new();
  let mut expired_num = 0;
  // 主树胜者：同 key 取 timestamp 最大者，相等时后出现的覆盖（对齐 write_batch）。
  let mut winner_map: BTreeMap<&[u8], &BatchEntry> = BTreeMap::new();
  for entry in batch_li {
    if is_expired(entry.expire_ts, now_ms) {
      expired_num += 1;
      // 不在这里无条件删 TTL 索引：本批胜者可能 timestamp 更旧，主树 merge
      // 会保留旧 entry 与旧 TTL 索引（对齐 C++ `change_ts < base_ts` 不走
      // `UpdateTTL`）。统一由 `free_replaced_values` 按 timestamp 胜负决定
      // 是否删旧 TTL；这里生成 delete 会在旧值被保留时把 TTL 索引删空，
      // 导致过期 entry 永不清理。
    }
    // 过期 upsert 也参与折叠：它是「最新写的值」，只是该值等价于不存在。
    match winner_map.entry(entry.key.as_slice()) {
      Entry::Occupied(mut slot) => {
        if entry.timestamp >= slot.get().timestamp {
          slot.insert(entry);
        }
      }
      Entry::Vacant(slot) => {
        slot.insert(entry);
      }
    }
  }
  // 只对折叠后的胜者生成 TTL upsert：
  // - 胜者为删除（显式 delete 或已过期 upsert）时该 key 本批结束后不存在，
  //   不能留下 TTL upsert，否则到期会误删之后重建的同名 key；
  // - 败者（同 key 的其他 live upsert）的 expire_ts 不会生效，为它生成 TTL 项
  //   只会产生多余的 TTL 写入与唤醒（R4-04）。
  for winner in winner_map.values() {
    if winner.op == WriteOp::Upsert
      && winner.expire_ts != 0
      && !is_expired(winner.expire_ts, now_ms)
    {
      let key = ttl_key(winner.expire_ts, &winner.key);
      ttl_map.insert(key.clone(), BatchEntry::upsert(key, Vec::new(), 0));
    }
  }
  let mut live_li = Vec::with_capacity(winner_map.len());
  for winner in winner_map.into_values() {
    if winner.op == WriteOp::Upsert && is_expired(winner.expire_ts, now_ms) {
      live_li.push(BatchEntry::delete(winner.key.as_slice(), winner.timestamp));
    } else {
      live_li.push(winner.clone());
    }
  }
  PreparedWriteBatch {
    live_li: Cow::Owned(live_li),
    ttl_li: ttl_map.into_values().collect(),
    expired_num,
  }
}
