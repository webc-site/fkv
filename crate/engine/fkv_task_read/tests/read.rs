use std::{
  cell::{Cell, RefCell},
  future::Future,
  pin::{Pin, pin},
  sync::Arc,
  task::{Context, Poll, Waker},
};

use fkv_btree::{CacheCounter, IndexCache, IndexNode};
use fkv_error::KvError;
use fkv_io::PageIo;
use fkv_io_mem::MemStore;
use fkv_mapping::{AllocMode, MappingSnapshot, PageMapper, SnapshotRegistry, SnapshotSource};
use fkv_page::{
  CompressionType, DataPageBuilder, EntrySpec, IndexPageBuilder, encode_pointers,
  overflow::{build as build_overflow_page, capacity as overflow_capacity},
  set_prev_page_id,
};
use fkv_task_read::{IndexRead, ReadCtx, read_entry, read_floor_entry, read_overflow};
use fkv_types::{FilePageId, NULL_PAGE, PageId, TableIdent};

/// 测试用快照：登记表只存弱引用，临时 registry 丢弃后快照仍有效。
fn snap(tbl: &TableIdent, mapper: &PageMapper, root_id: PageId) -> MappingSnapshot {
  SnapshotRegistry::new().open(
    tbl,
    SnapshotSource {
      data: mapper,
      segment: None,
      root_id,
      ttl_root_id: NULL_PAGE,
    },
  )
}

/// 点读便捷包装：只取 value，语义等同删除前的 `read_value`。
async fn read_value<I: PageIo>(
  store: &I,
  tbl: &TableIdent,
  key: &[u8],
  page_size: usize,
  ctx: ReadCtx<'_>,
) -> fkv_error::Result<Option<Vec<u8>>> {
  read_entry(store, tbl, key, page_size, ctx)
    .await
    .map(|entry| entry.map(|entry| entry.value))
}

fn data_page() -> Vec<u8> {
  let mut builder = DataPageBuilder::new(512, 16).unwrap();
  builder
    .add(EntrySpec {
      key: b"a",
      value: b"A",
      ts: 1,
      expire_ts: 0,
      overflow: false,
      compression: CompressionType::None,
      large_value: false,
    })
    .unwrap();
  builder
    .add(EntrySpec {
      key: b"b",
      value: b"B",
      ts: 2,
      expire_ts: 0,
      overflow: false,
      compression: CompressionType::None,
      large_value: false,
    })
    .unwrap();
  builder.finish().unwrap()
}

fn index_page(leaf: bool, leftmost: PageId) -> Vec<u8> {
  let mut builder = IndexPageBuilder::new(512, 16, leaf).unwrap();
  builder.add(b"", leftmost).unwrap();
  builder.finish().unwrap()
}

#[test]
fn single_level_read() {
  pollster::block_on(async {
    let store = MemStore::new(512);
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let data_id = mapper.allocate_page();
    let root_id = mapper.allocate_page();
    mapper.set_file_page(data_id, 0);
    mapper.set_file_page(root_id, 1);
    store.write_page(&tbl, 0, &data_page()).await.unwrap();
    store
      .write_page(&tbl, 1, &index_page(true, data_id))
      .await
      .unwrap();
    let snapshot = snap(&tbl, &mapper, root_id);
    assert_eq!(
      read_value(
        &store,
        &tbl,
        b"b",
        512,
        ReadCtx::new(&snapshot, IndexRead::none())
      )
      .await
      .unwrap(),
      Some(b"B".to_vec())
    );
    assert_eq!(
      read_value(
        &store,
        &tbl,
        b"z",
        512,
        ReadCtx::new(&snapshot, IndexRead::none())
      )
      .await
      .unwrap(),
      None
    );
  });
}

#[test]
fn two_level_read() {
  pollster::block_on(async {
    let store = MemStore::new(512);
    let tbl = TableIdent::new("demo", 2).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let data_id = mapper.allocate_page();
    let leaf_index_id = mapper.allocate_page();
    let root_id = mapper.allocate_page();
    mapper.set_file_page(data_id, 0);
    mapper.set_file_page(leaf_index_id, 1);
    mapper.set_file_page(root_id, 2);
    store.write_page(&tbl, 0, &data_page()).await.unwrap();
    store
      .write_page(&tbl, 1, &index_page(true, data_id))
      .await
      .unwrap();
    store
      .write_page(&tbl, 2, &index_page(false, leaf_index_id))
      .await
      .unwrap();
    let snapshot = snap(&tbl, &mapper, root_id);
    assert_eq!(
      read_value(
        &store,
        &tbl,
        b"a",
        512,
        ReadCtx::new(&snapshot, IndexRead::none())
      )
      .await
      .unwrap(),
      Some(b"A".to_vec())
    );
  });
}

