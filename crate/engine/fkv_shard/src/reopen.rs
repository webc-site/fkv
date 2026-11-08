//! manifest 暂不可见时的延迟 reopen 队列与到期重试。

use std::{
  cmp::Ordering as CmpOrdering,
  collections::{BinaryHeap, VecDeque},
  mem::take,
  time::{Duration, Instant},
};

use fkv_api::Request;
use fkv_error::KvError;
use fkv_task_reopen::AutoReopenOption;
use fkv_types::TableIdent;

use crate::{
  dispatch::handle,
  lifecycle::ShardState,
  sender::Envelope,
  standby::{SyncAdmit, admit_standby_sync},
};

/// 挂起的等 manifest 条目：manifest 暂不可见时延迟尝试一次 `reopen`，期间 shard 继续
/// 服务其它请求。同表只保留一条（`park` 合并 waiter），对齐 C++
/// `PendingReopenState` + `delayed_requests_` 堆：driver 每次**只尝试一次**，
/// 失败即连坐全部 waiter（`CompleteReopenWaiters(err)`），不由 driver 自己重试。
pub(crate) struct PendingReopen {
  tbl: TableIdent,
  /// 下次尝试时间（堆排序键）。
  due: Instant,
  /// 等 manifest 可见的请求（读请求，以及 driver 之后到达的 `Reopen`）。
  waiter_li: Vec<Envelope>,
}

impl PendingReopen {
  /// driver 拿到 manifest：waiter 交回主循环重新投递（外部 `Reopen` 也只作为
  /// waiter 搭车，成功时随读请求一起被重排、由 `Request::Reopen` 正常回执）。
  pub(crate) fn wake(self, wake_li: &mut VecDeque<Envelope>) {
    wake_li.extend(self.waiter_li);
  }

  /// 等不到 manifest（driver 单次尝试失败 / detach / shutdown）：全部 waiter
  /// 同错（对齐 C++ `CompleteReopenWaiters(err)`），不重排、不再延迟。
  /// 调用方可能已取消，忽略发送失败。
  fn fail(self, err: KvError) {
    for envelope in self.waiter_li {
      let _ = envelope.reply.send(Err(err));
    }
  }
}

/// 延迟堆项：`Ord` 只比较 `due` 并反转，使 `BinaryHeap::peek` 取最早到期。
/// 堆内元素的其它字段（waiter 列表）不参与排序，调整它们不影响堆序。
pub(crate) struct DelayedReopen {
  due: Instant,
  pub(crate) entry: PendingReopen,
}

impl PartialEq for DelayedReopen {
  fn eq(&self, other: &Self) -> bool {
    self.due == other.due
  }
}

impl Eq for DelayedReopen {}

impl PartialOrd for DelayedReopen {
  fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
    Some(self.cmp(other))
  }
}

impl Ord for DelayedReopen {
  fn cmp(&self, other: &Self) -> CmpOrdering {
    // 反转：`BinaryHeap` 是最大堆，反转后 peek 得到最早到期条目（最小堆语义）。
    other.due.cmp(&self.due)
  }
}

/// 调度让路（分区写占用 / `Busy`）时的顺延下限。
///
/// `pending_time == 0` 表示「立即尝试一次」（对齐 C++ `EnqueueDelayedReopenRequest`
/// 的 `delay_us == 0`：直接进 pending 队列，不进延迟堆）。但顺延分支不能以
/// 0 延迟把条目原地重入堆：`due <= now` 立即再次成立，而循环体内没有会返回
/// `Pending` 的 await，事件循环会整 shard 忙转（io-26）。顺延至少让出这么
/// 长时间，保证主循环回到等待集、持有分区 `Rc` 的读任务有机会被 poll。
const MIN_REOPEN_RETRY_DELAY: Duration = Duration::from_millis(1);

/// 等 manifest 可见的延迟重试队列。
///
/// 一个表最多一条：新的 `ResourceMissing` 请求合并进同表条目，不新增延迟项
/// （对齐 C++ `pending_reopens_` 每表一个状态 + delayed heap 一次提升）。
pub(crate) struct ReopenQueue {
  pub(crate) heap: BinaryHeap<DelayedReopen>,
  /// 每请求 auto-reopen 轮数种子（对齐 C++ `auto_reopen_retry_times` 语义）。
  retry_times: u8,
  pending_time: Duration,
}

impl ReopenQueue {
  pub(crate) fn new(option: AutoReopenOption) -> Self {
    Self {
      heap: BinaryHeap::new(),
      retry_times: option.retry_times,
      pending_time: option.pending_time,
    }
  }

