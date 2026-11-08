//! pooled-09：branch 预留上限必须在 `reopen` / `reopen_archive` 安装路径恢复，
//! 且块基址不能从移动的 `max_fp_id` 反推。
//!
//! 缺陷一：`branch_file_limit` / `branch_segment_limit` 只在 `open_branch`
//! 派生。`adopt_attach` / `prepare_attach` 会先把它们清成 `None`，而
//! `Store::reopen`（具名 branch manifest）与 `Store::reopen_archive`
//! （shard `ReopenArchive` 是 `SKIPS_OPEN`，不会先走 `open_branch`）安装后
//! 不再重建，`write.rs` 的 `mapper_max > limit` 检查因上限为 `None` 整段跳过。
//!
//! 缺陷二：`open_branch` 用 `max_fp_id >> shift` 反推块基址。`max_fp_id` 是
//! 移动的分配水位，分支**恰好写满本块**（`max == limit`，合法终态）时落点
//! 正好是下一块基址，重算出的上限多扩一块（pooled-08 的越界 max 也会被
//! 「合法化」成更大的块）。修复后块基址取 `file_range_li` 中首个本 branch
//! 区间起点的预留边界，创建后不随写入漂移。
//!
//! 夹具块布局（`pages_per_file_shift = 2`，每文件 4 页）：feature 预留块
//! `[1024, 2048)` 文件 = `[4096, 8192)` 页，`BLOCK_END_FP = 8192` 是合法终态。
//! segment 侧同一布局（`segments_per_file_shift = 2`）。

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArg};
use fkv_store::{AllocMode, BRANCH_FILE_RESERVE_FILE_NUM, Store};
use fkv_types::{BranchFileRange, NULL_PAGE, TableIdent};

const PAGE_SIZE: usize = 512;
const RESTART: u16 = 8;
const SHIFT: u8 = 2;
/// feature 预留块起点（file id）：父分支高水位 1023 → ceil(1024/1024)*1024。
const BLOCK_BASE_FILE: u64 = 1024;
/// feature 块末尾（页 id，半开区间上界）：`(1024 + 1024) << 2`。
const BLOCK_END_FP: u64 = 8192;
const SEGMENT_SIZE: usize = 4096;
const LARGE_THRESHOLD: usize = 1024;

fn up(key: &[u8], value: &[u8], ts: u64) -> BatchEntry {
  BatchEntry::upsert(key.to_vec(), value.to_vec(), ts)
}

fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

fn mem() -> MemStore {
  MemStore::with_pages_per_file(PAGE_SIZE, SHIFT)
}

fn store(io: MemStore) -> Store<MemStore> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(SHIFT)
    .with_alloc_mode(AllocMode::Pooled)
}

fn segment_store(io: MemStore) -> Store<MemStore> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(SHIFT)
    .with_large_value(
      SegmentLayout::new(SEGMENT_SIZE as u32, SHIFT).unwrap(),
      LARGE_THRESHOLD,
    )
    .with_alloc_mode(AllocMode::Pooled)
}

fn segment_mem() -> MemStore {
  mem().with_segment_config(SEGMENT_SIZE, SHIFT)
}

fn range(branch: &str, max_file_id: u64, max_segment_file_id: u64) -> BranchFileRange {
  BranchFileRange {
    branch_name: branch.into(),
    term: 0,
    max_file_id,
    max_segment_file_id,
  }
}

/// 与 `create_branch` 产物同形状的 branch manifest：
/// 父区间到 file `BLOCK_BASE_FILE - 1`，本分支预留块 `[1024, 2048)`。
///
/// 区间链是块基址的唯一稳定来源：首个本 branch 区间起点 = 1024，向上取整到
/// 预留边界仍是 1024；`max_fp_id` 只决定「已分配到哪」。
fn branch_manifest(max_fp_id: u64, max_segment_fp_id: u64, with_segment: bool) -> Vec<u8> {
  let metadata = BranchManifestMetadata {
    branch_name: "feature".into(),
    term: 0,
    file_range_li: vec![
      range("main", BLOCK_BASE_FILE - 1, 0),
      range(
        "feature",
        BLOCK_BASE_FILE + BRANCH_FILE_RESERVE_FILE_NUM,
        if with_segment { BLOCK_BASE_FILE } else { 0 },
      ),
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
        max_segment_fp_id,
      },
      &[],
      with_segment.then_some(&[] as &[u64]),
    )
    .unwrap()
}

