//! perf-03 回归：单候选页读的候选路径解析必须零堆分配。
//!
//! 计数方式：compio 每次提交 IO 会为 op 做固定分配（`ErasedKey::new` 内
//! `Box<Inner<RawOp>>` 等），驱动还会周期性分配 32 KiB 事件/缓冲，这些都不
//! 在本层控制范围内。因此本测试不直接断言「0 次分配」，而是：
//!
//! 1. 连续采样多次 `read_page`，取每次分配次数的最小值（噪声只会额外增加
//!    分配，最小值逼近本层真实开销）；
//! 2. 用同参数、同样次数的裸 `read_at` 作为基线，要求两者最小值相等，
//!    即本层（路径解析 / fd 复用 / 读缓冲池）额外分配为 0。
//!
//! 改前每次页读是基线 + 2（`Vec<PathBuf>` + `PathBuf::clone`），本断言会失败。
//! 多候选（同 file id 存在旧 term 变体）保留分配：回退链需要一份有序候选
//! 列表；测试只要求 primary 命中时不多提交 pread。

use std::{
  alloc::{GlobalAlloc, Layout, System},
  cell::Cell,
  path::Path,
};

use compio::{buf::BufResult, fs::File, io::AsyncReadAt};
use fkv_io::PageIo;
use fkv_io_compio::CompioStore;
use fkv_page::{PageType, set_checksum, set_page_type};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
/// 每个 data 文件 `1 << 2` 页。
const PAGES_PER_FILE_SHIFT: u8 = 2;
/// 采样次数：最小值只需一次无噪声样本。
const SAMPLES: usize = 32;

thread_local! {
  /// 本线程已记录的堆分配次数。
  static ALLOC_CNT: Cell<usize> = const { Cell::new(0) };
  /// 是否处于测量窗口；关闭时分配器只透传。
  static COUNTING: Cell<bool> = const { Cell::new(false) };
}

/// 进程级计数分配器，按线程统计（`#[compio::test]` 的 future 与 `block_on`
/// 在同一线程执行；其它测试线程的分配不会混入）。
struct CountingAlloc;

impl CountingAlloc {
  fn record() {
    if COUNTING.try_with(Cell::get).unwrap_or(false) {
      ALLOC_CNT.with(|cnt| cnt.set(cnt.get() + 1));
    }
  }
}

unsafe impl GlobalAlloc for CountingAlloc {
  unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    Self::record();
    unsafe { System.alloc(layout) }
  }

  unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
    Self::record();
    unsafe { System.alloc_zeroed(layout) }
  }

  unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    Self::record();
    unsafe { System.realloc(ptr, layout, new_size) }
  }

  unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
    unsafe { System.dealloc(ptr, layout) }
  }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

fn measure_begin() {
  ALLOC_CNT.with(|cnt| cnt.set(0));
  COUNTING.with(|counting| counting.set(true));
}

fn measure_end() -> usize {
  COUNTING.with(|counting| counting.set(false));
  ALLOC_CNT.with(Cell::get)
}

/// checksum 合法的 data page（读路径默认校验 checksum）。
fn data_page(fill: u8) -> Vec<u8> {
  let mut page = vec![0u8; PAGE_SIZE];
  set_page_type(&mut page, PageType::Data).unwrap();
  page[100] = fill;
  set_checksum(&mut page).unwrap();
  page
}

/// 同一文件上裸 `read_at` 的逐次分配数：本层「额外分配为 0」的基线。
async fn raw_read_alloc_seq(path: &Path, samples: usize) -> Vec<usize> {
  let file = File::open(path).await.unwrap();
  let mut buf = vec![0u8; PAGE_SIZE];
  // 预热：compio 的 op / driver 侧一次性初始化不计入稳态基线。
  for _ in 0..2 {
    let BufResult(result, returned) = file.read_at(buf, 0).await;
    result.unwrap();
    buf = returned;
  }
  let mut seq = Vec::with_capacity(samples);
  for _ in 0..samples {
    measure_begin();
    let BufResult(result, returned) = file.read_at(buf, 0).await;
    result.unwrap();
    seq.push(measure_end());
    buf = returned;
  }
  seq
}

