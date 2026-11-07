//! test-17：`Store` → `fkv_task_compact` 的 yield 预算接线回归。
//!
//! l2-09 的让出点本身已有 task 级用例（`fkv_task_compact/tests/compact_yield.rs`，
//! 直接调用 `compact_file_li_budget`）；本文件锁住**生产接线**：`Store` 必须把
//! `self.yield_budget_us` 真正传进 task，而不是常量 0。
//!
//! - data 侧：`Store::compact_file`；
//! - segment 侧：`Store::compact_segment_if_needed`（内部经
//!   `compact_segment_file_li_budget`）。
//!
//! `MemStore` 的 IO future 全部立即 `Ready`，compact 链路上唯一的 `Pending`
//! 来源是 `YieldBudget::maybe_yield` 的自唤醒让出；因此「future 返回 `Pending`
//! 的次数」就是让出次数。
//!
//! 夹具刻意让「mapping 扫描规模」远大于「被搬页数」：扫描段每
//! [`YIELD_CHECK_STRIDE`] 个 mapping 项检查一次预算，数千项会稳定让出多次；
//! 而被搬页数保持在一个步长以内，`append_mapping_log` 编码脏页时只会做 seq=0
//! 的一次检查（新建预算后立即检查，不会命中），不会给计数注入噪声。
//!
//! 断言：
//! - `yield_budget_us = 1` 时让出次数必须达到扫描段的理论下限；
//! - `yield_budget_us = 0` 时让出次数必须为 0（关闭语义，不读时钟）。
//!
//! 把 `fkv_store/src/compact.rs` 的两处传参改成 0 后，两条用例都会确定性变红
//! （不是超时）：task 收到的预算为 0，扫描段不再产生任何 `Pending`。

use core::{
  future::{Future, poll_fn},
  pin::pin,
  task::Poll,
};

use fkv_btree::BatchEntry;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_store::Store;
use fkv_task_read::YIELD_CHECK_STRIDE;
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
/// data 文件按 8 页切分：目标文件只有 8 个 live 页（< 一个让出检查步长），
/// 但整张 mapping 有 4000+ 项，扫描段足以跨过 1µs 预算。
const DATA_FILE_SHIFT: u8 = 3;

/// 手工 poll 到完成，返回 `(输出, Pending 次数)`。
async fn poll_count<F: Future>(fut: F) -> (F::Output, usize) {
  let mut fut = pin!(fut);
  let mut pending = 0usize;
  let out = poll_fn(|cx| match fut.as_mut().poll(cx) {
    Poll::Ready(out) => Poll::Ready(out),
    Poll::Pending => {
      pending += 1;
      Poll::Pending
    }
  })
  .await;
  (out, pending)
}

/// data 侧夹具：8 个 256KB value 拆成 4000+ 个 overflow 页，全部是 mapping 项；
/// 每 8 页一个文件，compact 目标 file 0 只有 8 个 live 页。
async fn data_store(yield_budget_us: u64) -> (Store<MemStore>, TableIdent) {
  let tbl = TableIdent::new("compact_yield_wiring_data", 1).unwrap();
  let mut store = Store::new(
    MemStore::with_pages_per_file(PAGE_SIZE, DATA_FILE_SHIFT),
    PAGE_SIZE,
    16,
  )
  .with_yield_budget_us(yield_budget_us);
  let value = vec![0xA5u8; 256 * 1024];
  let batch: Vec<BatchEntry> = (0..8u32)
    .map(|i| BatchEntry::upsert(format!("k{i:02}").into_bytes(), value.clone(), 1))
    .collect();
  store.put(&tbl, &batch).await.unwrap();
  let pages = store.mapper(&tbl).unwrap().mapping_count();
  assert!(
    pages > 8 * YIELD_CHECK_STRIDE,
    "夹具 mapping 项过少（{pages}），扫描段无法稳定产生多次让出"
  );
  (store, tbl)
}