/// 缓存命中的每层下降只计一次 hit；重复点读不应再翻倍。
#[test]
fn cache_hit_counted_once_per_level() {
  pollster::block_on(async {
    let store = MemStore::new(512);
    let tbl = TableIdent::new("demo", 4).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let data_id = mapper.allocate_page();
    let leaf_index_id = mapper.allocate_page();
    let root_id = mapper.allocate_page();
    mapper.set_file_page(data_id, 0);
    mapper.set_file_page(leaf_index_id, 1);
    mapper.set_file_page(root_id, 2);
    store.write_page(&tbl, 0, &data_page()).await.unwrap();
    store
      .write_page(&tbl, 1, &index_page(true, data_id))
      .await
      .unwrap();
    store
      .write_page(&tbl, 2, &index_page(false, leaf_index_id))
      .await
      .unwrap();

    let counter = Arc::new(CacheCounter::default());
    let cache = RefCell::new(IndexCache::with_counter(0, counter.clone()));
    let snapshot = snap(&tbl, &mapper, root_id);
    let ctx = ReadCtx {
      index: IndexRead::new(Some(&cache), None),
      ..ReadCtx::new(&snapshot, IndexRead::none())
    };
    // 首读：两级 index 各回填一次。
    assert_eq!(
      read_value(&store, &tbl, b"a", 512, ctx).await.unwrap(),
      Some(b"A".to_vec())
    );
    assert_eq!((counter.hit(), counter.miss()), (0, 2));
    // 再读：每层 index 单次 get，命中各 +1；data 页不进缓存。
    assert_eq!(
      read_value(&store, &tbl, b"a", 512, ctx).await.unwrap(),
      Some(b"A".to_vec())
    );
    assert_eq!((counter.hit(), counter.miss()), (2, 2));
  });
}

#[test]
fn overflow_single_batch() {
  pollster::block_on(async {
    let page_size = 512;
    let pointer_num = 2;
    let store = MemStore::new(page_size);
    let tbl = TableIdent::new("demo", 3).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let data_id = mapper.allocate_page();
    mapper.set_file_page(data_id, 0);
    let value = vec![b'x'; 100];
    let page = build_overflow_page(page_size, &value, &[], pointer_num).unwrap();
    store.write_page(&tbl, 0, &page).await.unwrap();
    let snapshot = snap(&tbl, &mapper, data_id);
    assert_eq!(
      read_overflow(
        &store,
        &tbl,
        &snapshot,
        page_size,
        &encode_pointers(&[data_id]),
      )
      .await
      .unwrap(),
      value
    );
  });
}

#[test]
fn overflow_cross_batch_chain() {
  pollster::block_on(async {
    let page_size = 512;
    let pointer_num = 2;
    let cap_mid = overflow_capacity(page_size, pointer_num, false).unwrap();
    let cap_end = overflow_capacity(page_size, pointer_num, true).unwrap();
    let store = MemStore::new(page_size);
    let tbl = TableIdent::new("demo", 4).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let page_id_li: Vec<_> = (0..3).map(|_| mapper.allocate_page()).collect();
    for (file_page_id, page_id) in page_id_li.iter().enumerate() {
      mapper.set_file_page(*page_id, file_page_id as u64);
    }
    // 首批两页，末页带下一批指针；第二批一页收尾。
    let value_li = [
      vec![b'a'; cap_mid],
      vec![b'b'; cap_end],
      vec![b'c'; cap_mid],
    ];
    let first_batch = [
      build_overflow_page(page_size, &value_li[0], &[], pointer_num).unwrap(),
      build_overflow_page(page_size, &value_li[1], &[page_id_li[2]], pointer_num).unwrap(),
    ];
    for (file_page_id, page) in first_batch.iter().enumerate() {
      store
        .write_page(&tbl, file_page_id as u64, page)
        .await
        .unwrap();
    }
    let last_page = build_overflow_page(page_size, &value_li[2], &[], pointer_num).unwrap();
    store.write_page(&tbl, 2, &last_page).await.unwrap();
    let expected: Vec<u8> = value_li.concat();
    let snapshot = snap(&tbl, &mapper, page_id_li[0]);
    assert_eq!(
      read_overflow(
        &store,
        &tbl,
        &snapshot,
        page_size,
        &encode_pointers(&page_id_li[..2]),
      )
      .await
      .unwrap(),
      expected
    );
  });
}

