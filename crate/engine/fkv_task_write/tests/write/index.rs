//! 索引页更新、批量路由与删除重平衡测试。

use super::*;

#[test]
fn delete_many_rebalances_index() {
  pollster::block_on(async {
    let store = MemStore::new(128);
    let tbl = TableIdent::new("demo", 5).unwrap();
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
    let batch_li: Vec<BatchEntry> = (0..90u32)
      .map(|i| BatchEntry::delete(format!("k{i:03}").into_bytes(), 2))
      .collect();
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
        ttl_filter_now_ms: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 10);
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"k095", 128)
        .await
        .unwrap(),
      Some(vec![b'v'; 30])
    );
  });
}

/// issue 29：写批次按 key 走一次有序路由，不再为每个 entry 从 root 各下降一次。
/// 100 条更新跨约 20 个 data page、树高 <= 3；读次数应远低于逐 entry 下降。
#[test]
fn batch_route_does_one_descent_not_per_entry() {
  pollster::block_on(async {
    let store = MemStore::new(128);
    let tbl = TableIdent::new("demo", 9).unwrap();
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
        layout: PageLayout::new(128, 8, 8, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();

    let before = store.read_num();
    let batch_li: Vec<BatchEntry> = (0..100u32)
      .map(|i| BatchEntry::upsert(format!("k{i:03}").into_bytes(), vec![b'w'; 30], 2))
      .collect();
    write_batch(
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
    let reads = store.read_num() - before;
    // 逐 entry 预下降约 100*树高 + 每组下降，合计 >= 400；
    // 单次有序路由 + 每组一次下降约 groups*(树高) ≈ 150。
    assert!(
      reads < 200,
      "page reads = {reads}, expected batch_li routing"
    );
  });
}

/// issue 49：原位覆盖（不分裂、不删空）不改子项集合，index 层必须零写入。
#[test]
fn in_place_update_rewrites_no_index_page() {
  pollster::block_on(async {
    let store = CountingStore::new(128);
    let tbl = TableIdent::new("demo", 11).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let batch_li: Vec<BatchEntry> = (0..120u32)
      .map(|i| BatchEntry::upsert(format!("k{i:03}").into_bytes(), vec![b'v'; 30], 1))
      .collect();
    let root = write_batch(&store, args(&tbl, PageId::MAX, &mut mapper, &batch_li))
      .await
      .unwrap();
    let before = index_snapshot(&store, &tbl, &mapper, root, 128)
      .await
      .unwrap();
    assert!(before.len() >= 2, "需要多层 index：{}", before.len());
    let index_ids: Vec<PageId> = before.iter().map(|snap| snap.page_id).collect();
    mapper.take_changed_page_li();
    let _ = store.take_write_count();

    // 等长 value 原位覆盖：不触发分裂，也不清空任何 data 页。
    let batch_li: Vec<BatchEntry> = (0..120u32)
      .step_by(5)
      .map(|i| BatchEntry::upsert(format!("k{i:03}").into_bytes(), vec![b'w'; 30], 2))
      .collect();
    let group_num = batch_li.len();
    let root2 = write_batch(&store, args(&tbl, root, &mut mapper, &batch_li))
      .await
      .unwrap();
    let writes = store.take_write_count();
    let changed = mapper.take_changed_page_li();
    let index_changed = changed
      .iter()
      .filter(|page_id| index_ids.contains(page_id))
      .count();
    assert_eq!(root2, root, "index 未变时 root 逻辑页不应变");
    assert_eq!(
      index_changed, 0,
      "原位更新重写了 {index_changed} 次 index 页"
    );
    assert!(
      writes <= group_num,
      "writes={writes} 只应是 data 页写入（<= {group_num}）"
    );
    let after = index_snapshot(&store, &tbl, &mapper, root2, 128)
      .await
      .unwrap();
    for (left, right) in before.iter().zip(after.iter()) {
      assert_eq!(left.page_id, right.page_id);
      assert_eq!(left.file_page_id, right.file_page_id, "index 页被重写");
      assert_eq!(left.child_li, right.child_li, "index 子项不应变化");
    }
    for i in 0..120u32 {
      let key = format!("k{i:03}");
      let value = read_value(&store, &tbl, root2, &mapper, key.as_bytes(), 128)
        .await
        .unwrap();
      let expected = if i % 5 == 0 {
        vec![b'w'; 30]
      } else {
        vec![b'v'; 30]
      };
      assert_eq!(value, Some(expected), "key {key}");
    }
  });
}

/// issue 49：同一父 index 下多组 data 页变更，父页只重建一次。
#[test]
fn grouped_batch_rebuilds_parent_index_once() {
  pollster::block_on(async {
    let store = CountingStore::new(256);
    let tbl = TableIdent::new("demo", 12).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let build: Vec<BatchEntry> = (0..24u32)
      .map(|i| BatchEntry::upsert(format!("k{i:03}").into_bytes(), vec![b'v'; 20], 1))
      .collect();
    // 手写参数：page_size 256，使 24 条落在同一 leaf index（单层树）下。
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &build,
        layout: PageLayout::new(256, 8, 8, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    let before = index_snapshot(&store, &tbl, &mapper, root, 256)
      .await
      .unwrap();
    assert_eq!(before.len(), 1, "应是单层树");
    assert!(before[0].leaf && before[0].child_li.len() >= 3);
    let leaf_id = before[0].page_id;
    mapper.take_changed_page_li();

    let before_reads = store.inner.read_num();
    // 全部 key 换成更大的 value：每个 data 页都会分裂，即每组都要改父 index。
    let update: Vec<BatchEntry> = (0..24u32)
      .map(|i| BatchEntry::upsert(format!("k{i:03}").into_bytes(), vec![b'w'; 100], 2))
      .collect();
    let root2 = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &update,
        layout: PageLayout::new(256, 8, 8, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    let reads = store.inner.read_num() - before_reads;
    let leaf_changed = mapper
      .take_changed_page_li()
      .iter()
      .filter(|page_id| **page_id == leaf_id)
      .count();
    // 单层树：一次路由 + 每组一次 data 页读；旧实现每组再下降一次。
    assert!(reads <= 12, "page reads = {reads}");
    assert_eq!(leaf_changed, 1, "父 leaf index 应只重建一次");
    for i in 0..24u32 {
      let key = format!("k{i:03}");
      assert_eq!(
        read_value(&store, &tbl, root2, &mapper, key.as_bytes(), 256)
          .await
          .unwrap(),
        Some(vec![b'w'; 100]),
        "key {key}"
      );
    }
    let all = scan_range(
      &store,
      ScanArg {
        tbl: &tbl,
        snapshot: &snap(&tbl, &mapper, root2),
        tree: ScanTree::Data,
        begin: b"",
        end: b"",
        page_size: 256,
        max_entries: 0,
        dict: None,
        ttl_filter_now_ms: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 24);
  });
}

/// eng-53：空表批量导入变长 value 时，data 页同样做四分页重分配；端到端读回一致。
#[test]
fn bulk_import_empty_store_variable_values() {
  pollster::block_on(async {
    let store = MemStore::new(128);
    let tbl = TableIdent::new("demo", 17).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let len_of = |i: u32| if i % 3 == 0 { 1usize } else { 30 };
    let batch_li: Vec<BatchEntry> = (0..200u32)
      .map(|i| BatchEntry::upsert(format!("k{i:03}").into_bytes(), vec![b'v'; len_of(i)], 1))
      .collect();
    let root = write_batch(&store, args(&tbl, PageId::MAX, &mut mapper, &batch_li))
      .await
      .unwrap();
    for i in 0..200u32 {
      let key = format!("k{i:03}");
      assert_eq!(
        read_value(&store, &tbl, root, &mapper, key.as_bytes(), 128)
          .await
          .unwrap(),
        Some(vec![b'v'; len_of(i)]),
        "key {key}"
      );
    }
  });
}

/// issue 49：删掉 data 页首条 key 时不得抬高父 index 分隔键（否则破坏路由）。
#[test]
fn delete_page_first_entry_keeps_parent_separator() {
  pollster::block_on(async {
    let store = CountingStore::new(128);
    let tbl = TableIdent::new("demo", 13).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let build: Vec<BatchEntry> = (0..60u32)
      .map(|i| BatchEntry::upsert(format!("k{i:03}").into_bytes(), vec![b'v'; 30], 1))
      .collect();
    let root = write_batch(&store, args(&tbl, PageId::MAX, &mut mapper, &build))
      .await
      .unwrap();
    let before = index_snapshot(&store, &tbl, &mapper, root, 128)
      .await
      .unwrap();
    let leaf = before
      .iter()
      .find(|snap| snap.leaf && snap.child_li.len() >= 3)
      .expect("需要含多个子页的 leaf index");
    let (leaf_id, leaf_fp) = (leaf.page_id, leaf.file_page_id);
    let (target_page, sep) = leaf.child_li[2].clone();
    assert!(!sep.is_empty() && target_page != leaf.child_li[0].0);
    let index_ids: Vec<PageId> = before.iter().map(|snap| snap.page_id).collect();
    mapper.take_changed_page_li();

    let removed = sep.clone();
    let batch_li = [BatchEntry::delete(removed.clone(), 2)];
    let root2 = write_batch(&store, args(&tbl, root, &mut mapper, &batch_li))
      .await
      .unwrap();
    assert_eq!(root2, root, "index 未变时 root 不应变");
    let index_changed = mapper
      .take_changed_page_li()
      .iter()
      .filter(|page_id| index_ids.contains(page_id))
      .count();
    assert_eq!(index_changed, 0, "删首条 key 不应重写 index 页");
    assert_eq!(mapper.file_page_id(leaf_id).unwrap(), leaf_fp);
    let after = index_snapshot(&store, &tbl, &mapper, root2, 128)
      .await
      .unwrap();
    assert_eq!(
      before
        .iter()
        .map(|snap| (snap.page_id, snap.file_page_id, snap.child_li.clone()))
        .collect::<Vec<_>>(),
      after
        .iter()
        .map(|snap| (snap.page_id, snap.file_page_id, snap.child_li.clone()))
        .collect::<Vec<_>>(),
    );
    // 被删 key 读不到；其余 key 仍能按原分隔键路由到各自 data 页。
    for i in 0..60u32 {
      let key = format!("k{i:03}").into_bytes();
      let expected = if key == removed {
        None
      } else {
        Some(vec![b'v'; 30])
      };
      assert_eq!(
        read_value(&store, &tbl, root2, &mapper, &key, 128)
          .await
          .unwrap(),
        expected,
        "key {key:?}"
      );
    }
    // 删出的空洞可以重新写入，且落在原 data 页范围内。
    let redo = [BatchEntry::upsert(removed.clone(), vec![b'x'; 30], 3)];
    let root3 = write_batch(&store, args(&tbl, root2, &mut mapper, &redo))
      .await
      .unwrap();
    assert_eq!(
      read_value(&store, &tbl, root3, &mapper, &removed, 128)
        .await
        .unwrap(),
      Some(vec![b'x'; 30])
    );
    let all = scan_range(
      &store,
      ScanArg {
        tbl: &tbl,
        snapshot: &snap(&tbl, &mapper, root3),
        tree: ScanTree::Data,
        begin: b"",
        end: b"",
        page_size: 128,
        max_entries: 0,
        dict: None,
        ttl_filter_now_ms: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 60);
  });
}
