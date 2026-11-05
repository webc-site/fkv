//! rev-39 变体粒度 GC：同 `file_id` 跨 `(branch, term)` 的物理文件独立判删。
//!
//! 场景背景（`fkv_store::open_branch` 的 reserve 桶对齐）：fork 后父 branch 与
//! 子 branch 从同一水位继续分配，同号 file id 真实出现——main 的 file 42 已死、
//! branch 的 file 42 活着时，只能删 main 的那个。

use std::fs::{create_dir_all, read_dir, write};

use fkv_io::{FileIo, FileVariant, ManifestIo, PageIo};
use fkv_io_compio::CompioStore;
use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArg};
use fkv_task_gc::{
  InflightGuard, gc_files_with_retained, gc_segment_files_with_retained,
  manifest_retained_file_page_set,
};
use fkv_testkit::TempRoot;
use fkv_types::{BranchFileRange, FilePageId, TableIdent, encode_file_page_id};
use gxhash::HashSet;

const DATA_LEN: usize = 512;
const SEGMENT_LEN: usize = 4096;

/// 构造一份 snapshot manifest：mapping 槽位 0 指向 `file_page_ids[0]`。
///
/// `max_file_fp_id` / `max_segment_fp_id` 是 branch 高水位（FilePageId 粒度），
/// replay 后经 `>> shift` 折入 guard 的 `least_unflushed` 边界。
fn snapshot_bytes(
  metadata: &BranchManifestMetadata,
  max_file_fp_id: FilePageId,
  file_page_id_li: &[FilePageId],
  max_segment_fp_id: FilePageId,
  segment_page_id_li: &[FilePageId],
) -> Vec<u8> {
  let mapping_value_li: Vec<u64> = file_page_id_li
    .first()
    .map(|fp| vec![encode_file_page_id(*fp)])
    .unwrap_or_default();
  let segment_value_li: Option<Vec<u64>> = (max_segment_fp_id > 0).then(|| {
    segment_page_id_li
      .first()
      .map(|fp| encode_file_page_id(*fp))
      .into_iter()
      .collect()
  });
  ManifestBuilder::new()
    .snapshot(
      SnapshotArg {
        root_id: 0,
        ttl_root: 0,
        max_fp_id: max_file_fp_id,
        dict_bytes: &[],
        branch_metadata: metadata,
        max_segment_fp_id,
      },
      &mapping_value_li,
      segment_value_li.as_deref(),
    )
    .unwrap()
}

/// 两条 branch 的高水位链：main 在 fork 时跳过预留桶（2048），b-1 从 fork 点
/// 起分配到 2050；main 自己继续写到 2052。
fn range_li(inherited_main_max: u64, branch_max: u64) -> Vec<BranchFileRange> {
  vec![
    BranchFileRange {
      branch_name: "main".into(),
      term: 5,
      max_file_id: inherited_main_max,
      max_segment_file_id: inherited_main_max,
    },
    BranchFileRange {
      branch_name: "b-1".into(),
      term: 3,
      max_file_id: branch_max,
      max_segment_file_id: branch_max,
    },
  ]
}

fn remain_li(root: &TempRoot, tbl: &TableIdent) -> Vec<String> {
  let dir = root.join(format!("{}.{}", tbl.tbl_name, tbl.partition_id));
  let mut li: Vec<String> = read_dir(dir)
    .unwrap()
    .filter_map(|entry| entry.ok())
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .filter(|name| name.starts_with("data_") || name.starts_with("segment_"))
    .collect();
  li.sort_unstable();
  li
}

