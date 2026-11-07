use std::{
  future::{Future, poll_fn},
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
  },
  task::{Context, Poll},
  thread::scope,
};

use bytes::Bytes;
use fkv_btree::BatchEntry;
use fkv_cloud::{CacheGroup, CloudStore, EVICT_PENDING_FILE_MAX};
use fkv_error::{KvError, Result};
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo, TermIo};
use fkv_io_mem::MemStore;
use fkv_metrics::ShardMetric;
use fkv_object::{MemObjectStore, ObjectMeta, ObjectStore};
use fkv_store::Store;
use fkv_types::{BranchFileRange, FileId, FilePageId, InstallOutcome, TableIdent};
use futures::{future::join_all, task::noop_waker};

mod cache;
mod evict;
mod io;
mod manifest;
mod reopen;
mod term;

/// 写 `file_num` 个整文件（每文件 4 页 × 512B = 2048B）并 flush，制造可淘汰的
/// clean cache。dirty 文件不可淘汰，必须先 `sync_data` 才有统一预算语义。
async fn fill_file_li(
  cloud: &CloudStore<MemStore, MemObjectStore>,
  tbl: &TableIdent,
  file_num: u64,
) {
  for file_id in 0..file_num {
    for index in 0..4u64 {
      let fp_id = file_id * 4 + index;
      let mut page = vec![0u8; 512];
      page[0] = (fp_id % 255) as u8 + 1;
      cloud.write_page(tbl, fp_id, &page).await.unwrap();
    }
  }
  cloud.sync_data(tbl).await.unwrap();
}

/// 本地后端实际驻留的 data 字节（MemStore 上等价于「占用磁盘」）。
async fn local_bytes(cloud: &CloudStore<MemStore, MemObjectStore>, tbl: &TableIdent) -> u64 {
  let mut total = 0u64;
  for file_id in cloud.local().list_file_id_li(tbl).await.unwrap() {
    total += cloud
      .local()
      .file_size(tbl, file_id)
      .await
      .unwrap()
      .unwrap_or(0);
  }
  total
}

/// cld-41 读取探针：统计 term CAS 路径发出的读取次数，并可在首次读之后
/// 注入一次“别的 writer 推进 term”的写入。
struct TermProbe {
  /// 读到 term 文件后立即写入的 term（模拟 GET 与 CAS 之间的并发更新）。
  race: Option<u64>,
  raced: AtomicBool,
  get: AtomicUsize,
  head: AtomicUsize,
  versioned: AtomicUsize,
}

impl TermProbe {
  fn new(race: Option<u64>) -> Self {
    Self {
      race,
      raced: AtomicBool::new(false),
      get: AtomicUsize::new(0),
      head: AtomicUsize::new(0),
      versioned: AtomicUsize::new(0),
    }
  }

  /// `(get, head, get_versioned)` 调用次数。
  fn counts(&self) -> (usize, usize, usize) {
    (
      self.get.load(Ordering::SeqCst),
      self.head.load(Ordering::SeqCst),
      self.versioned.load(Ordering::SeqCst),
    )
  }

  /// 只注入一次：写入发生在本次读取返回之后、调用方 CAS 之前。
  async fn race_after_read(&self, inner: &MemObjectStore, key: &str) {
    let Some(term) = self.race else {
      return;
    };
    if self.raced.swap(true, Ordering::SeqCst) {
      return;
    }
    inner
      .put(key, Bytes::from(term.to_string()), None, false)
      .await
      .unwrap();
  }
}

/// 包装 `MemObjectStore`，在 term CAS 的读取路径上做计数与并发注入。
struct TermProbeStore {
  inner: MemObjectStore,
  probe: Arc<TermProbe>,
}

impl ObjectStore for TermProbeStore {
  async fn get(&self, key: &str) -> Result<Option<Bytes>> {
    self.probe.get.fetch_add(1, Ordering::SeqCst);
    let value = self.inner.get(key).await?;
    self.probe.race_after_read(&self.inner, key).await;
    Ok(value)
  }

  async fn get_versioned(&self, key: &str) -> Result<Option<(Bytes, String)>> {
    self.probe.versioned.fetch_add(1, Ordering::SeqCst);
    let pair = self.inner.get_versioned(key).await?;
    self.probe.race_after_read(&self.inner, key).await;
    Ok(pair)
  }

  async fn put(
    &self,
    key: &str,
    value: Bytes,
    if_match: Option<&str>,
    if_none_match: bool,
  ) -> Result<String> {
    self.inner.put(key, value, if_match, if_none_match).await
  }

  async fn head(&self, key: &str) -> Result<Option<ObjectMeta>> {
    self.probe.head.fetch_add(1, Ordering::SeqCst);
    self.inner.head(key).await
  }

  async fn delete(&self, key: &str) -> Result<()> {
    self.inner.delete(key).await
  }

  async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>> {
    self.inner.list(prefix).await
  }
}

/// 删除失败时项回队、记账不丢：热路径只告警不失败，显式 drain 重试成功。
struct FlakyRemoveStore {
  inner: MemStore,
  fail_num: AtomicUsize,
}

