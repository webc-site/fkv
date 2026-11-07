//! 基础写 / 读 / 扫描与 COW 测试。

use super::*;

#[test]
fn write_read_scan() {
  pollster::block_on(async {
    let store = MemStore::new(512);
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let batch_li = [
      BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
      BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
      BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 1),
    ];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &batch_li,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"b", 512)
        .await
        .unwrap(),
      Some(b"B".to_vec())
    );
    let all = scan_range(
      &store,
      ScanArg {
        tbl: &tbl,
        snapshot: &snap(&tbl, &mapper, root),
        tree: ScanTree::Data,
        begin: b"",
        end: b"",
        page_size: 512,
        max_entries: 0,
        dict: None,
        index_cache: None,
        index_flight: None,
        ttl_filter_now_ms: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 3);

    let batch_li = [
      BatchEntry::delete(b"b".to_vec(), 2),
      BatchEntry::upsert(b"d".to_vec(), b"D".to_vec(), 2),
    ];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &batch_li,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"b", 512)
        .await
        .unwrap(),
      None
    );
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"d", 512)
        .await
        .unwrap(),
      Some(b"D".to_vec())
    );
  });
}

/// 同批重复 key 按 timestamp 取较新者（相等取后者），乱序批不被较旧写入覆盖；
/// 空树首写与已有树增量写两条路径都要遵守（perf-79 排序去重）。
#[test]
fn duplicate_keys_keep_newest() {
  pollster::block_on(async {
    let store = MemStore::new(512);
    let tbl = TableIdent::new("dup", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let write = async |root_id: PageId, batch_li: &[BatchEntry], mapper: &mut PageMapper| {
      write_batch(
        &store,
        WriteArg {
          tbl: &tbl,
          root_id,
          mapper,
          batch_li,
          layout: PageLayout::new(512, 16, 16, None, 16),
          segment_mapper: None,
          segment_size: 0,
          ttl_delete_sink: None,
        },
      )
      .await
      .unwrap()
    };
    let first = [
      BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
      BatchEntry::upsert(b"k".to_vec(), b"v1".to_vec(), 1),
      BatchEntry::upsert(b"k".to_vec(), b"v3".to_vec(), 3),
      BatchEntry::upsert(b"k".to_vec(), b"v2".to_vec(), 2),
    ];
    let root = write(PageId::MAX, &first, &mut mapper).await;
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"k", 512)
        .await
        .unwrap(),
      Some(b"v3".to_vec())
    );
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"a", 512)
        .await
        .unwrap(),
      Some(b"A".to_vec())
    );

    // 已有树：较旧的重复项在批内靠后出现，也不得覆盖较新值；删除项同样取较新者。
    let second = [
      BatchEntry::upsert(b"k".to_vec(), b"stale".to_vec(), 2),
      BatchEntry::delete(b"a".to_vec(), 5),
      BatchEntry::upsert(b"a".to_vec(), b"old".to_vec(), 4),
    ];
    let root = write(root, &second, &mut mapper).await;
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"k", 512)
        .await
        .unwrap(),
      Some(b"v3".to_vec())
    );
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"a", 512)
        .await
        .unwrap(),
      None
    );
  });
}

#[test]
fn multi_page_cow() {
  pollster::block_on(async {
    let store = MemStore::new(128);
    let tbl = TableIdent::new("demo", 2).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let mut batch_li = Vec::new();
    for i in 0..100u32 {
      batch_li.push(BatchEntry::upsert(
        format!("k{i:03}").into_bytes(),
        vec![b'v'; 30],
        1,
      ));
    }
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &batch_li,
        layout: PageLayout::new(128, 8, 8, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    let all = scan_range(
      &store,
      ScanArg {
        tbl: &tbl,
        snapshot: &snap(&tbl, &mapper, root),
        tree: ScanTree::Data,
        begin: b"",
        end: b"",
        page_size: 128,
        max_entries: 0,
        dict: None,
        index_cache: None,
        index_flight: None,
        ttl_filter_now_ms: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 100);

    let batch_li = [
      BatchEntry::upsert(b"k050".to_vec(), b"new".to_vec(), 2),
      BatchEntry::delete(b"k000".to_vec(), 2),
    ];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &batch_li,
        layout: PageLayout::new(128, 8, 8, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"k050", 128)
        .await
        .unwrap(),
      Some(b"new".to_vec())
    );
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"k000", 128)
        .await
        .unwrap(),
      None
    );
    let all = scan_range(
      &store,
      ScanArg {
        tbl: &tbl,
        snapshot: &snap(&tbl, &mapper, root),
        tree: ScanTree::Data,
        begin: b"",
        end: b"",
        page_size: 128,
        max_entries: 0,
        dict: None,
        index_cache: None,
        index_flight: None,
        ttl_filter_now_ms: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 99);
  });
}

#[test]
fn cow_writes_only_affected_pages() {
  pollster::block_on(async {
    let store = CountingStore::new(128);
    let tbl = TableIdent::new("demo", 3).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let mut batch_li = Vec::new();
    for i in 0..100u32 {
      batch_li.push(BatchEntry::upsert(
        format!("k{i:03}").into_bytes(),
        vec![b'v'; 30],
        1,
      ));
    }
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &batch_li,
        layout: PageLayout::new(128, 8, 8, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    let initial_writes = store.take_write_count();
    assert!(initial_writes > 10);

    let batch_li = [BatchEntry::upsert(b"k050".to_vec(), b"new".to_vec(), 2)];
    let _root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &batch_li,
        layout: PageLayout::new(128, 8, 8, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    let cow_writes = store.take_write_count();
    assert!(
      cow_writes < initial_writes / 2,
      "cow writes {cow_writes} should be far less than {initial_writes}"
    );
  });
}

#[test]
fn batch_same_page_one_cow() {
  pollster::block_on(async {
    let store = CountingStore::new(512);
    let tbl = TableIdent::new("demo", 4).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let mut batch_li = Vec::new();
    for i in 0..20u32 {
      batch_li.push(BatchEntry::upsert(
        format!("k{i:03}").into_bytes(),
        vec![b'v'; 8],
        1,
      ));
    }
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &batch_li,
        layout: PageLayout::new(512, 8, 8, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    let _ = store.take_write_count();

    let batch_li: Vec<BatchEntry> = (0..5u32)
      .map(|i| BatchEntry::upsert(format!("k{i:03}").into_bytes(), vec![b'x'; 8], 2))
      .collect();
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &batch_li,
        layout: PageLayout::new(512, 8, 8, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    let writes = store.take_write_count();
    assert!(writes <= 8, "grouped batch_li wrote {writes} pages");
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"k003", 512)
        .await
        .unwrap(),
      Some(vec![b'x'; 8])
    );
  });
}
