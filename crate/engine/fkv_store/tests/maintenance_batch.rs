//! core-25：维护写路径（compact / truncate / clean_expired）的提交时机。
//!
//! manifest 写失败时 mapping / 分配器 / root 必须与调用前完全一致，
//! 后续读与重试都从干净状态开始。

use fkv_btree::BatchEntry;
use fkv_io::FileIo;
use fkv_io_compio::CompioStore;
use fkv_large::SegmentLayout;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn big_batch() -> Vec<BatchEntry> {
  (0..200u32)
    .map(|i| BatchEntry::upsert(format!("k{i:04}").into_bytes(), vec![b'v'; 30], 1))
    .collect()
}

/// compact：manifest 失败后物理搬迁必须回滚，逻辑 root / max_fp_id 不变。
#[compio::test]
async fn compact_manifest_failure_rolls_back_mapping() {
  const SHIFT: u8 = 2;
  let root = TempRoot::new("store_maint_compact");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(
    CompioStore::new(root.path().to_path_buf(), 512, SHIFT),
    512,
    16,
  );
  store.put(&tbl, &big_batch()).await.unwrap();

  let mut file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
  file_li.sort_unstable();
  let live_map = fkv_task_gc::live_page_li_by_file(store.mapper(&tbl).unwrap(), &file_li, SHIFT);
  let target = file_li
    .iter()
    .copied()
    .find(|file_id| live_map.contains_key(file_id))
    .expect("no file with live pages");
  let before_max = store.mapper(&tbl).unwrap().max_fp_id();
  let before_root = store.root_id(&tbl);

  fkv_failpoint::set_spec("AppendManifest:Write@1!err=IoFail");
  let failed = store.compact_file(&tbl, target, SHIFT).await;
  fkv_failpoint::reset();

  assert!(failed.is_err());
  assert_eq!(store.mapper(&tbl).unwrap().max_fp_id(), before_max);
  assert_eq!(store.root_id(&tbl), before_root);
  assert!(store.mapper(&tbl).unwrap().dirty_page_li().is_empty());
  assert!(store.mapper(&tbl).unwrap().changed_page_li().is_empty());
  assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 200);

  // 回滚后重试成功。
  let moved = store.compact_file(&tbl, target, SHIFT).await.unwrap();
  assert!(moved > 0);
  assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 200);
}

/// truncate：manifest 失败后释放的页必须全部恢复，root 仍旧。
#[compio::test]
async fn truncate_manifest_failure_restores_mapping() {
  let root = TempRoot::new("store_maint_truncate");
  let tbl = TableIdent::new("demo", 2).unwrap();
  let mut store = Store::new(CompioStore::new(root.path().to_path_buf(), 512, 2), 512, 16);
  store
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  let before_max = store.mapper(&tbl).unwrap().max_fp_id();
  let before_root = store.root_id(&tbl);

  fkv_failpoint::set_spec("AtomicWriteFile:Sync@1!err=IoFail");
  let failed = store.truncate(&tbl).await;
  fkv_failpoint::reset();

  assert!(failed.is_err());
  assert_eq!(store.mapper(&tbl).unwrap().max_fp_id(), before_max);
  assert_eq!(store.root_id(&tbl), before_root);
  assert!(store.mapper(&tbl).unwrap().dirty_page_li().is_empty());
  assert!(store.mapper(&tbl).unwrap().changed_page_li().is_empty());
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

  store.truncate(&tbl).await.unwrap();
  assert!(store.scan(&tbl, b"", b"", 0).await.unwrap().is_empty());
}

/// clean_expired：manifest 失败后 data / TTL 两棵树的 root 与 mapping 都保持原样。
#[compio::test]
async fn clean_expired_manifest_failure_restores_roots() {
  let root = TempRoot::new("store_maint_clean");
  let tbl = TableIdent::new("demo", 3).unwrap();
  let mut store = Store::new(CompioStore::new(root.path().to_path_buf(), 512, 2), 512, 16);
  let mut expired = BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1);
  expired.expire_ts = 500;
  let mut alive = BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1);
  alive.expire_ts = 900;
  // now=0 写入，避免历史时间戳被写时过滤。
  store.put_at(&tbl, &[expired, alive], 0).await.unwrap();

  let before_max = store.mapper(&tbl).unwrap().max_fp_id();
  let before_root = store.root_id(&tbl);
  let before_ttl_root = store.ttl_root_id(&tbl);

  fkv_failpoint::set_spec("AppendManifest:Write@1!err=IoFail");
  let failed = store.clean_expired(&tbl, 600).await;
  fkv_failpoint::reset();

  assert!(failed.is_err());
  assert_eq!(store.mapper(&tbl).unwrap().max_fp_id(), before_max);
  assert_eq!(store.root_id(&tbl), before_root);
  assert_eq!(store.ttl_root_id(&tbl), before_ttl_root);
  assert!(store.mapper(&tbl).unwrap().dirty_page_li().is_empty());
  // now=400 时 a 仍未过期，说明旧 root 仍可读。
  assert_eq!(
    store.get_at(&tbl, b"a", 400).await.unwrap(),
    Some(b"A".to_vec())
  );

  // 回滚后重试成功：a 过期被清，b 保留。
  let stats = store.clean_expired(&tbl, 600).await.unwrap();
  assert_eq!(stats.ttl_deleted, 1);
  assert_eq!(store.get_at(&tbl, b"a", 400).await.unwrap(), None);
  assert_eq!(
    store.get_at(&tbl, b"b", 600).await.unwrap(),
    Some(b"B".to_vec())
  );
}

