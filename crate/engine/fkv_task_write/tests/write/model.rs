//! 随机批次模型与树清空测试。

use super::*;

/// 随机 upsert / delete 连打多批：分裂、删空、多层 index 生长/收缩后，
/// 点读与全量扫描必须与内存模型完全一致（覆盖 B+Tree 路由不变量）。
#[test]
fn random_batches_match_model() {
  pollster::block_on(async {
    let store = MemStore::new(128);
    let tbl = TableIdent::new("demo", 14).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let mut root = PageId::MAX;
    let mut model: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut ts = 0u64;
    for round in 0..32 {
      let mut batch_li = Vec::new();
      for _ in 0..16 {
        ts += 1;
        let key = format!("k{:03}", rng.below(150)).into_bytes();
        if rng.below(4) == 0 {
          batch_li.push(BatchEntry::delete(key.clone(), ts));
          model.remove(&key);
        } else {
          let len = 20 + rng.below(60);
          let value = vec![b'a' + u8::try_from(rng.below(26)).unwrap(); len];
          batch_li.push(BatchEntry::upsert(key.clone(), value.clone(), ts));
          model.insert(key, value);
        }
      }
      root = write_batch(&store, args(&tbl, root, &mut mapper, &batch_li))
        .await
        .unwrap();
      // 全量扫描：顺序、内容、页链都必须与模型一致。
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
      let expected: Vec<(Vec<u8>, Vec<u8>)> = model
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
      assert_eq!(all, expected, "round {round} 扫描结果不一致");
      // 点读抽样：删掉/改过的 key 也必须按新分隔键路由到正确 data 页。
      for i in (round * 5)..(round * 5 + 12) {
        let key = format!("k{:03}", i % 150).into_bytes();
        let got = read_value(&store, &tbl, root, &mapper, &key, 128)
          .await
          .unwrap();
        assert_eq!(got, model.get(&key).cloned(), "round {round} key {key:?}");
      }
    }
    assert!(!model.is_empty());
  });
}

/// 全删后树塌缩为空：index 页逐层释放至 root = `PageId::MAX`，再重建仍可用。
#[test]
fn delete_all_collapses_tree_to_empty() {
  pollster::block_on(async {
    let store = MemStore::new(128);
    let tbl = TableIdent::new("demo", 15).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let build: Vec<BatchEntry> = (0..60u32)
      .map(|i| BatchEntry::upsert(format!("k{i:03}").into_bytes(), vec![b'v'; 30], 1))
      .collect();
    let mut root = write_batch(&store, args(&tbl, PageId::MAX, &mut mapper, &build))
      .await
      .unwrap();
    assert_ne!(root, PageId::MAX);
    // 分两批删光：覆盖「单批内整页清空 + index 页子项清空」两条塌缩路径。
    for (start, end) in [(0u32, 30u32), (30, 60)] {
      let batch_li: Vec<BatchEntry> = (start..end)
        .map(|i| BatchEntry::delete(format!("k{i:03}").into_bytes(), 2))
        .collect();
      root = write_batch(&store, args(&tbl, root, &mut mapper, &batch_li))
        .await
        .unwrap();
    }
    assert_eq!(root, PageId::MAX, "全删后 root 应为空");
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
    assert!(all.is_empty());
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"k001", 128)
        .await
        .unwrap(),
      None
    );
    // 空树重建。
    let rebuild: Vec<BatchEntry> = (0..20u32)
      .map(|i| BatchEntry::upsert(format!("n{i:03}").into_bytes(), vec![b'n'; 30], 3))
      .collect();
    root = write_batch(&store, args(&tbl, root, &mut mapper, &rebuild))
      .await
      .unwrap();
    assert_ne!(root, PageId::MAX);
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
    assert_eq!(all.len(), 20);
  });
}
