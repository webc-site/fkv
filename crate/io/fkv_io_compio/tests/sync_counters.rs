//! 写路径每批 IO 次数回归：脏文件才 fsync、manifest 一写一 fsync、fd 复用。
//!
//! 目标是把「每批 100 条写」的 IO 契约钉死为常数：1 次数据 fdatasync（单文件）、
//! 1 次 manifest 追加写 + 1 次 fdatasync、目录 fsync 只在首次创建时发生。

use fkv_io::{ManifestIo, PageIo};
use fkv_io_compio::CompioStore;
use fkv_testkit::TempRoot;
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, TableIdent};

#[compio::test]
async fn per_batch_io_count_is_minimal() {
  let root = TempRoot::new("sync_counters");
  let page_size = 512;
  let store = CompioStore::new(
    root.path().to_path_buf(),
    page_size,
    DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_checksum_verify(false);
  let tbl = TableIdent::new("demo", 1).unwrap();

  // 连续两页合并成一次 write_at；该文件成为唯一脏文件。
  let page_li = [(0u64, vec![1u8; page_size]), (1, vec![2u8; page_size])];
  let write_before = store.write_at_num();
  store.write_page_li(&tbl, &page_li).await.unwrap();
  assert_eq!(store.write_at_num() - write_before, 1);
  assert_eq!(store.dirty_file_num(&tbl), 1);

  // 一次 sync_data = 一次 fdatasync，且脏集合清空。
  let sync_before = store.data_sync_num();
  let fsync_before = store.data_fsync_num();
  store.sync_data(&tbl).await.unwrap();
  assert_eq!(store.data_sync_num() - sync_before, 1);
  assert_eq!(store.data_fsync_num() - fsync_before, 1);
  assert_eq!(store.dirty_file_num(&tbl), 0);

  // 脏集合为空时再 sync_data 不得发出 fdatasync（只允许目录项检查）。
  let dir_before = store.dir_fsync_num();
  store.sync_data(&tbl).await.unwrap();
  assert_eq!(store.data_fsync_num(), fsync_before + 1);
  assert_eq!(store.dir_fsync_num(), dir_before);

  // manifest 追加：每条 record 一次写 + 一次 fdatasync；第二次复用缓存 fd，
  // 不得新增 open（fd 缓存命中路径）。
  let record = vec![0x5au8; page_size];
  let fd_before = store.fd_open_num();
  store.append_manifest(&tbl, &record).await.unwrap();
  store.append_manifest(&tbl, &record).await.unwrap();
  assert_eq!(store.manifest_append_num(), 2);
  assert_eq!(store.manifest_sync_num(), 2);
  assert_eq!(store.fd_open_num() - fd_before, 1);
}
