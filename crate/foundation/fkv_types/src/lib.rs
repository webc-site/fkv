//! 基础类型与常量，对齐 `include/types.h` / `include/common.h`。

use core::{fmt, str::FromStr};

use coarsetime::Clock;
use fkv_error::{KvError, Result};
use itoa::Buffer;

pub type PageId = u32;
/// 空页哨兵：空树 root / ttl_root、无 next / prev、空链表尾都用它。
pub const NULL_PAGE: PageId = PageId::MAX;

pub type FilePageId = u64;

pub type FileId = u64;

pub type DsShardId = u32;

/// 数据页大小 / 4 KiB 缓冲对齐粒度。
///
/// C++ `page_align` 是 OS 页大小（Linux 通常 4 KiB、Apple Silicon 16 KiB），
/// 但 data page 本身的固定大小仍是 4 KiB；本常量只用于 4 KiB 倍数的数据页
/// 与缓冲对齐。manifest record 的 framing 走 [`os_page_align`]。
pub const PAGE_ALIGN: usize = 4096;
/// [`PAGE_ALIGN`] 的 u64 视图，供配置校验等 u64 算术使用。
pub const PAGE_ALIGN_U64: u64 = PAGE_ALIGN as u64;

/// OS 页大小（对齐 C++ `page_align = sysconf(_SC_PAGESIZE)`）。
///
/// manifest record 按这个粒度补齐 / 推进；Linux 为 4 KiB，Apple Silicon 为
/// 16 KiB。非 Unix 平台回落 4 KiB。
pub fn os_page_align() -> usize {
  #[cfg(unix)]
  {
    use std::sync::LazyLock;

    static PAGE_SIZE: LazyLock<usize> = LazyLock::new(|| {
      // SAFETY: `sysconf` 只读进程级常量，无参数生命周期问题。
      let value = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
      if value > 0 {
        value as usize
      } else {
        PAGE_ALIGN
      }
    });
    *PAGE_SIZE
  }
  #[cfg(not(unix))]
  {
    PAGE_ALIGN
  }
}
/// B+ 树最大高度（含叶层）。
pub const MAX_TREE_DEPTH: usize = 64;
/// 单 segment 默认大小（256 KiB，对齐 C++ `max_segment_size`）。
pub const DEFAULT_SEGMENT_SIZE: usize = 256 * 1024;
/// 每个 segment 文件的槽位数位移默认值（128 个/文件）。
pub const DEFAULT_SEGMENTS_PER_FILE_SHIFT: u8 = 7;
/// 每个 data 文件的页数位移默认值（256 Ki 页/文件）。
pub const DEFAULT_PAGES_PER_FILE_SHIFT: u8 = 18;
/// manifest 文件默认上限（8 MiB，对齐 C++ `KvOption::manifest_limit`）。
pub const DEFAULT_MANIFEST_LIMIT: u32 = 8 * 1024 * 1024;
/// 高优先请求连续服务的时间片默认值（微秒，对齐 C++ release
/// `max_processing_time_microseconds = 400`；`0` = 关闭）。
///
/// `fkv_config` 的 `KvOption` 默认值与 `fkv_shard` 的缺省回退共用本常量。
pub const DEFAULT_MAX_PROCESSING_TIME_US: u64 = 400;

pub(crate) const FILE_NAME_SEPARATOR: char = '_';
pub const FILE_NAME_DATA: &str = "data";
pub const FILE_NAME_SEGMENT: &str = "segment";
pub const FILE_NAME_MANIFEST: &str = "manifest";
pub const CURRENT_TERM_FILE_NAME: &str = "CURRENT_TERM";
pub const TMP_SUFFIX: &str = ".tmp";
pub const MAIN_BRANCH_NAME: &str = "main";

/// 映射值低 3 位编码类型，高位为 id：`(id << VAL_TYPE_BITS) | tag`。
///
/// 盘面唯一编码来源：`fkv_mapping`（写页映射）与 `fkv_manifest`（snapshot）
/// 必须共用本模块，避免两份实现漂移写坏盘面。对外只暴露 `val_type` /
/// `encode_*` / `decode_id` 函数，位宽与掩码是实现细节。
pub(crate) const VAL_TYPE_BITS: u8 = 3;
pub(crate) const VAL_TYPE_MASK: u64 = (1 << VAL_TYPE_BITS) - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ValType {
  SwizzlingPointer = 0,
  FilePageId = 1,
  PageId = 2,
  Invalid = 7,
}

/// 无效映射槽位（已删除 / 未分配）。
pub const INVALID_VALUE: u64 = ValType::Invalid as u64;
/// 空闲逻辑页链表尾哨兵。
pub const FREE_PAGE_NONE: PageId = NULL_PAGE;

pub fn val_type(value: u64) -> ValType {
  match value & VAL_TYPE_MASK {
    0 => ValType::SwizzlingPointer,
    1 => ValType::FilePageId,
    2 => ValType::PageId,
    _ => ValType::Invalid,
  }
}

