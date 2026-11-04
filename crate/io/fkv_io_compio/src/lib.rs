//! compio-fs 后端：本地文件页 / manifest。
//!
//! 第一版每次 IO 打开文件，后续在 `fkv_io_compio` 内部补 FD LRU。

#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
#[cfg(target_os = "linux")]
use std::sync::Arc;
use std::{
  cell::RefCell,
  collections::HashMap,
  fs::{create_dir_all, metadata, read_dir},
  io::ErrorKind,
  path::{Path, PathBuf},
  sync::atomic::{AtomicU64, Ordering},
  time::SystemTime,
};

use compio::{
  buf::BufResult,
  fs::{File, OpenOptions, read, remove_file, rename},
  io::{AsyncReadAt, AsyncWriteAt},
};
use fkv_error::{KvError, Result};
use fkv_io::{
  DEFAULT_SEGMENT_SIZE, DEFAULT_SEGMENTS_PER_FILE_SHIFT, FileIo, FixedBuffer, FixedSegmentIo,
  ManifestIo, PageIo, SegmentIo, TermIo,
};
#[cfg(target_os = "linux")]
mod fixed;
use fkv_types::{FileId, FilePageId, TableIdent};

#[derive(Debug, Clone)]
struct FileIdentity {
  branch: String,
  term: u64,
}

/// 目录快照签名：目录 mtime + 目录项字节数。用于判断缓存的目录索引是否过期。
#[derive(Debug, Clone, PartialEq, Eq)]
struct DirStamp {
  modified: Option<SystemTime>,
  len: u64,
}

/// 每个 table 分区目录一份索引。一次 read_dir 同时收集 data / segment，
/// 负查询只在目录签名变化时重新扫描。
#[derive(Debug, Default)]
struct DirIndex {
  stamp: Option<DirStamp>,
  scanned: bool,
  data: HashMap<FileId, PathBuf>,
  segment: HashMap<FileId, PathBuf>,
}

#[derive(Debug)]
pub struct CompioStore {
  root: PathBuf,
  page_size: usize,
  pages_per_file_shift: u8,
  segment_size: usize,
  segments_per_file_shift: u8,
  identity: RefCell<FileIdentity>,
  dir_index_map: RefCell<HashMap<TableIdent, DirIndex>>,
  data_scan_num: AtomicU64,
  segment_scan_num: AtomicU64,
  #[cfg(target_os = "linux")]
  fixed_io: RefCell<Option<fixed::FixedBuffers>>,
}

impl CompioStore {
  pub fn new(root: impl Into<PathBuf>, page_size: usize, pages_per_file_shift: u8) -> Self {
    Self {
      root: root.into(),
      page_size,
      pages_per_file_shift,
      segment_size: DEFAULT_SEGMENT_SIZE,
      segments_per_file_shift: DEFAULT_SEGMENTS_PER_FILE_SHIFT,
      identity: RefCell::new(FileIdentity {
        branch: "main".to_owned(),
        term: 0,
      }),
      dir_index_map: RefCell::new(HashMap::new()),
      data_scan_num: AtomicU64::new(0),
      segment_scan_num: AtomicU64::new(0),
      #[cfg(target_os = "linux")]
      fixed_io: RefCell::new(None),
    }
  }

  pub fn with_segment_config(mut self, segment_size: usize, segments_per_file_shift: u8) -> Self {
    self.segment_size = segment_size;
    self.segments_per_file_shift = segments_per_file_shift;
    self
  }

  pub fn root(&self) -> &Path {
    &self.root
  }

  pub fn segment_size(&self) -> usize {
    self.segment_size
  }

  pub fn segments_per_file_shift(&self) -> u8 {
    self.segments_per_file_shift
  }

  fn part_dir(&self, tbl: &TableIdent) -> PathBuf {
    self
      .root
      .join(format!("{}.{}", tbl.tbl_name, tbl.partition_id))
  }

  fn dir_stamp(&self, tbl: &TableIdent) -> Option<DirStamp> {
    let meta = metadata(self.part_dir(tbl)).ok()?;
    Some(DirStamp {
      modified: meta.modified().ok(),
      len: meta.len(),
    })
  }

