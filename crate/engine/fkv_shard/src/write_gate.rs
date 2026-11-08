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
  /// 该分区因 standby 追赶挂起：队首条目（[`WriteGate::suspend`] 放回的
  /// 挂起请求）保留在队列里等 [`WriteGate::resume`]，期间 `take_next` 跳过
  /// 本分区，后到的同分区写自然排在其后。全局在跑计数已在挂起时释放
  /// （追赶不占写并发额度），故本标志与 `running` 独立。
  suspended: bool,
}

impl<T> Default for PendingQueue<T> {
  fn default() -> Self {
    Self {
      pending_li: VecDeque::new(),
      running: false,
      suspended: false,
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
    // 分区原本就有排队条目（或在跑 / 已挂起）时，它已在 `ready_li` 里或不该
    // 被登记，无需重复登记。
    if was_empty && !queue.running && !queue.suspended {
      self.ready_li.push_back(tbl);
    }
    self.publish();
  }

  /// 挂起一条**已从队首取出**的写请求（standby 追赶在飞）：把条目按原顺序
  /// 放回该分区队首，分区置挂起并释放它的全局在跑额度。
  ///
  /// 保留写门占用的意义（sem-79）：挂起期间后到的同分区写只能排在挂起条目
  /// 之后（`take_next` 跳过挂起分区），不会被先取出执行；追赶完成后
  /// [`resume`](Self::resume) 让 `take_next` 从队首取回挂起条目，先于后到写。
  /// 不 `finish` 的话重放会撞上自己的占用——这里是放回队首 + 显式恢复，不会。
  pub fn suspend(&mut self, tbl: TableIdent, items: Vec<T>) {
    // `take_next` 已为本次出队 +1：追赶期间释放该全局额度（不占写并发）。
    self.running = self.running.saturating_sub(1);
    let queue = self.queue_map.entry(tbl).or_default();
    queue.running = false;
    queue.suspended = true;
    let added = items.len();
    for item in items.into_iter().rev() {
      queue.pending_li.push_front(item);
    }
    self.pending_li += added;
    self.publish();
  }

  /// 追赶完成：解除分区挂起，队首条目重新可被 `take_next` 取走。
  ///
  /// 不在这里取条目：由调用方随后的 `drain_write_gate` 按正常路径取队首
  /// （挂起条目在后到写之前），保证 FIFO。
  pub fn resume(&mut self, tbl: &TableIdent) {
    let Some(queue) = self.queue_map.get_mut(tbl) else {
      return;
    };
    if !queue.suspended {
      return;
    }
    queue.suspended = false;
    if !queue.pending_li.is_empty() {
      self.ready_li.push_back(tbl.clone());
    }
    self.publish();
  }

  /// 取一条可启动的写请求：分区空闲且全局额度未满。
  pub fn take_next(&mut self) -> Option<(TableIdent, T)> {
    while let Some(tbl) = self.ready_li.pop_front() {
      let Some(queue) = self.queue_map.get_mut(&tbl) else {
        continue;
      };
      if queue.running || queue.suspended || queue.pending_li.is_empty() {
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
  /// 负责清理；挂起分区（`suspended`）不在其中，因此这里只查队列是否为空，
  /// 不做全表扫描。
  pub fn has_ready(&self) -> bool {
    !self.ready_li.is_empty()
  }

  /// 是否仍有一条**当前就能被 [`take_next`](Self::take_next) 取走**的排队写。
  ///
  /// 与 [`has_ready`](Self::has_ready) 的区别只在全局并发额度：额度已满时
  /// `take_next` 会把就绪分区放回队首并返回 `None`（就绪队非空但没有可启动
  /// 项）。事件循环据此决定能否安全 park：额度满时必然有在飞写任务可等
  /// （`running` 与在飞任务一一对应），应落进 `wait_event` 等它完成；额度
  /// 未满而就绪队非空时 `take_next` 一定能取到条目，绝不能在此时 park
  /// （io-32：低优先拒收不派发任务时会无事件源可等）。
  ///
  /// 挂起分区不在 `ready_li`：追赶在飞期间它是等追赶任务完成的事件，不是
  /// 可推进的写，事件循环落进 `wait_event` 是对的（等待集含追赶任务）。
  pub fn has_takeable(&self) -> bool {
    !self.ready_li.is_empty()
      && !(self.max_concurrency != 0 && self.running >= self.max_concurrency)
  }

  /// 当前可启动分区数（一轮最多尝试这么多个不同分区）。
  pub fn ready_len(&self) -> usize {
    self.ready_li.len()
  }

  /// 撤销一次 [`take_next`](Self::take_next)：把条目放回分区队首、分区置空闲，
  /// 并排到就绪队尾稍后再试。
  ///
  /// 只用于「写已出队但目标分区仍有在飞读」的让路：先派发其它分区，避免一条
  /// 慢 scan 把排在后面的跨分区写一起拖住。FIFO 不变（条目仍在原分区队首）。
  /// 放回就绪队尾而非队首：否则 `take_next` 会立刻再次取到同一分区，其它
  /// 就绪分区永远轮不到。
  pub fn unstart(&mut self, tbl: TableIdent, item: T) {
    self.running = self.running.saturating_sub(1);
    if let Some(queue) = self.queue_map.get_mut(&tbl) {
      queue.running = false;
      queue.pending_li.push_front(item);
      self.pending_li += 1;
    }
    // `take_next` 已从就绪队首弹出该分区：这里只重新登记一次。
    self.ready_li.push_back(tbl);
    self.publish();
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

  /// io-09：让路（`unstart`）把条目放回分区队首、撤销 running，并把分区排到
  /// 就绪队尾；同分区 FIFO 与全局在跑计数不漂移。
  #[test]
  fn unstart_preserves_fifo_and_running_count() {
    let (mut gate, metrics) = gate(0);
    let (a, b) = (tbl("a"), tbl("b"));
    gate.push(a.clone(), 1);
    gate.push(a.clone(), 2);
    gate.push(b.clone(), 3);
    let (taken_tbl, taken) = gate.take_next().unwrap();
    assert_eq!(taken_tbl, a);
    assert_eq!(taken, 1);
    assert_eq!(gate.running_len(), 1);
    gate.unstart(taken_tbl, taken);
    assert_eq!(gate.running_len(), 0);
    assert_eq!(metrics.snapshot().write_running, 0);
    assert_eq!(gate.pending_len(), 3);
    // A 让路后排在就绪队尾：先轮到其它分区，再回到 A 的队首条目。
    assert_eq!(gate.take_next(), Some((b.clone(), 3)));
    gate.finish(&b);
    assert_eq!(gate.take_next(), Some((a.clone(), 1)));
    gate.finish(&a);
    assert_eq!(gate.take_next(), Some((a.clone(), 2)));
    gate.finish(&a);
    assert_eq!(gate.take_next(), None);
    assert_eq!(gate.pending_len(), 0);
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

  /// sem-79：standby 追赶挂起（`suspend`）保留分区 FIFO——挂起条目留在队首，
  /// 挂起期间到达的后到写只能排在其后；同时释放全局在跑额度（不占写并发），
  /// 别的分区可照常推进；`resume` 后先到的挂起条目先被取走。
  #[test]
  fn suspend_keeps_partition_fifo_and_frees_global_slot() {
    let (mut gate, metrics) = gate(1);
    let (a, b) = (tbl("a"), tbl("b"));

    // 取走 A 后挂起（等价 standby 追赶准入返回 Parked）。
    gate.push(a.clone(), 1);
    assert_eq!(gate.take_next(), Some((a.clone(), 1)));
    assert_eq!(gate.running_len(), 1);
    gate.suspend(a.clone(), vec![1]);
    // 全局额度释放（追赶不占写并发），但 A 仍在队列里、分区被挂起。
    assert_eq!(gate.running_len(), 0);
    assert_eq!(metrics.snapshot().write_running, 0);
    assert_eq!(gate.pending_len(), 1);
    assert_eq!(metrics.snapshot().write_queue_depth, 1);
    assert!(!gate.has_ready());
    assert_eq!(gate.take_next(), None);

    // 挂起期间后到的同分区写 B：排在挂起条目之后，不可启动。
    gate.push(a.clone(), 2);
    assert_eq!(gate.pending_len(), 2);
    assert_eq!(gate.take_next(), None);

    // 全局额度已释放：别的分区可启动（挂起只阻塞本分区）。
    gate.push(b.clone(), 3);
    assert!(gate.has_ready());
    assert!(gate.has_takeable());
    assert_eq!(gate.take_next(), Some((b.clone(), 3)));
    gate.finish(&b);

    // 追赶完成：恢复分区，先到的挂起条目（1）先于后到写（2）。
    gate.resume(&a);
    assert_eq!(gate.take_next(), Some((a.clone(), 1)));
    gate.finish(&a);
    assert_eq!(gate.take_next(), Some((a.clone(), 2)));
    gate.finish(&a);
    assert_eq!(gate.take_next(), None);
    assert_eq!(gate.pending_len(), 0);
    // `resume` 幂等：已恢复的分区再 resume 不影响就绪状态。
    gate.resume(&a);
    assert!(!gate.has_ready());
  }

  /// sem-79：挂起的是整组（Put envelope_li commit）时按原顺序插回队首，
  /// 与挂起期间到达的后到写保持到达顺序（不反转）。
  #[test]
  fn suspend_group_preserves_order_before_later_write() {
    let (mut gate, _metrics) = gate(0);
    let a = tbl("a");
    gate.push(a.clone(), 1);
    gate.push(a.clone(), 2);
    assert_eq!(gate.take_next(), Some((a.clone(), 1)));
    let extra = gate.take_pending_while(&a, 8, |_| true);
    assert_eq!(extra, vec![2]);
    assert_eq!(gate.pending_len(), 0);

    // 挂起期间到达的后到写。
    gate.push(a.clone(), 3);
    assert_eq!(gate.pending_len(), 1);

    // 整组按原顺序插回队首：队首顺序 1, 2, 3。
    gate.suspend(a.clone(), vec![1, 2]);
    assert_eq!(gate.pending_len(), 3);
    gate.resume(&a);
    assert_eq!(gate.take_next(), Some((a.clone(), 1)));
    gate.finish(&a);
    assert_eq!(gate.take_next(), Some((a.clone(), 2)));
    gate.finish(&a);
    assert_eq!(gate.take_next(), Some((a.clone(), 3)));
    gate.finish(&a);
    assert_eq!(gate.take_next(), None);
  }
}
