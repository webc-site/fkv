//! 读 / 写缓冲池与 O_DIRECT 对齐缓冲。

use core::{mem::MaybeUninit, slice::from_raw_parts_mut};

use compio::buf::{IoBuf, IoBufMut, SetLen};
use fkv_buf::{AlignedBuf, PagesPool};

use crate::{BufPool, CompioStore, DirectBuf, ReadChunk};

/// 缓冲池保留的缓冲块数上限（按大小分组）。
const BUF_POOL_LIMIT: usize = 8;
/// 读 / 写缓冲池各自的常驻字节预算。
///
/// 块数上限不约束块长，而归还的缓冲长度随请求无界（manifest 整文件读默认
/// 上限 8 MiB，data 文件整读最大 1 GiB）。没有字节上限时，一次大文件读失败
/// 就能让整文件长度的缓冲常驻，池上界退化为「块数 × 文件大小」。预算与
/// `DIRECT_POOL_BUDGET` 同口径（归还时按池内累计字节封顶），只作用于空闲
/// 缓冲，不影响在飞 IO。
///
/// 取 4 MiB：默认稳态读池 8 × 256 KiB 段 = 2 MiB、写池 8 × 256 KiB = 2 MiB
/// 均完整容纳并留一倍余量；单池最坏常驻从无界压到 4 MiB，读 + 写 + direct
/// 三池合计上界 4 + 4 + 16 = 24 MiB/store。
const BUF_POOL_BUDGET: usize = 4 << 20;
/// 允许入池的单块长度上限。
///
/// 页读（page_size）、默认段读（256 KiB）、合并写（≤ 128 KiB）都远低于该值；
/// manifest 整文件读（默认上限 8 MiB）与 data 文件整读（最大 1 GiB）是一次性
/// 大块，复用收益趋零，失败归还时直接丢弃。
const BUF_POOL_MAX_ENTRY: usize = 1 << 20;
/// O_DIRECT 对齐池的总内存上限：`segment` 读长度随值长度变化，按长度分桶
/// 会放大常驻内存，归还时按全局字节数封顶。
const DIRECT_POOL_BUDGET: usize = 16 * 1024 * 1024;

impl DirectBuf {
  pub(crate) fn from_buf(buf: AlignedBuf, len: usize) -> Self {
    debug_assert!(len <= buf.len());
    Self { buf, len }
  }

  pub(crate) fn from_slice(buf: AlignedBuf, src: &[u8]) -> Self {
    let mut out = Self::from_buf(buf, src.len());
    out.as_mut_slice().copy_from_slice(src);
    out
  }

  pub(crate) fn into_inner(self) -> AlignedBuf {
    self.buf
  }
}

impl IoBuf for DirectBuf {
  fn as_init(&self) -> &[u8] {
    &self.buf[..self.len]
  }
}

impl SetLen for DirectBuf {
  unsafe fn set_len(&mut self, len: usize) {
    self.len = len;
  }
}

impl IoBufMut for DirectBuf {
  fn as_uninit(&mut self) -> &mut [MaybeUninit<u8>] {
    // SAFETY: `AlignedBuf` 独占整块 `len` 字节；`MaybeUninit` 允许未初始化读取。
    unsafe { from_raw_parts_mut(self.buf.as_mut_ptr().cast(), self.buf.len()) }
  }
}

impl ReadChunk {
  pub(crate) fn as_slice(&self) -> &[u8] {
    match self {
      Self::Plain(buf) => buf,
      Self::Direct(buf) => buf.as_init(),
    }
  }
}

impl BufPool {
  /// 归还准入：块数、单块长度与字节预算三重上限。
  ///
  /// 先判单块长度再累加字节，`len` 超过 `BUF_POOL_MAX_ENTRY` 时不会参与加法，
  /// 不存在 `pooled_bytes + len` 溢出。
  fn admit(&self, len: usize) -> bool {
    self.total < BUF_POOL_LIMIT
      && len <= BUF_POOL_MAX_ENTRY
      && self.pooled_bytes + len <= BUF_POOL_BUDGET
  }

