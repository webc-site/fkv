//! compio-fs 后端：本地文件页 / manifest。
//!
//! - 按路径缓存已打开文件（FD LRU）：compio `File` 内部共享 `AsyncFd<std::fs::File>`，
//!   clone 只加引用计数，不产生新 fd；写路径用本地 `&mut File`/`&File` 即可。
//! - 读 / 写缓冲按大小复用，避免每页 IO 重新分配与清零。
//! - Linux 上合并页写优先走 io_uring 注册写槽位（`WRITE_FIXED`），槽位常驻复用。
//! - page 短读 / checksum 失败 `KvError::Corrupted`；短写 `KvError::IoFail`；
//!   segment 短读按定长段语义零填充（与 Mem / Sim / fixed 读一致，io-41）。
//! - term 文件 `CURRENT_TERM_<branch>_<shard_id>` 放在 store 根目录，见 `term` 模块。
//! - `sync_data` 只 fdatasync 本表写过的 data / segment 文件（dirty 集合）。
//! - manifest rename / 首次创建后 fsync 父目录，保证目录项持久化。
//! - `read_dir` / `metadata` / `create_dir_all` 走 `spawn_blocking`，不阻塞 reactor。

#[cfg(target_os = "linux")]
use std::sync::Mutex;
use std::{
  cell::{Cell, RefCell},
  io,
  path::{Path, PathBuf},
  sync::{Arc, atomic::AtomicU64},
  task::Waker,
  thread::ThreadId,
  time::SystemTime,
};

use compio::fs::{File, OpenOptions};
use fkv_buf::{AlignedBuf, PagesPool};
use fkv_error::KvError;
#[cfg(target_os = "linux")]
use fkv_io::FixedBuffer;
use fkv_types::{FileId, TMP_SUFFIX, TableIdent};
use gxhash::{HashMap, HashSet};

mod api;
mod buf;
mod dir;
mod fd_cache;
mod file_io;
#[cfg(target_os = "linux")]
mod fixed;
mod gate;
mod manifest;
mod open;
mod page_io;
mod paths;
mod read;
mod segment;
mod term;
mod write;

// FD 缓存与 IO 在飞上限的唯一来源是 `fkv_config`（对齐 C++ `kv_options.h`）：
// `DEFAULT_FD_LIMIT` / `DEFAULT_MAX_INFLIGHT_WRITE` / `DEFAULT_MAX_WRITE_BATCH_PAGES`。
// shard / runtime / SDK 装配时总会用配置值覆盖，这里的默认只服务直接构造
// `CompioStore` 的场景。
/// 注册写槽位缓存上限（槽位按 `max_write_batch_pages * page_size` 定长）。
///
/// 这不是在飞 IO 上限：超过后 `take_fixed_slot` 回落普通写池，不阻塞提交；
/// 真正的在飞上限只有 `max_inflight_write` 一个。槽位缓存单独设上限，
/// 避免配置放大后按 `max_inflight_write` 个 128 KiB 槽位常驻。
#[cfg(target_os = "linux")]
const FIXED_WRITE_SLOT_LIMIT: usize = 16;

/// managed read 单缓冲上限。
///
/// runtime 缓冲池是单尺寸常驻（默认 8 块），段尺寸过大时常驻内存与
/// provided-buffer 的收益不成比，超过该阈值仍走自管池。
const MANAGED_SEG_MAX_BUF: usize = 4 << 20;

/// 段尺寸能否启用内核 provided-buffer 读（`File::read_managed_at`）。
///
/// 只覆盖定长 segment：page 读长度多档，与单尺寸池不匹配。
pub fn managed_segment_read_supported(segment_size: usize) -> bool {
  segment_size > 0 && segment_size <= MANAGED_SEG_MAX_BUF
}

/// 单页读取遇到瞬时错误时，同一路径的最大尝试次数（含首次）。
const READ_RETRY_TIMES: usize = 3;

/// 连续页合并读的最小段长（页）。
///
/// 1 页没有可合并的邻居；2 页起合并读就已经少一次 `pread`（代价是一块段缓冲
/// 与逐页拷贝），取 2 让「窗口退化到单页」时自然回落到逐页路径。
const SPAN_COALESCE_MIN_PAGES: usize = 2;

