//! pooled-07：Pooled 复用下界（sticky floor）。
//!
//! branch / archive 是持久化的页集合快照，不在读快照注册表里；回收与 reopen
//! 回填都必须尊重由 manifest 重建的下界，低于下界的物理页永不回池、永不分配。

use fkv_mapping::{AllocMode, GenerationSource, MappingTbl, PageMapper};
use fkv_types::encode_file_page_id;

fn tbl_with(used: &[u64]) -> MappingTbl {
  let mut tbl = MappingTbl::new();
  for &file_page_id in used {
    tbl.push(encode_file_page_id(file_page_id));
  }
  tbl
}

/// 抬高下界时：空闲池与 retired 队列同步裁剪，高水位不低于下界。
#[test]
fn raise_floor_prunes_pool_and_retired() {
  let mut mapper = PageMapper::with_tbl_and_source(
    tbl_with(&[0, 1, 5]),
    GenerationSource::new(),
    AllocMode::Pooled,
    8,
  );
  // 空闲池 = [2, 5) ∪ [6, 8)。
  assert_eq!(mapper.pooled_free_len(), 5);
  assert_eq!(mapper.reuse_floor(), 0);

  mapper.raise_reuse_floor(6);
  assert_eq!(mapper.reuse_floor(), 6);
  assert_eq!(mapper.pooled_free_len(), 2, "低于下界的空闲区间必须丢弃");
  assert!(mapper.max_fp_id() >= 6);

  let page = mapper.allocate_page();
  assert_eq!(mapper.file_page_id(page).unwrap(), 7, "分配不得低于下界");
}

/// 下界以下的退役页不进 retired 队列；下界以上的仍按快照水位正常回收。
#[test]
fn raise_floor_filters_retire_and_reclaim() {
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  let low = mapper.allocate_page();
  assert_eq!(mapper.file_page_id(low).unwrap(), 0);

  mapper.raise_reuse_floor(4);
  let new_fp = mapper.remap_page(low).unwrap();
  assert!(new_fp >= 4, "COW 新页必须 >= 下界: {new_fp}");
  assert_eq!(
    mapper.retired_len(),
    0,
    "下界以下的旧页不得进入 retired 队列"
  );
  assert_eq!(mapper.reclaim_retired(u64::MAX), 0);

  // 下界以上的页仍正常退役 / 回收。
  let high = mapper.allocate_page();
  let high_fp = mapper.file_page_id(high).unwrap();
  assert!(high_fp >= 4);
  mapper.remap_page(high).unwrap();
  assert_eq!(mapper.retired_len(), 1, "下界以上的页仍进入 retired 队列");
  assert_eq!(mapper.reclaim_retired(u64::MAX), 1);
}

/// reopen 回填空闲池时从下界起算：低于下界的映射空洞不可复用。
#[test]
fn reopen_rebuild_starts_at_floor() {
  let used = [0u64, 6, 7];
  let mut mapper = PageMapper::with_tbl_and_source(
    tbl_with(&used),
    GenerationSource::new(),
    AllocMode::Pooled,
    16,
  );
  assert_eq!(mapper.pooled_free_len(), 13, "未设下界时回填全部空洞");

  mapper.raise_reuse_floor(8);
  assert_eq!(
    mapper.pooled_free_len(),
    8,
    "只保留 [8, 16) 的空闲页位（8..16 未引用）"
  );
  // 从高往低分配，全部 >= 下界。
  let taken: Vec<u64> = (0..4)
    .map(|_| {
      let page_id = mapper.allocate_page();
      mapper.file_page_id(page_id).unwrap()
    })
    .collect();
  assert_eq!(taken, vec![15, 14, 13, 12]);
  assert!(taken.iter().all(|id| *id >= 8));
}

/// Append 模式没有池化复用，下界调用是 no-op。
#[test]
fn append_ignores_raise_floor() {
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  mapper.raise_reuse_floor(4);
  assert_eq!(mapper.reuse_floor(), 0);
  assert_eq!(mapper.max_fp_id(), 0);
  let page = mapper.allocate_page();
  assert_eq!(mapper.file_page_id(page).unwrap(), 0);
}

/// 下界高于当前高水位时跳过未分配区间，第一次分配即落在下界上。
#[test]
fn floor_above_high_water_skips_range() {
  let mut mapper = PageMapper::new(AllocMode::Pooled, 2);
  mapper.raise_reuse_floor(9);
  assert_eq!(mapper.max_fp_id(), 9);
  let page = mapper.allocate_page();
  assert_eq!(mapper.file_page_id(page).unwrap(), 9);
  assert_eq!(mapper.pooled_free_len(), 0, "跳过的区间不属于可复用空闲页");
  // 预留跳过的区间仍按文件归属记入高水位，不产生空闲池条目。
  assert_eq!(mapper.pooled_free_range_cnt(), 0);
}

/// 下界只升不降：后续更小的 floor（例如删除归档后试图回收）被忽略。
#[test]
fn floor_is_sticky() {
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  mapper.raise_reuse_floor(6);
  mapper.raise_reuse_floor(3);
  assert_eq!(mapper.reuse_floor(), 6);
}