  /// 扫描分区目录并重建索引。`force=false` 时仅在目录签名变化时扫描。
  /// 返回是否真的执行了扫描。
  fn refresh_dir_index(&self, tbl: &TableIdent, force: bool) -> bool {
    let stamp = self.dir_stamp(tbl);
    if !force {
      let index_map = self.dir_index_map.borrow();
      if let Some(index) = index_map.get(tbl)
        && index.scanned
        && index.stamp == stamp
      {
        return false;
      }
    }
    let mut data = HashMap::new();
    let mut segment = HashMap::new();
    if let Ok(entry_li) = read_dir(self.part_dir(tbl)) {
      for entry in entry_li.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
          continue;
        };
        let is_data = name.starts_with("data_");
        let is_segment = name.starts_with("segment_");
        if !is_data && !is_segment {
          continue;
        }
        if let Some((file_id, ..)) = fkv_types::parse_data_file_name(name) {
          let path = entry.path();
          if is_data {
            data.insert(file_id, path);
          } else {
            segment.insert(file_id, path);
          }
        }
      }
    }
    self.data_scan_num.fetch_add(1, Ordering::Relaxed);
    self.segment_scan_num.fetch_add(1, Ordering::Relaxed);
    self.dir_index_map.borrow_mut().insert(
      tbl.clone(),
      DirIndex {
        stamp,
        scanned: true,
        data,
        segment,
      },
    );
    true
  }

  fn index_path(&self, tbl: &TableIdent, file_id: FileId, segment: bool) -> Option<PathBuf> {
    self.dir_index_map.borrow().get(tbl).and_then(|index| {
      let path = if segment {
        index.segment.get(&file_id)
      } else {
        index.data.get(&file_id)
      };
      path.cloned()
    })
  }

  fn cached_data_path(&self, tbl: &TableIdent, file_page_id: FilePageId) -> Option<PathBuf> {
    let file_id = file_page_id >> self.pages_per_file_shift;
    if let Some(path) = self.index_path(tbl, file_id, false) {
      return Some(path);
    }
    self.refresh_dir_index(tbl, false);
    self.index_path(tbl, file_id, false)
  }

  fn data_path(&self, tbl: &TableIdent, file_page_id: FilePageId) -> PathBuf {
    if let Some(path) = self.cached_data_path(tbl, file_page_id) {
      return path;
    }
    let file_id = file_page_id >> self.pages_per_file_shift;
    let identity = self.identity.borrow();
    let path = self.part_dir(tbl).join(fkv_types::data_file_name(
      file_id,
      &identity.branch,
      identity.term,
    ));
    self
      .dir_index_map
      .borrow_mut()
      .entry(tbl.clone())
      .or_default()
      .data
      .insert(file_id, path.clone());
    path
  }

  fn cached_segment_path(&self, tbl: &TableIdent, segment_fp_id: FilePageId) -> Option<PathBuf> {
    let file_id = segment_fp_id >> self.segments_per_file_shift;
    if let Some(path) = self.index_path(tbl, file_id, true) {
      return Some(path);
    }
    self.refresh_dir_index(tbl, false);
    self.index_path(tbl, file_id, true)
  }

  fn segment_path(&self, tbl: &TableIdent, segment_fp_id: FilePageId) -> PathBuf {
    if let Some(path) = self.cached_segment_path(tbl, segment_fp_id) {
      return path;
    }
    let file_id = segment_fp_id >> self.segments_per_file_shift;
    let identity = self.identity.borrow();
    let path = self.part_dir(tbl).join(fkv_types::segment_file_name(
      file_id,
      &identity.branch,
      identity.term,
    ));
    self
      .dir_index_map
      .borrow_mut()
      .entry(tbl.clone())
      .or_default()
      .segment
      .insert(file_id, path.clone());
    path
  }

  /// 丢弃一条路径缓存并强制下次 lookup 重扫目录（文件被外部替换时使用）。
  fn forget_path(&self, tbl: &TableIdent, file_id: FileId, segment: bool) {
    if let Some(index) = self.dir_index_map.borrow_mut().get_mut(tbl) {
      if segment {
        index.segment.remove(&file_id);
      } else {
        index.data.remove(&file_id);
      }
      index.stamp = None;
    }
  }

  async fn open_data_file(&self, tbl: &TableIdent, file_page_id: FilePageId) -> Result<File> {
    let path = self
      .cached_data_path(tbl, file_page_id)
      .ok_or(KvError::ResourceMissing)?;
    match File::open(path).await {
      Ok(file) => Ok(file),
      Err(err) if err.kind() == ErrorKind::NotFound => {
        self.forget_path(tbl, file_page_id >> self.pages_per_file_shift, false);
        let path = self
          .cached_data_path(tbl, file_page_id)
          .ok_or(KvError::ResourceMissing)?;
        File::open(path).await.map_err(|_| KvError::ResourceMissing)
      }
      Err(_) => Err(KvError::ResourceMissing),
    }
  }

  async fn open_segment_file(&self, tbl: &TableIdent, segment_fp_id: FilePageId) -> Result<File> {
    let path = self
      .cached_segment_path(tbl, segment_fp_id)
      .ok_or(KvError::ResourceMissing)?;
    match File::open(path).await {
      Ok(file) => Ok(file),
      Err(err) if err.kind() == ErrorKind::NotFound => {
        self.forget_path(tbl, segment_fp_id >> self.segments_per_file_shift, true);
        let path = self
          .cached_segment_path(tbl, segment_fp_id)
          .ok_or(KvError::ResourceMissing)?;
        File::open(path).await.map_err(|_| KvError::ResourceMissing)
      }
      Err(_) => Err(KvError::ResourceMissing),
    }
  }

  /// 预加载单个 table 目录：一次扫描建立 data / segment 的 file id -> path 索引。
  /// 返回被索引的文件数。
  pub fn preload(&self, tbl: &TableIdent) -> usize {
    self.refresh_dir_index(tbl, true);
    self
      .dir_index_map
      .borrow()
      .get(tbl)
      .map_or(0, |index| index.data.len() + index.segment.len())
  }

  pub fn preload_tables(&self, tbl_li: &[TableIdent]) -> usize {
    tbl_li.iter().map(|tbl| self.preload(tbl)).sum()
  }

  /// 目录被外部同步工具（standby / rsync）改动后主动失效；下一次访问会重新扫描。
  pub fn invalidate_path_cache(&self) {
    self.dir_index_map.borrow_mut().clear();
  }

  pub fn path_scan_num(&self) -> (u64, u64) {
    (
      self.data_scan_num.load(Ordering::Relaxed),
      self.segment_scan_num.load(Ordering::Relaxed),
    )
  }

  fn page_offset(&self, file_page_id: FilePageId) -> u64 {
    let mask = (1u64 << self.pages_per_file_shift) - 1;
    (file_page_id & mask) * self.page_size as u64
  }

  fn segment_offset(&self, segment_fp_id: FilePageId) -> u64 {
    let mask = (1u64 << self.segments_per_file_shift) - 1;
    (segment_fp_id & mask) * self.segment_size as u64
  }

  fn manifest_path(&self, tbl: &TableIdent) -> PathBuf {
    self.part_dir(tbl).join("manifest_main_0")
  }

  fn tmp_manifest_path(&self, tbl: &TableIdent) -> PathBuf {
    self.part_dir(tbl).join("manifest_main_0.tmp")
  }

  fn ensure_dir(&self, tbl: &TableIdent) -> Result<()> {
    create_dir_all(self.part_dir(tbl)).map_err(|_| KvError::IoFail)
  }
}