pub fn encode_file_page_id(file_page_id: FilePageId) -> u64 {
  (file_page_id << VAL_TYPE_BITS) | ValType::FilePageId as u64
}

pub fn encode_page_id(page_id: PageId) -> u64 {
  (u64::from(page_id) << VAL_TYPE_BITS) | ValType::PageId as u64
}

pub fn decode_id(value: u64) -> u64 {
  value >> VAL_TYPE_BITS
}

pub fn is_file_page_id(value: u64) -> bool {
  val_type(value) == ValType::FilePageId
}

/// 毫秒墙钟。全仓唯一时间入口，内部实现可替换（coarsetime / 注入时钟）。
pub fn now_ms() -> u64 {
  Clock::now_since_epoch().as_millis()
}

/// 微秒墙钟，供需要更高精度的时间戳使用（归档 tag 等）。
pub fn now_us() -> u64 {
  Clock::now_since_epoch().as_micros()
}

pub const MAX_OVERFLOW_POINTERS: u8 = 128;
pub const MAX_READ_PAGES_BATCH: u16 = 128;
pub const MAX_SEGMENTS_BATCH: u16 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoreMode {
  Local = 0,
  StandbyMaster,
  StandbyReplica,
  Cloud,
}

/// 采纳外部（远端）当前快照的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallOutcome {
  /// 远端状态已 replay 并原子安装。
  Installed,
  /// 远端尚无 manifest：本地状态已清空，等待远端状态出现。
  WaitingRemote,
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

  /// 分区 -> shard 路由。对齐 C++ `TableIdent::ShardIndex`：
  /// 只看 `partition_id % num_shards`，与表名无关。只影响运行期路由，
  /// 不写入盘面。
  ///
  /// 因此同一 `partition_id` 的不同表必然落在同一 shard：分区号应由
  /// 调用方按 shard 数均匀分配，避免热点集中（见 issue 47）。
  pub fn shard_index(&self, num_shards: u16) -> u16 {
    // `Runtime::start_impl` 拒绝 0 线程，生产路径不会传 0；这里不再静默兜底，
    // 免得把「配置成 0 shard」这种编程错误变成「全表挤到 0 号 shard」。
    debug_assert_ne!(num_shards, 0, "shard 数必须非 0");
    (self.partition_id % u32::from(num_shards)) as u16
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
  id_file_name(FILE_NAME_DATA, file_id, branch, term)
}

pub fn segment_file_name(file_id: FileId, branch: &str, term: u64) -> String {
  id_file_name(FILE_NAME_SEGMENT, file_id, branch, term)
}

/// `<prefix>_<file_id>_<branch>_<term>`，data / segment 共用。
fn id_file_name(prefix: &str, file_id: FileId, branch: &str, term: u64) -> String {
  let mut name = String::with_capacity(prefix.len() + branch.len() + 48);
  name.push_str(prefix);
  name.push(FILE_NAME_SEPARATOR);
  itoa_u64(&mut name, file_id);
  name.push(FILE_NAME_SEPARATOR);
  name.push_str(branch);
  name.push(FILE_NAME_SEPARATOR);
  itoa_u64(&mut name, term);
  name
}

fn itoa_u64(dst: &mut String, value: u64) {
  let mut buf = Buffer::new();
  dst.push_str(buf.format(value));
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
    .strip_prefix(FILE_NAME_DATA)
    .or_else(|| name.strip_prefix(FILE_NAME_SEGMENT))?;
  let rest = rest.strip_prefix(FILE_NAME_SEPARATOR)?;
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
  // `<name>.tmp` 是整写未 rename 的中间态（`CompioStore` tmp + rename），不是
  // 可见的 manifest：列举时必须跳过，否则同一归档周期会同时看到 tmp 与正式名，
  // `(tag, term)` 去重后仍多出一条。
  if name.ends_with(TMP_SUFFIX) {
    return None;
  }
  let rest = name.strip_prefix(FILE_NAME_MANIFEST)?;
  let rest = rest.strip_prefix(FILE_NAME_SEPARATOR)?;
  let mut part_li = rest.splitn(2, FILE_NAME_SEPARATOR);
  let branch = part_li.next()?;
  if !is_valid_branch_name(branch) {
    return None;
  }
  // 对齐 C++ `ParseManifestFileSuffix`：拒绝 legacy `manifest_<term>`（branch 段
  // 为纯数字的格式），否则会被误当成 branch 名为数字的 manifest。
  if branch.parse::<u64>().is_ok() {
    return None;
  }
  let remainder = part_li.next()?;
  let mut part_li = remainder.splitn(2, FILE_NAME_SEPARATOR);
  let term = part_li.next()?.parse().ok()?;
  let tag = part_li.next();
  Some((branch, term, tag))
}

/// TTL 过期判定：`expire_ts == 0` 表示永不过期。
pub fn is_expired(expire_ts: u64, now_ms: u64) -> bool {
  expire_ts != 0 && expire_ts <= now_ms
}
