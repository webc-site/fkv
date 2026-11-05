use std::{
  env::temp_dir,
  fs::remove_dir_all,
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
  },
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use compio::{runtime::spawn, time::sleep};
use fkv_error::{KvError, Result};
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_metrics::{
  BUCKET_NUM, BUCKET_UPPER_US, HistogramSnapshot, METRIC_REQUEST_TYPE_LI, MeteredIo,
  MetricsSnapshot, Op, ReadBatchSnapshot, ShardMetric,
};
use fkv_types::{FilePageId, TableIdent};

#[test]
fn histogram_buckets_min_max_and_errors() {
  let metrics = ShardMetric::new(3);
  metrics.observe(Op::Get, Duration::from_micros(49), false);
  metrics.observe(Op::Get, Duration::from_micros(50), false);
  metrics.observe(Op::Get, Duration::from_micros(51), true);
  metrics.observe(Op::Get, Duration::from_micros(10_000), false);
  metrics.observe(Op::Get, Duration::from_micros(6_000_000), false);

  let snapshot = metrics.snapshot();
  assert_eq!(snapshot.shard_id, 3);
  let get = snapshot.op(Op::Get);
  assert_eq!(get.total, 5);
  assert_eq!(get.error, 1);
  assert_eq!(get.latency.count, 5);
  assert_eq!(get.latency.bucket_count[0], 2); // 49us, 50us
  assert_eq!(get.latency.bucket_count[1], 1); // 51us
  assert_eq!(get.latency.bucket_count[7], 1); // 10_000us
  assert_eq!(get.latency.bucket_count[BUCKET_NUM - 1], 1); // +Inf
  assert_eq!(get.latency.min_ns, 49_000);
  assert_eq!(get.latency.max_ns, 6_000_000_000);
  assert_eq!(
    get.latency.sum_ns,
    49_000 + 50_000 + 51_000 + 10_000_000 + 6_000_000_000
  );
  assert_eq!(get.latency.avg_ns(), get.latency.sum_ns / 5);
  assert_eq!(HistogramSnapshot::bucket_upper_us(0), Some(50));
  assert_eq!(HistogramSnapshot::bucket_upper_us(BUCKET_NUM - 1), None);
  assert_eq!(BUCKET_UPPER_US[BUCKET_NUM - 2], 5_000_000);
}

#[test]
fn metrics_aggregate_across_shards() {
  let first = ShardMetric::new(0);
  let second = ShardMetric::new(1);
  first.observe(Op::Put, Duration::from_millis(1), false);
  first.observe(Op::Get, Duration::from_millis(1), false);
  second.observe(Op::Put, Duration::from_millis(1), true);

  let snapshot = MetricsSnapshot {
    shard_li: vec![first.snapshot(), second.snapshot()],
  };
  assert_eq!(snapshot.shard_li.len(), 2);
  assert_eq!(snapshot.total_requests(Op::Put), 2);
  assert_eq!(snapshot.total_errors(Op::Put), 1);
  assert_eq!(snapshot.total_requests(Op::Get), 1);
}

/// 名字不同的映射与 Gc/Drop 的拆分逐条钉死。
#[test]
fn op_to_request_type_mapping() {
  assert_eq!(Op::Put.request_type_li(), &["batch_write"]);
  assert_eq!(Op::Get.request_type_li(), &["read"]);
  assert_eq!(Op::CreateArchive.request_type_li(), &["archive"]);
  assert_eq!(Op::FileGc.request_type_li(), &["file_gc"]);
  assert_eq!(Op::LocalGc.request_type_li(), &["local_gc"]);
  assert_eq!(Op::Drop.request_type_li(), &["drop", "drop_table"]);
  assert_eq!(Op::ListObject.request_type_li(), &["list_object"]);
  assert_eq!(
    Op::ListStandbyPartition.request_type_li(),
    &["list_standby_partition"]
  );
  // 控制面请求没有独立标签。
  assert!(Op::Prewarm.request_type_li().is_empty());
  assert!(Op::ListArchiveTags.request_type_li().is_empty());
  assert!(Op::Shutdown.request_type_li().is_empty());

  assert_eq!(Op::from_request_type("batch_write"), Some(Op::Put));
  assert_eq!(Op::from_request_type("read"), Some(Op::Get));
  assert_eq!(Op::from_request_type("archive"), Some(Op::CreateArchive));
  assert_eq!(Op::from_request_type("local_gc"), Some(Op::LocalGc));
  assert_eq!(Op::from_request_type("file_gc"), Some(Op::FileGc));
  assert_eq!(Op::from_request_type("drop_table"), Some(Op::Drop));
  assert_eq!(Op::from_request_type("list_object"), Some(Op::ListObject));
  assert_eq!(
    Op::from_request_type("list_standby_partition"),
    Some(Op::ListStandbyPartition)
  );
  assert_eq!(Op::from_request_type("global_archive"), None);
  assert_eq!(Op::from_request_type("global_list_archive_tags"), None);
}

