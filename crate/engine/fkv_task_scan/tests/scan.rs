use fkv_error::{KvError, Result};
use fkv_io::PageIo;
use fkv_io_mem::MemStore;
use fkv_mapping::{AllocMode, MappingSnapshot, PageMapper, SnapshotRegistry, SnapshotSource};
use fkv_page::{
  CompressionType, DataPageBuilder, EntrySpec, IndexPageBuilder, set_next_page_id, set_prev_page_id,
};
use fkv_task_scan::{ScanTree, scan_range};
use fkv_types::{FilePageId, NULL_PAGE, PageId, TableIdent};

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

fn data_page(entry_li: &[(&str, &str)]) -> Vec<u8> {
  let mut builder = DataPageBuilder::new(512, 16).unwrap();
  for (key, value) in entry_li {
    builder
      .add(EntrySpec {
        key: key.as_bytes(),
        value: value.as_bytes(),
        ts: 0,
        expire_ts: 0,
        overflow: false,
        compression: CompressionType::None,
        large_value: false,
      })
      .unwrap();
  }
  builder.finish().unwrap()
}

fn index_page(leaf: bool, leftmost: PageId) -> Vec<u8> {
  let mut builder = IndexPageBuilder::new(512, 16, leaf).unwrap();
  builder.add(b"", leftmost).unwrap();
  builder.finish().unwrap()
}

