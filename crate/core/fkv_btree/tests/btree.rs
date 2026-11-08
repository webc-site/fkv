use std::borrow::Cow;

use fkv_btree::{
  BatchEntry, MergeOption, decompress_value, merge_data_pages_with_options,
  merge_sorted_unique_data_pages_with_options,
};
use fkv_compress::DictCompression;
use fkv_page::{
  CompressionType, DEFAULT_OVERFLOW_POINTERS, DataPageBuilder, DataPageIter, EntrySpec, IndexPage,
  IndexPageBuilder, IndexPageIter,
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
    now_ms: 0,
    ttl_upsert_sink: None,
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

/// 带 expire 的 data 页：`(key, ts, expire_ts)`。
fn data_page_with_expire(entry_li: &[(u8, u64, u64)]) -> Vec<u8> {
  let mut builder = DataPageBuilder::new(512, 16).unwrap();
  for (key, ts, expire) in entry_li {
    builder
      .add(EntrySpec {
        key: &[*key],
        value: &[*key],
        ts: *ts,
        expire_ts: *expire,
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

/// index 下降用的 floor 定位：`seek_floor` 命中则取该 entry 的子页 id，
/// 否则回退到 leftmost 指针（与 `fkv_task_read::process_page` 同构）。
fn index_child(page: &[u8], key: &[u8]) -> u32 {
  let mut iter = IndexPageIter::new(page).unwrap();
  if iter.seek_floor(key).unwrap() {
    iter.page_id()
  } else {
    IndexPage::new(page).unwrap().leftmost_ptr().unwrap()
  }
}

#[test]
fn index_seek() {
  let page = index_page();
  assert_eq!(index_child(&page, b"a"), 1);
  assert_eq!(index_child(&page, b"b"), 2);
  assert_eq!(index_child(&page, b"bb"), 2);
  assert_eq!(index_child(&page, b"c"), 3);
  assert_eq!(index_child(&page, b"z"), 3);
}

#[test]
fn decompress_none_borrows() {
  let value = b"plain".as_slice();
  let out = decompress_value(value, CompressionType::None, None).unwrap();
  assert!(matches!(out, Cow::Borrowed(_)));
  assert_eq!(out.as_ptr(), value.as_ptr());
}

#[test]
fn decompress_dict_without_dict_fails() {
  assert!(decompress_value(b"x", CompressionType::Dictionary, None).is_err());
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

/// 有序去重快路径必须与通用入口逐字节一致，并记录与页内容一致的首 / 末键。
#[test]
fn sorted_unique_merge_matches_generic_and_records_page_boundaries() {
  let old = data_page(&[(b'a', 1), (b'b', 1)]);
  let batch: Vec<BatchEntry> = (0..200u32)
    .map(|idx| BatchEntry::upsert(format!("k{idx:04}").into_bytes(), vec![b'v'; 40], 2))
    .collect();
  let option = merge_opt(512, None);
  let generic = merge_data_pages_with_options(&[&old], &batch, option).unwrap();
  let fast = merge_sorted_unique_data_pages_with_options(&[&old], &batch, option).unwrap();
  assert_eq!(generic.page_li, fast.page_li);
  assert_eq!(generic.overflow_li, fast.overflow_li);
  assert_eq!(generic.old_release_needed, fast.old_release_needed);
  assert!(fast.page_li.len() > 1, "夹具必须跨多页");
  assert_eq!(fast.page_first_key_li.len(), fast.page_li.len());
  assert_eq!(fast.page_last_key_li.len(), fast.page_li.len());
  for (idx, page) in fast.page_li.iter().enumerate() {
    let mut iter = DataPageIter::new(page).unwrap();
    assert!(iter.advance().unwrap());
    assert_eq!(iter.key(), fast.page_first_key_li[idx].as_slice());
    let mut last = iter.key().to_vec();
    while iter.advance().unwrap() {
      last.clear();
      last.extend_from_slice(iter.key());
    }
    assert_eq!(last, fast.page_last_key_li[idx]);
  }

  // 单页输出不记录边界键（写路径不需要，也避免覆盖写热路径的小分配）。
  let single = [BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 2)];
  let one = merge_sorted_unique_data_pages_with_options(&[&old], &single, option).unwrap();
  assert_eq!(one.page_li.len(), 1);
  assert!(one.page_first_key_li.is_empty());
  assert!(one.page_last_key_li.is_empty());
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

/// sem-50：重写 data 页时 `expire_ts <= now_ms` 的旧条目直接丢弃（对齐 C++
/// `BatchWriteTask::add_to_page`），未过期条目照常带出；边界 `expire == now`
/// 也算过期，`now_ms = 0` 不过滤。
#[test]
fn merge_drops_expired_old_entries() {
  let old = data_page_with_expire(&[(b'a', 1, 50), (b'b', 1, 0), (b'c', 1, 200), (b'e', 1, 100)]);
  let batch = [BatchEntry::upsert(b"d".to_vec(), b"D".to_vec(), 2)];

  let mut opt = merge_opt(512, None);
  opt.now_ms = 100;
  let merged = merge_data_pages_with_options(&[&old], &batch, opt).unwrap();
  // a 已过期、e 恰好到期：都不进入新页；b（无 TTL）与 c（未到期）保留。
  assert_eq!(
    key_li_of(&merged.page_li),
    vec![b"b".to_vec(), b"c".to_vec(), b"d".to_vec()]
  );

  // now_ms = 0 视为不过滤：全部旧条目带出，行为与改动前一致。
  let unfiltered = merge_data_pages_with_options(&[&old], &batch, merge_opt(512, None)).unwrap();
  assert_eq!(
    key_li_of(&unfiltered.page_li),
    vec![
      b"a".to_vec(),
      b"b".to_vec(),
      b"c".to_vec(),
      b"d".to_vec(),
      b"e".to_vec()
    ]
  );
}

/// 更旧时间戳的 batch 写入被 merge 丢弃时，过期判定仍按旧条目自身的
/// `expire_ts` 生效：C++ 在 `cmp_ret == 0 && change_ts < base_ts` 分支同样把
/// 旧条目交给 `add_to_page`，过期即丢弃。
#[test]
fn merge_drops_expired_old_entry_even_if_batch_is_older() {
  let old = data_page_with_expire(&[(b'a', 10, 50)]);
  let mut opt = merge_opt(512, None);
  opt.now_ms = 50;
  let merged = merge_data_pages_with_options(
    &[&old],
    &[BatchEntry::upsert(b"a".to_vec(), b"new".to_vec(), 5)],
    opt,
  )
  .unwrap();
  assert!(merged.page_li.is_empty());
}

/// `MergeOutput::old_release_needed` 必须与 `rewrite_page_values` 的释放条件
/// 逐项对应：只有**被丢弃**（本批覆盖 / 删除，或已过期）且占用 overflow 链、
/// 大 value 段或旧 TTL 索引项的旧条目才算。
///
/// 写路径据此外跳过对旧页的第二次遍历；判成 `false` 却仍需释放会造成空间 /
/// TTL 索引泄漏，判成 `true` 只是少一次快路径（正确性不变）。
#[test]
fn merge_reports_old_release_need() {
  let inline = data_page(&[(b'a', 1), (b'b', 1), (b'c', 1)]);
  let ttl = data_page_with_expire(&[(b'a', 1, 9), (b'b', 1, 0), (b'c', 1, 0)]);
  // overflow 指针数组占位（页内 flags 置 overflow 位）。
  let mut builder = DataPageBuilder::new(512, 16).unwrap();
  builder
    .add(EntrySpec {
      key: b"a",
      value: &[0u8; 8],
      ts: 1,
      expire_ts: 0,
      overflow: true,
      compression: CompressionType::None,
      large_value: false,
    })
    .unwrap();
  let overflow = builder.finish().unwrap();

  // 纯内联值、无 TTL：批量删除不需要任何回收（本次优化的快路径）。
  let merged = merge_data_pages_with_options(
    &[&inline],
    &[BatchEntry::delete(b"b".to_vec(), 2)],
    merge_opt(512, None),
  )
  .unwrap();
  assert!(!merged.old_release_needed);

  // 旧条目带 TTL 且被本批删除：必须回收旧 TTL 索引项。
  let merged = merge_data_pages_with_options(
    &[&ttl],
    &[BatchEntry::delete(b"a".to_vec(), 2)],
    merge_opt(512, None),
  )
  .unwrap();
  assert!(merged.old_release_needed);

  // 旧条目带 TTL 但本批时间戳更旧（旧条目存活）：不释放。
  let merged = merge_data_pages_with_options(
    &[&ttl],
    &[BatchEntry::upsert(b"a".to_vec(), b"new".to_vec(), 0)],
    merge_opt(512, None),
  )
  .unwrap();
  assert!(!merged.old_release_needed);

  // 过期丢弃：即使本批没有覆盖它，也要回收旧值与 TTL 索引项。
  let mut opt = merge_opt(512, None);
  opt.now_ms = 9;
  let merged = merge_data_pages_with_options(&[&ttl], &[] as &[BatchEntry], opt).unwrap();
  assert!(merged.old_release_needed);

  // overflow 值被覆盖 / 删除：必须 free 旧链。
  let merged = merge_data_pages_with_options(
    &[&overflow],
    &[BatchEntry::delete(b"a".to_vec(), 2)],
    merge_opt(512, None),
  )
  .unwrap();
  assert!(merged.old_release_needed);
}
