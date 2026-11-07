//! io-28 / io-29：legacy 空区间链 term bump 时，data 侧抬升、segment 侧被
//! branch 预留上限挡回（`bumped_segment_fp_id == branch_segment_limit`）。
//!
//! pooled-14 补的合成旧 term 区间只检查了 `max_fp_id == bumped_fp_id`，segment
//! 上界最初按旧水位无条件填写。被挡回时旧水位 `max_segment_fp_id` 恰好是**新
//! term 下一次段分配所在的 file id**：写路径按当前 identity 写到
//! `segment_<f>_<branch>_<new>`，读路径按区间链解析成 `segment_<f>_..._<old>`
//! （旧 term 从未写过该文件），PutLarge 回执成功但 GetLarge 返回
//! `ResourceMissing`。
//!
//! 修复：被挡回时只声称**完全属于旧 term** 的最后一个文件
//! `(旧水位 >> shift) - 1`：
//! - 共享 file（旧水位所在文件）留给当前 identity，新 term 段页照旧可读
//!   （io-28）；
//! - 旧 term 独占的 file 1 .. 水位-1 仍按旧 identity 解析（io-29）；填 0 会把
//!   这些文件让给新 term，旧大值读不回。
//!
//! 夹具（`pages_per_file_shift=2`、`segments_per_file_shift=0`）：
//! - term 0 写 1023 条 4 KiB 大值 ⇒ `max_segment_fp_id=1023`（每段一文件，
//!   `legacy0` → file 0、`old0001` → file 1、`old1022` → file 1022）；
//! - legacy 空链 + term 1 reopen：data bump 到下一文件边界，segment bump
//!   撞 legacy fallback 上限 1024 被挡回，旧水位保持 1023；
//! - 新 term 写一条 4 KiB 大值 ⇒ 段落在 file 1023（与旧/新 term 共享的文件
//!   号边界），必须写 `segment_1023_br_1` 并能按同一 identity 读回。

use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArg};
use fkv_object::{MemObjectStore, ObjectStore};
use fkv_store::{AllocMode, Store};
use fkv_types::{NULL_PAGE, TableIdent};

const PAGE_SIZE: usize = 512;
const SHIFT: u8 = 2;
const RESTART: u16 = 16;
const SEGMENT_SIZE: usize = 4096;
/// legacy 空链 fallback 的 segment 预留上限（file id 半开）：1024。
const SEGMENT_LIMIT_FILE: u64 = 1024;
const LEGACY_NUM: u32 = 1023;
const BRANCH: &str = "br";

type Cloud = Store<CloudStore<MemStore, MemObjectStore>>;

fn cloud(remote: &MemObjectStore, term: u64, mode: AllocMode) -> Cloud {
  Store::new(
    CloudStore::new(
      MemStore::with_pages_per_file(PAGE_SIZE, SHIFT).with_segment_config(SEGMENT_SIZE, 0),
      remote.clone(),
      "fkv",
      SHIFT,
    )
    .with_segment_config(SEGMENT_SIZE, 0),
    PAGE_SIZE,
    RESTART,
  )
  .with_pages_per_file_shift(SHIFT)
  .with_large_value(
    SegmentLayout::new(SEGMENT_SIZE as u32, 0).expect("segment layout"),
    SEGMENT_SIZE,
  )
  .with_alloc_mode(mode)
  .with_branch(BRANCH, term)
}

fn entry(key: &[u8], value: &[u8], ts: u64) -> BatchEntry {
  BatchEntry::upsert(key.to_vec(), value.to_vec(), ts)
}

/// 把 branch manifest 覆写成 pre-range-chain legacy 形状：`(br, 0)` + 空链，
/// mapping / root / 两个水位原样保留。
fn legacy_snapshot(store: &Cloud, tbl: &TableIdent) -> Vec<u8> {
  let mapper = store.mapper(tbl).expect("mapper");
  let segment_mapper = store.segment_mapper(tbl).expect("segment mapper");
  let mapping_value_li: Vec<u64> = mapper.tbl().iter_value_li().collect();
  let segment_value_li: Vec<u64> = segment_mapper.tbl().iter_value_li().collect();
  ManifestBuilder::new()
    .snapshot(
      SnapshotArg {
        root_id: store.root_id(tbl).expect("root"),
        ttl_root: NULL_PAGE,
        max_fp_id: mapper.max_fp_id(),
        dict_bytes: &[],
        branch_metadata: &BranchManifestMetadata::new(BRANCH, 0),
        max_segment_fp_id: segment_mapper.max_fp_id(),
      },
      &mapping_value_li,
      Some(&segment_value_li),
    )
    .unwrap()
}

