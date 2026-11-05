//! in-flight 高水位：per-branch 边界折叠与归属判定。

use fkv_task_gc::InflightGuard;
use fkv_types::BranchFileRange;

fn range(branch: &str, term: u64, max_file_id: u64, max_segment_file_id: u64) -> BranchFileRange {
  BranchFileRange {
    branch_name: branch.into(),
    term,
    max_file_id,
    max_segment_file_id,
  }
}

#[test]
fn guard_keeps_files_at_or_above_latest_bound() {
  let mut guard = InflightGuard::default();
  guard.add_range_li(&[range("main", 3, 10, 0)]);
  guard.fold("main", 3, Some(10), None);
  guard.seed("main", 3, 10 << 2, 2);

  // 旧文件（已 flush）可删，边界文件及其后的未提交文件必须保留。
  assert!(!guard.retain_data(9));
  assert!(guard.retain_data(10));
  assert!(guard.retain_data(11));
}

#[test]
fn guard_folds_by_latest_term_and_max_file() {
  let mut guard = InflightGuard::default();
  guard.fold("feature", 1, Some(2048), Some(4));
  guard.fold("feature", 2, Some(2050), Some(5));
  // 旧 term 的区间不抬高新 term 的边界。
  guard.fold("feature", 1, Some(3000), Some(9));
  guard.fold("feature", 2, Some(2051), Some(6));
  guard.seed("feature", 2, 2051 << 2, 2);

  assert!(!guard.retain_segment(5));
  assert!(guard.retain_segment(6));
  assert!(!guard.retain_data(2050));
  assert!(guard.retain_data(2051));
}

#[test]
fn guard_attributes_file_id_to_owning_branch_range() {
  let mut guard = InflightGuard::default();
  // main 让出 [1024, 2048) 给 feature，自己继续从 2048 写。
  guard.add_range_li(&[range("main", 1, 2048, 0)]);
  guard.add_range_li(&[range("feature", 0, 1024, 0)]);
  guard.fold("main", 1, Some(2048), None);
  guard.fold("feature", 0, Some(1024), None);
  guard.seed("main", 1, 2048 << 2, 2);

  // feature 块的 in-flight 文件归 feature 守卫；main 块归 main。
  assert!(guard.retain_data(1024));
  assert!(!guard.retain_data(1023));
  assert!(guard.retain_data(2050));
}

/// 同高水位的多个 branch（父 branch 预留下界 == 子 branch 起始）不依赖
/// manifest 读取顺序：任一 branch 判定 in-flight 即保留。
#[test]
fn guard_resolves_tied_high_water_order_independently() {
  let mut forward = InflightGuard::default();
  forward.add_range_li(&[range("main", 0, 8, 0)]);
  forward.add_range_li(&[range("feature", 0, 8, 0)]);
  forward.fold("main", 0, Some(16), None);
  forward.fold("feature", 0, Some(8), None);
  forward.seed("main", 0, 24 << 2, 2);

  let mut backward = InflightGuard::default();
  backward.add_range_li(&[range("feature", 0, 8, 0)]);
  backward.add_range_li(&[range("main", 0, 8, 0)]);
  backward.fold("main", 0, Some(16), None);
  backward.fold("feature", 0, Some(8), None);
  backward.seed("main", 0, 24 << 2, 2);

  // main 已 flush 到 16，feature 的边界正好是 8：8 及其后是 feature 的
  // 未 flush 写入，两种折入顺序都必须保留。
  for guard in [&forward, &backward] {
    assert!(guard.retain_data(8));
    assert!(guard.retain_data(9));
  }
}

/// r7a-core-04：区间链惰性排序必须在每次追加后重新置脏。
///
/// 第一次 `retain_data` 触发排序后，再追加一条上界更小的区间；若脏标记没有
/// 重新置位，链停在「[16, 12]」这种未排序状态，二分把 14 归到链尾区间，
/// 走链尾兜底而误保留已可删的文件。
#[test]
fn chain_resorts_after_late_append() {
  let mut guard = InflightGuard::default();
  // main 已 flush 到 16；第一次查询把单元素链排好并清掉脏标记。
  guard.add_range_li(&[range("main", 0, 16, 0)]);
  guard.fold("main", 0, Some(16), None);
  assert!(guard.retain_data(16));
  // 追加更小上界的 feature 区间：链变成 [16, 12]；随后 feature 又 flush 到 20，
  // 该区间内的文件都已不是 in-flight。
  guard.add_range_li(&[range("feature", 0, 12, 0)]);
  guard.fold("feature", 0, Some(20), None);
  // 14 归 feature 区间（上界 12）且 feature 已 flush 到 20，链上还有更高的
  // main 区间（16）覆盖它：排序后二分判为可删；未重排则落到链尾兜底误保留。
  assert!(!guard.retain_data(14), "被更新区间覆盖的文件应可删");
  assert!(guard.retain_data(16), "main 的未 flush 边界仍保留");
}

/// term 维度参与归属判定：file id 落在区间链中部（100 < 150 < 200），归属到
/// feature 旧 term(5) 的区间；该 branch 已进入 term 9 且只写到 80，旧区间已越过
/// 新 term 高水位，按 C++ `term >= newest_term_` 条件应淘汰。
#[test]
fn guard_ignores_mid_chain_range_from_older_term() {
  let mut guard = InflightGuard::default();
  guard.add_range_li(&[range("feature", 5, 100, 0), range("main", 2, 200, 0)]);
  // feature 新 term 的 manifest 只推进到 80：旧区间（max 100）落在链中部。
  guard.add_range_li(&[range("feature", 9, 80, 0)]);
  guard.seed("feature", 9, 80, 0);

  // 只看 file id 会误判 in-flight（150 ≥ 80）；旧 term 区间应淘汰。
  assert!(!guard.retain_data(150));
  // 链尾之上的文件仍由 active 高水位保护。
  assert!(guard.retain_data(250));
}

/// term 更新但新 term 尚未落 manifest（只有 in-memory seed）时，file id 落在
/// 区间链中部的 in-flight 文件不能按旧区间 term 淘汰：它属于新 term，旧区间
/// 上界（100）还没越过高水位（101）。
#[test]
fn guard_keeps_mid_chain_file_of_unflushed_new_term() {
  let mut guard = InflightGuard::default();
  guard.add_range_li(&[range("feature", 5, 100, 0), range("main", 2, 200, 0)]);
  // term 9 的 allocator 从旧上界之后起步，第一笔尚未 flush。
  guard.seed("feature", 9, 101, 0);

  assert!(guard.retain_data(101));
}

/// branch 尚无正高水位（区间上界 0）时保守保留，避免删掉它尚未 flush 的
/// 首批文件；本进程没有 active branch 时同样保守保留高水位之上的文件。
#[test]
fn guard_conservative_when_bound_or_active_missing() {
  let mut guard = InflightGuard::default();
  // feature 刚创建、manifest 只记到 0；main 已推进到 16。
  guard.add_range_li(&[range("main", 0, 16, 0), range("feature", 0, 0, 0)]);
  guard.fold("main", 0, Some(16), None);

  // 低于全部正高水位，且 feature 无边界：保守保留。
  assert!(guard.retain_data(3));
  // 超过全部高水位、本进程无 active branch（表未打开）：保守保留。
  assert!(guard.retain_data(17));

  // 完全没有区间信息的调用方维持原语义：不额外保留。
  assert!(!InflightGuard::default().retain_data(0));
}
