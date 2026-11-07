//! 物理页分配：append / pooled 两种模式与文件级空间统计。

use fkv_types::{FileId, FilePageId};

/// 物理页分配策略，对齐 C++ `PageManager` 的 append / pooled 两种模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocMode {
  /// 只追加：`free` 为 no-op，旧物理页永不复用，COW 天然安全。
  Append,
  /// 池化复用：释放的物理页入 LIFO 空闲链，下次 `allocate` 立即弹出。
  ///
  /// **回收协议**：`PageMapper` 在 Pooled 模式下不会把 `remap_page` /
  /// `free_page` 的旧页直接 `free` 给池，而是先放入 retired 队列，并记录
  /// 释放时刻的快照代际；调用方用
  /// [`crate::SnapshotRegistry::min_live_generation`] 作为 watermark 调用
  /// [`crate::PageMapper::reclaim_retired`]，只有代际更老的页才回池复用。
  /// 这样在途快照读者仍按旧 `FilePageId` 读时不会读到已被重分配的页。
  ///
  /// 未接线快照代际来源（`set_snapshot_generation_source`）时视为「无在途
  /// 读者」的旧单写者语义；接入并发读的 Store 必须接线并周期调用 reclaim。
  /// 对齐 C++ `MappingSnapshot::to_free_file_pages_` + `next_snapshot_` 代际链。
  Pooled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendAllocator {
  max_fp_id: FilePageId,
  /// 最老的非空文件；之前由 GC / compact 更新。
  min_file_id: FileId,
  /// `min_file_id` 之后连续的空文件数。
  empty_file_cnt: u32,
}

impl AppendAllocator {
  pub fn new(max_fp_id: FilePageId) -> Self {
    Self {
      max_fp_id,
      min_file_id: 0,
      empty_file_cnt: 0,
    }
  }

  pub fn allocate(&mut self) -> FilePageId {
    let id = self.max_fp_id;
    self.max_fp_id += 1;
    id
  }

  pub fn free(&mut self, _file_page_id: FilePageId) {}

  pub fn max_fp_id(&self) -> FilePageId {
    self.max_fp_id
  }

  /// 预留到指定 file page id，后续分配从该位置开始。
  pub fn reserve_until(&mut self, file_page_id: FilePageId) {
    self.max_fp_id = self.max_fp_id.max(file_page_id);
  }

  /// 有效页空间（排除 `[min_file_id, min_file_id+empty_file_cnt)` 的空文件），
  /// 对齐 C++ `PageMapper::SpaceSize()`。
  pub fn space_size(&self, pages_per_file_shift: u8) -> u64 {
    let base = self.min_file_id << pages_per_file_shift;
    let empty = u64::from(self.empty_file_cnt) << pages_per_file_shift;
    self.max_fp_id.saturating_sub(base).saturating_sub(empty)
  }

  /// GC / compact 每轮扫描后回写：最老非空文件与紧随其后的空文件数。
  pub fn update_stat(&mut self, min_file_id: FileId, empty_file_cnt: u32) {
    debug_assert!(min_file_id >= self.min_file_id);
    self.min_file_id = min_file_id;
    self.empty_file_cnt = empty_file_cnt;
  }

  /// 全空映射时把 high-water 推到当前文件的上取整边界，并同步 `min_file_id`，
  /// 使 `space_size` 归零。对齐 C++ `DoCompactDataFile` 的 `fp_ids.empty()` 分支
  /// 与 `UpdateStat`：文件号按 ceil 计算，`max_fp_id` 已对齐时不动，
  /// 只把统计挪到该文件，避免多烧一整个文件 id 段。
  pub fn skip_to_next_file(&mut self, pages_per_file_shift: u8) {
    let mask = (1u64 << pages_per_file_shift) - 1;
    let next_fp_id = (self.max_fp_id + mask) & !mask;
    if next_fp_id > self.max_fp_id {
      self.max_fp_id = next_fp_id;
    }
    self.min_file_id = self.max_fp_id >> pages_per_file_shift;
    self.empty_file_cnt = 0;
  }

  pub fn min_file_id(&self) -> FileId {
    self.min_file_id
  }

  pub fn empty_file_cnt(&self) -> u32 {
    self.empty_file_cnt
  }
}

/// 一段连续空闲物理页，半开区间 `[start, end)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FreeRange {
  start: FilePageId,
  end: FilePageId,
}

impl FreeRange {
  #[inline]
  fn len(&self) -> u64 {
    self.end - self.start
  }
}

/// Pooled 物理页空闲链（LIFO）。
///
/// `free` 立即入栈、`allocate` 立即弹出复用，本层不做在途引用保护。
/// 快照代际安全的回收由 [`crate::PageMapper`] 的 retired 队列负责：
/// 只有 `reclaim_retired` 判定安全后才调用这里的 `free`。
///
/// 空闲页按**半开区间**记账而非逐页展开：`max_fp_id` 的量纲是
/// `file_id << pages_per_file_shift`（默认每文件 2^18 页），reopen 时
/// `[0, max_fp_id)` 里「从未分配过」的尾段可能达到数亿个页位。逐页入链会
/// 让一次 reopen 分配 GB 级内存；区间栈只记录真实空洞，成本降到
/// O(空洞数 + 区间数)。
///
/// 栈序即 LIFO：
/// - reopen 重建按空闲页升序压入区间，`allocate` 从栈顶区间高端取页，等价
///   于旧实现逐页 `push` 后从全局最高空闲页递减；
/// - `free` 只在栈顶区间高端相邻时合并，弹出顺序与逐页 `push` 完全一致
///   （最近释放的页总是下一个被分配）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePagePool {
  free_ranges: Vec<FreeRange>,
  max_fp_id: FilePageId,
}

