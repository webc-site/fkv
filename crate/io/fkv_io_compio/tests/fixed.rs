#![cfg(target_os = "linux")]

use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_io::{FixedSegmentIo, PageIo, SegmentIo};
use fkv_io_compio::CompioStore;
use fkv_types::TableIdent;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_fixed_{}_{}_{}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos(),
    seq,
  ));
  create_dir_all(&root).unwrap();
  root
}

fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

#[compio::test]
async fn fixed_buffer_ops_complete_on_runtime_ring() {
  let root = temp_root();
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.clone(), 512, 18).with_segment_config(4096, 2);
  let mut src = store.register_fixed_buffer(4096).unwrap();
  let mut dst = store.register_fixed_buffer(4096).unwrap();
  assert!(src.is_registered());
  assert!(dst.is_registered());
  assert_eq!(src.index(), 0);
  assert_eq!(dst.index(), 1);

  let value = pattern(4096);
  src.as_mut_slice().copy_from_slice(&value);
  store
    .write_segment_fixed(&tbl, 0, &src, 0, 4096)
    .await
    .unwrap();

  // 普通读确认 WRITE_FIXED 真正落盘。
  let mut plain = vec![0u8; 4096];
  store.read_segment(&tbl, 0, &mut plain).await.unwrap();
  assert_eq!(plain, value);

  // READ_FIXED 通过 compio completion queue 完成后，数据直接落在注册 buffer。
  dst.as_mut_slice().fill(0);
  store
    .read_segment_fixed(&tbl, 0, &mut dst, 0, 4096)
    .await
    .unwrap();
  assert_eq!(dst.as_slice(), value);

  // 非零 buffer 偏移 + 另一个 segment 文件。
  store
    .write_segment_fixed(&tbl, 4, &src, 100, 512)
    .await
    .unwrap();
  let mut out = vec![0u8; 512];
  store.read_segment(&tbl, 4, &mut out).await.unwrap();
  assert_eq!(out, value[100..612]);

  remove_dir_all(root).unwrap();
}

#[compio::test]
async fn fixed_reads_interleave_with_other_compio_io() {
  let root = temp_root();
  let tbl = TableIdent::new("demo", 2).unwrap();
  let store = CompioStore::new(root.clone(), 512, 18).with_segment_config(4096, 2);
  let mut src = store.register_fixed_buffer(4096).unwrap();
  let mut dst = store.register_fixed_buffer(4096).unwrap();
  let value = pattern(4096);
  src.as_mut_slice().copy_from_slice(&value);
  store
    .write_segment_fixed(&tbl, 0, &src, 0, 4096)
    .await
    .unwrap();

  // 在 fixed read 之间插入普通 compio 文件 IO，两者共享同一个 runtime ring。
  let mut page = vec![0u8; 512];
  store.write_page(&tbl, 0, &page).await.unwrap();
  page[0] = 7;
  store.write_page(&tbl, 0, &page).await.unwrap();
  store
    .read_segment_fixed(&tbl, 0, &mut dst, 0, 4096)
    .await
    .unwrap();
  store.read_page(&tbl, 0, &mut page).await.unwrap();
  assert_eq!(page[0], 7);
  assert_eq!(dst.as_slice(), value);

  remove_dir_all(root).unwrap();
}
