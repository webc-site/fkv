use fkv_page::{
  CompressionType, DATA_HEADER_SIZE, DataPageBuilder, EntrySpec, FastIndexPageBuilder,
  INDEX_HEADER_SIZE, IndexPageBuilder, IndexPageIter, PageRegionIter, content_len,
  data_restart_offset, region_entry_cnt, restart_num, validate_checksum,
};

/// 逐条编码造一个 index 页（leftmost=1，其余 key→递增 page_id）。
fn index_page(keys: &[Vec<u8>], leaf: bool, page_size: usize, restart_interval: u16) -> Vec<u8> {
  let mut builder = IndexPageBuilder::new(page_size, restart_interval, leaf).unwrap();
  assert!(builder.add(b"", 1).unwrap());
  for (idx, key) in keys.iter().enumerate() {
    assert!(builder.add(key, u32::try_from(idx + 2).unwrap()).unwrap());
  }
  builder.finish().unwrap()
}

fn keys(n: usize) -> Vec<Vec<u8>> {
  (0..n).map(|i| format!("key{i:05}").into_bytes()).collect()
}

fn iter_keys(page: &[u8]) -> Vec<Vec<u8>> {
  let mut iter = IndexPageIter::new(page).unwrap();
  let mut out = Vec::new();
  while iter.advance().unwrap() {
    out.push(iter.key().to_vec());
  }
  out
}

/// region 拼回 == 页头到 restart 数组之间的原始 entry 字节；每个 region 以 `shared=0` 起始。
#[test]
fn regions_partition_entry_area() {
  for (leaf, restart_interval) in [(true, 4u16), (false, 4), (true, 8), (false, 16)] {
    let page = index_page(&keys(60), leaf, 4096, restart_interval);
    let end = data_restart_offset(&page).unwrap();
    let mut joined = Vec::new();
    let mut cnt = 0;
    for region in PageRegionIter::new(&page).unwrap() {
      assert!(!region.is_empty());
      assert_eq!(region[0], 0, "region 必须以 restart（shared=0）起始");
      joined.extend_from_slice(region);
      cnt += 1;
    }
    assert!(cnt >= 2, "多 restart 才有多个 region：{cnt}");
    assert_eq!(joined, page[INDEX_HEADER_SIZE..end]);
  }
}

/// data 页同样可按 restart 数组切 region。
#[test]
fn data_page_regions_partition_entry_area() {
  let mut builder = DataPageBuilder::new(1024, 4).unwrap();
  for idx in 0..40u32 {
    assert!(
      builder
        .add(EntrySpec {
          key: format!("k{idx:04}").as_bytes(),
          value: b"value",
          ts: u64::from(idx),
          expire_ts: 0,
          overflow: false,
          compression: CompressionType::None,
          large_value: false,
        })
        .unwrap()
    );
  }
  let page = builder.finish().unwrap();
  let end = data_restart_offset(&page).unwrap();
  let mut joined = Vec::new();
  let mut cnt = 0;
  for region in PageRegionIter::new(&page).unwrap() {
    assert_eq!(region[0], 0);
    joined.extend_from_slice(region);
    cnt += 1;
  }
  assert!(cnt >= 2, "restart_num={}", restart_num(&page).unwrap());
  assert_eq!(joined, page[DATA_HEADER_SIZE..end]);
}

/// region 直拷重建的 index 页与逐条重编码的产物逐字节一致（含 checksum）。
#[test]
fn region_rebuild_is_byte_identical() {
  for (leaf, restart_interval) in [(true, 4u16), (false, 8), (true, 16), (false, 3)] {
    let page = index_page(&keys(50), leaf, 2048, restart_interval);
    let mut builder = FastIndexPageBuilder::from_page(&page).unwrap();
    for region in PageRegionIter::new(&page).unwrap() {
      assert!(builder.add_region(region).unwrap());
    }
    let rebuilt = builder.finish().unwrap();
    assert_eq!(rebuilt, page, "leaf={leaf} ri={restart_interval}");
    assert!(validate_checksum(&rebuilt).unwrap());
    assert_eq!(iter_keys(&rebuilt), iter_keys(&page));
    assert_eq!(
      usize::from(content_len(&rebuilt).unwrap()),
      usize::from(content_len(&page).unwrap())
    );
  }
}

