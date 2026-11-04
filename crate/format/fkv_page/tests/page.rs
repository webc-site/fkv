use fkv_page::{
  CompressionType, DataPageBuilder, DataPageIter, IndexPage, IndexPageBuilder, IndexPageIter,
  PageType, content_len, data_restart_offset,
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
        .add(
          key,
          value,
          overflow,
          ts,
          expire,
          CompressionType::None,
          large
        )
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
    .add(b"a", b"b", false, 0, 0, CompressionType::None, false)
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
      .add(key, b"v", false, 0, 0, CompressionType::None, false)
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
