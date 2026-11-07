//! 映射器 [`PageMapper`]：逻辑页 → 物理页映射、空闲链表与可回滚批次记账。

use std::{collections::VecDeque, mem::take};

use fkv_error::{KvError, Result};
use fkv_types::{
  BranchFileRange, FREE_PAGE_NONE, FileId, FilePageId, INVALID_VALUE, PageId, ValType, decode_id,
  encode_file_page_id, encode_page_id, is_file_page_id, val_type,
};

use crate::{
  allocator::{AllocMode, FilePageAllocator},
  chunk::is_free_slot,
  generation::GenerationSource,
  mapping_tbl::MappingTbl,
};

/// 一次写批次的可回滚日志（OOM / 失败时 Abort）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BatchJournal {
  change_li: Vec<(PageId, u64)>,
  old_len: usize,
  allocator: FilePageAllocator,
  free_page_head: PageId,
  free_page_cnt: u32,
  dirty_len: usize,
  /// 回滚时截断 `retired_li`：批次内 retire 的物理页在回滚后重新变成活跃映射，
  /// 必须撤销 retire，否则会被后续 `reclaim_retired` 回池复用造成双写。
  retired_len: usize,
  /// 回滚时恢复 `reuse_floor`：下界在批次内抬高会裁剪空闲池，回滚必须把
  /// 池与下界一起还原，否则池里可能残留低于下界的页。
  reuse_floor: FilePageId,
  /// 回滚时截断 `changed_page_li`：批次失败后这些逻辑页实际未变，
  /// 留给上层会在下一批被当成变更而误逐索引缓存（eng-68）。
  changed_len: usize,
  /// 回滚时截断 `freed_file_page_li`：批次内摘除的物理页在回滚后重新被活跃
  /// 映射引用，不能让上层把它们当作已释放页失效写缓存。
  freed_len: usize,
}

/// Pooled 空闲池回填时的 branch 归属过滤（对齐 C++ `FileIdInBranch`）。
///
/// 只有 file id 经区间链解析后属于 `branch_name` 的物理页才能回池。父 branch /
/// 兄弟 branch 名下的空洞页即使不在本映射里，也不能被当前 branch 复用：写入会
/// 按 file id 解析到对方的 `(branch, term)` 命名空间（云端对象 key / 本地文件
/// 变体），静默覆盖其活数据。
///
/// 区间链为空（legacy manifest / 首个 main manifest）时调用方按 C++ 旧语义
/// 全量回填，本结构不参与判断。
#[derive(Debug, Clone, Copy)]
pub struct BranchPageOwner<'a> {
  /// 当前 manifest 的 branch 名（`BranchManifestMetadata::branch_name`）。
  pub branch_name: &'a str,
  /// 单调不减的 `(branch, term)` 文件区间链。
  pub range_li: &'a [BranchFileRange],
  /// `true` 时按 `max_segment_file_id` 解析（segment 文件），否则按 `max_file_id`。
  pub segment: bool,
  /// file id 位移：data 用 `pages_per_file_shift`，segment 用 `segments_per_file_shift`。
  pub shift: u8,
}

/// 空闲链槽位编码：对齐 C++ `PageMapper::FreePage`（page_mapper.cpp:377-387）。
///
/// 链尾写 `INVALID_VALUE`，其余写 `encode_page_id(next)`。C++ 用
/// `InvalidValue` 表示「没有下一个空闲页」；旧 fkv 盘面把链尾写成
/// `encode_page_id(FREE_PAGE_NONE)`，读取侧由 [`next_free_page`] 按类型兜底。
#[inline]
fn encode_free_next(next: PageId) -> u64 {
  if next == FREE_PAGE_NONE {
    INVALID_VALUE
  } else {
    encode_page_id(next)
  }
}

/// 空闲链槽位的下一跳：只有 `PageId` 编码才解码，其余一律视为链尾。
///
/// `INVALID_VALUE(7)` 经 `decode_id` 会得到逻辑页 0，不按类型区分就会把链尾
/// 错接到 0；旧 fkv 盘面的 `encode_page_id(FREE_PAGE_NONE)` 则正常解码为
/// `FREE_PAGE_NONE`。两种尾哨兵都归一为链尾（对齐 C++ `GetNextFree`）。
#[inline]
fn next_free_page(value: u64) -> PageId {
  if val_type(value) == ValType::PageId {
    // `encode_page_id` 只编码 `PageId`（u32），解码结果必在范围内。
    decode_id(value) as PageId
  } else {
    FREE_PAGE_NONE
  }
}

