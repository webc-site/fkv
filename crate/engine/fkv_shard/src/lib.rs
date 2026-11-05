//! 1 shard = 1 OS thread + 1 compio Runtime + 1 async request queue。

use std::{io, mem::take};

use compio::{runtime, runtime::spawn};
use fkv_task_prewarm::CompletionReason;
mod backend;
mod partition;
mod roots;
mod write_gate;

use std::{
  cmp::Ordering as CmpOrdering,
  collections::{BinaryHeap, VecDeque},
  future::Future,
  path::PathBuf,
  pin::Pin,
  rc::Rc,
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
    mpsc::sync_channel,
  },
  task::{Context, Poll},
  thread::{self, JoinHandle},
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub use backend::{Backend, CloudBackend, StoreBackend, split_cloud_store_path};
use compio::{driver::ProactorBuilder, runtime::Runtime, time::timeout};
use crossfire::{
  MAsyncRx, MAsyncTx, SendTimeoutError,
  mpmc::{Array, bounded_async},
};
use fkv_api::{
  ArchiveReply, ArchiveTagInfo, BranchReply, CleanExpiredReply, FloorReply, LargeValueReply,
  ListPage, ObjectMeta, Request, Response, ScanReply,
};
use fkv_btree::BatchEntry;
use fkv_error::{KvError, Result};
use fkv_io::{FixedBuffer, FixedSegmentIo, ManifestIo, PageIo, TermIo};
use fkv_large::SegmentLayout;
use fkv_metrics::{MeteredIo, Op, ShardMetric, ShardSnapshot};
use fkv_standby::RsyncStandbyService;
use fkv_store::{PageOption, ScanOption};
use fkv_task_reopen::AutoReopenOption;
use fkv_types::{
  InstallOutcome, MAIN_BRANCH_NAME, StoreMode, TableIdent, WriteOp, normalize_branch_name,
};
use futures::{
  future::{Either, select},
  stream::{FuturesUnordered, StreamExt},
};
use futures_channel::oneshot;
use futures_timer::Delay;
use gxhash::HashMap;
use log::{debug, error, info, warn};
use partition::{PartitionStoreMap, ShardStore, StoreCfg};
pub use roots::{RootedConfig, RootedStore};
use write_gate::WriteGate;

/// 读批上限：限制同轮在飞的读页缓冲与 fd 数，避免单轮吃满内存。
///
/// 同时是读侧 in-flight 上限（`read_in_flight` 峰值不应超过本值）。
pub const MAX_READ_BATCH: usize = 32;

/// 同一分区一次 envelope_li commit 合并的最大 Put 请求数。
const MAX_PUT_GROUP_REQUESTS: usize = 64;

/// 一轮最多从高优先通道收集的写请求数，用于形成 envelope_li commit 批次。
const MAX_WRITE_COLLECT: usize = 64;

/// 请求优先级：前台读/写/控制为高，后台维护为低（对齐 C++
/// `ready_tasks_` / `low_priority_ready_tasks_`）。
///
/// 注意：不把写请求静态降为低优先。C++ 是写任务在 `Apply` / `WritePage` 后
/// 主动 `YieldToLowPQ`，入队即低会破坏 eng-33 的每分区写顺序与前台写延迟。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pri {
  High,
  Low,
}

/// 后台维护类请求走低优先：没有前台请求时一次跑一条，高优先连续占用超过
/// [`ShardConfig::max_processing_time_us`] 时也先插一条低优先防饥饿。
fn pri_of(req: &Request) -> Pri {
  match req {
    Request::FileGc { .. }
    | Request::LocalGc { .. }
    | Request::Compact { .. }
    | Request::CreateArchive { .. }
    | Request::DeleteArchive { .. }
    | Request::DeleteBranch { .. }
    | Request::CleanExpired { .. }
    // 远端批量操作：预热会连续下载多个对象，ListObject 可能扫大 prefix；
    // 都放低优先，避免长时间占用高优先通道。
    | Request::Prewarm { .. }
    | Request::ListObject { .. } => Pri::Low,
    // ListArchiveTags / ListStandbyPartition 是控制面观察请求：只列名字 /
    // 目录，开销小且调用方期望快速回执，保持高优先。
    _ => Pri::High,
  }
}

#[derive(Debug, Clone)]
pub struct ShardConfig {
  pub shard_id: u32,
  pub root: PathBuf,
  pub page_size: usize,
  /// data 页 restart 间隔（对齐 C++ `KvOption::data_page_restart_interval`）。
  pub restart_interval: u16,
  /// index 页 restart 间隔（对齐 C++ `KvOption::index_page_restart_interval`）。
  pub index_restart_interval: u16,
  pub pages_per_file_shift: u8,
  pub segment_size: u32,
  pub segments_per_file_shift: u8,
  pub large_value_threshold: usize,
  pub fixed_buffer_size: usize,
  pub queue_size: usize,
  pub auto_reopen_retry_times: u8,
  pub auto_reopen_pending_time_us: u64,
  /// `OutOfMem` 时整请求重试次数（对齐 C++ `auto_oom_retry_times`）。
  pub oom_retry_times: u32,
  /// 启用压缩时的 zstd level（None = 关闭）。
  pub compression_level: Option<i32>,
  /// 是否校验读 page checksum。
  pub verify_checksum: bool,
  /// 一批 overflow 页最大指针数。
  pub overflow_pointers: u8,
  /// 文件空间放大触发 compaction 阈值（0 = 关闭）。
  pub file_amplify_factor: u8,
  /// segment 文件空间放大触发 compaction 阈值（0 = 关闭）。
  ///
  /// 与 `file_amplify_factor` 独立：segment 文件的槽位密度与搬段成本不同。
  pub segment_file_amplify_factor: u8,
  /// manifest 大小上限（字节）。
  pub manifest_limit: usize,
  /// 单次批量写合并的页数上限（对齐 C++ `max_write_batch_pages`）。
  pub max_write_batch_pages: usize,
  /// IO 层在飞写组上限（对齐 C++ `max_inflight_write`）；0 按 1 处理。
  pub max_inflight_write: usize,
  /// FD 缓存上限（按 shard 均分后的值，对齐 C++ `store_fd_limit / num_threads`）；
  /// 0 表示不限制。
  pub fd_limit: usize,
  /// 每表 index page 缓存字节预算；0 表示不限制。
  pub index_cache_bytes: usize,
  /// 全 store index page 缓存聚合字节预算；0 表示不限制。
  pub index_cache_total_bytes: usize,
  /// 本 shard 的 root meta 字节预算；0 表示不限制。
  ///
  /// 计入 data / segment mapping、index 缓存、压缩字典与 branch 元数据，
  /// 对齐 C++ `KvOption::root_meta_cache_size`（runtime 已按 shard 数均分）。
  pub root_meta_cache_bytes: usize,
  /// io_uring SQ 深度（对齐 C++ `io_queue_size`）。
  pub io_queue_size: u32,
  /// IO 后端：Local / Mem / Cloud。默认 `Local`。
  pub backend: Backend,
  /// 运行模式：决定 `ResourceMissing`（manifest 暂不可见）的语义。
  /// `Cloud` / `StandbyReplica` 下不做本地空表懒创建，读请求挂等待重排
  /// （对齐 C++ `auto_reopen` + `AutoReopenRetry`）；`Local` / `Mem` 保持懒创建。
  pub mode: StoreMode,
  /// `StandbyReplica` 的追赶服务：reopen 前先把该分区从 master 同步一次
  /// （对齐 C++ `ReopenTask::Reopen` 的 `RsyncPartition` + `WaitIo`）。
  /// `None` = 非 standby 模式。
  pub standby: Option<Arc<RsyncStandbyService>>,
  /// 全局同时进行的写任务上限（对齐 C++ `max_write_concurrency_`）。
  /// `0` = 不限，与 `fkv_config` 默认值一致。
  pub max_write_concurrency: usize,
  /// 高优先连续服务的时间片（微秒）。到点且低优先队列非空时先跑一条后台请求，
  /// 再回到高优先，防后台饿死；`0` = 关闭（仅在没有前台请求时跑后台）。
  /// 默认 400us，对齐 C++ release 的 `FLAGS_max_processing_time_microseconds`。
  ///
  /// 只是请求边界上的时间片，不能抢占已在执行的任务；单任务内的 CPU 长段
  /// 让出由 [`Self::yield_budget_us`] 控制（scan 页链 / overflow 链 /
  /// GC 文件遍历的 `maybe_yield` 插桩）。
  pub max_processing_time_us: u64,
  /// 单任务内长循环的协作让出段预算（微秒；`0` = 关闭）。
  ///
  /// 与 `max_processing_time_us`（请求边界轮预算）分列：轮预算管前后台公平，
  /// 段预算管单请求内部的 p99 尾延迟（默认 20us）。
  /// 只在 CPU 长段 / 批量 IO 边界检查，不插每个 `await`。
  pub yield_budget_us: u64,
}

struct Envelope {
  req: Request,
  reply: oneshot::Sender<Result<Response>>,
  /// 调用方取消标志：`RequestHandle` 被丢弃（drop future / 超时）时置位。
  /// shard 在开始执行前检查，未开始的请求不再白跑；执行中的请求不受影响。
  cancel: Arc<AtomicBool>,
  /// 服务端截止时刻（`None` = 不限）：出队后、开始执行前比较。
  /// 过期请求直接回 `Timeout`，不进入 `admit` / `prepare_table`。
  deadline: Option<Instant>,
  /// 本请求剩余的 auto-reopen 轮数（对齐 C++ `KvRequest::reopen_retry_remaining_`）。
  ///
  /// `None` = 尚未因 `ResourceMissing` 挂起过；首次挂起时按
  /// `auto_reopen_retry_times` 播种，每次挂起消耗 1。预算挂在请求上而不是
  /// driver 上：driver 每次只做一次延迟尝试，失败即连坐 waiter；读请求被成功
  /// 重排后再次 `ResourceMissing` 才消耗下一轮。
  reopen_retry: Option<u8>,
}

impl Envelope {
  /// 调用方是否已放弃等待。
  fn canceled(&self) -> bool {
    self.cancel.load(Ordering::Relaxed)
  }

  /// 取一轮 auto-reopen 预算（首次按 `seed` 播种）。返回 `false` 表示预算
  /// 耗尽，调用方应立即以 `ResourceMissing` 回执而不是再挂起。
  fn take_reopen_retry(&mut self, seed: u8) -> bool {
    let remaining = self.reopen_retry.get_or_insert(seed);
    if *remaining == 0 {
      return false;
    }
    *remaining -= 1;
    true
  }

  /// 排队期间是否已过截止时刻。只在开始执行前调用；执行中不检查。
  fn expired(&self) -> bool {
    self
      .deadline
      .is_some_and(|deadline| Instant::now() >= deadline)
  }

  /// 回执。调用方可能已离开（取消 / 超时），发送失败不影响 shard。
  fn send(self, result: Result<Response>) {
    let _ = self.reply.send(result);
  }
}

/// 请求回执句柄：`Drop` 即置取消标志，保证「调用方放弃 = 未开始的请求不执行」。
///
/// 超时用 `compio::time::timeout` 包裹 [`ShardSender::request`] 即可：超时 drop
/// 本句柄，shard 侧在 IO 边界（请求开始前）看到取消跳过；已在执行的请求仍会跑完
/// （与 C++ 请求不可中途取消一致）。
struct RequestHandle {
  rx: oneshot::Receiver<Result<Response>>,
  cancel: Arc<AtomicBool>,
}

impl Future for RequestHandle {
  type Output = Result<Response>;

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    let this = self.get_mut();
    match Pin::new(&mut this.rx).poll(cx) {
      Poll::Ready(Ok(result)) => Poll::Ready(result),
      // shard 退出 / 回执通道被丢弃：统一按 NotRunning 完成。
      Poll::Ready(Err(_)) => Poll::Ready(Err(KvError::NotRunning)),
      Poll::Pending => Poll::Pending,
    }
  }
}

impl Drop for RequestHandle {
  fn drop(&mut self) {
    self.cancel.store(true, Ordering::Relaxed);
  }
}

/// 按 `ShardConfig.backend` 在 shard 线程内装配 IO 后端。
///
/// `metrics` 是本 shard 的计数器：Cloud 后端需要它上报本地 cache 的
/// hit / miss / evict / bytes（cld-55），Local / Mem 后端不用。
fn build_backend(config: &ShardConfig, metrics: &Arc<ShardMetric>) -> Result<StoreBackend> {
  match &config.backend {
    Backend::Local => Ok(StoreBackend::local(config)),
    Backend::LocalRouted(rooted) => Ok(StoreBackend::local_routed(config, rooted)),
    Backend::Mem => Ok(StoreBackend::mem(config)),
    Backend::Cloud(cloud) => StoreBackend::cloud(config, cloud, metrics.clone()),
  }
}

/// 构造 shard 线程的 compio runtime。
///
/// 对齐 C++ `IouringMgr::BootstrapRing`：SQ 深度取 `io_queue_size`，并开启
/// `SINGLE_ISSUER + DEFER_TASKRUN + TASKRUN_FLAG`（thread-per-core 下减少
/// task_work 与原子开销）。老内核不支持这些 flag 时首次 `build` 会报
/// `EINVAL`，回落为仅设置容量的 ring，避免整机起不来。
fn build_runtime(io_queue_size: u32, segment_size: usize) -> io::Result<Runtime> {
  // 段读走内核 provided-buffer（io-35）：runtime 缓冲池是单尺寸常驻，按段大小
  // 配置；尺寸不合适时不配置，读路径自动回落自管池。
  let managed_buf_len =
    fkv_io_compio::managed_segment_read_supported(segment_size).then_some(segment_size);
  let mut proactor = ProactorBuilder::new();
  proactor
    .capacity(io_queue_size)
    .single_issuer(true)
    .defer_taskrun(true)
    .taskrun_flag(true);
  if let Some(len) = managed_buf_len {
    proactor.buffer_pool_buffer_len(len);
  }
  match Runtime::builder().with_proactor(proactor).build() {
    Ok(runtime) => Ok(runtime),
    Err(err) => {
      warn!(
        target: "fkv_shard",
        "io-uring tuned ring unavailable error={} fallback=capacity_only",
        err
      );
      let mut proactor = ProactorBuilder::new();
      proactor.capacity(io_queue_size);
      if let Some(len) = managed_buf_len {
        proactor.buffer_pool_buffer_len(len);
      }
      Runtime::builder().with_proactor(proactor).build()
    }
  }
}

#[derive(Clone)]
pub struct ShardSender {
  /// 高优先通道：前台读/写/控制请求。
  tx_hi: MAsyncTx<Array<Envelope>>,
  /// 低优先通道：Gc / Compact / CreateArchive / DeleteBranch / CleanExpired。
  tx_lo: MAsyncTx<Array<Envelope>>,
  /// 本 shard 的指标：低优先通道深度 gauge 在入队侧记账（eng-34c）。
  metrics: Arc<ShardMetric>,
  /// 本端默认请求 deadline；`None` = 调用方自行兜底（保持旧行为）。
  request_timeout: Option<Duration>,
  /// 入队满时的有界等待；`None` = 无限等待（保持旧行为）。
  queue_timeout: Option<Duration>,
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
  async fn enqueue(&self, req: Request, deadline: Option<Instant>) -> Result<RequestHandle> {
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
    Ok(RequestHandle { rx, cancel })
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

/// shard 生命周期在 shard 线程外的可观测形态（对齐 [`DsShardState`]）。
///
/// 仅 crate 内使用：对外只暴露 [`ShardLifecycle::is_attached`] 等语义化判定，
/// 避免外部把生命周期镜像当作状态机直接匹配。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum LifecycleState {
  /// 未托管：普通请求 `NotRunning`。
  #[default]
  Detached = 0,
  /// 正常托管：普通请求放行。
  Attached = 1,
  /// 更高 term 已接管：普通请求 `ExpiredTerm`，直到重新 attach / detach。
  Expired = 2,
}

impl LifecycleState {
  /// 原子镜像解码：未知编码按最保守的 `Detached` 处理（普通请求必被拒）。
  const fn from_u8(raw: u8) -> Self {
    if raw == Self::Attached as u8 {
      Self::Attached
    } else if raw == Self::Expired as u8 {
      Self::Expired
    } else {
      Self::Detached
    }
  }
}

impl From<DsShardState> for LifecycleState {
  fn from(state: DsShardState) -> Self {
    match state {
      DsShardState::Detached => Self::Detached,
      DsShardState::Attached => Self::Attached,
      DsShardState::Expired => Self::Expired,
    }
  }
}

/// shard 生命周期的跨线程探针：`AtomicU8` 镜像 + 读取接口。
///
/// shard 线程独占 `ShardState`，但调用方在请求出错（尤其是本地 `Timeout`，此时
/// 请求可能已执行）后必须知道 shard 的真实状态，才能决定是否清掉本层 attach 视图。
#[derive(Clone, Debug)]
pub struct ShardLifecycle {
  state: Arc<AtomicU8>,
}

impl ShardLifecycle {
  fn new(state: DsShardState) -> Self {
    Self {
      state: Arc::new(AtomicU8::new(LifecycleState::from(state) as u8)),
    }
  }

  /// 当前生命周期镜像（模块内用；对外只暴露语义化判定 [`Self::is_attached`]）。
  fn get(&self) -> LifecycleState {
    LifecycleState::from_u8(self.state.load(Ordering::Acquire))
  }

  /// shard 正处于托管态（普通请求可被受理）。
  pub fn is_attached(&self) -> bool {
    self.get() == LifecycleState::Attached
  }