impl PageIo for CompioStore {
  fn set_file_identity(&self, branch: &str, term: u64) {
    self.dir_index_map.borrow_mut().clear();
    *self.identity.borrow_mut() = FileIdentity {
      branch: branch.to_owned(),
      term,
    };
  }

  fn preload_paths(&self, tbl: &TableIdent) -> usize {
    self.preload(tbl)
  }

  fn invalidate_path_cache(&self) {
    self.dir_index_map.borrow_mut().clear();
  }

  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    if dst.len() < self.page_size {
      return Err(KvError::InvalidArgs);
    }
    let file = self.open_data_file(tbl, file_page_id).await?;
    let BufResult(result, buf) = file
      .read_at(vec![0u8; self.page_size], self.page_offset(file_page_id))
      .await;
    result.map_err(|_| KvError::IoFail)?;
    dst[..self.page_size].copy_from_slice(&buf[..self.page_size]);
    Ok(())
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    if src.len() != self.page_size {
      return Err(KvError::InvalidArgs);
    }
    self.ensure_dir(tbl)?;
    let mut file = OpenOptions::new()
      .write(true)
      .create(true)
      .truncate(false)
      .open(self.data_path(tbl, file_page_id))
      .await
      .map_err(|_| KvError::IoFail)?;
    let BufResult(result, _) = file
      .write_at(src.to_vec(), self.page_offset(file_page_id))
      .await;
    result.map_err(|_| KvError::IoFail)?;
    Ok(())
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    let file = OpenOptions::new()
      .write(true)
      .create(true)
      .truncate(false)
      .open(self.manifest_path(tbl))
      .await
      .map_err(|_| KvError::IoFail)?;
    file.sync_data().await.map_err(|_| KvError::IoFail)
  }
}

