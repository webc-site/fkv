//! shard 生命周期：启动 / 装配、attach / detach、term、shutdown 与运行期状态定义。

use std::{
  io,
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
    mpsc::{SyncSender, sync_channel},
  },
  thread,
  thread::JoinHandle,
  time::Duration,
};

use compio::{driver::ProactorBuilder, runtime::Runtime};
use crossfire::{
  MAsyncRx, MAsyncTx,
  mpmc::{Array, bounded_async},
};
use fkv_api::{Request, Response};
use fkv_error::{KvError, Result};
use fkv_io::{FixedBuffer, FixedSegmentIo};
use fkv_large::SegmentLayout;
use fkv_metrics::{ShardMetric, ShardSnapshot};
use fkv_standby::RsyncStandbyService;
use fkv_store::AllocMode;
use fkv_task_reopen::AutoReopenOption;
use fkv_types::{StoreMode, TableIdent};
use futures_channel::oneshot;
use gxhash::HashMap;
use log::{error, info, warn};

use crate::{
  backend::{Backend, StoreBackend},
  metered::MeteredIo,
  partition::{PartitionStoreMap, ShardStore, StoreCfg},
  reopen::ReopenQueue,
  sender::{Envelope, ShardExit, ShardSender},
  standby::StandbyFlight,
};

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
  /// 物理页分配策略（对齐 C++ `data_append_mode`：true = Append，false = Pooled）。
  pub alloc_mode: AllocMode,
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
  /// 每表页缓存字节预算；0 表示不限制。
  pub page_cache_bytes: usize,
  /// 本 shard 的 root meta 字节预算；0 表示不限制。
  ///
  /// 计入 data / segment mapping、页缓存、压缩字典与 branch 元数据，
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