  fn store(&self, state: DsShardState) {
    self
      .state
      .store(LifecycleState::from(state) as u8, Ordering::Release);
  }
}

pub struct Shard {
  tx_hi: MAsyncTx<Array<Envelope>>,
  tx_lo: MAsyncTx<Array<Envelope>>,
  join: Option<JoinHandle<()>>,
  metrics: Arc<ShardMetric>,
  /// shard 线程内 `ShardState::lifecycle` 的原子镜像：运行时在请求出错后据此判断
  /// shard 的真实状态（本地 `Timeout` 时请求可能已执行并把 shard 置为 `Expired`）。
  lifecycle: ShardLifecycle,
}

impl Shard {
  pub fn start(config: ShardConfig) -> Result<Self> {
    // `RuntimeConfig` 直传路径不经 `KvOption::validate`：移位上界在这里兜住，
    // 否则 `1u64 << shift` 在 debug 下 panic、release 下掩码为 0。
    if config.queue_size == 0 || config.pages_per_file_shift >= 63 {
      return Err(KvError::InvalidArgs);
    }
    let layout = SegmentLayout::new(config.segment_size, config.segments_per_file_shift)?;
    // 两条有界通道按优先级分离，各自承担 `queue_size` 的背压。
    let (tx_hi, rx_hi) = bounded_async::<Envelope>(config.queue_size);
    let (tx_lo, rx_lo) = bounded_async::<Envelope>(config.queue_size);
    let page_size = config.page_size;
    let restart_interval = config.restart_interval;
    let index_restart_interval = config.index_restart_interval;
    let large_value_threshold = if config.large_value_threshold == 0 {
      page_size / 2
    } else {
      config.large_value_threshold
    };
    let metrics = Arc::new(ShardMetric::new(config.shard_id));
    let thread_metrics = metrics.clone();
    let lifecycle = ShardLifecycle::new(DsShardState::Detached);
    let thread_lifecycle = lifecycle.clone();
    // 启动握手：线程内 Runtime 与 store 就绪后才回传结果，运行时初始化失败必须
    // 让 `Shard::start` 报错而不是返回假成功（对齐 C++ `StartInstance` 的 fail_start）。
    let (ready_tx, ready_rx) = sync_channel::<Result<()>>(1);
    let join = thread::Builder::new()
      .name("fkv-shard".to_owned())
      .spawn(move || {
        let runtime = match build_runtime(config.io_queue_size, config.segment_size as usize) {
          Ok(runtime) => runtime,
          Err(err) => {
            error!(
              target: "fkv_shard",
              "shard runtime init failed shard_id={} error={}",
              config.shard_id,
              err
            );
            let _ = ready_tx.send(Err(KvError::IoFail));
            return;
          }
        };
        info!(
          target: "fkv_shard",
          "shard start shard_id={} root={} page_size={} pages_per_file_shift={}",
          config.shard_id,
          config.root.display(),
          config.page_size,
          config.pages_per_file_shift
        );
        runtime.block_on(async move {
          let backend = match build_backend(&config, &thread_metrics) {
            Ok(backend) => backend,
            Err(err) => {
              error!(
                target: "fkv_shard",
                "shard backend init failed shard_id={} error={}",
                config.shard_id,
                err
              );
              let _ = ready_tx.send(Err(err));
              return;
            }
          };
          let io = MeteredIo::new(backend, thread_metrics.clone());
          let fixed_buffer = if config.fixed_buffer_size > 0 {
            match io.register_fixed_buffer(config.fixed_buffer_size) {
              Ok(buf) => {
                info!(
                  target: "fkv_shard",
                  "shard fixed buffer shard_id={} len={} registered={}",
                  config.shard_id,
                  buf.len(),
                  buf.is_registered()
                );
                Some(buf)
              }
              Err(err) => {
                warn!(
                  target: "fkv_shard",
                  "shard fixed buffer register failed shard_id={} error={}",
                  config.shard_id,
                  err
                );
                None
              }
            }
          } else {
            None
          };
          // 每分区独立 Store：共享 IO 后端与 index 缓存账本，跨分区写可并发。
          let store_cfg = StoreCfg {
            page_size,
            restart_interval,
            index_restart_interval,
            layout,
            large_value_threshold,
            verify_checksum: config.verify_checksum,
            overflow_pointers: config.overflow_pointers,
            file_amplify_factor: config.file_amplify_factor,
            segment_file_amplify_factor: config.segment_file_amplify_factor,
            manifest_limit: config.manifest_limit,
            index_cache_bytes: config.index_cache_bytes,
            index_cache_total_bytes: config.index_cache_total_bytes,
            pages_per_file_shift: config.pages_per_file_shift,
            yield_budget_us: config.yield_budget_us,
            compression_level: config.compression_level,
          };
          let mut state = ShardState {
            stores: PartitionStoreMap::new(io, store_cfg, config.root_meta_cache_bytes),
            fixed_buffer,
            lifecycle: DsShardState::Detached,
            lifecycle_mirror: thread_lifecycle,
            term: None,
            branch: String::new(),
            // shard 身份在启动时固定：attach 只接受同一 id，否则持久化
            // `CURRENT_TERM_<branch>_<shard_id>` 会在两个 shard 间共用而失去 fencing。
            shard_id: config.shard_id,
            pages_per_file_shift: config.pages_per_file_shift,
            reopen_on_missing: matches!(config.mode, StoreMode::Cloud | StoreMode::StandbyReplica),
            standby: config.standby.clone(),
            table_open: TableOpen::new(),
          };
          // `Reopen` 延迟重试队列：manifest 暂不可见时挂起请求，期间本 shard 继续
          // 服务其它请求（对齐 C++ `pending_reopens_` + delayed reopen 提升）。
          let mut reopen_li = ReopenQueue::new(AutoReopenOption::new(
            config.auto_reopen_retry_times,
            Duration::from_micros(config.auto_reopen_pending_time_us),
          ));
          // 握手：store 与事件循环已就绪，调用方拿到 `Ok` 即保证请求会被消费。
          let _ = ready_tx.send(Ok(()));
          // 读批收集 / 读窗口退出时可能多取到非只读请求：按 FIFO 暂存，
          // 下一轮优先处理，保证与后续写 / 控制请求的先后顺序不变（eng-32a）。
          let mut pending: VecDeque<Envelope> = VecDeque::new();
          // manifest 出现后重排的请求（原挂起的读请求 / 后续 `Reopen`）：优先处理，
          // 但不与新到达的请求混批，保持它们在挂起前的相对顺序。
          let mut wake_li: VecDeque<Envelope> = VecDeque::new();
          // per-partition FIFO 写队列 + 全局写并发门（eng-33）；队列深度 / 在跑写
          // 通过 gauge 上报（eng-33b），读侧从 `Shard::metrics_snapshot` 取。
          let mut gate = WriteGate::new(config.max_write_concurrency, thread_metrics.clone());
          // 在飞读任务：读请求 spawn 成独立 compio task，主循环只做准入 / 打开 /
          // 回执（eng-32b）。任何需要独占 Store 的路径先 `finish_read_li`。
          // in-flight / 读批大小指标随 `ReadFlight` 上报（eng-32c）。
          let mut read_flight = ReadFlight::new(thread_metrics.clone());
          // 在飞写任务：高优先 Put 组按分区派发成独立 compio task，跨分区真并发；
          // 完成时把 Store 交回槽位并释放写门额度（eng-33a）。
          let mut write_flight = WriteFlight::new();
          // 目标分区正在写（Store 被写任务持有）的读请求：等写完成后再准入，
          // 不阻塞其它分区 / 其它请求。
          let mut blocked_read_li: VecDeque<Envelope> = VecDeque::new();
          // 高优先连续服务的起点：超过 `max_processing_time_us` 后先让一条
          // 低优先，防止后台被持续的前台流量饿死。
          let mut round_start = Instant::now();
          'shard_loop: loop {
            // 回收已完成的写任务：Store 放回分区槽位、释放写门额度、同步指标；
            // 再唤醒目标分区已空闲的阻塞读。
            while let Some(done) = write_flight.try_next() {
              finish_write(&mut state, &mut gate, &thread_metrics, done);
            }
            promote_blocked_reads(&state, &mut blocked_read_li, &mut pending);
            // 读并发窗口：在飞读由 compio 推进，主循环同时接收新请求；遇到
            // 非读请求 / 未打开表 / 配额满即 drain 并交回独占路径。
            if !read_flight.is_empty() {
              if pending.is_empty() {
                match read_window(
                  &mut state,
                  &thread_metrics,
                  &mut reopen_li,
                  &mut read_flight,
                  &rx_hi,
                  config.oom_retry_times,
                )
                .await
                {
                  ReadEvent::Drained => {}
                  ReadEvent::Exclusive(envelope) => {
                    if is_blocked_read(&state, &envelope) {
                      blocked_read_li.push_back(envelope);
                    } else {
                      // 保持先后顺序：该请求排在已暂存的非只读请求之后。
                      pending.push_back(envelope);
                    }
                    continue;
                  }
                }
              } else {
                // 还有更早的暂存请求未处理：先等在飞读完，回到独占路径按顺序处理。
                finish_read_li(
                  &mut state,
                  &thread_metrics,
                  &mut reopen_li,
                  &mut read_flight,
                  config.oom_retry_times,
                )
                .await;
              }
            }
            // 把上一轮请求产生的 index 缓存计数推入 shard 指标；计数器单调，
            // 单点同步即可覆盖读批 / 写队列 / 控制请求全部路径。
            sync_cache_metrics(&mut state, &thread_metrics);
            // 先跑到期的重试再等请求：挂起条目非空时按最早到期时间等待，睡够即回
            // 循环顶部重试，绝不在请求任务内 `sleep` 阻塞整个 shard。
            run_due_reopen(&mut state, &mut reopen_li, &mut wake_li).await;
            // 上一轮为给低优先让路而暂存的写请求继续推进；高优先写不在此被饿。
            if gate.has_ready()
              && drain_write_gate(
                &mut state,
                &thread_metrics,
                &mut reopen_li,
                &mut gate,
                &mut read_flight,
                &mut write_flight,
                config.oom_retry_times,
              )
              .await
            {
              break;
            }
            // 选下一条请求：wake / 暂存队列与高优先通道先跑；低优先只在没有
            // 前台请求，或高优先已连续占用超过时间片时各跑一条（eng-34）。
            let mut next = wake_li.pop_front().or_else(|| pending.pop_front());
            if next.is_none() {
              next = rx_hi.try_recv().ok();
            }
            // 时间片到点且有后台请求在等：先跑一条低优先，高优先暂存到
            // `pending` 下一轮先跑，保证后台不被持续前台流量饿死。
            if round_budget_due(config.max_processing_time_us, round_start) {
              // 预算到点就计数（eng-34c）：低优先通道为空时也算「前台霸占过时间片」。
              thread_metrics.inc_round_over_budget();
              // 到点即重置：通道为空时不清零会让后续每轮都重复计数。
              round_start = Instant::now();
              if let Ok(low) = rx_lo.try_recv() {
                thread_metrics.inc_yield();
                thread_metrics.dec_low_pq_depth();
                if let Some(envelope) = next.take() {
                  pending.push_front(envelope);
                }
                next = Some(low);
              }
            }
            if next.is_none()
              && let Ok(low) = rx_lo.try_recv()
            {
              thread_metrics.dec_low_pq_depth();
              next = Some(low);
              round_start = Instant::now();
            }
            let envelope = match next {
              Some(envelope) => envelope,
              None => {
                match wait_event(&rx_hi, &rx_lo, &mut write_flight, reopen_li.next_due()).await {
                  WaitEvent::Request(envelope) => {
                    // 空闲等待不计入高优先连续占用：否则等一小时后的第一条请求
                    // 会被立刻判定「超时片」而误让路。
                    round_start = Instant::now();
                    dec_low_pq_if_low(&thread_metrics, envelope)
                  }
                  WaitEvent::WriteDone(done) => {
                    // 没有就绪请求时也要推进写任务：完成后回顶部重放 pending / gate。
                    finish_write(&mut state, &mut gate, &thread_metrics, *done);
                    round_start = Instant::now();
                    continue;
                  }
                  WaitEvent::ReopenDue => {
                    // 等待到期不算前台占用；同时避免下一轮立即再次判定超时片。
                    round_start = Instant::now();
                    continue;
                  }
                  WaitEvent::Closed => {
                    // 两条通道都关闭：等在飞写落地后再退出。
                    write_flight
                      .drain(&mut state, &mut gate, &thread_metrics)
                      .await;
                    break;
                  }
                }
              }
            };
            // 连续只读请求成批同轮推进：不阻塞地多取，遇到非只读立即停止。
            // `pending` 非空说明还有更早的非只读请求未处理，不能再插批。
            let mut batch = vec![envelope];
            if wake_li.is_empty()
              && pending.is_empty()
              && batch
                .first()
                .is_some_and(|envelope| is_read_op(&envelope.req))
            {
              while batch.len() < MAX_READ_BATCH {
                match rx_hi.try_recv() {
                  Ok(next) if is_read_op(&next.req) => batch.push(next),
                  Ok(next) => {
                    // 非只读的高优先请求：留待下一轮先跑。
                    pending.push_back(next);
                    break;
                  }
                  Err(_) => break,
                }
              }
            }
            // 只读请求（含单条）：串行准入 + 打开后 spawn 成独立任务，进入读并发
            // 窗口；不再 `join_all` 串行等到批结束。
            if batch
              .first()
              .is_some_and(|envelope| is_read_op(&envelope.req))
            {
              let mut runnable = Vec::with_capacity(batch.len());
              for envelope in batch {
                if is_blocked_read(&state, &envelope) {
                  // 目标分区写进行中：等写完成再准入，不阻塞其它读。
                  blocked_read_li.push_back(envelope);
                } else {
                  runnable.push(envelope);
                }
              }
              if !runnable.is_empty() {
                submit_read_li(
                  &mut state,
                  &thread_metrics,
                  &mut reopen_li,
                  &mut read_flight,
                  runnable,
                )
                .await;
              }
              continue;
            }
            let Some(envelope) = batch.pop() else {
              continue;
            };
            // 写类请求进 per-partition FIFO 队列（eng-33）：同一分区严格串行，
            // 跨分区按 ready 顺序推进，全局并发由 `max_write_concurrency` 限流。
            // `Reopen` / AttachShard / DetachShard / Shutdown 是控制请求，不进队列：
            // 前者有独立的延迟重试队列，后者必须旁路，否则会与排空逻辑互等。
            if is_write_op(&envelope.req) {
              // 先连续收一批高优先写请求，再统一进 gate；这样 drain 时
              // 同分区的 Put 能被 envelope_li commit 合并（一次 manifest fsync）。
              let mut write_li = vec![envelope];
              if pending.is_empty() {
                while write_li.len() < MAX_WRITE_COLLECT {
                  match rx_hi.try_recv() {
                    Ok(next) if is_write_op(&next.req) => write_li.push(next),
                    Ok(next) => {
                      pending.push_back(next);
                      break;
                    }
                    Err(_) => break,
                  }
                }
              }
              for envelope in write_li {
                let Some(tbl) = envelope.req.table().cloned() else {
                  // 写类请求均带分区，此臂只为防御性兜底。
                  if process_envelope(
                    &mut state,
                    &thread_metrics,
                    &mut reopen_li,
                    envelope,
                    config.oom_retry_times,
                  )
                  .await
                  {
                    break 'shard_loop;
                  }
                  continue;
                };
                let pri = pri_of(&envelope.req);
                gate.push(tbl, (envelope, pri));
              }
              if drain_write_gate(
                &mut state,
                &thread_metrics,
                &mut reopen_li,
                &mut gate,
                &mut read_flight,
                &mut write_flight,
                config.oom_retry_times,
              )
              .await
              {
                break;
              }
              continue;
            }
            let was_detach = matches!(envelope.req, Request::DetachShard { .. });
            let was_attach = matches!(envelope.req, Request::AttachShard { .. });
            let lifecycle_before = state.lifecycle;
            // 控制请求（Reopen / ReopenExternal / Attach / Detach / Shutdown）可能改
            // 全局身份或取全部分区 Store 的独占：先等在飞写全部落地。
            write_flight
              .drain(&mut state, &mut gate, &thread_metrics)
              .await;
            let stop = process_envelope(
              &mut state,
              &thread_metrics,
              &mut reopen_li,
              envelope,
              config.oom_retry_times,
            )
            .await;
            if stop || write_queue_stale(was_attach, was_detach, lifecycle_before, state.lifecycle)
            {
              // detach / Fresh attach / shutdown：排队中的写与阻塞读都属于旧身份，
              // 不能在新 branch / term 下执行，统一回 NotRunning。
              fail_pending(gate.clear().into_iter().map(|(envelope, _)| envelope));
              fail_pending(blocked_read_li.drain(..));
              // 挂起的 reopen 同样带旧 (branch, term)：Fresh attach 后不能拿新身份
              // 重跑旧请求（会读到另一 branch 的数据），统一失败。
              reopen_li.fail_all(KvError::NotRunning);
            }
            if stop {
              break;
            }
          }
          info!(
            target: "fkv_shard",
            "shard stop shard_id={}",
            thread_metrics.shard_id()
          );
        });
      })
      .map_err(|_| KvError::IoFail)?;
    // 线程提前退出（初始化失败 / panic）时 recv 出错，统一按初始化失败处理。
    if let Err(err) = ready_rx.recv().unwrap_or(Err(KvError::IoFail)) {
      let _ = join.join();
      return Err(err);
    }
    Ok(Self {
      tx_hi,
      tx_lo,
      join: Some(join),
      metrics,
      lifecycle,
    })
  }

  pub fn metrics(&self) -> &Arc<ShardMetric> {
    &self.metrics
  }

  pub fn metrics_snapshot(&self) -> ShardSnapshot {
    self.metrics.snapshot()
  }

