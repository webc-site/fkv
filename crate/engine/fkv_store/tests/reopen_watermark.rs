//! l2-05：reopen 时 `max_fp_id` 的抬升与 term bump 的 branch 上限保护。
//!
//! 两个用例都锁定 fkv 的**单侧加固**（C++ 无对应机制），登记见
//! `COMPAT_BASELINE`「reopen 时的 `max_fp_id` 抬升」与「term bump 条件」
//! 两条；差分对拍遇到这两种输入应判为已知差异，不得以 C++ 行为为 oracle。
//!
//! 1. `low_watermark_manifest_is_raised_and_pages_are_not_reused`：
//!    manifest 自报水位低于被引用页时必须抬到映射最大值 + 1。不抬升时
//!    `AppendAllocator(max_fp_id_)` / `PooledFilePages` 的下一次 `Allocate()`
//!    就是水位本身，会交出仍被引用的物理页（两个逻辑页共用一个 file page id）。
//! 2. `term_bump_at_branch_limit_keeps_high_water`：`branch_file_limit` 是 fkv 的
//!    branch 预留块上界（C++ `CreateBranch` 不预留，无此概念）。term 变化时若
//!    「跳到下一文件边界」会落到块末尾（甚至越界），必须保留原高水位，否则
//!    reopen 后的每一笔写都撞上限报 `OutOfSpace`。

use std::collections::BTreeSet;

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArg};
use fkv_store::{AllocMode, BRANCH_FILE_RESERVE_FILE_NUM, Store};
use fkv_types::{
  BranchFileRange, NULL_PAGE, TableIdent, decode_id, encode_file_page_id, is_file_page_id,
};

const PAGE_SIZE: usize = 512;
const RESTART: u16 = 8;
const SHIFT: u8 = 2;
/// feature 分支预留块基址（file id）：区间链首个本 branch 区间起点。
const BLOCK_BASE_FILE: u64 = 1024;
/// 块末尾（页 id，半开上界）：`(1024 + 1024) << 2`。
const BLOCK_END_FP: u64 = (BLOCK_BASE_FILE + BRANCH_FILE_RESERVE_FILE_NUM) << SHIFT;

fn up(key: &[u8], value: &[u8], ts: u64) -> BatchEntry {
  BatchEntry::upsert(key.to_vec(), value.to_vec(), ts)
}

fn mem() -> MemStore {
  MemStore::with_pages_per_file(PAGE_SIZE, SHIFT)
}

fn store(io: MemStore, mode: AllocMode) -> Store<MemStore> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(SHIFT)
    .with_alloc_mode(mode)
}

/// `max_fp_id` 低于映射最大页的 manifest（水位 3，引用物理页 5 / 100）。
///
/// 写侧不变量「水位 = 分配器高水位」被刻意破坏：这正是 C++ 断言
/// `assert(using_fp_ids.back() < max_fp_id_)`（`eloqstore/src/replayer.cpp:388`）
/// 拦住的输入形状。
fn low_watermark_main_manifest() -> Vec<u8> {
  let metadata = BranchManifestMetadata::new("main", 0);
  ManifestBuilder::new()
    .snapshot(
      SnapshotArg {
        root_id: NULL_PAGE,
        ttl_root: NULL_PAGE,
        max_fp_id: 3,
        dict_bytes: &[],
        branch_metadata: &metadata,
        max_segment_fp_id: 0,
      },
      &[encode_file_page_id(5), encode_file_page_id(100)],
      None,
    )
    .unwrap()
}

/// manifest 自报水位低于被引用页：reopen 必须抬到 `max(mapping) + 1`，
/// 且后续写不得再分配同一物理页。
#[test]
fn low_watermark_manifest_is_raised_and_pages_are_not_reused() {
  pollster::block_on(async {
    for mode in [AllocMode::Append, AllocMode::Pooled] {
      let tbl = TableIdent::new("l205_watermark", 1).unwrap();
      let io = mem();
      io.switch_manifest(&tbl, &low_watermark_main_manifest())
        .await
        .unwrap();
      let mut store = store(io, mode);
      store.prepare_attach("main", 0).unwrap();
      store.reopen(&tbl).await.unwrap();
      assert_eq!(
        store.mapper(&tbl).unwrap().max_fp_id(),
        101,
        "reopen 后水位必须覆盖映射最大页 + 1 mode={mode:?}"
      );

      // 继续写：新分配的物理页不得与仍在映射里的 5 / 100 撞号。
      for i in 0..16u32 {
        let key = format!("k{i:03}").into_bytes();
        store
          .put(&tbl, &[up(&key, b"V", u64::from(i) + 1)])
          .await
          .unwrap();
      }
      let mut seen = BTreeSet::new();
      for value in store.mapper(&tbl).unwrap().tbl().iter_value_li() {
        if is_file_page_id(value) {
          let fp_id = decode_id(value);
          assert!(
            seen.insert(fp_id),
            "物理页 {fp_id} 被两个逻辑页引用 mode={mode:?}（水位未按映射抬升？）"
          );
        }
      }
      assert!(
        seen.contains(&5) && seen.contains(&100),
        "被引用页不得被回收"
      );
    }
  });
}

