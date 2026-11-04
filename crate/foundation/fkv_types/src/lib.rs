//! 基础类型与常量，对齐 `include/types.h` / `include/common.h`。

use core::{fmt, str::FromStr};

use fkv_error::{KvError, Result};

pub type PageId = u32;
pub const MAX_PAGE_ID: PageId = u32::MAX;

pub type FilePageId = u64;
pub const MAX_FILE_PAGE_ID: FilePageId = u64::MAX;

pub type FileId = u64;
pub const MAX_FILE_ID: FileId = u64::MAX;

pub type DsShardId = u32;
pub const UNSET_DS_SHARD: DsShardId = u32::MAX;

pub const FILE_NAME_SEPARATOR: char = '_';
pub const FILE_NAME_DATA: &str = "data";
pub const FILE_NAME_SEGMENT: &str = "segment";
pub const FILE_NAME_MANIFEST: &str = "manifest";
pub const CURRENT_TERM_FILE_NAME: &str = "CURRENT_TERM";
pub const TMP_SUFFIX: &str = ".tmp";
pub const MAIN_BRANCH_NAME: &str = "main";

pub const MAX_OVERFLOW_POINTERS: u8 = 128;
pub const MAX_READ_PAGES_BATCH: u16 = 128;
pub const MAX_SEGMENTS_BATCH: u16 = 8;
pub const DEFAULT_SCAN_PREFETCH_PAGE_COUNT: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoreMode {
  Local = 0,
  StandbyMaster,
  StandbyReplica,
  Cloud,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum WriteOp {
  Upsert = 0,
  Delete,
}

impl WriteOp {
  pub const fn is_delete(self) -> bool {
    matches!(self, Self::Delete)
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TableIdent {
  pub tbl_name: Box<str>,
  pub partition_id: u32,
}

impl TableIdent {
  pub fn new(tbl_name: impl Into<Box<str>>, partition_id: u32) -> Result<Self> {
    let tbl_name = tbl_name.into();
    if tbl_name.is_empty() || tbl_name.contains('\0') {
      return Err(KvError::InvalidArgs);
    }
    Ok(Self {
      tbl_name,
      partition_id,
    })
  }

  pub fn is_valid(&self) -> bool {
    !self.tbl_name.is_empty() && !self.tbl_name.contains('\0')
  }

  /// 分区 -> shard 的稳定哈希。只影响运行期路由，不写入盘面。
  pub fn shard_index(&self, num_shards: u16) -> u16 {
    if num_shards == 0 {
      return 0;
    }
    let hash = gxhash::gxhash64(self.tbl_name.as_bytes(), self.partition_id as i64);
    (hash % u64::from(num_shards)) as u16
  }
}

impl fmt::Display for TableIdent {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}.{}", self.tbl_name, self.partition_id)
  }
}

impl FromStr for TableIdent {
  type Err = KvError;

  fn from_str(text: &str) -> Result<Self> {
    let Some((name, partition)) = text.rsplit_once('.') else {
      return Err(KvError::InvalidArgs);
    };
    let partition_id = partition.parse().map_err(|_| KvError::InvalidArgs)?;
    Self::new(name, partition_id)
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BranchFileRange {
  pub branch_name: Box<str>,
  pub term: u64,
  pub max_file_id: FileId,
  pub max_segment_file_id: FileId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RetainedFileKey {
  pub file_id: FileId,
  pub branch_name: Box<str>,
  pub term: u64,
}

/// 对齐 `NormalizeBranchName`：只允许 `[a-zA-Z0-9-]`，转小写。
pub fn normalize_branch_name(name: &str) -> Option<Box<str>> {
  if name.is_empty() {
    return None;
  }
  let mut out = String::with_capacity(name.len());
  for ch in name.chars() {
    match ch {
      'a'..='z' | '0'..='9' | '-' => out.push(ch),
      'A'..='Z' => out.push(ch.to_ascii_lowercase()),
      _ => return None,
    }
  }
  Some(out.into_boxed_str())
}

pub fn is_valid_branch_name(name: &str) -> bool {
  !name.is_empty()
    && name
      .chars()
      .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
}

pub fn data_file_name(file_id: FileId, branch: &str, term: u64) -> String {
  format!(
    "{FILE_NAME_DATA}{FILE_NAME_SEPARATOR}{file_id}{FILE_NAME_SEPARATOR}{branch}{FILE_NAME_SEPARATOR}{term}"
  )
}

pub fn segment_file_name(file_id: FileId, branch: &str, term: u64) -> String {
  format!(
    "{FILE_NAME_SEGMENT}{FILE_NAME_SEPARATOR}{file_id}{FILE_NAME_SEPARATOR}{branch}{FILE_NAME_SEPARATOR}{term}"
  )
}

pub fn manifest_file_name(branch: &str, term: u64) -> String {
  format!("{FILE_NAME_MANIFEST}{FILE_NAME_SEPARATOR}{branch}{FILE_NAME_SEPARATOR}{term}")
}

pub fn archive_file_name(branch: &str, term: u64, tag: &str) -> String {
  format!(
    "{FILE_NAME_MANIFEST}{FILE_NAME_SEPARATOR}{branch}{FILE_NAME_SEPARATOR}{term}{FILE_NAME_SEPARATOR}{tag}"
  )
}

pub fn current_term_file_name(branch: &str, shard_id: DsShardId) -> String {
  format!("{CURRENT_TERM_FILE_NAME}{FILE_NAME_SEPARATOR}{branch}{FILE_NAME_SEPARATOR}{shard_id}")
}

/// 解析 `data_<file_id>_<branch>_<term>` / `segment_...`。
pub fn parse_data_file_name(name: &str) -> Option<(FileId, &str, u64)> {
  let rest = name
    .strip_prefix("data_")
    .or_else(|| name.strip_prefix("segment_"))?;
  let mut part_li = rest.splitn(3, FILE_NAME_SEPARATOR);
  let file_id = part_li.next()?.parse().ok()?;
  let branch = part_li.next()?;
  let term = part_li.next()?.parse().ok()?;
  if !is_valid_branch_name(branch) {
    return None;
  }
  Some((file_id, branch, term))
}

/// 解析 `manifest_<branch>_<term>` 或 `manifest_<branch>_<term>_<tag>`。
pub fn parse_manifest_file_name(name: &str) -> Option<(&str, u64, Option<&str>)> {
  let rest = name.strip_prefix("manifest_")?;
  let mut part_li = rest.splitn(2, FILE_NAME_SEPARATOR);
  let branch = part_li.next()?;
  if !is_valid_branch_name(branch) {
    return None;
  }
  let remainder = part_li.next()?;
  let mut part_li = remainder.splitn(2, FILE_NAME_SEPARATOR);
  let term = part_li.next()?.parse().ok()?;
  let tag = part_li.next();
  Some((branch, term, tag))
}