  /// 生命周期镜像：shard 线程内的真实状态（跨线程 `AtomicU8`）。
  pub fn lifecycle(&self) -> &ShardLifecycle {
    &self.lifecycle
  }

  /// 启动配置的 shard id（attach / detach / term 的作用域）。
  pub fn shard_id(&self) -> u32 {
    self.metrics.shard_id()
  }

  pub async fn request(&self, req: Request) -> Result<Response> {
    self.sender().request(req).await
  }

  pub fn sender(&self) -> ShardSender {
    ShardSender {
      tx_hi: self.tx_hi.clone(),
      tx_lo: self.tx_lo.clone(),
      metrics: self.metrics.clone(),
      request_timeout: None,
      queue_timeout: None,
    }
  }

  pub async fn shutdown(mut self) -> Result<()> {
    let _ = self.request(Request::Shutdown).await;
    if let Some(join) = self.join.take() {
      join.join().map_err(|_| KvError::IoFail)?;
    }
    Ok(())
  }
}

impl Drop for Shard {
  fn drop(&mut self) {
    // 未显式 `shutdown` 就 Drop：队列发送端断开后事件循环处理完在途请求即退出，
    // 资源仍会释放，但调用方拿不到清理完成时机与错误。此处只告警，不在 Drop 里
    // join：Drop 可能落在 async 任务 poll 栈上，等待在途长请求（如 auto_reopen
    // 退避、大文件 compact）会长时间阻塞该线程；需要确定性清理请调用 `shutdown`。
    if self.join.is_some() {
      warn!(
        target: "fkv_shard",
        "shard dropped without shutdown shard_id={}",
        self.metrics.shard_id()
      );
    }
  }
}

/// 打开目标表：读路径唯一需要 `&mut Store` 的动作，只允许在串行准入路径调用。
///
/// 幂等 single-flight：`Store::is_open` 为权威，已打开时只补登记不重复 replay；
/// 同一批读请求里对同一张表只会在第一次准入时真正打开。
async fn ensure_table_ready(state: &mut ShardState, tbl: &TableIdent) -> Result<()> {
  if state.stores.is_open(tbl) {
    state.table_open.mark(tbl, state.term, &state.branch);
    state.stores.touch(tbl);
    return Ok(());
  }
  // 写任务正持有该分区 Store：不能新建（会与在飞写分叉），交回调用方等待。
  if state.stores.is_writing(tbl) {
    return Err(KvError::Busy);
  }
  let branch = state.branch.clone();
  let term = state.term.unwrap_or(0);
  let reopen_on_missing = state.reopen_on_missing;
  // `StandbyReplica`：懒开前先 `sync_standby_partition`。本地后端（`CompioStore`）
  // 的 `read_manifest` 只读本地文件，没有 Cloud 的回源护栏；不先追 master 会
  // replay 上次 rsync 留下的旧 manifest（副本重启 / 换主后静默读旧 root）。
  // 这是**有意超越 C++ 基线**：C++ 懒开路径 `FindRoot` 也是纯本地 replay，
  // 新鲜度依赖控制面发 Reopen；fkv 在副本重启后无进程内收敛机制，故在懒开
  // 入口补一次追赶。失败 `ResourceMissing` 交给上层挂等待重排，不能装空表。
  if branch == MAIN_BRANCH_NAME && state.standby.is_some() {
    sync_standby_partition(state, tbl).await?;
  }
  // 新分区 Store 采纳当前 attach 身份后加载该表。
  let result = {
    let store = state.stores.ensure_store(tbl, &branch, term)?;
    if branch == MAIN_BRANCH_NAME {
      match store.reopen(tbl).await {
        Ok(()) => Ok(()),
        // 无 manifest 的首次访问：Local / Mem 下装一个空表状态（首次写时惰性创建，
        // 之后 is_open 为 true），对齐 C++ 懒创建语义。Cloud / StandbyReplica 下
        // 不能懒创建：manifest 只是暂不可见（远端已有数据），装成空表会静默丢读，
        // 且 manifest 之后可见也不会再读该表；把 `ResourceMissing` 交给上层挂等待重排
        // （对齐 C++ `auto_reopen`）。
        Err(KvError::ResourceMissing) if !reopen_on_missing => {
          store.open_empty(tbl);
          Ok(())
        }
        Err(err) => Err(err),
      }
    } else {
      // attach term 直接交给 store 安装 replay：branch 区间必须记当前 identity，
      // 否则 attach 后写出的 `_T` 对象在远端按 `_0` 找不到。
      store.open_branch(tbl, &branch, term).await
    }
  };
  result?;
  // term 与 Store 不一致时才重设：`set_term` 会清共享 IO 缓存，不能每表打开都做。
  let store = state.stores.ensure_store(tbl, &branch, term)?;
  if store.term() != term {
    store.set_term(term);
  }
  state.table_open.mark(tbl, state.term, &branch);
  state.stores.touch(tbl);
  Ok(())
}

/// 挂起的等 manifest 条目：manifest 暂不可见时延迟尝试一次 `reopen`，期间 shard 继续
/// 服务其它请求。同表只保留一条（`park` 合并 waiter），对齐 C++
/// `PendingReopenState` + `delayed_requests_` 堆：driver 每次**只尝试一次**，
/// 失败即连坐全部 waiter（`CompleteReopenWaiters(err)`），不由 driver 自己重试。
struct PendingReopen {
  tbl: TableIdent,
  /// 下次尝试时间（堆排序键）。
  due: Instant,
  /// 等 manifest 可见的请求（读请求，以及 driver 之后到达的 `Reopen`）。
  waiter_li: Vec<Envelope>,
}

impl PendingReopen {
  /// driver 拿到 manifest：waiter 交回主循环重新投递（外部 `Reopen` 也只作为
  /// waiter 搭车，成功时随读请求一起被重排、由 `Request::Reopen` 正常回执）。
  fn wake(self, wake_li: &mut VecDeque<Envelope>) {
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
struct DelayedReopen {
  due: Instant,
  entry: PendingReopen,
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

/// 等 manifest 可见的延迟重试队列。
///
/// 一个表最多一条：新的 `ResourceMissing` 请求合并进同表条目，不新增延迟项
/// （对齐 C++ `pending_reopens_` 每表一个状态 + delayed heap 一次提升）。
struct ReopenQueue {
  heap: BinaryHeap<DelayedReopen>,
  /// 每请求 auto-reopen 轮数种子（对齐 C++ `auto_reopen_retry_times` 语义）。
  retry_times: u8,
  pending_time: Duration,
}

impl ReopenQueue {
  fn new(option: AutoReopenOption) -> Self {
    Self {
      heap: BinaryHeap::new(),
      retry_times: option.retry_times,
      pending_time: option.pending_time,
    }
  }

  /// 最早到期时间；无挂起条目时为 `None`（主循环据此决定是否带超时等待）。
  fn next_due(&self) -> Option<Instant> {
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
  fn park(&mut self, mut envelope: Envelope) -> Option<Envelope> {
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

  /// 以指定错误结束全部挂起条目（detach / shutdown 清理）。
  fn fail_all(&mut self, err: KvError) {
    for delayed in self.heap.drain() {
      delayed.entry.fail(err);
    }
  }
}

/// 外部 `Reopen`（含空 tag 的 `ReopenArchive`）：C++ 里由调用方自己退避重试，
/// shard 不把它当 driver 挂起。
fn is_external_reopen(req: &Request) -> bool {
  matches!(req, Request::Reopen { .. })
    || matches!(req, Request::ReopenArchive { tag, .. } if tag.is_empty())
}

/// 跑到期的延迟重新打开尝试（每个条目**只尝试一次**）：
/// 成功则重排 waiter；失败（含仍 `ResourceMissing`）立即连坐 driver 与 waiter，
/// 对齐 C++ `CompleteReopenWaiters(err)`。整轮不阻塞在途。
///
/// 只重试 `ResourceMissing`、不用 [`KvError::is_retryable`]：这是 manifest 可见性
/// 等待，而非 `TryAgain/Busy/OpenFileLimit` 的瞬时 IO 错误分类。
async fn run_due_reopen(
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
      delayed.due = Instant::now() + reopen_li.pending_time;
      delayed.entry.due = delayed.due;
      reopen_li.heap.push(delayed);
      continue;
    }
    let req = Request::Reopen {
      tbl: delayed.entry.tbl.clone(),
    };
    match handle(state, &req).await {
      Ok(_) => delayed.entry.wake(wake_li),
      // driver 只尝试一次：失败即让全部 waiter 拿到同一错误，不重排队。
      Err(err) => delayed.entry.fail(err),
    }
  }
}

/// 毫秒墙钟，用于 TTL 判定。
fn wall_clock_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn op_of(req: &Request) -> Option<Op> {
  match req {
    Request::Put { .. } | Request::PutLarge { .. } => Some(Op::Put),
    Request::Get { .. } | Request::GetLarge { .. } => Some(Op::Get),
    Request::Scan { .. } => Some(Op::Scan),
    Request::Reopen { .. } => Some(Op::Reopen),
    Request::ReopenExternal { .. } => Some(Op::Reopen),
    // C++ `ReopenRequest`（含 tag）就是 `RequestType::Reopen`。
    Request::ReopenArchive { .. } => Some(Op::Reopen),
    // C++ `FileGcRequest` / `LocalGcRequest` 各自对应一个 `RequestType`。
    Request::FileGc { .. } => Some(Op::FileGc),
    Request::LocalGc { .. } => Some(Op::LocalGc),
    Request::Compact { .. } => Some(Op::Compact),
    Request::Truncate { .. } => Some(Op::Truncate),
    Request::Floor { .. } => Some(Op::Floor),
    Request::Drop { .. } => Some(Op::Drop),
    Request::CleanExpired { .. } => Some(Op::CleanExpired),
    Request::CreateArchive { .. } => Some(Op::CreateArchive),
    // C++ `ArchiveRequest::Action::Delete` 的 `Type()` 同为 `Archive`（指标
    // 标签 `archive`），沿用 `Op::CreateArchive` 保证跨语言对账唯一。
    Request::DeleteArchive { .. } => Some(Op::CreateArchive),
    Request::CreateBranch { .. } => Some(Op::CreateBranch),
    Request::DeleteBranch { .. } => Some(Op::DeleteBranch),
    // fkv 扩展的 Cloud 预热请求；C++ 预热由 `PrewarmService` 后台触发，没有
    // 同名 `RequestType`，因此 `Op::Prewarm` 的 C++ 标签集为空。
    Request::Prewarm { .. } => Some(Op::Prewarm),
    // per-table 归档列举：C++ 只有未注册指标的 `global_list_archive_tags`，
    // 这里给 fkv 自有的控制面 Op，不参与跨语言对账。
    Request::ListArchiveTags { .. } => Some(Op::ListArchiveTags),
    Request::ListObject { .. } => Some(Op::ListObject),
    Request::ListStandbyPartition => Some(Op::ListStandbyPartition),
    Request::AttachShard { .. } => Some(Op::AttachShard),
    Request::DetachShard { .. } => Some(Op::DetachShard),
    Request::Shutdown => Some(Op::Shutdown),
  }
}

struct ShardState {
  /// 每分区独立 Store（共享 IO 后端 / index 缓存账本）：跨分区写可并发。
  stores: PartitionStoreMap,
  fixed_buffer: Option<FixedBuffer>,
  /// `StandbyReplica`：reopen 前的分区追赶服务。
  standby: Option<Arc<RsyncStandbyService>>,
  lifecycle: DsShardState,
  /// `lifecycle` 的跨线程镜像，随每次赋值同步写入。
  lifecycle_mirror: ShardLifecycle,
  term: Option<u64>,
  branch: String,
  /// 启动配置的 shard id：attach 必须与之相同（shard 身份不可在运行期改绑）。
  shard_id: u32,
  /// 存储层 data 文件页数位移：`Gc` / `Compact` 请求必须与之一致。
  pages_per_file_shift: u8,
  /// Cloud / StandbyReplica：`ResourceMissing` 表示 manifest 暂不可见，读请求挂
  /// 等待重排，且不做本地空表懒创建（对齐 C++ `auto_reopen`）。
  reopen_on_missing: bool,
  /// 每表打开状态：读请求准入只做只读查询，打开动作收敛在串行路径（eng-32b）。
  table_open: TableOpen,
}

/// 每表打开状态：读路径只读查询，打开动作只发生在串行的准入路径。
///
/// `Store::is_open` 仍是权威事实源；本结构额外记录「打开时的 attach 代际与
/// 身份」，用于读任务完成时拒绝跨代结果。attach / detach / shutdown 会先 drain
/// 在飞读再改代际，因此正常情况下不会出现跨代结果，这里是线性化的第二道保险。
struct TableOpen {
  /// attach 代际：attach / detach 成功时自增。
  epoch: u64,
  /// 已打开表 → 打开时的代际与 term / branch。
  slot_map: HashMap<TableIdent, TableSlot>,
}

#[derive(Clone, Debug)]
struct TableSlot {
  epoch: u64,
  term: Option<u64>,
  branch: String,
}

impl TableOpen {
  fn new() -> Self {
    Self {
      epoch: 0,
      slot_map: HashMap::default(),
    }
  }

  /// 只读就绪判定：Store 已打开、登记在当前 attach 代际，且 term / branch
  /// 与当前身份一致。
  ///
  /// 登记缺失 / 代际或身份不符按「未就绪」处理，由串行打开路径补齐；这样登记表
  /// 与 `Store` 的淘汰（detach / drop_table / max_open_tables）不会产生
  /// 「登记为已打开、实际不存在」的静默丢读。
  fn ready(
    &self,
    stores: &PartitionStoreMap,
    tbl: &TableIdent,
    term: Option<u64>,
    branch: &str,
  ) -> bool {
    stores.is_open(tbl) && self.slot_ready(tbl, term, branch)
  }

  /// 登记表侧的判定（不含 Store 实际打开状态）：代际与身份必须完全一致。
  fn slot_ready(&self, tbl: &TableIdent, term: Option<u64>, branch: &str) -> bool {
    self
      .slot_map
      .get(tbl)
      .is_some_and(|slot| slot.epoch == self.epoch && slot.term == term && slot.branch == branch)
  }

  /// 打开成功后登记（幂等）。
  fn mark(&mut self, tbl: &TableIdent, term: Option<u64>, branch: &str) {
    self.slot_map.insert(
      tbl.clone(),
      TableSlot {
        epoch: self.epoch,
        term,
        branch: branch.to_owned(),
      },
    );
  }

  fn forget(&mut self, tbl: &TableIdent) {
    self.slot_map.remove(tbl);
  }

  /// 身份变化（attach / detach）：代际 +1，清空全部表登记。
  fn bump(&mut self) {
    self.epoch = self.epoch.wrapping_add(1);
    self.slot_map.clear();
  }
}

impl ShardState {
  /// 独占某分区的 `Store`。
  ///
  /// 读任务持有该分区 `Rc<ShardStore>` 克隆、或写任务正持有 Store 时返回
  /// `Busy`；因此所有需要 `&mut Store` 的路径必须先 drain 在飞读 / 写。
  /// 这里的 `Busy` 只是防御性兜底，正常路径不会走到。
  fn store_mut(&mut self, tbl: &TableIdent) -> Result<&mut ShardStore> {
    match self.stores.store_mut(tbl) {
      Err(KvError::Busy) => {
        debug_assert!(
          false,
          "分区 Store 被在飞任务持有：调用前必须先 drain 读 / 写"
        );
        Err(KvError::Busy)
      }
      other => other,
    }
  }

  /// 确保分区槽位存在（不打开表）。
  ///
  /// 归档 / 远端列举 / `ReopenExternal` 等控制面请求只依赖 IO 与 branch 身份，
  /// 不应为它们懒创建空表；但每分区 Store 是这些操作的载体，需要先建槽。
  fn ensure_slot(&mut self, tbl: &TableIdent) -> Result<()> {
    if self.stores.has_slot(tbl) {
      return Ok(());
    }
    let branch = self.branch.clone();
    let term = self.term.unwrap_or(0);
    self.stores.ensure_store(tbl, &branch, term)?;
    Ok(())
  }
}

/// 单个 data store shard 在 shard 线程内的生命周期，对齐 C++ `DsShardState`。
///
/// C++ 的 `Attaching` / `Detaching` 用于跨线程 CAS 以及「中间态拒绝请求」；
/// 本实现的控制请求由 shard 线程串行处理，中间态不会跨越两个请求，故不落成状态。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DsShardState {
  /// 未托管：普通请求快速失败 `NotRunning`。
  #[default]
  Detached,
  /// 正常托管：普通请求放行。
  Attached,
  /// 更高 term 已接管：普通请求快速失败 `ExpiredTerm`，直到同一 branch
  /// 以不低于当前 term 重新 attach 或 detach（对齐 C++ `MarkShardExpired`）。
  Expired,
}

/// attach 的转换结果：是否需要重建 store 状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttachKind {
  /// 已托管同一 shard / branch / term：幂等，不驱逐 table 状态。
  Idempotent,
  /// 从 `Detached` 进入，或 `Expired` 后重新托管：需要 claim term 并重置 Store。
  Fresh,
}

impl ShardState {
  /// 生命周期唯一写入口：线程内状态与跨线程镜像必须同步，否则运行时会在错误
  /// 之后按陈旧状态决定是否清视图。
  fn set_lifecycle(&mut self, next: DsShardState) {
    self.lifecycle = next;
    self.lifecycle_mirror.store(next);
  }

