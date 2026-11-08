//! Pooled 物理页回收：retired 队列 + 快照代际水位测试。
//!
//! 语义：Pooled 模式下 `remap_page` / `free_page` 只把旧物理页放入 retired
//! 队列并记录释放时刻的快照代际；只有 `retired_gen < min_live_generation`
//! 的页才会被 `reclaim_retired` 归还 `FilePagePool` 并允许 `allocate` 复用。

use fkv_mapping::{AllocMode, PageMapper, SnapshotRegistry, SnapshotSource};
use fkv_types::{NULL_PAGE, PageId, TableIdent, encode_file_page_id};

fn tbl() -> TableIdent {
  TableIdent::new("retire", 1).unwrap()
}

fn source(mapper: &PageMapper, root_id: PageId) -> SnapshotSource<'_> {
  SnapshotSource {
    data: mapper,
    segment: None,
    root_id,
    ttl_root_id: NULL_PAGE,
  }
}

/// 无存活快照：水位为 `u64::MAX`，reclaim 后物理页立即可复用。
#[test]
fn no_snapshot_reclaim_makes_page_reusable() {
  let registry = SnapshotRegistry::new();
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  mapper.set_snapshot_generation_source(registry.generation_source());

  let page = mapper.allocate_page();
  assert_eq!(mapper.file_page_id(page).unwrap(), 0);
  let old_fp = mapper.remap_page(page).unwrap();
  assert_eq!(old_fp, 1, "remap 分配新物理页");
  assert_eq!(mapper.retired_len(), 1, "旧物理页进入 retired 队列");
  assert_eq!(registry.min_live_generation(), u64::MAX);

  assert_eq!(mapper.reclaim_retired(registry.min_live_generation()), 1);
  assert_eq!(mapper.retired_len(), 0);

  let next = mapper.allocate_page();
  assert_eq!(
    mapper.file_page_id(next).unwrap(),
    0,
    "回收后的旧物理页可被下一次分配复用"
  );
}

/// 旧快照存活：`min_live_generation` 阻止 reclaim，`allocate` 不复用该页。
#[test]
fn live_snapshot_blocks_reclaim_and_reuse() {
  let registry = SnapshotRegistry::new();
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  mapper.set_snapshot_generation_source(registry.generation_source());

  let page = mapper.allocate_page();
  let old_fp = mapper.file_page_id(page).unwrap();
  let snapshot = registry.open(&tbl(), source(&mapper, page));
  assert_eq!(snapshot.generation(), 1);
  assert_eq!(registry.min_live_generation(), 1);

  let new_fp = mapper.remap_page(page).unwrap();
  assert_ne!(new_fp, old_fp);
  assert_eq!(mapper.retired_len(), 1);

  // 旧快照仍可能按 old_fp 读取：水位 1 不能回收 retire 代际 1 的页。
  assert_eq!(mapper.reclaim_retired(registry.min_live_generation()), 0);
  assert_eq!(mapper.retired_len(), 1);

  // 分配继续取新页，绝不会取回 retired 的 old_fp。
  let next = mapper.allocate_page();
  assert_eq!(mapper.file_page_id(next).unwrap(), 2);
  assert_ne!(mapper.file_page_id(next).unwrap(), old_fp);

  // 快照读到的仍是旧映射。
  assert_eq!(
    snapshot.data_tbl().get(page).unwrap(),
    encode_file_page_id(old_fp)
  );
}

/// 快照 drop 后水位回到 `u64::MAX`，下一次 reclaim 即可复用。
#[test]
fn snapshot_drop_enables_reclaim_on_next_round() {
  let registry = SnapshotRegistry::new();
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  mapper.set_snapshot_generation_source(registry.generation_source());

  let page = mapper.allocate_page();
  let snapshot = registry.open(&tbl(), source(&mapper, page));
  let old_fp = mapper.remap_page(page).unwrap();
  assert_eq!(old_fp, 1);
  assert_eq!(mapper.reclaim_retired(registry.min_live_generation()), 0);
  assert_eq!(mapper.retired_len(), 1);

  drop(snapshot);
  // 惰性摘链：无存活快照时直接返回 MAX，无需另行调用 prune。
  assert_eq!(registry.min_live_generation(), u64::MAX);
  assert_eq!(registry.entry_num(), 0);

  assert_eq!(mapper.reclaim_retired(registry.min_live_generation()), 1);
  assert_eq!(mapper.retired_len(), 0);
  let next = mapper.allocate_page();
  assert_eq!(
    mapper.file_page_id(next).unwrap(),
    0,
    "快照 drop 后旧物理页回池并可复用"
  );
}