impl BranchPageOwner<'_> {
  /// 该物理页是否落在当前 branch 的 file id 区间内。
  #[inline]
  pub fn owns(&self, file_page_id: FilePageId) -> bool {
    let file_id = file_page_id >> self.shift;
    let index = if self.segment {
      self
        .range_li
        .partition_point(|range| range.max_segment_file_id < file_id)
    } else {
      self
        .range_li
        .partition_point(|range| range.max_file_id < file_id)
    };
    self
      .range_li
      .get(index)
      .is_some_and(|range| range.branch_name.as_ref() == self.branch_name)
  }

  /// 当前 branch 名下的文件区间，半开、升序，且裁剪到 `[0, file_limit)`。
  ///
  /// 区间链按 `max_file_id`（segment 侧按 `max_segment_file_id`）单调：第 i 段
  /// 覆盖 `(prev_max, max_i]`，首段从 file 0 起。逐段判定归属可把 reopen 的
  /// page 位扫描（`max_fp_id` 量纲，默认每文件 2^18 页）降为 O(区间链长度)，
  /// 尾部未分配页位不产生开销。
  pub fn owned_file_ranges(
    &self,
    file_limit: FileId,
  ) -> impl Iterator<Item = (FileId, FileId)> + '_ {
    let mut next_file = 0u64;
    self.range_li.iter().filter_map(move |range| {
      let max_file_id = if self.segment {
        range.max_segment_file_id
      } else {
        range.max_file_id
      };
      let start = next_file.min(file_limit);
      next_file = max_file_id.saturating_add(1);
      let end = next_file.min(file_limit);
      (start < end && range.branch_name.as_ref() == self.branch_name).then_some((start, end))
    })
  }
}

/// 映射器：`tbl` + allocator + 批次记账。
///
/// 不实现 `PartialEq`：`generation` 是 Store 内单调代际（同名内容每次化身都不同），
/// 结构相等既无意义也无调用点；需要比对映射内容时直接比 `tbl()`。
#[derive(Debug, Clone)]
pub struct PageMapper {
  tbl: MappingTbl,
  allocator: FilePageAllocator,
  /// 空闲逻辑页链表头（`FREE_PAGE_NONE` 表示无空闲页）。
  ///
  /// 链节原位复用 `tbl` 槽位存下一跳：链尾写 `INVALID_VALUE`，其余写
  /// `encode_page_id(next)`（对齐 C++ `PageMapper::FreePage`），零额外内存分配。
  free_page_head: PageId,
  /// 空闲逻辑页数量，恒等于链表长度；供 `BatchJournal` 回滚与诊断。
  free_page_cnt: u32,
  /// 待写入 manifest 的日志增量：`(逻辑页, 操作时的 log 值)`。
  ///
  /// 值在操作发生时固化：`allocate_page` / `remap_page` / `set_file_page` 记
  /// `encode_file_page_id`，`free_page` 记 `INVALID_VALUE`。不能留到 flush 时
  /// 再读 `tbl` 的最终槽值：同一批「释放后复用」的页在 flush 时只剩最终
  /// `FilePageId`，释放项会被错写成 `FilePageId`，与 C++ WAL 的
  /// `DeleteMapping + UpdateMapping` 追加序列不一致（sem-45）。
  ///
  /// 按操作顺序追加、不去重：与 `ManifestBuilder` 的记录条数和顺序一一对应。
  dirty_li: Vec<(PageId, u64)>,
  /// 内容已变更（或释放）的既有逻辑 PageId，供上层失效索引缓存（eng-68）。
  changed_page_li: Vec<PageId>,
  /// 已从活跃映射摘除的物理页（`free_page` 释放的页 + `remap_page` 退役的旧页），
  /// 供上层失效按物理页键的写路径页缓存。
  ///
  /// 写缓存键是 `FilePageId`：Pooled 分配器会复用释放过的物理页，若缓存仍
  /// 持有旧内容，复用页在被写出之前被读到就会返回陈旧字节。释放点记录、上层
  /// 在批次出口消费并逐条失效，使「已释放的物理页不可能命中缓存」成为硬不变量。
  /// Append 模式物理页永不复用，本表仍记录以便统一失效逻辑。
  freed_file_page_li: Vec<FilePageId>,
  /// 映射代际：任何可能改变活跃映射的操作递增一次。快照在创建时记录该值，
  /// 索引缓存项按它区分「旧快照节点」与「新代节点」（27b）。
  generation: u64,
  /// 代际来源：同一 Store 内的所有 mapper（含 data / segment 与重建化身）
  /// 必须共享一份，保证「同值 ⇒ 同映射」不因 Store 内更换 mapper 而失效。
  pub(crate) generation_source: GenerationSource,
  /// Pooled 模式下已释放、但可能仍被在途快照引用的物理页。
  ///
  /// 每项为 `(物理页, 释放时刻已发出的最大快照代际)`。Append 模式恒为空。
  /// `remap_page` / `free_page` 只是把页放入本队列；真正归还
  /// [`FilePagePool`] 由 [`Self::reclaim_retired`] 按快照水位显式触发。
  ///
  /// 用 `VecDeque` 而非 `Vec`：队列按代际**单调非降**（见
  /// [`Self::retire_file_page`]），可回收项总是队头前缀，`reclaim_retired`
  /// 只需 `pop_front` 到第一条未过水位的条目，单次成本 O(回收数 + 1)。
  /// 旧实现每批 `retain` 全量扫描，长读快照存活期间队列只增不减，写路径
  /// 退化为 O(批数²×B)（perf-12）。
  retired_li: VecDeque<(FilePageId, u64)>,
  /// `reclaim_retired` 累计检查过的 retired 条目数（诊断 / 复杂度回归）。
  ///
  /// 前缀回收的扫描量 = 回收数 + 1；若实现退回全量扫描，该计数会随队列长度
  /// 线性增长，测试据此断言复杂度而不依赖不稳定的计时（perf-12）。
  retired_scan_cnt: u64,
  /// 快照代际来源（来自 [`crate::SnapshotRegistry::generation_source`]）。
  ///
  /// `None` 表示调用方承诺没有在途快照读者（旧单写者语义）：retire 记录 0，
  /// 任意 `watermark > 0` 的 reclaim 都会立即回池。接入快照的 Store 必须
  /// 用 [`Self::set_snapshot_generation_source`] 接线，否则可能提前复用旧页。
  snapshot_generation_source: Option<GenerationSource>,
  /// Pooled 页复用下界（sticky，只升不降）：`< reuse_floor` 的物理页永不
  /// 回池、也永不被重新分配。Append 模式恒为 0。
  ///
  /// 读快照水位（`SnapshotRegistry`）只覆盖在途 `MappingSnapshot`；branch /
  /// archive 是**持久化**的页集合快照，不在注册表里。它们引用的旧页在父表
  /// 覆盖写后离开活跃映射、也没有在途读者，代际水位判据会放行回池，下一次
  /// 分配即覆盖快照数据（pooled-07）。因此回收与 reopen 回填还必须尊重这个
  /// 由 manifest 重建的持久引用下界。
  reuse_floor: FilePageId,
  journal: Option<BatchJournal>,
}