async fn write_manifest(io: &MemStore, tbl: &TableIdent, name: &str, bytes: Vec<u8>) {
  io.write_manifest_file(tbl, name, &bytes).await.unwrap();
}

/// `prepare_attach` + `reopen`：公开 Store API 的第二条 branch 安装路径。
#[test]
fn branch_reopen_restores_file_limit() {
  pollster::block_on(async {
    let tbl = TableIdent::new("p09_reopen", 1).unwrap();
    let mut main = store(mem());
    main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
    main.create_branch(&tbl, "feature").await.unwrap();
    // 兄弟分支把父表水位推高到 16384，pooled-08 修复后不再污染本分支下界。
    main.create_branch(&tbl, "other").await.unwrap();
    main.put(&tbl, &[up(b"m", b"MAIN", 2)]).await.unwrap();
    let io = main.into_io();

    let mut feature = store(io);
    feature.prepare_attach("feature", 0).unwrap();
    feature.reopen(&tbl).await.unwrap();
    assert_eq!(
      feature.branch_file_limit(),
      Some(BLOCK_END_FP),
      "reopen 后必须恢复 branch 预留上限"
    );
    assert!(
      feature.mapper(&tbl).unwrap().max_fp_id() <= BLOCK_END_FP,
      "reopen 后分配水位越过了本分支块末尾"
    );
    feature
      .put(&tbl, &[up(b"f", b"F", 3)])
      .await
      .expect("块内写必须成功");
    assert_eq!(feature.get(&tbl, b"f").await.unwrap(), Some(b"F".to_vec()));
    let io = feature.into_io();

    let mut main = store(io);
    main.prepare_attach("main", 0).unwrap();
    main.reopen(&tbl).await.unwrap();
    assert_eq!(
      main.get(&tbl, b"m").await.unwrap(),
      Some(b"MAIN".to_vec()),
      "branch reopen 后的写覆盖了父表数据"
    );
    assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  });
}

/// `prepare_attach` + `reopen_archive`：shard `ReopenArchive`（`SKIPS_OPEN`）
/// 走的同一条安装路径。
#[test]
fn branch_reopen_archive_restores_file_limit() {
  pollster::block_on(async {
    let tbl = TableIdent::new("p09_reopen_archive", 1).unwrap();
    let mut main = store(mem());
    main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
    main.create_branch(&tbl, "feature").await.unwrap();
    let io = main.into_io();

    let mut feature = store(io);
    feature.open_branch(&tbl, "feature", 0).await.unwrap();
    feature.put(&tbl, &[up(b"f", b"F", 2)]).await.unwrap();
    feature.create_archive(&tbl, "t1", 4).await.unwrap();
    let io = feature.into_io();

    // 归档后父表继续推进（含兄弟分支），复现 issue 的触发顺序。
    let mut main = store(io);
    main.prepare_attach("main", 0).unwrap();
    main.reopen(&tbl).await.unwrap();
    main.create_branch(&tbl, "other").await.unwrap();
    main.put(&tbl, &[up(b"m", b"MAIN", 3)]).await.unwrap();
    let io = main.into_io();

    let mut feature = store(io);
    feature.prepare_attach("feature", 2).unwrap();
    feature.reopen_archive(&tbl, "t1").await.unwrap();
    assert_eq!(
      feature.branch_file_limit(),
      Some(BLOCK_END_FP),
      "reopen_archive 后必须恢复 branch 预留上限"
    );
    assert!(feature.mapper(&tbl).unwrap().max_fp_id() <= BLOCK_END_FP);
    assert_eq!(feature.get(&tbl, b"f").await.unwrap(), Some(b"F".to_vec()));
    feature
      .put(&tbl, &[up(b"f2", b"F2", 4)])
      .await
      .expect("归档重开后的块内写必须成功");
    let io = feature.into_io();

    let mut main = store(io);
    main.prepare_attach("main", 0).unwrap();
    main.reopen(&tbl).await.unwrap();
    assert_eq!(
      main.get(&tbl, b"m").await.unwrap(),
      Some(b"MAIN".to_vec()),
      "reopen_archive 后的写覆盖了父表数据"
    );
    assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  });
}

