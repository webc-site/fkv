//! pooled-14：legacy 空 `file_range_li` + term bump 的复用下界回归。
//!
//! 旧行为：manifest 的区间链为空（pre-range-chain legacy 形状），attach term 与
//! manifest term 不同 → `install_replay_state` 把 `max_fp_id` 抬到下一文件边界，
//! 当次下界正确；但 bump 后的第一笔写在 `refresh_branch_metadata` 里往**空链**
//! 追加 `(main, new_term)` 区间，区间隐式起点是 file 0，旧 term 文件整段被标成
//! 新 term。同 term 重启后 `term_range_start` 返回 0，io-15 的复用下界失效：
//! 旧 term 文件的空洞重新进池，新 term 写进旧 file id。
//!
//! 修复：bump 且区间链为空时，在覆盖 `metadata.term` 之前补一条合成旧 term
//! 区间（`max_file_id = 新文件边界 - 1`），使新 term 区间起点 = bump 边界。
//! 本文件对 MemStore / SimStore / CompioStore 三种页记账后端 × Pooled / Append
//! 两模式锁定：同 term 重启后下界 ≥ bump 边界、旧 key 在
//! `skip_verify_checksum=true`（关闭校验）下全部可读、旧 file id 下不出现新
//! term 文件。

use std::slice;

use fkv_btree::BatchEntry;
use fkv_io::{ManifestIo, PageIo};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_io_sim::SimStore;
use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArg};
use fkv_store::{AllocMode, Store};
use fkv_testkit::TempRoot;
use fkv_types::{NULL_PAGE, TableIdent};

const PAGE_SIZE: usize = 512;
/// 每文件 4 页：32 个小 key 至少跨两个文件，bump 边界与空洞都容易构造。
const SHIFT: u8 = 2;
const RESTART: u16 = 16;

/// legacy 表的原始内容：32 个 key，单批写入。
fn legacy_batch() -> Vec<BatchEntry> {
  (0..32u32)
    .map(|i| BatchEntry::upsert(format!("k{i:02}").into_bytes(), vec![i as u8 + 1; 64], 1))
    .collect()
}

/// bump 后、同 term 重启前后的新写内容：足够多页以驱动 COW 与池分配。
fn new_batch() -> Vec<BatchEntry> {
  (0..8u32)
    .map(|i| BatchEntry::upsert(format!("n{i}").into_bytes(), vec![0xA0 + i as u8; 64], 2))
    .collect()
}

fn open<I: PageIo + ManifestIo>(io: I, term: u64, mode: AllocMode) -> Store<I> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(SHIFT)
    .with_alloc_mode(mode)
    .with_branch("main", term)
    // 触发面正是 skip_verify_checksum=true 的静默命中，必须关掉校验。
    .with_verify_checksum(false)
}

/// 把当前状态覆写成 pre-range-chain legacy 形状（旧口径称「C++ Local 首个
/// main manifest」）：`(main, 0)` + 空 `file_range_li`，mapping / `max_fp_id` /
/// root 原样保留（pooled-14 的触发面）。
fn legacy_snapshot<I: PageIo>(store: &Store<I>, tbl: &TableIdent) -> Vec<u8> {
  let mapper = store.mapper(tbl).expect("mapper");
  let mapping_value_li: Vec<u64> = mapper.tbl().iter_value_li().collect();
  ManifestBuilder::new()
    .snapshot(
      SnapshotArg {
        root_id: store.root_id(tbl).expect("root"),
        ttl_root: NULL_PAGE,
        max_fp_id: mapper.max_fp_id(),
        dict_bytes: &[],
        branch_metadata: &BranchManifestMetadata::new("main", 0),
        max_segment_fp_id: 0,
      },
      &mapping_value_li,
      None,
    )
    .unwrap()
}

/// pooled-14 的完整序列：term 0 写 legacy 表 → 覆写成空区间链 → term 1 reopen
/// （bump）→ 写 seed → 同 term 1 再 reopen（旧实现此时下界回落 0）→ 再写 8 个 key。
///
/// 返回最终 store 与 `(bumped, seed_max)`。断言只在 `Pooled` 下检查下界；
/// `Append` 作为对照（无空闲池，旧实现也通过）。
async fn legacy_term_bump_roundtrip<I: PageIo + ManifestIo>(
  io: I,
  tbl: &TableIdent,
  mode: AllocMode,
) -> (Store<I>, u64, u64) {
  let entries = legacy_batch();
  let mut term0 = open(io, 0, mode);
  term0.put(tbl, &entries).await.unwrap();
  let old_max = term0.mapper(tbl).unwrap().max_fp_id();
  let snapshot = legacy_snapshot(&term0, tbl);
  let io = term0.into_io();
  io.switch_manifest(tbl, &snapshot).await.unwrap();

  // 第一次 reopen：manifest term 0 ≠ attach term 1 → bump。
  let mut term1 = open(io, 1, mode);
  term1.reopen(tbl).await.unwrap();
  let bumped = ((old_max >> SHIFT) + 1) << SHIFT;
  assert_eq!(
    term1.mapper(tbl).unwrap().max_fp_id(),
    bumped,
    "legacy manifest 的 term bump 必须抬到下一文件边界"
  );
  // 第一笔新 term 写：旧实现从这一步开始把新 term 区间挂到空链上（隐式起点 0）。
  for entry in &new_batch()[..1] {
    term1.put(tbl, slice::from_ref(entry)).await.unwrap();
  }
  let seed_max = term1.mapper(tbl).unwrap().max_fp_id();
  let io = term1.into_io();

  // 第二次 reopen（同 term 1）：旧实现 term_range_start = 0，下界回落。
  let mut again = open(io, 1, mode);
  again.reopen(tbl).await.unwrap();
  assert_eq!(
    again.mapper(tbl).unwrap().max_fp_id(),
    seed_max,
    "同 term 重启不得再抬水位"
  );
  if mode == AllocMode::Pooled {
    let floor = again.mapper(tbl).unwrap().reuse_floor();
    assert!(
      floor >= bumped,
      "同 term 重启后 Pooled 复用下界必须 >= term bump 边界: floor={floor} bumped={bumped}"
    );
    // 池内不得残留低于 bump 边界的页（旧 term 文件的空洞）。
    let mapper = again.mapper(tbl).unwrap();
    let mut probe = mapper.clone();
    for _ in 0..mapper.pooled_free_len() {
      let page_id = probe.allocate_page();
      let fp_id = probe.file_page_id(page_id).expect("mapped");
      assert!(
        fp_id >= bumped,
        "空闲池发放了低于 bump 边界的旧 term 页: fp_id={fp_id} bumped={bumped}"
      );
    }
  }

  for entry in &new_batch()[1..] {
    again.put(tbl, slice::from_ref(entry)).await.unwrap();
  }
  (again, bumped, seed_max)
}

