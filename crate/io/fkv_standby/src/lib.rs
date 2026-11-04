//! Standby：Master 导出本地文件列表，Replica 通过远端 list + 整文件拉取追赶。
//!
//! 第一版同步流程：
//! 1. 列出远端 table 目录，挑最新 manifest；
//! 2. 先拉 data / segment 文件到 staging 再原子 rename；
//! 3. 最后拉 manifest 到 staging，校验完整 record 链后原子安装。
//!
//! 这样任何时刻本地 manifest 都不会指向尚未下载的数据文件。

use core::future::Future;
use std::{
  ffi::OsStr,
  fs,
  io::{Error, ErrorKind},
  path::{Path, PathBuf},
  time::UNIX_EPOCH,
};

use fkv_config::KvOptions;
use fkv_error::{KvError, Result};
use fkv_manifest::{HEADER_BYTES, MANIFEST_ALIGN, parse_header, validate_record};
use fkv_types::{StoreMode, TableIdent};

pub mod rsync;

pub use rsync::{CommandRunner, RsyncShell, RsyncSource, StdCommandRunner};

pub const STAGING_DIR: &str = ".standby";
pub const DATA_PREFIX: &str = "data_";
pub const SEGMENT_PREFIX: &str = "segment_";
pub const MANIFEST_PREFIX: &str = "manifest_";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandbyRole {
  Master,
  Replica,
}

impl StandbyRole {
  pub fn from_mode(mode: StoreMode) -> Result<Self> {
    match mode {
      StoreMode::StandbyMaster => Ok(Self::Master),
      StoreMode::StandbyReplica => Ok(Self::Replica),
      StoreMode::Local | StoreMode::Cloud => Err(KvError::InvalidArgs),
    }
  }
}

#[derive(Debug, Clone)]
pub struct StandbyConfig {
  pub role: StandbyRole,
  pub local_root: PathBuf,
  pub master_addr: String,
  pub master_paths: Vec<String>,
  pub max_concurrency: u16,
}

impl StandbyConfig {
  pub fn from_options(options: &KvOptions, local_root: impl Into<PathBuf>) -> Result<Self> {
    let config = Self {
      role: StandbyRole::from_mode(options.derive_mode())?,
      local_root: local_root.into(),
      master_addr: options.standby_master_addr.clone(),
      master_paths: options.standby_master_store_paths.clone(),
      max_concurrency: options.standby_max_concurrency,
    };
    config.validate()?;
    Ok(config)
  }

  pub fn validate(&self) -> Result<()> {
    if self.local_root.as_os_str().is_empty() || self.max_concurrency == 0 {
      return Err(KvError::InvalidArgs);
    }
    Ok(())
  }

  pub fn table_dir(&self, tbl: &TableIdent) -> PathBuf {
    table_dir(&self.local_root, tbl)
  }
}

/// 远端目录里的一个普通文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
  pub name: String,
  pub size: u64,
  pub modified: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncStats {
  pub data_fetched: usize,
  pub data_skipped: usize,
  pub manifest_installed: bool,
}

/// 远端 list / fetch 抽象。生产实现是 rsync / ssh，测试用本地目录或内存实现。
pub trait RemoteSource {
  fn list<'a>(&'a self, tbl: &'a TableIdent) -> impl Future<Output = Result<Vec<RemoteFile>>> + 'a;

  fn fetch<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
    dst: &'a Path,
  ) -> impl Future<Output = Result<()>> + 'a;
}

/// 本地目录作为远端源：用于 Master 自测 / 单机 standby。
#[derive(Debug, Clone)]
pub struct LocalDirSource {
  root: PathBuf,
}

impl LocalDirSource {
  pub fn new(root: impl Into<PathBuf>) -> Self {
    Self { root: root.into() }
  }

  pub fn root(&self) -> &Path {
    &self.root
  }
}

impl RemoteSource for LocalDirSource {
  async fn list(&self, tbl: &TableIdent) -> Result<Vec<RemoteFile>> {
    list_local_file_li(&table_dir(&self.root, tbl))
  }

