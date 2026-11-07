//! 每 shard 指标：请求计数、延迟直方图、cache / file 计数。
//!
//! 全部使用原子量，shard 线程只做 `fetch_add`，Runtime 线程随时 snapshot。

use std::{
  array::from_fn,
  fmt,
  sync::atomic::{AtomicU64, Ordering},
  time::Duration,
};

/// 延迟桶上界（微秒），最后一桶为 +Inf（共 `BUCKET_NUM = 17` 桶）。
///
/// 桶边界是 fkv 的自定义口径；跨实现比较 p99 前必须先统一桶边界。
pub const BUCKET_UPPER_US: [u64; 16] = [
  50, 100, 250, 500, 1_000, 2_500, 5_000, 10_000, 25_000, 50_000, 100_000, 250_000, 500_000,
  1_000_000, 2_500_000, 5_000_000,
];
pub const BUCKET_NUM: usize = BUCKET_UPPER_US.len() + 1;

/// 读批大小直方图上界（条数），最后一桶为 +Inf。
///
/// 与延迟桶分开：读批大小是计数而非微秒，复用 `BUCKET_UPPER_US` 会产出
/// 无法解读的桶。桶边界取 2 的幂，覆盖当前 `MAX_READ_BATCH = 32`。
pub const READ_BATCH_UPPER: [u64; 6] = [1, 2, 4, 8, 16, 32];
pub const READ_BATCH_BUCKET_NUM: usize = READ_BATCH_UPPER.len() + 1;

/// `Op` 的唯一定义处：一次列出变体与抓取名，同时生成枚举、`COUNT`、`ALL`、`as_str`。
/// 增删变体只改这一处，数组长度与变体表不会漂移。
macro_rules! op_enum {
  ($($variant:ident => $name:literal => [$($tag:literal),*]),+ $(,)?) => {
    /// 请求类型（fkv 内部口径）。
    ///
    /// 一个 `Op` 可以对应多个指标标签；对账时先按标签求和。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    #[repr(usize)]
    pub enum Op {
      $($variant),+
    }

    impl Op {
      /// 变体数，编译期算出。
      pub const COUNT: usize = [$(stringify!($variant)),+].len();

      /// 全部变体，顺序与 `repr(usize)` 判别值一致。
      pub const ALL: [Op; Op::COUNT] = [$(Op::$variant),+];

      /// 抓取名（`ShardSnapshot` 标签用）。
      pub fn as_str(self) -> &'static str {
        match self {
          $(Self::$variant => $name),+
        }
      }

      /// 对应的指标 `request_type` 标签。
      ///
      /// 空切片 = 没有独立标签；两个标签 = 两类请求合并成一个 `Op`。
      /// 标签全集见 [`METRIC_REQUEST_TYPE_LI`]。
      pub fn request_type_li(self) -> &'static [&'static str] {
        match self {
          $(Self::$variant => &[$($tag),*],)+
        }
      }

      /// 指标 `request_type` 标签反查 `Op`。
      ///
      /// 未注册标签返回 `None`；已注册的 19 个标签都能反查到唯一 `Op`。
      pub fn from_request_type(tag: &str) -> Option<Self> {
        Self::ALL
          .iter()
          .copied()
          .find(|op| op.request_type_li().contains(&tag))
      }
    }
  };
}

op_enum! {
  Put => "put" => ["batch_write"],
  Get => "get" => ["read"],
  Scan => "scan" => ["scan"],
  Reopen => "reopen" => ["reopen"],
  FileGc => "file_gc" => ["file_gc"],
  LocalGc => "local_gc" => ["local_gc"],
  Compact => "compact" => ["compact"],
  Truncate => "truncate" => ["truncate"],
  Floor => "floor" => ["floor"],
  Drop => "drop" => ["drop", "drop_table"],
  CreateArchive => "create_archive" => ["archive"],
  CreateBranch => "create_branch" => ["create_branch"],
  DeleteBranch => "delete_branch" => ["delete_branch"],
  CleanExpired => "clean_expired" => ["clean_expired"],
  AttachShard => "attach_shard" => ["attach_shard"],
  DetachShard => "detach_shard" => ["detach_shard"],
  Prewarm => "prewarm" => [],
  ListArchiveTags => "list_archive_tags" => [],
  ListObject => "list_object" => ["list_object"],
  ListStandbyPartition => "list_standby_partition" => ["list_standby_partition"],
  Shutdown => "shutdown" => [],
}

