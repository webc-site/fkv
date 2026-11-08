//! in-flight 保留判定：branch 高水位、manifest 区间归属与变体级引用知识。

use std::{
  cell::{Cell, RefCell},
  rc::Rc,
};

use fkv_mapping::MappingTbl;
use fkv_types::{BranchFileRange, FileId, FilePageId, decode_id, is_file_page_id};
use gxhash::{HashMap, HashSet};

/// 单 branch 的 in-flight 高水位。
///
/// 对齐 C++ `BranchGuard`：`newest_term` 是该 branch 已知的最新 term，
/// `least_unflushed_*` 是该 term 下已写入的最后一个文件 id。file id ≥ 边界的
/// 文件可能「数据页已 sync、增量 manifest 尚未提交」，GC 必须保留；归属 term
/// 早于 `newest_term` 的旧区间在越过该边界后不再兜底（见 `entry_inflight`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BranchGuard {
  pub newest_term: u64,
  pub least_unflushed_file_id: Option<FileId>,
  pub least_unflushed_segment_file_id: Option<FileId>,
}

/// 文件类别：data / segment 的高水位字段分开存放。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileKind {
  Data,
  Segment,
}

impl FileKind {
  const fn guard_bound(self, guard: &BranchGuard) -> Option<FileId> {
    match self {
      Self::Data => guard.least_unflushed_file_id,
      Self::Segment => guard.least_unflushed_segment_file_id,
    }
  }
}

/// `file_range_li` 链中的一个归属区间。
///
/// `max` 是该 manifest 记录的 branch 高水位（区间上界），`term` 是该区间
/// 写入时的 term。只服务无身份后端（`FileVariant::identity == None`）的
/// in-flight 区间归属代理；有身份的候选走 `RefKnowledge` 的变体精确判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RangeEntry {
  pub(crate) max: FileId,
  pub(crate) branch_idx: u32,
  pub(crate) term: u64,
}

/// 引用侧的变体归属知识（rev-39）。
///
/// data / segment 各一份：同一 `FileId` 在两类文件里是两个编号空间，归属
/// 链的上界字段也不同（`max_file_id` / `max_segment_file_id`）。
#[derive(Debug, Default)]
pub(crate) struct RefKnowledge {
  /// 被 manifest 引用过的 file id。归属失败的引用不丢弃（sto-54a：id 级
  /// 并集是安全下限），只把变体精度落在归属成功的条目上。
  pub(crate) ref_ids: HashSet<FileId>,
  /// 引用归属失败：file id 超出该 manifest 的区间链（或链为空）。变体身份
  /// 未知，同 id 全部变体保守保留。
  pub(crate) attr_failed: HashSet<FileId>,
  /// 归属成功的被引用变体：file id → [(branch, term)]。并列上界的候选归属
  /// 全部采纳（C++ `lower_bound` 对并列上界的取舍依赖容器序，这里放宽为
  /// 全保留，宁可少删）。
  pub(crate) variants: HashMap<FileId, Vec<(Box<str>, u64)>>,
}

impl RefKnowledge {
  /// 本变体是否被引用命中。
  pub(crate) fn hits(&self, file_id: FileId, branch: &str, term: u64) -> bool {
    self.variants.get(&file_id).is_some_and(|variant_li| {
      variant_li
        .iter()
        .any(|(hit_branch, hit_term)| hit_branch.as_ref() == branch && *hit_term == term)
    })
  }
}

