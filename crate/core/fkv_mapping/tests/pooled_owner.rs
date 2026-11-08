//! Pooled reopen 空闲池的 branch 归属过滤（对齐 C++ `Replayer::GetMapper` 的
//! `FileIdInBranch`）：只回填当前 manifest branch 名下的物理页。
//!
//! 背景：`pooled-05`。branch 继承父映射后，映射里的空洞包含父 branch 的空闲页；
//! 若把它们回填进 branch 的空闲池，branch 分配后会按 file id 写到父 branch 的
//! `(branch, term)` 命名空间，覆盖父的活数据。

use fkv_mapping::{AllocMode, BranchPageOwner, GenerationSource, MappingTbl, PageMapper};
use fkv_types::{BranchFileRange, encode_file_page_id};

/// 4 页 / file，便于用小 id 覆盖多个文件区间。
const SHIFT: u8 = 2;
/// 5 个 file（0..=4）的 page 空间。
const MAX_FP: u64 = 20;

fn range(branch: &str, max_file_id: u64, max_segment_file_id: u64) -> BranchFileRange {
  BranchFileRange {
    branch_name: branch.into(),
    term: 0,
    max_file_id,
    max_segment_file_id,
  }
}

fn owner<'a>(
  branch: &'a str,
  range_li: &'a [BranchFileRange],
  segment: bool,
) -> BranchPageOwner<'a> {
  BranchPageOwner {
    branch_name: branch,
    range_li,
    segment,
    shift: SHIFT,
  }
}

/// 构造 Pooled mapper：`used` 为已落盘的物理页，`branch` 决定空闲池归属。
fn build(used: &[u64], range_li: &[BranchFileRange], branch: &str) -> PageMapper {
  build_with_max(used, range_li, branch, MAX_FP)
}

fn build_with_max(
  used: &[u64],
  range_li: &[BranchFileRange],
  branch: &str,
  max_fp_id: u64,
) -> PageMapper {
  let mut tbl = MappingTbl::new();
  for &file_page_id in used {
    tbl.push(encode_file_page_id(file_page_id));
  }
  PageMapper::with_tbl_and_source_in_branch(
    tbl,
    GenerationSource::new(),
    AllocMode::Pooled,
    max_fp_id,
    owner(branch, range_li, false),
  )
}

/// 连续分配 `count` 个逻辑页，返回每次拿到的物理页。
fn allocate_li(mapper: &mut PageMapper, count: usize) -> Vec<u64> {
  (0..count)
    .map(|_| {
      let page_id = mapper.allocate_page();
      mapper.file_page_id(page_id).expect("allocated page maps")
    })
    .collect()
}

/// 父 branch 拥有 file 0，当前 branch 拥有 file 1..=4；父文件里的空洞不得回池。
#[test]
fn pooled_owner_filter_skips_parent_file_pages() {
  let range_li = [range("main", 0, 0), range("feature", 4, 4)];
  // file 0（page 0..3）里只有 0、1 在用，2、3 是父 branch 的空闲页。
  let used = [0, 1];

  let mut branch = build(&used, &range_li, "feature");
  // file 1..=4 = page 4..=19 全部是当前 branch 的空洞，共 16 页；LIFO 降序。
  let got = allocate_li(&mut branch, 16);
  assert!(
    got.iter().all(|file_page_id| *file_page_id >= 4),
    "branch allocation must stay in its own files: {got:?}"
  );
  assert_eq!(got, (4..=19).rev().collect::<Vec<_>>());
  // 池耗尽后从高水位继续，仍在当前 branch 的新文件空间。
  assert_eq!(allocate_li(&mut branch, 1), vec![MAX_FP]);

  // 父 branch 自己的池只含 file 0 的两个空洞，不碰子 branch 的预留页。
  let mut main = build(&used, &range_li, "main");
  assert_eq!(allocate_li(&mut main, 2), vec![3, 2]);
  assert_eq!(allocate_li(&mut main, 1), vec![MAX_FP]);
}

/// 链中间夹兄弟 branch：只回填末尾当前 branch 的区间。
#[test]
fn pooled_owner_filter_handles_sibling_ranges() {
  // main 拥用 file 0..=2，sib 拥有 3..=5，self 拥有 6..=8；无任何已用页。
  let range_li = [range("main", 2, 2), range("sib", 5, 5), range("self", 8, 8)];
  // 9 个 file（0..=8）。
  let max_fp_id = 36;
  let mut mapper = build_with_max(&[], &range_li, "self", max_fp_id);
  // 只有 file 6..=8（page 24..=35）回池。
  let got = allocate_li(&mut mapper, 12);
  assert_eq!(got, (24..=35).rev().collect::<Vec<_>>());
  assert_eq!(allocate_li(&mut mapper, 1), vec![max_fp_id]);
}

/// 当前 branch 不在区间链里（manifest 与 attach 身份不一致）：一页都不回填。
#[test]
fn pooled_owner_filter_without_matching_range_returns_empty_pool() {
  let range_li = [range("main", 4, 4)];
  let mut mapper = build(&[0], &range_li, "feature");
  assert_eq!(allocate_li(&mut mapper, 1), vec![MAX_FP]);
}

/// legacy 回退：区间链为空时保留原「全部空洞回填」语义（对齐 C++ `!ranges.empty()`）。
#[test]
fn pooled_owner_filter_falls_back_to_all_pages_on_empty_range_chain() {
  let mut mapper = build(&[0, 1], &[], "feature");
  let got = allocate_li(&mut mapper, 18);
  assert_eq!(got, (2..=19).rev().collect::<Vec<_>>());
}

/// segment 侧按 `max_segment_file_id` 解析，归属规则与 data 侧一致。
#[test]
fn pooled_owner_filter_uses_segment_range() {
  // data 链把 file 0 给 main、1..=4 给 feature；segment 链给出不同切分，
  // 验证解析字段没有串用。
  let range_li = [range("main", 0, 2), range("feature", 4, 4)];
  let mut tbl = MappingTbl::new();
  for file_page_id in [0, 1] {
    tbl.push(encode_file_page_id(file_page_id));
  }
  let mut mapper = PageMapper::with_tbl_and_source_in_branch(
    tbl,
    GenerationSource::new(),
    AllocMode::Pooled,
    MAX_FP,
    owner("feature", &range_li, true),
  );
  // segment 侧 main 拥到 file 2，feature 拥到 file 4：page 2、3（file 0）与
  // page 8..=11（file 2）属于 main；feature 只收 page 12..=19。
  let got = allocate_li(&mut mapper, 8);
  assert_eq!(got, (12..=19).rev().collect::<Vec<_>>());
  assert_eq!(allocate_li(&mut mapper, 1), vec![MAX_FP]);
}

/// 区间上界是含端上界：file id == 上界属于该区间（C++ `lower_bound` 口径）。
#[test]
fn pooled_owner_range_boundary_is_inclusive() {
  let range_li = [range("main", 0, 0), range("feature", 4, 4)];
  // 让 file 4 的唯一页（16）成为空洞，并验证 owner=feature 收下它。
  let mut mapper = build(&[0, 1, 2, 3], &range_li, "feature");
  let got = allocate_li(&mut mapper, 4);
  assert_eq!(got, vec![19, 18, 17, 16]);
}
