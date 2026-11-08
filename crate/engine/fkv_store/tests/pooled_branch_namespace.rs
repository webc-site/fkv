//! pooled-05：云端对象命名下的跨 branch 物理页复用回归。
//!
//! `MemStore` 的页 key 不含 branch，无法区分「branch 写父 branch 区间」与
//! 「父写自身区间」。本文件用 [`ScopedMem`] 复刻云端对象命名：物理页 key 为
//! `(branch, term, tbl, file_page_id)`，归属按 manifest 的 `file_range_li` 解析
//! （与 `CloudStore::ranged_name` 同口径）。父 branch 写 file N 落到它自己的
//! 对象，子 branch 若错误地把父区间页回池，也会解析到父的 `(branch, term)`，
//! 从而覆盖父的活数据——正是 pooled-05 的损坏链路。
//!
//! 夹具直接落两份 manifest（形状与 `create_branch` 产物一致）：
//! - 父（create_branch 之后）：`max_fp_id = 2048`（预留到子分支区间末尾），
//!   file 0..=1022 已用，区间链只有 `main(2048)`；
//! - 子：`max_fp_id = 1024`，继承同样的 1023 个已用页，区间链
//!   `[main(1023), feature(1024)]`。
//!
//! 修复前子分支重开的空闲池含 page 1023（父区间），首写即落到 `main/0` 对象；
//! 父写入 1023..2047 后覆盖该页，子分支数据丢失。

use std::{cell::RefCell, rc::Rc};

use fkv_btree::BatchEntry;
use fkv_error::{KvError, Result};
use fkv_io::{ManifestIo, PageIo};
use fkv_io_mem::MemStore;
use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArg};
use fkv_store::{AllocMode, Store};
use fkv_types::{
  BranchFileRange, FilePageId, NULL_PAGE, TableIdent, decode_id, encode_file_page_id,
  is_file_page_id,
};
use gxhash::HashMap;

const PAGE_SIZE: usize = 512;
const RESTART: u16 = 8;
/// 父 branch 已用页数：page/file 位移为 0 时高水位停在 file 1023。
const USED_FP_NUM: u64 = 1023;
/// 子分支预留区间起点（file id = page id）。
const BRANCH_BASE: u64 = 1024;
/// 父 branch 预留后的高水位：子分支区间 [1024, 2048)。
const PARENT_MAX_FP: u64 = 2048;

type PageKey = (String, u64, TableIdent, FilePageId);

/// 按 `(branch, term)` 命名物理页的内存后端（云端对象命名语义）。
struct ScopedMem {
  inner: Rc<MemStore>,
  pages: Rc<RefCell<HashMap<PageKey, Vec<u8>>>>,
  identity: Rc<RefCell<(String, u64)>>,
  ranges: Rc<RefCell<Vec<BranchFileRange>>>,
}

impl ScopedMem {
  fn new() -> Self {
    Self {
      inner: Rc::new(MemStore::with_pages_per_file(PAGE_SIZE, 0)),
      pages: Rc::new(RefCell::new(HashMap::default())),
      identity: Rc::new(RefCell::new(("main".to_owned(), 0))),
      ranges: Rc::new(RefCell::new(Vec::new())),
    }
  }

  /// 共享物理页 / manifest，但身份与区间链按 Store 句柄独立（每个 Store 各自
  /// attach 一个 branch）。
  fn fork(&self) -> Self {
    Self {
      inner: self.inner.clone(),
      pages: self.pages.clone(),
      identity: Rc::new(RefCell::new(("main".to_owned(), 0))),
      ranges: Rc::new(RefCell::new(Vec::new())),
    }
  }

  /// file id 的写时归属：区间链命中取 `(branch, term)`，否则回退当前 identity。
  fn resolve(&self, file_page_id: FilePageId) -> (String, u64) {
    let ranges = self.ranges.borrow();
    let index = ranges.partition_point(|range| range.max_file_id < file_page_id);
    ranges.get(index).map_or_else(
      || self.identity.borrow().clone(),
      |range| (range.branch_name.to_string(), range.term),
    )
  }
}

impl PageIo for ScopedMem {
  fn set_file_identity(&self, branch: &str, term: u64) {
    *self.identity.borrow_mut() = (branch.to_owned(), term);
  }

  fn set_file_range_li(&self, _tbl: &TableIdent, range_li: &[BranchFileRange]) {
    *self.ranges.borrow_mut() = range_li.to_vec();
  }

  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    let (branch, term) = self.resolve(file_page_id);
    let pages = self.pages.borrow();
    let src = pages
      .get(&(branch, term, tbl.clone(), file_page_id))
      .ok_or(KvError::ResourceMissing)?;
    if dst.len() < src.len() {
      return Err(KvError::InvalidArgs);
    }
    dst[..src.len()].copy_from_slice(src);
    Ok(())
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    if src.len() != PAGE_SIZE {
      return Err(KvError::InvalidArgs);
    }
    let (branch, term) = self.resolve(file_page_id);
    self
      .pages
      .borrow_mut()
      .insert((branch, term, tbl.clone(), file_page_id), src.to_vec());
    Ok(())
  }

  async fn sync_data(&self, _tbl: &TableIdent) -> Result<()> {
    Ok(())
  }
}

