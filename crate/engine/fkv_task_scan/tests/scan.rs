use std::sync::Mutex;

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

/// 带 `ts` / `expire_ts` 的 data page：`(key, value, ts, expire_ts)`。
fn data_page_ttl(entry_li: &[(&str, &str, u64, u64)]) -> Vec<u8> {
  let mut builder = DataPageBuilder::new(512, 16).unwrap();
  for (key, value, ts, expire_ts) in entry_li {
    builder
      .add(EntrySpec {
        key: key.as_bytes(),
        value: value.as_bytes(),
        ts: *ts,
        expire_ts: *expire_ts,
        overflow: false,
        compression: CompressionType::None,
        large_value: false,
      })
      .unwrap();
  }
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

/// 记录每次批量读（`read_page_li`）提交的物理页号序列。
struct BatchIo {
  inner: MemStore,
  batch_li: Mutex<Vec<Vec<FilePageId>>>,
}

impl BatchIo {
  fn new(page_size: usize) -> Self {
    Self {
      inner: MemStore::new(page_size),
      batch_li: Mutex::new(Vec::new()),
    }
  }

  fn take_batch_li(&self) -> Vec<Vec<FilePageId>> {
    std::mem::take(&mut *self.batch_li.lock().unwrap())
  }
}

impl PageIo for BatchIo {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    self.inner.read_page(tbl, file_page_id, dst).await
  }

  async fn read_page_owned(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    buf: &mut Vec<u8>,
  ) -> Result<()> {
    self.inner.read_page_owned(tbl, file_page_id, buf).await
  }

  async fn read_page_li(
    &self,
    tbl: &TableIdent,
    file_page_id_li: &[FilePageId],
    page_buf_li: &mut [Vec<u8>],
  ) -> Result<()> {
    self.batch_li.lock().unwrap().push(file_page_id_li.to_vec());
    self
      .inner
      .read_page_li(tbl, file_page_id_li, page_buf_li)
      .await
  }

  async fn read_page_span(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    page_buf_li: &mut [Vec<u8>],
  ) -> Result<()> {
    let span: Vec<FilePageId> = (0..page_buf_li.len() as FilePageId)
      .map(|offset| file_page_id + offset)
      .collect();
    self.batch_li.lock().unwrap().push(span);
    self
      .inner
      .read_page_span(tbl, file_page_id, page_buf_li)
      .await
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    self.inner.write_page(tbl, file_page_id, src).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.inner.sync_data(tbl).await
  }
}

/// 三层树：非叶 root -> 3 个 LeafIndex -> 9 个 data 页（每页 3 条，键升序）。
///
/// data 页物理页号 0..8，LeafIndex 20..22，root 30。叶子链与索引顺序一致：
/// `k00..k26` 连续。返回 snapshot 用的 root。
async fn multi_level_tree<I: PageIo>(io: &I, tbl: &TableIdent, mapper: &mut PageMapper) -> PageId {
  const DATA_PER_PAGE: usize = 3;
  const PAGE_NUM: usize = 9;
  const LEAF_NUM: usize = 3;
  let key_at = |index: usize| format!("k{index:02}").into_bytes();

  let data_li: Vec<PageId> = (0..PAGE_NUM).map(|_| mapper.allocate_page()).collect();
  let leaf_li: Vec<PageId> = (0..LEAF_NUM).map(|_| mapper.allocate_page()).collect();
  let root = mapper.allocate_page();
  for (fp, id) in data_li.iter().enumerate() {
    mapper.set_file_page(*id, fp as FilePageId);
  }
  for (index, id) in leaf_li.iter().enumerate() {
    mapper.set_file_page(*id, (20 + index) as FilePageId);
  }
  mapper.set_file_page(root, 30);

  for page_idx in 0..PAGE_NUM {
    let mut builder = DataPageBuilder::new(512, 16).unwrap();
    for offset in 0..DATA_PER_PAGE {
      let key = key_at(page_idx * DATA_PER_PAGE + offset);
      builder
        .add(EntrySpec {
          key: &key,
          value: b"v",
          ts: 0,
          expire_ts: 0,
          overflow: false,
          compression: CompressionType::None,
          large_value: false,
        })
        .unwrap();
    }
    let mut page = builder.finish().unwrap();
    let next = if page_idx + 1 < PAGE_NUM {
      data_li[page_idx + 1]
    } else {
      PageId::MAX
    };
    set_next_page_id(&mut page, next).unwrap();
    if page_idx > 0 {
      set_prev_page_id(&mut page, data_li[page_idx - 1]).unwrap();
    }
    io.write_page(tbl, page_idx as FilePageId, &page)
      .await
      .unwrap();
  }

  for group in 0..LEAF_NUM {
    let first = group * DATA_PER_PAGE;
    let mut builder = IndexPageBuilder::new(512, 16, true).unwrap();
    builder.add(&key_at(first), data_li[first]).unwrap();
    for offset in 1..DATA_PER_PAGE {
      let index = first + offset;
      builder.add(&key_at(index), data_li[index]).unwrap();
    }
    let page = builder.finish().unwrap();
    io.write_page(tbl, (20 + group) as FilePageId, &page)
      .await
      .unwrap();
  }

  let mut builder = IndexPageBuilder::new(512, 16, false).unwrap();
  builder.add(&key_at(0), leaf_li[0]).unwrap();
  builder.add(&key_at(DATA_PER_PAGE), leaf_li[1]).unwrap();
  builder.add(&key_at(2 * DATA_PER_PAGE), leaf_li[2]).unwrap();
  io.write_page(tbl, 30, &builder.finish().unwrap())
    .await
    .unwrap();
  root
}

