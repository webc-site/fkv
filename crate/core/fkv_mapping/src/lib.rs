//! 逻辑页 → 物理页映射与文件页分配器。
//!
//! 值格式：
//! 低 3 位为 `ValType`，高位为 `FilePageId` / `PageId`。

use std::{
  cell::{Cell, RefCell},
  mem::take,
  rc::{Rc, Weak},
  sync::{
    Arc, LazyLock,
    atomic::{AtomicU64, Ordering as AtomicOrdering},
  },
};

use fkv_codec::{append_varint64, get_varint64};
use fkv_error::{KvError, Result};
use fkv_types::{
  FREE_PAGE_NONE, FileId, FilePageId, INVALID_VALUE, PageId, TableIdent, ValType, decode_id,
  encode_file_page_id, encode_page_id, is_file_page_id, val_type,
};
use gxhash::HashSet;

/// 槽位是否可回收进逻辑页空闲链表：非活跃页（`Invalid`）或空闲链表节点（`PageId`）。
///
/// C++ 按 `!IsFilePageId` 判定；Rust 目前没有 swizzling 写入路径，这里额外排除
/// `SwizzlingPointer`，避免将来 swizzle 落地后误回收仍然活跃的页。
fn is_free_slot(value: u64) -> bool {
  !is_file_page_id(value) && val_type(value) != ValType::SwizzlingPointer
}

/// 分块大小：对齐 C++ `MappingTbl` 的 `kChunkShift=9`（512 个 slot/chunk）。
pub(crate) const CHUNK_SHIFT: u32 = 9;
pub const CHUNK_SIZE: usize = 1 << CHUNK_SHIFT;

/// 映射代际来源：进程内单调递增，任何 mapper 化身 / 任何一次映射变更都取新值。
///
/// 代际是索引缓存的键，语义必须是「同一个值 ⇒ 同一份映射」。旧实现用
/// 「本实例内 +1、abort 回退、重建归 0」，同一数值会在不同时间对应不同映射：
/// 批次回滚后新批次又走到同一个数值，在途旧快照回填的节点于是可能命中新读
/// 者（r7a-core-01/02）。改成全局取值后，回滚只让映射回到旧内容、代际继续
/// 前进，被回滚的取值永不复用，「同值 ⇒ 同映射」在进程内恒成立。
static GENERATION_SOURCE: AtomicU64 = AtomicU64::new(1);

/// 取一个从未使用过的映射代际。
///
/// 单线程 shard 模型下取值不承担同步语义，`Relaxed` 即可：只需要唯一性，
/// 由 `fetch_add` 保证。u64 回绕在物理上不可达。
fn next_generation() -> u64 {
  GENERATION_SOURCE.fetch_add(1, AtomicOrdering::Relaxed)
}

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
  static CHUNK_DIR_COPY_NUM: Cell<u64> = const { Cell::new(0) };
}

