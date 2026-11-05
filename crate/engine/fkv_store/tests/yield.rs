//! eng-34b：长循环（scan 页链 / overflow 链）的协作让出。
//!
//! `MemStore` 的所有 IO future 都立即 `Ready`，没有真实 await 让出点：
//! 长循环若不带 `maybe_yield`，会在一次 poll 内跑完，后续任务只能等它结束。
//! 测试用「标记任务是否在长任务完成前被 poll」证明让出点生效，并验证
//! `yield_budget_us = 0` 时行为回到无插桩状态、结果不变。

use std::{cell::Cell, rc::Rc};

use compio::runtime::spawn;
use fkv_btree::BatchEntry;
use fkv_io_mem::MemStore;
use fkv_store::{PageOption, ScanOption, Store};
use fkv_types::TableIdent;

/// 长 scan 是否在完成前让出给标记任务。
async fn scan_yields(yield_budget_us: u64) -> bool {
  const TOTAL: u32 = 4000;
  let tbl = TableIdent::new("yield_scan", 1).unwrap();
  let mut store = Store::new(MemStore::new(512), 512, 16).with_yield_budget_us(yield_budget_us);
  let batch: Vec<BatchEntry> = (0..TOTAL)
    .map(|index| BatchEntry::upsert(format!("k{index:05}").into_bytes(), vec![b'v'; 40], 1))
    .collect();
  store.put(&tbl, &batch).await.unwrap();

  let store = Rc::new(store);
  let done = Rc::new(Cell::new(false));
  let yielded = Rc::new(Cell::new(false));
  let read_store = Rc::clone(&store);
  let read_done = Rc::clone(&done);
  let read = spawn(async move {
    let result = read_store
      .scan_page(&tbl, b"", b"", ScanOption::default(), PageOption::default())
      .await;
    read_done.set(true);
    result
  });
  let marker_done = Rc::clone(&done);
  let marker_yielded = Rc::clone(&yielded);
  let marker = spawn(async move {
    marker_yielded.set(!marker_done.get());
  });

  let page = read.await.unwrap().unwrap();
  marker.await.unwrap();
  assert_eq!(page.entry_li.len(), TOTAL as usize);
  yielded.get()
}

/// 长 overflow 链是否在完成前让出给标记任务。
async fn overflow_yields(yield_budget_us: u64) -> bool {
  // 每批 overflow 指针约拼回 8KB，256KB 需要几十批，足以跨过 1us 段预算。
  let tbl = TableIdent::new("yield_overflow", 1).unwrap();
  let mut store = Store::new(MemStore::new(512), 512, 16).with_yield_budget_us(yield_budget_us);
  let value = vec![0xA5u8; 256 * 1024];
  store
    .put(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), value.clone(), 1)],
    )
    .await
    .unwrap();

  let store = Rc::new(store);
  let done = Rc::new(Cell::new(false));
  let yielded = Rc::new(Cell::new(false));
  let read_store = Rc::clone(&store);
  let read_done = Rc::clone(&done);
  let read = spawn(async move {
    let result = read_store.get(&tbl, b"big").await;
    read_done.set(true);
    result
  });
  let marker_done = Rc::clone(&done);
  let marker_yielded = Rc::clone(&yielded);
  let marker = spawn(async move {
    marker_yielded.set(!marker_done.get());
  });

  let got = read.await.unwrap().unwrap();
  marker.await.unwrap();
  assert_eq!(got, Some(value));
  yielded.get()
}

#[compio::test]
async fn scan_page_loop_yields_to_other_task() {
  assert!(scan_yields(1).await, "长 scan 未触发协作让出");
}

#[compio::test]
async fn overflow_chain_yields_to_other_task() {
  assert!(overflow_yields(1).await, "overflow 链未触发协作让出");
}

#[compio::test]
async fn zero_budget_disables_yield_points() {
  assert!(!scan_yields(0).await, "budget=0 时 scan 不应额外让出");
  assert!(
    !overflow_yields(0).await,
    "budget=0 时 overflow 不应额外让出"
  );
}
