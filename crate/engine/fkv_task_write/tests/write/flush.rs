//! 跨组 flush 与溢出链预算测试。

use super::*;

#[test]
fn cross_group_writes_flush_once() {
  pollster::block_on(async {
    const PAGE_SIZE: usize = 128;
    let store = CountingStore::new(PAGE_SIZE);
    let tbl = TableIdent::new("agg", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let batch_li: Vec<BatchEntry> = (0..100u32)
      .map(|i| BatchEntry::upsert(format!("k{i:03}").into_bytes(), vec![b'v'; 30], 1))
      .collect();
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &batch_li,
        layout: PageLayout::new(PAGE_SIZE, 8, 8, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    store.take_write_count();
    store.take_batch_count();

    // 跨多个 data 页的更新批：聚合后只应有 1 次 `write_page_li`。
    let batch_li: Vec<BatchEntry> = (0..100u32)
      .step_by(20)
      .map(|i| BatchEntry::upsert(format!("k{i:03}").into_bytes(), vec![b'w'; 16], 2))
      .collect();
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &batch_li,
        layout: PageLayout::new(PAGE_SIZE, 8, 8, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    let page_num = store.take_write_count();
    let batch_num = store.take_batch_count();
    assert!(page_num >= 3, "测试须跨至少 3 个 data 页，实际 {page_num}");
    assert_eq!(
      batch_num, 1,
      "{page_num} 页应聚合成一次写，实际 {batch_num} 次"
    );

    for i in (0..100u32).step_by(20) {
      let value = read_value(
        &store,
        &tbl,
        root,
        &mapper,
        format!("k{i:03}").as_bytes(),
        PAGE_SIZE,
      )
      .await
      .unwrap();
      assert_eq!(value, Some(vec![b'w'; 16]));
    }
  });
}

/// perf-02：一次更新批里多个 leaf index 页同时变脏时，索引页不能逐页独立写；
/// data + index 页应在批末合并成常数次 `write_page_li`（1-2 次）。
#[test]
fn multiple_dirty_index_pages_flush_in_constant_batches() {
  pollster::block_on(async {
    const PAGE_SIZE: usize = 128;
    let store = CountingStore::new(PAGE_SIZE);
    let tbl = TableIdent::new("idx_agg", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let seed: Vec<BatchEntry> = (0..600u32)
      .map(|i| BatchEntry::upsert(format!("k{i:03}").into_bytes(), vec![b'v'; 8], 1))
      .collect();
    let root = write_batch(&store, args(&tbl, PageId::MAX, &mut mapper, &seed))
      .await
      .unwrap();
    let before = index_snapshot(&store, &tbl, &mapper, root, PAGE_SIZE)
      .await
      .unwrap();
    let leaf_cnt = before.iter().filter(|snap| snap.leaf).count();
    assert!(leaf_cnt >= 3, "测试须含多个 leaf index 页，实际 {leaf_cnt}");
    let index_ids: Vec<PageId> = before.iter().map(|snap| snap.page_id).collect();
    mapper.take_changed_page_li();
    store.take_write_count();
    store.take_batch_count();

    // 全部 key 换成更大的 value：每个 data 页都分裂，多个 leaf index 同时变脏。
    // value 取 60B（< 128B 页的 overflow 阈值），保证走分裂而不是 overflow 指针，
    // 否则 data 页子项集合不变、index 不会重建。
    let update: Vec<BatchEntry> = (0..600u32)
      .map(|i| BatchEntry::upsert(format!("k{i:03}").into_bytes(), vec![b'w'; 60], 2))
      .collect();
    let root2 = write_batch(&store, args(&tbl, root, &mut mapper, &update))
      .await
      .unwrap();
    let page_num = store.take_write_count();
    let batch_num = store.take_batch_count();
    let dirty_index_cnt = mapper
      .take_changed_page_li()
      .iter()
      .filter(|page_id| index_ids.contains(page_id))
      .count();
    assert!(
      dirty_index_cnt >= 3,
      "测试须产生多个脏 index 页，实际 {dirty_index_cnt}"
    );
    assert!(
      (1..=2).contains(&batch_num),
      "{page_num} 页 / {dirty_index_cnt} 个脏 index 页应聚合成 1-2 次写，实际 {batch_num} 次"
    );
    assert!(
      batch_num < dirty_index_cnt,
      "索引页仍逐页独立写：{dirty_index_cnt} 个脏页对应 {batch_num} 次 write_page_li"
    );

    let all = scan_range(
      &store,
      ScanArg {
        tbl: &tbl,
        snapshot: &snap(&tbl, &mapper, root2),
        tree: ScanTree::Data,
        begin: b"",
        end: b"",
        page_size: PAGE_SIZE,
        max_entries: 0,
        dict: None,
        index_cache: None,
        index_flight: None,
        ttl_filter_now_ms: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 600, "聚合写不得影响树内容");
  });
}

/// R6：overflow 链必须边构造边按预算落盘。
///
/// 修复前整链先攒在临时 Vec，再整批搬进写缓冲并一次落盘：峰值内存 2×value，
/// 单次 `write_page_li` 直接越过 `PENDING_WRITE_BUDGET`（1 MiB）。
#[test]
fn overflow_chain_flushes_within_write_budget() {
  pollster::block_on(async {
    const PAGE_SIZE: usize = 512;
    const BUDGET: usize = 1 << 20;
    let store = BatchRecordStore::new(PAGE_SIZE);
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let layout = PageLayout::new(PAGE_SIZE, 16, 16, None, 16);
    let value = vec![b'x'; 3 << 20];
    let batch_li = [BatchEntry::upsert(b"big".to_vec(), value.clone(), 1)];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: NULL_PAGE,
        mapper: &mut mapper,
        batch_li: &batch_li,
        layout,
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    let len_li = store.take_batch_len_li();
    assert!(
      len_li.len() > 1,
      "overflow 链必须分段落盘，实际批次: {len_li:?}"
    );
    let max_bytes = len_li.iter().copied().max().unwrap_or(0) * PAGE_SIZE;
    assert!(
      max_bytes <= BUDGET + PAGE_SIZE,
      "单次落盘 {max_bytes} 字节超过预算 {BUDGET}"
    );
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"big", PAGE_SIZE)
        .await
        .unwrap(),
      Some(value)
    );
  });
}