  /// attach 合法转换判定。`Attached` / `Expired` 下只接受同一 shard 与 branch：
  /// 换 branch 或换 shard 必须先 detach（对齐 C++ `AttachShard` 要求 slot `Detached`）。
  fn admit_attach(&mut self, branch: &str, shard_id: u32, term: u64) -> Result<AttachKind> {
    let same_owner = self.shard_id == shard_id && self.branch == branch;
    match self.lifecycle {
      DsShardState::Detached => Ok(AttachKind::Fresh),
      DsShardState::Attached | DsShardState::Expired if !same_owner => Err(KvError::InvalidArgs),
      DsShardState::Attached => match self.term {
        Some(current) if term == current => Ok(AttachKind::Idempotent),
        // 旧 term 重新 attach：标记过期，保持快速失败（对齐 C++ term 文件 CAS）。
        Some(current) if term < current => {
          self.set_lifecycle(DsShardState::Expired);
          Err(KvError::ExpiredTerm)
        }
        // 已托管时提 term 属于换主，必须先 detach 排出旧状态。
        Some(_) => Err(KvError::InvalidArgs),
        None => Err(KvError::InvalidArgs),
      },
      DsShardState::Expired => match self.term {
        Some(current) if term < current => Err(KvError::ExpiredTerm),
        // 同 term 或更高 term 重新 attach 即恢复（运行时错误恢复路径依赖此语义）。
        _ => Ok(AttachKind::Fresh),
      },
    }
  }

  /// detach 合法转换判定。`Detached` 幂等成功；`shard_id` 不匹配是非法转换。
  fn admit_detach(&self, shard_id: u32) -> Result<bool> {
    match self.lifecycle {
      DsShardState::Detached => Ok(false),
      DsShardState::Attached | DsShardState::Expired if self.shard_id != shard_id => {
        Err(KvError::InvalidArgs)
      }
      DsShardState::Attached | DsShardState::Expired => Ok(true),
    }
  }
}

/// 把所有分区 Store 的 index 缓存累计计数与占用推入 shard 指标。
fn sync_cache_metrics(state: &mut ShardState, metrics: &ShardMetric) {
  state.stores.sync_cache_metrics(metrics);
}

/// 写后维护：compact 放大文件 + 回收零引用文件 + 清理延迟 TTL。
/// 对齐 C++ `WriteTask::UpdateMeta` 提交后调度的 `Compact` / `FileGc` /
/// `CleanExpired`；调用点在写请求回复之后，失败只告警，不影响写结果。
async fn maintain_after_write(state: &mut ShardState, tbl: &TableIdent) {
  let needs_maintain = state.stores.needs_maintain(tbl);
  let pending_ttl = state.stores.pending_ttl_len() > 0;
  match state.store_mut(tbl) {
    Ok(store) => maintain_store(store, tbl, needs_maintain, pending_ttl).await,
    Err(err) => warn!(
      target: "fkv_shard",
      "post-write maintain skipped tbl={} error={}",
      tbl,
      err
    ),
  }
}

/// 写后维护的 Store 级实现；写任务在任务内直接复用（Store 由任务独占）。
async fn maintain_store(
  store: &mut ShardStore,
  tbl: &TableIdent,
  needs_maintain: bool,
  pending_ttl: bool,
) {
  if needs_maintain && let Err(err) = store.maintain(tbl).await {
    warn!(
      target: "fkv_shard",
      "post-write maintain failed tbl={} error={}",
      tbl,
      err
    );
  }
  if pending_ttl && let Err(err) = store.drain_pending_ttl().await {
    warn!(
      target: "fkv_shard",
      "post-write ttl drain failed error={}",
      err
    );
  }
}

/// 只读请求：只借 `&Store`，可提交为独立 compio task 并发推进（eng-32b）。
///
/// `GetLarge` 不在列：它借用 `fixed_buffer.as_mut()`，仍需串行。
/// `ListArchiveTags` / `ListObject` 同样只读（列 manifest 名 / 远端对象），
/// 不需要 `&mut Store`，可与普通读并发。
fn is_read_op(req: &Request) -> bool {
  matches!(
    req,
    Request::Get { .. }
      | Request::Scan { .. }
      | Request::Floor { .. }
      | Request::ListArchiveTags { .. }
      | Request::ListObject { .. }
  )
}

/// 写类请求：进 per-partition FIFO 写队列，同一分区严格串行（对齐 C++
/// `PendingWriteQueue`）。`Reopen` / `AttachShard` / `DetachShard` / `Shutdown`
/// 是控制请求，不进队列：`Reopen` 有独立的延迟重试队列，其余必须旁路，
/// 否则会与排空逻辑互等。
///
/// `Prewarm` 会写本地 cache 文件（与同分区写路径可能争用文件名），因此也进
/// per-partition 队列串行化；它不需要 `&mut Store`，队列只提供顺序与并发门。
fn is_write_op(req: &Request) -> bool {
  matches!(
    req,
    Request::Put { .. }
      | Request::PutLarge { .. }
      | Request::Truncate { .. }
      | Request::Drop { .. }
      | Request::FileGc { .. }
      | Request::LocalGc { .. }
      | Request::Compact { .. }
      | Request::CreateArchive { .. }
      | Request::ReopenArchive { .. }
      | Request::DeleteArchive { .. }
      | Request::CreateBranch { .. }
      | Request::DeleteBranch { .. }
      | Request::CleanExpired { .. }
      | Request::Prewarm { .. }
  )
}

/// `ResourceMissing` 时挂等待 manifest 的读请求。对齐 C++ `AutoReopenRetry`：
/// 只有读请求（`Read` / `Floor` / `Scan`，以及 fkv 的大值读）自动重排；
/// 写请求直接失败，由调用方决定是否重试。
fn is_auto_reopen_read(req: &Request) -> bool {
  matches!(
    req,
    Request::Get { .. } | Request::GetLarge { .. } | Request::Floor { .. } | Request::Scan { .. }
  )
}

/// 高优先是否已连续服务超过时间片（`budget_us == 0` 表示关闭）。
///
/// 到点且有后台积压时，主循环先跑一条低优先再回到前台，对齐 C++
/// `ExecuteReadyTasks` 的「至少跑一个低优先防饿死」。
fn round_budget_due(budget_us: u64, round_start: Instant) -> bool {
  budget_us != 0
    && u64::try_from(round_start.elapsed().as_micros()).unwrap_or(u64::MAX) >= budget_us
}

/// 取到的是低优先请求时同步深度 gauge（eng-34c）。
///
/// 只在 `recv_prioritized` 的返回处调用：`try_recv` 弹出的两处（预算让路 /
/// 空闲）已经各自记账，在公共出口再减一次会双减。
fn dec_low_pq_if_low(metrics: &ShardMetric, envelope: Envelope) -> Envelope {
  if pri_of(&envelope.req) == Pri::Low {
    metrics.dec_low_pq_depth();
  }
  envelope
}

/// 等待下一条请求：高优先通道优先；一侧关闭后继续抽另一侧，两侧都关闭才
/// 返回 `None`。
///
/// 用 `futures::select` 而非跨线程阻塞 select：本函数跑在 shard 的 poll 栈上，
/// 禁止阻塞线程（compio thread-per-core 纪律）。
async fn recv_prioritized(
  rx_hi: &MAsyncRx<Array<Envelope>>,
  rx_lo: &MAsyncRx<Array<Envelope>>,
) -> Option<Envelope> {
  match select(Box::pin(rx_hi.recv()), Box::pin(rx_lo.recv())).await {
    Either::Left((Ok(envelope), _)) | Either::Right((Ok(envelope), _)) => Some(envelope),
    // 高优先通道关闭：低优先通道仍可能有货。
    Either::Left((Err(_), _)) => rx_lo.recv().await.ok(),
    Either::Right((Err(_), _)) => rx_hi.recv().await.ok(),
  }
}

/// 主循环的等待事件：新请求 / 写任务完成 / reopen 到期 / 通道关闭。
enum WaitEvent {
  Request(Envelope),
  /// Boxed：`WriteDone` 内含整个分区 `Store`，避免枚举被单一大变体撑大。
  WriteDone(Box<WriteDone>),
  ReopenDue,
  Closed,
}

/// 等待下一个可推进事件：请求、写任务完成、或到期重试。
///
/// 有在飞写时必须同时 poll 写完成：否则「pending 里的读在等某个分区写结束」
/// 时主循环会一直卡在 recv 上。`reopen_due` 已到期且仍有在飞写时不立即返回，
/// 先等一次写完成（写任务总会结束，避免零时长超时导致忙转）。
async fn wait_event(
  rx_hi: &MAsyncRx<Array<Envelope>>,
  rx_lo: &MAsyncRx<Array<Envelope>>,
  write_flight: &mut WriteFlight,
  reopen_due: Option<Instant>,
) -> WaitEvent {
  let recv = recv_prioritized(rx_hi, rx_lo);
  if write_flight.is_empty() {
    return match reopen_due {
      Some(due) => {
        // 用剩余时长而非绝对时刻：`due` 可能刚过去几微秒，`timeout_at` 会在
        // `deadline - now` 上 panic；0 时长即刻到期，回到顶部重试。
        let wait = due.saturating_duration_since(Instant::now());
        match timeout(wait, recv).await {
          Ok(Some(envelope)) => WaitEvent::Request(envelope),
          Ok(None) => WaitEvent::Closed,
          Err(_) => WaitEvent::ReopenDue,
        }
      }
      None => match recv.await {
        Some(envelope) => WaitEvent::Request(envelope),
        None => WaitEvent::Closed,
      },
    };
  }
  let select_fut = select(Box::pin(write_flight.next()), Box::pin(recv));
  // 到期但仍在写：等「写完成 / 新请求」，不设零时长超时（否则空转）。
  let wait = reopen_due
    .map(|due| due.saturating_duration_since(Instant::now()))
    .filter(|wait| !wait.is_zero());
  let outcome = match wait {
    Some(wait) => timeout(wait, select_fut).await.ok(),
    None => Some(select_fut.await),
  };
  match outcome {
    None => WaitEvent::ReopenDue,
    Some(Either::Left((Some(done), _))) => WaitEvent::WriteDone(Box::new(done)),
    Some(Either::Left((None, _))) => WaitEvent::Closed,
    Some(Either::Right((Some(envelope), _))) => WaitEvent::Request(envelope),
    Some(Either::Right((None, _))) => WaitEvent::Closed,
  }
}

/// 读目标分区是否正被写任务占用（Store 不在槽位里）。
fn is_blocked_read(state: &ShardState, envelope: &Envelope) -> bool {
  is_read_op(&envelope.req)
    && envelope
      .req
      .table()
      .is_some_and(|tbl| state.stores.is_writing(tbl))
}

/// 把已解除阻塞的读放回 `pending`；仍在写的留在原队列。
fn promote_blocked_reads(
  state: &ShardState,
  blocked: &mut VecDeque<Envelope>,
  pending: &mut VecDeque<Envelope>,
) {
  if blocked.is_empty() {
    return;
  }
  let mut remain = VecDeque::with_capacity(blocked.len());
  while let Some(envelope) = blocked.pop_front() {
    if envelope
      .req
      .table()
      .is_some_and(|tbl| state.stores.is_writing(tbl))
    {
      remain.push_back(envelope);
    } else {
      pending.push_back(envelope);
    }
  }
  *blocked = remain;
}

/// 准入检查：attach 状态与 term 失效。对齐 C++ `AdmitRequest`：
/// 整个 data store shard 一旦 ExpiredTerm，除 attach/detach/shutdown 外全部快速失败。
fn admit(state: &ShardState, req: &Request) -> Result<()> {
  let control = matches!(
    req,
    Request::AttachShard { .. } | Request::DetachShard { .. } | Request::Shutdown
  );
  if control {
    return Ok(());
  }
  match state.lifecycle {
    DsShardState::Attached => Ok(()),
    DsShardState::Detached => Err(KvError::NotRunning),
    DsShardState::Expired => Err(KvError::ExpiredTerm),
  }
}

/// 读写前确保目标表已打开。`Reopen` / `ReopenArchive` 自己会执行 auto_reopen；
/// main branch 下无需先 ensure 再 reopen（否则首次访问同一 manifest 读两次）。
/// `DeleteArchive` 只删归档文件，不应为了删归档而给未打开的分区建空表。
/// 同理，`Prewarm` / `ListArchiveTags` / `ListObject` 是控制面操作：
/// 预热只拉远端文件、列举只看文件名 / 远端对象，都不应触发本地空表懒创建。
/// 不依赖「表已打开」的控制面请求。
///
/// 这些请求只碰 manifest 文件名 / 远端对象 / 本地 cache，不应为了执行它们而
/// 触发本地空表懒创建或等待 manifest 可见：
/// - `ReopenArchive` / `DeleteArchive`：归档文件读写；
/// - `Prewarm`：只拉远端文件到本地 cache；
/// - `ListArchiveTags`：只列 manifest 名；
/// - `ListObject`：只列远端对象。
///
/// 读窗口的准入（`window_admit`）与读任务提交（`submit_read_li`）也复用它，
/// 否则 `ListArchiveTags` / `ListObject` 会先 `ensure_table_ready` 再执行，
/// 在 Cloud 上无 manifest 的分区会被误判为 `ResourceMissing`。
fn skips_table_open(req: &Request) -> bool {
  matches!(
    req,
    Request::ReopenArchive { .. }
      | Request::DeleteArchive { .. }
      | Request::Drop { .. }
      | Request::Prewarm { .. }
      | Request::ListArchiveTags { .. }
      | Request::ListObject { .. }
  )
}

async fn prepare_table(state: &mut ShardState, req: &Request) -> Result<()> {
  let skip_ensure = if skips_table_open(req) {
    true
  } else {
    match req {
      Request::Reopen { .. } | Request::ReopenExternal { .. } => state.branch == MAIN_BRANCH_NAME,
      _ => false,
    }
  };
  if !skip_ensure && let Some(tbl) = req.table() {
    // CreateArchive / CreateBranch / DeleteBranch 对“尚无 manifest 的空分区”
    // 是对齐 C++ 的成功无操作，不是错误：尝试打开（远端已有 manifest 时仍能
    // 正常操作），仅 `ResourceMissing` 被忽略。
    let allow_missing = matches!(
      req,
      Request::CreateArchive { .. } | Request::CreateBranch { .. } | Request::DeleteBranch { .. }
    );
    match ensure_table_ready(state, tbl).await {
      Ok(()) => {}
      Err(KvError::ResourceMissing) if allow_missing => {}
      // Cloud main 的首写：远端没有 manifest 等价于空表（对齐 C++
      // `MakeCowRoot` 的 NotFound → stub）。读请求仍交给 auto-reopen park，
      // StandbyReplica / branch 不做隐式创建。
      Err(KvError::ResourceMissing)
        if state.reopen_on_missing
          && state.standby.is_none()
          && state.branch == MAIN_BRANCH_NAME
          && !is_read_op(req)
          && is_write_op(req) =>
      {
        let branch = state.branch.clone();
        let term = state.term.unwrap_or(0);
        state
          .stores
          .ensure_store(tbl, &branch, term)?
          .open_empty(tbl);
        state.table_open.mark(tbl, state.term, &branch);
        state.stores.touch(tbl);
      }
      Err(err) => return Err(err),
    }
  }
  Ok(())
}

async fn handle(state: &mut ShardState, req: &Request) -> Result<Response> {
  admit(state, req)?;
  prepare_table(state, req).await?;
  dispatch(state, req).await
}

/// `StandbyReplica`：reopen 前把该分区从 master 追一次。
///
/// 对齐 C++ `ReopenTask::Reopen`：非 standby（`standby == None`）直接返回；
/// 同步用当前 attach 到的 term 做 fencing（C++ 用 `store_->Term()`）。
/// 远端分区已不存在（`NotFound` / `ResourceMissing`）时按 C++ 的 `clear_local_state`
/// 语义只清本地 manifest（绝不删远端对象），并把 `ResourceMissing` 交给上层的
/// reopen 延迟队列重试；其余错误原样上抛。
async fn sync_standby_partition(state: &mut ShardState, tbl: &TableIdent) -> Result<()> {
  let Some(service) = state.standby.clone() else {
    return Ok(());
  };
  let result = service
    .sync_table_with_term(tbl, state.term.unwrap_or(0))
    .await;
  // 同步用 rename 替换本地 data / segment / manifest：旧 inode 的 FD 缓存与
  // 路径索引必须掉，否则随后的 reopen 会继续读被替换掉的旧文件（表现为
  // `Corrupted`）。对齐 C++ reopen 前重新打开分区文件。
  state.stores.backend().evict_table(tbl);
  match result {
    Ok(_) => Ok(()),
    Err(KvError::NotFound | KvError::ResourceMissing) => {
      warn!(
        target: "fkv_shard",
        "standby partition absent on master, drop local manifest tbl={tbl}"
      );
      state.stores.backend().drop_manifest(tbl).await?;
      Err(KvError::ResourceMissing)
    }
    Err(err) => Err(err),
  }
}

/// 处理一条非只读请求：执行、OOM 重试、写后维护与回执。
/// 返回 `true` 表示收到 `Shutdown`，事件循环应退出。
async fn process_envelope(
  state: &mut ShardState,
  metrics: &ShardMetric,
  reopen_li: &mut ReopenQueue,
  envelope: Envelope,
  oom_retry_times: u32,
) -> bool {
  let shutdown = envelope.req.is_shutdown();
  let op = op_of(&envelope.req);
  let started = Instant::now();
  // 调用方在排队期间放弃等待：请求尚未开始，直接回执退出，避免白跑一次写。
  // `Shutdown` 例外（`shutdown()` 必须拿到回执并真正停机）。
  if !shutdown && envelope.canceled() {
    envelope.send(Err(KvError::NotRunning));
    return false;
  }
  // 排队期间已过 deadline：未开始就快速失败，不进入 admit / prepare_table。
  if !shutdown && envelope.expired() {
    warn!(
      target: "fkv_shard",
      "request deadline expired shard_id={} op={}",
      metrics.shard_id(),
      op.map_or("unknown", Op::as_str)
    );
    record_request(metrics, op, started, &Err(KvError::Timeout));
    envelope.send(Err(KvError::Timeout));
    return false;
  }
  // 借用而非搬走请求：遇 `ResourceMissing` 要挂起整条 envelope（含原请求与回执）。
  let req = &envelope.req;
  // shutdown 后挂起的 reopen 立即失败（对齐 C++ Stop 路径先清
  // `pending_reopens_` 再停 shard），避免用已驱逐的状态继续重试。
  // `DetachShard` 的合法性由 `dispatch` 的 `admit_detach` 判定：只有真正
  // detach 成功才清挂起条目，非法 / 幂等 detach 不能误清。
  if shutdown {
    reopen_li.fail_all(KvError::NotRunning);
  }
  // OutOfMem 重试：请求已在 put_at 内回滚，重试从干净 mapping 开始。
  // 由事件循环持有原请求、`handle` 只借用，重试零深拷贝（对齐 C++
  // 把原请求对象挂在任务上重跑）。
  let mut remaining = oom_retry_times;
  let result = loop {
    match handle(state, req).await {
      Ok(response) => break Ok(response),
      Err(KvError::OutOfMem) if remaining > 0 => remaining -= 1,
      Err(err) => break Err(err),
    }
  };
  // 写后维护登记：`needs_maintain` 是 O(1) 放大率预判，只有需要
  // compact/GC 或有待清理 TTL 时才在本请求回复后跑维护（对齐 C++
  // `WriteTask::UpdateMeta` 提交后抛给后台的 Compact / FileGc / CleanExpired）。
  let mut maintain_tbl = None;
  if result.is_ok()
    && matches!(req, Request::Put { .. } | Request::PutLarge { .. })
    && let Some(tbl) = req.table()
    && (state.stores.needs_maintain(tbl) || state.stores.pending_ttl_len() > 0)
  {
    maintain_tbl = Some(tbl.clone());
  }
  // detach 成功：旧身份已驱逐，挂起的 reopen 立即失败（对齐 C++ Stop 路径先清
  // `pending_reopens_`）。非法 shard_id / 已 Detached 的幂等路径不走到这里。
  if matches!(result, Ok(Response::DetachShard)) {
    reopen_li.fail_all(KvError::NotRunning);
  }
  // 回执发出前先同步缓存计数：调用方拿到响应后立刻查指标即可见。
  sync_cache_metrics(state, metrics);
  // 等 manifest 可见：Cloud/Standby 下的读请求挂起重排；`Reopen` / 空 tag 的
  // `ReopenArchive` 只有在同表已有 driver 时才搭 waiter，否则 `park` 立即把
  // `ResourceMissing` 交回调用方（对齐 C++ 外部 `ReopenRequest` 一次即回）。
  //
  // `ReopenArchive` 只有空 tag（= 重开活跃快照）走同一语义；非空 tag 指向确定的
  // 归档快照，缺失即「调用方要的状态不存在」，直接回 `ResourceMissing`，不能拿
  // 活跃 manifest 的可见性当作它的前置条件。
  let park = matches!(result, Err(KvError::ResourceMissing))
    && (matches!(req, Request::Reopen { .. })
      || matches!(req, Request::ReopenArchive { tag, .. } if tag.is_empty())
      || (state.reopen_on_missing && is_auto_reopen_read(req)));
  let envelope = if park {
    reopen_li.park(envelope)
  } else {
    Some(envelope)
  };
  // 只有真正回执的请求才计数：被 park 的请求稍后唤醒会重跑，这里先记会双计。
  if envelope.is_some() {
    record_request(metrics, op, started, &result);
  }
  if let Some(envelope) = envelope {
    envelope.send(result);
  }
  if shutdown {
    return true;
  }
  // 回复已发出，compaction/GC/TTL 清理不占写延迟窗口。
  if let Some(tbl) = maintain_tbl {
    maintain_after_write(state, &tbl).await;
  }
  false
}

/// 写队列被 `Drop` / `DetachShard` / `Shutdown` 清除时，排队请求统一回
/// `NotRunning`（对齐 C++ 清理 `pending_queues_`）。
fn fail_pending(pending: impl IntoIterator<Item = Envelope>) {
  for envelope in pending {
    envelope.send(Err(KvError::NotRunning));
  }
}

/// 控制请求完成后，排队写是否已因身份切换而失效。
///
/// 旧身份下排队的 `FileGc` / `CleanExpired` 若在新 attach（Fresh）后执行，
/// 会作用到新 branch / term 的盘面；detach 成功后同理。
/// 被取消 / 超时提前回 `NotRunning` 的 detach 没有驱逐任何分区状态，
/// 不清队以免误伤其它分区已排队的写；旧 term attach 导致的
/// `Attached -> Expired` 也不清队（排队写会在 drain 时按 `ExpiredTerm` 拒绝）。
fn write_queue_stale(
  was_attach: bool,
  was_detach: bool,
  before: DsShardState,
  after: DsShardState,
) -> bool {
  (was_detach && after != before)
    || (was_attach && before != DsShardState::Attached && after == DsShardState::Attached)
}

/// 执行一条已从写队列取出的非 Put 写请求；`Drop` 结束后丢弃该分区槽位与排队请求。
async fn run_gated_write(
  state: &mut ShardState,
  metrics: &ShardMetric,
  reopen_li: &mut ReopenQueue,
  gate: &mut WriteGate<(Envelope, Pri)>,
  tbl: TableIdent,
  envelope: Envelope,
  oom_retry_times: u32,
) -> bool {
  let drop_table = matches!(envelope.req, Request::Drop { .. });
  let stop = process_envelope(state, metrics, reopen_li, envelope, oom_retry_times).await;
  gate.finish(&tbl);
  // 只有真正删空（表已不在 Store 里）才丢槽位：失败 / 被取消的 Drop 不影响分区。
  if drop_table && !state.stores.is_open(&tbl) {
    state.stores.evict_table(&tbl);
    fail_pending(
      gate
        .remove_pending(&tbl)
        .into_iter()
        .map(|(envelope, _)| envelope),
    );
  }
  stop
}

/// 排空写队列里所有可启动条目：同分区按 FIFO 续跑，跨分区按 ready 顺序推进。
///
/// 高优先 `Put` 组取到就派发成独立 compio task：任务独占该分区 Store，跨分区
/// 写因此真并发；同分区仍由 `WriteGate::running` 串行。其余写（Gc / Compact /
/// Truncate / Drop / 归档 / 分支 / Prewarm）在原地执行，语义与之前一致。
/// 低优先条目一次只跑一条，回到主循环让已就绪的高优先请求插队（eng-34）；
/// 剩余的写条目由循环顶部的 `gate.has_ready()` 继续驱动，不会滞留。
/// 返回 `true` 表示遇到 `Shutdown`。
async fn drain_write_gate(
  state: &mut ShardState,
  metrics: &Arc<ShardMetric>,
  reopen_li: &mut ReopenQueue,
  gate: &mut WriteGate<(Envelope, Pri)>,
  read_flight: &mut ReadFlight,
  write_flight: &mut WriteFlight,
  oom_retry_times: u32,
) -> bool {
  while let Some((tbl, (envelope, pri))) = gate.take_next() {
    if pri == Pri::High
      && let Some(term) = put_term(&envelope.req)
    {
      // envelope_li commit：同一分区、同一 term 的连续 Put 合并成一次 Store::put，
      // 共享一次 manifest append + fsync。遇到非 Put 立即停止，保持 FIFO。
      let extra = gate.take_pending_while(&tbl, MAX_PUT_GROUP_REQUESTS - 1, |(next, next_pri)| {
        *next_pri == Pri::High && put_term(&next.req) == Some(term)
      });
      let mut envelope_li = Vec::with_capacity(extra.len() + 1);
      envelope_li.push(envelope);
      envelope_li.extend(extra.into_iter().map(|(envelope, _)| envelope));
      // 写任务要独占 Store（`Rc` 唯一），先 drain 在飞读。
      finish_read_li(state, metrics, reopen_li, read_flight, oom_retry_times).await;
      match prepare_put_group(state, metrics, envelope_li).await {
        Some((live_li, merged)) => match state.stores.take(&tbl) {
          Ok(store) => write_flight.push(spawn_put_group(
            store,
            tbl,
            live_li,
            merged,
            metrics.clone(),
            oom_retry_times,
          )),
          Err(err) => {
            for envelope in live_li {
              record_request(metrics, Some(Op::Put), Instant::now(), &Err(err));
              envelope.send(Err(err));
            }
            gate.finish(&tbl);
          }
        },
        // 已回执（取消 / 超时 / 准入失败）或没有可执行请求。
        None => gate.finish(&tbl),
      }
      continue;
    }
    if run_gated_write(
      state,
      metrics,
      reopen_li,
      gate,
      tbl,
      envelope,
      oom_retry_times,
    )
    .await
    {
      return true;
    }
    if pri == Pri::Low {
      break;
    }
  }
  false
}

fn put_term(req: &Request) -> Option<Option<u64>> {
  match req {
    Request::Put { term, .. } => Some(*term),
    _ => None,
  }
}

/// 把一个分区的连续 Put 合并成一次 Store::put。
///
/// 语义：
/// - 每个请求内部同 key 仍按 timestamp 折叠：timestamp 大者胜、相等时后写胜
///   （与 `prepare_write_batch` / `Store::put` 一致，不是无条件后写覆盖）；
/// - 跨请求同 key 时，timestamp 大的胜；timestamp 相等则后请求胜
///   （与 Store 对旧页的 `entry.timestamp < old.timestamp` 过滤一致）；
/// - 请求 term 必须相同（envelope_li 构造时保证），合并后统一校验。
///
/// 实现同 perf-79：先展开成引用、按 `(key, 请求序, 批内序)` 排序后单趟折叠，
/// 只为幸存者各 clone 一次；请求内被覆盖 / 跨请求 timestamp 落败的 entry
/// 全程不分配（旧实现为每 entry 双 clone + 两层 BTreeMap）。
fn merge_put_batch_li(envelope_li: &[Envelope]) -> Vec<BatchEntry> {
  let mut item_li: Vec<(usize, usize, &BatchEntry)> = Vec::with_capacity(
    envelope_li
      .iter()
      .map(|envelope| match &envelope.req {
        Request::Put { batch_li, .. } => batch_li.len(),
        _ => 0,
      })
      .sum(),
  );
  for (req_idx, envelope) in envelope_li.iter().enumerate() {
    let Request::Put { batch_li, .. } = &envelope.req else {
      continue;
    };
    item_li.extend(
      batch_li
        .iter()
        .enumerate()
        .map(|(entry_idx, entry)| (req_idx, entry_idx, entry)),
    );
  }
  // 引用排序零 clone：key 升序让输出与旧 BTreeMap 一致；同 key 段内请求序 /
  // 批内序升序，保证「后写」稳定排在后面。
  item_li.sort_unstable_by(|left, right| {
    left
      .2
      .key
      .cmp(&right.2.key)
      .then_with(|| left.0.cmp(&right.0))
      .then_with(|| left.1.cmp(&right.1))
  });

  let mut out = Vec::with_capacity(item_li.len());
  let mut item_iter = item_li.into_iter().peekable();
  while let Some((req_idx, _, entry)) = item_iter.next() {
    let key = entry.key.as_slice();
    // 当前请求的代表条：同请求同 key 的后写无条件覆盖先写。
    let mut cand = entry;
    let mut cand_req = req_idx;
    // 已结算请求的跨请求胜者；None 表示本 key 还没有结算过的请求。
    let mut win: Option<&BatchEntry> = None;
    while let Some(&(next_req, _, next_entry)) = item_iter.peek() {
      if next_entry.key.as_slice() != key {
        break;
      }
      item_iter.next();
      if next_req == cand_req {
        // 同请求同 key 仍按 Store::put / write_batch 的 timestamp 折叠：
        // 只有不低于当前候选的 ts 才覆盖；相等后写胜。
        if next_entry.timestamp >= cand.timestamp {
          cand = next_entry;
        }
      } else {
        // 请求切换：上一请求代表先结算，再以新请求的第一条开新段。
        win = Some(pick_newer(win, cand));
        cand = next_entry;
        cand_req = next_req;
      }
    }
    out.push(pick_newer(win, cand).clone());
  }
  out
}

/// 跨请求合并：timestamp 大者胜；相等时后出现的 `cand` 胜。
fn pick_newer<'a>(win: Option<&'a BatchEntry>, cand: &'a BatchEntry) -> &'a BatchEntry {
  match win {
    Some(old) if old.timestamp > cand.timestamp => old,
    _ => cand,
  }
}

/// 准入一条 Put 组：过滤取消 / 超时，串行打开目标表并校验 term。
///
/// 返回 `Some((live_li, merged))` 表示可以派发写任务；`None` 表示请求已回执
/// （取消 / 超时 / 准入失败）或没有可执行请求，调用方只需 `gate.finish`。
/// 这里不触碰 Store，只做 `&mut ShardState` 的串行准备。
async fn prepare_put_group(
  state: &mut ShardState,
  metrics: &ShardMetric,
  envelope_li: Vec<Envelope>,
) -> Option<(Vec<Envelope>, Vec<BatchEntry>)> {
  let started = Instant::now();
  let mut live_li = Vec::with_capacity(envelope_li.len());
  for envelope in envelope_li {
    if envelope.canceled() {
      envelope.send(Err(KvError::NotRunning));
    } else if envelope.expired() {
      // 排队 / 等待 envelope_li commit 期间过 deadline：不参与合并，直接回 Timeout。
      warn!(
        target: "fkv_shard",
        "request deadline expired shard_id={} op=put",
        metrics.shard_id()
      );
      record_request(metrics, Some(Op::Put), started, &Err(KvError::Timeout));
      envelope.send(Err(KvError::Timeout));
    } else {
      live_li.push(envelope);
    }
  }
  if live_li.is_empty() {
    return None;
  }

  let term = put_term(&live_li[0].req).unwrap_or(None);
  let mut result = admit(state, &live_li[0].req);
  if result.is_ok() {
    result = prepare_table(state, &live_li[0].req).await;
  }
  if result.is_ok()
    && let Some(term) = term
    && state.term != Some(term)
  {
    // 与单条 Put 路径对齐：stale term 之后整个 shard 进入 Expired 快速失败。
    state.set_lifecycle(DsShardState::Expired);
    result = Err(KvError::ExpiredTerm);
  }
  if let Err(err) = result {
    for envelope in live_li {
      record_request(metrics, Some(Op::Put), started, &Err(err));
      envelope.send(Err(err));
    }
    return None;
  }
  let merged = merge_put_batch_li(&live_li);
  Some((live_li, merged))
}

/// 一条已派发的 Put 任务：独占目标分区 Store，完成时把 Store 与缓存计数交回。
struct WriteTask {
  tbl: TableIdent,
  handle: runtime::JoinHandle<WriteDone>,
}

/// 写任务完成结果。
struct WriteDone {
  tbl: TableIdent,
  /// `None` = 任务 panic / 取消，Store 已随任务丢失；槽位按需 reopen。
  store: Option<ShardStore>,
}

impl Future for WriteTask {
  type Output = WriteDone;

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    let this = self.get_mut();
    match Pin::new(&mut this.handle).poll(cx) {
      Poll::Ready(Ok(done)) => Poll::Ready(done),
      Poll::Ready(Err(err)) => {
        error!(
          target: "fkv_shard",
          "write task failed tbl={} error={}",
          this.tbl,
          err
        );
        Poll::Ready(WriteDone {
          tbl: this.tbl.clone(),
          store: None,
        })
      }
      Poll::Pending => Poll::Pending,
    }
  }
}