/// 整文件读（O_DIRECT）的分块大小，同时也是单块对齐暂存缓冲的上限。
///
/// 整文件读必须返回拥有所有权的 `Vec<u8>`，而 `Vec` 的普通分配不保证页对齐，
/// 无法直接作为 O_DIRECT 读目标。分块读把对齐要求限制在一块可复用的暂存
/// 缓冲上：峰值常驻 = 输出 `Vec(len)` + 1 块，清零只发生在块首次分配时。
pub const WHOLE_READ_CHUNK: usize = 1 << 20;

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

/// 非 identity 文件变体：文件名解析出的 `(branch, term)` 与路径（rev-39）。
///
/// branch 必须保留：跨 branch 同号 file id 的变体要按自身身份独立判删，
/// 只存 term 会让 GC 无法区分 main 与 branch 的同 id 文件。
#[derive(Debug, Clone)]
struct OtherVariant {
  branch: Box<str>,
  term: u64,
  path: PathBuf,
}

/// 每个 table 分区目录一份索引。一次 read_dir 同时收集 data / segment，
/// 负查询只在目录签名变化时重新扫描。
#[derive(Debug, Default)]
struct DirIndex {
  stamp: Option<DirStamp>,
  scanned: bool,
  /// 当前 `(branch, term)` 对应的文件。
  ///
  /// 值用 `Arc<PathBuf>` 而非 `PathBuf`：页读热路径需要把路径带过 `.await`
  /// （不能跨 await 持有 `RefCell` 借用），命中时只做一次 refcount 自增，
  /// 不克隆路径、不产生堆分配（perf-03）。
  data: HashMap<FileId, Arc<PathBuf>>,
  segment: HashMap<FileId, Arc<PathBuf>>,
  /// 其它 `(branch, term)` 的同 file id 文件（保留全部 term / branch）。
  ///
  /// 读旧 term 遗留文件时取最大 term；删除按变体粒度精确清（rev-39），
  /// 整 id 删除时清掉全部变体，否则同 id 的旧文件永远无法被 GC 回收。
  /// 跨 branch 的同 id 读路由仍需 `BranchFileMapping`（todo 11）。
  data_other: HashMap<FileId, Vec<OtherVariant>>,
  segment_other: HashMap<FileId, Vec<OtherVariant>>,
}

/// data / segment 两类文件：目录索引选择、路径生成与删除入口按编译期枚举分派，
/// 避免 `bool` 参数在调用点失去语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKind {
  Data,
  Segment,
}

impl FileKind {
  /// 诊断位：data / segment 各自独立的 O_DIRECT 回退状态。
  const fn bit(self) -> u8 {
    match self {
      Self::Data => 1 << 0,
      Self::Segment => 1 << 1,
    }
  }
}

/// 分区目录扫描结果。在阻塞线程里构造，回到 reactor 后一次性替换索引。
#[derive(Debug, Default)]
struct DirScan {
  stamp: Option<DirStamp>,
  data: HashMap<FileId, Arc<PathBuf>>,
  segment: HashMap<FileId, Arc<PathBuf>>,
  data_other: HashMap<FileId, Vec<OtherVariant>>,
  segment_other: HashMap<FileId, Vec<OtherVariant>>,
}

/// 文件访问模式：同一路径的只读 / 读写句柄分开缓存，
/// 避免只读句柄被后续写路径复用（写模式带 create，不做 truncate）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Access {
  Read,
  Write,
}

/// FD 缓存节点：`prev` / `next` 为 `FdCache::node_li` 的槽位索引，
/// 与槽位数组共同组成侵入式双向链表（无 unsafe）。
#[derive(Debug)]
struct FdNode {
  path: PathBuf,
  access: Access,
  file: File,
  prev: Option<usize>,
  next: Option<usize>,
}

/// 按 (路径, 访问模式) 复用已打开文件；全局 LRU 链表 O(1) 命中 / 插入 / 淘汰。
///
/// read / write 共用一条链表，`limit` 约束两者合计条数；槽位数组 + 空闲索引栈
/// 复用被淘汰节点的位置，避免每次淘汰重新分配。
#[derive(Debug)]
struct FdCache {
  node_li: Vec<Option<FdNode>>,
  /// 空闲槽位索引栈。
  free_li: Vec<usize>,
  read: HashMap<PathBuf, usize>,
  write: HashMap<PathBuf, usize>,
  /// 最近使用端。
  head: Option<usize>,
  /// 最久未用端，淘汰入口。
  tail: Option<usize>,
  limit: usize,
  /// 缓存句柄所属线程；跨线程使用时整体丢弃。
  owner: Option<ThreadId>,
}