/// 单候选（只有当前 identity 的 data 文件）热读：本层额外分配必须为 0。
#[compio::test]
async fn single_candidate_read_page_adds_no_allocation() {
  let root = TempRoot::new("read_alloc_single");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, PAGES_PER_FILE_SHIFT);
  let expected = data_page(0x5A);
  store.write_page(&tbl, 0, &expected).await.unwrap();

  let mut dst = vec![0u8; PAGE_SIZE];
  // 预热 fd cache / 读缓冲池 / 目录索引；首次页读允许分配。
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, expected);

  let fd_open_num = store.fd_open_num();
  let read_at_num = store.read_at_num();
  let mut read_page_alloc_li = Vec::with_capacity(SAMPLES);
  for _ in 0..SAMPLES {
    measure_begin();
    store.read_page(&tbl, 0, &mut dst).await.unwrap();
    read_page_alloc_li.push(measure_end());
  }

  let raw_alloc_li = raw_read_alloc_seq(&root.join("demo.1").join("data_0_main_0"), SAMPLES).await;

  assert_eq!(dst, expected);
  assert_eq!(store.fd_open_num(), fd_open_num, "热路径不应重复 open");
  assert_eq!(
    store.read_at_num(),
    read_at_num + SAMPLES as u64,
    "每次页读只提交一次 pread"
  );
  let read_page_min = read_page_alloc_li.iter().copied().min().unwrap();
  let raw_min = raw_alloc_li.iter().copied().min().unwrap();
  // 最小值过滤 compio 驱动侧周期性分配（噪声只会增加计数）。
  assert_eq!(
    read_page_min, raw_min,
    "单候选页读应零额外分配：read_page 最小 {read_page_min} 次，裸 read_at 基线最小 \
     {raw_min} 次\nread_page={read_page_alloc_li:?}\nraw={raw_alloc_li:?}"
  );
}

/// 多候选（同 file id 有旧 term 变体）：回退链需要候选列表，允许分配，
/// 但 primary 命中语义与旧实现一致，且只提交一次 pread。
#[compio::test]
async fn multi_candidate_read_page_keeps_fallback_order_and_allocates() {
  let root = TempRoot::new("read_alloc_multi");
  let tbl = TableIdent::new("demo", 1).unwrap();

  // 旧 term 文件：identity main/0。
  let old_store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, PAGES_PER_FILE_SHIFT);
  old_store
    .write_page(&tbl, 0, &data_page(0xA0))
    .await
    .unwrap();

  // 当前 term 文件：identity main/1；写入后索引里只有 primary。
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, PAGES_PER_FILE_SHIFT);
  store.set_file_identity("main", 1);
  let expected = data_page(0xB0);
  store.write_page(&tbl, 0, &expected).await.unwrap();

  let mut dst = vec![0u8; PAGE_SIZE];
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, expected);

  // 单候选热路径（索引里尚无 data_other）。
  let mut single_alloc_li = Vec::with_capacity(SAMPLES);
  for _ in 0..SAMPLES {
    measure_begin();
    store.read_page(&tbl, 0, &mut dst).await.unwrap();
    single_alloc_li.push(measure_end());
  }

  // 扫描目录：primary = main/1，旧文件进 data_other，候选变为 2 个。
  store.preload(&tbl);
  let read_at_num = store.read_at_num();
  let mut multi_alloc_li = Vec::with_capacity(SAMPLES);
  for _ in 0..SAMPLES {
    measure_begin();
    store.read_page(&tbl, 0, &mut dst).await.unwrap();
    multi_alloc_li.push(measure_end());
  }

  assert_eq!(dst, expected, "primary 命中，不应回退旧 term");
  assert_eq!(
    store.read_at_num(),
    read_at_num + SAMPLES as u64,
    "primary 命中只提交一次 pread"
  );
  let single_min = single_alloc_li.iter().copied().min().unwrap();
  let multi_min = multi_alloc_li.iter().copied().min().unwrap();
  // 多候选保留分配（Vec + PathBuf clone + 变体排序），这是文档化行为。
  assert!(
    multi_min > single_min,
    "多候选路径按设计保留分配：单候选 {single_min} 次，多候选 {multi_min} 次"
  );
}