impl FileIo for FlakyRemoveStore {
  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<u64>> {
    self.inner.list_file_id_li(tbl).await
  }

  async fn read_file(&self, tbl: &TableIdent, file_id: u64) -> Result<Option<Vec<u8>>> {
    self.inner.read_file(tbl, file_id).await
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: u64, bytes: &[u8]) -> Result<()> {
    self.inner.write_file(tbl, file_id, bytes).await
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: u64) -> Result<Option<u64>> {
    self.inner.file_size(tbl, file_id).await
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: u64) -> Result<()> {
    if self.fail_num.load(Ordering::Relaxed) > 0 {
      self.fail_num.fetch_sub(1, Ordering::Relaxed);
      return Err(KvError::IoFail);
    }
    self.inner.remove_file(tbl, file_id).await
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<u64>> {
    self.inner.list_segment_file_id_li(tbl).await
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: u64) -> Result<()> {
    self.inner.remove_segment_file(tbl, file_id).await
  }
}

/// 包装 `MemObjectStore`：统计 `get` 次数，并让 `get` 主动让出一次执行权，
/// 保证并发冷读能在 owner 完成前全部挂到 single-flight 上。
#[derive(Clone)]
struct CountingStore {
  inner: MemObjectStore,
  get_count: Arc<AtomicUsize>,
  yield_on_get: bool,
}

impl CountingStore {
  fn new(yield_on_get: bool) -> Self {
    Self {
      inner: MemObjectStore::new(),
      get_count: Arc::new(AtomicUsize::new(0)),
      yield_on_get,
    }
  }

  fn get_count(&self) -> usize {
    self.get_count.load(Ordering::SeqCst)
  }
}

impl ObjectStore for CountingStore {
  async fn get(&self, key: &str) -> Result<Option<Bytes>> {
    self.get_count.fetch_add(1, Ordering::SeqCst);
    if self.yield_on_get {
      yield_once().await;
    }
    self.inner.get(key).await
  }

  async fn put(
    &self,
    key: &str,
    value: Bytes,
    if_match: Option<&str>,
    if_none_match: bool,
  ) -> Result<String> {
    self.inner.put(key, value, if_match, if_none_match).await
  }

  async fn head(&self, key: &str) -> Result<Option<ObjectMeta>> {
    self.inner.head(key).await
  }

  async fn delete(&self, key: &str) -> Result<()> {
    self.inner.delete(key).await
  }

  async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>> {
    self.inner.list(prefix).await
  }
}

/// 让出一次执行权：同线程上的其它 ready future 得以继续 poll。
async fn yield_once() {
  let mut yielded = false;
  poll_fn(move |cx| {
    if yielded {
      Poll::Ready(())
    } else {
      yielded = true;
      cx.waker().wake_by_ref();
      Poll::Pending
    }
  })
  .await
}

/// 本地后端：不实现整表列举（`list_segment_file_id_li` 直接报错）。
///
/// `adopt_local_file` 若仍逐文件列目录，本后端会让预热失败；改成单文件
/// 完整性判定后，只读尾槽即可登记。
struct NoListStore {
  inner: MemStore,
}

impl PageIo for NoListStore {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> fkv_error::Result<()> {
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

impl SegmentIo for NoListStore {
  async fn read_segment<'a>(
    &'a self,
    tbl: &'a TableIdent,
    segment_fp_id: FilePageId,
    dst: &'a mut [u8],
  ) -> Result<()> {
    self.inner.read_segment(tbl, segment_fp_id, dst).await
  }

  async fn write_segment<'a>(
    &'a self,
    tbl: &'a TableIdent,
    segment_fp_id: FilePageId,
    src: &'a [u8],
  ) -> Result<()> {
    self.inner.write_segment(tbl, segment_fp_id, src).await
  }
}

impl FileIo for NoListStore {
  async fn list_file_id_li<'a>(&'a self, tbl: &'a TableIdent) -> Result<Vec<FileId>> {
    self.inner.list_file_id_li(tbl).await
  }

  async fn read_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
  ) -> Result<Option<Vec<u8>>> {
    self.inner.read_file(tbl, file_id).await
  }

  async fn write_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
    bytes: &'a [u8],
  ) -> Result<()> {
    self.inner.write_file(tbl, file_id, bytes).await
  }

  async fn file_size<'a>(&'a self, tbl: &'a TableIdent, file_id: FileId) -> Result<Option<u64>> {
    self.inner.file_size(tbl, file_id).await
  }

  async fn remove_file<'a>(&'a self, tbl: &'a TableIdent, file_id: FileId) -> Result<()> {
    self.inner.remove_file(tbl, file_id).await
  }

  async fn list_segment_file_id_li<'a>(&'a self, _tbl: &'a TableIdent) -> Result<Vec<FileId>> {
    Err(KvError::InvalidArgs)
  }

  async fn remove_segment_file<'a>(&'a self, tbl: &'a TableIdent, file_id: FileId) -> Result<()> {
    self.inner.remove_segment_file(tbl, file_id).await
  }
}