/// 按大小复用 IO 缓冲。
///
/// 两类语义分开：
/// - 写路径（`take` / `put`）：`write_at` 只写 `buf_len()` 字节，因此缓冲必须
///   `len == capacity == 请求长度`，新分配时清零一次后复用。
/// - 读路径（`take_read` / `put_read`）：`read_at` 的写入长度取缓冲的**容量**
///   （`Vec` 的未初始化容量 `0..capacity`），长度随后按实际读到的字节数推进，
///   因此可以用 `Vec::with_capacity` 不做整块清零；归还时只把长度归零
///   （`clear`）即可，未读区域保持原样。
///
/// 两类缓冲存放在不同 `BufPool` 实例（`read_buf` / `write_buf`），不可混用。
///
/// 每个池除块数上限外还有独立的字节预算：块长随请求长度变化（整文件读可达
/// 文件大小），只按块数封顶会让失败路径归还的大块常驻。
#[derive(Debug, Default)]
struct BufPool {
  slots: HashMap<usize, Vec<Vec<u8>>>,
  total: usize,
  /// 池内常驻缓冲的字节数（各桶 `len × 块数` 之和）。
  pooled_bytes: usize,
  alloc_num: u64,
  /// 累计租借次数（含池命中）；测试用，验证 owned 直读不碰读池。
  take_num: u64,
}

/// 4 KiB 对齐的临时缓冲，供 O_DIRECT 读写；`len` 随读入长度推进。
struct DirectBuf {
  buf: AlignedBuf,
  len: usize,
}

enum WriteChunk {
  Plain(Vec<u8>),
  Direct(DirectBuf),
}

/// 页读缓冲：普通池化缓冲或 O_DIRECT 对齐缓冲。
enum ReadChunk {
  Plain(Vec<u8>),
  Direct(DirectBuf),
}

/// 页读目标：借用切片（`read_page`）或调用方拥有的 Vec（`read_page_owned` /
/// `read_page_li`）。
///
/// compio 的 `IoBuf: 'static`，`&mut [u8]` 无法实现 `IoBufMut`；但批读签名
/// `&mut [Vec<u8>]` 里的 Vec 所有权在调用方，可 `mem::take` 后直接交给内核
/// 读（io-06）。两条路径共用同一套候选回退 / 短读重试语义。
enum PageDst<'a> {
  /// 借用切片：无所有权，池缓冲直读 + 一次性整页拷贝。
  Slice(&'a mut [u8]),
  /// 调用方拥有的 Vec：非 O_DIRECT 时内核直读（零拷贝、零读池租借）。
  Owned(&'a mut Vec<u8>),
}

impl<'a> PageDst<'a> {
  fn as_slice(&self) -> &[u8] {
    match self {
      Self::Slice(dst) => dst,
      Self::Owned(dst) => dst,
    }
  }

  fn as_mut_slice(&mut self) -> &mut [u8] {
    match self {
      Self::Slice(dst) => dst,
      Self::Owned(dst) => dst,
    }
  }
}

/// 单个候选文件的读取结论。
///
/// 只有 `Eof` / `Miss` 允许回退到更旧的 term 候选；短读耗尽、非可重试错误
/// 属于当前文件自身的终局故障，由 `read_page_candidate` 直接上抛。
enum CandidateRead {
  /// 整页读到且 checksum 通过。
  Hit,
  /// offset 处读到 0 字节（确定性 EOF，未迁移页），可回退旧 term。
  Eof,
  /// 该候选没有可用的完整页（文件不存在 / checksum 失败），继续下一候选。
  Miss,
}

/// IO 层在飞写组闸门。
///
/// task 层 `WriteGate` 限制任务级并发；这里限制 SQE 级并发，避免后台
/// compaction / GC 与前台写叠加时无界排队。单线程运行时，用 `Cell` 计数 +
/// waker 队列即可，无需跨线程同步。
///
/// 容量唯一来源是 `KvOption::max_inflight_write`（经 `with_max_inflight_write`
/// 投影）；`write_page_li` 的 `buffer_unordered` 窗口与这里取同一值，
/// 因此不存在「单次提交上限」与「全局在飞上限」两个数。
#[derive(Debug)]
struct WriteGate {
  limit: usize,
  active: Cell<usize>,
  waiter_li: RefCell<Vec<Waker>>,
}

/// 闸门许可：`Drop` 时归还名额并唤醒等待者。
struct WritePermit<'a> {
  gate: &'a WriteGate,
}

