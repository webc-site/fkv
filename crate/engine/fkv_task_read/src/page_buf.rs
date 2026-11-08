//! 整页缓冲的线程本地池。
//!
//! 读 / 扫描路径按页大小取用整页缓冲；池跨请求复用，避免每次请求都
//! `vec![0; page_size]`。缓冲在 `await` 期间由调用方独占（从池中 `pop` 出来），
//! 不会产生跨 `await` 的 `RefCell` 借用；同线程并发任务池空时自行分配，
//! 不阻塞也不 panic。

use std::{
  cell::RefCell,
  mem::take,
  ops::{Deref, DerefMut},
};

/// 页缓冲线程本地池上限（块）。
///
/// 单请求的整页缓冲工作集 = 当前页 1 块 + 叶子链块窗口 ≤ `CHAIN_WINDOW_DEPTH`
/// 块（见 `fkv_task_scan`，窗口深度上界 16），池上限必须覆盖这个工作集：否则
/// 每次窗口请求都要重新分配整页缓冲（4 KiB 页 × 16 块 = 每请求 64 KiB 的
/// malloc + 清零），perf-10 的「预热后 scan 零整页分配」回归直接失效。
///
/// 保留量按块数封顶（而非字节）：4 KiB 页时 24 块 = 96 KiB / 线程。
pub const PAGE_BUF_POOL_MAX: usize = 24;

thread_local! {
  static PAGE_BUF_POOL: RefCell<Vec<Vec<u8>>> = const { RefCell::new(Vec::new()) };
}

/// 借一块长度为 `page_size` 的缓冲：优先复用池内缓冲，池空时分配。
pub(crate) fn acquire_buf(page_size: usize) -> Vec<u8> {
  match PAGE_BUF_POOL.try_with(|pool| pool.borrow_mut().pop()) {
    Ok(Some(mut buf)) => {
      // 同页大小的稳态里 `resize` 不分配、不写；只有池内页大小不同才补零。
      buf.resize(page_size, 0);
      buf
    }
    // TLS 已销毁或池空：退化为一次性分配。
    Ok(None) | Err(_) => vec![0u8; page_size],
  }
}

/// 归还缓冲；超出池上限或 TLS 已销毁时直接丢弃，不影响正确性。
pub(crate) fn release_buf(buf: Vec<u8>) {
  let _ = PAGE_BUF_POOL.try_with(|pool| {
    let mut pool = pool.borrow_mut();
    if pool.len() < PAGE_BUF_POOL_MAX {
      pool.push(buf);
    }
  });
}

/// 借用的整页缓冲，`Drop` 时归还线程本地池。
pub struct PageBuf(Vec<u8>);

/// 借一批长度为 `page_size` 的缓冲（批量整页读的目标，如
/// [`fkv_io::PageIo::read_page_span`]）。
///
/// 与 [`PageBuf`] 共用同一个线程本地池，归还走 [`release_buf_li`]：调用方按
/// 「一批」为单位持有，省掉每个槽各自的守卫类型。
pub fn acquire_buf_li(num: usize, page_size: usize) -> Vec<Vec<u8>> {
  (0..num).map(|_| acquire_buf(page_size)).collect()
}

/// 归还一批整页缓冲；超出池上限或空缓冲直接丢弃。
pub fn release_buf_li(buf_li: Vec<Vec<u8>>) {
  for buf in buf_li {
    if !buf.is_empty() {
      release_buf(buf);
    }
  }
}

impl PageBuf {
  pub fn new(page_size: usize) -> Self {
    Self(acquire_buf(page_size))
  }

  /// 可变借出内部 `Vec`，供 [`fkv_io::PageIo::read_page_owned`] 直读。
  ///
  /// 长度保持 `page_size`；借出期间缓冲仍归 `PageBuf` 所有，`Drop` 时照常
  /// 归还线程本地池。
  pub fn as_mut_vec(&mut self) -> &mut Vec<u8> {
    &mut self.0
  }

  /// 取走内部缓冲的所有权（调用方把它当作输出页持有/落盘），不再归还池。
  pub fn into_vec(mut self) -> Vec<u8> {
    take(&mut self.0)
  }
}

impl Deref for PageBuf {
  type Target = [u8];

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl DerefMut for PageBuf {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.0
  }
}

impl Drop for PageBuf {
  fn drop(&mut self) {
    let buf = take(&mut self.0);
    // `into_vec` 取走后缓冲为空：不把空 Vec 放进池（否则下次取用仍要分配）。
    if !buf.is_empty() {
      release_buf(buf);
    }
  }
}
