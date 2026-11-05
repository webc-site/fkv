//! 每 shard 指标：请求计数、延迟直方图、cache / file 计数。
//!
//! 全部使用原子量，shard 线程只做 `fetch_add`，Runtime 线程随时 snapshot。

use std::{
  array::from_fn,
  fmt,
  future::Future,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  time::Duration,
};

use fkv_error::Result;
use fkv_io::{
  FileIo, FileVariant, FixedBuffer, FixedSegmentIo, ManifestIo, PageIo, SegmentIo, TermIo,
};
use fkv_types::{BranchFileRange, FileId, FilePageId, TableIdent};

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
  /// 全 shard 在飞 IO 操作数（gauge，eng-32c）：`MeteredIo` 每个异步 IO 进出各一次。
  io_in_flight: AtomicU64,
  /// 在飞 IO 峰值（gauge，进程内单调，eng-32c）。
  io_in_flight_max: AtomicU64,
  /// 时间片到点、为低优先让出的次数（eng-34c）。
  yield_total: AtomicU64,
  /// 高优先连续服务超过 `max_processing_time_us` 预算的轮数（eng-34c）。
  round_over_budget_total: AtomicU64,
  /// 低优先通道排队深度（gauge，eng-34c）：入队 +1、被 shard 取走 -1。
  low_pq_depth: AtomicU64,
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

  /// 一个异步 IO 开始：在飞数 +1 并更新峰值（`MeteredIo` 内部调用）。
  fn inc_io_in_flight(&self) {
    let in_flight = self.io_in_flight.fetch_add(1, Ordering::Relaxed) + 1;
    self
      .io_in_flight_max
      .fetch_max(in_flight, Ordering::Relaxed);
  }

  /// 一个异步 IO 完成 / 取消：在飞数 -1。
  fn dec_io_in_flight(&self) {
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

  /// 低优先请求入队（gauge +1）。
  pub fn inc_low_pq_depth(&self) {
    self.low_pq_depth.fetch_add(1, Ordering::Relaxed);
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

/// 给任意 `I` 包一层计数，用于 shard 的本地 IO。
#[derive(Debug, Clone)]
pub struct MeteredIo<I> {
  inner: I,
  metrics: Arc<ShardMetric>,
}

impl<I> MeteredIo<I> {
  pub fn new(inner: I, metrics: Arc<ShardMetric>) -> Self {
    Self { inner, metrics }
  }

  /// 内层 IO 引用。
  ///
  /// 计数只覆盖通用 IO 六件套；Cloud 的远端列举 / 预热等后端专有能力不在
  /// trait 上，需要调用方经此下探到具体后端（shard 的 per-table 请求）。
  pub fn inner(&self) -> &I {
    &self.inner
  }

  /// 包一层「在飞 IO」计数：future 首次 poll 时 +1，完成 / 取消 / 展开时 -1。
  ///
  /// 用 RAII 覆盖所有退出路径；被调用方 drop 的 IO future 不会漏计。
  async fn track<T, F: Future<Output = Result<T>>>(&self, fut: F) -> Result<T> {
    let _flight = IoFlightGuard::new(&self.metrics);
    fut.await
  }
}

/// 在飞 IO 守卫（见 [`MeteredIo::track`]）。
struct IoFlightGuard<'a> {
  metrics: &'a ShardMetric,
}

impl<'a> IoFlightGuard<'a> {
  fn new(metrics: &'a ShardMetric) -> Self {
    metrics.inc_io_in_flight();
    Self { metrics }
  }
}

impl Drop for IoFlightGuard<'_> {
  fn drop(&mut self) {
    self.metrics.dec_io_in_flight();
  }
}

impl<I: PageIo> PageIo for MeteredIo<I> {
  fn set_file_identity(&self, branch: &str, term: u64) {
    self.inner.set_file_identity(branch, term);
  }

  fn set_file_range_li(&self, tbl: &TableIdent, range_li: &[BranchFileRange]) {
    // 必须下探：Cloud 的 per-file term 范围决定跨 term 读的对象 key，
    // 默认 no-op 会让包裹后的后端按当前 identity 选错 key。
    self.inner.set_file_range_li(tbl, range_li);
  }

  fn preload_path_li(&self, tbl: &TableIdent) -> usize {
    self.inner.preload_path_li(tbl)
  }

  fn invalidate_path_cache(&self) {
    self.inner.invalidate_path_cache();
  }

  fn set_verify_checksum(&self, enabled: bool) {
    self.inner.set_verify_checksum(enabled);
  }