/// 在飞 fdatasync 计数（测试观察用）。
///
/// `CompioStore` 是 thread-per-core 单线程对象，用 `Cell` 即可，无需原子。
/// `now` 在每个同步 future 进入时 +1、`Drop`（含 future 取消）时 -1；
/// `peak` 记录历史最大值。用途是让计数测试能断言 `sync_data` 的实际并发度，
/// 证明它跟随脏文件数 / `max_inflight_write`，而不是某个硬编码常量。
#[derive(Debug, Default)]
struct SyncInflight {
  now: Cell<usize>,
  peak: Cell<usize>,
}

/// 在飞 fdatasync 计数守卫；`Drop` 时归还名额。
struct SyncInflightGuard<'a> {
  now: &'a Cell<usize>,
}

/// io_uring 注册写槽位池：槽位为固定长度对齐缓冲，写入走 `WRITE_FIXED`。
///
/// 槽位常驻复用，避免每次合并写重新分配大 `Vec`；池达到
/// `min(max_inflight_write, FIXED_WRITE_SLOT_LIMIT)` 后回落普通写池，
/// 不阻塞提交（在飞名额仍由 `WriteGate` 控制）。
#[cfg(target_os = "linux")]
#[derive(Debug, Default)]
struct FixedWritePool {
  free: HashMap<usize, Vec<FixedBuffer>>,
  /// 已注册槽位总数（空闲 + 出借）。
  total: usize,
  /// 累计注册次数；命中缓存时不增长，用于验证槽位复用。
  alloc_num: u64,
  /// 槽位被丢弃（写组 future 取消、在飞 op 未确认）而非复用的累计次数。
  discard_num: u64,
}

