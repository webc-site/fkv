//! 叶子链块窗口（perf-11）的语义回归：窗口只决定「一次读几页」，遍历仍以 data
//! page 的 `next_page_id` 链表为权威，因此
//!
//! 1. 整区间扫描（大窗口 / 满深度）必须与分页推进（小窗口）逐条一致；
//! 2. 计数路径（`scan_count`）必须等于物化扫描长度；
//! 3. 窗口不得把同一个叶子页读两遍（整表扫描的页读数 ≈ 叶子页数）。

use fkv_btree::BatchEntry;
use fkv_io_mem::MemStore;
use fkv_store::{PageOption, ScanOption, Store};
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
const TOTAL: u32 = 600;
/// 分页步长：`entry_limit=7` → 窗口深度 `min(16, max(2, 7/8)) = 2`，
/// 与整表扫描的满深度窗口走不同代码路径。
const STEP: usize = 7;

fn key_of(index: u32) -> Vec<u8> {
  format!("k{index:04}").into_bytes()
}

/// 600 条：删除 index % 5 == 0，剩余里 index % 7 == 1 设 100ms 过期
/// （now=1000 被过滤），其余为 live。
fn live_index_li() -> Vec<u32> {
  (0..TOTAL)
    .filter(|index| index % 5 != 0 && index % 7 != 1)
    .collect()
}

async fn churned_store(tbl: &TableIdent) -> Store<MemStore> {
  let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
  let put_li: Vec<BatchEntry> = (0..TOTAL)
    .map(|index| {
      let expire_ts = if index % 7 == 1 { 100 } else { 0 };
      let mut entry = BatchEntry::upsert(key_of(index), vec![b'v'; 8], 1);
      entry.expire_ts = expire_ts;
      entry
    })
    .collect();
  store.put_at(tbl, &put_li, 0).await.unwrap();
  let del_li: Vec<BatchEntry> = (0..TOTAL)
    .step_by(5)
    .map(|index| BatchEntry::delete(key_of(index), 2))
    .collect();
  store.put_at(tbl, &del_li, 0).await.unwrap();
  store
}

/// 分页推进整条链：`begin_inclusive=false` + 上一页最后一条 key 作为下一页起点。
async fn walk_paginated(
  store: &Store<MemStore>,
  tbl: &TableIdent,
  page: PageOption,
) -> (Vec<(Vec<u8>, Vec<u8>)>, usize) {
  let option = ScanOption {
    begin_inclusive: false,
    ..ScanOption::default()
  };
  let mut out: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
  let mut begin: Vec<u8> = Vec::new();
  let mut req_num = 0usize;
  loop {
    req_num += 1;
    let page_result = store
      .scan_page_at(tbl, &begin, b"", 1000, option, page)
      .await
      .unwrap();
    if page_result.entry_li.is_empty() {
      break;
    }
    begin = page_result.entry_li.last().unwrap().0.clone();
    out.extend(page_result.entry_li);
    if !page_result.has_remaining {
      break;
    }
    assert!(out.len() <= TOTAL as usize, "分页推进不得超出表内条数");
  }
  (out, req_num)
}

/// 整区间扫描（满深度窗口）与分页推进（浅窗口）必须逐条一致，且都等于闭式期望。
#[test]
fn scan_matches_paginated_walk_across_window_depths() {
  pollster::block_on(async {
    let tbl = TableIdent::new("depths", 1).unwrap();
    let store = churned_store(&tbl).await;

    let expected: Vec<(Vec<u8>, Vec<u8>)> = live_index_li()
      .into_iter()
      .map(|index| (key_of(index), vec![b'v'; 8]))
      .collect();

    let full = store.scan_at(&tbl, b"", b"", 0, 1000).await.unwrap();
    assert_eq!(full, expected, "整区间扫描结果不得改变");

    // 计数路径与物化路径同口径。
    assert_eq!(
      store.scan_count_at(&tbl, b"", b"", 1000).await.unwrap(),
      full.len()
    );

    // 逐条分页（窗口深度 2）与按字节配额分页（窗口深度 page_size/page_size）都要
    // 重建出同一份结果。
    let (stepped, step_req_num) = walk_paginated(
      &store,
      &tbl,
      PageOption {
        page_entries: STEP,
        page_size: 0,
      },
    )
    .await;
    assert_eq!(stepped, expected, "逐条分页结果必须与整区间扫描一致");
    assert!(step_req_num > 1, "逐条分页必须真的分多页");

    let (by_byte, byte_req_num) = walk_paginated(
      &store,
      &tbl,
      PageOption {
        page_entries: 0,
        page_size: PAGE_SIZE * 3,
      },
    )
    .await;
    assert_eq!(by_byte, expected, "按字节分页结果必须与整区间扫描一致");
    assert!(byte_req_num > 1, "按字节分页必须真的分多页");

    // 深窗口分页（entry_limit ≥ 表内条数 → 满深度窗口）同样一致。
    let (deep, deep_req_num) = walk_paginated(
      &store,
      &tbl,
      PageOption {
        page_entries: TOTAL as usize,
        page_size: 0,
      },
    )
    .await;
    assert_eq!(deep, expected);
    assert_eq!(deep_req_num, 1, "宽分页上限应一次取完");
  });
}

/// 窗口不得让叶子页被读两遍：整表扫描的页读数与叶子页数同阶，且远小于条数。
///
/// 窗口按连续物理页投机，整表遍历时各窗口首尾相接，因此每个叶子页恰好读一次；
/// 若窗口重填时把已读过的页再读一遍（或退化成逐页读两遍），这里的读数会翻倍。
#[test]
fn full_scan_reads_each_leaf_page_once() {
  pollster::block_on(async {
    let tbl = TableIdent::new("window_reads", 2).unwrap();
    let store = churned_store(&tbl).await;

    let before = store.page_io().read_num();
    let full = store.scan_at(&tbl, b"", b"", 0, 1000).await.unwrap();
    let full_reads = store.page_io().read_num() - before;
    assert_eq!(full.len(), live_index_li().len());

    // 512B 页放 ~30 条：600 条 ⇒ ~20 个叶子页 + 下降路径。整表扫描只应读到
    // 叶子链自己（含窗口里多投机但确实属于链的页），不该出现重复读。
    assert!(
      full_reads < (full.len() / 15) as u64,
      "整表扫描页读数与条数同阶（{full_reads} 次读 / {} 条）说明窗口做了整链顺序读",
      full.len()
    );

    // 浅分页请求（窗口深度 2）只读链头几页，不会因为窗口把整条链读进来。
    let before = store.page_io().read_num();
    let _ = store
      .scan_page_at(
        &tbl,
        b"",
        b"",
        1000,
        ScanOption::default(),
        PageOption {
          page_entries: STEP,
          page_size: 0,
        },
      )
      .await
      .unwrap();
    let page_reads = store.page_io().read_num() - before;
    assert!(
      page_reads <= 8,
      "浅分页只该读下降路径 + 浅窗口，实际 {page_reads} 页"
    );
  });
}
