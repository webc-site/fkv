//! `MeteredIo` 的计数语义：在飞深度、页 / manifest / 段 / 文件累计、
//! 失败不计数、定长段按整段计。

use std::{
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
  },
  time::Duration,
};

use compio::{runtime::spawn, time::sleep};
use fkv_error::{KvError, Result};
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_metrics::ShardMetric;
use fkv_shard::metered::MeteredIo;
use fkv_testkit::TempRoot;
use fkv_types::{FilePageId, TableIdent};

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
  let root = TempRoot::new("fkv_metrics_segment");
  let metrics = Arc::new(ShardMetric::new(0));
  let io = MeteredIo::new(
    CompioStore::new(root.path().to_path_buf(), 512, 2).with_segment_config(4096, 2),
    metrics.clone(),
  );
  let tbl = TableIdent::new("demo", 1).unwrap();
  io.write_segment(&tbl, 0, b"seg").await.unwrap();
  assert_eq!(metrics.snapshot().segment_writes, 1);
  assert_eq!(metrics.snapshot().bytes_written, 4096);
}
