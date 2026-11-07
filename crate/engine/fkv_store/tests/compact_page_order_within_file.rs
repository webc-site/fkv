//! sem-60c：文件内搬移序按旧 fp 升序（C++ `FilePageLess` 堆排模型）。
//!
//! 可达性（真实 oplog）：overflow 链删除把低 page id 段压回空闲链表头，
//! 后续大值写入按 LIFO 复用这些低 page id、却从 append 分配器尾部拿到更高的
//! fp——与文件内未动过的高 page id 页（低 fp）构成「逻辑页序 ≠ fp 序」。
//! 此状态下 compaction 批内搬移序决定新 fp 发号：C++ 按旧 fp 升序
//! （background_write.cpp:66-98），旧 fkv 按逻辑页升序，发号互换、
//! manifest 的 mapping delta 逐字节分叉。本文件用模型断言锁死：
//! delta（= remap 序）的旧 fp 严格递增，且第 i 条搬迁的新 fp == base+i。

use fkv_btree::BatchEntry;
use fkv_gc::{compact_plan, file_live_count};
use fkv_io::{FileIo, ManifestIo};
use fkv_io_mem::MemStore;
use fkv_mapping::PageMapper;
use fkv_store::{AllocMode, Store};
use fkv_task_reopen::split_manifest;
use fkv_types::{FileId, PageId, TableIdent, ValType, decode_id, is_file_page_id, val_type};

mod common;

use common::log_mapping_entry_li;

const PAGE_SIZE: usize = 512;
const SHIFT: u8 = 6;
const FACTOR: u8 = 1;

fn pattern(len: usize, seed: u8) -> Vec<u8> {
  (0..len)
    .map(|index| (index as u8).wrapping_add(seed))
    .collect()
}

/// 快照 mapper 的「逻辑页 → 物理 fp」（只取活跃 `FilePageId` 槽位）。
fn live_fp_snapshot(mapper: &PageMapper) -> Vec<(PageId, u64)> {
  (0..mapper.tbl().len() as PageId)
    .filter_map(|page_id| {
      let value = mapper.tbl().get(page_id).ok()?;
      is_file_page_id(value).then(|| (page_id, decode_id(value)))
    })
    .collect()
}

/// 最后一条 record 的 data mapping delta。
fn last_record_data_delta(bytes: &[u8]) -> Vec<(PageId, u64)> {
  log_mapping_entry_li(split_manifest(bytes).unwrap().pop().unwrap())
}

/// data 侧：同文件内「逻辑页序 ≠ fp 序」时，批内搬移序必须按旧 fp 升序。
#[test]
fn data_compact_moves_pages_in_old_fp_order_within_file() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem60c_order", 1).unwrap();
    let mut store = Store::new(
      MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
      PAGE_SIZE,
      16,
    )
    .with_pages_per_file_shift(SHIFT)
    .with_alloc_mode(AllocMode::Append)
    .with_file_amplify_factor(FACTOR);

    // b（fp 27..51，page 27..51）与 a（page 0..24）共存；删 a 释放低 page id
    // 段，c 复用它们拿到 fp 54.. 的高号（仍在 file 0 的部分与 b 构成乱序对）。
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"a".to_vec(), pattern(12_000, 0), 1)],
      )
      .await
      .unwrap();
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"b".to_vec(), pattern(12_000, 1), 1)],
      )
      .await
      .unwrap();
    store.delete(&tbl, b"a", 2).await.unwrap();
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"c".to_vec(), pattern(12_000, 2), 3)],
      )
      .await
      .unwrap();

    // 前置 1：同一非尾文件内存在「page 更小、fp 更大」的 live 乱序对——
    // 否则逻辑页升序与 fp 升序不可区分，本用例失去判别力。
    let tail_file_id = store.mapper(&tbl).unwrap().max_fp_id() >> SHIFT;
    let snapshot = live_fp_snapshot(store.mapper(&tbl).unwrap());
    assert!(
      snapshot.windows(2).any(|wi| {
        let (_, f0) = wi[0];
        let (_, f1) = wi[1];
        f0 >> SHIFT == f1 >> SHIFT && (f1 >> SHIFT) != tail_file_id && f0 > f1
      }),
      "前置：须存在同文件内 page 序与 fp 序相反的 live 对 snapshot={snapshot:?} tail={tail_file_id}"
    );

    // 前置 2：乱序所在文件进入搬迁计划（超阈值且非尾）。
    let live_map = file_live_count(store.mapper(&tbl).unwrap().tbl(), SHIFT);
    let candidate_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
    let plan: Vec<FileId> = compact_plan(
      &live_map,
      &candidate_li,
      1u64 << SHIFT,
      FACTOR,
      tail_file_id,
    );
    assert!(
      !plan.is_empty(),
      "前置：乱序文件必须超阈值 live={live_map:?}"
    );
    let base = store.mapper(&tbl).unwrap().max_fp_id();
    let old_fp_of = |page_id: PageId| snapshot.iter().find(|e| e.0 == page_id).unwrap().1;

    let plan_run = store.compact_if_needed(&tbl).await.unwrap();
    assert_eq!(plan_run, plan, "计划按文件号升序（sem-61 已锁定）");

    // 模型断言：delta = remap 序 = 搬移序。旧 fp 严格递增（C++ fp 升序
    // 堆排）；新 fp 自 base 起按搬移序连号（append 分配器发号序）。
    // 旧口径（逻辑页升序）在乱序盘面上旧 fp 必然出现逆序，此处即红。
    let bytes = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let delta = last_record_data_delta(&bytes);
    let moved: u64 = plan
      .iter()
      .map(|f| live_map.get(f).copied().unwrap_or(0))
      .sum();
    assert_eq!(delta.len(), moved as usize, "delta 条数必须等于搬迁页数");
    for (index, &(page_id, value)) in delta.iter().enumerate() {
      assert_eq!(val_type(value), ValType::FilePageId);
      assert_eq!(
        decode_id(value),
        base + index as u64,
        "第 {index} 条搬迁（page={page_id}）的新 fp 必须为 base+index"
      );
    }
    let old_fp_li: Vec<u64> = delta.iter().map(|&(p, _)| old_fp_of(p)).collect();
    assert!(
      old_fp_li.windows(2).all(|w| w[0] < w[1]),
      "delta 必须按旧 fp 升序（C++ FilePageLess 堆排）: {old_fp_li:?}"
    );

    // 搬迁后数据仍完整。
    assert_eq!(
      store.get(&tbl, b"b").await.unwrap(),
      Some(pattern(12_000, 1))
    );
    assert_eq!(
      store.get(&tbl, b"c").await.unwrap(),
      Some(pattern(12_000, 2))
    );
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), None);
  });
}
