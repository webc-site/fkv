use std::fs::metadata;

use fkv_error::KvError;
use fkv_io::SegmentIo;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::{
  HAS_METADATA_BIT, LENGTH_MASK, LargeValueHeader, SegmentLayout, decode, decode_header,
  decode_metadata_only, encode, free_value, read_value, write_value,
};
use fkv_mapping::{AllocMode, MappingTbl, PageMapper};
use fkv_page::{CompressionType, DataPageBuilder, DataPageIter, EntrySpec};
use fkv_testkit::TempRoot;
use fkv_types::{TableIdent, is_file_page_id};
fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

#[test]
fn encode_and_decode_with_metadata_trailer() {
  let segment_id_li = [3u32, 7, 9];
  let metadata = b"cache-block-descriptor";
  let content = encode(10_000, &segment_id_li, metadata).unwrap();
  assert_eq!(
    content.len(),
    4 + segment_id_li.len() * 4 + 2 + metadata.len()
  );

  let header = decode_header(&content, 4096).unwrap();
  assert_eq!(header.actual_len, 10_000);
  assert_eq!(header.segment_num, 3);
  assert_eq!(header.metadata_len, metadata.len() as u16);
  assert_ne!(
    u32::from_le_bytes(content[..4].try_into().unwrap()) & HAS_METADATA_BIT,
    0
  );

  let decoded = decode(&content, 4096).unwrap();
  assert_eq!(decoded.segment_id_li, segment_id_li);
  assert_eq!(decoded.metadata, metadata);

  let (_, metadata_only) = decode_metadata_only(&content, 4096).unwrap();
  assert_eq!(metadata_only, metadata);

  let no_metadata = encode(10_000, &segment_id_li, b"").unwrap();
  assert_eq!(no_metadata.len(), 4 + segment_id_li.len() * 4);
  let header = decode_header(&no_metadata, 4096).unwrap();
  assert_eq!(header.metadata_len, 0);
  assert_eq!(
    u32::from_le_bytes(no_metadata[..4].try_into().unwrap()) & HAS_METADATA_BIT,
    0
  );
  assert!(decode(&no_metadata, 4096).unwrap().metadata.is_empty());

  let empty = encode(0, &[], b"").unwrap();
  assert_eq!(empty.len(), 4);
  assert_eq!(
    decode_header(&empty, 4096).unwrap(),
    LargeValueHeader {
      actual_len: 0,
      segment_num: 0,
      metadata_len: 0,
    }
  );
}

#[test]
fn decode_rejects_malformed_content() {
  assert_eq!(
    decode_header(&[0u8; 3], 4096).unwrap_err(),
    KvError::Corrupted
  );
  assert_eq!(
    decode_header(&[0u8; 4], 0).unwrap_err(),
    KvError::InvalidArgs
  );

  // actual_len 推导 3 个 segment，但塞了 4 个 id。
  let mut bad = 10_000u32.to_le_bytes().to_vec();
  for id in 0..4u32 {
    bad.extend_from_slice(&id.to_le_bytes());
  }
  assert_eq!(decode_header(&bad, 4096).unwrap_err(), KvError::Corrupted);

  // 多一个尾字节。
  let content = encode(10_000, &[1, 2, 3], b"meta").unwrap();
  let mut trailing = content.clone();
  trailing.push(0);
  assert_eq!(
    decode_header(&trailing, 4096).unwrap_err(),
    KvError::Corrupted
  );

  // metadata 长度声明超过实际内容。
  let mut truncated = content;
  truncated.truncate(truncated.len() - 1);
  assert_eq!(
    decode_header(&truncated, 4096).unwrap_err(),
    KvError::Corrupted
  );

  assert_eq!(
    encode(LENGTH_MASK + 1, &[], b"").unwrap_err(),
    KvError::InvalidArgs
  );
  assert_eq!(
    encode(1, &[], &vec![0u8; 65_536]).unwrap_err(),
    KvError::InvalidArgs
  );
}

