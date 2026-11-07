use std::sync::atomic::{AtomicUsize, Ordering};

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
        index_cache: None,
        index_flight: None,
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
        index_flight: None,
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
      index_flight: None,
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
        index_flight: None,
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

/// 记录窗口批量读（`read_page_span`）与逐页 owned 读（`read_page_owned`）次数：
/// 验证深度退化到 1 后窗口被整条旁路，后续页只走逐页直读。
struct CountingIo {
  inner: MemStore,
  span_num: AtomicUsize,
  owned_num: AtomicUsize,
}

impl PageIo for CountingIo {
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
    self.owned_num.fetch_add(1, Ordering::Relaxed);
    self.inner.read_page_owned(tbl, file_page_id, buf).await
  }

  async fn read_page_span(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    page_buf_li: &mut [Vec<u8>],
  ) -> Result<()> {
    self.span_num.fetch_add(1, Ordering::Relaxed);
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

/// 投机白读多于有用读、深度退化到 1 之后：窗口被丢弃，不再批量读，剩余链页
/// 全部走窗口引入前的逐页 `read_page_owned`。
#[test]
fn degraded_window_bypasses_speculation() {
  use fkv_task_scan::{ScanArg, scan_range};

  pollster::block_on(async {
    let io = CountingIo {
      inner: MemStore::new(512),
      span_num: AtomicUsize::new(0),
      owned_num: AtomicUsize::new(0),
    };
    let tbl = TableIdent::new("demo", 5).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let ids: Vec<PageId> = (0..26).map(|_| mapper.allocate_page()).collect();
    for (fp, id) in ids.iter().enumerate() {
      mapper.set_file_page(*id, fp as FilePageId);
    }
    let (p0, p10, p1, p2, root) = (ids[0], ids[10], ids[1], ids[2], ids[25]);
    // 链 p0(fp0) -> p10(fp10) -> p1(fp1) -> p2(fp2)：fp10 起的 16 页都真实存在，
    // 投机段一次读成功，但本请求只用到首页 -> 白读 15 页。
    for fp in [
      3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
    ] {
      io.write_page(&tbl, fp, &vec![0u8; 512]).await.unwrap();
    }
    let mut page0 = data_page(&[("a", "A"), ("b", "B")]);
    set_next_page_id(&mut page0, p10).unwrap();
    let mut page10 = data_page(&[("c", "C")]);
    set_prev_page_id(&mut page10, p0).unwrap();
    set_next_page_id(&mut page10, p1).unwrap();
    let mut page1 = data_page(&[("d", "D")]);
    set_prev_page_id(&mut page1, p10).unwrap();
    set_next_page_id(&mut page1, p2).unwrap();
    let mut page2 = data_page(&[("e", "E")]);
    set_prev_page_id(&mut page2, p1).unwrap();
    set_next_page_id(&mut page2, PageId::MAX).unwrap();
    io.write_page(&tbl, 0, &page0).await.unwrap();
    io.write_page(&tbl, 10, &page10).await.unwrap();
    io.write_page(&tbl, 1, &page1).await.unwrap();
    io.write_page(&tbl, 2, &page2).await.unwrap();
    io.write_page(&tbl, 25, &index_page(true, p0))
      .await
      .unwrap();
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
        index_cache: None,
        index_flight: None,
        ttl_filter_now_ms: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(out.len(), 5);
    // 首页投机段读一次；退化后剩余两页各一次 owned 直读。
    assert_eq!(io.span_num.load(Ordering::Relaxed), 1);
    // owned 读：index root + 首页 p0 + 退化后的 p1 / p2。
    assert_eq!(io.owned_num.load(Ordering::Relaxed), 4);
  });
}

/// 落盘 sem-66 对拍用的 4 页叶子链：entry 内容 / TTL / next-prev 链接固定为
/// 同一份逻辑 oplog，只有物理布局（`fp_li` / `root_fp`）可变，用于铺出
/// 「连续（投机命中）」与「散乱（批量读失败退化旁路）」两种盘面。
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

/// issue sem-66：同一份逻辑 oplog（entry / TTL / 链接）铺成两种盘面对拍——
/// 连续物理布局（投机窗口全程命中，链页零逐页读）与退化旁路（批量读失败把
/// 深度降 1、调用方丢弃窗口，此后逐页直读）——`scan_page_with` 的逐条 entry
/// （key / value / timestamp / expire_ts 全字段）、TTL 过滤与配额截断
/// （`has_remaining`）必须一致，并与显式期望逐值相等。
///
/// 退化入口选批量读失败而非白读自适应：配额把窗口深度压到 2（`entry_limit / 8`
/// 的下界），该深度下自适应不可达——每窗首页必是链的下一页、必被取走，单窗
/// `wasted == consumed` 恒成立（白读自适应需深度 ≥ 3 即 `entry_limit >= 24`，
/// 见无配额的 `degraded_window_bypasses_speculation`）；两条入口都终结于同
/// 一个旁路（refill 置 `depth = 1` → 调用方 `window = None` 丢弃）。
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
        index_cache: None,
        index_flight: None,
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

    // 盘面一（投机生效）：链页物理连续 fp0..fp3、root@fp4，每个窗口都命中。
    let io_spec = CountingIo {
      inner: MemStore::new(512),
      span_num: AtomicUsize::new(0),
      owned_num: AtomicUsize::new(0),
    };
    let tbl_spec = TableIdent::new("demo", 66).unwrap();
    let mut mapper_spec = PageMapper::new(AllocMode::Append, 0);
    let root_spec = ttl_chain(&io_spec, &tbl_spec, &mut mapper_spec, [0, 1, 2, 3], 4).await;
    let snap_spec = snap(&tbl_spec, &mapper_spec, root_spec);
    let page_spec = quota_ttl_scan(&io_spec, &tbl_spec, &snap_spec)
      .await
      .unwrap();
    assert_eq!(page_spec.entry_li, expected);
    assert!(page_spec.has_remaining, "配额截断必须置 has_remaining");
    // IO 形态锚定投机路径确实生效：owned 仅 index root + 首个 data 页，
    // 两次窗口批量读（fp1 起两页 / fp3 起两页，后段尾页即 root@fp4）。
    assert_eq!(io_spec.owned_num.load(Ordering::Relaxed), 2);
    assert_eq!(io_spec.span_num.load(Ordering::Relaxed), 2);

    // 盘面二（退化旁路）：同一 oplog，P1 散落到 fp10 且窗口第二页 fp11 未落盘，
    // 首次批量读失败 → 深度降 1 → 窗口被丢弃 → P1..P3 逐页直读。
    let io_byp = CountingIo {
      inner: MemStore::new(512),
      span_num: AtomicUsize::new(0),
      owned_num: AtomicUsize::new(0),
    };
    let tbl_byp = TableIdent::new("demo", 67).unwrap();
    let mut mapper_byp = PageMapper::new(AllocMode::Append, 0);
    let root_byp = ttl_chain(&io_byp, &tbl_byp, &mut mapper_byp, [0, 10, 1, 2], 25).await;
    let snap_byp = snap(&tbl_byp, &mapper_byp, root_byp);
    let page_byp = quota_ttl_scan(&io_byp, &tbl_byp, &snap_byp).await.unwrap();
    // 对拍核心：两种盘面逐条一致且与期望一致，配额截断 / has_remaining 同值。
    assert_eq!(page_byp.entry_li, page_spec.entry_li);
    assert_eq!(page_byp.entry_li, expected);
    assert_eq!(page_byp.has_remaining, page_spec.has_remaining);
    // IO 形态锚定旁路确实发生：唯一一次批量读即失败的那次，root + 4 个链页
    // 全部逐页 owned 直读。
    assert_eq!(io_byp.span_num.load(Ordering::Relaxed), 1);
    assert_eq!(io_byp.owned_num.load(Ordering::Relaxed), 5);
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
        index_cache: None,
        index_flight: None,
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
        index_flight: None,
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
      index_cache: None,
      index_flight: None,
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
