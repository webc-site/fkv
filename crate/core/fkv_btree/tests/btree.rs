use fkv_btree::{
  BatchEntry, decompress_value, merge_data_pages, merge_data_pages_with_compression, seek_index,
};
use fkv_compress::DictCompression;
use fkv_page::{CompressionType, DataPageBuilder, DataPageIter, IndexPageBuilder};

fn index_page() -> Vec<u8> {
  let mut builder = IndexPageBuilder::new(512, 16, false).unwrap();
  builder.add(b"", 1).unwrap();
  builder.add(b"b", 2).unwrap();
  builder.add(b"c", 3).unwrap();
  builder.finish().unwrap()
}

fn data_page(entry_li: &[(u8, u64)]) -> Vec<u8> {
  let mut builder = DataPageBuilder::new(512, 16).unwrap();
  for (key, ts) in entry_li {
    builder
      .add(
        &[*key],
        &[*key],
        false,
        *ts,
        0,
        CompressionType::None,
        false,
      )
      .unwrap();
  }
  builder.finish().unwrap()
}

fn keys(page_li: &[Vec<u8>]) -> Vec<Vec<u8>> {
  let mut out = Vec::new();
  for page in page_li {
    let mut iter = DataPageIter::new(page).unwrap();
    while iter.advance().unwrap() {
      out.push(iter.key().to_vec());
    }
  }
  out
}

#[test]
fn index_seek() {
  let page = index_page();
  assert_eq!(seek_index(&page, b"a").unwrap(), 1);
  assert_eq!(seek_index(&page, b"b").unwrap(), 2);
  assert_eq!(seek_index(&page, b"bb").unwrap(), 2);
  assert_eq!(seek_index(&page, b"c").unwrap(), 3);
  assert_eq!(seek_index(&page, b"z").unwrap(), 3);
}

#[test]
fn merge_and_delete() {
  let old = data_page(&[(b'a', 1), (b'c', 1)]);
  let batch = [
    BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2),
    BatchEntry::upsert(b"d".to_vec(), b"D".to_vec(), 2),
    BatchEntry::delete(b"c".to_vec(), 3),
  ];
  let page_li = merge_data_pages(&[&old], &batch, 512, 16).unwrap();
  assert_eq!(
    keys(&page_li),
    vec![b"a".to_vec(), b"b".to_vec(), b"d".to_vec()]
  );
}

#[test]
fn older_timestamp_is_skipped() {
  let old = data_page(&[(b'a', 10)]);
  let batch = [BatchEntry::upsert(b"a".to_vec(), b"new".to_vec(), 9)];
  let page_li = merge_data_pages(&[&old], &batch, 512, 16).unwrap();
  let mut iter = DataPageIter::new(&page_li[0]).unwrap();
  assert!(iter.advance().unwrap());
  assert_eq!(iter.value(), b"a");
}

/// 写路径压缩：值超过阈值且压后更小才标记 Standalone，读路径能还原。
#[test]
fn compression_marks_standalone_and_roundtrips() {
  let value = vec![b'x'; 400];
  let batch = [BatchEntry::upsert(b"k".to_vec(), value.clone(), 1)];
  let page_li = merge_data_pages_with_compression(&[], &batch, 4096, 16, Some(&DictCompression::new(3))).unwrap();
  let mut iter = DataPageIter::new(&page_li[0]).unwrap();
  assert!(iter.advance().unwrap());
  assert_eq!(iter.compression(), CompressionType::Standalone);
  assert!(iter.value().len() < value.len());
  assert_eq!(
    decompress_value(iter.value(), iter.compression(), None).unwrap(),
    value
  );
}

/// 短值不压缩，保持 None。
#[test]
fn short_value_is_not_compressed() {
  let batch = [BatchEntry::upsert(b"k".to_vec(), b"tiny".to_vec(), 1)];
  let page_li = merge_data_pages_with_compression(&[], &batch, 4096, 16, Some(&DictCompression::new(3))).unwrap();
  let mut iter = DataPageIter::new(&page_li[0]).unwrap();
  assert!(iter.advance().unwrap());
  assert_eq!(iter.compression(), CompressionType::None);
  assert_eq!(iter.value(), b"tiny");
}

/// 再合并已压缩页时不得二次压缩：`from_page` 直接透传原字节与标记。
#[test]
fn remerge_does_not_double_compress() {
  let value = vec![b'y'; 400];
  let page_li = merge_data_pages_with_compression(
    &[],
    &[BatchEntry::upsert(b"k".to_vec(), value.clone(), 1)],
    4096,
    16,
    Some(&DictCompression::new(3)),
  )
  .unwrap();
  let first = page_li[0].clone();
  let remerged = merge_data_pages_with_compression(&[&first], &[], 4096, 16, Some(&DictCompression::new(3))).unwrap();
  let mut iter = DataPageIter::new(&remerged[0]).unwrap();
  assert!(iter.advance().unwrap());
  assert_eq!(iter.compression(), CompressionType::Standalone);
  assert_eq!(
    decompress_value(iter.value(), iter.compression(), None).unwrap(),
    value
  );
}