#[derive(Debug)]
pub struct CompioStore {
  root: PathBuf,
  page_size: usize,
  /// 单次 `write_at` 合并的页数上限；连续 run 按此值切组。
  max_write_batch_pages: usize,
  /// IO 层在飞 IO 上限；`write_page_li` 的提交窗口、跨调用的 `WriteGate`
  /// 与 `sync_data` 的 fdatasync 并发度共用此值（对齐 C++ `max_inflight_write`）。
  max_inflight_write: usize,
  pages_per_file_shift: u8,
  segment_size: usize,
  segments_per_file_shift: u8,
  /// 对齐 C++ `skip_verify_checksum`：true 时读 page 不校验 checksum。
  skip_verify_checksum: Cell<bool>,
  /// 数据页 O_DIRECT；仅当 page_size 4 KiB 对齐时生效。文件系统不支持
  /// `O_DIRECT`（open 返 `EINVAL` / `EOPNOTSUPP`）时按类别回退 buffered（io-18）。
  direct_io: Cell<bool>,
  /// segment 文件 O_DIRECT 请求；`segment_size` 页对齐时生效
  /// （见 [`CompioStore::direct_segment`]）。与 data 分开，回退互不影响。
  direct_seg: Cell<bool>,
  /// 已成功按 O_DIRECT 打开过的类别位。回退只在「该类还没有 direct 句柄」时
  /// 关闭开关：已经打开的 O_DIRECT fd 仍按对齐缓冲读写，不能中途切到 buffered
  /// 缓冲选择（否则对齐 fd + 非对齐写会 `EINVAL`）。
  ///
  /// 置位前先复查类别开关（io-21）：open await 期间另一任务可能已因 `EINVAL`
  /// 关闭开关，此时本次 direct 句柄必须丢弃并按 buffered 重开，不能落缓存。
  direct_opened: Cell<u8>,
  /// 已告警过回退的类别位，避免同一类别每次 open 刷屏。
  direct_fallback_warned: Cell<u8>,
  /// O_DIRECT open 命中「不支持」并回退 buffered 的累计次数（诊断 / 测试）。
  direct_fallback_num: AtomicU64,
  /// O_DIRECT open 成功返回时类别开关已被另一任务关闭、句柄被丢弃并按
  /// buffered 重开的累计次数（io-21 竞态诊断）。
  direct_discard_num: AtomicU64,
  /// 测试注入：让指定类别的 O_DIRECT open 返回 `EINVAL`（bit 见 `FileKind::bit`）。
  direct_open_err: Cell<u8>,
  /// 测试注入：只让该路径的 O_DIRECT open 返回 `EINVAL`（并发竞态用例）。
  direct_open_err_path: RefCell<Option<PathBuf>>,
  /// segment 读请求走内核 provided-buffer（compio managed read）。
  ///
  /// 仅 buffered segment 生效；O_DIRECT 下会显式关闭并告警（io-16），
  /// 生效判定见 [`CompioStore::managed_segment_read_enabled`]。
  managed_seg: Cell<bool>,
  /// O_DIRECT 对齐缓冲池：按长度分组的空闲 `AlignedBuf`。
  direct_pool: RefCell<HashMap<usize, PagesPool>>,
  identity: RefCell<FileIdentity>,
  dir_index_map: RefCell<HashMap<TableIdent, DirIndex>>,
  /// 已确认存在（`create_dir_all` 成功）的分区目录，避免每次写都重建。
  dir_ready: RefCell<HashSet<PathBuf>>,
  fd_cache: RefCell<FdCache>,
  read_buf: RefCell<BufPool>,
  write_buf: RefCell<BufPool>,
  /// IO 层在飞写组闸门（与 task 层 WriteGate 分层）。
  write_gate: WriteGate,
  /// 在飞 fdatasync 计数与峰值（测试观察用，见 `sync_inflight_peak`）。
  sync_inflight: SyncInflight,
  /// 注册写槽位池（Linux io_uring）。
  #[cfg(target_os = "linux")]
  fixed_write: RefCell<FixedWritePool>,
  /// 本表写过的 data / segment 文件；`sync_data` 对其 fdatasync 后清脏。
  dirty: RefCell<HashMap<TableIdent, HashSet<PathBuf>>>,
  /// 新建 / 删除过 data / segment 文件的表：`sync_data` 需 fsync 分区目录项。
  dirty_dir: RefCell<HashSet<TableIdent>>,
  /// 已 fsync 过目录项的 manifest 目录（首次创建 manifest 后需要一次）。
  manifest_dir_synced: RefCell<HashSet<PathBuf>>,
  /// manifest 文件追加偏移缓存：避免每次 append 前 `fstat`（io-05）。
  manifest_offset: RefCell<HashMap<PathBuf, u64>>,
  /// 下一次 manifest 追加只写前 N 字节（一次性注入，仅测试短写回滚）。
  short_write_bytes: Cell<Option<usize>>,
  /// 下一次 `read_whole` 普通读最多返回的字节数（一次性注入，仅测试短读）。
  short_read_bytes: Cell<Option<usize>>,
  /// 接下来 `n` 次页读注入瞬时 `TryAgain`（仅测试）。
  read_try_again_times: Cell<usize>,
  /// 已按 `sequence_file_size` 预分配过的 segment 文件。
  segment_preallocated: RefCell<HashSet<PathBuf>>,
  data_scan_num: AtomicU64,
  segment_scan_num: AtomicU64,
  fd_open_num: AtomicU64,
  write_at_num: AtomicU64,
  /// 页读系统调用尝试次数（含同路径重试与测试注入）。
  read_at_num: AtomicU64,
  /// 整文件读（O_DIRECT 分块路径）累计提交的块读次数（测试 / 诊断用）。
  read_whole_chunk_num: AtomicU64,
  /// 旧 term 候选回退命中次数：更新的候选 EOF / checksum 失败后从旧 term
  /// 读到完整页的次数（`todo/io-31` 的陈旧页可观测性打点）。
  stale_read_num: AtomicU64,
  /// `sync_data` 调用次数（测试用；数据 / 段文件批量 fdatasync 的 barrier 数）。
  data_sync_num: AtomicU64,
  /// 成功的目录项 fsync 次数（分区目录创建 / 文件增删 / manifest 目录）。
  dir_fsync_num: AtomicU64,
  /// 实际提交的 data / segment 文件 fdatasync 次数（每个脏文件一次）。
  ///
  /// 与 `data_sync_num`（`sync_data` 调用次数）对照：两者之差即同一 barrier
  /// 内并发同步的脏文件数，用于验证「每批只 fsync 实际写过的文件、不清扫空集合」。
  data_fsync_num: AtomicU64,
  /// manifest 追加写次数（每条 record 一次 write_at）。
  manifest_append_num: AtomicU64,
  /// manifest 文件 fdatasync 次数（与 append 一一对应，snapshot 替换走
  /// `write_manifest_record` 不计入）。
  manifest_sync_num: AtomicU64,
  #[cfg(target_os = "linux")]
  fixed_io: Arc<Mutex<Option<fixed::FixedBufferRegistry>>>,
}

