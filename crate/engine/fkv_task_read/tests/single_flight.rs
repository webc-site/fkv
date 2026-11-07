//! l2-08 / l2-13 验收：index 页首访 single-flight。
//!
//! 覆盖：关闭在飞表时 N 个并发首访各付一次读盘（IO 放大）；开启后同一
//! `(tbl, page_id, generation)` 合并为一次盘读；多层 index 每层各合并一次；
//! 在飞期间映射代际变更时新旧两代互不串页；缓存命中与在飞表共存；owner
//! 失败 / 取消向全体等待者广播。盘面计数直接取 `SimStore` 的 `Op::ReadPage`
//! 数并逐物理页统计，不是 mock 自报。
//!
//! 关于 `CountIo` 的 `yield_once` 注入（l2-13 ④）：`SimStore` 的读同步完成，
//! 单线程执行器上 owner 会在一次 poll 内跑完，第二个读者根本没有机会挂到
//! 首访登记上。`CountIo` 因此在每次 `read_page` 里精确让出一次，模拟真实
//! 后端「IO 已提交、尚未完成」的在飞状态。这是测试替身对异步后端的建模，
//! 不是生产路径：用例证明的是「合并机制成立」（同一页的盘读收敛到 1），
//! 不是「生产拓扑下的延迟收益」——后者由真实后端的 IO 在飞窗口提供，同步
//! 内存盘上无法复现，注入也造不出更真实的收益数字。
//!
//! 实测去掉该让出后：单层用例从 2 次退化为 9 次、3 层用例从 `[1,1,1,1]`
//! 退化为 `[1,1,1,8]`（owner 在一次 poll 内跑完整条下降并回填 index 缓存，
//! 后续读者转为命中缓存、只剩 data 页各读一次）。即：不注入就没有「N 个
//! 读者同时首访」这个被验证的前提，计数既不是 1 也不是 N。

use std::{
  cell::{Cell, RefCell},
  future::{Future, poll_fn},
  pin::{Pin, pin},
  sync::Arc,
  task::{Context, Poll},
};

use fkv_btree::{CacheCounter, IndexCache, IndexNode};
use fkv_error::KvError;
use fkv_io::PageIo;
use fkv_io_sim::{OpKind, SimStore};
use fkv_mapping::{AllocMode, MappingSnapshot, PageMapper, SnapshotRegistry, SnapshotSource};
use fkv_page::{CompressionType, DataPageBuilder, EntrySpec, IndexPageBuilder};
use fkv_task_read::{IndexRead, PageFlight, ReadCtx, read_entry};
use fkv_types::{FilePageId, NULL_PAGE, PageId, TableIdent};
use futures::{
  stream::{FuturesUnordered, StreamExt},
  task::noop_waker,
};

const PAGE_SIZE: usize = 512;
/// 并发读规模：每次并发读都会走 index 层 → data 页的多次 IO。
const READERS: usize = 8;
/// 多层 index 用例的 index 层数（不含末尾 data 页）。
const INDEX_LEVELS: usize = 3;

/// 计数 IO：每次 `read_page` 先记物理页、精确让出一次，再落到 `SimStore`。
///
/// `SimStore` 的读是同步完成的（无 await 点），直接并发不会交错；让出一次
/// 恰好复现真实后端「首个 IO 在飞」的窗口，使 N 个读者都停在首访未命中处。
struct CountIo {
  inner: SimStore,
  reads: Cell<u32>,
  /// 每次 `read_page` 的物理页 id，按发生顺序；逐页计数据此统计。
  read_log: RefCell<Vec<FilePageId>>,
  /// 置位后，下一次 `read_page` 在让出一次后返回 `IoFail`（模拟盘面读失败，
  /// 但保证 owner 已把等待者挂上），用于验证失败向等待者传播。
  fail_next: Cell<bool>,
}

impl CountIo {
  fn new() -> Self {
    Self {
      inner: SimStore::new(PAGE_SIZE),
      reads: Cell::new(0),
      read_log: RefCell::new(Vec::new()),
      fail_next: Cell::new(false),
    }
  }

  /// 让下一次读盘失败（仅一次）。
  fn arm_fault(&self) {
    self.fail_next.set(true);
  }

