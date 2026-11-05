//! per-partition FIFO 写队列与全局写并发门。
//!
//! 对齐 C++ `PendingWriteQueue` + `TryStartPendingWrite` + `max_write_concurrency_`：
//! 同一分区同一时刻只有一个写任务在跑（`running`），不同分区互相独立；全局在跑
//! 写数达到 `max_write_concurrency` 时，队首留在队列里，等已跑任务 [`finish`] 释放
//! 额度后再启动。
//!
//! 高优先 Put 组由 `WriteFlight` 按分区派发成独立 task，跨分区真正并行；
//! `running` / `max_concurrency` 同时承担并发额度与背压计数（eng-33a）。
//! 同一分区仍严格 FIFO，队列语义不因并行化改变。
//!
//! [`finish`]: WriteGate::finish

use std::{collections::VecDeque, sync::Arc};

use fkv_metrics::ShardMetric;
use fkv_types::TableIdent;
use gxhash::HashMap;

/// 单分区写队列（对齐 C++ `PendingWriteQueue`）。
struct PendingQueue<T> {
  /// 等待启动的写请求，FIFO。
  pending_li: VecDeque<T>,
  /// 该分区是否已有写任务在跑（对齐 C++ `running_`）。
  running: bool,
}

impl<T> Default for PendingQueue<T> {
  fn default() -> Self {
    Self {
      pending_li: VecDeque::new(),
      running: false,
    }
  }
}

/// per-partition FIFO 写队列 + 全局写并发门。
pub struct WriteGate<T> {
  queue_map: HashMap<TableIdent, PendingQueue<T>>,
  /// 队非空且当前没有在跑条目的分区；`take_next` 只从这里取，避免全表扫描。
  ready_li: VecDeque<TableIdent>,
  /// 全局在跑写数。
  running: usize,
  /// 跨分区排队条目总数（增量维护，`publish` 时 O(1) 上报，不全表求和）。
  pending_li: usize,
  /// 全局在跑写上限；`0` = 不限（对齐 `KvOption::max_write_concurrency` 默认值）。
  max_concurrency: usize,
  /// 队列深度 / 在跑写的 gauge 出口（shard 线程写，Runtime 线程读）。
  metrics: Arc<ShardMetric>,
}

impl<T> WriteGate<T> {
  pub fn new(max_concurrency: usize, metrics: Arc<ShardMetric>) -> Self {
    Self {
      queue_map: HashMap::default(),
      ready_li: VecDeque::new(),
      running: 0,
      pending_li: 0,
      max_concurrency,
      metrics,
    }
  }

  /// 把当前队列深度与在跑数推入 gauge。每次队列结构变更后调用，
  /// 由 shard 线程独占写入，读侧从 `Shard::metrics_snapshot` 取（eng-33b）。
  fn publish(&self) {
    self
      .metrics
      .set_write_queue_depth(u64::try_from(self.pending_li).unwrap_or(u64::MAX));
    self
      .metrics
      .set_write_running(u64::try_from(self.running).unwrap_or(u64::MAX));
  }

  /// 写请求入队。是否可立即启动由 [`take_next`](Self::take_next) 决定。
  pub fn push(&mut self, tbl: TableIdent, item: T) {
    let queue = self.queue_map.entry(tbl.clone()).or_default();
    let was_empty = queue.pending_li.is_empty();
    queue.pending_li.push_back(item);
    self.pending_li += 1;
    // 分区原本就有排队条目时，它已在 `ready_li` 里（running 则不可启动），无需重复登记。
    if was_empty && !queue.running {
      self.ready_li.push_back(tbl);
    }
    self.publish();
  }

  /// 取一条可启动的写请求：分区空闲且全局额度未满。
  pub fn take_next(&mut self) -> Option<(TableIdent, T)> {
    while let Some(tbl) = self.ready_li.pop_front() {
      let Some(queue) = self.queue_map.get_mut(&tbl) else {
        continue;
      };
      if queue.running || queue.pending_li.is_empty() {
        continue;
      }
      if self.max_concurrency != 0 && self.running >= self.max_concurrency {
        // 全局额度已满：放回队首，等 `finish` 释放后重试。
        self.ready_li.push_front(tbl);
        return None;
      }
      let Some(item) = queue.pending_li.pop_front() else {
        continue;
      };
      self.pending_li -= 1;
      queue.running = true;
      self.running += 1;
      self.publish();
      return Some((tbl, item));
    }
    None
  }