/// 在飞写任务集合；任务由 compio 独立推进，主循环只负责完成回调。
struct WriteFlight {
  task_set: FuturesUnordered<WriteTask>,
}

impl WriteFlight {
  fn new() -> Self {
    Self {
      task_set: FuturesUnordered::new(),
    }
  }

  fn is_empty(&self) -> bool {
    self.task_set.is_empty()
  }

  fn push(&mut self, task: WriteTask) {
    self.task_set.push(task);
  }

  /// 非阻塞回收已完成任务。
  fn try_next(&mut self) -> Option<WriteDone> {
    use futures::FutureExt;
    self.task_set.next().now_or_never().flatten()
  }

  async fn next(&mut self) -> Option<WriteDone> {
    self.task_set.next().await
  }

  /// 等在飞写全部完成并交回 Store / 释放写门额度。
  async fn drain(
    &mut self,
    state: &mut ShardState,
    gate: &mut WriteGate<(Envelope, Pri)>,
    metrics: &ShardMetric,
  ) {
    while let Some(done) = self.task_set.next().await {
      finish_write(state, gate, metrics, done);
    }
  }
}

/// 把一条已准入的 Put 组派发为独立 compio task。
///
/// 任务持有分区 Store 的独占所有权：同分区后续写由 `WriteGate::running`
/// 挡住，不同分区各自的 Store 互不共享 `&mut`，IO 可以重叠。回执、指标、
/// 写后维护都在任务内完成；Store 通过 [`WriteDone`] 交回主循环。
fn spawn_put_group(
  store: ShardStore,
  tbl: TableIdent,
  envelope_li: Vec<Envelope>,
  merged: Vec<BatchEntry>,
  metrics: Arc<ShardMetric>,
  oom_retry_times: u32,
) -> WriteTask {
  let task_tbl = tbl.clone();
  WriteTask {
    tbl,
    handle: spawn(async move {
      let mut store = store;
      let started = Instant::now();
      let mut result = Ok(());
      let mut remaining = oom_retry_times;
      loop {
        match store.put(&task_tbl, &merged).await {
          Ok(()) => break,
          Err(KvError::OutOfMem) if remaining > 0 => remaining -= 1,
          Err(err) => {
            result = Err(err);
            break;
          }
        }
      }
      for envelope in envelope_li {
        let response = result.map(|()| Response::Put);
        record_request(&metrics, Some(Op::Put), started, &response);
        envelope.send(response);
      }
      if result.is_ok() {
        let needs_maintain =
          store.needs_maintain(&task_tbl) || store.needs_segment_maintain(&task_tbl);
        let pending_ttl = store.pending_ttl_len() > 0;
        maintain_store(&mut store, &task_tbl, needs_maintain, pending_ttl).await;
      }
      WriteDone {
        tbl: task_tbl,
        store: Some(store),
      }
    }),
  }
}