impl PageMapper {
  pub fn new(mode: AllocMode, max_fp_id: FilePageId) -> Self {
    Self::empty(GenerationSource::new(), MappingTbl::new(), mode, max_fp_id)
  }

  /// 用给定的 Store 级代际来源构造空 mapper。
  ///
  /// `Store` 必须传自己的来源：同一 Store 内 data / segment mapper 及 reopen
  /// 重建的化身共享同一序列，索引缓存键才不会跨化身重叠（perf-06）。
  pub fn new_with_source(source: GenerationSource, mode: AllocMode, max_fp_id: FilePageId) -> Self {
    Self::empty(source, MappingTbl::new(), mode, max_fp_id)
  }

  /// 用已落盘的 `tbl` 重建 mapper（reopen / branch 恢复路径）。
  ///
  /// 契约：`tbl` 必须是本 mapper 独占的 owned 值——重建会让非活跃槽位原地串成
  /// 空闲链表（对齐 C++ `Replayer::GetMapper`），不能与其他快照 clone 共享。
  pub fn with_tbl(tbl: MappingTbl, mode: AllocMode, max_fp_id: FilePageId) -> Self {
    Self::with_tbl_and_source(tbl, GenerationSource::new(), mode, max_fp_id)
  }

  /// 用已落盘的 `tbl` 重建 mapper，并继续消费给定的 Store 级代际来源。
  ///
  /// `tbl` 的独占契约同 [`Self::with_tbl`]。Pooled 空闲池按 C++ legacy 语义
  /// 全量回填（不区分 branch）；branch 归属过滤用
  /// [`Self::with_tbl_and_source_in_branch`]。
  pub fn with_tbl_and_source(
    tbl: MappingTbl,
    source: GenerationSource,
    mode: AllocMode,
    max_fp_id: FilePageId,
  ) -> Self {
    let mut mapper = Self::empty(source, tbl, mode, max_fp_id);
    mapper.rebuild_free_page_li();
    mapper.rebuild_pooled_free_file_page_li(None);
    mapper
  }

  /// 用已落盘的 `tbl` 重建 mapper，并只把 `owner` 名下的物理页回填空闲池。
  ///
  /// `owner.range_li` 为空时退回 [`Self::with_tbl_and_source`] 的全量回填
  /// （对齐 C++ `Replayer::GetMapper` 的 `!ranges.empty()` 门控）。`tbl` 的
  /// 独占契约同上。
  pub fn with_tbl_and_source_in_branch(
    tbl: MappingTbl,
    source: GenerationSource,
    mode: AllocMode,
    max_fp_id: FilePageId,
    owner: BranchPageOwner<'_>,
  ) -> Self {
    let mut mapper = Self::empty(source, tbl, mode, max_fp_id);
    mapper.rebuild_free_page_li();
    mapper.rebuild_pooled_free_file_page_li(Some(owner));
    mapper
  }

  /// 基本构造：不重建空闲链表（`new` 的 `max_fp_id` 语义是「从此开始分配」）。
  pub(crate) fn empty(
    source: GenerationSource,
    tbl: MappingTbl,
    mode: AllocMode,
    max_fp_id: FilePageId,
  ) -> Self {
    Self {
      tbl,
      allocator: FilePageAllocator::new(mode, max_fp_id),
      free_page_head: FREE_PAGE_NONE,
      free_page_cnt: 0,
      dirty_li: Vec::new(),
      changed_page_li: Vec::new(),
      freed_file_page_li: Vec::new(),
      // 每个化身都取全新代际：重建后的 mapper 与旧化身的缓存键不重叠，
      // 在途旧快照回填的节点不会被新化身读者命中（r7a-core-02）。
      generation: source.next_generation(),
      generation_source: source,
      retired_li: VecDeque::new(),
      retired_scan_cnt: 0,
      snapshot_generation_source: None,
      reuse_floor: 0,
      journal: None,
    }
  }