/// 索引栈预取必须按 key 顺序批量提交叶子页：批次拼起来等于叶子链顺序
/// （fp0..fp8），单批不超过 K=6，且确实发生跨 LeafIndex 页（第 3 页起）
/// 的下降。
#[test]
fn prefetch_batches_leaf_pages_in_key_order() {
  use fkv_task_scan::{ScanArg, scan_range};

  pollster::block_on(async {
    let io = BatchIo::new(512);
    let tbl = TableIdent::new("demo", 21).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let root = multi_level_tree(&io, &tbl, &mut mapper).await;
    let snapshot = snap(&tbl, &mapper, root);

    let out = scan_range(
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
        ttl_filter_now_ms: None,
      },
    )
    .await
    .unwrap();
    let expected_keys: Vec<Vec<u8>> = (0..27).map(|i| format!("k{i:02}").into_bytes()).collect();
    assert_eq!(
      out.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>(),
      expected_keys,
      "扫描结果必须按 key 升序"
    );

    let batch_li = io.take_batch_li();
    let submitted: Vec<FilePageId> = batch_li.iter().flatten().copied().collect();
    assert_eq!(
      submitted,
      (0..9).collect::<Vec<FilePageId>>(),
      "预取必须按叶子链 / key 顺序提交且不重复: {batch_li:?}"
    );
    assert!(
      batch_li.iter().any(|batch| batch.len() > 1),
      "索引栈预取必须一次提交多页: {batch_li:?}"
    );
    assert!(
      batch_li.iter().all(|batch| batch.len() <= 6),
      "单批不得超过 C++ 对齐的 6 页: {batch_li:?}"
    );
  });
}

/// 预取路径必须与逐页直读逐 key 一致：`end` 为空走索引栈批量预取；带 `end`
/// 的有界扫描不预取（同一条叶子链，逐页 `read_page_owned`）。两种路径的
/// key/value 序列必须完全相同。
#[test]
fn prefetch_order_matches_direct_scan() {
  use fkv_task_scan::{ScanArg, ScanOption, scan_entries_with, scan_range};

  pollster::block_on(async {
    let io = MemStore::new(512);
    let tbl = TableIdent::new("demo", 22).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let root = multi_level_tree(&io, &tbl, &mut mapper).await;
    let snapshot = snap(&tbl, &mapper, root);
    let arg = |begin: &'static [u8], end: &'static [u8]| ScanArg {
      tbl: &tbl,
      snapshot: &snapshot,
      tree: ScanTree::Data,
      begin,
      end,
      page_size: 512,
      max_entries: 0,
      dict: None,
      ttl_filter_now_ms: None,
    };
    let inclusive = ScanOption {
      end_inclusive: true,
      ..ScanOption::default()
    };

    for begin in [b"".as_slice(), b"k05".as_slice(), b"k04".as_slice()] {
      let prefetched = scan_range(&io, arg(begin, b"")).await.unwrap();
      let direct = scan_entries_with(&io, arg(begin, b"k26"), inclusive)
        .await
        .unwrap();
      assert_eq!(
        prefetched,
        direct
          .iter()
          .map(|entry| (entry.key.clone(), entry.value.clone()))
          .collect::<Vec<_>>(),
        "begin={begin:?} 预取与逐页直读必须逐 key 一致"
      );
    }
  });
}