/// 写任务完成：Store 放回槽位（panic 则丢槽位）、释放写门额度、同步指标。
fn finish_write(
  state: &mut ShardState,
  gate: &mut WriteGate<(Envelope, Pri)>,
  metrics: &ShardMetric,
  done: WriteDone,
) {
  match done.store {
    Some(store) => state.stores.put_back(&done.tbl, store),
    None => {
      state.stores.lose(&done.tbl);
      state.table_open.forget(&done.tbl);
    }
  }
  gate.finish(&done.tbl);
  sync_cache_metrics(state, metrics);
}

/// 只读分发：只借 `&Store`，读任务与串行路径共用。
async fn dispatch_read(store: &ShardStore, req: &Request) -> Result<Response> {
  match req {
    Request::Get { tbl, key } => Ok(Response::Get(store.get(tbl, key).await?)),
    Request::Scan {
      tbl,
      begin,
      end,
      max_entries,
      option,
    } => {
      let page = store
        .scan_page(
          tbl,
          begin,
          end,
          ScanOption {
            begin_inclusive: option.begin_inclusive,
            end_inclusive: option.end_inclusive,
            ..ScanOption::default()
          },
          // 统一为一个 entry 上限：显式分页优先，其次 Runtime::scan 的 max_entries。
          PageOption {
            page_entries: if option.page_entries != 0 {
              option.page_entries
            } else {
              *max_entries
            },
            page_size: option.page_size,
          },
        )
        .await?;
      Ok(Response::Scan(ScanReply {
        entry_li: page.entry_li,
        has_remaining: page.has_remaining,
      }))
    }
    Request::Floor { tbl, key } => {
      let floor = store.floor(tbl, key).await?;
      Ok(Response::Floor(FloorReply {
        key: floor.key,
        value: floor.value,
        metadata: floor.metadata,
        timestamp: floor.timestamp,
        expire_ts: floor.expire_ts,
      }))
    }
    Request::ListArchiveTags { tbl } => {
      let entry_li = store.list_archive_tags(tbl).await?;
      Ok(Response::ListArchiveTags(
        entry_li
          .into_iter()
          .map(|entry| ArchiveTagInfo {
            tag: entry.tag,
            term: entry.term,
          })
          .collect(),
      ))
    }
    Request::ListObject {
      prefix,
      token,
      max_keys,
      ..
    } => {
      // `tbl` 只用于路由；`prefix` 是远端 key 的原始前缀（C++ RemotePath）。
      // Local / Mem 后端在 `StoreBackend::list_object` 内显式返回 InvalidArgs。
      let page = store
        .page_io()
        .inner()
        .list_object(prefix, token.as_deref(), *max_keys)
        .await?;
      Ok(Response::ListObject(ListPage {
        meta_li: page
          .meta_li
          .into_iter()
          .map(|meta| ObjectMeta {
            key: meta.key,
            etag: meta.etag,
            size: meta.size,
          })
          .collect(),
        next_token: page.next_token,
      }))
    }
    _ => Err(KvError::InvalidArgs),
  }
}

/// 记录单请求指标并打印状态行；读批与串行路径共用。
fn record_request(
  metrics: &ShardMetric,
  op: Option<Op>,
  started: Instant,
  result: &Result<Response>,
) {
  let Some(op) = op else {
    return;
  };
  let elapsed = started.elapsed();
  metrics.observe(op, elapsed, result.is_err());
  let latency_us = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
  match result {
    Ok(_) => debug!(
      target: "fkv_shard",
      "request shard_id={} op={} status=ok latency_us={}",
      metrics.shard_id(),
      op.as_str(),
      latency_us
    ),
    Err(err) => warn!(
      target: "fkv_shard",
      "request shard_id={} op={} status=error error={} latency_us={}",
      metrics.shard_id(),
      op.as_str(),
      err,
      latency_us
    ),
  }
}

/// 一条已提交的读请求：请求体移入任务，完成时随结果带回，避免深拷贝。
struct ReadSlot {
  reply: oneshot::Sender<Result<Response>>,
  cancel: Arc<AtomicBool>,
  deadline: Option<Instant>,
  /// 该读请求的 auto-reopen 预算（挂起 / 唤醒时随请求带回）。
  reopen_retry: Option<u8>,
  started: Instant,
  /// 提交时的 attach 代际；完成时代际不符按 `NotRunning` 丢弃结果。
  epoch: u64,
  /// 任务完成后送回原请求；在飞期间为 `None`。
  req: Option<Request>,
  result: Option<Result<Response>>,
}

/// 单个读任务：`JoinHandle` 本身不带 slot 编号，包一层补上，
/// 使任务 panic / 取消时也能对号回执。
struct ReadTask {
  slot: usize,
  handle: runtime::JoinHandle<(Request, Result<Response>)>,
}

impl ReadTask {
  /// 把已通过准入与打开检查的读请求提交为独立 compio task。
  ///
  /// 任务只持 `Rc<ShardStore>`（共享借用即可读）与请求体，不再触达
  /// `&mut ShardState`；写路径要独占 Store 时必须等它结束（见 `finish_read_li`）。
  fn new(slot: usize, store: Rc<ShardStore>, req: Request) -> Self {
    Self {
      slot,
      handle: spawn(async move {
        let result = dispatch_read(&store, &req).await;
        (req, result)
      }),
    }
  }
}

impl Future for ReadTask {
  type Output = (usize, Option<Request>, Result<Response>);

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    let this = self.get_mut();
    match Pin::new(&mut this.handle).poll(cx) {
      Poll::Ready(Ok((req, result))) => Poll::Ready((this.slot, Some(req), result)),
      Poll::Ready(Err(err)) => {
        error!(
          target: "fkv_shard",
          "read task failed slot={} error={}",
          this.slot,
          err
        );
        // 请求体随任务丢失，无法重试；按内部错误回执。
        Poll::Ready((this.slot, None, Err(KvError::IoFail)))
      }
      Poll::Pending => Poll::Pending,
    }
  }
}

/// 在飞读任务集合：每条读请求一个独立 compio task，互不串行等待。
///
/// 读任务只持 `Rc<ShardStore>`。任何需要 `&mut Store` 的路径（写、控制、
/// 维护）必须先 [`finish_read_li`]：既保证 `Rc::get_mut` 成功，也保证
/// 「先到的读先完成、后到的写才开始」与 attach/detach 线性化。
struct ReadFlight {
  task_set: FuturesUnordered<ReadTask>,
  slot_li: Vec<ReadSlot>,
  /// 在飞读 gauge 出口：提交 / 回收后写当前任务数（eng-32c）。
  metrics: Arc<ShardMetric>,
}

impl ReadFlight {
  fn new(metrics: Arc<ShardMetric>) -> Self {
    Self {
      task_set: FuturesUnordered::new(),
      slot_li: Vec::new(),
      metrics,
    }
  }

  fn is_empty(&self) -> bool {
    self.slot_li.is_empty()
  }

  /// 把当前在飞读任务数推入 gauge（峰值由 `ShardMetric` 内部维护）。
  fn publish(&self) {
    self
      .metrics
      .set_read_in_flight(u64::try_from(self.task_set.len()).unwrap_or(u64::MAX));
  }

  fn is_full(&self) -> bool {
    self.slot_li.len() >= MAX_READ_BATCH
  }

  /// 提交一条已准入的读请求。
  fn push(&mut self, store: Rc<ShardStore>, epoch: u64, envelope: Envelope, started: Instant) {
    let Envelope {
      req,
      reply,
      cancel,
      deadline,
      reopen_retry,
    } = envelope;
    let slot = self.slot_li.len();
    self.slot_li.push(ReadSlot {
      reply,
      cancel,
      deadline,
      reopen_retry,
      started,
      epoch,
      req: None,
      result: None,
    });
    self.task_set.push(ReadTask::new(slot, store, req));
    self.publish();
  }

  /// 登记一条已完成任务的结果。
  fn record(&mut self, slot: usize, req: Option<Request>, result: Result<Response>) {
    let Some(entry) = self.slot_li.get_mut(slot) else {
      return;
    };
    if req.is_some() {
      entry.req = req;
    }
    entry.result = Some(result);
    self.publish();
  }

  /// 等在飞读全部完成并登记结果；不触达 `ShardState`。
  async fn drain(&mut self) {
    while let Some((slot, req, result)) = self.task_set.next().await {
      self.record(slot, req, result);
    }
    self.publish();
  }
}

/// 读窗口的一次事件。
enum ReadEvent {
  /// 在飞读全部完成并回执。
  Drained,
  /// 遇到需要独占 `&mut Store` 的请求（或读表未打开 / 读配额已满）：
  /// 已在飞读已 drain，请求交回主循环按原顺序处理。
  Exclusive(Envelope),
}

/// 提交一批只读请求：串行完成准入 + 打开（唯一需要 `&mut Store` 的部分），
/// 之后每条请求 spawn 成独立 compio task，彼此并发、与主循环解耦。
///
/// 两阶段：所有打开完成后再统一 spawn。否则前一条读任务的 `Rc` 克隆会让
/// 后一条的 `ensure_table_ready` 拿不到 `&mut Store`。
async fn submit_read_li(
  state: &mut ShardState,
  metrics: &ShardMetric,
  reopen_li: &mut ReopenQueue,
  flight: &mut ReadFlight,
  batch_li: Vec<Envelope>,
) {
  // 读批大小分布：本次高优先通道合并提交的读请求条数（eng-32c）。
  metrics.record_read_batch(u64::try_from(batch_li.len()).unwrap_or(u64::MAX));
  let mut ready_li = Vec::with_capacity(batch_li.len());
  for envelope in batch_li {
    let started = Instant::now();
    let op = op_of(&envelope.req);
    // 排队期间调用方已放弃：不执行，按 `NotRunning` 回执（oneshot 完成契约）。
    if envelope.canceled() {
      envelope.send(Err(KvError::NotRunning));
      continue;
    }
    // 排队期间已过 deadline：跳过，不进入 admit / prepare_table。
    if envelope.expired() {
      warn!(
        target: "fkv_shard",
        "request deadline expired shard_id={} op={}",
        metrics.shard_id(),
        op.map_or("unknown", Op::as_str)
      );
      let err = Err(KvError::Timeout);
      record_request(metrics, op, started, &err);
      envelope.send(err);
      continue;
    }
    if let Err(err) = admit(state, &envelope.req) {
      record_request(metrics, op, started, &Err(err));
      envelope.send(Err(err));
      continue;
    }
    let Some(tbl) = envelope.req.table().cloned() else {
      let err = Err(KvError::InvalidArgs);
      record_request(metrics, op, started, &err);
      envelope.send(err);
      continue;
    };
    // 归档列举 / 远端列举不依赖表打开（不懒建空表、不等待 manifest 可见），
    // 但仍需要一个分区槽位承载 branch / manifest 身份。
    if skips_table_open(&envelope.req)
      && !state.stores.has_slot(&tbl)
      && let Err(err) = state.ensure_slot(&tbl)
    {
      record_request(metrics, op, started, &Err(err));
      envelope.send(Err(err));
      continue;
    }
    // 串行打开：每表只在这里 replay 一次 manifest（幂等），读任务本身不再打开。
    if !skips_table_open(&envelope.req)
      && !state
        .table_open
        .ready(&state.stores, &tbl, state.term, &state.branch)
      && let Err(err) = ensure_table_ready(state, &tbl).await
    {
      // Cloud / StandbyReplica：manifest 暂不可见 → 挂等待重排（对齐 C++
      // `AutoReopenRetry`）；其余错误直接回执。
      let park = state.reopen_on_missing
        && matches!(err, KvError::ResourceMissing)
        && is_auto_reopen_read(&envelope.req);
      let envelope = if park {
        reopen_li.park(envelope)
      } else {
        Some(envelope)
      };
      if envelope.is_some() {
        record_request(metrics, op, started, &Err(err));
      }
      if let Some(envelope) = envelope {
        envelope.send(Err(err));
      }
      continue;
    }
    ready_li.push((tbl, envelope, started));
  }
  // 全部打开完成后统一提交：此后不再需要 `&mut Store`。
  let epoch = state.table_open.epoch;
  for (tbl, envelope, started) in ready_li {
    match state.stores.store_rc(&tbl) {
      Ok(store) => flight.push(store, epoch, envelope, started),
      Err(err) => {
        record_request(metrics, op_of(&envelope.req), started, &Err(err));
        envelope.send(Err(err));
      }
    }
  }
}

/// 等在飞读全部完成并逐条回执。需要 `&mut Store` 的路径必须先调用本函数：
/// 读任务释放 `Rc` 克隆后 `Rc::get_mut` 才能成功。
async fn finish_read_li(
  state: &mut ShardState,
  metrics: &ShardMetric,
  reopen_li: &mut ReopenQueue,
  flight: &mut ReadFlight,
  oom_retry_times: u32,
) {
  if flight.is_empty() {
    return;
  }
  flight.drain().await;
  let slot_li = take(&mut flight.slot_li);
  // drain 已把每条结果 publish 到 gauge；这里显式归零，保证空窗口读数为 0。
  flight.publish();
  sync_cache_metrics(state, metrics);
  let epoch = state.table_open.epoch;
  for slot in slot_li {
    let ReadSlot {
      reply,
      cancel,
      deadline,
      reopen_retry,
      started,
      epoch: slot_epoch,
      req,
      result,
    } = slot;
    // 任务 panic / 取消：请求体已随任务丢失，只能按内部错误回执。
    let Some(req) = req else {
      let _ = reply.send(Err(KvError::IoFail));
      continue;
    };
    let mut result = result.unwrap_or(Err(KvError::IoFail));
    // 跨 attach 代际的读结果作废（drain 屏障下正常不会发生）。
    if slot_epoch != epoch {
      result = Err(KvError::NotRunning);
    }
    let op = op_of(&req);
    let mut remaining = oom_retry_times;
    while matches!(result, Err(KvError::OutOfMem)) && remaining > 0 {
      remaining -= 1;
      result = handle(state, &req).await;
    }
    let envelope = Envelope {
      req,
      reply,
      cancel,
      deadline,
      reopen_retry,
    };
    // Cloud / StandbyReplica：manifest 暂不可见 → 挂等待 manifest 可见后重排。
    let envelope = if state.reopen_on_missing && matches!(result, Err(KvError::ResourceMissing)) {
      reopen_li.park(envelope)
    } else {
      Some(envelope)
    };
    // 被 park 的读请求唤醒后会重跑，先记会双计；只记真正回执的这一条。
    if envelope.is_some() {
      record_request(metrics, op, started, &result);
    }
    if let Some(envelope) = envelope {
      envelope.send(result);
    }
  }
}

/// 读窗口内处理一条新到的高优先请求。
///
/// 返回 `Some(event)` 表示窗口结束，`None` 表示已提交新读、继续窗口。
async fn window_admit(
  state: &mut ShardState,
  metrics: &ShardMetric,
  reopen_li: &mut ReopenQueue,
  flight: &mut ReadFlight,
  envelope: Envelope,
  oom_retry_times: u32,
) -> Option<ReadEvent> {
  let read = is_read_op(&envelope.req);
  let ready = read
    && (skips_table_open(&envelope.req)
      || envelope.req.table().is_some_and(|tbl| {
        state
          .table_open
          .ready(&state.stores, tbl, state.term, &state.branch)
      }));
  // 非读请求、读表未打开、读配额已满：先 drain 在飞读，再交回主循环。
  // 未打开的表由主循环的串行准入路径补开（唯一需要 `&mut Store` 的动作）。
  if !ready || flight.is_full() {
    finish_read_li(state, metrics, reopen_li, flight, oom_retry_times).await;
    return Some(ReadEvent::Exclusive(envelope));
  }
  if envelope.canceled() {
    envelope.send(Err(KvError::NotRunning));
    return None;
  }
  let started = Instant::now();
  let op = op_of(&envelope.req);
  if envelope.expired() {
    warn!(
      target: "fkv_shard",
      "request deadline expired shard_id={} op={}",
      metrics.shard_id(),
      op.map_or("unknown", Op::as_str)
    );
    let err = Err(KvError::Timeout);
    record_request(metrics, op, started, &err);
    envelope.send(err);
    return None;
  }
  if let Err(err) = admit(state, &envelope.req) {
    record_request(metrics, op, started, &Err(err));
    envelope.send(Err(err));
    return None;
  }
  let epoch = state.table_open.epoch;
  let tbl = envelope.req.table().cloned();
  match tbl.as_ref().map(|tbl| state.stores.store_rc(tbl)) {
    Some(Ok(store)) => flight.push(store, epoch, envelope, started),
    _ => {
      // Store 不可借（刚被写任务取走 / 槽位消失）：交回主循环按 FIFO 重排。
      finish_read_li(state, metrics, reopen_li, flight, oom_retry_times).await;
      return Some(ReadEvent::Exclusive(envelope));
    }
  }
  None
}

