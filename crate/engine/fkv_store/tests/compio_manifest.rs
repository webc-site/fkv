use std::fs::{metadata, read, write};

use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

#[compio::test]
async fn compio_reopen_after_snapshot_and_log() {
  let root = TempRoot::new("store_compio_manifest_reopen");
  let tbl = TableIdent::new("demo", 2).unwrap();
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
  drop(store);

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
  assert_eq!(reopened.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  reopened
    .put(&tbl, &[BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 2)])
    .await
    .unwrap();
  drop(reopened);

  let mut second = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  second.reopen(&tbl).await.unwrap();
  assert_eq!(second.get(&tbl, b"c").await.unwrap(), Some(b"C".to_vec()));
  assert_eq!(second.scan(&tbl, b"", b"", 0).await.unwrap().len(), 3);
}

#[compio::test]
async fn compio_manifest_rotation() {
  let root = TempRoot::new("store_compio_manifest_limit");
  let tbl = TableIdent::new("demo", 3).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  )
  .with_manifest_limit(1);
  for (i, key) in [b"a", b"b", b"c"].into_iter().enumerate() {
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(
          key.to_vec(),
          vec![b'v'; 8],
          i as u64 + 1,
        )],
      )
      .await
      .unwrap();
  }
  drop(store);
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
  assert_eq!(reopened.scan(&tbl, b"", b"", 0).await.unwrap().len(), 3);
}

/// sto-51 / core-20：manifest 尾部撕裂垃圾在 reopen 时截断，
/// 后续写从有效边界追加，不会落到垃圾之后导致丢失或 Corrupted。
#[compio::test]
async fn compio_reopen_truncates_corrupted_tail() {
  let root = TempRoot::new("store_compio_manifest_log");
  let tbl = TableIdent::new("demo", 7).unwrap();
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
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  // 第二次写入走增量日志 record，manifest 才有多条 record 可切分。
  store
    .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)])
    .await
    .unwrap();
  drop(store);

  let manifest = root.join("demo.7").join("manifest_main_0");
  let mut bytes = read(&manifest).unwrap();
  let valid_len = bytes.len();
  bytes.extend_from_slice(&[0xff; 64]);
  write(&manifest, &bytes).unwrap();

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
  assert_eq!(metadata(&manifest).unwrap().len() as usize, valid_len);
  reopened
    .put(&tbl, &[BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 3)])
    .await
    .unwrap();
  drop(reopened);

  let mut third = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  third.reopen(&tbl).await.unwrap();
  assert_eq!(third.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(third.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
  assert_eq!(third.get(&tbl, b"c").await.unwrap(), Some(b"C".to_vec()));
}

/// manifest 写失败后 store 必须回到一致状态。
///
/// 失败批次整体回滚（脏页在磁盘写成功后才清除，回滚不能丢此前已提交的映射）；
/// 后续写入与 reopen 不受影响。
#[compio::test]
async fn failed_manifest_append_keeps_store_consistent() {
  let root = TempRoot::new("store_compio_manifest_failpoint");
  let tbl = TableIdent::new("demo", 9).unwrap();
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
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  // 强制第二次写入走增量日志（首次写入写 snapshot）。
  store
    .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)])
    .await
    .unwrap();

  fkv_failpoint::set_spec("AppendManifest:Write@1!err=IoFail");
  let failed = store
    .put(&tbl, &[BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 3)])
    .await;
  assert!(failed.is_err());
  fkv_failpoint::reset();

  // 后续写入会把未提交的脏页一起补上。
  store
    .put(&tbl, &[BatchEntry::upsert(b"d".to_vec(), b"D".to_vec(), 4)])
    .await
    .unwrap();
  drop(store);

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
  assert_eq!(reopened.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(reopened.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
  // 失败批次整体回滚：c 不应出现，但 store 必须保持可 reopen 且 a/b/d 完好。
  assert_eq!(reopened.get(&tbl, b"c").await.unwrap(), None);
  assert_eq!(reopened.get(&tbl, b"d").await.unwrap(), Some(b"D".to_vec()));
}

/// manifest fsync 失败必须回退文件偏移并截断本次 record；否则下一次 append
/// 会写在未提交记录之后，reopen 把已回滚的批次 replay 回来，覆盖已确认数据。
#[compio::test]
async fn failed_manifest_sync_rolls_back_record_and_offset() {
  let root = TempRoot::new("store_compio_manifest_sync_rollback");
  let tbl = TableIdent::new("demo", 10).unwrap();
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
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  // 第二条走增量日志；注入 fsync 失败。
  fkv_failpoint::set_spec("AppendManifest:SyncFail@1!err=IoFail");
  let failed = store
    .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)])
    .await;
  assert!(failed.is_err());
  fkv_failpoint::reset();

  // 失败批次回滚后继续写：c 必须落盘，b 不得复活。
  store
    .put(&tbl, &[BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 3)])
    .await
    .unwrap();
  drop(store);

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
  assert_eq!(reopened.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(reopened.get(&tbl, b"b").await.unwrap(), None);
  assert_eq!(reopened.get(&tbl, b"c").await.unwrap(), Some(b"C".to_vec()));
}
