//! fmt-01/05：时间戳 clamp 与乱序 key 显式报错。
use fkv_error::KvError;
use fkv_page::{CompressionType, DataPageBuilder, DataPageIter, EntrySpec, IndexPageBuilder};

#[test]
fn timestamp_clamps_to_i64_max() {
  let mut builder = DataPageBuilder::new(512, 16).unwrap();
  builder
    .add(EntrySpec {
      key: b"k".as_slice(),
      value: b"v".as_slice(),
      ts: u64::MAX,
      expire_ts: 0,
      overflow: false,
      compression: CompressionType::None,
      large_value: false,
    })
    .unwrap();
  let page = builder.finish().unwrap();
  let mut iter = DataPageIter::new(&page).unwrap();
  assert!(iter.advance().unwrap());
  // 对齐 C++ ClampTimestampToInt64：u64 超界饱和到 i64::MAX。
  assert_eq!(iter.timestamp(), i64::MAX as u64);

  // 第二条 delta 以 clamp 后的值为基准，编码可正确解回。
  let mut builder = DataPageBuilder::new(512, 16).unwrap();
  builder
    .add(EntrySpec {
      key: b"a".as_slice(),
      value: b"v".as_slice(),
      ts: u64::MAX,
      expire_ts: 0,
      overflow: false,
      compression: CompressionType::None,
      large_value: false,
    })
    .unwrap();
  builder
    .add(EntrySpec {
      key: b"b".as_slice(),
      value: b"v".as_slice(),
      ts: (i64::MAX as u64) - 5,
      expire_ts: 0,
      overflow: false,
      compression: CompressionType::None,
      large_value: false,
    })
    .unwrap();
  let page = builder.finish().unwrap();
  let mut iter = DataPageIter::new(&page).unwrap();
  assert!(iter.advance().unwrap());
  assert_eq!(iter.timestamp(), i64::MAX as u64);
  assert!(iter.advance().unwrap());
  assert_eq!(iter.timestamp(), (i64::MAX - 5) as u64);
}

#[test]
fn out_of_order_add_is_rejected() {
  let mut builder = DataPageBuilder::new(512, 16).unwrap();
  builder
    .add(EntrySpec {
      key: b"b".as_slice(),
      value: b"v".as_slice(),
      ts: 1,
      expire_ts: 0,
      overflow: false,
      compression: CompressionType::None,
      large_value: false,
    })
    .unwrap();
  for key in [b"a".as_slice(), b"b".as_slice()] {
    assert_eq!(
      builder
        .add(EntrySpec {
          key,
          value: b"v".as_slice(),
          ts: 1,
          expire_ts: 0,
          overflow: false,
          compression: CompressionType::None,
          large_value: false,
        })
        .unwrap_err(),
      KvError::InvalidArgs
    );
  }

  let mut index = IndexPageBuilder::new(512, 16, true).unwrap();
  index.add(b"".as_slice(), 1).unwrap();
  index.add(b"b".as_slice(), 2).unwrap();
  assert_eq!(
    index.add(b"a".as_slice(), 3).unwrap_err(),
    KvError::InvalidArgs
  );
}

/// fmt-06：builder 的 `finish_without_checksum` 与 C++ `Finish` 一致（checksum 为 0）。
#[test]
fn builder_checksum_free_output_matches_cpp_finish() {
  let with = {
    let mut builder = DataPageBuilder::new(512, 16).unwrap();
    builder
      .add(EntrySpec {
        key: b"k",
        value: b"v",
        ts: 1,
        expire_ts: 0,
        overflow: false,
        compression: CompressionType::None,
        large_value: false,
      })
      .unwrap();
    builder.finish().unwrap()
  };
  let without = {
    let mut builder = DataPageBuilder::new(512, 16).unwrap();
    builder
      .add(EntrySpec {
        key: b"k",
        value: b"v",
        ts: 1,
        expire_ts: 0,
        overflow: false,
        compression: CompressionType::None,
        large_value: false,
      })
      .unwrap();
    builder.finish_without_checksum().unwrap()
  };
  assert_eq!(&without[..8], &[0u8; 8]);
  assert_eq!(&with[8..], &without[8..], "除 checksum 外应逐字节一致");

  let without = {
    let mut builder = IndexPageBuilder::new(512, 16, true).unwrap();
    builder.add(b"", 1).unwrap();
    builder.add(b"k", 2).unwrap();
    builder.finish_without_checksum().unwrap()
  };
  assert_eq!(&without[..8], &[0u8; 8]);

  let overflow = fkv_page::build_overflow_without_checksum(512, b"value", &[], 16).unwrap();
  assert_eq!(&overflow[..8], &[0u8; 8]);
}

/// fmt-07：overflow 估算与 C++ `IsOverflowKV` 同口径。
#[test]
fn overflow_kv_estimate_matches_page_capacity() {
  use fkv_page::is_overflow_kv;
  // 明显超过单页。
  assert!(is_overflow_kv(b"k", 600, 1, 0, 512));
  // 小 value 不拆。
  assert!(!is_overflow_kv(b"k", 10, 1, 0, 512));
  // 接近边界时仍能内联（留出 header + restart 预留）。
  assert!(!is_overflow_kv(b"k", 400, 1, 0, 512));
}

/// fmt-20：构造缓冲只初始化头部，尾部 padding 由 `finish` 的 resize 补零。
#[test]
fn builder_padding_is_zero_filled() {
  let page = {
    let mut builder = DataPageBuilder::new(512, 16).unwrap();
    builder
      .add(EntrySpec {
        key: b"k",
        value: b"v",
        ts: 1,
        expire_ts: 0,
        overflow: false,
        compression: CompressionType::None,
        large_value: false,
      })
      .unwrap();
    builder.finish().unwrap()
  };
  let end = usize::from(fkv_page::content_len(&page).unwrap());
  assert!(end < page.len());
  assert!(
    page[end..].iter().all(|&b| b == 0),
    "data 页 padding 必须为 0"
  );

  let page = {
    let mut builder = IndexPageBuilder::new(512, 16, true).unwrap();
    builder.add(b"", 1).unwrap();
    builder.add(b"k", 2).unwrap();
    builder.finish().unwrap()
  };
  let end = usize::from(fkv_page::content_len(&page).unwrap());
  assert!(end < page.len());
  assert!(
    page[end..].iter().all(|&b| b == 0),
    "index 页 padding 必须为 0"
  );
}

/// fmt-21：非首条 index entry 的 page_id 超 i32 域显式报错（C++ 侧 `assert(page_id <= INT32_MAX)`）。
#[test]
fn index_page_id_beyond_i32_is_rejected() {
  let mut builder = IndexPageBuilder::new(512, 16, true).unwrap();
  builder.add(b"", 1).unwrap();
  assert_eq!(
    builder.add(b"k", i32::MAX as u32 + 1).unwrap_err(),
    KvError::InvalidArgs
  );
  // 边界内仍可编解码。
  assert!(builder.add(b"k", i32::MAX as u32).unwrap());
}