/// per-branch in-flight 保留判定。
///
/// 有身份候选（`FileVariant::identity`）按文件自身 `(branch, term)` 精确判定
/// （`retain_*_variant`），引用归属知识由 `data_ref` / `segment_ref` 承载；
/// `data_li` / `segment_li` 与区间归属只服务无身份后端的兜底路径。
/// `guard_li` 折叠出每 branch 最新的未 flush 边界，两条路径共用。
#[derive(Debug)]
pub struct InflightGuard {
  /// 去重后的 branch 名，下标同时索引 `guard_li` 与 `RangeEntry::branch_idx`。
  branch_li: Vec<Box<str>>,
  branch_map: HashMap<Box<str>, u32>,
  /// 与 `branch_li` 平行。
  guard_li: Vec<BranchGuard>,
  /// data / segment 归属链，按区间上界升序（无身份后端的兜底归属）。
  ///
  /// 惰性排序：`add_range_li` 在 manifest 循环里逐份调用，只追加并把链标记为
  /// 脏；第一次 `retain` 前才排序去重一次。旧实现每次追加都对整条已累积链
  /// 重排，M 份 manifest、每份链长 k 时是 O(M²·k·log(M·k))（r7a-core-04）。
  ///
  /// 存 `Rc<Vec<_>>` 而非裸 `Vec`：`chain` 只在内部短暂借用，返回 `Rc` 快照。
  /// `retain` 因此不长期持有 `Ref`，期间若有同线程的 `add_range_li` 补录
  /// manifest 也不会撞上 `RefCell` 借用冲突；无快照在外时 `Rc::make_mut`
  /// 是唯一强引用，不复制。
  data_li: RefCell<Rc<Vec<RangeEntry>>>,
  segment_li: RefCell<Rc<Vec<RangeEntry>>>,
  /// 两条链是否有未排序的追加。
  chain_dirty: Cell<bool>,
  /// 本进程正在写的 branch；无区间归属的 in-flight 文件只归它保护。
  active: Option<u32>,
  /// 本进程当前 term：`term > process_term` 的文件属于更高的新 owner，
  /// GC 必须保留（对齐 C++ `file_gc.cpp` 的 `term > process_term` 跳过）。
  process_term: u64,
  /// manifest 引用的变体归属（data / segment 分开）。
  data_ref: RefKnowledge,
  segment_ref: RefKnowledge,
  /// 只由内存 mapping / 活跃快照等外部注册引用的 file id（data / segment）。
  /// 外部引用不带 `(branch, term)`：同 id 的任一物理变体都可能是读者目标，
  /// 变体级删除必须整体跳过，只保留 id 级判定。
  data_pin: HashSet<FileId>,
  segment_pin: HashSet<FileId>,
  /// 读取或解析失败的 manifest 名单。引用集合因此不完整，`retain` 一律
  /// 保守保留，绝不误删仍可能被引用的文件。
  damaged_li: Vec<Box<str>>,
}

impl Default for InflightGuard {
  fn default() -> Self {
    Self {
      branch_li: Vec::new(),
      branch_map: HashMap::default(),
      guard_li: Vec::new(),
      data_li: RefCell::new(Rc::new(Vec::new())),
      segment_li: RefCell::new(Rc::new(Vec::new())),
      chain_dirty: Cell::new(false),
      active: None,
      // 默认不启用 term 门控；Store::gc_with_scope 会显式 set_process_term。
      // 直接调用 task_gc 的测试 / 工具保持 rev-39 之前的判定。
      process_term: u64::MAX,
      data_ref: RefKnowledge::default(),
      segment_ref: RefKnowledge::default(),
      data_pin: HashSet::default(),
      segment_pin: HashSet::default(),
      damaged_li: Vec::new(),
    }
  }
}

impl InflightGuard {
  /// 本轮是否存在损坏 / 不可读的 manifest。
  pub fn is_damaged(&self) -> bool {
    !self.damaged_li.is_empty()
  }

  /// 损坏 / 不可读的 manifest 名单（用于上层记录与诊断）。
  pub fn damaged_li(&self) -> &[Box<str>] {
    &self.damaged_li
  }

  pub(crate) fn mark_damaged(&mut self, name: &str) {
    if !self.damaged_li.iter().any(|item| &**item == name) {
      self.damaged_li.push(name.into());
    }
  }

  pub(crate) fn branch_idx(&mut self, branch: &str) -> u32 {
    if let Some(idx) = self.branch_map.get(branch) {
      return *idx;
    }
    // branch 数量远小于 u32 上限，截断不会发生。
    let idx = self.branch_li.len() as u32;
    self.branch_li.push(branch.into());
    self.branch_map.insert(branch.into(), idx);
    self.guard_li.push(BranchGuard::default());
    idx
  }

  /// 折入一条 manifest 区间；同 branch 取最大 term，同 term 取最大文件 id。
  pub fn fold(
    &mut self,
    branch: &str,
    term: u64,
    max_file_id: Option<FileId>,
    max_segment_file_id: Option<FileId>,
  ) {
    let idx = self.branch_idx(branch);
    self.fold_idx(idx, term, max_file_id, max_segment_file_id);
  }

