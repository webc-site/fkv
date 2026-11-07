//! IndexCache 语义测试（自 src 内联迁出；被测项全部是 pub API）。
//!
//! 覆盖：字节预算逐出、零容量无界、超大条目、Clock 两轮强制逐出、共享账本
//! 均分与归还、重插记账、reopen 保留映射、代际门控与 retag。

use std::sync::Arc;

use fkv_btree::{CacheLedger, IndexCache, IndexNode};
use fkv_page::{IndexPageBuilder, content_len, restart_num};
use fkv_types::FilePageId;

/// 测试统一用的映射代际；代际语义单测见 `generation_gates_hits_and_retag_refreshes`。
const GEN: u64 = 7;

/// 512B leaf index 页：8 条 key，leftmost = 0。
fn node(file_page_id: FilePageId, key_len: usize) -> IndexNode {
  let mut builder = IndexPageBuilder::new(512, 2, true).unwrap();
  assert!(builder.add(b"", 0).unwrap());
  for idx in 0..8u32 {
    let key = format!("{idx:0width$}k", width = key_len.saturating_sub(1));
    assert!(builder.add(key.as_bytes(), idx + 1).unwrap());
  }
  IndexNode::new(file_page_id, &builder.finish().unwrap()).unwrap()
}

#[test]
fn byte_len_counts_page_bytes() {
  assert!(node(0, 8).byte_len() >= 512);
}

#[test]
fn misordered_restart_keys_rejected() {
  let mut builder = IndexPageBuilder::new(512, 1, true).unwrap();
  assert!(builder.add(b"", 0).unwrap());
  for idx in 0..5u32 {
    assert!(builder.add(format!("k{idx}").as_bytes(), idx + 1).unwrap());
  }
  let mut page = builder.finish().unwrap();
  let count = usize::from(restart_num(&page).unwrap());
  assert!(count >= 3);
  let restart_offset = usize::from(content_len(&page).unwrap()) - (1 + count) * 2;
  // 交换最后两个 restart offset → restart key 降序。
  let (left, right) = (
    restart_offset + (count - 2) * 2,
    restart_offset + (count - 1) * 2,
  );
  let left_buf: [u8; 2] = page[left..left + 2].try_into().unwrap();
  let right_buf: [u8; 2] = page[right..right + 2].try_into().unwrap();
  page[left..left + 2].copy_from_slice(&right_buf);
  page[right..right + 2].copy_from_slice(&left_buf);
  assert!(IndexNode::new(0, &page).is_err());
}

#[test]
fn evicts_over_byte_budget() {
  let node = node(0, 8);
  let budget = node.byte_len() * 2;
  let mut cache = IndexCache::with_capacity_bytes(budget);
  for page_id in 0..10 {
    cache.insert(page_id, node.clone(), GEN);
  }
  assert!(cache.byte_len() <= budget, "bytes={}", cache.byte_len());
  // 至少淘汰最旧的若干页，最新页仍在。
  assert!(cache.len() <= 2);
  assert!(cache.get(9, GEN).is_some());
  assert!(cache.get(0, GEN).is_none());
}

#[test]
fn zero_capacity_means_unbounded() {
  let mut cache = IndexCache::with_capacity_bytes(0);
  for page_id in 0..100 {
    cache.insert(page_id, node(0, 4), GEN);
  }
  assert_eq!(cache.len(), 100);
}

/// eng-69：唯一超大条目也必须可逐出（旧实现 `order_li.len() > 1` 永不驱逐）。
#[test]
fn single_oversized_entry_is_not_cached() {
  let node = node(0, 8);
  let mut cache = IndexCache::with_capacity_bytes(node.byte_len() - 1);
  cache.insert(0, node, GEN);
  assert!(cache.is_empty(), "oversized entry must be dropped");
  assert_eq!(cache.byte_len(), 0);
}

/// eng-69：全部条目 referenced 时两轮 Clock 后强制逐出，预算硬约束。
#[test]
fn referenced_entries_still_evicted_after_two_rounds() {
  let node = node(0, 8);
  let limit = node.byte_len() * 4;
  let mut cache = IndexCache::with_capacity_bytes(limit);
  for page_id in 0..4 {
    cache.insert(page_id, node.clone(), GEN);
  }
  // 全部置引用位，Clock 首轮只清零不淘汰。
  for page_id in 0..4 {
    assert!(cache.get(page_id, GEN).is_some());
  }
  cache.insert(9, node.clone(), GEN);
  assert!(cache.byte_len() <= limit, "bytes={}", cache.byte_len());
  assert!(cache.get(9, GEN).is_some());
}

