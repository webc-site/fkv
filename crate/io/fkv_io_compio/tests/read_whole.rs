//! io-17：O_DIRECT 整文件读不再「整文件零初始化对齐缓冲 + 再分配输出 Vec」。
//!
//! 断言三层：
//! 1. 内容：direct / buffered 两种模式读同一文件逐字节一致，对齐与非对齐
//!    文件尾都正确（分块不能改变短读 / EOF 语义）；
//! 2. 峰值：计数分配器观测到的「≥ 半个文件大小」的分配只有输出 `Vec` 一次，
//!    窗口内总分配字节 < 2 × len（旧实现是对齐缓冲 + 输出 Vec，约 2 × len）；
//! 3. 缓冲：对齐池只新建一块 ≤ `WHOLE_READ_CHUNK` 的暂存缓冲，重复整文件读
//!    零新增分配；块读次数 = ceil(len / WHOLE_READ_CHUNK)。
//!
//! 这些断言与平台无关：`direct_io` 开关在非 Linux 上也把文件读引导到同一条
//! 「逻辑直读」路径（`apply_direct_flag` 只在 Linux 真正加 O_DIRECT），所以
//! macOS 同样覆盖。

use std::{
  alloc::{GlobalAlloc, Layout, System},
  cell::Cell,
  fs::{create_dir_all, write},
};

use fkv_io::FileIo;
use fkv_io_compio::{CompioStore, WHOLE_READ_CHUNK};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 4096;
const SHIFT: u8 = 2;

thread_local! {
  /// 是否处于测量窗口；关闭时分配器只透传。
  static COUNTING: Cell<bool> = const { Cell::new(false) };
  /// 「大分配」阈值：窗口内单次请求字节数达到该值即计入。
  static BIG_THRESHOLD: Cell<usize> = const { Cell::new(usize::MAX) };
  static ALLOC_NUM: Cell<usize> = const { Cell::new(0) };
  static ALLOC_BYTES: Cell<usize> = const { Cell::new(0) };
  static BIG_NUM: Cell<usize> = const { Cell::new(0) };
}

/// 进程级计数分配器，按线程统计（`#[compio::test]` 的 future 与运行时在同一
/// 线程执行，其它测试线程的分配不会混入）。
struct CountingAlloc;

