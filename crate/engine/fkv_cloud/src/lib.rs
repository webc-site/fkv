//! 云 overlay：本地 cache 为第一层，对象存储为 durable 副本。
//!
//! data 以**整文件**为单位上传 / 下载；manifest 也是整对象。
//! 本地 cache 按字节预算做 LRU 淘汰，dirty 文件在上传完成前不淘汰。
//!
//! 淘汰只做内存摘链 / 记账；本地文件删除进入待删队列，按预算的 1/8
//! 批量 [`CloudStore::drain_evictions`]（flush 时全量 drain），读写路径不再
//! 逐文件同步 unlink。预算记账在摘链时立即扣减，删除失败项回队重试。

use core::{
  cell::{Cell, Ref, RefCell},
  cmp,
  future::Future,
  mem::replace,
  ops::Deref,
};
use std::{
  collections::hash_map::Entry,
  mem::take,
  str::from_utf8,
  sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicU64, Ordering},
  },
  time::Duration,
};

use bytes::Bytes;
use fkv_error::{KvError, Result};
use fkv_io::{
  FileIo, FileVariant, FixedBuffer, FixedSegmentIo, ManifestIo, PageIo, SegmentIo, TermIo,
};
use fkv_metrics::ShardMetric;
use fkv_object::ObjectStore;
use fkv_types::{
  BranchFileRange, DEFAULT_SEGMENT_SIZE, DEFAULT_SEGMENTS_PER_FILE_SHIFT, FileId, FilePageId,
  MAIN_BRANCH_NAME, TableIdent, current_term_file_name, manifest_file_name,
};
use futures::channel::oneshot;
use futures_timer::Delay;
use gxhash::{HashMap, HashSet};

/// 当前分支 / term，参与云对象文件名。对齐 C++ `ComposeKey`：
/// `<prefix>/<table>.<partition>/<data|segment|manifest>_<...>_<branch>_<term>`。
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileIdentity {
  branch: String,
  term: u64,
}

impl Default for FileIdentity {
  fn default() -> Self {
    Self {
      branch: MAIN_BRANCH_NAME.to_owned(),
      term: 0,
    }
  }
}

/// 标记「本地 manifest 已写、远端发布未完成」；prewarm 不得据此删除本地快照。
///
/// 登记落在共享组上：prewarm 与 shard 是不同实例，实例私有登记看不到对方的
/// 首写窗口，会删掉 shard 正在发布（本地有、远端还没有）的本地 manifest。
struct PublishGuard<'a> {
  group: &'a CacheGroup,
  tbl: TableIdent,
}

impl<'a> PublishGuard<'a> {
  fn new(group: &'a CacheGroup, tbl: &TableIdent) -> Self {
    group.publish_guard().insert(tbl.clone());
    Self {
      group,
      tbl: tbl.clone(),
    }
  }
}

impl Drop for PublishGuard<'_> {
  fn drop(&mut self) {
    self.group.publish_guard().remove(&self.tbl);
  }
}

/// 在 `file_range_li` 链中二分定位 file id 的 `(branch, term)`。
///
/// `max_file_id` / `max_segment_file_id` 跨区间单调不减（对齐 C++ `FindBranchRange`
/// 的 `lower_bound`），因此 `partition_point` 对两类文件都成立。
fn range_ident(
  range_li: &[BranchFileRange],
  segment: bool,
  file_id: FileId,
) -> Option<(&str, u64)> {
  let index = if segment {
    range_li.partition_point(|range| range.max_segment_file_id < file_id)
  } else {
    range_li.partition_point(|range| range.max_file_id < file_id)
  };
  range_li
    .get(index)
    .map(|range| (range.branch_name.as_ref(), range.term))
}

/// LRU 链表空指针哨兵：节点下标 `u32::MAX` 表示无。
const LRU_NIL: u32 = u32::MAX;

/// manifest 上传的最大尝试次数（含首次）。
const MANIFEST_RETRY_TIMES: u32 = 10;
/// manifest 重试首次退避的毫秒数。
const MANIFEST_RETRY_INIT_DELAY_MS: u64 = 10;
/// manifest 重试退避的毫秒上限。
const MANIFEST_RETRY_MAX_DELAY_MS: u64 = 1000;
/// term CAS 的最大尝试次数（含首次）。
const TERM_CAS_RETRY_TIMES: u32 = 10;
/// 待删队列达到 `cache_budget >> EVICT_DRAIN_SHIFT`（预算的 1/8）时批量删除。
/// 即本地磁盘最多比预算多出一份「已摘链、未删除」的字节。
const EVICT_DRAIN_SHIFT: u32 = 3;
/// 待删队列文件数上限：文件很小时也保证批量删除的规模有界。
const EVICT_DRAIN_FILE_NUM: usize = 1024;
/// 待删队列驻留上限：坏盘 / 权限错误时失败项回队不能无界增长。
/// 超出后丢弃本轮淘汰项（本地文件留在盘上，由下次全量清理处理）。
pub const EVICT_PENDING_FILE_MAX: usize = 8192;
/// 连续 drain 失败达到该次数后暂停热路径重试，避免每个 IO 都撞同一个坏盘。
const EVICT_FAIL_PAUSE: u32 = 8;

/// segment 文件完成标记的文件名前缀（本地 cache 侧旁路元数据）。
///
/// 名字不以 `manifest_` 开头，本地后端的 manifest 列举与
/// `parse_manifest_file_name` 都不会把它当成 manifest；它只是复用
/// `ManifestIo::write_manifest_file` 的原子整写能力落一个旁路文件。
/// 标记是**可选**的：旧目录没有它时继续按尾槽规则判定，on-disk 兼容。
const SEGMENT_MARKER_PREFIX: &str = "segcomplete";
/// 完成标记内容：固定魔数，便于识别脏标记与将来演进。
const SEGMENT_MARKER_MAGIC: &[u8] = b"fkv-segcomplete-v1";

/// 某 segment 文件的完成标记名。
fn segment_marker_name(file_id: FileId) -> String {
  format!("{SEGMENT_MARKER_PREFIX}_{file_id}")
}

/// 是否为 segment 完成标记（表级 scheme 标记或单文件标记）。
///
/// 这些名字不是 manifest，`list_manifest_file_name_li` 合并本地 cache 时要滤掉。
fn is_segment_marker_name(name: &str) -> bool {
  name.starts_with(SEGMENT_MARKER_PREFIX)
}

/// manifest 重试判据：通用瞬时错误（`is_retryable`）之上，额外接纳超时与云服务错误。
/// 云端不可用表现为 `Timeout` / `CloudErr`，不属于进程内瞬时错误分类。
const fn manifest_retryable(err: KvError) -> bool {
  err.is_retryable() || matches!(err, KvError::Timeout | KvError::CloudErr)
}

/// 侵入式 LRU 节点：只登记**干净**（已上传）的 data / segment 文件，
/// 对齐 C++ `closed_files_` 不含 dirty 文件。`prev/next` 为节点 arena 下标。
#[derive(Debug, Clone, Copy)]
struct LruNode {
  prev: u32,
  next: u32,
  /// 反查 `CacheState::table_li` 的下标，节点不复制 `TableIdent`。
  table_idx: u32,
  file_id: FileId,
  segment: bool,
}

#[derive(Debug, Clone)]
struct FileState {
  size: u64,
  dirty: bool,
  /// 已修改但未上传的字节区间 `(offset, len)`；空表示整文件重传。
  dirty_range_li: Vec<(u64, u64)>,
  /// `dirty_range_li` 对应的写时 `(branch, term)`。identity 变更后区间指向旧对象，
  /// flush 时必须丢弃并改为整对象上传，否则区间写会用零前缀覆盖新对象。
  dirty_ident: Option<(String, u64)>,
  /// 强制整对象上传。identity 首次变更后必须置位：旧脏页属于旧对象，
  /// 新区间又归属新 identity，只写新区间会让新对象前段留下空洞。
  dirty_all: bool,
  /// LRU 节点下标；`LRU_NIL` 表示脏文件不在链上。
  lru_idx: u32,
}

/// 单表的缓存条目。data 与 segment 分开记账：`FileId` 同一编号空间，不能共用一个 map。
#[derive(Debug, Default)]
struct TableCache {
  /// 反查 `CacheState::table_li` 的下标。
  table_idx: u32,
  data: HashMap<FileId, FileState>,
  segment: HashMap<FileId, SegmentState>,
}

impl TableCache {
  /// 该表占用的驻留字节。
  fn bytes(&self) -> u64 {
    self.data.values().map(|entry| entry.size).sum::<u64>()
      + self.segment.values().map(|entry| entry.size).sum::<u64>()
  }

  fn len(&self) -> usize {
    self.data.len() + self.segment.len()
  }
}

/// 借 `tbl` 查表；只有该表首次出现时 clone 一次 `TableIdent`。
/// 同时分配 `table_li` 槽位，供 LRU 节点反查表名。
fn table_slot<'a>(
  table_map: &'a mut HashMap<TableIdent, TableCache>,
  table_li: &mut Vec<Option<TableIdent>>,
  free_table_li: &mut Vec<u32>,
  tbl: &TableIdent,
) -> &'a mut TableCache {
  if let Some(cache) = table_map.get_mut(tbl) {
    return cache;
  }
  let table_idx = match free_table_li.pop() {
    Some(idx) => idx,
    None => {
      table_li.push(None);
      (table_li.len() - 1) as u32
    }
  };
  table_li[table_idx as usize] = Some(tbl.clone());
  table_map.insert(
    tbl.clone(),
    TableCache {
      table_idx,
      ..TableCache::default()
    },
  );
  // 刚插入，get_mut 必中。
  table_map.get_mut(tbl).unwrap()
}

/// 归还 `table_li` 槽位；调用前表内条目必须已全部摘链。
fn free_table_slot(table_li: &mut [Option<TableIdent>], free_table_li: &mut Vec<u32>, idx: u32) {
  table_li[idx as usize] = None;
  free_table_li.push(idx);
}

/// 表内条目全空时清掉外层登记，避免空表常驻。
fn drop_if_empty(state: &mut CacheState, tbl: &TableIdent) {
  let table_idx = state
    .table_map
    .get(tbl)
    .filter(|cache| cache.data.is_empty() && cache.segment.is_empty())
    .map(|cache| cache.table_idx);
  if let Some(table_idx) = table_idx {
    state.table_map.remove(tbl);
    free_table_slot(&mut state.table_li, &mut state.free_table_li, table_idx);
  }
}

/// 从空闲链取槽位并挂到链尾（最近使用端），返回节点下标。
///
/// 空链复用 `node_li` 的空闲槽，节点 arena 上界 = 历史峰值驻留条目数。
fn lru_push(
  node_li: &mut Vec<LruNode>,
  free_node: &mut u32,
  head: &mut u32,
  tail: &mut u32,
  table_idx: u32,
  file_id: FileId,
  segment: bool,
) -> u32 {
  let node = LruNode {
    prev: *tail,
    next: LRU_NIL,
    table_idx,
    file_id,
    segment,
  };
  let idx = if *free_node == LRU_NIL {
    node_li.push(node);
    (node_li.len() - 1) as u32
  } else {
    let idx = *free_node;
    *free_node = node_li[idx as usize].next;
    node_li[idx as usize] = node;
    idx
  };
  match *tail {
    LRU_NIL => *head = idx,
    tail_idx => node_li[tail_idx as usize].next = idx,
  }
  *tail = idx;
  idx
}

/// 摘链并归还槽位（`next` 改作空闲链指针）。
fn lru_unlink(
  node_li: &mut [LruNode],
  free_node: &mut u32,
  head: &mut u32,
  tail: &mut u32,
  idx: u32,
) {
  let node = node_li[idx as usize];
  match node.prev {
    LRU_NIL => *head = node.next,
    prev => node_li[prev as usize].next = node.next,
  }
  match node.next {
    LRU_NIL => *tail = node.prev,
    next => node_li[next as usize].prev = node.prev,
  }
  node_li[idx as usize].next = *free_node;
  *free_node = idx;
}

/// 已在链上的节点挪到链尾（读命中 / 重新变干净）。
fn lru_move_tail(node_li: &mut [LruNode], head: &mut u32, tail: &mut u32, idx: u32) {
  if *tail == idx {
    return;
  }
  let node = node_li[idx as usize];
  match node.prev {
    LRU_NIL => *head = node.next,
    prev => node_li[prev as usize].next = node.next,
  }
  match node.next {
    LRU_NIL => *tail = node.prev,
    next => node_li[next as usize].prev = node.prev,
  }
  let tail_idx = *tail;
  node_li[idx as usize].prev = tail_idx;
  node_li[idx as usize].next = LRU_NIL;
  match tail_idx {
    LRU_NIL => *head = idx,
    t => node_li[t as usize].next = idx,
  }
  *tail = idx;
}

/// 缓存状态：一条全局侵入式 LRU 链表 + 按表嵌套的条目表。
///
/// 链表顺序即 LRU 顺序（head 最旧、tail 最新），victim 取 head 为 O(1)，
/// 不再全表扫描。对齐 C++ `lru_file_head_/tail_`。
#[derive(Debug)]
struct CacheState {
  table_map: HashMap<TableIdent, TableCache>,
  /// `table_li[table_idx]` 反查表名；`None` 表示槽位空闲。
  table_li: Vec<Option<TableIdent>>,
  free_table_li: Vec<u32>,
  node_li: Vec<LruNode>,
  free_node: u32,
  lru_head: u32,
  lru_tail: u32,
  resident_bytes: u64,
  /// 已摘链、待本地删除的淘汰对象；`resident_bytes` 入队时已扣减。
  pending_delete_li: Vec<CacheVictim>,
  pending_delete_bytes: u64,
  /// 入队代数。`drain_seq` 追平时说明这批淘汰已尝试过删除：失败后没有
  /// 新淘汰就不在热路径上重试，避免本地盘持续报错时每个 IO 都重试。
  evict_seq: u64,
  drain_seq: u64,
  /// 连续 drain 失败次数；达到 [`EVICT_FAIL_PAUSE`] 后暂停热路径 drain。
  drain_fail_streak: u32,
}

impl Default for CacheState {
  fn default() -> Self {
    Self {
      table_map: HashMap::default(),
      table_li: Vec::new(),
      free_table_li: Vec::new(),
      node_li: Vec::new(),
      free_node: LRU_NIL,
      lru_head: LRU_NIL,
      lru_tail: LRU_NIL,
      resident_bytes: 0,
      pending_delete_li: Vec::new(),
      pending_delete_bytes: 0,
      evict_seq: 0,
      drain_seq: 0,
      drain_fail_streak: 0,
    }
  }
}

/// segment 文件固定整文件上传，只记大小 / 脏标记 / LRU。
#[derive(Debug, Clone)]
struct SegmentState {
  size: u64,
  dirty: bool,
  /// LRU 节点下标；`LRU_NIL` 表示脏文件不在链上。
  lru_idx: u32,
}

/// 远端列举每页条数；与 C++ prewarm 的有界队列（1000）同量级，
/// 页内对象只在消费期间驻留，内存 O(页) 而非 O(bucket)。
pub const CLOUD_LIST_PAGE_KEYS: usize = 1000;

/// 远端文件条目：id + 对象字节数。预热要按 `(剩余空间 < 文件大小)` 预判
/// 磁盘是否够用（对齐 C++ `LocalCacheRemained() < file.file_size`），
/// 大小从列举结果直接拿，不额外发 HEAD。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteFileEntry {
  pub file_id: FileId,
  pub size: u64,
}

/// 远端文件条目的一页；`next_token` 语义同 [`ObjectStore::list_page`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePage {
  pub entry_li: Vec<RemoteFileEntry>,
  pub next_token: Option<String>,
}

/// 远端分区（`{tbl}.{partition}`）列举的一页。
///
/// 同一分区的对象在字节序列举里连续，因此跨页去重只需记住「本页处理到的最后
/// 一个分区」(`last`)：下一页把它作为 `after` 传入即可，内存 O(页) 而非 O(bucket)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartitionPage {
  pub table_li: Vec<TableIdent>,
  /// 本页处理到的最后一个分区（含尚未列举完的那个），下一页据此去重。
  pub last: Option<TableIdent>,
  pub next_token: Option<String>,
}

/// LRU 淘汰对象：data / segment 走不同的本地删除接口。
///
/// `bytes` 是摘链时已从 `resident_bytes` 扣减的字节数，删除失败回队时用它
/// 恢复待删字节记账。
#[derive(Debug, Clone)]
struct CacheVictim {
  tbl: TableIdent,
  file_id: FileId,
  segment: bool,
  bytes: u64,
}

/// 单飞下载键：同一文件的并发冷读只允许一个 owner 回源。
type DownloadKey = (TableIdent, bool, FileId);

/// 单飞下载结果：owner 已把文件装入本地，或远端确认不存在。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DownloadOutcome {
  Cached,
  Missing,
}

/// 本地 segment 文件的完整状态（只读尾槽 + 标记，O(1)）。
///
/// `Missing` 与 `Incomplete` 必须分开：前者直接回源，后者要先删掉崩溃残留，
/// 否则整文件覆盖会在中段留零洞；混成一个 `false` 会多一次无谓的删目录索引
/// 刷新（compio `remove_segment_file` 会走 `variant_path_li`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalSegmentState {
  Missing,
  Incomplete,
  Complete,
}

/// owner 下载期间挂起的等待者；owner 完成后统一下发结果。
type DownloadWaiter = oneshot::Sender<Result<DownloadOutcome>>;

/// 在飞下载登记：`generation` 标识当前 owner。
///
/// `evict_table` 摘掉登记后，同一 key 会被新一代 owner 重新登记；旧 owner
/// 完成 / 取消时只能摘**自己那代**的登记，否则会删掉新 owner 的入口并把旧
/// 结果发给新 owner 的等待者（同一 key 双 owner 重复下载、`<file>.tmp`
/// 竞争、等待者拿到错误结果）。
#[derive(Debug)]
struct InflightDownload {
  generation: u64,
  waiter_li: Vec<DownloadWaiter>,
}

/// owner 取消（future 被 drop）时移除在飞登记并唤醒等待者，避免永久挂起。
struct DownloadGuard<'a, I, O> {
  store: &'a CloudStore<I, O>,
  key: DownloadKey,
  /// 本 owner 的世代号：完成时只摘自己这代登记。
  generation: u64,
  done: bool,
}

impl<I, O> Drop for DownloadGuard<'_, I, O> {
  fn drop(&mut self) {
    if !self.done {
      self
        .store
        .finish_download(&self.key, self.generation, Err(KvError::TryAgain));
    }
  }
}

impl<I, O> DownloadGuard<'_, I, O> {
  fn finish(mut self, outcome: Result<DownloadOutcome>) -> Result<DownloadOutcome> {
    self
      .store
      .finish_download(&self.key, self.generation, outcome);
    self.done = true;
    outcome
  }
}

/// 组级 scheme 回填守卫：同一 `CacheGroup` 内每表只允许一个回填者。
///
/// 实例私有的登记挡不住「两个 shard 实例同时回填同一张表」；登记落到组上，
/// `Drop` 释放，future 被取消时不会永久占位。
struct EnablingGuard<'a> {
  group: &'a CacheGroup,
  tbl: TableIdent,
}

impl Drop for EnablingGuard<'_> {
  fn drop(&mut self) {
    self.group.enabling_guard().remove(&self.tbl);
  }
}

/// 一次 segment 安装 / 写入的组内在飞登记。
///
/// 所有安装路径（读 / 写 / 预热）都从 [`CloudStore::begin_segment_install`] 入口
/// 登记，`Drop` 时注销：不再依赖 `single_flight`，预热直连安装也参与计数。
/// 按 owner 记深度：写路径的登记与其内部 install 的登记会嵌套，深度配对后
/// 内层 `Drop` 不会把外层仍在写的文件提前注销。
struct SegmentInflightGuard<'a> {
  group: &'a CacheGroup,
  owner: u64,
  tbl: TableIdent,
  file_id: FileId,
}

impl<'a> SegmentInflightGuard<'a> {
  /// 无条件登记一次在飞（写路径用：本实例正在写这个文件）。
  fn new(group: &'a CacheGroup, owner: u64, tbl: &TableIdent, file_id: FileId) -> Self {
    group.set_segment_inflight(owner, tbl, file_id, true);
    Self {
      group,
      owner,
      tbl: tbl.clone(),
      file_id,
    }
  }

  /// 抢占安装独占权：登记与「他人是否在飞 / 持脏」判定在同一把锁内完成。
  ///
  /// 返回 `None` 时必须让路：同组另一实例正在写同一文件（或持有未上传的脏
  /// 数据），覆盖会把它的写抹成远端旧值，随后它的 flush 会把旧值上传成权威。
  fn claim(group: &'a CacheGroup, owner: u64, tbl: &TableIdent, file_id: FileId) -> Option<Self> {
    group
      .claim_segment_inflight(owner, tbl, file_id)
      .then(|| Self {
        group,
        owner,
        tbl: tbl.clone(),
        file_id,
      })
  }
}

impl Drop for SegmentInflightGuard<'_> {
  fn drop(&mut self) {
    self
      .group
      .set_segment_inflight(self.owner, &self.tbl, self.file_id, false);
  }
}

/// `CloudStore` 在共享组内的 owner 租约。
///
/// 共享组通常比实例长寿：实例销毁时必须清掉本 owner 的脏登记，否则该
/// segment 文件在组内永久 busy，后续 scheme 回填永远跳过它。
#[derive(Debug)]
struct OwnerLease {
  group: Arc<CacheGroup>,
  id: u64,
}

impl OwnerLease {
  fn new(group: Arc<CacheGroup>) -> Self {
    let id = group.alloc_instance_id();
    Self { group, id }
  }
}

impl Deref for OwnerLease {
  type Target = CacheGroup;

  fn deref(&self) -> &CacheGroup {
    &self.group
  }
}

impl Drop for OwnerLease {
  fn drop(&mut self) {
    self.group.release_owner(self.id);
  }
}

/// 跨实例共享的本地 cache 预算组。
///
/// 同一个 `CacheGroup` 下的所有 `CloudStore`（例如同一运行时的各 shard 与运行时
/// 侧预热线程）共用一个字节上限：`resident_bytes` 是整组记账，任一实例写入后
/// 按**全局**用量触发淘汰，因此 `local_space_limit` 是整机本地 cache 上界，
/// 不再是 `(实例数 × 预算)`。
///
/// 未共享（`with_cache_budget`）时每个实例自己一组，行为与旧版完全一致。
/// 淘汰仍只能摘本实例的 LRU：某个实例在全局超预算但自己没有可淘汰条目时
/// 无法替别的实例删文件；它自己新写入的 clean 条目会立即被摘回，磁盘上界
/// 只额外多出待删队列（`cache_budget / 8`，`drain_evictions` 后归零）。
///
/// 实例销毁不主动归还自己的记账：共享组通常与运行时同生命周期，若组比实例
/// 长寿，残留记账只会让后续实例更早淘汰（保守方向），不会突破预算。
#[derive(Debug)]
pub struct CacheGroup {
  limit: u64,
  resident_bytes: AtomicU64,
  /// 整组在飞 / 脏的 segment 文件登记：scheme 回填据此跳过**任一实例**正在
  /// 写的文件（见 [`CloudStore::segment_backfill_skip`]）。
  ///
  /// 覆盖边界：只覆盖共享同一 `CacheGroup` 的实例（同一运行时内的各 shard 与
  /// 预热实例）。跨进程共用同一本地 root 时观测不到对方的写现场，这种部署
  /// 限定只由该分区的写实例启用 scheme，预热 / 只读实例不得触发回填。
  ///
  /// 按表嵌套，查询只借 `&TableIdent`：`segment_backfill_skip` 在回填循环里
  /// 对每个文件调一次，扁平 key 每次都要 clone 两个 `TableIdent`。
  segment_busy: Mutex<HashMap<TableIdent, HashMap<FileId, SegmentBusy>>>,
  /// 整组正在回填 scheme 的表：实例私有的登记挡不住两个 shard 实例同时回填。
  enabling_tbl_set: Mutex<HashSet<TableIdent>>,
  /// 整组正在发布本地 manifest 的表：本地已写、远端 PUT 尚未确认。
  publish_tbl_set: Mutex<HashSet<TableIdent>>,
  /// 实例 id 分配器：脏登记按 owner 集合记，避免一个实例 `mark_clean` 抹掉
  /// 另一个实例仍在写的脏状态。
  next_instance_id: AtomicU64,
}

/// 单个 segment 文件的跨实例写现场。
#[derive(Debug, Default)]
struct SegmentBusy {
  /// 各实例的在飞安装 / 写入深度：同一实例的写路径与其内部 install 会嵌套
  /// 登记，用深度配对。只记计数（旧实现）无法区分 owner，一个实例的注销会
  /// 抹掉另一个实例仍在写的状态。
  inflight_owner: HashMap<u64, u32>,
  /// 持有未上传脏数据的实例 id。
  dirty_owner_set: HashSet<u64>,
}

impl SegmentBusy {
  fn busy(&self) -> bool {
    !self.inflight_owner.is_empty() || !self.dirty_owner_set.is_empty()
  }

  /// 除 `owner` 之外是否有实例正在安装或有未上传脏数据。
  fn foreign_busy(&self, owner: u64) -> bool {
    self.inflight_owner.keys().any(|id| *id != owner)
      || self.dirty_owner_set.iter().any(|id| *id != owner)
  }

  #[cfg(test)]
  fn inflight_num(&self) -> u32 {
    self.inflight_owner.values().sum()
  }
}

impl CacheGroup {
  /// `limit == 0` 表示不限制预算（与 [`CloudStore::with_cache_budget`] 一致）。
  pub fn new(limit: u64) -> Self {
    Self {
      limit,
      resident_bytes: AtomicU64::new(0),
      segment_busy: Mutex::new(HashMap::default()),
      enabling_tbl_set: Mutex::new(HashSet::default()),
      publish_tbl_set: Mutex::new(HashSet::default()),
      next_instance_id: AtomicU64::new(1),
    }
  }