/// 注册的 19 个标签全部能反查；每个标签只属于一个 `Op`。
#[test]
fn metric_tags_are_covered_once() {
  for tag in METRIC_REQUEST_TYPE_LI {
    assert!(Op::from_request_type(tag).is_some(), "tag {tag}");
  }
  for op in Op::ALL {
    for &tag in op.request_type_li() {
      assert!(METRIC_REQUEST_TYPE_LI.contains(&tag), "tag {tag}");
      assert_eq!(Op::from_request_type(tag), Some(op), "tag {tag}");
      let owners = Op::ALL
        .iter()
        .filter(|other| other.request_type_li().contains(&tag))
        .count();
      assert_eq!(owners, 1, "tag {tag} 被多个 Op 映射");
    }
  }
  assert_eq!(Op::ALL.len(), 21);
}

/// C++ 的 index buffer pool 与 local space 是两个 gauge，快照必须分开可读。
#[test]
fn cache_gauges_split_cloud_and_index() {
  let metrics = ShardMetric::new(0);
  metrics.set_cache_bytes(4096);
  metrics.set_index_cache_bytes(1024);

  let snapshot = metrics.snapshot();
  assert_eq!(snapshot.cloud_cache_bytes, 4096);
  assert_eq!(snapshot.index_cache_bytes, 1024);
  assert_eq!(snapshot.cache_bytes, 5120);
}

/// 读侧在飞 gauge：当前值随写入变化，峰值保持单调（eng-32c）。
#[test]
fn read_in_flight_gauge_tracks_current_and_peak() {
  let metrics = ShardMetric::new(0);
  metrics.set_read_in_flight(3);
  metrics.set_read_in_flight(7);
  metrics.set_read_in_flight(2);

  let snapshot = metrics.snapshot();
  assert_eq!(snapshot.read_in_flight, 2);
  assert_eq!(snapshot.read_in_flight_max, 7);
}

/// 读批大小走计数型桶，空批不记；`avg` / `max` 与桶边界可读（eng-32c）。
#[test]
fn read_batch_histogram_records_batch_sizes() {
  let metrics = ShardMetric::new(0);
  metrics.record_read_batch(1);
  metrics.record_read_batch(4);
  metrics.record_read_batch(32);
  metrics.record_read_batch(0);

  let batch = metrics.snapshot().read_batch_size;
  assert_eq!(batch.count, 3);
  assert_eq!(batch.sum, 37);
  assert_eq!(batch.max, 32);
  assert_eq!(batch.avg(), 12);
  // 桶上界 [1, 2, 4, 8, 16, 32] + Inf：1 / 4 / 32 各落一桶。
  assert_eq!(batch.bucket_count[0], 1);
  assert_eq!(batch.bucket_count[2], 1);
  assert_eq!(batch.bucket_count[5], 1);
  assert_eq!(ReadBatchSnapshot::bucket_upper(5), Some(32));
  assert_eq!(ReadBatchSnapshot::bucket_upper(6), None);
}

/// 每个读 IO 先停留一段固定时间，让多个 IO 同时处于在飞状态。
#[derive(Clone)]
struct GatePageIo {
  started: Arc<AtomicU64>,
}