/// COW：`remap_page` 先 `allocate` 新页，再把旧页 retire，绝不原地覆盖。
#[test]
fn remap_allocates_new_page_before_retiring_old() {
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  let page0 = mapper.allocate_page();
  let page1 = mapper.allocate_page();
  let old_fp = mapper.file_page_id(page1).unwrap();
  assert_eq!(old_fp, 1);

  // 先制造一个可复用的空闲物理页 0，验证 remap 会分配已有空闲页而不是原地写 1。
  assert_eq!(mapper.free_page(page0).unwrap(), Some(0));
  assert_eq!(mapper.reclaim_retired(u64::MAX), 1);

  let new_fp = mapper.remap_page(page1).unwrap();
  assert_eq!(new_fp, 0, "新页来自空闲池，而不是旧页本身");
  assert_ne!(new_fp, old_fp);
  assert_eq!(mapper.file_page_id(page1).unwrap(), new_fp);
  assert_eq!(
    mapper.retired_len(),
    1,
    "旧物理页 1 在分配之后才 retire，未回池"
  );

  // 旧页在 reclaim 之前不会被复用。
  let page2 = mapper.allocate_page();
  assert_eq!(mapper.file_page_id(page2).unwrap(), 2);
  assert_ne!(mapper.file_page_id(page2).unwrap(), old_fp);
}

/// Append 模式：旧页本就不复用，retire / reclaim 都是 no-op。
#[test]
fn append_mode_retire_is_noop() {
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let page = mapper.allocate_page();
  let old_fp = mapper.file_page_id(page).unwrap();
  let new_fp = mapper.remap_page(page).unwrap();
  assert_ne!(new_fp, old_fp);
  assert_eq!(mapper.retired_len(), 0);

  assert!(mapper.free_page(page).unwrap().is_some());
  assert_eq!(mapper.retired_len(), 0);
  assert_eq!(mapper.reclaim_retired(u64::MAX), 0);
}

/// 批次回滚必须撤销批次内 retire 的页，否则回滚后的活跃页会被回池复用。
#[test]
fn abort_batch_restores_retired_queue() {
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  let page = mapper.allocate_page();
  let old_fp = mapper.file_page_id(page).unwrap();

  mapper.begin_batch();
  let new_fp = mapper.remap_page(page).unwrap();
  assert_ne!(new_fp, old_fp);
  assert_eq!(mapper.retired_len(), 1);
  assert_eq!(
    mapper.reclaim_retired(u64::MAX),
    0,
    "批次进行中不回收，保持 abort 的栈式回滚点"
  );
  assert!(mapper.abort_batch());

  assert_eq!(
    mapper.retired_len(),
    0,
    "回滚后旧页重新活跃，不能留在 retired"
  );
  assert_eq!(mapper.reclaim_retired(u64::MAX), 0);
  assert_eq!(mapper.file_page_id(page).unwrap(), old_fp);
}

/// `reserve_until` 跳过区间的 retired 页不得回池复用（branch 预留语义）。
#[test]
fn reserve_until_discards_retired_pages_below_limit() {
  let mut mapper = PageMapper::new(AllocMode::Pooled, 10);
  let page = mapper.allocate_page();
  assert_eq!(mapper.file_page_id(page).unwrap(), 10);
  assert_eq!(mapper.free_page(page).unwrap(), Some(10));
  assert_eq!(mapper.retired_len(), 1);

  mapper.reserve_until(11);
  assert_eq!(mapper.retired_len(), 0, "预留区间内的 retired 页不再回池");
  assert_eq!(mapper.reclaim_retired(u64::MAX), 0);

  let next = mapper.allocate_page();
  assert_eq!(mapper.file_page_id(next).unwrap(), 11);
}

/// `min_live_generation` 取最老存活快照，且惰性摘链。
#[test]
fn min_live_generation_tracks_oldest_snapshot() {
  let registry = SnapshotRegistry::new();
  let mapper = PageMapper::new(AllocMode::Append, 0);
  let tbl = tbl();

  assert_eq!(registry.min_live_generation(), u64::MAX);
  let first = registry.open(&tbl, source(&mapper, 0));
  let second = registry.open(&tbl, source(&mapper, 0));
  assert_eq!(first.generation(), 1);
  assert_eq!(second.generation(), 2);
  assert_eq!(registry.min_live_generation(), 1);

  drop(first);
  assert_eq!(
    registry.min_live_generation(),
    2,
    "摘链后以仍存活的最老快照为准"
  );
  drop(second);
  assert_eq!(registry.min_live_generation(), u64::MAX);
  assert_eq!(registry.entry_num(), 0);
}

/// retire 之后才打开的快照不可能引用旧页，不应阻塞回收。
#[test]
fn snapshot_opened_after_retire_does_not_block_reclaim() {
  let registry = SnapshotRegistry::new();
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  mapper.set_snapshot_generation_source(registry.generation_source());

  let page = mapper.allocate_page();
  let new_fp = mapper.remap_page(page).unwrap();
  assert_eq!(mapper.retired_len(), 1);

  let snapshot = registry.open(&tbl(), source(&mapper, page));
  assert_eq!(snapshot.generation(), 1);
  assert_eq!(
    snapshot.data_tbl().get(page).unwrap(),
    encode_file_page_id(new_fp)
  );

  // retired 代际 0 < 水位 1：新快照看的是新映射，旧页可安全回池。
  assert_eq!(mapper.reclaim_retired(registry.min_live_generation()), 1);
  assert_eq!(mapper.retired_len(), 0);
}
