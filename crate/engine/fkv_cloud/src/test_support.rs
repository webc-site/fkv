//! test_support

use core::{cell::Cell, task::Poll};
use std::{
  collections::{HashMap, HashSet},
  rc::Rc,
  sync::Mutex,
};

use bytes::Bytes as BytesBytes;
use fkv_error::KvError;
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_io_mem::MemStore;
use fkv_object::{MemObjectStore, ObjectStore};
use fkv_types::{FileId, FilePageId, TableIdent};
use futures::future::poll_fn;

use crate::segment::SEGMENT_MARKER_PREFIX;

/// 本轮第一次 poll 主动让出执行权（模拟真实后端 `write_file` 的 IO 挂起点）。
pub(crate) async fn yield_once(yielded: &Cell<bool>) {
  poll_fn(|cx| {
    if yielded.replace(false) {
      cx.waker().wake_by_ref();
      Poll::Pending
    } else {
      Poll::Ready(())
    }
  })
  .await
}

/// 本地后端：`write_file` 首次 poll 挂起一次，用于在写路径中途插入驱逐。
pub(crate) struct YieldWriteStore {
  pub(crate) inner: fkv_io_mem::MemStore,
  pub(crate) yielded: Cell<bool>,
  /// 段写入首次 poll 挂起一次（预热安装中途断言回填跳过）。
  pub(crate) segment_write_yielded: Cell<bool>,
  /// 段读取首次 poll 挂起一次（回填判定中途插入跨实例 busy 登记）。
  pub(crate) segment_read_yielded: Cell<bool>,
}

impl YieldWriteStore {
  pub(crate) fn new() -> Self {
    Self {
      inner: MemStore::new(512),
      yielded: Cell::new(true),
      segment_write_yielded: Cell::new(false),
      segment_read_yielded: Cell::new(false),
    }
  }

  pub(crate) fn new_segment_write_yield() -> Self {
    Self {
      yielded: Cell::new(false),
      segment_write_yielded: Cell::new(true),
      ..Self::new()
    }
  }

  pub(crate) fn new_segment_read_yield() -> Self {
    Self {
      segment_read_yielded: Cell::new(true),
      ..Self::new()
    }
  }
}

impl FileIo for YieldWriteStore {
  async fn list_file_id_li(&self, tbl: &TableIdent) -> fkv_error::Result<Vec<FileId>> {
    self.inner.list_file_id_li(tbl).await
  }

  async fn read_file(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
  ) -> fkv_error::Result<Option<Vec<u8>>> {
    self.inner.read_file(tbl, file_id).await
  }

