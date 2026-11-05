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

use core::{mem::MaybeUninit, slice::from_raw_parts_mut};
#[cfg(target_os = "linux")]
use std::sync::{Arc, Mutex};
use std::{
  cell::{Cell, RefCell},
  cmp::Reverse,
  fs,
  fs::{create_dir_all, metadata, read_dir, remove_dir},
  future::poll_fn,
  io::{self, ErrorKind},
  path::{Path, PathBuf},
  sync::atomic::{AtomicU64, Ordering},
  task::{Poll, Waker},
  thread::{self, ThreadId},
  time::SystemTime,
};

#[cfg(target_os = "linux")]
use compio::driver::ToSharedFd;
use compio::{
  buf::{BufResult, IoBuf, IoBufMut, SetLen},
  fs::{File, OpenOptions, remove_file, rename},
  io::{AsyncReadAt, AsyncReadManagedAt, AsyncWriteAt},
  runtime::spawn_blocking,
};
use fkv_buf::{AlignedBuf, PagesPool};
use fkv_error::{KvError, Result};
use fkv_failpoint::points::{
  APPEND_SYNC, APPEND_SYNC_FAIL, APPEND_WRITE, ATOMIC_RENAME, ATOMIC_SYNC, ATOMIC_SYNC_DIR,
  SYNC_DIR, WRITE_FILE_RENAME, WRITE_FILE_SYNC_FAIL, WRITE_PAGE,
};
use fkv_io::{
  FileIo, FileVariant, FixedBuffer, FixedSegmentIo, FixedSegmentRead, ManifestIo, PageIo,
  SegmentIo, TermIo, validate_fixed_segment_li,
};
use fkv_page::validate_checksum;
use fkv_types::{
  DEFAULT_SEGMENT_SIZE, DEFAULT_SEGMENTS_PER_FILE_SHIFT, FILE_NAME_DATA, FILE_NAME_MANIFEST,
  FILE_NAME_SEGMENT, MAIN_BRANCH_NAME, PAGE_ALIGN, TMP_SUFFIX, data_file_name, manifest_file_name,
  parse_data_file_name, parse_manifest_file_name, segment_file_name,
};
use futures::{StreamExt, future::try_join_all, stream};
#[cfg(target_os = "linux")]
mod fixed;
mod term;
use fkv_config::{DEFAULT_FD_LIMIT, DEFAULT_MAX_INFLIGHT_WRITE, DEFAULT_MAX_WRITE_BATCH_PAGES};
use fkv_types::{FileId, FilePageId, TableIdent};
use gxhash::{HashMap, HashSet};

// FD 缓存与写 IO 上限的唯一来源是 `fkv_config`（对齐 C++ `kv_options.h`）：
// `DEFAULT_FD_LIMIT` / `DEFAULT_MAX_INFLIGHT_WRITE` / `DEFAULT_MAX_WRITE_BATCH_PAGES`。
// shard / runtime / SDK 装配时总会用配置值覆盖，这里的默认只服务直接构造
// `CompioStore` 的场景。
/// 注册写槽位缓存上限（槽位按 `max_write_batch_pages * page_size` 定长）。
///
/// 这不是在飞写上限：超过后 `take_fixed_slot` 回落普通写池，不阻塞提交；
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

/// 单次 `sync_data` 同时在飞的 fdatasync 数。
const FSYNC_INFLIGHT_LIMIT: usize = 16;
/// 缓冲池保留的缓冲块数上限（按大小分组）。
const BUF_POOL_LIMIT: usize = 8;
/// O_DIRECT 对齐池的总内存上限：`segment` 读长度随值长度变化，按长度分桶
/// 会放大常驻内存，归还时按全局字节数封顶。
const DIRECT_POOL_BUDGET: usize = 16 * 1024 * 1024;
/// 单页读取遇到瞬时错误时，同一路径的最大尝试次数（含首次）。
const READ_RETRY_TIMES: usize = 3;

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

impl OtherVariant {
  /// 变体身份三元组的 `(branch, term)` 部分。
  fn ident(&self) -> (&str, u64) {
    (self.branch.as_ref(), self.term)
  }
}

/// 每个 table 分区目录一份索引。一次 read_dir 同时收集 data / segment，
/// 负查询只在目录签名变化时重新扫描。
#[derive(Debug, Default)]
struct DirIndex {
  stamp: Option<DirStamp>,
  scanned: bool,
  /// 当前 `(branch, term)` 对应的文件。
  data: HashMap<FileId, PathBuf>,
  segment: HashMap<FileId, PathBuf>,
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

impl DirIndex {
  /// 当前 `(branch, term)` 的 file id → 路径。
  fn primary(&self, kind: FileKind) -> &HashMap<FileId, PathBuf> {
    match kind {
      FileKind::Data => &self.data,
      FileKind::Segment => &self.segment,
    }
  }

  fn primary_mut(&mut self, kind: FileKind) -> &mut HashMap<FileId, PathBuf> {
    match kind {
      FileKind::Data => &mut self.data,
      FileKind::Segment => &mut self.segment,
    }
  }

  /// 其它 `(branch, term)` 的同 file id 文件。
  fn other(&self, kind: FileKind) -> &HashMap<FileId, Vec<OtherVariant>> {
    match kind {
      FileKind::Data => &self.data_other,
      FileKind::Segment => &self.segment_other,
    }
  }

  fn other_mut(&mut self, kind: FileKind) -> &mut HashMap<FileId, Vec<OtherVariant>> {
    match kind {
      FileKind::Data => &mut self.data_other,
      FileKind::Segment => &mut self.segment_other,
    }
  }
}

/// 分区目录扫描结果。在阻塞线程里构造，回到 reactor 后一次性替换索引。
#[derive(Debug, Default)]
struct DirScan {
  stamp: Option<DirStamp>,
  data: HashMap<FileId, PathBuf>,
  segment: HashMap<FileId, PathBuf>,
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

impl Default for FdCache {
  fn default() -> Self {
    Self {
      node_li: Vec::new(),
      free_li: Vec::new(),
      read: HashMap::default(),
      write: HashMap::default(),
      head: None,
      tail: None,
      limit: DEFAULT_FD_LIMIT as usize,
      owner: None,
    }
  }
}

// SAFETY: compio `File` 内部是线程本地的引用计数，本身 `!Send`。这里显式实现 `Send`
// 依赖 `bind_thread`：句柄只在打开它的线程内使用，`CompioStore` 被整体 move 到别的线程时
// 首次访问即清空缓存，不存在两个线程同时持有同一句柄的情况。
unsafe impl Send for FdCache {}

impl FdCache {
  /// 绑定当前线程；线程变化时丢弃旧句柄（compio thread-per-core 契约）。
  fn bind_thread(&mut self) {
    let current = thread::current().id();
    if self.owner != Some(current) {
      self.clear();
      self.owner = Some(current);
    }
  }

  fn map(&self, access: Access) -> &HashMap<PathBuf, usize> {
    match access {
      Access::Read => &self.read,
      Access::Write => &self.write,
    }
  }

  fn map_mut(&mut self, access: Access) -> &mut HashMap<PathBuf, usize> {
    match access {
      Access::Read => &mut self.read,
      Access::Write => &mut self.write,
    }
  }

  /// 从链表摘除节点；索引无效时空操作。
  fn unlink(&mut self, index: usize) {
    let Some(node) = self.node_li[index].as_mut() else {
      return;
    };
    let (prev, next) = (node.prev.take(), node.next.take());
    match prev {
      Some(prev) => {
        if let Some(prev_node) = self.node_li[prev].as_mut() {
          prev_node.next = next;
        }
      }
      None => self.head = next,
    }
    match next {
      Some(next) => {
        if let Some(next_node) = self.node_li[next].as_mut() {
          next_node.prev = prev;
        }
      }
      None => self.tail = prev,
    }
  }

  /// 头插：标记为最近使用。
  fn push_front(&mut self, index: usize) {
    let old_head = self.head;
    if let Some(node) = self.node_li[index].as_mut() {
      node.prev = None;
      node.next = old_head;
    }
    match old_head {
      Some(head) => {
        if let Some(head_node) = self.node_li[head].as_mut() {
          head_node.prev = Some(index);
        }
      }
      None => self.tail = Some(index),
    }
    self.head = Some(index);
  }

  /// 取一个槽位：优先复用空闲索引，否则追加。
  fn alloc(&mut self, node: FdNode) -> usize {
    match self.free_li.pop() {
      Some(index) => {
        self.node_li[index] = Some(node);
        index
      }
      None => {
        self.node_li.push(Some(node));
        self.node_li.len() - 1
      }
    }
  }

  /// 摘除并回收槽位（同时清索引）；文件句柄随节点 drop 关闭。
  fn release(&mut self, index: usize) {
    let Some(node) = self.node_li[index].take() else {
      return;
    };
    self.map_mut(node.access).remove(&node.path);
    self.free_li.push(index);
  }

  /// 命中后移到队首（O(1)），返回句柄。
  fn touch(&mut self, access: Access, path: &Path) -> Option<File> {
    self.bind_thread();
    let index = *self.map(access).get(path)?;
    self.unlink(index);
    self.push_front(index);
    let node = self.node_li[index].as_ref()?;
    Some(node.file.clone())
  }

  /// 取只读句柄。
  fn get_read(&mut self, path: &Path) -> Option<File> {
    self.touch(Access::Read, path)
  }

  /// 取写句柄。
  fn get_write(&mut self, path: &Path) -> Option<File> {
    self.touch(Access::Write, path)
  }

  fn insert(&mut self, path: PathBuf, access: Access, file: File) {
    self.bind_thread();
    // 同 (路径, 模式) 已缓存：原位替换句柄并移到队首，不增长条数。
    if let Some(&index) = self.map(access).get(&path) {
      if let Some(node) = self.node_li[index].as_mut() {
        node.file = file;
      }
      self.unlink(index);
      self.push_front(index);
      return;
    }
    let index = self.alloc(FdNode {
      path: path.clone(),
      access,
      file,
      prev: None,
      next: None,
    });
    self.map_mut(access).insert(path, index);
    self.push_front(index);
    // 超限从队尾逐个淘汰，单次 O(1)；`limit == 0` 表示不限制。
    if self.limit != 0 {
      while self.len() > self.limit {
        let Some(tail) = self.tail else {
          break;
        };
        self.unlink(tail);
        self.release(tail);
      }
    }
  }

  /// 丢弃某目录下所有句柄（detach 驱逐 table）。
  fn forget_under(&mut self, dir: &Path) {
    let index_li: Vec<usize> = self
      .node_li
      .iter()
      .enumerate()
      .filter_map(|(index, node)| {
        node
          .as_ref()
          .filter(|node| node.path.starts_with(dir))
          .map(|_| index)
      })
      .collect();
    for index in index_li {
      self.unlink(index);
      self.release(index);
    }
  }

  /// 失效一条路径的两种模式句柄。
  fn remove(&mut self, path: &Path) {
    for access in [Access::Read, Access::Write] {
      if let Some(&index) = self.map(access).get(path) {
        self.unlink(index);
        self.release(index);
      }
    }
  }

  fn len(&self) -> usize {
    self.read.len() + self.write.len()
  }

  fn clear(&mut self) {
    self.node_li.clear();
    self.free_li.clear();
    self.read.clear();
    self.write.clear();
    self.head = None;
    self.tail = None;
  }
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
#[derive(Debug, Default)]
struct BufPool {
  slots: HashMap<usize, Vec<Vec<u8>>>,
  total: usize,
  alloc_num: u64,
}

/// 4 KiB 对齐的临时缓冲，供 O_DIRECT 读写；`len` 随读入长度推进。
struct DirectBuf {
  buf: AlignedBuf,
  len: usize,
}

impl DirectBuf {
  fn from_buf(buf: AlignedBuf, len: usize) -> Self {
    debug_assert!(len <= buf.len());
    Self { buf, len }
  }

  fn from_slice(buf: AlignedBuf, src: &[u8]) -> Self {
    let mut out = Self::from_buf(buf, src.len());
    out.as_mut_slice().copy_from_slice(src);
    out
  }

