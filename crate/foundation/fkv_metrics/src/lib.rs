//! 每 shard 指标：请求计数、延迟直方图、cache / file 计数。
//!
//! 全部使用原子量，shard 线程只做 `fetch_add`，Runtime 线程随时 snapshot。

use std::{
  array::from_fn,
  fmt,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  time::Duration,
};

use fkv_error::Result;
use fkv_io::{FileIo, FixedBuffer, FixedSegmentIo, ManifestIo, PageIo, SegmentIo, TermIo};
use fkv_types::{FileId, FilePageId, TableIdent};

/// 延迟桶上界（微秒），最后一桶为 +Inf。
pub const BUCKET_UPPER_US: [u64; 16] = [
  50, 100, 250, 500, 1_000, 2_500, 5_000, 10_000, 25_000, 50_000, 100_000, 250_000, 500_000,
  1_000_000, 2_500_000, 5_000_000,
];
pub const BUCKET_NUM: usize = BUCKET_UPPER_US.len() + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(usize)]
pub enum Op {
  Put = 0,
  Get,
  Scan,
  Reopen,
  Gc,
  Compact,
  Truncate,
  Floor,
  Drop,
  CreateArchive,
  CreateBranch,
  DeleteBranch,
  AttachShard,
  DetachShard,
  Shutdown,
}

pub const OP_NUM: usize = 15;

impl Op {
  pub const ALL: [Op; OP_NUM] = [
    Op::Put,
    Op::Get,
    Op::Scan,
    Op::Reopen,
    Op::Gc,
    Op::Compact,
    Op::Truncate,
    Op::Floor,
    Op::Drop,
    Op::CreateArchive,
    Op::CreateBranch,
    Op::DeleteBranch,
    Op::AttachShard,
    Op::DetachShard,
    Op::Shutdown,
  ];

  pub fn as_str(self) -> &'static str {
    match self {
      Self::Put => "put",
      Self::Get => "get",
      Self::Scan => "scan",
      Self::Reopen => "reopen",
      Self::Gc => "gc",
      Self::Compact => "compact",
      Self::Truncate => "truncate",
      Self::Floor => "floor",
      Self::Drop => "drop",
      Self::CreateArchive => "create_archive",
      Self::CreateBranch => "create_branch",
      Self::DeleteBranch => "delete_branch",
      Self::AttachShard => "attach_shard",
      Self::DetachShard => "detach_shard",
      Self::Shutdown => "shutdown",
    }
  }
}

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

#[derive(Debug)]
struct OpMetrics {
  total: AtomicU64,
  error: AtomicU64,
  latency: Histogram,
}

impl OpMetrics {
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
pub struct ShardMetrics {
  shard_id: u32,
  op_metrics: [OpMetrics; OP_NUM],
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
  cache_bytes: AtomicU64,
}

impl ShardMetrics {
  pub fn new(shard_id: u32) -> Self {
    Self {
      shard_id,
      op_metrics: from_fn(|_| OpMetrics::new()),
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
    }
  }

  pub fn shard_id(&self) -> u32 {
    self.shard_id
  }

