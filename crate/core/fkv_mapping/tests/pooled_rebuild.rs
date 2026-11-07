//! pooled-06：Pooled reopen 的空闲池按半开区间记账，不逐页物化 `max_fp_id`。
//!
//! `max_fp_id` 的量纲是 `file_id << pages_per_file_shift`（默认每文件 2^18 页），
//! branch 预留后可达数亿页位。重建必须只记录真实空洞区间：
//! - 逻辑空闲页数 / 分配顺序与逐页入链一致（从全局最高空闲页递减）；
//! - 区间条目数只与空洞段数同阶，不随 `max_fp_id` 展开；
//! - branch 归属过滤按文件区间整段判定，结果与逐页 `owns` 等价。

use std::time::{Duration, Instant};

use fkv_mapping::{AllocMode, BranchPageOwner, GenerationSource, MappingTbl, PageMapper};
use fkv_types::{BranchFileRange, encode_file_page_id};

const SHIFT: u8 = 18;
/// 1024 个文件 × 2^18 页 = 2.68 亿个 page 位：branch 预留的最小量级。
const MAX_FP: u64 = 1024 << SHIFT;

fn range(branch: &str, max_file_id: u64) -> BranchFileRange {
  BranchFileRange {
    branch_name: branch.into(),
    term: 0,
    max_file_id,
    max_segment_file_id: max_file_id,
  }
}

fn owner<'a>(branch: &'a str, range_li: &'a [BranchFileRange], shift: u8) -> BranchPageOwner<'a> {
  BranchPageOwner {
    branch_name: branch,
    range_li,
    segment: false,
    shift,
  }
}

fn tbl_with(used: &[u64]) -> MappingTbl {
  let mut tbl = MappingTbl::new();
  for &file_page_id in used {
    tbl.push(encode_file_page_id(file_page_id));
  }
  tbl
}

fn take_li(mapper: &mut PageMapper, count: usize) -> Vec<u64> {
  (0..count)
    .map(|_| {
      let page_id = mapper.allocate_page();
      mapper.file_page_id(page_id).expect("allocated page maps")
    })
    .collect()
}

/// 无 branch 归属信息（legacy 全量回填）：尾部数亿页位只对应一个区间。
#[test]
fn reopen_tail_is_one_range_and_allocation_order_is_descending() {
  let used = [3u64, 100 << SHIFT, (1000 << SHIFT) + 7];
  let start = Instant::now();
  let mut mapper = PageMapper::with_tbl_and_source(
    tbl_with(&used),
    GenerationSource::new(),
    AllocMode::Pooled,
    MAX_FP,
  );
  let elapsed = start.elapsed();
  assert!(
    elapsed < Duration::from_secs(1),
    "reopen 不应逐页物化：{elapsed:?}"
  );

  assert_eq!(mapper.max_fp_id(), MAX_FP);
  assert_eq!(
    mapper.pooled_free_len(),
    MAX_FP - used.len() as u64,
    "逻辑空闲页数必须覆盖全部未引用页位"
  );
  // 3 个活页把 [0, MAX_FP) 切成 used+1 段（活页互不相邻）。
  assert_eq!(mapper.pooled_free_range_cnt(), used.len() + 1);
  assert_eq!(
    take_li(&mut mapper, 3),
    vec![MAX_FP - 1, MAX_FP - 2, MAX_FP - 3],
    "仍从全局最高空闲页递减（对齐 C++ free_ids pop_back）"
  );
}

