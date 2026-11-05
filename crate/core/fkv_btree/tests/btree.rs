use fkv_btree::{
  BatchEntry, IndexNode, MergeOption, decompress_value, merge_data_pages_with_options,
};
use fkv_compress::DictCompression;
use fkv_page::{
  CompressionType, DEFAULT_OVERFLOW_POINTERS, DataPageBuilder, DataPageIter, EntrySpec,
  IndexPageBuilder,
};

fn index_page() -> Vec<u8> {
  let mut builder = IndexPageBuilder::new(512, 16, false).unwrap();
  builder.add(b"", 1).unwrap();
  builder.add(b"b", 2).unwrap();
  builder.add(b"c", 3).unwrap();
  builder.finish().unwrap()
}

/// 测试通用 merge 配置：restart 间隔固定 16，overflow 指针取默认值。
fn merge_opt(page_size: usize, dict: Option<&DictCompression>) -> MergeOption<'_> {
  MergeOption {
    page_size,
    restart_interval: 16,
    overflow_pointers: DEFAULT_OVERFLOW_POINTERS,
    dict,
  }
}

fn data_page(entry_li: &[(u8, u64)]) -> Vec<u8> {
  let mut builder = DataPageBuilder::new(512, 16).unwrap();
  for (key, ts) in entry_li {
    builder
      .add(EntrySpec {
        key: &[*key],
        value: &[*key],
        ts: *ts,
        expire_ts: 0,
        overflow: false,
        compression: CompressionType::None,
        large_value: false,
      })
      .unwrap();
  }
  builder.finish().unwrap()
}

fn key_li_of(page_li: &[Vec<u8>]) -> Vec<Vec<u8>> {
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
  let node = IndexNode::new(0, &page).unwrap();
  assert_eq!(node.seek(b"a").unwrap(), 1);
  assert_eq!(node.seek(b"b").unwrap(), 2);
  assert_eq!(node.seek(b"bb").unwrap(), 2);
  assert_eq!(node.seek(b"c").unwrap(), 3);
  assert_eq!(node.seek(b"z").unwrap(), 3);
}

#[test]
fn merge_and_delete() {
  let old = data_page(&[(b'a', 1), (b'c', 1)]);
  let batch = [
    BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2),
    BatchEntry::upsert(b"d".to_vec(), b"D".to_vec(), 2),
    BatchEntry::delete(b"c".to_vec(), 3),
  ];
  let page_li = merge_data_pages_with_options(&[&old], &batch, merge_opt(512, None)).unwrap();
  assert_eq!(
    key_li_of(&page_li.page_li),
    vec![b"a".to_vec(), b"b".to_vec(), b"d".to_vec()]
  );
}

#[test]
fn older_timestamp_is_skipped() {
  let old = data_page(&[(b'a', 10)]);
  let batch = [BatchEntry::upsert(b"a".to_vec(), b"new".to_vec(), 9)];
  let page_li = merge_data_pages_with_options(&[&old], &batch, merge_opt(512, None)).unwrap();
  let mut iter = DataPageIter::new(&page_li.page_li[0]).unwrap();
  assert!(iter.advance().unwrap());
  assert_eq!(iter.value(), b"a");
}

/// 有字典时 100B~10MiB 的值走 Dictionary，读路径用同一字典还原。
#[test]
fn dictionary_compression_roundtrips() {
  let mut dict = DictCompression::new(3);
  dict.load_dictionary(vec![b'x'; 512]).unwrap();
  let value = vec![b'x'; 400];
  let batch = [BatchEntry::upsert(b"k".to_vec(), value.clone(), 1)];
  let page_li = merge_data_pages_with_options(&[], &batch, merge_opt(4096, Some(&dict))).unwrap();
  let mut iter = DataPageIter::new(&page_li.page_li[0]).unwrap();
  assert!(iter.advance().unwrap());
  assert_eq!(iter.compression(), CompressionType::Dictionary);
  assert!(iter.value().len() < value.len());
  assert_eq!(
    decompress_value(iter.value(), iter.compression(), Some(&dict))
      .unwrap()
      .into_owned(),
    value
  );
}

