use fkv_gc::{compact_plan, segment_compact_plan, should_compact};
use gxhash::HashMap;

#[test]
fn compaction_threshold() {
  assert!(should_compact(10, 21, 2));
  assert!(!should_compact(10, 20, 2));
  assert!(!should_compact(0, 20, 0));
}

/// sto-56：一次产出全部超阈值文件，按放大率降序；空文件与未超阈值文件不入计划。
#[test]
fn compact_plan_covers_all_amplified_files() {
  let mut live_map = HashMap::default();
  live_map.insert(0, 10u64); // 100/10 = 10x
  live_map.insert(1, 50); // 2x，恰好等于阈值，不入选
  live_map.insert(2, 20); // 5x
  live_map.insert(3, 0); // 空文件交给 GC
  live_map.insert(4, 4); // 25x

  assert_eq!(
    compact_plan(&live_map, &[0, 1, 2, 3, 4], 100, 2, 99),
    vec![4, 0, 2]
  );
  assert!(compact_plan(&live_map, &[0, 1, 2, 3, 4], 100, 0, 99).is_empty());
  // 尾文件即使放大也跳过，其余规则不变。
  assert_eq!(
    compact_plan(&live_map, &[0, 1, 2, 3, 4], 100, 2, 4),
    vec![0, 2]
  );
}

/// 盘上最大 file id 小于分配器尾（空文件已被 GC 删）：已写满的满文件仍可 compact。
#[test]
fn data_plan_uses_allocator_tail_not_disk_max() {
  let mut live_map = HashMap::default();
  live_map.insert(2, 1u64); // 100/1 = 100x，已写满待搬
  live_map.insert(7, 1); // 分配器尾，未写满，必须跳过

  assert_eq!(compact_plan(&live_map, &[2, 7], 100, 2, 7), vec![2]);
  // 尾文件不在候选列表时不影响其它候选。
  assert_eq!(compact_plan(&live_map, &[2], 100, 2, 9), vec![2]);
}

/// sto-56b：segment 候选跳过分配器追加尾文件，其余规则与 data 相同。
#[test]
fn segment_plan_skips_tail_file() {
  let mut live_map = HashMap::default();
  live_map.insert(0, 4u64); // 32/4 = 8x
  live_map.insert(1, 0); // 空文件交给 GC
  live_map.insert(2, 2); // 16x
  live_map.insert(3, 1); // 尾文件，即使放大也必须跳过

  let candidate_li = [3, 0, 2, 1];
  assert_eq!(
    segment_compact_plan(&live_map, &candidate_li, 32, 2, 3),
    vec![2, 0]
  );
  assert!(segment_compact_plan(&live_map, &candidate_li, 32, 0, 3).is_empty());
  // 只有一个文件时它就是尾文件，不能入选。
  assert!(segment_compact_plan(&live_map, &[3], 32, 2, 3).is_empty());
  assert!(segment_compact_plan(&live_map, &[], 32, 2, 3).is_empty());
}

/// 盘上最大 file id 小于分配器尾（空文件已被 GC 删）：满文件仍必须可 compact。
#[test]
fn segment_plan_uses_allocator_tail_not_disk_max() {
  let mut live_map = HashMap::default();
  live_map.insert(2, 1u64); // 32/1 = 32x，已写满待搬
  live_map.insert(7, 1); // 分配器尾，未写满，必须跳过
  let candidate_li = [2, 7];

  assert_eq!(
    segment_compact_plan(&live_map, &candidate_li, 32, 2, 7),
    vec![2]
  );
  // 尾文件不在候选列表时也不影响其它候选。
  assert_eq!(segment_compact_plan(&live_map, &[2], 32, 2, 9), vec![2]);
}
