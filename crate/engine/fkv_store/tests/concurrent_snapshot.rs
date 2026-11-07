//! 27c：真实并发读写下的 COW 快照自洽与 GC 保留验收。
//!
//! 读请求以独立 compio task 启动（与 shard 读 spawn 同型），通过可控 IO
//! 在首次 `read_page` 精确挂起，不手动 poll future、不依赖 sleep；挂起期间
//! 执行真实 `Store::put`（COW remap）或 `Store::gc`。读任务只持
//! `MappingSnapshot` 与共享 IO 句柄，与写任务在同一运行时上并发推进。

use std::{
  cell::{Cell, RefCell},
  collections::HashSet,
  future::poll_fn,
  rc::Rc,
  sync::Arc,
  task::{Poll, Waker},
};

use compio::runtime::spawn;
use fkv_btree::{BatchEntry, CacheCounter, IndexCache};
use fkv_error::{KvError, Result};
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_io_mem::MemStore;
use fkv_mapping::MappingSnapshot;
use fkv_store::{AllocMode, Store};
use fkv_task_read::{IndexRead, ReadCtx, read_entry, read_floor_entry};
use fkv_task_scan::{ScanArg, ScanTree, scan_entries};
use fkv_types::{FileId, FilePageId, TableIdent};

const PAGE_SIZE: usize = 512;
const RESTART_INTERVAL: u16 = 16;

fn key(index: u32) -> Vec<u8> {
  format!("k{index:05}").into_bytes()
}

/// 每个 key 的值都带唯一序号，混读新旧页会直接体现在结果上。
fn value(tag: u8, index: u32) -> Vec<u8> {
  let mut value = vec![tag; 48];
  value[..4].copy_from_slice(&index.to_be_bytes());
  value
}

fn upsert_batch(tag: u8, ts: u64, index_li: impl IntoIterator<Item = u32>) -> Vec<BatchEntry> {
  index_li
    .into_iter()
    .map(|index| BatchEntry::upsert(key(index), value(tag, index), ts))
    .collect()
}

fn new_store(shift: u8) -> Store<SharedIo> {
  new_store_with_mode(shift, AllocMode::Append)
}

fn new_store_with_mode(shift: u8, alloc_mode: AllocMode) -> Store<SharedIo> {
  Store::new(
    SharedIo::with_pages_per_file(PAGE_SIZE, shift),
    PAGE_SIZE,
    RESTART_INTERVAL,
  )
  .with_pages_per_file_shift(shift)
  .with_alloc_mode(alloc_mode)
}

/// 首次 `read_page` 精确挂起一次的读闸门。
///
/// 读任务在 `pass` 里置 `blocked` 并停住；测试等 `wait_blocked` 返回后执行
/// 写 / GC，再 `release`。全部状态单线程 `Cell` / `RefCell`，无 sleep。
#[derive(Default)]
struct ReadGate {
  armed: Cell<bool>,
  blocked: Cell<bool>,
  released: Cell<bool>,
  blocked_waker: RefCell<Option<Waker>>,
  read_waker: RefCell<Option<Waker>>,
}

impl ReadGate {
  fn new() -> Self {
    Self {
      armed: Cell::new(false),
      ..Self::default()
    }
  }

  /// 让护栏只对被测读任务生效；建表/取期望值等准备读必须绕过护栏。
  fn arm(&self) {
    self.armed.set(true);
  }

  /// 读任务侧：首个 `read_page` 挂起，直到测试调用 `release`。
  async fn pass(&self) {
    if !self.armed.replace(false) {
      return;
    }
    self.blocked.set(true);
    if let Some(waker) = self.blocked_waker.borrow_mut().take() {
      waker.wake();
    }
    poll_fn(|cx| {
      if self.released.get() {
        return Poll::Ready(());
      }
      *self.read_waker.borrow_mut() = Some(cx.waker().clone());
      Poll::Pending
    })
    .await;
  }

  /// 测试侧：等到读任务真正停在第一个 IO 上。
  async fn wait_blocked(&self) {
    poll_fn(|cx| {
      if self.blocked.get() {
        return Poll::Ready(());
      }
      *self.blocked_waker.borrow_mut() = Some(cx.waker().clone());
      Poll::Pending
    })
    .await;
  }

  fn release(&self) {
    self.released.set(true);
    if let Some(waker) = self.read_waker.borrow_mut().take() {
      waker.wake();
    }
  }
}

/// 可在读任务与写任务之间共享的 IO 句柄。
///
/// `Store` 独占持有本类型，测试再克隆一份交给读任务；两份指向同一
/// `Rc<MemStore>`，读任务不借用 `Store`，因此写路径可同时独占 `&mut Store`。
#[derive(Clone)]
struct SharedIo {
  inner: Rc<MemStore>,
  gate: Rc<ReadGate>,
}