/// 计数路径与物化路径在各种边界下等值：整表、半开 / 闭区间、页边界键、
/// 跨 LeafIndex 页分裂，以及链上的空 data 页。
#[test]
fn scan_count_matches_materialized_across_index_splits() {
  use fkv_task_scan::{ScanArg, ScanOption, scan_count, scan_entries_with};

  pollster::block_on(async {
    let io = MemStore::new(512);
    let tbl = TableIdent::new("demo", 23).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let root = multi_level_tree(&io, &tbl, &mut mapper).await;
    let snapshot = snap(&tbl, &mapper, root);
    let arg = |begin: &'static [u8], end: &'static [u8]| ScanArg {
      tbl: &tbl,
      snapshot: &snapshot,
      tree: ScanTree::Data,
      begin,
      end,
      page_size: 512,
      max_entries: 0,
      dict: None,
      ttl_filter_now_ms: None,
    };

    let cases: &[(&'static [u8], &'static [u8])] = &[
      (b"", b""),
      (b"k00", b"k26"),
      (b"k03", b"k20"),
      (b"k04", b"k04"),
      (b"k05", b"k06"),
      (b"k08", b"k09"),
      (b"k26", b""),
      (b"zzz", b""),
    ];
    for &(begin, end) in cases {
      for end_inclusive in [false, true] {
        for begin_inclusive in [true, false] {
          let option = ScanOption {
            begin_inclusive,
            end_inclusive,
            ..ScanOption::default()
          };
          let entries = scan_entries_with(&io, arg(begin, end), option)
            .await
            .unwrap();
          let count = scan_count(&io, arg(begin, end), option).await.unwrap();
          assert_eq!(
            count,
            entries.len(),
            "begin={begin:?} end={end:?} begin_incl={begin_inclusive} end_incl={end_inclusive}"
          );
        }
      }
    }
    // 整表计数 = 27 条。
    assert_eq!(
      scan_count(&io, arg(b"", b""), ScanOption::default())
        .await
        .unwrap(),
      27
    );

    // 叶子链中间夹一个空 data 页：索引 / 链都包含它，扫描必须跳过而不报错，
    // 计数与物化等值。
    let empty_io = MemStore::new(512);
    let empty_tbl = TableIdent::new("demo", 24).unwrap();
    let mut empty_mapper = PageMapper::new(AllocMode::Append, 0);
    let p0 = empty_mapper.allocate_page();
    let p1 = empty_mapper.allocate_page();
    let empty_root = empty_mapper.allocate_page();
    empty_mapper.set_file_page(p0, 0);
    empty_mapper.set_file_page(p1, 1);
    empty_mapper.set_file_page(empty_root, 2);
    let mut page0 = data_page(&[("a", "A"), ("b", "B")]);
    set_next_page_id(&mut page0, p1).unwrap();
    let mut page1 = DataPageBuilder::new(512, 16).unwrap().finish().unwrap();
    set_prev_page_id(&mut page1, p0).unwrap();
    set_next_page_id(&mut page1, PageId::MAX).unwrap();
    empty_io.write_page(&empty_tbl, 0, &page0).await.unwrap();
    empty_io.write_page(&empty_tbl, 1, &page1).await.unwrap();
    let mut index = IndexPageBuilder::new(512, 16, true).unwrap();
    index.add(b"", p0).unwrap();
    index.add(b"c", p1).unwrap();
    empty_io
      .write_page(&empty_tbl, 2, &index.finish().unwrap())
      .await
      .unwrap();
    let empty_snap = snap(&empty_tbl, &empty_mapper, empty_root);
    let empty_arg = || ScanArg {
      tbl: &empty_tbl,
      snapshot: &empty_snap,
      tree: ScanTree::Data,
      begin: b"",
      end: b"",
      page_size: 512,
      max_entries: 0,
      dict: None,
      ttl_filter_now_ms: None,
    };
    assert_eq!(
      scan_count(&empty_io, empty_arg(), ScanOption::default())
        .await
        .unwrap(),
      2
    );
    assert_eq!(
      scan_entries_with(&empty_io, empty_arg(), ScanOption::default())
        .await
        .unwrap()
        .len(),
      2
    );
  });
}