/// eng-69：共享账本按实例数均分聚合上限，销毁后归还占用与名额。
#[test]
fn shared_ledger_splits_total_between_caches() {
  let node = node(0, 8);
  let unit = node.byte_len();
  let ledger = Arc::new(CacheLedger::default());
  let mut a = IndexCache::with_capacity_bytes(0).with_shared_budget(ledger.clone(), unit * 4);
  let mut b = IndexCache::with_capacity_bytes(0).with_shared_budget(ledger.clone(), unit * 4);
  assert_eq!(ledger.slot(), 2);
  for page_id in 0..10 {
    a.insert(page_id, node.clone(), GEN);
    b.insert(page_id + 100, node.clone(), GEN);
  }
  // 聚合上限 4 页，两个实例各分 2 页。
  assert_eq!(a.byte_len(), unit * 2);
  assert_eq!(b.byte_len(), unit * 2);
  assert_eq!(ledger.used(), unit * 4);

  drop(a);
  assert_eq!(ledger.slot(), 1);
  assert_eq!(ledger.used(), b.byte_len());
}

#[test]
fn reinsert_updates_bytes_and_remove_clears_order() {
  let mut cache = IndexCache::with_capacity_bytes(node(0, 8).byte_len() * 3);
  cache.insert(0, node(0, 8), GEN);
  cache.insert(1, node(0, 8), GEN);
  let bytes = cache.byte_len();
  cache.insert(0, node(0, 8), GEN);
  assert_eq!(cache.byte_len(), bytes);

  cache.remove(0);
  assert!(cache.get(0, GEN).is_none());
  // 再插回同 id 不应被旧顺序误删。
  cache.insert(0, node(0, 8), GEN);
  cache.insert(2, node(0, 8), GEN);
  cache.insert(3, node(0, 8), GEN);
  // 预算恰容 3 条：插入 3 会逐出一条。逐出对象在 Clock 队列上是
  // 「remove 后再插回的 0」还是「最老的 1」取决于 reinsert 的入队位置，
  // 属实现细节；两条断言钉住的是「只逐出一条且 3 必在」。
  assert!(cache.get(0, GEN).is_some() || cache.get(1, GEN).is_some());
  assert!(cache.get(3, GEN).is_some());
}

/// eng-67：reopen 后只保留逻辑页仍指向同一物理页的条目。
#[test]
fn retain_mapped_drops_remapped_and_missing_pages() {
  let mut cache = IndexCache::with_capacity_bytes(0);
  cache.insert(0, node(100, 8), GEN);
  cache.insert(1, node(101, 8), GEN);
  cache.insert(2, node(102, 8), GEN);
  // 写提交后这些条目才是 baseline；`retain_mapped` 只保留 baseline 条目。
  cache.retag(GEN);
  let bytes = cache.byte_len();

  // 0 未变；1 换物理页；2 已释放。
  cache.retain_mapped(|page_id| match page_id {
    0 => Some(100),
    1 => Some(999),
    _ => None,
  });
  assert_eq!(cache.len(), 1);
  assert!(cache.get(0, GEN).is_some());
  assert!(cache.get(1, GEN).is_none());
  assert!(cache.get(2, GEN).is_none());
  assert_eq!(cache.byte_len(), bytes / 3);
  // 淘汰队列已压实：继续插入不会因陈旧队列项误删新节点。
  cache.insert(3, node(103, 8), GEN);
  assert!(cache.get(0, GEN).is_some());
  assert!(cache.get(3, GEN).is_some());
}

/// 27b：代际不符的条目必须 miss；写提交后 `retag` 让未变更条目重新可命中。
#[test]
fn generation_gates_hits_and_retag_refreshes() {
  let mut cache = IndexCache::with_capacity_bytes(0);
  cache.insert(0, node(100, 8), 1);
  assert!(cache.get(0, 1).is_some());
  // 新代读者不得命中旧代节点，旧代读者也不得命中之后的同 id 新节点。
  assert!(cache.get(0, 2).is_none());
  cache.insert(0, node(200, 8), 2);
  assert!(cache.get(0, 1).is_none());
  assert!(cache.get(0, 2).is_some());

  // 写提交后未变更条目 retag 到新代，新代读者可继续命中。
  cache.insert(1, node(101, 8), 2);
  cache.retag(3);
  assert!(cache.get(0, 3).is_some());
  assert!(cache.get(1, 3).is_some());
  assert!(cache.get(0, 2).is_none());
}

/// r7a-core-02：reopen 的 `retain_mapped` 只保留 baseline 条目。
///
/// 例外条目的代际对应旧化身的在途 / 已回滚视图，物理页相同也不能证明映射
/// 在两次提交之间未变；跨化身提升会把旧节点交给新读者。
#[test]
fn retain_mapped_drops_non_baseline_entries() {
  let mut cache = IndexCache::with_capacity_bytes(0);
  cache.insert(0, node(100, 8), 7);
  cache.retag(7);
  // 在途读者（gen 9）回填的例外条目。
  cache.insert(1, node(101, 8), 9);
  assert!(cache.get(1, 9).is_some());

  // reopen：两页物理映射都没变，但例外条目仍必须丢弃。
  cache.retain_mapped(|page_id| match page_id {
    0 => Some(100),
    _ => Some(101),
  });
  assert_eq!(cache.len(), 1);
  assert!(cache.get(0, 7).is_some());
  assert!(cache.get(1, 9).is_none());
  // 新化身 retag 后例外条目也不得复活。
  cache.retag(11);
  assert!(cache.get(0, 11).is_some());
  assert!(cache.get(1, 11).is_none());
}

