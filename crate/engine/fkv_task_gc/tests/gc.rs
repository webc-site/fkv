use std::{
  cell::Cell,
  fs::{create_dir_all, write},
  rc::Rc,
};

use compio::runtime::spawn;
use fkv_io::{FileIo, ManifestIo, PageIo};
use fkv_io_mem::MemStore;
use fkv_mapping::{AllocMode, PageMapper};
use fkv_task_gc::{
  InflightGuard, add_mapping_retained, gc_files_with_retained, gc_files_with_retained_budget,
  gc_segment_files_with_retained, live_page_li_by_file, manifest_retained_file_page_set,
};
use fkv_testkit::TempRoot;
use fkv_types::{FileId, FilePageId, TableIdent};
use gxhash::HashSet;

/// 单个损坏 manifest 只记入 damaged 名单，整轮不返回 Err；引用集合不完整时
/// GC 保守保留全部文件，绝不误删。
#[test]
fn corrupt_manifest_is_isolated_and_blocks_deletion() {
  pollster::block_on(async {
    let store = MemStore::with_pages_per_file(8, 1);
    let tbl = TableIdent::new("demo", 1).unwrap();
    store.write_page(&tbl, 0, b"12345678").await.unwrap();
    store
      .write_manifest_file(&tbl, "manifest_main_0", b"not a manifest")
      .await
      .unwrap();

    let (_, _, guard) = manifest_retained_file_page_set(&store, &tbl, 1, 1)
      .await
      .unwrap();
    assert!(guard.is_damaged());
    assert_eq!(guard.damaged_li(), [Box::<str>::from("manifest_main_0")]);

    let retained = HashSet::default();
    assert_eq!(
      gc_files_with_retained(&store, &tbl, &retained, &guard, 1)
        .await
        .unwrap(),
      0
    );
    assert_eq!(store.list_file_id_li(&tbl).await.unwrap(), vec![0]);
  });
}

#[test]
fn gc_removes_empty_files() {
  pollster::block_on(async {
    let store = MemStore::with_pages_per_file(8, 1);
    let tbl = TableIdent::new("demo", 1).unwrap();
    store.write_page(&tbl, 0, b"12345678").await.unwrap();
    store.write_page(&tbl, 2, b"12345678").await.unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let p0 = mapper.allocate_page();
    let p1 = mapper.allocate_page();
    mapper.set_file_page(p0, 0);
    mapper.set_file_page(p1, 2);
    let mut live_map = live_page_li_by_file(&mapper, &[0], 1);
    assert_eq!(live_map.remove(&0), Some(vec![p0]));
    assert!(mapper.free_page(p0).unwrap().is_some());
    let mut retained = HashSet::default();
    add_mapping_retained(mapper.tbl(), &mut retained);
    let removed = gc_files_with_retained(&store, &tbl, &retained, &InflightGuard::default(), 1)
      .await
      .unwrap();
    assert_eq!(removed, 1);
    let file_li = store.list_file_id_li(&tbl).await.unwrap();
    assert_eq!(file_li, vec![1]);
  });
}

/// io-04：GC 按 file id 回收时必须清掉同 id 的全部 (branch, term) 变体，
/// 不能只删当前 identity 的一个文件而把旧 term 垃圾留在盘上。
#[compio::test]
async fn gc_reclaims_all_identity_variants_of_unretained_file() {
  use std::fs::{read_dir, write};

  use fkv_io_compio::CompioStore;

  let root = TempRoot::new("gc_variant");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  for name in [
    "data_0_main_0",
    "data_0_main_1",
    "data_0_feature-1_2",
    "segment_0_main_0",
    "segment_0_main_1",
    "segment_0_feature-1_2",
  ] {
    let len = if name.starts_with("data_") { 512 } else { 4096 };
    write(dir.join(name), vec![7u8; len]).unwrap();
  }
  let store = CompioStore::new(root.path().to_path_buf(), 512, 1)
    .with_segment_config(4096, 1)
    .with_checksum_verify(false);
  store.set_file_identity("main", 1);

  let retained = HashSet::<FilePageId>::default();
  // rev-39 起按变体计数：同 id 的 3 个 (branch, term) 变体各删一次，
  // 每个变体独立判删后目录整体清空（io-04 契约不变）。
  assert_eq!(
    gc_files_with_retained(&store, &tbl, &retained, &InflightGuard::default(), 1)
      .await
      .unwrap(),
    3
  );
  assert_eq!(
    gc_segment_files_with_retained(&store, &tbl, &retained, &InflightGuard::default(), 1)
      .await
      .unwrap(),
    3
  );

  let remain_li: Vec<_> = read_dir(&dir)
    .unwrap()
    .filter_map(|entry| entry.ok())
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .collect();
  assert!(remain_li.is_empty(), "GC 残留文件: {remain_li:?}");
}