/// 盘面损坏成环：首批页尾指针指回自身，必须报 `Corrupted` 而非死循环。
#[test]
fn overflow_cycle_detected() {
  pollster::block_on(async {
    let page_size = 512;
    let pointer_num = 2;
    let store = MemStore::new(page_size);
    let tbl = TableIdent::new("demo", 5).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let page_id = mapper.allocate_page();
    mapper.set_file_page(page_id, 0);
    let page = build_overflow_page(page_size, b"loop", &[page_id], pointer_num).unwrap();
    store.write_page(&tbl, 0, &page).await.unwrap();
    let snapshot = snap(&tbl, &mapper, page_id);
    assert_eq!(
      read_overflow(
        &store,
        &tbl,
        &snapshot,
        page_size,
        &encode_pointers(&[page_id]),
      )
      .await
      .unwrap_err(),
      KvError::Corrupted
    );
  });
}

/// floor 命中已过期项且无更小存活项：TTL 下推返回 `None`，且不物化 value。
#[test]
fn floor_filters_expired_entry() {
  pollster::block_on(async {
    let page_size = 512;
    let store = MemStore::new(page_size);
    let tbl = TableIdent::new("demo", 6).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let root_id = mapper.allocate_page();
    mapper.set_file_page(root_id, 0);
    let mut builder = DataPageBuilder::new(page_size, 16).unwrap();
    builder
      .add(EntrySpec {
        key: b"a",
        value: b"A",
        ts: 1,
        expire_ts: 1,
        overflow: false,
        compression: CompressionType::None,
        large_value: false,
      })
      .unwrap();
    store
      .write_page(&tbl, 0, &builder.finish().unwrap())
      .await
      .unwrap();

    let snapshot = snap(&tbl, &mapper, root_id);
    let ctx = ReadCtx {
      ttl_filter_now_ms: Some(1000),
      ..ReadCtx::new(&snapshot, IndexRead::none())
    };
    assert!(
      read_floor_entry(&store, &tbl, b"z", page_size, ctx)
        .await
        .unwrap()
        .is_none()
    );
    // 不过滤时仍能读到该 entry，确认上面的 None 来自 TTL 而非定位失败。
    assert_eq!(
      read_floor_entry(
        &store,
        &tbl,
        b"z",
        page_size,
        ReadCtx::new(&snapshot, IndexRead::none()),
      )
      .await
      .unwrap()
      .unwrap()
      .key,
      b"a".to_vec()
    );
  });
}

/// 页内 floor 命中过期项时，回退到更小的存活 key；更小项也过期则返回 `None`。
#[test]
fn floor_retreats_to_live_entry_in_page() {
  pollster::block_on(async {
    let page_size = 512;
    let store = MemStore::new(page_size);
    let tbl = TableIdent::new("demo", 7).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let root_id = mapper.allocate_page();
    mapper.set_file_page(root_id, 0);
    let mut builder = DataPageBuilder::new(page_size, 16).unwrap();
    builder
      .add(EntrySpec {
        key: b"a",
        value: b"A",
        ts: 1,
        expire_ts: 1,
        overflow: false,
        compression: CompressionType::None,
        large_value: false,
      })
      .unwrap();
    builder
      .add(EntrySpec {
        key: b"b",
        value: b"B",
        ts: 2,
        expire_ts: 0,
        overflow: false,
        compression: CompressionType::None,
        large_value: false,
      })
      .unwrap();
    builder
      .add(EntrySpec {
        key: b"c",
        value: b"C",
        ts: 3,
        expire_ts: 1,
        overflow: false,
        compression: CompressionType::None,
        large_value: false,
      })
      .unwrap();
    store
      .write_page(&tbl, 0, &builder.finish().unwrap())
      .await
      .unwrap();

    let snapshot = snap(&tbl, &mapper, root_id);
    let ctx = ReadCtx {
      ttl_filter_now_ms: Some(1000),
      ..ReadCtx::new(&snapshot, IndexRead::none())
    };
    let entry = read_floor_entry(&store, &tbl, b"z", page_size, ctx)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(entry.key, b"b".to_vec());
    assert_eq!(entry.value, b"B".to_vec());
    // 候选本身存活时直接命中。
    assert_eq!(
      read_floor_entry(&store, &tbl, b"b", page_size, ctx)
        .await
        .unwrap()
        .unwrap()
        .key,
      b"b".to_vec()
    );
    // 更小项全部过期：无存活 floor。
    assert!(
      read_floor_entry(&store, &tbl, b"a", page_size, ctx)
        .await
        .unwrap()
        .is_none()
    );
  });
}

