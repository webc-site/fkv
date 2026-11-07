//! 在飞读任务与槽位记账：`ReadSlot` / `ReadDone` / `ReadTask` / `ReadFlight`。

// 单测探针（FORCE_FULL）用的内存序；生产构建不引用。
#[cfg(test)]
use std::sync::atomic::Ordering;
use std::{
  future::Future,
  pin::Pin,
  rc::Rc,
  sync::{Arc, atomic::AtomicBool},
  task::{Context, Poll},
  time::Instant,
};

use compio::runtime::{self, spawn};
use fkv_api::{Request, Response};
use fkv_error::{KvError, Result};
use fkv_io::FixedBuffer;
use fkv_metrics::ShardMetric;
use fkv_types::TableIdent;
use futures::{StreamExt, stream::FuturesUnordered};
use futures_channel::oneshot;
use gxhash::HashMap;
use log::error;

use super::dispatch::{dispatch_read, get_large_body};
use crate::{MAX_READ_IN_FLIGHT, partition::ShardStore, sender::Envelope};

/// 一条已提交读请求的调度元数据：请求体移入任务，完成时随结果带回，避免深拷贝。
///
/// slot 只承载「回执 / 取消 / deadline / reopen 预算」这些任务外仍有用的状态：
/// 即使读任务 panic（请求体丢失），也能凭它按内部错误恰好回执一次。
pub(crate) struct ReadSlot {
  pub(super) reply: oneshot::Sender<Result<Response>>,
  pub(super) cancel: Arc<AtomicBool>,
  pub(super) deadline: Option<Instant>,
  /// 该读请求的 auto-reopen 预算（挂起 / 唤醒时随请求带回）。
  pub(super) reopen_retry: Option<u8>,
  pub(super) started: Instant,
  /// 提交时的 attach 代际；完成时代际不符按 `NotRunning` 丢弃结果。
  pub(super) epoch: u64,
  /// 本请求剩余的 OOM 重试次数：首次提交按 `oom_retry_times` 播种，
  /// 每次重新入队减一。
  pub(super) oom_remaining: u32,
  /// 大值读标记：完成时归还借出的 fixed buffer 并解除单飞限制（io-05）。
  pub(super) large: bool,
  /// 任务归属分区（`ReadFlight::in_flight` 的计数键）。
  ///
  /// 读任务只持**自己分区**的 `Rc<ShardStore>`，写任务要独占某一分区时只需等
  /// 该分区的读任务结束；全量排空只用于 attach / detach 等控制路径。
  /// 读请求按定义都带分区（`Request::table()`），`Option` 只作防御。
  pub(super) tbl: Option<TableIdent>,
}

/// 一条读任务的完成结果：slot 由窗口回收，请求体随任务带回。
pub(crate) struct ReadDone {
  pub(super) slot: ReadSlot,
  /// 任务 panic 时为 `None`：请求体随任务丢失，只能按内部错误回执。
  pub(super) req: Option<Request>,
  pub(crate) result: Result<Response>,
  /// 大值读任务借出的 fixed buffer：完成后原样交回（panic 时随任务丢失）。
  pub(super) buf: Option<FixedBuffer>,
}

/// 单个读任务：`JoinHandle` 本身不带 slot 编号，包一层补上，
/// 使任务 panic / 取消时也能对号回执。
struct ReadTask {
  slot: usize,
  handle: runtime::JoinHandle<(Request, Result<Response>, Option<FixedBuffer>)>,
}