/// `MemStore` 文件操作全部立即 Ready：长 GC 文件循环若不带让出点会在一次
/// poll 内跑完。用标记任务是否先被 poll 验证 `yield_budget_us` 生效。
async fn gc_yields(yield_budget_us: u64) -> bool {
  // pages_per_file_shift = 1：每 2 个逻辑页一个文件，2000 页 → 1000 个文件。
  const PAGE_NUM: u64 = 2000;
  let store = MemStore::with_pages_per_file(8, 1);
  let tbl = TableIdent::new("yield_gc", 1).unwrap();
  for file_page_id in 0..PAGE_NUM {
    store
      .write_page(&tbl, file_page_id, b"12345678")
      .await
      .unwrap();
  }
  let retained = HashSet::<FilePageId>::default();
  let done = Rc::new(Cell::new(false));
  let yielded = Rc::new(Cell::new(false));
  let gc_done = Rc::clone(&done);
  let gc_tbl = tbl.clone();
  let gc = spawn(async move {
    let result = gc_files_with_retained_budget(
      &store,
      &gc_tbl,
      &retained,
      &InflightGuard::default(),
      1,
      yield_budget_us,
    )
    .await;
    gc_done.set(true);
    result
  });
  let marker_done = Rc::clone(&done);
  let marker_yielded = Rc::clone(&yielded);
  let marker = spawn(async move {
    marker_yielded.set(!marker_done.get());
  });

  let removed = gc.await.unwrap().unwrap();
  marker.await.unwrap();
  assert_eq!(removed, PAGE_NUM as usize / 2);
  yielded.get()
}

#[compio::test]
async fn gc_file_loop_yields_to_other_task() {
  assert!(gc_yields(1).await, "GC 文件循环未触发协作让出");
}

#[compio::test]
async fn gc_zero_budget_disables_yield_points() {
  assert!(!gc_yields(0).await, "budget=0 时 GC 不应额外让出");
}

/// 构造一份 manifest record：`ref_file_li` 的每个 file id 引用一个数据页。
fn manifest_record(
  branch: &str,
  range_li: Vec<fkv_types::BranchFileRange>,
  max_file_id: FileId,
  ref_file_li: &[FileId],
  shift: u8,
) -> Vec<u8> {
  use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArg};
  use fkv_types::{NULL_PAGE, encode_file_page_id};

  let metadata = BranchManifestMetadata {
    branch_name: branch.into(),
    term: 0,
    file_range_li: range_li,
  };
  let value_li: Vec<u64> = ref_file_li
    .iter()
    .map(|file_id| encode_file_page_id((file_id << shift) | 1))
    .collect();
  ManifestBuilder::new()
    .snapshot(
      SnapshotArg {
        root_id: NULL_PAGE,
        ttl_root: NULL_PAGE,
        max_fp_id: (max_file_id + 1) << shift,
        dict_bytes: &[],
        branch_metadata: &metadata,
        max_segment_fp_id: 0,
      },
      &value_li,
      None,
    )
    .unwrap()
}