impl FilePagePool {
  pub fn new(max_fp_id: FilePageId) -> Self {
    Self {
      free_ranges: Vec::new(),
      max_fp_id,
    }
  }

  pub fn allocate(&mut self) -> FilePageId {
    if let Some(range) = self.free_ranges.last_mut() {
      debug_assert!(range.start < range.end, "空闲区间不得为空");
      range.end -= 1;
      let id = range.end;
      if range.start == range.end {
        self.free_ranges.pop();
      }
      return id;
    }
    let id = self.max_fp_id;
    self.max_fp_id += 1;
    id
  }

  pub fn free(&mut self, file_page_id: FilePageId) {
    self.free_range(file_page_id, file_page_id + 1);
  }

  /// 压入半开空闲区间 `[start, end)`。
  ///
  /// 只在栈顶区间高端相邻时合并（`last.end == start`），不向低端扩展：低端
  /// 合并会把更早的空闲页排到栈顶，破坏 LIFO 弹出顺序。调用方需按升序压入
  /// 一次重建的多个区间。
  pub(crate) fn free_range(&mut self, start: FilePageId, end: FilePageId) {
    debug_assert!(start < end, "空闲区间不得为空");
    if let Some(last) = self.free_ranges.last_mut()
      && last.end == start
    {
      last.end = end;
      return;
    }
    self.free_ranges.push(FreeRange { start, end });
  }

  pub fn max_fp_id(&self) -> FilePageId {
    self.max_fp_id
  }

  pub fn reserve_until(&mut self, file_page_id: FilePageId) {
    self.max_fp_id = self.max_fp_id.max(file_page_id);
    self.truncate_below(file_page_id);
  }

  /// 丢弃低于 `floor` 的空闲页区间（sticky 复用下界，pooled-07）。
  ///
  /// 与 [`Self::reserve_until`] 的区别：这里不抬高高水位，只裁剪可复用集合。
  /// branch / archive manifest 引用的物理页低于下界，回池复用会静默覆盖快照。
  pub(crate) fn truncate_below(&mut self, floor: FilePageId) {
    self.free_ranges.retain_mut(|range| {
      if range.end <= floor {
        return false;
      }
      range.start = range.start.max(floor);
      true
    });
  }

  /// 逻辑空闲页数（区间长度之和，不展开存储）。
  pub fn free_len(&self) -> u64 {
    self.free_ranges.iter().map(FreeRange::len).sum()
  }

  /// 空闲区间条目数（reopen 重建成本 / 内存上界的诊断口径）。
  pub fn free_range_cnt(&self) -> usize {
    self.free_ranges.len()
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilePageAllocator {
  Append(AppendAllocator),
  Pooled(FilePagePool),
}

impl FilePageAllocator {
  pub fn new(mode: AllocMode, max_fp_id: FilePageId) -> Self {
    match mode {
      AllocMode::Append => Self::Append(AppendAllocator::new(max_fp_id)),
      AllocMode::Pooled => Self::Pooled(FilePagePool::new(max_fp_id)),
    }
  }

  pub fn allocate(&mut self) -> FilePageId {
    match self {
      Self::Append(allocator) => allocator.allocate(),
      Self::Pooled(allocator) => allocator.allocate(),
    }
  }

  pub fn free(&mut self, file_page_id: FilePageId) {
    match self {
      Self::Append(allocator) => allocator.free(file_page_id),
      Self::Pooled(allocator) => allocator.free(file_page_id),
    }
  }

  pub fn max_fp_id(&self) -> FilePageId {
    match self {
      Self::Append(allocator) => allocator.max_fp_id(),
      Self::Pooled(allocator) => allocator.max_fp_id(),
    }
  }

  pub fn reserve_until(&mut self, file_page_id: FilePageId) {
    match self {
      Self::Append(allocator) => allocator.reserve_until(file_page_id),
      Self::Pooled(allocator) => allocator.reserve_until(file_page_id),
    }
  }

  pub fn space_size(&self, pages_per_file_shift: u8) -> u64 {
    match self {
      Self::Append(allocator) => allocator.space_size(pages_per_file_shift),
      // Pooled 的空闲链是离散页，不存在连续空文件；按 max_fp_id 近似。
      Self::Pooled(allocator) => allocator.max_fp_id(),
    }
  }

  pub fn update_stat(&mut self, min_file_id: FileId, empty_file_cnt: u32) {
    if let Self::Append(allocator) = self {
      allocator.update_stat(min_file_id, empty_file_cnt);
    }
  }

  pub fn skip_to_next_file(&mut self, pages_per_file_shift: u8) {
    if let Self::Append(allocator) = self {
      allocator.skip_to_next_file(pages_per_file_shift);
    }
  }

  /// 最老非空文件；Pooled 无对照语义，返回 0。
  pub fn min_file_id(&self) -> FileId {
    match self {
      Self::Append(allocator) => allocator.min_file_id(),
      Self::Pooled(_) => 0,
    }
  }
}