  pub fn limit(&self) -> u64 {
    self.limit
  }

  /// 整组当前驻留字节（各实例 resident 记账之和）。
  ///
  /// 同一批文件被多个实例各自登记时会重复计数（偏保守，只会早淘汰，不会超限）。
  pub fn resident_bytes(&self) -> u64 {
    self.resident_bytes.load(Ordering::Relaxed)
  }

  /// 为一个 `CloudStore` 分配组内唯一 owner id。
  fn alloc_instance_id(&self) -> u64 {
    self.next_instance_id.fetch_add(1, Ordering::Relaxed)
  }

  /// 登记 / 注销一次在飞 segment 下载（按 owner 记深度）。
  fn set_segment_inflight(&self, owner: u64, tbl: &TableIdent, file_id: FileId, inflight: bool) {
    let mut map = self.busy_guard();
    let file_map = map.entry(tbl.clone()).or_default();
    let entry = file_map.entry(file_id).or_default();
    if inflight {
      *entry.inflight_owner.entry(owner).or_insert(0) += 1;
    } else {
      match entry.inflight_owner.get_mut(&owner) {
        Some(depth) if *depth > 1 => *depth -= 1,
        Some(_) => {
          entry.inflight_owner.remove(&owner);
        }
        // 未配对的注销：不能减别人的深度，也不能让记账下溢。
        None => {}
      }
    }
    if !entry.busy() {
      file_map.remove(&file_id);
    }
    if file_map.is_empty() {
      map.remove(tbl);
    }
  }

  /// 抢占该 segment 文件的安装独占权：登记本 owner 并判定他人是否在飞 / 持脏。
  ///
  /// 判定与登记在同一把锁内完成：返回 `true` 说明此刻没有别的实例在写该文件，
  /// 调用方的整文件覆盖不会抹掉对方未上传的写。返回 `false` 时不留下任何登记。
  fn claim_segment_inflight(&self, owner: u64, tbl: &TableIdent, file_id: FileId) -> bool {
    let mut map = self.busy_guard();
    let file_map = map.entry(tbl.clone()).or_default();
    let entry = file_map.entry(file_id).or_default();
    let foreign = entry.foreign_busy(owner);
    if !foreign {
      *entry.inflight_owner.entry(owner).or_insert(0) += 1;
    }
    if !entry.busy() {
      file_map.remove(&file_id);
    }
    if file_map.is_empty() {
      map.remove(tbl);
    }
    !foreign
  }

  /// 除 `owner` 之外是否有实例正在写该 segment 文件（在飞或持脏）。
  fn segment_foreign_busy(&self, owner: u64, tbl: &TableIdent, file_id: FileId) -> bool {
    self
      .busy_guard()
      .get(tbl)
      .and_then(|file_map| file_map.get(&file_id))
      .is_some_and(|entry| entry.foreign_busy(owner))
  }

  /// 批量注销某 owner 在一张表下的多份脏登记（一次加锁）。
  ///
  /// 表级清理（detach / `Drop` / `invalidate`）要清 N 份登记，逐个调用
  /// `set_segment_dirty` 会加 N 次锁并在每次里重建 / 删除外层表条目。
  fn clear_segment_dirty_li(&self, owner: u64, tbl: &TableIdent, file_id_li: &[FileId]) {
    if file_id_li.is_empty() {
      return;
    }
    let mut map = self.busy_guard();
    let Some(file_map) = map.get_mut(tbl) else {
      return;
    };
    for file_id in file_id_li {
      let Some(entry) = file_map.get_mut(file_id) else {
        continue;
      };
      entry.dirty_owner_set.remove(&owner);
      if !entry.busy() {
        file_map.remove(file_id);
      }
    }
    if file_map.is_empty() {
      map.remove(tbl);
    }
  }

  /// 登记 / 注销某实例持有的脏 segment 文件。
  fn set_segment_dirty(&self, owner: u64, tbl: &TableIdent, file_id: FileId, dirty: bool) {
    let mut map = self.busy_guard();
    let file_map = map.entry(tbl.clone()).or_default();
    let entry = file_map.entry(file_id).or_default();
    if dirty {
      entry.dirty_owner_set.insert(owner);
    } else {
      entry.dirty_owner_set.remove(&owner);
    }
    if !entry.busy() {
      file_map.remove(&file_id);
    }
    if file_map.is_empty() {
      map.remove(tbl);
    }
  }

  /// 实例销毁时清掉它名下的全部脏登记。
  ///
  /// 共享组比实例长寿时，残留 owner 会让该文件在组内永久 busy，scheme 回填
  /// 永远跳过它。
  fn release_owner(&self, owner: u64) {
    let mut map = self.busy_guard();
    map.retain(|_, file_map| {
      file_map.retain(|_, entry| {
        entry.dirty_owner_set.remove(&owner);
        entry.busy()
      });
      !file_map.is_empty()
    });
  }

  /// 该 segment 文件是否有任一实例正在安装或有未上传脏数据。
  fn segment_busy(&self, tbl: &TableIdent, file_id: FileId) -> bool {
    self
      .busy_guard()
      .get(tbl)
      .and_then(|file_map| file_map.get(&file_id))
      .is_some_and(SegmentBusy::busy)
  }

  /// 尝试取得组级回填权；已有实例在回填同一表时返回 `None`。
  fn try_begin_enable(&self, tbl: &TableIdent) -> Option<EnablingGuard<'_>> {
    let mut set = self.enabling_guard();
    set.insert(tbl.clone()).then(|| EnablingGuard {
      group: self,
      tbl: tbl.clone(),
    })
  }

  /// 锁中毒时继续使用数据：登记表只做保守判定，不参与内存安全。
  fn busy_guard(&self) -> MutexGuard<'_, HashMap<TableIdent, HashMap<FileId, SegmentBusy>>> {
    self
      .segment_busy
      .lock()
      .unwrap_or_else(|poison| poison.into_inner())
  }

  fn enabling_guard(&self) -> MutexGuard<'_, HashSet<TableIdent>> {
    self
      .enabling_tbl_set
      .lock()
      .unwrap_or_else(|poison| poison.into_inner())
  }

  fn publish_guard(&self) -> MutexGuard<'_, HashSet<TableIdent>> {
    self
      .publish_tbl_set
      .lock()
      .unwrap_or_else(|poison| poison.into_inner())
  }

  /// 该表是否有任一实例正在发布本地 manifest。
  fn is_publishing(&self, tbl: &TableIdent) -> bool {
    self.publish_guard().contains(tbl)
  }

  fn reserve(&self, bytes: u64) {
    if bytes > 0 {
      self.resident_bytes.fetch_add(bytes, Ordering::Relaxed);
    }
  }

  fn release(&self, bytes: u64) {
    if bytes == 0 {
      return;
    }
    // 饱和减法一次完成（`try_update` 自 1.95 stable，即本仓 MSRV）。记账对称时
    // 不会下溢；实例销毁未归还等异常路径按 0 饱和，避免整组记账回绕成超大值
    // 后把 cache 全部淘汰。
    let _ = self
      .resident_bytes
      .try_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
        Some(used.saturating_sub(bytes))
      });
  }

  /// 记账从 `old` 变为 `new`（新文件 / 文件增长 / 文件缩小）。
  fn adjust(&self, old: u64, new: u64) {
    match new.cmp(&old) {
      cmp::Ordering::Greater => self.reserve(new - old),
      cmp::Ordering::Less => self.release(old - new),
      cmp::Ordering::Equal => {}
    }
  }
}

#[derive(Debug)]
pub struct CloudStore<I, O> {
  local: I,
  remote: O,
  prefix: String,
  pages_per_file_shift: u8,
  segment_size: usize,
  segments_per_file_shift: u8,
  /// 本地 cache 预算组与 owner 租约：默认本实例独占一组；`with_cache_group`
  /// 可与其他实例共享。租约 `Drop` 时注销本实例的脏登记。
  owner: OwnerLease,
  metrics: Option<Arc<ShardMetric>>,
  state: RefCell<CacheState>,
  identity: RefCell<FileIdentity>,
  /// 每表 per-file `(branch, term)` 归属链（对齐 C++ `BranchFileMapping`），
  /// replay 后由 Store 同步；旧 term / 父 branch 的 data 对象按写时 key 读取。
  file_range_map: RefCell<HashMap<TableIdent, Vec<BranchFileRange>>>,
  /// 是否复用启动前已存在的本地 cache（对齐 `allow_reuse_local_caches`）。
  /// `false` 时按表懒恢复会清空该表的本地 data / segment。
  reuse_local_caches: bool,
  /// 只读实例（预热 / 只读副本）：照常安装 segment 文件，但**不参与** scheme
  /// 回填（见 [`Self::enable_segment_marker`] 的跨进程边界）。
  ///
  /// 跨进程共用同一本地 root 时看不到对方的写现场（`CacheGroup` 只在同一
  /// 运行时内可见），回填会把对方半装文件按旧尾槽规则盖成「完整」。多进程
  /// 部署必须让该分区的写实例独占回填，预热 / 只读实例置 `true`。
  read_only: bool,
  /// 已完成本地 cache 记账恢复的表；按表懒扫描，避免每次预热重复列目录。
  restored_table_set: RefCell<HashSet<TableIdent>>,
  /// 已确认**存在**表级 segment scheme 标记的表。
  ///
  /// 只缓存肯定存在的结论；不存在仍每次查盘（共用 `local_root` 的另一实例
  /// 可能刚建立 scheme）。restore 每表要判 N 个段，缓存后 scheme 读取从 N 次
  /// 降到 1 次；`drop` / `invalidate` 删除 scheme 时必须同步移除。
  segment_scheme_set: RefCell<HashSet<TableIdent>>,
  /// 在飞整文件下载；后到者等待同一结果，避免并发重复下载与 `<file>.tmp` 争用。
  inflight_download: RefCell<HashMap<DownloadKey, InflightDownload>>,
  /// 在飞下载世代号分配器：每次登记取一个新世代，旧 owner 完成时按世代核对。
  download_generation: Cell<u64>,
  /// 本实例只是「借用」的本地文件（另一实例 / shard 已写好的 data/segment）。
  /// 驱逐时只注销记账，不 unlink 共享路径：否则 prewarm 会把 shard 尚未 flush
  /// 的脏文件当自己的 clean cache 删掉。
  foreign_file_set: RefCell<HashSet<(TableIdent, bool, FileId)>>,
  /// 驱逐代数：`evict_table` 时递增。owner 在下载开始前取值，落盘 / 登记前
  /// 复核；不一致说明期间发生过驱逐（含本表 detach / Drop），必须撤销本次
  /// 下载结果，不能把已驱逐的表重新 `upsert_resident`（否则 reopen 会按本地
  /// 残留当权威）。全局一个计数而不是按表记，避免为每张表长期保留代数；
  /// 代价是其它表的驱逐会让在飞下载多失败一次（`TryAgain`，可重试）。
  evict_epoch: Cell<u64>,
}

impl<I, O> CloudStore<I, O> {
  pub fn new(local: I, remote: O, prefix: impl Into<String>, pages_per_file_shift: u8) -> Self {
    Self {
      local,
      remote,
      prefix: prefix.into(),
      pages_per_file_shift,
      segment_size: DEFAULT_SEGMENT_SIZE,
      segments_per_file_shift: DEFAULT_SEGMENTS_PER_FILE_SHIFT,
      owner: OwnerLease::new(Arc::new(CacheGroup::new(0))),
      metrics: None,
      state: RefCell::new(CacheState::default()),
      identity: RefCell::new(FileIdentity::default()),
      file_range_map: RefCell::new(HashMap::default()),
      reuse_local_caches: true,
      read_only: false,
      restored_table_set: RefCell::new(HashSet::default()),
      segment_scheme_set: RefCell::new(HashSet::default()),
      inflight_download: RefCell::new(HashMap::default()),
      download_generation: Cell::new(0),
      foreign_file_set: RefCell::new(HashSet::default()),
      evict_epoch: Cell::new(0),
    }
  }

  pub fn with_segment_config(mut self, segment_size: usize, segments_per_file_shift: u8) -> Self {
    self.segment_size = segment_size;
    self.segments_per_file_shift = segments_per_file_shift;
    self
  }

  /// `0` 表示不限制预算。实例独占一组（不与其他 `CloudStore` 共享记账）。
  pub fn with_cache_budget(mut self, cache_budget: u64) -> Self {
    self.owner = OwnerLease::new(Arc::new(CacheGroup::new(cache_budget)));
    self
  }

  /// 与其他实例共享同一预算组：整组共用一个 `limit`，任一实例的驻留 / 淘汰
  /// 都计入同一份全局记账（12d：shard 与运行时侧预热不再各记一份预算）。
  pub fn with_cache_group(mut self, group: Arc<CacheGroup>) -> Self {
    self.owner = OwnerLease::new(group);
    self
  }

  /// 本实例挂的预算组；装配点自检 / 测试用（`Arc::ptr_eq` 证明共享）。
  pub fn cache_group(&self) -> &Arc<CacheGroup> {
    &self.owner.group
  }

  /// 是否复用启动前已存在的本地 cache（对齐 `allow_reuse_local_caches`）。
  ///
  /// 默认复用（`true`）。运行时应按 `KvOption::allow_reuse_local_caches` 显式设置：
  /// `false` 时按表懒恢复会清空该表的本地 data / segment，只从远端重建。
  pub fn with_reuse_local_caches(mut self, allow: bool) -> Self {
    self.reuse_local_caches = allow;
    self
  }

  pub fn reuse_local_caches(&self) -> bool {
    self.reuse_local_caches
  }

  /// 声明本实例是只读实例（预热 / 只读副本）：安装 segment 文件时不启用
  /// 表级 scheme（不做回填）。默认 `false` = 写实例。
  ///
  /// 与 [`Self::with_reuse_local_caches`] 一样由装配点显式接线；跨进程共用
  /// `local_root` 的部署必须对预热 / 只读实例置位，否则回填会按旧尾槽规则
  /// 给对方进程正在写的半装文件盖「完整」标记。
  pub fn with_read_only(mut self, read_only: bool) -> Self {
    self.read_only = read_only;
    self
  }

  pub fn read_only(&self) -> bool {
    self.read_only
  }

  /// 同步 per-file `(branch, term)` 归属链（对齐 `PageIo::set_file_range_li`）。
  ///
  /// 预热在安装 manifest 后自行解析 branch metadata，需要无 `PageIo` 约束的入口。
  pub fn set_file_range(&self, tbl: &TableIdent, range_li: &[BranchFileRange]) {
    let mut map = self.file_range_map.borrow_mut();
    if map.get(tbl).is_none_or(|old| old != range_li) {
      map.insert(tbl.clone(), range_li.to_vec());
    }
  }

  /// 分配一个新的在飞下载世代号。
  fn next_download_generation(&self) -> u64 {
    let generation = self.download_generation.get().wrapping_add(1);
    self.download_generation.set(generation);
    generation
  }

  /// 结束一次在飞下载：移除**本代**登记并唤醒本代等待者（取消路径传 `Err`）。
  ///
  /// 只按 `generation` 命中自己那代：`evict_table` 摘掉登记后同一 key 可能已被
  /// 新一代 owner 接手，旧 owner 完成时删掉它会把旧结果发给新登记的等待者。
  fn finish_download(&self, key: &DownloadKey, generation: u64, outcome: Result<DownloadOutcome>) {
    let waiter_li = {
      let mut map = self.inflight_download.borrow_mut();
      match map.get(key) {
        Some(entry) if entry.generation == generation => {
          // 组内在飞登记由 `begin_segment_install` / `SegmentInflightGuard`
          // 的 RAII 守卫负责（预热直连安装也走那条路），单飞只负责等待者队列。
          map.remove(key).map(|entry| entry.waiter_li)
        }
        _ => None,
      }
    };
    for waiter in waiter_li.into_iter().flatten() {
      // 等待者可能已被取消；发送失败不影响 owner。
      let _ = waiter.send(outcome);
    }
  }

  /// 同一个 `(table, segment, file_id)` 的并发冷读只回源一次。
  ///
  /// 后到者挂起等待 owner 的 `Cached` / `Missing`；owner 被取消时由
  /// [`DownloadGuard`] 唤醒等待者并返回 `TryAgain`，不会永久挂起。
  ///
  /// 返回值第二项表示**本任务是否是 owner**：等待者拿到的 `Cached` 只代表
  /// owner 执行过 fetch，不保证覆盖等待者自己的起点（见
  /// [`Self::ensure_resident`]）。
  async fn single_flight<F, Fut>(
    &self,
    key: DownloadKey,
    fetch: F,
  ) -> Result<(DownloadOutcome, bool)>
  where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<DownloadOutcome>>,
  {
    let (waiter, generation) = {
      let mut map = self.inflight_download.borrow_mut();
      match map.get_mut(&key) {
        Some(entry) => {
          let (tx, rx) = oneshot::channel();
          entry.waiter_li.push(tx);
          (Some(rx), entry.generation)
        }
        None => {
          let generation = self.next_download_generation();
          map.insert(
            key.clone(),
            InflightDownload {
              generation,
              waiter_li: Vec::new(),
            },
          );
          (None, generation)
        }
      }
    };
    if let Some(rx) = waiter {
      // owner 发送的是 `Result<DownloadOutcome>`；先解 oneshot 取消，再传播
      // owner 的回源错误。
      let outcome = rx.await.map_err(|_| KvError::TryAgain)??;
      return Ok((outcome, false));
    }
    let guard = DownloadGuard {
      store: self,
      key,
      generation,
      done: false,
    };
    guard.finish(fetch().await).map(|outcome| (outcome, true))
  }

  pub fn with_metrics(mut self, metrics: Arc<ShardMetric>) -> Self {
    self.metrics = Some(metrics);
    self
  }

  /// 挂在本实例上的指标实例（`None` = 没接线，四项 cache 指标恒 0）。
  ///
  /// 供**装配点自检**用（cld-55）：shard 侧必须把本 shard 的
  /// `Arc<ShardMetric>` 传进来，测试断言 `Arc::ptr_eq` 而不是只看计数 ——
  /// 「没接线」与「cache 一直没命中」在计数上长得一样。
  pub fn metrics(&self) -> Option<&Arc<ShardMetric>> {
    self.metrics.as_ref()
  }

  fn record_cache_hit(&self) {
    if let Some(metrics) = &self.metrics {
      metrics.inc_cache_hit();
    }
  }

  fn record_cache_miss(&self) {
    if let Some(metrics) = &self.metrics {
      metrics.inc_cache_miss();
    }
  }

  fn sync_cache_bytes(&self) {
    if let Some(metrics) = &self.metrics {
      metrics.set_cache_bytes(self.resident_bytes());
    }
  }

  pub fn cache_budget(&self) -> u64 {
    self.owner.limit()
  }

  /// 本地 cache 还能再装多少字节；预算为 0（不限制）时返回 `u64::MAX`。
  /// 对齐 C++ `CloudStoreMgr::LocalCacheRemained()`：预热在下载前用它判断
  /// 下一个文件装不装得下。共享预算组时按**整组**余量计算，预热不会把
  /// shard 已占用的额度再算一遍。
  pub fn cache_remained(&self) -> u64 {
    let limit = self.owner.limit();
    if limit == 0 {
      u64::MAX
    } else {
      limit.saturating_sub(self.owner.resident_bytes())
    }
  }

  pub fn resident_bytes(&self) -> u64 {
    self.state.borrow().resident_bytes
  }

  pub fn resident_file_count(&self) -> usize {
    self
      .state
      .borrow()
      .table_map
      .values()
      .map(TableCache::len)
      .sum()
  }

  /// 待删队列条目数。坏盘时用于确认失败回队有界，避免内存无界增长。
  pub fn pending_delete_count(&self) -> usize {
    self.state.borrow().pending_delete_li.len()
  }

  pub fn local(&self) -> &I {
    &self.local
  }

  pub fn remote(&self) -> &O {
    &self.remote
  }

  pub fn into_parts(self) -> (I, O) {
    (self.local, self.remote)
  }

  /// 清空该表的 resident 记账与 LRU 节点（不触碰本地文件）。
  fn forget_table_cache(&self, tbl: &TableIdent) {
    let mut state = self.state.borrow_mut();
    // 按表嵌套后，detach 直接删一个外层条目；连带摘掉该表在 LRU 链上的节点。
    if let Some(cache) = state.table_map.remove(tbl) {
      // 本实例对该表已无脏文件：清掉组内 owner 标记，别让别的实例回填永久跳过。
      // 一次加锁批量清，避免 O(n) 次 mutex 往返。
      let file_id_li: Vec<FileId> = cache.segment.keys().copied().collect();
      self
        .owner
        .clear_segment_dirty_li(self.owner.id, tbl, &file_id_li);
      let bytes = cache.bytes();
      state.resident_bytes -= bytes;
      self.owner.release(bytes);
      let CacheState {
        table_li,
        free_table_li,
        node_li,
        free_node,
        lru_head,
        lru_tail,
        ..
      } = &mut *state;
      for entry in cache.data.values() {
        if entry.lru_idx != LRU_NIL {
          lru_unlink(node_li, free_node, lru_head, lru_tail, entry.lru_idx);
        }
      }
      for entry in cache.segment.values() {
        if entry.lru_idx != LRU_NIL {
          lru_unlink(node_li, free_node, lru_head, lru_tail, entry.lru_idx);
        }
      }
      free_table_slot(table_li, free_table_li, cache.table_idx);
    }
  }