/// 跨 branch 同 file id 的变体级回收（rev-39）。
///
/// 物理盘面：`main/0` 的 4、50 被 main manifest 引用，`feature-1/0` 的 42
/// 被 branch manifest 引用；`main/0/42` 与 `feature-1/0/50` 无任何 manifest
/// 引用。一轮 GC 只删这两个死变体，同 id 的活变体与其它 id 全部保留。
#[compio::test]
async fn gc_reclaims_only_dead_variant_across_branch() {
  use std::fs::{read_dir, write};

  use fkv_io_compio::CompioStore;
  use fkv_types::BranchFileRange;

  let root = TempRoot::new("gc_branch_variant");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  for name in [
    "data_4_main_0",
    "data_42_main_0",
    "data_50_main_0",
    "data_42_feature-1_0",
    "data_50_feature-1_0",
  ] {
    write(dir.join(name), vec![7u8; 512]).unwrap();
  }
  let store = CompioStore::new(root.path().to_path_buf(), 512, 1).with_checksum_verify(false);
  store.set_file_identity("main", 0);
  let main_record = manifest_record(
    "main",
    vec![BranchFileRange {
      branch_name: "main".into(),
      term: 0,
      max_file_id: 60,
      max_segment_file_id: 0,
    }],
    60,
    &[4, 50],
    1,
  );
  store.switch_manifest(&tbl, &main_record).await.unwrap();
  let branch_record = manifest_record(
    "feature-1",
    vec![
      BranchFileRange {
        branch_name: "main".into(),
        term: 0,
        max_file_id: 10,
        max_segment_file_id: 0,
      },
      BranchFileRange {
        branch_name: "feature-1".into(),
        term: 0,
        max_file_id: 55,
        max_segment_file_id: 0,
      },
    ],
    55,
    &[42],
    1,
  );
  store
    .write_manifest_file(&tbl, "manifest_feature-1_0", &branch_record)
    .await
    .unwrap();

  let (data_set, _, guard) = manifest_retained_file_page_set(&store, &tbl, 1, 1)
    .await
    .unwrap();
  assert_eq!(
    gc_files_with_retained(&store, &tbl, &data_set, &guard, 1)
      .await
      .unwrap(),
    2,
    "只应回收 main/0/42 与 feature-1/0/50 两个死变体"
  );

  let mut remain_li: Vec<(FileId, String, u64)> = store
    .list_file_variants(&tbl)
    .await
    .unwrap()
    .into_iter()
    .map(|variant| {
      let (branch, term) = variant.identity.unwrap();
      (variant.file_id, branch.to_string(), term)
    })
    .collect();
  remain_li.sort();
  assert_eq!(
    remain_li,
    vec![
      (4, "main".to_owned(), 0),
      (42, "feature-1".to_owned(), 0),
      (50, "main".to_owned(), 0),
    ],
    "死变体应删除，同 id 的活变体必须保留"
  );
  let name_li: Vec<String> = read_dir(&dir)
    .unwrap()
    .filter_map(|entry| entry.ok())
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .filter(|name| name.starts_with("data_"))
    .collect();
  assert!(
    name_li
      .iter()
      .all(|name| !name.contains("_main_0") || name == "data_4_main_0" || name == "data_50_main_0"),
    "main/0/42 应已从盘面删除: {name_li:?}"
  );
}

/// 外部注册（活跃快照 / 内存 mapping）只带 file id、不带 `(branch, term)`：
/// 同 id 的变体身份不可见时一律整体保留（rev-39 pin 语义），不能按其它
/// manifest 的归属删掉读者可能正在读的变体。
#[compio::test]
async fn pinned_file_id_keeps_all_variants() {
  use fkv_io_compio::CompioStore;
  use fkv_types::BranchFileRange;

  let root = TempRoot::new("gc_branch_variant_pin");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  for name in ["data_42_main_0", "data_42_feature-1_0"] {
    write(dir.join(name), vec![7u8; 512]).unwrap();
  }
  let store = CompioStore::new(root.path().to_path_buf(), 512, 1).with_checksum_verify(false);
  store.set_file_identity("main", 0);
  let main_record = manifest_record(
    "main",
    vec![BranchFileRange {
      branch_name: "main".into(),
      term: 0,
      max_file_id: 60,
      max_segment_file_id: 0,
    }],
    60,
    &[4],
    1,
  );
  store.switch_manifest(&tbl, &main_record).await.unwrap();
  let branch_record = manifest_record(
    "feature-1",
    vec![
      BranchFileRange {
        branch_name: "main".into(),
        term: 0,
        max_file_id: 10,
        max_segment_file_id: 0,
      },
      BranchFileRange {
        branch_name: "feature-1".into(),
        term: 0,
        max_file_id: 55,
        max_segment_file_id: 0,
      },
    ],
    55,
    &[42],
    1,
  );
  store
    .write_manifest_file(&tbl, "manifest_feature-1_0", &branch_record)
    .await
    .unwrap();

  let (data_set, _, mut guard) = manifest_retained_file_page_set(&store, &tbl, 1, 1)
    .await
    .unwrap();
  // 模拟活跃快照独占 file 42 的页（file page 85 落在 file 42）。
  let mut pinned = HashSet::default();
  pinned.insert(85u64);
  guard.pin_data_page_set(&pinned, 1);
  assert_eq!(
    gc_files_with_retained(&store, &tbl, &data_set, &guard, 1)
      .await
      .unwrap(),
    0,
    "pin 的 file id 不得做变体级删除"
  );
  let variant_num = store.list_file_variants(&tbl).await.unwrap().len();
  assert_eq!(variant_num, 2, "两个变体都必须保留");
}