  async fn fetch(&self, tbl: &TableIdent, name: &str, dst: &Path) -> Result<()> {
    if !is_safe_file_name(name) {
      return Err(KvError::InvalidArgs);
    }
    let src = table_dir(&self.root, tbl).join(name);
    match fs::copy(&src, dst) {
      Ok(_) => Ok(()),
      Err(err) if err.kind() == ErrorKind::NotFound => Err(KvError::ResourceMissing),
      Err(_) => Err(KvError::IoFail),
    }
  }
}

pub struct StandbyService<S> {
  config: StandbyConfig,
  source: S,
}

impl<S: RemoteSource> StandbyService<S> {
  pub fn new(config: StandbyConfig, source: S) -> Result<Self> {
    config.validate()?;
    Ok(Self { config, source })
  }

  pub fn config(&self) -> &StandbyConfig {
    &self.config
  }

  pub fn role(&self) -> StandbyRole {
    self.config.role
  }

  pub async fn list_remote(&self, tbl: &TableIdent) -> Result<Vec<RemoteFile>> {
    self.source.list(tbl).await
  }

  /// Master：列出本地 table 目录，供 rsync / standby client 拉取。
  pub fn export_table(&self, tbl: &TableIdent) -> Result<Vec<RemoteFile>> {
    if self.config.role != StandbyRole::Master {
      return Err(KvError::InvalidArgs);
    }
    list_local_file_li(&self.config.table_dir(tbl))
  }

  /// Replica：追赶一次某个 table。data 先落盘，manifest 最后原子安装。
  pub async fn sync_table(&self, tbl: &TableIdent) -> Result<SyncStats> {
    if self.config.role != StandbyRole::Replica {
      return Err(KvError::InvalidArgs);
    }
    let remote_li = self.source.list(tbl).await?;
    let manifest = select_manifest(&remote_li)
      .ok_or(KvError::ResourceMissing)?
      .clone();
    if !is_safe_file_name(&manifest.name) {
      return Err(KvError::Corrupted);
    }

    let local_dir = self.config.table_dir(tbl);
    let staging_dir = self
      .config
      .local_root
      .join(STAGING_DIR)
      .join(format!("{}.{}", tbl.tbl_name, tbl.partition_id));
    fs::create_dir_all(&local_dir).map_err(map_io)?;
    fs::create_dir_all(&staging_dir).map_err(map_io)?;

    let mut data_li: Vec<&RemoteFile> = remote_li
      .iter()
      .filter(|file| is_data_file(&file.name))
      .collect();
    data_li.sort_by(|left, right| left.name.cmp(&right.name));
    let mut stats = SyncStats::default();
    for file in data_li {
      if !is_safe_file_name(&file.name) {
        return Err(KvError::Corrupted);
      }
      let dst = local_dir.join(&file.name);
      if local_len(&dst)? == Some(file.size) {
        stats.data_skipped += 1;
        continue;
      }
      let tmp = staging_dir.join(format!("{}.tmp", file.name));
      self.source.fetch(tbl, &file.name, &tmp).await?;
      let fetched_len = local_len(&tmp)?.ok_or(KvError::ResourceMissing)?;
      // data 文件只追加：master 并发写入时可以比列表里的更大，但不能更小。
      if fetched_len < file.size {
        let _ = fs::remove_file(&tmp);
        return Err(KvError::TryAgain);
      }
      fs::rename(&tmp, &dst).map_err(map_io)?;
      stats.data_fetched += 1;
    }

    let tmp_manifest = staging_dir.join(format!("{}.tmp", manifest.name));
    self
      .source
      .fetch(tbl, &manifest.name, &tmp_manifest)
      .await?;
    let bytes = match fs::read(&tmp_manifest) {
      Ok(bytes) => bytes,
      Err(err) => {
        let _ = fs::remove_file(&tmp_manifest);
        return Err(map_io(err));
      }
    };
    if bytes.len() as u64 != manifest.size {
      // 列表与内容不一致说明 master 正在追写 manifest，下一轮重试。
      let _ = fs::remove_file(&tmp_manifest);
      let _ = fs::remove_dir(&staging_dir);
      return Err(KvError::TryAgain);
    }
    if let Err(err) = validate_manifest_bytes(&bytes) {
      let _ = fs::remove_file(&tmp_manifest);
      let _ = fs::remove_dir(&staging_dir);
      return Err(err);
    }
    fs::rename(&tmp_manifest, local_dir.join(&manifest.name)).map_err(map_io)?;
    stats.manifest_installed = true;
    let _ = fs::remove_dir(&staging_dir);
    Ok(stats)
  }
}