#[test]
fn segment_layout_maps_file_and_offset() {
  let layout = SegmentLayout::new(4096, 2).unwrap();
  assert_eq!(layout.file_size().unwrap(), 16_384);
  assert_eq!(layout.segment_num(0), 0);
  assert_eq!(layout.segment_num(1), 1);
  assert_eq!(layout.segment_num(4096), 1);
  assert_eq!(layout.segment_num(4097), 2);
  assert_eq!(layout.file_id(0), 0);
  assert_eq!(layout.offset(0), 0);
  assert_eq!(layout.file_id(5), 1);
  assert_eq!(layout.offset(5), 4096);
  assert_eq!(layout.file_id(7), 1);
  assert_eq!(layout.offset(7), 12_288);

  assert_eq!(SegmentLayout::new(0, 2).unwrap_err(), KvError::InvalidArgs);
  assert_eq!(
    SegmentLayout::new(4000, 2).unwrap_err(),
    KvError::InvalidArgs
  );
  assert_eq!(
    SegmentLayout::new(4096, 63).unwrap_err(),
    KvError::InvalidArgs
  );
}

#[compio::test]
async fn memstore_segment_read_write_and_free() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let store = MemStore::new(512).with_segment_config(4096, 2);
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  let value = pattern(9000);
  let metadata = b"meta";
  let written = write_value(&store, &tbl, &mut mapper, layout, &value, metadata)
    .await
    .unwrap();
  assert_eq!(written.segment_id_li.len(), 3);
  assert_eq!(mapper.tbl().len(), 3);

  let read = read_value(&store, &tbl, &mapper, layout, &written.content)
    .await
    .unwrap();
  assert_eq!(read.value, value);
  assert_eq!(read.metadata, metadata);
  assert_eq!(read.header.segment_num, 3);

  let first_fp = mapper.file_page_id(written.segment_id_li[0]).unwrap();
  let mut tail = vec![0u8; 904];
  store.read_segment(&tbl, first_fp, &mut tail).await.unwrap();
  assert_eq!(tail, value[..904]);

  let second_fp = mapper.file_page_id(written.segment_id_li[1]).unwrap();
  let mut tail = vec![0u8; 4096];
  store
    .read_segment(&tbl, second_fp, &mut tail)
    .await
    .unwrap();
  assert_eq!(tail, value[4096..8192]);

  let third_fp = mapper.file_page_id(written.segment_id_li[2]).unwrap();
  let mut tail = vec![0u8; 4096];
  store.read_segment(&tbl, third_fp, &mut tail).await.unwrap();
  assert_eq!(&tail[..808], &value[8192..9000]);
  assert!(tail[808..].iter().all(|byte| *byte == 0));

  assert_eq!(free_value(&mut mapper, &written.content, 4096).unwrap(), 3);
  assert_eq!(free_value(&mut mapper, &written.content, 4096).unwrap(), 0);
  // 释放后不再有活跃 `FilePageId` 槽位（物理段进入 retired 队列等待回收）。
  let live = mapper
    .tbl()
    .iter_value_li()
    .filter(|value| is_file_page_id(*value))
    .count();
  assert_eq!(live, 0);

  // Pooled allocator：释放只把物理页放入 retired 队列，显式回收后才回池复用。
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  let first = mapper.allocate_page();
  let second = mapper.allocate_page();
  let second_fp = mapper.file_page_id(second).unwrap();
  assert_eq!(mapper.free_page(second).unwrap(), Some(second_fp));
  // 回收前不复用：下一次分配仍取新的物理 id。
  let fresh = mapper.allocate_page();
  assert_ne!(mapper.file_page_id(fresh).unwrap(), second_fp);
  // 无在途快照读者：水位 MAX 立即回收，之后分配复用刚释放的物理 id。
  assert_eq!(mapper.reclaim_retired(u64::MAX), 1);
  let reused = mapper.allocate_page();
  assert_eq!(mapper.file_page_id(reused).unwrap(), second_fp);
  // 仍活跃的 first 页物理 id 始终不被复用。
  assert_ne!(mapper.file_page_id(first).unwrap(), second_fp);
}

