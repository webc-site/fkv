//! 对齐 C++ `tests/global_registered_memory.cpp` 的池语义：对齐、独占分配、
//! LIFO 回收、只复用已归还块、尺寸不符丢弃。
//!
//! C++ `GlobalRegisteredMemory` 是 io_uring 注册内存的分段池；Rust 侧对应的
//! 页级复用原语是 `PagesPool`（chunk / 注册由 compio provided-buffer 机制承担）。

use fkv_buf::{AlignedBuf, PagesPool};

fn ptr(buf: &fkv_buf::AlignedBuf) -> usize {
  buf.as_ptr() as usize
}

/// 未归还时分配的块两两不同；全部归还后计数恢复（对齐 "allocates every
/// segment exactly once, then empties"）。
#[test]
fn allocate_distinct_until_recycled() {
  let pool = PagesPool::new(4096);
  let a = pool.allocate();
  let b = pool.allocate();
  let c = pool.allocate();
  // 对齐 C++ "chunks correct size and alignment"：页缓冲 4 KiB 对齐。
  assert!(a.is_aligned(4096));
  assert_ne!(ptr(&a), ptr(&b));
  assert_ne!(ptr(&b), ptr(&c));
  assert_eq!(pool.cached(), 0);

  pool.recycle(a);
  pool.recycle(b);
  pool.recycle(c);
  assert_eq!(pool.cached(), 3);
}

/// 回收顺序 A,B,C → 再分配依次得到 C,B,A（LIFO，对齐 "recycle is LIFO"）。
#[test]
fn recycle_is_lifo() {
  let pool = PagesPool::new(4096);
  let a = pool.allocate();
  let b = pool.allocate();
  let c = pool.allocate();
  let (pa, pb, pc) = (ptr(&a), ptr(&b), ptr(&c));
  assert_ne!(pa, pb);
  assert_ne!(pb, pc);

  pool.recycle(a);
  pool.recycle(b);
  pool.recycle(c);

  let (r1, r2, r3) = (pool.allocate(), pool.allocate(), pool.allocate());
  assert_eq!(ptr(&r1), pc);
  assert_eq!(ptr(&r2), pb);
  assert_eq!(ptr(&r3), pa);
}

/// 排空后归还一块，下一次分配必须是那一块（对齐 "only returns recycled
/// segments when pool was drained"）。
#[test]
fn recycled_buf_reused_exactly() {
  let pool = PagesPool::new(4096);
  let all: Vec<_> = (0..4).map(|_| pool.allocate()).collect();
  assert_eq!(pool.cached(), 0);

  let only_ptr = ptr(&all[2]);
  let only = all.into_iter().nth(2).unwrap();
  pool.recycle(only);

  let next = pool.allocate();
  assert_eq!(ptr(&next), only_ptr);
  assert_eq!(pool.cached(), 0);
}

/// 尺寸不匹配的归还直接丢弃，不进池、不失配复用（对齐 recycle 语义）。
#[test]
fn recycle_size_mismatch_dropped() {
  let pool = PagesPool::new(4096);
  let wrong = AlignedBuf::new(8192);
  pool.recycle(wrong);
  assert_eq!(pool.cached(), 0);

  // 归还正确尺寸后池计数才增长。
  let right = pool.allocate();
  pool.recycle(right);
  assert_eq!(pool.cached(), 1);
}

/// 超过 max_cached 的归还静默丢弃，缓存数有界。
#[test]
fn recycle_caps_at_max_cached() {
  let pool = PagesPool::with_max_cached(4096, 2);
  let bufs: Vec<_> = (0..4).map(|_| pool.allocate()).collect();
  for buf in bufs {
    pool.recycle(buf);
  }
  assert_eq!(pool.cached(), 2);
}