impl ReadTask {
  /// 把已通过准入与打开检查的读请求提交为独立 compio task。
  ///
  /// 任务只持 `Rc<ShardStore>`（共享借用即可读）与请求体，不再触达
  /// `&mut ShardState`；写路径要独占目标分区时必须等该分区的读任务结束
  /// （见 `finish_read_partition`），控制路径才等全部读（`finish_read_li`）。
  /// 大值读任务额外持有借出的 fixed buffer（独占使用），完成时随结果交回。
  fn new(
    slot: usize,
    store: Rc<ShardStore>,
    req: Request,
    large: bool,
    buf: Option<FixedBuffer>,
  ) -> Self {
    Self {
      slot,
      handle: spawn(async move {
        let (result, buf) = match &req {
          Request::GetLarge { tbl, key } if large => {
            let mut buf = buf;
            let result = get_large_body(&store, tbl, key, buf.as_mut()).await;
            (result.map(Response::GetLarge), buf)
          }
          _ => (dispatch_read(&store, &req).await, None),
        };
        (req, result, buf)
      }),
    }
  }
}

impl Future for ReadTask {
  type Output = (
    usize,
    Option<Request>,
    Result<Response>,
    Option<FixedBuffer>,
  );

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    let this = self.get_mut();
    match Pin::new(&mut this.handle).poll(cx) {
      Poll::Ready(Ok((req, result, buf))) => Poll::Ready((this.slot, Some(req), result, buf)),
      Poll::Ready(Err(err)) => {
        error!(
          target: "fkv_shard",
          "read task failed slot={} error={}",
          this.slot,
          err
        );
        // 请求体随任务丢失，无法重试；按内部错误回执。
        Poll::Ready((this.slot, None, Err(KvError::IoFail), None))
      }
      Poll::Pending => Poll::Pending,
    }
  }
}

/// 读任务的提交形态：是否大值读、借出的 fixed buffer 与 OOM 重试预算。
///
/// `large` 与 `buf` 存在耦合不变量（只有大值读才借 fixed buffer，未配置时
/// 大值读的 `buf` 仍可为 `None`），故用构造函数收口，避免调用点平铺参数后
/// 把无关组合（普通读带 buffer）传进来。
struct PushSpec {
  large: bool,
  buf: Option<FixedBuffer>,
  oom_remaining: u32,
}

impl PushSpec {
  /// 普通读（含 OOM 重排）：不占大值单飞名额、不借 fixed buffer。
  fn plain(oom_remaining: u32) -> Self {
    Self {
      large: false,
      buf: None,
      oom_remaining,
    }
  }

  /// 大值读：独占借出的 fixed buffer（未配置时为 `None`，走普通大值读）。
  fn large(buf: Option<FixedBuffer>, oom_remaining: u32) -> Self {
    Self {
      large: true,
      buf,
      oom_remaining,
    }
  }
}

/// 单测开关（test-15）：置位时 [`ReadFlight::len`] 伪装成满额。
///
/// 事件循环级用例要在「在飞读满额」的稳定状态下观察批提交路径把新读放进
/// `deferred_read_li`；真实满额需要 128 条同时在飞读，单测无法稳定构造。开关
/// 只影响 `len()`（`is_full()` / 批提交额度随之满额），`is_empty()` /
/// `next()` / `drain()` 仍按真实空集合工作，因此不阻塞 attach / detach 等
/// 控制请求的全量排空。`cfg(test)` 下才存在，生产路径无此分支。
#[cfg(test)]
static FORCE_FULL: AtomicBool = AtomicBool::new(false);

/// 在飞读任务集合：每条读请求一个独立 compio task，互不串行等待。
///
/// 读任务只持 `Rc<ShardStore>`。需要**某分区** `&mut Store` 的写派发先用
/// [`finish_read_partition`] 只排空该分区：既保证 `Rc::get_mut` 成功，也保证
/// 「先到的读先完成、后到的写才开始」，同时不把其它分区的在飞读拖进等待。
/// 需要全 shard 线性化的控制路径（attach / detach / shutdown / drop /
/// reopen / 写队列清扫）仍用 [`finish_read_li`] 全量排空。
///
/// slot 表只登记**在跑任务**：任务完成被取走时立即回收槽位（空位复用）。
/// 用空位表而非 swap-remove：`FuturesUnordered` 无法就地改写任务里的槽号，
/// swap-remove 会把末项挪走后留下指向旧槽号的任务；空位复用保持下标稳定，
/// 回收同样 O(1)，表大小不超过同时在飞任务数上界。
pub(crate) struct ReadFlight {
  task_set: FuturesUnordered<ReadTask>,
  slot_li: Vec<Option<ReadSlot>>,
  /// 已完成回收的槽位下标，供下一次 `push` 复用。
  free_li: Vec<usize>,
  /// 分区 → 该分区在飞读任务数；`push` +1，`remove_slot` -1，计数归零即删键。
  ///
  /// 只由 shard 线程维护：写派发排空目标分区时看到的计数与 `Rc` 强引用一一对应
  /// （每个任务恰好持有一份该分区的 `Rc<ShardStore>` 克隆）。
  in_flight: HashMap<TableIdent, usize>,
  /// 每个新读请求的 OOM 重试预算种子（`ShardConfig::oom_retry_times`）。
  oom_retry_times: u32,
  /// 在飞读 gauge 出口：提交 / 回收后写当前任务数（eng-32c）。
  metrics: Arc<ShardMetric>,
}

