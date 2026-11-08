//! MappingSnapshot / journal 语义测试（自 src 内联迁出；被测项全部是 pub API）。

use fkv_mapping::{
  AllocMode, CHUNK_SIZE, MappingTbl, PageMapper, SnapshotRegistry, SnapshotSource,
  chunk_dir_copy_num,
};
use fkv_types::{INVALID_VALUE, NULL_PAGE, PageId, TableIdent, encode_file_page_id};
use gxhash::HashSet;

fn source(mapper: &PageMapper, root_id: PageId) -> SnapshotSource<'_> {
  SnapshotSource {
    data: mapper,
    segment: None,
    root_id,
    ttl_root_id: NULL_PAGE,
  }
}

#[test]
fn snapshot_is_cow_isolated_and_registry_prunes_on_drop() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let other = TableIdent::new("demo", 2).unwrap();
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let page = mapper.allocate_page();
  let old_fp = mapper.file_page_id(page).unwrap();
  let old_value = mapper.tbl().get(page).unwrap();

  let registry = SnapshotRegistry::new();
  let snapshot = registry.open(&tbl, source(&mapper, page));
  assert_eq!(snapshot.generation(), 1);
  assert_eq!(snapshot.mapping_generation(), mapper.generation());
  assert_eq!(snapshot.root_id(), page);
  assert_eq!(snapshot.max_fp_id(), 1);

  let mut retained = HashSet::default();
  registry.data_retained_into(&tbl, &mut retained);
  assert_eq!(retained.len(), 1);
  assert!(retained.contains(&old_fp));
  // 别的表不并集：FilePageId 命名空间按表独立。
  let mut other_retained = HashSet::default();
  registry.data_retained_into(&other, &mut other_retained);
  assert!(other_retained.is_empty());
  assert_eq!(registry.live_num(&tbl), 1);
  assert_eq!(registry.live_num(&other), 0);

  // 写入路径 remap 后，快照仍看到取出时刻的旧物理页。
  let new_fp = mapper.remap_page(page).unwrap();
  assert_ne!(new_fp, old_fp);
  assert_eq!(snapshot.data_tbl().get(page).unwrap(), old_value);

  // clone 共享同一份快照，最后一份 drop 才摘链。
  let held = snapshot.clone();
  drop(snapshot);
  assert_eq!(registry.live_num(&tbl), 1);
  drop(held);
  assert_eq!(registry.live_num(&tbl), 0);
  retained.clear();
  registry.data_retained_into(&tbl, &mut retained);
  assert!(retained.is_empty(), "drop 后不得再并集旧页");
}

#[test]
fn abort_restores_mapping_and_allocator() {
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let page = mapper.allocate_page();
  let base_fp = mapper.file_page_id(page).unwrap();
  let next = mapper.allocate_page();
  let before_len = mapper.tbl().len();
  let before_max = mapper.max_fp_id();
  let before_value = mapper.tbl().get(page).unwrap();

  mapper.begin_batch();
  let new_fp = mapper.remap_page(page).unwrap();
  assert_ne!(new_fp, base_fp);
  let next_fp = mapper.file_page_id(next).unwrap();
  assert!(mapper.free_page(next).unwrap().is_some());
  assert!(mapper.file_page_id(next).is_err());
  mapper.abort_batch();

  assert_eq!(mapper.tbl().len(), before_len);
  assert_eq!(mapper.max_fp_id(), before_max);
  assert_eq!(mapper.tbl().get(page).unwrap(), before_value);
  assert_eq!(mapper.file_page_id(page).unwrap(), base_fp);
  assert_eq!(mapper.file_page_id(next).unwrap(), next_fp);
}

#[test]
fn commit_keeps_changes() {
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let page = mapper.allocate_page();
  mapper.begin_batch();
  let new_fp = mapper.remap_page(page).unwrap();
  mapper.commit_batch();
  assert_eq!(mapper.file_page_id(page).unwrap(), new_fp);
}

/// rev-45：大库（≥ 1e3 chunk）上 open 只共享 chunk 指针表，不逐 chunk 复制。
#[test]
fn snapshot_open_shares_chunk_dir_in_o1() {
  let tbl = TableIdent::new("demo", 3).unwrap();
  let mut mapping = MappingTbl::new();
  let page_cnt = CHUNK_SIZE * 1_000 + 7;
  let last_page = (page_cnt - 1) as PageId;
  mapping.resize(page_cnt, INVALID_VALUE);
  let first_fp = encode_file_page_id(11);
  let last_fp = encode_file_page_id(12);
  mapping.set(0, first_fp);
  mapping.set(last_page, last_fp);
  let mapper = PageMapper::read_view(mapping, 0);
  let registry = SnapshotRegistry::new();
  let copy_before = chunk_dir_copy_num();

  let snapshot_li: Vec<_> = (0..64)
    .map(|_| registry.open(&tbl, source(&mapper, 0)))
    .collect();
  assert_eq!(
    chunk_dir_copy_num(),
    copy_before,
    "open 不得复制 chunk 指针表"
  );
  assert_eq!(mapper.tbl().chunk_dir_share_cnt(), snapshot_li.len() + 1);
  assert_eq!(snapshot_li[0].data_tbl().get(0).unwrap(), first_fp);
  assert_eq!(snapshot_li[0].data_tbl().get(last_page).unwrap(), last_fp);

  drop(snapshot_li);
  assert_eq!(mapper.tbl().chunk_dir_share_cnt(), 1);
}

/// rev-45：快照存活时首次写只复制一次指针表（之后独占），快照内容不变。
#[test]
fn mapper_mutation_copies_shared_chunk_dir_once() {
  let tbl = TableIdent::new("demo", 4).unwrap();
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let page = mapper.allocate_page();
  let old_value = mapper.tbl().get(page).unwrap();
  let registry = SnapshotRegistry::new();
  let snapshot = registry.open(&tbl, source(&mapper, page));
  assert_eq!(mapper.tbl().chunk_dir_share_cnt(), 2);

  let copy_before = chunk_dir_copy_num();
  mapper.remap_page(page).unwrap();
  assert_eq!(
    chunk_dir_copy_num(),
    copy_before + 1,
    "共享指针表的首次写只复制一次"
  );
  assert_eq!(mapper.tbl().chunk_dir_share_cnt(), 1);
  assert_eq!(snapshot.data_tbl().get(page).unwrap(), old_value);

  // 指针表已独占：后续写不再复制指针表。
  let next = mapper.allocate_page();
  mapper.remap_page(next).unwrap();
  assert_eq!(chunk_dir_copy_num(), copy_before + 1);
  assert_eq!(snapshot.data_tbl().len(), 1);
}

/// rev-45：注册表按阈值增量摘链，长期 open/drop 不随请求数线性增长。
#[test]
fn registry_registration_is_incremental() {
  let tbl = TableIdent::new("demo", 5).unwrap();
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let page = mapper.allocate_page();
  let registry = SnapshotRegistry::new();
  let held = registry.open(&tbl, source(&mapper, page));
  for _ in 0..1_000 {
    drop(registry.open(&tbl, source(&mapper, page)));
  }
  // 先看原始条目数：最后一次 open 刚 push 完，死弱引用尚未被强制摘链。
  assert!(
    registry.entry_num() <= 8,
    "死弱引用应按阈值增量摘链，实际 {}",
    registry.entry_num()
  );
  assert_eq!(registry.live_num(&tbl), 1);
  drop(held);
  assert_eq!(registry.live_num(&tbl), 0);
}