  pub(crate) fn fold_idx(
    &mut self,
    idx: u32,
    term: u64,
    max_file_id: Option<FileId>,
    max_segment_file_id: Option<FileId>,
  ) {
    // 边界 0 说明该 branch 还没写满/写出第一个文件，不足以保护后续文件；
    // 否则会把整表文件都当成 in-flight（truncate 后无法回收）。
    let max_file_id = max_file_id.filter(|id| *id > 0);
    let max_segment_file_id = max_segment_file_id.filter(|id| *id > 0);
    let guard = &mut self.guard_li[idx as usize];
    if term > guard.newest_term {
      guard.newest_term = term;
      guard.least_unflushed_file_id = max_file_id;
      guard.least_unflushed_segment_file_id = max_segment_file_id;
    } else if term == guard.newest_term {
      guard.least_unflushed_file_id = max_opt(guard.least_unflushed_file_id, max_file_id);
      guard.least_unflushed_segment_file_id =
        max_opt(guard.least_unflushed_segment_file_id, max_segment_file_id);
    }
  }

  /// 记录 manifest 的区间链，供 `file_id → (branch, term)` 归属解析。
  pub fn add_range_li(&mut self, range_li: &[BranchFileRange]) {
    if range_li.is_empty() {
      return;
    }
    // 继承链里可能有父 branch 的区间，逐个折入 guard，避免只有子 branch
    // manifest 时父 branch 的 in-flight 文件失去保护。
    for range in range_li {
      let idx = self.branch_idx(&range.branch_name);
      self.fold_idx(
        idx,
        range.term,
        Some(range.max_file_id),
        Some(range.max_segment_file_id),
      );
      // `get_mut` 直接拿 `&mut Rc<Vec<_>>`，不产生运行时借用，避免与
      // `branch_idx` 的 `&mut self` 冲突；此时无外部快照，`make_mut` 不复制。
      Rc::make_mut(self.data_li.get_mut()).push(RangeEntry {
        max: range.max_file_id,
        branch_idx: idx,
        term: range.term,
      });
      Rc::make_mut(self.segment_li.get_mut()).push(RangeEntry {
        max: range.max_segment_file_id,
        branch_idx: idx,
        term: range.term,
      });
    }
    self.chain_dirty.set(true);
  }

  /// 用本进程 allocator 的高水位补种 active branch guard。
  pub fn seed(&mut self, branch: &str, term: u64, max_fp_id: FilePageId, shift: u8) {
    let file_id = max_fp_id >> shift;
    let idx = self.branch_idx(branch);
    self.active = Some(idx);
    self.process_term = term;
    self.fold_idx(idx, term, Some(file_id), None);
  }

  pub fn seed_segment(&mut self, branch: &str, term: u64, max_fp_id: FilePageId, shift: u8) {
    let file_id = max_fp_id >> shift;
    let idx = self.branch_idx(branch);
    self.active = Some(idx);
    self.process_term = term;
    self.fold_idx(idx, term, None, Some(file_id));
  }

  /// 记录本进程 term；没有内存 mapper 时也要设置，否则变体门控会把所有
  /// 正 term 文件当成更高 owner 而永不回收。
  pub fn set_process_term(&mut self, term: u64) {
    self.process_term = term;
  }

  /// 是否存在 term 高于本进程的 manifest。C++ 此时整轮 GC 返回
  /// `ExpiredTerm`，避免删除新 owner 的对象。
  pub fn term_above(&self, term: u64) -> bool {
    self.guard_li.iter().any(|guard| guard.newest_term > term)
  }

  /// 是否存在**指定 branch** 的 term 高于给定 term 的 manifest。
  ///
  /// attach term 是 per-branch（term 文件按 `(branch, shard_id)` CAS）：别的
  /// branch 的更高 term 只说明同一分区里并存另一个 owner，不表示本 branch 被
  /// 接管。维护侧的 `ExpiredTerm` 只用本结果；删除安全仍用全局 [`Self::term_above`]。
  pub fn term_above_branch(&self, branch: &str, term: u64) -> bool {
    self
      .branch_map
      .get(branch)
      .is_some_and(|idx| self.guard_li[*idx as usize].newest_term > term)
  }

  pub fn retain_data(&self, file_id: FileId) -> bool {
    self.retain(file_id, FileKind::Data)
  }

  pub fn retain_segment(&self, file_id: FileId) -> bool {
    self.retain(file_id, FileKind::Segment)
  }