/// main file 2049 已死 + b-1 file 2049 被引用 → 一轮 GC 只删 main 的变体
/// （data 与 segment 同构）；rev-39 之前 id 级 retained 并集会把两个都留住。
#[compio::test]
async fn gc_deletes_only_dead_variant_of_referenced_file_id() {
  let root = TempRoot::new("rev39_variant_alive");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  for name in [
    "data_2049_main_5",
    "data_2049_b-1_3",
    "segment_2049_main_5",
    "segment_2049_b-1_3",
  ] {
    let len = if name.starts_with("data_") {
      DATA_LEN
    } else {
      SEGMENT_LEN
    };
    write(dir.join(name), vec![7u8; len]).unwrap();
  }
  let store = CompioStore::new(root.path().to_path_buf(), DATA_LEN, 1)
    .with_segment_config(SEGMENT_LEN, 1)
    .with_checksum_verify(false);

  // main manifest：高水位已越过 2049（文件死后继续分配），mapping 不再引用它。
  let main_metadata = BranchManifestMetadata {
    branch_name: "main".into(),
    term: 5,
    file_range_li: vec![BranchFileRange {
      branch_name: "main".into(),
      term: 5,
      max_file_id: 2052,
      max_segment_file_id: 2052,
    }],
  };
  // b-1 manifest：继承 fork 时的 main 区间（2048），自身水位 2050；
  // mapping 引用 file 2049（fp = 2049 << 1），归属链解析为 (b-1, 3)。
  let branch_metadata = BranchManifestMetadata {
    branch_name: "b-1".into(),
    term: 3,
    file_range_li: range_li(2048, 2050),
  };
  let branch_fp = 2049 << 1;
  store
    .write_manifest_file(
      &tbl,
      "manifest_main_5",
      &snapshot_bytes(&main_metadata, 2052 << 1, &[], 2052 << 1, &[]),
    )
    .await
    .unwrap();
  store
    .write_manifest_file(
      &tbl,
      "manifest_b-1_3",
      &snapshot_bytes(
        &branch_metadata,
        2050 << 1,
        &[branch_fp],
        2050 << 1,
        &[branch_fp],
      ),
    )
    .await
    .unwrap();

  let (data_set, segment_set, guard) = manifest_retained_file_page_set(&store, &tbl, 1, 1)
    .await
    .unwrap();
  // 引用归属到 (b-1, 3)，不是 id 级含糊并集。
  assert!(guard.retain_data_variant(2049, "b-1", 3, true));
  assert!(!guard.retain_data_variant(2049, "main", 5, true));

  assert_eq!(
    gc_files_with_retained(&store, &tbl, &data_set, &guard, 1)
      .await
      .unwrap(),
    1
  );
  assert_eq!(
    gc_segment_files_with_retained(&store, &tbl, &segment_set, &guard, 1)
      .await
      .unwrap(),
    1
  );
  assert_eq!(
    remain_li(&root, &tbl),
    ["data_2049_b-1_3", "segment_2049_b-1_3"]
  );
}

/// 同 id 的全部变体都无人引用 → 全部删除（guard 存在但水位之上无引用文件，
/// 不受 in-flight 兜底保护）。
#[compio::test]
async fn gc_deletes_all_variants_when_no_variant_referenced() {
  let root = TempRoot::new("rev39_variant_dead");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  for name in ["data_2049_main_5", "data_2049_b-1_3"] {
    write(dir.join(name), vec![7u8; DATA_LEN]).unwrap();
  }
  let store = CompioStore::new(root.path().to_path_buf(), DATA_LEN, 1).with_checksum_verify(false);

  // 两份 manifest 都不引用 file 2049：引用集合为空，两个变体都在
  // 各自 branch 高水位（2052 / 2050）之下，全部可回收。
  let main_metadata = BranchManifestMetadata {
    branch_name: "main".into(),
    term: 5,
    file_range_li: vec![BranchFileRange {
      branch_name: "main".into(),
      term: 5,
      max_file_id: 2052,
      max_segment_file_id: 0,
    }],
  };
  let branch_metadata = BranchManifestMetadata {
    branch_name: "b-1".into(),
    term: 3,
    file_range_li: range_li(2048, 2050),
  };
  store
    .write_manifest_file(
      &tbl,
      "manifest_main_5",
      &snapshot_bytes(&main_metadata, 2052 << 1, &[], 0, &[]),
    )
    .await
    .unwrap();
  store
    .write_manifest_file(
      &tbl,
      "manifest_b-1_3",
      &snapshot_bytes(&branch_metadata, 2050 << 1, &[], 0, &[]),
    )
    .await
    .unwrap();

  let (data_set, _, guard) = manifest_retained_file_page_set(&store, &tbl, 1, 1)
    .await
    .unwrap();
  assert!(data_set.is_empty());
  assert_eq!(
    gc_files_with_retained(&store, &tbl, &data_set, &guard, 1)
      .await
      .unwrap(),
    2
  );
  assert!(
    remain_li(&root, &tbl).is_empty(),
    "{:?}",
    remain_li(&root, &tbl)
  );
}

/// in-flight 判定的 term 维度（eng-88 语义，对齐 C++ `file_gc.cpp:1207-1208`）：
/// `term >= newest_term && file_id >= least_unflushed` 同时成立才算在飞。
/// 旧 term 文件即使 id 高于水位也不再被 guard 兜底。
#[test]
fn variant_inflight_requires_both_term_and_file_id_bound() {
  let mut guard = InflightGuard::default();
  guard.fold("main", 7, Some(10), None);
  // 当前 term + 水位之上：in-flight。
  assert!(guard.retain_data_variant(12, "main", 7, false));
  // 旧 term：即使 id 在水位之上也判可删（term 维度判定）。
  assert!(!guard.retain_data_variant(12, "main", 5, false));
  // 当前 term 但低于水位：已提交，可删。
  assert!(!guard.retain_data_variant(5, "main", 7, false));
  // guard 缺失的 branch：按可删除处理（Rust branch 生命周期保证先有 manifest）。
  assert!(!guard.retain_data_variant(12, "other", 7, false));
}