  /// 借用当前 branch / term；格式化 key 期间持有 `Ref`，不再 clone branch。
  fn identity(&self) -> Ref<'_, FileIdentity> {
    self.identity.borrow()
  }

  /// `<prefix>/<table>.<partition>`，与 C++ `ComposeKey` 的第一段一致。
  fn tbl_prefix(&self, tbl: &TableIdent) -> String {
    let prefix = self.prefix.trim_matches('/');
    if prefix.is_empty() {
      tbl.to_string()
    } else {
      format!("{prefix}/{tbl}")
    }
  }

  /// 根对象 key（term 文件等非分区对象）：`<prefix>/<name>`，空 prefix 时是裸名字。
  ///
  /// 与 [`Self::tbl_prefix`] 同一规则，对齐 C++ `ComposeKeyFromRemote`：空组件直接
  /// 跳过、不加分隔符。此前 term key 用 `format!("{}/{}", ...)`，空 prefix 下会多出
  /// 前导 `/`，与同 bucket 里 data/manifest key 分裂，跨实现（C++ master）互相看不见
  /// 对方的 term 文件、fencing 静默失效。
  fn root_object_key(&self, name: &str) -> String {
    let prefix = self.prefix.trim_matches('/');
    if prefix.is_empty() {
      name.to_owned()
    } else {
      format!("{prefix}/{name}")
    }
  }

  /// 列目录用：`<prefix>/<table>.<partition>/`。
  fn tbl_scan_prefix(&self, tbl: &TableIdent) -> String {
    format!("{}/", self.tbl_prefix(tbl))
  }

  /// manifest 对象列举前缀：`<prefix>/<table>.<partition>/manifest_`。
  ///
  /// 同一 table 目录下 data / segment 对象可能上百万；manifest 选择与剪枝只
  /// 关心 manifest 名字，按该前缀列举避免扫整目录。
  fn manifest_scan_prefix(&self, tbl: &TableIdent) -> String {
    format!("{}manifest_", self.tbl_scan_prefix(tbl))
  }

  fn object_key(&self, tbl: &TableIdent, name: &str) -> String {
    format!("{}/{name}", self.tbl_prefix(tbl))
  }

  /// 按 `file_range_li` 链解析 file id 的写时 `(branch, term)`；链为空或 file id
  /// 超出链尾（当前 term 新分配）时回退到当前 identity。
  fn ranged_name(&self, tbl: &TableIdent, file_id: FileId, segment: bool) -> String {
    let id = self.identity();
    let range_map = self.file_range_map.borrow();
    let (branch, term) = range_map
      .get(tbl)
      .and_then(|range_li| range_ident(range_li, segment, file_id))
      .unwrap_or((&id.branch, id.term));
    if segment {
      fkv_types::segment_file_name(file_id, branch, term)
    } else {
      fkv_types::data_file_name(file_id, branch, term)
    }
  }

  /// data 对象（读 / 删）：`<prefix>/<table>/data_<file_id>_<branch>_<term>`。
  fn file_key(&self, tbl: &TableIdent, file_id: FileId) -> String {
    self.object_key(tbl, &self.ranged_name(tbl, file_id, false))
  }

  /// data 对象（写）：只按当前 identity，绝不回退旧 term key。
  fn write_file_key(&self, tbl: &TableIdent, file_id: FileId) -> String {
    let id = self.identity();
    self.object_key(
      tbl,
      &fkv_types::data_file_name(file_id, &id.branch, id.term),
    )
  }

  /// 当前 active manifest 对象：`<prefix>/<table>/manifest_<branch>_<term>`。
  fn manifest_key(&self, tbl: &TableIdent) -> String {
    let id = self.identity();
    self.object_key(tbl, &fkv_types::manifest_file_name(&id.branch, id.term))
  }

  fn manifest_file_key(&self, tbl: &TableIdent, name: &str) -> String {
    self.object_key(tbl, name)
  }

  /// segment 对象（读 / 删）：`<prefix>/<table>/segment_<file_id>_<branch>_<term>`。
  fn segment_key(&self, tbl: &TableIdent, file_id: FileId) -> String {
    self.object_key(tbl, &self.ranged_name(tbl, file_id, true))
  }

  /// segment 对象（写）：只按当前 identity。
  fn write_segment_key(&self, tbl: &TableIdent, file_id: FileId) -> String {
    let id = self.identity();
    self.object_key(
      tbl,
      &fkv_types::segment_file_name(file_id, &id.branch, id.term),
    )
  }

  fn segment_file_id(&self, segment_fp_id: FilePageId) -> FileId {
    segment_fp_id >> self.segments_per_file_shift
  }

  fn file_id_of(&self, file_page_id: FilePageId) -> FileId {
    file_page_id >> self.pages_per_file_shift
  }

  fn page_size_of(&self, file_page_id: FilePageId, src_len: usize) -> u64 {
    let index = file_page_id & ((1u64 << self.pages_per_file_shift) - 1);
    (index + 1) * src_len as u64
  }

  fn contains(&self, tbl: &TableIdent, file_id: FileId) -> bool {
    self
      .state
      .borrow()
      .table_map
      .get(tbl)
      .is_some_and(|cache| cache.data.contains_key(&file_id))
  }

  /// 本实例记账的 data 文件长度是否覆盖写入起点 `start`。
  ///
  /// 只读内存记账、不 stat：磁盘上真实长度由 [`Self::ensure_resident`] 的
  /// 单飞 owner 复核一次，快路径不必为每个写页付系统调用。
  fn resident_covers(&self, tbl: &TableIdent, file_id: FileId, start: u64) -> bool {
    self
      .state
      .borrow()
      .table_map
      .get(tbl)
      .and_then(|cache| cache.data.get(&file_id))
      .is_some_and(|entry| entry.size >= start)
  }

  /// 该 data / segment 文件是否已在本实例 cache 记账中，读取无需回源。
  ///
  /// 预热据此在磁盘余量判定之前跳过已热文件：本地已有副本时不会新增占用，
  /// 预算已满也不该因此中止整轮。
  pub fn is_file_resident(&self, tbl: &TableIdent, file_id: FileId, segment: bool) -> bool {
    if segment {
      self.segment_contains(tbl, file_id)
    } else {
      self.contains(tbl, file_id)
    }
  }

  /// 该 data 文件是否含未上传区间；脏文件不能从远端回源覆盖。
  fn is_dirty(&self, tbl: &TableIdent, file_id: FileId) -> bool {
    self
      .state
      .borrow()
      .table_map
      .get(tbl)
      .and_then(|cache| cache.data.get(&file_id))
      .is_some_and(|entry| entry.dirty)
  }

  fn touch(&self, tbl: &TableIdent, file_id: FileId) {
    let state = &mut *self.state.borrow_mut();
    let CacheState {
      table_map,
      node_li,
      lru_head,
      lru_tail,
      ..
    } = state;
    let Some(entry) = table_map
      .get_mut(tbl)
      .and_then(|cache| cache.data.get_mut(&file_id))
    else {
      return;
    };
    // 脏文件不在链上（不可淘汰），命中只更新链表位置。
    if entry.lru_idx != LRU_NIL {
      lru_move_tail(node_li, lru_head, lru_tail, entry.lru_idx);
    }
  }

  fn upsert_resident(&self, tbl: &TableIdent, file_id: FileId, size: u64, dirty: bool) {
    // 干净回填的本地文件来自某个远端 key：记下当前 identity，后续 identity
    // 变更后的首个写区间必须整传，否则新 key 前段会成为零洞。
    let clean_ident = (!dirty).then(|| {
      let id = self.identity();
      (id.branch.clone(), id.term)
    });
    let state = &mut *self.state.borrow_mut();
    let CacheState {
      table_map,
      table_li,
      free_table_li,
      node_li,
      free_node,
      lru_head,
      lru_tail,
      resident_bytes,
      ..
    } = state;
    let cache = table_slot(table_map, table_li, free_table_li, tbl);
    let table_idx = cache.table_idx;
    match cache.data.entry(file_id) {
      Entry::Vacant(vac) => {
        let lru_idx = if dirty {
          LRU_NIL
        } else {
          lru_push(
            node_li, free_node, lru_head, lru_tail, table_idx, file_id, false,
          )
        };
        vac.insert(FileState {
          dirty_range_li: Vec::new(),
          dirty_ident: clean_ident,
          dirty_all: false,
          size,
          dirty,
          lru_idx,
        });
        *resident_bytes += size;
        self.owner.reserve(size);
      }
      Entry::Occupied(mut occ) => {
        let entry = occ.get_mut();
        // 本次写入可能只覆盖前几页：驻留字节只能取 max，否则预算低估。
        let old_size = entry.size;
        entry.size = old_size.max(size);
        entry.dirty |= dirty;
        if entry.dirty {
          // 转脏即摘链：dirty 文件不可被淘汰（对齐 C++ 不进 closed_files_）。
          if entry.lru_idx != LRU_NIL {
            lru_unlink(node_li, free_node, lru_head, lru_tail, entry.lru_idx);
            entry.lru_idx = LRU_NIL;
          }
        } else if entry.lru_idx == LRU_NIL {
          entry.lru_idx = lru_push(
            node_li, free_node, lru_head, lru_tail, table_idx, file_id, false,
          );
        } else {
          lru_move_tail(node_li, lru_head, lru_tail, entry.lru_idx);
        }
        *resident_bytes = *resident_bytes - old_size + entry.size;
        self.owner.adjust(old_size, entry.size);
      }
    }
  }

  fn mark_clean(&self, tbl: &TableIdent, file_id: FileId, size: u64) {
    // 本地内容此刻与本次上传的 key 一致；记为「干净副本所属 identity」，
    // 后续 identity 变更后的首个写区间必须整传，否则会在新 key 上留零洞。
    let ident = {
      let id = self.identity();
      (id.branch.clone(), id.term)
    };
    let state = &mut *self.state.borrow_mut();
    let CacheState {
      table_map,
      node_li,
      free_node,
      lru_head,
      lru_tail,
      resident_bytes,
      ..
    } = state;
    let Some(cache) = table_map.get_mut(tbl) else {
      return;
    };
    let table_idx = cache.table_idx;
    let Some(entry) = cache.data.get_mut(&file_id) else {
      return;
    };
    entry.dirty = false;
    entry.dirty_ident = Some(ident);
    entry.dirty_all = false;
    let old_size = replace(&mut entry.size, size);
    if entry.lru_idx == LRU_NIL {
      // 刚上传完成：按最新使用入链尾。
      entry.lru_idx = lru_push(
        node_li, free_node, lru_head, lru_tail, table_idx, file_id, false,
      );
    } else {
      lru_move_tail(node_li, lru_head, lru_tail, entry.lru_idx);
    }
    *resident_bytes = *resident_bytes - old_size + size;
    self.owner.adjust(old_size, size);
  }

  fn mark_dirty_range(&self, tbl: &TableIdent, file_id: FileId, offset: u64, len: u64) {
    if len == 0 {
      return;
    }
    let ident = {
      let id = self.identity();
      (id.branch.clone(), id.term)
    };
    let state = &mut *self.state.borrow_mut();
    let CacheState {
      table_map,
      node_li,
      free_node,
      lru_head,
      lru_tail,
      ..
    } = state;
    let Some(entry) = table_map
      .get_mut(tbl)
      .and_then(|cache| cache.data.get_mut(&file_id))
    else {
      return;
    };
    entry.dirty = true;
    if entry.lru_idx != LRU_NIL {
      lru_unlink(node_li, free_node, lru_head, lru_tail, entry.lru_idx);
      entry.lru_idx = LRU_NIL;
    }
    // identity 变更（换 term / branch）后旧区间属于旧对象 key：不能带到新 key 上，
    // 否则 put_range 会用零前缀覆盖旧 term 数据。旧脏页仍留在本地文件里，
    // 新 identity 的首个对象必须整文件上传（`dirty_all`），只写新区间会让
    // 新对象前段成为零洞。
    if entry.dirty_ident.as_ref() != Some(&ident) {
      // 已有旧 identity 的脏区间 / 干净副本：旧页仍在本地文件里，新 key 必须
      // 整对象上传。全新写入条目（无 ident）的本地内容只由本次写构成，
      // 区间可覆盖。
      if entry.dirty_ident.is_some() {
        entry.dirty_all = true;
      }
      entry.dirty_range_li.clear();
      entry.dirty_ident = Some(ident);
    }
    entry.dirty_range_li.push((offset, len));
  }

  /// 取出待上传区间；`None` 表示必须整对象上传。
  ///
  /// identity 变更 / `dirty_all` 置位 / 区间归属与本次 flush 目标不一致时，
  /// 丢弃区间并返回 `None`，由调用方整文件 PUT 兜底。
  fn take_dirty_range_li(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    ident: &(String, u64),
  ) -> Option<Vec<(u64, u64)>> {
    let mut state = self.state.borrow_mut();
    let Some(entry) = state
      .table_map
      .get_mut(tbl)
      .and_then(|cache| cache.data.get_mut(&file_id))
    else {
      return Some(Vec::new());
    };
    if entry.dirty_all || entry.dirty_ident.as_ref() != Some(ident) {
      entry.dirty_range_li.clear();
      entry.dirty_ident = None;
      entry.dirty_all = false;
      return None;
    }
    let mut range_li = take(&mut entry.dirty_range_li);
    range_li.sort_unstable();
    let mut merged_range_li: Vec<(u64, u64)> = Vec::with_capacity(range_li.len());
    for (start, len) in range_li {
      match merged_range_li.last_mut() {
        Some((last_start, last_len)) if start <= *last_start + *last_len => {
          let end = (*last_start + *last_len).max(start + len);
          *last_len = end - *last_start;
        }
        _ => merged_range_li.push((start, len)),
      }
    }
    Some(merged_range_li)
  }

  /// 上传失败 / 部分跳过时把未覆盖区间还回 dirty 记账，下一轮重试。
  ///
  /// `None` 表示本轮原计划整对象上传：保持空区间即可，下一轮仍会整传。
  fn restore_dirty_range_li(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    range_li: Option<Vec<(u64, u64)>>,
  ) {
    if let Some(range_li) = range_li {
      for (offset, len) in range_li {
        self.mark_dirty_range(tbl, file_id, offset, len);
      }
    }
  }

  fn forget(&self, tbl: &TableIdent, file_id: FileId) {
    let mut state = self.state.borrow_mut();
    {
      let CacheState {
        table_map,
        node_li,
        free_node,
        lru_head,
        lru_tail,
        resident_bytes,
        ..
      } = &mut *state;
      let removed = table_map
        .get_mut(tbl)
        .and_then(|cache| cache.data.remove(&file_id));
      if let Some(entry) = removed {
        *resident_bytes -= entry.size;
        self.owner.release(entry.size);
        if entry.lru_idx != LRU_NIL {
          lru_unlink(node_li, free_node, lru_head, lru_tail, entry.lru_idx);
        }
      }
    }
    drop_if_empty(&mut state, tbl);
  }

  fn dirty_file_li(&self, tbl: &TableIdent) -> Vec<FileId> {
    let mut file_id_li: Vec<FileId> = self
      .state
      .borrow()
      .table_map
      .get(tbl)
      .map(|cache| {
        cache
          .data
          .iter()
          .filter(|(_, entry)| entry.dirty)
          .map(|(file_id, _)| *file_id)
          .collect()
      })
      .unwrap_or_default();
    file_id_li.sort_unstable();
    file_id_li
  }

  fn segment_contains(&self, tbl: &TableIdent, file_id: FileId) -> bool {
    self
      .state
      .borrow()
      .table_map
      .get(tbl)
      .is_some_and(|cache| cache.segment.contains_key(&file_id))
  }

  fn touch_segment(&self, tbl: &TableIdent, file_id: FileId) {
    let state = &mut *self.state.borrow_mut();
    let CacheState {
      table_map,
      node_li,
      lru_head,
      lru_tail,
      ..
    } = state;
    let Some(entry) = table_map
      .get_mut(tbl)
      .and_then(|cache| cache.segment.get_mut(&file_id))
    else {
      return;
    };
    if entry.lru_idx != LRU_NIL {
      lru_move_tail(node_li, lru_head, lru_tail, entry.lru_idx);
    }
  }

  fn upsert_segment_resident(&self, tbl: &TableIdent, file_id: FileId, size: u64, dirty: bool) {
    if dirty {
      self
        .owner
        .set_segment_dirty(self.owner.id, tbl, file_id, true);
    }
    let state = &mut *self.state.borrow_mut();
    let CacheState {
      table_map,
      table_li,
      free_table_li,
      node_li,
      free_node,
      lru_head,
      lru_tail,
      resident_bytes,
      ..
    } = state;
    let cache = table_slot(table_map, table_li, free_table_li, tbl);
    let table_idx = cache.table_idx;
    match cache.segment.entry(file_id) {
      Entry::Vacant(vac) => {
        let lru_idx = if dirty {
          LRU_NIL
        } else {
          lru_push(
            node_li, free_node, lru_head, lru_tail, table_idx, file_id, true,
          )
        };
        vac.insert(SegmentState {
          size,
          dirty,
          lru_idx,
        });
        *resident_bytes += size;
        self.owner.reserve(size);
      }
      Entry::Occupied(mut occ) => {
        let entry = occ.get_mut();
        let old_size = replace(&mut entry.size, size);
        entry.dirty |= dirty;
        if entry.dirty {
          if entry.lru_idx != LRU_NIL {
            lru_unlink(node_li, free_node, lru_head, lru_tail, entry.lru_idx);
            entry.lru_idx = LRU_NIL;
          }
        } else if entry.lru_idx == LRU_NIL {
          entry.lru_idx = lru_push(
            node_li, free_node, lru_head, lru_tail, table_idx, file_id, true,
          );
        } else {
          lru_move_tail(node_li, lru_head, lru_tail, entry.lru_idx);
        }
        *resident_bytes = *resident_bytes - old_size + size;
        self.owner.adjust(old_size, size);
      }
    }
  }

  fn mark_segment_clean(&self, tbl: &TableIdent, file_id: FileId, size: u64) {
    // 先清组内 owner：本地表 / 条目缺失时也必须清，否则该文件在组内永久
    // busy，别的实例的 scheme 回填永远跳过它。
    self
      .owner
      .set_segment_dirty(self.owner.id, tbl, file_id, false);
    let state = &mut *self.state.borrow_mut();
    let CacheState {
      table_map,
      node_li,
      free_node,
      lru_head,
      lru_tail,
      resident_bytes,
      ..
    } = state;
    let Some(cache) = table_map.get_mut(tbl) else {
      return;
    };
    let table_idx = cache.table_idx;
    let Some(entry) = cache.segment.get_mut(&file_id) else {
      return;
    };
    entry.dirty = false;
    let old_size = replace(&mut entry.size, size);
    if entry.lru_idx == LRU_NIL {
      entry.lru_idx = lru_push(
        node_li, free_node, lru_head, lru_tail, table_idx, file_id, true,
      );
    } else {
      lru_move_tail(node_li, lru_head, lru_tail, entry.lru_idx);
    }
    *resident_bytes = *resident_bytes - old_size + size;
    self.owner.adjust(old_size, size);
  }

  fn forget_segment(&self, tbl: &TableIdent, file_id: FileId) {
    self
      .owner
      .set_segment_dirty(self.owner.id, tbl, file_id, false);
    let mut state = self.state.borrow_mut();
    {
      let CacheState {
        table_map,
        node_li,
        free_node,
        lru_head,
        lru_tail,
        resident_bytes,
        ..
      } = &mut *state;
      let removed = table_map
        .get_mut(tbl)
        .and_then(|cache| cache.segment.remove(&file_id));
      if let Some(entry) = removed {
        *resident_bytes -= entry.size;
        self.owner.release(entry.size);
        if entry.lru_idx != LRU_NIL {
          lru_unlink(node_li, free_node, lru_head, lru_tail, entry.lru_idx);
        }
      }
    }
    drop_if_empty(&mut state, tbl);
  }

  fn dirty_segment_li(&self, tbl: &TableIdent) -> Vec<FileId> {
    let mut file_id_li: Vec<FileId> = self
      .state
      .borrow()
      .table_map
      .get(tbl)
      .map(|cache| {
        cache
          .segment
          .iter()
          .filter(|(_, entry)| entry.dirty)
          .map(|(file_id, _)| *file_id)
          .collect()
      })
      .unwrap_or_default();
    file_id_li.sort_unstable();
    file_id_li
  }

  /// 把驻留字节同步压回预算：只做 LRU 摘链 / 记账，不碰本地文件。
  ///
  /// 返回摘链条数；真正的 `remove_file` / `remove_segment_file` 由
  /// [`Self::drain_evictions`] 批量执行，读写路径只在本批超过阈值时触发。
  ///
  /// 共享预算组时按整组用量收敛：别的实例写超预算，本实例也会从自己的 LRU
  /// 摘条目，直到整组回到 `limit` 或本实例无可淘汰项。本实例没有 victim 时
  /// 不越权删别的实例的文件，只把自己刚写入的条目摘回（磁盘短暂多一份，
  /// `drain_evictions` 后回落到预算内）。
  fn trim_cache(&self) -> usize {
    let limit = self.owner.limit();
    if limit == 0 {
      return 0;
    }
    let evicted = {
      let mut state = self.state.borrow_mut();
      let mut evicted = 0;
      while self.owner.resident_bytes() > limit {
        let Some(victim) = take_victim(&mut state) else {
          break;
        };
        // 条目已从本实例摘链：同步清组内本 owner 的脏登记。不清的话 owner
        // 随被淘汰的 file id 永久残留，该文件在组内对别的实例永远 busy
        // （scheme 回填永远跳过），且 file id 轮转时无界增长。
        if victim.segment {
          self
            .owner
            .set_segment_dirty(self.owner.id, &victim.tbl, victim.file_id, false);
        }
        // `take_victim` 已从本实例 resident 扣减，共享记账同步扣减。
        self.owner.release(victim.bytes);
        let foreign_key = (victim.tbl.clone(), victim.segment, victim.file_id);
        if self.foreign_file_set.borrow_mut().remove(&foreign_key) {
          // 借用文件由另一实例拥有：只注销本实例记账，绝不 unlink 共享路径
          // （shard 可能正在写它且尚未 flush）。
          continue;
        }
        // 失败项回队 / 反复淘汰都可能顶到上限：超限不再入队，文件留在本地
        // 由下一次全量清理（flush / 运维 drain）处理，避免内存无界增长。
        if state.pending_delete_li.len() < EVICT_PENDING_FILE_MAX {
          state.pending_delete_bytes += victim.bytes;
          state.evict_seq += 1;
          state.pending_delete_li.push(victim);
        }
        evicted += 1;
      }
      evicted
    };
    if evicted > 0
      && let Some(metrics) = &self.metrics
    {
      for _ in 0..evicted {
        metrics.inc_cache_evict();
      }
    }
    // 与旧路径一致：每次读写后都刷新 gauge，下载 / 写入增长也能及时反映。
    self.sync_cache_bytes();
    evicted
  }

  /// 待删队列是否达到批量删除阈值。
  ///
  /// `evict_seq == drain_seq` 表示这批淘汰已经尝试过删除（失败项已回队），
  /// 没有新的淘汰就等显式 drain 或下一次淘汰再重试，避免坏盘时每个 IO 都重试。
  fn evict_drain_due(&self) -> bool {
    let state = self.state.borrow();
    !state.pending_delete_li.is_empty()
      && state.evict_seq != state.drain_seq
      && state.drain_fail_streak < EVICT_FAIL_PAUSE
      && (state.pending_delete_bytes >= self.owner.limit() >> EVICT_DRAIN_SHIFT
        || state.pending_delete_li.len() >= EVICT_DRAIN_FILE_NUM)
  }

  /// 淘汰对象是否又回到 cache（重新下载 / 写入）：是则取消删除。
  fn is_resident(&self, victim: &CacheVictim) -> bool {
    if victim.segment {
      self.segment_contains(&victim.tbl, victim.file_id)
    } else {
      self.contains(&victim.tbl, victim.file_id)
    }
  }
}

/// 从全局 LRU 头摘下一个最久未用的干净文件并注销记账。
///
/// 链表头即 O(1) victim；摘链后 `resident_bytes` 立即扣减，语义与「已淘汰」
/// 一致，本地文件删除延后到批量 drain。
fn take_victim(state: &mut CacheState) -> Option<CacheVictim> {
  let head = state.lru_head;
  if head == LRU_NIL {
    return None;
  }
  // 链表头节点与表槽位由本结构维护，越界即状态损坏，按无候选处理。
  let node = *state.node_li.get(head as usize)?;
  // `table_li` 槽位在表内条目清空前不会被释放。
  let tbl = state.table_li.get(node.table_idx as usize)?.clone()?;
  let bytes = {
    let CacheState {
      table_map,
      node_li,
      free_node,
      lru_head,
      lru_tail,
      resident_bytes,
      ..
    } = state;
    let cache = table_map.get_mut(&tbl)?;
    let bytes = if node.segment {
      cache.segment.remove(&node.file_id)?.size
    } else {
      cache.data.remove(&node.file_id)?.size
    };
    *resident_bytes -= bytes;
    lru_unlink(node_li, free_node, lru_head, lru_tail, head);
    bytes
  };
  drop_if_empty(state, &tbl);
  Some(CacheVictim {
    tbl,
    file_id: node.file_id,
    segment: node.segment,
    bytes,
  })
}

impl<I: FileIo + ManifestIo, O: ObjectStore> CloudStore<I, O> {
  /// manifest 是 durability anchor：瞬态错误重试到成功 / 空间不足，
  /// 对齐 C++ `SyncFile` 的 `while (err != NoError && err != OssInsufficientStorage)`。
  ///
  /// `bytes` 移交所有权：重试只是 `Bytes` 引用计数 +1，不会每次重发前再复制一遍负载。
  async fn put_manifest_with_retry(&self, key: &str, bytes: Bytes) -> Result<()> {
    let mut delay_ms = MANIFEST_RETRY_INIT_DELAY_MS;
    for attempt in 0..MANIFEST_RETRY_TIMES {
      // 测试注入：模拟远端 manifest 发布持续失败，验证本地追加回滚。
      fkv_failpoint::fail_point!(CLOUD_MANIFEST_PUT);
      match self.remote.put(key, bytes.clone(), None, false).await {
        Ok(_) => return Ok(()),
        Err(err) if manifest_retryable(err) && attempt + 1 < MANIFEST_RETRY_TIMES => {
          Delay::new(Duration::from_millis(delay_ms)).await;
          delay_ms = (delay_ms * 2).min(MANIFEST_RETRY_MAX_DELAY_MS);
        }
        Err(err) => return Err(err),
      }
    }
    Err(KvError::CloudErr)
  }

  /// 批量删除待删队列里的本地文件；失败项回队，记账不丢。
  ///
  /// 删除前复核驻留状态：淘汰后同一文件可能已被重新下载 / 写入，此时必须
  /// 取消删除，否则会误删刚回填的 cache。显式调用（flush / 运维）返回错误；
  /// 热路径走 `drain_evictions_if_due` 只告警，不把清理失败升级成数据面
  /// 读写失败。
  pub async fn drain_evictions(&self) -> Result<()> {
    let (batch, attempted_seq) = {
      let mut state = self.state.borrow_mut();
      state.pending_delete_bytes = 0;
      (take(&mut state.pending_delete_li), state.evict_seq)
    };
    if batch.is_empty() {
      return Ok(());
    }
    let mut failed_li = Vec::new();
    let mut first_err = None;
    for victim in batch {
      if self.is_resident(&victim) {
        continue;
      }
      let result = if victim.segment {
        // 先删段文件，再删完成标记：文件删除失败时标记必须保留（文件仍完整，
        // 下一轮重试）。只删文件不删标记会按淘汰量线性泄漏小文件，分区目录
        // 永远无法回收。
        match self
          .local
          .remove_segment_file(&victim.tbl, victim.file_id)
          .await
        {
          Ok(()) => {
            self
              .local
              .remove_sidecar_file_name(&victim.tbl, &segment_marker_name(victim.file_id))
              .await
          }
          Err(err) => Err(err),
        }
      } else {
        self.local.remove_file(&victim.tbl, victim.file_id).await
      };
      if let Err(err) = result {
        failed_li.push(victim);
        if first_err.is_none() {
          first_err = Some(err);
        }
      }
    }
    {
      let mut state = self.state.borrow_mut();
      // 只把本次快照标记为已尝试；期间新入队的仍会在下一轮触发。
      state.drain_seq = attempted_seq;
      if !failed_li.is_empty() {
        state.drain_fail_streak = state.drain_fail_streak.saturating_add(1);
        // 回队同样受上限约束：坏盘持续失败时丢弃溢出的失败项，只保留有界重试窗口。
        let mut kept_bytes = 0u64;
        for victim in failed_li {
          if state.pending_delete_li.len() >= EVICT_PENDING_FILE_MAX {
            break;
          }
          kept_bytes += victim.bytes;
          state.pending_delete_li.push(victim);
        }
        state.pending_delete_bytes += kept_bytes;
      } else {
        state.drain_fail_streak = 0;
      }
    }
    match first_err {
      Some(err) => Err(err),
      None => Ok(()),
    }
  }

  /// 热路径的批量删除：达到阈值才执行，失败只告警并回队。
  async fn drain_evictions_if_due(&self) {
    if !self.evict_drain_due() {
      return;
    }
    if let Err(err) = self.drain_evictions().await {
      log::warn!("本地 cache 淘汰删除失败，已回队等待重试: {err}");
    }
  }