  /// 在同一分区写正在运行时，从队首连续取出满足 `predicate` 的条目。
  ///
  /// 遇到第一条不满足的条目立即停止，保持 FIFO；不改变 `running` 状态。
  /// 用于把同分区的多条 `Put` 合并成一次 `Store::put`（group commit）。
  pub fn take_pending_while<F>(&mut self, tbl: &TableIdent, max: usize, mut predicate: F) -> Vec<T>
  where
    F: FnMut(&T) -> bool,
  {
    if max == 0 {
      return Vec::new();
    }
    let Some(queue) = self.queue_map.get_mut(tbl) else {
      return Vec::new();
    };
    let mut taken = Vec::new();
    while taken.len() < max {
      let Some(front) = queue.pending_li.front() else {
        break;
      };
      if !predicate(front) {
        break;
      }
      let Some(item) = queue.pending_li.pop_front() else {
        break;
      };
      taken.push(item);
    }
    self.pending_li = self.pending_li.saturating_sub(taken.len());
    self.publish();
    taken
  }

  /// 是否仍有可启动的排队写。
  ///
  /// `ready_li` 只登记「分区空闲且队非空」的条目，`finish` / `remove_pending`
  /// 负责清理，因此这里只查队列是否为空，不做全表扫描。
  pub fn has_ready(&self) -> bool {
    !self.ready_li.is_empty()
  }

  /// 一条写任务结束：分区置空闲、释放全局额度；同分区仍有排队则重新可启动。
  pub fn finish(&mut self, tbl: &TableIdent) {
    self.running = self.running.saturating_sub(1);
    if let Some(queue) = self.queue_map.get_mut(tbl) {
      queue.running = false;
      if !queue.pending_li.is_empty() {
        self.ready_li.push_back(tbl.clone());
      }
    }
    self.publish();
  }

  /// 丢弃某分区的排队条目（`Drop` 后这些写已无意义）；在跑条目不回收，
  /// 由 `finish` 收尾。
  pub fn remove_pending(&mut self, tbl: &TableIdent) -> Vec<T> {
    self.ready_li.retain(|id| id != tbl);
    let Some(queue) = self.queue_map.get_mut(tbl) else {
      self.publish();
      return Vec::new();
    };
    let pending_li: Vec<T> = queue.pending_li.drain(..).collect();
    self.pending_li = self.pending_li.saturating_sub(pending_li.len());
    if !queue.running {
      self.queue_map.remove(tbl);
    }
    self.publish();
    pending_li
  }

  /// 清空全部排队条目（`DetachShard` 驱逐全部分区），在跑条目不回收。
  pub fn clear(&mut self) -> Vec<T> {
    self.ready_li.clear();
    let pending_li = self
      .queue_map
      .values_mut()
      .flat_map(|queue| queue.pending_li.drain(..))
      .collect();
    self.pending_li = 0;
    self.queue_map.retain(|_, queue| queue.running);
    self.publish();
    pending_li
  }

  /// 排队条目总数（跨分区合计）。
  #[cfg(test)]
  pub fn pending_len(&self) -> usize {
    self.pending_li
  }

  /// 全局在跑写数。
  #[cfg(test)]
  pub fn running_len(&self) -> usize {
    self.running
  }
}

#[cfg(test)]
mod test {
  use std::sync::Arc;

  use fkv_metrics::ShardMetric;
  use fkv_types::TableIdent;

  use super::WriteGate;

  fn tbl(name: &str) -> TableIdent {
    TableIdent::new(name, 1).unwrap()
  }

  /// 带 gauge 出口的 gate；`metrics` 一并返回便于断言上报值。
  fn gate(max: usize) -> (WriteGate<i32>, Arc<ShardMetric>) {
    let metrics = Arc::new(ShardMetric::new(0));
    (WriteGate::new(max, metrics.clone()), metrics)
  }