  pub(crate) fn retain(&self, file_id: FileId, kind: FileKind) -> bool {
    // manifest 损坏时引用集合不完整：一律保留，本轮不误删任何文件。
    if self.is_damaged() {
      return true;
    }
    let range_li = self.chain(kind);
    if range_li.is_empty() {
      // 无任何区间信息：只能按 active branch 兜底（未接 manifest 的调用方）。
      return self
        .active_bound(kind)
        .is_some_and(|bound| file_id >= bound);
    }
    // 区间上界单调不减：取「上界不超过 file id 的最近区间」作为归属。
    let end = range_li.partition_point(|entry| entry.max <= file_id);
    if end == 0 {
      // 低于全部高水位：归属落在链首之前，由 active branch 兜底判定。
      return self
        .active_bound(kind)
        .is_some_and(|bound| file_id >= bound);
    }
    // 同一高水位可能对应多个 branch（父 branch 预留下界 == 子 branch 起始），
    // 任一 branch 判定 in-flight 即保留，避免依赖 manifest 的读取顺序。
    let owner_max = range_li[end - 1].max;
    // 上界 0 的区间说明该 branch 还没有已 flush 文件，不携带信息，不能占有
    // 文件；一律交 active branch 兜底（truncate 后 / 空表仍可回收）。
    if owner_max > 0 {
      let owner_inflight = range_li[..end]
        .iter()
        .rev()
        .take_while(|entry| entry.max == owner_max)
        .any(|entry| self.entry_inflight(*entry, file_id, kind));
      if owner_inflight {
        return true;
      }
      if end < range_li.len() {
        // 链上还有更高的正高水位：file id 已被更新的区间覆盖，不是 in-flight。
        return false;
      }
    }
    // file id 已在全部高水位之上：可能属于任何写入者，按 active 兜底；
    // 无 active branch（表未打开 / 别的实例在写）时无法判定，只有在链上存在
    // 正高水位（即确有已 flush 文件）时才保守保留。
    match self.active_bound(kind) {
      Some(bound) => file_id >= bound,
      None => range_li.last().is_some_and(|entry| entry.max > 0),
    }
  }

  /// 取按上界升序、已去重的归属链；首次访问时把自上次排序以来的追加压实一次。
  ///
  /// manifest replay 循环里 `add_range_li` 逐份追加，这里集中排序把总代价压到
  /// O(M·k·log(M·k))；排序后二分才成立，故 `retain` 一律经此取值。
  ///
  /// 返回 `Rc` 快照而非 `Ref`：排序 / 压实只在函数内部短暂借用，调用方
  /// （`retain`）拿到快照后不再持有任何 `RefCell` 借用。
  pub(crate) fn chain(&self, kind: FileKind) -> Rc<Vec<RangeEntry>> {
    if self.chain_dirty.get() {
      // 两条链共用一个脏标记：标记置位时两边都可能带未排序的追加。
      sort_chain(Rc::make_mut(&mut self.data_li.borrow_mut()));
      sort_chain(Rc::make_mut(&mut self.segment_li.borrow_mut()));
      self.chain_dirty.set(false);
    }
    match kind {
      FileKind::Data => Rc::clone(&self.data_li.borrow()),
      FileKind::Segment => Rc::clone(&self.segment_li.borrow()),
    }
  }

  pub(crate) fn entry_inflight(&self, entry: RangeEntry, file_id: FileId, kind: FileKind) -> bool {
    let guard = &self.guard_li[entry.branch_idx as usize];
    let bound = kind.guard_bound(guard);
    // 对齐 C++ `file_gc.cpp:1207-1208` 的 `term >= newest_term_`：归属区间
    // 的 term 早于 branch 最新 term 时，该区间在上一 term 已 complete。候选
    // 文件自身的 term 不可得（`FileIo` 只列 file id），用归属区间的 term 作
    // 代理；但新 term 的 allocator 从旧上界之后起步，in-flight 文件会落在
    // 区间链中部并归到旧区间上。只有旧区间上界已越过最新 term 的高水位
    // （新 term 明确没有写到这里）时才按旧 term 淘汰，其余仍按 file id 判定。
    if entry.term < guard.newest_term && bound.is_some_and(|bound| entry.max > bound) {
      return false;
    }
    // 该 branch 尚无正高水位（新 term 未写文件）：不占有文件，不参与保留。
    bound.is_some_and(|bound| file_id >= bound)
  }

  pub(crate) fn active_bound(&self, kind: FileKind) -> Option<FileId> {
    self
      .active
      .and_then(|idx| kind.guard_bound(&self.guard_li[idx as usize]))
  }

  pub(crate) fn knowledge(&self, kind: FileKind) -> &RefKnowledge {
    match kind {
      FileKind::Data => &self.data_ref,
      FileKind::Segment => &self.segment_ref,
    }
  }

  pub(crate) fn knowledge_mut(&mut self, kind: FileKind) -> &mut RefKnowledge {
    match kind {
      FileKind::Data => &mut self.data_ref,
      FileKind::Segment => &mut self.segment_ref,
    }
  }

