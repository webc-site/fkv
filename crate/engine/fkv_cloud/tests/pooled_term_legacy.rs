//! pooled-14 Cloud 后端：legacy 空区间链 + term bump 后旧 term 对象仍按 `_main_0` 解析。
//!
//! 旧行为：区间链为空时，读键回退当前 identity（`_main_<new>`），而旧 term 数据在
//! `data_<fid>_main_0`；Pooled 还会把旧 file id 的空洞重新发放，读键进一步被新
//! term 的稀疏对象遮蔽。修复后空链在 bump 时补一条合成 `(main, 0)` 区间，旧 file id
//! 继续解析到旧对象。Pooled 与 Append 都受影响（读键解析与分配模式无关），故两种
//! 模式都锁定。

use std::slice;

use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArg};
use fkv_object::{MemObjectStore, ObjectStore};
use fkv_store::{AllocMode, Store};
use fkv_types::{NULL_PAGE, TableIdent};

const PAGE_SIZE: usize = 512;
/// 每文件 4 页：32 个小 key 跨两个文件，bump 边界与空洞都容易构造。
const SHIFT: u8 = 2;
const RESTART: u16 = 16;

type Cloud = Store<CloudStore<MemStore, MemObjectStore>>;

fn cloud(remote: &MemObjectStore, term: u64, mode: AllocMode) -> Cloud {
  Store::new(
    CloudStore::new(
      MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
      remote.clone(),
      "fkv",
      SHIFT,
    ),
    PAGE_SIZE,
    RESTART,
  )
  .with_pages_per_file_shift(SHIFT)
  .with_alloc_mode(mode)
  .with_branch("main", term)
  // 触发面是 skip_verify_checksum=true 的静默命中。
  .with_verify_checksum(false)
}

fn legacy_batch() -> Vec<BatchEntry> {
  (0..32u32)
    .map(|i| BatchEntry::upsert(format!("k{i:02}").into_bytes(), vec![i as u8 + 1; 64], 1))
    .collect()
}

fn new_batch() -> Vec<BatchEntry> {
  (0..8u32)
    .map(|i| BatchEntry::upsert(format!("n{i}").into_bytes(), vec![0xA0 + i as u8; 64], 2))
    .collect()
}

/// 把当前状态覆写成 pre-range-chain legacy 形状（旧口径称「C++ Local 首个
/// main manifest」）：`(main, 0)` + 空链。
fn legacy_snapshot(store: &Cloud, tbl: &TableIdent) -> Vec<u8> {
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

/// term 0 写 legacy 表 → 覆写空链 → term 1 bump + seed 写 → 冷启动同 term 1 重开
/// （旧实现下界回落 0）→ 再写 8 个 key → 新旧 key 全读。返回 `(store, bumped,
/// old_max)` 供调用方检查对象布局。
async fn legacy_roundtrip(
  remote: &MemObjectStore,
  tbl: &TableIdent,
  mode: AllocMode,
) -> (Cloud, u64, u64) {
  let entries = legacy_batch();
  let mut term0 = cloud(remote, 0, mode);
  term0.put(tbl, &entries).await.unwrap();
  let old_max = term0.mapper(tbl).unwrap().max_fp_id();
  let snapshot = legacy_snapshot(&term0, tbl);
  let io = term0.into_io();
  io.switch_manifest(tbl, &snapshot).await.unwrap();
  drop(io);

  // 第一次 reopen：manifest term 0 ≠ attach term 1 → bump。
  let mut term1 = cloud(remote, 1, mode);
  term1.reopen(tbl).await.unwrap();
  let bumped = ((old_max >> SHIFT) + 1) << SHIFT;
  assert_eq!(
    term1.mapper(tbl).unwrap().max_fp_id(),
    bumped,
    "legacy manifest 的 term bump 必须抬到下一文件边界"
  );
  term1
    .put(tbl, slice::from_ref(&new_batch()[0]))
    .await
    .unwrap();
  let seed_max = term1.mapper(tbl).unwrap().max_fp_id();
  drop(term1);

  // 冷启动（全新本地 cache）以同 term 1 重开。
  let mut again = cloud(remote, 1, mode);
  again.reopen(tbl).await.unwrap();
  assert_eq!(
    again.mapper(tbl).unwrap().max_fp_id(),
    seed_max,
    "同 term 冷启动不得再抬水位"
  );
  if mode == AllocMode::Pooled {
    let mapper = again.mapper(tbl).unwrap();
    assert!(
      mapper.reuse_floor() >= bumped,
      "同 term 冷启动后 Pooled 复用下界必须 >= term bump 边界: \
       floor={} bumped={bumped}",
      mapper.reuse_floor()
    );
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
  for entry in &entries {
    assert_eq!(
      again.get(tbl, &entry.key).await.unwrap(),
      Some(entry.value.clone()),
      "旧 term key 在冷启动后不可读: {:?}",
      String::from_utf8_lossy(&entry.key)
    );
  }
  for entry in new_batch() {
    assert_eq!(
      again.get(tbl, &entry.key).await.unwrap(),
      Some(entry.value.clone()),
      "新 term key 不可读: {:?}",
      String::from_utf8_lossy(&entry.key)
    );
  }
  (again, bumped, old_max)
}

/// 旧 file id 必须继续解析到旧 term 对象；不得出现新 term 对象。
async fn assert_remote_objects(
  remote: &MemObjectStore,
  tbl: &TableIdent,
  old_max: u64,
  bumped: u64,
) {
  let dir = format!("fkv/{tbl}");
  let bumped_file = bumped >> SHIFT;
  // 旧 term 实际写过的文件（高水位是「下一次分配」，最后一个可能写到的文件 =
  // (old_max - 1) >> shift）。
  let old_last_file = (old_max.max(1) - 1) >> SHIFT;
  for file_id in 0..bumped_file {
    let new_key = format!("{dir}/data_{file_id}_main_1");
    assert!(
      remote.get(&new_key).await.unwrap().is_none(),
      "旧 file id 下出现了新 term 对象: {new_key}"
    );
    if file_id <= old_last_file {
      let old_key = format!("{dir}/data_{file_id}_main_0");
      assert!(
        remote.get(&old_key).await.unwrap().is_some(),
        "旧 term 对象必须保留: {old_key}"
      );
    }
  }
}

#[compio::test]
async fn cloud_pooled_legacy_empty_chain_keeps_old_term_objects() {
  let tbl = TableIdent::new("legacy_cloud_pooled", 1).unwrap();
  let remote = MemObjectStore::new();
  let (_again, bumped, old_max) = legacy_roundtrip(&remote, &tbl, AllocMode::Pooled).await;
  assert_remote_objects(&remote, &tbl, old_max, bumped).await;
}

#[compio::test]
async fn cloud_append_legacy_empty_chain_keeps_old_term_objects() {
  let tbl = TableIdent::new("legacy_cloud_append", 1).unwrap();
  let remote = MemObjectStore::new();
  let (_again, bumped, old_max) = legacy_roundtrip(&remote, &tbl, AllocMode::Append).await;
  assert_remote_objects(&remote, &tbl, old_max, bumped).await;
}