impl ManifestIo for CompioStore {
  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.ensure_dir(tbl)?;
    let path = self.manifest_path(tbl);
    let mut file = OpenOptions::new()
      .write(true)
      .create(true)
      .truncate(false)
      .open(&path)
      .await
      .map_err(|_| KvError::IoFail)?;
    let pos = file.metadata().await.map_err(|_| KvError::IoFail)?.len();
    let BufResult(result, _) = file.write_at(record.to_vec(), pos).await;
    result.map_err(|_| KvError::IoFail)?;
    file.sync_data().await.map_err(|_| KvError::IoFail)
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.ensure_dir(tbl)?;
    let tmp = self.tmp_manifest_path(tbl);
    let path = self.manifest_path(tbl);
    let mut file = File::create(&tmp).await.map_err(|_| KvError::IoFail)?;
    let BufResult(result, _) = file.write_at(record.to_vec(), 0).await;
    result.map_err(|_| KvError::IoFail)?;
    file.sync_data().await.map_err(|_| KvError::IoFail)?;
    rename(&tmp, &path).await.map_err(|_| KvError::IoFail)
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    match read(self.manifest_path(tbl)).await {
      Ok(bytes) => Ok(Some(bytes)),
      Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
      Err(_) => Err(KvError::IoFail),
    }
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
    match remove_file(self.manifest_path(tbl)).await {
      Ok(()) => Ok(()),
      Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
      Err(_) => Err(KvError::IoFail),
    }
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self.ensure_dir(tbl)?;
    let path = self.part_dir(tbl).join(name);
    let tmp = self.part_dir(tbl).join(format!("{name}.tmp"));
    let mut file = File::create(&tmp).await.map_err(|_| KvError::IoFail)?;
    let BufResult(result, _) = file.write_at(record.to_vec(), 0).await;
    result.map_err(|_| KvError::IoFail)?;
    file.sync_data().await.map_err(|_| KvError::IoFail)?;
    rename(&tmp, &path).await.map_err(|_| KvError::IoFail)
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    let dir = self.part_dir(tbl);
    let entry_li = match read_dir(dir) {
      Ok(entry_li) => entry_li,
      Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
      Err(_) => return Err(KvError::IoFail),
    };
    let mut name_li = Vec::new();
    for entry in entry_li.flatten() {
      let name = entry.file_name();
      let Some(name) = name.to_str() else {
        continue;
      };
      if !name.starts_with("manifest_") || name.ends_with(".tmp") {
        continue;
      }
      if fkv_types::parse_manifest_file_name(name).is_some() {
        name_li.push(name.to_owned());
      }
    }
    name_li.sort();
    Ok(name_li)
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    match read(self.part_dir(tbl).join(name)).await {
      Ok(bytes) => Ok(Some(bytes)),
      Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
      Err(_) => Err(KvError::IoFail),
    }
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    match remove_file(self.part_dir(tbl).join(name)).await {
      Ok(()) => Ok(()),
      Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
      Err(_) => Err(KvError::IoFail),
    }
  }
}