/// 旧 term 的三条大值各落在独立的 segment 文件：`legacy0` → file 0、
/// `old0001` → file 1、`old1022` → file 1022（见 [`legacy_segment_roundtrip`]
/// 的写入顺序）。任一条读不回都说明合成区间的段归属上界覆盖不足。
async fn assert_old_values(store: &Cloud, tbl: &TableIdent, phase: &str) {
  let old_first = vec![0x11u8; SEGMENT_SIZE];
  let old_last = vec![0x22u8; SEGMENT_SIZE];
  let read0 = store.get_large(tbl, b"legacy0").await.unwrap().unwrap();
  assert_eq!(read0.value, old_first, "{phase}：旧 term file 0 不可读");
  let read1 = store.get_large(tbl, b"old0001").await.unwrap().unwrap();
  assert_eq!(read1.value, old_last, "{phase}：旧 term file 1 不可读");
  let read1022 = store.get_large(tbl, b"old1022").await.unwrap().unwrap();
  assert_eq!(
    read1022.value, old_last,
    "{phase}：旧 term file 1022 不可读"
  );
}

/// 返回修复后被重开的新 term Store。
async fn legacy_segment_roundtrip(
  remote: &MemObjectStore,
  tbl: &TableIdent,
  mode: AllocMode,
) -> Cloud {
  let old_first = vec![0x11u8; SEGMENT_SIZE];
  let old_last = vec![0x22u8; SEGMENT_SIZE];
  let new_value = vec![0xA5u8; SEGMENT_SIZE];

  // term 0：具名 branch 直接 attach（legacy 空链由后续覆写构造），先写一条落
  // 在 segment file 0 的旧值，再补满到 segment 水位 1023（共 1023 段）。
  let mut term0 = cloud(remote, 0, mode);
  term0.adopt_attach(BRANCH, 0).unwrap();
  term0
    .put_large(tbl, &[entry(b"legacy0", &old_first, 1)])
    .await
    .unwrap();
  for chunk in (1..LEGACY_NUM).collect::<Vec<_>>().chunks(64) {
    let batch: Vec<_> = chunk
      .iter()
      .map(|&index| entry(format!("old{index:04}").as_bytes(), &old_last, 1))
      .collect();
    term0.put_large(tbl, &batch).await.unwrap();
  }
  let old_max = term0.mapper(tbl).unwrap().max_fp_id();
  let old_seg_max = term0.segment_mapper(tbl).unwrap().max_fp_id();
  assert_eq!(
    old_seg_max, LEGACY_NUM as u64,
    "每段一文件：1023 条大值后段水位必须是 1023"
  );
  // legacy 空链覆写前没有区间归属，三条旧值都按当前 identity (`_br_0`) 可读。
  assert_old_values(&term0, tbl, "legacy 覆写前").await;
  {
    let dir = format!("fkv/{tbl}");
    for file in [1u64, 1022] {
      assert!(
        remote
          .get(&format!("{dir}/segment_{file}_br_0"))
          .await
          .unwrap()
          .is_some(),
        "旧 term 的 segment file {file} 必须存在"
      );
    }
  }
  let snapshot = legacy_snapshot(&term0, tbl);
  let io = term0.into_io();
  io.write_manifest_file(tbl, &format!("manifest_{BRANCH}_0"), &snapshot)
    .await
    .unwrap();
  drop(io);

  // term 1 冷启动：data bump 成功，segment bump 撞 1024 上限被挡回。
  let mut term1 = cloud(remote, 1, mode);
  term1.prepare_attach(BRANCH, 1).unwrap();
  term1.reopen(tbl).await.unwrap();
  let bumped = ((old_max >> SHIFT) + 1) << SHIFT;
  assert_eq!(
    term1.mapper(tbl).unwrap().max_fp_id(),
    bumped,
    "data 侧 bump 必须抬到下一文件边界"
  );
  assert_eq!(
    term1.segment_mapper(tbl).unwrap().max_fp_id(),
    old_seg_max,
    "segment bump 被 branch 上限挡回时必须保留旧水位"
  );
  assert_eq!(
    term1.branch_segment_limit(),
    Some(SEGMENT_LIMIT_FILE),
    "legacy 空链 fallback 的 segment 上限"
  );

  // 新 term 还没写任何值：合成旧 term 区间的段上界必须覆盖旧 term 独占的
  // file 1 .. 1022。填 0 时 file 1..1022 超出链尾回退当前 identity，读成
  // `_br_1` 对象 → ResourceMissing。
  assert_old_values(&term1, tbl, "reopen 后、写新值前").await;

  // 新 term 的第一条大值落在共享 file 1023（= 旧水位，下一次分配）。
  term1
    .put_large(tbl, &[entry(b"new-large", &new_value, 2)])
    .await
    .unwrap();
  assert_eq!(
    term1.segment_mapper(tbl).unwrap().max_fp_id(),
    old_seg_max + 1,
    "新值只分配 1 个 segment"
  );
  {
    let dir = format!("fkv/{tbl}");
    assert!(
      remote
        .get(&format!("{dir}/segment_{LEGACY_NUM}_br_1"))
        .await
        .unwrap()
        .is_some(),
      "新 term 段页必须写当前 identity"
    );
    assert!(
      remote
        .get(&format!("{dir}/segment_{LEGACY_NUM}_br_0"))
        .await
        .unwrap()
        .is_none(),
      "旧 term 从未写过共享 file {LEGACY_NUM}"
    );
    // 旧 term 独占文件的写键不因 reopen / 新值写入而改道：远端只有
    // `segment_<f>_br_0`，没有新 term 的同号对象。
    for file in [1u64, 1022] {
      assert!(
        remote
          .get(&format!("{dir}/segment_{file}_br_0"))
          .await
          .unwrap()
          .is_some(),
        "旧 term 的 segment file {file} 被改写或丢失"
      );
      assert!(
        remote
          .get(&format!("{dir}/segment_{file}_br_1"))
          .await
          .unwrap()
          .is_none(),
        "新 term 不应写到旧 term 独占的 file {file}"
      );
    }
  }

  // 写成功必须能读回完整新值（修复前这里稳定 ResourceMissing）。
  let read_new = term1.get_large(tbl, b"new-large").await.unwrap().unwrap();
  assert_eq!(read_new.value, new_value, "新 term 大值读回不完整");
  // 旧 term 独占文件（file 0 / 1 / 1022）在写新值后仍按旧 identity 解析：
  // io-29 的核心断言——旧上界填 0 时 file 1..1022 读不回。
  assert_old_values(&term1, tbl, "写新值后").await;
  term1
}

/// 反向实验（两侧）：
/// - 上界改回「无条件填旧段水位」→ `get_large(new-large)` 处稳定
///   `ResourceMissing`（io-28）；
/// - 上界改回「被挡回时填 0」→ `old0001` / `old1022` 处稳定
///   `ResourceMissing`（io-29）。
/// 修复后新值与旧值全部完整读回。
#[compio::test]
async fn cloud_pooled_legacy_segment_bump_blocked_reads_new_term() {
  let tbl = TableIdent::new("legacy_seg_pooled", 1).unwrap();
  let remote = MemObjectStore::new();
  let term1 = legacy_segment_roundtrip(&remote, &tbl, AllocMode::Pooled).await;
  drop(term1);
}

#[compio::test]
async fn cloud_append_legacy_segment_bump_blocked_reads_new_term() {
  let tbl = TableIdent::new("legacy_seg_append", 1).unwrap();
  let remote = MemObjectStore::new();
  let term1 = legacy_segment_roundtrip(&remote, &tbl, AllocMode::Append).await;
  drop(term1);
}
