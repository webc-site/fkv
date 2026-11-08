use std::fs::{create_dir_all, write};

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io::{FileIo, ManifestIo};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_store::{CleanExpiredStat, Store};
use fkv_testkit::TempRoot;
use fkv_types::{PageId, TableIdent};

#[test]
fn truncate_clears_data_ttl_and_manifest() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let mut ttl_entry = BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 1);
    ttl_entry.expire_ts = 100;
    store
      .put_at(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
          ttl_entry,
        ],
        0,
      )
      .await
      .unwrap();
    assert_eq!(store.scan_at(&tbl, b"", b"", 0, 0).await.unwrap().len(), 3);
    let file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
    assert!(!file_li.is_empty());

    store.truncate(&tbl).await.unwrap();
    assert_eq!(store.root_id(&tbl), Some(PageId::MAX));
    assert_eq!(store.ttl_root_id(&tbl), Some(PageId::MAX));
    assert_eq!(store.get_at(&tbl, b"a", 0).await.unwrap(), None);
    assert!(
      store
        .scan_at(&tbl, b"", b"", 0, 0)
        .await
        .unwrap()
        .is_empty()
    );
    assert_eq!(
      store.clean_expired(&tbl, u64::MAX).await.unwrap(),
      CleanExpiredStat::default()
    );
    // truncate 只重置映射 / manifest，data 文件留给 gc。
    assert!(
      !store
        .page_io()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty()
    );

    let io = store.into_io();
    let mut reopened = Store::new(io, 512, 16);
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(reopened.root_id(&tbl), Some(PageId::MAX));
    assert!(reopened.scan(&tbl, b"", b"", 0).await.unwrap().is_empty());

    let removed = reopened
      .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
      .await
      .unwrap();
    assert!(removed >= 1);
    assert!(
      reopened
        .page_io()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty()
    );

    reopened
      .put(
        &tbl,
        &[BatchEntry::upsert(b"new".to_vec(), b"N".to_vec(), 2)],
      )
      .await
      .unwrap();
    assert_eq!(
      reopened.get(&tbl, b"new").await.unwrap(),
      Some(b"N".to_vec())
    );
    assert_eq!(reopened.get(&tbl, b"a").await.unwrap(), None);
    assert_eq!(reopened.scan(&tbl, b"", b"", 0).await.unwrap().len(), 1);
  });
}

#[test]
fn drop_table_removes_manifest_files_and_state() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let mut ttl_entry = BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 1);
    ttl_entry.expire_ts = 100;
    store
      .put_at(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          ttl_entry,
        ],
        0,
      )
      .await
      .unwrap();
    assert!(store.root_id(&tbl).is_some());

    store.drop_table(&tbl).await.unwrap();
    assert_eq!(store.root_id(&tbl), None);
    assert_eq!(store.ttl_root_id(&tbl), None);
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), None);
    // Drop 只做逻辑删除；物理文件由 FileGc 回收。
    store
      .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
      .await
      .unwrap();
    assert!(
      store
        .page_io()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty()
    );
    store.drop_table(&tbl).await.unwrap();

    let io = store.into_io();
    assert!(
      ManifestIo::read_manifest(&io, &tbl)
        .await
        .unwrap()
        .is_none()
    );
    let mut store = Store::new(io, 512, 16);
    store
      .put(&tbl, &[BatchEntry::upsert(b"x".to_vec(), b"X".to_vec(), 3)])
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, b"x").await.unwrap(), Some(b"X".to_vec()));
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), None);
  });
}

#[compio::test]
async fn compio_truncate_and_drop() {
  let root = TempRoot::new("store_truncate_compio");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store
    .put(
      &tbl,
      &[
        BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
        BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
      ],
    )
    .await
    .unwrap();
  let data_file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
  assert!(!data_file_li.is_empty());
  let manifest_path = root.join("demo.1").join("manifest_main_0");
  assert!(manifest_path.exists());

  store.truncate(&tbl).await.unwrap();
  assert!(manifest_path.exists());
  assert_eq!(store.root_id(&tbl), Some(PageId::MAX));
  drop(store);

  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store.reopen(&tbl).await.unwrap();
  assert!(store.scan(&tbl, b"", b"", 0).await.unwrap().is_empty());
  assert!(
    !store
      .page_io()
      .list_file_id_li(&tbl)
      .await
      .unwrap()
      .is_empty()
  );
  let removed = store
    .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .await
    .unwrap();
  assert!(removed >= 1);
  assert!(
    store
      .page_io()
      .list_file_id_li(&tbl)
      .await
      .unwrap()
      .is_empty()
  );

  store
    .put(
      &tbl,
      &[BatchEntry::upsert(b"new".to_vec(), b"N".to_vec(), 2)],
    )
    .await
    .unwrap();
  assert_eq!(store.get(&tbl, b"new").await.unwrap(), Some(b"N".to_vec()));

  store.drop_table(&tbl).await.unwrap();
  assert!(!manifest_path.exists());
  store
    .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .await
    .unwrap();
  assert!(
    store
      .page_io()
      .list_file_id_li(&tbl)
      .await
      .unwrap()
      .is_empty()
  );
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  assert_eq!(
    store.reopen(&tbl).await.unwrap_err(),
    KvError::ResourceMissing
  );
  store
    .put(&tbl, &[BatchEntry::upsert(b"z".to_vec(), b"Z".to_vec(), 3)])
    .await
    .unwrap();
  assert_eq!(store.get(&tbl, b"z").await.unwrap(), Some(b"Z".to_vec()));
}