impl SharedIo {
  fn with_pages_per_file(page_size: usize, shift: u8) -> Self {
    Self {
      inner: Rc::new(MemStore::with_pages_per_file(page_size, shift)),
      gate: Rc::new(ReadGate::new()),
    }
  }

  fn gate(&self) -> Rc<ReadGate> {
    Rc::clone(&self.gate)
  }
}

impl PageIo for SharedIo {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    self.gate.pass().await;
    self.inner.read_page(tbl, file_page_id, dst).await
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    self.inner.write_page(tbl, file_page_id, src).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.inner.sync_data(tbl).await
  }
}

impl ManifestIo for SharedIo {
  async fn reset_manifest(&self, tbl: &TableIdent, size: u64) -> Result<()> {
    self.inner.reset_manifest(tbl, size).await
  }

  async fn reset_manifest_file(&self, tbl: &TableIdent, name: &str, size: u64) -> Result<()> {
    self.inner.reset_manifest_file(tbl, name, size).await
  }

  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.inner.append_manifest(tbl, record).await
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.inner.switch_manifest(tbl, record).await
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    self.inner.read_manifest(tbl).await
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
    self.inner.drop_manifest(tbl).await
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self.inner.write_manifest_file(tbl, name, record).await
  }

  async fn append_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self.inner.append_manifest_file(tbl, name, record).await
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    self.inner.list_manifest_file_name_li(tbl).await
  }

  async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    self.inner.list_sidecar_file_name_li(tbl).await
  }

  async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.inner.remove_sidecar_file_name(tbl, name).await
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    self.inner.read_manifest_file(tbl, name).await
  }

  async fn seed_local_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
    bytes: &[u8],
  ) -> Result<()> {
    self.inner.seed_local_manifest_file(tbl, name, bytes).await
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.inner.remove_manifest_file(tbl, name).await
  }
}

impl SegmentIo for SharedIo {
  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    self.inner.read_segment(tbl, segment_fp_id, dst).await
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> Result<()> {
    self.inner.write_segment(tbl, segment_fp_id, src).await
  }
}

