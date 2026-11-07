//! 映射快照与登记表：读请求跨 `await` 持有的只读视图、GC retained 并集。

use std::{
  cell::{Cell, RefCell},
  rc::{Rc, Weak},
  sync::LazyLock,
};

use fkv_types::{FilePageId, PageId, TableIdent, slow_read};
use gxhash::HashSet;

use crate::{
  generation::GenerationSource,
  mapper::PageMapper,
  mapping_tbl::{MappingTbl, add_mapping_retained},
};

/// 无 segment mapper 的共享空表，避免每份快照各建一个空 `MappingTbl`。
///
/// `chunk_li` 为 `Arc`，无法 `const` 构造，用 `LazyLock` 延迟初始化一次。
pub(crate) static EMPTY_MAPPING_TBL: LazyLock<MappingTbl> = LazyLock::new(MappingTbl::new);

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
pub(crate) struct SnapshotCore {
  /// 创建顺序号；同一份快照的 clone 共享同一个值。
  pub(crate) generation: u64,
  /// 取出时的映射代际，索引缓存按它拒绝新代节点（27b）。
  pub(crate) mapping_generation: u64,
  pub(crate) root_id: PageId,
  pub(crate) ttl_root_id: PageId,
  pub(crate) data: MappingTbl,
  /// segment 侧只读视图；无 segment mapper 时为 `None`。
  pub(crate) segment: Option<PageMapper>,
  pub(crate) max_fp_id: FilePageId,
  pub(crate) max_segment_fp_id: FilePageId,
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
  /// `open` 顺序号来源。与映射代际分列两个序列，但通过
  /// [`Self::generation_source`] 暴露给 `PageMapper`，让 Pooled 页 retire
  /// 时能记录「释放时刻已发出的最大快照代际」。
  generation: GenerationSource,
}

/// 注册表惰性摘链的最小触发长度：低于该长度不扫描，登记摊还 O(1)。
pub(crate) const PRUNE_MIN_LEN: usize = 8;

impl SnapshotRegistry {
  pub fn new() -> Self {
    Self::default()
  }

  /// 登记一份新快照并返回读请求持有的句柄；调用 `drop` 即释放登记。
  pub fn open(&self, tbl: &TableIdent, source: SnapshotSource<'_>) -> MappingSnapshot {
    let open_start = slow_read::start();
    let generation = self.generation.next_generation();
    let core = Rc::new(SnapshotCore {
      generation,
      mapping_generation: source.data.generation(),
      root_id: source.root_id,
      ttl_root_id: source.ttl_root_id,
      data: source.data.tbl().clone(),
      segment: source.segment.map(|mapper| {
        PageMapper::read_view_with_source(
          mapper.tbl().clone(),
          source.data.generation_source.clone(),
          mapper.max_fp_id(),
        )
      }),
      max_fp_id: source.data.max_fp_id(),
      max_segment_fp_id: source.segment.map_or(0, PageMapper::max_fp_id),
    });
    let snapshot = MappingSnapshot { core: core.clone() };
    let mut entry_li = self.entry_li.borrow_mut();
    // 增量登记：条目数达到阈值才整表摘链，摊还 O(1)；死弱引用最多累积到
    // 约两倍存活数。GC retained / 诊断计数仍每次精确摘链。
    if entry_li.len() >= self.prune_len.get() {
      let prune_start = slow_read::start();
      prune_dead(&mut entry_li);
      self.prune_len.set((entry_li.len() * 2).max(PRUNE_MIN_LEN));
      slow_read::stop("snapshot_prune", prune_start);
    }
    entry_li.push((tbl.clone(), Rc::downgrade(&core)));
    drop(entry_li);
    slow_read::stop("snapshot_open", open_start);
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

  /// 快照代际来源；交给同一 Store 的 `PageMapper`（
  /// [`PageMapper::set_snapshot_generation_source`]）后，Pooled 页在 retire
  /// 时才能记录释放时刻的快照代际，`reclaim_retired` 才能与
  /// [`Self::min_live_generation`] 配合判断何时可安全复用。
  ///
  /// 同一个来源可被同一 Store 的多个 mapper 克隆共享；跨 Store 不得共用。
  pub fn generation_source(&self) -> GenerationSource {
    self.generation.clone()
  }

  /// 当前仍存活快照的最小 `generation`；无存活快照时返回 `u64::MAX`。
  ///
  /// 作为 [`PageMapper::reclaim_retired`] 的 watermark：`retired_gen` 小于它
  /// 说明所有可能引用该页的旧快照都已 drop，页可安全回池复用。
  ///
  /// 与 retained 并集一样惰性摘链（弱引用失效即视为快照已 drop），因此
  /// 调用本身会顺带清理死条目。无存活快照时 `u64::MAX` 让所有 `retired_gen`
  /// （最大不超过已发出的代际）都满足 `< watermark`。
  pub fn min_live_generation(&self) -> u64 {
    let mut entry_li = self.entry_li.borrow_mut();
    prune_dead(&mut entry_li);
    self.prune_len.set((entry_li.len() * 2).max(PRUNE_MIN_LEN));
    entry_li
      .iter()
      .filter_map(|(_, weak)| weak.upgrade())
      .map(|core| core.generation)
      .min()
      .unwrap_or(u64::MAX)
  }

  pub(crate) fn retained_into(
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
pub(crate) fn prune_dead(entry_li: &mut Vec<(TableIdent, Weak<SnapshotCore>)>) {
  entry_li.retain(|(_, weak)| weak.strong_count() != 0);
}
