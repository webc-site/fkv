//! 批量提交深度：同一批的段 IO 必须同时在飞。
use std::{
  cell::Cell,
  future::Future,
  pin::Pin,
  task::{Context, Poll},
};

use fkv_error::Result;
use fkv_io::SegmentIo;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::{SegmentLayout, read_value, write_value};
use fkv_mapping::{AllocMode, PageMapper};
use fkv_testkit::TempRoot;
use fkv_types::{FilePageId, TableIdent};

/// 记录同时在飞的段 IO 数量。
#[derive(Debug)]
struct ProbeIo {
  inner: MemStore,
  in_flight: Cell<usize>,
  peak: Cell<usize>,
}

impl ProbeIo {
  fn new(inner: MemStore) -> Self {
    Self {
      inner,
      in_flight: Cell::new(0),
      peak: Cell::new(0),
    }
  }

  fn enter(&self) {
    let now = self.in_flight.get() + 1;
    self.in_flight.set(now);
    self.peak.set(self.peak.get().max(now));
  }

  fn leave(&self) {
    self.in_flight.set(self.in_flight.get() - 1);
  }
}

/// 让出一次，模拟 IO 尚未完成。
struct YieldOnce(bool);

impl Future for YieldOnce {
  type Output = ();

  fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
    if self.0 {
      return Poll::Ready(());
    }
    self.0 = true;
    cx.waker().wake_by_ref();
    Poll::Pending
  }
}

impl SegmentIo for ProbeIo {
  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    self.enter();
    YieldOnce(false).await;
    let result = self.inner.read_segment(tbl, segment_fp_id, dst).await;
    self.leave();
    result
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> Result<()> {
    self.enter();
    YieldOnce(false).await;
    let result = self.inner.write_segment(tbl, segment_fp_id, src).await;
    self.leave();
    result
  }
}

#[compio::test]
async fn segment_batch_keeps_batch_in_flight() {
  let io = ProbeIo::new(MemStore::new(512).with_segment_config(4096, 2));
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  // 64 段 = 8 个提交批（批大小 8），在飞数量始终被批大小钉住。
  let value: Vec<u8> = (0..4096 * 63 + 7)
    .map(|index| (index % 251) as u8)
    .collect();

  let written = write_value(&io, &tbl, &mut mapper, layout, &value, b"")
    .await
    .unwrap();
  assert_eq!(written.segment_id_li.len(), 64);
  assert_eq!(io.peak.get(), 8);

  io.peak.set(0);
  let read = read_value(&io, &tbl, &mapper, layout, &written.content)
    .await
    .unwrap();
  assert_eq!(read.value, value);
  assert_eq!(io.peak.get(), 8);

  // 单段 value 不夸大在飞数量。
  io.peak.set(0);
  let single = value[..4096].to_vec();
  write_value(&io, &tbl, &mut mapper, layout, &single, b"")
    .await
    .unwrap();
  assert_eq!(io.peak.get(), 1);
}

/// 16 MiB（默认 256 KiB × 64 段）真实后端往返：跨 8 个提交批，首段写入后只预分配一次。
#[compio::test]
async fn compio_16m_value_roundtrip_across_eight_batches() {
  let root = TempRoot::new("large_batch_16m");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(256 * 1024, 7).unwrap();
  let store = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(256 * 1024, 7);
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let value: Vec<u8> = (0..16 * 1024 * 1024)
    .map(|index| (index % 251) as u8)
    .collect();

  let written = write_value(&store, &tbl, &mut mapper, layout, &value, b"")
    .await
    .unwrap();
  assert_eq!(written.segment_id_li.len(), 64);
  // 每份 segment 文件 128 槽（32 MiB）：64 段只落 segment_0。
  assert!(root.join("demo.1").join("segment_0_main_0").exists());

  let read = read_value(&store, &tbl, &mapper, layout, &written.content)
    .await
    .unwrap();
  assert_eq!(read.value, value);
}