/// 端到端：旧 term 文件在水位之上、无引用 → 删；同 id 当前 term 文件 → 留。
#[compio::test]
async fn gc_reclaims_stale_term_file_above_high_water() {
  let root = TempRoot::new("rev39_variant_term");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  for name in ["data_12_main_5", "data_12_main_7"] {
    write(dir.join(name), vec![7u8; DATA_LEN]).unwrap();
  }
  let store = CompioStore::new(root.path().to_path_buf(), DATA_LEN, 1).with_checksum_verify(false);

  // term 7 的 manifest：高水位 10，不引用 file 12。
  let metadata = BranchManifestMetadata {
    branch_name: "main".into(),
    term: 7,
    file_range_li: vec![BranchFileRange {
      branch_name: "main".into(),
      term: 7,
      max_file_id: 10,
      max_segment_file_id: 0,
    }],
  };
  store
    .write_manifest_file(
      &tbl,
      "manifest_main_7",
      &snapshot_bytes(&metadata, 10 << 1, &[], 0, &[]),
    )
    .await
    .unwrap();

  let (data_set, _, guard) = manifest_retained_file_page_set(&store, &tbl, 1, 1)
    .await
    .unwrap();
  // 旧 term 的 12 号文件在水位之上也不能靠 guard 兜底。
  assert!(!guard.retain_data_variant(12, "main", 5, false));
  assert_eq!(
    gc_files_with_retained(&store, &tbl, &data_set, &guard, 1)
      .await
      .unwrap(),
    1
  );
  assert_eq!(remain_li(&root, &tbl), ["data_12_main_7"]);
}

///Mem 后端无文件名身份：变体列举回退 `list_file_id_li`（identity=None），
/// 判定保持 rev-39 之前的 id 级行为——引用即保留，无引用且无 guard 兜底即删。
#[test]
fn mem_backend_keeps_id_level_semantics() {
  pollster::block_on(async {
    use fkv_io_mem::MemStore;

    let store = MemStore::with_pages_per_file(8, 1);
    let tbl = TableIdent::new("demo", 1).unwrap();
    store.write_page(&tbl, 0, b"12345678").await.unwrap();
    store.write_page(&tbl, 2, b"12345678").await.unwrap();
    let variant_li = FileIo::list_file_variants(&store, &tbl).await.unwrap();
    assert_eq!(
      variant_li,
      vec![FileVariant::unknown(0), FileVariant::unknown(1)]
    );
    let mut retained = HashSet::default();
    retained.insert(2);
    // file 0 无引用：删除；file 1 无此文件；id 级判定不变。
    assert_eq!(
      gc_files_with_retained(&store, &tbl, &retained, &InflightGuard::default(), 1)
        .await
        .unwrap(),
      1
    );
    assert_eq!(store.list_file_id_li(&tbl).await.unwrap(), vec![1]);
  });
}

/// 更高 term 的 writer 已接管：其文件即使 id 低于本进程水位也必须保留
/// （对齐 C++ `file_gc.cpp` 的 `term > process_term` 跳过）。
#[test]
fn future_term_variant_is_always_retained() {
  let mut guard = InflightGuard::default();
  guard.set_process_term(5);
  assert!(guard.retain_data_variant(12, "main", 7, false));
  assert!(guard.retain_segment_variant(12, "main", 7, false));
  // 同 term / 低 term 仍按常规判定（无 guard 时旧 term 可删）。
  assert!(!guard.retain_data_variant(12, "main", 5, false));
}

/// seeded 的进程 term（生产入口）下，更高 term 的文件即便 id 低于本地高水位
/// 也必须保留，且一轮 GC 删除数为 0。
#[compio::test]
async fn gc_seeded_future_term_deletes_nothing() {
  let root = TempRoot::new("gc_future_term_seeded");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  write(dir.join("data_7_main_7"), vec![7u8; DATA_LEN]).unwrap();
  write(dir.join("segment_7_main_7"), vec![7u8; SEGMENT_LEN]).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), DATA_LEN, 1)
    .with_segment_config(SEGMENT_LEN, 1)
    .with_checksum_verify(false);
  let guard = {
    let mut guard = InflightGuard::default();
    guard.seed("main", 5, 10 << 1, 1);
    guard.seed_segment("main", 5, 10 << 1, 1);
    guard
  };
  assert!(guard.retain_data_variant(7, "main", 7, false));
  assert!(guard.retain_segment_variant(7, "main", 7, false));
  let mut data_set = HashSet::default();
  data_set.insert(7 << 1);
  assert_eq!(
    gc_files_with_retained(&store, &tbl, &data_set, &guard, 1)
      .await
      .unwrap(),
    0
  );
  assert_eq!(
    remain_li(&root, &tbl),
    ["data_7_main_7", "segment_7_main_7"]
  );
}