/// 末页全部过期时沿 prev 链回退，取前一页的存活 key。
#[test]
fn floor_retreats_across_prev_page() {
  pollster::block_on(async {
    let page_size = 512;
    let store = MemStore::new(page_size);
    let tbl = TableIdent::new("demo", 8).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let p0 = mapper.allocate_page();
    let p1 = mapper.allocate_page();
    let root_id = mapper.allocate_page();
    mapper.set_file_page(p0, 0);
    mapper.set_file_page(p1, 1);
    mapper.set_file_page(root_id, 2);

    let mut page0 = DataPageBuilder::new(page_size, 16).unwrap();
    page0
      .add(EntrySpec {
        key: b"a",
        value: b"A",
        ts: 1,
        expire_ts: 0,
        overflow: false,
        compression: CompressionType::None,
        large_value: false,
      })
      .unwrap();
    store
      .write_page(&tbl, 0, &page0.finish().unwrap())
      .await
      .unwrap();
    let mut page1 = DataPageBuilder::new(page_size, 16).unwrap();
    page1
      .add(EntrySpec {
        key: b"c",
        value: b"C",
        ts: 2,
        expire_ts: 1,
        overflow: false,
        compression: CompressionType::None,
        large_value: false,
      })
      .unwrap();
    let mut page1 = page1.finish().unwrap();
    set_prev_page_id(&mut page1, p0).unwrap();
    store.write_page(&tbl, 1, &page1).await.unwrap();

    let mut index = IndexPageBuilder::new(page_size, 16, true).unwrap();
    index.add(b"", p0).unwrap();
    index.add(b"c", p1).unwrap();
    store
      .write_page(&tbl, 2, &index.finish().unwrap())
      .await
      .unwrap();

    let snapshot = snap(&tbl, &mapper, root_id);
    let ctx = ReadCtx {
      ttl_filter_now_ms: Some(1000),
      ..ReadCtx::new(&snapshot, IndexRead::none())
    };
    let entry = read_floor_entry(&store, &tbl, b"z", page_size, ctx)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(entry.key, b"a".to_vec());
    // 不过滤时应命中末页过期项本身，确认测试覆盖的是回退路径。
    assert_eq!(
      read_floor_entry(
        &store,
        &tbl,
        b"z",
        page_size,
        ReadCtx::new(&snapshot, IndexRead::none()),
      )
      .await
      .unwrap()
      .unwrap()
      .key,
      b"c".to_vec()
    );
  });
}

/// 27b：读在 `io.read_page` 挂起时并发 `remap_page` + 写新值，恢复后仍按
/// 读起点快照解析物理页，结果等于旧值。
#[test]
fn read_suspended_at_io_uses_start_snapshot() {
  let page_size = 512;
  let io = GateIo::new(MemStore::new(page_size));
  let tbl = TableIdent::new("demo", 9).unwrap();
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let data_id = mapper.allocate_page();
  let root_id = mapper.allocate_page();
  mapper.set_file_page(data_id, 0);
  mapper.set_file_page(root_id, 1);
  pollster::block_on(io.write_page(&tbl, 0, &data_page())).unwrap();
  pollster::block_on(io.write_page(&tbl, 1, &index_page(true, data_id))).unwrap();

  let snapshot = snap(&tbl, &mapper, root_id);
  let mut fut = pin!(read_entry(
    &io,
    &tbl,
    b"a",
    page_size,
    ReadCtx::new(&snapshot, IndexRead::none())
  ));
  let mut mutated = false;
  let result = loop {
    match fut.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
      Poll::Ready(result) => break result.unwrap(),
      Poll::Pending => {
        assert!(!mutated, "read 只应在首个 IO 前挂起一次");
        mutated = true;
        // 读挂起期间：写者 remap data 页，并把新值写到新物理页。
        let new_fp = mapper.remap_page(data_id).unwrap();
        let mut builder = DataPageBuilder::new(page_size, 16).unwrap();
        builder
          .add(EntrySpec {
            key: b"a",
            value: b"Z",
            ts: 2,
            expire_ts: 0,
            overflow: false,
            compression: CompressionType::None,
            large_value: false,
          })
          .unwrap();
        pollster::block_on(io.write_page(&tbl, new_fp, &builder.finish().unwrap())).unwrap();
      }
    }
  };
  assert_eq!(result.map(|entry| entry.value), Some(b"A".to_vec()));
}

