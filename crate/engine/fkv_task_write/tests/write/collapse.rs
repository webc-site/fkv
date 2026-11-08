//! 树收缩与根塌缩测试。

use super::*;

/// R6：树高收缩时根只剩余一个子页，必须释放旧根页并上提子页。
///
/// 修复前根页被重建成「单子项根页」写盘且不释放，树高永不下降、每次收缩
/// 多占一页。删到只剩一个 data 页后，根应最终折叠成 data 页本身。
#[test]
fn root_collapses_when_tree_shrinks() {
  pollster::block_on(async {
    let store = MemStore::new(256);
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let layout = PageLayout::new(256, 16, 16, None, 16);
    let value = vec![b'v'; 40];
    let batch_li: Vec<BatchEntry> = (0..900u32)
      .map(|i| BatchEntry::upsert(format!("k{i:05}").into_bytes(), value.clone(), 1))
      .collect();
    let mut root = write_batch(
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
    let mut page = vec![0u8; 256];
    let root_fp = mapper.file_page_id(root).unwrap();
    store.read_page(&tbl, root_fp, &mut page).await.unwrap();
    assert_ne!(
      page_type(&page).unwrap(),
      PageType::Data,
      "初始应建成多级树"
    );
    let pages_before = mapper.page_cnt();

    // 分批删除，只留最后两条（同一 data 页）。
    for start in (2..900u32).step_by(100) {
      let delete_li: Vec<BatchEntry> = (start..(start + 100).min(900))
        .map(|i| BatchEntry::delete(format!("k{i:05}").into_bytes(), 2))
        .collect();
      root = write_batch(
        &store,
        WriteArg {
          tbl: &tbl,
          root_id: root,
          mapper: &mut mapper,
          batch_li: &delete_li,
          layout,
          segment_mapper: None,
          segment_size: 0,
          ttl_delete_sink: None,
        },
      )
      .await
      .unwrap();
    }
    let root_fp = mapper.file_page_id(root).unwrap();
    store.read_page(&tbl, root_fp, &mut page).await.unwrap();
    // 修复前根页只会被原地重建成「单子项根页」，层级永不下降（仍是
    // NonLeafIndex）；修复后最后一轮收缩把 non-leaf 根折叠成 leaf index。
    assert_eq!(
      page_type(&page).unwrap(),
      PageType::LeafIndex,
      "收缩后树高必须下降（non-leaf 根折叠到 leaf index）"
    );
    let mut child_cnt = 1; // leftmost 指针
    let mut iter = IndexPageIter::new(&page).unwrap();
    while iter.advance().unwrap() {
      child_cnt += 1;
    }
    assert_eq!(child_cnt, 1, "折叠后的根应只指向一个子页");
    assert!(
      mapper.page_cnt() <= pages_before,
      "收缩不得累积新页: before={pages_before} after={}",
      mapper.page_cnt()
    );
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"k00000", 256)
        .await
        .unwrap(),
      Some(value)
    );
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"k00001", 256)
        .await
        .unwrap(),
      Some(vec![b'v'; 40])
    );
  });
}

/// r7a-core-03：根是「唯一子页为 data 页的 leaf index」时，改写该 data 页
/// 不得把根折叠成 data 页。
///
/// data 根与「单 data 子的 leaf index 根」语义等价：折叠后下一次写走
/// `route.data_page` 分支，又会由 `build_new_index` 重新包一层 leaf index 根，
/// 每批白烧一页。这里连写两批，根类型必须一直是 leaf index 且不往返。
#[test]
fn leaf_root_with_single_data_child_is_not_collapsed() {
  pollster::block_on(async {
    let store = MemStore::new(256);
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let layout = PageLayout::new(256, 16, 16, None, 16);
    let value = vec![b'v'; 40];
    async fn read_root_type(
      store: &MemStore,
      tbl: &TableIdent,
      mapper: &PageMapper,
      root: PageId,
    ) -> PageType {
      let mut page = vec![0u8; 256];
      store
        .read_page(tbl, mapper.file_page_id(root).unwrap(), &mut page)
        .await
        .unwrap();
      page_type(&page).unwrap()
    }
    let upsert_li: Vec<BatchEntry> = (0..12u32)
      .map(|i| BatchEntry::upsert(format!("k{i:05}").into_bytes(), value.clone(), 1))
      .collect();
    let mut root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: NULL_PAGE,
        mapper: &mut mapper,
        batch_li: &upsert_li,
        layout,
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(
      read_root_type(&store, &tbl, &mapper, root).await,
      PageType::LeafIndex,
      "小表根应是单层 leaf index"
    );

    // 删到只剩同一 data 页上的两条：其余 data 页被释放，根只剩一个子页。
    let delete_li: Vec<BatchEntry> = (0..10u32)
      .map(|i| BatchEntry::delete(format!("k{i:05}").into_bytes(), 2))
      .collect();
    root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &delete_li,
        layout,
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(
      read_root_type(&store, &tbl, &mapper, root).await,
      PageType::LeafIndex,
      "收缩到单 data 子页后根仍是 leaf index（修复前折叠成 data 根）"
    );

    // 再写一批：改写唯一 data 页，根类型不得往返。
    let new_value = vec![b'x'; 40];
    let put_li = [BatchEntry::upsert(b"k00010".to_vec(), new_value.clone(), 3)];
    root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &put_li,
        layout,
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(
      read_root_type(&store, &tbl, &mapper, root).await,
      PageType::LeafIndex,
      "leaf 根不得折叠成 data 根（会与下一批的重新包装往返）"
    );
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"k00010", 256)
        .await
        .unwrap(),
      Some(new_value)
    );
  });
}