impl ReadFlight {
  pub(crate) fn new(metrics: Arc<ShardMetric>, oom_retry_times: u32) -> Self {
    Self {
      task_set: FuturesUnordered::new(),
      slot_li: Vec::new(),
      free_li: Vec::new(),
      in_flight: HashMap::default(),
      oom_retry_times,
      metrics,
    }
  }

  /// 指定分区当前在飞读任务数（0 = 可以安全地 `Rc::get_mut` / `take` 该分区）。
  pub(crate) fn partition_len(&self, tbl: &TableIdent) -> usize {
    self.in_flight.get(tbl).copied().unwrap_or(0)
  }

  /// 分区在飞计数 +1（`push_task` 与 slot 登记在同一临界区内完成）。
  fn add_in_flight(&mut self, tbl: Option<&TableIdent>) {
    let Some(tbl) = tbl else {
      return;
    };
    *self.in_flight.entry(tbl.clone()).or_insert(0) += 1;
  }

  /// 分区在飞计数 -1；归零即删键，避免表随历史分区无限增长。
  fn remove_in_flight(&mut self, tbl: Option<&TableIdent>) {
    let Some(tbl) = tbl else {
      return;
    };
    let Some(count) = self.in_flight.get_mut(tbl) else {
      return;
    };
    *count = count.saturating_sub(1);
    if *count == 0 {
      self.in_flight.remove(tbl);
    }
  }

  pub(crate) fn is_empty(&self) -> bool {
    self.task_set.is_empty()
  }

  pub(crate) fn len(&self) -> usize {
    #[cfg(test)]
    if FORCE_FULL.load(Ordering::Relaxed) {
      return MAX_READ_IN_FLIGHT;
    }
    self.task_set.len()
  }

  /// 单测钩子（test-15）：伪装 / 取消「读在飞满额」，见 [`FORCE_FULL`]。
  #[cfg(test)]
  pub(crate) fn force_full_for_test(force: bool) {
    FORCE_FULL.store(force, Ordering::Relaxed);
  }

  /// slot 表里仍被在跑任务占用的槽位数（单测验证「完成即回收」）。
  #[cfg(test)]
  pub(crate) fn slot_count(&self) -> usize {
    self.slot_li.iter().filter(|slot| slot.is_some()).count()
  }

  /// 把当前在飞读任务数推入 gauge（峰值由 `ShardMetric` 内部维护）。
  pub(super) fn publish(&self) {
    self
      .metrics
      .set_read_in_flight(u64::try_from(self.task_set.len()).unwrap_or(u64::MAX));
  }

  /// 在飞读是否已达上限：只约束在跑任务数，不再兼作单轮收集上限。
  pub(crate) fn is_full(&self) -> bool {
    self.len() >= MAX_READ_IN_FLIGHT
  }

  /// 提交一条已准入的读请求。
  pub(crate) fn push(
    &mut self,
    store: Rc<ShardStore>,
    epoch: u64,
    envelope: Envelope,
    started: Instant,
  ) {
    let spec = PushSpec::plain(self.oom_retry_times);
    self.push_task(store, epoch, envelope, started, spec);
  }