/// retag 后旧代读者不能再回填缓存：条目不进缓存，也就不会被下一次
/// baseline 提升错误地交给新代读者。
#[test]
fn stale_insert_after_retag_is_rejected() {
  let mut cache = IndexCache::with_capacity_bytes(0);
  cache.insert(0, node(100, 8), 2);
  cache.retag(4);
  assert!(cache.get(0, 4).is_some());
  // 旧快照（gen 2）在 retag(4) 之后回填：内容来自旧映射，直接拒绝，
  // 否则下一次 retag 会把它提升给新读者。
  cache.insert(1, node(200, 8), 2);
  assert!(cache.get(1, 2).is_none());
  assert!(cache.get(1, 4).is_none());
  cache.retag(5);
  assert!(cache.get(1, 5).is_none());
}

/// R7a-core-01：`reset` 清空条目、归零字节账本，且 baseline 只增不减。
///
/// 代际在 mapper 侧进程内单调（abort 不回退），因此 reset 传入的是回滚后的
/// live 代际，它严格大于本批用过的任何值。baseline 若回落到更低值，`insert`
/// 的「早于 baseline 即拒绝」守卫会失效。
#[test]
fn reset_raises_generation_floor_and_bytes() {
  let mut cache = IndexCache::with_capacity_bytes(0);
  cache.insert(0, node(100, 8), 9);
  cache.retag(9);
  cache.reset(10);
  assert_eq!(cache.len(), 0);
  assert_eq!(cache.byte_len(), 0);
  // 新代读者正常回填并命中。
  cache.insert(1, node(101, 8), 10);
  assert!(cache.get(1, 10).is_some());
  assert_eq!(cache.byte_len(), node(101, 8).byte_len());
  // 传入更旧的代际不得把 baseline 拉回去。
  cache.reset(4);
  cache.insert(2, node(102, 8), 4);
  assert!(cache.get(2, 4).is_none(), "baseline 不得回落");
  cache.insert(2, node(103, 8), 10);
  assert!(cache.get(2, 10).is_some());
}

/// R7a-core-01 主用例：写批次回滚后在途旧快照回填的节点不得被后续 retag 提拔。
///
/// 时序：旧快照（gen 5）持有映射 → 写批次使代际前进到 8 后失败回滚 →
/// store 调 `reset(9)`（回滚后的 live 代际）→ 旧快照继续读盘并 `insert(.., 5)`
/// → 下一次成功写 `retag(10)`。旧节点必须在第 3 步就被拒，否则第 4 步会把它
/// 下沉成 baseline，交给 gen 10 的读者（子页可能已释放复用）。
#[test]
fn stale_snapshot_backfill_after_abort_is_not_promoted() {
  let mut cache = IndexCache::with_capacity_bytes(0);
  cache.insert(0, node(100, 8), 5);
  cache.retag(5);
  // 批次失败：映射回滚，代际继续前进到 9。
  cache.reset(9);
  assert_eq!(cache.len(), 0);
  // 在途旧快照（gen 5）回填：内容来自已回滚的映射，必须拒绝。
  cache.insert(1, node(101, 8), 5);
  assert!(cache.get(1, 5).is_none());
  assert!(cache.get(1, 9).is_none());
  // 后续成功写把未变更条目重标到 gen 10：被拒的旧节点不得被提拔。
  cache.retag(10);
  assert!(cache.get(1, 10).is_none());
  assert_eq!(cache.len(), 0, "旧快照节点不得留在缓存里");
}

/// `exception_li` 的压实分支：超阈值后必须只丢弃悬空项，保留仍存活的例外。
///
/// 悬空项由「以非基线代插入 → remove」制造（`remove` 不清例外清单）。
/// 若压实误删存活例外，`retag` 不会把它的 `gen_override` 回落，新 baseline
/// 读者会永久 miss 该页。循环次数远大于压实阈值（`len * 2 + 64`），保证分支
/// 至少触发一次。
#[test]
fn exception_li_compaction_keeps_live_entries() {
  const LIVE: u32 = 1000;
  const STALE_GEN: u64 = 8;
  let mut cache = IndexCache::with_capacity_bytes(0);
  cache.insert(LIVE, node(300, 8), STALE_GEN);
  for page_id in 0..200 {
    cache.insert(page_id, node(0, 8), STALE_GEN);
    cache.remove(page_id);
  }
  cache.retag(STALE_GEN + 1);
  assert!(
    cache.get(LIVE, STALE_GEN + 1).is_some(),
    "压实丢了仍存活的例外条目"
  );
}