#[test]
fn scan_chain() {
  pollster::block_on(async {
    let store = MemStore::new(512);
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let p0 = mapper.allocate_page();
    let p1 = mapper.allocate_page();
    let root = mapper.allocate_page();
    mapper.set_file_page(p0, 0);
    mapper.set_file_page(p1, 1);
    mapper.set_file_page(root, 2);

    let mut page0 = data_page(&[("a", "A"), ("b", "B")]);
    set_next_page_id(&mut page0, p1).unwrap();
    let mut page1 = data_page(&[("c", "C"), ("d", "D")]);
    set_prev_page_id(&mut page1, p0).unwrap();
    set_next_page_id(&mut page1, PageId::MAX).unwrap();
    store.write_page(&tbl, 0, &page0).await.unwrap();
    store.write_page(&tbl, 1, &page1).await.unwrap();
    store
      .write_page(&tbl, 2, &index_page(true, p0))
      .await
      .unwrap();

    let snapshot = snap(&tbl, &mapper, root);
    let all = scan_range(
      &store,
      fkv_task_scan::ScanArg {
        tbl: &tbl,
        snapshot: &snapshot,
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
    assert_eq!(
      all,
      vec![
        (b"a".to_vec(), b"A".to_vec()),
        (b"b".to_vec(), b"B".to_vec()),
        (b"c".to_vec(), b"C".to_vec()),
        (b"d".to_vec(), b"D".to_vec()),
      ]
    );
    let part = scan_range(
      &store,
      fkv_task_scan::ScanArg {
        tbl: &tbl,
        snapshot: &snapshot,
        tree: ScanTree::Data,
        begin: b"b",
        end: b"d",
        page_size: 512,
        max_entries: 0,
        dict: None,
        index_cache: None,
        ttl_filter_now_ms: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(
      part,
      vec![
        (b"b".to_vec(), b"B".to_vec()),
        (b"c".to_vec(), b"C".to_vec()),
      ]
    );
  });
}

/// issue eng-43：`page_entries` 与 `max_entries` 统一为单一 entry 上限
/// （取较小非零值），`has_remaining` 仅在分页上限生效时为真。
#[test]
fn scan_page_entry_limit_semantics() {
  use fkv_task_scan::{PageOption, ScanArg, ScanOption, scan_page_with};

  pollster::block_on(async {
    let store = MemStore::new(512);
    let tbl = TableIdent::new("demo", 2).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let p0 = mapper.allocate_page();
    let p1 = mapper.allocate_page();
    let root = mapper.allocate_page();
    mapper.set_file_page(p0, 0);
    mapper.set_file_page(p1, 1);
    mapper.set_file_page(root, 2);
    let mut page0 = data_page(&[("a", "A"), ("b", "B")]);
    set_next_page_id(&mut page0, p1).unwrap();
    let mut page1 = data_page(&[("c", "C"), ("d", "D")]);
    set_prev_page_id(&mut page1, p0).unwrap();
    set_next_page_id(&mut page1, PageId::MAX).unwrap();
    store.write_page(&tbl, 0, &page0).await.unwrap();
    store.write_page(&tbl, 1, &page1).await.unwrap();
    store
      .write_page(&tbl, 2, &index_page(true, p0))
      .await
      .unwrap();

    let snapshot = snap(&tbl, &mapper, root);
    let args = |max_entries: usize| ScanArg {
      tbl: &tbl,
      snapshot: &snapshot,
      tree: ScanTree::Data,
      begin: b"",
      end: b"",
      page_size: 512,
      max_entries,
      dict: None,
      index_cache: None,
      ttl_filter_now_ms: None,
    };

    let page = scan_page_with(
      &store,
      args(3),
      ScanOption::default(),
      PageOption {
        page_entries: 1,
        page_size: 0,
      },
    )
    .await
    .unwrap();
    assert_eq!(page.entry_li.len(), 1);
    assert!(
      page.has_remaining,
      "pagination limit must set has_remaining"
    );

    let page = scan_page_with(
      &store,
      args(3),
      ScanOption::default(),
      PageOption {
        page_entries: 0,
        page_size: 0,
      },
    )
    .await
    .unwrap();
    assert_eq!(page.entry_li.len(), 3);
    assert!(!page.has_remaining);

    let page = scan_page_with(
      &store,
      args(1),
      ScanOption::default(),
      PageOption {
        page_entries: 3,
        page_size: 0,
      },
    )
    .await
    .unwrap();
    assert_eq!(page.entry_li.len(), 1);
    assert!(!page.has_remaining);
  });
}

/// issue eng-65：遍历来源是叶子 next 链，链成环时必须报 `Corrupted` 而非无限推进。
#[test]
fn scan_leaf_chain_cycle_detected() {
  use fkv_error::KvError;

  pollster::block_on(async {
    let store = MemStore::new(512);
    let tbl = TableIdent::new("demo", 3).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let p0 = mapper.allocate_page();
    let p1 = mapper.allocate_page();
    mapper.set_file_page(p0, 0);
    mapper.set_file_page(p1, 1);
    let mut page0 = data_page(&[("a", "A")]);
    set_next_page_id(&mut page0, p1).unwrap();
    let mut page1 = data_page(&[("b", "B")]);
    set_next_page_id(&mut page1, p0).unwrap();
    store.write_page(&tbl, 0, &page0).await.unwrap();
    store.write_page(&tbl, 1, &page1).await.unwrap();

    let snapshot = snap(&tbl, &mapper, p0);
    let err = scan_range(
      &store,
      fkv_task_scan::ScanArg {
        tbl: &tbl,
        snapshot: &snapshot,
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
    .unwrap_err();
    assert_eq!(err, KvError::Corrupted);
  });
}

/// 指定物理页读取必然失败的 IO 包装：验证预取错误的作用域。
struct FlakyIo {
  inner: MemStore,
  fail_fp: FilePageId,
}

impl PageIo for FlakyIo {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    if file_page_id == self.fail_fp {
      return Err(KvError::IoFail);
    }
    self.inner.read_page(tbl, file_page_id, dst).await
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    self.inner.write_page(tbl, file_page_id, src).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.inner.sync_data(tbl).await
  }
}

/// 建一条 p0 -> p1 的叶子链，root 为单层 index。返回 (mapper, snapshot 用的 root)。
async fn flaky_chain(store: &FlakyIo, tbl: &TableIdent, mapper: &mut PageMapper) -> PageId {
  let p0 = mapper.allocate_page();
  let p1 = mapper.allocate_page();
  let root = mapper.allocate_page();
  mapper.set_file_page(p0, 0);
  mapper.set_file_page(p1, 1);
  mapper.set_file_page(root, 2);
  let mut page0 = data_page(&[("a", "A"), ("b", "B")]);
  set_next_page_id(&mut page0, p1).unwrap();
  let mut page1 = data_page(&[("c", "C")]);
  set_prev_page_id(&mut page1, p0).unwrap();
  set_next_page_id(&mut page1, PageId::MAX).unwrap();
  store.write_page(tbl, 0, &page0).await.unwrap();
  store.write_page(tbl, 1, &page1).await.unwrap();
  store
    .write_page(tbl, 2, &index_page(true, p0))
    .await
    .unwrap();
  root
}

/// 分页配额提前停止后，下一页预取失败不得让已经完整的分页请求失败。
#[test]
fn stop_ignores_prefetch_error() {
  use fkv_task_scan::{PageOption, ScanArg, ScanOption, scan_page_with};

  pollster::block_on(async {
    let io = FlakyIo {
      inner: MemStore::new(512),
      fail_fp: 1,
    };
    let tbl = TableIdent::new("demo", 3).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let root = flaky_chain(&io, &tbl, &mut mapper).await;
    let snapshot = snap(&tbl, &mapper, root);
    let page = scan_page_with(
      &io,
      ScanArg {
        tbl: &tbl,
        snapshot: &snapshot,
        tree: ScanTree::Data,
        begin: b"",
        end: b"",
        page_size: 512,
        max_entries: 0,
        dict: None,
        index_cache: None,
        ttl_filter_now_ms: None,
      },
      ScanOption::default(),
      PageOption {
        page_entries: 1,
        page_size: 0,
      },
    )
    .await
    .unwrap();
    assert_eq!(page.entry_li.len(), 1);
    assert!(page.has_remaining);
  });
}

/// 需要下一页时，预取错误必须上抛（不能被吞掉当成扫描结束）。
#[test]
fn prefetch_error_propagates_when_next_page_needed() {
  pollster::block_on(async {
    let io = FlakyIo {
      inner: MemStore::new(512),
      fail_fp: 1,
    };
    let tbl = TableIdent::new("demo", 4).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let root = flaky_chain(&io, &tbl, &mut mapper).await;
    let snapshot = snap(&tbl, &mapper, root);
    let err = scan_range(
      &io,
      fkv_task_scan::ScanArg {
        tbl: &tbl,
        snapshot: &snapshot,
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
    .unwrap_err();
    assert_eq!(err, KvError::IoFail);
  });
}