/// 无字典时 100B~10MiB 的值不压缩（对齐 C++ `Prepare`），按原值落页。
#[test]
fn no_dictionary_skips_mid_size_values() {
  let value = vec![b'x'; 400];
  let batch = [BatchEntry::upsert(b"k".to_vec(), value.clone(), 1)];
  let page_li =
    merge_data_pages_with_options(&[], &batch, merge_opt(4096, Some(&DictCompression::new(3))))
      .unwrap();
  let mut iter = DataPageIter::new(&page_li.page_li[0]).unwrap();
  assert!(iter.advance().unwrap());
  assert_eq!(iter.compression(), CompressionType::None);
  assert_eq!(iter.value(), value);
}

/// 短值不压缩，保持 None。
#[test]
fn short_value_is_not_compressed() {
  let batch = [BatchEntry::upsert(b"k".to_vec(), b"tiny".to_vec(), 1)];
  let page_li =
    merge_data_pages_with_options(&[], &batch, merge_opt(4096, Some(&DictCompression::new(3))))
      .unwrap();
  let mut iter = DataPageIter::new(&page_li.page_li[0]).unwrap();
  assert!(iter.advance().unwrap());
  assert_eq!(iter.compression(), CompressionType::None);
  assert_eq!(iter.value(), b"tiny");
}

/// 再合并已压缩页时不得二次压缩：`from_page` 直接透传原字节与标记。
#[test]
fn remerge_does_not_double_compress() {
  let mut dict = DictCompression::new(3);
  dict.load_dictionary(vec![b'y'; 512]).unwrap();
  let value = vec![b'y'; 400];
  let page_li = merge_data_pages_with_options(
    &[],
    &[BatchEntry::upsert(b"k".to_vec(), value.clone(), 1)],
    merge_opt(4096, Some(&dict)),
  )
  .unwrap();
  let first = page_li.page_li[0].clone();
  let remerged = merge_data_pages_with_options(
    &[&first],
    &[] as &[BatchEntry],
    merge_opt(4096, Some(&dict)),
  )
  .unwrap();
  let mut iter = DataPageIter::new(&remerged.page_li[0]).unwrap();
  assert!(iter.advance().unwrap());
  assert_eq!(iter.compression(), CompressionType::Dictionary);
  assert_eq!(
    decompress_value(iter.value(), iter.compression(), Some(&dict))
      .unwrap()
      .into_owned(),
    value
  );
}

/// 同一 key 的批内重复项按 timestamp 折叠，乱序批也不被较旧写入覆盖。
#[test]
fn duplicate_batch_keys_keep_newest() {
  let batch = [
    BatchEntry::upsert(b"k".to_vec(), b"v1".to_vec(), 1),
    BatchEntry::upsert(b"k".to_vec(), b"v3".to_vec(), 3),
    BatchEntry::upsert(b"k".to_vec(), b"v2".to_vec(), 2),
    BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
  ];
  let merged = merge_data_pages_with_options(&[], &batch, merge_opt(512, None)).unwrap();
  assert_eq!(
    key_li_of(&merged.page_li),
    vec![b"a".to_vec(), b"k".to_vec()]
  );
  let mut iter = DataPageIter::new(&merged.page_li[0]).unwrap();
  while iter.advance().unwrap() {
    if iter.key() == b"k" {
      assert_eq!(iter.value(), b"v3");
    }
  }
}

/// 多个旧页与 batch 三路交错，输出严格按 key 升序且不丢项。
#[test]
fn merges_multiple_old_pages_in_order() {
  let first = data_page(&[(b'a', 1), (b'b', 1)]);
  let second = data_page(&[(b'd', 1), (b'e', 1)]);
  let batch = [
    BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 2),
    BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2),
  ];
  let merged =
    merge_data_pages_with_options(&[&first, &second], &batch, merge_opt(512, None)).unwrap();
  assert_eq!(
    key_li_of(&merged.page_li),
    vec![
      b"a".to_vec(),
      b"b".to_vec(),
      b"c".to_vec(),
      b"d".to_vec(),
      b"e".to_vec()
    ]
  );
}

/// timestamp 相等时 batch 覆盖旧页；删除项使整条 key 消失。
#[test]
fn delete_at_equal_timestamp_wins() {
  let old = data_page(&[(b'k', 5)]);
  let merged = merge_data_pages_with_options(
    &[&old],
    &[BatchEntry::delete(b"k".to_vec(), 5)],
    merge_opt(512, None),
  )
  .unwrap();
  assert!(merged.page_li.is_empty());
}
