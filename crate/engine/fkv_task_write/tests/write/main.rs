use core::{cell::RefCell, mem::take};
use std::collections::BTreeMap;

use fkv_btree::BatchEntry;
use fkv_error::{KvError, Result as FkvResult};
use fkv_io::PageIo;
use fkv_io_mem::MemStore;
use fkv_mapping::{AllocMode, MappingSnapshot, PageMapper, SnapshotRegistry, SnapshotSource};
use fkv_page::{IndexPage, IndexPageIter, PageType, page_type};
use fkv_task_read::{ReadCtx, read_entry};
use fkv_task_scan::{ScanArg, ScanTree, scan_range};
use fkv_task_write::{PageLayout, WriteArg, write_batch};
use fkv_types::{FilePageId, NULL_PAGE, PageId, TableIdent};

mod basic;
mod collapse;
mod flush;
mod index;
mod model;
mod ttl;

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