/// 读并发窗口：在飞读由 compio 独立推进，主循环同时接收新请求。
///
/// 任何非读请求到达即 drain 在飞读并把它交回主循环，保证「读不越过后到的写」。
async fn read_window(
  state: &mut ShardState,
  metrics: &ShardMetric,
  reopen_li: &mut ReopenQueue,
  flight: &mut ReadFlight,
  rx_hi: &MAsyncRx<Array<Envelope>>,
  oom_retry_times: u32,
) -> ReadEvent {
  loop {
    // 先非阻塞取高优先请求；没有就等「读完成」或「新请求」。
    if let Ok(envelope) = rx_hi.try_recv()
      && let Some(event) =
        window_admit(state, metrics, reopen_li, flight, envelope, oom_retry_times).await
    {
      return event;
    }
    match select(Box::pin(flight.task_set.next()), Box::pin(rx_hi.recv())).await {
      Either::Left((Some((slot, req, result)), _)) => {
        flight.record(slot, req, result);
        if flight.task_set.is_empty() {
          finish_read_li(state, metrics, reopen_li, flight, oom_retry_times).await;
          return ReadEvent::Drained;
        }
      }
      Either::Left((None, _)) => {
        finish_read_li(state, metrics, reopen_li, flight, oom_retry_times).await;
        return ReadEvent::Drained;
      }
      Either::Right((Ok(envelope), _)) => {
        if let Some(event) =
          window_admit(state, metrics, reopen_li, flight, envelope, oom_retry_times).await
        {
          return event;
        }
      }
      Either::Right((Err(_), _)) => {
        // 高优先通道关闭：先让在飞读完，交回主循环处理低优先 / 关停。
        finish_read_li(state, metrics, reopen_li, flight, oom_retry_times).await;
        return ReadEvent::Drained;
      }
    }
  }
}

async fn dispatch(state: &mut ShardState, req: &Request) -> Result<Response> {
  if is_read_op(req) {
    let tbl = req.table().ok_or(KvError::InvalidArgs)?;
    // 归档 / 远端列举不依赖表打开，但需要一个分区 Store 承载 branch 身份。
    if skips_table_open(req) && !state.stores.has_slot(tbl) {
      state.ensure_slot(tbl)?;
    }
    return dispatch_read(state.stores.store(tbl)?, req).await;
  }
  match req {
    Request::Put {
      tbl,
      batch_li,
      term,
    } => {
      if let Some(term) = *term
        && state.term != Some(term)
      {
        state.set_lifecycle(DsShardState::Expired);
        return Err(KvError::ExpiredTerm);
      }
      state.stores.store_mut(tbl)?.put(tbl, batch_li).await?;
      Ok(Response::Put)
    }
    Request::PutLarge {
      tbl,
      batch_li,
      term,
    } => {
      if let Some(term) = *term
        && state.term != Some(term)
      {
        state.set_lifecycle(DsShardState::Expired);
        return Err(KvError::ExpiredTerm);
      }
      let max_len = batch_li
        .iter()
        .filter(|entry| entry.op == WriteOp::Upsert)
        .map(|entry| entry.value.len())
        .max()
        .unwrap_or(0);
      let fixed_buffer = state.fixed_buffer.as_mut();
      let store = state.stores.store_mut(tbl)?;
      if let Some(buf) = fixed_buffer
        && max_len <= buf.len()
      {
        store.put_large_fixed(tbl, batch_li, buf).await?;
      } else {
        store.put_large(tbl, batch_li).await?;
      }
      Ok(Response::Put)
    }
    Request::GetLarge { tbl, key } => {
      let fixed_buffer = state.fixed_buffer.as_mut();
      let store = state.stores.store_mut(tbl)?;
      let value = if let Some(buf) = fixed_buffer {
        match store.get_large_into(tbl, key, buf).await {
          Ok(value) => value.map(|into| LargeValueReply {
            value: buf.as_slice()[..into.value_len].to_vec(),
            metadata: into.metadata,
          }),
          Err(KvError::InvalidArgs) => {
            store
              .get_large(tbl, key)
              .await?
              .map(|large| LargeValueReply {
                value: large.value,
                metadata: large.metadata,
              })
          }
          Err(err) => return Err(err),
        }
      } else {
        store
          .get_large(tbl, key)
          .await?
          .map(|large| LargeValueReply {
            value: large.value,
            metadata: large.metadata,
          })
      };
      Ok(Response::GetLarge(value))
    }
    Request::Reopen { tbl } => {
      // StandbyReplica：先把该分区从 master 追一次再打开本地 manifest，
      // 对齐 C++ `ReopenTask::Reopen`（`RsyncPartition` + `WaitIo` 后才继续）。
      sync_standby_partition(state, tbl).await?;
      // main branch 下 `prepare_table` 不再预先 ensure：先建分区槽位再 reopen。
      state.ensure_slot(tbl)?;
      // 单次尝试：`ResourceMissing` 由事件循环的延迟队列重试，不在本任务内
      // `sleep`（否则整个 shard 线程被阻塞最长 `retry_times * pending_time`）。
      state.stores.store_mut(tbl)?.reopen(tbl).await?;
      state.table_open.mark(tbl, state.term, &state.branch);
      state.stores.touch(tbl);
      Ok(Response::Reopen)
    }
    Request::ReopenArchive { tbl, tag } => {
      // StandbyReplica 与普通 Reopen 一样先追一次 master（同步会带上归档
      // manifest）；Cloud 下 `read_manifest_file` 会按需回源远端对象。
      sync_standby_partition(state, tbl).await?;
      // 归档重开不经过 `ensure_table_ready`（避免懒建空表），只需分区槽位。
      state.ensure_slot(tbl)?;
      state
        .stores
        .store_mut(tbl)?
        .reopen_archive(tbl, tag)
        .await?;
      state.table_open.mark(tbl, state.term, &state.branch);
      Ok(Response::Reopen)
    }
    Request::ReopenExternal { tbl } => {
      // 仅 Cloud / StandbyReplica：Local / Mem 没有远端状态可采纳。
      if !state.reopen_on_missing {
        return Err(KvError::InvalidArgs);
      }
      // StandbyReplica 先追 master；master 也没有该分区时，本地 manifest 已被
      // 同步路径删掉，这里只清内存状态并回「等待远端」，不以空表起跑。
      if state.standby.is_some() {
        match sync_standby_partition(state, tbl).await {
          Ok(()) => {}
          Err(KvError::NotFound | KvError::ResourceMissing) => {
            state.stores.evict_table(tbl);
            state.table_open.forget(tbl);
            return Ok(Response::ReopenExternal(InstallOutcome::WaitingRemote));
          }
          Err(err) => return Err(err),
        }
      }
      state.ensure_slot(tbl)?;
      let outcome = state
        .stores
        .store_mut(tbl)?
        .install_external_snapshot(tbl)
        .await?;
      if matches!(outcome, InstallOutcome::Installed) {
        state.table_open.mark(tbl, state.term, &state.branch);
      } else {
        state.table_open.forget(tbl);
      }
      Ok(Response::ReopenExternal(outcome))
    }
    // 布局位宽只取 shard 配置：请求不携带，避免调用方传入与盘面不符的位宽
    // 让 retained file_id 换算错位（删活文件）。
    Request::FileGc { tbl } => {
      let shift = state.pages_per_file_shift;
      Ok(Response::FileGc(
        state.stores.store_mut(tbl)?.file_gc(tbl, shift).await?,
      ))
    }
    Request::LocalGc { tbl } => {
      let shift = state.pages_per_file_shift;
      Ok(Response::LocalGc(
        state.stores.store_mut(tbl)?.local_gc(tbl, shift).await?,
      ))
    }
    Request::Compact {
      tbl,
      file_id,
      pages_per_file_shift,
    } if *pages_per_file_shift == state.pages_per_file_shift => Ok(Response::Compact(
      state
        .stores
        .store_mut(tbl)?
        .compact_file(tbl, *file_id, *pages_per_file_shift)
        .await?,
    )),
    Request::Compact { .. } => Err(KvError::InvalidArgs),
    Request::Truncate { tbl, position } => {
      let maintain = {
        let store = state.stores.store_mut(tbl)?;
        if position.is_empty() {
          store.truncate(tbl).await?;
        } else {
          store.truncate_from(tbl, position).await?;
        }
        store.needs_maintain(tbl)
      };
      // 对齐 C++ Truncate 走 `UpdateMeta` 触发 compact / GC：维护失败只告警，
      // 不回滚已生效的截断。
      if maintain && let Err(err) = state.stores.store_mut(tbl)?.maintain(tbl).await {
        log::warn!(target: "fkv_shard", "truncate 后维护失败 tbl={tbl} err={err}");
      }
      Ok(Response::Truncate)
    }
    Request::CleanExpired { tbl } => {
      let now_ms = wall_clock_ms();
      let (stats, maintain) = {
        let store = state.stores.store_mut(tbl)?;
        let stats = store.clean_expired(tbl, now_ms).await?;
        let maintain = store.needs_maintain(tbl);
        (stats, maintain)
      };
      if maintain && let Err(err) = state.stores.store_mut(tbl)?.maintain(tbl).await {
        log::warn!(target: "fkv_shard", "clean_expired 后维护失败 tbl={tbl} err={err}");
      }
      Ok(Response::CleanExpired(CleanExpiredReply {
        deleted: stats.deleted,
        ttl_deleted: stats.ttl_deleted,
        next_expire_ts: stats.next_expire_ts,
      }))
    }
    Request::Drop { tbl } => {
      // 本地活跃 manifest 固定名（`manifest_main_0`）不携带 term，旧 owner
      // 无法仅凭文件名与新 owner 隔离。Drop 前先读 term 文件确认本实例仍是
      // 当前 owner：更高 term 已接管时拒绝并标记 Expired（对齐 C++ 高 term
      // 只删自己 `manifest_<branch>_<term>` 时的 fencing 效果）。
      if let Some(term) = state.term {
        let branch = state.branch.clone();
        let current = state
          .stores
          .backend()
          .read_term(&branch, state.shard_id)
          .await?;
        if current.is_some_and(|current| current > term) {
          state.set_lifecycle(DsShardState::Expired);
          return Err(KvError::ExpiredTerm);
        }
      }
      // Drop 不依赖表已打开：`open_empty` 的失败分区 / 损坏 manifest 也要能删。
      // `store_mut` 在槽位不存在时返回 ResourceMissing，这里按需建 Store。
      let branch = state.branch.clone();
      let term = state.term.unwrap_or(0);
      let shift = state.pages_per_file_shift;
      let store = state.stores.ensure_store(tbl, &branch, term)?;
      store.drop_table(tbl).await?;
      // 对齐 C++ `BatchWriteTask::Drop` 末尾的 `TryAddFileGc`：逻辑删除后立刻
      // 跑一轮 FileGc。这里同步执行（C++ 是低优先排队），语义相同：只回收
      // 不再被任何 active / branch / archive manifest 引用的文件。GC 失败只
      // 告警，不回滚已完成的 Drop。
      if let Err(err) = store.file_gc(tbl, shift).await {
        log::warn!(target: "fkv_shard", "drop 后 FileGc 失败 tbl={tbl} err={err}");
      }
      state.table_open.forget(tbl);
      Ok(Response::Drop)
    }
    Request::CreateArchive {
      tbl,
      tag,
      num_retained,
    } => {
      // 空 tag：自动生成定宽微秒时间戳（对齐 C++ `background_write.cpp` 空 tag 走
      // 时间戳；定宽见 `fkv_api::archive_tag`，保证 prune 的名字排序=时间排序）。
      let generated;
      let tag = if tag.is_empty() {
        generated = fkv_api::archive_tag();
        generated.as_str()
      } else {
        tag.as_str()
      };
      if state.stores.store(tbl).is_err() {
        // 尚无 manifest 的空分区：C++ `CreateArchive` 直接成功且不写文件。
        return Ok(Response::CreateArchive(ArchiveReply {
          name: String::new(),
          bytes: 0,
          removed_num: 0,
        }));
      }
      let info = state
        .stores
        .store_mut(tbl)?
        .create_archive(tbl, tag, *num_retained)
        .await?;
      Ok(Response::CreateArchive(ArchiveReply {
        name: info.name,
        bytes: info.bytes,
        removed_num: info.removed_num,
      }))
    }
    Request::DeleteArchive { tbl, tag } => {
      // 只删归档文件，不为它懒建空表；分区槽位承载 branch / manifest 身份。
      state.ensure_slot(tbl)?;
      let removed = state
        .stores
        .store_mut(tbl)?
        .delete_archive(tbl, tag)
        .await?;
      Ok(Response::DeleteArchive(removed))
    }
    Request::Prewarm { tbl } => {
      // 仅 Cloud 后端能预热；Local / Mem / Standby 在 `StoreBackend::prewarm_table`
      // 内显式返回 `InvalidArgs`（不静默成功）。预热只写本地 cache，不碰
      // Store 的表状态，因此借 `&Store` 即可。
      let stats = state.stores.backend().inner().prewarm_table(tbl).await?;
      // 显式 Prewarm 请求必须把本轮失败回传给调用方（API 注释承诺
      // `Err(KvError)`）；后台 crond 走 `prewarm_tables`，仍以统计报告为准。
      match stats.completion_reason {
        CompletionReason::Success => Ok(Response::Prewarm),
        CompletionReason::DiskFull => Err(KvError::OutOfSpace),
        CompletionReason::Shutdown => Err(KvError::NotRunning),
        CompletionReason::ListingError | CompletionReason::DownloadError => Err(KvError::CloudErr),
      }
    }
    Request::ListStandbyPartition => {
      // 非 standby 模式没有 master 来源，显式拒绝；不走「返回空列表」的静默路径。
      let Some(service) = state.standby.clone() else {
        return Err(KvError::InvalidArgs);
      };
      Ok(Response::ListStandbyPartition(
        service.list_remote_partition_li().await?,
      ))
    }
    Request::CreateBranch { tbl, branch } => {
      if state.stores.store(tbl).is_err() {
        // 空分区：C++ `CreateBranch` 成功但无 branch manifest。
        return Ok(Response::CreateBranch(BranchReply {
          name: String::new(),
          bytes: 0,
        }));
      }
      let info = state
        .stores
        .store_mut(tbl)?
        .create_branch(tbl, branch)
        .await?;
      Ok(Response::CreateBranch(BranchReply {
        name: info.name,
        bytes: info.bytes,
      }))
    }
    Request::DeleteBranch { tbl, branch } => {
      let removed = if state.stores.store(tbl).is_ok() {
        state
          .stores
          .store_mut(tbl)?
          .delete_branch(tbl, branch)
          .await?
      } else {
        // 空分区 / 未打开：仍要清 term 文件，manifest 数为 0。
        0
      };
      // 对齐 C++ `DeleteBranchFiles`：branch 的 manifest 删完后同时删
      // `CURRENT_TERM_<branch>_<shard>`，否则重建同名 branch 时旧高 term
      // 会让 attach 直接 ExpiredTerm。
      let normalized = normalize_branch_name(branch).ok_or(KvError::InvalidArgs)?;
      state
        .stores
        .backend()
        .remove_term(normalized.as_ref(), state.shard_id)
        .await?;
      Ok(Response::DeleteBranch(removed))
    }
    Request::AttachShard {
      branch,
      shard_id,
      term,
    } => {
      // 分支名先归一到盘面 / term 文件用的规范形式（小写、仅 `[a-z0-9-]`）：
      // 不归一会让 `"MAIN"` 写出 `CURRENT_TERM_MAIN_0`、并与 `MAIN_BRANCH_NAME`
      // 的判定全面失配（sto-69）。
      let branch = normalize_branch_name(branch).ok_or(KvError::InvalidArgs)?;
      // shard 身份固定在启动配置：别的 shard id 走同一线程会共用 term 文件，
      // fencing 失效（旧主仍能写）。
      if *shard_id != state.shard_id {
        return Err(KvError::InvalidArgs);
      }
      if state.admit_attach(&branch, *shard_id, *term)? == AttachKind::Idempotent {
        // 同 shard / branch / term 重复 attach：不驱逐 table 状态，不清缓存。
        return Ok(Response::AttachShard);
      }
      info!(
        target: "fkv_shard",
        "shard attach shard_id={} branch={} term={}",
        shard_id,
        branch,
        term
      );
      // fencing token 持久化：同 term 幂等，旧 term 必须被磁盘 / 云端拒绝。
      // Local 模式同样落 `CURRENT_TERM_<branch>_<shard_id>`（与 Cloud / Mem 一致）：
      // `Request::Put { term }` 的 fencing 契约不因后端而分叉，交叉进程重新 attach
      // 时由该文件拒绝旧 term；C++ Local 不写该文件属于已知差异（sto-61c）。
      state
        .stores
        .backend()
        .upsert_term(&branch, *shard_id, *term)
        .await?;
      // 身份切换：旧分区 Store 全部作废（各自持有旧 branch/term 的 manifest 与
      // 文件身份）；共享后端只统一失效一次路径 / fd 缓存。
      state.stores.clear();
      state.stores.backend().set_file_identity(&branch, *term);
      state.set_lifecycle(DsShardState::Attached);
      state.term = Some(*term);
      state.branch = branch.into();
      // 身份变化：在飞读已在读窗口 drain，代际 +1 防止旧代结果误回执。
      state.table_open.bump();
      Ok(Response::AttachShard)
    }
    Request::DetachShard { shard_id } => {
      if state.admit_detach(*shard_id)? {
        // 驱逐全部分区 Store 与 fd/路径缓存（对齐 C++ EvictDsShard）。
        state.stores.clear();
        // 运行期身份归零（对齐 C++ `slot.term_.store(0)`）：detach 后本 shard 不再
        // 持有 term / branch，任何残留请求都先被 `NotRunning` 拒绝。旧 term 的重新
        // attach 仍由持久化 term 文件 CAS 拒绝（`claim_term`），不依赖内存状态。
        state.set_lifecycle(DsShardState::Detached);
        state.term = None;
        state.branch.clear();
        state.table_open.bump();
        info!(
          target: "fkv_shard",
          "shard detach shard_id={}",
          shard_id
        );
      }
      Ok(Response::DetachShard)
    }
    Request::Shutdown => Ok(Response::Shutdown),
    // 只读请求已在函数入口走 `dispatch_read`；此臂只为 match 穷尽。
    Request::Get { .. }
    | Request::Scan { .. }
    | Request::Floor { .. }
    | Request::ListArchiveTags { .. }
    | Request::ListObject { .. } => {
      let tbl = req.table().ok_or(KvError::InvalidArgs)?;
      dispatch_read(state.stores.store(tbl)?, req).await
    }
  }
}

