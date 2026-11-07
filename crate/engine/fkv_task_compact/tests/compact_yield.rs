//! l2-09：`fkv_task_compact` 的 CPU 段（mapping 扫描 / 逐文件循环）协作让出。
//!
//! `MemStore` 的 IO future 全部立即 `Ready`，compact 内部唯一的让出点就是
//! `YieldBudget`；因此「compact future 返回 `Pending` 的次数」等于让出次数，
//! 可直接判定 CPU 段是否在长循环里让出，而不依赖墙钟阈值或读 p99。
//!
//! 两个用例分别钉住两个让出点：
//! - 扫描段：单文件、页数从 512 增到 8192，让出次数必须随之增长；去掉扫描
//!   循环里的 `maybe_yield_every` 后两者都退化为「仅逐文件让出一次」，增长
//!   断言必红。
//! - 逐文件段：200 个文件各 1 页（扫描仅 200 项、几乎不让出），去掉逐文件
//!   循环里的 `maybe_yield` 后让出次数归零，必红。

use core::{future::Future, pin::pin, task::Poll};

use fkv_io::PageIo;
use fkv_io_mem::MemStore;
use fkv_mapping::{AllocMode, PageMapper};
use fkv_task_compact::compact_file_li_budget;
use fkv_types::{FilePageId, PageId, TableIdent};
use futures::future::poll_fn;

const PAGE_SIZE: usize = 512;
/// 扫描段用：页数远小于 `1 << FILE_SHIFT`，全部落在单个文件里，
/// 让逐文件循环固定只跑一轮，把让出次数差异唯一归因到扫描段。
const FILE_SHIFT: u8 = 16;

/// 构造 `pages` 个 live 页的 mapper 与对应内存后端。
///
/// `Append` 分配器下 `allocate_page` 逐页分配，物理页 id 与逻辑页 id 相同，
/// 页 → 文件的归属完全由后续 compact 传入的 `pages_per_file_shift` 决定，
/// 因此同一布局可分别按「单文件」与「每页一文件」两种方式搬迁。
async fn layout(pages: u64) -> (MemStore, PageMapper, TableIdent) {
  let store = MemStore::with_pages_per_file(PAGE_SIZE, FILE_SHIFT);
  let tbl = TableIdent::new("compact_yield", 1).unwrap();
  let buf = vec![0x5Au8; PAGE_SIZE];
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  for _ in 0..pages {
    let page_id: PageId = mapper.allocate_page();
    let file_page_id: FilePageId = mapper.file_page_id(page_id).unwrap();
    store.write_page(&tbl, file_page_id, &buf).await.unwrap();
  }
  (store, mapper, tbl)
}

/// 驱动一次 compact，返回 `(搬移页数, 让出次数)`。
///
/// 手工 `poll` 计数：`MemStore` 没有真实 IO pending，`Pending` 只可能来自
/// compact 内部 `YieldBudget::maybe_yield` 的自唤醒让出。
async fn compact_once(
  pages: u64,
  shift: u8,
  file_li: &[fkv_types::FileId],
  yield_budget_us: u64,
) -> (usize, usize) {
  let (store, mut mapper, tbl) = layout(pages).await;
  let (moved, yields) = {
    let mut fut = pin!(compact_file_li_budget(
      &store,
      &tbl,
      &mut mapper,
      file_li,
      PAGE_SIZE,
      shift,
      yield_budget_us,
    ));
    let mut yields = 0usize;
    let moved = poll_fn(|cx| match fut.as_mut().poll(cx) {
      Poll::Ready(result) => Poll::Ready(result.unwrap()),
      Poll::Pending => {
        yields += 1;
        Poll::Pending
      }
    })
    .await;
    (moved, yields)
  };
  // 结果与功能无关：每个 live 页都必须搬一次。
  assert_eq!(moved as u64, pages, "compact 搬移页数与 live 页数不符");
  (moved, yields)
}

#[test]
fn scan_segment_yield_count_scales_with_mapping() {
  pollster::block_on(async {
    // 单文件：扫描段遍历全部 mapping 项，是让出的主要来源。
    let small = compact_once(512, FILE_SHIFT, &[0], 1).await.1;
    let large = compact_once(8192, FILE_SHIFT, &[0], 1).await.1;
    assert!(
      large > small,
      "让出次数未随 mapping 规模增长：512 页 {small} 次，8192 页 {large} 次"
    );
    assert!(large >= 4, "8192 页扫描段让出过少：{large} 次");

    // 预算为 0：不读时钟、不让出（对齐未插桩路径）。
    let zero = compact_once(8192, FILE_SHIFT, &[0], 0).await.1;
    assert_eq!(zero, 0, "yield_budget_us = 0 时不应额外让出");
  });
}

#[test]
fn file_loop_yields_between_files() {
  pollster::block_on(async {
    // 每页一个文件（shift 0）：扫描只有 200 项、基本不让出，让出全部来自
    // 逐文件循环的文件边界。
    const FILES: u64 = 200;
    let file_li: Vec<fkv_types::FileId> = (0..FILES).collect();
    let yields = compact_once(FILES, 0, &file_li, 1).await.1;
    assert!(
      yields >= 10,
      "逐文件循环未按文件边界让出：{FILES} 个文件仅让出 {yields} 次"
    );

    let zero = compact_once(FILES, 0, &file_li, 0).await.1;
    assert_eq!(zero, 0, "yield_budget_us = 0 时不应额外让出");
  });
}
