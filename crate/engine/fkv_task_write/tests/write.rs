use core::mem::take;

use fkv_btree::BatchEntry;
use fkv_io::PageIo;
use fkv_io_mem::MemStore;
use fkv_mapping::{AllocMode, MappingSnapshot, PageMapper, SnapshotRegistry, SnapshotSource};
use fkv_task_read::{ReadCtx, read_entry};
use fkv_task_scan::{ScanArg, ScanTree, scan_range};
use fkv_task_write::{PageLayout, WriteArg, write_batch};
use fkv_types::{NULL_PAGE, PageId, TableIdent};

/// 测试用快照：登记表只存弱引用，临时 registry 丢弃后快照仍有效。
fn snap(tbl: &TableIdent, mapper: &PageMapper, root: PageId) -> MappingSnapshot {
  SnapshotRegistry::new().open(
    tbl,
    SnapshotSource {
      data: mapper,
      segment: None,
      root_id: root,
      ttl_root_id: NULL_PAGE,
    },
  )
}

/// 点读便捷包装：只取 value，语义等同 `fkv_task_read` 已删除的 `read_value`。
async fn read_value<I: PageIo>(
  store: &I,
  tbl: &TableIdent,
  root: PageId,
  mapper: &PageMapper,
  key: &[u8],
  page_size: usize,
) -> fkv_error::Result<Option<Vec<u8>>> {
  let snapshot = snap(tbl, mapper, root);
  read_entry(store, tbl, key, page_size, ReadCtx::new(&snapshot))
    .await
    .map(|entry| entry.map(|entry| entry.value))
}

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
        ttl_filter_now_ms: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 99);
  });
}

use core::cell::RefCell;

use fkv_error::Result as FkvResult;
use fkv_types::FilePageId;

struct CountingStore {
  inner: MemStore,
  write_count: RefCell<usize>,
  /// `write_page_li` 调用次数（perf-82：跨组写聚合后应远小于页数）。
  batch_count: RefCell<usize>,
}

impl CountingStore {
  fn new(page_size: usize) -> Self {
    Self {
      inner: MemStore::new(page_size),
      write_count: RefCell::new(0),
      batch_count: RefCell::new(0),
    }
  }

  fn take_write_count(&self) -> usize {
    let count = *self.write_count.borrow();
    *self.write_count.borrow_mut() = 0;
    count
  }

  fn take_batch_count(&self) -> usize {
    let count = *self.batch_count.borrow();
    *self.batch_count.borrow_mut() = 0;
    count
  }
}

impl PageIo for CountingStore {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> FkvResult<()> {
    self.inner.read_page(tbl, file_page_id, dst).await
  }

  async fn write_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    src: &[u8],
  ) -> FkvResult<()> {
    *self.write_count.borrow_mut() += 1;
    self.inner.write_page(tbl, file_page_id, src).await
  }

  async fn write_page_li(
    &self,
    tbl: &TableIdent,
    page_li: &[(FilePageId, Vec<u8>)],
  ) -> FkvResult<()> {
    *self.batch_count.borrow_mut() += 1;
    *self.write_count.borrow_mut() += page_li.len();
    self.inner.write_page_li(tbl, page_li).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> FkvResult<()> {
    self.inner.sync_data(tbl).await
  }
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
        index_cache: None,
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

use std::collections::BTreeMap;

use fkv_error::KvError;
use fkv_page::{IndexPage, IndexPageIter, PageType, page_type};

/// 测试用写参数：page_size 128 / restart 8。
fn args<'a>(
  tbl: &'a TableIdent,
  root_id: PageId,
  mapper: &'a mut PageMapper,
  batch_li: &'a [BatchEntry],
) -> WriteArg<'a> {
  WriteArg {
    tbl,
    root_id,
    mapper,
    batch_li,
    layout: PageLayout::new(128, 8, 8, None, 16),
    segment_mapper: None,
    segment_size: 0,
    ttl_delete_sink: None,
  }
}

/// 一个 index 页的快照：`(逻辑 id, 文件页 id, 是否 leaf, 子项 (子页 id, 分隔键))`。
struct IndexSnap {
  page_id: PageId,
  file_page_id: FilePageId,
  leaf: bool,
  child_li: Vec<(PageId, Vec<u8>)>,
}

/// 自 root 遍历 index 层，按逻辑 id 排序返回快照（data 页本身不入表）。
async fn index_snapshot<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &PageMapper,
  root: PageId,
  page_size: usize,
) -> FkvResult<Vec<IndexSnap>> {
  let mut out = Vec::new();
  let mut stack = vec![root];
  let mut page = vec![0u8; page_size];
  while let Some(page_id) = stack.pop() {
    let file_page_id = mapper.file_page_id(page_id)?;
    page.fill(0);
    io.read_page(tbl, file_page_id, &mut page).await?;
    let page_kind = page_type(&page)?;
    if page_kind == PageType::Data {
      continue;
    }
    if !matches!(page_kind, PageType::LeafIndex | PageType::NonLeafIndex) {
      return Err(KvError::Corrupted);
    }
    let leftmost = IndexPage::new(&page)?.leftmost_ptr()?;
    stack.push(leftmost);
    let mut child_li = vec![(leftmost, Vec::new())];
    let mut iter = IndexPageIter::new(&page)?;
    while iter.advance()? {
      stack.push(iter.page_id());
      child_li.push((iter.page_id(), iter.key().to_vec()));
    }
    out.push(IndexSnap {
      page_id,
      file_page_id,
      leaf: page_kind == PageType::LeafIndex,
      child_li,
    });
  }
  out.sort_unstable_by_key(|snap| snap.page_id);
  Ok(out)
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
        index_cache: None,
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
        index_cache: None,
        ttl_filter_now_ms: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 60);
  });
}