/// 落盘 sem-66 对拍用的 4 页叶子链：entry 内容 / TTL / next-prev 链接固定为
/// 同一份逻辑 oplog，只有物理布局（`fp_li` / `root_fp`）可变。
/// 返回 snapshot 用的 root `PageId`。
async fn ttl_chain<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  fp_li: [FilePageId; 4],
  root_fp: FilePageId,
) -> PageId {
  let id_li: Vec<PageId> = (0..5).map(|_| mapper.allocate_page()).collect();
  for (index, fp) in fp_li.iter().enumerate() {
    mapper.set_file_page(id_li[index], *fp);
  }
  mapper.set_file_page(id_li[4], root_fp);
  // b / e 在 now = 1000 已过期（500 / 900）；d / g 带未来 expire_ts，须原样返回。
  let page_li = [
    vec![("a", "A1", 1, 0), ("b", "B1", 2, 500)],
    vec![("c", "C1", 3, 0), ("d", "D1", 4, 2000), ("e", "E1", 5, 900)],
    vec![("f", "F1", 6, 0), ("g", "G1", 7, 4000)],
    vec![("h", "H1", 8, 0), ("i", "I1", 9, 0)],
  ];
  for (index, entry_li) in page_li.iter().enumerate() {
    let mut page = data_page_ttl(entry_li);
    if index > 0 {
      set_prev_page_id(&mut page, id_li[index - 1]).unwrap();
    }
    let next = if index + 1 < page_li.len() {
      id_li[index + 1]
    } else {
      PageId::MAX
    };
    set_next_page_id(&mut page, next).unwrap();
    io.write_page(tbl, fp_li[index], &page).await.unwrap();
  }
  io.write_page(tbl, root_fp, &index_page(true, id_li[0]))
    .await
    .unwrap();
  id_li[4]
}

/// issue sem-66：同一份逻辑 oplog（entry / TTL / 链接）铺成两种物理布局
/// （顺序 fp0..fp3 与散乱 fp0/fp10/fp1/fp2）——`scan_page_with` 的逐条 entry
/// （key / value / timestamp / expire_ts 全字段）、TTL 过滤与配额截断
/// （`has_remaining`）必须一致，并与显式期望逐值相等。
///
/// 小配额把预取深度压到 1（不预取），两种盘面都走逐页直读；布局差异不得影响
/// 结果。
#[test]
fn bypass_and_speculation_paths_agree() {
  use fkv_task_scan::{PageOption, ScanArg, ScanOption, ScanPage, ScannedEntry, scan_page_with};

  /// 对拍固定参数：TTL 过滤 now = 1000，分页配额 5 条、全局上限 10 条。
  async fn quota_ttl_scan(
    io: &impl PageIo,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
  ) -> Result<ScanPage> {
    scan_page_with(
      io,
      ScanArg {
        tbl,
        snapshot,
        tree: ScanTree::Data,
        begin: b"",
        end: b"",
        page_size: 512,
        max_entries: 10,
        dict: None,
        ttl_filter_now_ms: Some(1000),
      },
      ScanOption::default(),
      PageOption {
        page_entries: 5,
        page_size: 0,
      },
    )
    .await
  }

  pollster::block_on(async {
    // 显式期望（三重对拍基准）：存活 a,c,d,f,g，第 6 条存活 h 触发配额截断；
    // timestamp 与存活的非零 expire_ts（d / g）逐字段锁定。
    let expected = [
      ("a", "A1", 1, 0),
      ("c", "C1", 3, 0),
      ("d", "D1", 4, 2000),
      ("f", "F1", 6, 0),
      ("g", "G1", 7, 4000),
    ]
    .map(|(key, value, ts, expire)| ScannedEntry {
      key: key.as_bytes().to_vec(),
      value: value.as_bytes().to_vec(),
      timestamp: ts,
      expire_ts: expire,
    });

    // 盘面一：链页物理连续 fp0..fp3、root@fp4。
    let io_spec = MemStore::new(512);
    let tbl_spec = TableIdent::new("demo", 66).unwrap();
    let mut mapper_spec = PageMapper::new(AllocMode::Append, 0);
    let root_spec = ttl_chain(&io_spec, &tbl_spec, &mut mapper_spec, [0, 1, 2, 3], 4).await;
    let snap_spec = snap(&tbl_spec, &mapper_spec, root_spec);
    let page_spec = quota_ttl_scan(&io_spec, &tbl_spec, &snap_spec)
      .await
      .unwrap();
    assert_eq!(page_spec.entry_li, expected);
    assert!(page_spec.has_remaining, "配额截断必须置 has_remaining");

    // 盘面二：同一 oplog，P1 散落到 fp10，root@fp25。
    let io_byp = MemStore::new(512);
    let tbl_byp = TableIdent::new("demo", 67).unwrap();
    let mut mapper_byp = PageMapper::new(AllocMode::Append, 0);
    let root_byp = ttl_chain(&io_byp, &tbl_byp, &mut mapper_byp, [0, 10, 1, 2], 25).await;
    let snap_byp = snap(&tbl_byp, &mapper_byp, root_byp);
    let page_byp = quota_ttl_scan(&io_byp, &tbl_byp, &snap_byp).await.unwrap();
    // 对拍核心：两种盘面逐条一致且与期望一致，配额截断 / has_remaining 同值。
    assert_eq!(page_byp.entry_li, page_spec.entry_li);
    assert_eq!(page_byp.entry_li, expected);
    assert_eq!(page_byp.has_remaining, page_spec.has_remaining);
  });
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
        ttl_filter_now_ms: None,
      },
    )
    .await
    .unwrap_err();
    assert_eq!(err, KvError::IoFail);
  });
}