  #[test]
  fn same_partition_is_fifo_and_exclusive() {
    let (mut gate, metrics) = gate(0);
    let a = tbl("a");
    gate.push(a.clone(), 1);
    gate.push(a.clone(), 2);
    // 两条排队、零在跑：深度 gauge 反映排队长度（eng-33b）。
    assert_eq!(metrics.snapshot().write_queue_depth, 2);
    assert_eq!(metrics.snapshot().write_running, 0);
    assert_eq!(gate.take_next(), Some((a.clone(), 1)));
    assert_eq!(metrics.snapshot().write_queue_depth, 1);
    assert_eq!(metrics.snapshot().write_running, 1);
    // 同分区已有在跑任务，第二条不可启动。
    assert_eq!(gate.take_next(), None);
    assert_eq!(gate.running_len(), 1);
    gate.finish(&a);
    assert_eq!(metrics.snapshot().write_running, 0);
    assert_eq!(gate.take_next(), Some((a.clone(), 2)));
    gate.finish(&a);
    assert_eq!(gate.take_next(), None);
    assert_eq!(gate.running_len(), 0);
    assert_eq!(gate.pending_len(), 0);
    assert_eq!(metrics.snapshot().write_queue_depth, 0);
    assert_eq!(metrics.snapshot().write_running, 0);
  }

  #[test]
  fn take_pending_while_preserves_partition_fifo() {
    let (mut gate, metrics) = gate(0);
    let a = tbl("a");
    for value in 1..=4 {
      gate.push(a.clone(), value);
    }
    assert_eq!(gate.take_next(), Some((a.clone(), 1)));
    // 只取队首连续满足 predicate 的条目；遇到 4 停止，3 留在队列里。
    let batch = gate.take_pending_while(&a, 8, |value| *value < 3);
    assert_eq!(batch, vec![2]);
    assert_eq!(gate.pending_len(), 2);
    assert_eq!(metrics.snapshot().write_queue_depth, 2);
    gate.finish(&a);
    assert_eq!(gate.take_next(), Some((a.clone(), 3)));
    gate.finish(&a);
    assert_eq!(gate.take_next(), Some((a.clone(), 4)));
    gate.finish(&a);
  }

  #[test]
  fn different_partitions_start_independently() {
    let (mut gate, metrics) = gate(0);
    let (a, b) = (tbl("a"), tbl("b"));
    gate.push(a.clone(), 1);
    gate.push(b.clone(), 2);
    let first = gate.take_next().unwrap();
    let second = gate.take_next().unwrap();
    assert_ne!(first.0, second.0);
    assert_eq!(gate.running_len(), 2);
    assert_eq!(metrics.snapshot().write_running, 2);
    assert_eq!(metrics.snapshot().write_queue_depth, 0);
    gate.finish(&first.0);
    gate.finish(&second.0);
    assert_eq!(gate.take_next(), None);
    assert_eq!(metrics.snapshot().write_running, 0);
  }

  #[test]
  fn max_concurrency_blocks_then_resumes() {
    let (mut gate, metrics) = gate(1);
    let (a, b) = (tbl("a"), tbl("b"));
    gate.push(a.clone(), 1);
    gate.push(b.clone(), 2);
    assert_eq!(gate.take_next(), Some((a.clone(), 1)));
    // 上限 1：在跑恒为 1，另一分区留在队列里（深度 = 1）。
    assert_eq!(metrics.snapshot().write_running, 1);
    assert_eq!(metrics.snapshot().write_queue_depth, 1);
    // 额度用尽：另一分区也不能启动。
    assert_eq!(gate.take_next(), None);
    gate.finish(&a);
    let next = gate.take_next().unwrap();
    assert_eq!(next.1, 2);
    assert_eq!(metrics.snapshot().write_running, 1);
    gate.finish(&next.0);
  }

  #[test]
  fn zero_means_unlimited() {
    let (mut gate, metrics) = gate(0);
    for name in ["a", "b", "c"] {
      gate.push(tbl(name), 1);
    }
    for _ in 0..3 {
      assert!(gate.take_next().is_some());
    }
    assert_eq!(gate.running_len(), 3);
    assert_eq!(metrics.snapshot().write_running, 3);
    assert_eq!(gate.take_next(), None);
  }

  #[test]
  fn remove_pending_and_clear() {
    let (mut gate, metrics) = gate(0);
    let (a, b) = (tbl("a"), tbl("b"));
    gate.push(a.clone(), 1);
    gate.push(a.clone(), 2);
    gate.push(b.clone(), 3);
    assert_eq!(metrics.snapshot().write_queue_depth, 3);
    assert_eq!(gate.remove_pending(&a), vec![1, 2]);
    assert_eq!(gate.pending_len(), 1);
    assert_eq!(metrics.snapshot().write_queue_depth, 1);
    assert_eq!(gate.clear(), vec![3]);
    assert_eq!(gate.pending_len(), 0);
    assert_eq!(metrics.snapshot().write_queue_depth, 0);
  }
}