impl CountingAlloc {
  fn record(size: usize) {
    if !COUNTING.try_with(Cell::get).unwrap_or(false) {
      return;
    }
    ALLOC_NUM.with(|cnt| cnt.set(cnt.get() + 1));
    ALLOC_BYTES.with(|bytes| bytes.set(bytes.get() + size));
    if size >= BIG_THRESHOLD.with(Cell::get) {
      BIG_NUM.with(|cnt| cnt.set(cnt.get() + 1));
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

/// 一次测量窗口的分配统计。
#[derive(Debug, Clone, Copy)]
struct AllocStat {
  num: usize,
  bytes: usize,
  /// 单次请求 ≥ 阈值的分配次数。
  big: usize,
}

/// 开始测量：`big_threshold` 以上的单次分配计入 `big`。
fn alloc_begin(big_threshold: usize) {
  BIG_THRESHOLD.with(|threshold| threshold.set(big_threshold));
  ALLOC_NUM.with(|cnt| cnt.set(0));
  ALLOC_BYTES.with(|bytes| bytes.set(0));
  BIG_NUM.with(|cnt| cnt.set(0));
  COUNTING.with(|counting| counting.set(true));
}

fn alloc_end() -> AllocStat {
  COUNTING.with(|counting| counting.set(false));
  AllocStat {
    num: ALLOC_NUM.with(Cell::get),
    bytes: ALLOC_BYTES.with(Cell::get),
    big: BIG_NUM.with(Cell::get),
  }
}

/// 确定性内容：每字节只依赖偏移，便于定位分块边界错误。
fn content(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

/// 在 `demo.1` 分区里直接落一个 `len` 字节的 data 文件（绕过写路径，
/// 便于构造任意非页对齐长度）。
fn put_data_file(root: &TempRoot, len: usize) -> Vec<u8> {
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  let bytes = content(len);
  write(dir.join("data_0_main_0"), &bytes).unwrap();
  bytes
}

/// direct / buffered 读同一文件内容一致；覆盖页对齐长度、非对齐尾与
/// 小于一块的文件长度。
#[compio::test]
async fn read_whole_matches_between_direct_and_buffered() {
  for len in [
    PAGE_SIZE,
    100,
    8 << 20,
    (8 << 20) + 100,
    WHOLE_READ_CHUNK + 7,
  ] {
    let root = TempRoot::new("read_whole_content");
    let tbl = TableIdent::new("demo", 1).unwrap();
    let expected = put_data_file(&root, len);

    let buffered = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);
    let direct = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT).with_direct_io(true);
    let got_buffered = buffered.read_file(&tbl, 0).await.unwrap().unwrap();
    let got_direct = direct.read_file(&tbl, 0).await.unwrap().unwrap();
    assert_eq!(got_buffered, expected, "buffered 读内容不一致，len={len}");
    assert_eq!(got_direct, expected, "direct 读内容不一致，len={len}");
    assert_eq!(got_direct.len(), len);
  }
}

/// 17 MiB（> `DIRECT_POOL_BUDGET`，旧实现必须整文件分配且无法入池）：
/// 峰值只多输出 Vec 本身，对齐池只建一块 1 MiB 暂存，重复读零新增分配。
#[compio::test]
async fn read_whole_direct_allocates_one_chunk_for_large_file() {
  let root = TempRoot::new("read_whole_large");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let len = 17 << 20;
  let expected = put_data_file(&root, len);
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT).with_direct_io(true);

  // 预热 fd 缓存 / 目录索引；首次读允许分配一块暂存缓冲。
  let first = store.read_file(&tbl, 0).await.unwrap().unwrap();
  assert_eq!(first, expected);
  assert_eq!(
    store.direct_buf_alloc_num(),
    1,
    "整文件读只应新建一块对齐暂存缓冲"
  );
  assert!(
    store.direct_pool_max_entry() <= WHOLE_READ_CHUNK,
    "出现了按文件长度分配的对齐缓冲：{} > {WHOLE_READ_CHUNK}",
    store.direct_pool_max_entry()
  );
  assert_eq!(
    store.read_whole_chunk_num(),
    (len / WHOLE_READ_CHUNK) as u64,
    "块读次数应为 ceil(len / WHOLE_READ_CHUNK)"
  );

  // 稳态第二读：暂存缓冲从池复用，窗口内只允许一次 ≥ len/2 的分配（输出
  // Vec）。旧实现这里会有「对齐缓冲 + 输出 Vec」两次大分配。
  let alloc_num = store.direct_buf_alloc_num();
  alloc_begin(len / 2);
  let second = store.read_file(&tbl, 0).await.unwrap().unwrap();
  let stat = alloc_end();
  assert_eq!(second, expected);
  assert_eq!(
    store.direct_buf_alloc_num(),
    alloc_num,
    "第二次整文件读不应再新增对齐缓冲"
  );
  assert_eq!(
    stat.big, 1,
    "≥ len/2 的分配只允许输出 Vec 一次，实际 {stat:?}"
  );
  assert!(stat.num >= 1, "输出 Vec 必须真实分配：{stat:?}");
  assert!(
    stat.bytes < 2 * len,
    "窗口内总分配字节应小于 2 × len，实际 {stat:?}"
  );
}

/// 非页对齐文件尾：尾块按页对齐请求、内核按 EOF 短读返回，内容仍逐字节一致，
/// 块数按 ceil(len / WHOLE_READ_CHUNK) 计。
#[compio::test]
async fn read_whole_direct_unaligned_tail_roundtrip() {
  let root = TempRoot::new("read_whole_tail");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let len = (8 << 20) + 100;
  let expected = put_data_file(&root, len);
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT).with_direct_io(true);

  let got = store.read_file(&tbl, 0).await.unwrap().unwrap();
  assert_eq!(got, expected);
  assert_eq!(
    store.read_whole_chunk_num(),
    len.div_ceil(WHOLE_READ_CHUNK) as u64
  );
  assert!(store.direct_pool_max_entry() <= WHOLE_READ_CHUNK);
}

/// 0 字节文件：直接返回空 Vec，不碰对齐池。
#[compio::test]
async fn read_whole_empty_file_allocates_no_direct_buf() {
  let root = TempRoot::new("read_whole_empty");
  let tbl = TableIdent::new("demo", 1).unwrap();
  put_data_file(&root, 0);
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT).with_direct_io(true);

  assert_eq!(
    store.read_file(&tbl, 0).await.unwrap().unwrap(),
    Vec::<u8>::new()
  );
  assert_eq!(store.direct_buf_alloc_num(), 0);
  assert_eq!(store.read_whole_chunk_num(), 0);
}

/// buffered 分支保持原样：一次整文件读入池缓冲、零额外整文件拷贝。
#[compio::test]
async fn read_whole_buffered_keeps_single_whole_file_buffer() {
  let root = TempRoot::new("read_whole_buffered");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let len = 8 << 20;
  let expected = put_data_file(&root, len);
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);

  let got = store.read_file(&tbl, 0).await.unwrap().unwrap();
  assert_eq!(got, expected);
  // buffered 分支不消费分块计数，也不建对齐缓冲。
  assert_eq!(store.read_whole_chunk_num(), 0);
  assert_eq!(store.direct_buf_alloc_num(), 0);
}