/// 从线程池取一块缓冲（已清空，容量 ≥ `CHUNK_SIZE`）；池空则新分配。
fn take_chunk() -> Vec<u64> {
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
fn give_chunk(value_li: Vec<u64>) {
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
struct Chunk {
  value_li: Vec<u64>,
}

impl Chunk {
  /// 以 `value` 填满的新块。
  fn filled(value: u64) -> Self {
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

/// 逻辑页 → 编码值的分块表。chunk 指针表整体用 `Arc` 共享：`clone` 只递增一次
/// 引用计数（O(1)，不复制指针数组），修改时对指针表与受影响 chunk 做写时复制
/// （`Arc::make_mut`，为 COW 快照服务）；释放的 chunk 归还线程内池复用。
#[derive(Debug, Clone, Default)]
pub struct MappingTbl {
  chunk_li: Arc<Vec<Arc<Chunk>>>,
  len: usize,
  /// 活跃映射数（`ValType::FilePageId` 槽位数），对齐 C++ `MappingCount()`。
  /// 随 `set` / `push` / `resize` 增量维护，读取 O(1)。
  live_cnt: usize,
}

impl PartialEq for MappingTbl {
  fn eq(&self, other: &Self) -> bool {
    self.len == other.len && self.iter_value_li().eq(other.iter_value_li())
  }
}

impl Eq for MappingTbl {}

impl MappingTbl {
  pub fn new() -> Self {
    Self::default()
  }

  /// 可变 chunk 指针表：被快照 / clone 共享时先整体复制指针数组（写时复制），
  /// 保证旧快照仍看到取出时刻的 chunk 集合。
  fn chunk_li_mut(&mut self) -> &mut Vec<Arc<Chunk>> {
    if Arc::strong_count(&self.chunk_li) > 1 {
      let _ = CHUNK_DIR_COPY_NUM.try_with(|num| num.set(num.get() + 1));
    }
    Arc::make_mut(&mut self.chunk_li)
  }

  /// chunk 指针表当前共享份数（含自身）；测试 / 诊断用。
  pub fn chunk_dir_share_cnt(&self) -> usize {
    Arc::strong_count(&self.chunk_li)
  }

  pub fn len(&self) -> usize {
    self.len
  }

  /// 活跃映射数，对齐 C++ `MappingCount()`：只计指向物理页的槽位，
  /// 不含空闲逻辑页（`PageId` 链节）与 `Invalid` 槽位。
  pub fn mapping_count(&self) -> usize {
    self.live_cnt
  }

  pub fn is_empty(&self) -> bool {
    self.len == 0
  }

  pub fn get(&self, page_id: PageId) -> Result<u64> {
    let index = page_id as usize;
    if index >= self.len {
      return Err(KvError::InvalidArgs);
    }
    // 每个 chunk 恒为 `CHUNK_SIZE` 长（`push`/`resize`/`deserialize` 都填满），
    // 上界已检查，这里直接索引，省掉一次 `Option` 分支。
    let chunk = &self.chunk_li[index >> CHUNK_SHIFT];
    debug_assert_eq!(chunk.value_li.len(), CHUNK_SIZE);
    Ok(chunk.value_li[index & (CHUNK_SIZE - 1)])
  }

  /// 逻辑页当前映射的物理页；非活跃槽位（空闲链表节点 / `Invalid`）报缺失。
  pub fn file_page_id(&self, page_id: PageId) -> Result<FilePageId> {
    let value = self.get(page_id)?;
    if !is_file_page_id(value) {
      return Err(KvError::ResourceMissing);
    }
    Ok(decode_id(value))
  }

  /// 写入一个槽位；越界自动按 chunk 扩容。没有失败路径，故不返回 `Result`。
  pub fn set(&mut self, page_id: PageId, value: u64) {
    let index = page_id as usize;
    if index >= self.len {
      self.resize(index + 1, INVALID_VALUE);
    }
    // 先写槽位并取旧值是否活跃，结束对 chunk_li 的借用后再改 live_cnt
    // （`chunk_li_mut` 借用整个 self，不能与字段写入同时存活）。
    let was_live = {
      let chunk_li = self.chunk_li_mut();
      let slot =
        &mut Arc::make_mut(&mut chunk_li[index >> CHUNK_SHIFT]).value_li[index & (CHUNK_SIZE - 1)];
      let was_live = is_file_page_id(*slot);
      *slot = value;
      was_live
    };
    let is_live = is_file_page_id(value);
    if was_live != is_live {
      if is_live {
        self.live_cnt += 1;
      } else {
        self.live_cnt -= 1;
      }
    }
  }

  pub fn push(&mut self, value: u64) -> PageId {
    let page_id = self.len as PageId;
    let index = self.len;
    let chunk_idx = index >> CHUNK_SHIFT;
    let chunk_li = self.chunk_li_mut();
    if chunk_idx == chunk_li.len() {
      chunk_li.push(Arc::new(Chunk::filled(INVALID_VALUE)));
    }
    Arc::make_mut(&mut chunk_li[chunk_idx]).value_li[index & (CHUNK_SIZE - 1)] = value;
    if is_file_page_id(value) {
      self.live_cnt += 1;
    }
    self.len += 1;
    page_id
  }

  pub fn resize(&mut self, len: usize, value: u64) {
    if len <= self.len {
      if len < self.len {
        // 截断区间内的活跃槽位要从计数里扣掉（`abort_batch` 回滚尾部页）。
        let removed_live = self
          .iter_value_li()
          .skip(len)
          .filter(|value| is_file_page_id(*value))
          .count();
        self.live_cnt -= removed_live;
      }
      // chunk 保持满长（含 len 之后的陈旧 slot），只收缩 chunk 数与逻辑长度。
      self.chunk_li_mut().truncate(len.div_ceil(CHUNK_SIZE));
      self.len = len;
      return;
    }
    let chunk_num = len.div_ceil(CHUNK_SIZE);
    let old_len = self.len;
    let chunk_li = self.chunk_li_mut();
    while chunk_li.len() < chunk_num {
      chunk_li.push(Arc::new(Chunk::filled(INVALID_VALUE)));
    }
    // 填充新增区间 [old_len, len) 为 value。
    let mut index = old_len;
    while index < len {
      let chunk_idx = index >> CHUNK_SHIFT;
      let offset = index & (CHUNK_SIZE - 1);
      let end = (offset + (len - index)).min(CHUNK_SIZE);
      Arc::make_mut(&mut chunk_li[chunk_idx]).value_li[offset..end].fill(value);
      index += end - offset;
    }
    if is_file_page_id(value) {
      self.live_cnt += len - old_len;
    }
    self.len = len;
  }

  /// 按 PageId 升序遍历编码值。
  pub fn iter_value_li(&self) -> impl Iterator<Item = u64> + '_ {
    self
      .chunk_li
      .iter()
      .flat_map(|chunk| chunk.value_li.iter().copied())
      .take(self.len)
  }

  /// 按 `(PageId, value)` 升序遍历。
  pub fn iter(&self) -> impl Iterator<Item = (PageId, u64)> + '_ {
    self
      .iter_value_li()
      .enumerate()
      .map(|(index, value)| (index as PageId, value))
  }

  pub fn serialize(&self) -> Vec<u8> {
    let mut out = Vec::with_capacity(self.len * 5);
    for value in self.iter_value_li() {
      append_varint64(&mut out, value);
    }
    out
  }

  pub fn deserialize(data: &[u8]) -> Result<Self> {
    // 直接按 chunk 边解码边填充：省掉整表中间 `Vec<u64>`，也避免对满 chunk
    // 先 `vec![INVALID; CHUNK_SIZE]` 再整块覆盖的冗余零填。
    let mut chunk_li: Vec<Arc<Chunk>> = Vec::new();
    let mut value_li = take_chunk();
    let mut len = 0usize;
    let mut live_cnt = 0usize;
    let mut pos = 0usize;
    while pos < data.len() {
      let (value, read) = get_varint64(&data[pos..])?;
      if is_file_page_id(value) {
        live_cnt += 1;
      }
      value_li.push(value);
      len += 1;
      pos += read;
      if value_li.len() == CHUNK_SIZE {
        chunk_li.push(Arc::new(Chunk {
          value_li: take(&mut value_li),
        }));
        value_li = take_chunk();
      }
    }
    // 末尾不足一个 chunk：补 `INVALID` 到定长，与 C++ `MappingTbl` 一致。
    if !value_li.is_empty() {
      value_li.resize(CHUNK_SIZE, INVALID_VALUE);
      chunk_li.push(Arc::new(Chunk { value_li }));
    }
    Ok(Self {
      chunk_li: Arc::new(chunk_li),
      len,
      live_cnt,
    })
  }

  /// 由活跃槽位反推文件级空间统计，对齐 C++ `Replayer`（由 `using_fp_ids` 反推
  /// `min_file_id_` / `empty_file_cnt_`），让 `space_size` 在 reopen 后即可用。
  ///
  /// 返回 `(最老非空 file id, [min_file, max_live_file] 之间的空文件数, 最大 live fp id)`；
  /// 空表返回 `(None, 0, None)`。只统计 `is_file_page_id` 的活跃页。
  ///
  /// 第一遍只求 `min_file` / `max_file` / `max_fp_id`，第二遍用位图标记占用，
  /// 内存为 `O(文件数/64)`（而非 `O(活跃页数)` 或每文件 1 字节）。
  pub fn file_stat(&self, pages_per_file_shift: u8) -> (Option<FileId>, u32, Option<FilePageId>) {
    let mut min_file = FileId::MAX;
    let mut max_file = 0u64;
    let mut max_fp_id = 0u64;
    let mut has_live = false;
    for value in self.iter_value_li() {
      if !is_file_page_id(value) {
        continue;
      }
      has_live = true;
      let fp_id = decode_id(value);
      max_fp_id = max_fp_id.max(fp_id);
      let file_id = fp_id >> pages_per_file_shift;
      min_file = min_file.min(file_id);
      max_file = max_file.max(file_id);
    }
    if !has_live {
      return (None, 0, None);
    }
    let file_cnt = (max_file - min_file + 1) as usize;
    let word_bits = u64::BITS as usize;
    let mut bits = vec![0u64; file_cnt.div_ceil(word_bits)];
    for value in self.iter_value_li() {
      if is_file_page_id(value) {
        let slot = ((decode_id(value) >> pages_per_file_shift) - min_file) as usize;
        bits[slot / word_bits] |= 1 << (slot % word_bits);
      }
    }
    let occupied_cnt: u32 = bits.iter().map(|word| word.count_ones()).sum();
    (
      Some(min_file),
      file_cnt as u32 - occupied_cnt,
      Some(max_fp_id),
    )
  }
}

/// 物理页分配策略，对齐 C++ `PageManager` 的 append / pooled 两种模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocMode {
  /// 只追加：`free` 为 no-op，旧物理页永不复用，COW 天然安全。
  Append,
  /// 池化复用：释放的物理页入 LIFO 空闲链，下次 `allocate` 立即弹出。
  ///
  /// **不变量：只有「单写者 + 无在途读者」时才能启用。** `PageMapper`
  /// 目前没有快照引用计数、没有 retired 队列，无法判断某个物理页是否仍被
  /// in-flight 读者持有（C++ 靠 `MappingSnapshot::to_free_file_pages_` +
  /// `next_snapshot_` 代际链保证）。接入并发读或启用 Pooled 写路径前，
  /// 必须先实现等价机制，否则读者按旧 `FilePageId` 会读到已被重分配的页。
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

/// Pooled 物理页空闲链（LIFO）。
///
/// `free` 立即入链、`allocate` 立即弹出复用，**不做任何在途引用保护**；
/// 调用方须保证释放时已无读者持有该 `FilePageId`。语义与约束见
/// [`AllocMode::Pooled`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePagePool {
  free_li: Vec<FilePageId>,
  max_fp_id: FilePageId,
}

impl FilePagePool {
  pub fn new(max_fp_id: FilePageId) -> Self {
    Self {
      free_li: Vec::new(),
      max_fp_id,
    }
  }

  pub fn allocate(&mut self) -> FilePageId {
    if let Some(id) = self.free_li.pop() {
      return id;
    }
    let id = self.max_fp_id;
    self.max_fp_id += 1;
    id
  }

  pub fn free(&mut self, file_page_id: FilePageId) {
    self.free_li.push(file_page_id);
  }

  pub fn max_fp_id(&self) -> FilePageId {
    self.max_fp_id
  }

  pub fn reserve_until(&mut self, file_page_id: FilePageId) {
    self.max_fp_id = self.max_fp_id.max(file_page_id);
    self.free_li.retain(|id| *id >= file_page_id);
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

/// 一次写批次的可回滚日志（OOM / 失败时 Abort）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct BatchJournal {
  change_li: Vec<(PageId, u64)>,
  old_len: usize,
  allocator: FilePageAllocator,
  free_page_head: PageId,
  free_page_cnt: u32,
  dirty_len: usize,
  /// 回滚时截断 `changed_page_li`：批次失败后这些逻辑页实际未变，
  /// 留给上层会在下一批被当成变更而误逐索引缓存（eng-68）。
  changed_len: usize,
}

/// 映射器：`tbl` + allocator + 批次记账。
///
/// 不实现 `PartialEq`：`generation` 是进程级单调代际（同名内容每次化身都不同），
/// 结构相等既无意义也无调用点；需要比对映射内容时直接比 `tbl()`。
#[derive(Debug, Clone)]
pub struct PageMapper {
  tbl: MappingTbl,
  allocator: FilePageAllocator,
  /// 空闲逻辑页链表头（`FREE_PAGE_NONE` 表示无空闲页）。
  ///
  /// 链节原位复用 `tbl` 槽位存 `encode_page_id(next)`，零额外内存分配，
  /// 对齐 C++ `PageMapper::free_page_head_`。
  free_page_head: PageId,
  /// 空闲逻辑页数量，恒等于链表长度；供 `BatchJournal` 回滚与诊断。
  free_page_cnt: u32,
  dirty_li: Vec<PageId>,
  /// 内容已变更（或释放）的既有逻辑 PageId，供上层失效索引缓存（eng-68）。
  changed_page_li: Vec<PageId>,
  /// 映射代际：任何可能改变活跃映射的操作递增一次。快照在创建时记录该值，
  /// 索引缓存项按它区分「旧快照节点」与「新代节点」（27b）。
  generation: u64,
  journal: Option<BatchJournal>,
}

impl PageMapper {
  pub fn new(mode: AllocMode, max_fp_id: FilePageId) -> Self {
    Self::empty(MappingTbl::new(), mode, max_fp_id)
  }

  /// 用已落盘的 `tbl` 重建 mapper（reopen / branch 恢复路径）。
  ///
  /// 契约：`tbl` 必须是本 mapper 独占的 owned 值——重建会让非活跃槽位原地串成
  /// 空闲链表（对齐 C++ `Replayer::GetMapper`），不能与其他快照 clone 共享。
  pub fn with_tbl(tbl: MappingTbl, mode: AllocMode, max_fp_id: FilePageId) -> Self {
    let mut mapper = Self::empty(tbl, mode, max_fp_id);
    mapper.rebuild_free_page_li();
    mapper.rebuild_pooled_free_file_page_li();
    mapper
  }

  /// 基本构造：不重建空闲链表（`new` 的 `max_fp_id` 语义是「从此开始分配」）。
  fn empty(tbl: MappingTbl, mode: AllocMode, max_fp_id: FilePageId) -> Self {
    Self {
      tbl,
      allocator: FilePageAllocator::new(mode, max_fp_id),
      free_page_head: FREE_PAGE_NONE,
      free_page_cnt: 0,
      dirty_li: Vec::new(),
      changed_page_li: Vec::new(),
      // 每个化身都取全新代际：重建后的 mapper 与旧化身的缓存键不重叠，
      // 在途旧快照回填的节点不会被新化身读者命中（r7a-core-02）。
      generation: next_generation(),
      journal: None,
    }
  }

  /// 只读视图：用给定 `tbl` 构造，不重建空闲链表。
  ///
  /// 供快照持有 segment 侧的只读映射（`fkv_large` 的读接口仍收 `&PageMapper`）。
  /// 视图上的写接口语义未定义，调用方不得使用；底层 chunk 与来源 COW 共享。
  pub fn read_view(tbl: MappingTbl, max_fp_id: FilePageId) -> Self {
    Self::empty(tbl, AllocMode::Append, max_fp_id)
  }

  /// 映射代际：任何可能改变活跃映射的操作递增一次。
  ///
  /// 快照创建时记录该值；索引缓存项按它区分不同代节点（27b）。同一代内
  /// 未变更的映射可安全复用缓存；取值进程内单调且不复用（见
  /// `GENERATION_SOURCE`），因此「同代 ⇒ 同映射」恒成立。
  pub fn generation(&self) -> u64 {
    self.generation
  }

  #[inline]
  fn bump_generation(&mut self) {
    self.generation = next_generation();
  }

  /// 重建逻辑页空闲链表：`tbl` 中所有非活跃槽位按升序重新串链。
  fn rebuild_free_page_li(&mut self) {
    let free_li: Vec<PageId> = self
      .tbl
      .iter()
      .filter_map(|(page_id, value)| is_free_slot(value).then_some(page_id))
      .collect();
    let mut head = FREE_PAGE_NONE;
    let mut cnt = 0u32;
    for page_id in free_li {
      self.tbl.set(page_id, encode_page_id(head));
      head = page_id;
      cnt += 1;
    }
    self.free_page_head = head;
    self.free_page_cnt = cnt;
  }

  /// Pooled 模式 reopen：把 `0..max_fp_id` 中未被引用的物理页填回空闲池。
  ///
  /// 对齐 C++ `Replayer::GetMapper` 扫描 `using_fp_ids_set` 之外 id 的做法。
  /// Append 模式 `free` 为 no-op，无需恢复；Pooled 当前仅 core 测试使用。
  fn rebuild_pooled_free_file_page_li(&mut self) {
    let FilePageAllocator::Pooled(pool) = &mut self.allocator else {
      return;
    };
    let max_fp_id = pool.max_fp_id();
    let mut used_li: Vec<FilePageId> = self
      .tbl
      .iter_value_li()
      .filter(|value| is_file_page_id(*value))
      .map(decode_id)
      .collect();
    used_li.sort_unstable();
    used_li.dedup();
    let mut next = 0;
    for used in used_li {
      for id in next..used {
        pool.free(id);
      }
      next = used + 1;
    }
    for id in next..max_fp_id {
      pool.free(id);
    }
  }

  /// 开始一个可回滚的写批次；嵌套调用忽略（最外层负责提交/回滚）。
  ///
  /// 返回 `true` 表示本次真正新建了日志，调用方负责 `commit_batch` /
  /// `abort_batch`；`false` 说明已有外层批次，由外层结算。
  pub fn begin_batch(&mut self) -> bool {
    if self.journal.is_some() {
      return false;
    }
    self.journal = Some(BatchJournal {
      change_li: Vec::new(),
      old_len: self.tbl.len(),
      allocator: self.allocator.clone(),
      free_page_head: self.free_page_head,
      free_page_cnt: self.free_page_cnt,
      dirty_len: self.dirty_li.len(),
      changed_len: self.changed_page_li.len(),
    });
    true
  }

  /// 提交：丢弃回滚日志。
  pub fn commit_batch(&mut self) {
    self.journal = None;
  }

  /// 回滚到 `begin_batch` 时的状态；返回是否有活跃日志。
  pub fn abort_batch(&mut self) -> bool {
    let Some(journal) = self.journal.take() else {
      return false;
    };
    for (page_id, value) in journal.change_li.into_iter().rev() {
      self.tbl.set(page_id, value);
    }
    self.tbl.resize(journal.old_len, INVALID_VALUE);
    self.allocator = journal.allocator;
    self.free_page_head = journal.free_page_head;
    self.free_page_cnt = journal.free_page_cnt;
    self.dirty_li.truncate(journal.dirty_len);
    self.changed_page_li.truncate(journal.changed_len);
    // 映射内容回到批次前，但代际必须继续前进：回退到旧值会让「同代 ⇒ 同映射」
    // 失效（本批写入过的代际会被下一批重新取到），在途旧快照回填的索引节点
    // 就能在之后被当作本代条目命中（r7a-core-01）。回退后的映射是一份新的
    // 状态，取新代际后旧快照（代际更小）一律被缓存拒绝。
    self.bump_generation();
    true
  }

  fn journal_change(&mut self, page_id: PageId) {
    let Some(journal) = &mut self.journal else {
      return;
    };
    if let Ok(value) = self.tbl.get(page_id) {
      journal.change_li.push((page_id, value));
    }
  }

  pub fn tbl(&self) -> &MappingTbl {
    &self.tbl
  }

  /// 活跃映射数，对齐 C++ `PageMapper::MappingCount()`。
  pub fn mapping_count(&self) -> usize {
    self.tbl.mapping_count()
  }

  /// 尚未写入 manifest 的脏页（只读借用；写盘成功后再 `clear_dirty_page_li`）。
  ///
  /// 不能用 `take` 提前清空：manifest 写失败需要保留脏页供重试，且
  /// `abort_batch` 依赖 `dirty_li` 与 journal 的长度关系回滚。
  pub fn dirty_page_li(&self) -> &[PageId] {
    &self.dirty_li
  }

  /// manifest 落盘成功后清空脏页记录（保留堆容量）。
  pub fn clear_dirty_page_li(&mut self) {
    self.dirty_li.clear();
  }

  /// 取出内容变更过的逻辑 PageId 并清空（用于精确失效索引缓存）。
  pub fn take_changed_page_li(&mut self) -> Vec<PageId> {
    take(&mut self.changed_page_li)
  }

  /// 尚未被上层取走的内容变更记录（诊断：跨批次累积检查）。
  pub fn changed_page_li(&self) -> &[PageId] {
    &self.changed_page_li
  }

  /// 丢弃本批变更记录（保留堆容量）。
  ///
  /// 只适用于「逻辑页内容未变」的批次：`compact` 仅换物理 `FilePageId`，
  /// `truncate` 末尾整表清空缓存。若留到下一批，`take_changed_page_li`
  /// 会把陈旧条目当本批变更成批逐出热点索引页（eng-68）。
  pub fn clear_changed_page_li(&mut self) {
    self.changed_page_li.clear();
  }

  /// 分配逻辑页：优先复用空闲链表头（对齐 C++ `GetPage`），链表为空才追加新槽位。
  pub fn allocate_page(&mut self) -> PageId {
    let file_page_id = self.allocator.allocate();
    let page_id = if self.free_page_cnt == 0 {
      self.tbl.push(encode_file_page_id(file_page_id))
    } else {
      // 复用链表头：槽位里存的是下一个空闲页（`FREE_PAGE_NONE` 结尾）。
      let page_id = self.free_page_head;
      debug_assert!((page_id as usize) < self.tbl.len(), "空闲链表节点越界");
      self.journal_change(page_id);
      // 链节由 `encode_page_id` 写入，id 必在 u32 内，这里 `as` 无损。
      let next = self
        .tbl
        .get(page_id)
        .map_or(FREE_PAGE_NONE, |value| decode_id(value) as PageId);
      self.free_page_head = next;
      self.free_page_cnt -= 1;
      self.tbl.set(page_id, encode_file_page_id(file_page_id));
      page_id
    };
    self.dirty_li.push(page_id);
    self.bump_generation();
    page_id
  }

  /// 逻辑 PageId 不变，换一个新的 FilePageId（COW）。
  pub fn remap_page(&mut self, page_id: PageId) -> Result<FilePageId> {
    self.journal_change(page_id);
    let old = self.tbl.get(page_id)?;
    debug_assert!(
      val_type(old) != ValType::PageId,
      "remap_page 只能作用于活跃页，空闲链表节点不可 remap"
    );
    // 校验通过后才记录：失败调用不留幽灵 PageId 给索引缓存失效逻辑。
    self.changed_page_li.push(page_id);
    // 先 allocate 再 free：Pooled 的 free list 是 LIFO，先 free 会让
    // allocate 立刻取回同一物理页，COW 退化成原地覆盖。
    let file_page_id = self.allocator.allocate();
    if is_file_page_id(old) {
      let old_file_page_id = decode_id(old);
      self.allocator.free(old_file_page_id);
    }
    self.tbl.set(page_id, encode_file_page_id(file_page_id));
    self.dirty_li.push(page_id);
    self.bump_generation();
    Ok(file_page_id)
  }

  /// 直接改写逻辑页到物理页的映射（reopen / 快照安装用，无失败路径）。
  pub fn set_file_page(&mut self, page_id: PageId, file_page_id: FilePageId) {
    self.journal_change(page_id);
    self.changed_page_li.push(page_id);
    self.tbl.set(page_id, encode_file_page_id(file_page_id));
    self.dirty_li.push(page_id);
    self.bump_generation();
  }

  pub fn file_page_id(&self, page_id: PageId) -> Result<FilePageId> {
    let value = self.tbl.get(page_id)?;
    if !is_file_page_id(value) {
      return Err(KvError::ResourceMissing);
    }
    Ok(decode_id(value))
  }

  /// 释放逻辑页：物理页交回分配器，槽位原位串入空闲链表（幂等）。
  pub fn free_page(&mut self, page_id: PageId) -> Result<Option<FilePageId>> {
    self.journal_change(page_id);
    let value = self.tbl.get(page_id)?;
    if !is_file_page_id(value) {
      // 已是空闲节点（`PageId`）或无效槽位：不重复入链，也不算内容变更。
      return Ok(None);
    }
    // 判定为真实释放后才记录：重复 free 不进 `changed_page_li`。
    self.changed_page_li.push(page_id);
    let file_page_id = decode_id(value);
    self.allocator.free(file_page_id);
    self.tbl.set(page_id, encode_page_id(self.free_page_head));
    self.free_page_head = page_id;
    self.free_page_cnt += 1;
    self.dirty_li.push(page_id);
    self.bump_generation();
    Ok(Some(file_page_id))
  }

  /// 空闲逻辑页数量，恒等于空闲链表长度。
  pub fn free_page_cnt(&self) -> u32 {
    self.free_page_cnt
  }

  /// 逻辑页槽位数（含空闲槽）。互异逻辑页的访问序列长度不会超过此值，
  /// 供扫描等按逻辑页推进的读路径做链环检测上界。
  pub fn page_cnt(&self) -> usize {
    self.tbl.len()
  }

  pub fn max_fp_id(&self) -> FilePageId {
    self.allocator.max_fp_id()
  }

  /// 有效页空间（扣除 `[min_file_id, min_file_id+empty_file_cnt)` 的空文件）。
  pub fn space_size(&self, pages_per_file_shift: u8) -> u64 {
    self.allocator.space_size(pages_per_file_shift)
  }

  /// 扫一遍当前映射反推 `min_file_id` / `empty_file_cnt`（对齐 C++ `Replayer`），
  /// 供 reopen / GC 轮次结束后回写。仅 `AllocMode::Append` 有效。
  ///
  /// 空映射对齐 C++ 空 reopen：`min_file_id = max_fp_id >> shift`、
  /// `empty_file_cnt = 0`，`space_size` 只剩尾文件残量（对齐时为 0），
  /// 避免 reopen 后按文件 0 起算把整段历史空间算成放大。
  ///
  /// 不推进 `max_fp_id`：空尾 snap-up 需与盘上文件清理时序绑定，另行处理；
  /// 派生值回退（小于已记）时整体忽略，避免 min 与 empty 计数错配。
  pub fn update_stat_by_scan(&mut self, pages_per_file_shift: u8) {
    let (Some(min_file), empty_file_cnt, _) = self.tbl.file_stat(pages_per_file_shift) else {
      let tail_file = self.allocator.max_fp_id() >> pages_per_file_shift;
      if tail_file >= self.allocator.min_file_id() {
        self.allocator.update_stat(tail_file, 0);
      }
      return;
    };
    if min_file < self.allocator.min_file_id() {
      return;
    }
    self.allocator.update_stat(min_file, empty_file_cnt);
  }

  /// 全空映射时跳过已死空间（见 [`AppendAllocator::skip_to_next_file`]）。
  pub fn skip_empty_space(&mut self, pages_per_file_shift: u8) {
    self.allocator.skip_to_next_file(pages_per_file_shift);
  }

  pub fn min_file_id(&self) -> FileId {
    self.allocator.min_file_id()
  }

  /// 跳过一段 file page id 区间（branch 预留）。
  pub fn reserve_until(&mut self, file_page_id: FilePageId) {
    self.allocator.reserve_until(file_page_id);
  }

  pub fn serialize(&self) -> Vec<u8> {
    self.tbl.serialize()
  }
}

/// 把一个 mapping 中的活跃 `FilePageId` 并入保留集合。
///
/// GC 的 retained set 由 manifest、当前 mapper 与活跃快照三部分并集而成；
/// 本函数是三处共用的一份实现，避免「哪些槽位算活跃」的判定漂移。
pub fn add_mapping_retained(mapping: &MappingTbl, file_page_set: &mut HashSet<FilePageId>) {
  for value in mapping.iter_value_li() {
    if is_file_page_id(value) {
      file_page_set.insert(decode_id(value));
    }
  }
}

/// 无 segment mapper 的共享空表，避免每份快照各建一个空 `MappingTbl`。
///
/// `chunk_li` 为 `Arc`，无法 `const` 构造，用 `LazyLock` 延迟初始化一次。
static EMPTY_MAPPING_TBL: LazyLock<MappingTbl> = LazyLock::new(MappingTbl::new);

/// 一份读请求可跨 `await` 持有的映射快照。
///
/// data / segment 两侧的 [`MappingTbl`] 通过 `Arc` 共享 chunk 指针表与 chunk：
/// 创建快照 O(1)（只递增引用计数 + 少量元数据），不复制指针数组、也不复制
/// 4 KiB 块；写入路径继续原地改 `PageMapper`，只在共享 chunk 上触发
/// `Arc::make_mut` 拷贝。
///
/// 快照存活期间，它引用的全部 `FilePageId` 必须出现在 GC retained set 里，
/// 否则旧文件会在读者读完之前被删除。登记 / 摘链由 [`SnapshotRegistry`] 负责；
/// `MappingSnapshot` 的 `clone` 共享同一份快照与同一个代际号。
#[derive(Debug, Clone)]
pub struct MappingSnapshot {
  core: Rc<SnapshotCore>,
}

#[derive(Debug)]
struct SnapshotCore {
  /// 创建顺序号；同一份快照的 clone 共享同一个值。
  generation: u64,
  /// 取出时的映射代际，索引缓存按它拒绝新代节点（27b）。
  mapping_generation: u64,
  root_id: PageId,
  ttl_root_id: PageId,
  data: MappingTbl,
  /// segment 侧只读视图；无 segment mapper 时为 `None`。
  segment: Option<PageMapper>,
  max_fp_id: FilePageId,
  max_segment_fp_id: FilePageId,
}

impl MappingSnapshot {
  /// 创建顺序号；同一份快照的 clone 共享同一个值。
  pub fn generation(&self) -> u64 {
    self.core.generation
  }

  /// 取出时的映射代际；索引缓存 `get` / `insert` 以它为键，
  /// 旧快照不得命中新代节点。
  pub fn mapping_generation(&self) -> u64 {
    self.core.mapping_generation
  }

  pub fn root_id(&self) -> PageId {
    self.core.root_id
  }

  pub fn ttl_root_id(&self) -> PageId {
    self.core.ttl_root_id
  }

  /// data 侧逻辑页 → 物理页映射。
  pub fn data_tbl(&self) -> &MappingTbl {
    &self.core.data
  }

  /// segment 侧逻辑页 → 物理页映射（无 segment mapper 时为空表）。
  pub fn segment_tbl(&self) -> &MappingTbl {
    self
      .core
      .segment
      .as_ref()
      .map_or(&*EMPTY_MAPPING_TBL, PageMapper::tbl)
  }

  /// segment 侧只读映射视图，供 `fkv_large` 读接口使用。
  pub fn segment_mapper(&self) -> Option<&PageMapper> {
    self.core.segment.as_ref()
  }

  pub fn max_fp_id(&self) -> FilePageId {
    self.core.max_fp_id
  }

  pub fn max_segment_fp_id(&self) -> FilePageId {
    self.core.max_segment_fp_id
  }
}

/// 快照来源：Store 在请求入口把当前 mapper / root 交给注册表。
#[derive(Debug, Clone, Copy)]
pub struct SnapshotSource<'a> {
  pub data: &'a PageMapper,
  pub segment: Option<&'a PageMapper>,
  pub root_id: PageId,
  pub ttl_root_id: PageId,
}

/// active mapper 快照登记表（单线程，弱引用，无原子操作）。
///
/// 每份快照以 `(表, Weak<SnapshotCore>)` 登记；快照 `drop` 后弱引用失效，
/// 下一次访问注册表时惰性摘链，不额外实现 `Drop` 回调，也不在 drop 当场重跑 GC。
/// `open` 的登记是增量的：只有条目数达到阈值才整表摘链一次（摊还 O(1)），
/// 避免每次读请求都扫描全部活跃快照。
///
/// **回收语义**：快照释放只让旧页「从下一轮 GC 起变得可回收」，注册表不主动
/// 触发 GC。GC 由 `Store::maintain` / `Store::gc` 的既有节奏驱动；旧文件滞留
/// 到下一轮是预期行为，不是泄漏。
///
/// 无活跃快照时不产生额外映射分配：并集只是一次空表遍历。
///
/// 有快照时 retained 并集按「快照数 × 映射页数」扫描；当前实现优先保证正确性，
/// 尚未做 `max_fp_id` 水线或文件级粗保留优化（见 27a 风险记录）。
#[derive(Debug, Default)]
pub struct SnapshotRegistry {
  /// `(所属表, 快照弱引用)`。按表过滤，避免把别的表的 FilePageId 命名空间
  /// 并进本轮 retained set。
  entry_li: RefCell<Vec<(TableIdent, Weak<SnapshotCore>)>>,
  /// 下次触发惰性摘链的条目数阈值；`open` 达到阈值才整表扫描。
  /// 默认 0：首次 `open` 先摘一次链，随后阈值抬到 [`PRUNE_MIN_LEN`]。
  prune_len: Cell<usize>,
  generation: Cell<u64>,
}

/// 注册表惰性摘链的最小触发长度：低于该长度不扫描，登记摊还 O(1)。
const PRUNE_MIN_LEN: usize = 8;

impl SnapshotRegistry {
  pub fn new() -> Self {
    Self::default()
  }

  /// 登记一份新快照并返回读请求持有的句柄；调用 `drop` 即释放登记。
  pub fn open(&self, tbl: &TableIdent, source: SnapshotSource<'_>) -> MappingSnapshot {
    let generation = self.generation.get() + 1;
    self.generation.set(generation);
    let core = Rc::new(SnapshotCore {
      generation,
      mapping_generation: source.data.generation(),
      root_id: source.root_id,
      ttl_root_id: source.ttl_root_id,
      data: source.data.tbl().clone(),
      segment: source
        .segment
        .map(|mapper| PageMapper::read_view(mapper.tbl().clone(), mapper.max_fp_id())),
      max_fp_id: source.data.max_fp_id(),
      max_segment_fp_id: source.segment.map_or(0, PageMapper::max_fp_id),
    });
    let snapshot = MappingSnapshot { core: core.clone() };
    let mut entry_li = self.entry_li.borrow_mut();
    // 增量登记：条目数达到阈值才整表摘链，摊还 O(1)；死弱引用最多累积到
    // 约两倍存活数。GC retained / 诊断计数仍每次精确摘链。
    if entry_li.len() >= self.prune_len.get() {
      prune_dead(&mut entry_li);
      self.prune_len.set((entry_li.len() * 2).max(PRUNE_MIN_LEN));
    }
    entry_li.push((tbl.clone(), Rc::downgrade(&core)));
    snapshot
  }

  /// 把该表全部活跃快照的 data 侧 file page 并入 retained set。
  pub fn data_retained_into(&self, tbl: &TableIdent, file_page_set: &mut HashSet<FilePageId>) {
    self.retained_into(tbl, file_page_set, |core| &core.data);
  }

  /// 把该表全部活跃快照的 segment 侧 file page 并入 retained set。
  pub fn segment_retained_into(&self, tbl: &TableIdent, file_page_set: &mut HashSet<FilePageId>) {
    self.retained_into(tbl, file_page_set, |core| {
      core
        .segment
        .as_ref()
        .map_or(&*EMPTY_MAPPING_TBL, PageMapper::tbl)
    });
  }

  /// 该表仍被读请求持有的快照数（诊断 / 测试）。
  pub fn live_num(&self, tbl: &TableIdent) -> usize {
    let mut entry_li = self.entry_li.borrow_mut();
    prune_dead(&mut entry_li);
    self.prune_len.set((entry_li.len() * 2).max(PRUNE_MIN_LEN));
    entry_li
      .iter()
      .filter(|(entry_tbl, _)| entry_tbl == tbl)
      .count()
  }

  /// 注册表当前条目数（含尚未摘链的死弱引用）；诊断 / 测试用。
  pub fn entry_num(&self) -> usize {
    self.entry_li.borrow().len()
  }

  fn retained_into(
    &self,
    tbl: &TableIdent,
    file_page_set: &mut HashSet<FilePageId>,
    side: fn(&SnapshotCore) -> &MappingTbl,
  ) {
    let mut entry_li = self.entry_li.borrow_mut();
    // 弱引用失效即快照已 drop：惰性摘链。空表时 retain 不分配、不遍历页。
    prune_dead(&mut entry_li);
    self.prune_len.set((entry_li.len() * 2).max(PRUNE_MIN_LEN));
    for (entry_tbl, weak) in entry_li.iter() {
      if entry_tbl != tbl {
        continue;
      }
      if let Some(core) = weak.upgrade() {
        add_mapping_retained(side(&core), file_page_set);
      }
    }
  }
}

/// 摘掉已 drop 快照的弱引用。
fn prune_dead(entry_li: &mut Vec<(TableIdent, Weak<SnapshotCore>)>) {
  entry_li.retain(|(_, weak)| weak.strong_count() != 0);
}