/// segment 侧夹具：3000 个 live 段分布在前 750 个文件（每文件 4 槽，全满）。
///
/// 只覆盖 key0：它的 3 个旧段变死、3 个新段落在尾文件，于是仅 file 0 剩下
/// 1 个 live 段进入压缩计划（尾文件按分配器高水位跳过）。被搬段数 = 1，
/// 而 segment mapping 扫描仍是 3000 项，让出计数只可能来自扫描段。
async fn segment_store(yield_budget_us: u64) -> (Store<MemStore>, TableIdent) {
  let tbl = TableIdent::new("compact_yield_wiring_segment", 2).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let mut store = Store::new(
    MemStore::new(PAGE_SIZE).with_segment_config(layout.segment_size() as usize, 2),
    PAGE_SIZE,
    16,
  )
  .with_large_value(layout, 1024)
  .with_segment_amplify_factor(1)
  .with_yield_budget_us(yield_budget_us);

  let value = vec![0x5Au8; 9000];
  let batch: Vec<BatchEntry> = (0..1000u32)
    .map(|i| BatchEntry::upsert(format!("k{i:04}").into_bytes(), value.clone(), 1))
    .collect();
  store.put_large(&tbl, &batch).await.unwrap();

  // 覆盖 key0：仅 file 0 变成「部分 live」，制造一个可压缩的候选文件。
  store
    .put_large(
      &tbl,
      &[BatchEntry::upsert(b"k0000".to_vec(), vec![0x3Cu8; 9000], 2)],
    )
    .await
    .unwrap();

  let live = store.segment_mapper(&tbl).unwrap().mapping_count();
  assert!(
    live > 8 * YIELD_CHECK_STRIDE,
    "夹具 segment mapping 项过少（{live}）"
  );
  assert!(
    store.needs_segment_maintain(&tbl),
    "夹具应触发 segment compaction"
  );
  (store, tbl)
}

/// data 侧：`Store::compact_file` 必须消费 Store 上的 yield 预算。
#[compio::test]
async fn store_compact_file_forwards_yield_budget() {
  let (mut store, tbl) = data_store(1).await;
  let (moved, yields) = {
    let fut = store.compact_file(&tbl, 0, DATA_FILE_SHIFT);
    poll_count(fut).await
  };
  let moved = moved.unwrap();
  assert!(moved > 0, "file 0 应有 live 页可搬");
  assert!(
    moved < YIELD_CHECK_STRIDE,
    "夹具应把被搬页数压在一个让出步长内（实际 {moved}）"
  );
  // 扫描段有 4000+ 项：每个步长检查一次，预算 1µs 下至少命中数次。
  assert!(
    yields >= 4,
    "Store::compact_file 未把 yield_budget_us 传给 task：Pending = {yields}"
  );

  // 预算为 0：接线正确时 task 完全不让出。
  let (mut store, tbl) = data_store(0).await;
  let (moved, yields) = {
    let fut = store.compact_file(&tbl, 0, DATA_FILE_SHIFT);
    poll_count(fut).await
  };
  assert!(moved.unwrap() > 0);
  assert_eq!(yields, 0, "yield_budget_us = 0 时不应额外让出");
}

/// segment 侧：`Store::compact_segment_if_needed` 必须消费 Store 上的预算。
#[compio::test]
async fn store_compact_segment_forwards_yield_budget() {
  let (mut store, tbl) = segment_store(1).await;
  let (plan_li, yields) = {
    let fut = store.compact_segment_if_needed(&tbl);
    poll_count(fut).await
  };
  let plan_li = plan_li.unwrap();
  assert_eq!(plan_li, vec![0], "只应压缩留下 live 段的 file 0");
  assert!(
    yields >= 3,
    "Store::compact_segment_* 未把 yield_budget_us 传给 task：Pending = {yields}"
  );

  let (mut store, tbl) = segment_store(0).await;
  let (plan_li, yields) = {
    let fut = store.compact_segment_if_needed(&tbl);
    poll_count(fut).await
  };
  assert_eq!(plan_li.unwrap(), vec![0]);
  assert_eq!(yields, 0, "yield_budget_us = 0 时不应额外让出");
}
