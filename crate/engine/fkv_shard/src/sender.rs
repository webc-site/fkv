//! 请求入队侧：`Envelope` / `RequestHandle` / `ShardSender`、优先级与入队等待。

use std::{
  future::Future,
  pin::Pin,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  task::{Context, Poll},
  time::{Duration, Instant},
};

use crossfire::{MAsyncTx, SendTimeoutError, mpmc::Array};
use fkv_api::{Request, RequestPriority, Response};
use fkv_error::{KvError, Result};
use fkv_metrics::{Op, ShardMetric};
use futures::{
  FutureExt,
  future::{Either, Shared, select},
};
use futures_channel::oneshot;
use futures_timer::Delay;
use log::warn;

use crate::metrics::op_of;

/// 请求优先级：前台读/写/控制为高，后台维护为低（对齐 C++
/// `ready_tasks_` / `low_priority_ready_tasks_`）。
///
/// 注意：不把写请求静态降为低优先。C++ 是写任务在 `Apply` / `WritePage` 后
/// 主动 `YieldToLowPQ`，入队即低会破坏 eng-33 的每分区写顺序与前台写延迟。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pri {
  High,
  Low,
}

/// 后台维护类请求走低优先：没有前台请求时一次跑一条，高优先连续占用超过
/// [`ShardConfig::max_processing_time_us`] 时也先插一条低优先防饥饿。
///
/// 逐变体优先级由 `fkv_api` 的 `request_meta!` 变体表生成，这里只做
/// `RequestPriority` → 入队通道的穷举映射。
pub(crate) fn pri_of(req: &Request) -> Pri {
  match req.priority() {
    RequestPriority::High => Pri::High,
    RequestPriority::Low => Pri::Low,
  }
}
/// shard 线程退出信号（test-05）：事件循环退出后，请求通道缓冲区内未被消费的
/// envelope 无人回执——其 oneshot 发送端随通道存活，而通道要等全部发送端析构，
/// 等待回执的任务又恰好持有发送端，形成循环依赖，请求永久 Pending。线程退出
/// 时广播本信号，让这些句柄排空为 `NotRunning`。
#[derive(Clone)]
pub(crate) struct ShardExit {
  /// 线程退出时完成；`Shared` 让每个回执句柄共享同一完成事件。
  done: Shared<oneshot::Receiver<()>>,
}

/// 线程侧发送端：Drop（线程正常收尾或 panic 展开）即广播退出信号。
pub(crate) struct ExitGuard(Option<oneshot::Sender<()>>);

impl ShardExit {
  pub(crate) fn new() -> (Self, ExitGuard) {
    let (tx, rx) = oneshot::channel();
    (Self { done: rx.shared() }, ExitGuard(Some(tx)))
  }

  /// 回执句柄侧的等待视图（克隆共享同一完成事件）。
  pub(crate) fn wait(&self) -> Shared<oneshot::Receiver<()>> {
    self.done.clone()
  }
}

impl Drop for ExitGuard {
  fn drop(&mut self) {
    if let Some(tx) = self.0.take() {
      let _ = tx.send(());
    }
  }
}

pub(crate) struct Envelope {
  pub(crate) req: Request,
  pub(crate) reply: oneshot::Sender<Result<Response>>,
  /// 调用方取消标志：`RequestHandle` 被丢弃（drop future / 超时）时置位。
  /// shard 在开始执行前检查，未开始的请求不再白跑；执行中的请求不受影响。
  pub(crate) cancel: Arc<AtomicBool>,
  /// 服务端截止时刻（`None` = 不限）：出队后、开始执行前比较。
  /// 过期请求直接回 `Timeout`，不进入 `admit` / `prepare_table`。
  pub(crate) deadline: Option<Instant>,
  /// 本请求剩余的 auto-reopen 轮数（对齐 C++ `KvRequest::reopen_retry_remaining_`）。
  ///
  /// `None` = 尚未因 `ResourceMissing` 挂起过；首次挂起时按
  /// `auto_reopen_retry_times` 播种，每次挂起消耗 1。预算挂在请求上而不是
  /// driver 上：driver 每次只做一次延迟尝试，失败即连坐 waiter；读请求被成功
  /// 重排后再次 `ResourceMissing` 才消耗下一轮。
  pub(crate) reopen_retry: Option<u8>,
}

impl Envelope {
  /// 调用方是否已放弃等待。
  pub(crate) fn canceled(&self) -> bool {
    self.cancel.load(Ordering::Relaxed)
  }

  /// 取一轮 auto-reopen 预算（首次按 `seed` 播种）。返回 `false` 表示预算
  /// 耗尽，调用方应立即以 `ResourceMissing` 回执而不是再挂起。
  pub(crate) fn take_reopen_retry(&mut self, seed: u8) -> bool {
    let remaining = self.reopen_retry.get_or_insert(seed);
    if *remaining == 0 {
      return false;
    }
    *remaining -= 1;
    true
  }