  fn into_inner(self) -> AlignedBuf {
    self.buf
  }
}

impl IoBuf for DirectBuf {
  fn as_init(&self) -> &[u8] {
    &self.buf[..self.len]
  }
}

impl SetLen for DirectBuf {
  unsafe fn set_len(&mut self, len: usize) {
    self.len = len;
  }
}

impl IoBufMut for DirectBuf {
  fn as_uninit(&mut self) -> &mut [MaybeUninit<u8>] {
    // SAFETY: `AlignedBuf` 独占整块 `len` 字节；`MaybeUninit` 允许未初始化读取。
    unsafe { from_raw_parts_mut(self.buf.as_mut_ptr().cast(), self.buf.len()) }
  }
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

impl ReadChunk {
  fn as_slice(&self) -> &[u8] {
    match self {
      Self::Plain(buf) => buf,
      Self::Direct(buf) => buf.as_init(),
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

impl BufPool {
  /// 取一块长度为 `len` 的写缓冲；池空时新分配（清零一次）并计数。
  fn take(&mut self, len: usize) -> Vec<u8> {
    if let Some(buf) = self.slots.get_mut(&len).and_then(Vec::pop) {
      self.total -= 1;
      debug_assert_eq!(buf.len(), len);
      return buf;
    }
    self.alloc_num += 1;
    vec![0u8; len]
  }

  /// 归还写缓冲；容量不符请求长度的缓冲不复用（写入长度即缓冲长度）。
  fn put(&mut self, len: usize, mut buf: Vec<u8>) {
    if buf.capacity() != len || self.total >= BUF_POOL_LIMIT {
      return;
    }
    if buf.len() != len {
      buf.resize(len, 0);
    }
    self.slots.entry(len).or_default().push(buf);
    self.total += 1;
  }

  /// 取一块**容量**为 `len` 的读缓冲：不预清零，交给 compio 直接读入。
  fn take_read(&mut self, len: usize) -> Vec<u8> {
    if let Some(buf) = self.slots.get_mut(&len).and_then(Vec::pop) {
      self.total -= 1;
      debug_assert_eq!(buf.capacity(), len);
      return buf;
    }
    self.alloc_num += 1;
    Vec::with_capacity(len)
  }

  /// 归还读缓冲：长度归零、内容不清零（短读后的未读区域也保持原样）。
  fn put_read(&mut self, len: usize, mut buf: Vec<u8>) {
    if buf.capacity() != len || self.total >= BUF_POOL_LIMIT {
      return;
    }
    buf.clear();
    self.slots.entry(len).or_default().push(buf);
    self.total += 1;
  }
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

impl Drop for WritePermit<'_> {
  fn drop(&mut self) {
    self.gate.release();
  }
}

impl WriteGate {
  const fn new(limit: usize) -> Self {
    Self {
      limit,
      active: Cell::new(0),
      waiter_li: RefCell::new(Vec::new()),
    }
  }

  /// 装配期覆盖容量。只能在闸门空闲（尚未取过名额）时调用。
  fn set_limit(&mut self, limit: usize) {
    debug_assert_eq!(self.active.get(), 0, "WriteGate 必须在空闲时改容量");
    self.limit = limit;
  }

  /// 取一个名额；已满时注册 waker 挂起，由归还方唤醒。
  fn acquire(&self) -> impl Future<Output = WritePermit<'_>> + '_ {
    poll_fn(move |cx| {
      if self.active.get() < self.limit {
        self.active.set(self.active.get() + 1);
        return Poll::Ready(WritePermit { gate: self });
      }
      let mut waiter_li = self.waiter_li.borrow_mut();
      if !waiter_li.iter().any(|waker| waker.will_wake(cx.waker())) {
        waiter_li.push(cx.waker().clone());
      }
      Poll::Pending
    })
  }

  fn release(&self) {
    self.active.set(self.active.get().saturating_sub(1));
    for waker in self.waiter_li.borrow_mut().drain(..) {
      waker.wake();
    }
  }
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
}

#[derive(Debug)]
pub struct CompioStore {
  root: PathBuf,
  page_size: usize,
  /// 单次 `write_at` 合并的页数上限；连续 run 按此值切组。
  max_write_batch_pages: usize,
  /// IO 层在飞写组上限；`write_page_li` 的提交窗口与跨调用的 `WriteGate`
  /// 共用此值（对齐 C++ `max_inflight_write`）。
  max_inflight_write: usize,
  pages_per_file_shift: u8,
  segment_size: usize,
  segments_per_file_shift: u8,
  /// 对齐 C++ `skip_verify_checksum`：true 时读 page 不校验 checksum。
  skip_verify_checksum: Cell<bool>,
  /// 数据页 O_DIRECT；仅当 page_size 4 KiB 对齐时生效。
  direct_io: Cell<bool>,
  /// segment 读走内核 provided-buffer（compio managed read）。
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
  /// 旧 term 候选回退命中次数：更新的候选 EOF / checksum 失败后从旧 term
  /// 读到完整页的次数（`todo/io-31` 的陈旧页可观测性打点）。
  stale_read_num: AtomicU64,
  /// `sync_data` 调用次数（测试用；数据 / 段文件批量 fdatasync 的 barrier 数）。
  data_sync_num: AtomicU64,
  /// 成功的目录项 fsync 次数（分区目录创建 / 文件增删 / manifest 目录）。
  dir_fsync_num: AtomicU64,
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

/// 删除分区目录里残留的 `*.tmp`（原子写崩溃半成品）。只用于 attach 前。
fn remove_stale_tmp(dir: &Path) {
  let Ok(entry_li) = read_dir(dir) else {
    return;
  };
  for entry in entry_li.flatten() {
    let path = entry.path();
    let is_tmp = path
      .file_name()
      .and_then(|name| name.to_str())
      .is_some_and(|name| name.ends_with(TMP_SUFFIX));
    if is_tmp {
      let _ = fs::remove_file(&path);
    }
  }
}

/// 分区目录扫描：目录签名 + data / segment 路径索引（纯同步，供阻塞线程调用）。
fn scan_dir(dir: &Path, branch: &str, term: u64) -> DirScan {
  let mut scan = DirScan {
    stamp: dir_stamp_of(dir),
    ..DirScan::default()
  };
  let Ok(entry_li) = read_dir(dir) else {
    return scan;
  };
  for entry in entry_li.flatten() {
    let name = entry.file_name();
    let Some(name) = name.to_str() else {
      continue;
    };
    let is_data = name.starts_with(FILE_NAME_DATA);
    let is_segment = name.starts_with(FILE_NAME_SEGMENT);
    if !is_data && !is_segment {
      continue;
    }
    if let Some((file_id, file_branch, file_term)) = parse_data_file_name(name) {
      let path = entry.path();
      if file_branch == branch && file_term == term {
        if is_data {
          scan.data.insert(file_id, path);
        } else {
          scan.segment.insert(file_id, path);
        }
      } else {
        // 保留全部非当前 identity 的变体（branch + term）：GC 按变体身份
        // 精确判删（rev-39），缺 branch 无法区分跨 branch 的同 id 文件。
        let other = if is_data {
          &mut scan.data_other
        } else {
          &mut scan.segment_other
        };
        other.entry(file_id).or_default().push(OtherVariant {
          branch: file_branch.into(),
          term: file_term,
          path,
        });
      }
    }
  }
  scan
}

/// 目录签名。`metadata` 是阻塞 syscall，只允许在阻塞线程或 attach 阶段调用。
fn dir_stamp_of(dir: &Path) -> Option<DirStamp> {
  let meta = metadata(dir).ok()?;
  Some(DirStamp {
    modified: meta.modified().ok(),
    len: meta.len(),
  })
}

/// 在阻塞线程池执行同步目录 IO。阻塞线程 panic 时按 `IoFail` 处理。
async fn spawn_dir_io<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T> {
  spawn_blocking(f).await.map_err(|_join| KvError::IoFail)
}

/// 取已打开文件的 (dev, ino)。非 unix 平台不做替换检测。
impl CompioStore {
  /// 该路径是否按 O_DIRECT 打开。
  ///
  /// data 文件由 `page_size` 对齐保证；segment 文件必须 `segment_size`
  /// 为页大小整数倍（否则整段定长读写无法对齐，保持 buffered）。
  fn is_direct_path(&self, path: &Path) -> bool {
    if !self.direct_io.get() {
      return false;
    }
    path
      .file_name()
      .and_then(|name| name.to_str())
      .is_some_and(|name| {
        name.starts_with(FILE_NAME_DATA)
          || (name.starts_with(FILE_NAME_SEGMENT) && self.direct_segment())
      })
  }

  /// segment 是否可用 O_DIRECT：`segment_size` 必须是页大小整数倍。
  fn direct_segment(&self) -> bool {
    self.direct_io.get() && self.segment_size.is_multiple_of(PAGE_ALIGN)
  }

  pub fn new(root: impl Into<PathBuf>, page_size: usize, pages_per_file_shift: u8) -> Self {
    Self {
      root: root.into(),
      page_size,
      max_write_batch_pages: DEFAULT_MAX_WRITE_BATCH_PAGES as usize,
      max_inflight_write: DEFAULT_MAX_INFLIGHT_WRITE as usize,
      pages_per_file_shift,
      segment_size: DEFAULT_SEGMENT_SIZE,
      segments_per_file_shift: DEFAULT_SEGMENTS_PER_FILE_SHIFT,
      skip_verify_checksum: Cell::new(false),
      direct_io: Cell::new(false),
      managed_seg: Cell::new(false),
      direct_pool: RefCell::new(HashMap::default()),
      identity: RefCell::new(FileIdentity {
        branch: MAIN_BRANCH_NAME.to_owned(),
        term: 0,
      }),
      dir_index_map: RefCell::new(HashMap::default()),
      dir_ready: RefCell::new(HashSet::default()),
      fd_cache: RefCell::new(FdCache::default()),
      read_buf: RefCell::new(BufPool::default()),
      write_buf: RefCell::new(BufPool::default()),
      write_gate: WriteGate::new(DEFAULT_MAX_INFLIGHT_WRITE as usize),
      #[cfg(target_os = "linux")]
      fixed_write: RefCell::new(FixedWritePool::default()),
      dirty: RefCell::new(HashMap::default()),
      dirty_dir: RefCell::new(HashSet::default()),
      manifest_dir_synced: RefCell::new(HashSet::default()),
      manifest_offset: RefCell::new(HashMap::default()),
      short_write_bytes: Cell::new(None),
      read_try_again_times: Cell::new(0),
      segment_preallocated: RefCell::new(HashSet::default()),
      data_scan_num: AtomicU64::new(0),
      segment_scan_num: AtomicU64::new(0),
      fd_open_num: AtomicU64::new(0),
      write_at_num: AtomicU64::new(0),
      read_at_num: AtomicU64::new(0),
      stale_read_num: AtomicU64::new(0),
      data_sync_num: AtomicU64::new(0),
      dir_fsync_num: AtomicU64::new(0),
      #[cfg(target_os = "linux")]
      fixed_io: Arc::new(Mutex::new(None)),
    }
  }

  pub fn with_segment_config(mut self, segment_size: usize, segments_per_file_shift: u8) -> Self {
    self.segment_size = segment_size;
    self.segments_per_file_shift = segments_per_file_shift;
    self
  }

  /// 单组合并写页数上限（对齐 C++ `max_write_batch_pages`）；0 按 1 处理。
  pub fn with_max_write_batch_pages(mut self, pages: usize) -> Self {
    self.max_write_batch_pages = pages.max(1);
    self
  }

  /// IO 层在飞写组上限（对齐 C++ `max_inflight_write`）；0 按 1 处理。
  ///
  /// 同一个值既作跨调用的写闸门容量，也作单次 `write_page_li` 的提交窗口，
  /// 因此全局只有这一个在飞写上限；`max_write_batch_pages` 只决定每组页数，
  /// 不改变总额。
  pub fn with_max_inflight_write(mut self, limit: usize) -> Self {
    self.max_inflight_write = limit.max(1);
    self.write_gate.set_limit(self.max_inflight_write);
    self
  }

  /// 当前生效的在飞写组上限（诊断 / 测试观察用）。
  pub fn max_inflight_write(&self) -> usize {
    self.max_inflight_write
  }

  /// data / segment 使用 O_DIRECT（仅 Linux；page_size 必须是 4 KiB 整数倍）。
  /// manifest 仍走 buffered：其 record 长度由上层 framing 决定，IO 层不做补零。
  pub fn with_direct_io(self, enabled: bool) -> Self {
    let aligned = self.page_size.is_multiple_of(PAGE_ALIGN);
    self.direct_io.set(enabled && aligned);
    self
  }

  /// segment 读走内核 provided-buffer（`ProactorBuilder::buffer_pool_buffer_len`
  /// 必须不小于 `segment_size`，由 shard 装配 runtime 时配置）。
  /// 须在 `with_segment_config` 之后调用，开关按最终 `segment_size` 判定。
  ///
  /// 池未配置、耗尽或短读时自动回落自管读池，开关只影响性能不影响正确性。
  pub fn with_managed_segment_read(self) -> Self {
    self
      .managed_seg
      .set(managed_segment_read_supported(self.segment_size));
    self
  }

  /// 读 page 后是否校验 checksum（默认开启）。
  /// 只写原始字节的测试 / 迁移工具可关闭。
  pub fn with_checksum_verify(self, enabled: bool) -> Self {
    self.skip_verify_checksum.set(!enabled);
    self
  }

  /// 运行期切换 checksum 校验（对齐 `KvOption::skip_verify_checksum`）。
  pub fn set_skip_verify_checksum(&self, skip: bool) {
    self.skip_verify_checksum.set(skip);
  }

  /// 限制 FD 缓存条数（对齐 C++ `fd_limit`）；0 表示不限制。
  pub fn with_fd_cache_limit(self, limit: usize) -> Self {
    self.fd_cache.borrow_mut().limit = if limit == 0 { usize::MAX } else { limit };
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

  /// 累计 open 次数：FD 缓存命中不计数，用于验证 FD 复用。
  pub fn fd_open_num(&self) -> u64 {
    self.fd_open_num.load(Ordering::Relaxed)
  }

  /// 实际 `write_at` 次数（测试用；批量写合并后应小于页数）。
  pub fn write_at_num(&self) -> u64 {
    self.write_at_num.load(Ordering::Relaxed)
  }

  /// 页读系统调用尝试次数（含同路径重试与测试注入）。
  pub fn read_at_num(&self) -> u64 {
    self.read_at_num.load(Ordering::Relaxed)
  }

  /// 旧 term 候选回退命中次数：更新的候选 EOF / checksum 失败后，从更旧 term
  /// 读到完整页的次数。非零只说明发生过回退读，不代表读到的一定陈旧。
  pub fn stale_read_num(&self) -> u64 {
    self.stale_read_num.load(Ordering::Relaxed)
  }

  /// 测试用：让下一次 manifest 追加只写前 `len` 字节，验证短写回滚。
  ///
  /// 一次性生效；`len == 0` 表示下一次写入直接返回 0 字节。
  pub fn inject_short_manifest_write(&self, len: usize) {
    self.short_write_bytes.set(Some(len));
  }

  /// 测试用：接下来 `times` 次页读返回瞬时 `TryAgain`（模拟 EAGAIN）。
  ///
  /// 用于验证瞬时错误只在同一路径重试、耗尽后上抛，绝不切换旧 term 候选。
  pub fn inject_read_try_again(&self, times: usize) {
    self.read_try_again_times.set(times);
  }

  /// 当前 FD 缓存中的句柄数。
  pub fn fd_cache_len(&self) -> usize {
    self.fd_cache.borrow().len()
  }

  /// FD 缓存条数上限；`usize::MAX` 表示不限制（`with_fd_cache_limit(0)`）。
  ///
  /// 对应 C++ `IouringMgr::GetOpenFileLimit`；`fd_cache_len` 对应
  /// `GetOpenFileCount`。两者是 `open_file_count/limit` gauge 的数据源，
  /// 上报通道尚未接到 `fkv_metrics`，见 `doc/metrics.md`。
  pub fn fd_cache_limit(&self) -> usize {
    self.fd_cache.borrow().limit
  }

  /// 读 / 写缓冲池累计分配次数。
  pub fn buf_alloc_num(&self) -> u64 {
    self.read_buf.borrow().alloc_num + self.write_buf.borrow().alloc_num
  }

  /// 注册写槽位累计注册次数（非 Linux 恒为 0）。
  pub fn fixed_write_alloc_num(&self) -> u64 {
    #[cfg(target_os = "linux")]
    {
      self.fixed_write.borrow().alloc_num
    }
    #[cfg(not(target_os = "linux"))]
    {
      0
    }
  }

  /// 本表尚未 fdatasync 的 data / segment 文件数。
  pub fn dirty_file_num(&self, tbl: &TableIdent) -> usize {
    self.dirty.borrow().get(tbl).map_or(0, HashSet::len)
  }

  /// `sync_data` 累计调用次数（测试用；只计入口命中，不计脏文件数）。
  pub fn data_sync_num(&self) -> u64 {
    self.data_sync_num.load(Ordering::Relaxed)
  }

  /// 成功的目录项 fsync 次数（测试用，对应 C++ `mkdirat` 后的目录 fdatasync）。
  pub fn dir_fsync_num(&self) -> u64 {
    self.dir_fsync_num.load(Ordering::Relaxed)
  }

  fn part_dir(&self, tbl: &TableIdent) -> PathBuf {
    self
      .root
      .join(format!("{}.{}", tbl.tbl_name, tbl.partition_id))
  }

  /// 同步扫描并重建索引（attach / reopen 阶段使用）。
  /// `force=false` 时仅在目录签名变化时扫描；返回是否真的扫描了。
  fn refresh_dir_index(&self, tbl: &TableIdent, force: bool) -> bool {
    let dir = self.part_dir(tbl);
    if !force {
      let stamp = dir_stamp_of(&dir);
      let index_map = self.dir_index_map.borrow();
      if let Some(index) = index_map.get(tbl)
        && index.scanned
        && index.stamp == stamp
      {
        return false;
      }
    }
    let (branch, term) = {
      let identity = self.identity.borrow();
      (identity.branch.clone(), identity.term)
    };
    let scan = scan_dir(&dir, &branch, term);
    self.apply_scan(tbl, scan)
  }

  /// 异步扫描：`metadata` + `read_dir` 都在阻塞线程池执行，不阻塞 reactor。
  async fn refresh_dir_index_async(&self, tbl: &TableIdent, force: bool) -> bool {
    let dir = self.part_dir(tbl);
    let cached = if force {
      None
    } else {
      self
        .dir_index_map
        .borrow()
        .get(tbl)
        .filter(|index| index.scanned)
        .map(|index| index.stamp.clone())
    };
    let (branch, term) = {
      let identity = self.identity.borrow();
      (identity.branch.clone(), identity.term)
    };
    // 闭包只捕获 PathBuf / identity 与目录签名，不把 &self / RefCell 送进阻塞线程。
    let scan = spawn_dir_io(move || {
      // 先做一次 stat，签名未变则直接跳过 getdents。
      if let Some(stamp) = &cached
        && dir_stamp_of(&dir) == *stamp
      {
        return None;
      }
      let scan = scan_dir(&dir, &branch, term);
      match &cached {
        Some(stamp) if *stamp == scan.stamp => None,
        _ => Some(scan),
      }
    })
    .await
    .unwrap_or(None);
    match scan {
      Some(scan) => self.apply_scan(tbl, scan),
      None => false,
    }
  }

  /// 用扫描结果替换索引；扫描计数在此累计。
  fn apply_scan(&self, tbl: &TableIdent, scan: DirScan) -> bool {
    self.data_scan_num.fetch_add(1, Ordering::Relaxed);
    self.segment_scan_num.fetch_add(1, Ordering::Relaxed);
    self.dir_index_map.borrow_mut().insert(
      tbl.clone(),
      DirIndex {
        stamp: scan.stamp,
        scanned: true,
        data: scan.data,
        segment: scan.segment,
        data_other: scan.data_other,
        segment_other: scan.segment_other,
      },
    );
    true
  }

  /// 读路径：优先当前 identity，否则回退到其它 identity 中 term 最大的文件。
  fn index_path(&self, tbl: &TableIdent, file_id: FileId, kind: FileKind) -> Option<PathBuf> {
    self.dir_index_map.borrow().get(tbl).and_then(|index| {
      index.primary(kind).get(&file_id).cloned().or_else(|| {
        index
          .other(kind)
          .get(&file_id)
          .and_then(|li| li.iter().max_by_key(|variant| variant.term))
          .map(|variant| variant.path.clone())
      })
    })
  }

  async fn cached_data_path(&self, tbl: &TableIdent, file_page_id: FilePageId) -> Option<PathBuf> {
    let file_id = file_page_id >> self.pages_per_file_shift;
    if let Some(path) = self.index_path(tbl, file_id, FileKind::Data) {
      return Some(path);
    }
    self.refresh_dir_index_async(tbl, false).await;
    self.index_path(tbl, file_id, FileKind::Data)
  }

  /// 同一 file id 的全部候选路径，当前 identity 优先，其余按 term 降序。
  /// 新 term 会在同 file id 下另建文件，旧 term 页必须能回退读取。
  fn collect_data_path_li(&self, tbl: &TableIdent, file_page_id: FilePageId) -> Vec<PathBuf> {
    let file_id = file_page_id >> self.pages_per_file_shift;
    let mut path_li = Vec::new();
    if let Some(index) = self.dir_index_map.borrow().get(tbl) {
      if let Some(path) = index.data.get(&file_id) {
        path_li.push(path.clone());
      }
      if let Some(other_li) = index.data_other.get(&file_id) {
        let mut sorted = other_li.clone();
        sorted.sort_by_key(|variant| Reverse(variant.term));
        path_li.extend(sorted.into_iter().map(|variant| variant.path));
      }
    }
    path_li.dedup();
    path_li
  }

  async fn data_path_li(&self, tbl: &TableIdent, file_page_id: FilePageId) -> Vec<PathBuf> {
    let mut path_li = self.collect_data_path_li(tbl, file_page_id);
    if path_li.is_empty() {
      // 索引未扫描 / 文件被外部新增或删除：重扫一次再定位。
      self.refresh_dir_index_async(tbl, false).await;
      path_li = self.collect_data_path_li(tbl, file_page_id);
    }
    path_li
  }

  /// 由当前 `(branch, term)` 确定性生成路径。
  fn current_path(&self, tbl: &TableIdent, file_id: FileId, kind: FileKind) -> PathBuf {
    let identity = self.identity.borrow();
    let name = match kind {
      FileKind::Data => data_file_name(file_id, &identity.branch, identity.term),
      FileKind::Segment => segment_file_name(file_id, &identity.branch, identity.term),
    };
    self.part_dir(tbl).join(name)
  }

  /// 写路径：只按当前 `(branch, term)` 生成，绝不回退旧 identity 文件
  /// （否则新 term 会覆写旧 term 文件），也不触发目录扫描。
  fn write_path(&self, tbl: &TableIdent, file_id: FileId, kind: FileKind) -> PathBuf {
    let path = self.current_path(tbl, file_id, kind);
    let mut index_map = self.dir_index_map.borrow_mut();
    let index = index_map.entry(tbl.clone()).or_default();
    let previous = index.primary_mut(kind).insert(file_id, path.clone());
    drop(index_map);
    if previous.is_none() {
      // 新文件名首次出现：目录项需要在下一次 durability barrier 落盘。
      self.dirty_dir.borrow_mut().insert(tbl.clone());
    }
    path
  }

  fn data_path(&self, tbl: &TableIdent, file_page_id: FilePageId) -> PathBuf {
    self.write_path(
      tbl,
      file_page_id >> self.pages_per_file_shift,
      FileKind::Data,
    )
  }

  async fn cached_segment_path(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
  ) -> Option<PathBuf> {
    let file_id = segment_fp_id >> self.segments_per_file_shift;
    if let Some(path) = self.index_path(tbl, file_id, FileKind::Segment) {
      return Some(path);
    }
    self.refresh_dir_index_async(tbl, false).await;
    self.index_path(tbl, file_id, FileKind::Segment)
  }

  fn segment_path(&self, tbl: &TableIdent, segment_fp_id: FilePageId) -> PathBuf {
    self.write_path(
      tbl,
      segment_fp_id >> self.segments_per_file_shift,
      FileKind::Segment,
    )
  }

  /// 某 file id 在目录中出现的全部路径：当前 identity 与所有其它 identity 的变体。
  /// 末尾补当前 identity 的推导路径，覆盖索引尚未扫描到的情况。
  fn variant_path_li(&self, tbl: &TableIdent, file_id: FileId, kind: FileKind) -> Vec<PathBuf> {
    let mut path_li = Vec::new();
    if let Some(index) = self.dir_index_map.borrow().get(tbl) {
      if let Some(path) = index.primary(kind).get(&file_id) {
        path_li.push(path.clone());
      }
      if let Some(li) = index.other(kind).get(&file_id) {
        path_li.extend(li.iter().map(|variant| variant.path.clone()));
      }
    }
    path_li.push(self.current_path(tbl, file_id, kind));
    path_li.sort_unstable();
    path_li.dedup();
    path_li
  }

  /// 某 `(file_id, branch, term)` 变体的候选路径（rev-39 精确删除）。
  ///
  /// identity 命中时取索引路径并补推导路径（索引可能未扫描）；非 identity
  /// 变体只取 other 表中匹配 `(branch, term)` 的条目。
  fn variant_paths_of(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    kind: FileKind,
    branch: &str,
    term: u64,
  ) -> Vec<PathBuf> {
    let identity_match = {
      let identity = self.identity.borrow();
      identity.branch == branch && identity.term == term
    };
    let mut path_li = Vec::new();
    if let Some(index) = self.dir_index_map.borrow().get(tbl) {
      if identity_match && let Some(path) = index.primary(kind).get(&file_id) {
        path_li.push(path.clone());
      }
      if !identity_match && let Some(li) = index.other(kind).get(&file_id) {
        path_li.extend(
          li.iter()
            .filter(|variant| variant.ident() == (branch, term))
            .map(|variant| variant.path.clone()),
        );
      }
    }
    if identity_match {
      path_li.push(self.current_path(tbl, file_id, kind));
    }
    path_li.sort_unstable();
    path_li.dedup();
    path_li
  }

  /// 删除前确保目录索引已建立：GC / drop_table 会先 `list_*`，
  /// 直接调用时索引可能为空，不扫描会漏删同 id 的其它 identity 变体。
  async fn ensure_dir_index(&self, tbl: &TableIdent) {
    let scanned = self
      .dir_index_map
      .borrow()
      .get(tbl)
      .is_some_and(|index| index.scanned);
    if !scanned {
      self.refresh_dir_index_async(tbl, false).await;
    }
  }

  /// 丢弃一条路径缓存并强制下次 lookup 重扫目录（文件被外部替换时使用）。
  fn forget_path(&self, tbl: &TableIdent, file_id: FileId, kind: FileKind) {
    if let Some(index) = self.dir_index_map.borrow_mut().get_mut(tbl) {
      index.primary_mut(kind).remove(&file_id);
      index.other_mut(kind).remove(&file_id);
      index.stamp = None;
    }
  }

  /// 变体删除后清对应索引条目：identity 命中清 primary，否则只清 other 表里
  /// 匹配 `(branch, term)` 的条目（同 id 其它变体的读路由不受影响）。
  fn forget_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    kind: FileKind,
    branch: &str,
    term: u64,
  ) {
    let mut index_map = self.dir_index_map.borrow_mut();
    let Some(index) = index_map.get_mut(tbl) else {
      return;
    };
    let identity_match = {
      let identity = self.identity.borrow();
      identity.branch == branch && identity.term == term
    };
    if identity_match {
      index.primary_mut(kind).remove(&file_id);
    }
    if let Some(li) = index.other_mut(kind).get_mut(&file_id) {
      li.retain(|variant| variant.ident() != (branch, term));
      if li.is_empty() {
        index.other_mut(kind).remove(&file_id);
      }
    }
    index.stamp = None;
  }

  /// 打开只读句柄；命中缓存时零系统调用返回（对齐 C++ `OpenOrCreateFD`）。
  ///
  /// 缓存失效依赖显式路径：`remove_file`/`remove_segment_file`/`switch_manifest`/
  /// `write_manifest_file`/`evict_table`/`invalidate_path_cache` 都会 `forget_fd`。
  /// 不再在命中路径做 `stat` 校验 inode——那会把每次页读变成 `statx`+`pread`
  /// 两次串行系统调用（io-01）。
  async fn open_read_file(&self, path: &Path) -> io::Result<File> {
    if let Some(file) = self.fd_cache.borrow_mut().get_read(path) {
      return Ok(file);
    }
    let mut opts = OpenOptions::new();
    opts.read(true);
    apply_direct_flag(&mut opts, self.is_direct_path(path));
    let file = opts.open(path).await?;
    self.fd_open_num.fetch_add(1, Ordering::Relaxed);
    self
      .fd_cache
      .borrow_mut()
      .insert(path.to_path_buf(), Access::Read, file.clone());
    Ok(file)
  }

  /// 打开写句柄（带 create、不 truncate）；命中缓存直接复用。
  async fn open_write_file(&self, path: &Path) -> io::Result<File> {
    if let Some(file) = self.fd_cache.borrow_mut().get_write(path) {
      return Ok(file);
    }
    let mut opts = OpenOptions::new();
    opts.read(true).write(true).create(true).truncate(false);
    apply_direct_flag(&mut opts, self.is_direct_path(path));
    let file = opts.open(path).await?;
    self.fd_open_num.fetch_add(1, Ordering::Relaxed);
    self
      .fd_cache
      .borrow_mut()
      .insert(path.to_path_buf(), Access::Write, file.clone());
    Ok(file)
  }

  /// 失效一条路径的缓存句柄（删除 / 替换 / rename 后调用）。
  fn forget_fd(&self, path: &Path) {
    self.fd_cache.borrow_mut().remove(path);
    // 句柄失效后偏移也不再可信（文件可能被替换 / 删除）。
    self.manifest_offset.borrow_mut().remove(path);
    self.segment_preallocated.borrow_mut().remove(path);
  }

  /// 首次使用某个 segment 文件时预分配一次，后续段写入不再 `ftruncate`。
  async fn preallocate_segment_file(&self, path: &Path, file: &File) -> Result<()> {
    if self.segment_preallocated.borrow().contains(path) {
      return Ok(());
    }
    file
      .set_len(self.sequence_file_size()?)
      .await
      .map_err(map_io_err)?;
    self
      .segment_preallocated
      .borrow_mut()
      .insert(path.to_path_buf());
    Ok(())
  }

  /// sync 用句柄：优先复用缓存句柄；不 create，文件已删除时返回 `NotFound`。
  async fn open_for_sync(&self, path: &Path) -> io::Result<File> {
    if let Some(file) = self.fd_cache.borrow_mut().get_write(path) {
      return Ok(file);
    }
    let mut opts = OpenOptions::new();
    opts.read(true).write(true).truncate(false);
    apply_direct_flag(&mut opts, self.is_direct_path(path));
    let file = opts.open(path).await?;
    self.fd_open_num.fetch_add(1, Ordering::Relaxed);
    Ok(file)
  }

  /// 取读缓冲：只保证容量为 `len`，不预清零（compio 按容量读入）。
  fn take_read_buf(&self, len: usize) -> Vec<u8> {
    self.read_buf.borrow_mut().take_read(len)
  }

  /// 归还读缓冲：长度归零、内容不清零。
  fn put_read_buf(&self, len: usize, buf: Vec<u8>) {
    self.read_buf.borrow_mut().put_read(len, buf);
  }

  fn take_direct_buf(&self, len: usize) -> AlignedBuf {
    self
      .direct_pool
      .borrow_mut()
      .entry(len)
      .or_insert_with(|| PagesPool::new(len))
      .allocate()
  }

  fn put_direct_buf(&self, buf: AlignedBuf) {
    let len = buf.len();
    let pool_map = self.direct_pool.borrow();
    let Some(pool) = pool_map.get(&len) else {
      return;
    };
    let used: usize = pool_map
      .values()
      .map(|cached| cached.cached() * cached.page_size())
      .sum();
    if used + len <= DIRECT_POOL_BUDGET {
      pool.recycle(buf);
    }
  }

  /// 发起一次页读：按 `direct_io` 选普通池化缓冲或 O_DIRECT 对齐缓冲。
  async fn read_page_chunk(
    &self,
    file: &File,
    len: usize,
    offset: u64,
  ) -> (io::Result<usize>, ReadChunk) {
    let direct = self.direct_io.get();
    if self.take_injected_read_err() {
      // 注入路径不碰文件，但仍取同类型缓冲，保证归还池路径与真实读一致。
      let chunk = if direct {
        ReadChunk::Direct(DirectBuf::from_buf(self.take_direct_buf(len), 0))
      } else {
        ReadChunk::Plain(self.take_read_buf(len))
      };
      return (Err(io::Error::from(ErrorKind::WouldBlock)), chunk);
    }
    if direct {
      let BufResult(result, buf) = file
        .read_at(DirectBuf::from_buf(self.take_direct_buf(len), 0), offset)
        .await;
      (result, ReadChunk::Direct(buf))
    } else {
      let BufResult(result, buf) = file.read_at(self.take_read_buf(len), offset).await;
      (result, ReadChunk::Plain(buf))
    }
  }

  /// 取出一次注入的瞬时读错误；未配置返回 false。
  fn take_injected_read_err(&self) -> bool {
    let left = self.read_try_again_times.get();
    if left == 0 {
      return false;
    }
    self.read_try_again_times.set(left - 1);
    true
  }

  /// 归还页读缓冲。
  fn put_read_chunk(&self, len: usize, chunk: ReadChunk) {
    match chunk {
      ReadChunk::Plain(buf) => self.put_read_buf(len, buf),
      ReadChunk::Direct(buf) => self.put_direct_buf(buf.into_inner()),
    }
  }

  /// 读单个候选文件。
  ///
  /// 瞬时错误（`TryAgain` / `Busy` / `OpenFileLimit`）与短读只在同一路径内
  /// 重试；短读重试耗尽按 `Corrupted` 终局上报，不换旧 term 候选。EOF 与
  /// checksum 失败返回 `Eof` / `Miss`，由调用方决定是否回退。
  async fn read_page_candidate(
    &self,
    path: &Path,
    page_size: usize,
    offset: u64,
    dst: &mut [u8],
  ) -> Result<CandidateRead> {
    let file = match self.open_read_file(path).await {
      Ok(file) => file,
      // 索引里的路径被外部删除：该候选不存在，允许尝试其它候选。
      Err(err) if err.kind() == ErrorKind::NotFound => return Ok(CandidateRead::Miss),
      Err(err) => return Err(map_io_err(err)),
    };
    for attempt in 0..READ_RETRY_TIMES {
      self.read_at_num.fetch_add(1, Ordering::Relaxed);
      let (result, chunk) = self.read_page_chunk(&file, page_size, offset).await;
      let read_len = match result {
        Ok(read_len) => read_len,
        Err(err) => {
          self.put_read_chunk(page_size, chunk);
          // `is_retryable()` 覆盖 TryAgain / Busy / OpenFileLimit，均为瞬时错误。
          let err = map_io_err(err);
          if err.is_retryable() && attempt + 1 < READ_RETRY_TIMES {
            // 只在同一路径重试，耗尽后上抛，绝不切换旧 term 候选。
            continue;
          }
          return Err(err);
        }
      };
      // `res == 0` 是确定性 EOF：本候选不重试，交给上一层尝试更旧的 term
      // 候选，否则新 term 的稀疏文件会遮蔽旧 term 已落盘的尾部页。
      if read_len == 0 {
        self.put_read_chunk(page_size, chunk);
        return Ok(CandidateRead::Eof);
      }
      if read_len != page_size {
        // 短读不是 EOF：torn 写 / 截断属于当前文件自身损坏。同路径有限
        // 重试，耗尽即终局 Corrupted，不换旧 term 候选读回陈旧页。
        self.put_read_chunk(page_size, chunk);
        continue;
      }
      dst[..page_size].copy_from_slice(chunk.as_slice());
      self.put_read_chunk(page_size, chunk);
      if self.skip_verify_checksum.get() || validate_checksum(&dst[..page_size]).unwrap_or(false) {
        return Ok(CandidateRead::Hit);
      }
      // 整页可读但 checksum 不通过：稀疏洞（全零页）或损坏，试更旧 term。
      return Ok(CandidateRead::Miss);
    }
    // 只有短读会耗尽循环（瞬时错误在最后一次尝试时已上抛）。
    Err(KvError::Corrupted)
  }

  fn take_write_buf(&self, len: usize) -> Vec<u8> {
    self.write_buf.borrow_mut().take(len)
  }

  fn put_write_buf(&self, len: usize, buf: Vec<u8>) {
    self.write_buf.borrow_mut().put(len, buf);
  }

  /// 借一个长度 `len` 的注册写槽位：优先复用空闲槽位，池未满时新注册。
  /// 注册失败返回 `None`，调用方回落普通写路径。
  #[cfg(target_os = "linux")]
  fn take_fixed_slot(&self, len: usize) -> Option<FixedBuffer> {
    // 槽位缓存上限跟随在飞上限收紧，但不超过注册缓冲的常驻上限。
    let slot_limit = self.max_inflight_write.min(FIXED_WRITE_SLOT_LIMIT);
    {
      let mut pool = self.fixed_write.borrow_mut();
      if let Some(buf) = pool.free.get_mut(&len).and_then(Vec::pop) {
        return Some(buf);
      }
      // 批次页数配置变化后旧长度槽位不再复用，先释放再为新长度让位。
      if pool.total >= slot_limit {
        let other_li: Vec<usize> = pool
          .free
          .keys()
          .copied()
          .filter(|other| *other != len)
          .collect();
        for other in other_li {
          if let Some(buf_li) = pool.free.remove(&other) {
            pool.total -= buf_li.len();
          }
        }
      }
      if pool.total >= slot_limit {
        return None;
      }
      pool.total += 1;
    }
    // 注册要拿 `fixed_io` 锁，必须放在池借用之外。
    match self.register_fixed_buffer(len) {
      Ok(buf) => {
        self.fixed_write.borrow_mut().alloc_num += 1;
        Some(buf)
      }
      Err(_) => {
        self.fixed_write.borrow_mut().total -= 1;
        None
      }
    }
  }

  #[cfg(target_os = "linux")]
  fn put_fixed_slot(&self, buf: FixedBuffer) {
    let len = buf.len();
    self
      .fixed_write
      .borrow_mut()
      .free
      .entry(len)
      .or_default()
      .push(buf);
  }

  /// 丢弃一个注册槽位：`Drop` 把 index 还给后端，池配额同步扣减。
  ///
  /// 与 [`Self::put_fixed_slot`] 是两条归还路径，都必须让 `total` 回到
  /// `take_fixed_slot` 借出前的值，否则配额永久泄漏、后续注册被拒。
  #[cfg(target_os = "linux")]
  fn discard_fixed_slot(&self, buf: FixedBuffer) {
    drop(buf);
    self.fixed_write.borrow_mut().total -= 1;
  }

  /// 注册槽位是否属于当前线程 / runtime 的 ring。跨线程或 runtime 重建后
  /// fixed IO 必须回落普通路径。
  #[cfg(target_os = "linux")]
  fn owns_fixed_ring(&self) -> bool {
    self.fixed_io.lock().ok().is_some_and(|slot| {
      slot
        .as_ref()
        .is_some_and(fixed::FixedBufferRegistry::owns_current_ring)
    })
  }

  /// runtime 重建后丢弃旧 ring 的槽位池，下一次 `take_fixed_slot` 重新注册。
  #[cfg(target_os = "linux")]
  fn reset_fixed_ring(&self) {
    if let Ok(mut slot) = self.fixed_io.lock() {
      *slot = None;
    }
    let mut pool = self.fixed_write.borrow_mut();
    pool.free.clear();
    pool.total = 0;
  }

  /// 为一次在飞 fixed op 登记槽位租约（io-37）。
  ///
  /// 租约随 op 生命周期存活：compio 取消不可靠，外层 future 被 drop 后
  /// SQE 仍在 ring 中；只有 driver 收割 CQE 并释放 op 后，槽位才可复用。
  #[cfg(target_os = "linux")]
  fn lease_fixed_slot(&self, index: u16) -> Result<fixed::SlotLease> {
    let mut slot = self.fixed_io.lock().map_err(|_| KvError::IoFail)?;
    let buffers = slot.as_mut().ok_or(KvError::IoFail)?;
    if !buffers.owns_current_ring() {
      return Err(KvError::IoFail);
    }
    Ok(buffers.lease(Arc::downgrade(&self.fixed_io), index))
  }

  /// 用注册槽位提交一次 `WRITE_FIXED`。
  #[cfg(target_os = "linux")]
  async fn submit_fixed_write(
    &self,
    file: &File,
    buf: &FixedBuffer,
    len: usize,
    offset: u64,
  ) -> Result<usize> {
    let lease = buf.share().ok_or(KvError::Busy)?;
    let slot = self.lease_fixed_slot(buf.index())?;
    let op = fixed::FixedWrite::new(file.to_shared_fd(), lease, 0, len, offset, buf.index())?
      .with_slot(slot);
    let BufResult(result, _) = compio::runtime::submit(op).await;
    result.map_err(map_io_err)
  }

  /// 提交一次写并归还缓冲：direct 缓冲回对齐池，普通缓冲回写池。
  async fn write_chunk(
    &self,
    file: &mut File,
    chunk: WriteChunk,
    offset: u64,
  ) -> io::Result<usize> {
    match chunk {
      WriteChunk::Plain(buf) => {
        let len = buf.len();
        let BufResult(result, buf) = file.write_at(buf, offset).await;
        self.put_write_buf(len, buf);
        result
      }
      WriteChunk::Direct(buf) => {
        let BufResult(result, buf) = file.write_at(buf, offset).await;
        self.put_direct_buf(buf.into_inner());
        result
      }
    }
  }

  /// 提交一组连续页写。
  ///
  /// Linux 优先借注册写槽位走 `WRITE_FIXED`（槽位复用，无每次分配）；
  /// 注册不可用或运行时已重建时回落到本地池化 / O_DIRECT 对齐缓冲。
  async fn write_page_group(
    &self,
    file: &mut File,
    page_li: &[&[u8]],
    bytes: usize,
    offset: u64,
  ) -> Result<usize> {
    #[cfg(target_os = "linux")]
    {
      let slot_len = self.max_write_batch_pages * self.page_size;
      if let Some(mut buf) = self.take_fixed_slot(slot_len) {
        if self.owns_fixed_ring() {
          match buf.as_mut_slice() {
            Some(dst) => {
              for (index, page) in page_li.iter().enumerate() {
                let start = index * self.page_size;
                dst[start..start + page.len()].copy_from_slice(page);
              }
              let result = self.submit_fixed_write(file, &buf, bytes, offset).await;
              self.put_fixed_slot(buf);
              return result;
            }
            // 槽位缓冲仍有取消未确认的在飞 op：丢弃该槽位（Drop 归还 index，
            // 同时释放池配额），本次回落普通写路径。
            None => self.discard_fixed_slot(buf),
          }
        } else {
          // 运行时已重建：丢弃旧 ring 的槽位池，下一次重新注册。
          drop(buf);
          self.reset_fixed_ring();
        }
      }
    }
    let result = if self.direct_io.get() {
      let mut buf = DirectBuf::from_buf(self.take_direct_buf(bytes), bytes);
      for (index, page) in page_li.iter().enumerate() {
        let start = index * self.page_size;
        buf.as_mut_slice()[start..start + page.len()].copy_from_slice(page);
      }
      self
        .write_chunk(file, WriteChunk::Direct(buf), offset)
        .await
    } else {
      let mut buf = self.take_write_buf(bytes);
      for (index, page) in page_li.iter().enumerate() {
        let start = index * self.page_size;
        buf[start..start + page.len()].copy_from_slice(page);
      }
      self.write_chunk(file, WriteChunk::Plain(buf), offset).await
    };
    result.map_err(map_io_err)
  }

  /// 记录本表未落盘的 data / segment 文件。
  fn mark_dirty(&self, tbl: &TableIdent, path: PathBuf) {
    let mut dirty = self.dirty.borrow_mut();
    if let Some(set) = dirty.get_mut(tbl) {
      set.insert(path);
      return;
    }
    dirty.entry(tbl.clone()).or_default().insert(path);
  }

  /// 文件删除后从 dirty 集合剔除。
  fn forget_dirty(&self, path: &Path) {
    for set in self.dirty.borrow_mut().values_mut() {
      set.remove(path);
    }
  }

  /// 读取整个文件。短读视为 `IoFail`。
  ///
  /// `direct` 为目标文件是否 O_DIRECT：此时按页对齐长度读入对齐缓冲，
  /// 再把有效前缀拷入返回的 `Vec`（整文件读不是热路径，允许一次拷贝）。
  async fn read_whole(&self, file: &File, direct: bool) -> Result<Vec<u8>> {
    let len = usize::try_from(file.metadata().await.map_err(map_io_err)?.len())
      .map_err(|_| KvError::InvalidArgs)?;
    if len == 0 {
      return Ok(Vec::new());
    }
    if direct {
      // 文件尾可能不足一页：请求长度向上取整，内核按 EOF 返回短读。
      let aligned = len.next_multiple_of(PAGE_ALIGN);
      let BufResult(result, buf) = file
        .read_at(DirectBuf::from_buf(self.take_direct_buf(aligned), 0), 0)
        .await;
      let read_len = match result {
        Ok(read_len) => read_len,
        Err(err) => {
          self.put_direct_buf(buf.into_inner());
          return Err(map_io_err(err));
        }
      };
      if read_len != len {
        self.put_direct_buf(buf.into_inner());
        return Err(KvError::IoFail);
      }
      let mut out = vec![0u8; len];
      out.copy_from_slice(&buf.as_init()[..len]);
      self.put_direct_buf(buf.into_inner());
      return Ok(out);
    }
    // 走读缓冲池：容量直读，不预先清零。
    let BufResult(result, buf) = file.read_at(self.take_read_buf(len), 0).await;
    let read_len = match result {
      Ok(read_len) => read_len,
      Err(err) => {
        self.put_read_buf(len, buf);
        return Err(map_io_err(err));
      }
    };
    if read_len != len {
      self.put_read_buf(len, buf);
      return Err(KvError::IoFail);
    }
    Ok(buf)
  }

  /// 写整个文件并校验写入长度。
  async fn write_whole(&self, file: &mut File, bytes: &[u8], direct: bool) -> Result<()> {
    if direct && !bytes.len().is_multiple_of(PAGE_ALIGN) {
      return Err(KvError::InvalidArgs);
    }
    let chunk = if direct {
      WriteChunk::Direct(DirectBuf::from_slice(
        self.take_direct_buf(bytes.len()),
        bytes,
      ))
    } else {
      let mut buf = self.take_write_buf(bytes.len());
      debug_assert_eq!(buf.len(), bytes.len());
      buf.copy_from_slice(bytes);
      WriteChunk::Plain(buf)
    };
    let write_len = self.write_chunk(file, chunk, 0).await.map_err(map_io_err)?;
    if write_len != bytes.len() {
      return Err(KvError::IoFail);
    }
    Ok(())
  }

  /// manifest 族文件在既有偏移后追加一条记录并 fdatasync。
  /// 偏移缓存 / FD 缓存 / 写缓冲与活跃 manifest 共用（io-05）。
  async fn append_file_at(&self, dir: &Path, path: &Path, record: &[u8]) -> Result<()> {
    let mut file = self.open_write_file(path).await.map_err(map_io_err)?;
    // 偏移来自缓存；只有首次访问该文件才 fstat（io-05）。
    let cached_pos = self.manifest_offset.borrow().get(path).copied();
    let pos = match cached_pos {
      Some(pos) => pos,
      None => file.metadata().await.map_err(map_io_err)?.len(),
    };
    fkv_failpoint::fail_point!(APPEND_WRITE);
    // 测试注入：只写前 N 字节，走与真实短写完全相同的回滚分支。
    let write_len_limit = self
      .short_write_bytes
      .take()
      .map_or(record.len(), |len| len.min(record.len()));
    let mut buf = self.take_write_buf(write_len_limit);
    debug_assert_eq!(buf.len(), write_len_limit);
    buf.copy_from_slice(&record[..write_len_limit]);
    let BufResult(result, buf) = file.write_at(buf, pos).await;
    let write_len = result.map_err(map_io_err)?;
    self.put_write_buf(write_len_limit, buf);
    if write_len != record.len() {
      // 半写 record 必须截掉再报错：否则重试按 fstat 追加到撕裂前缀之后，
      // 盘面形成 valid/corrupt/valid，reopen 永远判 Corrupted。
      file.set_len(pos).await.map_err(map_io_err)?;
      return Err(KvError::IoFail);
    }
    self
      .manifest_offset
      .borrow_mut()
      .insert(path.to_path_buf(), pos + record.len() as u64);
    fkv_failpoint::kill_point(APPEND_WRITE);
    // 生产环境 no-op；测试用 `AppendManifest:SyncFail` 在真实 sync_data 之后
    // 注入失败，确保走下面的截断 / 偏移回滚分支（`AppendManifest:Sync` 留给
    // 掉电 kill 语义，位于 sync 之前）。
    let mut sync_result = file.sync_data().await;
    if fkv_failpoint::fail_point(APPEND_SYNC_FAIL).is_some() {
      // 注入必须落到下面的回滚分支，而不是 `fail_point!` 的提前 return。
      sync_result = Err(io::Error::other("injected manifest sync failure"));
    }
    if let Err(err) = sync_result {
      // fsync 失败：record 可能已落入页缓存 / 盘面，但事务未提交。调用方会
      // 回滚本批 mapping 并复用 FilePageId，因此必须把文件截回 pos 且把偏移
      // 缓存退回 pos；否则下一次 append 会写在 stale record 之后，reopen 会
      // replay 这条未提交记录，导致已确认数据丢失。
      self
        .manifest_offset
        .borrow_mut()
        .insert(path.to_path_buf(), pos);
      match file.set_len(pos).await {
        Ok(()) => {
          // 截断也要落盘：否则“fsync 失败 + 立即掉电”会让未提交 record 复活。
          if let Err(trunc_sync_err) = file.sync_data().await {
            log::warn!(
              target: "fkv_io_compio",
              "manifest 回滚截断 fsync 失败 path={} pos={} err={}",
              path.display(),
              pos,
              trunc_sync_err
            );
          }
        }
        Err(trunc_err) => log::warn!(
          target: "fkv_io_compio",
          "manifest append 回滚截断失败 path={} pos={} err={}",
          path.display(),
          pos,
          trunc_err
        ),
      }
      return Err(map_io_err(err));
    }
    fkv_failpoint::kill_point(APPEND_SYNC);
    // 首次创建文件后落盘一次父目录项；append 本身不改目录项。文件内容已经
    // fsync 成功，目录项失败只影响掉电后的可见性，不能让调用方回滚内存状态
    // （那样会与盘上已提交的 record 分叉）。
    if let Err(err) = self.sync_manifest_dir_once(dir).await {
      log::warn!(
        target: "fkv_io_compio",
        "manifest 目录 fsync 失败（记录已写入） dir={} err={}",
        dir.display(),
        err
      );
    }
    Ok(())
  }

  /// 原子替换：写 tmp → fdatasync → rename → 失效句柄 → fsync 父目录。
  async fn atomic_write_file(&self, dir: &Path, path: &Path, record: &[u8]) -> Result<()> {
    let tmp = tmp_path(path);
    let mut file = File::create(&tmp).await.map_err(map_io_err)?;
    self.write_whole(&mut file, record, false).await?;
    file.sync_data().await.map_err(map_io_err)?;
    // 每个名字只对应**一个**两阶段窗口（gap-12），这样按名字枚举的 fuzz 驱动
    // 才有区分度：
    // - `AtomicWriteFile:Sync`：tmp 内容已耐久、尚未改名（旧 manifest 仍是权威）；
    // - `AtomicWriteFile:Rename`：已改名、父目录项尚未 fsync（新 snapshot 已可见，
    //   但掉电后可能回退）；
    // - `AtomicWriteFile:SyncDir`：目录项已 fsync，提交完成。
    fkv_failpoint::kill_point(ATOMIC_SYNC);
    fkv_failpoint::fail_point!(ATOMIC_SYNC);
    rename(&tmp, path).await.map_err(map_io_err)?;
    fkv_failpoint::kill_point(ATOMIC_RENAME);
    fkv_failpoint::fail_point!(ATOMIC_RENAME);
    // rename 后旧句柄指向被替换的 inode，必须失效；目录项也要落盘。
    self.forget_fd(path);
    // rename 已生效：新 manifest 已经是命名空间里的权威状态。此时目录 fsync
    // 失败若返回 Err，调用方会回滚内存 root/mapping，但盘上已是新快照，下一次
    // append 会把旧 root 写成新 snapshot 的 log，reopen 后树不可读。这里按
    // 「已提交、目录项 durability 降级」处理：告警但返回成功。
    if let Err(err) = self.sync_dir(dir).await {
      log::warn!(
        target: "fkv_io_compio",
        "manifest rename 后目录 fsync 失败（新快照已生效） dir={} err={}",
        dir.display(),
        err
      );
    }
    fkv_failpoint::kill_point(ATOMIC_SYNC_DIR);
    Ok(())
  }

  async fn open_segment_file(&self, tbl: &TableIdent, segment_fp_id: FilePageId) -> Result<File> {
    let path = self
      .cached_segment_path(tbl, segment_fp_id)
      .await
      .ok_or(KvError::ResourceMissing)?;
    match self.open_read_file(&path).await {
      Ok(file) => Ok(file),
      Err(err) if err.kind() == ErrorKind::NotFound => {
        self.forget_path(
          tbl,
          segment_fp_id >> self.segments_per_file_shift,
          FileKind::Segment,
        );
        let path = self
          .cached_segment_path(tbl, segment_fp_id)
          .await
          .ok_or(KvError::ResourceMissing)?;
        match self.open_read_file(&path).await {
          Ok(file) => Ok(file),
          // 重查后仍不存在：段确实缺失。
          Err(err) if err.kind() == ErrorKind::NotFound => Err(KvError::ResourceMissing),
          // 权限 / fd 耗尽 / EIO 等底层错误按 io 契约映射，便于上层区分
          // 可重试错误与真正的 cache miss。
          Err(err) => Err(map_io_err(err)),
        }
      }
      Err(err) => Err(map_io_err(err)),
    }
  }

  /// 按 file id 打开 data 文件；`NotFound` 时清路径缓存并重查一次目录。
  async fn open_data_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<File>> {
    let file_page_id = file_id << self.pages_per_file_shift;
    let Some(path) = self.cached_data_path(tbl, file_page_id).await else {
      return Ok(None);
    };
    match self.open_read_file(&path).await {
      Ok(file) => Ok(Some(file)),
      Err(err) if err.kind() == ErrorKind::NotFound => {
        self.forget_path(tbl, file_id, FileKind::Data);
        let Some(path) = self.cached_data_path(tbl, file_page_id).await else {
          return Ok(None);
        };
        match self.open_read_file(&path).await {
          Ok(file) => Ok(Some(file)),
          Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
          Err(err) => Err(map_io_err(err)),
        }
      }
      Err(err) => Err(map_io_err(err)),
    }
  }

  /// 列出某类文件在目录中出现的全部 file id（当前与其它 identity）。
  async fn list_file_id_by_kind(&self, tbl: &TableIdent, kind: FileKind) -> Result<Vec<FileId>> {
    self.refresh_dir_index_async(tbl, true).await;
    let mut file_id_li: Vec<FileId> =
      self
        .dir_index_map
        .borrow()
        .get(tbl)
        .map_or_else(Vec::new, |index| {
          // 当前 (branch, term) 与其它 term 的文件都要列出（GC 需要全量）。
          index
            .primary(kind)
            .keys()
            .chain(index.other(kind).keys())
            .copied()
            .collect()
        });
    file_id_li.sort_unstable();
    file_id_li.dedup();
    Ok(file_id_li)
  }

  /// 列出某类文件的全部物理变体（rev-39）：identity 命中的 primary 条目带当前
  /// `(branch, term)`，other 条目带文件名解析出的 `(branch, term)`。
  async fn list_file_variants_by_kind(
    &self,
    tbl: &TableIdent,
    kind: FileKind,
  ) -> Result<Vec<FileVariant>> {
    self.refresh_dir_index_async(tbl, true).await;
    let (identity_branch, identity_term) = {
      let identity = self.identity.borrow();
      (identity.branch.clone(), identity.term)
    };
    let mut variant_li: Vec<FileVariant> =
      self
        .dir_index_map
        .borrow()
        .get(tbl)
        .map_or_else(Vec::new, |index| {
          let primary_li = index.primary(kind).keys().map(|file_id| FileVariant {
            file_id: *file_id,
            identity: Some((identity_branch.as_str().into(), identity_term)),
          });
          let other_li = index.other(kind).iter().flat_map(|(file_id, li)| {
            li.iter().map(|variant| FileVariant {
              file_id: *file_id,
              identity: Some((variant.branch.clone(), variant.term)),
            })
          });
          primary_li.chain(other_li).collect()
        });
    variant_li.sort_unstable();
    variant_li.dedup();
    Ok(variant_li)
  }

  /// 删除某类 file id 的全部 identity 变体，并失效句柄 / 脏标记 / 路径缓存。
  async fn remove_file_by_kind(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    kind: FileKind,
  ) -> Result<()> {
    self.ensure_dir_index(tbl).await;
    let mut result = Ok(());
    for path in self.variant_path_li(tbl, file_id, kind) {
      if let Err(err) = remove_file(&path).await
        && err.kind() != ErrorKind::NotFound
      {
        result = Err(KvError::IoFail);
      }
      self.forget_fd(&path);
      self.forget_dirty(&path);
    }
    self.forget_path(tbl, file_id, kind);
    self.dirty_dir.borrow_mut().insert(tbl.clone());
    result
  }

  /// 只删除一个 `(file_id, branch, term)` 物理变体（rev-39）。
  ///
  /// 同 id 的其它 `(branch, term)` 变体不动；索引只清对应条目，目录脏标记
  /// 照常登记（删除后的目录项需要随 durability barrier 落盘）。
  async fn remove_file_variant_by_kind(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    kind: FileKind,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self.ensure_dir_index(tbl).await;
    let mut result = Ok(());
    for path in self.variant_paths_of(tbl, file_id, kind, branch, term) {
      if let Err(err) = remove_file(&path).await
        && err.kind() != ErrorKind::NotFound
      {
        result = Err(KvError::IoFail);
      }
      self.forget_fd(&path);
      self.forget_dirty(&path);
    }
    self.forget_variant(tbl, file_id, kind, branch, term);
    self.dirty_dir.borrow_mut().insert(tbl.clone());
    result
  }

  /// 预加载单个 table 目录：一次扫描建立 data / segment 的 file id -> path 索引。
  /// 同步扫描，只适合 attach / reopen 阶段；运行期请走异步路径。
  /// 返回被索引的文件数。
  pub fn preload(&self, tbl: &TableIdent) -> usize {
    // attach / reopen 阶段没有并发写：清掉上次崩溃残留的 `*.tmp` 半成品，
    // 避免磁盘长期泄漏（正常写入的 tmp 会紧接 rename 消失）。
    remove_stale_tmp(&self.part_dir(tbl));
    self.refresh_dir_index(tbl, true);
    self
      .dir_index_map
      .borrow()
      .get(tbl)
      .map_or(0, |index| index.data.len() + index.segment.len())
  }

  /// 确保分区目录存在：`create_dir_all` 走阻塞线程池，成功后记住不再重建。
  ///
  /// 新建目录必须 fsync 父目录（对齐 C++ `mkdirat` 后 fdatasync 目录）：否则
  /// 断电后 root 里没有 `demo.1` 这个 entry，目录内的 manifest / data 即使已
  /// fsync 也随目录一起消失。root 本身也是懒创建时，root 的父目录一并同步。
  async fn ensure_dir(&self, tbl: &TableIdent) -> Result<()> {
    let dir = self.part_dir(tbl);
    if self.dir_ready.borrow().contains(&dir) {
      return Ok(());
    }
    let target = dir.clone();
    let root = self.root.clone();
    let created = spawn_dir_io(move || {
      // 目录项同步的范围取决于 root 是否本就存在：root 由本次 create 建立时，
      // 它的 entry 记在更上一层目录里。
      let root_existed = root.is_dir();
      create_dir_all(&target).ok().map(|()| root_existed)
    })
    .await?;
    let Some(root_existed) = created else {
      return Err(KvError::IoFail);
    };
    self.sync_dir_entry(&self.root).await?;
    if !root_existed
      && let Some(parent) = self
        .root
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
      self.sync_dir_entry(parent).await?;
    }
    self.dir_ready.borrow_mut().insert(dir);
    Ok(())
  }

  /// fsync 一个目录 fd。调用方负责决定是否需要记入 `manifest_dir_synced`。
  ///
  /// 目录句柄不进 `fd_cache`：FD 缓存只统计 data / segment / manifest 文件，
  /// 目录句柄一次 fsync 即弃，缓存它只会挤占 `fd_limit` 并污染 `fd_open_num`。
  async fn sync_dir_entry(&self, dir: &Path) -> Result<()> {
    fkv_failpoint::fail_point!(SYNC_DIR);
    // 目录项未落盘的窗口：`sync_data` 的 dirty 目录分支、`ensure_dir`、
    // `append_file_at` 都走这里，因此「文件已耐久、目录项未持久」可被注入（gap-12）。
    fkv_failpoint::kill_point(SYNC_DIR);
    let mut opts = OpenOptions::new();
    opts.read(true);
    let file = opts.open(dir).await.map_err(map_io_err)?;
    file.sync_data().await.map_err(map_io_err)?;
    self.dir_fsync_num.fetch_add(1, Ordering::Relaxed);
    Ok(())
  }

  /// fsync 目录项：打开目录 fd 后 `sync_data`（Linux fdatasync / macOS fsync）。
  ///
  /// 用于 manifest 目录：首次创建 manifest 后落盘一次父目录项。
  async fn sync_dir(&self, dir: &Path) -> Result<()> {
    self.sync_dir_entry(dir).await?;
    self
      .manifest_dir_synced
      .borrow_mut()
      .insert(dir.to_path_buf());
    Ok(())
  }

  /// manifest 目录项持久化：首次创建 manifest 文件后 fsync 一次父目录。
  async fn sync_manifest_dir_once(&self, dir: &Path) -> Result<()> {
    if self.manifest_dir_synced.borrow().contains(dir) {
      return Ok(());
    }
    self.sync_dir(dir).await
  }

  /// 目录被外部同步工具（standby / rsync）改动后主动失效；下一次访问会重新扫描。
  pub fn invalidate_path_cache(&self) {
    self.clear_caches();
  }

  /// 清空路径索引 / 目录就绪标记 / FD 缓存。
  fn clear_caches(&self) {
    self.dir_index_map.borrow_mut().clear();
    self.dir_ready.borrow_mut().clear();
    self.fd_cache.borrow_mut().clear();
  }

  /// 清掉某表在 IO 层的全部缓存状态：FD / 路径索引 / 目录就绪标记 / 脏集合 /
  /// manifest 追加偏移 / segment 预分配标记 / 目录 fsync 标记。
  ///
  /// evict（detach / LRU 驱逐）与空目录删除共用同一份清理集合，避免两处漂移。
  /// 写路径在落 manifest 前已 `sync_data`，这里丢弃的只是未提交页的脏标记。
  fn forget_table_state(&self, tbl: &TableIdent) {
    let dir = self.part_dir(tbl);
    self.fd_cache.borrow_mut().forget_under(&dir);
    self.dir_index_map.borrow_mut().remove(tbl);
    self.dir_ready.borrow_mut().remove(&dir);
    self.dirty.borrow_mut().remove(tbl);
    self.dirty_dir.borrow_mut().remove(tbl);
    // 目录可能被外部工具（standby / 运维）整体重建：路径前缀下的偏移与
    // 预分配标记一并作废，后续 append 重新走 fstat。
    self
      .manifest_offset
      .borrow_mut()
      .retain(|path, _| !path.starts_with(&dir));
    self
      .segment_preallocated
      .borrow_mut()
      .retain(|path| !path.starts_with(&dir));
    self.manifest_dir_synced.borrow_mut().remove(&dir);
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

  fn sequence_file_size(&self) -> Result<u64> {
    (self.segment_size as u64)
      .checked_shl(u32::from(self.segments_per_file_shift))
      .ok_or(KvError::InvalidArgs)
  }

  fn manifest_path(&self, tbl: &TableIdent) -> PathBuf {
    // 本地活跃 manifest 固定为 `manifest_main_0`（sto-70 决策）：term 只影响
    // data/segment 对象名，standby 安装也统一落这个名字。branch manifest 走
    // `write_manifest_file` / `reset_manifest_file` 的显式名字，不经过这里。
    self
      .part_dir(tbl)
      .join(manifest_file_name(MAIN_BRANCH_NAME, 0))
  }
}

impl PageIo for CompioStore {
  fn set_file_identity(&self, branch: &str, term: u64) {
    self.dir_index_map.borrow_mut().clear();
    self.fd_cache.borrow_mut().clear();
    *self.identity.borrow_mut() = FileIdentity {
      branch: branch.to_owned(),
      term,
    };
  }

  fn preload_path_li(&self, tbl: &TableIdent) -> usize {
    self.preload(tbl)
  }

  fn invalidate_path_cache(&self) {
    self.clear_caches();
  }

  fn set_verify_checksum(&self, enabled: bool) {
    self.skip_verify_checksum.set(!enabled);
  }

  fn evict_table(&self, tbl: &TableIdent) {
    self.forget_table_state(tbl);
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
    let page_size = self.page_size;
    let path_li = self.data_path_li(tbl, file_page_id).await;
    if path_li.is_empty() {
      return Err(KvError::ResourceMissing);
    }
    // 为什么不把 `dst` 直接交给 compio 读：`compio::buf::IoBuf: 'static`，
    // 调用方借入的 `&mut [u8]` 不是 `'static`，无法实现 `IoBufMut`；而签名
    // 又要求保留调用方所有权（不能 `mem::take` 换入 `Vec<u8>`）。
    // 因此退化为「池缓冲直读 + 一次性 copy」：池缓冲按容量复用且不预清零，
    // 额外 memcpy 固定为下面这一处，且只覆盖 `dst[..page_size]`，
    // `dst` 其余区域一个字节都不写。
    // 同一 file id 可能同时存在旧 term / 新 term 文件：只有「该候选确定没有
    // 这一页」（EOF / checksum 失败）才换下一个候选；短读耗尽、非可重试错误
    // 属于当前文件自身的终局故障，直接上抛——否则新 term 的 torn 页会静默
    // 落回旧 term 读回陈旧页。
    let offset = self.page_offset(file_page_id);
    let mut saw_eof = false;
    for (index, path) in path_li.into_iter().enumerate() {
      let outcome = self
        .read_page_candidate(&path, page_size, offset, dst)
        .await?;
      match outcome {
        CandidateRead::Hit => {
          if index > 0 {
            // 更新的候选不可用后从旧 term 读到完整页：记一次回退命中。
            self.stale_read_num.fetch_add(1, Ordering::Relaxed);
          }
          return Ok(());
        }
        CandidateRead::Eof => saw_eof = true,
        CandidateRead::Miss => {}
      }
    }
    if saw_eof {
      Err(KvError::EndOfFile)
    } else {
      Err(KvError::Corrupted)
    }
  }

  /// 批内每页各压一个 SQE 后统一等待（对齐 C++ `ReadPages`）；批大小由
  /// overflow 链每层指针数决定（≤ `overflow_pointers`），天然有界。
  async fn read_page_li(
    &self,
    tbl: &TableIdent,
    file_page_id_li: &[FilePageId],
    page_buf_li: &mut [Vec<u8>],
  ) -> Result<()> {
    if file_page_id_li.len() != page_buf_li.len() {
      return Err(KvError::InvalidArgs);
    }
    try_join_all(
      file_page_id_li
        .iter()
        .zip(page_buf_li.iter_mut())
        .map(|(file_page_id, buf)| self.read_page(tbl, *file_page_id, buf)),
    )
    .await?;
    Ok(())
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    if src.len() != self.page_size {
      return Err(KvError::InvalidArgs);
    }
    fkv_failpoint::fail_point!(WRITE_PAGE);
    fkv_failpoint::kill_point(WRITE_PAGE);
    self.ensure_dir(tbl).await?;
    let path = self.data_path(tbl, file_page_id);
    let mut file = self.open_write_file(&path).await.map_err(map_io_err)?;
    let chunk = if self.direct_io.get() {
      WriteChunk::Direct(DirectBuf::from_slice(self.take_direct_buf(src.len()), src))
    } else {
      let mut buf = self.take_write_buf(src.len());
      debug_assert_eq!(buf.len(), src.len());
      buf.copy_from_slice(src);
      WriteChunk::Plain(buf)
    };
    self.write_at_num.fetch_add(1, Ordering::Relaxed);
    let write_len = self
      .write_chunk(&mut file, chunk, self.page_offset(file_page_id))
      .await
      .map_err(map_io_err)?;
    if write_len != src.len() {
      return Err(KvError::IoFail);
    }
    self.mark_dirty(tbl, path);
    Ok(())
  }

  async fn write_page_li(&self, tbl: &TableIdent, page_li: &[(FilePageId, Vec<u8>)]) -> Result<()> {
    for (_, page) in page_li {
      if page.len() != self.page_size {
        return Err(KvError::InvalidArgs);
      }
      fkv_failpoint::fail_point!(WRITE_PAGE);
      fkv_failpoint::kill_point(WRITE_PAGE);
    }
    self.ensure_dir(tbl).await?;
    // 按 (file_id, page_index) 排序，连续页合并为一次 write_at。
    let mut sorted: Vec<(u64, u64, &Vec<u8>)> = page_li
      .iter()
      .map(|(file_page_id, page)| {
        let file_id = file_page_id >> self.pages_per_file_shift;
        let page_index = file_page_id & ((1u64 << self.pages_per_file_shift) - 1);
        (file_id, page_index, page)
      })
      .collect();
    sorted.sort_unstable_by_key(|(file_id, page_index, _)| (*file_id, *page_index));
    // 按连续段切组，页切片只借用不拷贝；缓冲在组 future 内按需借出，
    // 避免先把整批页拷进临时大 Vec。组内仍是一次连续写，页数受
    // `max_write_batch_pages` 约束。
    let max_pages = self.max_write_batch_pages;
    let mut group_li: Vec<(PathBuf, u64, Vec<&[u8]>, usize)> = Vec::new();
    let mut start = 0usize;
    while start < sorted.len() {
      let (file_id, first_index, _) = sorted[start];
      let mut end = start + 1;
      while end < sorted.len()
        && end - start < max_pages
        && sorted[end].0 == file_id
        && sorted[end].1 == sorted[end - 1].1 + 1
      {
        end += 1;
      }
      let page_li: Vec<&[u8]> = sorted[start..end]
        .iter()
        .map(|(_, _, page)| page.as_slice())
        .collect();
      let bytes = page_li.len() * self.page_size;
      let file_offset = first_index * self.page_size as u64;
      let path = self.data_path(tbl, file_id << self.pages_per_file_shift);
      group_li.push((path, file_offset, page_li, bytes));
      start = end;
    }
    let write_li = group_li
      .into_iter()
      .map(|(path, file_offset, page_li, bytes)| async move {
        // IO 层闸门：限制所有 write_page_li 调用叠加后的在飞写组数。
        let _permit = self.write_gate.acquire().await;
        let mut file = self.open_write_file(&path).await.map_err(map_io_err)?;
        self.write_at_num.fetch_add(1, Ordering::Relaxed);
        let write_len = self
          .write_page_group(&mut file, &page_li, bytes, file_offset)
          .await?;
        if write_len != bytes {
          return Err(KvError::IoFail);
        }
        Ok::<PathBuf, KvError>(path)
      });
    // 提交窗口与写闸门取同一容量：闸门是唯一的全局在飞上限，
    // `buffer_unordered` 只是不让本轮把超过总额的组 future 一起推进。
    let inflight_limit = self.max_inflight_write;
    let result_li: Vec<Result<PathBuf>> = stream::iter(write_li)
      .buffer_unordered(inflight_limit)
      .collect()
      .await;
    // 成功组先记脏再返回首个错误，避免失败组跳过其它已落盘文件的 sync。
    let mut first_err = None;
    for result in result_li {
      match result {
        Ok(path) => self.mark_dirty(tbl, path),
        Err(err) if first_err.is_none() => first_err = Some(err),
        Err(_) => {}
      }
    }
    first_err.map_or(Ok(()), Err)
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.data_sync_num.fetch_add(1, Ordering::Relaxed);
    fkv_failpoint::fail_point!(SYNC_DATA);
    // 快照路径集合，避免持有 RefCell 借用跨 await。
    let path_li: Vec<PathBuf> = self
      .dirty
      .borrow()
      .get(tbl)
      .map_or_else(Vec::new, |set| set.iter().cloned().collect());
    let mut file_li: Vec<(PathBuf, File)> = Vec::with_capacity(path_li.len());
    for path in path_li {
      match self.open_for_sync(&path).await {
        Ok(file) => file_li.push((path, file)),
        Err(err) if err.kind() == ErrorKind::NotFound => {
          // 文件已删除，无需落盘。
          self.forget_dirty(&path);
        }
        Err(_) => return Err(KvError::IoFail),
      }
    }
    // 多个脏文件并发 fdatasync（对齐 C++ `FdatasyncFiles` 批量提交）。
    let sync_li = file_li.iter().map(|(path, file)| {
      let path = path.clone();
      async move { (path, file.sync_data().await) }
    });
    let result_li: Vec<(PathBuf, io::Result<()>)> = stream::iter(sync_li)
      .buffer_unordered(FSYNC_INFLIGHT_LIMIT)
      .collect()
      .await;
    for (path, result) in result_li {
      result.map_err(map_io_err)?;
      if let Some(set) = self.dirty.borrow_mut().get_mut(tbl) {
        set.remove(&path);
      }
    }
    // 新建 / 删除过文件名：先文件后目录，保证 manifest 引用的文件目录项已落盘。
    // 只有目录 fsync 成功后才清 dirty_dir：失败时保留条目，下一次 sync_data
    // 仍会重试目录同步；先清后同步会让失败后的重试静默跳过。
    if self.dirty_dir.borrow().contains(tbl) {
      let dir = self.part_dir(tbl);
      self.sync_dir(&dir).await?;
      self.dirty_dir.borrow_mut().remove(tbl);
    }
    Ok(())
  }
}

impl ManifestIo for CompioStore {
  async fn reset_manifest(&self, tbl: &TableIdent, size: u64) -> Result<()> {
    let path = self.manifest_path(tbl);
    self.manifest_offset.borrow_mut().insert(path.clone(), size);
    let file = self.open_write_file(&path).await.map_err(map_io_err)?;
    // 截掉撕裂尾部，后续 append 从有效边界开始。
    //
    // 注意：`set_len` 本身**不 fsync**（对齐 C++ 的 `Reopen` 路径）。即"截断未持久"
    // 是一个真实窗口：掉电后撕裂尾可能复活，reopen 必须幂等地再切分并再截断一次
    // （gap-12）。此处保持不 fsync 是为了不在 reopen 热路径上多一次盘同步。
    file.set_len(size).await.map_err(map_io_err)?;
    Ok(())
  }

  async fn reset_manifest_file(&self, tbl: &TableIdent, name: &str, size: u64) -> Result<()> {
    let path = self.part_dir(tbl).join(name);
    self.manifest_offset.borrow_mut().insert(path.clone(), size);
    let file = self.open_write_file(&path).await.map_err(map_io_err)?;
    // branch manifest 与活跃 manifest 同契约：丢弃 torn / 损坏尾部。
    // 同 `reset_manifest`：截断不 fsync，掉电后撕裂尾可能复活（gap-12）。
    file.set_len(size).await.map_err(map_io_err)?;
    Ok(())
  }

  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.ensure_dir(tbl).await?;
    let dir = self.part_dir(tbl);
    self
      .append_file_at(&dir, &self.manifest_path(tbl), record)
      .await
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.ensure_dir(tbl).await?;
    self
      .atomic_write_file(&self.part_dir(tbl), &self.manifest_path(tbl), record)
      .await
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    self.read_manifest_path(&self.manifest_path(tbl)).await
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
    let path = self.manifest_path(tbl);
    match remove_file(&path).await {
      Ok(()) => {
        self.forget_fd(&path);
        Ok(())
      }
      Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
      Err(_) => Err(KvError::IoFail),
    }
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self.ensure_dir(tbl).await?;
    let dir = self.part_dir(tbl);
    self.atomic_write_file(&dir, &dir.join(name), record).await
  }

  async fn append_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self.ensure_dir(tbl).await?;
    let dir = self.part_dir(tbl);
    self.append_file_at(&dir, &dir.join(name), record).await
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    let dir = self.part_dir(tbl);
    // read_dir 是阻塞 syscall：整段扫描放在阻塞线程池。
    spawn_dir_io(move || -> Result<Vec<String>> {
      let entry_li = match read_dir(&dir) {
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
        if !name.starts_with(FILE_NAME_MANIFEST) || name.ends_with(TMP_SUFFIX) {
          continue;
        }
        if parse_manifest_file_name(name).is_some() {
          name_li.push(name.to_owned());
        }
      }
      name_li.sort();
      Ok(name_li)
    })
    .await?
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    self
      .read_manifest_path(&self.part_dir(tbl).join(name))
      .await
  }

  /// 只列非 manifest 的旁路文件（如云端 cache 的 segment 完成标记）。
  ///
  /// 与 [`Self::list_manifest_file_name_li`] 互补：manifest 名单由命名规则过滤，
  /// 这里返回其余普通文件；临时文件（`.tmp`）不暴露。
  async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    let dir = self.part_dir(tbl);
    spawn_dir_io(move || -> Result<Vec<String>> {
      let entry_li = match read_dir(&dir) {
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
        if name.starts_with(FILE_NAME_MANIFEST) || name.ends_with(TMP_SUFFIX) {
          continue;
        }
        name_li.push(name.to_owned());
      }
      name_li.sort_unstable();
      name_li.dedup();
      Ok(name_li)
    })
    .await?
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.remove_part_file(tbl, name).await
  }

  /// 旁路文件与 manifest 同目录存储，删除语义一致（不存在视为成功）。
  async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.remove_part_file(tbl, name).await
  }
}

impl CompioStore {
  /// 删除分区目录内的一个普通文件（manifest 与旁路文件共用）。
  async fn remove_part_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    let path = self.part_dir(tbl).join(name);
    match remove_file(&path).await {
      Ok(()) => {
        self.forget_fd(&path);
        Ok(())
      }
      Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
      Err(_) => Err(KvError::IoFail),
    }
  }
}

impl CompioStore {
  /// 按路径读整个文件；不存在返回 `None`。
  ///
  /// 不走 `FdCache`：standby 同步与 `switch_manifest` 会用 rename 替换整个文件，
  /// 缓存里的旧 fd 仍指向被替换的 inode（读旧内容、后续追加写进已删除文件）。
  /// 读前顺手清掉该路径的缓存 fd。
  async fn read_manifest_path(&self, path: &Path) -> Result<Option<Vec<u8>>> {
    self.forget_fd(path);
    let mut opts = OpenOptions::new();
    opts.read(true);
    let file = match opts.open(path).await {
      Ok(file) => file,
      Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
      Err(_) => return Err(KvError::IoFail),
    };
    self.read_whole(&file, false).await.map(Some)
  }
}

impl TermIo for CompioStore {
  async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>> {
    let path = self
      .root
      .join(fkv_types::current_term_file_name(branch, shard_id));
    spawn_dir_io(move || term::load(&path)).await?
  }

