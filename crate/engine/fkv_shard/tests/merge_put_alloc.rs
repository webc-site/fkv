//! perf-05：写组合并走所有权移动，entry 级分配为 0。
//!
//! 计数分配器只在本测试进程内生效：统计合并调用期间的堆分配总数，以及
//! 与 key / value 等长的 entry 级分配次数（旧实现为每幸存 entry clone 出
//! key、value 两个缓冲，均落在该尺寸上）。

use std::{
  alloc::{GlobalAlloc, Layout, System},
  sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
  },
};

use fkv_btree::BatchEntry;
use fkv_shard::merge_put_batches_for_test;

const REQ_NUM: usize = 64;
const ENTRY_NUM: usize = 64;
const KEY_BYTES: usize = 16;
const VALUE_BYTES: usize = 128;

/// 与 key / value 缓冲同尺寸的分配次数（clone 的 key、value 各算一次）。
static ENTRY_ALLOC_CNT: AtomicUsize = AtomicUsize::new(0);
/// 全部堆分配次数（含 `alloc_zeroed` 与 `realloc`）。
static ALLOC_CNT: AtomicUsize = AtomicUsize::new(0);
/// 计数器是进程级全局量：串行化测量窗口（nextest 每测试独立进程）。
static TEST_LOCK: Mutex<()> = Mutex::new(());

struct CountingAlloc;

impl CountingAlloc {
  fn record(size: usize) {
    ALLOC_CNT.fetch_add(1, Ordering::Relaxed);
    if size == KEY_BYTES || size == VALUE_BYTES {
      ENTRY_ALLOC_CNT.fetch_add(1, Ordering::Relaxed);
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

/// 固定 `KEY_BYTES` 字节的 key；`distinct = false` 时所有请求共享同一个 key。
fn key(req: usize, index: usize, distinct: bool) -> Vec<u8> {
  let (left, right) = if distinct {
    (req as u64, index as u64)
  } else {
    (0, 0)
  };
  let mut key = Vec::with_capacity(KEY_BYTES);
  key.extend_from_slice(&left.to_be_bytes());
  key.extend_from_slice(&right.to_be_bytes());
  key
}

fn batch_li(distinct: bool) -> Vec<Vec<BatchEntry>> {
  (0..REQ_NUM)
    .map(|req| {
      (0..ENTRY_NUM)
        .map(|index| {
          BatchEntry::upsert(
            key(req, index, distinct),
            vec![b'v'; VALUE_BYTES],
            1 + index as u64,
          )
        })
        .collect()
    })
    .collect()
}

/// 返回（总分配次数, entry 级分配次数, 幸存条数）。
fn merge(distinct: bool) -> (usize, usize, usize) {
  let mut batch_li = batch_li(distinct);
  ALLOC_CNT.store(0, Ordering::Relaxed);
  ENTRY_ALLOC_CNT.store(0, Ordering::Relaxed);
  let merged = merge_put_batches_for_test(&mut batch_li);
  let alloc_cnt = ALLOC_CNT.load(Ordering::Relaxed);
  let entry_alloc_cnt = ENTRY_ALLOC_CNT.load(Ordering::Relaxed);
  assert!(
    batch_li.iter().all(Vec::is_empty),
    "合并应 mem::take 走每个请求的 batch_li"
  );
  (alloc_cnt, entry_alloc_cnt, merged.len())
}

/// 合并阶段不按幸存 entry 分配：entry 级分配为 0，总分配固定为排序表 + 输出容器。
///
/// 修复前：每个幸存 entry clone 出 key / value（2 次 entry 级分配），4096 个
/// 幸存者时总分配约 8194；修复后 1 个与 4096 个幸存者都只有 2 次容器分配。
#[test]
fn merge_put_batches_allocates_no_entry_buffers() {
  let _guard = TEST_LOCK.lock().unwrap();
  let (alloc_same, entry_same, out_same) = merge(false);
  let (alloc_distinct, entry_distinct, out_distinct) = merge(true);
  assert_eq!(out_same, 1);
  assert_eq!(out_distinct, REQ_NUM * ENTRY_NUM);
  assert_eq!(
    entry_same, 0,
    "合并仍有 {entry_same} 次 entry 级分配（{out_same} 个幸存者）"
  );
  assert_eq!(
    entry_distinct, 0,
    "合并仍有 {entry_distinct} 次 entry 级分配（{out_distinct} 个幸存者）"
  );
  assert_eq!(
    alloc_same, alloc_distinct,
    "合并分配次数不应随幸存 entry 数变化：{alloc_same} vs {alloc_distinct}"
  );
  assert_eq!(
    alloc_distinct, 2,
    "合并应只分配排序表与输出容器，实际 {alloc_distinct}"
  );
  eprintln!(
    "merge allocations: same_key(1 survivor)={alloc_same}, distinct({out_distinct} survivors)={alloc_distinct}"
  );
}

/// 语义：同 key 不同 timestamp 取大者；timestamp 相等时后写胜
/// （跨请求后请求胜、请求内后 entry 胜）。
#[test]
fn merge_put_batches_keeps_last_write_semantics() {
  let _guard = TEST_LOCK.lock().unwrap();
  let mut batch_li = vec![
    vec![
      BatchEntry::upsert(b"k".to_vec(), b"ts1".to_vec(), 1),
      BatchEntry::upsert(b"k".to_vec(), b"ts3".to_vec(), 3),
      BatchEntry::upsert(b"k".to_vec(), b"tie-first".to_vec(), 3),
    ],
    vec![
      BatchEntry::upsert(b"k".to_vec(), b"tie-second".to_vec(), 3),
      BatchEntry::upsert(b"k".to_vec(), b"ts2".to_vec(), 2),
      BatchEntry::upsert(b"z".to_vec(), b"only".to_vec(), 9),
    ],
  ];
  let merged = merge_put_batches_for_test(&mut batch_li);
  assert_eq!(merged.len(), 2);
  assert_eq!(merged[0].key.as_slice(), b"k");
  assert_eq!(merged[0].timestamp, 3);
  assert_eq!(merged[0].value.as_slice(), b"tie-second");
  assert_eq!(merged[1].key.as_slice(), b"z");
  assert_eq!(merged[1].value.as_slice(), b"only");
}