  /// 提交一条已准入的大值读：独占借出的 fixed buffer（未配置时为 `None`，
  /// 走普通大值读），完成后随任务交回（io-05）。
  pub(crate) fn push_large(
    &mut self,
    store: Rc<ShardStore>,
    epoch: u64,
    envelope: Envelope,
    started: Instant,
    buf: Option<FixedBuffer>,
  ) {
    let spec = PushSpec::large(buf, self.oom_retry_times);
    self.push_task(store, epoch, envelope, started, spec);
  }

  /// OOM 重排：保留原请求与剩余重试次数，重新 spawn 一条读任务。
  pub(crate) fn push_retry(
    &mut self,
    store: Rc<ShardStore>,
    epoch: u64,
    envelope: Envelope,
    started: Instant,
    oom_remaining: u32,
  ) {
    let spec = PushSpec::plain(oom_remaining);
    self.push_task(store, epoch, envelope, started, spec);
  }

  fn push_task(
    &mut self,
    store: Rc<ShardStore>,
    epoch: u64,
    envelope: Envelope,
    started: Instant,
    spec: PushSpec,
  ) {
    // 准入不变量（io-23）：窗口与主循环批提交都必须先确认有空位。这里只做
    // 调试期门禁，生产路径由调用方的额度检查保证，避免为一次比较多一条分支。
    debug_assert!(
      !self.is_full(),
      "read in-flight {} exceeds MAX_READ_IN_FLIGHT={MAX_READ_IN_FLIGHT}",
      self.len()
    );
    let PushSpec {
      large,
      buf,
      oom_remaining,
    } = spec;
    let Envelope {
      req,
      reply,
      cancel,
      deadline,
      reopen_retry,
    } = envelope;
    let tbl = req.table().cloned();
    let slot = self.free_li.pop().unwrap_or_else(|| {
      self.slot_li.push(None);
      self.slot_li.len() - 1
    });
    self.slot_li[slot] = Some(ReadSlot {
      reply,
      cancel,
      deadline,
      reopen_retry,
      started,
      epoch,
      oom_remaining,
      large,
      tbl: tbl.clone(),
    });
    self.add_in_flight(tbl.as_ref());
    self
      .task_set
      .push(ReadTask::new(slot, store, req, large, buf));
    self.publish();
  }

  /// 取走一条已完成任务的 slot（空位立即回归复用池）。
  ///
  /// 返回 `None` 只可能是任务号与 slot 表不一致的内部错误：元数据已丢，
  /// 无从回执，记录后跳过（不能 panic 拖垮整个 shard）。
  fn remove_slot(&mut self, slot: usize) -> Option<ReadSlot> {
    let entry = self.slot_li.get_mut(slot).and_then(Option::take);
    if let Some(entry) = &entry {
      // 任务结束（完成 / panic 回收）即释放该分区的在飞计数：它与任务持有的
      // `Rc<ShardStore>` 克隆一一对应，计数归零表示该分区可以安全独占。
      self.remove_in_flight(entry.tbl.as_ref());
      self.free_li.push(slot);
    } else {
      error!(target: "fkv_shard", "read slot missing slot={}", slot);
    }
    entry
  }

  /// 取一条已完成读；在飞 gauge 同步下降。
  pub(crate) async fn next(&mut self) -> Option<ReadDone> {
    loop {
      let (slot, req, result, buf) = self.task_set.next().await?;
      let slot = self.remove_slot(slot);
      self.publish();
      if let Some(slot) = slot {
        return Some(ReadDone {
          slot,
          req,
          result,
          buf,
        });
      }
    }
  }

  /// 等在飞读全部完成并回收全部 slot；不触达 `ShardState`。
  pub(crate) async fn drain(&mut self) -> Vec<ReadDone> {
    let mut done_li = Vec::with_capacity(self.task_set.len());
    while let Some(done) = self.next().await {
      done_li.push(done);
    }
    done_li
  }
}