/// 确定性 xorshift：保持测试可复现，不引入新依赖。
struct Rng(u64);

impl Rng {
  fn next(&mut self) -> u64 {
    let mut x = self.0;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    self.0 = x;
    x
  }

  fn below(&mut self, bound: usize) -> usize {
    usize::try_from(self.next() % bound as u64).unwrap()
  }
}

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
        ttl_filter_now_ms: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 20);
  });
}

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

/// sto-65：更旧时间戳的 delete 会被 merge 丢弃（旧条目存活），此时既不能释放旧值、
/// 也不能清旧 TTL 索引——否则存活条目指向已释放的页、TTL 过期永不触发。
#[test]
fn older_delete_keeps_old_value_and_ttl_index() {
  let store = MemStore::new(512);
  let tbl = TableIdent::new("demo", 1).unwrap();
  pollster::block_on(async {
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let mut sink = Vec::new();
    let seed = [BatchEntry {
      expire_ts: 1_000,
      ..BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 100)
    }];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &seed,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"a", 512)
        .await
        .unwrap(),
      Some(b"A".to_vec())
    );

    // 更旧的 delete（ts=50 < 100）：merge 丢弃它，旧条目存活。
    let mut sink = Vec::new();
    let older = [BatchEntry::delete(b"a".to_vec(), 50)];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &older,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    assert!(
      sink.is_empty(),
      "更旧的 delete 不应清存活条目的 TTL 索引：{sink:?}"
    );
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"a", 512)
        .await
        .unwrap(),
      Some(b"A".to_vec()),
      "更旧的 delete 不应释放存活条目的值"
    );

    // 更新的 delete（ts=200 > 100）：真正覆盖 → 值消失、TTL 索引被清。
    let mut sink = Vec::new();
    let newer = [BatchEntry::delete(b"a".to_vec(), 200)];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &newer,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    assert_eq!(sink.len(), 1, "更新的 delete 必须清旧 TTL 索引：{sink:?}");
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"a", 512)
        .await
        .unwrap(),
      None
    );
  });
}

/// sto-66：相同 expire_ts 的覆盖不能把 TTL 索引项删掉，否则到期后 `clean_expired`
/// 扫不到该 key、主树条目（含 overflow 链）永久滞留。
#[test]
fn same_expire_overwrite_keeps_ttl_index_entry() {
  let store = MemStore::new(512);
  let tbl = TableIdent::new("demo", 1).unwrap();
  pollster::block_on(async {
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let mut sink = Vec::new();
    let seed = [BatchEntry {
      expire_ts: 1_000,
      ..BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 100)
    }];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &seed,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();

    // 相同 expire_ts 覆盖：TTL 索引项不变，不许推删除项。
    let mut sink = Vec::new();
    let same_expire = [BatchEntry {
      expire_ts: 1_000,
      ..BatchEntry::upsert(b"a".to_vec(), b"B".to_vec(), 200)
    }];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &same_expire,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    assert!(
      sink.is_empty(),
      "相同 expire_ts 的覆盖不应删旧 TTL 索引项：{sink:?}"
    );
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"a", 512)
        .await
        .unwrap(),
      Some(b"B".to_vec())
    );

    // expire_ts 真变了才删旧 TTL 索引项。
    let mut sink = Vec::new();
    let new_expire = [BatchEntry {
      expire_ts: 2_000,
      ..BatchEntry::upsert(b"a".to_vec(), b"C".to_vec(), 300)
    }];
    write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &new_expire,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    assert_eq!(sink.len(), 1, "expire_ts 变了必须删旧 TTL 索引项：{sink:?}");
  });
}

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

/// 记录每次批量写页条数的 `MemStore` 包装，用于验证写缓冲不越过预算。
struct BatchRecordStore {
  inner: MemStore,
  batch_len_li: RefCell<Vec<usize>>,
}

impl BatchRecordStore {
  fn new(page_size: usize) -> Self {
    Self {
      inner: MemStore::new(page_size),
      batch_len_li: RefCell::new(Vec::new()),
    }
  }

  fn take_batch_len_li(&self) -> Vec<usize> {
    take(&mut *self.batch_len_li.borrow_mut())
  }
}

impl PageIo for BatchRecordStore {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: fkv_types::FilePageId,
    dst: &mut [u8],
  ) -> fkv_error::Result<()> {
    self.inner.read_page(tbl, file_page_id, dst).await
  }

  async fn write_page(
    &self,
    tbl: &TableIdent,
    file_page_id: fkv_types::FilePageId,
    src: &[u8],
  ) -> fkv_error::Result<()> {
    self.inner.write_page(tbl, file_page_id, src).await
  }

  async fn write_page_li(
    &self,
    tbl: &TableIdent,
    page_li: &[(fkv_types::FilePageId, Vec<u8>)],
  ) -> fkv_error::Result<()> {
    self.batch_len_li.borrow_mut().push(page_li.len());
    self.inner.write_page_li(tbl, page_li).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> fkv_error::Result<()> {
    self.inner.sync_data(tbl).await
  }
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