impl GatePageIo {
  fn new() -> Self {
    Self {
      started: Arc::new(AtomicU64::new(0)),
    }
  }
}

impl PageIo for GatePageIo {
  async fn read_page(
    &self,
    _tbl: &TableIdent,
    _file_page_id: FilePageId,
    _dst: &mut [u8],
  ) -> Result<()> {
    self.started.fetch_add(1, Ordering::Relaxed);
    sleep(Duration::from_millis(200)).await;
    Ok(())
  }

  async fn write_page(
    &self,
    _tbl: &TableIdent,
    _file_page_id: FilePageId,
    _src: &[u8],
  ) -> Result<()> {
    Ok(())
  }

  async fn sync_data(&self, _tbl: &TableIdent) -> Result<()> {
    Ok(())
  }
}

/// `MeteredIo` 的在飞 IO 计数覆盖并发读：4 个 IO 同时卡住时 gauge 与峰值都为 4，
/// 全部完成后归零（eng-32c「每请求 IO 深度」的数据源）。
#[compio::test]
async fn metered_io_tracks_in_flight_depth_and_peak() {
  let metrics = Arc::new(ShardMetric::new(0));
  let io = MeteredIo::new(GatePageIo::new(), metrics.clone());
  let tbl = TableIdent::new("io-depth", 1).unwrap();

  let mut handle_li = Vec::new();
  for _ in 0..4 {
    let io = io.clone();
    let tbl = tbl.clone();
    handle_li.push(spawn(async move {
      let mut page = [0u8; 8];
      io.read_page(&tbl, 0, &mut page).await
    }));
  }
  // 等 4 个 IO 都进入在飞状态；每轮让出执行权给读任务。
  for _ in 0..1_000 {
    if io.inner().started.load(Ordering::Relaxed) >= 4 {
      break;
    }
    sleep(Duration::from_millis(1)).await;
  }
  assert_eq!(io.inner().started.load(Ordering::Relaxed), 4);
  let snapshot = metrics.snapshot();
  assert_eq!(snapshot.io_in_flight, 4);
  assert_eq!(snapshot.io_in_flight_max, 4);

  for handle in handle_li {
    handle.await.unwrap().unwrap();
  }
  assert_eq!(metrics.snapshot().io_in_flight, 0);
}

#[compio::test]
async fn metered_io_counts_pages_manifests_segments_and_files() {
  let metrics = Arc::new(ShardMetric::new(0));
  let io = MeteredIo::new(
    MemStore::new(8).with_segment_config(4096, 2),
    metrics.clone(),
  );
  let tbl = TableIdent::new("demo", 1).unwrap();

  io.write_page(&tbl, 0, b"12345678").await.unwrap();
  let mut page = [0u8; 8];
  io.read_page(&tbl, 0, &mut page).await.unwrap();
  io.append_manifest(&tbl, b"abc").await.unwrap();
  io.switch_manifest(&tbl, b"xy").await.unwrap();
  assert_eq!(io.read_manifest(&tbl).await.unwrap().unwrap(), b"xy");
  io.write_segment(&tbl, 0, b"seg").await.unwrap();
  let mut segment = [0u8; 3];
  io.read_segment(&tbl, 0, &mut segment).await.unwrap();
  io.write_file(&tbl, 0, b"12345678").await.unwrap();
  assert!(io.read_file(&tbl, 0).await.unwrap().is_some());
  io.remove_file(&tbl, 0).await.unwrap();
  io.sync_data(&tbl).await.unwrap();

  let snapshot = metrics.snapshot();
  assert_eq!(snapshot.page_reads, 1);
  assert_eq!(snapshot.page_writes, 1);
  assert_eq!(snapshot.manifest_reads, 1);
  assert_eq!(snapshot.manifest_writes, 2);
  assert_eq!(snapshot.segment_reads, 1);
  assert_eq!(snapshot.segment_writes, 1);
  assert_eq!(snapshot.file_reads, 1);
  assert_eq!(snapshot.file_writes, 1);
  assert_eq!(snapshot.file_removes, 1);
  assert_eq!(snapshot.bytes_written, 8 + 3 + 2 + 3 + 8);
  assert_eq!(snapshot.bytes_read, 8 + 2 + 3 + 8);
  // sync_data + append_manifest + switch_manifest 各一次文件 fsync；
  // switch_manifest 额外一次目录 fsync。
  assert_eq!(snapshot.fsync_num, 3);
  assert_eq!(snapshot.dir_fsync_num, 1);
}

