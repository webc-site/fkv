//! io-22：慢读探针按请求归集。
//!
//! shard 的每条读都是独立 task，在同一线程上交错推进。用可注入让出点的 IO
//! 让两条 `get_at` 在页读处交替挂起，验证报告不会串段；再逐个入口验证
//! scan / floor / get_large 也会产生自己的报告；最后验证默认关闭时读路径
//! 不分配采样槽、不产生报告。

use std::{
  cell::Cell,
  future::{Future, poll_fn},
  pin::{Pin, pin},
  rc::Rc,
  task::{Context, Poll},
  time::Duration,
};

use fkv_btree::BatchEntry;
use fkv_error::Result;
use fkv_io::{ManifestIo, PageIo, SegmentIo};
use fkv_io_mem::MemStore;
use fkv_store::Store;
use fkv_types::{FilePageId, TableIdent, slow_read, slow_read::Report};

const PAGE_SIZE: usize = 512;
const RESTART_INTERVAL: u16 = 16;

fn tbl() -> TableIdent {
  TableIdent::new("slow_read_probe", 1).unwrap()
}

fn has_phase(report: &Report, name: &str) -> bool {
  report.phase_li.iter().any(|(phase, _)| *phase == name)
}

/// 每次页读前让出一次，制造两条读 future 在同一线程上的交错推进；
/// 前两次页读各打一个全局序号标记，用来断言段归属。
#[derive(Clone)]
struct ProbeIo {
  inner: Rc<MemStore>,
  read_seq: Rc<Cell<usize>>,
}

impl ProbeIo {
  fn new() -> Self {
    Self {
      inner: Rc::new(MemStore::new(PAGE_SIZE)),
      read_seq: Rc::new(Cell::new(0)),
    }
  }

  fn reset_seq(&self) {
    self.read_seq.set(0);
  }
}

impl PageIo for ProbeIo {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    let seq = self.read_seq.get();
    self.read_seq.set(seq + 1);
    match seq {
      0 => slow_read::record("io_first", Duration::ZERO),
      1 => slow_read::record("io_second", Duration::ZERO),
      _ => {}
    }
    fkv_task_read::yield_now().await;
    self.inner.read_page(tbl, file_page_id, dst).await
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    self.inner.write_page(tbl, file_page_id, src).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.inner.sync_data(tbl).await
  }
}

impl ManifestIo for ProbeIo {
  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.inner.append_manifest(tbl, record).await
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.inner.switch_manifest(tbl, record).await
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    self.inner.read_manifest(tbl).await
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
    self.inner.drop_manifest(tbl).await
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self.inner.write_manifest_file(tbl, name, record).await
  }

  async fn append_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self.inner.append_manifest_file(tbl, name, record).await
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    self.inner.list_manifest_file_name_li(tbl).await
  }

  async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    self.inner.list_sidecar_file_name_li(tbl).await
  }

  async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.inner.remove_sidecar_file_name(tbl, name).await
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    self.inner.read_manifest_file(tbl, name).await
  }

  async fn seed_local_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
    bytes: &[u8],
  ) -> Result<()> {
    self.inner.seed_local_manifest_file(tbl, name, bytes).await
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.inner.remove_manifest_file(tbl, name).await
  }
}

impl SegmentIo for ProbeIo {
  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    self.inner.read_segment(tbl, segment_fp_id, dst).await
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> Result<()> {
    self.inner.write_segment(tbl, segment_fp_id, src).await
  }
}

