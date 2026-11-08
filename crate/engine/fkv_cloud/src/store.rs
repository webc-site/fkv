//! `CloudStore` 构造、配置投影与只读访问器。

use core::cell::{Cell, RefCell};
use std::sync::Arc;

use fkv_metrics::ShardMetric;
use fkv_types::{DEFAULT_SEGMENT_SIZE, DEFAULT_SEGMENTS_PER_FILE_SHIFT};
use gxhash::{HashMap, HashSet};

use crate::{
  CloudStore,
  cache::{CacheState, TableCache},
  group::{CacheGroup, OwnerLease},
  object_key::FileIdentity,
};

impl<I, O> CloudStore<I, O> {
  pub fn new(local: I, remote: O, prefix: impl Into<String>, pages_per_file_shift: u8) -> Self {
    Self {
      local,
      remote,
      prefix: prefix.into(),
      pages_per_file_shift,
      segment_size: DEFAULT_SEGMENT_SIZE,
      segments_per_file_shift: DEFAULT_SEGMENTS_PER_FILE_SHIFT,
      owner: OwnerLease::new(Arc::new(CacheGroup::new(0))),
      metrics: None,
      state: RefCell::new(CacheState::default()),
      identity: RefCell::new(FileIdentity::default()),
      file_range_map: RefCell::new(HashMap::default()),
      reuse_local_caches: true,
      read_only: false,
      restored_table_set: RefCell::new(HashSet::default()),
      segment_scheme_set: RefCell::new(HashSet::default()),
      inflight_download: RefCell::new(HashMap::default()),
      download_generation: Cell::new(0),
      foreign_file_set: RefCell::new(HashSet::default()),
      evict_epoch: Cell::new(0),
    }
  }
}

impl<I, O> CloudStore<I, O> {
  pub fn with_segment_config(mut self, segment_size: usize, segments_per_file_shift: u8) -> Self {
    self.segment_size = segment_size;
    self.segments_per_file_shift = segments_per_file_shift;
    self
  }
}

impl<I, O> CloudStore<I, O> {
  /// `0` 表示不限制预算。实例独占一组（不与其他 `CloudStore` 共享记账）。
  pub fn with_cache_budget(mut self, cache_budget: u64) -> Self {
    self.owner = OwnerLease::new(Arc::new(CacheGroup::new(cache_budget)));
    self
  }
}

impl<I, O> CloudStore<I, O> {
  /// 与其他实例共享同一预算组：整组共用一个 `limit`，任一实例的驻留 / 淘汰
  /// 都计入同一份全局记账（12d：shard 与运行时侧预热不再各记一份预算）。
  pub fn with_cache_group(mut self, group: Arc<CacheGroup>) -> Self {
    self.owner = OwnerLease::new(group);
    self
  }
}

impl<I, O> CloudStore<I, O> {
  /// 本实例挂的预算组；装配点自检 / 测试用（`Arc::ptr_eq` 证明共享）。
  pub fn cache_group(&self) -> &Arc<CacheGroup> {
    &self.owner.group
  }
}

impl<I, O> CloudStore<I, O> {
  /// 是否复用启动前已存在的本地 cache（对齐 `allow_reuse_local_caches`）。
  ///
  /// 默认复用（`true`）。运行时应按 `KvOption::allow_reuse_local_caches` 显式设置：
  /// `false` 时按表懒恢复会清空该表的本地 data / segment，只从远端重建。
  pub fn with_reuse_local_caches(mut self, allow: bool) -> Self {
    self.reuse_local_caches = allow;
    self
  }
}

impl<I, O> CloudStore<I, O> {
  pub fn reuse_local_caches(&self) -> bool {
    self.reuse_local_caches
  }
}

impl<I, O> CloudStore<I, O> {
  /// 声明本实例是只读实例（预热 / 只读副本）：安装 segment 文件时不启用
  /// 表级 scheme（不做回填）。默认 `false` = 写实例。
  ///
  /// 与 [`Self::with_reuse_local_caches`] 一样由装配点显式接线；跨进程共用
  /// `local_root` 的部署必须对预热 / 只读实例置位，否则回填会按旧尾槽规则
  /// 给对方进程正在写的半装文件盖「完整」标记。
  pub fn with_read_only(mut self, read_only: bool) -> Self {
    self.read_only = read_only;
    self
  }
}

impl<I, O> CloudStore<I, O> {
  pub fn read_only(&self) -> bool {
    self.read_only
  }
}

impl<I, O> CloudStore<I, O> {
  pub fn with_metrics(mut self, metrics: Arc<ShardMetric>) -> Self {
    self.metrics = Some(metrics);
    self
  }
}

impl<I, O> CloudStore<I, O> {
  /// 挂在本实例上的指标实例（`None` = 没接线，四项 cache 指标恒 0）。
  ///
  /// 供**装配点自检**用（cld-55）：shard 侧必须把本 shard 的
  /// `Arc<ShardMetric>` 传进来，测试断言 `Arc::ptr_eq` 而不是只看计数 ——
  /// 「没接线」与「cache 一直没命中」在计数上长得一样。
  pub fn metrics(&self) -> Option<&Arc<ShardMetric>> {
    self.metrics.as_ref()
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn record_cache_hit(&self) {
    if let Some(metrics) = &self.metrics {
      metrics.inc_cache_hit();
    }
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn record_cache_miss(&self) {
    if let Some(metrics) = &self.metrics {
      metrics.inc_cache_miss();
    }
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn sync_cache_bytes(&self) {
    if let Some(metrics) = &self.metrics {
      metrics.set_cache_bytes(self.resident_bytes());
    }
  }
}

impl<I, O> CloudStore<I, O> {
  pub fn cache_budget(&self) -> u64 {
    self.owner.limit()
  }
}

impl<I, O> CloudStore<I, O> {
  /// 本地 cache 还能再装多少字节；预算为 0（不限制）时返回 `u64::MAX`。
  /// 对齐 C++ `CloudStoreMgr::LocalCacheRemained()`：预热在下载前用它判断
  /// 下一个文件装不装得下。共享预算组时按**整组**余量计算，预热不会把
  /// shard 已占用的额度再算一遍。
  pub fn cache_remained(&self) -> u64 {
    let limit = self.owner.limit();
    if limit == 0 {
      u64::MAX
    } else {
      limit.saturating_sub(self.owner.resident_bytes())
    }
  }
}

impl<I, O> CloudStore<I, O> {
  pub fn resident_bytes(&self) -> u64 {
    self.state.borrow().resident_bytes
  }
}

impl<I, O> CloudStore<I, O> {
  pub fn resident_file_count(&self) -> usize {
    self
      .state
      .borrow()
      .table_map
      .values()
      .map(TableCache::len)
      .sum()
  }
}

impl<I, O> CloudStore<I, O> {
  /// 待删队列条目数。坏盘时用于确认失败回队有界，避免内存无界增长。
  pub fn pending_delete_count(&self) -> usize {
    self.state.borrow().pending_delete_li.len()
  }
}

impl<I, O> CloudStore<I, O> {
  pub fn local(&self) -> &I {
    &self.local
  }
}

impl<I, O> CloudStore<I, O> {
  pub fn remote(&self) -> &O {
    &self.remote
  }
}

impl<I, O> CloudStore<I, O> {
  pub fn into_parts(self) -> (I, O) {
    (self.local, self.remote)
  }
}