impl ShardConfig {
  /// 单测 / 嵌入式小配置：512 B 页、4 KiB segment、本地同步后端。
  ///
  /// 只给测试与极小规模嵌入用（`#[doc(hidden)]`）：生产默认值在
  /// `fkv_runtime::startup` 里按 `KvOption` 推导，不在这里。
  ///
  /// 放在本 crate 而不是测试夹具 crate：夹具 crate 依赖本 crate 会与
  /// `fkv_config`（dev 依赖夹具 crate）形成依赖环。
  #[doc(hidden)]
  pub fn for_tests(root: impl Into<PathBuf>) -> Self {
    Self {
      alloc_mode: AllocMode::Append,
      shard_id: 0,
      root: root.into(),
      page_size: 512,
      restart_interval: 16,
      index_restart_interval: 16,
      pages_per_file_shift: fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      segment_size: 4096,
      segments_per_file_shift: 2,
      large_value_threshold: 1024,
      fixed_buffer_size: 0,
      queue_size: 128,
      auto_reopen_retry_times: 0,
      auto_reopen_pending_time_us: 0,
      oom_retry_times: 0,
      compression_level: None,
      verify_checksum: true,
      // 与 `fkv_page::DEFAULT_OVERFLOW_POINTERS` 一致（由 `defaults_match_*` 测试钉住）。
      overflow_pointers: 16,
      file_amplify_factor: 0,
      segment_file_amplify_factor: 0,
      manifest_limit: 8 * 1024 * 1024,
      max_write_batch_pages: 32,
      max_inflight_write: 32 * 1024,
      fd_limit: 10_000,
      // 与 `fkv_config::DEFAULT_PAGE_CACHE_BYTES` 一致（同上测试钉住）。
      page_cache_bytes: 64 * 1024 * 1024,
      root_meta_cache_bytes: 0,
      io_queue_size: 4096,
      backend: Default::default(),
      mode: StoreMode::Local,
      standby: None,
      max_write_concurrency: 0,
      max_processing_time_us: 0,
      yield_budget_us: 0,
    }
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
/// 高 / 低优先请求通道：按优先级分离，各自承担 `queue_size` 的背压。
struct ShardChannels {
  tx_hi: MAsyncTx<Array<Envelope>>,
  rx_hi: MAsyncRx<Array<Envelope>>,
  tx_lo: MAsyncTx<Array<Envelope>>,
  rx_lo: MAsyncRx<Array<Envelope>>,
}
pub struct Shard {
  tx_hi: MAsyncTx<Array<Envelope>>,
  tx_lo: MAsyncTx<Array<Envelope>>,
  join: Option<JoinHandle<()>>,
  metrics: Arc<ShardMetric>,
  /// shard 线程内 `ShardState::lifecycle` 的原子镜像：运行时在请求出错后据此判断
  /// shard 的真实状态（本地 `Timeout` 时请求可能已执行并把 shard 置为 `Expired`）。
  lifecycle: ShardLifecycle,
  /// 线程退出信号：随 sender 下发，未回执请求在线程退出后排空为 NotRunning。
  exit: ShardExit,
}
impl Shard {
  /// 启动 shard：校验配置、建立请求通道、拉起专用线程，等握手完成后返回句柄。
  ///
  /// 线程内 Runtime 构建 / Store 装配失败都会经握手通道回传，保证
  /// `Shard::start` 不会把初始化失败报成成功（对齐 C++ `StartInstance` 的 fail_start）。
  pub fn start(config: ShardConfig) -> Result<Self> {
    // `RuntimeConfig` 直传路径不经 `KvOption::validate`：移位上界在这里兜住，
    // 否则 `1u64 << shift` 在 debug 下 panic、release 下掩码为 0。
    if config.queue_size == 0 || config.pages_per_file_shift >= 63 {
      return Err(KvError::InvalidArgs);
    }
    let layout = SegmentLayout::new(config.segment_size, config.segments_per_file_shift)?;
    // 两条有界通道按优先级分离，各自承担 `queue_size` 的背压。
    let ShardChannels {
      tx_hi,
      rx_hi,
      tx_lo,
      rx_lo,
    } = Self::bind_shards(config.queue_size);
    let metrics = Arc::new(ShardMetric::new(config.shard_id));
    let thread_metrics = metrics.clone();
    let lifecycle = ShardLifecycle::new(DsShardState::Detached);
    let thread_lifecycle = lifecycle.clone();
    // 线程退出信号：guard 随线程闭包存活，正常收尾或 panic 展开都会广播，
    // 仍在等待回执的请求据此排空为 NotRunning（test-05）。
    let (exit, exit_guard) = ShardExit::new();
    // 启动握手：线程内 Runtime 与 store 就绪后才回传结果，运行时初始化失败必须
    // 让 `Shard::start` 报错而不是返回假成功（对齐 C++ `StartInstance` 的 fail_start）。
    let (ready_tx, ready_rx) = sync_channel::<Result<()>>(1);
    let join = thread::Builder::new()
      .name("fkv-shard".to_owned())
      .spawn(move || {
        let _exit_guard = exit_guard;
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
        runtime.block_on(Self::run_shard_thread(
          config,
          layout,
          thread_metrics,
          thread_lifecycle,
          rx_hi,
          rx_lo,
          ready_tx,
        ));
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
      exit,
    })
  }

  /// 建立高 / 低两条有界请求通道：按优先级分离，各自承担 `queue_size` 的背压。
  fn bind_shards(queue_size: usize) -> ShardChannels {
    let (tx_hi, rx_hi) = bounded_async::<Envelope>(queue_size);
    let (tx_lo, rx_lo) = bounded_async::<Envelope>(queue_size);
    ShardChannels {
      tx_hi,
      rx_hi,
      tx_lo,
      rx_lo,
    }
  }

  /// 装配 shard 运行状态：IO 后端（含 Cloud 指标）、fixed buffer 与分区 Store 表。
  ///
  /// 只有后端构造会失败；fixed buffer 注册失败按原语义降级为不注册（仅告警）。
  fn assemble_state(
    config: &ShardConfig,
    layout: SegmentLayout,
    metrics: &Arc<ShardMetric>,
    thread_lifecycle: ShardLifecycle,
  ) -> Result<ShardState> {
    let backend = match build_backend(config, metrics) {
      Ok(backend) => backend,
      Err(err) => {
        error!(
          target: "fkv_shard",
          "shard backend init failed shard_id={} error={}",
          config.shard_id,
          err
        );
        return Err(err);
      }
    };
    let io = MeteredIo::new(backend, metrics.clone());
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
    let page_size = config.page_size;
    let restart_interval = config.restart_interval;
    let index_restart_interval = config.index_restart_interval;
    let large_value_threshold = if config.large_value_threshold == 0 {
      page_size / 2
    } else {
      config.large_value_threshold
    };
    // 每分区独立 Store：共享 IO 后端，跨分区写可并发。
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
      page_cache_bytes: config.page_cache_bytes,
      pages_per_file_shift: config.pages_per_file_shift,
      yield_budget_us: config.yield_budget_us,
      compression_level: config.compression_level,
      alloc_mode: config.alloc_mode,
    };
    Ok(ShardState {
      stores: PartitionStoreMap::new(io, store_cfg, config.root_meta_cache_bytes),
      fixed_buffer,
      large_read_out: false,
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
      standby_flight: StandbyFlight::new(),
      table_open: TableOpen::new(),
    })
  }

  /// shard 线程主体：装配 Store 与 reopen 重试队列，握手后进入事件循环。
  async fn run_shard_thread(
    config: ShardConfig,
    layout: SegmentLayout,
    thread_metrics: Arc<ShardMetric>,
    thread_lifecycle: ShardLifecycle,
    rx_hi: MAsyncRx<Array<Envelope>>,
    rx_lo: MAsyncRx<Array<Envelope>>,
    ready_tx: SyncSender<Result<()>>,
  ) {
    let state = match Self::assemble_state(&config, layout, &thread_metrics, thread_lifecycle) {
      Ok(state) => state,
      Err(err) => {
        let _ = ready_tx.send(Err(err));
        return;
      }
    };
    // `Reopen` 延迟重试队列：manifest 暂不可见时挂起请求，期间本 shard 继续
    // 服务其它请求（对齐 C++ `pending_reopens_` + delayed reopen 提升）。
    let mut reopen_li = ReopenQueue::new(AutoReopenOption::new(
      config.auto_reopen_retry_times,
      Duration::from_micros(config.auto_reopen_pending_time_us),
    ));
    // 握手：store 与事件循环已就绪，调用方拿到 `Ok` 即保证请求会被消费。
    let _ = ready_tx.send(Ok(()));
    Self::run_event_loop(
      state,
      thread_metrics.clone(),
      &mut reopen_li,
      &rx_hi,
      &rx_lo,
      &config,
    )
    .await;
    info!(
      target: "fkv_shard",
      "shard stop shard_id={}",
      thread_metrics.shard_id()
    );
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
      exit: self.exit.clone(),
      request_timeout: None,
      queue_timeout: None,
    }
  }

