//! io-31：空分区目录移除（rmdir）的错误语义——目录非空 / 已不在按「未移除」
//! 吞掉；其余 rmdir 错误仍回 `Err`，由 GC 收尾按 warning-only 口径处理
//! （对齐 C++ `TryCleanupLocalPartitionDir`，`async_io_manager.cpp:1470-1476`）。

use std::fs::{create_dir_all, remove_file, write};

use fkv_error::KvError;
use fkv_failpoint::set_spec;
use fkv_io::FileIo;
use fkv_io_compio::CompioStore;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

#[compio::test]
async fn rmdir_semantics_keep_dir_or_err() {
  let root = TempRoot::new("compio_rmdir_semantics");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  let io = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );

  // 空目录移除成功。
  assert!(io.try_remove_empty_partition_dir(&tbl).await.unwrap());
  assert!(!dir.exists());

  // 目录非空（未知残留）原样保留：read_dir 见到条目即 Ok(false)，不到 rmdir。
  create_dir_all(&dir).unwrap();
  write(dir.join("stray"), b"x").unwrap();
  assert!(!io.try_remove_empty_partition_dir(&tbl).await.unwrap());
  assert!(dir.exists());

  // 目录收尾 IO 失败（注入点在入口，等价 read_dir EMFILE / rmdir ENOTEMPTY
  // 竞态 / EACCES）：IO 层仍回 `Err`，错误冒泡交调用方按 warning-only 吞。
  remove_file(dir.join("stray")).unwrap();
  set_spec("TryRemoveEmptyDir:Fail@1");
  assert_eq!(
    io.try_remove_empty_partition_dir(&tbl).await.unwrap_err(),
    KvError::IoFail
  );
  set_spec("");
  assert!(dir.exists(), "注入的 rmdir 失败应保留目录");

  // 无注入对照：同一空目录被移除。
  assert!(io.try_remove_empty_partition_dir(&tbl).await.unwrap());
  assert!(!dir.exists());
}