  /// 写页前保证整文件已在本地：优先信任本地 cache，否则从远端拉整文件。
  ///
  /// `start` 是本次待写页的**起点**偏移。本地文件短于起点说明写到文件内部
  /// 而本地副本缺前段（崩溃残留的半截文件）：直接写入会在中段留下零洞，必须
  /// 删掉后回源重拉。`start <= size` 都放行：起点之后是正常扩展（含追加到
  /// EOF），远端本来也没有这些页，回源只会白拉整个文件。
  ///
  /// 快路径必须比较**记账长度**而不是只 `contains`：冷启动
  /// [`Self::restore_table_cache`] 会把崩溃残留的短文件按实际长度登记为
  /// resident，只判存在会把短文件当完整副本，越界写留零洞；prewarm 也因
  /// 「本地已 resident」不会修。
  ///
  /// 单飞 + 起点复核：等到别的 owner 的 `Cached` 时只保证覆盖**它自己的**
  /// 起点（快路径「本地文件已存在即返回」甚至可能一次回源都没做）。
  /// 因此不能按轮次决定是否回源：只有**本任务作为 owner 且执行了回源**之后，
  /// 才允许「远端确实短于起点 → 按扩展写放行」；否则自己再发起一轮回源。
  async fn ensure_resident(&self, tbl: &TableIdent, file_id: FileId, start: u64) -> Result<()> {
    let key = (tbl.clone(), false, file_id);
    // 是否已经由本任务完成过一次真正的回源（短文件删除 + 远端整文件覆盖）。
    let mut refetched = false;
    // 最多三轮：等待者轮 / 本方回源轮 / 远端确认短于起点后的放行轮。
    for _ in 0..3 {
      if self.resident_covers(tbl, file_id, start) {
        return Ok(());
      }
      // 未完成强制回源前，每一轮都以回源语义执行；完成后才允许短文件放行。
      let refetch = !refetched;
      let (outcome, owner) = self
        .single_flight(key.clone(), || async {
          let epoch = self.evict_epoch.get();
          // 本地短于写起点：记下「需要先删本地」，但**推迟到远端确认存在之后**。
          let mut short_local = false;
          match self.local.file_size(tbl, file_id).await? {
            Some(size) if size >= start => {
              // `file_size` await 期间表可能已被驱逐：已驱逐的表不得重新登记，
              // 否则 reopen 会把本地残留当权威。
              if self.evict_epoch.get() != epoch {
                return Err(KvError::TryAgain);
              }
              self.upsert_resident(tbl, file_id, size, false);
              return Ok(DownloadOutcome::Cached);
            }
            Some(size) => {
              if self.is_dirty(tbl, file_id) {
                // 脏文件只有本地一份，回源会丢掉未上传的写；短于本次写起点时
                // 只能保留文件交给写路径扩展（远端没有可补齐的版本）。
                log::warn!(
                  "脏文件短于写入起点，跳过回源 tbl={tbl} file_id={file_id} size={size} start={start}"
                );
                return Ok(DownloadOutcome::Cached);
              }
              if !refetch {
                // 远端对象本身短于写起点：按扩展写放行，不再重复整文件下载。
                return Ok(DownloadOutcome::Cached);
              }
              short_local = true;
            }
            None => {
              // 记账存在但本地文件已不在：清掉旧长度，避免快路径按过期值放行。
              if self.contains(tbl, file_id) {
                self.forget(tbl, file_id);
                self.sync_cache_bytes();
              }
              if !refetch {
                return Ok(DownloadOutcome::Missing);
              }
            }
          }
          // 先取远端再删本地：远端 `Missing` 时本地前缀页是唯一副本，提前删除
          // 会让调用方从 `start` 落盘，`[size, start)` 留零洞。
          let Some(bytes) = self.remote.get(&self.write_file_key(tbl, file_id)).await? else {
            return Ok(DownloadOutcome::Missing);
          };
          if short_local {
            // 远端已有完整副本：本地短文件是崩溃残留，删掉后整文件覆盖，
            // 否则整写同样会在中段留下零洞。
            self.local.remove_file(tbl, file_id).await?;
            self.forget(tbl, file_id);
            self.sync_cache_bytes();
          }
          self
            .cache_local_file(tbl, file_id, &bytes, epoch)
            .await?;
          Ok(DownloadOutcome::Cached)
        })
        .await?;
      // 脏文件无法从远端补齐（会丢未上传写），第二轮没有意义。
      if outcome == DownloadOutcome::Missing || self.is_dirty(tbl, file_id) {
        return Ok(());
      }
      // 等待者的结果不作数：本方还没回源，下一轮继续强制回源。
      refetched |= owner && refetch;
    }
    // 三轮都只是当 waiter、期间没有任何一次真正的回源：不能报成功，否则写路径
    // 会在中段留下零洞。远端确认短于起点（`refetched`）时按扩展写放行。
    if !refetched && !self.resident_covers(tbl, file_id, start) {
      return Err(KvError::TryAgain);
    }
    Ok(())
  }

  /// 远端整文件回填本地 cache：后端负责 tmp + rename 原子落盘，失败时删掉
  /// 可能留下的短文件。
  ///
  /// CompioStore 的整写走 tmp + rename，崩溃只会留下 `.tmp`；其它后端若
  /// 非原子，失败 / 崩溃可能留下 `file_size().is_some()` 的短文件，这里的
  /// 长度复核与 [`Self::ensure_resident`] 的起点判定会把它当作 miss。
  ///
  /// `epoch` 是下载开始前的 [`Self::evict_epoch`]：表在下载期间被驱逐时，
  /// 落盘结果与记账都必须撤销，否则已 detach / Drop 的表会复活。
  async fn cache_local_file(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    bytes: &[u8],
    epoch: u64,
  ) -> Result<()> {
    if self.evict_epoch.get() != epoch {
      return Err(KvError::TryAgain);
    }
    if let Err(err) = self.local.write_file(tbl, file_id, bytes).await {
      let _ = self.local.remove_file(tbl, file_id).await;
      self.forget(tbl, file_id);
      self.sync_cache_bytes();
      return Err(err);
    }
    let len = bytes.len() as u64;
    // 后端若未实现原子写，至少拒绝短写 / 未落盘：`file_size` 为 `None`
    // （写成功但文件不存在）同样按失败处理，绝不能按 `bytes.len()` 登记为
    // 完整 resident —— 否则后续读写不再回源，直到进程重启。
    let size = self.local.file_size(tbl, file_id).await?;
    let err = if self.evict_epoch.get() != epoch {
      Some(KvError::TryAgain)
    } else if size == Some(len) {
      None
    } else {
      Some(KvError::IoFail)
    };
    if let Some(err) = err {
      let _ = self.local.remove_file(tbl, file_id).await;
      self.forget(tbl, file_id);
      self.sync_cache_bytes();
      return Err(err);
    }
    self.upsert_resident(tbl, file_id, len, false);
    Ok(())
  }

  /// 按读路径 key（`file_key`，含区间归属的旧 term）单飞取整文件。
  async fn fetch_data_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<DownloadOutcome> {
    let key = (tbl.clone(), false, file_id);
    self
      .single_flight(key, || async {
        let epoch = self.evict_epoch.get();
        // owner 可能已被前一个等待者取代；先看本地是否已装入。
        if self.local.file_size(tbl, file_id).await?.is_some() {
          return Ok(DownloadOutcome::Cached);
        }
        let Some(bytes) = self.remote.get(&self.file_key(tbl, file_id)).await? else {
          return Ok(DownloadOutcome::Missing);
        };
        self.cache_local_file(tbl, file_id, &bytes, epoch).await?;
        Ok(DownloadOutcome::Cached)
      })
      .await
      .map(|(outcome, _)| outcome)
  }

  /// 把某个 table 的 dirty data 文件整文件上传。
  async fn flush_tbl(&self, tbl: &TableIdent) -> Result<()> {
    for file_id in self.dirty_file_li(tbl) {
      let Some(bytes) = self.local.read_file(tbl, file_id).await? else {
        // 本地文件已不在：脏标记永远无法完成上传，继续循环会卡死整表 flush。
        // 丢弃该条目并告警，其余表 / 条目照常处理。
        log::warn!("flush 跳过本地缺失的脏文件 tbl={tbl} file_id={file_id}");
        self.forget(tbl, file_id);
        self.sync_cache_bytes();
        continue;
      };
      // `Vec<u8> -> Bytes` 零拷贝；后续整对象上传 / 区间切片都只借用同一块内存。
      let bytes = Bytes::from(bytes);
      let len = bytes.len() as u64;
      let key = self.write_file_key(tbl, file_id);
      let ident = {
        let id = self.identity();
        (id.branch.clone(), id.term)
      };
      let range_li = self.take_dirty_range_li(tbl, file_id, &ident);
      if range_li.as_ref().is_none_or(|range_li| range_li.is_empty())
        || !self.remote.supports_put_range()
      {
        // 无区间信息 / identity 变更 / 后端不支持区间写：整对象一次上传
        // （对齐 C++ 每文件一次 PUT）。
        if let Err(err) = self.remote.put(&key, bytes, None, false).await {
          // 区间已从记账摘除，失败必须归还，否则后续 flush 以为无脏数据。
          self.restore_dirty_range_li(tbl, file_id, range_li);
          return Err(err);
        }
      } else {
        let range_li = range_li.unwrap_or_default();
        let mut covered = 0usize;
        let mut uncovered_range_li = Vec::new();
        for (index, (start, range_len)) in range_li.iter().enumerate() {
          let start_usize = *start as usize;
          let end = start_usize
            .saturating_add(*range_len as usize)
            .min(bytes.len());
          if start_usize >= end {
            uncovered_range_li.push((*start, *range_len));
            continue;
          }
          if let Err(err) = self
            .remote
            .put_range(&key, start_usize as u64, &bytes[start_usize..end])
            .await
          {
            uncovered_range_li.push((*start, *range_len));
            uncovered_range_li.extend(range_li[index + 1..].iter().copied());
            self.restore_dirty_range_li(tbl, file_id, Some(uncovered_range_li));
            return Err(err);
          }
          covered += end - start_usize;
        }
        if covered == 0 {
          // 区间全落在文件实际长度之外：整对象上传已覆盖全部数据。
          if let Err(err) = self.remote.put(&key, bytes, None, false).await {
            self.restore_dirty_range_li(tbl, file_id, None);
            return Err(err);
          }
        } else if !uncovered_range_li.is_empty() {
          // 有区间没上传成功（越界）：不能标 clean，归还后下轮整写兜底。
          self.restore_dirty_range_li(tbl, file_id, Some(uncovered_range_li));
          continue;
        }
      }
      self.mark_clean(tbl, file_id, len);
    }
    self.trim_cache();
    self.drain_evictions().await
  }
}

impl<I: SegmentIo, O> CloudStore<I, O> {
  /// 每个 segment 文件的定长槽位数。
  fn segment_slot_num(&self) -> Result<usize> {
    if self.segment_size == 0 || self.segments_per_file_shift >= 63 {
      return Err(KvError::InvalidArgs);
    }
    1usize
      .checked_shl(u32::from(self.segments_per_file_shift))
      .ok_or(KvError::InvalidArgs)
  }

  /// segment 文件名义字节数（按定长槽位计，与本地后端 preallocate 一致）。
  fn segment_file_bytes(&self) -> Result<u64> {
    Ok(self.segment_slot_num()? as u64 * self.segment_size as u64)
  }

  async fn assemble_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Vec<u8>> {
    let slot_num = self.segment_slot_num()?;
    let size = self.segment_size;
    let mut bytes = vec![0u8; slot_num * size];
    let base = file_id << self.segments_per_file_shift;
    // 直接读进目标切片：缺槽保持零，不再每槽分配一份临时缓冲（原实现
    // slot_num 次分配 + 2x 峰值内存）。
    for (index, chunk) in bytes.chunks_exact_mut(size).enumerate() {
      match self
        .local
        .read_segment(tbl, base + index as u64, chunk)
        .await
      {
        Ok(()) => {}
        Err(KvError::ResourceMissing) => chunk.fill(0),
        Err(err) => return Err(err),
      }
    }
    Ok(bytes)
  }
}

impl<I: ManifestIo, O> CloudStore<I, O> {
  /// 本表是否已启用显式完成标记；「存在」按表缓存一次（见 `segment_scheme_set`）。
  async fn segment_scheme_enabled(&self, tbl: &TableIdent) -> Result<bool> {
    if self.segment_scheme_set.borrow().contains(tbl) {
      return Ok(true);
    }
    let present = self
      .segment_marker_present(tbl, SEGMENT_MARKER_PREFIX)
      .await?;
    if present {
      self.segment_scheme_set.borrow_mut().insert(tbl.clone());
    }
    Ok(present)
  }

  /// 某旁路标记文件是否存在且内容为约定魔数（脏 / 截断标记一律不算数）。
  async fn segment_marker_present(&self, tbl: &TableIdent, name: &str) -> Result<bool> {
    Ok(
      self
        .local
        .read_manifest_file(tbl, name)
        .await?
        .is_some_and(|bytes| bytes.as_slice() == SEGMENT_MARKER_MAGIC),
    )
  }

  /// 写某 segment 文件的完成标记（槽位全部落盘后调用）。
  async fn write_segment_marker(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self
      .local
      .write_manifest_file(tbl, &segment_marker_name(file_id), SEGMENT_MARKER_MAGIC)
      .await
  }

  /// 删某 segment 文件的完成标记（删文件 / 变体时连带调用，幂等）。
  ///
  /// 标记只落本地（从不随 manifest 上传），走 `local` 的旁路删除接口，
  /// 不触达远端对象。
  async fn remove_segment_marker(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self
      .local
      .remove_sidecar_file_name(tbl, &segment_marker_name(file_id))
      .await
  }

  /// 清掉本表全部 segment 旁路标记（表级 scheme + 单文件标记）。
  ///
  /// `drop` / `invalidate` 必须调用：残留标记会让同 `file_id` 复用时半装文件
  /// 被 stale 标记判成完整；分区目录也会因标记文件永远无法回收。
  async fn clear_segment_marker_li(&self, tbl: &TableIdent) -> Result<()> {
    self.segment_scheme_set.borrow_mut().remove(tbl);
    // 标记不匹配 manifest 命名规则，`list_manifest_file_name_li` 看不到，
    // 必须走旁路文件列举（compio 按 manifest 前缀过滤了 manifest 名单）。
    // 标记只在本地：走 `local` 的旁路接口，不触达远端对象。
    for name in self.local.list_sidecar_file_name_li(tbl).await? {
      if is_segment_marker_name(&name) {
        self.local.remove_sidecar_file_name(tbl, &name).await?;
      }
    }
    Ok(())
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 按表懒重建本地 cache 记账（对齐 C++ `RestoreLocalCacheState`）。
  ///
  /// 冷启动后旧进程留下的 data / segment 仍在盘上：不重建记账会让它们既不计预算
  /// 也不参与 LRU，预热还会因为内存态 miss 而重复下载。`allow_reuse_local_caches
  /// == false` 时反向清理该表的本地 data / segment，只从远端重建。
  ///
  /// 同一实例内每表只扫描一次；新写入走正常路径，不重复登记。
  pub async fn restore_table_cache(&self, tbl: &TableIdent) -> Result<()> {
    if self.restored_table_set.borrow().contains(tbl) {
      return Ok(());
    }
    if !self.reuse_local_caches {
      self.invalidate_local_file_li(tbl).await?;
      self.sync_cache_bytes();
      self.restored_table_set.borrow_mut().insert(tbl.clone());
      return Ok(());
    }
    for file_id in self.local.list_file_id_li(tbl).await? {
      if let Some(size) = self.local.file_size(tbl, file_id).await? {
        self.upsert_resident(tbl, file_id, size, false);
      }
    }
    for file_id in self.local.list_segment_file_id_li(tbl).await? {
      if self.segment_file_complete(tbl, file_id).await? {
        self.upsert_segment_resident(tbl, file_id, self.segment_file_bytes()?, false);
      }
    }
    self.trim_cache();
    self.sync_cache_bytes();
    self.restored_table_set.borrow_mut().insert(tbl.clone());
    Ok(())
  }

  /// 批量恢复多个表的本地 cache 记账（启动扫描入口）。
  pub async fn restore_local_cache_state(&self, tbl_li: &[TableIdent]) -> Result<()> {
    for tbl in tbl_li {
      self.restore_table_cache(tbl).await?;
    }
    Ok(())
  }

  /// 预热的磁盘余量判定前，把本地**物理已存在**的文件登记进记账。
  ///
  /// 与 [`Self::restore_table_cache`] 的区别：不做整表扫描、不删任何文件；
  /// 只认调用方已经列举到的文件，用于覆盖「预热期间 shard 新落盘」的窗口。
  /// data 文件按本地实际长度登记（短文件也占磁盘）；segment 只登记完整文件。
  pub async fn adopt_local_file(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    segment: bool,
    expected_size: u64,
  ) -> Result<bool> {
    if self.is_file_resident(tbl, file_id, segment) {
      return Ok(true);
    }
    if segment {
      // 只做单文件完整性判定（读尾槽，O(1)）：预热对每个 segment 文件都会调
      // 本函数，再列一次整表 segment id 会让大批 segment 表退化成 O(N²)
      // 目录扫描。
      if !self.segment_file_complete(tbl, file_id).await? {
        return Ok(false);
      }
      // 文件由另一实例（shard）写好：只借用记账，驱逐时不能 unlink。
      self
        .foreign_file_set
        .borrow_mut()
        .insert((tbl.clone(), true, file_id));
      self.upsert_segment_resident(
        tbl,
        file_id,
        expected_size.max(self.segment_file_bytes()?),
        false,
      );
      return Ok(true);
    }
    let Some(size) = self.local.file_size(tbl, file_id).await? else {
      return Ok(false);
    };
    // 借用 shard 已写文件：驱逐只注销记账，不删共享路径。
    self
      .foreign_file_set
      .borrow_mut()
      .insert((tbl.clone(), false, file_id));
    self.upsert_resident(tbl, file_id, size, false);
    // 短文件只登记实际占用，不当作预热命中（是否回源由调用方决定，预热不覆写）。
    Ok(size >= expected_size)
  }

  /// 按读路径 key（`segment_key`）单飞取整文件。
  async fn fetch_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<DownloadOutcome> {
    let key = (tbl.clone(), true, file_id);
    self
      .single_flight(key, || async {
        // 回填权 / 组内在飞登记前移到 **下载之前**：抢不到时立刻失败，
        // 不会先白下载整个对象再丢弃。
        let claim = self.begin_segment_install(tbl, file_id).await?;
        let epoch = self.evict_epoch.get();
        let Some(bytes) = self.remote.get(&self.segment_key(tbl, file_id)).await? else {
          return Ok(DownloadOutcome::Missing);
        };
        self.install_claimed(tbl, file_id, &bytes, epoch).await?;
        drop(claim);
        Ok(DownloadOutcome::Cached)
      })
      .await
      .map(|(outcome, _)| outcome)
  }

  /// 安装前的组内独占权：登记在飞 + 判定他人 busy + 确保表级 scheme 已启用。
  ///
  /// 顺序要求：**先**抢回填权 / 登记，**再**下载整对象。回填权落到
  /// `remote.get` 之后才争取的话，抢不到时已经付过一次整文件下载的代价
  /// （压缩的 8 路批量会因此整批 abort）。
  ///
  /// 登记与「他人是否在飞 / 持脏」判定在同一把锁内完成：同组另一实例正在写
  /// 该文件时返回 `TryAgain`，绝不把对方未上传的写覆盖成远端旧值。
  ///
  /// 只读实例（[`Self::with_read_only`]）不落 scheme：跨进程共用本地 root 时
  /// 看不到写实例的现场，回填会把对方半装文件盖成完整。
  async fn begin_segment_install(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
  ) -> Result<SegmentInflightGuard<'_>> {
    let Some(claim) = SegmentInflightGuard::claim(&self.owner.group, self.owner.id, tbl, file_id)
    else {
      return Err(KvError::TryAgain);
    };
    if !self.read_only && !self.segment_scheme_enabled(tbl).await? {
      self.enable_segment_marker(tbl).await?;
    }
    Ok(claim)
  }

  /// 「取独占权 + 安装」的合并入口；生产路径把两步拆开、把独占权放到下载
  /// 之前，仅测试直接调用本函数。
  #[cfg(test)]
  async fn install_segment_file(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    bytes: &[u8],
    epoch: u64,
  ) -> Result<()> {
    let claim = self.begin_segment_install(tbl, file_id).await?;
    self.install_claimed(tbl, file_id, bytes, epoch).await?;
    drop(claim);
    Ok(())
  }

  /// 在已持有 [`SegmentInflightGuard`] 的前提下写槽位并落完成标记。
  ///
  /// 调用方必须已经通过 [`Self::begin_segment_install`] 取得独占权：回填权
  /// 与组内在飞登记都已就位，这里只做复核与落盘。
  async fn install_claimed(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    bytes: &[u8],
    epoch: u64,
  ) -> Result<()> {
    let slot_num = self.segment_slot_num()?;
    let file_bytes = slot_num * self.segment_size;
    // 短对象不能零补齐伪装成合法文件：缺槽说明上传端 / 远端对象损坏。
    if bytes.len() != file_bytes {
      return Err(KvError::Corrupted);
    }
    if self.evict_epoch.get() != epoch {
      return Err(KvError::TryAgain);
    }
    // 下载期间同组另一实例可能才开始写同一文件：**写槽前复核**，绝不把它
    // 正在写的脏段覆盖成远端旧值（否则它 flush 后会把旧值上传成权威）。
    if self.owner.segment_foreign_busy(self.owner.id, tbl, file_id) {
      return Err(KvError::TryAgain);
    }
    let base = file_id << self.segments_per_file_shift;
    // 重新装同一文件前先摘掉旧标记：半装期间不得留着上一轮的成功标记。
    self
      .local
      .remove_sidecar_file_name(tbl, &segment_marker_name(file_id))
      .await?;
    // 逐段写不是原子的：中途失败 / 崩溃会留下「前半段有数据、尾段空洞」的
    // 文件。因此失败时删文件，且调用方必须用 `segment_file_complete` 复核
    // 完成标记，不能只看存在性 / 长度。
    for index in 0..slot_num {
      let start = index * self.segment_size;
      if let Err(err) = self
        .local
        .write_segment(
          tbl,
          base + index as u64,
          &bytes[start..start + self.segment_size],
        )
        .await
      {
        let _ = self.local.remove_segment_file(tbl, file_id).await;
        self.forget_segment(tbl, file_id);
        self.sync_cache_bytes();
        return Err(err);
      }
    }
    // 下载期间表被驱逐：落盘的分段必须撤掉，不能登记进已 detach 的表。
    if self.evict_epoch.get() != epoch {
      let _ = self.local.remove_segment_file(tbl, file_id).await;
      self.forget_segment(tbl, file_id);
      self.sync_cache_bytes();
      return Err(KvError::TryAgain);
    }
    // 槽位写只进页缓存：完成标记是「数据已 durable」的承诺，必须先把本表
    // 段数据 fdatasync 落盘再写标记。否则崩溃后标记在、段数据未落盘（定长
    // 后端尾槽可读零），`restore_table_cache` 按标记判完整并登记 clean，读
    // 本地零洞，后续 flush 再把零上传成云端权威。
    self.local.sync_data(tbl).await?;
    // 全部槽位落盘且表未被驱逐：最后一步写完成标记（原子整写）。
    self.write_segment_marker(tbl, file_id).await?;
    self.upsert_segment_resident(tbl, file_id, file_bytes as u64, false);
    Ok(())
  }

  /// 标记本表使用显式 segment 完成标记（表级 scheme）。
  ///
  /// 只有出现过新格式写入的表，缺失单文件标记才表示「半装」；旧目录没有这个
  /// scheme 标记，继续按尾槽规则判定，保持 on-disk 兼容。
  ///
  /// 首次启用时**先给盘上已完整的旧段回填单文件标记，再落 scheme**：
  /// 顺序反过来的话，旧段在新 scheme 下缺标记会被当半装 —— `restore_table_cache`
  /// 不记账（预算低估、预热误报 `DiskFull`），首次写还要整包重下。回填用旧规则
  /// （尾槽可读即完整），与升级前判定一致，不改变半装文件的既有归类。
  /// 先回填后落 scheme：中途崩溃时盘上仍无 scheme，标记被忽略，行为回到旧目录。
  ///
  /// 并发安全：同一表同一时刻只允许一个任务回填（表级守卫），且回填**跳过**
  /// 任一共享 `CacheGroup` 实例在飞下载 / 脏的 file id——它们正是另一半装
  /// 文件的现场，按旧尾槽规则（定长段后端首写即 `set_len`，尾槽可读零）会把
  /// 半装文件盖成完整，崩溃后留下「新头 + 旧尾」被当成完整读。
  ///
  /// 跨进程边界：`CacheGroup` 只覆盖同一运行时内的实例。多个进程共用同一本地
  /// root 时看不到对方的写现场，这种部署要求**只由该分区的写实例**启用 scheme。
  /// [`CloudStore::with_read_only`] 置位的实例直接跳过回填（只写自己的单文件
  /// 标记），由写实例稍后的回填补齐 scheme。
  async fn enable_segment_marker(&self, tbl: &TableIdent) -> Result<()> {
    if self.read_only {
      return Ok(());
    }
    // 已确认 scheme 存在：直接返回。
    if self.segment_scheme_set.borrow().contains(tbl) {
      return Ok(());
    }
    // 回填权重落组上：同一 `CacheGroup` 内不同实例（shard / 预热）也只允许
    // 一个回填者；否则两边会各自给对方在写的文件盖「完整」标记。
    //
    // 抢不到回填权时必须 `TryAgain`，不能报成功：调用方 `begin_segment_install`
    // 紧接着会删旧标记、写槽位，此刻盘上还没有 scheme，写入变成「无 scheme
    // 的半装文件」，重启后按旧尾槽规则被当完整；同时与持权者的回填互相盖标记。
    let Some(_guard) = self.owner.try_begin_enable(tbl) else {
      return Err(KvError::TryAgain);
    };
    for file_id in self.local.list_segment_file_id_li(tbl).await? {
      if self.segment_backfill_skip(tbl, file_id) {
        continue;
      }
      // 此刻盘上还没有 scheme，回填采用旧规则（首尾槽都可读）。
      if !self.legacy_segment_complete(tbl, file_id).await? {
        continue;
      }
      // 判定期间另一实例可能刚开始 install（登记在入口）：写标记前复核。
      if self.segment_backfill_skip(tbl, file_id) {
        continue;
      }
      self.write_segment_marker(tbl, file_id).await?;
    }
    self
      .local
      .write_manifest_file(tbl, SEGMENT_MARKER_PREFIX, SEGMENT_MARKER_MAGIC)
      .await?;
    self.segment_scheme_set.borrow_mut().insert(tbl.clone());
    Ok(())
  }