  /// 取一块长度为 `len` 的写缓冲；池空时新分配（清零一次）并计数。
  fn take(&mut self, len: usize) -> Vec<u8> {
    self.take_num += 1;
    if let Some(buf) = self.slots.get_mut(&len).and_then(Vec::pop) {
      self.total -= 1;
      self.pooled_bytes -= len;
      debug_assert_eq!(buf.len(), len);
      return buf;
    }
    self.alloc_num += 1;
    vec![0u8; len]
  }

  /// 归还写缓冲；容量与请求长度不符或超出预算的缓冲不复用（写入长度即缓冲长度）。
  fn put(&mut self, len: usize, mut buf: Vec<u8>) {
    if buf.capacity() != len || !self.admit(len) {
      return;
    }
    if buf.len() != len {
      buf.resize(len, 0);
    }
    self.slots.entry(len).or_default().push(buf);
    self.total += 1;
    self.pooled_bytes += len;
  }

  /// 取一块**容量**为 `len` 的读缓冲：不预清零，交给 compio 直接读入。
  fn take_read(&mut self, len: usize) -> Vec<u8> {
    self.take_num += 1;
    if let Some(buf) = self.slots.get_mut(&len).and_then(Vec::pop) {
      self.total -= 1;
      self.pooled_bytes -= len;
      debug_assert_eq!(buf.capacity(), len);
      return buf;
    }
    self.alloc_num += 1;
    Vec::with_capacity(len)
  }

  /// 归还读缓冲：长度归零、内容不清零（短读后的未读区域也保持原样）。
  ///
  /// 超出块长上限或字节预算的缓冲直接丢弃：`read_whole` 的失败 / 短读路径
  /// 会归还整文件长度的缓冲，这类大块复用收益趋零且会把池变成隐性大内存
  /// 持有者。
  fn put_read(&mut self, len: usize, mut buf: Vec<u8>) {
    if buf.capacity() != len || !self.admit(len) {
      return;
    }
    buf.clear();
    self.slots.entry(len).or_default().push(buf);
    self.total += 1;
    self.pooled_bytes += len;
  }
}

impl CompioStore {
  /// 取读缓冲：只保证容量为 `len`，不预清零（compio 按容量读入）。
  pub(crate) fn take_read_buf(&self, len: usize) -> Vec<u8> {
    self.read_buf.borrow_mut().take_read(len)
  }

  /// 归还读缓冲：长度归零、内容不清零。
  pub(crate) fn put_read_buf(&self, len: usize, buf: Vec<u8>) {
    self.read_buf.borrow_mut().put_read(len, buf);
  }

  /// 取一块长度为 `len` 的 O_DIRECT 对齐缓冲。
  ///
  /// 桶按长度建立、新块用 `alloc_zeroed` 分配（整块清零）。**超过
  /// `DIRECT_POOL_BUDGET` 的块归还时直接丢弃**：旧实现按整文件长度取块，
  /// 于是 >16 MiB 的 data 文件每次整文件读都要重新分配并清零一遍；现在整
  /// 文件读按 `WHOLE_READ_CHUNK` 分块，块长有界、可复用，不会再建大桶。
  pub(crate) fn take_direct_buf(&self, len: usize) -> AlignedBuf {
    self
      .direct_pool
      .borrow_mut()
      .entry(len)
      .or_insert_with(|| PagesPool::new(len))
      .allocate()
  }

  /// 归还 O_DIRECT 对齐缓冲；池内累计字节超过 `DIRECT_POOL_BUDGET` 即丢弃。
  ///
  /// 预算只在归还时检查：在飞 IO 不受限，但空闲块总量有界。块长超过预算的
  /// 大块永远无法入池，每次使用都要重新分配 + 清零（整文件读已改为分块，
  /// 不再出现这种块）。
  pub(crate) fn put_direct_buf(&self, buf: AlignedBuf) {
    let len = buf.len();
    let pool_map = self.direct_pool.borrow();
    let Some(pool) = pool_map.get(&len) else {
      return;
    };
    let used: usize = pool_map
      .values()
      .map(|cached| cached.cached() * cached.page_size())
      .sum();
    if used + len <= DIRECT_POOL_BUDGET {
      pool.recycle(buf);
    }
  }
}

