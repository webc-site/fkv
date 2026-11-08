//! eng-89：compact / compact_segment 必须在追加 manifest 之前 `sync_data`。
//!
//! 断言方式不是“看代码注释”，而是注入 `SyncData` 失败：
//! - 修复前：compact 不调用 `sync_data`，注入点永不命中，compact 成功、manifest 被追加；
//! - 修复后：compact 在搬迁页之后、manifest 之前调用 `sync_data`，注入命中后
//!   compact 返回 `IoFail`，manifest 字节与 mapping / root 全部保持调用前状态，
//!   搬迁页仍在 dirty 集合里等待下一次 barrier。

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io::{FileIo, ManifestIo};
use fkv_io_compio::CompioStore;
use fkv_large::SegmentLayout;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
const SHIFT: u8 = 2;

fn big_batch() -> Vec<BatchEntry> {
  (0..200u32)
    .map(|i| BatchEntry::upsert(format!("k{i:04}").into_bytes(), vec![b'v'; 30], 1))
    .collect()
}

fn pattern(len: usize, seed: u8) -> Vec<u8> {
  (0..len)
    .map(|index| (index as u8).wrapping_add(seed))
    .collect()
}

/// data / segment 两条 compact 路径都必须先 sync_data 再追加 manifest。
///
/// 两段断言放在同一个测试里：`fkv_failpoint` 是进程级注册表，注入用例与
/// 期望成功的用例并行跑会互相清空 spec。
#[compio::test]
async fn compact_syncs_data_before_manifest_append() {
  compact_data_section().await;
  compact_segment_section().await;
}

async fn compact_data_section() {
  let root = TempRoot::new("store_durability_compact");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(
    CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT),
    PAGE_SIZE,
    16,
  );
  store.put(&tbl, &big_batch()).await.unwrap();

  let before_manifest = store
    .page_io()
    .read_manifest(&tbl)
    .await
    .unwrap()
    .expect("put 之后应有 manifest");
  let before_max = store.mapper(&tbl).unwrap().max_fp_id();
  let before_root = store.root_id(&tbl);
  assert_eq!(store.page_io().dirty_file_num(&tbl), 0, "put 已 sync 干净");

  let mut file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
  file_li.sort_unstable();
  let live_map = fkv_task_gc::live_page_li_by_file(store.mapper(&tbl).unwrap(), &file_li, SHIFT);
  let target = file_li
    .iter()
    .copied()
    .find(|file_id| live_map.contains_key(file_id))
    .expect("no file with live pages");
  let sync_before = store.page_io().data_sync_num();

  fkv_failpoint::set_spec("SyncData@1!err=IoFail");
  let failed = store.compact_file(&tbl, target, SHIFT).await;
  fkv_failpoint::reset();

  assert_eq!(
    failed.unwrap_err(),
    KvError::IoFail,
    "compact 必须先走 sync_data，注入点才会命中"
  );
  assert_eq!(
    store.page_io().data_sync_num(),
    sync_before + 1,
    "compact 必须在 manifest 之前调用一次 sync_data"
  );
  assert_eq!(
    store.page_io().read_manifest(&tbl).await.unwrap().unwrap(),
    before_manifest,
    "sync_data 失败时绝不能追加 manifest"
  );
  assert_eq!(store.mapper(&tbl).unwrap().max_fp_id(), before_max);
  assert_eq!(store.root_id(&tbl), before_root);
  assert!(
    store.page_io().dirty_file_num(&tbl) > 0,
    "搬迁页应仍在 dirty 集合等待下一次 durability barrier"
  );
  assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 200);

  // 回滚后重试成功：sync 恰好一次、脏集合清空、数据完整。
  let sync_retry = store.page_io().data_sync_num();
  let moved = store.compact_file(&tbl, target, SHIFT).await.unwrap();
  assert!(moved > 0);
  assert_eq!(store.page_io().data_sync_num(), sync_retry + 1);
  assert_eq!(store.page_io().dirty_file_num(&tbl), 0);
  assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 200);
}

async fn compact_segment_section() {
  let root = TempRoot::new("store_durability_segment");
  let tbl = TableIdent::new("demo", 2).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let mut store = Store::new(
    CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT).with_segment_config(4096, 2),
    PAGE_SIZE,
    16,
  )
  .with_large_value(layout, 1024)
  .with_segment_amplify_factor(1);

  let keep = pattern(9000, 9);
  store
    .put_large(
      &tbl,
      &[BatchEntry::upsert(b"keep".to_vec(), keep.clone(), 1)],
    )
    .await
    .unwrap();
  let mut last = Vec::new();
  for round in 0..12u32 {
    last = pattern(9000 + round as usize, round as u8);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(
          b"big".to_vec(),
          last.clone(),
          u64::from(round) + 1,
        )],
      )
      .await
      .unwrap();
  }
  assert!(
    store.needs_segment_maintain(&tbl),
    "测试夹具应触发 segment 压缩"
  );

  let before_manifest = store
    .page_io()
    .read_manifest(&tbl)
    .await
    .unwrap()
    .expect("应有 manifest");
  let before_segment_max = store.segment_mapper(&tbl).unwrap().max_fp_id();
  assert_eq!(store.page_io().dirty_file_num(&tbl), 0);
  let sync_before = store.page_io().data_sync_num();

  fkv_failpoint::set_spec("SyncData@1!err=IoFail");
  let failed = store.compact_segment_if_needed(&tbl).await;
  fkv_failpoint::reset();

  assert_eq!(
    failed.unwrap_err(),
    KvError::IoFail,
    "segment compact 必须先走 sync_data"
  );
  assert_eq!(store.page_io().data_sync_num(), sync_before + 1);
  assert_eq!(
    store.page_io().read_manifest(&tbl).await.unwrap().unwrap(),
    before_manifest,
    "sync_data 失败时绝不能追加 manifest"
  );
  assert_eq!(
    store.segment_mapper(&tbl).unwrap().max_fp_id(),
    before_segment_max
  );
  assert!(
    store.page_io().dirty_file_num(&tbl) > 0,
    "搬迁段应仍在 dirty 集合"
  );

  // 回滚后重试成功，数据仍可读。
  let sync_retry = store.page_io().data_sync_num();
  let plan_li = store.compact_segment_if_needed(&tbl).await.unwrap();
  assert!(!plan_li.is_empty());
  assert_eq!(store.page_io().data_sync_num(), sync_retry + 1);
  assert_eq!(store.page_io().dirty_file_num(&tbl), 0);
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    last
  );
  assert_eq!(
    store.get_large(&tbl, b"keep").await.unwrap().unwrap().value,
    keep
  );
}