  pub(crate) fn pin_set(&self, kind: FileKind) -> &HashSet<FileId> {
    match kind {
      FileKind::Data => &self.data_pin,
      FileKind::Segment => &self.segment_pin,
    }
  }

  /// 内存 mapping 引用：并入 retained 页集合并把 file id 标为「只可整体保留」
  /// （一次遍历）。mapping 引用的物理变体身份不可见（可能被同 store 的其它
  /// term / branch 同 id 文件遮蔽），变体级删除会让活跃读者拿不到自己的文件。
  pub fn add_pinned_data_mapping(
    &mut self,
    mapping: &MappingTbl,
    retained_set: &mut HashSet<FilePageId>,
    shift: u8,
  ) {
    add_mapping_retained_pinned(mapping, retained_set, shift, &mut self.data_pin);
  }

  /// 同 [`Self::add_pinned_data_mapping`]，segment 侧（独立编号空间）。
  pub fn add_pinned_segment_mapping(
    &mut self,
    mapping: &MappingTbl,
    retained_set: &mut HashSet<FilePageId>,
    shift: u8,
  ) {
    add_mapping_retained_pinned(mapping, retained_set, shift, &mut self.segment_pin);
  }

  /// 活跃快照等外部注册的 retained 页：对应 file id 只可整体保留。
  pub fn pin_data_page_set(&mut self, page_set: &HashSet<FilePageId>, shift: u8) {
    pin_file_id(page_set, shift, &mut self.data_pin);
  }

  /// 同 [`Self::pin_data_page_set`]，segment 侧。
  pub fn pin_segment_page_set(&mut self, page_set: &HashSet<FilePageId>, shift: u8) {
    pin_file_id(page_set, shift, &mut self.segment_pin);
  }

  /// 记录一份 manifest 的引用归属（rev-39）。
  ///
  /// retained file id 经**该 manifest 自带**的区间链解析 `(branch, term)`
  /// （对齐 C++ `GetRetainedFiles` 的 per-manifest 归属——跨 manifest 的链
  /// 视图不同，混用会把引用归到错误的变体）。解析失败的引用不丢弃，整 id
  /// 进入 `attr_failed` 保守保留。
  pub(crate) fn note_refs(
    &mut self,
    kind: FileKind,
    range_li: &[BranchFileRange],
    file_id_iter: impl Iterator<Item = FileId>,
  ) {
    let segment = matches!(kind, FileKind::Segment);
    // 归属链：上界升序；上界 0 说明该 branch 尚无已 flush 文件，不占有
    // 文件，不参与归属（对齐 `retain` 的 `owner_max > 0` 口径）。
    let mut chain: Vec<(&str, u64, FileId)> = range_li
      .iter()
      .map(|range| {
        (
          range.branch_name.as_ref(),
          range.term,
          if segment {
            range.max_segment_file_id
          } else {
            range.max_file_id
          },
        )
      })
      .filter(|(_, _, max)| *max > 0)
      .collect();
    chain.sort_unstable_by_key(|(_, _, max)| *max);
    let knowledge = self.knowledge_mut(kind);
    for file_id in file_id_iter {
      knowledge.ref_ids.insert(file_id);
      // 对齐 C++ `FindBranchRange`：第一个上界 >= file id 的区间为归属。
      let start = chain.partition_point(|(_, _, max)| *max < file_id);
      if start == chain.len() {
        knowledge.attr_failed.insert(file_id);
        continue;
      }
      let owner_max = chain[start].2;
      let entry = knowledge.variants.entry(file_id).or_default();
      for (branch, term, _) in chain[start..]
        .iter()
        .take_while(|(_, _, max)| *max == owner_max)
      {
        if !entry
          .iter()
          .any(|(b, t)| b.as_ref() == *branch && *t == *term)
        {
          entry.push(((*branch).into(), *term));
        }
      }
    }
  }

  /// 变体级保留判定（rev-39）：候选文件自带 `(branch, term)` 身份时使用。
  ///
  /// `referenced` 表示该 file id 出现在 retained 集合（manifest 或外部注册）。
  /// 保留条件按序：
  /// 1. 存在损坏 / 不可读 manifest → 全量保留；
  /// 2. 引用归属命中本变体 → 保留；
  /// 3. 引用归属失败（变体身份未知）→ 保留（sto-54a，不丢引用）；
  /// 4. id 被引用但引用只来自无法归属的外部注册（快照 / 活跃 mapping）→ 保留；
  /// 5. 其余按文件自身 `(branch, term)` 的 in-flight 判定。
  pub fn retain_data_variant(
    &self,
    file_id: FileId,
    branch: &str,
    term: u64,
    referenced: bool,
  ) -> bool {
    self.retain_variant(file_id, branch, term, referenced, FileKind::Data)
  }

