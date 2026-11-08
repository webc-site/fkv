//! 写路径小分配计数回归测试。
//!
//! 计数分配器统计 ≤ [`SMALL_ALLOC`] 字节的堆分配次数：index 路由/重建按子项
//! 逐个 `Vec<u8>` 分配键时，单次覆盖写会产生数百次小分配；子项键改为页级扁平
//! 键区后只剩容器、路径快照等常数项。计数窗口内的固定树与批次必须完全一致，
//! 避免数据页分裂 / index 页重建引入额外噪声。

use std::{
  alloc::{GlobalAlloc, Layout, System},
  sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
  },
};

use fkv_btree::BatchEntry;
use fkv_io::PageIo;
use fkv_io_mem::MemStore;
use fkv_mapping::{AllocMode, PageMapper};
use fkv_page::{
  CompressionType, DataPageBuilder, EntrySpec, IndexPageBuilder, set_checksum, set_next_page_id,
  set_prev_page_id,
};
use fkv_task_write::{PageLayout, WriteArg, write_batch};
use fkv_types::{NULL_PAGE, PageId, TableIdent};

const PAGE_SIZE: usize = 4096;
/// 小分配阈值：index 子项键 / 路径快照 / 变更容器都落在此范围内。
const SMALL_ALLOC: usize = 64;
/// 固定树规模：对齐 bench 默认口径（rows 20 K），此时 root 即单个 leaf index 页。
const ROWS: u32 = 20_000;
/// 计数器是进程级全局量：同进程并行测试时串行化测量窗口。
static TEST_LOCK: Mutex<()> = Mutex::new(());
/// ≤ `SMALL_ALLOC` 字节的堆分配次数。
static SMALL_CNT: AtomicUsize = AtomicUsize::new(0);
/// 全部堆分配次数（含 `realloc` 增长）。
static ALLOC_CNT: AtomicUsize = AtomicUsize::new(0);
/// 整页统计的目标大小；`0` 表示不统计整页分配（见 [`write_batch_reuses_page_buffers`]）。
static TARGET_SIZE: AtomicUsize = AtomicUsize::new(0);
/// 与目标大小相同的分配次数。
static PAGE_CNT: AtomicUsize = AtomicUsize::new(0);
/// 其中 `alloc_zeroed` 的次数：即 `vec![0u8; page_size]` 这类零初始化整页缓冲。
static PAGE_ZERO_CNT: AtomicUsize = AtomicUsize::new(0);

struct CountingAlloc;

impl CountingAlloc {
  fn record(size: usize, zeroed: bool) {
    ALLOC_CNT.fetch_add(1, Ordering::Relaxed);
    if size <= SMALL_ALLOC {
      SMALL_CNT.fetch_add(1, Ordering::Relaxed);
    }
    if TARGET_SIZE.load(Ordering::Relaxed) == size {
      PAGE_CNT.fetch_add(1, Ordering::Relaxed);
      if zeroed {
        PAGE_ZERO_CNT.fetch_add(1, Ordering::Relaxed);
      }
    }
  }
}

unsafe impl GlobalAlloc for CountingAlloc {
  unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    Self::record(layout.size(), false);
    unsafe { System.alloc(layout) }
  }

  unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
    Self::record(layout.size(), true);
    unsafe { System.alloc_zeroed(layout) }
  }

  unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    Self::record(new_size, false);
    unsafe { System.realloc(ptr, layout, new_size) }
  }

  unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
    unsafe { System.dealloc(ptr, layout) }
  }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

fn key(idx: u32) -> Vec<u8> {
  format!("k{idx:08}").into_bytes()
}

fn upsert(idx: u32, ts: u64) -> BatchEntry {
  BatchEntry::upsert(key(idx), vec![b'v'; 128], ts)
}

async fn write<'a>(
  store: &'a MemStore,
  tbl: &'a TableIdent,
  mapper: &'a mut PageMapper,
  root: PageId,
  batch_li: &'a [BatchEntry],
  layout: PageLayout<'a>,
) -> fkv_error::Result<PageId> {
  write_batch(
    store,
    WriteArg {
      tbl,
      root_id: root,
      mapper,
      batch_li,
      layout,
      segment_mapper: None,
      segment_size: 0,
      ttl_delete_sink: None,
    },
  )
  .await
}