  /// 回填是否跳过该 file id：在飞下载（另一半装现场）或有未上传脏数据。
  ///
  /// 跳过不是漏标记：在飞 install 结束时自己写标记，脏数据由 flush 成功后写。
  fn segment_backfill_skip(&self, tbl: &TableIdent, file_id: FileId) -> bool {
    // 组内登记覆盖同运行时的全部实例（shard / 预热），不只本实例。
    if self.owner.segment_busy(tbl, file_id) {
      return true;
    }
    let inflight = self.inflight_download.borrow();
    if inflight
      .keys()
      .any(|(key_tbl, segment, id)| *segment && *id == file_id && key_tbl == tbl)
    {
      return true;
    }
    drop(inflight);
    self
      .state
      .borrow()
      .table_map
      .get(tbl)
      .and_then(|cache| cache.segment.get(&file_id))
      .is_some_and(|entry| entry.dirty)
  }

  /// 本地 segment 文件状态（只读尾槽一次，不列举目录）。
  ///
  /// 长度不可判：定长段后端首写即 `set_len` 整个文件（尾部补零），半装文件的
  /// 尾槽同样「可读」，读到的零与真实零数据无法区分 —— 只看尾槽会把半装文件
  /// 永久判成完整，之后读零、flush 再把零上传成云端权威。因此以显式完成标记
  /// 为准：
  ///
  /// 1. 首尾槽都不可读 → 文件缺失 `Missing`；
  /// 2. 尾槽不可读但首槽可读 → 截断 `Incomplete`；
  /// 3. 本表未启用标记（旧目录）→ 尾槽可读即 `Complete`，保持兼容；
  /// 4. 本表已启用标记：有单文件标记 → `Complete`，缺标记 → `Incomplete`。
  async fn segment_state(&self, tbl: &TableIdent, file_id: FileId) -> Result<LocalSegmentState> {
    let slot_num = self.segment_slot_num()?;
    // 尾槽可读即「文件存在且按旧规则完整」：旧目录的判定口径必须保持不变，
    // 只写了尾槽的稀疏文件在升级前就是完整，不能因本次改动变成半装。
    if self
      .segment_slot_readable(tbl, file_id, slot_num - 1)
      .await?
    {
      if !self.segment_scheme_enabled(tbl).await? {
        return Ok(LocalSegmentState::Complete);
      }
      return Ok(
        if self
          .segment_marker_present(tbl, &segment_marker_name(file_id))
          .await?
        {
          LocalSegmentState::Complete
        } else {
          LocalSegmentState::Incomplete
        },
      );
    }
    // 尾槽不可读：首槽可读说明文件存在但被截断（崩溃残留），要删掉重装；
    // 两个槽都不可读按缺失处理，直接回源。
    if slot_num > 1 && self.segment_slot_readable(tbl, file_id, 0).await? {
      return Ok(LocalSegmentState::Incomplete);
    }
    Ok(LocalSegmentState::Missing)
  }

  /// 旧规则下的完整判定：尾槽可读即完整（未启用 scheme 时使用）。
  async fn legacy_segment_complete(&self, tbl: &TableIdent, file_id: FileId) -> Result<bool> {
    let slot_num = self.segment_slot_num()?;
    self.segment_slot_readable(tbl, file_id, slot_num - 1).await
  }

  /// 某段槽位是否可读；只读一个段，不列举目录。
  async fn segment_slot_readable(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    index: usize,
  ) -> Result<bool> {
    let mut buf = vec![0u8; self.segment_size];
    let segment_fp_id = (file_id << self.segments_per_file_shift) + index as u64;
    match self.local.read_segment(tbl, segment_fp_id, &mut buf).await {
      Ok(()) => Ok(true),
      Err(KvError::ResourceMissing | KvError::IoFail | KvError::EndOfFile) => Ok(false),
      Err(err) => Err(err),
    }
  }

  /// 本地 segment 文件是否完整。
  async fn segment_file_complete(&self, tbl: &TableIdent, file_id: FileId) -> Result<bool> {
    Ok(self.segment_state(tbl, file_id).await? == LocalSegmentState::Complete)
  }

  /// 本地 segment 文件不完整时删掉（连同完成标记），交给调用方按 cache miss 回源。
  async fn drop_incomplete_segment(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.local.remove_segment_file(tbl, file_id).await?;
    self.remove_segment_marker(tbl, file_id).await?;
    self.forget_segment(tbl, file_id);
    self.sync_cache_bytes();
    Ok(())
  }

  /// 写 segment 前保证整文件在本地：否则只写新段会把同文件旧段冲成空洞。
  async fn ensure_segment_resident(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    if self.segment_contains(tbl, file_id) {
      return Ok(());
    }
    let key = (tbl.clone(), true, file_id);
    self
      .single_flight(key, || async {
        // 回填权与组内在飞登记在 **下载 / 状态判定之前** 取得：判定本身有
        // await，之后再抢权会留下「判完即被对方开写」的覆盖窗口。
        let claim = self.begin_segment_install(tbl, file_id).await?;
        let epoch = self.evict_epoch.get();
        if self.segment_contains(tbl, file_id) {
          return Ok(DownloadOutcome::Cached);
        }
        // 单文件尾槽判定，O(1)：每次新段都列整表 segment id 会让压缩 / 新写
        // 退化成 O(N²)。
        match self.segment_state(tbl, file_id).await? {
          LocalSegmentState::Complete => {
            // 判定 await 期间表可能已被驱逐：已驱逐的表不得重新登记。
            if self.evict_epoch.get() != epoch {
              return Err(KvError::TryAgain);
            }
            self.upsert_segment_resident(tbl, file_id, self.segment_file_bytes()?, false);
            return Ok(DownloadOutcome::Cached);
          }
          // 崩溃残留的半截文件：删掉后按 cache miss 重新拉整文件。
          LocalSegmentState::Incomplete => self.drop_incomplete_segment(tbl, file_id).await?,
          LocalSegmentState::Missing => {}
        }
        let Some(bytes) = self
          .remote
          .get(&self.write_segment_key(tbl, file_id))
          .await?
        else {
          return Ok(DownloadOutcome::Missing);
        };
        self.install_claimed(tbl, file_id, &bytes, epoch).await?;
        drop(claim);
        Ok(DownloadOutcome::Cached)
      })
      .await
      .map(|_| ())
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 整文件上传该 table 的 dirty segment 文件，每个脏文件只组装 / 上传一次。
  async fn flush_segment_tbl(&self, tbl: &TableIdent) -> Result<()> {
    let file_id_li = self.dirty_segment_li(tbl);
    if !file_id_li.is_empty() {
      // 组装的段来自本地页缓存：整表段数据先落盘，再逐个写完成标记。
      // 标记不得早于数据 durability（崩溃后标记在、数据零洞会被读零上传）。
      self.local.sync_data(tbl).await?;
    }
    for file_id in file_id_li {
      let bytes = Bytes::from(self.assemble_segment_file(tbl, file_id).await?);
      let len = bytes.len() as u64;
      self
        .remote
        .put(&self.write_segment_key(tbl, file_id), bytes, None, false)
        .await?;
      // 上传成功后立即写完成标记：本实例本地新建的段文件不走 install 回填，
      // 缺标记会被 `segment_state` 判半装 → 淘汰 / 重启后整文件重下，
      // `restore_table_cache` 不登记（磁盘占用穿透预算）。
      self.write_segment_marker(tbl, file_id).await?;
      self.mark_segment_clean(tbl, file_id, len);
    }
    self.trim_cache();
    self.drain_evictions().await
  }

  /// 先 data 后 segment；两者都落 S3 之后才允许发布 manifest。
  async fn flush_all_tbl(&self, tbl: &TableIdent) -> Result<()> {
    self.flush_tbl(tbl).await?;
    self.flush_segment_tbl(tbl).await
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> SegmentIo for CloudStore<I, O> {
  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    match self.local.read_segment(tbl, segment_fp_id, dst).await {
      Ok(()) => {
        self.touch_segment(tbl, self.segment_file_id(segment_fp_id));
        self.record_cache_hit();
        return Ok(());
      }
      Err(KvError::ResourceMissing) => {
        self.record_cache_miss();
      }
      Err(err) => return Err(err),
    }
    let file_id = self.segment_file_id(segment_fp_id);
    if self.fetch_segment_file(tbl, file_id).await? == DownloadOutcome::Missing {
      return Err(KvError::ResourceMissing);
    }
    let result = self.local.read_segment(tbl, segment_fp_id, dst).await;
    self.trim_cache();
    self.drain_evictions_if_due().await;
    result
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> Result<()> {
    let file_id = self.segment_file_id(segment_fp_id);
    // 入口即登记组内在飞，覆盖整个「补齐 + 本地写 + 标脏」：只在写完后置
    // `dirty` 的话，写 await 期间表上仍无 scheme 时，同组回填会按旧尾槽规则
    // 给半装文件盖「完整」标记。
    let _inflight = SegmentInflightGuard::new(&self.owner.group, self.owner.id, tbl, file_id);
    self.ensure_segment_resident(tbl, file_id).await?;
    // 与 `write_page` 同契约：`ensure_segment_resident` 与本地写都可能 await，
    // 期间表被 detach / Drop 时不得继续登记（`table_slot` 会让已驱逐表复活）。
    let epoch = self.evict_epoch.get();
    self.local.write_segment(tbl, segment_fp_id, src).await?;
    if self.evict_epoch.get() != epoch {
      return Err(KvError::TryAgain);
    }
    // 只标脏：flush_segment_tbl 里每个 segment 文件整份组装一次、上传一次。
    self.upsert_segment_resident(tbl, file_id, self.segment_file_bytes()?, true);
    self.trim_cache();
    self.drain_evictions_if_due().await;
    Ok(())
  }
}

impl<I: ManifestIo + PageIo + FixedSegmentIo + FileIo, O: ObjectStore> FixedSegmentIo
  for CloudStore<I, O>
{
  fn register_fixed_buffer(&self, len: usize) -> Result<FixedBuffer> {
    self.local.register_fixed_buffer(len)
  }

  async fn read_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &mut FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    match self
      .local
      .read_segment_fixed(tbl, segment_fp_id, buf, offset, len)
      .await
    {
      Ok(()) => {
        self.touch_segment(tbl, self.segment_file_id(segment_fp_id));
        self.record_cache_hit();
        return Ok(());
      }
      Err(KvError::ResourceMissing) => {
        self.record_cache_miss();
      }
      Err(err) => return Err(err),
    }
    let file_id = self.segment_file_id(segment_fp_id);
    if self.fetch_segment_file(tbl, file_id).await? == DownloadOutcome::Missing {
      return Err(KvError::ResourceMissing);
    }
    let result = self
      .local
      .read_segment_fixed(tbl, segment_fp_id, buf, offset, len)
      .await;
    self.trim_cache();
    self.drain_evictions_if_due().await;
    result
  }

  async fn write_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    let file_id = self.segment_file_id(segment_fp_id);
    // 同 `write_segment`：登记必须覆盖整个写，不能等写完后才标脏。
    let _inflight = SegmentInflightGuard::new(&self.owner.group, self.owner.id, tbl, file_id);
    self.ensure_segment_resident(tbl, file_id).await?;
    let epoch = self.evict_epoch.get();
    self
      .local
      .write_segment_fixed(tbl, segment_fp_id, buf, offset, len)
      .await?;
    if self.evict_epoch.get() != epoch {
      return Err(KvError::TryAgain);
    }
    self.upsert_segment_resident(tbl, file_id, self.segment_file_bytes()?, true);
    self.trim_cache();
    self.drain_evictions_if_due().await;
    Ok(())
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> PageIo for CloudStore<I, O> {
  fn set_file_range_li(&self, tbl: &TableIdent, range_li: &[BranchFileRange]) {
    // reopen 会重复同步同一份链，内容不变时不重建。
    self.set_file_range(tbl, range_li);
  }

  async fn write_page_li(&self, tbl: &TableIdent, page_li: &[(FilePageId, Vec<u8>)]) -> Result<()> {
    if page_li.is_empty() {
      return Ok(());
    }
    // 逐页确保文件常驻，再把整批交给本地 `write_page_li` 合并写。
    // 同一文件取末端最大的那页的起点：它决定本批写入是否落在本地已有内容之后。
    let mut start_map = HashMap::<FileId, (u64, u64)>::default();
    for (file_page_id, page) in page_li {
      let len = page.len() as u64;
      let end = self.page_size_of(*file_page_id, page.len());
      let span = start_map.entry(self.file_id_of(*file_page_id)).or_default();
      if end > span.1 {
        *span = (end - len, end);
      }
    }
    for (file_id, (start, _)) in start_map {
      self.ensure_resident(tbl, file_id, start).await?;
    }
    // 落盘前取值，落盘后复核：本批写入 await 期间表可能已被驱逐，已驱逐的表
    // 不得重新登记（否则 reopen 会把本地残留当权威）。
    let epoch = self.evict_epoch.get();
    self.local.write_page_li(tbl, page_li).await?;
    if self.evict_epoch.get() != epoch {
      return Err(KvError::TryAgain);
    }
    for (file_page_id, page) in page_li {
      let file_id = self.file_id_of(*file_page_id);
      let size = self.page_size_of(*file_page_id, page.len());
      self.upsert_resident(tbl, file_id, size, true);
      let index = *file_page_id & ((1u64 << self.pages_per_file_shift) - 1);
      self.mark_dirty_range(tbl, file_id, index * page.len() as u64, page.len() as u64);
    }
    self.trim_cache();
    self.drain_evictions_if_due().await;
    Ok(())
  }

  fn set_file_identity(&self, branch: &str, term: u64) {
    *self.identity.borrow_mut() = FileIdentity {
      branch: branch.to_owned(),
      term,
    };
    self.local.set_file_identity(branch, term);
  }

  fn preload_path_li(&self, tbl: &TableIdent) -> usize {
    self.local.preload_path_li(tbl)
  }

  fn invalidate_path_cache(&self) {
    self.local.invalidate_path_cache();
  }

  fn set_verify_checksum(&self, enabled: bool) {
    self.local.set_verify_checksum(enabled);
  }

  fn evict_table(&self, tbl: &TableIdent) {
    self
      .foreign_file_set
      .borrow_mut()
      .retain(|(file_tbl, ..)| file_tbl != tbl);
    // 先推进代数：在飞 owner 落盘 / 登记前会复核，撤销已驱逐表的结果。
    // 全局递增，见 `evict_epoch` 注释。
    self.evict_epoch.set(self.evict_epoch.get().wrapping_add(1));
    // 摘掉该表所有在飞下载并唤醒等待者：owner 完成时不能再把已 detach 的表
    // `upsert_resident`（否则 reopen 会把本地残留当权威）。
    let mut inflight = self.inflight_download.borrow_mut();
    let key_li: Vec<DownloadKey> = inflight
      .keys()
      .filter(|(key_tbl, ..)| key_tbl == tbl)
      .cloned()
      .collect();
    for key in key_li {
      if let Some(entry) = inflight.remove(&key) {
        for waiter in entry.waiter_li {
          let _ = waiter.send(Err(KvError::TryAgain));
        }
      }
    }
    drop(inflight);
    self.forget_table_cache(tbl);
    self.local.evict_table(tbl);
    // 表状态已驱逐，per-file 归属链随 reopen 重新同步，避免残留旧 manifest 视图。
    self.file_range_map.borrow_mut().remove(tbl);
    // 下次打开必须重新扫描本地文件并登记：只清记账不摘「已扫描」标记会让
    // 盘上文件既不计预算也不参与 LRU（预算与驱逐脱钩）。
    self.restored_table_set.borrow_mut().remove(tbl);
  }

  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    let file_id = self.file_id_of(file_page_id);
    let local_err = match self.local.read_page(tbl, file_page_id, dst).await {
      Ok(()) => {
        self.touch(tbl, file_id);
        self.record_cache_hit();
        self.trim_cache();
        self.drain_evictions_if_due().await;
        return Ok(());
      }
      // 本地缺失 / 短读 / checksum 失败都按 cache miss 处理：回源一次。
      Err(err @ (KvError::ResourceMissing | KvError::EndOfFile | KvError::Corrupted)) => err,
      Err(err) => return Err(err),
    };
    self.record_cache_miss();
    // 脏文件含未上传数据：远端没有对应版本，回源会丢写。
    if self.is_dirty(tbl, file_id) {
      return Err(local_err);
    }
    let key = (tbl.clone(), false, file_id);
    let (outcome, _) = self
      .single_flight(key, || async {
        let epoch = self.evict_epoch.get();
        // 只有 owner 清理半截 / 损坏 cache，等待者不碰 owner 正在写的文件。
        self.local.remove_file(tbl, file_id).await?;
        self.forget(tbl, file_id);
        self.sync_cache_bytes();
        let Some(bytes) = self.remote.get(&self.file_key(tbl, file_id)).await? else {
          return Ok(DownloadOutcome::Missing);
        };
        self.cache_local_file(tbl, file_id, &bytes, epoch).await?;
        Ok(DownloadOutcome::Cached)
      })
      .await?;
    if outcome == DownloadOutcome::Missing {
      return Err(KvError::ResourceMissing);
    }
    let result = self.local.read_page(tbl, file_page_id, dst).await;
    // dst 已拿到数据后再淘汰，预算可以严格执行。
    self.trim_cache();
    self.drain_evictions_if_due().await;
    result
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    let file_id = self.file_id_of(file_page_id);
    let size = self.page_size_of(file_page_id, src.len());
    let index = file_page_id & ((1u64 << self.pages_per_file_shift) - 1);
    self
      .ensure_resident(tbl, file_id, index * src.len() as u64)
      .await?;
    let epoch = self.evict_epoch.get();
    self.local.write_page(tbl, file_page_id, src).await?;
    if self.evict_epoch.get() != epoch {
      return Err(KvError::TryAgain);
    }
    self.upsert_resident(tbl, file_id, size, true);
    self.mark_dirty_range(tbl, file_id, index * src.len() as u64, src.len() as u64);
    self.trim_cache();
    self.drain_evictions_if_due().await;
    Ok(())
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.flush_all_tbl(tbl).await?;
    self.local.sync_data(tbl).await
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> ManifestIo for CloudStore<I, O> {
  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    // 先上传 data 文件，再让 manifest 指向它们。
    self.flush_all_tbl(tbl).await?;
    let _publish = PublishGuard::new(&self.owner.group, tbl);
    // 本地 cache 不是 append 的可靠基线：首写窗口 / 被 prewarm 删除后本地
    // 可能为空，而远端已有完整快照。先回源补齐，仍无 manifest 则拒绝 append
    // （否则单条 log 会成为远端首记录，reopen 直接 EndOfFile）。
    let mut old = self.local.read_manifest(tbl).await?;
    if old.is_none() {
      self.read_manifest(tbl).await?;
      old = self.local.read_manifest(tbl).await?;
    }
    // 上面已补齐本地 manifest，缺它就是远端也没有：拒绝 append。
    let old_len = old.as_ref().ok_or(KvError::ResourceMissing)?.len() as u64;
    self.local.append_manifest(tbl, record).await?;
    let Some(bytes) = self.local.read_manifest(tbl).await? else {
      self.local.reset_manifest(tbl, old_len).await?;
      return Err(KvError::ResourceMissing);
    };
    if let Err(err) = self
      .put_manifest_with_retry(&self.manifest_key(tbl), Bytes::from(bytes))
      .await
    {
      // 远端发布失败：本地追加必须回滚，否则下一次成功的 append 会把这条
      // 未提交 record 一并上传；其 FilePageId 已被 allocator 回滚并被后续
      // 批次复用，reopen 会按错误 mapping 读到别页数据。
      let rollback = self.local.reset_manifest(tbl, old_len).await;
      if let Err(rollback_err) = rollback {
        log::warn!(
          target: "fkv_cloud",
          "manifest 上传失败后回滚本地失败 tbl={tbl} err={rollback_err}"
        );
      }
      return Err(err);
    }
    Ok(())
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.flush_all_tbl(tbl).await?;
    let _publish = PublishGuard::new(&self.owner.group, tbl);
    let old = self.local.read_manifest(tbl).await?;
    self.local.switch_manifest(tbl, record).await?;
    if let Err(err) = self
      .put_manifest_with_retry(&self.manifest_key(tbl), Bytes::copy_from_slice(record))
      .await
    {
      // 远端发布失败：本地已切到新快照，必须切回旧快照；否则下一次写以
      // 这份未提交快照为基线追加 log 并上传，把失败的 truncate / archive
      // 提升成远端权威。
      let rollback = match old {
        Some(bytes) => self.local.switch_manifest(tbl, &bytes).await,
        None => self.local.drop_manifest(tbl).await,
      };
      if let Err(rollback_err) = rollback {
        log::warn!(
          target: "fkv_cloud",
          "switch manifest 上传失败后回滚本地失败 tbl={tbl} err={rollback_err}"
        );
      }
      return Err(err);
    }
    Ok(())
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    // 冷启动第一次打开该表：先按本地物理文件重建 cache 记账（按表懒，仅一次）。
    // 否则旧进程留下的文件不计预算、不参与 LRU，还会被预热当成 miss。
    self.restore_table_cache(tbl).await?;
    let local = self.local.read_manifest(tbl).await?;
    // 远端是权威（对齐 C++ `PageManager` Cloud 路径无条件 `RefreshManifest`）：
    // 本地命中也要回源。本地 manifest 文件名与 term 无关（恒为 `manifest_main_0`），
    // 跨 term 接管复用旧目录时会命中上一任的 stale 快照；若直接以它为 replay 基线，
    // 不仅会读到过期 root，后续写回还会把它晋升成远端权威（rev-34）。
    let remote = self.latest_remote_manifest_bytes(tbl).await?;
    match (local, remote) {
      // 本地 cache 与远端当前快照一致：无需改写本地。
      (Some(local), Some(remote)) if remote.as_ref() == local.as_slice() => Ok(Some(local)),
      // 本地缺失或落后：采纳远端并以本地 cache 覆写，避免下一次命中 stale。
      (_, Some(remote)) => {
        self.local.switch_manifest(tbl, &remote).await?;
        Ok(Some(remote.into()))
      }
      // 远端没有 manifest：本地只是 cache，不能继续作为权威基线
      // （对齐 C++ NotFound → `DropLocalManifest`）。返回 None 让 reopen 走
      // ResourceMissing / 重试队列，绝不以 stale 快照懒创建。
      (Some(_), None) => {
        self.local.drop_manifest(tbl).await?;
        Ok(None)
      }
      (None, None) => Ok(None),
    }
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
    // 本地 cache 删除失败不能阻断远端 durability anchor 的删除；收集首错，
    // 继续清标记与远端 manifest。
    let mut first_err = match self.local.drop_manifest(tbl).await {
      Ok(()) => None,
      Err(err) => {
        log::warn!("drop 删除本地 manifest 失败 tbl={tbl}: {err}");
        Some(err)
      }
    };
    // 清掉 segment 旁路标记：残留标记会让分区目录永不回收，也会在 file_id
    // 复用时把新的半装文件误判完整。失败不阻断后续远端 manifest 删除。
    if let Err(err) = self.clear_segment_marker_li(tbl).await {
      log::warn!("drop 清理 segment 标记失败 tbl={tbl}: {err}");
      first_err.get_or_insert(err);
    }
    // 活跃 manifest 名带**写时 term**（`append_manifest` / `switch_manifest`
    // 都走 [`Self::manifest_key`]）。attach 抬高 term 后本 term 可能从未写过该
    // 表，远端权威 manifest 仍在被接管的旧 term；只删 identity 名会落空，
    // reopen 按 `latest_remote_manifest` 又选中它 → 表永久复活。
    //
    // 因此删除**同 branch、无 tag、term 不高于本 identity 的全部** manifest：
    // 只删最新一条会留下更旧 term 的 durability anchor（`manifest{1,3}` +
    // drop@term3 删掉 3 后 reopen 会选中 1）；更高 term 属于新 owner，保留其
    // durability anchor（term fencing）。
    let (branch, term) = {
      let id = self.identity();
      (id.branch.clone(), id.term)
    };
    let own = fkv_types::manifest_file_name(&branch, term);
    let mut name_li: Vec<(u64, String)> = self
      .remote_manifest_name_li(tbl)
      .await?
      .into_iter()
      .filter_map(|name| {
        let (file_branch, file_term, tag) = fkv_types::parse_manifest_file_name(&name)?;
        (tag.is_none() && file_branch == branch.as_str() && file_term <= term)
          .then_some((file_term, name))
      })
      .collect();
    // 本 term 可能从未写过该表：own 名不在远端列举里也要幂等删一次。
    if !name_li.iter().any(|(_, name)| name == &own) {
      name_li.push((term, own));
    }
    // 旧 term 先删、own（本 identity 的最高 term）最后。反过来先删 own，并发
    // 读者会在两次删除之间只看到旧 manifest，采纳后回写把它晋升成新权威；
    // own 最后时中间窗口最多采纳 own 本身，其回写要与最后一步删除竞争。
    name_li.sort_unstable();
    for (_, name) in name_li {
      let key = self.manifest_file_key(tbl, &name);
      // 单条失败不能中断：否则 own 留在远端，reopen 会把已 drop 的表复活。
      // 收集首个错误，继续删到 own，最后统一返回。
      if let Err(err) = self.remote.delete(&key).await {
        log::warn!("drop 删除远端 manifest 失败 tbl={tbl} name={name}: {err}");
        first_err.get_or_insert(err);
        continue;
      }
      // 远端已删；本地同名缓存清理失败不影响删除结果。
      if let Err(err) = self.local.remove_manifest_file(tbl, &name).await {
        log::warn!("本地 manifest 缓存清理失败 tbl={tbl} name={name}: {err}");
      }
    }
    match first_err {
      Some(err) => Err(err),
      None => Ok(()),
    }
  }

  /// 无条件回源远端 active manifest 覆盖本地 cache（采纳外部换主后的状态）。
  ///
  /// 与 [`Self::read_manifest`] 的区别：本地命中也要刷新。远端缺失返回
  /// `ResourceMissing`，交给 `Store::install_external_snapshot` 清本地状态。
  async fn refresh_manifest(&self, tbl: &TableIdent) -> Result<()> {
    // 与 `read_manifest` 同一选择规则：外部换主可能把 manifest 写到更高 term，
    // 固定拼当前 identity 会永远读不到。
    let bytes = self.latest_remote_manifest_bytes(tbl).await?;
    // 外部状态可能追加 / 重排文件：无论远端是否存在，本地 cache 都必须作废，
    // 否则旧文件会被当作有效 cache 跳过远端（远端缺失时也要清，等待新状态）。
    self.invalidate_local_file_li(tbl).await?;
    let Some(bytes) = bytes else {
      return Err(KvError::ResourceMissing);
    };
    self.local.switch_manifest(tbl, &bytes).await?;
    Ok(())
  }

  /// 只删本地 cache，不触碰远端 manifest（对齐 C++ `DropLocalManifest`）。
  async fn drop_local_manifest(&self, tbl: &TableIdent) -> Result<()> {
    self.local.drop_manifest(tbl).await
  }

  async fn append_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    // 分支 manifest 与活跃 manifest 同契约：先落数据，再发布引用它的 manifest。
    self.flush_all_tbl(tbl).await?;
    let _publish = PublishGuard::new(&self.owner.group, tbl);
    // 本地具名 manifest 缺失时先回源；远端也没有就拒绝 append，避免单条 log
    // 成为 branch manifest 首记录。
    let mut old = self.local.read_manifest_file(tbl, name).await?;
    if old.is_none() {
      self.read_manifest_file(tbl, name).await?;
      old = self.local.read_manifest_file(tbl, name).await?;
    }
    let Some(ref old_bytes) = old else {
      return Err(KvError::ResourceMissing);
    };
    let old_len = old_bytes.len() as u64;
    self.local.append_manifest_file(tbl, name, record).await?;
    let Some(bytes) = self.local.read_manifest_file(tbl, name).await? else {
      self.local.reset_manifest_file(tbl, name, old_len).await?;
      return Err(KvError::ResourceMissing);
    };
    if let Err(err) = self
      .put_manifest_with_retry(&self.manifest_file_key(tbl, name), Bytes::from(bytes))
      .await
    {
      let rollback = match old {
        Some(_) => self.local.reset_manifest_file(tbl, name, old_len).await,
        None => self.local.remove_manifest_file(tbl, name).await,
      };
      if let Err(rollback_err) = rollback {
        log::warn!(
          target: "fkv_cloud",
          "branch manifest 上传失败后回滚本地失败 tbl={tbl} name={name} err={rollback_err}"
        );
      }
      return Err(err);
    }
    Ok(())
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self.flush_all_tbl(tbl).await?;
    let _publish = PublishGuard::new(&self.owner.group, tbl);
    let old = self.local.read_manifest_file(tbl, name).await?;
    self.local.write_manifest_file(tbl, name, record).await?;
    if let Err(err) = self
      .put_manifest_with_retry(
        &self.manifest_file_key(tbl, name),
        Bytes::copy_from_slice(record),
      )
      .await
    {
      // 远端发布失败：本地具名快照（branch / archive）必须回滚，否则下一次
      // append 会以失败快照为基线整份上传，把 abort 的批次 / truncate 转正。
      let rollback = match old {
        Some(bytes) => self.local.write_manifest_file(tbl, name, &bytes).await,
        None => self.local.remove_manifest_file(tbl, name).await,
      };
      if let Err(rollback_err) = rollback {
        log::warn!(
          target: "fkv_cloud",
          "具名 manifest 上传失败后回滚本地失败 tbl={tbl} name={name} err={rollback_err}"
        );
      }
      return Err(err);
    }
    Ok(())
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    // 远端是 manifest 名单的唯一权威：本地 cache 可能残留失败发布 / 崩溃前
    // 写入的「幻影」具名 manifest，合并列举会让 archive reopen 选中幻影、
    // 让 prune 多删真实归档。segment 完成标记属旁路 sidecar，不在此列。
    let mut name_li: Vec<String> = self.remote_manifest_name_li(tbl).await?;
    name_li.sort();
    Ok(name_li)
  }

  /// 旁路文件只在本地 cache，绝不上传远端；必须显式下探本地后端。
  async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    self.local.list_sidecar_file_name_li(tbl).await
  }

  async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.local.remove_sidecar_file_name(tbl, name).await
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    // branch / archive 打开同样先恢复该表的记账。
    self.restore_table_cache(tbl).await?;
    // 具名 manifest 也以远端为权威：本地 cache 可能属于旧 term / 旧进程，
    // 命中后直接返回会让 reopen 读到 stale 前缀，下一次 append 再把它整份
    // 发布回远端，永久覆盖别的主机已提交的 record。
    let Some(bytes) = self.remote.get(&self.manifest_file_key(tbl, name)).await? else {
      // 远端已删：本地 cache 同步删除，避免 archive/branch 被“复活”。
      self.local.remove_manifest_file(tbl, name).await?;
      return Ok(None);
    };
    let local = self.local.read_manifest_file(tbl, name).await?;
    if local.as_deref() != Some(bytes.as_ref()) {
      self.local.write_manifest_file(tbl, name, &bytes).await?;
      // 覆盖整文件后重置追加偏移，后续 append 从新长度继续。
      self
        .local
        .reset_manifest_file(tbl, name, bytes.len() as u64)
        .await?;
    }
    Ok(Some(bytes.into()))
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    let local_err = self.local.remove_manifest_file(tbl, name).await.err();
    let remote_result = self.remote.delete(&self.manifest_file_key(tbl, name)).await;
    match (local_err, remote_result) {
      (_, Err(err)) => Err(err),
      (Some(err), Ok(())) => Err(err),
      (None, Ok(())) => Ok(()),
    }
  }