  /// 「调度让路」的顺延时长：`pending_time` 与下限取大。
  ///
  /// 首次挂起仍按 `pending_time`（0 = 下一轮立即尝试），只有 Busy / 写占用
  /// 的顺延需要下限，避免 0 时长重入堆形成无 await 自旋（io-26）。
  fn retry_delay(&self) -> Duration {
    self.pending_time.max(MIN_REOPEN_RETRY_DELAY)
  }

  /// 最早到期时间；无挂起条目时为 `None`（主循环据此决定是否带超时等待）。
  pub(crate) fn next_due(&self) -> Option<Instant> {
    self.heap.peek().map(|delayed| delayed.due)
  }

  /// 登记一条遇 `ResourceMissing` 的请求。
  ///
  /// - 读请求消耗本轮预算；预算耗尽立即回执，不挂起（对齐 C++ `OnTaskFinished`
  ///   先判 `reopen_retry_remaining_ > 0` 再决定是否入 waiter）。
  /// - 同表已有条目：挂成 waiter（对齐 C++ `EnqueueReopenWaiter`），不重复
  ///   replay。
  /// - 无条目且是外部 `Reopen`：**不建 driver**，立即以 `ResourceMissing` 回执
  ///   （对齐 C++ 外部 `ReopenRequest` 一次即回，不在 fkv 内挂满
  ///   `retry_times × pending_time`）。
  /// - 无条目且是读请求：建内部 driver，按 `pending_time` 延迟尝试一次。
  ///
  /// 返回 `Some(envelope)` 表示不挂起（预算为 0 / 外部 Reopen 无 driver），
  /// 调用方按原语义立即回执。
  pub(crate) fn park(&mut self, mut envelope: Envelope) -> Option<Envelope> {
    let Some(tbl) = envelope.req.table().cloned() else {
      // 只有防御性：挂起集合里的请求都带分区。
      return Some(envelope);
    };
    let external_reopen = is_external_reopen(&envelope.req);
    if !external_reopen && !envelope.take_reopen_retry(self.retry_times) {
      return Some(envelope);
    }
    let envelope = self.merge_waiter(&tbl, envelope)?;
    if external_reopen {
      return Some(envelope);
    }
    // 首次尝试按配置延迟；`0` = 下一轮立即尝试一次（对齐 C++ `delay_us == 0`
    // 的「立即」语义）。Busy / 写占用的顺延另有下限，见 [`Self::retry_delay`]。
    let due = Instant::now() + self.pending_time;
    self.heap.push(DelayedReopen {
      due,
      entry: PendingReopen {
        tbl,
        due,
        waiter_li: vec![envelope],
      },
    });
    None
  }

  /// 把请求合并进同表条目的 waiter；没有该表条目时原样返回。
  ///
  /// `BinaryHeap` 不支持 `iter_mut`，用 `into_vec` / `from` 重建（延迟堆规模 =
  /// 挂起表数，通常 0~1，重建成本可忽略）。
  fn merge_waiter(&mut self, tbl: &TableIdent, envelope: Envelope) -> Option<Envelope> {
    let mut delayed_li = take(&mut self.heap).into_vec();
    let result = match delayed_li
      .iter_mut()
      .find(|delayed| delayed.entry.tbl == *tbl)
    {
      Some(delayed) => {
        delayed.entry.waiter_li.push(envelope);
        None
      }
      None => Some(envelope),
    };
    self.heap = BinaryHeap::from(delayed_li);
    result
  }

  /// 把同表条目的到期时间提前到 `now`（io-35）：standby 追赶任务完成、结果
  /// 已就绪时调用，让 driver 的下一轮 `run_due_reopen` 立即重试消费结果，
  /// 而不是等一整轮 [`Self::retry_delay`]（生产默认 `pending_time` = 10s）。
  ///
  /// 只提前已有条目的 `due`，不新增条目、不改预算、不动 `retry_delay` 的
  /// io-26 下限语义：提前后 driver 走完整 `admit → Ready → dispatch_reopen`
  /// 消费路径，一次重试要么成功消费、要么按「driver 只尝试一次」连坐 waiter
  /// （不再重入堆），要么在新的 `Busy` / 写占用顺延分支上重入堆——每条支路
  /// 都以 `now + retry_delay()`（≥ 1ms）重新入堆，不会形成忙转。
  ///
  /// `BinaryHeap` 不支持定位修改（`due` 是排序键），用 `into_vec` 重建，
  /// 规模 = 挂起表数（通常 0~1），成本可忽略。无同表条目时为 no-op。
  pub(crate) fn bump_due(&mut self, tbl: &TableIdent, now: Instant) {
    let mut delayed_li = take(&mut self.heap).into_vec();
    for delayed in &mut delayed_li {
      if delayed.entry.tbl == *tbl {
        delayed.due = now;
        delayed.entry.due = now;
      }
    }
    self.heap = BinaryHeap::from(delayed_li);
  }