impl FileIo for SharedIo {
  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.inner.list_file_id_li(tbl).await
  }

  async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>> {
    self.inner.read_file(tbl, file_id).await
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()> {
    self.inner.write_file(tbl, file_id, bytes).await
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>> {
    self.inner.file_size(tbl, file_id).await
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.inner.remove_file(tbl, file_id).await
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.inner.list_segment_file_id_li(tbl).await
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.inner.remove_segment_file(tbl, file_id).await
  }
}

/// 点读（27c 用例 1）：读挂起在首个 `read_page`，写者整批覆盖并 remap；
/// 恢复后必须返回读起点快照里的旧值。
#[compio::test]
async fn suspended_get_reads_start_snapshot_after_put() {
  let tbl = TableIdent::new("snapshot_get", 1).unwrap();
  let mut store = new_store(18);
  store
    .put(&tbl, &upsert_batch(b'A', 1, 0..40))
    .await
    .unwrap();
  let expected = store.get(&tbl, &key(7)).await.unwrap().unwrap();

  let snapshot = store.snapshot(&tbl).unwrap();
  let io = store.page_io().clone();
  let gate = io.gate();
  gate.arm();
  let read_tbl = tbl.clone();
  let handle = spawn(async move {
    read_entry(
      &io,
      &read_tbl,
      &key(7),
      PAGE_SIZE,
      ReadCtx::new(&snapshot, IndexRead::none()),
    )
    .await
    .map(|entry| entry.map(|entry| entry.value))
  });
  gate.wait_blocked().await;

  // 读挂起期间执行真实写入：40 个 key 全部换值并 remap。
  store
    .put(&tbl, &upsert_batch(b'B', 2, 0..40))
    .await
    .unwrap();
  assert_eq!(
    store.get(&tbl, &key(7)).await.unwrap(),
    Some(value(b'B', 7)),
    "写必须已提交"
  );

  gate.release();
  let got = handle.await.unwrap().unwrap();
  assert_eq!(
    got,
    Some(expected),
    "读结果必须等于读起点快照，不能混入写后的新页"
  );
}

/// floor（27c 用例 1 的另一读类型）：读挂起期间删除整批 key；恢复后
/// 仍按旧快照返回删除前的最大 key。
#[compio::test]
async fn suspended_floor_reads_start_snapshot_after_delete() {
  let tbl = TableIdent::new("snapshot_floor", 2).unwrap();
  let mut store = new_store(18);
  store
    .put(&tbl, &upsert_batch(b'A', 1, 0..40))
    .await
    .unwrap();
  let expected = store.floor(&tbl, b"zz").await.unwrap();

  let snapshot = store.snapshot(&tbl).unwrap();
  let io = store.page_io().clone();
  let gate = io.gate();
  gate.arm();
  let read_tbl = tbl.clone();
  let handle = spawn(async move {
    read_floor_entry(
      &io,
      &read_tbl,
      b"zz",
      PAGE_SIZE,
      ReadCtx::new(&snapshot, IndexRead::none()),
    )
    .await
  });
  gate.wait_blocked().await;

  let delete_li: Vec<BatchEntry> = (0..40)
    .map(|index| BatchEntry::delete(key(index), 2))
    .collect();
  store.put(&tbl, &delete_li).await.unwrap();
  assert_eq!(
    store.floor(&tbl, b"zz").await.unwrap_err(),
    KvError::NotFound,
    "写必须已删除旧 floor"
  );

  gate.release();
  let got = handle.await.unwrap().unwrap().unwrap();
  assert_eq!(got.key, expected.key);
  assert_eq!(got.value, expected.value);
}

/// scan（27c 用例 2）：scan 跨多个 data page，写者只改部分页（并删除部分
/// key）。恢复后整条遍历链与叶子内容都必须来自同一旧快照。
#[compio::test]
async fn suspended_scan_reads_start_snapshot_across_pages() {
  let tbl = TableIdent::new("snapshot_scan", 3).unwrap();
  let mut store = new_store(3);
  let total = 160u32;
  store
    .put(&tbl, &upsert_batch(b'A', 1, 0..total))
    .await
    .unwrap();
  let expected_li: Vec<(Vec<u8>, Vec<u8>)> = (0..total)
    .map(|index| (key(index), value(b'A', index)))
    .collect();

  let snapshot = store.snapshot(&tbl).unwrap();
  assert!(
    snapshot.data_tbl().mapping_count() > 4,
    "scan 必须跨多个 data / index 页"
  );
  let io = store.page_io().clone();
  let gate = io.gate();
  gate.arm();
  let read_tbl = tbl.clone();
  let handle = spawn(async move {
    let entry_li = scan_entries(
      &io,
      ScanArg {
        tbl: &read_tbl,
        snapshot: &snapshot,
        tree: ScanTree::Data,
        begin: b"",
        end: b"",
        page_size: PAGE_SIZE,
        max_entries: 0,
        dict: None,
        index_cache: None,
        index_flight: None,
        ttl_filter_now_ms: None,
      },
    )
    .await?;
    Ok::<_, KvError>(
      entry_li
        .into_iter()
        .map(|entry| (entry.key, entry.value))
        .collect::<Vec<_>>(),
    )
  });
  gate.wait_blocked().await;

  // 只改一部分页：每 3 个 key 覆盖，每 7 个 key 删除，制造页内混合。
  let mut write_li = Vec::new();
  for index in 0..total {
    if index % 3 == 0 {
      write_li.push(BatchEntry::upsert(key(index), value(b'B', index), 2));
    } else if index % 7 == 0 {
      write_li.push(BatchEntry::delete(key(index), 2));
    }
  }
  store.put(&tbl, &write_li).await.unwrap();
  let current_li = store.scan(&tbl, b"", b"", 0).await.unwrap();
  assert_ne!(current_li, expected_li, "写必须已改变当前扫描结果");

  gate.release();
  let got = handle.await.unwrap().unwrap();
  assert_eq!(got, expected_li, "scan 结果必须来自读起点快照");
}

/// GC（27c 用例 4）：读任务持快照挂起时，快照独占的旧文件不得回收；
/// 读任务结束（快照释放）后下一轮 GC 才删除。
#[compio::test]
async fn gc_retains_files_referenced_by_suspended_read_snapshot() {
  const SHIFT: u8 = 1;
  let tbl = TableIdent::new("snapshot_gc", 4).unwrap();
  let mut store = new_store(SHIFT);
  store
    .put(&tbl, &upsert_batch(b'A', 1, 0..40))
    .await
    .unwrap();

  // 覆盖写之前已封存的文件：覆盖后只可能被旧快照引用。
  let tail_file = store.mapper(&tbl).unwrap().max_fp_id() >> SHIFT;
  let frozen_li: Vec<FileId> = store
    .page_io()
    .list_file_id_li(&tbl)
    .await
    .unwrap()
    .into_iter()
    .filter(|file_id| *file_id < tail_file)
    .collect();
  assert!(
    frozen_li.len() >= 2,
    "需要多个已封存文件才能覆盖快照独占场景: {frozen_li:?}"
  );

  let snapshot = store.snapshot(&tbl).unwrap();
  let io = store.page_io().clone();
  let gate = io.gate();
  gate.arm();
  let read_tbl = tbl.clone();
  let handle = spawn(async move {
    read_entry(
      &io,
      &read_tbl,
      &key(0),
      PAGE_SIZE,
      ReadCtx::new(&snapshot, IndexRead::none()),
    )
    .await
    .map(|entry| entry.map(|entry| entry.value))
  });
  gate.wait_blocked().await;

  store
    .put(&tbl, &upsert_batch(b'B', 2, 0..40))
    .await
    .unwrap();
  let mapped_file_li: HashSet<FileId> = store
    .mapper(&tbl)
    .unwrap()
    .tbl()
    .iter_value_li()
    .filter(|value| fkv_types::is_file_page_id(*value))
    .map(|value| fkv_types::decode_id(value) >> SHIFT)
    .collect();
  // 单次 COW 只重写经过的路径，未被触碰的 index/leaf 页仍可能引用封存文件；
  // 这里只要求「至少有一个文件仅被旧快照引用」，正是 27c 要钉住的独占语义。
  let snapshot_only_li: Vec<FileId> = frozen_li
    .iter()
    .copied()
    .filter(|file_id| !mapped_file_li.contains(file_id))
    .collect();
  assert!(
    !snapshot_only_li.is_empty(),
    "覆盖后应至少有一个文件只被旧快照引用: frozen={frozen_li:?} mapped={mapped_file_li:?}"
  );

  let removed = store.file_gc(&tbl, SHIFT).await.unwrap();
  let file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
  for file_id in &snapshot_only_li {
    assert!(
      file_li.contains(file_id),
      "file {file_id} 仍被挂起读的快照引用: {file_li:?} removed={removed}"
    );
  }

  gate.release();
  assert_eq!(handle.await.unwrap().unwrap(), Some(value(b'A', 0)));
  assert_eq!(store.snapshot_num(&tbl), 0, "读任务结束后快照必须已释放");

  store.file_gc(&tbl, SHIFT).await.unwrap();
  let file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
  for file_id in &snapshot_only_li {
    assert!(!file_li.contains(file_id), "file {file_id} 应已回收");
  }
  for index in 0..40 {
    assert_eq!(
      store.get(&tbl, &key(index)).await.unwrap(),
      Some(value(b'B', index)),
      "GC 不得误删当前数据: {index}"
    );
  }
}

/// 索引缓存代际（27c 用例 3）：旧快照读者与写后新快照读者共用同一份
/// `IndexCache`；写后新代回填的 `IndexNode` 不能被旧快照命中。
async fn read_with_cache(
  io: &SharedIo,
  tbl: &TableIdent,
  snapshot: &MappingSnapshot,
  cache: &RefCell<IndexCache>,
  index: u32,
) -> Result<Option<Vec<u8>>> {
  read_entry(
    io,
    tbl,
    &key(index),
    PAGE_SIZE,
    ReadCtx {
      index: IndexRead::new(Some(cache), None),
      ..ReadCtx::new(snapshot, IndexRead::none())
    },
  )
  .await
  .map(|entry| entry.map(|entry| entry.value))
}

/// test-11：Pooled 生产默认下「读任务持快照 + 写任务覆盖并 reclaim」不得串读。
///
/// 与 Append 的挂起读用例同构，但额外钉住回收协议：
/// - 快照存活期间（水位 = 快照代际），两轮覆盖写都把旧页留在 retired 队列；
///   若 reclaim 忽略水位提前回池，后续写会复用旧页，挂起读恢复后读到新值；
/// - 快照释放后下一轮写开批前回收，物理页高水位不再随覆盖轮次增长。
#[compio::test]
async fn pooled_snapshot_blocks_reclaim_until_suspended_read_finishes() {
  let tbl = TableIdent::new("snapshot_pooled", 6).unwrap();
  let mut store = new_store_with_mode(18, AllocMode::Pooled);
  store
    .put(&tbl, &upsert_batch(b'A', 1, 0..40))
    .await
    .unwrap();
  let expected = store.get(&tbl, &key(7)).await.unwrap().unwrap();
  let max_after_first = store.mapper(&tbl).unwrap().max_fp_id();

  let snapshot = store.snapshot(&tbl).unwrap();
  let io = store.page_io().clone();
  let gate = io.gate();
  gate.arm();
  let read_tbl = tbl.clone();
  let handle = spawn(async move {
    read_entry(
      &io,
      &read_tbl,
      &key(7),
      PAGE_SIZE,
      ReadCtx::new(&snapshot, IndexRead::none()),
    )
    .await
    .map(|entry| entry.map(|entry| entry.value))
  });
  gate.wait_blocked().await;

  // 第一轮覆盖：旧页 COW 后进入 retired 队列，但快照仍在，不得回收。
  store
    .put(&tbl, &upsert_batch(b'B', 2, 0..40))
    .await
    .unwrap();
  assert_eq!(
    store.get(&tbl, &key(7)).await.unwrap(),
    Some(value(b'B', 7))
  );
  assert!(
    store.mapper(&tbl).unwrap().retired_len() > 0,
    "快照存活时第一轮覆盖的旧页必须留在 retired 队列"
  );

  // 第二轮覆盖：开批前的 reclaim 水位仍是快照代际，旧页继续滞留；
  // 若提前回池，这一轮会复用旧物理页并覆盖挂起读将读的内容。
  store
    .put(&tbl, &upsert_batch(b'C', 3, 0..40))
    .await
    .unwrap();
  assert_eq!(
    store.get(&tbl, &key(7)).await.unwrap(),
    Some(value(b'C', 7))
  );
  assert!(
    store.mapper(&tbl).unwrap().retired_len() > 0,
    "快照存活时第二轮写入也不得回收 retired 页"
  );

  gate.release();
  assert_eq!(
    handle.await.unwrap().unwrap(),
    Some(expected),
    "挂起读必须仍读到快照里的旧值，不能串到被复用的旧物理页"
  );
  assert_eq!(store.snapshot_num(&tbl), 0, "读任务结束后快照必须已释放");

  // 快照释放后：下一轮写开批前回收全部 retired 页，从空闲池分配，
  // 不再抬高物理页高水位。
  let max_before_reclaim = store.mapper(&tbl).unwrap().max_fp_id();
  assert!(
    max_before_reclaim > max_after_first,
    "快照存活期间的两轮覆盖必须抬高过高水位: first={max_after_first}, before={max_before_reclaim}"
  );
  store
    .put(&tbl, &upsert_batch(b'D', 4, 0..40))
    .await
    .unwrap();
  let max_after_reclaim = store.mapper(&tbl).unwrap().max_fp_id();
  assert!(
    max_after_reclaim <= max_before_reclaim,
    "快照释放后的写入应复用回收页：before={max_before_reclaim}, after={max_after_reclaim}"
  );
  assert_eq!(
    store.get(&tbl, &key(7)).await.unwrap(),
    Some(value(b'D', 7))
  );
}

#[compio::test]
async fn old_snapshot_misses_index_cache_entries_from_new_generation() {
  let tbl = TableIdent::new("snapshot_cache", 5).unwrap();
  let mut store = new_store(18);
  store
    .put(&tbl, &upsert_batch(b'A', 1, 0..50))
    .await
    .unwrap();

  let old_snapshot = store.snapshot(&tbl).unwrap();
  let counter = Arc::new(CacheCounter::default());
  let cache = RefCell::new(IndexCache::with_counter(1 << 20, Arc::clone(&counter)));
  // 旧快照首读：回填旧代 index 节点。
  assert_eq!(
    read_with_cache(store.page_io(), &tbl, &old_snapshot, &cache, 3)
      .await
      .unwrap(),
    Some(value(b'A', 3))
  );
  assert!(!cache.borrow().is_empty(), "旧快照读应回填索引缓存");

  store
    .put(&tbl, &upsert_batch(b'B', 2, 0..50))
    .await
    .unwrap();

  let new_snapshot = store.snapshot(&tbl).unwrap();
  assert_eq!(
    new_snapshot.root_id(),
    old_snapshot.root_id(),
    "本用例要求写后仍复用同一逻辑 root，才能验证同 key 的代际隔离"
  );
  // 新快照读：同一逻辑 root 的缓存项代际不符，必须 miss 后回填新代。
  assert_eq!(
    read_with_cache(store.page_io(), &tbl, &new_snapshot, &cache, 3)
      .await
      .unwrap(),
    Some(value(b'B', 3))
  );
  let miss_after_new = counter.miss();

  // 旧快照再读：不得复用新代节点，结果仍是旧值，且缓存必须 miss。
  assert_eq!(
    read_with_cache(store.page_io(), &tbl, &old_snapshot, &cache, 3)
      .await
      .unwrap(),
    Some(value(b'A', 3))
  );
  assert!(
    counter.miss() > miss_after_new,
    "旧快照命中新代缓存节点时 miss 不会增长"
  );
}