  /// 同步 IO（flock + fsync + rename）放在阻塞线程池，不阻塞 reactor。
  async fn upsert_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<u64> {
    let root = self.root.clone();
    let name = fkv_types::current_term_file_name(branch, shard_id);
    spawn_dir_io(move || term::upsert(&root, &name, term)).await?
  }

  async fn remove_term(&self, branch: &str, shard_id: u32) -> Result<()> {
    let path = self
      .root
      .join(fkv_types::current_term_file_name(branch, shard_id));
    spawn_dir_io(move || match fs::remove_file(&path) {
      Ok(()) => Ok(()),
      Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
      Err(_) => Err(KvError::IoFail),
    })
    .await?
  }
}

impl SegmentIo for CompioStore {
  fn write_segment_len(&self, _src_len: usize) -> u64 {
    self.segment_size as u64
  }

  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    if <[u8]>::is_empty(dst) {
      return Ok(());
    }
    if dst.len() > self.segment_size {
      return Err(KvError::InvalidArgs);
    }
    let len = dst.len();
    let file = self.open_segment_file(tbl, segment_fp_id).await?;
    let offset = self.segment_offset(segment_fp_id);
    if self.direct_segment() {
      // O_DIRECT 要求长度 / 缓冲对齐：读入向上取整的对齐缓冲，再取有效前缀。
      // `segment_size` 是页大小整数倍，任意 `len <= segment_size` 取整后仍落在
      // 该段范围内，不会越界读到下一段。
      let aligned = len.next_multiple_of(PAGE_ALIGN);
      let BufResult(result, buf) = file
        .read_at(
          DirectBuf::from_buf(self.take_direct_buf(aligned), 0),
          offset,
        )
        .await;
      let read_len = match result {
        Ok(read_len) => read_len.min(len),
        Err(err) => {
          // 失败路径同样归还对齐缓冲，保持池命中率（io-41）。
          self.put_direct_buf(buf.into_inner());
          return Err(map_io_err(err));
        }
      };
      // 段定长落盘、尾部补零：短读按零填充处理，不当作错误。
      dst[..read_len].copy_from_slice(&buf.as_init()[..read_len]);
      dst[read_len..].fill(0);
      self.put_direct_buf(buf.into_inner());
      return Ok(());
    }
    // provided-buffer 读：缓冲由 runtime 池常驻，io_uring 下免每次读的
    // page pin/unpin；仍是一次 copy。池耗尽 / 未配置 / 短读时回落自管池。
    if self.managed_seg.get()
      && let Ok(Some(buf)) = file.read_managed_at(len, offset).await
      && buf.len() >= len
    {
      dst.copy_from_slice(&buf[..len]);
      return Ok(());
    }
    let buf = self.take_read_buf(len);
    let BufResult(result, buf) = file.read_at(buf, offset).await;
    let read_len = match result {
      Ok(read_len) => read_len.min(len),
      Err(err) => {
        self.put_read_buf(len, buf);
        return Err(map_io_err(err));
      }
    };
    dst[..read_len].copy_from_slice(&buf[..read_len]);
    dst[read_len..].fill(0);
    self.put_read_buf(len, buf);
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
    fkv_failpoint::fail_point!(WRITE_SEGMENTS);
    self.ensure_dir(tbl).await?;
    let path = self.segment_path(tbl, segment_fp_id);
    let mut file = self.open_write_file(&path).await.map_err(map_io_err)?;
    // segment 按 segment_size 定长落盘，尾部补零。
    // O_DIRECT 下缓冲与长度都必须整页对齐：用对齐缓冲定长整段写。
    let chunk = if self.direct_segment() {
      let mut buf = DirectBuf::from_buf(self.take_direct_buf(self.segment_size), self.segment_size);
      buf.as_mut_slice()[..src.len()].copy_from_slice(src);
      buf.as_mut_slice()[src.len()..].fill(0);
      WriteChunk::Direct(buf)
    } else {
      let mut buf = self.take_write_buf(self.segment_size);
      debug_assert_eq!(buf.len(), self.segment_size);
      buf[..src.len()].copy_from_slice(src);
      buf[src.len()..].fill(0);
      WriteChunk::Plain(buf)
    };
    let write_len = self
      .write_chunk(&mut file, chunk, self.segment_offset(segment_fp_id))
      .await
      .map_err(map_io_err)?;
    if write_len != self.segment_size {
      return Err(KvError::IoFail);
    }
    self.preallocate_segment_file(&path, &file).await?;
    self.mark_dirty(tbl, path);
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
      let buf = Arc::new(AlignedBuf::new(len));
      let mut slot = self.fixed_io.lock().map_err(|_| KvError::IoFail)?;
      // 旧 ring 的注册表不能继续 register：ring_fd 可能已被新 runtime 复用，
      // BUFFERS_UPDATE 会写进兄弟 ring 的 sparse 表。换代即重建注册表。
      if slot
        .as_ref()
        .is_some_and(|buffers| !buffers.owns_current_ring())
      {
        *slot = None;
      }
      if slot.is_none() {
        *slot = Some(fixed::FixedBufferRegistry::current()?);
      }
      let buffers = slot.as_mut().ok_or(KvError::IoFail)?;
      let token = buffers.token();
      let index = buffers.register(buf.clone())?;
      drop(slot);
      let weak = Arc::downgrade(&self.fixed_io);
      let hook = Arc::new(move |index: u16| {
        if let Some(shared) = weak.upgrade()
          && let Ok(mut slot) = shared.lock()
          && let Some(buffers) = slot.as_mut()
          && Arc::ptr_eq(&buffers.token(), &token)
        {
          buffers.release(index);
        }
      });
      Ok(FixedBuffer::registered_arc_with_hook(buf, index, hook))
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
    // O_DIRECT 注册读要求缓冲地址与长度都按页对齐；段内偏移非对齐时回落
    // `read_segment`（内部用对齐缓冲承接）。buffered 句柄不受此限制。
    #[cfg(target_os = "linux")]
    let direct_ok = !self.direct_segment()
      || (offset.is_multiple_of(PAGE_ALIGN) && len.is_multiple_of(PAGE_ALIGN));
    #[cfg(target_os = "linux")]
    if direct_ok
      && buf.is_registered()
      && self.fixed_io.lock().ok().is_some_and(|slot| {
        slot
          .as_ref()
          .is_some_and(fixed::FixedBufferRegistry::owns_current_ring)
      })
    {
      let file = self.open_segment_file(tbl, segment_fp_id).await?;
      let slot = self.lease_fixed_slot(buf.index())?;
      let op = fixed::FixedRead::new(
        file.to_shared_fd(),
        buf.share().ok_or(KvError::Busy)?,
        offset,
        len,
        self.segment_offset(segment_fp_id),
        buf.index(),
      )?
      .with_slot(slot);
      let BufResult(result, _) = compio::runtime::submit(op).await;
      let read_len = result.map_err(map_io_err)?;
      // 段尾未写满部分补零。
      if read_len < len {
        buf.as_mut_slice().ok_or(KvError::Busy)?[offset + read_len..end].fill(0);
      }
      return Ok(());
    }
    let dst = buf.as_mut_slice().ok_or(KvError::Busy)?;
    self
      .read_segment(tbl, segment_fp_id, &mut dst[offset..end])
      .await
  }