/// `scan_count` 与 `scan_range` 同口径：计数路径与物化路径走同一叶子链遍历，
/// begin/end 边界与 TTL 过期过滤一致，返回相同的存活条数。
#[test]
fn scan_count_matches_scan_range() {
  use fkv_task_scan::{ScanArg, ScanOption, scan_count, scan_range};

  pollster::block_on(async {
    let store = MemStore::new(512);
    let tbl = TableIdent::new("demo", 12).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let p0 = mapper.allocate_page();
    let p1 = mapper.allocate_page();
    let root = mapper.allocate_page();
    mapper.set_file_page(p0, 0);
    mapper.set_file_page(p1, 1);
    mapper.set_file_page(root, 2);
    let mut page0 = data_page(&[("a", "A"), ("b", "B")]);
    set_next_page_id(&mut page0, p1).unwrap();
    // d 在 now=100 过期：计数与物化都必须过滤掉。
    let mut page1 = data_page_ttl(&[("c", "C", 0, 0), ("d", "D", 0, 100), ("e", "E", 0, 0)]);
    set_prev_page_id(&mut page1, p0).unwrap();
    set_next_page_id(&mut page1, PageId::MAX).unwrap();
    store.write_page(&tbl, 0, &page0).await.unwrap();
    store.write_page(&tbl, 1, &page1).await.unwrap();
    store
      .write_page(&tbl, 2, &index_page(true, p0))
      .await
      .unwrap();

    let snapshot = snap(&tbl, &mapper, root);
    let args = |begin: &'static [u8], end: &'static [u8]| ScanArg {
      tbl: &tbl,
      snapshot: &snapshot,
      tree: ScanTree::Data,
      begin,
      end,
      page_size: 512,
      max_entries: 0,
      dict: None,
      ttl_filter_now_ms: Some(100),
    };

    // 全区间：a,b,c,e 存活（d 过期被过滤）。
    let full = scan_range(&store, args(b"", b"")).await.unwrap();
    assert_eq!(full.len(), 4);
    assert_eq!(
      scan_count(&store, args(b"", b""), ScanOption::default())
        .await
        .unwrap(),
      4
    );

    // 非空区间：begin/end 边界与物化扫描同口径。
    let part = scan_range(&store, args(b"b", b"e")).await.unwrap();
    assert_eq!(
      scan_count(&store, args(b"b", b"e"), ScanOption::default())
        .await
        .unwrap(),
      part.len()
    );
  });
}