/// sem-59：truncate + maintain 后分区目录与活跃 manifest 的收尾形态对齐 C++。
/// C++ GC 收尾（`ExecuteLocalGC` 第 5 步 → `TryCleanupLocalPartitionDir`）是
/// 一次 `rmdir(2)`：manifest 文件挡住 rmdir 时返回 `-ENOTEMPTY`，调用方按成功
/// 吞掉 —— 目录与 manifest 永久残留，`manifest_size_ > 0`，其后 no-op 批整写
/// 空快照（sem-56）、真实写批走 append。删 manifest 的唯一生产调用点是显式
/// Drop（`drop_table`，另见 `drop_table_removes_manifest_files_and_state`）。
#[compio::test]
async fn compio_truncate_then_maintain_keeps_manifest_and_dir() {
  let root = TempRoot::new("store_truncate_empty_cleanup");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store
    .put(
      &tbl,
      &[
        BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
        BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
      ],
    )
    .await
    .unwrap();
  let part_dir = root.join("demo.1");
  let manifest_path = part_dir.join("manifest_main_0");
  assert!(manifest_path.exists());

  store.truncate(&tbl).await.unwrap();
  // truncate 只写空 snapshot，data 文件仍在，目录必须保留。
  assert!(part_dir.exists());
  assert!(manifest_path.exists());

  store.maintain(&tbl).await.unwrap();
  // GC 删光 data 文件，但活跃 manifest 保留：目录因 manifest 存在而原样保留
  // （对齐 C++ `-ENOTEMPTY` 分支）。
  assert_eq!(store.root_id(&tbl), Some(PageId::MAX));
  assert!(
    store
      .page_io()
      .list_file_id_li(&tbl)
      .await
      .unwrap()
      .is_empty(),
    "GC 必须删光 data 文件（目录残留只因 manifest）"
  );
  assert!(manifest_path.exists(), "维护收尾不得删除活跃 manifest");
  assert!(part_dir.exists(), "manifest 挡住 rmdir，目录原样保留");
  // 收尾幂等：再次维护不应报错，也不误删。
  store.maintain(&tbl).await.unwrap();
  assert!(manifest_path.exists());
  assert!(part_dir.exists());

  // 维护后的 no-op 批：空快照整写（sem-56 不被 cleanup 旁路），不追加 delta。
  let appends = store.page_io().manifest_append_num();
  store.put(&tbl, &[]).await.unwrap();
  assert_eq!(
    store.page_io().manifest_append_num(),
    appends,
    "空批整写空快照，不追加 delta"
  );

  // 同一 Store 继续写：manifest 未删，走 append 续写，目录仍保留。
  store
    .put(
      &tbl,
      &[BatchEntry::upsert(b"new".to_vec(), b"N".to_vec(), 2)],
    )
    .await
    .unwrap();
  assert_eq!(store.get(&tbl, b"new").await.unwrap(), Some(b"N".to_vec()));
  assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 1);
  assert!(manifest_path.exists());
  assert!(part_dir.exists());
  drop(store);

  // reopen 回放保留的 manifest（空快照 + delta）：new 恢复、旧 key 不可见。
  let mut reopened = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  reopened.reopen(&tbl).await.unwrap();
  assert_eq!(
    reopened.get(&tbl, b"new").await.unwrap(),
    Some(b"N".to_vec())
  );
  assert_eq!(reopened.get(&tbl, b"a").await.unwrap(), None);
  assert!(
    part_dir.exists(),
    "reopen 后目录与 manifest 仍残留（对齐 C++）"
  );
}

#[compio::test]
async fn maintain_keeps_partition_dir_for_branch_with_parent_data() {
  let root = TempRoot::new("store_truncate_branch_keep");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut main = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  main
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  main.create_branch(&tbl, "feature").await.unwrap();

  let mut branch = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  branch.open_branch(&tbl, "feature", 0).await.unwrap();
  branch
    .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)])
    .await
    .unwrap();
  branch.truncate(&tbl).await.unwrap();
  // main 仍有数据文件，branch 收尾必须跳过，不能删 branch manifest 或目录。
  branch.maintain(&tbl).await.unwrap();
  let part_dir = root.join("demo.1");
  assert!(part_dir.join("manifest_feature_0").exists());
  assert!(part_dir.join("manifest_main_0").exists());
  assert!(part_dir.exists());
  assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
}

#[compio::test]
async fn maintain_keeps_partition_dir_when_table_not_empty() {
  let root = TempRoot::new("store_truncate_keep");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  let part_dir = root.join("demo.1");
  let manifest_path = part_dir.join("manifest_main_0");

  store
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  store.maintain(&tbl).await.unwrap();
  assert!(manifest_path.exists());
  assert!(part_dir.exists());
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

  // truncate 后又写入新数据：目录非空，收尾必须跳过。
  store.truncate(&tbl).await.unwrap();
  store
    .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)])
    .await
    .unwrap();
  store.maintain(&tbl).await.unwrap();
  assert!(manifest_path.exists());
  assert!(part_dir.exists());
  assert_eq!(store.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
}

/// Drop 不 replay manifest：损坏的活跃 manifest 也必须能删掉（对齐 C++
/// `BatchWriteTask::Drop` 第一步 `DropManifest`）。
#[compio::test]
async fn drop_works_on_corrupted_manifest() {
  let root = TempRoot::new("store_drop_corrupt_manifest");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  let manifest = dir.join("manifest_main_0");
  write(&manifest, b"corrupted").unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  // 正常 reopen 必须报 Corrupted（证明语料确实损坏）。
  assert_eq!(store.reopen(&tbl).await.unwrap_err(), KvError::Corrupted);
  store.drop_table(&tbl).await.unwrap();
  assert!(!manifest.exists());
}