/// 手动 poll 一次固定住的 future。
async fn poll_once<F: Future>(mut fut: Pin<&mut F>) -> Poll<F::Output> {
  poll_fn(|cx: &mut Context<'_>| Poll::Ready(fut.as_mut().poll(cx))).await
}

/// 反复手动 poll 直到完成；测试里的挂起点只有自唤醒的 `yield_now`。
async fn drive<F: Future>(mut fut: Pin<&mut F>) -> F::Output {
  loop {
    if let Poll::Ready(out) = poll_once(fut.as_mut()).await {
      return out;
    }
  }
}

#[test]
fn slow_read_probe_is_request_scoped() {
  pollster::block_on(async {
    // 本文件断言的是「页读在 IO 层如何分段 / 交错」：必须关闭页缓存，否则写批
    // 刚回填的页会让读请求直接命中、根本走不到 ProbeIo::read_page。
    let mut store =
      Store::new(ProbeIo::new(), PAGE_SIZE, RESTART_INTERVAL).with_page_cache_bytes(0);
    let tbl = tbl();
    // 第二条请求用**另一张表**：两表互不共享在飞 index / data 页，避免
    // l2-08 首访 single-flight 把两条请求的页读合并掉（合并后 B 就没有自己的
    // 页读段可标记了，本用例要验证的正是两条请求的段互不串）。
    let tbl_b = TableIdent::new("slow_read_probe", 2).unwrap();
    let batch = [
      BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
      BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
    ];
    store.put(&tbl, &batch).await.unwrap();
    store.put(&tbl_b, &batch).await.unwrap();

    // 1) 默认关闭：读路径不分配采样槽、不产生报告。
    slow_read::set_threshold_us(0);
    assert!(!slow_read::enabled());
    let _ = slow_read::take_reports();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert!(
      slow_read::take_reports().is_empty(),
      "探针关闭时不应产生报告"
    );
    assert_eq!(slow_read::slot_num(), 0, "探针关闭时不应分配采样槽");

    // 2) 开启后两条 get（分属两张表）交错完成：A 先挂起、B 启动、A 先完成。
    slow_read::set_threshold_us(1);
    assert!(slow_read::enabled());
    store.page_io().reset_seq();
    let _ = slow_read::take_reports();
    let mut a = pin!(store.get_at(&tbl, b"a", 1));
    let mut b = pin!(store.get_at(&tbl_b, b"b", 1));
    assert!(poll_once(a.as_mut()).await.is_pending(), "A 应停在页读");
    assert!(poll_once(b.as_mut()).await.is_pending(), "B 应停在页读");
    assert_eq!(
      drive(a.as_mut()).await.unwrap(),
      Some(b"A".to_vec()),
      "A 先完成"
    );
    assert_eq!(
      drive(b.as_mut()).await.unwrap(),
      Some(b"B".to_vec()),
      "B 后完成"
    );
    assert_eq!(slow_read::slot_num(), 0, "请求完成后槽位应释放");

    let report_li = slow_read::take_reports();
    assert_eq!(report_li.len(), 2, "两条请求各产生一条报告");
    let (first, second) = if has_phase(&report_li[0], "io_first") {
      (&report_li[0], &report_li[1])
    } else {
      (&report_li[1], &report_li[0])
    };
    assert!(has_phase(first, "io_first"), "A 的报告应只含自己的页读段");
    assert!(!has_phase(first, "io_second"), "A 的报告不能串入 B 的段");
    assert!(has_phase(second, "io_second"), "B 的报告应只含自己的页读段");
    assert!(!has_phase(second, "io_first"), "B 的报告不能串入 A 的段");

    // 3) scan / floor / get_large 各自产生一条属于自己的报告。
    let _ = slow_read::take_reports();
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 2);
    let report_li = slow_read::take_reports();
    assert_eq!(report_li.len(), 1, "scan 应产生一条报告");
    assert!(has_phase(&report_li[0], "data_io"), "scan 应有页读段");

    let _ = slow_read::take_reports();
    assert_eq!(store.floor(&tbl, b"b").await.unwrap().key.as_slice(), b"b");
    let report_li = slow_read::take_reports();
    assert_eq!(report_li.len(), 1, "floor 应产生一条报告");
    assert!(has_phase(&report_li[0], "data_io"), "floor 应有页读段");

    let _ = slow_read::take_reports();
    assert_eq!(
      store.get_large(&tbl, b"a").await.unwrap().unwrap().value,
      b"A".to_vec()
    );
    let report_li = slow_read::take_reports();
    assert_eq!(report_li.len(), 1, "get_large 应产生一条报告");
    assert!(has_phase(&report_li[0], "data_io"), "get_large 应有页读段");
    assert_eq!(slow_read::slot_num(), 0, "全部请求完成后槽位应释放");
  });
}

/// l2-11 验收：被 single-flight 合并的等待者记 `flight_wait` 段。
///
/// 两条同表同页并发 `get_at`：owner 读 root index 页与 data 页各一次，等待者
/// 两页都复用广播字节。等待者的报告必须含 `flight_wait`（等待时长独立成段，
/// IO 时间仍可归因），且不含 `index_io` / `data_io`（它自己没有读盘）；owner
/// 的报告保留自己的 `index_io` / `data_io`，不出现 `flight_wait`。
#[test]
fn merged_reader_reports_flight_wait() {
  pollster::block_on(async {
    // 本文件断言的是「页读在 IO 层如何分段 / 交错」：必须关闭页缓存，否则写批
    // 刚回填的页会让读请求直接命中、根本走不到 ProbeIo::read_page。
    let mut store =
      Store::new(ProbeIo::new(), PAGE_SIZE, RESTART_INTERVAL).with_page_cache_bytes(0);
    let tbl = tbl();
    let batch = [
      BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
      BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
    ];
    store.put(&tbl, &batch).await.unwrap();

    slow_read::set_threshold_us(1);
    let _ = slow_read::take_reports();
    store.page_io().reset_seq();

    // 手工步进，保证 owner / 等待者两页都真正交错：
    // owner 停在 index 页读 → 等待者挂上 index 首访 → owner 停在 data 页读 →
    // 等待者挂上 data 首访 → 各自完成。
    let mut owner = pin!(store.get_at(&tbl, b"a", 1));
    assert!(
      poll_once(owner.as_mut()).await.is_pending(),
      "owner 应停在 index 页读"
    );
    let mut waiter = pin!(store.get_at(&tbl, b"a", 1));
    assert!(
      poll_once(waiter.as_mut()).await.is_pending(),
      "等待者应挂在 index 页首访"
    );
    assert!(
      poll_once(owner.as_mut()).await.is_pending(),
      "owner 应停在 data 页读"
    );
    assert!(
      poll_once(waiter.as_mut()).await.is_pending(),
      "等待者应挂在 data 页首访"
    );
    assert_eq!(drive(owner.as_mut()).await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(drive(waiter.as_mut()).await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(
      store.page_io().read_seq.get(),
      2,
      "index / data 两页各只读一次，等待者不读盘"
    );

    let report_li = slow_read::take_reports();
    assert_eq!(report_li.len(), 2, "两条请求各产生一条报告");
    let owner_report = &report_li[0];
    assert!(
      has_phase(owner_report, "index_io"),
      "owner 应记 index 页读段"
    );
    assert!(has_phase(owner_report, "data_io"), "owner 应记 data 页读段");
    assert!(!has_phase(owner_report, "flight_wait"), "owner 没有等待");
    let waiter_report = &report_li[1];
    assert!(
      has_phase(waiter_report, "flight_wait"),
      "等待者必须记等待段: {waiter_report:?}"
    );
    assert!(
      !has_phase(waiter_report, "index_io") && !has_phase(waiter_report, "data_io"),
      "等待者没有页读段: {waiter_report:?}"
    );
  });
}