/// 指标注册的 `request_type` 标签全集（19 个）。
///
/// 与 C++ 对账口径（登记见 `eloqstore2fkv/COMPAT_BASELINE`「全局操作不计入 shard
/// 指标」）：C++ `RequestType` 另有 `global_archive` / `global_reopen` /
/// `global_list_archive_tags` / `global_create_branch` / `unknown` 五个名字，但它们
/// 由 `EloqStore` 在 store 层直接处理，从不进入 `Shard::OnTaskFinished` 的埋点，
/// 因此从不产出直方图 / 计数样本。fkv 的全局操作同样只在 runtime 层 fanout，
/// 指标落在每分区的 `archive` / `reopen` / `create_branch` 上，故这里不登记这些
/// 全局标签，避免产出 C++ 侧不存在、且把整体耗时错记到单个 shard 的序列。
pub const METRIC_REQUEST_TYPE_LI: [&str; 19] = [
  "read",
  "floor",
  "scan",
  "list_object",
  "list_standby_partition",
  "batch_write",
  "reopen",
  "truncate",
  "drop",
  "drop_table",
  "archive",
  "compact",
  "local_gc",
  "file_gc",
  "clean_expired",
  "create_branch",
  "delete_branch",
  "attach_shard",
  "detach_shard",
];

impl fmt::Display for Op {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

#[derive(Debug, Default)]
struct Histogram {
  bucket_count: [AtomicU64; BUCKET_NUM],
  count: AtomicU64,
  sum_ns: AtomicU64,
  min_ns: AtomicU64,
  max_ns: AtomicU64,
}

impl Histogram {
  fn new() -> Self {
    Self {
      bucket_count: from_fn(|_| AtomicU64::new(0)),
      count: AtomicU64::new(0),
      sum_ns: AtomicU64::new(0),
      min_ns: AtomicU64::new(u64::MAX),
      max_ns: AtomicU64::new(0),
    }
  }

  fn record(&self, elapsed_ns: u64) {
    let elapsed_us = elapsed_ns / 1_000;
    let index = BUCKET_UPPER_US
      .iter()
      .position(|upper| elapsed_us <= *upper)
      .unwrap_or(BUCKET_NUM - 1);
    self.bucket_count[index].fetch_add(1, Ordering::Relaxed);
    self.count.fetch_add(1, Ordering::Relaxed);
    self.sum_ns.fetch_add(elapsed_ns, Ordering::Relaxed);
    self.min_ns.fetch_min(elapsed_ns, Ordering::Relaxed);
    self.max_ns.fetch_max(elapsed_ns, Ordering::Relaxed);
  }

