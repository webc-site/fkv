use fkv_error::KvError;
use fkv_page::{
  CompressionType, DATA_HEADER_SIZE, DataPageBuilder, DataPageIter, EntrySpec, INDEX_HEADER_SIZE,
  IndexPage, IndexPageBuilder, IndexPageIter, PageType, content_len, data_restart_offset,
  header::validate_checksum,
  overflow::{OverflowPage, build as build_overflow},
  overflow_capacity, page_type, restart_num,
};

#[test]
fn data_page_roundtrip() {
  let mut builder = DataPageBuilder::new(512, 2).unwrap();
  let items = [
    (b"a".as_slice(), b"1".as_slice(), 100u64, 0u64, false, false),
    (b"b".as_slice(), b"2".as_slice(), 120, 0, false, false),
    (b"c".as_slice(), b"3".as_slice(), 110, 999, false, false),
    (b"d".as_slice(), b"4".as_slice(), 200, 0, true, false),
    (b"e".as_slice(), b"5".as_slice(), 300, 0, false, true),
  ];
  for (key, value, ts, expire, overflow, large) in items {
    assert!(
      builder
        .add(EntrySpec {
          key,
          value,
          ts,
          expire_ts: expire,
          overflow,
          compression: CompressionType::None,
          large_value: large,
        })
        .unwrap()
    );
  }
  let page = builder.finish().unwrap();
  assert_eq!(page_type(&page).unwrap(), PageType::Data);
  assert!(validate_checksum(&page).unwrap());
  let restart_count = usize::from(restart_num(&page).unwrap());
  assert_eq!(
    content_len(&page).unwrap() as usize,
    data_restart_offset(&page).unwrap() + (1 + restart_count) * 2
  );
  assert_eq!(restart_count, 3);

  let mut iter = DataPageIter::new(&page).unwrap();
  let mut got = Vec::new();
  while iter.advance().unwrap() {
    got.push((
      iter.key().to_vec(),
      iter.value().to_vec(),
      iter.timestamp(),
      iter.expire_ts(),
      iter.is_overflow(),
      iter.is_large_value(),
    ));
  }
  let expected = [
    (b"a".to_vec(), b"1".to_vec(), 100, 0, false, false),
    (b"b".to_vec(), b"2".to_vec(), 120, 0, false, false),
    (b"c".to_vec(), b"3".to_vec(), 110, 999, false, false),
    (b"d".to_vec(), b"4".to_vec(), 200, 0, true, false),
    (b"e".to_vec(), b"5".to_vec(), 300, 0, false, true),
  ];
  assert_eq!(got, expected);
}

#[test]
fn data_page_single_entry_layout() {
  let mut builder = DataPageBuilder::new(128, 16).unwrap();
  builder
    .add(EntrySpec {
      key: b"a",
      value: b"b",
      ts: 0,
      expire_ts: 0,
      overflow: false,
      compression: CompressionType::None,
      large_value: false,
    })
    .unwrap();
  let page = builder.finish().unwrap();
  assert_eq!(page_type(&page).unwrap(), PageType::Data);
  assert_eq!(&page[19..25], &[0, 1, 16, b'a', b'b', 0]);
  assert_eq!(&page[25..27], &19u16.to_le_bytes());
  assert_eq!(&page[27..29], &1u16.to_le_bytes());
  assert_eq!(content_len(&page).unwrap(), 29);
  assert!(validate_checksum(&page).unwrap());
}

#[test]
fn data_page_seek() {
  let mut builder = DataPageBuilder::new(256, 2).unwrap();
  for key in [b"a", b"b", b"c", b"d"] {
    builder
      .add(EntrySpec {
        key,
        value: b"v",
        ts: 0,
        expire_ts: 0,
        overflow: false,
        compression: CompressionType::None,
        large_value: false,
      })
      .unwrap();
  }
  let page = builder.finish().unwrap();
  let mut iter = DataPageIter::new(&page).unwrap();
  assert!(iter.seek(b"c").unwrap());
  assert_eq!(iter.key(), b"c");
  assert!(iter.seek_floor(b"cc").unwrap());
  assert_eq!(iter.key(), b"c");
  assert!(!iter.seek(b"z").unwrap());
}