/// 预热后一次 100 条、跨 1 个 leaf index 页的覆盖写：小分配应只剩常数项。
#[test]
fn write_batch_small_allocations_are_constant() {
  let _guard = TEST_LOCK.lock().unwrap();
  pollster::block_on(async {
    let store = MemStore::new(PAGE_SIZE);
    let tbl = TableIdent::new("alloc", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let layout = PageLayout::new(PAGE_SIZE, 16, 16, None, 16);

    let batch_li: Vec<BatchEntry> = (0..ROWS).map(|idx| upsert(idx, 1)).collect();
    let root = write(&store, &tbl, &mut mapper, NULL_PAGE, &batch_li, layout)
      .await
      .unwrap();
    assert_ne!(root, NULL_PAGE);

    // 固定树：覆盖首 100 个 key（同长度 value，不触发 data/index 页分裂）。
    let update_li: Vec<BatchEntry> = (0..100).map(|idx| upsert(idx, 2)).collect();
    let root = write(&store, &tbl, &mut mapper, root, &update_li, layout)
      .await
      .unwrap();
    let update_li: Vec<BatchEntry> = (0..100).map(|idx| upsert(idx, 3)).collect();

    SMALL_CNT.store(0, Ordering::Relaxed);
    ALLOC_CNT.store(0, Ordering::Relaxed);
    let root = write(&store, &tbl, &mut mapper, root, &update_li, layout)
      .await
      .unwrap();
    let small = SMALL_CNT.load(Ordering::Relaxed);
    let total = ALLOC_CNT.load(Ordering::Relaxed);
    assert_ne!(root, NULL_PAGE);
    eprintln!("write_batch allocations: small={small}, total={total}");

    assert!(
      small <= 32,
      "一次覆盖写产生了 {small} 次 ≤{SMALL_ALLOC}B 小分配（总分配 {total} 次）"
    );
  });
}

/// 写路径整页缓冲复用回归测试。
///
/// 手工搭一棵两层 index 树（root 非叶 → 1 个 leaf index → data 页），本批只覆盖
/// 首个 data 页的 3 条 key：单写组、不分裂、父 index 子项集合不变，因此测量窗口
/// 内的输出只有 1 个 data 页。
///
/// 修复前（perf-08）：路由 `open` 1 次整页零初始化 + 2 次帧 clone（root / leaf
/// index），读旧 data 页再 1 次零初始化 → 4 次临时整页分配；修复后临时缓冲为 0。
/// 窗口内剩余 2 次整页分配是输出 data 页（`DataPageBuilder`）与
/// `MemStore::write_page` 的落盘拷贝，按验收口径不计。
#[test]
fn write_batch_reuses_page_buffers() {
  const PAGE: usize = 512;

  fn data_page(entry_li: &[(&[u8], &[u8])]) -> Vec<u8> {
    let mut builder = DataPageBuilder::new(PAGE, 16).unwrap();
    for (key, value) in entry_li {
      builder
        .add(EntrySpec {
          key,
          value,
          ts: 0,
          expire_ts: 0,
          overflow: false,
          compression: CompressionType::None,
          large_value: false,
        })
        .unwrap();
    }
    let mut page = builder.finish().unwrap();
    set_prev_page_id(&mut page, NULL_PAGE).unwrap();
    set_next_page_id(&mut page, NULL_PAGE).unwrap();
    set_checksum(&mut page).unwrap();
    page
  }

  fn index_page(leaf: bool, child_li: &[(&[u8], PageId)]) -> Vec<u8> {
    let mut builder = IndexPageBuilder::new(PAGE, 16, leaf).unwrap();
    for (key, page_id) in child_li {
      builder.add(key, *page_id).unwrap();
    }
    builder.finish().unwrap()
  }

  let _guard = TEST_LOCK.lock().unwrap();
  pollster::block_on(async {
    let store = MemStore::new(PAGE);
    let tbl = TableIdent::new("alloc_page", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let layout = PageLayout::new(PAGE, 16, 16, None, 16);

    // root(非叶) → l0 / l1 两个 leaf index；本批只落在 l0 下的 p0。
    let (p0, p1, l0, l1, root) = (
      mapper.allocate_page(),
      mapper.allocate_page(),
      mapper.allocate_page(),
      mapper.allocate_page(),
      mapper.allocate_page(),
    );
    for (file_page_id, page_id) in [p0, p1, l0, l1, root].into_iter().enumerate() {
      mapper.set_file_page(page_id, file_page_id as u64);
    }
    let value = vec![b'v'; 128];
    let (k0, k1, k2, k9) = (key(0), key(1), key(2), key(9));
    let page_li = [
      data_page(&[(&k0, &value), (&k1, &value), (&k2, &value)]),
      data_page(&[(&k9, &value)]),
      index_page(true, &[(b"", p0)]),
      index_page(true, &[(b"", p1)]),
      index_page(false, &[(b"", l0), (k9.as_slice(), l1)]),
    ];
    for (file_page_id, page) in page_li.iter().enumerate() {
      store
        .write_page(&tbl, file_page_id as u64, page)
        .await
        .unwrap();
    }

    // 预热：首次写从空池分配整页缓冲，批末全部归还线程本地池。
    let first: Vec<BatchEntry> = (0..3).map(|idx| upsert(idx, 1)).collect();
    let root = write(&store, &tbl, &mut mapper, root, &first, layout)
      .await
      .unwrap();
    assert_ne!(root, NULL_PAGE);
    let second: Vec<BatchEntry> = (0..3).map(|idx| upsert(idx, 2)).collect();

    TARGET_SIZE.store(PAGE, Ordering::Relaxed);
    PAGE_CNT.store(0, Ordering::Relaxed);
    PAGE_ZERO_CNT.store(0, Ordering::Relaxed);
    let root = write(&store, &tbl, &mut mapper, root, &second, layout)
      .await
      .unwrap();
    let page_cnt = PAGE_CNT.load(Ordering::Relaxed);
    let zero_cnt = PAGE_ZERO_CNT.load(Ordering::Relaxed);
    TARGET_SIZE.store(0, Ordering::Relaxed);

    assert_ne!(root, NULL_PAGE);
    eprintln!("write_batch page allocations: total={page_cnt}, zeroed={zero_cnt}");
    assert_eq!(
      zero_cnt, 0,
      "预热后写路径仍分配了 {zero_cnt} 次零初始化整页缓冲（总整页分配 {page_cnt} 次）"
    );
    assert_eq!(
      page_cnt, 2,
      "除输出 data 页与落盘拷贝外不应再有整页分配，实际 {page_cnt} 次"
    );
  });
}