  /// 批量 fixed 读：已注册且当前 ring 拥有槽位表时，整段
  /// 对齐项一次并发提交多个 READ_FIXED（对齐 C++ `ReadSegments` 批内并发）；
  /// O_DIRECT 下非页对齐的项（值尾段）与未注册 / 跨 ring 场景逐段串行回退。
  ///
  /// 并发安全：各区间互不重叠由 [`validate_fixed_segment_li`] 在提交前整批
  /// 强制；同槽位多个在飞 op 由 `SlotLease` 在飞计数与 `share_disjoint`
  /// 区间租约共同看护（io-37），future 被取消时租约随 op 存活到 CQE 收割。
  async fn read_segment_fixed_li(
    &self,
    tbl: &TableIdent,
    item_li: &[FixedSegmentRead],
    buf: &mut FixedBuffer,
  ) -> Result<()> {
    validate_fixed_segment_li(buf.len(), item_li)?;
    #[cfg(target_os = "linux")]
    if buf.is_registered() && self.owns_fixed_ring() {
      if item_li.iter().any(|item| item.len > self.segment_size) {
        return Err(KvError::InvalidArgs);
      }
      // O_DIRECT 下 READ_FIXED 要求缓冲偏移与长度页对齐；值尾段常不满足，
      // 留给串行回退用 `read_segment` 的对齐暂存缓冲承接。
      let (fixed_li, plain_li): (Vec<FixedSegmentRead>, Vec<FixedSegmentRead>) =
        item_li.iter().copied().partition(|item| {
          !self.direct_segment()
            || (item.offset.is_multiple_of(PAGE_ALIGN) && item.len.is_multiple_of(PAGE_ALIGN))
        });
      let read_len_li = {
        // 批内共享借用：各 READ_FIXED future 以互不重叠的区间租约并发持有
        // 同一注册缓冲，全部 CQE 收割后借用结束、在飞计数归零。
        let buf = &*buf;
        try_join_all(fixed_li.iter().map(|item| async move {
          let file = self.open_segment_file(tbl, item.segment_fp_id).await?;
          let slot = self.lease_fixed_slot(buf.index())?;
          let op = fixed::FixedRead::new(
            file.to_shared_fd(),
            buf.share_disjoint(),
            item.offset,
            item.len,
            self.segment_offset(item.segment_fp_id),
            buf.index(),
          )?
          .with_slot(slot);
          let BufResult(result, _) = compio::runtime::submit(op).await;
          result.map_err(map_io_err)
        }))
        .await?
      };
      // 短读（段文件被截断）补零尾部，语义与单段读一致。
      let dst = buf.as_mut_slice().ok_or(KvError::Busy)?;
      for (item, read_len) in fixed_li.iter().zip(read_len_li) {
        if read_len < item.len {
          dst[item.offset + read_len..item.offset + item.len].fill(0);
        }
      }
      for item in plain_li {
        self
          .read_segment_fixed(tbl, item.segment_fp_id, buf, item.offset, item.len)
          .await?;
      }
      return Ok(());
    }
    // 未注册 / 非 Linux / 跨 ring：逐段串行（同默认实现语义）。
    for item in item_li {
      self
        .read_segment_fixed(tbl, item.segment_fp_id, buf, item.offset, item.len)
        .await?;
    }
    Ok(())
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
    // 批量段写允许同一缓冲上多个互不重叠区间的 op 并发（fkv_large 的
    // `run_batch`），故用区间租约；`as_mut_slice` 仍被在飞计数挡住。
    let lease = buf.share_disjoint();
    #[cfg(target_os = "linux")]
    if buf.is_registered()
      && self.fixed_io.lock().ok().is_some_and(|slot| {
        slot
          .as_ref()
          .is_some_and(fixed::FixedBufferRegistry::owns_current_ring)
      })
    {
      // 尾段不足一整段时走普通路径：需要补零到 `segment_size`，否则复用旧段会
      // 残留旧字节；`write_segment` 用对齐缓冲按段起始偏移提交，O_DIRECT 下安全。
      if len != self.segment_size {
        return self
          .write_segment(tbl, segment_fp_id, &lease[offset..end])
          .await;
      }
      // 整段 WRITE_FIXED 在 O_DIRECT 下要求用户缓冲地址、长度与文件偏移都按页
      // 对齐：缓冲基址由注册时的 `AlignedBuf` 保证，文件偏移是 `segment_offset`
      // 加 `offset`。段内偏移非对齐时内核回 EINVAL，会被 `map_io_err` 摊平成
      // `IoFail` 硬错误，掩盖调用方的错位参数。此处提交前拒绝，返回
      // `InvalidArgs`，不静默落回 buffered（与 read 的回落语义不同：写有副作用，
      // 静默换路径会掩盖上层越界/错位 bug）。
      if self.direct_segment()
        && !(offset.is_multiple_of(PAGE_ALIGN) && len.is_multiple_of(PAGE_ALIGN))
      {
        return Err(KvError::InvalidArgs);
      }
      self.ensure_dir(tbl).await?;
      let path = self.segment_path(tbl, segment_fp_id);
      let file = self.open_write_file(&path).await.map_err(map_io_err)?;
      let slot = self.lease_fixed_slot(buf.index())?;
      let op = fixed::FixedWrite::new(
        file.to_shared_fd(),
        lease,
        offset,
        len,
        self.segment_offset(segment_fp_id),
        buf.index(),
      )?
      .with_slot(slot);
      let BufResult(result, _) = compio::runtime::submit(op).await;
      let write_len = result.map_err(map_io_err)?;
      if write_len != len {
        return Err(KvError::IoFail);
      }
      self.preallocate_segment_file(&path, &file).await?;
      self.mark_dirty(tbl, path);
      return Ok(());
    }
    self
      .write_segment(tbl, segment_fp_id, &lease[offset..end])
      .await
  }
}

impl FileIo for CompioStore {
  async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>> {
    match self.open_data_file(tbl, file_id).await? {
      Some(file) => self.read_whole(&file, self.direct_io.get()).await.map(Some),
      None => Ok(None),
    }
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()> {
    if !bytes.len().is_multiple_of(self.page_size) {
      return Err(KvError::InvalidArgs);
    }
    self.ensure_dir(tbl).await?;
    let path = self.data_path(tbl, file_id << self.pages_per_file_shift);
    // tmp + rename 原子替换：崩溃只可能留下 `.tmp`，不会让可见文件出现
    // 「前半段新内容、后半段旧内容 / 零洞」的中间态。
    let tmp = tmp_path(&path);
    let mut file = File::create(&tmp).await.map_err(map_io_err)?;
    if let Err(err) = self.write_whole(&mut file, bytes, false).await {
      drop(file);
      let _ = remove_file(&tmp).await;
      return Err(err);
    }
    // 对齐 C++ `WriteSnapshot`：rename 提交目录项前必须先 fdatasync tmp，
    // 否则掉电后可见文件可能是 0 长 / 稀疏洞，而 cloud 已按 bytes.len()
    // 把它登记为 resident，后续读不再回源。
    let mut sync_result = file.sync_data().await;
    if fkv_failpoint::fail_point(WRITE_FILE_SYNC_FAIL).is_some() {
      sync_result = Err(io::Error::other("injected write_file sync failure"));
    }
    if let Err(err) = sync_result {
      drop(file);
      let _ = remove_file(&tmp).await;
      return Err(map_io_err(err));
    }
    drop(file);
    rename(&tmp, &path).await.map_err(map_io_err)?;
    // 已改名、但目录项 fsync 仍推迟到下一次 `sync_data`（见函数尾）：这是掉电后
    // 文件可能"消失"的窗口。补 kill point 让它可注入（gap-12）。
    fkv_failpoint::kill_point(WRITE_FILE_RENAME);
    // rename 后旧句柄指向被替换的 inode，必须失效。
    self.forget_fd(&path);
    self.mark_dirty(tbl, path);
    // 新文件名首次出现的目录项同步交给下一次 sync_data。
    self.dirty_dir.borrow_mut().insert(tbl.clone());
    Ok(())
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>> {
    match self.open_data_file(tbl, file_id).await? {
      Some(file) => file
        .metadata()
        .await
        .map(|meta| Some(meta.len()))
        .map_err(map_io_err),
      None => Ok(None),
    }
  }

  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.list_file_id_by_kind(tbl, FileKind::Data).await
  }

  async fn list_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    self.list_file_variants_by_kind(tbl, FileKind::Data).await
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.remove_file_by_kind(tbl, file_id, FileKind::Data).await
  }