impl CompioStore {
  pub(crate) fn take_write_buf(&self, len: usize) -> Vec<u8> {
    self.write_buf.borrow_mut().take(len)
  }

  pub(crate) fn put_write_buf(&self, len: usize, buf: Vec<u8>) {
    self.write_buf.borrow_mut().put(len, buf);
  }
}

#[cfg(test)]
mod test {
  use super::{BUF_POOL_BUDGET, BUF_POOL_LIMIT, BUF_POOL_MAX_ENTRY};
  use crate::BufPool;

  /// `read_whole` 失败路径会归还整文件长度的缓冲（manifest 默认上限 8 MiB，
  /// data 文件最大 1 GiB）：超过单块上限的块直接丢弃，不进入任何桶。
  #[test]
  fn oversized_read_buf_is_dropped_instead_of_pooled() {
    let mut pool = BufPool::default();
    for len in [BUF_POOL_MAX_ENTRY + 1, 8 << 20] {
      pool.put_read(len, Vec::with_capacity(len));
      assert_eq!(pool.total, 0, "len {len} 不应入池");
      assert_eq!(pool.pooled_bytes, 0);
      assert!(pool.slots.is_empty());
    }
  }

  /// 1 MiB 块同时受块数（8）与字节预算（4 MiB）约束：只保留预算内的块，
  /// 取出时字节计数同步回落。
  #[test]
  fn read_pool_stays_within_byte_budget() {
    let mut pool = BufPool::default();
    let len = BUF_POOL_MAX_ENTRY;
    let fit = BUF_POOL_BUDGET / len;
    for _ in 0..fit + 2 {
      pool.put_read(len, Vec::with_capacity(len));
    }
    assert_eq!(pool.total, fit);
    assert_eq!(pool.pooled_bytes, BUF_POOL_BUDGET);
    for _ in 0..fit {
      assert_eq!(pool.take_read(len).capacity(), len);
    }
    assert_eq!(pool.total, 0);
    assert_eq!(pool.pooled_bytes, 0);
  }

  /// 默认 256 KiB 段：8 块共 2 MiB 全部入池且全部命中，预算不误伤稳态复用。
  #[test]
  fn segment_sized_read_bufs_still_hit_pool() {
    let mut pool = BufPool::default();
    let len = 256 << 10;
    for _ in 0..BUF_POOL_LIMIT {
      pool.put_read(len, Vec::with_capacity(len));
    }
    assert_eq!(pool.total, BUF_POOL_LIMIT);
    assert_eq!(pool.pooled_bytes, BUF_POOL_LIMIT * len);
    let alloc_num = pool.alloc_num;
    for _ in 0..BUF_POOL_LIMIT {
      assert_eq!(pool.take_read(len).capacity(), len);
    }
    assert_eq!(pool.alloc_num, alloc_num, "稳态段读不应重新分配");
    assert_eq!(pool.pooled_bytes, 0);
  }

  /// 写池同口径：超出单块上限的缓冲不入池。
  #[test]
  fn write_pool_drops_oversized_buf() {
    let mut pool = BufPool::default();
    let len = BUF_POOL_MAX_ENTRY + 1;
    pool.put(len, vec![0u8; len]);
    assert_eq!(pool.total, 0);
    assert_eq!(pool.pooled_bytes, 0);
    assert!(pool.slots.is_empty());
  }

  /// 写池字节预算：最多保留 4 个 1 MiB 块，第 5 块被拒绝。
  #[test]
  fn write_pool_stays_within_byte_budget() {
    let mut pool = BufPool::default();
    let len = BUF_POOL_MAX_ENTRY;
    let fit = BUF_POOL_BUDGET / len;
    for _ in 0..fit + 1 {
      pool.put(len, vec![0u8; len]);
    }
    assert_eq!(pool.total, fit);
    assert_eq!(pool.pooled_bytes, BUF_POOL_BUDGET);
  }
}