#[compio::test]
async fn compio_segment_file_and_tail_padding() {
  let root = TempRoot::new("large_compio");
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
  let written = write_value(&store, &tbl, &mut mapper, layout, &value, b"meta")
    .await
    .unwrap();
  assert_eq!(written.segment_id_li.len(), 3);

  let path = root.join("demo.1").join("segment_0_main_0");
  assert!(path.exists());
  assert_eq!(metadata(&path).unwrap().len(), layout.file_size().unwrap());

  let read = read_value(&store, &tbl, &mapper, layout, &written.content)
    .await
    .unwrap();
  assert_eq!(read.value, value);
  assert_eq!(read.metadata, b"meta");

  // 每段都按完整 segment_size 落盘，tail 补零。
  let third_fp = mapper.file_page_id(written.segment_id_li[2]).unwrap();
  let mut tail = vec![0u8; 4096];
  store.read_segment(&tbl, third_fp, &mut tail).await.unwrap();
  assert_eq!(&tail[..808], &value[8192..9000]);
  assert!(tail[808..].iter().all(|byte| *byte == 0));

  // 跨文件：physical segment 5 -> segment_1_main_0 的第二个槽位。
  let extra = pattern(128);
  store.write_segment(&tbl, 5, &extra).await.unwrap();
  let second_path = root.join("demo.1").join("segment_1_main_0");
  assert!(second_path.exists());
  assert_eq!(
    metadata(&second_path).unwrap().len(),
    layout.file_size().unwrap()
  );
  let mut read_back = vec![0u8; 128];
  store.read_segment(&tbl, 5, &mut read_back).await.unwrap();
  assert_eq!(read_back, extra);
}

/// 64 段 = 跨越 8 个提交批（批大小 8），验证多批并发读写与尾段补零。
#[compio::test]
async fn compio_multi_batch_roundtrip() {
  let root = TempRoot::new("large_compio_batch");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let store = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 2);
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let value = pattern(4096 * 63 + 777);
  let written = write_value(&store, &tbl, &mut mapper, layout, &value, b"meta")
    .await
    .unwrap();
  assert_eq!(written.segment_id_li.len(), 64);

  let read = read_value(&store, &tbl, &mapper, layout, &written.content)
    .await
    .unwrap();
  assert_eq!(read.value, value);
  assert_eq!(read.metadata, b"meta");
  assert_eq!(read.header.segment_num, 64);

  // 64 段 × 4096B，文件每份 4 槽：落到 segment_0..segment_15。
  for file_id in 0..16 {
    assert!(
      root
        .join("demo.1")
        .join(format!("segment_{file_id}_main_0"))
        .exists()
    );
  }

  // 尾段（第 64 段）按完整 segment_size 落盘并补零。
  let last_fp = mapper.file_page_id(written.segment_id_li[63]).unwrap();
  let mut tail = vec![0u8; 4096];
  store.read_segment(&tbl, last_fp, &mut tail).await.unwrap();
  assert_eq!(&tail[..777], &value[4096 * 63..]);
  assert!(tail[777..].iter().all(|byte| *byte == 0));
}

#[test]
fn large_value_fits_in_data_page_entry() {
  let segment_id_li = [1u32, 2, 3];
  let content = encode(10_000, &segment_id_li, b"trailer").unwrap();
  let mut builder = DataPageBuilder::new(4096, 16).unwrap();
  assert!(
    builder
      .add(EntrySpec {
        key: b"big",
        value: &content,
        ts: 1,
        expire_ts: 0,
        overflow: false,
        compression: CompressionType::None,
        large_value: true,
      })
      .unwrap()
  );
  let page = builder.finish().unwrap();
  let mut iter = DataPageIter::new(&page).unwrap();
  assert!(iter.advance().unwrap());
  assert_eq!(iter.key(), b"big");
  assert!(iter.is_large_value());
  assert_eq!(iter.value(), content);
  let decoded = decode(iter.value(), 4096).unwrap();
  assert_eq!(decoded.segment_id_li, segment_id_li);
  assert_eq!(decoded.metadata, b"trailer");
  assert!(!iter.advance().unwrap());
}

#[compio::test]
async fn segment_mapping_survives_serialize_roundtrip() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let store = MemStore::new(512).with_segment_config(4096, 2);
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let value = pattern(10_000);
  let written = write_value(&store, &tbl, &mut mapper, layout, &value, b"meta")
    .await
    .unwrap();
  let mapping_bytes = mapper.serialize();
  let mapping_tbl = MappingTbl::deserialize(&mapping_bytes).unwrap();
  let restored = PageMapper::with_tbl(mapping_tbl, AllocMode::Append, mapper.max_fp_id());
  assert!(
    restored
      .tbl()
      .iter_value_li()
      .eq(mapper.tbl().iter_value_li())
  );
  let read = read_value(&store, &tbl, &restored, layout, &written.content)
    .await
    .unwrap();
  assert_eq!(read.value, value);
  assert_eq!(read.metadata, b"meta");
}