/// branch 过滤：只把当前 branch 文件区间内的空洞压入区间栈。
#[test]
fn reopen_with_owner_keeps_only_own_file_ranges() {
  let used = [3u64, 100 << SHIFT, (1000 << SHIFT) + 7];
  let range_li = [range("main", 0), range("feature", 1024)];

  // feature 拥有 file 1..=1024：file 0（page 0..2^18）是父 branch 的空洞，不回池。
  let mut feature = PageMapper::with_tbl_and_source_in_branch(
    tbl_with(&used),
    GenerationSource::new(),
    AllocMode::Pooled,
    MAX_FP,
    owner("feature", &range_li, SHIFT),
  );
  assert_eq!(feature.pooled_free_len(), MAX_FP - (1 << SHIFT) - 2);
  assert_eq!(feature.pooled_free_range_cnt(), 3);
  assert_eq!(take_li(&mut feature, 2), vec![MAX_FP - 1, MAX_FP - 2]);

  // main 只拥有 file 0：page 0..2^18 去掉已用页 3 后的两个区间。
  let mut main = PageMapper::with_tbl_and_source_in_branch(
    tbl_with(&used),
    GenerationSource::new(),
    AllocMode::Pooled,
    MAX_FP,
    owner("main", &range_li, SHIFT),
  );
  assert_eq!(main.pooled_free_len(), (1 << SHIFT) - 1);
  assert_eq!(main.pooled_free_range_cnt(), 2);
  assert_eq!(
    take_li(&mut main, 2),
    vec![(1 << SHIFT) - 1, (1 << SHIFT) - 2]
  );
}

/// 高水位落在文件中间：裁剪到 `max_fp_id`，不越界回填。
#[test]
fn reopen_clips_partial_tail_file() {
  const SHIFT: u8 = 2;
  let max_fp_id = (3 << SHIFT) + 2;
  let used = [(1 << SHIFT) + 2];
  let range_li = [range("feature", 4)];
  let mut mapper = PageMapper::with_tbl_and_source_in_branch(
    tbl_with(&used),
    GenerationSource::new(),
    AllocMode::Pooled,
    max_fp_id,
    owner("feature", &range_li, SHIFT),
  );
  assert_eq!(mapper.pooled_free_len(), max_fp_id - 1);
  assert_eq!(
    take_li(&mut mapper, 3),
    vec![max_fp_id - 1, max_fp_id - 2, max_fp_id - 3]
  );
}

/// 区间记账不得改变释放后复用的 LIFO 语义：最近释放的页总是下一个被分配。
#[test]
fn free_after_reopen_is_lifo() {
  let mut mapper = PageMapper::with_tbl_and_source(
    tbl_with(&[0]),
    GenerationSource::new(),
    AllocMode::Pooled,
    16,
  );
  let first = take_li(&mut mapper, 1)[0];
  assert_eq!(first, 15);
  let page = mapper.allocate_page();
  let file_page_id = mapper.file_page_id(page).unwrap();
  mapper.free_page(page).unwrap();
  assert_eq!(
    mapper.reclaim_retired(u64::MAX),
    1,
    "无在途快照时释放页立即可回收"
  );
  assert_eq!(mapper.pooled_free_range_cnt(), 1, "释放页必须并回栈顶区间");
  let next = take_li(&mut mapper, 1)[0];
  assert_eq!(next, file_page_id, "释放页必须被立即复用");
}

/// 区间逻辑与逐页 `owns` 语义等价（小空间穷举分支区间组合）。
#[test]
fn interval_owner_matches_per_page_owns() {
  const SHIFT: u8 = 2;
  const MAX_FP: u64 = 40; // 10 个文件
  let used = [0u64, 5, 17, 18, 35];
  let chains: [&[BranchFileRange]; 4] = [
    &[range("main", 9)],
    &[range("main", 0), range("feature", 9)],
    &[range("main", 3), range("sib", 6), range("feature", 9)],
    &[range("main", 4), range("feature", 9)],
  ];
  for branch in ["main", "sib", "feature"] {
    for chain in chains {
      let owner = owner(branch, chain, SHIFT);
      let expected: Vec<u64> = (0..MAX_FP)
        .filter(|id| !used.contains(id))
        .filter(|id| owner.owns(*id))
        .rev()
        .collect();
      let mut mapper = PageMapper::with_tbl_and_source_in_branch(
        tbl_with(&used),
        GenerationSource::new(),
        AllocMode::Pooled,
        MAX_FP,
        owner,
      );
      let got = take_li(&mut mapper, expected.len());
      assert_eq!(got, expected, "branch={branch} chain={chain:?}");
    }
  }
}