impl TermIo for CompioStore {
  async fn read_term(&self, _branch: &str, _shard_id: u32) -> Result<Option<u64>> {
    Ok(None)
  }

  async fn upsert_term(&self, _branch: &str, _shard_id: u32, _term: u64) -> Result<u64> {
    Err(KvError::NotRunning)
  }
}

impl SegmentIo for CompioStore {
  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    if dst.is_empty() {
      return Ok(());
    }
    if dst.len() > self.segment_size {
      return Err(KvError::InvalidArgs);
    }
    let file = self.open_segment_file(tbl, segment_fp_id).await?;
    let BufResult(result, buf) = file
      .read_at(vec![0u8; dst.len()], self.segment_offset(segment_fp_id))
      .await;
    let read_len = result.map_err(|_| KvError::IoFail)?;
    let copy_len = read_len.min(dst.len());
    dst[..copy_len].copy_from_slice(&buf[..copy_len]);
    dst[copy_len..].fill(0);
    Ok(())
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> Result<()> {
    if src.len() > self.segment_size {
      return Err(KvError::InvalidArgs);
    }
    self.ensure_dir(tbl)?;
    let mut buf = vec![0u8; self.segment_size];
    buf[..src.len()].copy_from_slice(src);
    let file_size = (self.segment_size as u64)
      .checked_shl(u32::from(self.segments_per_file_shift))
      .ok_or(KvError::InvalidArgs)?;
    let mut file = OpenOptions::new()
      .write(true)
      .create(true)
      .truncate(false)
      .open(self.segment_path(tbl, segment_fp_id))
      .await
      .map_err(|_| KvError::IoFail)?;
    let BufResult(result, _) = file.write_at(buf, self.segment_offset(segment_fp_id)).await;
    result.map_err(|_| KvError::IoFail)?;
    // 与 eloqstore 一致：segment 文件按 SegmentFileSize 预留固定大小。
    file.set_len(file_size).await.map_err(|_| KvError::IoFail)?;
    Ok(())
  }
}

impl FixedSegmentIo for CompioStore {
  fn register_fixed_buffer(&self, len: usize) -> Result<FixedBuffer> {
    #[cfg(target_os = "linux")]
    {
      if len == 0 {
        return Err(KvError::InvalidArgs);
      }
      let buf = Arc::new(fkv_buf::AlignedBuf::new(len));
      let mut slot = self.fixed_io.borrow_mut();
      if slot.is_none() {
        *slot = Some(fixed::FixedBuffers::current()?);
      }
      let index = slot
        .as_mut()
        .ok_or(KvError::IoFail)?
        .register(buf.clone())?;
      Ok(FixedBuffer::registered_arc(buf, index))
    }
    #[cfg(not(target_os = "linux"))]
    {
      Ok(FixedBuffer::new(len))
    }
  }

  async fn read_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &mut FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    let end = offset.checked_add(len).ok_or(KvError::InvalidArgs)?;
    if end > buf.len() || len > self.segment_size {
      return Err(KvError::InvalidArgs);
    }
    #[cfg(target_os = "linux")]
    if buf.is_registered() {
      let file = self.open_segment_file(tbl, segment_fp_id).await?;
      let op = fixed::FixedRead::new(
        file.as_raw_fd(),
        buf.share(),
        offset,
        len,
        self.segment_offset(segment_fp_id),
        buf.index(),
      )?;
      let BufResult(result, _) = compio::runtime::submit(op).await;
      let read_len = result.map_err(|_| KvError::IoFail)?;
      if read_len < len {
        buf.as_mut_slice()[offset + read_len..end].fill(0);
      }
      return Ok(());
    }
    self
      .read_segment(tbl, segment_fp_id, &mut buf.as_mut_slice()[offset..end])
      .await
  }

  async fn write_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    let end = offset.checked_add(len).ok_or(KvError::InvalidArgs)?;
    if end > buf.len() || len > self.segment_size {
      return Err(KvError::InvalidArgs);
    }
    #[cfg(target_os = "linux")]
    if buf.is_registered() {
      self.ensure_dir(tbl)?;
      let file_size = (self.segment_size as u64)
        .checked_shl(u32::from(self.segments_per_file_shift))
        .ok_or(KvError::InvalidArgs)?;
      let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(self.segment_path(tbl, segment_fp_id))
        .await
        .map_err(|_| KvError::IoFail)?;
      let op = fixed::FixedWrite::new(
        file.as_raw_fd(),
        buf.share(),
        offset,
        len,
        self.segment_offset(segment_fp_id),
        buf.index(),
      )?;
      let BufResult(result, _) = compio::runtime::submit(op).await;
      let write_len = result.map_err(|_| KvError::IoFail)?;
      if write_len != len {
        return Err(KvError::IoFail);
      }
      file.set_len(file_size).await.map_err(|_| KvError::IoFail)?;
      return Ok(());
    }
    self
      .write_segment(tbl, segment_fp_id, &buf.as_slice()[offset..end])
      .await
  }
}