  /// 只读视图：用给定 `tbl` 构造，不重建空闲链表。
  ///
  /// 供快照持有 segment 侧的只读映射（`fkv_large` 的读接口仍收 `&PageMapper`）。
  /// 视图上的写接口语义未定义，调用方不得使用；底层 chunk 与来源 COW 共享。
  pub fn read_view(tbl: MappingTbl, max_fp_id: FilePageId) -> Self {
    Self::empty(GenerationSource::new(), tbl, AllocMode::Append, max_fp_id)
  }

  /// 快照内部只读视图：共享来源避免每次 `open` 新建一份计数器。
  pub(crate) fn read_view_with_source(
    tbl: MappingTbl,
    source: GenerationSource,
    max_fp_id: FilePageId,
  ) -> Self {
    Self::empty(source, tbl, AllocMode::Append, max_fp_id)
  }

  /// 映射代际：任何可能改变活跃映射的操作递增一次。
  ///
  /// 快照创建时记录该值；索引缓存项按它区分不同代节点（27b）。同一代内
  /// 未变更的映射可安全复用缓存；取值在本 Store 内单调且不复用（见
  /// [`GenerationSource`]），因此「同代 ⇒ 同映射」恒成立。
  pub fn generation(&self) -> u64 {
    self.generation
  }

  #[inline]
  pub(crate) fn bump_generation(&mut self) {
    self.generation = self.generation_source.next_generation();
  }

  /// 接线快照代际来源（通常来自同一 Store 的
  /// [`crate::SnapshotRegistry::generation_source`]）。
  ///
  /// Pooled 页在 `remap_page` / `free_page` 时记录「释放时刻已发出的最大
  /// 快照代际」；此后用 `registry.min_live_generation()` 作为
  /// [`Self::reclaim_retired`] 的 watermark 即可安全回池。未接线的 mapper
  /// 视为「无在途快照读者」，retire 记录 0。
  ///
  /// 同一 Store 的 data / segment mapper 应共享同一个来源；跨 Store 不得共用。
  pub fn set_snapshot_generation_source(&mut self, source: GenerationSource) {
    self.snapshot_generation_source = Some(source);
  }

  /// 重建逻辑页空闲链表：`tbl` 中所有非活跃槽位按升序重新串链。
  pub(crate) fn rebuild_free_page_li(&mut self) {
    let free_li: Vec<PageId> = self
      .tbl
      .iter()
      .filter_map(|(page_id, value)| is_free_slot(value).then_some(page_id))
      .collect();
    let mut head = FREE_PAGE_NONE;
    let mut cnt = 0u32;
    for page_id in free_li {
      self.tbl.set(page_id, encode_free_next(head));
      head = page_id;
      cnt += 1;
    }
    self.free_page_head = head;
    self.free_page_cnt = cnt;
  }