  /// 排队期间是否已过截止时刻。只在开始执行前调用；执行中不检查。
  pub(crate) fn expired(&self) -> bool {
    self
      .deadline
      .is_some_and(|deadline| Instant::now() >= deadline)
  }

  /// 回执。调用方可能已离开（取消 / 超时），发送失败不影响 shard。
  pub(crate) fn send(self, result: Result<Response>) {
    let _ = self.reply.send(result);
  }
}

/// 请求回执句柄：`Drop` 即置取消标志，保证「调用方放弃 = 未开始的请求不执行」。
///
/// 超时用 `compio::time::timeout` 包裹 [`ShardSender::request`] 即可：超时 drop
/// 本句柄，shard 侧在 IO 边界（请求开始前）看到取消跳过；已在执行的请求仍会跑完
/// （与 C++ 请求不可中途取消一致）。
pub(crate) struct RequestHandle {
  rx: oneshot::Receiver<Result<Response>>,
  cancel: Arc<AtomicBool>,
  /// shard 线程退出信号：退出后未回执的请求不会再被处理，排空为 `NotRunning`。
  exit: Shared<oneshot::Receiver<()>>,
}

impl Future for RequestHandle {
  type Output = Result<Response>;

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    let this = self.get_mut();
    // 先轮询真实回执：退出信号只在全部回执发送之后才广播，见到信号时
    // 已有的回执必然可见，真实结果优先。
    match Pin::new(&mut this.rx).poll(cx) {
      Poll::Ready(Ok(result)) => Poll::Ready(result),
      // shard 退出 / 回执通道被丢弃：统一按 NotRunning 完成。
      Poll::Ready(Err(_)) => Poll::Ready(Err(KvError::NotRunning)),
      // 未见回执且线程已退出：请求停留在无人消费的通道缓冲里，永不执行，
      // 按 NotRunning 排空（test-05：否则恰好 queue_size 条请求永久 Pending）。
      Poll::Pending => match Pin::new(&mut this.exit).poll(cx) {
        Poll::Pending => Poll::Pending,
        Poll::Ready(_) => Poll::Ready(Err(KvError::NotRunning)),
      },
    }
  }
}

impl Drop for RequestHandle {
  fn drop(&mut self) {
    self.cancel.store(true, Ordering::Relaxed);
  }
}
#[derive(Clone)]
pub struct ShardSender {
  /// 高优先通道：前台读/写/控制请求。
  pub(crate) tx_hi: MAsyncTx<Array<Envelope>>,
  /// 低优先通道：Gc / Compact / CreateArchive / DeleteBranch / CleanExpired。
  pub(crate) tx_lo: MAsyncTx<Array<Envelope>>,
  /// 本 shard 的指标：低优先通道深度 gauge 在入队侧记账（eng-34c）。
  pub(crate) metrics: Arc<ShardMetric>,
  /// shard 线程退出信号：随句柄传递，线程退出后未回执请求排空为 NotRunning。
  pub(crate) exit: ShardExit,
  /// 本端默认请求 deadline；`None` = 调用方自行兜底（保持旧行为）。
  pub(crate) request_timeout: Option<Duration>,
  /// 入队满时的有界等待；`None` = 无限等待（保持旧行为）。
  pub(crate) queue_timeout: Option<Duration>,
}

/// 低优先深度记账守护：投递成功即解除（消费侧负责递减）；投递失败或 future
/// 被取消（drop）时回滚，避免关停 / 取消后 gauge 残留非零。
struct LowPqGuard {
  metrics: Arc<ShardMetric>,
  armed: bool,
}

impl Drop for LowPqGuard {
  fn drop(&mut self) {
    if self.armed {
      self.metrics.dec_low_pq_depth();
    }
  }
}

impl ShardSender {
  pub async fn request(&self, req: Request) -> Result<Response> {
    self.request_with_timeout(req, self.request_timeout).await
  }

  /// 带 deadline 的请求：截止时刻写入 `Envelope`，shard 在开始执行前拒绝过期
  /// 请求；本端同时按同一时长兜底，超时回 `Timeout` 并置取消标志（sto-62a）。
  ///
  /// 已开始执行的请求不因本端超时中断，仍按既有语义跑完。
  pub async fn request_deadline(&self, req: Request, timeout: Duration) -> Result<Response> {
    self.request_with_timeout(req, Some(timeout)).await
  }

  /// 只设服务端截止时刻、不做本端兜底的请求：shard 出队时已过期即回 `Timeout`。
  pub async fn request_at(&self, req: Request, deadline: Instant) -> Result<Response> {
    self.enqueue(req, Some(deadline)).await?.await
  }