  /// 剪枝被同表当前 term 取代的远端 manifest 对象。
  ///
  /// 保守约束（任一不满足即整轮跳过）：
  /// - `term > 0`（无更旧 term 可删；`main` 在云端同样按 term 命名，参与剪枝）；
  /// - 本实例 identity 与调用参数一致，避免替别的 branch / term 做判定；
  /// - 远端同 branch 无 tag manifest 的最高 term 必须等于 `term`，出现更高 term
  ///   说明另有实例在推进，本轮不删任何对象；
  /// - 只删 `term` 更小者，当前 term 对象永不删除。
  ///
  /// 删除顺序为远端对象 → 本地 cache；任一失败只记录日志，不阻断 GC 收尾。
  async fn prune_superseded_manifest_file_li(
    &self,
    tbl: &TableIdent,
    branch: &str,
    term: u64,
  ) -> Result<usize> {
    if term == 0 {
      return Ok(0);
    }
    {
      let id = self.identity();
      if id.branch != branch || id.term != term {
        return Ok(0);
      }
    }
    // 只信远端真实现状：本地 cache 里的残留名字不能当作对象存在的证据。
    let name_li = self.remote_manifest_name_li(tbl).await?;
    let mut term_li: Vec<(u64, &str)> = name_li
      .iter()
      .filter_map(|name| {
        let (file_branch, file_term, tag) = fkv_types::parse_manifest_file_name(name)?;
        (tag.is_none() && file_branch == branch).then_some((file_term, name.as_str()))
      })
      .collect();
    term_li.sort_unstable_by_key(|(file_term, _)| *file_term);
    // 更高 term 存在时放弃：旧读者可能仍在旧 term 上按 manifest 判定引用集。
    if term_li.last().map(|(file_term, _)| *file_term) != Some(term) {
      return Ok(0);
    }
    let mut removed = 0;
    for (file_term, name) in term_li {
      if file_term >= term {
        continue;
      }
      let key = self.manifest_file_key(tbl, name);
      if let Err(err) = self.remote.delete(&key).await {
        log::warn!("远端 manifest 剪枝失败 tbl={tbl} name={name}: {err}");
        continue;
      }
      removed += 1;
      // 远端已删，本地缓存清理失败不影响结果：下次列举仍以远端为准。
      if let Err(err) = self.local.remove_manifest_file(tbl, name).await {
        log::warn!("本地 manifest 缓存清理失败 tbl={tbl} name={name}: {err}");
      }
    }
    Ok(removed)
  }
}

impl<I, O: ObjectStore> CloudStore<I, O> {
  /// 分页列举远端 manifest 文件名（不合并本地 cache），升序返回。
  async fn remote_manifest_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    let mut name_set = HashSet::default();
    let mut token = None;
    loop {
      let page = self
        .remote
        .list_page(
          &self.manifest_scan_prefix(tbl),
          token.as_deref(),
          CLOUD_LIST_PAGE_KEYS,
        )
        .await?;
      for meta in page.meta_li {
        let Some(name) = meta.key.rsplit('/').next() else {
          continue;
        };
        if fkv_types::parse_manifest_file_name(name).is_some() {
          name_set.insert(name.to_owned());
        }
      }
      match page.next_token {
        Some(next) => token = Some(next),
        None => break,
      }
    }
    let mut name_li: Vec<String> = name_set.into_iter().collect();
    name_li.sort();
    Ok(name_li)
  }

  /// 选出远端当前 branch 的最新 term manifest 并读取内容。
  ///
  /// 对齐 C++ `GetManifest`：跨 term 写入后 manifest 对象名带写时 term，
  /// 只按进程 identity 拼 key 会看不到旧 / 新 term 的对象。这里列举同 branch
  /// 全部无 tag manifest，取最大 term；term 与进程不同的对齐交给
  /// `Store::install_replay_state`（跨 term bump allocator）完成 —— cloud 层
  /// 不改进程 identity term：Rust 的 term 由 attach 的 term 文件 CAS 决定，
  /// cloud 擅自改写会与 Store / term 文件脱节。
  async fn latest_remote_manifest_bytes(&self, tbl: &TableIdent) -> Result<Option<Bytes>> {
    Ok(
      self
        .latest_remote_manifest(tbl)
        .await?
        .map(|(_, bytes)| bytes),
    )
  }

  /// 选远端当前 branch 的最新 term manifest，返回**对象名 + 内容**。
  async fn latest_remote_manifest(&self, tbl: &TableIdent) -> Result<Option<(String, Bytes)>> {
    let Some(name) = self.latest_remote_manifest_name(tbl).await? else {
      return Ok(None);
    };
    let key = self.manifest_file_key(tbl, &name);
    Ok(self.remote.get(&key).await?.map(|bytes| (name, bytes)))
  }

  /// 选远端当前 branch 最新 term 的无 tag manifest 对象名（不发 GET）。
  async fn latest_remote_manifest_name(&self, tbl: &TableIdent) -> Result<Option<String>> {
    // `Ref` 不跨 await 持有。
    let branch = self.identity().branch.clone();
    Ok(
      self
        .remote_manifest_name_li(tbl)
        .await?
        .into_iter()
        .filter_map(|name| {
          let (file_branch, file_term, tag) = fkv_types::parse_manifest_file_name(&name)?;
          (tag.is_none() && file_branch == branch).then_some((file_term, name))
        })
        .max_by_key(|(file_term, _)| *file_term)
        .map(|(_, name)| name),
    )
  }

  /// 分页列某 table 目录下属于当前 branch 的 data / segment 文件
  /// （id + 对象大小）。
  ///
  /// 当前 identity 的 term 直接收；旧 term 对象只有在该 file id 被 manifest
  /// `file_range_li` 引用时才收（对齐 C++ `FindBranchRange`）。只按
  /// `name_term == term` 过滤会漏掉 branch / 旧 term 区间引用的文件，导致这些
  /// 数据永不预热。
  ///
  /// `token` / `next_token` 语义同 [`ObjectStore::list_page`]，
  /// 调用方逐页消费即可，无需全量物化。
  pub async fn remote_file_entry_page(
    &self,
    tbl: &TableIdent,
    segment: bool,
    token: Option<&str>,
    max_keys: usize,
  ) -> Result<FilePage> {
    let kind = if segment { "segment_" } else { "data_" };
    let page = self
      .remote
      .list_page(&self.tbl_scan_prefix(tbl), token, max_keys)
      .await?;
    // 借用放在 await 之后：`Ref` 不跨 await 持有。
    let id = self.identity();
    let (branch, term) = (id.branch.as_str(), id.term);
    let range_map = self.file_range_map.borrow();
    let range_li = range_map.get(tbl).map_or(&[][..], Vec::as_slice);
    let entry_li = page
      .meta_li
      .into_iter()
      .filter_map(|meta| {
        let name = meta.key.rsplit('/').next()?;
        if !name.starts_with(kind) {
          return None;
        }
        let (file_id, name_branch, name_term) = fkv_types::parse_data_file_name(name)?;
        // 当前 branch + 当前 term 走快路径；其余对象必须是 manifest 区间仍引用的
        // 写时 `(branch, term)` —— 显式放行父 branch 与旧 term，否则子分支继承的
        // 数据永远不会被预热。
        let current = name_branch == branch && name_term == term;
        if !current && range_ident(range_li, segment, file_id) != Some((name_branch, name_term)) {
          return None;
        }
        Some(RemoteFileEntry {
          file_id,
          size: meta.size,
        })
      })
      .collect();
    Ok(FilePage {
      entry_li,
      next_token: page.next_token,
    })
  }

  pub async fn remote_data_file_page(
    &self,
    tbl: &TableIdent,
    token: Option<&str>,
    max_keys: usize,
  ) -> Result<FilePage> {
    self
      .remote_file_entry_page(tbl, false, token, max_keys)
      .await
  }

  pub async fn remote_segment_file_page(
    &self,
    tbl: &TableIdent,
    token: Option<&str>,
    max_keys: usize,
  ) -> Result<FilePage> {
    self
      .remote_file_entry_page(tbl, true, token, max_keys)
      .await
  }

  /// 从对象 key 解析分区名。key 形如 `<prefix>/<tbl>.<partition>/<name>`；
  /// 不在本 prefix 下、或第一段目录名不合规（如 term 文件）时返回 `None`。
  fn partition_of_key(&self, key: &str) -> Option<TableIdent> {
    let prefix = self.prefix.trim_matches('/');
    let rest = if prefix.is_empty() {
      key
    } else {
      key.strip_prefix(prefix)?.strip_prefix('/')?
    };
    let dir = rest.split_once('/')?.0;
    let (tbl_name, partition_id) = dir.rsplit_once('.')?;
    TableIdent::new(tbl_name, partition_id.parse().ok()?).ok()
  }

  /// 设置当前 `(branch, term)`：与 [`PageIo::set_file_identity`] 同语义。
  ///
  /// 供预热后台线程在列举远端对象前对齐目标 shard 的 attach 身份——远端
  /// data / segment 对象名里带写时的 branch / term，身份不对会列举不到。
  pub fn set_identity(&self, branch: &str, term: u64)
  where
    I: ManifestIo + PageIo + SegmentIo + FileIo,
  {
    PageIo::set_file_identity(self, branch, term);
  }

  /// 分页列举远端已存在的分区（`{tbl}.{partition}`）。
  ///
  /// 用于 Cloud 模式下「远端已有但本地未 cache」的分区枚举：prewarm 的预热目标、
  /// archive crond 的归档对象都以远端为准。`after` 传上一页的 [`PartitionPage::last`]
  /// 做跨页去重，`token` 语义同 [`ObjectStore::list_page`]。
  pub async fn remote_partition_page(
    &self,
    after: Option<&TableIdent>,
    token: Option<&str>,
    max_keys: usize,
  ) -> Result<PartitionPage> {
    let prefix = self.prefix.trim_matches('/');
    let scan_prefix = if prefix.is_empty() {
      String::new()
    } else {
      format!("{prefix}/")
    };
    let page = self.remote.list_page(&scan_prefix, token, max_keys).await?;
    let mut table_li = Vec::new();
    let mut last = after.cloned();
    for meta in &page.meta_li {
      let Some(tbl) = self.partition_of_key(&meta.key) else {
        continue;
      };
      if last.as_ref() != Some(&tbl) {
        table_li.push(tbl.clone());
        last = Some(tbl);
      }
    }
    Ok(PartitionPage {
      table_li,
      last,
      next_token: page.next_token,
    })
  }

  /// 列全量文件 id（内部逐页翻）。大 bucket 下会物化整个 prefix，
  /// prewarm 等流式场景应改用 [`CloudStore::remote_data_file_page`]。
  ///
  /// 云端对象 key 带写时 `(branch, term)`：列举前缀只到 `data_` / `segment_`，
  /// 再解析对象名，**不按当前 identity 的 term 过滤**，否则旧 term 对象永远
  /// 进不了 GC 候选（云盘垃圾只增不减）。branch 仍按当前 identity 归一：
  /// 跨 branch 的对象由各自 branch 的 GC 轮次处理，避免误删父 / 子 branch 数据。
  async fn remote_file_id_li_of(&self, tbl: &TableIdent, segment: bool) -> Result<Vec<FileId>> {
    let mut variant_li = self.remote_file_variant_li(tbl, segment).await?;
    variant_li.sort_unstable_by_key(|variant| variant.file_id);
    variant_li.dedup_by_key(|variant| variant.file_id);
    Ok(
      variant_li
        .into_iter()
        .map(|variant| variant.file_id)
        .collect(),
    )
  }

  /// 列远端对象变体（rev-39）：对象名解析出 `(file_id, branch, term)`，branch
  /// 仍按当前 identity 过滤（跨 branch 对象留给各自 branch 的 GC 轮次），
  /// **term 不过滤**——旧 term 对象必须进 GC 候选。升序去重。
  async fn remote_file_variant_li(
    &self,
    tbl: &TableIdent,
    segment: bool,
  ) -> Result<Vec<FileVariant>> {
    let kind = if segment { "segment_" } else { "data_" };
    let scan_prefix = format!("{}{kind}", self.tbl_scan_prefix(tbl));
    // `Ref` 不跨 await 持有。
    let branch = self.identity().branch.clone();
    let mut variant_set = HashSet::default();
    let mut token: Option<String> = None;
    loop {
      let page = self
        .remote
        .list_page(&scan_prefix, token.as_deref(), CLOUD_LIST_PAGE_KEYS)
        .await?;
      for meta in page.meta_li {
        let Some(name) = meta.key.rsplit('/').next() else {
          continue;
        };
        let Some((file_id, name_branch, name_term)) = fkv_types::parse_data_file_name(name) else {
          continue;
        };
        if name_branch == branch.as_str() {
          variant_set.insert(FileVariant {
            file_id,
            identity: Some((branch.as_str().into(), name_term)),
          });
        }
      }
      match page.next_token {
        Some(next) => token = Some(next),
        None => break,
      }
    }
    let mut variant_li: Vec<FileVariant> = variant_set.into_iter().collect();
    variant_li.sort_unstable();
    Ok(variant_li)
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 从 manifest 内容采纳 branch metadata 的 per-file 归属链。
  ///
  /// 预热必须先有这条链，才能把父 branch / 旧 term 引用的 data / segment
  /// 也算进目标集合（见 [`Self::remote_file_entry_page`]）。
  fn adopt_manifest_range_li(&self, tbl: &TableIdent, bytes: &[u8]) -> Result<()> {
    let (state, _) =
      fkv_task_reopen::replay_manifest_bytes_with_len(bytes).map_err(|_| KvError::Corrupted)?;
    self.set_file_range(tbl, &state.branch_metadata.file_range_li);
    Ok(())
  }

  /// 确保当前 attach identity 的 manifest 已在本地；返回是否存在。
  ///
  /// 选择口径与 [`Self::latest_remote_manifest_bytes`] 一致：同 branch 最新无 tag
  /// manifest。branch manifest 名字恒为 `manifest_<branch>_0`，不能按 identity
  /// term 精确匹配（换 term 后永远选不到）；main 装成本地活跃 manifest，branch
  /// 装成本地具名文件 `manifest_<branch>_0`。装完立即解析 branch metadata 并
  /// 同步 `file_range_li`，后续分页预热才能覆盖父 branch / 旧 term 文件。
  ///
  /// 本地已有时**仍要取远端最新 term 比对内容**：重启后本地 manifest 可能是
  /// 旧 term 留下的，只按它采纳 `file_range_li` 会漏预热新 term 引用的文件。
  /// 内容不同则以远端为准重装本地（只写本地，不触发上传 / flush）。
  pub async fn prewarm_manifest(&self, tbl: &TableIdent) -> Result<bool> {
    let branch = self.identity().branch.clone();
    let branch_manifest = (branch != MAIN_BRANCH_NAME).then(|| manifest_file_name(&branch, 0));
    let local_bytes = match &branch_manifest {
      Some(name) => self.local.read_manifest_file(tbl, name).await?,
      None => self.local.read_manifest(tbl).await?,
    }
    .map(Bytes::from);
    // 远端最新无 tag manifest（`latest_remote_manifest` 已按当前 branch 过滤，
    // 不再重复 filter）。
    let remote_bytes = self
      .latest_remote_manifest(tbl)
      .await?
      .map(|(_, bytes)| bytes);
    let bytes = match (local_bytes, remote_bytes) {
      (Some(local), Some(remote)) => {
        if self.owner.is_publishing(tbl) {
          // shard 正持有本地新快照发布中：远端还是旧内容，覆盖本地会让发布
          // 成功后远端被“旧快照 + log”替换。本轮预热跳过。
          return Ok(false);
        }
        let local_slice: &[u8] = local.as_ref();
        let remote_slice: &[u8] = remote.as_ref();
        if local_slice.len() > remote_slice.len() && local_slice.starts_with(remote_slice) {
          // 本地是远端的前缀扩展：shard 刚 append 成功、manifest 上传尚在
          // 重试窗口。prewarm 与 shard 共享 local_root，此时用远端覆盖会把
          // 已 ack 的 record 抹掉；保留本地，等上传完成 / 失败回滚。
          local
        } else if local_slice != remote_slice {
          // 旧 term / 内容漂移：以远端最新为准重装，再按新内容采纳区间。
          self
            .install_local_manifest(tbl, branch_manifest.as_deref(), &remote)
            .await?;
          remote
        } else {
          local
        }
      }
      // 远端没有 manifest：发布进行中时本地新快照还没到远端，不能删；否则
      // 本地只是 stale cache，清掉避免 Drop 后预热把整表重新拉回。
      (Some(_), None) => {
        if self.owner.is_publishing(tbl) {
          return Ok(false);
        }
        match &branch_manifest {
          Some(name) => self.local.remove_manifest_file(tbl, name).await?,
          None => self.local.drop_manifest(tbl).await?,
        }
        // 旧 manifest 的 per-file 归属链同样作废：留着会让后续分页预热按旧区间
        // 把已 Drop 的文件重新拉回。
        self.file_range_map.borrow_mut().remove(tbl);
        return Ok(false);
      }
      (None, Some(remote)) => {
        self
          .install_local_manifest(tbl, branch_manifest.as_deref(), &remote)
          .await?;
        remote
      }
      (None, None) => {
        self.file_range_map.borrow_mut().remove(tbl);
        return Ok(false);
      }
    };
    self.adopt_manifest_range_li(tbl, &bytes)?;
    Ok(true)
  }

  /// 只写本地 manifest（不 flush / 不上传）：main 落活跃名，branch 落
  /// `manifest_<branch>_0`。预热实例与 shard 共用 `local_root`，这里刻意
  /// 不走 [`ManifestIo::switch_manifest`] 的 flush + 远端发布路径。
  async fn install_local_manifest(
    &self,
    tbl: &TableIdent,
    branch_manifest: Option<&str>,
    bytes: &[u8],
  ) -> Result<()> {
    match branch_manifest {
      Some(name) => self.local.write_manifest_file(tbl, name, bytes).await,
      None => self.local.switch_manifest(tbl, bytes).await,
    }
  }
}