/// 三种后端共用的最终断言：旧 key（不校验 checksum）与新 key 全部读回原值。
async fn assert_roundtrip<I: PageIo + ManifestIo>(
  store: &Store<I>,
  tbl: &TableIdent,
  context: &str,
) {
  for entry in legacy_batch() {
    assert_eq!(
      store.get(tbl, &entry.key).await.unwrap(),
      Some(entry.value.clone()),
      "{context}: 旧 term key 在新 term 下不可读: {:?}",
      String::from_utf8_lossy(&entry.key)
    );
  }
  for entry in new_batch() {
    assert_eq!(
      store.get(tbl, &entry.key).await.unwrap(),
      Some(entry.value.clone()),
      "{context}: 新 term key 不可读: {:?}",
      String::from_utf8_lossy(&entry.key)
    );
  }
}

/// CompioStore：旧 file id 下不得出现新 term 文件。
fn assert_no_new_term_file_in_old_range(root: &TempRoot, tbl: &TableIdent, bumped: u64) {
  let dir = root.join(tbl.to_string());
  let bumped_file = bumped >> SHIFT;
  for file_id in 0..bumped_file {
    let name = format!("data_{file_id}_main_1");
    assert!(
      !dir.join(&name).exists(),
      "旧 file id 下出现了新 term 文件: {name}"
    );
  }
}

#[test]
fn mem_pooled_legacy_empty_chain_keeps_term_floor() {
  pollster::block_on(async {
    let tbl = TableIdent::new("legacy_mem_pooled", 1).unwrap();
    let (again, ..) = legacy_term_bump_roundtrip(
      MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
      &tbl,
      AllocMode::Pooled,
    )
    .await;
    assert_roundtrip(&again, &tbl, "MemStore Pooled").await;
  });
}

#[test]
fn mem_append_legacy_empty_chain_keeps_old_pages() {
  pollster::block_on(async {
    let tbl = TableIdent::new("legacy_mem_append", 1).unwrap();
    let (again, ..) = legacy_term_bump_roundtrip(
      MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
      &tbl,
      AllocMode::Append,
    )
    .await;
    assert_roundtrip(&again, &tbl, "MemStore Append").await;
  });
}

#[test]
fn sim_pooled_legacy_empty_chain_keeps_term_floor() {
  pollster::block_on(async {
    let tbl = TableIdent::new("legacy_sim_pooled", 1).unwrap();
    let (again, ..) = legacy_term_bump_roundtrip(
      SimStore::new(PAGE_SIZE).with_pages_per_file(PAGE_SIZE, SHIFT),
      &tbl,
      AllocMode::Pooled,
    )
    .await;
    assert_roundtrip(&again, &tbl, "SimStore Pooled").await;
  });
}

#[test]
fn sim_append_legacy_empty_chain_keeps_old_pages() {
  pollster::block_on(async {
    let tbl = TableIdent::new("legacy_sim_append", 1).unwrap();
    let (again, ..) = legacy_term_bump_roundtrip(
      SimStore::new(PAGE_SIZE).with_pages_per_file(PAGE_SIZE, SHIFT),
      &tbl,
      AllocMode::Append,
    )
    .await;
    assert_roundtrip(&again, &tbl, "SimStore Append").await;
  });
}

#[compio::test]
async fn compio_pooled_legacy_empty_chain_keeps_term_floor() {
  let root = TempRoot::new("legacy_compio_pooled");
  let tbl = TableIdent::new("legacy_compio_pooled", 1).unwrap();
  let (again, bumped, _) = legacy_term_bump_roundtrip(
    CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT),
    &tbl,
    AllocMode::Pooled,
  )
  .await;
  assert_roundtrip(&again, &tbl, "CompioStore Pooled").await;
  assert_no_new_term_file_in_old_range(&root, &tbl, bumped);
}

#[compio::test]
async fn compio_append_legacy_empty_chain_keeps_old_pages() {
  let root = TempRoot::new("legacy_compio_append");
  let tbl = TableIdent::new("legacy_compio_append", 1).unwrap();
  let (again, bumped, _) = legacy_term_bump_roundtrip(
    CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT),
    &tbl,
    AllocMode::Append,
  )
  .await;
  assert_roundtrip(&again, &tbl, "CompioStore Append").await;
  // Append 同样按当前 identity 命名，不得回写旧 file id。
  assert_no_new_term_file_in_old_range(&root, &tbl, bumped);
}