  /// 同 [`Self::retain_data_variant`]，segment 侧（独立的编号空间与高水位）。
  pub fn retain_segment_variant(
    &self,
    file_id: FileId,
    branch: &str,
    term: u64,
    referenced: bool,
  ) -> bool {
    self.retain_variant(file_id, branch, term, referenced, FileKind::Segment)
  }

  pub(crate) fn retain_variant(
    &self,
    file_id: FileId,
    branch: &str,
    term: u64,
    referenced: bool,
    kind: FileKind,
  ) -> bool {
    if self.is_damaged() {
      return true;
    }
    // 更高 term 的 writer 已接管：它的文件即使 id 低于本地高水位也不能删。
    if term > self.process_term {
      return true;
    }
    // 外部注册（内存 mapping / 快照）只按 id 引用：物理变体身份不可见，
    // 同 id 全部变体保守保留，绝不按其它 manifest 的归属删掉读者要读的文件。
    if self.pin_set(kind).contains(&file_id) {
      return true;
    }
    if referenced {
      let knowledge = self.knowledge(kind);
      if knowledge.hits(file_id, branch, term)
        || knowledge.attr_failed.contains(&file_id)
        || !knowledge.ref_ids.contains(&file_id)
      {
        return true;
      }
    }
    self.variant_inflight(file_id, branch, term, kind)
  }

  /// 文件自身 `(branch, term)` 的 in-flight 判定，逐项对齐 C++
  /// `file_gc.cpp:1207-1208` 的 `term >= newest_term_ && file_id >=
  /// least_unflushed_file_id_`：旧 term 文件不再被 guard 兜底（eng-88 的
  /// term 维度），只有当前 term 的高水位之上的文件才算在飞。
  ///
  /// guard 缺失时按可删除处理：Rust 的 branch 生命周期保证先有 manifest
  /// 再有文件（fork 先写 branch manifest），缺 guard 的分支不可能有文件；
  /// 与 C++ 「非活跃 branch 缺 guard 保守保留」的差异见 rev-39 记档。
  pub(crate) fn variant_inflight(
    &self,
    file_id: FileId,
    branch: &str,
    term: u64,
    kind: FileKind,
  ) -> bool {
    let Some(idx) = self.branch_map.get(branch).copied() else {
      return false;
    };
    let guard = &self.guard_li[idx as usize];
    let Some(bound) = kind.guard_bound(guard) else {
      return false;
    };
    term >= guard.newest_term && file_id >= bound
  }
}

/// 按区间上界排序并合并同 `(branch, max)`：保留最新 term，二分查找才成立。
pub(crate) fn sort_chain(range_li: &mut Vec<RangeEntry>) {
  range_li.sort_unstable_by_key(|entry| (entry.max, entry.branch_idx, entry.term));
  range_li.dedup_by(|cur, kept| {
    if kept.max == cur.max && kept.branch_idx == cur.branch_idx {
      kept.term = kept.term.max(cur.term);
      true
    } else {
      false
    }
  });
}

pub(crate) const fn max_opt(left: Option<FileId>, right: Option<FileId>) -> Option<FileId> {
  match (left, right) {
    (Some(left), Some(right)) => Some(if left > right { left } else { right }),
    (left, right) => match left {
      Some(_) => left,
      None => right,
    },
  }
}

/// 内存 mapping 的 retained 页 + 对应 file id 的 pin 集合（一次遍历）。
pub(crate) fn add_mapping_retained_pinned(
  mapping: &MappingTbl,
  file_page_set: &mut HashSet<FilePageId>,
  shift: u8,
  pin_set: &mut HashSet<FileId>,
) {
  for value in mapping.iter_value_li() {
    if !is_file_page_id(value) {
      continue;
    }
    let file_page_id = decode_id(value);
    file_page_set.insert(file_page_id);
    pin_set.insert(file_page_id >> shift);
  }
}

pub(crate) fn pin_file_id(
  page_set: &HashSet<FilePageId>,
  shift: u8,
  pin_set: &mut HashSet<FileId>,
) {
  for file_page_id in page_set {
    pin_set.insert(file_page_id >> shift);
  }
}