/// 对照面：写侧不变量成立（水位 = 分配器高水位）时，抬升是恒等变换，
/// reopen 水位与 manifest 自报值逐位一致。差分对拍以该形状为准。
#[test]
fn well_formed_manifest_watermark_is_unchanged() {
  pollster::block_on(async {
    let tbl = TableIdent::new("l205_watermark_ok", 1).unwrap();
    let metadata = BranchManifestMetadata::new("main", 0);
    let bytes = ManifestBuilder::new()
      .snapshot(
        SnapshotArg {
          root_id: NULL_PAGE,
          ttl_root: NULL_PAGE,
          max_fp_id: 101,
          dict_bytes: &[],
          branch_metadata: &metadata,
          max_segment_fp_id: 0,
        },
        &[encode_file_page_id(5), encode_file_page_id(100)],
        None,
      )
      .unwrap();
    let io = mem();
    io.switch_manifest(&tbl, &bytes).await.unwrap();
    let mut store = store(io, AllocMode::Append);
    store.prepare_attach("main", 0).unwrap();
    store.reopen(&tbl).await.unwrap();
    assert_eq!(store.mapper(&tbl).unwrap().max_fp_id(), 101);
  });
}

fn range(branch: &str, max_file_id: u64) -> BranchFileRange {
  BranchFileRange {
    branch_name: branch.into(),
    term: 0,
    max_file_id,
    max_segment_file_id: 0,
  }
}

/// 与 `create_branch` 产物同形状的 feature manifest：父区间到 file 1023，
/// 本分支预留块 `[1024, 2048)` → `branch_file_limit = BLOCK_END_FP`。
fn feature_manifest(max_fp_id: u64) -> Vec<u8> {
  let metadata = BranchManifestMetadata {
    branch_name: "feature".into(),
    term: 0,
    file_range_li: vec![
      range("main", BLOCK_BASE_FILE - 1),
      range("feature", BLOCK_BASE_FILE + BRANCH_FILE_RESERVE_FILE_NUM),
    ],
  };
  ManifestBuilder::new()
    .snapshot(
      SnapshotArg {
        root_id: NULL_PAGE,
        ttl_root: NULL_PAGE,
        max_fp_id,
        dict_bytes: &[],
        branch_metadata: &metadata,
        max_segment_fp_id: 0,
      },
      &[],
      None,
    )
    .unwrap()
}

/// term 变化时「下一文件边界」正好落在 branch 预留块末尾：必须保留原高水位。
///
/// C++ 的 term bump 没有上限概念（`eloqstore/src/replayer.cpp:356-362`，
/// C++ `CreateBranch` 不预留块），因此该保护无法与 C++ 对齐——删掉它会让
/// reopen 后分配器停在 `max == limit` 的合法终态上、下一批写立即
/// `OutOfSpace`（`write.rs` 的 `mapper_max > limit` 检查）。
#[test]
fn term_bump_at_branch_limit_keeps_high_water() {
  pollster::block_on(async {
    let tbl = TableIdent::new("l205_limit", 1).unwrap();
    // 水位落在块末尾那个文件里（2047）：`next_file_boundary` = 8192 = 上限。
    let max_fp_id = BLOCK_END_FP - 4;
    let io = mem();
    io.write_manifest_file(&tbl, "manifest_feature_0", &feature_manifest(max_fp_id))
      .await
      .unwrap();

    // attach term 7，manifest 记 term 0 → bump 条件成立（对齐 C++ 的
    // `expect_term != 0 && manifest_term != expect_term`）。
    let mut feature = store(io, AllocMode::Append);
    feature.prepare_attach("feature", 7).unwrap();
    feature.reopen(&tbl).await.unwrap();
    assert_eq!(feature.branch_file_limit(), Some(BLOCK_END_FP));
    assert_eq!(
      feature.mapper(&tbl).unwrap().max_fp_id(),
      max_fp_id,
      "bump 会撞 branch 预留上限时必须保留原高水位"
    );

    // 保留高水位后块内仍可写读；不保留则水位抬到 8192，下一批写即 OutOfSpace。
    feature.put(&tbl, &[up(b"f", b"F", 1)]).await.unwrap();
    assert_eq!(feature.get(&tbl, b"f").await.unwrap(), Some(b"F".to_vec()));
    assert!(feature.mapper(&tbl).unwrap().max_fp_id() <= BLOCK_END_FP);
  });
}