/// manifest 文件由 `align(record, 4096)` 连续记录组成；必须完整、无 torn tail。
pub fn validate_manifest_bytes(bytes: &[u8]) -> Result<()> {
  if bytes.is_empty() || !bytes.len().is_multiple_of(MANIFEST_ALIGN) {
    return Err(KvError::Corrupted);
  }
  let mut pos = 0usize;
  let mut record_num = 0usize;
  while pos < bytes.len() {
    let header = parse_header(&bytes[pos..]).map_err(|_| KvError::Corrupted)?;
    let record_end = pos
      .checked_add(HEADER_BYTES)
      .and_then(|value| value.checked_add(header.payload_len as usize))
      .ok_or(KvError::Corrupted)?;
    if record_end > bytes.len()
      || !validate_record(&bytes[pos..record_end]).map_err(|_| KvError::Corrupted)?
    {
      return Err(KvError::Corrupted);
    }
    let next = (record_end + MANIFEST_ALIGN - 1) & !(MANIFEST_ALIGN - 1);
    if next <= pos || next > bytes.len() {
      return Err(KvError::Corrupted);
    }
    pos = next;
    record_num += 1;
  }
  if record_num == 0 || pos != bytes.len() {
    return Err(KvError::Corrupted);
  }
  Ok(())
}

fn select_manifest(file_li: &[RemoteFile]) -> Option<&RemoteFile> {
  file_li
    .iter()
    .filter(|file| file.name.starts_with(MANIFEST_PREFIX))
    .max_by(|left, right| {
      (left.modified, left.name.as_str()).cmp(&(right.modified, right.name.as_str()))
    })
}

fn is_data_file(name: &str) -> bool {
  name.starts_with(DATA_PREFIX) || name.starts_with(SEGMENT_PREFIX)
}

fn is_safe_file_name(name: &str) -> bool {
  !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\\')
}

pub fn table_dir(root: &Path, tbl: &TableIdent) -> PathBuf {
  root.join(format!("{}.{}", tbl.tbl_name, tbl.partition_id))
}

pub fn list_local_file_li(dir: &Path) -> Result<Vec<RemoteFile>> {
  let entry_li = match fs::read_dir(dir) {
    Ok(entry_li) => entry_li,
    Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
    Err(_) => return Err(KvError::IoFail),
  };
  let mut file_li = Vec::new();
  for entry in entry_li.flatten() {
    let path = entry.path();
    let Ok(meta) = entry.metadata() else {
      continue;
    };
    if !meta.is_file() {
      continue;
    }
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
      continue;
    };
    let modified = meta
      .modified()
      .ok()
      .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
      .map_or(0, |duration| duration.as_secs());
    file_li.push(RemoteFile {
      name: name.to_owned(),
      size: meta.len(),
      modified,
    });
  }
  file_li.sort_by(|left, right| left.name.cmp(&right.name));
  Ok(file_li)
}

fn local_len(path: &Path) -> Result<Option<u64>> {
  match fs::metadata(path) {
    Ok(meta) => Ok(Some(meta.len())),
    Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
    Err(_) => Err(KvError::IoFail),
  }
}

fn map_io(_err: Error) -> KvError {
  KvError::IoFail
}