#[cfg(test)]
mod tests {
  use std::collections::BTreeMap;

  use super::*;

  fn envelope(batch_li: Vec<BatchEntry>, term: Option<u64>) -> Envelope {
    let (reply, _rx) = oneshot::channel();
    Envelope {
      req: Request::Put {
        tbl: TableIdent::new("demo", 1).unwrap(),
        batch_li,
        term,
      },
      reply,
      cancel: Arc::new(AtomicBool::new(false)),
      deadline: None,
      reopen_retry: None,
    }
  }

  fn entry(key: &[u8], value: &[u8], timestamp: u64) -> BatchEntry {
    BatchEntry::upsert(key.to_vec(), value.to_vec(), timestamp)
  }

  #[test]
  fn merge_put_batches_keeps_cross_request_timestamp_semantics() {
    let envelope_li = vec![
      envelope(
        vec![
          entry(b"a", b"old", 1),
          entry(b"b", b"keep", 2),
          // 单请求内部同 key 也按 timestamp 折叠：ts=5 胜 ts=4。
          entry(b"c", b"first", 5),
          entry(b"c", b"last", 4),
        ],
        None,
      ),
      envelope(
        vec![
          entry(b"a", b"new", 2),
          // 跨请求旧 timestamp 不覆盖新值。
          entry(b"b", b"stale", 1),
          // c 的 timestamp 3 小于请求 1 合并后的 5，跳过。
          entry(b"c", b"stale", 3),
        ],
        None,
      ),
    ];
    let merged: BTreeMap<Vec<u8>, BatchEntry> = merge_put_batch_li(&envelope_li)
      .into_iter()
      .map(|entry| (entry.key.clone(), entry))
      .collect();
    assert_eq!(merged[b"a".as_slice()].value, b"new");
    assert_eq!(merged[b"b".as_slice()].value, b"keep");
    assert_eq!(merged[b"c".as_slice()].value, b"first");
  }

  #[test]
  fn merge_put_batches_equal_timestamp_later_request_wins() {
    let envelope_li = vec![
      envelope(vec![entry(b"k", b"first", 7)], Some(3)),
      envelope(vec![entry(b"k", b"second", 7)], Some(3)),
    ];
    let merged = merge_put_batch_li(&envelope_li);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].value, b"second");
  }

  /// 请求内代表条必须先结算，再参与跨请求 timestamp 比较。
  ///
  /// 反例：若单趟里边折请求内边比跨请求 ts，后请求的 ts5 会先覆盖老请求的
  /// ts7，再被请求内后写的 ts5 替换，错误得出 ts5。
  #[test]
  fn merge_put_batches_request_inner_rep_before_cross_request_compare() {
    let envelope_li = vec![
      envelope(vec![entry(b"k", b"old-req", 7)], None),
      envelope(
        vec![entry(b"k", b"fresh", 9), entry(b"k", b"fresh-last", 5)],
        None,
      ),
    ];
    let merged = merge_put_batch_li(&envelope_li);
    assert_eq!(merged.len(), 1);
    // 请求 2 内 ts=9 胜 ts=5；再与请求 1 的 ts=7 比较，ts=9 胜。
    assert_eq!(merged[0].timestamp, 9);
    assert_eq!(merged[0].value, b"fresh");
  }

  /// 组上限场景（64 请求 × 单 key）：只有最后的幸存者被 clone，输出单条。
  #[test]
  fn merge_put_batches_large_group_keeps_last_request() {
    let envelope_li: Vec<Envelope> = (0..64u64)
      .map(|index| envelope(vec![entry(b"k", &index.to_be_bytes(), index)], None))
      .collect();
    let merged = merge_put_batch_li(&envelope_li);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].timestamp, 63);
    assert_eq!(merged[0].value, 63u64.to_be_bytes());
  }

  /// 输出保持 key 升序（旧 BTreeMap 语义），非 Put 请求跳过。
  #[test]
  fn merge_put_batches_outputs_key_order_and_skips_non_put() {
    let envelope_li = vec![
      envelope(vec![entry(b"z", b"1", 1), entry(b"a", b"1", 1)], None),
      raw_envelope(),
      envelope(vec![entry(b"m", b"2", 2)], None),
    ];
    let merged = merge_put_batch_li(&envelope_li);
    let key_li: Vec<&[u8]> = merged.iter().map(|entry| entry.key.as_slice()).collect();
    assert_eq!(
      key_li,
      vec![b"a".as_slice(), b"m".as_slice(), b"z".as_slice()]
    );
  }

  fn get_request() -> Request {
    Request::Get {
      tbl: TableIdent::new("demo", 1).unwrap(),
      key: b"k".to_vec(),
    }
  }

  fn raw_envelope() -> Envelope {
    let (reply, _rx) = oneshot::channel();
    Envelope {
      req: Request::Shutdown,
      reply,
      cancel: Arc::new(AtomicBool::new(false)),
      deadline: None,
      reopen_retry: None,
    }
  }

  /// 占满容量的高优先通道 + 指定入队等待策略的 sender。
  fn full_sender(queue_timeout: Option<Duration>) -> (ShardSender, MAsyncRx<Array<Envelope>>) {
    let (tx_hi, rx_hi) = bounded_async::<Envelope>(1);
    let (tx_lo, _rx_lo) = bounded_async::<Envelope>(1);
    tx_hi.try_send(raw_envelope()).unwrap();
    let sender = ShardSender {
      tx_hi,
      tx_lo,
      metrics: Arc::new(ShardMetric::new(0)),
      request_timeout: None,
      queue_timeout,
    };
    (sender, rx_hi)
  }

  /// 队列满 + 配置 queue_timeout：按时返回 Busy，不静默挂起。
  #[compio::test]
  async fn enqueue_full_returns_busy_after_queue_timeout() {
    let (sender, rx_hi) = full_sender(Some(Duration::from_millis(20)));
    let started = Instant::now();
    let Err(err) = sender.enqueue(get_request(), None).await else {
      panic!("队列满时必须回 Busy");
    };
    assert_eq!(err, KvError::Busy);
    assert!(started.elapsed() >= Duration::from_millis(20));
    drop(rx_hi);
  }

  /// 未配置 queue_timeout：保持旧行为，槽位释放后投递成功。
  #[compio::test]
  async fn enqueue_default_waits_for_free_slot() {
    let (sender, rx_hi) = full_sender(None);
    let pending = Box::pin(sender.enqueue(get_request(), None));
    let Either::Left(((), pending)) = select(Delay::new(Duration::from_millis(30)), pending).await
    else {
      panic!("队列满且未配置 queue_timeout 时不应立即完成");
    };
    // 消费一条，释放槽位。
    rx_hi.try_recv().unwrap();
    let handle = pending.await.unwrap();
    drop(handle);
  }

  /// 队列满 + 已过期 deadline：优先回 Timeout，不被队列阻塞。
  #[compio::test]
  async fn enqueue_full_with_expired_deadline_returns_timeout() {
    let (sender, rx_hi) = full_sender(None);
    let expired = Instant::now() - Duration::from_millis(1);
    let Err(err) = sender.enqueue(get_request(), Some(expired)).await else {
      panic!("已过 deadline 且队列满时必须回 Timeout");
    };
    assert_eq!(err, KvError::Timeout);
    drop(rx_hi);
  }

  /// 每表打开状态：代际 / term / branch 任一变化都必须判为「未就绪」，
  /// 迫使读请求走串行打开路径，而不是拿旧身份直接 spawn。
  #[test]
  fn table_slot_requires_same_epoch_and_identity() {
    let mut open = TableOpen::new();
    let tbl = TableIdent::new("slot", 1).unwrap();
    assert!(!open.slot_ready(&tbl, Some(7), "main"));
    open.mark(&tbl, Some(7), "main");
    assert!(open.slot_ready(&tbl, Some(7), "main"));
    // 换 term / branch（重新 attach）后旧登记失效。
    assert!(!open.slot_ready(&tbl, Some(8), "main"));
    assert!(!open.slot_ready(&tbl, Some(7), "other"));
    // attach / detach 提升代际：全部登记失效，重新打开后恢复。
    open.bump();
    assert!(!open.slot_ready(&tbl, Some(7), "main"));
    open.mark(&tbl, Some(8), "other");
    assert!(open.slot_ready(&tbl, Some(8), "other"));
  }

  /// spawn 的读任务各自持一份 `Rc<Store>` 克隆：在飞期间写路径拿不到独占
  /// （`Rc::get_mut` 失败），`drain` 后引用归一并恢复独占。这是「读并发 +
  /// 写前 drain 屏障」的核心不变量。
  #[compio::test]
  async fn read_flight_holds_store_until_drained() {
    use fkv_io_mem::MemStore;
    use fkv_store::Store;

    let metrics = Arc::new(ShardMetric::new(0));
    let io = MeteredIo::new(
      StoreBackend::Mem(Rc::new(
        MemStore::with_pages_per_file(512, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
          .with_segment_config(4096, 2),
      )),
      metrics.clone(),
    );
    let mut store = Rc::new(Store::new(io, 512, 16));
    assert!(Rc::get_mut(&mut store).is_some());

    let mut flight = ReadFlight::new(metrics.clone());
    let tbl = TableIdent::new("flight", 1).unwrap();
    for key in [b"a".as_slice(), b"b".as_slice()] {
      let (reply, _rx) = oneshot::channel();
      let envelope = Envelope {
        req: Request::Get {
          tbl: tbl.clone(),
          key: key.to_vec(),
        },
        reply,
        cancel: Arc::new(AtomicBool::new(false)),
        deadline: None,
        reopen_retry: None,
      };
      flight.push(Rc::clone(&store), 0, envelope, Instant::now());
    }
    // 两条读任务 + 主线程各持一份：写路径此时不能独占。
    assert_eq!(Rc::strong_count(&store), 3);
    assert!(Rc::get_mut(&mut store).is_none());
    // 读侧 in-flight gauge 在提交后同步可见（eng-32c）。
    assert_eq!(metrics.snapshot().read_in_flight, 2);
    assert_eq!(metrics.snapshot().read_in_flight_max, 2);

    flight.drain().await;
    // 未打开的表读作空：两条任务都完成并带回结果，不丢请求。
    assert!(flight.slot_li.iter().all(|slot| {
      slot
        .result
        .as_ref()
        .is_some_and(|result| matches!(result, Ok(Response::Get(None))))
    }));
    assert_eq!(metrics.snapshot().read_in_flight, 0);
    assert_eq!(Rc::strong_count(&store), 1);
    assert!(Rc::get_mut(&mut store).is_some());
  }

  /// 读批上限边界：第 `MAX_READ_BATCH` 条读任务后 `is_full` 为真，
  /// gauge / 峰值与上限一致，drain 后归零（eng-32c）。
  #[compio::test]
  async fn read_flight_is_full_at_batch_limit() {
    use fkv_io_mem::MemStore;
    use fkv_store::Store;

    let metrics = Arc::new(ShardMetric::new(0));
    let io = MeteredIo::new(
      StoreBackend::Mem(Rc::new(
        MemStore::with_pages_per_file(512, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
          .with_segment_config(4096, 2),
      )),
      metrics.clone(),
    );
    let store = Rc::new(Store::new(io, 512, 16));
    let tbl = TableIdent::new("full", 1).unwrap();
    let mut flight = ReadFlight::new(metrics.clone());
    for index in 0..MAX_READ_BATCH {
      assert!(!flight.is_full(), "第 {index} 条提交前不应达到上限");
      flight.push(
        Rc::clone(&store),
        0,
        req_envelope(read_req(&tbl)),
        Instant::now(),
      );
    }
    assert!(flight.is_full());
    assert_eq!(metrics.snapshot().read_in_flight, MAX_READ_BATCH as u64);
    assert_eq!(metrics.snapshot().read_in_flight_max, MAX_READ_BATCH as u64);

    flight.drain().await;
    assert_eq!(metrics.snapshot().read_in_flight, 0);
  }

  fn req_envelope(req: Request) -> Envelope {
    let (reply, _rx) = oneshot::channel();
    Envelope {
      req,
      reply,
      cancel: Arc::new(AtomicBool::new(false)),
      deadline: None,
      reopen_retry: None,
    }
  }

  fn read_req(tbl: &TableIdent) -> Request {
    Request::Get {
      tbl: tbl.clone(),
      key: b"k".to_vec(),
    }
  }

  /// 外部 `Reopen` 无同表 driver：不建延迟条目，立即交回回执（对齐 C++）。
  #[test]
  fn reopen_queue_external_reopen_does_not_wait_without_driver() {
    let mut queue = ReopenQueue::new(AutoReopenOption::new(2, Duration::from_millis(10)));
    let tbl = TableIdent::new("ext", 1).unwrap();
    let envelope = req_envelope(Request::Reopen { tbl: tbl.clone() });
    assert!(queue.park(envelope).is_some(), "外部 Reopen 不应挂起");
    assert!(queue.next_due().is_none());
    // 空 tag 的 `ReopenArchive` 同理。
    let envelope = req_envelope(Request::ReopenArchive {
      tbl,
      tag: String::new(),
    });
    assert!(
      queue.park(envelope).is_some(),
      "外部 ReopenArchive 不应挂起"
    );
    assert!(queue.next_due().is_none());
  }

  /// 外部 `Reopen` 可搭已有读 driver 的 waiter，且不消耗读请求的预算。
  #[test]
  fn reopen_queue_external_reopen_joins_existing_driver() {
    let mut queue = ReopenQueue::new(AutoReopenOption::new(1, Duration::from_millis(10)));
    let tbl = TableIdent::new("join", 1).unwrap();
    assert!(queue.park(req_envelope(read_req(&tbl))).is_none());
    assert!(
      queue
        .park(req_envelope(Request::Reopen { tbl: tbl.clone() }))
        .is_none()
    );
    let delayed = queue.heap.pop().unwrap();
    let mut wake_li = VecDeque::new();
    delayed.entry.wake(&mut wake_li);
    assert_eq!(wake_li.len(), 2);
    assert!(matches!(wake_li[0].req, Request::Get { .. }));
    assert!(matches!(wake_li[1].req, Request::Reopen { .. }));
  }

  /// 预算挂在读请求上：driver 成功重排后再次 `ResourceMissing` 才消耗下一轮，
  /// 耗尽后不再挂起；同表请求合并进同一个延迟条目。
  #[test]
  fn reopen_queue_read_retry_budget_is_per_request_and_merged() {
    let mut queue = ReopenQueue::new(AutoReopenOption::new(2, Duration::from_millis(10)));
    let tbl = TableIdent::new("budget", 1).unwrap();
    // 第一轮：两个读各自播种 2 → 消耗 1，合并成一条延迟条目。
    assert!(queue.park(req_envelope(read_req(&tbl))).is_none());
    assert!(queue.park(req_envelope(read_req(&tbl))).is_none());
    assert_eq!(queue.heap.len(), 1);

    // driver 成功：waiter 重排（预算剩 1），再次失败消耗最后一轮并合并。
    let delayed = queue.heap.pop().unwrap();
    let mut wake_li = VecDeque::new();
    delayed.entry.wake(&mut wake_li);
    assert_eq!(wake_li.len(), 2);
    for envelope in wake_li {
      assert!(queue.park(envelope).is_none());
    }
    assert_eq!(queue.heap.len(), 1);

    // 第三轮：预算已耗尽，两个读都不再挂起。
    let delayed = queue.heap.pop().unwrap();
    let mut wake_li = VecDeque::new();
    delayed.entry.wake(&mut wake_li);
    assert_eq!(wake_li.len(), 2);
    for envelope in wake_li {
      assert!(queue.park(envelope).is_some(), "预算耗尽后不应再挂起");
    }
    assert!(queue.next_due().is_none());
  }

  /// `retry_times == 0`：读请求立即失败，不建延迟条目。
  #[test]
  fn reopen_queue_zero_budget_does_not_park() {
    let mut queue = ReopenQueue::new(AutoReopenOption::new(0, Duration::from_millis(10)));
    let tbl = TableIdent::new("zero", 1).unwrap();
    assert!(queue.park(req_envelope(read_req(&tbl))).is_some());
    assert!(queue.next_due().is_none());
  }

  /// Fresh attach（Detached / Expired -> Attached）后旧身份排队写必须作废；
  /// 幂等 attach 与旧 term attach 导致的 Expired 不清队。
  #[test]
  fn fresh_attach_invalidates_queued_writes_only_on_reentry() {
    use DsShardState::{Attached, Detached, Expired};
    assert!(write_queue_stale(true, false, Detached, Attached));
    assert!(write_queue_stale(true, false, Expired, Attached));
    // 同 shard / branch / term 重复 attach：不驱逐 table 状态，也不清队。
    assert!(!write_queue_stale(true, false, Attached, Attached));
    // 旧 term attach 只让 shard 过期，排队写在 drain 时自会回 ExpiredTerm。
    assert!(!write_queue_stale(true, false, Attached, Expired));
    // 成功 detach；取消 / 超时的 detach 生命周期不变。
    assert!(write_queue_stale(false, true, Attached, Detached));
    assert!(!write_queue_stale(false, true, Detached, Detached));
    assert!(!write_queue_stale(false, false, Attached, Attached));
  }
}
