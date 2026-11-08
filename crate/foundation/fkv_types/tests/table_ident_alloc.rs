//! `TableIdent::clone` 分配回归：`tbl_name` 为 `Arc<str>` 后，clone 只递增
//! 引用计数，不再 `malloc + memcpy`。
//!
//! 计数分配器只统计本线程测量窗口内的分配（thread-local 计数），因此与
//! `cargo test` 的同进程并行测试、nextest 的独立进程都无冲突。

use std::{
  alloc::{GlobalAlloc, Layout, System},
  cell::Cell,
  collections::hash_map::DefaultHasher,
  hash::{Hash, Hasher},
  hint::black_box,
  sync::Arc,
};

use fkv_types::TableIdent;

thread_local! {
  /// 当前线程是否处于测量窗口。
  static TRACKING: Cell<bool> = const { Cell::new(false) };
  /// 测量窗口内的堆分配次数（含 `alloc_zeroed` / `realloc`）。
  static ALLOC_CNT: Cell<usize> = const { Cell::new(0) };
}

fn note_alloc() {
  // TLS 析构期间访问会返回 Err，直接忽略：计数只服务于测试断言。
  let _ = TRACKING.try_with(|tracking| {
    if tracking.get() {
      let _ = ALLOC_CNT.try_with(|count| count.set(count.get() + 1));
    }
  });
}

struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
  unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    note_alloc();
    unsafe { System.alloc(layout) }
  }

  unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
    note_alloc();
    unsafe { System.alloc_zeroed(layout) }
  }

  unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    note_alloc();
    unsafe { System.realloc(ptr, layout, new_size) }
  }

  unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
    unsafe { System.dealloc(ptr, layout) }
  }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

/// 在关闭计数的前提下执行 `f`，返回结果与本线程期间的分配次数。
fn count_allocs<T>(f: impl FnOnce() -> T) -> (T, usize) {
  ALLOC_CNT.with(|count| count.set(0));
  TRACKING.with(|tracking| tracking.set(true));
  let out = f();
  TRACKING.with(|tracking| tracking.set(false));
  (out, ALLOC_CNT.with(Cell::get))
}

#[test]
fn clone_is_zero_alloc() {
  let tbl = TableIdent::new("demo_table", 7).unwrap();
  let ((), allocs) = count_allocs(|| {
    for _ in 0..64 {
      let clone = tbl.clone();
      black_box(&clone);
    }
  });
  assert_eq!(
    allocs, 0,
    "TableIdent::clone 不应有堆分配（实测 {allocs} 次）"
  );

  // 自检：计数分配器确实能看到本线程的堆分配，避免上面的断言变成空转。
  let (_, allocs) = count_allocs(|| TableIdent::new(String::from("demo"), 7).unwrap());
  assert!(allocs > 0, "计数分配器未生效（构造 String 也未记录分配）");
}

#[test]
fn new_accepts_str_string_and_boxed_str() {
  let from_str = TableIdent::new("demo", 1).unwrap();
  let from_string = TableIdent::new(String::from("demo"), 1).unwrap();
  let from_box = TableIdent::new(String::from("demo").into_boxed_str(), 1).unwrap();
  assert_eq!(from_str, from_string);
  assert_eq!(from_string, from_box);
}

#[test]
fn eq_and_hash_stay_content_based() {
  let a = TableIdent::new(String::from("demo"), 1).unwrap();
  let b = TableIdent::new("demo".to_owned(), 1).unwrap();
  // 两个独立构造的 `Arc<str>` 必然指向不同分配，但语义按内容比较。
  assert!(!Arc::ptr_eq(&a.tbl_name, &b.tbl_name));
  assert_eq!(a, b);

  fn hash_of(tbl: &TableIdent) -> u64 {
    let mut hasher = DefaultHasher::new();
    tbl.hash(&mut hasher);
    hasher.finish()
  }
  assert_eq!(hash_of(&a), hash_of(&b));
}

#[test]
fn display_debug_and_parse_are_unchanged() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  assert_eq!(tbl.dir_name(), "demo.1");
  assert_eq!(format!("{tbl}"), "demo.1");
  assert_eq!(
    format!("{tbl:?}"),
    r#"TableIdent { tbl_name: "demo", partition_id: 1 }"#
  );
  assert_eq!(TableIdent::parse_dir_name("demo.1"), Some(tbl));
  assert_eq!(
    "demo.1".parse::<TableIdent>().unwrap(),
    TableIdent::new("demo", 1).unwrap()
  );
}