#[test]
fn index_page_roundtrip() {
  let mut builder = IndexPageBuilder::new(512, 2, true).unwrap();
  assert!(builder.add(b"", 10).unwrap());
  for (key, page_id) in [(b"b".as_slice(), 20), (b"c", 30), (b"d", 40), (b"e", 50)] {
    assert!(builder.add(key, page_id).unwrap());
  }
  let page = builder.finish().unwrap();
  assert_eq!(page_type(&page).unwrap(), PageType::LeafIndex);
  assert!(validate_checksum(&page).unwrap());
  let index = IndexPage::new(&page).unwrap();
  assert!(index.is_leaf().unwrap());
  assert_eq!(index.leftmost_ptr().unwrap(), 10);

  let mut iter = IndexPageIter::new(&page).unwrap();
  let mut got = Vec::new();
  while iter.advance().unwrap() {
    got.push((iter.key().to_vec(), iter.page_id()));
  }
  assert_eq!(
    got,
    vec![
      (b"b".to_vec(), 20),
      (b"c".to_vec(), 30),
      (b"d".to_vec(), 40),
      (b"e".to_vec(), 50),
    ]
  );
}

#[test]
fn overflow_page_roundtrip() {
  let value = vec![7u8; 100];
  let page = build_overflow(512, &value, &[1, 2, 3], 16).unwrap();
  assert_eq!(page_type(&page).unwrap(), PageType::Overflow);
  assert!(validate_checksum(&page).unwrap());
  let overflow = OverflowPage::new(&page).unwrap();
  assert_eq!(overflow.value().unwrap(), &value);
  assert_eq!(overflow.pointer_li().unwrap(), vec![1, 2, 3]);
  assert_eq!(overflow_capacity(512, 16, true).unwrap(), 512 - 11 - 1 - 64);
}

/// 空首 key 是合法输入：第二条不得因此跳过 key 序校验，也不得按页首编码。
#[test]
fn empty_first_key_keeps_order_and_timestamp_chain() {
  let mut builder = DataPageBuilder::new(512, 16).unwrap();
  let add = |builder: &mut DataPageBuilder, key: &'static [u8], ts: u64| {
    builder.add(EntrySpec {
      key,
      value: b"v",
      ts,
      expire_ts: 0,
      overflow: false,
      compression: CompressionType::None,
      large_value: false,
    })
  };
  assert!(add(&mut builder, b"", 100).unwrap());
  // 第二条与前一条相同（空 key）属于重复 key，必须报错。
  assert_eq!(
    add(&mut builder, b"", 110).unwrap_err(),
    KvError::InvalidArgs
  );
  assert!(add(&mut builder, b"a", 120).unwrap());
  let page = builder.finish().unwrap();
  let mut iter = DataPageIter::new(&page).unwrap();
  let mut got = Vec::new();
  while iter.advance().unwrap() {
    got.push((iter.key().to_vec(), iter.timestamp()));
  }
  assert_eq!(got, vec![(Vec::new(), 100), (b"a".to_vec(), 120)]);
}

/// 损坏 overflow 页：value 长度侵入指针区、指针个数反向覆盖 value 区都必须拒绝。
#[test]
fn malformed_overflow_page_is_rejected() {
  let value = vec![7u8; 100];
  let page = build_overflow(512, &value, &[1, 2, 3], 16).unwrap();

  // content_len 位于 offset 9..11；改成 500 使 value_end(511) 越过指针起点(499)。
  let mut overlap_value = page.clone();
  overlap_value[9..11].copy_from_slice(&500u16.to_le_bytes());
  let overflow = OverflowPage::new(&overlap_value).unwrap();
  assert_eq!(overflow.value().unwrap_err(), KvError::Corrupted);
  assert_eq!(overflow.pointer_li().unwrap_err(), KvError::Corrupted);

  // 指针个数改成 200：512 - 1 - 800 下溢。
  let mut huge_count = page.clone();
  *huge_count.last_mut().unwrap() = 200;
  let overflow = OverflowPage::new(&huge_count).unwrap();
  assert_eq!(overflow.value().unwrap_err(), KvError::Corrupted);
  assert_eq!(overflow.pointer_li().unwrap_err(), KvError::Corrupted);
}

