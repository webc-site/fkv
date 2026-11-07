//! 分区目录创建后的目录项持久化（复审新发现，eng-89 同批）。
//!
//! `ensure_dir` 只 `create_dir_all` 不同步父目录时，断电后 root 里可能没有
//! `demo.1` 这个 entry：目录内的 manifest / data 即使已 fdatasync 也随目录消失。
//! 这里用 `dir_fsync_num` 计数与 `SyncDir` failpoint 验证顺序、次数与错误上抛。
//!
//! 三段断言放在同一个测试里：`fkv_failpoint` 是进程级注册表，失败注入与
//! 期望成功的断言并行跑会互相污染。

use std::path::Path;

use fkv_error::KvError;
use fkv_failpoint::set_spec;
use fkv_io::PageIo;
use fkv_io_compio::CompioStore;
use fkv_testkit::TempRoot;
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, TableIdent};

const PAGE_SIZE: usize = 512;

fn store_at(root: &Path) -> CompioStore {
  CompioStore::new(root.to_path_buf(), PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT)
}

/// root 懒创建：两级父目录都要 fsync；目录就绪后不重复；root 已存在只同步一次；
/// 目录 fsync 失败上抛且可重试。
#[compio::test]
async fn partition_dir_creation_syncs_parent_entries() {
  let page = vec![7u8; PAGE_SIZE];

  // (a) root 由本次写入懒创建：分区目录 entry 记在 root 里、root 的 entry
  // 记在 TempRoot 里，两级父目录都要 fsync。
  let temp = TempRoot::new("io_dir_sync_new_root");
  let root = temp.join("db");
  assert!(!root.exists(), "夹具前提：root 尚不存在");
  let store = store_at(&root);
  let tbl1 = TableIdent::new("demo", 1).unwrap();
  store.write_page(&tbl1, 0, &page).await.unwrap();
  assert_eq!(
    store.dir_fsync_num(),
    2,
    "root 新建：分区目录 entry（root）+ root 自身 entry（TempRoot）各一次"
  );

  // 同一分区再次写：目录已就绪，不再重复 fsync 目录项。
  store.write_page(&tbl1, 1, &page).await.unwrap();
  assert_eq!(store.dir_fsync_num(), 2);

  // 新分区：root 已存在，只需同步 root 一次。
  let tbl2 = TableIdent::new("demo", 2).unwrap();
  store.write_page(&tbl2, 0, &page).await.unwrap();
  assert_eq!(store.dir_fsync_num(), 3);
  assert!(root.join("demo.1").is_dir());
  assert!(root.join("demo.2").is_dir());

  // (b) root 已存在（生产上由运维 / 上次启动建好）：只 fsync root 一次。
  let existing = TempRoot::new("io_dir_sync_existing_root");
  let store = store_at(existing.path());
  let tbl3 = TableIdent::new("demo", 3).unwrap();
  store.write_page(&tbl3, 0, &page).await.unwrap();
  assert_eq!(store.dir_fsync_num(), 1);
  assert!(existing.path().join("demo.3").is_dir());

  // (c) 目录项 fsync 失败必须上抛，且不把目录标记为 ready：下一次写入重试成功。
  let tbl4 = TableIdent::new("demo", 4).unwrap();
  set_spec("SyncDir@1!err=IoFail");
  let failed = store.write_page(&tbl4, 0, &page).await;
  set_spec("");
  assert_eq!(failed.unwrap_err(), KvError::IoFail);
  assert_eq!(store.dir_fsync_num(), 1, "失败的目录 fsync 不计入成功数");
  store.write_page(&tbl4, 0, &page).await.unwrap();
  assert_eq!(store.dir_fsync_num(), 2, "重试必须重新同步目录项");
}

/// `sync_data` 的目录 fsync 失败后必须保留 `dirty_dir`，下一次 sync_data 重试
/// 目录同步；已成功 fdatasync 的文件不重复标脏（r8e-01 / r9e-01）。
#[compio::test]
async fn sync_data_retries_dir_fsync_after_failure() {
  let temp = TempRoot::new("io_sync_data_dir_retry");
  let store = store_at(temp.path());
  let tbl = TableIdent::new("demo", 1).unwrap();
  let page = vec![7u8; PAGE_SIZE];
  store.write_page(&tbl, 0, &page).await.unwrap();
  let before = store.dir_fsync_num();

  set_spec("SyncDir@1!err=IoFail");
  let failed = store.sync_data(&tbl).await;
  set_spec("");
  assert_eq!(failed.unwrap_err(), KvError::IoFail);
  // 失败轮不推进目录同步计数；dirty_dir 仍在，重试才会重新 fsync。
  assert_eq!(store.dir_fsync_num(), before);

  store.sync_data(&tbl).await.unwrap();
  assert_eq!(store.dir_fsync_num(), before + 1);
  assert_eq!(store.dirty_file_num(&tbl), 0);
}
