//! 映射代际来源：单个 Store 内单调递增、永不复用。

use std::sync::{
  Arc,
  atomic::{AtomicU64, Ordering as AtomicOrdering},
};

/// 映射代际来源：**单个 Store 内**单调递增，任何 mapper 化身 / 任何一次映射
/// 变更都取新值。
///
/// 代际是索引缓存的键，语义必须是「同一个值 ⇒ 同一份映射」。旧实现用
/// 「本实例内 +1、abort 回退、重建归 0」，同一数值会在不同时间对应不同映射：
/// 批次回滚后新批次又走到同一个数值，在途旧快照回填的节点于是可能命中新读
/// 者（r7a-core-01/02）。改成 Store 级取值后，回滚只让映射回到旧内容、代际
/// 继续前进，被回滚的取值永不复用；reopen / branch 重建的 mapper 化身继续
/// 消费同一来源，「同值 ⇒ 同映射」在本 Store 内恒成立。
///
/// 唯一性不需要跨 Store：快照（[`crate::SnapshotRegistry`]）与索引缓存
/// （`fkv_btree::IndexCache`）都是 per-Store 状态，代际比较只发生在同一 Store
/// 的 mapper / snapshot / cache 之间；跨 Store 共享的只有字节账本，不含代际。
/// 每个 Store（每个 shard 分区）各持一个来源，写路径不再争进程级同一 cacheline。
#[derive(Debug, Clone)]
pub struct GenerationSource {
  next: Arc<AtomicU64>,
}

impl Default for GenerationSource {
  fn default() -> Self {
    Self::new()
  }
}

impl GenerationSource {
  /// 从 1 开始的新序列；`clone` 共享同一序列。
  pub fn new() -> Self {
    Self {
      next: Arc::new(AtomicU64::new(1)),
    }
  }

  /// 取一个从未使用过的映射代际。
  ///
  /// 单线程 shard 模型下取值不承担同步语义，`Relaxed` 即可：只需要唯一性，
  /// 由 `fetch_add` 保证。u64 回绕在物理上不可达。
  pub(crate) fn next_generation(&self) -> u64 {
    self.next.fetch_add(1, AtomicOrdering::Relaxed)
  }

  /// 已发出的最大代际（不消耗新值）；尚未发出任何值时返回 0。
  ///
  /// Pooled 页 retire 时用它记录「释放时刻最后一笔可能引用该页的快照代际」：
  /// 之后才打开的 snapshot 代际更大，不可能引用该页，因此可以用
  /// `retired_gen < min_live_generation` 精确判定回收时机。
  pub(crate) fn last_issued(&self) -> u64 {
    self.next.load(AtomicOrdering::Relaxed).saturating_sub(1)
  }
}