  async fn write_file(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    bytes: &[u8],
  ) -> fkv_error::Result<()> {
    yield_once(&self.yielded).await;
    self.inner.write_file(tbl, file_id, bytes).await
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> fkv_error::Result<Option<u64>> {
    self.inner.file_size(tbl, file_id).await
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> fkv_error::Result<()> {
    self.inner.remove_file(tbl, file_id).await
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> fkv_error::Result<Vec<FileId>> {
    self.inner.list_segment_file_id_li(tbl).await
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> fkv_error::Result<()> {
    self.inner.remove_segment_file(tbl, file_id).await
  }
}

/// 假后端只覆写 IO 挂起，manifest 全部转发给 `inner`（segment 完成标记
/// 随 manifest 文件存放，`CloudStore` 的删除路径需要 `ManifestIo`）。
impl ManifestIo for YieldWriteStore {
  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> fkv_error::Result<()> {
    self.inner.append_manifest(tbl, record).await
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> fkv_error::Result<()> {
    self.inner.switch_manifest(tbl, record).await
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> fkv_error::Result<Option<Vec<u8>>> {
    self.inner.read_manifest(tbl).await
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> fkv_error::Result<()> {
    self.inner.drop_manifest(tbl).await
  }

  async fn write_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
    record: &[u8],
  ) -> fkv_error::Result<()> {
    self.inner.write_manifest_file(tbl, name, record).await
  }

  async fn append_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
    record: &[u8],
  ) -> fkv_error::Result<()> {
    self.inner.append_manifest_file(tbl, name, record).await
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> fkv_error::Result<Vec<String>> {
    self.inner.list_manifest_file_name_li(tbl).await
  }

  async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> fkv_error::Result<Vec<String>> {
    self.inner.list_sidecar_file_name_li(tbl).await
  }

  async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> fkv_error::Result<()> {
    self.inner.remove_sidecar_file_name(tbl, name).await
  }

  async fn read_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
  ) -> fkv_error::Result<Option<Vec<u8>>> {
    self.inner.read_manifest_file(tbl, name).await
  }

  async fn seed_local_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
    bytes: &[u8],
  ) -> fkv_error::Result<()> {
    self.inner.seed_local_manifest_file(tbl, name, bytes).await
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> fkv_error::Result<()> {
    self.inner.remove_manifest_file(tbl, name).await
  }
}

impl PageIo for YieldWriteStore {
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

impl PageIo for ProbeStore {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> fkv_error::Result<()> {
    self.shared.inner.read_page(tbl, file_page_id, dst).await
  }

  async fn write_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    src: &[u8],
  ) -> fkv_error::Result<()> {
    self.shared.inner.write_page(tbl, file_page_id, src).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> fkv_error::Result<()> {
    self
      .shared
      .log
      .lock()
      .unwrap_or_else(|poison| poison.into_inner())
      .push(format!("sync:{tbl}"));
    let moved: Vec<_> = self
      .shared
      .volatile
      .lock()
      .unwrap_or_else(|poison| poison.into_inner())
      .iter()
      .filter(|(key, _)| &key.0 == tbl)
      .map(|(key, bytes)| (key.clone(), bytes.clone()))
      .collect();
    {
      let mut volatile = self
        .shared
        .volatile
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
      let mut durable = self
        .shared
        .durable
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
      for (key, bytes) in moved {
        volatile.remove(&key);
        durable.insert(key, bytes);
      }
    }
    self.shared.inner.sync_data(tbl).await
  }
}

impl SegmentIo for YieldWriteStore {
  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> fkv_error::Result<()> {
    yield_once(&self.segment_read_yielded).await;
    self.inner.read_segment(tbl, segment_fp_id, dst).await
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> fkv_error::Result<()> {
    yield_once(&self.segment_write_yielded).await;
    self.inner.write_segment(tbl, segment_fp_id, src).await
  }
}

/// 崩溃语义探针：段写先进 volatile，`sync_data` 才转 durable；`crash` 丢掉
/// volatile（模拟未 fdatasync 的页缓存）。定长后端崩溃后文件仍按 `set_len`
/// 分配，丢失的槽位读零而不是缺失。
#[derive(Clone)]
pub(crate) struct ProbeStore {
  pub(crate) shared: Rc<ProbeShared>,
}

pub(crate) struct ProbeShared {
  pub(crate) segment_size: usize,
  pub(crate) segments_per_file_shift: u8,
  pub(crate) log: Mutex<Vec<String>>,
  pub(crate) volatile: Mutex<HashMap<(TableIdent, FilePageId), Vec<u8>>>,
  pub(crate) durable: Mutex<HashMap<(TableIdent, FilePageId), Vec<u8>>>,
  pub(crate) allocated: Mutex<HashSet<(TableIdent, FileId)>>,
  pub(crate) inner: MemStore,
}

impl ProbeStore {
  pub(crate) fn new(segment_size: usize, segments_per_file_shift: u8) -> Self {
    Self {
      shared: Rc::new(ProbeShared {
        segment_size,
        segments_per_file_shift,
        log: Mutex::new(Vec::new()),
        volatile: Mutex::new(HashMap::new()),
        durable: Mutex::new(HashMap::new()),
        allocated: Mutex::new(HashSet::new()),
        inner: MemStore::new(512).with_segment_config(segment_size, segments_per_file_shift),
      }),
    }
  }

  pub(crate) fn log_li(&self) -> Vec<String> {
    self
      .shared
      .log
      .lock()
      .unwrap_or_else(|poison| poison.into_inner())
      .clone()
  }

  /// 丢掉未 sync 的段写：已分配文件保留（读零），未 sync 的槽位内容丢失。
  pub(crate) fn crash(&self) {
    let durable = self
      .shared
      .durable
      .lock()
      .unwrap_or_else(|poison| poison.into_inner())
      .clone();
    *self
      .shared
      .volatile
      .lock()
      .unwrap_or_else(|poison| poison.into_inner()) = durable;
  }
}

impl FileIo for ProbeStore {
  async fn list_file_id_li(&self, tbl: &TableIdent) -> fkv_error::Result<Vec<FileId>> {
    self.shared.inner.list_file_id_li(tbl).await
  }

  async fn read_file(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
  ) -> fkv_error::Result<Option<Vec<u8>>> {
    self.shared.inner.read_file(tbl, file_id).await
  }

  async fn write_file(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    bytes: &[u8],
  ) -> fkv_error::Result<()> {
    self.shared.inner.write_file(tbl, file_id, bytes).await
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> fkv_error::Result<Option<u64>> {
    self.shared.inner.file_size(tbl, file_id).await
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> fkv_error::Result<()> {
    self.shared.inner.remove_file(tbl, file_id).await
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> fkv_error::Result<Vec<FileId>> {
    let mut set = HashSet::new();
    let shift = self.shared.segments_per_file_shift;
    for key in self
      .shared
      .volatile
      .lock()
      .unwrap_or_else(|poison| poison.into_inner())
      .keys()
      .chain(
        self
          .shared
          .durable
          .lock()
          .unwrap_or_else(|poison| poison.into_inner())
          .keys(),
      )
    {
      if &key.0 == tbl {
        set.insert(key.1 >> shift);
      }
    }
    let mut file_id_li: Vec<FileId> = set.into_iter().collect();
    file_id_li.sort_unstable();
    Ok(file_id_li)
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> fkv_error::Result<()> {
    self.shared.inner.remove_segment_file(tbl, file_id).await
  }
}

impl SegmentIo for ProbeStore {
  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> fkv_error::Result<()> {
    if dst.len() > self.shared.segment_size {
      return Err(KvError::InvalidArgs);
    }
    let key = (tbl.clone(), segment_fp_id);
    let bytes = self
      .shared
      .volatile
      .lock()
      .unwrap_or_else(|poison| poison.into_inner())
      .get(&key)
      .cloned()
      .or_else(|| {
        self
          .shared
          .durable
          .lock()
          .unwrap_or_else(|poison| poison.into_inner())
          .get(&key)
          .cloned()
      });
    match bytes {
      Some(bytes) => {
        let len = bytes.len().min(dst.len());
        dst[..len].copy_from_slice(&bytes[..len]);
        dst[len..].fill(0);
        Ok(())
      }
      None => {
        // 定长后端崩溃后文件仍然存在（set_len 已落盘），丢失的槽位读零。
        let file_id = segment_fp_id >> self.shared.segments_per_file_shift;
        if self
          .shared
          .allocated
          .lock()
          .unwrap_or_else(|poison| poison.into_inner())
          .contains(&(tbl.clone(), file_id))
        {
          dst.fill(0);
          Ok(())
        } else {
          Err(KvError::ResourceMissing)
        }
      }
    }
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> fkv_error::Result<()> {
    let file_id = segment_fp_id >> self.shared.segments_per_file_shift;
    self
      .shared
      .allocated
      .lock()
      .unwrap_or_else(|poison| poison.into_inner())
      .insert((tbl.clone(), file_id));
    self
      .shared
      .volatile
      .lock()
      .unwrap_or_else(|poison| poison.into_inner())
      .insert((tbl.clone(), segment_fp_id), src.to_vec());
    Ok(())
  }
}

impl ManifestIo for ProbeStore {
  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> fkv_error::Result<()> {
    self.shared.inner.append_manifest(tbl, record).await
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> fkv_error::Result<()> {
    self.shared.inner.switch_manifest(tbl, record).await
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> fkv_error::Result<Option<Vec<u8>>> {
    self.shared.inner.read_manifest(tbl).await
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> fkv_error::Result<()> {
    self.shared.inner.drop_manifest(tbl).await
  }

  async fn write_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
    record: &[u8],
  ) -> fkv_error::Result<()> {
    if name.starts_with(SEGMENT_MARKER_PREFIX) {
      self
        .shared
        .log
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .push(format!("marker:{name}"));
    }
    self
      .shared
      .inner
      .write_manifest_file(tbl, name, record)
      .await
  }

  async fn append_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
    record: &[u8],
  ) -> fkv_error::Result<()> {
    self
      .shared
      .inner
      .append_manifest_file(tbl, name, record)
      .await
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> fkv_error::Result<Vec<String>> {
    self.shared.inner.list_manifest_file_name_li(tbl).await
  }

  async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> fkv_error::Result<Vec<String>> {
    self.shared.inner.list_sidecar_file_name_li(tbl).await
  }

  async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> fkv_error::Result<()> {
    self.shared.inner.remove_sidecar_file_name(tbl, name).await
  }

  async fn read_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
  ) -> fkv_error::Result<Option<Vec<u8>>> {
    self.shared.inner.read_manifest_file(tbl, name).await
  }

  async fn seed_local_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
    bytes: &[u8],
  ) -> fkv_error::Result<()> {
    self
      .shared
      .inner
      .seed_local_manifest_file(tbl, name, bytes)
      .await
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> fkv_error::Result<()> {
    self.shared.inner.remove_manifest_file(tbl, name).await
  }
}

/// 统计 `get` 次数的远端：验证回填权在整文件下载**之前**争取。
#[derive(Clone)]
pub(crate) struct GetCountStore {
  pub(crate) inner: MemObjectStore,
  pub(crate) get_num: Rc<Cell<u32>>,
}

impl GetCountStore {
  pub(crate) fn new() -> Self {
    Self {
      inner: MemObjectStore::new(),
      get_num: Rc::new(Cell::new(0)),
    }
  }
}

impl ObjectStore for GetCountStore {
  async fn get(&self, key: &str) -> fkv_error::Result<Option<BytesBytes>> {
    self.get_num.set(self.get_num.get() + 1);
    self.inner.get(key).await
  }

  async fn put(
    &self,
    key: &str,
    value: BytesBytes,
    if_match: Option<&str>,
    if_none_match: bool,
  ) -> fkv_error::Result<String> {
    self.inner.put(key, value, if_match, if_none_match).await
  }

  async fn head(&self, key: &str) -> fkv_error::Result<Option<fkv_object::ObjectMeta>> {
    self.inner.head(key).await
  }

  async fn delete(&self, key: &str) -> fkv_error::Result<()> {
    self.inner.delete(key).await
  }

  async fn list(&self, prefix: &str) -> fkv_error::Result<Vec<fkv_object::ObjectMeta>> {
    self.inner.list(prefix).await
  }
}
