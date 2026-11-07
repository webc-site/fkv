//! chunk 缓冲：定长 4 KiB 分块、线程内回收池与分配 / 复制计数。

use std::{
  cell::{Cell, RefCell},
  mem::take,
};

use fkv_types::{ValType, is_file_page_id, val_type};

/// 槽位是否可回收进逻辑页空闲链表：非活跃页（`Invalid`）或空闲链表节点（`PageId`）。
///
/// C++ 按 `!IsFilePageId` 判定；Rust 目前没有 swizzling 写入路径，这里额外排除
/// `SwizzlingPointer`，避免将来 swizzle 落地后误回收仍然活跃的页。
pub(crate) fn is_free_slot(value: u64) -> bool {
  !is_file_page_id(value) && val_type(value) != ValType::SwizzlingPointer
}

/// 分块大小：对齐 C++ `MappingTbl` 的 `kChunkShift=9`（512 个 slot/chunk）。
pub(crate) const CHUNK_SHIFT: u32 = 9;
pub const CHUNK_SIZE: usize = 1 << CHUNK_SHIFT;

/// 线程内 chunk 缓冲回收池上限（每块 `CHUNK_SIZE * 8` 字节）。
///
/// 对齐 C++ `MappingChunkArena`：快照释放的 chunk 归池，下一次分配直接复用，
/// 避免反复 4 KiB malloc。shard 单线程模型下用线程局部即可，无需锁。
pub const CHUNK_CACHE_MAX: usize = 256;

thread_local! {
  /// 本线程空闲 chunk 缓冲（容量 ≥ `CHUNK_SIZE`，内容可能是旧值）。
  static CHUNK_POOL: RefCell<Vec<Vec<u64>>> = const { RefCell::new(Vec::new()) };
  /// 本线程真实分配（未命中池）的 chunk 数，供测试 / 诊断。
  static CHUNK_ALLOC_NUM: Cell<u64> = const { Cell::new(0) };
  /// 本线程因共享而复制 chunk 指针表的次数，供测试 / 诊断。
  pub(crate) static CHUNK_DIR_COPY_NUM: Cell<u64> = const { Cell::new(0) };
}

/// 从线程池取一块缓冲（已清空，容量 ≥ `CHUNK_SIZE`）；池空则新分配。
pub(crate) fn take_chunk() -> Vec<u64> {
  match CHUNK_POOL
    .try_with(|pool| pool.borrow_mut().pop())
    .ok()
    .flatten()
  {
    Some(mut value_li) => {
      value_li.clear();
      value_li
    }
    None => {
      let _ = CHUNK_ALLOC_NUM.try_with(|num| num.set(num.get() + 1));
      Vec::with_capacity(CHUNK_SIZE)
    }
  }
}

/// 归还一块缓冲；容量不足或池已满时直接释放。
pub(crate) fn give_chunk(value_li: Vec<u64>) {
  if value_li.capacity() < CHUNK_SIZE {
    return;
  }
  let _ = CHUNK_POOL.try_with(|pool| {
    let mut free = pool.borrow_mut();
    if free.len() < CHUNK_CACHE_MAX {
      free.push(value_li);
    }
  });
}

/// 本线程真实分配（未命中池）的 chunk 块数；测试 / 诊断用。
pub fn chunk_alloc_num() -> u64 {
  CHUNK_ALLOC_NUM.try_with(Cell::get).unwrap_or(0)
}

/// 本线程因快照共享而复制 chunk 指针表的次数；测试 / 诊断用。
pub fn chunk_dir_copy_num() -> u64 {
  CHUNK_DIR_COPY_NUM.try_with(Cell::get).unwrap_or(0)
}

/// 单块映射缓冲（`CHUNK_SIZE` 个槽）。释放时归还线程内 chunk 池，
/// 对齐 C++ `MappingChunkArena` 的跨快照复用。
#[derive(Debug)]
pub(crate) struct Chunk {
  pub(crate) value_li: Vec<u64>,
}

impl Chunk {
  /// 以 `value` 填满的新块。
  pub(crate) fn filled(value: u64) -> Self {
    let mut value_li = take_chunk();
    value_li.resize(CHUNK_SIZE, value);
    Self { value_li }
  }
}

impl Clone for Chunk {
  fn clone(&self) -> Self {
    // COW 拷贝：直接从池取一块复用缓冲，避免 `Vec::clone` 的新分配。
    let mut value_li = take_chunk();
    value_li.extend_from_slice(&self.value_li);
    Self { value_li }
  }
}

impl Drop for Chunk {
  fn drop(&mut self) {
    give_chunk(take(&mut self.value_li));
  }
}