/// fmt-04：entry 的 varint 只允许落在 entry 区内，越界报错而不是读到 restart 数组。
#[test]
fn entry_varint_beyond_restart_offset_is_rejected() {
  let mut builder = DataPageBuilder::new(512, 16).unwrap();
  builder
    .add(EntrySpec {
      key: b"key",
      value: b"value",
      ts: 1,
      expire_ts: 0,
      overflow: false,
      compression: CompressionType::None,
      large_value: false,
    })
    .unwrap();
  let mut page = builder.finish().unwrap();
  // 把 entry 起始的第一个字节改成 0x80（varint 续位），迫使 varint 读进 restart 区。
  let restart = data_restart_offset(&page).unwrap();
  assert!(restart > 19);
  page[19] = 0x80;
  let mut iter = DataPageIter::new(&page).unwrap();
  assert!(iter.advance().is_err());
}

/// restart 数组结构校验：首项必须等于页头、其余严格递增，data / index 迭代器
/// 与 region 切分共用同一口径，畸形页直接 `Corrupted`。
#[test]
fn malformed_restart_offsets_are_rejected() {
  let mut data_builder = DataPageBuilder::new(1024, 2).unwrap();
  for key in [b"a".as_slice(), b"b", b"c", b"d"] {
    assert!(
      data_builder
        .add(EntrySpec {
          key,
          value: b"v",
          ts: 1,
          expire_ts: 0,
          overflow: false,
          compression: CompressionType::None,
          large_value: false,
        })
        .unwrap()
    );
  }
  let data = data_builder.finish().unwrap();
  assert!(DataPageIter::new(&data).is_ok());

  let mut index_builder = IndexPageBuilder::new(1024, 2, true).unwrap();
  for (key, page_id) in [(b"a".as_slice(), 1u32), (b"b", 2), (b"c", 3), (b"d", 4)] {
    assert!(index_builder.add(key, page_id).unwrap());
  }
  let index = index_builder.finish().unwrap();
  assert!(IndexPageIter::new(&index).is_ok());

  let data_restart = data_restart_offset(&data).unwrap();
  assert!(data_restart > DATA_HEADER_SIZE);
  assert!(restart_num(&data).unwrap() >= 2);
  let mut bad_head = data.clone();
  bad_head[data_restart..data_restart + 2].copy_from_slice(&0u16.to_le_bytes());
  assert!(DataPageIter::new(&bad_head).is_err());
  let mut non_increasing = data;
  let first = [
    non_increasing[data_restart],
    non_increasing[data_restart + 1],
  ];
  non_increasing[data_restart + 2..data_restart + 4].copy_from_slice(&first);
  assert!(DataPageIter::new(&non_increasing).is_err());

  let index_restart = data_restart_offset(&index).unwrap();
  assert!(index_restart > INDEX_HEADER_SIZE);
  assert!(restart_num(&index).unwrap() >= 2);
  let mut bad_head = index.clone();
  bad_head[index_restart..index_restart + 2].copy_from_slice(&0u16.to_le_bytes());
  assert!(IndexPageIter::new(&bad_head).is_err());
  let mut non_increasing = index;
  let first = [
    non_increasing[index_restart],
    non_increasing[index_restart + 1],
  ];
  non_increasing[index_restart + 2..index_restart + 4].copy_from_slice(&first);
  assert!(IndexPageIter::new(&non_increasing).is_err());
}

/// fmt-03：单 entry 的 index 页迭代不出真实 entry，leftmost 需单独读取
/// （与 C++ 虚拟首条的差异在此显式固化）。
#[test]
fn single_entry_index_page_exposes_leftmost_only() {
  let mut builder = IndexPageBuilder::new(512, 16, true).unwrap();
  // 只加一条真实 entry：restart 数组会被清空。
  builder.add(b"b", 7).unwrap();
  let page = builder.finish().unwrap();
  assert_eq!(restart_num(&page).unwrap(), 0);
  let index = IndexPage::new(&page).unwrap();
  assert_eq!(index.leftmost_ptr().unwrap(), 7);
  let mut iter = IndexPageIter::new(&page).unwrap();
  assert!(!iter.advance().unwrap());
  // ceiling seek 在无 entry 时返回 false；下降语义由 `seek_floor` + leftmost 组合。
  assert!(!iter.seek(b"a").unwrap());
}
