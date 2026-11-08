//! 整页缓冲复用回归测试。
//!
//! 计数分配器只在本测试进程内生效：按目标页大小精确统计整页 `Vec<u8>` 的分配
//! （含 `alloc_zeroed` 与增长到页大小的 `realloc`）。扫描结果自身的 key / value /
//! 结果 Vec 容量不按页大小分配，因此不会混入计数。

use std::{
  alloc::{GlobalAlloc, Layout, System},
  panic::resume_unwind,
  sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
  },
  thread::spawn,
};

use fkv_io::PageIo;
use fkv_io_mem::MemStore;
use fkv_mapping::{AllocMode, MappingSnapshot, PageMapper, SnapshotRegistry, SnapshotSource};
use fkv_page::{
  CompressionType, DataPageBuilder, EntrySpec, IndexPageBuilder, set_next_page_id, set_prev_page_id,
};
use fkv_task_scan::{PageOption, ScanArg, ScanOption, ScanTree, scan_page_with_budget};
use fkv_types::{NULL_PAGE, PageId, TableIdent};

const PAGE_SIZE: usize = 512;

/// 目标页大小；`0` 表示只统计总分配次数、不统计整页分配。
static TARGET_SIZE: AtomicUsize = AtomicUsize::new(0);
/// 与目标页大小相同的分配次数。
static PAGE_ALLOC_CNT: AtomicUsize = AtomicUsize::new(0);
/// 全部堆分配次数（含 `realloc` 增长）。
static ALLOC_CNT: AtomicUsize = AtomicUsize::new(0);
/// 计数器是进程级全局量：`cargo test` 同进程并行时串行化测量窗口
/// （nextest 每个测试独立进程，无竞争）。
static TEST_LOCK: Mutex<()> = Mutex::new(());

struct CountingAlloc;

impl CountingAlloc {
  fn record(size: usize) {
    ALLOC_CNT.fetch_add(1, Ordering::Relaxed);
    let target = TARGET_SIZE.load(Ordering::Relaxed);
    if target != 0 && size == target {
      PAGE_ALLOC_CNT.fetch_add(1, Ordering::Relaxed);
    }
  }
}

unsafe impl GlobalAlloc for CountingAlloc {
  unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    Self::record(layout.size());
    unsafe { System.alloc(layout) }
  }

  unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
    Self::record(layout.size());
    unsafe { System.alloc_zeroed(layout) }
  }

  unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    Self::record(new_size);
    unsafe { System.realloc(ptr, layout, new_size) }
  }

  unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
    unsafe { System.dealloc(ptr, layout) }
  }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

/// 测试用快照：登记表只存弱引用，临时 registry 丢弃后快照仍有效。
fn snap(tbl: &TableIdent, mapper: &PageMapper, root: PageId) -> MappingSnapshot {
  SnapshotRegistry::new().open(
    tbl,
    SnapshotSource {
      data: mapper,
      segment: None,
      root_id: root,
      ttl_root_id: NULL_PAGE,
    },
  )
}

fn data_page(entry_li: &[(&str, &str)]) -> Vec<u8> {
  let mut builder = DataPageBuilder::new(PAGE_SIZE, 16).unwrap();
  for (key, value) in entry_li {
    builder
      .add(EntrySpec {
        key: key.as_bytes(),
        value: value.as_bytes(),
        ts: 0,
        expire_ts: 0,
        overflow: false,
        compression: CompressionType::None,
        large_value: false,
      })
      .unwrap();
  }
  builder.finish().unwrap()
}

fn index_page(leftmost: PageId) -> Vec<u8> {
  let mut builder = IndexPageBuilder::new(PAGE_SIZE, 16, true).unwrap();
  builder.add(b"", leftmost).unwrap();
  builder.finish().unwrap()
}

fn scan_arg<'a>(tbl: &'a TableIdent, snapshot: &'a MappingSnapshot) -> ScanArg<'a> {
  ScanArg {
    tbl,
    snapshot,
    tree: ScanTree::Data,
    begin: b"",
    end: b"",
    page_size: PAGE_SIZE,
    max_entries: 0,
    dict: None,
    ttl_filter_now_ms: None,
  }
}