/// 分支恰好写满本块（`max_fp_id == limit`，合法终态）后重开：
/// `open_branch` 的上限必须保持不变，不能多扩一块。
#[test]
fn branch_open_at_block_end_keeps_limit() {
  pollster::block_on(async {
    let tbl = TableIdent::new("p09_block_end", 1).unwrap();
    let io = mem();
    write_manifest(
      &io,
      &tbl,
      "manifest_feature_0",
      branch_manifest(BLOCK_END_FP, 0, false),
    )
    .await;

    let mut feature = store(io);
    feature.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(
      feature.branch_file_limit(),
      Some(BLOCK_END_FP),
      "块末尾的合法终态被反推成下一块（多扩一个预留块）"
    );
    assert_eq!(feature.mapper(&tbl).unwrap().max_fp_id(), BLOCK_END_FP);
    assert_eq!(
      feature.put(&tbl, &[up(b"x", b"X", 1)]).await.unwrap_err(),
      KvError::OutOfSpace,
      "块已满必须拒绝写入"
    );
  });
}

/// pooled-08 的 truncate 场景会把越界 `max` 写进 manifest：重开时上限不得
/// 被「合法化」成更大的块（`max=16384` 仍属预留块之外的脏水位）。
#[test]
fn branch_open_over_limit_max_does_not_widen_block() {
  pollster::block_on(async {
    let tbl = TableIdent::new("p09_over_limit", 1).unwrap();
    let io = mem();
    write_manifest(
      &io,
      &tbl,
      "manifest_feature_0",
      branch_manifest(BLOCK_END_FP * 2, 0, false),
    )
    .await;

    let mut feature = store(io);
    feature.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(
      feature.branch_file_limit(),
      Some(BLOCK_END_FP),
      "越界 max 不得把上限放宽到下一块"
    );
    assert_eq!(
      feature.put(&tbl, &[up(b"x", b"X", 1)]).await.unwrap_err(),
      KvError::OutOfSpace
    );
  });
}

/// 块末尾 manifest 经 `prepare_attach` + `reopen` 安装：上限同样不得漂移，
/// 且写必须在分配前被拒绝。
#[test]
fn branch_reopen_at_block_end_rejects_write() {
  pollster::block_on(async {
    let tbl = TableIdent::new("p09_reopen_end", 1).unwrap();
    let io = mem();
    write_manifest(
      &io,
      &tbl,
      "manifest_feature_0",
      branch_manifest(BLOCK_END_FP, 0, false),
    )
    .await;

    let mut feature = store(io);
    feature.prepare_attach("feature", 0).unwrap();
    feature.reopen(&tbl).await.unwrap();
    assert_eq!(feature.branch_file_limit(), Some(BLOCK_END_FP));
    let before = feature.mapper(&tbl).unwrap().max_fp_id();
    assert_eq!(
      feature.put(&tbl, &[up(b"x", b"X", 1)]).await.unwrap_err(),
      KvError::OutOfSpace
    );
    assert_eq!(
      feature.mapper(&tbl).unwrap().max_fp_id(),
      before,
      "失败批不得分配越界页"
    );
  });
}