  /// 设置本端默认请求 timeout；`None` 关闭（保持旧行为）。
  pub fn with_request_timeout(mut self, timeout: Option<Duration>) -> Self {
    self.request_timeout = timeout;
    self
  }

  /// 设置队列满时的有界等待；`None` = 无限等待（保持旧行为），超时回 `Busy`。
  pub fn with_queue_timeout(mut self, timeout: Option<Duration>) -> Self {
    self.queue_timeout = timeout;
    self
  }

  async fn request_with_timeout(
    &self,
    req: Request,
    timeout: Option<Duration>,
  ) -> Result<Response> {
    let deadline = timeout.map(|timeout| Instant::now() + timeout);
    let handle = self.enqueue(req, deadline).await?;
    let Some(deadline) = deadline else {
      return handle.await;
    };
    // 本端兜底：超时 drop 句柄 → 置取消标志；shard 侧未开始则跳过。
    // 先轮询回执：若已完成，即使 timer 同时到期也返回实际结果。
    let wait = deadline.saturating_duration_since(Instant::now());
    match select(handle, Delay::new(wait)).await {
      Either::Left((result, _)) => result,
      Either::Right(((), _)) => {
        warn!(
          target: "fkv_shard",
          "request deadline local shard_id={} timeout_us={}",
          self.metrics.shard_id(),
          timeout.unwrap_or_default().as_micros()
        );
        Err(KvError::Timeout)
      }
    }
  }

  /// 投递请求并返回可取消的回执句柄。
  ///
  /// `deadline` 同时用于 shard 侧过期判断与入队等待上限；队列满时按
  /// [`ShardSender::with_queue_timeout`] 有界等待，超时回 `Busy`。
  pub(crate) async fn enqueue(
    &self,
    req: Request,
    deadline: Option<Instant>,
  ) -> Result<RequestHandle> {
    let (reply, rx) = oneshot::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    // 按请求类型分通道：低优先后台请求不能挡在通道里阻塞后续前台请求。
    let pri = pri_of(&req);
    let op = op_of(&req);
    let tx = match pri {
      Pri::High => &self.tx_hi,
      Pri::Low => &self.tx_lo,
    };
    // 低优先深度 gauge：先记账再投递，失败 / 取消由 guard 回滚。
    let mut low_guard = (pri == Pri::Low).then(|| {
      self.metrics.inc_low_pq_depth();
      LowPqGuard {
        metrics: Arc::clone(&self.metrics),
        armed: true,
      }
    });
    let envelope = Envelope {
      req,
      reply,
      cancel: cancel.clone(),
      deadline,
      reopen_retry: None,
    };
    // 队列满时的等待上限：取请求 deadline 与 queue_timeout 的较早者，
    // 分别回 Timeout / Busy，避免把「背压」误报成「挂死」。
    let wait = queue_wait(self.queue_timeout, deadline);
    let sent = match wait {
      None => tx.send(envelope).await.map_err(|_| KvError::NotRunning),
      Some((deadline, err)) => {
        let wait = deadline.saturating_duration_since(Instant::now());
        match tx.send_with_timer(envelope, Delay::new(wait)).await {
          Ok(()) => Ok(()),
          Err(SendTimeoutError::Timeout(_)) => {
            warn!(
              target: "fkv_shard",
              "request enqueue wait timed out shard_id={} op={} wait_us={} error={}",
              self.metrics.shard_id(),
              op.map_or("unknown", Op::as_str),
              wait.as_micros(),
              err
            );
            Err(err)
          }
          Err(SendTimeoutError::Disconnected(_)) => Err(KvError::NotRunning),
        }
      }
    };
    sent?;
    if let Some(guard) = low_guard.as_mut() {
      guard.armed = false;
    }
    Ok(RequestHandle {
      rx,
      cancel,
      exit: self.exit.wait(),
    })
  }
}

/// 入队等待上限：请求 deadline 与队列 timeout 取较早者，携带对应错误。
fn queue_wait(
  queue_timeout: Option<Duration>,
  deadline: Option<Instant>,
) -> Option<(Instant, KvError)> {
  let now = Instant::now();
  let queue = queue_timeout.map(|timeout| (now + timeout, KvError::Busy));
  let request = deadline.map(|deadline| (deadline, KvError::Timeout));
  match (queue, request) {
    (Some((queue, queue_err)), Some((request, request_err))) => Some(if request <= queue {
      (request, request_err)
    } else {
      (queue, queue_err)
    }),
    (Some(queue), None) => Some(queue),
    (None, Some(request)) => Some(request),
    (None, None) => None,
  }
}