impl<I: ManifestIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 作废该表的本地 data / segment 缓存与 resident 记账（不触碰远端）。
  async fn invalidate_local_file_li(&self, tbl: &TableIdent) -> Result<()> {
    self.forget_table_cache(tbl);
    self
      .foreign_file_set
      .borrow_mut()
      .retain(|(file_tbl, ..)| file_tbl != tbl);
    self.file_range_map.borrow_mut().remove(tbl);
    // 本地文件已作废（或被外部状态替换）：允许下次打开重新扫描登记。
    self.restored_table_set.borrow_mut().remove(tbl);
    for file_id in self.local.list_file_id_li(tbl).await? {
      self.local.remove_file(tbl, file_id).await?;
    }
    for file_id in self.local.list_segment_file_id_li(tbl).await? {
      self.local.remove_segment_file(tbl, file_id).await?;
    }
    // segment 文件已全部删除，旁路标记必须同步清掉：否则 file_id 复用时
    // stale 标记会把新的半装文件判完整，分区目录也回收不掉。
    self.clear_segment_marker_li(tbl).await?;
    Ok(())
  }

  /// 确保 data 文件已在本地 cache；返回文件是否存在。
  ///
  /// [`Self::read_file`] 本地命中即直接返回，不会用远端对象覆盖本地文件：
  /// 预热实例与 shard 实例共用 `local_root` 时，shard 正在写的脏页不会被
  /// 预热的整文件 tmp+rename 冲掉。
  pub async fn prewarm_data_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<bool> {
    if self.contains(tbl, file_id) {
      return Ok(true);
    }
    Ok(self.read_file(tbl, file_id).await?.is_some())
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 确保 segment 文件已在本地 cache；返回文件是否存在。
  ///
  /// 只安装本地**不存在**的文件。本地已有文件时绝不删除或覆写：
  /// - 完整文件：直接登记 resident；
  /// - 半截文件：可能是另一实例（shard）正在写的段文件，删掉会让它的
  ///   `flush_segment_tbl` 组装出零洞。预热只跳过，修复留给该实例的读路径
  ///   （`ensure_segment_resident` / `read_segment`）。
  ///
  /// 跨实例共享 `local_root` 时没有写锁，因此安装前必须先在共享组里抢占
  /// 独占权（[`Self::begin_segment_install`]）：同组另一实例正在写该文件时
  /// 返回 `TryAgain`，预热的有界重试会在对方写完后再来，绝不覆盖它的脏段。
  pub async fn prewarm_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<bool> {
    if self.segment_contains(tbl, file_id) {
      return Ok(true);
    }
    // 下载前先抢占独占权：回填权（scheme 启用）与在飞登记都在 `remote.get`
    // 之前完成，抢不到时不会白下载整个对象。
    let claim = self.begin_segment_install(tbl, file_id).await?;
    // 单文件尾槽判定，O(1)；本地已有（含半截）文件一律跳过，不抢 shard 的写。
    // 判定在独占权保护下重做一次：上面的 `segment_contains` 与判定之间对方
    // 可能已经写完，旧判定结果不能再用。
    match self.segment_state(tbl, file_id).await? {
      LocalSegmentState::Complete => {
        // 本地已有：可能是 shard 正在写的文件，标记为借用避免本实例驱逐 unlink。
        self
          .foreign_file_set
          .borrow_mut()
          .insert((tbl.clone(), true, file_id));
        self.upsert_segment_resident(tbl, file_id, self.segment_file_bytes()?, false);
        return Ok(true);
      }
      LocalSegmentState::Incomplete => return Ok(true),
      LocalSegmentState::Missing => {}
    }
    // epoch 必须在 `remote.get` **之前**取：get 期间表可能被 `evict_table`
    // 驱逐，若 get 之后才取值，install 会把已 detach 的表重新登记（复活）。
    let epoch = self.evict_epoch.get();
    let Some(bytes) = self.remote.get(&self.segment_key(tbl, file_id)).await? else {
      return Ok(false);
    };
    self.install_claimed(tbl, file_id, &bytes, epoch).await?;
    drop(claim);
    self.trim_cache();
    self.drain_evictions_if_due().await;
    Ok(true)
  }
}

impl<I: ManifestIo + FileIo, O: ObjectStore> FileIo for CloudStore<I, O> {
  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    let mut file_id_set = HashSet::default();
    file_id_set.extend(self.local.list_file_id_li(tbl).await?);
    file_id_set.extend(self.remote_file_id_li_of(tbl, false).await?);
    // 各后端 `list_*` 统一升序（io-41），GC / compact 的遍历因此可复现。
    let mut file_id_li: Vec<FileId> = file_id_set.into_iter().collect();
    file_id_li.sort_unstable();
    Ok(file_id_li)
  }

  async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>> {
    if let Some(bytes) = self.local.read_file(tbl, file_id).await? {
      self.touch(tbl, file_id);
      self.record_cache_hit();
      self.trim_cache();
      self.drain_evictions_if_due().await;
      return Ok(Some(bytes));
    }
    self.record_cache_miss();
    if self.fetch_data_file(tbl, file_id).await? == DownloadOutcome::Missing {
      return Ok(None);
    }
    let bytes = self.local.read_file(tbl, file_id).await?;
    self.trim_cache();
    self.drain_evictions_if_due().await;
    Ok(bytes)
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()> {
    let epoch = self.evict_epoch.get();
    self.local.write_file(tbl, file_id, bytes).await?;
    // 写入 await 期间表可能已被驱逐：已驱逐的表不得重新登记。
    if self.evict_epoch.get() != epoch {
      return Err(KvError::TryAgain);
    }
    self.upsert_resident(tbl, file_id, bytes.len() as u64, true);
    self.trim_cache();
    self.drain_evictions_if_due().await;
    Ok(())
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>> {
    self.local.file_size(tbl, file_id).await
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.local.remove_file(tbl, file_id).await?;
    self.forget(tbl, file_id);
    self.sync_cache_bytes();
    self.remote.delete(&self.file_key(tbl, file_id)).await?;
    Ok(())
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    let mut file_id_set = HashSet::default();
    file_id_set.extend(self.local.list_segment_file_id_li(tbl).await?);
    file_id_set.extend(self.remote_file_id_li_of(tbl, true).await?);
    let mut file_id_li: Vec<FileId> = file_id_set.into_iter().collect();
    file_id_li.sort_unstable();
    Ok(file_id_li)
  }

  /// 全部 data 文件变体（rev-39）：本地 cache 的变体（compio 解析文件名）并上
  /// 远端对象变体（按当前 identity branch 过滤）。同 triple 去重——远端对象的
  /// 本地 cache 副本与远端条目是同一份。
  async fn list_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    let mut variant_set = HashSet::default();
    variant_set.extend(self.local.list_file_variants(tbl).await?);
    variant_set.extend(self.remote_file_variant_li(tbl, false).await?);
    let mut variant_li: Vec<FileVariant> = variant_set.into_iter().collect();
    variant_li.sort_unstable();
    Ok(variant_li)
  }

  async fn list_segment_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    let mut variant_set = HashSet::default();
    variant_set.extend(self.local.list_segment_file_variants(tbl).await?);
    variant_set.extend(self.remote_file_variant_li(tbl, true).await?);
    let mut variant_li: Vec<FileVariant> = variant_set.into_iter().collect();
    variant_li.sort_unstable();
    Ok(variant_li)
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.local.remove_segment_file(tbl, file_id).await?;
    // 文件已删，完成标记必须同步摘除：file_id 复用时 stale 标记会把半装
    // 文件判成完整，分区目录也会因残留标记无法回收。
    self.remove_segment_marker(tbl, file_id).await?;
    self.forget_segment(tbl, file_id);
    self.sync_cache_bytes();
    self.remote.delete(&self.segment_key(tbl, file_id)).await?;
    Ok(())
  }

  /// 只删一个 `(file_id, branch, term)` 物理变体（rev-39）：本地半按文件名精确
  /// 匹配，远端半直接拼对象 key——不再走 `file_key` 的区间解析，同 id 的其它
  /// `(branch, term)` 变体不受影响。
  async fn remove_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .local
      .remove_file_variant(tbl, file_id, branch, term)
      .await?;
    self.forget(tbl, file_id);
    self.sync_cache_bytes();
    let name = fkv_types::data_file_name(file_id, branch, term);
    self.remote.delete(&self.object_key(tbl, &name)).await?;
    Ok(())
  }

  async fn remove_segment_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .local
      .remove_segment_file_variant(tbl, file_id, branch, term)
      .await?;
    // 标记按 file_id 记，不区分变体：保守摘除，剩余的其它变体会按半装重装一次。
    self.remove_segment_marker(tbl, file_id).await?;
    self.forget_segment(tbl, file_id);
    self.sync_cache_bytes();
    let name = fkv_types::segment_file_name(file_id, branch, term);
    self.remote.delete(&self.object_key(tbl, &name)).await?;
    Ok(())
  }

  async fn list_local_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.local.list_file_id_li(tbl).await
  }

  /// 只删本地 cache 的 data 文件：`LocalGc` 不得删除远端对象（对齐 C++
  /// `ExecuteLocalGC`），远端副本留给后续读按需回源。
  async fn remove_local_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.local.remove_file(tbl, file_id).await?;
    self.forget(tbl, file_id);
    self.sync_cache_bytes();
    Ok(())
  }

  async fn list_local_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.local.list_segment_file_id_li(tbl).await
  }

  async fn remove_local_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.local.remove_segment_file(tbl, file_id).await?;
    // LocalGc 删文件必须带上标记：否则空分区目录永远回收不掉。
    self.remove_segment_marker(tbl, file_id).await?;
    self.forget_segment(tbl, file_id);
    self.sync_cache_bytes();
    Ok(())
  }

  /// 本地 cache 的 data 文件变体（不含远端对象）。
  async fn list_local_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    self.local.list_file_variants(tbl).await
  }

  /// 只删本地 cache 的一个物理变体，远端对象不动（对齐 `remove_local_file`）。
  async fn remove_local_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .local
      .remove_file_variant(tbl, file_id, branch, term)
      .await?;
    self.forget(tbl, file_id);
    self.sync_cache_bytes();
    Ok(())
  }

  async fn list_local_segment_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    self.local.list_segment_file_variants(tbl).await
  }

  async fn remove_local_segment_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .local
      .remove_segment_file_variant(tbl, file_id, branch, term)
      .await?;
    self.remove_segment_marker(tbl, file_id).await?;
    self.forget_segment(tbl, file_id);
    self.sync_cache_bytes();
    Ok(())
  }

  /// 转发本地后端的空分区目录回收。
  ///
  /// 默认实现是 no-op，`fkv_shard` 的 `Backend::Cloud` 分支会因此永远删不掉
  /// 本地 cache 目录。回收前由本地后端自行判定目录是否为空（有残留就保留），
  /// 云端不额外删任何文件。
  async fn try_remove_empty_partition_dir(&self, tbl: &TableIdent) -> Result<bool> {
    self.local.try_remove_empty_partition_dir(tbl).await
  }
}

impl<I, O: ObjectStore> TermIo for CloudStore<I, O> {
  async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>> {
    let key = self.root_object_key(&current_term_file_name(branch, shard_id));
    let Some(bytes) = self.remote.get(&key).await? else {
      return Ok(None);
    };
    let text = from_utf8(&bytes).map_err(|_| KvError::Corrupted)?;
    text
      .trim()
      .parse()
      .map(Some)
      .map_err(|_| KvError::Corrupted)
  }

  async fn upsert_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<u64> {
    let key = self.root_object_key(&current_term_file_name(branch, shard_id));
    let value = Bytes::from(term.to_string());
    for _ in 0..TERM_CAS_RETRY_TIMES {
      // 对齐 C++ `UpsertTermFile`：value 与 etag 必须来自同一次 GET，
      // 否则 GET 与 HEAD 之间被推进的 term 会被旧 writer 用新 etag 回退。
      let etag = if let Some((bytes, etag)) = self.remote.get_versioned(&key).await? {
        let text = from_utf8(&bytes).map_err(|_| KvError::Corrupted)?;
        let current: u64 = text.trim().parse().map_err(|_| KvError::Corrupted)?;
        if current > term {
          return Err(KvError::ExpiredTerm);
        }
        if current == term {
          return Ok(term);
        }
        Some(etag)
      } else {
        None
      };
      let result = match &etag {
        Some(etag) => {
          self
            .remote
            .put(&key, value.clone(), Some(etag), false)
            .await
        }
        None => self.remote.put(&key, value.clone(), None, true).await,
      };
      match result {
        Ok(_) => return Ok(term),
        // `is_retryable()` 覆盖 Busy 等瞬时冲突；`AlreadyExists` 是 CAS 创建冲突，
        // 不属于瞬时错误分类，但同样应重新读取 etag 后再试。
        Err(err) if err.is_retryable() || err == KvError::AlreadyExists => continue,
        Err(err) => return Err(err),
      }
    }
    Err(KvError::TryAgain)
  }

  async fn remove_term(&self, branch: &str, shard_id: u32) -> Result<()> {
    let key = self.root_object_key(&current_term_file_name(branch, shard_id));
    match self.remote.delete(&key).await {
      Ok(()) => Ok(()),
      // 对象不存在时 delete 可能返回 NotFound；删除语义幂等。
      Err(KvError::NotFound) => Ok(()),
      Err(err) => Err(err),
    }
  }
}