  fn evict_table(&self, tbl: &TableIdent) {
    self.inner.evict_table(tbl);
  }

  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    let result = self
      .track(self.inner.read_page(tbl, file_page_id, dst))
      .await;
    if result.is_ok() {
      self.metrics.add_page_read(dst.len() as u64);
    }
    result
  }

  async fn read_page_li(
    &self,
    tbl: &TableIdent,
    file_page_id_li: &[FilePageId],
    page_buf_li: &mut [Vec<u8>],
  ) -> Result<()> {
    // 转发而不是走默认逐页实现：内层可能一次提交多个 SQE。
    let result = self
      .track(self.inner.read_page_li(tbl, file_page_id_li, page_buf_li))
      .await;
    if result.is_ok() {
      let bytes: u64 = page_buf_li.iter().map(|page| page.len() as u64).sum();
      self.metrics.add_page_read(bytes);
    }
    result
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    let result = self
      .track(self.inner.write_page(tbl, file_page_id, src))
      .await;
    if result.is_ok() {
      self.metrics.add_page_write(src.len() as u64);
    }
    result
  }

  async fn write_page_li(&self, tbl: &TableIdent, page_li: &[(FilePageId, Vec<u8>)]) -> Result<()> {
    let result = self.track(self.inner.write_page_li(tbl, page_li)).await;
    if result.is_ok() {
      let bytes: u64 = page_li.iter().map(|(_, page)| page.len() as u64).sum();
      self.metrics.add_page_write(bytes);
    }
    result
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.track(self.inner.sync_data(tbl)).await?;
    self.metrics.add_fsync();
    Ok(())
  }
}

impl<I: ManifestIo> ManifestIo for MeteredIo<I> {
  async fn reset_manifest(&self, tbl: &TableIdent, size: u64) -> Result<()> {
    self.track(self.inner.reset_manifest(tbl, size)).await
  }

  async fn reset_manifest_file(&self, tbl: &TableIdent, name: &str, size: u64) -> Result<()> {
    self
      .track(self.inner.reset_manifest_file(tbl, name, size))
      .await
  }

  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    let result = self.track(self.inner.append_manifest(tbl, record)).await;
    if result.is_ok() {
      self.metrics.add_manifest_write(record.len() as u64);
      self.metrics.add_fsync();
    }
    result
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    let result = self.track(self.inner.switch_manifest(tbl, record)).await;
    if result.is_ok() {
      self.metrics.add_manifest_write(record.len() as u64);
      self.metrics.add_fsync();
      self.metrics.add_dir_fsync();
    }
    result
  }

  async fn append_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    let result = self
      .track(self.inner.append_manifest_file(tbl, name, record))
      .await;
    if result.is_ok() {
      self.metrics.add_manifest_write(record.len() as u64);
      self.metrics.add_fsync();
    }
    result
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    let result = self.track(self.inner.read_manifest(tbl)).await?;
    if let Some(bytes) = &result {
      self.metrics.add_manifest_read(bytes.len() as u64);
    }
    Ok(result)
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
    self.track(self.inner.drop_manifest(tbl)).await
  }

  async fn refresh_manifest(&self, tbl: &TableIdent) -> Result<()> {
    // 不额外读一次做指标：Cloud 内部已经下载并写入本地 cache。
    self.track(self.inner.refresh_manifest(tbl)).await
  }

  async fn drop_local_manifest(&self, tbl: &TableIdent) -> Result<()> {
    self.track(self.inner.drop_local_manifest(tbl)).await
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    let result = self
      .track(self.inner.write_manifest_file(tbl, name, record))
      .await;
    if result.is_ok() {
      self.metrics.add_manifest_write(record.len() as u64);
      self.metrics.add_fsync();
      self.metrics.add_dir_fsync();
    }
    result
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    self.track(self.inner.list_manifest_file_name_li(tbl)).await
  }

  /// 旁路文件列举必须下探：默认实现已删除，漏转发会编译失败。
  async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    self.track(self.inner.list_sidecar_file_name_li(tbl)).await
  }

  async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self
      .track(self.inner.remove_sidecar_file_name(tbl, name))
      .await
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    let result = self.track(self.inner.read_manifest_file(tbl, name)).await?;
    if let Some(bytes) = &result {
      self.metrics.add_manifest_read(bytes.len() as u64);
    }
    Ok(result)
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.track(self.inner.remove_manifest_file(tbl, name)).await
  }

  async fn prune_superseded_manifest_file_li(
    &self,
    tbl: &TableIdent,
    branch: &str,
    term: u64,
  ) -> Result<usize> {
    // 默认实现只走本地 list / remove，会绕过 Cloud 的远端剪枝。
    self
      .track(
        self
          .inner
          .prune_superseded_manifest_file_li(tbl, branch, term),
      )
      .await
  }
}

