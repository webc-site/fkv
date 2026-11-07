//! fail point 注入：默认 no-op，配置后按名字触发。
use std::fs::read;

use fkv_error::KvError;
use fkv_failpoint::set_spec;
use fkv_io::{ManifestIo, PageIo};
use fkv_io_compio::CompioStore;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

#[compio::test]
async fn io_failpoints_inject_errors() {
  // `TempRoot` 析构时递归清理，无需手工 `remove_dir_all`。
  let root = TempRoot::new("failpoint_io");
  let store = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  let tbl = TableIdent::new("demo", 1).unwrap();
  let page = vec![7u8; 512];

  set_spec("WritePage@1");
  assert_eq!(
    store.write_page(&tbl, 0, &page).await.unwrap_err(),
    KvError::IoFail
  );
  set_spec("");
  store.write_page(&tbl, 0, &page).await.unwrap();

  set_spec("AppendManifest:SyncFail@1");
  assert_eq!(
    store.append_manifest(&tbl, b"record").await.unwrap_err(),
    KvError::IoFail
  );
  set_spec("");
  store.append_manifest(&tbl, b"record").await.unwrap();

  set_spec("AtomicWriteFile:Rename@1");
  assert_eq!(
    store.switch_manifest(&tbl, b"snapshot").await.unwrap_err(),
    KvError::IoFail
  );
  set_spec("");
  store.switch_manifest(&tbl, b"snapshot").await.unwrap();
}

/// 短写失败必须截掉半写 record：重试不能追加在撕裂前缀之后，
/// 否则盘面出现 valid/corrupt/valid，reopen 永久判 Corrupted。
#[compio::test]
async fn short_manifest_append_truncates_torn_record_before_retry() {
  let root = TempRoot::new("failpoint_short_append");
  let store = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  let tbl = TableIdent::new("demo", 1).unwrap();
  let path = root.path().join("demo.1").join("manifest_main_0");

  store.append_manifest(&tbl, b"record-a").await.unwrap();
  assert_eq!(read(&path).unwrap(), b"record-a");

  // 只写入 4 字节就报短写。
  store.inject_short_manifest_write(4);
  assert_eq!(
    store.append_manifest(&tbl, b"record-b").await.unwrap_err(),
    KvError::IoFail
  );
  assert_eq!(read(&path).unwrap(), b"record-a");

  // 重试从有效边界续写，盘面没有撕裂碎片。
  store.append_manifest(&tbl, b"record-b").await.unwrap();
  assert_eq!(read(&path).unwrap(), b"record-arecord-b");
}