  /// 盘面读页总次数（`SimStore` 内部 op 计数）。
  fn disk_reads(&self) -> u32 {
    self.inner.op_count(OpKind::ReadPage)
  }

  /// 指定物理页被读了几次。
  fn reads_of(&self, file_page_id: FilePageId) -> usize {
    self
      .read_log
      .borrow()
      .iter()
      .filter(|read| **read == file_page_id)
      .count()
  }
}

/// 精确让出一次：首次 poll 返回 `Pending` 并自唤醒，第二次 `Ready`。
async fn yield_once() {
  let mut yielded = false;
  poll_fn(move |cx| {
    if yielded {
      return Poll::Ready(());
    }
    yielded = true;
    cx.waker().wake_by_ref();
    Poll::Pending
  })
  .await;
}

/// 单次 poll（noop waker）：精确控制「谁先进入在飞表」与取消发生的时刻。
fn poll_once<F: Future>(fut: Pin<&mut F>) -> Poll<F::Output> {
  let waker = noop_waker();
  let mut cx = Context::from_waker(&waker);
  fut.poll(&mut cx)
}

/// 让 `FuturesUnordered` 里当前就绪的 future 各 poll 一次（noop waker）。
///
/// 返回 `Pending` 表示本轮全部停在 IO 让出点（各自已登记在飞）；任一 future
/// 完成则立即返回 `Ready`，调用方据此断言时序。
fn poll_round<F: Future>(set: &mut FuturesUnordered<F>) -> Poll<Option<F::Output>> {
  let mut next = pin!(set.next());
  poll_once(next.as_mut())
}

impl PageIo for CountIo {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> fkv_error::Result<()> {
    self.reads.set(self.reads.get() + 1);
    self.read_log.borrow_mut().push(file_page_id);
    yield_once().await;
    if self.fail_next.replace(false) {
      return Err(KvError::IoFail);
    }
    self.inner.read_page(tbl, file_page_id, dst).await
  }

  async fn write_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    src: &[u8],
  ) -> fkv_error::Result<()> {
    self.inner.write_page(tbl, file_page_id, src).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> fkv_error::Result<()> {
    self.inner.sync_data(tbl).await
  }
}

fn data_page_with(value: &[u8]) -> Vec<u8> {
  let mut builder = DataPageBuilder::new(PAGE_SIZE, 16).unwrap();
  builder
    .add(EntrySpec {
      key: b"a",
      value,
      ts: 1,
      expire_ts: 0,
      overflow: false,
      compression: CompressionType::None,
      large_value: false,
    })
    .unwrap();
  builder.finish().unwrap()
}

/// index 页：`leaf` 决定页类型，`leftmost` 是唯一子页。
fn index_page_kind(leaf: bool, leftmost: PageId) -> Vec<u8> {
  let mut builder = IndexPageBuilder::new(PAGE_SIZE, 16, leaf).unwrap();
  builder.add(b"", leftmost).unwrap();
  builder.finish().unwrap()
}

/// 单层树的 root：leaf index 页。
fn index_page(leftmost: PageId) -> Vec<u8> {
  index_page_kind(true, leftmost)
}

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

/// 固定「缓存 + 在飞表」的读上下文；`flight` 为 `None` 即关闭去重。
fn read_ctx<'a>(
  snapshot: &'a MappingSnapshot,
  cache: &'a RefCell<IndexCache>,
  flight: Option<&'a RefCell<PageFlight>>,
) -> ReadCtx<'a> {
  ReadCtx {
    index: IndexRead::new(Some(cache), flight),
    ..ReadCtx::new(snapshot, IndexRead::none())
  }
}

/// 一棵测试树：`levels` 层 index + 末尾 1 个 data 页。
struct Tree {
  io: CountIo,
  tbl: TableIdent,
  mapper: PageMapper,
  root_id: PageId,
  /// 下降顺序：root → … → leaf index → data 页。
  fids: Vec<FilePageId>,
  /// 与 `fids` 对齐的页字节（缓存预热用）。
  pages: Vec<Vec<u8>>,
}

