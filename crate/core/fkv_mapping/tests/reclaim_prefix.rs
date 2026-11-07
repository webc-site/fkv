//! perf-12：retired 队列前缀回收的复杂度与语义等价。
//!
//! 旧实现每批 `retain` 全量扫描 retired 队列；长读快照存活期间队列只增不减，
//! 写路径退化为 O(批数²)。新实现利用「代际单调非降」只弹出可回收前缀。
//!
//! 这里用两种互补的护栏：
//! 1. 扫描计数（`retired_scan_cnt`）断言单次回收只扫「前缀 + 1」，不依赖计时；
//! 2. 穷举对拍：参考模型复刻旧 `retain` 语义 + `FilePagePool` 的 LIFO 区间栈，
//!    对小规模序列逐一比较回收数、剩余队列长度与空闲池分配顺序。

use fkv_mapping::{AllocMode, PageMapper, SnapshotRegistry, SnapshotSource};
use fkv_types::{NULL_PAGE, PageId, TableIdent};

fn tbl() -> TableIdent {
  TableIdent::new("reclaim_prefix", 1).unwrap()
}

fn source(mapper: &PageMapper, root_id: PageId) -> SnapshotSource<'_> {
  SnapshotSource {
    data: mapper,
    segment: None,
    root_id,
    ttl_root_id: NULL_PAGE,
  }
}

/// 1 万退役页 + 长快照存活：每次 reclaim 最多检查队头一条。
#[test]
fn long_snapshot_reclaim_scans_prefix_only() {
  const N: usize = 10_000;
  let registry = SnapshotRegistry::new();
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  mapper.set_snapshot_generation_source(registry.generation_source());

  let page = mapper.allocate_page();
  let snapshot = registry.open(&tbl(), source(&mapper, page));
  let watermark = registry.min_live_generation();

  for _ in 0..N {
    // 快照代际 = retire 记录代际：整条队列都不可回收。
    assert_eq!(mapper.reclaim_retired(watermark), 0);
    mapper.remap_page(page).unwrap();
  }
  assert_eq!(mapper.retired_len(), N, "长快照期间退役页只增不减");
  assert!(
    mapper.retired_scan_cnt() <= N as u64,
    "每次 reclaim 最多检查队头一条：scanned={} n={N}",
    mapper.retired_scan_cnt()
  );

  // 快照释放：下一次 reclaim 一次弹空整个队列，扫描量 N + 1（含末次空队列检查）。
  drop(snapshot);
  let before = mapper.retired_scan_cnt();
  assert_eq!(mapper.reclaim_retired(registry.min_live_generation()), N);
  assert_eq!(mapper.retired_len(), 0);
  assert_eq!(
    mapper.retired_scan_cnt() - before,
    N as u64,
    "一次 reclaim 扫描整个可回收前缀"
  );
}

/// 空闲池的参考模型：与 `FilePagePool` 同构的 LIFO 半开区间栈。
#[derive(Default)]
struct PoolModel {
  ranges: Vec<(u64, u64)>,
  max_fp: u64,
}

impl PoolModel {
  fn free(&mut self, id: u64) {
    if let Some(last) = self.ranges.last_mut()
      && last.1 == id
    {
      last.1 = id + 1;
      return;
    }
    self.ranges.push((id, id + 1));
  }

  fn allocate(&mut self) -> u64 {
    if let Some(last) = self.ranges.last_mut() {
      last.1 -= 1;
      let id = last.1;
      if last.0 == last.1 {
        self.ranges.pop();
      }
      return id;
    }
    let id = self.max_fp;
    self.max_fp += 1;
    id
  }

  fn free_len(&self) -> u64 {
    self.ranges.iter().map(|(start, end)| end - start).sum()
  }
}

/// 旧实现的回收语义：全量 `retain`（floor = 0 时只看 `generation < watermark`）。
fn reference_reclaim(retired: &[(u64, u64)], watermark: u64) -> (Vec<u64>, Vec<(u64, u64)>) {
  let mut freed = Vec::new();
  let mut remaining = Vec::new();
  for &(id, generation) in retired {
    if generation < watermark {
      freed.push(id);
    } else {
      remaining.push((id, generation));
    }
  }
  (freed, remaining)
}

const PAGES: u64 = 3;
const MAX_GEN: u64 = 2;

/// 枚举「互异物理页 + 非降代际」的全部退役序列（含空序列）。
fn enum_sequences() -> Vec<Vec<(u64, u64)>> {
  fn dfs(used: u32, last_gen: u64, cur: &mut Vec<(u64, u64)>, out: &mut Vec<Vec<(u64, u64)>>) {
    out.push(cur.clone());
    if cur.len() as u64 == PAGES {
      return;
    }
    for page in 0..PAGES {
      if used & (1 << page) != 0 {
        continue;
      }
      for target_gen in last_gen..=MAX_GEN {
        cur.push((page, target_gen));
        dfs(used | (1 << page), target_gen, cur, out);
        cur.pop();
      }
    }
  }
  let mut out = Vec::new();
  dfs(0, 0, &mut Vec::new(), &mut out);
  out
}

/// 按序列构造 mapper：先用快照发出代际，再 `free_page` 让旧页按该代际退役。
fn build(seq: &[(u64, u64)]) -> PageMapper {
  let registry = SnapshotRegistry::new();
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  mapper.set_snapshot_generation_source(registry.generation_source());
  let pages: Vec<PageId> = (0..PAGES).map(|_| mapper.allocate_page()).collect();

  let mut issued = 0u64;
  let mut held = Vec::new();
  for &(page, target_gen) in seq {
    while issued < target_gen {
      held.push(registry.open(&tbl(), source(&mapper, pages[page as usize])));
      issued += 1;
    }
    // 每页初始物理页 id 即逻辑页序号（新 mapper 顺序分配）。
    assert_eq!(mapper.free_page(pages[page as usize]).unwrap(), Some(page));
  }
  drop(held);
  mapper
}

/// 穷举对拍：回收数、剩余队列、空闲池 LIFO 分配顺序与旧实现一致。
#[test]
fn prefix_reclaim_matches_retain_reference_exhaustively() {
  let seqs = enum_sequences();
  assert!(seqs.len() > 100, "穷举序列数 = {}", seqs.len());

  for seq in &seqs {
    let base = build(seq);
    assert_eq!(base.retired_len(), seq.len());
    assert_eq!(base.pooled_free_len(), 0);

    for watermark in [0u64, 1, 2, 3, u64::MAX] {
      let mut mapper = base.clone();
      let (freed, remaining) = reference_reclaim(seq, watermark);
      let mut model = PoolModel {
        ranges: Vec::new(),
        max_fp: PAGES,
      };
      for id in &freed {
        model.free(*id);
      }

      assert_eq!(
        mapper.reclaim_retired(watermark),
        freed.len(),
        "seq={seq:?} watermark={watermark}: 回收数不一致"
      );
      assert_eq!(
        mapper.retired_len(),
        remaining.len(),
        "seq={seq:?} watermark={watermark}: 剩余队列长度不一致"
      );
      assert_eq!(mapper.pooled_free_len(), model.free_len());

      // 池内容 / LIFO 顺序：逐个取空闲页，序列必须与参考模型一致。
      let mut got = Vec::new();
      while mapper.pooled_free_len() > 0 {
        let page = mapper.allocate_page();
        got.push(mapper.file_page_id(page).unwrap());
      }
      let mut want = Vec::new();
      while model.free_len() > 0 {
        want.push(model.allocate());
      }
      assert_eq!(
        got, want,
        "seq={seq:?} watermark={watermark}: 空闲池分配顺序不一致"
      );
    }
  }
}