/// 可注入失败的 `MemStore` 包装：读写命中失败开关时直接返回错误，不落内部存储。
struct FlakyPageIo {
  inner: MemStore,
  fail_read: Arc<AtomicBool>,
  fail_write: Arc<AtomicBool>,
}

impl FlakyPageIo {
  fn new(page_size: usize) -> (Self, Arc<AtomicBool>, Arc<AtomicBool>) {
    let fail_read = Arc::new(AtomicBool::new(false));
    let fail_write = Arc::new(AtomicBool::new(false));
    let io = Self {
      inner: MemStore::new(page_size),
      fail_read: Arc::clone(&fail_read),
      fail_write: Arc::clone(&fail_write),
    };
    (io, fail_read, fail_write)
  }
}

impl PageIo for FlakyPageIo {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    if self.fail_read.load(Ordering::Relaxed) {
      return Err(KvError::IoFail);
    }
    self.inner.read_page(tbl, file_page_id, dst).await
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    if self.fail_write.load(Ordering::Relaxed) {
      return Err(KvError::IoFail);
    }
    self.inner.write_page(tbl, file_page_id, src).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.inner.sync_data(tbl).await
  }
}

/// 回归：`MeteredIo` 只累计成功的 IO；读写失败时 bytes / 次数必须原样不动。
#[compio::test]
async fn metered_io_does_not_count_failed_read_or_write() {
  let metrics = Arc::new(ShardMetric::new(0));
  let (inner, fail_read, fail_write) = FlakyPageIo::new(8);
  let io = MeteredIo::new(inner, metrics.clone());
  let tbl = TableIdent::new("demo", 1).unwrap();
  io.write_page(&tbl, 0, b"12345678").await.unwrap();
  let mut page = [0u8; 8];
  io.read_page(&tbl, 0, &mut page).await.unwrap();

  let before = metrics.snapshot();
  assert_eq!(before.page_writes, 1);
  assert_eq!(before.page_reads, 1);
  assert_eq!(before.bytes_written, 8);
  assert_eq!(before.bytes_read, 8);

  fail_read.store(true, Ordering::Relaxed);
  assert_eq!(
    io.read_page(&tbl, 0, &mut page).await.unwrap_err(),
    KvError::IoFail
  );
  assert_eq!(metrics.snapshot(), before, "失败读不得累加 bytes / 次数");

  fail_read.store(false, Ordering::Relaxed);
  fail_write.store(true, Ordering::Relaxed);
  assert_eq!(
    io.write_page(&tbl, 1, b"12345678").await.unwrap_err(),
    KvError::IoFail
  );
  assert_eq!(metrics.snapshot(), before, "失败写不得累加 bytes / 次数");
}

/// io-28：定长段后端实际落盘 `segment_size`，指标按整段计而非 payload。
#[compio::test]
async fn segment_write_counts_padded_slot() {
  let root = temp_dir().join(format!(
    "fkv_metrics_segment_{}",
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos()
  ));
  let metrics = Arc::new(ShardMetric::new(0));
  let io = MeteredIo::new(
    CompioStore::new(root.clone(), 512, 2).with_segment_config(4096, 2),
    metrics.clone(),
  );
  let tbl = TableIdent::new("demo", 1).unwrap();
  io.write_segment(&tbl, 0, b"seg").await.unwrap();
  assert_eq!(metrics.snapshot().segment_writes, 1);
  assert_eq!(metrics.snapshot().bytes_written, 4096);
  remove_dir_all(root).unwrap();
}