/// 完成标记走 manifest 旁路文件，本后端只需透传（不涉及整表列举）。
impl ManifestIo for NoListStore {
  async fn append_manifest<'a>(&'a self, tbl: &'a TableIdent, record: &'a [u8]) -> Result<()> {
    self.inner.append_manifest(tbl, record).await
  }

  async fn switch_manifest<'a>(&'a self, tbl: &'a TableIdent, record: &'a [u8]) -> Result<()> {
    self.inner.switch_manifest(tbl, record).await
  }

  async fn read_manifest<'a>(&'a self, tbl: &'a TableIdent) -> Result<Option<Vec<u8>>> {
    self.inner.read_manifest(tbl).await
  }

  async fn drop_manifest<'a>(&'a self, tbl: &'a TableIdent) -> Result<()> {
    self.inner.drop_manifest(tbl).await
  }

  async fn write_manifest_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
    record: &'a [u8],
  ) -> Result<()> {
    self.inner.write_manifest_file(tbl, name, record).await
  }

  async fn append_manifest_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
    record: &'a [u8],
  ) -> Result<()> {
    self.inner.append_manifest_file(tbl, name, record).await
  }

  async fn list_manifest_file_name_li<'a>(&'a self, tbl: &'a TableIdent) -> Result<Vec<String>> {
    self.inner.list_manifest_file_name_li(tbl).await
  }

  async fn list_sidecar_file_name_li<'a>(&'a self, tbl: &'a TableIdent) -> Result<Vec<String>> {
    self.inner.list_sidecar_file_name_li(tbl).await
  }

  async fn remove_sidecar_file_name<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
  ) -> Result<()> {
    self.inner.remove_sidecar_file_name(tbl, name).await
  }

  async fn read_manifest_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
  ) -> Result<Option<Vec<u8>>> {
    self.inner.read_manifest_file(tbl, name).await
  }

  async fn seed_local_manifest_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
    bytes: &'a [u8],
  ) -> Result<()> {
    self.inner.seed_local_manifest_file(tbl, name, bytes).await
  }

  async fn remove_manifest_file<'a>(&'a self, tbl: &'a TableIdent, name: &'a str) -> Result<()> {
    self.inner.remove_manifest_file(tbl, name).await
  }
}

/// 本地后端：`write_file` 成功但 `file_size` 恒为 `None`（写后不落盘）。
struct NoSizeStore {
  inner: MemStore,
}

impl FileIo for NoSizeStore {
  async fn list_file_id_li<'a>(&'a self, tbl: &'a TableIdent) -> Result<Vec<FileId>> {
    self.inner.list_file_id_li(tbl).await
  }

  async fn read_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
  ) -> Result<Option<Vec<u8>>> {
    self.inner.read_file(tbl, file_id).await
  }

  async fn write_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
    bytes: &'a [u8],
  ) -> Result<()> {
    self.inner.write_file(tbl, file_id, bytes).await
  }

  async fn file_size<'a>(&'a self, _tbl: &'a TableIdent, _file_id: FileId) -> Result<Option<u64>> {
    Ok(None)
  }

  async fn remove_file<'a>(&'a self, tbl: &'a TableIdent, file_id: FileId) -> Result<()> {
    self.inner.remove_file(tbl, file_id).await
  }

  async fn list_segment_file_id_li<'a>(&'a self, tbl: &'a TableIdent) -> Result<Vec<FileId>> {
    self.inner.list_segment_file_id_li(tbl).await
  }

  async fn remove_segment_file<'a>(&'a self, tbl: &'a TableIdent, file_id: FileId) -> Result<()> {
    self.inner.remove_segment_file(tbl, file_id).await
  }
}

/// 远端对象：首次 GET 让出执行权并返回短对象，之后返回完整对象。
///
/// 用于制造「等待者 round 0 拿到别人的 `Cached`（只覆盖别人起点）」的场景：
/// 等待者必须自己再回源一次，不能按短文件放行越界写。
#[derive(Clone)]
struct GrowingStore {
  inner: MemObjectStore,
  get_count: Arc<AtomicUsize>,
}

impl GrowingStore {
  fn new() -> Self {
    Self {
      inner: MemObjectStore::new(),
      get_count: Arc::new(AtomicUsize::new(0)),
    }
  }
}

impl ObjectStore for GrowingStore {
  async fn get(&self, _key: &str) -> Result<Option<Bytes>> {
    let count = self.get_count.fetch_add(1, Ordering::SeqCst);
    if count == 0 {
      yield_once().await;
      return Ok(Some(Bytes::from(vec![1u8; 1024])));
    }
    Ok(Some(Bytes::from(vec![2u8; 2048])))
  }

  async fn put(
    &self,
    key: &str,
    value: Bytes,
    if_match: Option<&str>,
    if_none_match: bool,
  ) -> Result<String> {
    self.inner.put(key, value, if_match, if_none_match).await
  }

  async fn head(&self, key: &str) -> Result<Option<ObjectMeta>> {
    self.inner.head(key).await
  }

  async fn delete(&self, key: &str) -> Result<()> {
    self.inner.delete(key).await
  }

  async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>> {
    self.inner.list(prefix).await
  }
}