/// 27b：索引缓存按映射代际隔离。旧快照不得命中写者/新读者回填的新代节点。
#[test]
fn old_snapshot_misses_new_generation_cache_node() {
  pollster::block_on(async {
    let page_size = 512;
    let store = MemStore::new(page_size);
    let tbl = TableIdent::new("demo", 10).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let old_data = mapper.allocate_page();
    let new_data = mapper.allocate_page();
    let root_id = mapper.allocate_page();
    mapper.set_file_page(old_data, 0);
    mapper.set_file_page(new_data, 1);
    mapper.set_file_page(root_id, 2);
    let page_of = |key: &[u8], value: &[u8]| {
      let mut builder = DataPageBuilder::new(page_size, 16).unwrap();
      builder
        .add(EntrySpec {
          key,
          value,
          ts: 1,
          expire_ts: 0,
          overflow: false,
          compression: CompressionType::None,
          large_value: false,
        })
        .unwrap();
      builder.finish().unwrap()
    };
    store
      .write_page(&tbl, 0, &page_of(b"a", b"A"))
      .await
      .unwrap();
    store
      .write_page(&tbl, 1, &page_of(b"a", b"Z"))
      .await
      .unwrap();
    store
      .write_page(&tbl, 2, &index_page(true, old_data))
      .await
      .unwrap();

    let snapshot = snap(&tbl, &mapper, root_id);
    // 模拟并发写 + 新读者回填：root 换物理页，缓存条目带新代。
    let new_root_fp = mapper.remap_page(root_id).unwrap();
    store
      .write_page(&tbl, new_root_fp, &index_page(true, new_data))
      .await
      .unwrap();
    let cache = RefCell::new(IndexCache::with_capacity_bytes(0));
    let new_page = index_page(true, new_data);
    cache.borrow_mut().insert(
      root_id,
      IndexNode::new(new_root_fp, &new_page).unwrap(),
      mapper.generation(),
    );

    let ctx = ReadCtx {
      index: IndexRead::new(Some(&cache), None),
      ..ReadCtx::new(&snapshot, IndexRead::none())
    };
    // 缓存里已是新代节点：旧快照查询必须 miss。
    assert!(
      cache
        .borrow()
        .get(root_id, snapshot.mapping_generation())
        .is_none()
    );
    let entry = read_entry(&store, &tbl, b"a", page_size, ctx)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(entry.value, b"A".to_vec(), "旧快照不得命中新代缓存节点");
  });
}

/// 首次 `read_page` 让出一次的控制 IO，用于在 IO 挂起点插入并发写。
struct GateIo {
  inner: MemStore,
  yield_left: Cell<usize>,
}

impl GateIo {
  fn new(inner: MemStore) -> Self {
    Self {
      inner,
      yield_left: Cell::new(1),
    }
  }
}

impl PageIo for GateIo {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> fkv_error::Result<()> {
    if self
      .yield_left
      .replace(self.yield_left.get().saturating_sub(1))
      > 0
    {
      YieldOnce::default().await;
    }
    self.inner.read_page(tbl, file_page_id, dst).await
  }

  fn write_page<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_page_id: FilePageId,
    src: &'a [u8],
  ) -> impl Future<Output = fkv_error::Result<()>> + 'a {
    self.inner.write_page(tbl, file_page_id, src)
  }

  fn sync_data<'a>(
    &'a self,
    tbl: &'a TableIdent,
  ) -> impl Future<Output = fkv_error::Result<()>> + 'a {
    self.inner.sync_data(tbl)
  }
}

/// 首次 poll 返回 `Pending`，第二次返回 `Ready`；由测试手动驱动，不依赖 waker。
#[derive(Default)]
struct YieldOnce(bool);

impl Future for YieldOnce {
  type Output = ();

  fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
    if self.0 {
      Poll::Ready(())
    } else {
      self.0 = true;
      Poll::Pending
    }
  }
}