  /// 以指定错误结束全部挂起条目（detach / shutdown 清理）。
  pub(crate) fn fail_all(&mut self, err: KvError) {
    for delayed in self.heap.drain() {
      delayed.entry.fail(err);
    }
  }
}

/// 外部 `Reopen`（含空 tag 的 `ReopenArchive`）：C++ 里由调用方自己退避重试，
/// shard 不把它当 driver 挂起。
///
/// `Reopen` 的静态属性来自 `Request::active_reopen`；`ReopenArchive` 是否算
/// 外部重开取决于运行期 tag（空 tag = 重开活跃快照，对齐 `Reopen`）。
fn is_external_reopen(req: &Request) -> bool {
  req.active_reopen() || matches!(req, Request::ReopenArchive { tag, .. } if tag.is_empty())
}

/// 跑到期的延迟重新打开尝试（每个条目**只尝试一次**）：
/// 成功则重排 waiter；失败（含仍 `ResourceMissing`）立即连坐 driver 与 waiter，
/// 对齐 C++ `CompleteReopenWaiters(err)`。整轮不阻塞在途。
///
/// 只重试 `ResourceMissing`、不用 [`KvError::is_retryable`]：这是 manifest 可见性
/// 等待，而非 `TryAgain/OpenFileLimit` 的瞬时 IO 错误分类。`Busy`（分区
/// `Store` 仍被在飞读持有）同样只是调度让路，顺延到下一轮，不消耗重试预算。
pub(crate) async fn run_due_reopen(
  state: &mut ShardState,
  reopen_li: &mut ReopenQueue,
  wake_li: &mut VecDeque<Envelope>,
) {
  while reopen_li
    .next_due()
    .is_some_and(|due| due <= Instant::now())
  {
    let Some(mut delayed) = reopen_li.heap.pop() else {
      break;
    };
    if state.stores.is_writing(&delayed.entry.tbl) {
      // 目标分区写进行中（Store 在写任务手里）：顺延一轮，等写完成后再试。
      // 这是调度让路，不是重试预算：不消耗请求的 auto-reopen 轮数。
      // 顺延带下限（io-26）：`pending_time == 0` 时 0 延迟会让本循环在同一轮
      // 里无 await 地反复重试，整 shard 忙转。
      delayed.due = Instant::now() + reopen_li.retry_delay();
      delayed.entry.due = delayed.due;
      reopen_li.heap.push(delayed);
      continue;
    }
    // manifest 已由其它路径（另一条读的懒开 / 外部 Reopen）变为可见：
    // driver 的目的已达成，直接唤醒 waiter。既省一次 replay，也避免读任务
    // 仍持有 `Rc` 时 `store_mut` 返回 Busy 而把唤醒拖到下一轮（io-08）。
    if state
      .table_open
      .ready(&state.stores, &delayed.entry.tbl, state.term, &state.branch)
    {
      delayed.entry.wake(wake_li);
      continue;
    }
    // standby 追赶任务化（io-33）：driver 的 Reopen 也不在事件循环内联等
    // rsync——需要追赶时派发任务（或合并在飞任务）并顺延，完成后的重试经
    // `dispatch_reopen` 消费结果。顺延带下限（io-26）：0 时长会立即再次到期，
    // 而追赶完成前本循环没有会推进的 await，形成忙转。
    if state.standby.is_some()
      && matches!(
        admit_standby_sync(state, &delayed.entry.tbl),
        SyncAdmit::Parked
      )
    {
      delayed.due = Instant::now() + reopen_li.retry_delay();
      delayed.entry.due = delayed.due;
      reopen_li.heap.push(delayed);
      continue;
    }
    let req = Request::Reopen {
      tbl: delayed.entry.tbl.clone(),
      // 延迟队列只服务非 clean 的 manifest 可见性等待。
      clean: false,
    };
    match handle(state, &req).await {
      Ok(_) => delayed.entry.wake(wake_li),
      // 目标分区仍有在飞读（读窗口按预算退出后主循环可带着读到达这里，
      // `store_mut` 拿不到唯一 `Rc`）：Busy 是调度让路而非 reopen 失败，
      // 顺延一轮，不能连坐 waiter、也不消耗请求的 auto-reopen 轮数。
      // 顺延带下限（io-26）：0 时长会立即再次到期，而本循环体内没有会返回
      // `Pending` 的 await，持 `Rc` 的读任务永远得不到 poll，形成自锁。
      Err(KvError::Busy) => {
        delayed.due = Instant::now() + reopen_li.retry_delay();
        delayed.entry.due = delayed.due;
        reopen_li.heap.push(delayed);
      }
      // driver 只尝试一次：失败即让全部 waiter 拿到同一错误，不重排队。
      Err(err) => delayed.entry.fail(err),
    }
  }
}