  async fn remove_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .remove_file_variant_by_kind(tbl, file_id, FileKind::Data, branch, term)
      .await
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.list_file_id_by_kind(tbl, FileKind::Segment).await
  }

  async fn list_segment_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    self
      .list_file_variants_by_kind(tbl, FileKind::Segment)
      .await
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self
      .remove_file_by_kind(tbl, file_id, FileKind::Segment)
      .await
  }

  async fn remove_segment_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .remove_file_variant_by_kind(tbl, file_id, FileKind::Segment, branch, term)
      .await
  }

  /// 空分区目录移除：目录里还有任何条目（data / segment / manifest / 临时
  /// 文件 / 未知残留）就原样保留，只 `remove_dir` 不递归删除。
  async fn try_remove_empty_partition_dir(&self, tbl: &TableIdent) -> Result<bool> {
    let dir = self.part_dir(tbl);
    let removed = spawn_dir_io({
      let dir = dir.clone();
      move || -> Result<bool> {
        let mut entry_li = match read_dir(&dir) {
          Ok(entry_li) => entry_li,
          Err(err) if err.kind() == ErrorKind::NotFound => return Ok(false),
          Err(_) => return Err(KvError::IoFail),
        };
        // 只要还有条目（含读不出名的残留）就不删，保守优先。
        if entry_li.next().is_some() {
          return Ok(false);
        }
        match remove_dir(&dir) {
          Ok(()) => Ok(true),
          Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
          Err(_) => Err(KvError::IoFail),
        }
      }
    })
    .await??;
    if removed {
      self.forget_table_state(tbl);
    }
    Ok(removed)
  }
}

#[cfg(test)]
mod test {
  use std::{pin::pin, task::Poll};

  use futures::{executor::block_on, poll};

  use super::WriteGate;

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