/// 空 index 页（只有 leftmost）region 数为 0，快速重建仍与原页一致。
#[test]
fn empty_index_page_has_no_regions() {
  let page = index_page(&[], true, 512, 8);
  assert_eq!(restart_num(&page).unwrap(), 0);
  assert_eq!(PageRegionIter::new(&page).unwrap().count(), 0);
  let rebuilt = FastIndexPageBuilder::from_page(&page)
    .unwrap()
    .finish()
    .unwrap();
  assert_eq!(rebuilt, page);
}

/// 容量不足时 `add_region` 返回 `false`，不修改已累积的页。
#[test]
fn add_region_enforces_capacity() {
  // 页头 15 + region 10 + restart 2 + 长度 2 = 29 ≤ 32。
  let mut builder = FastIndexPageBuilder::new(32, true).unwrap();
  builder.set_leftmost(7).unwrap();
  assert!(builder.add_region(&[0u8; 10]).unwrap());
  // 再加会超：29 + 2 + 2 + 2 = 35 > 32。
  assert!(!builder.add_region(&[0u8; 2]).unwrap());
  let page = builder.finish().unwrap();
  assert_eq!(usize::from(content_len(&page).unwrap()), 15 + 10 + 2 + 2);
  // 被拒的 region 未写入：仅 1 个 region。
  assert_eq!(PageRegionIter::new(&page).unwrap().count(), 1);
}

/// `region_entry_cnt` 与 region 内实际 entry 条数一致（逐 region 合计 = 总 entry 数）。
#[test]
fn region_entry_cnt_matches_entry_count() {
  let page = index_page(&keys(50), true, 2048, 4);
  let mut total = 0usize;
  for region in PageRegionIter::new(&page).unwrap() {
    total += usize::from(region_entry_cnt(region).unwrap());
  }
  assert_eq!(total, 50);

  // `encode_region` 造出的 region 也自描述。
  let mut encoder = IndexPageBuilder::new(2048, 4, true).unwrap();
  let entries = [
    (b"a".as_slice(), 1u32),
    (b"b".as_slice(), 2),
    (b"c".as_slice(), 3),
  ];
  let region = encoder.encode_region(&entries).unwrap();
  assert_eq!(region_entry_cnt(region).unwrap(), 3);
}

/// 非法 restart 数组（非递增 / 首项不等于页头）在构造期被拒。
#[test]
fn rejects_corrupt_restart_array() {
  let page = index_page(&keys(40), true, 2048, 4);
  let off = data_restart_offset(&page).unwrap();

  let mut non_increasing = page.clone();
  let first = [non_increasing[off], non_increasing[off + 1]];
  non_increasing[off + 2..off + 4].copy_from_slice(&first);
  assert!(PageRegionIter::new(&non_increasing).is_err());

  let mut bad_head = page.clone();
  bad_head[off..off + 2].copy_from_slice(&[0, 0]);
  assert!(PageRegionIter::new(&bad_head).is_err());
}

/// 空 data 页（无 entry，restart 数组只含页头 offset）归一为 0 个 region。
#[test]
fn empty_data_page_has_no_regions() {
  let page = DataPageBuilder::new(256, 8).unwrap().finish().unwrap();
  assert_eq!(restart_num(&page).unwrap(), 1);
  assert_eq!(data_restart_offset(&page).unwrap(), DATA_HEADER_SIZE);
  assert_eq!(PageRegionIter::new(&page).unwrap().count(), 0);
}

/// region 首条不是 restart（`shared != 0`）时拒绝直拷，避免拼出非法页。
#[test]
fn add_region_rejects_non_restart_bytes() {
  let mut builder = FastIndexPageBuilder::new(512, true).unwrap();
  // varint shared=1 → 非 restart 边界。
  assert!(builder.add_region(&[1, 1, b'k', 0]).is_err());
  assert!(builder.add_region(&[]).is_err());
  // 合法 restart region 仍可写入。
  assert!(builder.add_region(&[0, 1, b'k', 0]).unwrap());
}