impl<I: FileIo> FileIo for MeteredIo<I> {
  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.track(self.inner.list_file_id_li(tbl)).await
  }

  async fn list_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    // 默认实现会把 identity 全记为 None，丢掉文件名里的 (branch, term)。
    self.track(self.inner.list_file_variants(tbl)).await
  }

  async fn list_local_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    // LocalGc 必须落到后端的“只看本地”实现；透传到默认 list_file_variants
    // 会让 Cloud 列举远端对象并在删除时触达远端。
    self.track(self.inner.list_local_file_variants(tbl)).await
  }

  async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>> {
    let result = self.track(self.inner.read_file(tbl, file_id)).await?;
    if let Some(bytes) = &result {
      self.metrics.add_file_read(bytes.len() as u64);
    }
    Ok(result)
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()> {
    let result = self.track(self.inner.write_file(tbl, file_id, bytes)).await;
    if result.is_ok() {
      self.metrics.add_file_write(bytes.len() as u64);
    }
    result
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>> {
    self.track(self.inner.file_size(tbl, file_id)).await
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.track(self.inner.remove_file(tbl, file_id)).await?;
    self.metrics.add_file_remove();
    Ok(())
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.track(self.inner.list_segment_file_id_li(tbl)).await
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self
      .track(self.inner.remove_segment_file(tbl, file_id))
      .await?;
    self.metrics.add_file_remove();
    Ok(())
  }

  async fn list_local_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.track(self.inner.list_local_file_id_li(tbl)).await
  }

  async fn remove_local_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self
      .track(self.inner.remove_local_file(tbl, file_id))
      .await?;
    self.metrics.add_file_remove();
    Ok(())
  }

  async fn list_local_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self
      .track(self.inner.list_local_segment_file_id_li(tbl))
      .await
  }

  async fn remove_local_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self
      .track(self.inner.remove_local_segment_file(tbl, file_id))
      .await?;
    self.metrics.add_file_remove();
    Ok(())
  }

  async fn remove_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .track(self.inner.remove_file_variant(tbl, file_id, branch, term))
      .await?;
    self.metrics.add_file_remove();
    Ok(())
  }

  async fn remove_local_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .track(
        self
          .inner
          .remove_local_file_variant(tbl, file_id, branch, term),
      )
      .await?;
    self.metrics.add_file_remove();
    Ok(())
  }

  async fn remove_segment_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .track(
        self
          .inner
          .remove_segment_file_variant(tbl, file_id, branch, term),
      )
      .await?;
    self.metrics.add_file_remove();
    Ok(())
  }

  async fn remove_local_segment_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .track(
        self
          .inner
          .remove_local_segment_file_variant(tbl, file_id, branch, term),
      )
      .await?;
    self.metrics.add_file_remove();
    Ok(())
  }

  async fn list_segment_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    self.track(self.inner.list_segment_file_variants(tbl)).await
  }

  async fn list_local_segment_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    self
      .track(self.inner.list_local_segment_file_variants(tbl))
      .await
  }

  async fn try_remove_empty_partition_dir(&self, tbl: &TableIdent) -> Result<bool> {
    self
      .track(self.inner.try_remove_empty_partition_dir(tbl))
      .await
  }
}

impl<I: SegmentIo> SegmentIo for MeteredIo<I> {
  fn write_segment_len(&self, src_len: usize) -> u64 {
    // 定长段后端会补零到整段；不透传会把落盘字节数记少。
    self.inner.write_segment_len(src_len)
  }

  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    let result = self
      .track(self.inner.read_segment(tbl, segment_fp_id, dst))
      .await;
    if result.is_ok() {
      self.metrics.add_segment_read(dst.len() as u64);
    }
    result
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> Result<()> {
    let result = self
      .track(self.inner.write_segment(tbl, segment_fp_id, src))
      .await;
    if result.is_ok() {
      self
        .metrics
        .add_segment_write(self.inner.write_segment_len(src.len()));
    }
    result
  }
}

impl<I: FixedSegmentIo> FixedSegmentIo for MeteredIo<I> {
  fn register_fixed_buffer(&self, len: usize) -> Result<FixedBuffer> {
    self.inner.register_fixed_buffer(len)
  }

  async fn read_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &mut FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    let result = self
      .track(
        self
          .inner
          .read_segment_fixed(tbl, segment_fp_id, buf, offset, len),
      )
      .await;
    if result.is_ok() {
      self.metrics.add_segment_read(len as u64);
    }
    result
  }

  async fn write_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    let result = self
      .track(
        self
          .inner
          .write_segment_fixed(tbl, segment_fp_id, buf, offset, len),
      )
      .await;
    if result.is_ok() {
      self
        .metrics
        .add_segment_write(self.inner.write_segment_len(len));
    }
    result
  }
}

impl<I: TermIo> TermIo for MeteredIo<I> {
  async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>> {
    self.track(self.inner.read_term(branch, shard_id)).await
  }

  async fn upsert_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<u64> {
    self
      .track(self.inner.upsert_term(branch, shard_id, term))
      .await
  }

  async fn remove_term(&self, branch: &str, shard_id: u32) -> Result<()> {
    self.track(self.inner.remove_term(branch, shard_id)).await
  }
}