impl FileIo for CompioStore {
  async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>> {
    let file_page_id = file_id << self.pages_per_file_shift;
    let Some(path) = self.cached_data_path(tbl, file_page_id) else {
      return Ok(None);
    };
    match read(&path).await {
      Ok(bytes) => Ok(Some(bytes)),
      Err(err) if err.kind() == ErrorKind::NotFound => {
        self.forget_path(tbl, file_id, false);
        let Some(path) = self.cached_data_path(tbl, file_page_id) else {
          return Ok(None);
        };
        match read(path).await {
          Ok(bytes) => Ok(Some(bytes)),
          Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
          Err(_) => Err(KvError::IoFail),
        }
      }
      Err(_) => Err(KvError::IoFail),
    }
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()> {
    if !bytes.len().is_multiple_of(self.page_size) {
      return Err(KvError::InvalidArgs);
    }
    self.ensure_dir(tbl)?;
    let path = self.data_path(tbl, file_id << self.pages_per_file_shift);
    let mut file = File::create(&path).await.map_err(|_| KvError::IoFail)?;
    let BufResult(result, _) = file.write_at(bytes.to_vec(), 0).await;
    result.map_err(|_| KvError::IoFail)?;
    Ok(())
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>> {
    let file_page_id = file_id << self.pages_per_file_shift;
    let Some(path) = self.cached_data_path(tbl, file_page_id) else {
      return Ok(None);
    };
    match File::open(&path).await {
      Ok(file) => file
        .metadata()
        .await
        .map(|meta| Some(meta.len()))
        .map_err(|_| KvError::IoFail),
      Err(err) if err.kind() == ErrorKind::NotFound => {
        self.forget_path(tbl, file_id, false);
        let Some(path) = self.cached_data_path(tbl, file_page_id) else {
          return Ok(None);
        };
        match File::open(path).await {
          Ok(file) => file
            .metadata()
            .await
            .map(|meta| Some(meta.len()))
            .map_err(|_| KvError::IoFail),
          Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
          Err(_) => Err(KvError::IoFail),
        }
      }
      Err(_) => Err(KvError::IoFail),
    }
  }

  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.refresh_dir_index(tbl, true);
    let mut file_id_li: Vec<FileId> = self
      .dir_index_map
      .borrow()
      .get(tbl)
      .map_or_else(Vec::new, |index| index.data.keys().copied().collect());
    file_id_li.sort_unstable();
    Ok(file_id_li)
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    let path = self.data_path(tbl, file_id << self.pages_per_file_shift);
    let result = match remove_file(&path).await {
      Ok(()) => Ok(()),
      Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
      Err(_) => Err(KvError::IoFail),
    };
    self.forget_path(tbl, file_id, false);
    result
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.refresh_dir_index(tbl, true);
    let mut file_id_li: Vec<FileId> = self
      .dir_index_map
      .borrow()
      .get(tbl)
      .map_or_else(Vec::new, |index| index.segment.keys().copied().collect());
    file_id_li.sort_unstable();
    Ok(file_id_li)
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    let path = self.segment_path(tbl, file_id << self.segments_per_file_shift);
    let result = match remove_file(&path).await {
      Ok(()) => Ok(()),
      Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
      Err(_) => Err(KvError::IoFail),
    };
    self.forget_path(tbl, file_id, true);
    result
  }
}