/// 预热线程本地池后，单次 scan 不应再分配整页缓冲（修复前为 2 次）。
#[test]
fn scan_reuses_page_buffers() {
  let _guard = TEST_LOCK.lock().unwrap();
  pollster::block_on(async {
    let store = MemStore::new(PAGE_SIZE);
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let p0 = mapper.allocate_page();
    let p1 = mapper.allocate_page();
    let root = mapper.allocate_page();
    mapper.set_file_page(p0, 0);
    mapper.set_file_page(p1, 1);
    mapper.set_file_page(root, 2);

    let mut page0 = data_page(&[("a", "A"), ("b", "B")]);
    set_next_page_id(&mut page0, p1).unwrap();
    let mut page1 = data_page(&[("c", "C"), ("d", "D")]);
    set_prev_page_id(&mut page1, p0).unwrap();
    set_next_page_id(&mut page1, NULL_PAGE).unwrap();
    store.write_page(&tbl, 0, &page0).await.unwrap();
    store.write_page(&tbl, 1, &page1).await.unwrap();
    store.write_page(&tbl, 2, &index_page(p0)).await.unwrap();

    let snapshot = snap(&tbl, &mapper, root);
    let args = || scan_arg(&tbl, &snapshot);
    // 预热：首次请求从空池分配两块整页，请求结束归还池；第二次起应全部命中。
    for _ in 0..2 {
      let page = scan_page_with_budget(
        &store,
        args(),
        ScanOption::default(),
        PageOption::default(),
        0,
      )
      .await
      .unwrap();
      assert_eq!(page.entry_li.len(), 4);
      assert!(!page.has_remaining);
    }

    TARGET_SIZE.store(PAGE_SIZE, Ordering::Relaxed);
    PAGE_ALLOC_CNT.store(0, Ordering::Relaxed);
    ALLOC_CNT.store(0, Ordering::Relaxed);
    let page = scan_page_with_budget(
      &store,
      args(),
      ScanOption::default(),
      PageOption::default(),
      0,
    )
    .await
    .unwrap();
    let page_alloc_cnt = PAGE_ALLOC_CNT.load(Ordering::Relaxed);
    let alloc_cnt = ALLOC_CNT.load(Ordering::Relaxed);
    TARGET_SIZE.store(0, Ordering::Relaxed);

    assert_eq!(page.entry_li.len(), 4);
    eprintln!("scan allocations: total={alloc_cnt}, page_sized={page_alloc_cnt}");
    assert_eq!(
      page_alloc_cnt, 0,
      "预热后 scan 仍分配了 {page_alloc_cnt} 次整页缓冲（总分配 {alloc_cnt} 次）"
    );
  });
}

/// 链尾页不创建 `spare`：新建线程的 TLS 池为空，单页扫描只应分配 1 块整页。
#[test]
fn scan_single_page_skips_spare_buffer() {
  let _guard = TEST_LOCK.lock().unwrap();
  spawn(|| {
    pollster::block_on(async {
      let store = MemStore::new(PAGE_SIZE);
      let tbl = TableIdent::new("demo", 1).unwrap();
      let mut mapper = PageMapper::new(AllocMode::Append, 0);
      let p0 = mapper.allocate_page();
      let root = mapper.allocate_page();
      mapper.set_file_page(p0, 0);
      mapper.set_file_page(root, 1);

      let mut page0 = data_page(&[("a", "A"), ("b", "B")]);
      set_next_page_id(&mut page0, NULL_PAGE).unwrap();
      store.write_page(&tbl, 0, &page0).await.unwrap();
      store.write_page(&tbl, 1, &index_page(p0)).await.unwrap();

      let snapshot = snap(&tbl, &mapper, root);
      TARGET_SIZE.store(PAGE_SIZE, Ordering::Relaxed);
      PAGE_ALLOC_CNT.store(0, Ordering::Relaxed);
      let page = scan_page_with_budget(
        &store,
        scan_arg(&tbl, &snapshot),
        ScanOption::default(),
        PageOption::default(),
        0,
      )
      .await
      .unwrap();
      let page_alloc_cnt = PAGE_ALLOC_CNT.load(Ordering::Relaxed);
      TARGET_SIZE.store(0, Ordering::Relaxed);

      assert_eq!(page.entry_li.len(), 2);
      assert_eq!(
        page_alloc_cnt, 1,
        "单页扫描只需首页缓冲，实际分配 {page_alloc_cnt} 次整页"
      );
    });
  })
  .join()
  .unwrap_or_else(|payload| resume_unwind(payload));
}