  /// Pooled 模式 reopen：把 `0..max_fp_id` 中未被引用的物理页填回空闲池。
  ///
  /// 对齐 C++ `Replayer::GetMapper` 扫描 `using_fp_ids_set` 之外 id 的语义，
  /// 但**按半开区间记账、不逐页展开**：`max_fp_id` 是
  /// `file_id << pages_per_file_shift` 量纲（默认每文件 2^18 页），branch 预留
  /// 后可达数亿。逐页 `free` 会让单次 reopen 分配 GB 级内存；这里只压入真实
  /// 空洞区间（`used_li` 之间的 gap + 尾段），逻辑空闲页数不变，分配顺序也
  /// 与逐页入链完全一致（见 [`crate::FilePagePool`]）。
  ///
  /// Append 模式 `free` 为 no-op，无需恢复。
  ///
  /// `owner` 为 `Some` 时只回填当前 branch 名下的文件区间（C++ `FileIdInBranch`）：
  /// 映射里的空洞不等于可复用页，落在父 / 兄弟 branch 区间内的页一旦被本
  /// branch 分配，写入就会覆盖对方活数据。归属判定按文件区间整段进行，不再
  /// 逐页调 [`BranchPageOwner::owns`]。`owner.range_li` 为空或 `None` 时保留
  /// 全量回填（legacy manifest 对齐）。
  pub(crate) fn rebuild_pooled_free_file_page_li(&mut self, owner: Option<BranchPageOwner<'_>>) {
    let floor = self.reuse_floor;
    let FilePageAllocator::Pooled(pool) = &mut self.allocator else {
      return;
    };
    let max_fp_id = pool.max_fp_id();
    // 下界以下没有可复用页：映射空洞全部属于持久化快照（pooled-07）。
    if floor >= max_fp_id {
      return;
    }
    let mut used_li: Vec<FilePageId> = self
      .tbl
      .iter_value_li()
      .filter(|value| is_file_page_id(*value))
      .map(decode_id)
      .collect();
    used_li.sort_unstable();
    used_li.dedup();
    // 高水位之外的活页不应存在；若出现也不能用它切分空洞，否则会把空洞
    // 填到 `max_fp_id` 之外。
    used_li.truncate(used_li.partition_point(|id| *id < max_fp_id));
    // 空区间链 = 无归属信息：与 C++ `!ranges.empty()` 门控一致，全量回填。
    let Some(owner) = owner.filter(|owner| !owner.range_li.is_empty()) else {
      let mut next = floor;
      for used in used_li {
        if used < next {
          continue;
        }
        if next < used {
          pool.free_range(next, used);
        }
        next = used + 1;
      }
      if next < max_fp_id {
        pool.free_range(next, max_fp_id);
      }
      return;
    };
    // 只回填当前 branch 名下的文件区间；按文件区间扫描而不是逐页判 `owns`。
    let shift = owner.shift;
    let file_limit = max_fp_id.div_ceil(1u64 << shift);
    let mut used_idx = 0;
    for (file_start, file_end) in owner.owned_file_ranges(file_limit) {
      let start = (file_start << shift).max(floor);
      let end = (file_end << shift).min(max_fp_id);
      if start >= end {
        continue;
      }
      while used_idx < used_li.len() && used_li[used_idx] < start {
        used_idx += 1;
      }
      let mut next = start;
      while used_idx < used_li.len() && used_li[used_idx] < end {
        let used = used_li[used_idx];
        if next < used {
          pool.free_range(next, used);
        }
        next = used + 1;
        used_idx += 1;
      }
      if next < end {
        pool.free_range(next, end);
      }
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
      retired_len: self.retired_li.len(),
      reuse_floor: self.reuse_floor,
      changed_len: self.changed_page_li.len(),
      freed_len: self.freed_file_page_li.len(),
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
    // 批次内 retire 的页在回滚后重新被映射引用；撤销 retire，避免之后
    // reclaim 把它回池复用。已回池的页不会出现在队列里（reclaim 会移除条目）。
    self.retired_li.truncate(journal.retired_len);
    // 下界与空闲池必须一起还原：allocator 已回到批次前（含被裁剪的区间），
    // 若只还原其一，池里低于下界的页就可能被重新分配。
    self.reuse_floor = journal.reuse_floor;
    self.changed_page_li.truncate(journal.changed_len);
    // 批次内摘除的物理页在回滚后重新被映射引用：撤销释放记录，否则上层会把
    // 仍然有效的写缓存条目误失效（并可能让复用页在写出前命中陈旧内容）。
    self.freed_file_page_li.truncate(journal.freed_len);
    // 映射内容回到批次前，但代际必须继续前进：回退到旧值会让「同代 ⇒ 同映射」
    // 失效（本批写入过的代际会被下一批重新取到），在途旧快照回填的索引节点
    // 就能在之后被当作本代条目命中（r7a-core-01）。回退后的映射是一份新的
    // 状态，取新代际后旧快照（代际更小）一律被缓存拒绝。
    self.bump_generation();
    true
  }

  pub(crate) fn journal_change(&mut self, page_id: PageId) {
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

  /// 尚未写入 manifest 的日志增量（只读借用；写盘成功后再 `clear_dirty_page_li`）。
  ///
  /// 每项为 `(逻辑页, 操作时的 log 值)`，按操作顺序追加、不去重：同一页在
  /// 本批「释放后复用」时先记 `INVALID_VALUE`、再记新的 `FilePageId`，与 C++
  /// `ManifestBuilder::DeleteMapping` / `UpdateMapping` 的追加序列逐条对应。
  ///
  /// 不能用 `take` 提前清空：manifest 写失败需要保留脏页供重试，且
  /// `abort_batch` 依赖 `dirty_li` 与 journal 的长度关系回滚。
  pub fn dirty_page_li(&self) -> &[(PageId, u64)] {
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

  /// 取出本批摘除的物理页并清空（用于精确失效写路径页缓存）。
  pub fn take_freed_file_page_li(&mut self) -> Vec<FilePageId> {
    take(&mut self.freed_file_page_li)
  }

  /// 尚未被上层取走的已释放物理页（诊断：跨批次累积检查）。
  pub fn freed_file_page_li(&self) -> &[FilePageId] {
    &self.freed_file_page_li
  }

  /// 从空闲链表头摘下一个逻辑页（对齐 C++ `PageMapper::GetPage` 的链表维护）。
  ///
  /// 槽位在原位先置 `INVALID_VALUE`（对齐 C++ `map.Set(free_page, InvalidValue)`）：
  /// 调用方要么立即写入物理页（[`Self::allocate_page`]），要么留作预留槽位
  /// （[`Self::reserve_page`]）等写出相位再提交。返回 `None` 表示空闲链为空，
  /// 调用方需要追加新槽位。
  fn take_free_page(&mut self) -> Option<PageId> {
    if self.free_page_cnt == 0 {
      return None;
    }
    // 复用链表头：槽位里存的是下一个空闲页（链尾是 `INVALID_VALUE` 或旧盘面的
    // `encode_page_id(FREE_PAGE_NONE)`，统一由 `next_free_page` 归一为链尾）。
    let page_id = self.free_page_head;
    debug_assert!((page_id as usize) < self.tbl.len(), "空闲链表节点越界");
    self.journal_change(page_id);
    let next = self.tbl.get(page_id).map_or(FREE_PAGE_NONE, next_free_page);
    self.free_page_head = next;
    self.free_page_cnt -= 1;
    self.tbl.set(page_id, INVALID_VALUE);
    Some(page_id)
  }

  /// 分配逻辑页：优先复用空闲链表头（对齐 C++ `GetPage`），链表为空才追加新槽位。
  pub fn allocate_page(&mut self) -> PageId {
    let file_page_id = self.allocator.allocate();
    debug_assert!(
      file_page_id >= self.reuse_floor,
      "物理页分配低于 Pooled 复用下界"
    );
    let page_id = match self.take_free_page() {
      Some(page_id) => {
        self.tbl.set(page_id, encode_file_page_id(file_page_id));
        page_id
      }
      None => self.tbl.push(encode_file_page_id(file_page_id)),
    };
    // 记操作时的 log 值（新 FilePageId），不是 flush 时的最终槽值（sem-45）。
    self
      .dirty_li
      .push((page_id, encode_file_page_id(file_page_id)));
    self.bump_generation();
    page_id
  }

  /// 预留一个逻辑页槽位：占槽但**不分配物理页、不追加 mapping 记录**。
  ///
  /// 对齐 C++ `PageMapper::GetPage`（`page_mapper.cpp:358-374`）：`BatchWriteTask`
  /// 在 `FinishDataPage` 里对新增页只取逻辑 id（`batch_write_task.cpp:1070`），
  /// 物理页与 `UpdateMapping` 推迟到 `ShiftLeafLink` → `WritePage` →
  /// `AllocatePage`（`:122-131`、`write_task.cpp:238-267`）。写路径用本接口把
  /// 「分裂组最后一张分裂页」的 mapping 记录推迟到写出相位，避免它抢在后继页的
  /// `DeleteMapping` 之前（sem-53）。
  ///
  /// 预留槽位的槽值为 `INVALID_VALUE`，不构成活跃映射，因此不推进代际。
  /// 调用方必须在同一批内用 [`Self::commit_reserved_page`] 结算；`abort_batch`
  /// 会把 `tbl` 截断回批次前（或按 journal 恢复被复用的空闲槽），回滚无需显式
  /// 撤销预留。批次提交后仍留有未提交的预留槽属于调用方错误：该槽不会出现在
  /// 任何 mapping 记录里，reopen 时会被当作空闲页回收。
  pub fn reserve_page(&mut self) -> PageId {
    match self.take_free_page() {
      Some(page_id) => page_id,
      None => self.tbl.push(INVALID_VALUE),
    }
  }

  /// 为 [`Self::reserve_page`] 预留的槽位分配物理页并追加 `UpdateMapping`。
  ///
  /// 记录值与 [`Self::allocate_page`] 完全一致（新 `FilePageId`，sem-45 的操作时
  /// 取值），只是推迟到调用时；调用点即 C++ `WritePage` → `AllocatePage` 的写出
  /// 相位。`page_id` 必须仍是 `INVALID_VALUE`（预留后未被改写、也未提交过）。
  pub fn commit_reserved_page(&mut self, page_id: PageId) -> Result<FilePageId> {
    if self.tbl.get(page_id)? != INVALID_VALUE {
      return Err(KvError::Corrupted);
    }
    // 必须在写出相位才分配：物理页归属要与 C++ `ShiftLeafLink` 的写出顺序一致
    // （分裂尾页的记录排在删除页 `DeleteMapping` 之后，sem-53）。
    let file_page_id = self.allocator.allocate();
    debug_assert!(
      file_page_id >= self.reuse_floor,
      "物理页分配低于 Pooled 复用下界"
    );
    self.tbl.set(page_id, encode_file_page_id(file_page_id));
    self
      .dirty_li
      .push((page_id, encode_file_page_id(file_page_id)));
    self.bump_generation();
    Ok(file_page_id)
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
    // 先 allocate 再 retire 旧页：Pooled 的 free list 是 LIFO，直接 free 会让
    // allocate 立刻取回同一物理页，COW 退化成原地覆盖。retire 后旧页不会进入
    // free list，只有快照水位推进后 `reclaim_retired` 才归还，COW 因此不会
    // 覆盖在途读者仍引用的旧页。
    let file_page_id = self.allocator.allocate();
    debug_assert!(
      file_page_id >= self.reuse_floor,
      "物理页分配低于 Pooled 复用下界"
    );
    if is_file_page_id(old) {
      let old_file_page_id = decode_id(old);
      // 旧物理页退出活跃映射：登记给上层失效写路径页缓存。必须在 `retire_file_page`
      // 之前登记（`retire_file_page` 只对 Pooled 生效，Append 下旧页同样退出映射）。
      self.freed_file_page_li.push(old_file_page_id);
      self.retire_file_page(old_file_page_id);
    }
    self.tbl.set(page_id, encode_file_page_id(file_page_id));
    // 记操作时的 log 值（新 FilePageId），不是 flush 时的最终槽值（sem-45）。
    self
      .dirty_li
      .push((page_id, encode_file_page_id(file_page_id)));
    self.bump_generation();
    Ok(file_page_id)
  }

  /// 直接改写逻辑页到物理页的映射（reopen / 快照安装用，无失败路径）。
  pub fn set_file_page(&mut self, page_id: PageId, file_page_id: FilePageId) {
    self.journal_change(page_id);
    self.changed_page_li.push(page_id);
    self.tbl.set(page_id, encode_file_page_id(file_page_id));
    self
      .dirty_li
      .push((page_id, encode_file_page_id(file_page_id)));
    self.bump_generation();
  }

  pub fn file_page_id(&self, page_id: PageId) -> Result<FilePageId> {
    let value = self.tbl.get(page_id)?;
    if !is_file_page_id(value) {
      return Err(KvError::ResourceMissing);
    }
    Ok(decode_id(value))
  }

  /// Pooled：把物理页放入 retired 队列；Append：no-op（旧页永不复用）。
  ///
  /// 记录「释放时刻已发出的最大快照代际」：之后才 `open` 的快照代际更大，
  /// 不可能引用本页，因此 `generation < min_live_generation` 是精确的回收条件。
  fn retire_file_page(&mut self, file_page_id: FilePageId) {
    if !matches!(self.allocator, FilePageAllocator::Pooled(_)) {
      return;
    }
    // 低于复用下界的页仍被 branch / archive manifest 引用（持久化快照），
    // 不能进入回收队列，否则下一次 reclaim + allocate 就会覆盖快照数据。
    if file_page_id < self.reuse_floor {
      return;
    }
    let generation = self
      .snapshot_generation_source
      .as_ref()
      .map_or(0, GenerationSource::last_issued);
    // 前缀回收依赖的代际单调性：`last_issued` 只增不减，入队顺序即代际顺序。
    debug_assert!(
      self
        .retired_li
        .back()
        .is_none_or(|&(_, last)| last <= generation),
      "retired 队列代际必须单调非降"
    );
    self.retired_li.push_back((file_page_id, generation));
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
    self.freed_file_page_li.push(file_page_id);
    self.retire_file_page(file_page_id);
    // 与 C++ `PageMapper::FreePage` 同编码：链尾 `InvalidValue`，链节 `PageId`。
    self.tbl.set(page_id, encode_free_next(self.free_page_head));
    self.free_page_head = page_id;
    self.free_page_cnt += 1;
    // C++ `WriteTask::FreePage` 在操作时固定记 `DeleteMapping`（值 7），
    // 不能沿用槽位里的空闲链编码；同批复用也不改这条已追加的记录（sem-45）。
    self.dirty_li.push((page_id, INVALID_VALUE));
    self.bump_generation();
    Ok(Some(file_page_id))
  }

  /// 空闲逻辑页数量，恒等于空闲链表长度。
  pub fn free_page_cnt(&self) -> u32 {
    self.free_page_cnt
  }

  /// Pooled 空闲池的逻辑空闲物理页数（区间长度之和，Append 恒为 0）。
  pub fn pooled_free_len(&self) -> u64 {
    match &self.allocator {
      FilePageAllocator::Pooled(pool) => pool.free_len(),
      FilePageAllocator::Append(_) => 0,
    }
  }

  /// Pooled 空闲池的区间条目数（Append 恒为 0）。
  ///
  /// reopen 重建的成本口径：逻辑空闲页数可以很大（`max_fp_id` 量纲），但
  /// 条目数只与真实空洞段数有关，不随未分配页位数展开。
  pub fn pooled_free_range_cnt(&self) -> usize {
    match &self.allocator {
      FilePageAllocator::Pooled(pool) => pool.free_range_cnt(),
      FilePageAllocator::Append(_) => 0,
    }
  }

  /// 尚未回收的 retired 页数（诊断 / 测试）。
  pub fn retired_len(&self) -> usize {
    self.retired_li.len()
  }

  /// 把代际水位以下的 retired 页归还 Pooled 空闲池，返回回收页数。
  ///
  /// `watermark` 应传 [`crate::SnapshotRegistry::min_live_generation`]：
  /// 无存活快照时为 `u64::MAX`，回收全部；旧快照存活时水位不超过它记录
  /// 的代际，对应页继续滞留，`allocate` 不会复用。
  ///
  /// 只对 `AllocMode::Pooled` 生效；Append 模式 retire 本身是 no-op，恒返回 0。
  ///
  /// **复杂度**：retired 队列按代际单调非降（`retire_file_page` 记录的
  /// `last_issued` 只增不减，`abort_batch` / 下界过滤只删除条目、不改变相对
  /// 顺序），因此「代际早于水位」的条目必是队头前缀。这里只 `pop_front`
  /// 该前缀，遇到第一条 `generation >= watermark` 即停：单次成本
  /// O(回收数 + 1)。旧实现每批 `retain` 全量扫描，长读快照存活期间队列只增
  /// 不减（第 i 批扫 i×B 条），写路径退化为 O(批数²×B)（perf-12）。
  ///
  /// 批次进行中不回收：`abort_batch` 以 retire 队列长度为回滚点，批次内回收
  /// 会破坏这个栈式不变量（回滚后重新活跃的页可能已不在队列里）。回收应由
  /// 写路径在开批前、或维护路径（maintain / clean_expired / compact /
  /// truncate）在批次结算之后驱动，此时返回 0 只是延后。
  pub fn reclaim_retired(&mut self, watermark: u64) -> usize {
    if self.journal.is_some() {
      return 0;
    }
    let floor = self.reuse_floor;
    let FilePageAllocator::Pooled(pool) = &mut self.allocator else {
      return 0;
    };
    let mut reclaimed = 0;
    let mut scanned = 0u64;
    while let Some(&(file_page_id, generation)) = self.retired_li.front() {
      scanned += 1;
      if generation >= watermark {
        // 下界提升会同步清理 retired 队列、retire 也跳过低于下界的页，
        // 因此停在队头的条目必不低于下界（只需检查这一条，避免全量断言）。
        debug_assert!(file_page_id >= floor, "retired 队头低于 Pooled 复用下界");
        break;
      }
      self.retired_li.pop_front();
      // 下界以下的页永不回池：它们仍被持久化 manifest 引用。正常路径不会
      // 出现（retire 与下界提升都已过滤），保留与旧实现一致的丢弃语义。
      if file_page_id < floor {
        continue;
      }
      pool.free(file_page_id);
      reclaimed += 1;
    }
    self.retired_scan_cnt += scanned;
    reclaimed
  }

  /// `reclaim_retired` 累计检查过的 retired 条目数（诊断 / 复杂度回归）。
  ///
  /// 前缀回收下每次调用只增长「回收数 + 1」（队列非空时）；若实现退回全量
  /// 扫描，该计数会随队列长度线性增长（perf-12）。
  pub fn retired_scan_cnt(&self) -> u64 {
    self.retired_scan_cnt
  }

  /// 当前 Pooled 页复用下界；Append 模式恒为 0。
  pub fn reuse_floor(&self) -> FilePageId {
    self.reuse_floor
  }

  /// 抬高 Pooled 页复用下界（sticky，只升不降，pooled-07）。
  ///
  /// 低于新下界的空闲页立即从池中丢弃、retired 队列同步清理；此后的
  /// `retire_file_page` 不再登记这些页，`allocate` 永远不会返回它们。调用方
  /// 在 branch / archive manifest 落盘前后按其覆盖的物理页集合设置下界：
  ///
  /// - `create_branch`：父表下界 = 子分支预留区间末尾；
  /// - 子分支 open：下界 = 父分支创建时高水位（继承页永不复用）；
  /// - `create_archive`：下界 = 归档创建时 `max_fp_id`。
  ///
  /// 只在无活跃写批次时调用：下界变化与 branch / archive manifest 的发布绑定，
  /// 批次内的映射回滚不应把下界退回旧值。`BatchJournal` 记录了下界，仅为
  /// 池区间与下界保持一致，正常路径不要依赖它。
  pub fn raise_reuse_floor(&mut self, floor: FilePageId) {
    if floor <= self.reuse_floor || !matches!(self.allocator, FilePageAllocator::Pooled(_)) {
      return;
    }
    self.reuse_floor = floor;
    if let FilePageAllocator::Pooled(pool) = &mut self.allocator {
      // `reserve_until` 同时抬高高水位并裁剪低于下界的空闲区间：下界高于当前
      // 高水位时跳过这段未分配区间（branch 预留语义），保证 allocate 恒 >= floor。
      pool.reserve_until(floor);
    }
    self.retired_li.retain(|(id, _)| *id >= floor);
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

  /// 物理页分配模式（诊断 / 统计口径断言用）。
  ///
  /// `space_size` 的 append 口径（`min_file_id` / `empty_file_cnt`）只对
  /// [`AllocMode::Append`] 成立；Pooled 下它退化为 `max_fp_id`，且没有任何
  /// 缩减路径（pooled-03）。段 mapper 必须恒为 `Append`（pooled-02，对齐
  /// C++ `BatchWriteTask::EnsureSegmentMapper`），调用方可据此防御。
  pub fn alloc_mode(&self) -> AllocMode {
    match self.allocator {
      FilePageAllocator::Append(_) => AllocMode::Append,
      FilePageAllocator::Pooled(_) => AllocMode::Pooled,
    }
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

  /// 全空映射时跳过已死空间（见 [`crate::AppendAllocator::skip_to_next_file`]）。
  pub fn skip_empty_space(&mut self, pages_per_file_shift: u8) {
    self.allocator.skip_to_next_file(pages_per_file_shift);
  }

  pub fn min_file_id(&self) -> FileId {
    self.allocator.min_file_id()
  }

  /// 跳过一段 file page id 区间（branch 预留）。
  ///
  /// 落在预留区间（`< file_page_id`）的 retired 页不再回池复用，与
  /// [`crate::FilePagePool::reserve_until`] 对空闲链的处理保持一致。
  pub fn reserve_until(&mut self, file_page_id: FilePageId) {
    self.retired_li.retain(|(id, _)| *id >= file_page_id);
    self.allocator.reserve_until(file_page_id);
  }

  pub fn serialize(&self) -> Vec<u8> {
    self.tbl.serialize()
  }
}
