use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_buf::AlignedBuf;
use fkv_error::KvError;
use fkv_io::FixedSegmentIo;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::{
  SegmentLayout, read_value, read_value_fixed, read_value_into, read_value_pinned,
  write_value_fixed, write_value_pinned,
};
use fkv_mapping::{AllocMode, PageMapper};
use fkv_types::TableIdent;

fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

fn temp_root(tag: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_managed_{tag}_{}_{}_{}",
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

#[compio::test]
async fn managed_read_and_pinned_write_roundtrip() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let store = MemStore::new(512).with_segment_config(4096, 2);
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  let value = pattern(9000);
  let mut pinned = AlignedBuf::new(value.len());
  pinned.copy_from_slice(&value);
  assert!(pinned.is_aligned(4096));

  let written = write_value_pinned(&store, &tbl, &mut mapper, layout, &pinned, b"meta")
    .await
    .unwrap();
  assert_eq!(written.segment_id_li.len(), 3);

  let mut managed = AlignedBuf::new(value.len());
  let result = read_value_pinned(
    &store,
    &tbl,
    &mapper,
    layout,
    &written.content,
    &mut managed,
  )
  .await
  .unwrap();
  assert_eq!(&managed[..value.len()], value);
  assert_eq!(result.header.actual_len, value.len() as u32);
  assert_eq!(result.metadata, b"meta");

  // 普通 managed read 不要求对齐。
  let mut plain = vec![0u8; value.len()];
  read_value_into(
    &store,
    &tbl,
    &mapper,
    layout,
    &written.content,
    &mut plain.as_mut_slice(),
  )
  .await
  .unwrap();
  assert_eq!(plain, value);

  // 小 buffer 拒绝。
  let mut small = AlignedBuf::new(128);
  assert_eq!(
    read_value_into(&store, &tbl, &mapper, layout, &written.content, &mut small,)
      .await
      .unwrap_err(),
    KvError::InvalidArgs
  );

  // 未对齐 pinned buffer 拒绝。
  let mut misaligned = vec![0u8; value.len() + 1];
  assert_eq!(
    read_value_pinned(
      &store,
      &tbl,
      &mapper,
      layout,
      &written.content,
      &mut &mut misaligned[1..],
    )
    .await
    .unwrap_err(),
    KvError::InvalidArgs
  );

  let mut raw = vec![0u8; value.len() + 1];
  raw[1..].copy_from_slice(&value);
  assert_eq!(
    write_value_pinned(&store, &tbl, &mut mapper, layout, &&raw[1..], b"",)
      .await
      .unwrap_err(),
    KvError::InvalidArgs
  );

  // 普通 read_value 与 pinned 写内容一致。
  let read = read_value(&store, &tbl, &mapper, layout, &written.content)
    .await
    .unwrap();
  assert_eq!(read.value, value);
  assert_eq!(read.metadata, b"meta");
}

#[compio::test]
async fn compio_pinned_write_and_managed_read() {
  let root = temp_root("compio");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let store = CompioStore::new(root.clone(), 512, 18).with_segment_config(4096, 2);
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let value = pattern(5000);
  let mut pinned = AlignedBuf::new(value.len());
  pinned.copy_from_slice(&value);
  let written = write_value_pinned(&store, &tbl, &mut mapper, layout, &pinned, b"")
    .await
    .unwrap();

  let mut managed = AlignedBuf::new(value.len());
  read_value_pinned(
    &store,
    &tbl,
    &mapper,
    layout,
    &written.content,
    &mut managed,
  )
  .await
  .unwrap();
  assert_eq!(&managed[..value.len()], value);
  assert!(root.join("demo.1").join("segment_0_main_0").exists());

  remove_dir_all(root).unwrap();
}

#[compio::test]
async fn fixed_buffer_write_and_read() {
  let root = temp_root("fixed");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let store = CompioStore::new(root.clone(), 512, 18).with_segment_config(4096, 2);
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let value = pattern(9000);

  let mut src = store.register_fixed_buffer(value.len()).unwrap();
  assert_eq!(src.is_registered(), cfg!(target_os = "linux"));
  src.as_mut_slice().copy_from_slice(&value);
  let written = write_value_fixed(&store, &tbl, &mut mapper, layout, &src, b"meta")
    .await
    .unwrap();

  let mut dst = store.register_fixed_buffer(value.len()).unwrap();
  let result = read_value_fixed(&store, &tbl, &mapper, layout, &written.content, &mut dst)
    .await
    .unwrap();
  assert_eq!(dst.as_slice(), value);
  assert_eq!(result.metadata, b"meta");
  assert_eq!(result.header.actual_len, value.len() as u32);

  remove_dir_all(root).unwrap();
}

#[compio::test]
async fn memstore_fixed_buffer_falls_back() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let store = MemStore::new(512).with_segment_config(4096, 2);
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let value = pattern(4200);
  let mut src = store.register_fixed_buffer(value.len()).unwrap();
  assert!(!src.is_registered());
  src.as_mut_slice().copy_from_slice(&value);
  let written = write_value_fixed(&store, &tbl, &mut mapper, layout, &src, b"")
    .await
    .unwrap();
  let mut dst = store.register_fixed_buffer(value.len()).unwrap();
  read_value_fixed(&store, &tbl, &mapper, layout, &written.content, &mut dst)
    .await
    .unwrap();
  assert_eq!(dst.as_slice(), value);
}