/// 块末尾 archive manifest 经 `prepare_attach` + `reopen_archive` 安装
/// （shard 的 `SKIPS_OPEN` 路径）：上限恢复，写被拒绝。
#[test]
fn branch_reopen_archive_at_block_end_rejects_write() {
  pollster::block_on(async {
    let tbl = TableIdent::new("p09_archive_end", 1).unwrap();
    let io = mem();
    let full = branch_manifest(BLOCK_END_FP, 0, false);
    write_manifest(&io, &tbl, "manifest_feature_0", full.clone()).await;
    write_manifest(&io, &tbl, "manifest_feature_0_t1", full).await;

    let mut feature = store(io);
    feature.prepare_attach("feature", 2).unwrap();
    feature.reopen_archive(&tbl, "t1").await.unwrap();
    assert_eq!(
      feature.branch_file_limit(),
      Some(BLOCK_END_FP),
      "reopen_archive 的上限不能从块末尾 max 反推多扩一块"
    );
    assert_eq!(
      feature.put(&tbl, &[up(b"x", b"X", 1)]).await.unwrap_err(),
      KvError::OutOfSpace
    );
  });
}

/// segment 侧：`branch_segment_limit` 同样在 reopen 安装路径恢复。
#[test]
fn branch_reopen_restores_segment_limit() {
  pollster::block_on(async {
    let tbl = TableIdent::new("p09_segment_reopen", 1).unwrap();
    let mut main = segment_store(segment_mem());
    main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
    main.create_branch(&tbl, "feature").await.unwrap();
    let io = main.into_io();

    let mut feature = segment_store(io);
    feature.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(feature.branch_segment_limit(), Some(BLOCK_END_FP));
    feature
      .put_large(
        &tbl,
        &[BatchEntry::upsert(
          b"big".to_vec(),
          pattern(2 * SEGMENT_SIZE),
          2,
        )],
      )
      .await
      .unwrap();
    let io = feature.into_io();

    let mut feature = segment_store(io);
    feature.prepare_attach("feature", 0).unwrap();
    feature.reopen(&tbl).await.unwrap();
    assert_eq!(
      feature.branch_segment_limit(),
      Some(BLOCK_END_FP),
      "reopen 后必须恢复 segment 预留上限"
    );
    assert_eq!(feature.branch_file_limit(), Some(BLOCK_END_FP));
    assert!(
      feature.segment_mapper(&tbl).unwrap().max_fp_id() <= BLOCK_END_FP,
      "segment 分配水位越过了本分支块末尾"
    );
    let second = pattern(SEGMENT_SIZE);
    feature
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big2".to_vec(), second.clone(), 3)],
      )
      .await
      .expect("块内段写必须成功");
    assert_eq!(
      feature
        .get_large(&tbl, b"big2")
        .await
        .unwrap()
        .unwrap()
        .value,
      second
    );
  });
}

/// segment 侧块末尾：`open_branch` 与 `reopen` 都不得从 `max_segment_fp_id`
/// 反推多扩一块；`put_large` 必须在落段之前拒绝。
#[test]
fn branch_segment_limit_stable_at_block_end() {
  pollster::block_on(async {
    let tbl = TableIdent::new("p09_segment_end", 1).unwrap();

    for reopen in [false, true] {
      let io = segment_mem();
      write_manifest(
        &io,
        &tbl,
        "manifest_feature_0",
        branch_manifest(BLOCK_BASE_FILE << SHIFT, BLOCK_END_FP, true),
      )
      .await;
      let mut feature = segment_store(io);
      if reopen {
        feature.prepare_attach("feature", 0).unwrap();
        feature.reopen(&tbl).await.unwrap();
      } else {
        feature.open_branch(&tbl, "feature", 0).await.unwrap();
      }
      assert_eq!(
        feature.branch_segment_limit(),
        Some(BLOCK_END_FP),
        "块末尾 max_segment 被反推成下一块 reopen={reopen}"
      );
      assert_eq!(
        feature
          .put_large(
            &tbl,
            &[BatchEntry::upsert(
              b"too-big".to_vec(),
              pattern(2 * SEGMENT_SIZE),
              1
            )],
          )
          .await
          .unwrap_err(),
        KvError::OutOfSpace,
        "segment 块已满必须拒绝 reopen={reopen}"
      );
      assert_eq!(
        feature.segment_mapper(&tbl).unwrap().max_fp_id(),
        BLOCK_END_FP,
        "失败批不得分配越界段 reopen={reopen}"
      );
    }
  });
}
