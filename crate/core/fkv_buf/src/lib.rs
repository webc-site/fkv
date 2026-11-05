//! 4 KiB 对齐 buffer，供 O_DIRECT / 页缓存 / 直接 IO 使用。

use core::{
  fmt,
  ops::{Deref, DerefMut},
  ptr::NonNull,
  slice,
};
use std::{
  alloc::{Layout, alloc_zeroed, dealloc, handle_alloc_error},
  cell::RefCell,
};

use fkv_types::PAGE_ALIGN;

/// 固定对齐的堆 buffer。长度固定，禁止 realloc。
pub struct AlignedBuf {
  ptr: NonNull<u8>,
  len: usize,
  layout: Layout,
}

// SAFETY: `AlignedBuf` 独占一段堆内存，移动所有权不会产生别名；
// 所有访问都通过 `&self` / `&mut self` 的 Rust 借用规则约束。
unsafe impl Send for AlignedBuf {}
unsafe impl Sync for AlignedBuf {}

impl AlignedBuf {
  pub fn new(len: usize) -> Self {
    Self::with_align(len, PAGE_ALIGN)
  }

  pub fn with_align(len: usize, align: usize) -> Self {
    assert!(align.is_power_of_two());
    let layout = Layout::from_size_align(len, align).expect("invalid aligned buffer layout");
    // 零长缓冲不分配：`alloc_zeroed` 要求 size 非零，否则违反 GlobalAlloc 契约。
    if len == 0 {
      return Self {
        ptr: NonNull::dangling(),
        len,
        layout,
      };
    }
    // SAFETY: layout 非零；分配失败时 `alloc_zeroed` 返回 null，下面显式处理。
    let ptr = unsafe { alloc_zeroed(layout) };
    let ptr = NonNull::new(ptr).unwrap_or_else(|| handle_alloc_error(layout));
    Self { ptr, len, layout }
  }

  pub fn len(&self) -> usize {
    self.len
  }

  pub fn is_empty(&self) -> bool {
    self.len == 0
  }

  pub fn as_ptr(&self) -> *const u8 {
    self.ptr.as_ptr()
  }

  pub fn as_mut_ptr(&mut self) -> *mut u8 {
    self.ptr.as_ptr()
  }

  pub fn is_aligned(&self, align: usize) -> bool {
    (self.ptr.as_ptr() as usize).is_multiple_of(align)
  }
}

impl Deref for AlignedBuf {
  type Target = [u8];

  fn deref(&self) -> &Self::Target {
    // SAFETY: ptr/len 来自同一块有效分配，生命周期与 self 绑定。
    unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
  }
}

impl DerefMut for AlignedBuf {
  fn deref_mut(&mut self) -> &mut Self::Target {
    // SAFETY: 同上；`&mut self` 保证独占访问。
    unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
  }
}

impl Drop for AlignedBuf {
  fn drop(&mut self) {
    if self.len == 0 {
      return;
    }
    // SAFETY: 由 `with_align` 使用同一 layout 分配。
    unsafe { dealloc(self.ptr.as_ptr(), self.layout) }
  }
}

impl fmt::Debug for AlignedBuf {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("AlignedBuf")
      .field("len", &self.len)
      .field("align", &self.layout.align())
      .field("ptr", &self.ptr)
      .finish()
  }
}

/// 页池：固定页大小，回收后复用，最多缓存 `max_cached` 块。
#[derive(Debug)]
pub struct PagesPool {
  page_size: usize,
  max_cached: usize,
  free: RefCell<Vec<AlignedBuf>>,
}

impl PagesPool {
  pub fn new(page_size: usize) -> Self {
    Self::with_max_cached(page_size, 64)
  }

  pub fn with_max_cached(page_size: usize, max_cached: usize) -> Self {
    assert!(page_size > 0 && page_size.is_multiple_of(PAGE_ALIGN));
    Self {
      page_size,
      max_cached,
      free: RefCell::new(Vec::new()),
    }
  }

  pub fn page_size(&self) -> usize {
    self.page_size
  }

  pub fn allocate(&self) -> AlignedBuf {
    self
      .free
      .borrow_mut()
      .pop()
      .unwrap_or_else(|| AlignedBuf::new(self.page_size))
  }

  /// 归还一页；大小不匹配或池已满时直接释放。
  pub fn recycle(&self, buf: AlignedBuf) {
    if buf.len() != self.page_size {
      return;
    }
    let mut free = self.free.borrow_mut();
    if free.len() < self.max_cached {
      free.push(buf);
    }
  }

  pub fn cached(&self) -> usize {
    self.free.borrow().len()
  }
}
