use std::{
  fs::{OpenOptions, metadata, write},
  path::Path,
};

use fkv_io::{ManifestIo, PageIo, SegmentIo};
use fkv_io_compio::CompioStore;
use fkv_testkit::TempRoot;
use fkv_types::{MAIN_BRANCH_NAME, TableIdent, manifest_file_name, segment_file_name};

const PAGE_SIZE: usize = 512;
const SEGMENT_SIZE: usize = 4096;
const SEGMENT_SHIFT: u8 = 2;

fn store(root: &TempRoot) -> CompioStore {
  CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, 2)
    .with_segment_config(SEGMENT_SIZE, SEGMENT_SHIFT)
    .with_checksum_verify(false)
}

fn truncate(path: &Path, len: u64) {
  OpenOptions::new()
    .write(true)
    .open(path)
    .unwrap()
    .set_len(len)
    .unwrap();
}

/// evict 后脏集合归零，重新 append 从 fstat 取偏移而非旧缓存。
#[compio::test]
async fn evict_table_clears_dirty_and_manifest_offset() {
  let root = TempRoot::new("evict");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = store(&root);
  let manifest = root
    .join("demo.1")
    .join(manifest_file_name(MAIN_BRANCH_NAME, 0));

  store.write_page(&tbl, 0, &[1u8; PAGE_SIZE]).await.unwrap();
  assert_eq!(store.dirty_file_num(&tbl), 1);
  store.append_manifest(&tbl, b"aaaa").await.unwrap();

  // 外部重建 manifest（模拟 standby / 运维）：文件被截短。
  write(&manifest, b"bb").unwrap();

  store.evict_table(&tbl);
  assert_eq!(store.dirty_file_num(&tbl), 0);

  // 旧偏移缓存若残留会写到 offset=4，留下空洞；正确实现从 fstat 得到 2。
  store.append_manifest(&tbl, b"cc").await.unwrap();
  assert_eq!(store.read_manifest(&tbl).await.unwrap().unwrap(), b"bbcc");
}

/// evict 后 segment 预分配标记作废：下一次写入重新 `set_len` 到整文件长度。
#[compio::test]
async fn evict_table_clears_segment_preallocation() {
  let root = TempRoot::new("evict");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = store(&root);
  let path = root
    .join("demo.1")
    .join(segment_file_name(0, MAIN_BRANCH_NAME, 0));
  let full_len = (SEGMENT_SIZE as u64) << SEGMENT_SHIFT;

  store
    .write_segment(&tbl, 0, &[1u8; SEGMENT_SIZE])
    .await
    .unwrap();
  assert_eq!(metadata(&path).unwrap().len(), full_len);

  truncate(&path, SEGMENT_SIZE as u64);
  store.evict_table(&tbl);
  store
    .write_segment(&tbl, 0, &[2u8; SEGMENT_SIZE])
    .await
    .unwrap();
  assert_eq!(metadata(&path).unwrap().len(), full_len);
}