  /// shard 主循环在请求完成后调用。
  pub fn observe(&self, op: Op, elapsed: Duration, is_error: bool) {
    let metrics = &self.op_metrics[op as usize];
    metrics.total.fetch_add(1, Ordering::Relaxed);
    if is_error {
      metrics.error.fetch_add(1, Ordering::Relaxed);
    }
    metrics
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

  pub fn set_cache_bytes(&self, bytes: u64) {
    self.cache_bytes.store(bytes, Ordering::Relaxed);
  }

  pub fn snapshot(&self) -> ShardSnapshot {
    ShardSnapshot {
      shard_id: self.shard_id,
      op_li: Op::ALL
        .iter()
        .map(|op| self.op_metrics[*op as usize].snapshot(*op))
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
      cache_bytes: self.cache_bytes.load(Ordering::Relaxed),
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
  pub cache_bytes: u64,
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
  metrics: Arc<ShardMetrics>,
}

impl<I> MeteredIo<I> {
  pub fn new(inner: I, metrics: Arc<ShardMetrics>) -> Self {
    Self { inner, metrics }
  }

  pub fn inner(&self) -> &I {
    &self.inner
  }

  pub fn metrics(&self) -> &Arc<ShardMetrics> {
    &self.metrics
  }
}

impl<I: PageIo> PageIo for MeteredIo<I> {
  fn set_file_identity(&self, branch: &str, term: u64) {
    self.inner.set_file_identity(branch, term);
  }

  fn preload_paths(&self, tbl: &TableIdent) -> usize {
    self.inner.preload_paths(tbl)
  }

  fn invalidate_path_cache(&self) {
    self.inner.invalidate_path_cache();
  }

  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    self.inner.read_page(tbl, file_page_id, dst).await?;
    self.metrics.add_page_read(dst.len() as u64);
    Ok(())
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    self.inner.write_page(tbl, file_page_id, src).await?;
    self.metrics.add_page_write(src.len() as u64);
    Ok(())
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.inner.sync_data(tbl).await
  }
}

impl<I: ManifestIo> ManifestIo for MeteredIo<I> {
  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.inner.append_manifest(tbl, record).await?;
    self.metrics.add_manifest_write(record.len() as u64);
    Ok(())
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.inner.switch_manifest(tbl, record).await?;
    self.metrics.add_manifest_write(record.len() as u64);
    Ok(())
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    let result = self.inner.read_manifest(tbl).await?;
    if let Some(bytes) = &result {
      self.metrics.add_manifest_read(bytes.len() as u64);
    }
    Ok(result)
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
    self.inner.drop_manifest(tbl).await
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self.inner.write_manifest_file(tbl, name, record).await?;
    self.metrics.add_manifest_write(record.len() as u64);
    Ok(())
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    self.inner.list_manifest_file_name_li(tbl).await
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    let result = self.inner.read_manifest_file(tbl, name).await?;
    if let Some(bytes) = &result {
      self.metrics.add_manifest_read(bytes.len() as u64);
    }
    Ok(result)
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.inner.remove_manifest_file(tbl, name).await
  }
}

impl<I: FileIo> FileIo for MeteredIo<I> {
  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.inner.list_file_id_li(tbl).await
  }

  async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>> {
    let result = self.inner.read_file(tbl, file_id).await?;
    if let Some(bytes) = &result {
      self.metrics.add_file_read(bytes.len() as u64);
    }
    Ok(result)
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()> {
    self.inner.write_file(tbl, file_id, bytes).await?;
    self.metrics.add_file_write(bytes.len() as u64);
    Ok(())
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>> {
    self.inner.file_size(tbl, file_id).await
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.inner.remove_file(tbl, file_id).await?;
    self.metrics.add_file_remove();
    Ok(())
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.inner.list_segment_file_id_li(tbl).await
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.inner.remove_segment_file(tbl, file_id).await?;
    self.metrics.add_file_remove();
    Ok(())
  }
}

impl<I: SegmentIo> SegmentIo for MeteredIo<I> {
  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    self.inner.read_segment(tbl, segment_fp_id, dst).await?;
    self.metrics.add_segment_read(dst.len() as u64);
    Ok(())
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> Result<()> {
    self.inner.write_segment(tbl, segment_fp_id, src).await?;
    self.metrics.add_segment_write(src.len() as u64);
    Ok(())
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
    self
      .inner
      .read_segment_fixed(tbl, segment_fp_id, buf, offset, len)
      .await?;
    self.metrics.add_segment_read(len as u64);
    Ok(())
  }

  async fn write_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    self
      .inner
      .write_segment_fixed(tbl, segment_fp_id, buf, offset, len)
      .await?;
    self.metrics.add_segment_write(len as u64);
    Ok(())
  }
}

impl<I: TermIo> TermIo for MeteredIo<I> {
  async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>> {
    self.inner.read_term(branch, shard_id).await
  }

  async fn upsert_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<u64> {
    self.inner.upsert_term(branch, shard_id, term).await
  }
}