impl Tree {
  /// 建树并全部落盘：data 页在最小物理页，root 在最大物理页。
  async fn build(name: &str, levels: usize, value: &[u8]) -> Self {
    assert!(levels >= 1, "至少需要 1 层 index");
    let io = CountIo::new();
    let tbl = TableIdent::new(name, 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let data_id = mapper.allocate_page();
    // 自底向上分配：`index_ids[0]` 是 leaf，末尾元素是 root。
    let index_ids: Vec<PageId> = (0..levels).map(|_| mapper.allocate_page()).collect();
    let mut fids = Vec::with_capacity(levels + 1);
    let mut pages = Vec::with_capacity(levels + 1);
    // 自 root 向下逐层写页：leaf 指向 data 页，其余层指向下一层。
    for (level, id) in index_ids.iter().enumerate().rev() {
      let child = if level == 0 {
        data_id
      } else {
        index_ids[level - 1]
      };
      let page = index_page_kind(level == 0, child);
      let file_page_id = mapper.file_page_id(*id).unwrap();
      io.write_page(&tbl, file_page_id, &page).await.unwrap();
      fids.push(file_page_id);
      pages.push(page);
    }
    let data_page = data_page_with(value);
    let data_fp = mapper.file_page_id(data_id).unwrap();
    io.write_page(&tbl, data_fp, &data_page).await.unwrap();
    fids.push(data_fp);
    pages.push(data_page);
    let root_id = *index_ids.last().unwrap();
    Self {
      io,
      tbl,
      mapper,
      root_id,
      fids,
      pages,
    }
  }

  fn snapshot(&self) -> MappingSnapshot {
    snap(&self.tbl, &self.mapper, self.root_id)
  }
}

/// 并发跑 `READERS` 个点读，返回每个读者读到的 value。
async fn read_all(io: &CountIo, tbl: &TableIdent, ctx: ReadCtx<'_>) -> Vec<Vec<u8>> {
  let mut set = FuturesUnordered::new();
  for _ in 0..READERS {
    set.push(Box::pin(read_entry(io, tbl, b"a", PAGE_SIZE, ctx)));
  }
  let mut values = Vec::with_capacity(READERS);
  while let Some(result) = set.next().await {
    let entry = result.expect("read ok").expect("key must exist");
    values.push(entry.value);
  }
  values
}

/// N 个读任务并发首访同一 root index 页：返回盘面 `read_page` 总次数。
///
/// `flight` 为 false 时 `index_flight` 传 `None`，等价关闭在飞去重（改前行为）。
async fn concurrent_reads(flight: bool) -> u32 {
  let tree = Tree::build("single_flight", 1, b"A").await;
  let cache = RefCell::new(IndexCache::with_capacity_bytes(0));
  let flight_table = RefCell::new(PageFlight::default());
  let snapshot = tree.snapshot();
  let ctx = read_ctx(&snapshot, &cache, flight.then_some(&flight_table));
  let values = read_all(&tree.io, &tree.tbl, ctx).await;
  assert!(
    values.iter().all(|value| value == b"A"),
    "所有读者都应读到 A"
  );
  // 若在飞表生效，root index 页只读 1 次；data 页也只读 1 次。
  assert_eq!(
    tree.io.reads.get(),
    tree.io.disk_reads(),
    "计数包装与 SimStore 盘面计数不一致"
  );
  tree.io.disk_reads()
}

/// 多层树 × N 读者：返回按下降顺序（root → data）的逐页盘读次数。
async fn multi_level_reads(flight: bool) -> Vec<usize> {
  let tree = Tree::build("single_flight_multi", INDEX_LEVELS, b"A").await;
  let cache = RefCell::new(IndexCache::with_capacity_bytes(0));
  let flight_table = RefCell::new(PageFlight::default());
  let snapshot = tree.snapshot();
  let ctx = read_ctx(&snapshot, &cache, flight.then_some(&flight_table));
  let values = read_all(&tree.io, &tree.tbl, ctx).await;
  assert!(
    values.iter().all(|value| value == b"A"),
    "所有读者都应读到 A"
  );
  assert_eq!(
    tree.io.reads.get(),
    tree.io.disk_reads(),
    "计数包装与 SimStore 盘面计数不一致"
  );
  tree
    .fids
    .iter()
    .map(|file_page_id| tree.io.reads_of(*file_page_id))
    .collect()
}

/// 关闭在飞表：每个读者各读一次 root index 页与 data 页（IO 放大）。
#[test]
fn without_single_flight_each_reader_reads_itself() {
  pollster::block_on(async {
    assert_eq!(
      concurrent_reads(false).await,
      (READERS * 2) as u32,
      "无在飞去重时 N 个并发读者应各付一次 index / data IO"
    );
  });
}

/// 开启在飞表：同页并发首访合并为一次 IO（C++ `WaitingZone` 行为）。
#[test]
fn index_page_single_flight_merges_concurrent_first_visit() {
  pollster::block_on(async {
    assert_eq!(
      concurrent_reads(true).await,
      2,
      "root index 页 + data 页各只应读一次盘"
    );
  });
}

/// 多层 index（l2-13 ②）：3 层 index + 1 个 data 页 × N 读者。
///
/// 每个读者都要下降 4 页，但相邻层的首访被在飞表逐层合并，因此每层各只
/// 读 1 次盘（总读盘 == 层数）。反向对照：关闭在飞表时每层各被 N 个读者
/// 各读一次，说明逐页计数的判别力来自合并本身。
#[test]
fn multi_level_index_merges_one_read_per_layer() {
  pollster::block_on(async {
    assert_eq!(
      multi_level_reads(true).await,
      vec![1; INDEX_LEVELS + 1],
      "3 层 index + data 页应各只读一次盘"
    );
    assert_eq!(
      multi_level_reads(false).await,
      vec![READERS; INDEX_LEVELS + 1],
      "无在飞去重时每层每个读者各读一次"
    );
  });
}

/// 在飞期间映射代际变更（l2-13 ①）。
///
/// 旧代 owner 停在读盘让出点时，把 root / data 各 COW 到新物理页并写新代
/// 内容，再让新代读者切入：新代读者必须各自成立新 owner（不挂到旧代登记
/// 上）、读新代盘面，旧代读者仍读旧页。
///
/// 判据：盘读 == 新旧两代之和（旧 root / 旧 data / 新 root / 新 data 各 1
/// 次，每代内部 READERS 个读者仍合并）。若内层键漏了 generation，新代读者
/// 会挂到旧代在飞登记上：总读盘停在 2 且新代读到旧值。
#[test]
fn generation_change_while_in_flight_keeps_generations_apart() {
  pollster::block_on(async {
    const OLD_READERS: usize = READERS / 2;
    const NEW_READERS: usize = READERS / 2;

    let io = CountIo::new();
    let tbl = TableIdent::new("single_flight_generation", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let data_id = mapper.allocate_page();
    let root_id = mapper.allocate_page();
    let old_data_fp = mapper.file_page_id(data_id).unwrap();
    let old_root_fp = mapper.file_page_id(root_id).unwrap();
    io.write_page(&tbl, old_data_fp, &data_page_with(b"A"))
      .await
      .unwrap();
    io.write_page(&tbl, old_root_fp, &index_page(data_id))
      .await
      .unwrap();

    let old_snapshot = snap(&tbl, &mapper, root_id);
    let cache = RefCell::new(IndexCache::with_capacity_bytes(0));
    let flight_table = RefCell::new(PageFlight::default());

    // 旧代读者全部落到首访登记处：owner 停在读盘的让出点，其余挂在同键上。
    let mut old_readers = FuturesUnordered::new();
    for _ in 0..OLD_READERS {
      old_readers.push(Box::pin(read_entry(
        &io,
        &tbl,
        b"a",
        PAGE_SIZE,
        read_ctx(&old_snapshot, &cache, Some(&flight_table)),
      )));
    }
    assert!(
      poll_round(&mut old_readers).is_pending(),
      "旧代应停在首访读盘让出点"
    );
    assert_eq!(io.reads.get(), 1, "同代并发首访只应有 owner 读盘");

    // 在飞期间改映射：root / data 各 COW 到新物理页，写新代内容。
    let new_root_fp = mapper.remap_page(root_id).unwrap();
    let new_data_fp = mapper.remap_page(data_id).unwrap();
    io.write_page(&tbl, new_root_fp, &index_page(data_id))
      .await
      .unwrap();
    io.write_page(&tbl, new_data_fp, &data_page_with(b"Z"))
      .await
      .unwrap();
    let new_snapshot = snap(&tbl, &mapper, root_id);
    assert_ne!(
      old_snapshot.mapping_generation(),
      new_snapshot.mapping_generation(),
      "COW 必须推进映射代际"
    );

    // 新代读者切入：不得复用旧代在飞登记，必须各自读新代盘面。
    let mut new_readers = FuturesUnordered::new();
    for _ in 0..NEW_READERS {
      new_readers.push(Box::pin(read_entry(
        &io,
        &tbl,
        b"a",
        PAGE_SIZE,
        read_ctx(&new_snapshot, &cache, Some(&flight_table)),
      )));
    }
    assert!(
      poll_round(&mut new_readers).is_pending(),
      "新代应停在首访读盘让出点"
    );
    assert_eq!(
      io.reads.get(),
      2,
      "新代读者不得挂到旧代在飞登记上：应各自成立新 owner 并读一次盘"
    );

    // 各自收完：两代互不串页。
    let mut old_values = Vec::with_capacity(OLD_READERS);
    while let Some(result) = old_readers.next().await {
      old_values.push(result.expect("旧代读成功").expect("key 必须存在").value);
    }
    let mut new_values = Vec::with_capacity(NEW_READERS);
    while let Some(result) = new_readers.next().await {
      new_values.push(result.expect("新代读成功").expect("key 必须存在").value);
    }
    assert!(
      old_values.iter().all(|value| value == b"A"),
      "旧代读者必须读旧代页: {old_values:?}"
    );
    assert!(
      new_values.iter().all(|value| value == b"Z"),
      "新代读者必须读新代页: {new_values:?}"
    );

    // 每代内部仍合并：旧 root / 旧 data / 新 root / 新 data 各 1 次。
    let per_page = (
      io.reads_of(old_root_fp),
      io.reads_of(old_data_fp),
      io.reads_of(new_root_fp),
      io.reads_of(new_data_fp),
    );
    assert_eq!(per_page, (1, 1, 1, 1), "同代读者应合并、跨代不得共享读盘");
    assert_eq!(io.reads.get(), 4, "总读盘 == 新旧两代之和");
  });
}

/// 缓存命中 + 在飞表共存（l2-13 ③）。
///
/// root index 页预热进缓存，下一层 index 与 data 页仍是首访：root 由缓存
/// 供页（0 次盘读、每个读者各 1 次命中），leaf index 与 data 页各由在飞表
/// 把 READERS 个首访合并成 1 次盘读。
///
/// 这里必须用真实容量（`with_capacity_bytes(0)` 在本仓语义是「不限制」，
/// 不是「不缓存」）：预热后能稳定命中，才谈得上与在飞表共存。
#[test]
fn cache_hit_and_in_flight_coexist() {
  pollster::block_on(async {
    let tree = Tree::build("single_flight_cache", 2, b"A").await;
    let counter = Arc::new(CacheCounter::default());
    let cache = RefCell::new(IndexCache::with_counter(PAGE_SIZE * 8, counter.clone()));
    let flight_table = RefCell::new(PageFlight::default());
    let snapshot = tree.snapshot();
    // 预热 root：条目代际与读路径一致，后续读者必须命中。
    cache.borrow_mut().insert(
      tree.root_id,
      IndexNode::new(tree.fids[0], &tree.pages[0]).unwrap(),
      snapshot.mapping_generation(),
    );
    assert_eq!(cache.borrow().len(), 1, "预热后缓存里应只有 root");

    let ctx = read_ctx(&snapshot, &cache, Some(&flight_table));
    let values = read_all(&tree.io, &tree.tbl, ctx).await;
    assert!(
      values.iter().all(|value| value == b"A"),
      "所有读者都应读到 A"
    );

    assert_eq!(
      counter.hit(),
      READERS as u64,
      "每个读者都应命中预热的 root 页"
    );
    assert_eq!(
      tree.io.reads_of(tree.fids[0]),
      0,
      "root 命中缓存，不应产生盘读"
    );
    assert_eq!(
      tree.io.reads_of(tree.fids[1]),
      1,
      "leaf index 首访应被在飞表合并成 1 次盘读"
    );
    assert_eq!(
      tree.io.reads_of(tree.fids[2]),
      1,
      "data 页首访应被在飞表合并成 1 次盘读"
    );
    assert_eq!(tree.io.disk_reads(), 2, "总盘读 == 未命中的两层各 1 次");
    assert_eq!(
      cache.borrow().len(),
      2,
      "root 为预热条目，leaf 由 owner 回填"
    );
  });
}

/// 失败传播：owner 的首访读盘失败时，所有等待者拿到**同一个**错误，
/// 且盘面只尝试一次（不会因失败退化成 N 次重读）。
#[test]
fn single_flight_propagates_owner_error_to_waiters() {
  pollster::block_on(async {
    let tree = Tree::build("single_flight_fail", 1, b"A").await;
    let cache = RefCell::new(IndexCache::with_capacity_bytes(0));
    let flight_table = RefCell::new(PageFlight::default());
    let snapshot = tree.snapshot();
    let ctx = read_ctx(&snapshot, &cache, Some(&flight_table));

    // 首个读者（owner）的读盘失败；其余读者已挂在同一在飞键上。
    tree.io.arm_fault();
    let mut set = FuturesUnordered::new();
    for _ in 0..READERS {
      set.push(Box::pin(read_entry(
        &tree.io, &tree.tbl, b"a", PAGE_SIZE, ctx,
      )));
    }
    let mut err_li = Vec::new();
    while let Some(result) = set.next().await {
      err_li.push(result.expect_err("owner 失败必须传播给所有等待者"));
    }
    assert_eq!(err_li.len(), READERS);
    assert!(
      err_li.iter().all(|err| *err == KvError::IoFail),
      "等待者必须拿到 owner 的原始错误: {err_li:?}"
    );
    assert_eq!(
      tree.io.reads.get(),
      1,
      "失败后不应退化为每个等待者各重读一次"
    );
  });
}

/// 取消传播（l2-12）：owner future 被 drop 时，`LoadGuard::drop` 向全体等待者
/// 广播 `TryAgain`，且不依赖超时——每个等待者在广播后的**下一次** poll 就必须
/// `Ready(Err(TryAgain))`。
///
/// 手工逐次 poll：owner 先停在首访读盘的让出点，随后登记的读者全部成为等待者
/// （不各读一次盘）；drop owner 后若广播缺失，等待者会停在 `Pending`，断言立即
/// 变红。本用例不 sleep、不等超时。
#[test]
fn owner_cancel_broadcasts_try_again_to_all_waiters() {
  pollster::block_on(async {
    let tree = Tree::build("single_flight_cancel", 1, b"A").await;
    let cache = RefCell::new(IndexCache::with_capacity_bytes(0));
    let flight_table = RefCell::new(PageFlight::default());
    let snapshot = tree.snapshot();
    let ctx = read_ctx(&snapshot, &cache, Some(&flight_table));

    // 先让 owner 进入首访登记，停在读盘的让出点。
    let mut owner = Box::pin(read_entry(&tree.io, &tree.tbl, b"a", PAGE_SIZE, ctx));
    assert!(
      poll_once(owner.as_mut()).is_pending(),
      "owner 应停在首访读盘处"
    );

    // 再登记其余读者：它们必须挂到同一在飞键上，且尚未读盘。
    let mut waiters: Vec<_> = (1..READERS)
      .map(|_| Box::pin(read_entry(&tree.io, &tree.tbl, b"a", PAGE_SIZE, ctx)))
      .collect();
    for waiter in &mut waiters {
      assert!(
        poll_once(waiter.as_mut()).is_pending(),
        "等待者应挂在 owner 的在飞登记上"
      );
    }
    assert_eq!(tree.io.reads.get(), 1, "等待者不得各自读盘");

    // 取消 owner：广播必须让全体等待者立刻可推进，而不是永远挂起。
    drop(owner);
    for (i, waiter) in waiters.iter_mut().enumerate() {
      match poll_once(waiter.as_mut()) {
        Poll::Ready(Err(KvError::TryAgain)) => {}
        Poll::Ready(other) => panic!("等待者 #{i} 应收到 TryAgain，实际 {other:?}"),
        Poll::Pending => panic!("等待者 #{i} 挂起：owner 取消的 TryAgain 广播缺失"),
      }
    }

    // 被取消的登记必须已摘除：重试成为新的 owner 并读到值（TryAgain 可重试）。
    let entry = read_entry(&tree.io, &tree.tbl, b"a", PAGE_SIZE, ctx)
      .await
      .expect("取消后重试应成功")
      .expect("key 必须存在");
    assert_eq!(entry.value, b"A");
  });
}