impl ManifestIo for ScopedMem {
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

fn range(branch: &str, max_file_id: u64) -> BranchFileRange {
  BranchFileRange {
    branch_name: branch.into(),
    term: 0,
    max_file_id,
    max_segment_file_id: 0,
  }
}

/// 与 `ManifestBuilder` 的 snapshot 同格式：`used_fp_num` 个连续物理页已用。
fn manifest(max_fp_id: u64, used_fp_num: u64, metadata: &BranchManifestMetadata) -> Vec<u8> {
  let mapping: Vec<u64> = (0..used_fp_num).map(encode_file_page_id).collect();
  ManifestBuilder::new()
    .snapshot(
      SnapshotArg {
        root_id: NULL_PAGE,
        ttl_root: NULL_PAGE,
        max_fp_id,
        dict_bytes: &[],
        branch_metadata: metadata,
        max_segment_fp_id: 0,
      },
      &mapping,
      None,
    )
    .unwrap()
}

fn used_fp_li(store: &Store<ScopedMem>, tbl: &TableIdent) -> Vec<u64> {
  let mut li: Vec<u64> = store
    .mapper(tbl)
    .expect("mapper")
    .tbl()
    .iter_value_li()
    .filter(|value| is_file_page_id(*value))
    .map(decode_id)
    .collect();
  li.sort_unstable();
  li.dedup();
  li
}

fn new_store(io: ScopedMem) -> Store<ScopedMem> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(0)
    .with_alloc_mode(AllocMode::Pooled)
}

/// 子分支写自己的 key 后，父写自己的区间不得让子分支读到别的数据。
#[test]
fn branch_and_parent_writes_never_share_named_pages() {
  pollster::block_on(async {
    let tbl = TableIdent::new("scoped_branch", 1).unwrap();
    let parent_meta = BranchManifestMetadata {
      branch_name: "main".into(),
      term: 0,
      file_range_li: vec![range("main", PARENT_MAX_FP)],
    };
    let branch_meta = BranchManifestMetadata {
      branch_name: "feature".into(),
      term: 0,
      // 父 branch 区间到 file 1023（fork 时高水位），子分支自身从 file 1024 起。
      file_range_li: vec![range("main", 1023), range("feature", BRANCH_BASE)],
    };
    let io = ScopedMem::new();
    io.inner
      .switch_manifest(&tbl, &manifest(PARENT_MAX_FP, USED_FP_NUM, &parent_meta))
      .await
      .unwrap();
    io.inner
      .write_manifest_file(
        &tbl,
        "manifest_feature_0",
        &manifest(BRANCH_BASE, USED_FP_NUM, &branch_meta),
      )
      .await
      .unwrap();

    let mut parent = new_store(io.fork());
    parent.reopen(&tbl).await.unwrap();
    assert_eq!(parent.mapper(&tbl).unwrap().max_fp_id(), PARENT_MAX_FP);

    let mut branch = new_store(io.fork());
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    let before = used_fp_li(&branch, &tbl);
    branch
      .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)])
      .await
      .unwrap();
    let after = used_fp_li(&branch, &tbl);
    let new_li: Vec<u64> = after
      .iter()
      .copied()
      .filter(|id| before.binary_search(id).is_err())
      .collect();
    assert!(
      new_li.iter().all(|id| *id >= BRANCH_BASE),
      "branch 写入了父 branch 的命名空间: {new_li:?}"
    );
    assert_eq!(branch.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));

    // 父写满自身空洞区间 [1023, 2048)：修复前会把子分支 page 1023 一并覆盖。
    parent
      .put(
        &tbl,
        &[BatchEntry::upsert(b"a".to_vec(), vec![7u8; 600_000], 1)],
      )
      .await
      .unwrap();

    // 新开两个 Store，绕过进程内 page cache，强制从物理页读回。
    let mut branch = new_store(io.fork());
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(
      branch.get(&tbl, b"b").await.unwrap(),
      Some(b"B".to_vec()),
      "父写后子读串页"
    );
    assert_eq!(branch.get(&tbl, b"a").await.unwrap(), None);

    let mut parent = new_store(io.fork());
    parent.reopen(&tbl).await.unwrap();
    let value = parent.get(&tbl, b"a").await.unwrap().expect("parent value");
    assert_eq!(value.len(), 600_000);
    assert!(value.iter().all(|byte| *byte == 7));
    assert_eq!(parent.get(&tbl, b"b").await.unwrap(), None);
  });
}
