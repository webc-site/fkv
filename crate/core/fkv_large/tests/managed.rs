use fkv_buf::AlignedBuf;
use fkv_error::KvError;
use fkv_io::FixedSegmentIo;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::{
  SegmentLayout, read_value, read_value_fixed, read_value_into, read_value_pinned, write_value,
  write_value_fixed, write_value_pinned,
};
use fkv_mapping::{AllocMode, PageMapper};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
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
  let root = TempRoot::new("large_managed_compio");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let store = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 2);
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
}

#[compio::test]
async fn fixed_buffer_write_and_read() {
  let root = TempRoot::new("large_managed_fixed");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let store = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 2);
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let value = pattern(9000);

  let mut src = store.register_fixed_buffer(value.len()).unwrap();
  assert_eq!(src.is_registered(), cfg!(target_os = "linux"));
  src
    .as_mut_slice()
    .expect("无在飞 fixed op")
    .copy_from_slice(&value);
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
}

/// 18 段跨 3 个提交批：pinned / fixed 写与 managed 读都按批并发提交。
#[compio::test]
async fn multi_batch_pinned_and_fixed() {
  let root = TempRoot::new("large_managed_batch_pinned");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let store = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 2);
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let value = pattern(4096 * 17 + 5);

  let mut pinned = AlignedBuf::new(value.len());
  pinned.copy_from_slice(&value);
  let written = write_value_pinned(&store, &tbl, &mut mapper, layout, &pinned, b"meta")
    .await
    .unwrap();
  assert_eq!(written.segment_id_li.len(), 18);

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

  // fixed 写按批提交；尾段不足一整段走补零路径。
  let mut src = store.register_fixed_buffer(value.len()).unwrap();
  src
    .as_mut_slice()
    .expect("无在飞 fixed op")
    .copy_from_slice(&value);
  let written = write_value_fixed(&store, &tbl, &mut mapper, layout, &src, b"m")
    .await
    .unwrap();
  let mut dst = store.register_fixed_buffer(value.len()).unwrap();
  read_value_fixed(&store, &tbl, &mapper, layout, &written.content, &mut dst)
    .await
    .unwrap();
  assert_eq!(dst.as_slice(), value);
}

#[compio::test]
async fn memstore_fixed_buffer_falls_back() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let store = MemStore::new(512).with_segment_config(4096, 2);
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  // 11 段跨 2 个提交批：默认实现逐段回退时批量切分语义不变。
  let value = pattern(4096 * 10 + 3);
  let mut src = store.register_fixed_buffer(value.len()).unwrap();
  assert!(!src.is_registered());
  src
    .as_mut_slice()
    .expect("无在飞 fixed op")
    .copy_from_slice(&value);
  let written = write_value_fixed(&store, &tbl, &mut mapper, layout, &src, b"")
    .await
    .unwrap();
  assert_eq!(written.segment_id_li.len(), 11);
  let mut dst = store.register_fixed_buffer(value.len()).unwrap();
  read_value_fixed(&store, &tbl, &mapper, layout, &written.content, &mut dst)
    .await
    .unwrap();
  assert_eq!(dst.as_slice(), value);
}

/// 21 段跨 3 个提交批（8+8+5）：fixed 读批间串行、批内并发；`value_len` 之外
/// 的缓冲保留区不被改写（rev-44 残余计划 1）。
#[compio::test]
async fn fixed_read_multi_batch_keeps_slack_region() {
  let root = TempRoot::new("large_managed_batch_fixed_read");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let store = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 2);
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let value = pattern(4096 * 20 + 7);
  let written = write_value(&store, &tbl, &mut mapper, layout, &value, b"m")
    .await
    .unwrap();
  assert_eq!(written.segment_id_li.len(), 21);

  let mut dst = store.register_fixed_buffer(value.len() + 4096).unwrap();
  dst.as_mut_slice().expect("无在飞 fixed op").fill(0xEE);
  let result = read_value_fixed(&store, &tbl, &mapper, layout, &written.content, &mut dst)
    .await
    .unwrap();
  let view = dst.as_slice();
  assert_eq!(&view[..value.len()], value);
  assert!(view[value.len()..].iter().all(|byte| *byte == 0xEE));
  assert_eq!(result.header.actual_len, value.len() as u32);
  assert_eq!(result.metadata, b"m");
}