  pub async fn shutdown(mut self) -> Result<()> {
    let _ = self.request(Request::Shutdown).await;
    let Some(join) = self.join.take() else {
      return Ok(());
    };
    // join 移到短命线程异步回传：调用方线程一旦在 poll 栈上同步 join，shard
    // 线程 `fail_pending` 的回执唤醒会打满调用方 compio runtime 的跨线程同步
    // 队列（容量 64），`Remote::schedule` 满队自旋，双方互等 → shutdown 永不
    // 返回（test-05 死锁）。等待期间调用方线程返回 Pending 继续泵任务队列，
    // 回执唤醒得以排空；oneshot 与具体 executor 无关，任何异步上下文可用。
    let (done_tx, done_rx) = oneshot::channel();
    thread::Builder::new()
      .name("fkv-shard-join".to_owned())
      .spawn(move || {
        // shutdown future 可能已被取消（接收端 drop）：join 仍要完成以保证
        // shard 线程确定性退出，发送失败静默丢弃。
        done_tx.send(join.join()).ok();
      })
      .map_err(|_| KvError::IoFail)?;
    match done_rx.await {
      Ok(result) => result.map_err(|_| KvError::IoFail),
      // join 线程在回传前异常退出：按 IO 失败上报，不 panic。
      Err(_) => Err(KvError::IoFail),
    }
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
pub(crate) struct ShardState {
  /// 每分区独立 Store（共享 IO 后端 / 页缓存账本）：跨分区写可并发。
  pub(crate) stores: PartitionStoreMap,
  pub(crate) fixed_buffer: Option<FixedBuffer>,
  /// 是否有大值读任务借走了 fixed buffer（io-05）：同一时刻只允许一条
  /// `GetLarge` 在飞（buffer 独占 + 大值内存峰值约束），后来者排队。
  /// buffer 未配置时该标志仍生效（普通大值读也单飞，与旧内联串行语义一致）。
  pub(crate) large_read_out: bool,
  /// `StandbyReplica`：reopen 前的分区追赶服务。
  pub(crate) standby: Option<Arc<RsyncStandbyService>>,
  /// standby 追赶单飞集合（io-33）：追赶任务 + 挂起的触发请求。
  pub(crate) standby_flight: StandbyFlight,
  pub(crate) lifecycle: DsShardState,
  /// `lifecycle` 的跨线程镜像，随每次赋值同步写入。
  lifecycle_mirror: ShardLifecycle,
  pub(crate) term: Option<u64>,
  pub(crate) branch: String,
  /// 启动配置的 shard id：attach 必须与之相同（shard 身份不可在运行期改绑）。
  pub(crate) shard_id: u32,
  /// 存储层 data 文件页数位移：`Gc` / `Compact` 请求必须与之一致。
  pub(crate) pages_per_file_shift: u8,
  /// Cloud / StandbyReplica：`ResourceMissing` 表示 manifest 暂不可见，读请求挂
  /// 等待重排，且不做本地空表懒创建（对齐 C++ `auto_reopen`）。
  pub(crate) reopen_on_missing: bool,
  /// 每表打开状态：读请求准入只做只读查询，打开动作收敛在串行路径（eng-32b）。
  pub(crate) table_open: TableOpen,
}
/// 每表打开状态：读路径只读查询，打开动作只发生在串行的准入路径。
///
/// `Store::is_open` 仍是权威事实源；本结构额外记录「打开时的 attach 代际与
/// 身份」，用于读任务完成时拒绝跨代结果。attach / detach / shutdown 会先 drain
/// 在飞读再改代际，因此正常情况下不会出现跨代结果，这里是线性化的第二道保险。
pub(crate) struct TableOpen {
  /// attach 代际：attach / detach 成功时自增。
  pub(crate) epoch: u64,
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
  pub(crate) fn new() -> Self {
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
  pub(crate) fn ready(
    &self,
    stores: &PartitionStoreMap,
    tbl: &TableIdent,
    term: Option<u64>,
    branch: &str,
  ) -> bool {
    stores.is_open(tbl) && self.slot_ready(tbl, term, branch)
  }

  /// 登记表侧的判定（不含 Store 实际打开状态）：代际与身份必须完全一致。
  pub(crate) fn slot_ready(&self, tbl: &TableIdent, term: Option<u64>, branch: &str) -> bool {
    self
      .slot_map
      .get(tbl)
      .is_some_and(|slot| slot.epoch == self.epoch && slot.term == term && slot.branch == branch)
  }

  /// 打开成功后登记（幂等）。
  pub(crate) fn mark(&mut self, tbl: &TableIdent, term: Option<u64>, branch: &str) {
    self.slot_map.insert(
      tbl.clone(),
      TableSlot {
        epoch: self.epoch,
        term,
        branch: branch.to_owned(),
      },
    );
  }

  pub(crate) fn forget(&mut self, tbl: &TableIdent) {
    self.slot_map.remove(tbl);
  }

  /// 身份变化（attach / detach）：代际 +1，清空全部表登记。
  pub(crate) fn bump(&mut self) {
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
  pub(crate) fn store_mut(&mut self, tbl: &TableIdent) -> Result<&mut ShardStore> {
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
  pub(crate) fn ensure_slot(&mut self, tbl: &TableIdent) -> Result<()> {
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
pub(crate) enum DsShardState {
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
pub(crate) enum AttachKind {
  /// 已托管同一 shard / branch / term：幂等，不驱逐 table 状态。
  Idempotent,
  /// 从 `Detached` 进入，或 `Expired` 后重新托管：需要 claim term 并重置 Store。
  Fresh,
}
impl ShardState {
  /// 生命周期唯一写入口：线程内状态与跨线程镜像必须同步，否则运行时会在错误
  /// 之后按陈旧状态决定是否清视图。
  pub(crate) fn set_lifecycle(&mut self, next: DsShardState) {
    self.lifecycle = next;
    self.lifecycle_mirror.store(next);
  }

  /// 任务结果的统一收尾：任一任务返回 `ExpiredTerm` 即把整个 shard 置
  /// `Expired`（后续请求经 [`crate::dispatch::admit`] 快速失败）。
  ///
  /// 对齐 C++ `shard.cpp` `FinishTask`：任务错误为 `ExpiredTerm` 时对任务所属
  /// ds shard 调 `MarkShardExpired`，覆盖读 / 写 / Reopen / GC 全部任务类型
  /// （term 文件按 shard 共享，任一分区见过更高 term owner 就说明整 shard
  /// 陈旧）。云 manifest 采纳栅栏（sem-04）在惰性读 / Reopen 路径返回同一
  /// 错误，因此复用这条通道，而不是在各请求变体里重复判断。
  pub(crate) fn observe_task_err(&mut self, result: &Result<Response>) {
    if matches!(result, Err(KvError::ExpiredTerm)) {
      self.set_lifecycle(DsShardState::Expired);
    }
  }

  /// attach 合法转换判定。`Attached` / `Expired` 下只接受同一 shard 与 branch：
  /// 换 branch 或换 shard 必须先 detach（对齐 C++ `AttachShard` 要求 slot `Detached`）。
  pub(crate) fn admit_attach(
    &mut self,
    branch: &str,
    shard_id: u32,
    term: u64,
  ) -> Result<AttachKind> {
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
  pub(crate) fn admit_detach(&self, shard_id: u32) -> Result<bool> {
    match self.lifecycle {
      DsShardState::Detached => Ok(false),
      DsShardState::Attached | DsShardState::Expired if self.shard_id != shard_id => {
        Err(KvError::InvalidArgs)
      }
      DsShardState::Attached | DsShardState::Expired => Ok(true),
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{rc::Rc, sync::Arc};

  use fkv_api::Request;
  use fkv_error::KvError;
  use fkv_io_mem::MemStore;
  use fkv_large::SegmentLayout;
  use fkv_metrics::ShardMetric;
  use fkv_page::DEFAULT_OVERFLOW_POINTERS;
  use fkv_store::AllocMode as FkvStoreAllocMode;
  use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, MAIN_BRANCH_NAME, TableIdent};

  use super::{AttachKind, DsShardState, ShardLifecycle, ShardState, StandbyFlight, TableOpen};
  use crate::{
    backend::StoreBackend,
    dispatch::admit,
    metered::MeteredIo,
    partition::{PartitionStoreMap, StoreCfg},
  };

  /// 最小可用的 `ShardState`：Mem 后端 + 空分区表，只服务生命周期判定单测。
  fn test_state() -> ShardState {
    let metrics = Arc::new(ShardMetric::new(0));
    let io = MeteredIo::new(
      StoreBackend::Mem(Rc::new(
        MemStore::with_pages_per_file(512, DEFAULT_PAGES_PER_FILE_SHIFT)
          .with_segment_config(4096, 2),
      )),
      metrics,
    );
    let cfg = StoreCfg {
      page_size: 512,
      restart_interval: 16,
      index_restart_interval: 16,
      layout: SegmentLayout::new(4096, 2).unwrap(),
      large_value_threshold: 1024,
      verify_checksum: true,
      overflow_pointers: DEFAULT_OVERFLOW_POINTERS,
      file_amplify_factor: 0,
      segment_file_amplify_factor: 0,
      manifest_limit: 8 * 1024 * 1024,
      page_cache_bytes: 1024 * 1024,
      pages_per_file_shift: DEFAULT_PAGES_PER_FILE_SHIFT,
      yield_budget_us: 0,
      compression_level: None,
      alloc_mode: FkvStoreAllocMode::Append,
    };
    ShardState {
      stores: PartitionStoreMap::new(io, cfg, 0),
      fixed_buffer: None,
      large_read_out: false,
      standby: None,
      standby_flight: StandbyFlight::new(),
      lifecycle: DsShardState::Attached,
      lifecycle_mirror: ShardLifecycle::new(DsShardState::Attached),
      term: Some(3),
      branch: MAIN_BRANCH_NAME.to_owned(),
      shard_id: 0,
      pages_per_file_shift: DEFAULT_PAGES_PER_FILE_SHIFT,
      reopen_on_missing: true,
      table_open: TableOpen::new(),
    }
  }

  fn get_req() -> Request {
    Request::Get {
      tbl: TableIdent::new("fence", 1).unwrap(),
      key: b"k".to_vec(),
    }
  }

  /// sem-04：任务返回 `ExpiredTerm`（含云 manifest 采纳栅栏）后，整 shard
  /// 必须置 `Expired`，后续普通请求经 `admit` 快速失败；其它错误不改状态。
  #[test]
  fn expired_term_marks_shard_expired_and_fast_fails() {
    let mut state = test_state();
    assert_eq!(admit(&state, &get_req()), Ok(()));

    // 非 term 错误不改变生命周期。
    state.observe_task_err(&Err(KvError::ResourceMissing));
    assert_eq!(state.lifecycle, DsShardState::Attached);

    state.observe_task_err(&Err(KvError::ExpiredTerm));
    assert_eq!(state.lifecycle, DsShardState::Expired);
    assert_eq!(admit(&state, &get_req()), Err(KvError::ExpiredTerm));

    // 同 term 重新 attach 恢复托管（既有恢复路径语义不变）。
    assert_eq!(
      state.admit_attach(MAIN_BRANCH_NAME, 0, 3),
      Ok(AttachKind::Fresh)
    );
    assert_eq!(state.lifecycle, DsShardState::Expired);
    state.set_lifecycle(DsShardState::Attached);
    assert_eq!(admit(&state, &get_req()), Ok(()));
  }
}