/// 对照面：bump 不撞上限时照常跳到下一文件边界（对齐 C++ `GetMapper`）。
#[test]
fn term_bump_below_branch_limit_jumps_to_next_file_boundary() {
  pollster::block_on(async {
    let tbl = TableIdent::new("l205_limit_ok", 1).unwrap();
    let max_fp_id = BLOCK_END_FP - 20;
    let io = mem();
    io.write_manifest_file(&tbl, "manifest_feature_0", &feature_manifest(max_fp_id))
      .await
      .unwrap();

    let mut feature = store(io, AllocMode::Append);
    feature.prepare_attach("feature", 7).unwrap();
    feature.reopen(&tbl).await.unwrap();
    let bumped = ((max_fp_id >> SHIFT) + 1) << SHIFT;
    assert_eq!(feature.mapper(&tbl).unwrap().max_fp_id(), bumped);
    assert!(bumped < BLOCK_END_FP);
  });
}

/// 同 term 重开不 bump（`manifest_term == expect_term`），水位原地续写。
#[test]
fn same_term_reopen_does_not_bump() {
  pollster::block_on(async {
    let tbl = TableIdent::new("l205_same_term", 1).unwrap();
    let max_fp_id = BLOCK_END_FP - 20;
    let io = mem();
    io.write_manifest_file(&tbl, "manifest_feature_0", &feature_manifest(max_fp_id))
      .await
      .unwrap();

    // manifest 记 term 0，attach term 0 → 条件 `self.term != 0` 不成立。
    let mut feature = store(io, AllocMode::Append);
    feature.prepare_attach("feature", 0).unwrap();
    feature.reopen(&tbl).await.unwrap();
    assert_eq!(feature.mapper(&tbl).unwrap().max_fp_id(), max_fp_id);
  });
}

/// 上限保护只在真实的 branch 预留块（具名 branch manifest）下生效；
/// main 无预留块，bump 照常执行。锁住「保护不误伤 main」。
#[test]
fn main_branch_has_no_limit_and_bumps() {
  pollster::block_on(async {
    let tbl = TableIdent::new("l205_main_bump", 1).unwrap();
    let metadata = BranchManifestMetadata::new("main", 0);
    let bytes = ManifestBuilder::new()
      .snapshot(
        SnapshotArg {
          root_id: NULL_PAGE,
          ttl_root: NULL_PAGE,
          max_fp_id: 40,
          dict_bytes: &[],
          branch_metadata: &metadata,
          max_segment_fp_id: 0,
        },
        &[],
        None,
      )
      .unwrap();
    let io = mem();
    io.switch_manifest(&tbl, &bytes).await.unwrap();
    let mut store = store(io, AllocMode::Append);
    store.prepare_attach("main", 7).unwrap();
    store.reopen(&tbl).await.unwrap();
    assert_eq!(store.branch_file_limit(), None);
    assert_eq!(store.mapper(&tbl).unwrap().max_fp_id(), 44);
  });
}

/// 上限保护的对照面：水位已停在 `branch_file_limit` 上时写入必须 `OutOfSpace`
/// （`write.rs` 的前置 `max_fp_id >= limit` 检查），说明「保留原高水位」不是
/// 可有可无的宽容，而是这条拒绝规则的必要前提。
#[test]
fn branch_limit_overflow_is_out_of_space() {
  pollster::block_on(async {
    let tbl = TableIdent::new("l205_oom", 1).unwrap();
    let io = mem();
    io.write_manifest_file(&tbl, "manifest_feature_0", &feature_manifest(BLOCK_END_FP))
      .await
      .unwrap();
    let mut feature = store(io, AllocMode::Append);
    feature.prepare_attach("feature", 0).unwrap();
    feature.reopen(&tbl).await.unwrap();
    assert_eq!(feature.mapper(&tbl).unwrap().max_fp_id(), BLOCK_END_FP);
    let err = feature
      .put(&tbl, &[up(b"f", b"F", 1)])
      .await
      .expect_err("水位已到块末尾必须 OutOfSpace");
    assert_eq!(err, KvError::OutOfSpace);
  });
}