/// errno → `KvError`（对齐 C++ `ToKvError`）：保留可重试 / 可诊断语义。
pub(crate) fn map_io_err(err: io::Error) -> KvError {
  use io::ErrorKind;
  match err.kind() {
    ErrorKind::NotFound => KvError::ResourceMissing,
    ErrorKind::PermissionDenied => KvError::NoPermission,
    ErrorKind::WouldBlock | ErrorKind::Interrupted => KvError::TryAgain,
    // compio io_uring 把 CQE 的 -ENOBUFS 包成 `ErrorKind::ResourceBusy` 且
    // 丢失 raw errno；provided-buffer 环耗尽按瞬时错误对齐 C++ `ToKvError`。
    ErrorKind::ResourceBusy => KvError::TryAgain,
    ErrorKind::OutOfMemory => KvError::OutOfMem,
    _ => match err.raw_os_error() {
      Some(code) if code == libc::EINTR || code == libc::EAGAIN || code == libc::ENOBUFS => {
        KvError::TryAgain
      }
      Some(code) if code == libc::EMFILE || code == libc::ENFILE => KvError::OpenFileLimit,
      Some(code) if code == libc::ENOSPC || code == libc::EDQUOT => KvError::OutOfSpace,
      _ => KvError::IoFail,
    },
  }
}

/// 数据文件按需加 `O_DIRECT`；非 Linux 保持 buffered。
fn apply_direct_flag(opts: &mut OpenOptions, direct: bool) {
  #[cfg(target_os = "linux")]
  if direct {
    opts.custom_flags(libc::O_DIRECT);
  }
  #[cfg(not(target_os = "linux"))]
  {
    let _ = (opts, direct);
  }
}

/// 同目录临时文件名：原路径追加 `.tmp`。
fn tmp_path(path: &Path) -> PathBuf {
  let mut tmp = path.as_os_str().to_owned();
  tmp.push(TMP_SUFFIX);
  PathBuf::from(tmp)
}

#[cfg(test)]
mod test {
  use std::{pin::pin, task::Poll};

  use fkv_testkit::TempRoot;
  use futures::{executor::block_on, poll};

  use super::{CompioStore, WriteGate};

  /// io-02：`sync_data` 的并发度跟随脏文件数，并受 `max_inflight_write`
  /// 封顶；不再有独立于配置的硬编码上限（旧值 16）。
  #[test]
  fn sync_inflight_limit_follows_dirty_num_and_config() {
    let root = TempRoot::new("fkv_sync_inflight");
    let store = CompioStore::new(root.path().to_path_buf(), 512, 2);
    // 默认 `max_inflight_write`（32 << 10）远大于脏文件数：一波提交完。
    assert_eq!(store.sync_inflight_limit(64), 64);
    assert_eq!(store.sync_inflight_limit(1), 1);
    // 空集合取 1，避免 `buffer_unordered(0)` 不推进非空流。
    assert_eq!(store.sync_inflight_limit(0), 1);
    // 配置收紧后按配置封顶，而不是固定 16。
    let store = store.with_max_inflight_write(4);
    assert_eq!(store.sync_inflight_limit(64), 4);
  }

  /// 闸门容量是全局在飞写组上限：满额后新请求挂起，许可 drop 后唤醒。
  #[test]
  fn write_gate_blocks_at_limit_and_wakes_on_release() {
    block_on(async {
      let gate = WriteGate::new(1);
      let mut first = pin!(gate.acquire());
      let permit = match poll!(first.as_mut()) {
        Poll::Ready(permit) => permit,
        Poll::Pending => panic!("首个请求必须立即拿到名额"),
      };
      let mut second = pin!(gate.acquire());
      assert!(matches!(poll!(second.as_mut()), Poll::Pending));

      drop(permit);
      assert!(matches!(poll!(second.as_mut()), Poll::Ready(_)));
    });
  }

  /// 装配期把配置值投影到闸门：上限放大后同时容纳两个请求。
  #[test]
  fn write_gate_limit_is_raised_before_acquire() {
    block_on(async {
      let mut gate = WriteGate::new(1);
      gate.set_limit(2);
      let mut first = pin!(gate.acquire());
      let mut second = pin!(gate.acquire());
      let _first = match poll!(first.as_mut()) {
        Poll::Ready(permit) => permit,
        Poll::Pending => panic!("上限 2 时首个请求必须立即拿到名额"),
      };
      let _second = match poll!(second.as_mut()) {
        Poll::Ready(permit) => permit,
        Poll::Pending => panic!("上限 2 时第二个请求必须立即拿到名额"),
      };
    });
  }
}