  fn snapshot(&self) -> HistogramSnapshot {
    let count = self.count.load(Ordering::Relaxed);
    let mut bucket_count = [0u64; BUCKET_NUM];
    for (index, bucket) in bucket_count.iter_mut().enumerate() {
      *bucket = self.bucket_count[index].load(Ordering::Relaxed);
    }
    HistogramSnapshot {
      bucket_count,
      count,
      sum_ns: self.sum_ns.load(Ordering::Relaxed),
      min_ns: if count == 0 {
        0
      } else {
        self.min_ns.load(Ordering::Relaxed)
      },
      max_ns: self.max_ns.load(Ordering::Relaxed),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistogramSnapshot {
  pub bucket_count: [u64; BUCKET_NUM],
  pub count: u64,
  pub sum_ns: u64,
  pub min_ns: u64,
  pub max_ns: u64,
}

impl HistogramSnapshot {
  pub fn avg_ns(&self) -> u64 {
    self.sum_ns.checked_div(self.count).unwrap_or(0)
  }

  /// 桶上界（微秒）；`None` 表示 +Inf 桶。
  pub fn bucket_upper_us(index: usize) -> Option<u64> {
    BUCKET_UPPER_US.get(index).copied()
  }
}

/// 读批大小直方图（计数型，桶边界见 [`READ_BATCH_UPPER`]）。
#[derive(Debug)]
struct BatchHistogram {
  bucket_count: [AtomicU64; READ_BATCH_BUCKET_NUM],
  count: AtomicU64,
  sum: AtomicU64,
  max: AtomicU64,
}

impl BatchHistogram {
  fn new() -> Self {
    Self {
      bucket_count: from_fn(|_| AtomicU64::new(0)),
      count: AtomicU64::new(0),
      sum: AtomicU64::new(0),
      max: AtomicU64::new(0),
    }
  }

  fn record(&self, size: u64) {
    let index = READ_BATCH_UPPER
      .iter()
      .position(|upper| size <= *upper)
      .unwrap_or(READ_BATCH_BUCKET_NUM - 1);
    self.bucket_count[index].fetch_add(1, Ordering::Relaxed);
    self.count.fetch_add(1, Ordering::Relaxed);
    self.sum.fetch_add(size, Ordering::Relaxed);
    self.max.fetch_max(size, Ordering::Relaxed);
  }

  fn snapshot(&self) -> ReadBatchSnapshot {
    let mut bucket_count = [0u64; READ_BATCH_BUCKET_NUM];
    for (index, bucket) in bucket_count.iter_mut().enumerate() {
      *bucket = self.bucket_count[index].load(Ordering::Relaxed);
    }
    ReadBatchSnapshot {
      bucket_count,
      count: self.count.load(Ordering::Relaxed),
      sum: self.sum.load(Ordering::Relaxed),
      max: self.max.load(Ordering::Relaxed),
    }
  }
}

/// 读批大小快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadBatchSnapshot {
  pub bucket_count: [u64; READ_BATCH_BUCKET_NUM],
  pub count: u64,
  pub sum: u64,
  pub max: u64,
}

impl ReadBatchSnapshot {
  /// 桶上界（条数）；`None` 表示 +Inf 桶。
  pub fn bucket_upper(index: usize) -> Option<u64> {
    READ_BATCH_UPPER.get(index).copied()
  }

  pub fn avg(&self) -> u64 {
    self.sum.checked_div(self.count).unwrap_or(0)
  }
}

#[derive(Debug)]
struct OpMetric {
  total: AtomicU64,
  error: AtomicU64,
  latency: Histogram,
}

impl OpMetric {
  fn new() -> Self {
    Self {
      total: AtomicU64::new(0),
      error: AtomicU64::new(0),
      latency: Histogram::new(),
    }
  }

  fn snapshot(&self, op: Op) -> OpSnapshot {
    OpSnapshot {
      op,
      total: self.total.load(Ordering::Relaxed),
      error: self.error.load(Ordering::Relaxed),
      latency: self.latency.snapshot(),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpSnapshot {
  pub op: Op,
  pub total: u64,
  pub error: u64,
  pub latency: HistogramSnapshot,
}

#[derive(Debug)]
pub struct ShardMetric {
  shard_id: u32,
  op_metric_li: [OpMetric; Op::COUNT],
  bytes_read: AtomicU64,
  bytes_written: AtomicU64,
  page_reads: AtomicU64,
  page_writes: AtomicU64,
  manifest_reads: AtomicU64,
  manifest_writes: AtomicU64,
  segment_reads: AtomicU64,
  segment_writes: AtomicU64,
  file_reads: AtomicU64,
  file_writes: AtomicU64,
  file_removes: AtomicU64,
  cache_hit: AtomicU64,
  cache_miss: AtomicU64,
  cache_evict: AtomicU64,
  /// 云 overlay 本地 cache 当前占用（gauge，对应 C++ `local_space_used`）。
  cache_bytes: AtomicU64,
  /// index page 缓存占用（gauge，对应 C++ `index_buffer_pool_used`）。
  index_cache_bytes: AtomicU64,
  fsync_num: AtomicU64,
  dir_fsync_num: AtomicU64,
  /// 写队列深度（排队等待启动的写请求数，跨分区合计，gauge）。
  write_queue_depth: AtomicU64,
  /// 全局在跑写任务数（gauge）。
  write_running: AtomicU64,
  /// 读侧在飞任务数（gauge，eng-32c）：读请求 spawn 后 +1、回收后 -1。
  read_in_flight: AtomicU64,
  /// 读侧在飞任务峰值（gauge，进程内单调，eng-32c）。
  read_in_flight_max: AtomicU64,
  /// 读批大小分布（eng-32c）：每次批量提交的读请求条数。
  read_batch_size: BatchHistogram,
  /// 全 shard 在飞 IO 操作数（gauge，eng-32c）：IO 装饰器每个异步 IO 进出各一次。
  io_in_flight: AtomicU64,
  /// 在飞 IO 峰值（gauge，进程内单调，eng-32c）。
  io_in_flight_max: AtomicU64,
  /// 时间片到点、为低优先让出的次数（eng-34c）。
  yield_total: AtomicU64,
  /// 高优先连续服务超过 `max_processing_time_us` 预算的轮数（eng-34c）。
  round_over_budget_total: AtomicU64,
  /// 低优先通道排队深度（gauge，eng-34c）：入队 +1、被 shard 取走 -1。
  low_pq_depth: AtomicU64,
  /// 低优先通道入队累计次数（单调，eng-34c）：入队侧 +1，永不回退。
  ///
  /// 深度会被 shard 取走侧清零，外部采样只能看到瞬时值；累计入队次数与
  /// `low_pq_depth_max` 单调递增，用于判断「低优先通道确实积压过」。
  low_pq_enqueued_total: AtomicU64,
  /// 低优先通道深度历史峰值（单调，eng-34c）：每次入队后 `fetch_max`。
  low_pq_depth_max: AtomicU64,
}

impl ShardMetric {
  pub fn new(shard_id: u32) -> Self {
    Self {
      shard_id,
      op_metric_li: from_fn(|_| OpMetric::new()),
      bytes_read: AtomicU64::new(0),
      bytes_written: AtomicU64::new(0),
      page_reads: AtomicU64::new(0),
      page_writes: AtomicU64::new(0),
      manifest_reads: AtomicU64::new(0),
      manifest_writes: AtomicU64::new(0),
      segment_reads: AtomicU64::new(0),
      segment_writes: AtomicU64::new(0),
      file_reads: AtomicU64::new(0),
      file_writes: AtomicU64::new(0),
      file_removes: AtomicU64::new(0),
      cache_hit: AtomicU64::new(0),
      cache_miss: AtomicU64::new(0),
      cache_evict: AtomicU64::new(0),
      cache_bytes: AtomicU64::new(0),
      index_cache_bytes: AtomicU64::new(0),
      fsync_num: AtomicU64::new(0),
      dir_fsync_num: AtomicU64::new(0),
      write_queue_depth: AtomicU64::new(0),
      write_running: AtomicU64::new(0),
      read_in_flight: AtomicU64::new(0),
      read_in_flight_max: AtomicU64::new(0),
      read_batch_size: BatchHistogram::new(),
      io_in_flight: AtomicU64::new(0),
      io_in_flight_max: AtomicU64::new(0),
      yield_total: AtomicU64::new(0),
      round_over_budget_total: AtomicU64::new(0),
      low_pq_depth: AtomicU64::new(0),
      low_pq_enqueued_total: AtomicU64::new(0),
      low_pq_depth_max: AtomicU64::new(0),
    }
  }

  pub fn shard_id(&self) -> u32 {
    self.shard_id
  }

  /// shard 主循环在请求完成后调用。
  pub fn observe(&self, op: Op, elapsed: Duration, is_error: bool) {
    let metric = &self.op_metric_li[op as usize];
    metric.total.fetch_add(1, Ordering::Relaxed);
    if is_error {
      metric.error.fetch_add(1, Ordering::Relaxed);
    }
    metric
      .latency
      .record(u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX));
  }

  pub fn add_page_read(&self, bytes: u64) {
    self.page_reads.fetch_add(1, Ordering::Relaxed);
    self.bytes_read.fetch_add(bytes, Ordering::Relaxed);
  }

  pub fn add_page_write(&self, bytes: u64) {
    self.page_writes.fetch_add(1, Ordering::Relaxed);
    self.bytes_written.fetch_add(bytes, Ordering::Relaxed);
  }

  pub fn add_manifest_read(&self, bytes: u64) {
    self.manifest_reads.fetch_add(1, Ordering::Relaxed);
    self.bytes_read.fetch_add(bytes, Ordering::Relaxed);
  }

  pub fn add_manifest_write(&self, bytes: u64) {
    self.manifest_writes.fetch_add(1, Ordering::Relaxed);
    self.bytes_written.fetch_add(bytes, Ordering::Relaxed);
  }

  pub fn add_segment_read(&self, bytes: u64) {
    self.segment_reads.fetch_add(1, Ordering::Relaxed);
    self.bytes_read.fetch_add(bytes, Ordering::Relaxed);
  }

  pub fn add_segment_write(&self, bytes: u64) {
    self.segment_writes.fetch_add(1, Ordering::Relaxed);
    self.bytes_written.fetch_add(bytes, Ordering::Relaxed);
  }

  pub fn add_file_read(&self, bytes: u64) {
    self.file_reads.fetch_add(1, Ordering::Relaxed);
    self.bytes_read.fetch_add(bytes, Ordering::Relaxed);
  }

  pub fn add_file_write(&self, bytes: u64) {
    self.file_writes.fetch_add(1, Ordering::Relaxed);
    self.bytes_written.fetch_add(bytes, Ordering::Relaxed);
  }

  pub fn add_file_remove(&self) {
    self.file_removes.fetch_add(1, Ordering::Relaxed);
  }

  pub fn inc_cache_hit(&self) {
    self.cache_hit.fetch_add(1, Ordering::Relaxed);
  }

  pub fn inc_cache_miss(&self) {
    self.cache_miss.fetch_add(1, Ordering::Relaxed);
  }

  pub fn inc_cache_evict(&self) {
    self.cache_evict.fetch_add(1, Ordering::Relaxed);
  }

  /// index 缓存的增量计数（shard 线程按累计差值调用）。
  pub fn add_cache_counters(&self, hit: u64, miss: u64, evict: u64) {
    self.cache_hit.fetch_add(hit, Ordering::Relaxed);
    self.cache_miss.fetch_add(miss, Ordering::Relaxed);
    self.cache_evict.fetch_add(evict, Ordering::Relaxed);
  }

  pub fn add_fsync(&self) {
    self.fsync_num.fetch_add(1, Ordering::Relaxed);
  }

  pub fn add_dir_fsync(&self) {
    self.dir_fsync_num.fetch_add(1, Ordering::Relaxed);
  }

  pub fn set_cache_bytes(&self, bytes: u64) {
    self.cache_bytes.store(bytes, Ordering::Relaxed);
  }

  /// index page 缓存当前占用（gauge）。
  pub fn set_index_cache_bytes(&self, bytes: u64) {
    self.index_cache_bytes.store(bytes, Ordering::Relaxed);
  }

  /// 写队列深度（gauge）：由 shard 线程在写队列变更后写入。
  pub fn set_write_queue_depth(&self, depth: u64) {
    self.write_queue_depth.store(depth, Ordering::Relaxed);
  }

  /// 全局在跑写任务数（gauge）：由 shard 线程在写队列变更后写入。
  pub fn set_write_running(&self, running: u64) {
    self.write_running.store(running, Ordering::Relaxed);
  }

  /// 读侧在飞任务数（gauge）：`ReadFlight` 在提交 / 回收后写入，并更新峰值。
  pub fn set_read_in_flight(&self, in_flight: u64) {
    self.read_in_flight.store(in_flight, Ordering::Relaxed);
    self
      .read_in_flight_max
      .fetch_max(in_flight, Ordering::Relaxed);
  }

  /// 记录一次读批提交的请求条数（`0` 不记，避免空批污染分布）。
  pub fn record_read_batch(&self, size: u64) {
    if size > 0 {
      self.read_batch_size.record(size);
    }
  }

  /// 一个异步 IO 开始：在飞数 +1 并更新峰值（`fkv_shard::metered::MeteredIo` 调用）。
  pub fn inc_io_in_flight(&self) {
    let in_flight = self.io_in_flight.fetch_add(1, Ordering::Relaxed) + 1;
    self
      .io_in_flight_max
      .fetch_max(in_flight, Ordering::Relaxed);
  }

  /// 一个异步 IO 完成 / 取消：在飞数 -1（`fkv_shard::metered::MeteredIo` 调用）。
  pub fn dec_io_in_flight(&self) {
    self.io_in_flight.fetch_sub(1, Ordering::Relaxed);
  }

  /// 时间片到点、把一条低优先插到高优先前执行的次数（eng-34c）。
  pub fn inc_yield(&self) {
    self.yield_total.fetch_add(1, Ordering::Relaxed);
  }

  /// 本轮到点时高优先已连续占用超过预算的次数（eng-34c）。
  ///
  /// 与 [`Self::inc_yield`] 分开：预算到点但低优先通道为空时只加这个计数 ——
  /// 它衡量「前台确实霸占过时间片」，yield 衡量「后台确实被放行过」。
  pub fn inc_round_over_budget(&self) {
    self.round_over_budget_total.fetch_add(1, Ordering::Relaxed);
  }

  /// 低优先请求入队（gauge +1，并同步入队累计计数与深度峰值）。
  pub fn inc_low_pq_depth(&self) {
    self.low_pq_enqueued_total.fetch_add(1, Ordering::Relaxed);
    let depth = self.low_pq_depth.fetch_add(1, Ordering::Relaxed) + 1;
    self.low_pq_depth_max.fetch_max(depth, Ordering::Relaxed);
  }

  /// 低优先请求被 shard 取走（gauge -1，饱和到 0）。
  ///
  /// 提交方与 shard 线程并发：入队失败回滚 / 极端抢占下可能先减后加，
  /// 饱和减法保证 gauge 不会回绕成 `u64::MAX`。
  pub fn dec_low_pq_depth(&self) {
    // 饱和减法用 CAS 循环：`fetch_update` 在新 nightly 已弃用、其替代
    // `try_update` 又高于本仓 MSRV。
    let mut current = self.low_pq_depth.load(Ordering::Relaxed);
    loop {
      let next = current.saturating_sub(1);
      match self.low_pq_depth.compare_exchange_weak(
        current,
        next,
        Ordering::Relaxed,
        Ordering::Relaxed,
      ) {
        Ok(_) => return,
        Err(actual) => current = actual,
      }
    }
  }

  pub fn snapshot(&self) -> ShardSnapshot {
    let cloud_cache_bytes = self.cache_bytes.load(Ordering::Relaxed);
    let index_cache_bytes = self.index_cache_bytes.load(Ordering::Relaxed);
    ShardSnapshot {
      shard_id: self.shard_id,
      op_li: Op::ALL
        .iter()
        .map(|op| self.op_metric_li[*op as usize].snapshot(*op))
        .collect(),
      bytes_read: self.bytes_read.load(Ordering::Relaxed),
      bytes_written: self.bytes_written.load(Ordering::Relaxed),
      page_reads: self.page_reads.load(Ordering::Relaxed),
      page_writes: self.page_writes.load(Ordering::Relaxed),
      manifest_reads: self.manifest_reads.load(Ordering::Relaxed),
      manifest_writes: self.manifest_writes.load(Ordering::Relaxed),
      segment_reads: self.segment_reads.load(Ordering::Relaxed),
      segment_writes: self.segment_writes.load(Ordering::Relaxed),
      file_reads: self.file_reads.load(Ordering::Relaxed),
      file_writes: self.file_writes.load(Ordering::Relaxed),
      file_removes: self.file_removes.load(Ordering::Relaxed),
      cache_hit: self.cache_hit.load(Ordering::Relaxed),
      cache_miss: self.cache_miss.load(Ordering::Relaxed),
      cache_evict: self.cache_evict.load(Ordering::Relaxed),
      cache_bytes: cloud_cache_bytes + index_cache_bytes,
      cloud_cache_bytes,
      index_cache_bytes,
      fsync_num: self.fsync_num.load(Ordering::Relaxed),
      dir_fsync_num: self.dir_fsync_num.load(Ordering::Relaxed),
      write_queue_depth: self.write_queue_depth.load(Ordering::Relaxed),
      write_running: self.write_running.load(Ordering::Relaxed),
      read_in_flight: self.read_in_flight.load(Ordering::Relaxed),
      read_in_flight_max: self.read_in_flight_max.load(Ordering::Relaxed),
      read_batch_size: self.read_batch_size.snapshot(),
      io_in_flight: self.io_in_flight.load(Ordering::Relaxed),
      io_in_flight_max: self.io_in_flight_max.load(Ordering::Relaxed),
      yield_total: self.yield_total.load(Ordering::Relaxed),
      round_over_budget_total: self.round_over_budget_total.load(Ordering::Relaxed),
      low_pq_depth: self.low_pq_depth.load(Ordering::Relaxed),
      low_pq_enqueued_total: self.low_pq_enqueued_total.load(Ordering::Relaxed),
      low_pq_depth_max: self.low_pq_depth_max.load(Ordering::Relaxed),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardSnapshot {
  pub shard_id: u32,
  pub op_li: Vec<OpSnapshot>,
  pub bytes_read: u64,
  pub bytes_written: u64,
  pub page_reads: u64,
  pub page_writes: u64,
  pub manifest_reads: u64,
  pub manifest_writes: u64,
  pub segment_reads: u64,
  pub segment_writes: u64,
  pub file_reads: u64,
  pub file_writes: u64,
  pub file_removes: u64,
  pub cache_hit: u64,
  pub cache_miss: u64,
  pub cache_evict: u64,
  /// cache 占用合计（`cloud_cache_bytes + index_cache_bytes`，兼容字段）。
  pub cache_bytes: u64,
  /// 云 overlay 本地 cache 当前占用（gauge，对应 C++ `local_space_used`）。
  pub cloud_cache_bytes: u64,
  /// index page 缓存当前占用（gauge，对应 C++ `index_buffer_pool_used`）。
  pub index_cache_bytes: u64,
  pub fsync_num: u64,
  pub dir_fsync_num: u64,
  /// 写队列深度（排队等待启动的写请求数，跨分区合计，gauge）。
  pub write_queue_depth: u64,
  /// 全局在跑写任务数（gauge）。
  pub write_running: u64,
  /// 读侧在飞任务数（gauge，eng-32c）。
  pub read_in_flight: u64,
  /// 读侧在飞任务峰值（gauge，进程内单调，eng-32c）。
  pub read_in_flight_max: u64,
  /// 读批大小分布（eng-32c）。
  pub read_batch_size: ReadBatchSnapshot,
  /// 全 shard 在飞 IO 操作数（gauge，eng-32c）。
  pub io_in_flight: u64,
  /// 在飞 IO 峰值（gauge，进程内单调，eng-32c）。
  pub io_in_flight_max: u64,
  /// 时间片到点让出的次数（累计计数，eng-34c）。
  pub yield_total: u64,
  /// 高优先连续服务超过预算的轮数（累计计数，eng-34c）。
  pub round_over_budget_total: u64,
  /// 低优先通道排队深度（gauge，eng-34c）。
  pub low_pq_depth: u64,
  /// 低优先通道入队累计次数（单调计数，eng-34c）。
  pub low_pq_enqueued_total: u64,
  /// 低优先通道深度历史峰值（单调计数，eng-34c）。
  pub low_pq_depth_max: u64,
}

impl ShardSnapshot {
  pub fn op(&self, op: Op) -> &OpSnapshot {
    &self.op_li[op as usize]
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricsSnapshot {
  pub shard_li: Vec<ShardSnapshot>,
}

impl MetricsSnapshot {
  pub fn total_requests(&self, op: Op) -> u64 {
    self.shard_li.iter().map(|shard| shard.op(op).total).sum()
  }

  pub fn total_errors(&self, op: Op) -> u64 {
    self.shard_li.iter().map(|shard| shard.op(op).error).sum()
  }
}