#[cfg(test)]
mod tests {
  use core::{
    cell::Cell,
    pin::pin,
    task::{Context, Poll},
  };
  use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::{Arc, Mutex},
  };

  use bytes::Bytes as BytesBytes;
  use fkv_error::KvError;
  use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
  use fkv_io_mem::MemStore;
  use fkv_object::{MemObjectStore, ObjectStore};
  use fkv_types::{FileId, FilePageId, TableIdent};
  use futures::{future::poll_fn, task::noop_waker};

  use super::{
    CacheGroup, CloudStore, DownloadOutcome, InflightDownload, PublishGuard, SEGMENT_MARKER_MAGIC,
    SEGMENT_MARKER_PREFIX, manifest_retryable, segment_marker_name,
  };

  #[test]
  fn manifest_retryable_extends_is_retryable_with_cloud_transient() {
    // 超集部分：云端瞬时错误可重试（`is_retryable` 判否，见 fkv_error tests）。
    // 共同子集 TryAgain / Busy / OpenFileLimit 由 fkv_error::retryable_matches_cpp 钉死。
    for err in [KvError::Timeout, KvError::CloudErr] {
      assert!(manifest_retryable(err), "{err:?}");
    }
    // 空间不足 / 数据损坏 / 不存在不可重试。
    for err in [
      KvError::OssInsufficientStorage,
      KvError::OutOfSpace,
      KvError::Corrupted,
      KvError::NotFound,
    ] {
      assert!(!manifest_retryable(err), "{err:?}");
    }
  }

  /// 本轮第一次 poll 主动让出执行权（模拟真实后端 `write_file` 的 IO 挂起点）。
  async fn yield_once(yielded: &Cell<bool>) {
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
  struct YieldWriteStore {
    inner: fkv_io_mem::MemStore,
    yielded: Cell<bool>,
    /// 段写入首次 poll 挂起一次（预热安装中途断言回填跳过）。
    segment_write_yielded: Cell<bool>,
    /// 段读取首次 poll 挂起一次（回填判定中途插入跨实例 busy 登记）。
    segment_read_yielded: Cell<bool>,
  }

  impl YieldWriteStore {
    fn new() -> Self {
      Self {
        inner: MemStore::new(512),
        yielded: Cell::new(true),
        segment_write_yielded: Cell::new(false),
        segment_read_yielded: Cell::new(false),
      }
    }

    fn new_segment_write_yield() -> Self {
      Self {
        yielded: Cell::new(false),
        segment_write_yielded: Cell::new(true),
        ..Self::new()
      }
    }

    fn new_segment_read_yield() -> Self {
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

    async fn remove_segment_file(
      &self,
      tbl: &TableIdent,
      file_id: FileId,
    ) -> fkv_error::Result<()> {
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

    async fn remove_sidecar_file_name(
      &self,
      tbl: &TableIdent,
      name: &str,
    ) -> fkv_error::Result<()> {
      self.inner.remove_sidecar_file_name(tbl, name).await
    }

    async fn read_manifest_file(
      &self,
      tbl: &TableIdent,
      name: &str,
    ) -> fkv_error::Result<Option<Vec<u8>>> {
      self.inner.read_manifest_file(tbl, name).await
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

  /// 第 4 项：连续三轮都只是当 waiter、期间一次回源都没做时必须返回
  /// `TryAgain`，不能报成功（否则写路径中段留零洞）。
  ///
  /// 回退即失败：循环结束直接 `Ok(())` 时写路径会继续落盘。
  #[test]
  fn ensure_resident_waiter_rounds_exhausted_returns_try_again() {
    let tbl = TableIdent::new("waiter", 1).unwrap();
    let cloud = CloudStore::new(
      MemStore::new(512),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let key = (tbl.clone(), false, 0);
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut write = pin!(cloud.write_page(&tbl, 0, &[1u8; 512]));
    for round in 0..3 {
      // 手动扮演一个「登记了但一次回源都没做」的 owner：本轮只能当 waiter。
      cloud.inflight_download.borrow_mut().insert(
        key.clone(),
        InflightDownload {
          generation: cloud.next_download_generation(),
          waiter_li: Vec::new(),
        },
      );
      assert!(
        write.as_mut().poll(&mut cx).is_pending(),
        "第 {round} 轮应挂起等待 owner"
      );
      let waiter_li = cloud
        .inflight_download
        .borrow_mut()
        .remove(&key)
        .expect("在飞登记")
        .waiter_li;
      assert_eq!(waiter_li.len(), 1, "第 {round} 轮应挂上一个等待者");
      for waiter in waiter_li {
        waiter.send(Ok(DownloadOutcome::Cached)).unwrap();
      }
    }
    match write.as_mut().poll(&mut cx) {
      Poll::Ready(Err(KvError::TryAgain)) => {}
      other => panic!("三轮全当 waiter 必须返回 TryAgain，实际 {other:?}"),
    }
    assert_eq!(cloud.resident_file_count(), 0);
  }

  /// 第 2 项：scheme 回填必须跳过在飞下载 / 脏的 file id。
  ///
  /// 回退即失败：不跳过时，回填按旧尾槽规则（定长段后端首写即预分配，
  /// 尾槽可读零）给这些半装现场写「完整」标记。
  #[test]
  fn scheme_backfill_skips_inflight_and_dirty_segment() {
    pollster::block_on(async {
      let tbl = TableIdent::new("backfill_skip", 1).unwrap();
      let cloud = CloudStore::new(
        MemStore::new(512).with_segment_config(4096, 1),
        MemObjectStore::new(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1);
      // 旧目录：段 0 / 1 / 2 都按旧规则完整（首尾槽可读）。
      for file_id in 0..3u64 {
        cloud
          .local
          .write_segment(&tbl, file_id * 2, &[1u8; 4096])
          .await
          .unwrap();
        cloud
          .local
          .write_segment(&tbl, file_id * 2 + 1, &[0u8; 4096])
          .await
          .unwrap();
      }
      // 段 0 在飞（另一个 install 正在写）；段 1 有未上传脏数据。
      cloud.inflight_download.borrow_mut().insert(
        (tbl.clone(), true, 0),
        InflightDownload {
          generation: cloud.next_download_generation(),
          waiter_li: Vec::new(),
        },
      );
      cloud.upsert_segment_resident(&tbl, 1, 8192, true);

      cloud.enable_segment_marker(&tbl).await.unwrap();

      for file_id in [0, 1] {
        assert!(
          !cloud
            .segment_marker_present(&tbl, &segment_marker_name(file_id))
            .await
            .unwrap(),
          "在飞 / 脏段 {file_id} 不得被回填标记"
        );
      }
      assert!(
        cloud
          .segment_marker_present(&tbl, &segment_marker_name(2))
          .await
          .unwrap(),
        "无在飞 / 脏状态的旧段仍必须回填（升级兼容）"
      );
    });
  }

  /// 第 5 项：定长段后端首写即 `set_len` 整文件，半装文件尾槽也可读；
  /// 只按尾槽判定会把半装文件永久当完整。显式完成标记缺失时必须判半装。
  ///
  /// 回退即失败：旧实现只看尾槽，这里会返回 `true`。
  #[test]
  fn half_installed_segment_without_marker_is_incomplete() {
    pollster::block_on(async {
      let tbl = TableIdent::new("segmark", 1).unwrap();
      let cloud = CloudStore::new(
        MemStore::new(512).with_segment_config(4096, 1),
        MemObjectStore::new(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1);
      // 半装现场：本表已启用显式标记（install 先写 scheme），槽 0 已写，
      // 尾槽因预分配可读零，但单文件完成标记还没写。
      cloud
        .local
        .write_manifest_file(&tbl, SEGMENT_MARKER_PREFIX, SEGMENT_MARKER_MAGIC)
        .await
        .unwrap();
      cloud
        .local
        .write_segment(&tbl, 0, &[7u8; 4096])
        .await
        .unwrap();
      cloud
        .local
        .write_segment(&tbl, 1, &[0u8; 4096])
        .await
        .unwrap();
      assert!(
        !cloud.segment_file_complete(&tbl, 0).await.unwrap(),
        "缺完成标记必须判半装"
      );

      // 槽位全部落盘 + 标记写入后才是完整。
      let epoch = cloud.evict_epoch.get();
      cloud
        .install_segment_file(&tbl, 1, &[3u8; 8192], epoch)
        .await
        .unwrap();
      assert!(
        cloud.segment_file_complete(&tbl, 1).await.unwrap(),
        "写完完成标记后必须判完整"
      );
    });
  }

  /// 第 5 项兼容分支：旧目录没有 scheme 标记时继续按尾槽判定。
  #[test]
  fn legacy_segment_without_scheme_keeps_tail_check() {
    pollster::block_on(async {
      let tbl = TableIdent::new("seglegacy", 1).unwrap();
      let cloud = CloudStore::new(
        MemStore::new(512).with_segment_config(4096, 1),
        MemObjectStore::new(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1);
      cloud
        .local
        .write_segment(&tbl, 1, &[0u8; 4096])
        .await
        .unwrap();
      assert!(
        cloud.segment_file_complete(&tbl, 0).await.unwrap(),
        "旧目录（无 scheme 标记）必须保持尾槽判定，on-disk 兼容"
      );
      // 单文件标记名只在该表启用 scheme 后才被读取；旧目录下不参与判定。
      assert_eq!(segment_marker_name(0), "segcomplete_0");
    });
  }

  /// 第 6 项：`write_file` 的本地写入 await 期间表被驱逐，不得再把结果登记
  /// 回已驱逐的表。
  ///
  /// 回退即失败：旧实现会在写完后无条件 `upsert_resident`，驱逐后的表重新
  /// 出现在 resident 记账里（reopen 会把本地残留当权威）。
  #[test]
  fn write_file_after_evict_is_not_registered() {
    let tbl = TableIdent::new("epoch", 1).unwrap();
    let cloud = CloudStore::new(
      YieldWriteStore::new(),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut write = pin!(cloud.write_file(&tbl, 0, &[1u8; 512]));
    assert!(write.as_mut().poll(&mut cx).is_pending(), "应停在本地写入");
    cloud.evict_table(&tbl);
    match write.as_mut().poll(&mut cx) {
      Poll::Ready(Err(KvError::TryAgain)) => {}
      other => panic!("已驱逐的表不得重新登记，应返回 TryAgain，实际 {other:?}"),
    }
    assert_eq!(cloud.resident_file_count(), 0);
  }

  /// 第 3 项：本地有前缀页、远端对象缺失时，`ensure_resident` 必须先取远端；
  /// 远端 `Missing` 时保留本地前缀页，不能提前删除（否则调用方从 `start`
  /// 落盘会在中段留零洞）。
  ///
  /// 回退即失败：旧实现先删本地再回源，远端缺失时本地文件已被丢掉。
  #[test]
  fn ensure_resident_keeps_local_prefix_when_remote_missing() {
    pollster::block_on(async {
      let tbl = TableIdent::new("prefix", 1).unwrap();
      let cloud = CloudStore::new(
        MemStore::new(512),
        MemObjectStore::new(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      );
      cloud.local.write_file(&tbl, 0, &[1u8; 512]).await.unwrap();
      // 起点 4096 远大于本地 512 字节前缀，且远端没有该对象。
      cloud.ensure_resident(&tbl, 0, 4096).await.unwrap();
      assert_eq!(
        cloud.local.file_size(&tbl, 0).await.unwrap(),
        Some(512),
        "远端缺失时本地前缀页必须保留"
      );
    });
  }

  /// 第 4 项：首次建立 scheme 时给盘上已完整的旧段回填单文件标记，避免
  /// `restore_table_cache` 把旧段当半装（不记账 → 预热误报 DiskFull）。
  #[test]
  fn enabling_scheme_backfills_existing_complete_segments() {
    pollster::block_on(async {
      let tbl = TableIdent::new("backfill", 1).unwrap();
      let cloud = CloudStore::new(
        MemStore::new(512).with_segment_config(4096, 1),
        MemObjectStore::new(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1);
      // 旧目录：段 0 完整（首尾槽都可读），无 scheme、无标记。
      cloud
        .local
        .write_segment(&tbl, 0, &[1u8; 4096])
        .await
        .unwrap();
      cloud
        .local
        .write_segment(&tbl, 1, &[0u8; 4096])
        .await
        .unwrap();
      assert!(
        cloud.segment_file_complete(&tbl, 0).await.unwrap(),
        "旧目录按旧规则判完整"
      );
      // 首次新格式安装触发 scheme；旧段应被回填标记。
      let epoch = cloud.evict_epoch.get();
      cloud
        .install_segment_file(&tbl, 1, &[3u8; 8192], epoch)
        .await
        .unwrap();
      assert!(
        cloud
          .segment_marker_present(&tbl, &segment_marker_name(0))
          .await
          .unwrap(),
        "旧段必须回填单文件标记"
      );
      assert!(
        cloud.segment_file_complete(&tbl, 0).await.unwrap(),
        "回填后旧段仍判完整"
      );
    });
  }

  /// 崩溃语义探针：段写先进 volatile，`sync_data` 才转 durable；`crash` 丢掉
  /// volatile（模拟未 fdatasync 的页缓存）。定长后端崩溃后文件仍按 `set_len`
  /// 分配，丢失的槽位读零而不是缺失。
  #[derive(Clone)]
  struct ProbeStore {
    shared: Rc<ProbeShared>,
  }

  struct ProbeShared {
    segment_size: usize,
    segments_per_file_shift: u8,
    log: Mutex<Vec<String>>,
    volatile: Mutex<HashMap<(TableIdent, FilePageId), Vec<u8>>>,
    durable: Mutex<HashMap<(TableIdent, FilePageId), Vec<u8>>>,
    allocated: Mutex<HashSet<(TableIdent, FileId)>>,
    inner: MemStore,
  }

  impl ProbeStore {
    fn new(segment_size: usize, segments_per_file_shift: u8) -> Self {
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

    fn log_li(&self) -> Vec<String> {
      self
        .shared
        .log
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone()
    }

    /// 丢掉未 sync 的段写：已分配文件保留（读零），未 sync 的槽位内容丢失。
    fn crash(&self) {
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

    async fn remove_segment_file(
      &self,
      tbl: &TableIdent,
      file_id: FileId,
    ) -> fkv_error::Result<()> {
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

    async fn remove_sidecar_file_name(
      &self,
      tbl: &TableIdent,
      name: &str,
    ) -> fkv_error::Result<()> {
      self.shared.inner.remove_sidecar_file_name(tbl, name).await
    }

    async fn read_manifest_file(
      &self,
      tbl: &TableIdent,
      name: &str,
    ) -> fkv_error::Result<Option<Vec<u8>>> {
      self.shared.inner.read_manifest_file(tbl, name).await
    }

    async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> fkv_error::Result<()> {
      self.shared.inner.remove_manifest_file(tbl, name).await
    }
  }

  /// 完成标记必须晚于段数据的 durability：否则崩溃后标记在、数据是零洞，
  /// `restore_table_cache` 会把它登记成 clean 并读零上传。
  #[test]
  fn install_segment_syncs_slots_before_marker() {
    pollster::block_on(async {
      let tbl = TableIdent::new("syncorder", 1).unwrap();
      let probe = ProbeStore::new(4096, 1);
      let cloud = CloudStore::new(
        probe.clone(),
        MemObjectStore::new(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1);
      let epoch = cloud.evict_epoch.get();
      cloud
        .install_segment_file(&tbl, 0, &[5u8; 8192], epoch)
        .await
        .unwrap();

      let log_li = probe.log_li();
      let sync_pos = log_li
        .iter()
        .position(|entry| entry == &format!("sync:{tbl}"))
        .expect("install 必须先 sync 段数据");
      let marker_pos = log_li
        .iter()
        .position(|entry| entry == "marker:segcomplete_0")
        .expect("install 必须写单文件完成标记");
      assert!(
        sync_pos < marker_pos,
        "完成标记不得早于数据落盘: {log_li:?}"
      );

      probe.crash();
      let mut dst = vec![0u8; 4096];
      cloud.read_segment(&tbl, 0, &mut dst).await.unwrap();
      assert_eq!(dst, vec![5u8; 4096], "标记之前落盘的段数据必须扛住崩溃");
      assert!(cloud.segment_file_complete(&tbl, 0).await.unwrap());
    });
  }

  /// 回填必须跳过**同组另一实例**在飞 / 脏的段文件：运行时预热与 shard 是不同
  /// `CloudStore` 实例，只看本实例状态会把对方半装文件盖成完整。
  #[test]
  fn scheme_backfill_skips_segment_busy_of_other_instance() {
    pollster::block_on(async {
      let tbl = TableIdent::new("sharedbusy", 1).unwrap();
      let group = Arc::new(CacheGroup::new(0));
      let writer = CloudStore::new(
        MemStore::new(512).with_segment_config(4096, 1),
        MemObjectStore::new(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1)
      .with_cache_group(group.clone());
      // 写实例：段 0 在飞（另一任务 install 中），段 1 有未上传脏数据。
      group.set_segment_inflight(group.alloc_instance_id(), &tbl, 0, true);
      writer.upsert_segment_resident(&tbl, 1, 8192, true);

      let prewarm = CloudStore::new(
        MemStore::new(512).with_segment_config(4096, 1),
        MemObjectStore::new(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1)
      .with_cache_group(group);
      // 旧目录：段 0 / 1 / 2 都按旧尾槽规则完整。
      for file_id in 0..3u64 {
        prewarm
          .local
          .write_segment(&tbl, file_id * 2, &[1u8; 4096])
          .await
          .unwrap();
        prewarm
          .local
          .write_segment(&tbl, file_id * 2 + 1, &[0u8; 4096])
          .await
          .unwrap();
      }
      prewarm.enable_segment_marker(&tbl).await.unwrap();

      for file_id in [0, 1] {
        assert!(
          !prewarm
            .segment_marker_present(&tbl, &segment_marker_name(file_id))
            .await
            .unwrap(),
          "另一实例在飞 / 脏的段 {file_id} 不得被回填标记"
        );
      }
      assert!(
        prewarm
          .segment_marker_present(&tbl, &segment_marker_name(2))
          .await
          .unwrap(),
        "无跨实例 busy 状态的旧段仍必须回填"
      );
    });
  }

  /// R12-1：预热直连 `install_segment_file` 也必须在组内登记在飞。
  ///
  /// 回退即失败：登记只留在 `single_flight` 里时预热不登记，同组回填会在
  /// 段文件写到一半时把它当旧完整文件盖标记。
  #[test]
  fn prewarm_install_registers_group_inflight() {
    pollster::block_on(async {
      let tbl = TableIdent::new("prewarm_inflight", 1).unwrap();
      let group = Arc::new(CacheGroup::new(0));
      let remote = MemObjectStore::new();
      let prewarm = CloudStore::new(
        YieldWriteStore::new_segment_write_yield(),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1)
      .with_cache_group(group.clone());
      let other = CloudStore::new(
        MemStore::new(512).with_segment_config(4096, 1),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1)
      .with_cache_group(group.clone());
      let key = prewarm.segment_key(&tbl, 0);
      remote
        .put(&key, BytesBytes::from(vec![7u8; 8192]), None, false)
        .await
        .unwrap();
      assert!(remote.get(&key).await.unwrap().is_some(), "远端对象应可见");

      let waker = noop_waker();
      let mut cx = Context::from_waker(&waker);
      let mut prewarm_fut = pin!(prewarm.prewarm_segment_file(&tbl, 0));
      // 远端 `get` 自身可能先挂起一次；推进到 install 入口登记后停下。
      for _ in 0..8 {
        if group.segment_busy(&tbl, 0) {
          break;
        }
        assert!(
          prewarm_fut.as_mut().poll(&mut cx).is_pending(),
          "应停在安装的段写入"
        );
      }
      assert!(group.segment_busy(&tbl, 0), "预热安装必须在组内登记在飞");
      assert!(
        other.segment_backfill_skip(&tbl, 0),
        "同组回填必须跳过预热正在写的段"
      );
      match prewarm_fut.as_mut().poll(&mut cx) {
        Poll::Ready(Ok(found)) => assert!(found),
        other => panic!("预热安装应完成，实际 {other:?}"),
      }
      assert!(!group.segment_busy(&tbl, 0), "安装结束后必须注销登记");
    });
  }

  /// R12-2：回填权在组上串行，另一实例正在回填时不得再落 scheme。
  ///
  /// 回退即失败：实例私有登记时两个实例各回填一遍，会互相给在写文件盖标记。
  #[test]
  fn scheme_enabling_is_serialized_across_group_instances() {
    pollster::block_on(async {
      let tbl = TableIdent::new("group_enable", 1).unwrap();
      let group = Arc::new(CacheGroup::new(0));
      let mut store_li = (0..2)
        .map(|_| {
          CloudStore::new(
            MemStore::new(512).with_segment_config(4096, 1),
            MemObjectStore::new(),
            "fkv",
            fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
          )
          .with_segment_config(4096, 1)
          .with_cache_group(group.clone())
        })
        .collect::<Vec<_>>();
      let held = group.try_begin_enable(&tbl).expect("首个回填者取得回填权");
      let other = store_li.pop().unwrap();
      assert_eq!(
        other.enable_segment_marker(&tbl).await,
        Err(KvError::TryAgain),
        "抢不到回填权必须 TryAgain，不得报成功"
      );
      assert!(
        !other.segment_scheme_enabled(&tbl).await.unwrap(),
        "另一实例持有回填权时不得落 scheme"
      );
      drop(held);
      other.enable_segment_marker(&tbl).await.unwrap();
      assert!(
        other.segment_scheme_enabled(&tbl).await.unwrap(),
        "回填权释放后必须能落 scheme"
      );
    });
  }

  /// R12-2：旧规则判定期间同组实例开始安装，写标记前必须复核 busy。
  ///
  /// 回退即失败：只在循环开头判一次时，判定期间开始的 install 会被盖成完整。
  #[test]
  fn backfill_rechecks_busy_after_legacy_complete() {
    pollster::block_on(async {
      let tbl = TableIdent::new("backfill_recheck", 1).unwrap();
      let group = Arc::new(CacheGroup::new(0));
      let cloud = CloudStore::new(
        YieldWriteStore::new_segment_read_yield(),
        MemObjectStore::new(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1)
      .with_cache_group(group.clone());
      cloud
        .local
        .write_segment(&tbl, 0, &[1u8; 4096])
        .await
        .unwrap();
      cloud
        .local
        .write_segment(&tbl, 1, &[0u8; 4096])
        .await
        .unwrap();

      let waker = noop_waker();
      let mut cx = Context::from_waker(&waker);
      let mut enable = pin!(cloud.enable_segment_marker(&tbl));
      assert!(
        enable.as_mut().poll(&mut cx).is_pending(),
        "应停在旧规则完整性判定的段读取"
      );
      // 判定期间另一实例开始安装同一段文件。
      group.set_segment_inflight(group.alloc_instance_id(), &tbl, 0, true);
      match enable.as_mut().poll(&mut cx) {
        Poll::Ready(Ok(())) => {}
        other => panic!("回填应完成，实际 {other:?}"),
      }
      assert!(
        !cloud
          .segment_marker_present(&tbl, &segment_marker_name(0))
          .await
          .unwrap(),
        "判定期间开始的安装必须让回填跳过该段"
      );
    });
  }

  /// R12-3：共享组比实例长寿时，实例销毁必须清掉自己的脏 owner。
  ///
  /// 回退即失败：无释放时 owner 永久残留，该段在组内永久 busy，回填永远跳过。
  #[test]
  fn dropping_store_releases_group_dirty_owner() {
    let tbl = TableIdent::new("owner_drop", 1).unwrap();
    let group = Arc::new(CacheGroup::new(0));
    {
      let store = CloudStore::new(
        MemStore::new(512).with_segment_config(4096, 1),
        MemObjectStore::new(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1)
      .with_cache_group(group.clone());
      store.upsert_segment_resident(&tbl, 1, 8192, true);
      assert!(group.segment_busy(&tbl, 1));
    }
    assert!(!group.segment_busy(&tbl, 1), "实例销毁必须释放组内 owner");
  }

  /// R12-4：本地表 / 条目缺失时 `mark_segment_clean` 也必须清 owner。
  ///
  /// 回退即失败：提前 return 会跳过 owner 清理，文件在组内永久 busy。
  #[test]
  fn mark_segment_clean_clears_owner_without_local_entry() {
    let tbl = TableIdent::new("clean_missing", 1).unwrap();
    let cloud = CloudStore::new(
      MemStore::new(512).with_segment_config(4096, 1),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1);
    cloud.owner.set_segment_dirty(cloud.owner.id, &tbl, 7, true);
    assert!(cloud.owner.segment_busy(&tbl, 7));
    cloud.mark_segment_clean(&tbl, 7, 0);
    assert!(
      !cloud.owner.segment_busy(&tbl, 7),
      "本地缺条目也必须清 owner"
    );
  }

  /// R12-5：busy 查询按表作用域，同 file id 的另一张表不得串扰。
  #[test]
  fn segment_busy_is_scoped_by_table() {
    let tbl_a = TableIdent::new("busy_a", 1).unwrap();
    let tbl_b = TableIdent::new("busy_b", 1).unwrap();
    let group = CacheGroup::new(0);
    let owner = group.alloc_instance_id();
    group.set_segment_inflight(owner, &tbl_a, 0, true);
    assert!(group.segment_busy(&tbl_a, 0));
    assert!(!group.segment_busy(&tbl_b, 0), "另一张表必须不受影响");
    assert!(!group.segment_busy(&tbl_a, 1), "另一个文件必须不受影响");
    group.set_segment_inflight(owner, &tbl_a, 0, false);
    assert!(!group.segment_busy(&tbl_a, 0));
  }

  /// R13-1：回填权被同组实例持有时，install 必须在删旧标记 / 写槽位之前退出。
  ///
  /// 回退即失败：`enable_segment_marker` 返回 `Ok` 时安装会先删单文件标记再写
  /// 半装槽位，此刻盘上没有 scheme，重启后按旧尾槽规则被误判完整。
  #[test]
  fn install_aborts_when_backfill_held_by_other_instance() {
    pollster::block_on(async {
      let tbl = TableIdent::new("enable_held", 1).unwrap();
      let group = Arc::new(CacheGroup::new(0));
      let cloud = CloudStore::new(
        MemStore::new(512).with_segment_config(4096, 1),
        MemObjectStore::new(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1)
      .with_cache_group(group.clone());
      // 旧格式完整段 + 上一轮留下的单文件标记，盘上尚无 scheme。
      cloud
        .local
        .write_segment(&tbl, 0, &[1u8; 4096])
        .await
        .unwrap();
      cloud
        .local
        .write_segment(&tbl, 1, &[0u8; 4096])
        .await
        .unwrap();
      cloud.write_segment_marker(&tbl, 0).await.unwrap();

      let held = group.try_begin_enable(&tbl).expect("另一实例持有回填权");
      let epoch = cloud.evict_epoch.get();
      assert_eq!(
        cloud
          .install_segment_file(&tbl, 0, &[2u8; 8192], epoch)
          .await,
        Err(KvError::TryAgain),
        "回填权被持有时安装必须让路"
      );
      assert!(
        !cloud.segment_scheme_enabled(&tbl).await.unwrap(),
        "安装让路时不得落 scheme"
      );
      assert!(
        cloud
          .segment_marker_present(&tbl, &segment_marker_name(0))
          .await
          .unwrap(),
        "安装让路时不得先删旧完成标记"
      );
      drop(held);
    });
  }

  /// R13-2：`write_segment` 的组内在飞登记必须覆盖整个写（含本地写 await）。
  ///
  /// 回退即失败：只在写完后标脏时，写 await 期间 `segment_busy` 为假，同组
  /// 回填会按旧尾槽规则给半装文件盖「完整」标记。
  #[test]
  fn segment_write_registers_group_inflight_before_local_write() {
    pollster::block_on(async {
      let tbl = TableIdent::new("write_inflight", 1).unwrap();
      let group = Arc::new(CacheGroup::new(0));
      let cloud = CloudStore::new(
        YieldWriteStore::new_segment_write_yield(),
        MemObjectStore::new(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1)
      .with_cache_group(group.clone());

      let waker = noop_waker();
      let mut cx = Context::from_waker(&waker);
      let mut write = pin!(cloud.write_segment(&tbl, 0, &[1u8; 4096]));
      for _ in 0..8 {
        if group.segment_busy(&tbl, 0) {
          break;
        }
        assert!(
          write.as_mut().poll(&mut cx).is_pending(),
          "应停在本地段写入"
        );
      }
      assert!(
        group.segment_busy(&tbl, 0),
        "本地写 await 期间必须在组内登记在飞"
      );
      assert!(
        cloud.segment_backfill_skip(&tbl, 0),
        "同组回填必须跳过正在写的段"
      );
      match write.as_mut().poll(&mut cx) {
        Poll::Ready(Ok(())) => {}
        other => panic!("段写应完成，实际 {other:?}"),
      }
      // 写完后条目转为脏 owner（flush 前本就 busy），只看在飞计数是否归零。
      let inflight_num = group
        .busy_guard()
        .get(&tbl)
        .and_then(|file_map| file_map.get(&0))
        .map_or(0, |entry| entry.inflight_num());
      assert_eq!(inflight_num, 0, "写结束后必须注销在飞登记");
    });
  }

  /// R13-3：发布登记必须跨实例可见，prewarm 不得删掉 shard 正在发布的首写快照。
  ///
  /// 回退即失败：登记留在实例私有 `RefCell` 时，prewarm 看不到 shard 的首写
  /// 窗口，`(Some, None)` 分支会把本地 manifest 删掉。
  #[test]
  fn publish_inflight_is_visible_across_group_instances() {
    pollster::block_on(async {
      let tbl = TableIdent::new("publish_shared", 1).unwrap();
      let group = Arc::new(CacheGroup::new(0));
      // 共享本地 root：同一个 `ProbeStore` 句柄。
      let probe = ProbeStore::new(4096, 1);
      let writer = CloudStore::new(
        probe.clone(),
        MemObjectStore::new(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1)
      .with_cache_group(group.clone());
      let prewarm = CloudStore::new(
        probe.clone(),
        MemObjectStore::new(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1)
      .with_cache_group(group.clone());

      writer.local.append_manifest(&tbl, b"rec").await.unwrap();
      let _publish = PublishGuard::new(&writer.owner.group, &tbl);
      // 远端还没有 manifest：shard 的 PUT 尚在发布中。
      assert!(
        !prewarm.prewarm_manifest(&tbl).await.unwrap(),
        "发布中不应宣称本地快照可用"
      );
      assert!(
        probe.read_manifest(&tbl).await.unwrap().is_some(),
        "另一实例的发布窗口内不得删除本地 manifest"
      );
    });
  }

  /// R13-4：淘汰 segment 条目必须清组内本 owner 的脏登记。
  ///
  /// 回退即失败：owner 随条目摘链残留，该文件在组内永久 busy，scheme 回填
  /// 永远跳过它。
  #[test]
  fn trim_cache_clears_group_dirty_owner_on_segment_victim() {
    let tbl = TableIdent::new("evict_owner", 1).unwrap();
    let cloud = CloudStore::new(
      MemStore::new(512).with_segment_config(4096, 1),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
    .with_cache_budget(1);
    cloud.upsert_segment_resident(&tbl, 7, 8192, false);
    cloud.owner.set_segment_dirty(cloud.owner.id, &tbl, 7, true);
    assert!(cloud.owner.segment_busy(&tbl, 7));

    assert_eq!(cloud.trim_cache(), 1, "超预算必须淘汰该干净段");
    assert!(
      !cloud.owner.segment_busy(&tbl, 7),
      "淘汰摘链后必须清组内 owner"
    );
    assert_eq!(cloud.resident_file_count(), 0);
  }

  /// R14-3：同组另一实例持脏 / 在写同一段时，预热安装必须让路。
  ///
  /// 回退即失败：不查组内 busy 时预热会用远端旧值覆盖 shard 正在写的脏段，
  /// shard 随后 flush 会把旧值上传成权威（静默丢写）。
  #[test]
  fn prewarm_install_yields_to_foreign_dirty_segment() {
    pollster::block_on(async {
      let tbl = TableIdent::new("install_yield", 1).unwrap();
      let group = Arc::new(CacheGroup::new(0));
      // 共享本地 root：两个实例看同一批段文件。
      let probe = ProbeStore::new(4096, 1);
      let remote = MemObjectStore::new();
      let writer = CloudStore::new(
        probe.clone(),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1)
      .with_cache_group(group.clone());
      let prewarm = CloudStore::new(
        probe.clone(),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1)
      .with_cache_group(group.clone());
      // shard 已写段 3 的首槽（脏，未上传）；远端还有同一段文件的旧副本。
      writer
        .local
        .write_segment(&tbl, 6, &[1u8; 4096])
        .await
        .unwrap();
      writer.upsert_segment_resident(&tbl, 3, 8192, true);
      remote
        .put(
          &prewarm.segment_key(&tbl, 3),
          BytesBytes::from(vec![9u8; 8192]),
          None,
          false,
        )
        .await
        .unwrap();

      assert_eq!(
        prewarm.prewarm_segment_file(&tbl, 3).await,
        Err(KvError::TryAgain),
        "同组脏段必须让路"
      );
      let mut dst = vec![0u8; 4096];
      probe.read_segment(&tbl, 6, &mut dst).await.unwrap();
      assert_eq!(dst, vec![1u8; 4096], "shard 的写不得被远端旧值覆盖");

      // 无人在写的段照常安装，证明让路只针对冲突文件。
      remote
        .put(
          &prewarm.segment_key(&tbl, 4),
          BytesBytes::from(vec![8u8; 8192]),
          None,
          false,
        )
        .await
        .unwrap();
      assert!(prewarm.prewarm_segment_file(&tbl, 4).await.unwrap());
      probe.read_segment(&tbl, 8, &mut dst).await.unwrap();
      assert_eq!(dst, vec![8u8; 4096], "无冲突时以远端为准");
    });
  }

  /// R14-4：在飞登记按 owner 记深度：同实例嵌套登记不失衡，他人登记可见。
  ///
  /// 回退即失败：全局计数（旧实现）下写路径 guard 与嵌套 install 计 2，
  /// 内层注销就把外层仍在写的文件置空；`saturating_sub` 还会掩盖失衡。
  #[test]
  fn segment_inflight_is_tracked_per_owner_with_nesting() {
    let tbl = TableIdent::new("owner_depth", 1).unwrap();
    let group = CacheGroup::new(0);
    let owner_a = group.alloc_instance_id();
    let owner_b = group.alloc_instance_id();

    group.set_segment_inflight(owner_a, &tbl, 1, true);
    group.set_segment_inflight(owner_a, &tbl, 1, true);
    group.set_segment_inflight(owner_a, &tbl, 1, false);
    assert!(group.segment_busy(&tbl, 1), "嵌套登记不得被内层注销抹掉");
    assert!(group.segment_foreign_busy(owner_b, &tbl, 1));
    assert!(
      !group.segment_foreign_busy(owner_a, &tbl, 1),
      "自己的登记不算他人"
    );
    group.set_segment_inflight(owner_a, &tbl, 1, false);
    assert!(!group.segment_busy(&tbl, 1), "配对注销后必须归零");

    // 未配对的注销不得减别人的深度。
    group.set_segment_inflight(owner_b, &tbl, 2, true);
    group.set_segment_inflight(owner_a, &tbl, 2, false);
    assert!(group.segment_busy(&tbl, 2), "别人的在飞登记必须保留");
  }

  /// R14-6：表级清理一次加锁批量清脏登记，且只清本 owner。
  #[test]
  fn clear_segment_dirty_li_batches_owner_cleanup() {
    let tbl = TableIdent::new("batch_clear", 1).unwrap();
    let group = CacheGroup::new(0);
    let owner_a = group.alloc_instance_id();
    let owner_b = group.alloc_instance_id();
    let file_id_li: Vec<FileId> = (0..8).collect();
    for file_id in &file_id_li {
      group.set_segment_dirty(owner_a, &tbl, *file_id, true);
    }

    group.clear_segment_dirty_li(owner_a, &tbl, &file_id_li);
    for file_id in &file_id_li {
      assert!(
        !group.segment_busy(&tbl, *file_id),
        "段 {file_id} 的本 owner 脏登记必须清掉"
      );
    }
    // 别的 owner 的登记不属于这一批清理对象。
    group.set_segment_dirty(owner_b, &tbl, 3, true);
    group.set_segment_dirty(owner_b, &tbl, 5, true);
    group.clear_segment_dirty_li(owner_a, &tbl, &file_id_li);
    for file_id in [3, 5] {
      assert!(
        group.segment_foreign_busy(owner_a, &tbl, file_id),
        "批量清理不得动别的 owner"
      );
    }
  }

  /// R14-5：只读实例安装段文件，但不启用表级 scheme（不参与回填）。
  ///
  /// 回退即失败：只读实例回填会按旧尾槽规则给**另一进程**正在写的半装文件
  /// 盖「完整」标记，重启后读零并把零上传成权威。
  #[test]
  fn read_only_install_skips_scheme_backfill() {
    pollster::block_on(async {
      let tbl = TableIdent::new("read_only", 1).unwrap();
      let remote = MemObjectStore::new();
      let cloud = CloudStore::new(
        MemStore::new(512).with_segment_config(4096, 1),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1)
      .with_read_only(true);
      assert!(cloud.read_only());
      remote
        .put(
          &cloud.segment_key(&tbl, 0),
          BytesBytes::from(vec![7u8; 8192]),
          None,
          false,
        )
        .await
        .unwrap();

      assert!(cloud.prewarm_segment_file(&tbl, 0).await.unwrap());
      assert!(
        cloud
          .segment_marker_present(&tbl, &segment_marker_name(0))
          .await
          .unwrap(),
        "单文件完成标记照常写"
      );
      assert!(
        !cloud.segment_scheme_enabled(&tbl).await.unwrap(),
        "只读实例不得落表级 scheme"
      );
      // 直接调用回填入口同样拒绝。
      cloud.enable_segment_marker(&tbl).await.unwrap();
      assert!(!cloud.segment_scheme_enabled(&tbl).await.unwrap());
    });
  }

  /// 统计 `get` 次数的远端：验证回填权在整文件下载**之前**争取。
  #[derive(Clone)]
  struct GetCountStore {
    inner: MemObjectStore,
    get_num: Rc<Cell<u32>>,
  }

  impl GetCountStore {
    fn new() -> Self {
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

  /// R14-2：回填权（scheme 启用）在 `remote.get` **之前**争取。
  ///
  /// 回退即失败：先下载后抢权时，抢不到会白下载整个段对象再失败（压缩的
  /// 8 路批量因此整批 abort）。
  #[test]
  fn install_claims_backfill_before_remote_get() {
    pollster::block_on(async {
      let tbl = TableIdent::new("claim_before_get", 1).unwrap();
      let group = Arc::new(CacheGroup::new(0));
      let remote = GetCountStore::new();
      let cloud = CloudStore::new(
        MemStore::new(512).with_segment_config(4096, 1),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1)
      .with_cache_group(group.clone());
      remote
        .put(
          &cloud.segment_key(&tbl, 0),
          BytesBytes::from(vec![3u8; 8192]),
          None,
          false,
        )
        .await
        .unwrap();

      let held = group.try_begin_enable(&tbl).expect("另一实例持有回填权");
      assert_eq!(
        cloud.prewarm_segment_file(&tbl, 0).await,
        Err(KvError::TryAgain)
      );
      assert_eq!(remote.get_num.get(), 0, "抢不到回填权时不得先下载整对象");

      drop(held);
      assert!(cloud.prewarm_segment_file(&tbl, 0).await.unwrap());
      assert_eq!(remote.get_num.get(), 1, "拿到回填权后只下载一次");
    });
  }
}