/// rev-01：`put_at` 的批次必须同时覆盖 segment mapper。
///
/// 小值覆盖大 value 时 `write_batch` 已释放旧段；manifest 失败后 `abort_batch`
/// 必须把段映射恢复到调用前，否则 live key 指向已回收的段。
#[compio::test]
async fn put_over_large_value_manifest_failure_restores_segments() {
  let root = TempRoot::new("store_maint_rev01");
  let tbl = TableIdent::new("demo", 4).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 2),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  // 首次写建 snapshot manifest，后续写走 manifest log。
  store
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  let big = vec![7u8; 9000];
  store
    .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 2)])
    .await
    .unwrap();
  let before_root = store.root_id(&tbl);
  let before_segment = segment_state(&store, &tbl);
  assert_eq!(before_segment.0, 3);

  fkv_failpoint::set_spec("AppendManifest:Write@1!err=IoFail");
  let failed = store
    .put(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), b"tiny".to_vec(), 3)],
    )
    .await;
  fkv_failpoint::reset();

  assert!(failed.is_err());
  assert_eq!(store.root_id(&tbl), before_root);
  assert_eq!(
    segment_state(&store, &tbl),
    before_segment,
    "abort 必须恢复释放掉的段映射"
  );
  let segment_mapper = store.segment_mapper(&tbl).unwrap();
  assert!(segment_mapper.dirty_page_li().is_empty());
  assert!(segment_mapper.changed_page_li().is_empty());
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big
  );

  // 回滚后重试成功：旧段被真正释放，小值可见。
  store
    .put(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), b"tiny".to_vec(), 3)],
    )
    .await
    .unwrap();
  assert_eq!(segment_state(&store, &tbl).0, 0);
  assert_eq!(
    store.get(&tbl, b"big").await.unwrap(),
    Some(b"tiny".to_vec())
  );
}

/// `(活跃段数, 逻辑段槽位数, max_fp_id, 空闲段数)`。
fn segment_state(store: &Store<CompioStore>, tbl: &TableIdent) -> (usize, usize, u64, u32) {
  let mapper = store.segment_mapper(tbl).unwrap();
  let live = mapper
    .tbl()
    .iter_value_li()
    .filter(|value| fkv_types::is_file_page_id(*value))
    .count();
  (
    live,
    mapper.tbl().len(),
    mapper.max_fp_id(),
    mapper.free_page_cnt(),
  )
}

/// R4-01：`put_large` 的外层段批次不能被内层 TTL 清理的 abort 一起回滚。
///
/// TTL 清理失败只该回滚自己的批次；若它 abort 掉外层段 journal，会出现
/// manifest 已持久、段映射已回滚的撕裂状态，写入返回成功而读取报 ResourceMissing。
#[compio::test]
async fn put_large_ttl_cleanup_failure_keeps_committed_segments() {
  let tbl = TableIdent::new("demo", 7).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let root = TempRoot::new("store_maint_put_large_ttl");
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 2),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  // 写一个已过期 TTL 项：下一次写（now 远大于 10）会触发内联清理。
  let mut expired = BatchEntry::upsert(b"gone".to_vec(), b"G".to_vec(), 1);
  expired.expire_ts = 10;
  store.put_at(&tbl, &[expired], 0).await.unwrap();

  let big = vec![9u8; 9000];
  // 第 1 次 SyncData 属于 put，第 2 次属于内联 TTL 清理：让清理失败。
  fkv_failpoint::set_spec("SyncData@2!err=IoFail");
  let result = store
    .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 2)])
    .await;
  fkv_failpoint::reset();

  assert!(result.is_ok(), "TTL 清理失败不影响写入结果");
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big,
    "段映射被内层 abort 回滚了"
  );
  let mapper = store.segment_mapper(&tbl).unwrap();
  assert!(mapper.dirty_page_li().is_empty());
  assert!(mapper.changed_page_li().is_empty());
}
