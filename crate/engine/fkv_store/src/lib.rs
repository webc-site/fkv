//! 单线程 Store：可接 `MemStore` / `CompioStore`，打通 put / get / scan / delete，
//! 并在写路径后追加 manifest snapshot / log。

use std::{
  cell::RefCell,
  collections::{BTreeMap, VecDeque, btree_map::Entry},
  sync::Arc,
};

use fkv_btree::{BatchEntry, CacheCounter, CacheLedger, CacheStat, IndexCache};
use fkv_codec::{append_varint32, append_varint64};
use fkv_compress::DictCompression;
use fkv_error::{KvError, Result};
use fkv_io::{FileIo, FixedBuffer, FixedSegmentIo, ManifestIo, PageIo, SegmentIo, TermIo};
use fkv_large::{
  ManagedReadResult, SegmentLayout, WrittenLargeValue, read_value as read_large_value,
  read_value_fixed, write_value as write_large_value, write_value_fixed_with_len,
};
use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArg};
use fkv_mapping::{
  AllocMode, CHUNK_SIZE, MappingSnapshot, PageMapper, SnapshotRegistry, SnapshotSource,
};
use fkv_task_gc::GcScope;
use fkv_task_read::{ReadEntry, ReadFloorEntry, YieldBudget, read_entry, read_floor_entry};
pub use fkv_task_scan::{PageOption, ScanOption};
use fkv_task_scan::{
  ScanArg, ScanTree, ScannedEntry, scan_entries, scan_entries_with_budget, scan_page_with_budget,
};
use fkv_task_write::{
  PageLayout, PreparedWriteBatch, WriteArg, is_expired, parse_ttl_key, prepare_write_batch,
  ttl_key, write_batch,
};
use fkv_types::{
  BranchFileRange, DEFAULT_MANIFEST_LIMIT, DEFAULT_PAGES_PER_FILE_SHIFT,
  DEFAULT_SEGMENTS_PER_FILE_SHIFT, FileId, FilePageId, InstallOutcome, MAIN_BRANCH_NAME, NULL_PAGE,
  PageId, TableIdent, WriteOp, archive_file_name, is_valid_branch_name, manifest_file_name,
  normalize_branch_name, now_ms, parse_manifest_file_name,
};
use gxhash::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CleanExpiredStat {
  pub deleted: usize,
  pub ttl_deleted: usize,
  pub next_expire_ts: u64,
}

/// 单次 `clean_expired` 最多处理的过期 TTL 条目数。
///
/// 每条过期 TTL 都要在主树做一次点读复核（perf-86 复审确认的有意偏离，
/// 用于防御 TTL 索引与主树 `expire_ts` 不一致），上界把单次调用的尾延迟封顶；
/// 未清完的条目由 `next_expire_ts = now` 驱动下一轮（写路径 `maybe_clean_expired` /
/// 延迟清理 `drain_pending_ttl`）继续。
pub const CLEAN_EXPIRED_MAX_ENTRIES: usize = 256;

/// 单次 TTL 探测最多跳过的坏 key 数。
///
/// 坏 key 会被 `clean_expired` 一并删除，因此每轮清理都会向前推进；上界只
/// 用来封顶单轮的长尾读取，命中后把 `next_expire_ts` 压回当前时刻驱动下一轮
/// （r7a-core-05）。
const TTL_BAD_KEY_SKIP_MAX: usize = 16;

/// TTL 探测的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TtlPeek {
  /// 首个可解析 TTL 条目的到期时间。
  Expire(u64),
  /// 探测区间内已无条目。
  Empty,
  /// 连续坏 key 达到 [`TTL_BAD_KEY_SKIP_MAX`]，本轮放弃探测。
  Truncated,
}

/// `truncate_from` 单轮扫描 / 删除的 key 上界。
///
/// 整表物化后一次性删除会让大表的内存随表大小线性增长；按批扫描、删除、
/// 再从中断位置续扫，驻留内存只与本常量成正比（R2-1）。
pub const TRUNCATE_FROM_BATCH: usize = 4096;

/// 过期清理的删除版本号：至少要压过被删条目自身的 ts。
///
/// 用户 ts 由客户端提供，可能是微秒 / 纳秒量级（远大于毫秒墙钟）；若直接用
/// 调用方传入的毫秒 `now_ms` 作删除 ts，折叠时 delete 因 ts 更小而落败，
/// 过期条目永不物理删除、空间无界增长。取 `max(now_ms, 条目 ts)`：同 ts 时
/// 后项（delete）胜，因此纳秒量级的条目也能删掉；而毫秒量级客户端仍沿用
/// 墙钟作 tombstone，不会被未来写入的毫秒 ts 反压。
fn expired_delete_ts(entry_ts: u64) -> u64 {
  now_ms().max(entry_ts)
}

fn is_safe_tag(tag: &str) -> bool {
  !tag.is_empty()
    && tag
      .chars()
      .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeValue {
  pub value: Vec<u8>,
  pub metadata: Vec<u8>,
}

/// Rust 引擎只实现 append 分配模式（L1 = Local + append）。
///
/// `data_append_mode=false` 的 C++ `PooledFilePages` 原地复用行为不在 L1 对齐
/// 范围（见 sto-52 / sto-59）。集中成编译期常量，避免 `AllocMode::Append`
/// 字面量散落各建 mapper 点后语义悄悄漂移。
const STORE_ALLOC_MODE: AllocMode = AllocMode::Append;

/// 编译期断言：一旦有人把 `STORE_ALLOC_MODE` 改成 `Pooled`，直接编译失败。
/// 接线 pooled 必须先补 C++ `Replayer::GetMapper` 的 branch 区间过滤与空闲页重建。
const _: () = match STORE_ALLOC_MODE {
  AllocMode::Append => (),
  AllocMode::Pooled => panic!("Rust 引擎只支持 append 分配模式"),
};

/// branch 预留的连续 file id 数；main 跳过该区间，避免与 branch 并发写碰撞。
pub const BRANCH_FILE_RESERVE_FILE_NUM: u64 = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveInfo {
  pub name: String,
  pub bytes: usize,
  pub removed_num: usize,
}

/// 归档清单条目：归档 manifest 名里的 `(tag, term)`。
///
/// 字段顺序 = `Ord` 顺序（先 `tag` 后 `term`），与 C++
/// `GlobalListArchiveTagsRequest::ArchiveEntry` 及 `fkv_api::ArchiveTagInfo`
/// 保持一致。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArchiveTagInfo {
  pub tag: String,
  pub term: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInfo {
  pub name: String,
  pub bytes: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanPageResult {
  pub entry_li: Vec<(Vec<u8>, Vec<u8>)>,
  pub has_remaining: bool,
}

/// 扫描结果 entry：`expire_ts == 0` 表示不过期。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KvEntry {
  pub key: Vec<u8>,
  pub value: Vec<u8>,
  pub expire_ts: u64,
}

impl From<ScannedEntry> for KvEntry {
  fn from(entry: ScannedEntry) -> Self {
    Self {
      key: entry.key,
      value: entry.value,
      expire_ts: entry.expire_ts,
    }
  }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KvEntryPage {
  pub entry_li: Vec<KvEntry>,
  pub has_remaining: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloorValue {
  pub key: Vec<u8>,
  pub value: Vec<u8>,
  pub metadata: Vec<u8>,
  pub timestamp: u64,
  pub expire_ts: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeValueInto {
  pub value_len: usize,
  pub metadata: Vec<u8>,
}

/// 重新借用 `Option<&mut PageMapper>`，供同一作用域内多次 `write_batch` 使用。
fn reborrow_segment<'a>(segment: &'a mut Option<&mut PageMapper>) -> Option<&'a mut PageMapper> {
  segment.as_deref_mut()
}

/// 跳到下一个文件边界的首个页（对齐 C++ `Replayer::GetMapper` 的 term bump）。
fn next_file_boundary(max_fp_id: FilePageId, shift: u8) -> FilePageId {
  ((max_fp_id >> shift) + 1) << shift
}

#[derive(Debug)]
struct TableState {
  /// `None` 表示该表未加载（逻辑页表尚未从 manifest 恢复）。
  mapper: Option<PageMapper>,
  segment_mapper: Option<PageMapper>,
  /// `Some` 表示该表已打开；`NULL_PAGE` 为空树。
  root: Option<PageId>,
  ttl_root: PageId,
  /// 从 manifest 恢复 / 写 manifest 时维护的 branch 元数据（含祖先 file_range_li 链）。
  branch_metadata: Option<BranchManifestMetadata>,
  manifest_started: bool,
  manifest_size: usize,
  /// 已知最早 TTL 过期时间；0 表示未知。
  next_expire: u64,
  compression: Option<DictCompression>,
  index_cache: Option<RefCell<IndexCache>>,
  /// 当前 index 缓存所属分支名；reopen 复用要求与当前分支一致（eng-67）。
  cache_branch: Option<String>,
  /// 当前 index 缓存所属 term；跨 term 文件 id 空间可能不同，必须丢弃（eng-67）。
  cache_term: u64,
  /// 写后延迟 TTL 清理时标记待清理。
  ttl_pending: bool,
  /// 最近一次记账的近似常驻字节（[`table_state_bytes`] 结果）。
  ///
  /// 只在触摸 / 写批次收尾 / 驱逐时刷新；读路径填充 index 缓存带来的增长
  /// 由下一次触摸补记，避免读路径拿 `&mut self`。
  bytes: usize,
}

impl Default for TableState {
  fn default() -> Self {
    Self {
      mapper: None,
      segment_mapper: None,
      root: None,
      ttl_root: NULL_PAGE,
      branch_metadata: None,
      manifest_started: false,
      manifest_size: 0,
      next_expire: 0,
      compression: None,
      index_cache: None,
      cache_branch: None,
      cache_term: 0,
      ttl_pending: false,
      bytes: 0,
    }
  }
}

/// 单表 root 状态的近似常驻字节：结构体本身、data / segment 映射 chunk、
/// index 缓存、压缩字典与 branch 元数据。
///
/// `MappingTbl` 的 chunk 数与逻辑页数严格对应（`push` / `resize` 始终维持
/// `ceil(len / CHUNK_SIZE)` 个定长 chunk），因此无需遍历 chunk 即可算出字节数。
fn table_state_bytes(state: &TableState) -> usize {
  fn mapping_bytes(mapper: Option<&PageMapper>) -> usize {
    mapper.map_or(0, |mapper| {
      mapper.tbl().len().div_ceil(CHUNK_SIZE) * CHUNK_SIZE * size_of::<u64>()
    })
  }
  let branch_bytes = state.branch_metadata.as_ref().map_or(0, |metadata| {
    metadata.branch_name.len()
      + metadata.file_range_li.len() * size_of::<BranchFileRange>()
      + metadata
        .file_range_li
        .iter()
        .map(|range| range.branch_name.len())
        .sum::<usize>()
  });
  size_of::<TableState>()
    + mapping_bytes(state.mapper.as_ref())
    + mapping_bytes(state.segment_mapper.as_ref())
    + state
      .compression
      .as_ref()
      .map_or(0, DictCompression::estimated_memory_bytes)
    + state
      .index_cache
      .as_ref()
      .map_or(0, |cache| cache.borrow().byte_len())
    + branch_bytes
}

#[derive(Debug)]
pub struct Store<I> {
  io: I,
  page_size: usize,
  /// data 页 restart 间隔（对齐 C++ `KvOption::data_page_restart_interval`）。
  data_restart_interval: u16,
  /// index 页 restart 间隔（对齐 C++ `KvOption::index_page_restart_interval`）。
  index_restart_interval: u16,
  /// 每表全部内存状态；一张表一个条目，`TableState::root` 决定是否已打开。
  table_map: HashMap<TableIdent, TableState>,
  large_value_layout: Option<SegmentLayout>,
  large_value_threshold: usize,
  branch_name: String,
  term: u64,
  manifest_file: Option<String>,
  branch_file_limit: Option<FilePageId>,
  branch_segment_limit: Option<FilePageId>,
  pages_per_file_shift: u8,
  manifest_limit: usize,
  ttl_read_filter: bool,
  compression_level: Option<i32>,
  /// 一批 overflow 页的最大指针数（对齐 C++ `KvOption::overflow_pointers`）。
  overflow_pointers: u8,
  /// 文件空间放大触发 compaction 的阈值；0 表示不自动触发。
  file_amplify_factor: u8,
  /// segment 文件空间放大触发 compaction 的阈值；0 表示不自动触发。
  ///
  /// 与 `file_amplify_factor` 独立：segment 文件的槽位密度、搬段成本都和
  /// data 页不同，混用一个阈值无法分别调优。
  segment_file_amplify_factor: u8,
  /// 同时驻留的 table 状态上限；0 表示不限制。超限按 FIFO 驱逐最久未写的表。
  max_open_tables: usize,
  /// 表级 root 状态字节预算；0 表示不限制。
  ///
  /// 对齐 C++ `KvOption::root_meta_cache_size` 经 `RootMetaMgr` 实施的字节上限：
  /// data / segment mapping、index 缓存、字典与 branch 元数据合计超限时，
  /// 从 LRU 尾驱逐未被读快照引用的整表状态，之后按需 reopen 重载。
  root_meta_cache_bytes: usize,
  /// 已记账的 root 状态字节合计（各 `TableState::bytes` 之和）。
  root_meta_used: usize,
  /// 触摸顺序队列：`(表, 代际号)`。旧代际项惰性跳过，队列按需压实。
  table_order_li: VecDeque<(TableIdent, u64)>,
  /// 每表最新代际号，用于识别 `table_order_li` 中的陈旧项。
  touch_seq: HashMap<TableIdent, u64>,
  touch_clock: u64,
  /// 写后延迟 TTL 清理（对齐 C++ TriggerTTL + 独立 CleanExpired），默认内联清理。
  defer_ttl: bool,
  /// 每表 index page 缓存字节预算；`0` 表示不限制。
  index_cache_bytes: usize,
  /// 全 store 的 index 缓存聚合字节上限；`0` 表示不限制。
  ///
  /// 对齐 C++ `PageManager` 的单池记账：多表共享一份预算，按缓存的表数均分，
  /// 避免 RSS 随表数线性放大且无背压。
  index_cache_total_bytes: usize,
  /// 是否显式设置过聚合上限；否则跟随每表预算（单表行为不变）。
  index_cache_total_explicit: bool,
  /// index 缓存共享字节账本，跨表合计占用。
  index_cache_ledger: Arc<CacheLedger>,
  /// index 缓存计数器：跨表、跨 reopen 共享，保证上报单调。
  index_cache_counter: Arc<CacheCounter>,
  /// 活跃 mapper 快照登记表：GC retained set 必须并集所有仍被读请求持有的
  /// 旧映射，否则旧快照独占的 file page 会被提前回收（27a）。
  snapshots: SnapshotRegistry,
  /// 长循环（scan 页链 / overflow 链 / GC 文件遍历 / manifest 脏页）的
  /// 协作让出段预算（微秒；`0` = 关闭）。由 shard 从 `ShardConfig` 注入。
  yield_budget_us: u64,
}

/// 释放一个 mapper 的全部逻辑页；长表按 [`YieldBudget`] 定期让出。
///
/// 整表 truncate 的页数与表大小同阶，不让出会把整个 shard 线程占满、
/// 拖住同线程上的读请求。
async fn free_all_pages(mapper: &mut PageMapper, budget: &mut YieldBudget) -> Result<()> {
  let page_num = mapper.tbl().len();
  for page_id in 0..page_num as PageId {
    let _ = mapper.free_page(page_id)?;
    budget.maybe_yield_every(page_id as usize).await;
  }
  Ok(())
}

impl<I: PageIo> Store<I> {
  /// `restart_interval` 同时作为 data / index 页的初始间隔；需分别配置时用
  /// [`Store::with_index_restart_interval`] 覆盖 index 侧。
  pub fn new(io: I, page_size: usize, restart_interval: u16) -> Self {
    Self {
      io,
      page_size,
      data_restart_interval: restart_interval,
      index_restart_interval: restart_interval,
      table_map: HashMap::default(),
      large_value_layout: None,
      large_value_threshold: 0,
      branch_name: MAIN_BRANCH_NAME.to_owned(),
      term: 0,
      manifest_file: None,
      branch_file_limit: None,
      branch_segment_limit: None,
      pages_per_file_shift: DEFAULT_PAGES_PER_FILE_SHIFT,
      manifest_limit: DEFAULT_MANIFEST_LIMIT as usize,
      ttl_read_filter: true,
      compression_level: None,
      overflow_pointers: fkv_page::DEFAULT_OVERFLOW_POINTERS,
      file_amplify_factor: 0,
      segment_file_amplify_factor: 0,
      max_open_tables: 0,
      root_meta_cache_bytes: 0,
      root_meta_used: 0,
      table_order_li: VecDeque::new(),
      touch_seq: HashMap::default(),
      touch_clock: 0,
      defer_ttl: false,
      index_cache_bytes: fkv_btree::DEFAULT_INDEX_CACHE_BYTES,
      index_cache_total_bytes: fkv_btree::DEFAULT_INDEX_CACHE_BYTES,
      index_cache_total_explicit: false,
      index_cache_ledger: Arc::new(CacheLedger::default()),
      index_cache_counter: Arc::new(CacheCounter::default()),
      snapshots: SnapshotRegistry::new(),
      yield_budget_us: 0,
    }
  }

  /// 长循环的协作让出段预算（微秒；`0` = 关闭）。
  ///
  /// 与 shard 的 `max_processing_time_us`（请求边界轮预算）分列：本值管单个
  /// 请求内部的长段（scan 页链 / overflow 链 / GC 文件遍历），默认 20us。
  pub fn with_yield_budget_us(mut self, yield_budget_us: u64) -> Self {
    self.yield_budget_us = yield_budget_us;
    self
  }

  pub fn yield_budget_us(&self) -> u64 {
    self.yield_budget_us
  }

  /// 每表 index page 缓存字节预算（`0` = 不限制）。
  ///
  /// 对齐 C++ `buffer_pool_size` 中划给 index page 的部分；fkv 按表独立计量。
  /// 未显式调用 [`Self::with_index_cache_total_bytes`] 时，聚合上限跟随该值。
  pub fn with_index_cache_bytes(mut self, bytes: usize) -> Self {
    self.index_cache_bytes = bytes;
    // 未显式指定聚合上限时跟随每表预算：单表仍可用满，多表共享同一总量。
    if !self.index_cache_total_explicit {
      self.index_cache_total_bytes = bytes;
    }
    self
  }

  /// 全 store 的 index 缓存聚合上限（`0` = 不限制）。
  ///
  /// 多表场景下预算按挂载的缓存实例数均分（至少 1 份），总量不超过该上限。
  /// 默认跟随 [`Self::with_index_cache_bytes`]：单表行为不变，多表共享同一
  /// 总量（C++ 单池 index/data 分账的可行子集）。
  pub fn with_index_cache_total_bytes(mut self, bytes: usize) -> Self {
    self.index_cache_total_bytes = bytes;
    self.index_cache_total_explicit = true;
    self
  }

  /// 当前聚合上限配置（`0` = 不限制）。
  pub fn index_cache_total_bytes(&self) -> usize {
    self.index_cache_total_bytes
  }

  /// 采纳共享的 index 缓存字节账本（shard 的每分区 Store 模式）。
  ///
  /// 每个分区一个 `Store` 时，账本共享后聚合上限仍按全部缓存实例均分，
  /// 保持「整个 shard 一份预算」；计数器不共享（各分区独立累计，由 shard
  /// 汇总上报），避免跨 Store 的增量对账重复计数。
  pub fn with_index_cache_ledger(mut self, ledger: Arc<CacheLedger>) -> Self {
    self.index_cache_ledger = ledger;
    self
  }

  /// index 页 restart 间隔（对齐 C++ `KvOption::index_page_restart_interval`）。
  ///
  /// `Store::new` 的第三参同时作为 data / index 初始值；需要与 data 页不同时
  /// 由调用方（SDK / shard）用本方法覆盖 index 侧。
  pub fn with_index_restart_interval(mut self, index_restart_interval: u16) -> Self {
    self.index_restart_interval = index_restart_interval;
    self
  }

  /// 新建一个共享计数器的表级 index 缓存。
  fn new_index_cache(&self) -> IndexCache {
    IndexCache::with_counter(self.index_cache_bytes, self.index_cache_counter.clone())
      .with_shared_budget(
        self.index_cache_ledger.clone(),
        self.index_cache_total_bytes,
      )
  }

  /// 新建缓存并把 baseline 对齐到该表当前 live 映射代际。
  ///
  /// 空缓存的 baseline 若留在 0，任何代际的 `insert` 都会被接受：上一个
  /// mapper 化身（或已回滚批次）的在途快照能把旧映射节点写进新缓存，下一次
  /// `retag` 再把它当 baseline 交给新读者（r7a-core-01/02）。
  fn new_index_cache_for(&self, tbl: &TableIdent) -> IndexCache {
    let mut cache = self.new_index_cache();
    let generation = self
      .table_map
      .get(tbl)
      .and_then(|state| state.mapper.as_ref())
      .map_or(0, PageMapper::generation);
    cache.retag(generation);
    cache
  }

  /// 确保该表有 index 缓存，并登记缓存所属 `(branch, term)`。
  ///
  /// reopen 复用（eng-67）只认同一身份：写路径与懒创建建缓存时同样要登记，
  /// 否则后续 reopen 只能退化为整表重建。
  fn ensure_index_cache(&mut self, tbl: &TableIdent) {
    if self
      .table_map
      .get(tbl)
      .is_some_and(|state| state.index_cache.is_some())
    {
      return;
    }
    let cache = self.new_index_cache_for(tbl);
    let branch = self.branch_name.clone();
    let term = self.term;
    let state = self.table_state(tbl);
    state.index_cache = Some(RefCell::new(cache));
    state.cache_branch = Some(branch);
    state.cache_term = term;
    self.rebalance_cache_budget();
  }

  /// 缓存实例数变化后重算均分额度，立即裁剪所有表缓存。
  ///
  /// [`IndexCache`] 只在自身 `insert` 时自检收缩，挂新实例的入口必须主动
  /// 调用这里，否则既有缓存会长期持有旧份额（聚合上限失守）。
  fn rebalance_cache_budget(&mut self) {
    for state in self.table_map.values() {
      if let Some(cache) = &state.index_cache {
        cache.borrow_mut().trim_to_budget();
      }
    }
  }

  /// 全 store 的 index 缓存统计：计数器为累计值，占用为当前存活表的合计。
  pub fn index_cache_stat(&self) -> CacheStat {
    let (mut bytes, mut len) = (0, 0);
    for state in self.table_map.values() {
      let Some(cache) = state.index_cache.as_ref() else {
        continue;
      };
      let cache = cache.borrow();
      bytes += cache.byte_len();
      len += cache.len();
    }
    CacheStat {
      hit: self.index_cache_counter.hit(),
      miss: self.index_cache_counter.miss(),
      evict: self.index_cache_counter.evict(),
      bytes,
      len,
      capacity: self.index_cache_bytes,
    }
  }

  pub fn with_manifest_limit(mut self, manifest_limit: usize) -> Self {
    self.manifest_limit = manifest_limit;
    self
  }

  /// 读路径是否过滤 `expire_ts <= now` 的 entry（默认 true）。
  ///
  /// 关闭后过期项只在 CleanExpired 物理删除后消失。
  pub fn with_ttl_read_filter(mut self, enabled: bool) -> Self {
    self.ttl_read_filter = enabled;
    self
  }

  /// 启用写路径值压缩（zstd level）：`>= SKIP_COMPRESSION_THRESHOLD` 的值
  /// 压后更小才落盘，读路径按 entry 的 compression flag 自动解压。
  pub fn with_compression_level(mut self, level: i32) -> Self {
    self.compression_level = Some(level);
    self
  }

  /// 是否校验读到的 page checksum（默认开启；对齐 `KvOption::skip_verify_checksum`）。
  pub fn with_verify_checksum(self, enabled: bool) -> Self {
    self.io.set_verify_checksum(enabled);
    self
  }

  /// 写后是否延迟 TTL 清理（默认 false，保持内联清理）。
  /// 开启后用 `drain_pending_ttl*` 执行清理。
  pub fn with_deferred_ttl(mut self, defer_ttl: bool) -> Self {
    self.defer_ttl = defer_ttl;
    self
  }

  pub fn pending_ttl_len(&self) -> usize {
    self
      .table_map
      .values()
      .filter(|state| state.ttl_pending)
      .count()
  }

  fn mark_ttl_pending(&mut self, tbl: &TableIdent, now_ms: u64) {
    if let Some(state) = self.table_map.get_mut(tbl)
      && state.next_expire != 0
      && state.next_expire <= now_ms
    {
      state.ttl_pending = true;
    }
  }

  /// 限制同时驻留内存的 table 数；超限驱逐最久未写的表（0 = 不限制）。
  pub fn with_max_open_tables(mut self, max_open_tables: usize) -> Self {
    self.max_open_tables = max_open_tables;
    self
  }

  /// 表级 root 状态字节预算；`0` = 不限制（对齐 C++ `root_meta_cache_size`）。
  ///
  /// 计入 data / segment mapping、index 缓存、压缩字典与 branch 元数据。
  /// 超限时从 LRU 尾驱逐未被读快照引用、字典已持久化的整表状态，之后按需
  /// reopen 重载（不实现 C++ `pending_erase`：Rust 单 actor + Arc 快照生命周期
  /// 天然避免 entry re-enter）。
  ///
  /// `Store::new` 默认 `0`（不限制）；产品路径由 shard 从 `KvOption` 注入。
  pub fn with_root_meta_cache_bytes(mut self, bytes: usize) -> Self {
    self.root_meta_cache_bytes = bytes;
    self
  }

  /// 当前配置的表级 root 状态字节预算（`0` = 不限制）。
  pub fn root_meta_cache_bytes(&self) -> usize {
    self.root_meta_cache_bytes
  }

  /// 所有表 root 状态的近似常驻字节（诊断 / 测试；按当前状态重新汇总）。
  pub fn root_meta_bytes(&self) -> usize {
    self.table_map.values().map(table_state_bytes).sum()
  }

  /// 驱逐某 table 的内存状态（root / mapper / segment / index cache / 字典 /
  /// manifest 元数据），并清 IO 层缓存。detach 或重新 attach 前调用；
  /// 之后访问该表会走 reopen 重新加载。
  pub fn evict_table(&mut self, tbl: &TableIdent) {
    if let Some(state) = self.table_map.remove(tbl) {
      self.root_meta_used = self.root_meta_used.saturating_sub(state.bytes);
    }
    self.touch_seq.remove(tbl);
    self.io.evict_table(tbl);
  }

  /// 表是否已加载到内存（含懒创建的空表）。淘汰 / drop 后立即为 false。
  pub fn is_open(&self, tbl: &TableIdent) -> bool {
    self
      .table_map
      .get(tbl)
      .is_some_and(|state| state.root.is_some())
  }

  /// 已打开表的快照（测试 / 诊断用）：detach 驱逐由 shard 自持分区清单完成，
  /// 不经过本方法。
  pub fn open_table_li(&self) -> Vec<TableIdent> {
    self
      .table_map
      .iter()
      .filter(|(_, state)| state.root.is_some())
      .map(|(tbl, _)| tbl.clone())
      .collect()
  }

  /// 借出（必要时创建）该表的状态槽；只登记槽位，不改变 `root`（打开语义）。
  fn table_state(&mut self, tbl: &TableIdent) -> &mut TableState {
    self.table_map.entry(tbl.clone()).or_default()
  }

  /// 懒创建空表：无 manifest 的首次访问按空分区处理，避免每次请求重复 reopen。
  pub fn open_empty(&mut self, tbl: &TableIdent) {
    if self.is_open(tbl) {
      return;
    }
    let state = self.table_state(tbl);
    state
      .mapper
      .get_or_insert_with(|| PageMapper::new(STORE_ALLOC_MODE, 0));
    state.root = Some(NULL_PAGE);
    state.ttl_root = NULL_PAGE;
    // mapper 必须先就位：缓存 baseline 要按它的 live 代际初始化，否则空缓存
    // 会接受任意代际的回填（r7a-core-01/02）。
    self.ensure_index_cache(tbl);
    self.touch_table(tbl);
  }

  /// 记录 table 最近一次写 / 打开，并按表数上限与字节预算驱逐。
  ///
  /// 两条上限共用同一条 `(表, 代际号)` 队列：触摸与逐出均摊 O(1)，陈旧项
  /// 惰性跳过。两项上限都为 0 时直接返回（默认路径零开销）；字节记账在触摸
  /// 时刷新当前表，写批次收尾再补一次（[`Self::refresh_table_bytes`]）。
  fn touch_table(&mut self, tbl: &TableIdent) {
    if self.max_open_tables == 0 && self.root_meta_cache_bytes == 0 {
      return;
    }
    self.refresh_table_bytes(tbl);
    self.touch_clock += 1;
    let seq = self.touch_clock;
    self.touch_seq.insert(tbl.clone(), seq);
    self.table_order_li.push_back((tbl.clone(), seq));
    // 陈旧项最多占一半，队列长度保持 O(有效表数)。
    let queue_limit = self
      .max_open_tables
      .max(self.touch_seq.len())
      .saturating_mul(2)
      + 64;
    if self.table_order_li.len() > queue_limit {
      self
        .table_order_li
        .retain(|(item, item_seq)| self.touch_seq.get(item) == Some(item_seq));
    }
    while self.max_open_tables != 0 && self.touch_seq.len() > self.max_open_tables {
      let Some((oldest, oldest_seq)) = self.table_order_li.pop_front() else {
        break;
      };
      // 陈旧项 / 已驱逐项：跳过。
      if self.touch_seq.get(&oldest) != Some(&oldest_seq) {
        continue;
      }
      if &oldest == tbl {
        // 当前表刚触摸过，不能驱逐自己；放回队尾并结束本轮。
        self.table_order_li.push_back((oldest, oldest_seq));
        break;
      }
      self.evict_table(&oldest);
    }
    self.evict_over_root_meta(tbl);
  }

  /// 按当前状态重算该表的近似字节并更新合计。
  fn refresh_table_bytes(&mut self, tbl: &TableIdent) {
    let bytes = self.table_map.get(tbl).map_or(0, table_state_bytes);
    let Some(state) = self.table_map.get_mut(tbl) else {
      return;
    };
    self.root_meta_used = self
      .root_meta_used
      .saturating_sub(state.bytes)
      .saturating_add(bytes);
    state.bytes = bytes;
  }

  /// 超出 `root_meta_cache_bytes` 时从 LRU 尾驱逐整表状态。
  ///
  /// 保留当前正在写 / 打开的表；跳过仍被读快照引用（快照独占的 mapping 不会
  /// 因驱逐释放，且 reopen 会打断请求）或字典尚未随 manifest 持久化的表。
  /// 单轮最多扫描一次有效表数：候选全部被保留时接受超预算（例如单表大于预算），
  /// 不旋转队列死循环。
  fn evict_over_root_meta(&mut self, keep: &TableIdent) {
    if self.root_meta_cache_bytes == 0 || self.root_meta_used <= self.root_meta_cache_bytes {
      return;
    }
    let mut scanned = self.touch_seq.len();
    while self.root_meta_used > self.root_meta_cache_bytes && scanned > 0 {
      scanned -= 1;
      let Some((oldest, oldest_seq)) = self.table_order_li.pop_front() else {
        break;
      };
      if self.touch_seq.get(&oldest) != Some(&oldest_seq) {
        continue;
      }
      if &oldest == keep || !self.root_meta_evictable(&oldest) {
        self.table_order_li.push_back((oldest, oldest_seq));
        continue;
      }
      self.evict_table(&oldest);
    }
  }

  /// 表是否可被字节预算驱逐：无活跃读快照，且字典没有未持久化的训练结果。
  fn root_meta_evictable(&self, tbl: &TableIdent) -> bool {
    self.snapshots.live_num(tbl) == 0
      && self.table_map.get(tbl).is_none_or(|state| {
        !state
          .compression
          .as_ref()
          .is_some_and(DictCompression::dirty)
      })
  }

  /// 设置文件空间放大触发阈值（对齐 C++ `file_amplify_factor`）；0 关闭。
  pub fn with_file_amplify_factor(mut self, factor: u8) -> Self {
    self.file_amplify_factor = factor;
    self
  }

  /// 设置 segment 文件空间放大触发阈值（对齐 C++ `segment_file_amplify_factor`）；0 关闭。
  pub fn with_segment_amplify_factor(mut self, factor: u8) -> Self {
    self.segment_file_amplify_factor = factor;
    self
  }

  /// 设置一批 overflow 页的最大指针数（对齐 C++ `overflow_pointers`）。
  pub fn with_overflow_pointers(mut self, overflow_pointers: u8) -> Self {
    self.overflow_pointers = overflow_pointers;
    self
  }

  /// 该表当前字典字节；未启用压缩或无字典返回空。
  fn dict_bytes(&self, tbl: &TableIdent) -> &[u8] {
    self
      .table_map
      .get(tbl)
      .and_then(|state| state.compression.as_ref())
      .map_or(&[], DictCompression::dictionary_bytes)
  }

  /// 字典已随 snapshot 持久化，清除 dirty 标记（对齐 C++ `ClearDirty`）。
  fn clear_dict_dirty(&mut self, tbl: &TableIdent) {
    if let Some(compression) = self
      .table_map
      .get_mut(tbl)
      .and_then(|state| state.compression.as_mut())
    {
      compression.clear_dirty();
    }
  }

  /// 为表装载已有 zstd 字典；之后该表的写入按字典压缩，manifest 会持久化
  /// 字典字节，reopen 时自动恢复。
  pub fn set_dictionary(&mut self, tbl: &TableIdent, dict: Vec<u8>) -> Result<()> {
    let level = self.compression_level.unwrap_or_default();
    let state = self.table_state(tbl);
    let mut compression = state
      .compression
      .take()
      .unwrap_or_else(|| DictCompression::new(level));
    compression.load_dictionary(dict)?;
    self.table_state(tbl).compression = Some(compression);
    self.refresh_table_bytes(tbl);
    Ok(())
  }

  /// 该表是否已装载字典（写入会走 Dictionary 压缩）。
  pub fn has_dictionary(&self, tbl: &TableIdent) -> bool {
    self
      .table_map
      .get(tbl)
      .and_then(|state| state.compression.as_ref())
      .is_some_and(DictCompression::has_dictionary)
  }

  /// 读上下文：快照 + 字典 + 该表 index page 缓存。
  ///
  /// 快照由请求入口取一次并在整个点读 / floor 内复用；上下文不携带当前
  /// `PageMapper`，避免「快照 + 当前映射」混用（27b）。
  fn read_ctx<'a>(
    &'a self,
    tbl: &TableIdent,
    snapshot: &'a MappingSnapshot,
  ) -> fkv_task_read::ReadCtx<'a> {
    let state = self.table_map.get(tbl);
    fkv_task_read::ReadCtx {
      snapshot,
      dict: state.and_then(|state| state.compression.as_ref()),
      index_cache: state.and_then(|state| state.index_cache.as_ref()),
      ttl_filter_now_ms: None,
      yield_budget_us: self.yield_budget_us,
    }
  }

  /// 点读上下文：在 `read_ctx` 基础上带上 TTL 下推阈值。
  ///
  /// 过期项在 value 物化前即返回 `None`，省掉 overflow 链 IO 与解压；
  /// `ttl_read_filter == false` 时阈值仍为 `None`，由调用方事后判定。
  fn read_ctx_at<'a>(
    &'a self,
    tbl: &TableIdent,
    snapshot: &'a MappingSnapshot,
    now_ms: u64,
  ) -> fkv_task_read::ReadCtx<'a> {
    fkv_task_read::ReadCtx {
      ttl_filter_now_ms: self.ttl_read_filter.then_some(now_ms),
      ..self.read_ctx(tbl, snapshot)
    }
  }

  /// 写后精确失效该表 index 缓存：只移除本批次变更/释放的逻辑页。
  ///
  /// 未变更子树的 `IndexNode` 保持有效（对齐 C++ 只替换变更的 `MemIndexPage`）。
  /// core-02 之后逻辑 PageId 可复用（空闲链表），失效依赖 `free_page` 把释放的
  /// PageId 记入 `changed_page_li`：复用分配前旧缓存条目已在本函数中移除。
  fn invalidate_index_cache(&mut self, tbl: &TableIdent) {
    let Some(state) = self.table_map.get_mut(tbl) else {
      return;
    };
    // mapper 缺席（表未打开）时没有 live 代际可对齐，`retag` 保持原 baseline。
    let generation = state.mapper.as_ref().map(PageMapper::generation);
    let changed_li = state
      .mapper
      .as_mut()
      .map(|mapper| mapper.take_changed_page_li())
      .unwrap_or_default();
    if let Some(segment_mapper) = state.segment_mapper.as_mut() {
      let _ = segment_mapper.take_changed_page_li();
    }
    // 批出口不变量：变更记录必须已被消费，禁止跨批次累积（eng-68）。
    debug_assert!(
      state
        .mapper
        .as_ref()
        .is_none_or(|mapper| mapper.changed_page_li().is_empty()),
      "data changed_page_li 未在批出口清空"
    );
    debug_assert!(
      state
        .segment_mapper
        .as_ref()
        .is_none_or(|mapper| mapper.changed_page_li().is_empty()),
      "segment changed_page_li 未在批出口清空"
    );
    let Some(cache) = state.index_cache.as_ref() else {
      return;
    };
    let mut cache = cache.borrow_mut();
    for page_id in changed_li {
      cache.remove(page_id);
    }
    // 未变更页的映射代际仍是当前值：重标后新快照继续命中，旧快照因代际
    // 不符自然 miss（27b）。
    if let Some(generation) = generation {
      cache.retag(generation);
    }
  }

  /// 该表 index 缓存当前页数（诊断 / 测试用）。
  pub fn index_cache_len(&self, tbl: &TableIdent) -> usize {
    self
      .table_map
      .get(tbl)
      .and_then(|state| state.index_cache.as_ref())
      .map_or(0, |cache| cache.borrow().len())
  }

  /// 全量清空该表 index 缓存（reopen / 结构整体重建时使用）。
  fn clear_index_cache(&mut self, tbl: &TableIdent) {
    let Some(state) = self.table_map.get_mut(tbl) else {
      return;
    };
    // truncate 后逻辑页进空闲链，旧条目一律作废；baseline 抬到当前 live 代际，
    // 在途旧快照的回填因代际更早被拒（r7a-core-01）。
    let generation = state.mapper.as_ref().map_or(0, PageMapper::generation);
    if let Some(cache) = state.index_cache.as_ref() {
      cache.borrow_mut().reset(generation);
    }
  }

  /// 让后端预加载 table 目录的 file id -> path 索引，返回索引文件数。
  pub fn preload_path_li(&self, tbl: &TableIdent) -> usize {
    self.io.preload_path_li(tbl)
  }

  /// 外部同步工具改动目录后主动失效后端路径索引。
  pub fn invalidate_path_cache(&self) {
    self.io.invalidate_path_cache();
  }

  pub fn with_branch(mut self, branch_name: impl Into<String>, term: u64) -> Self {
    let branch_name = branch_name.into();
    let normalized = match normalize_branch_name(&branch_name) {
      Some(name) => name.into_string(),
      None => {
        // r9b-03：与同族入口（prepare_attach / adopt_attach / open_branch 均以
        // InvalidArgs 拒绝）及 C++ NormalizeBranchName 的 fail-fast 口径对齐，
        // 归一化失败不再静默沿用非法原名——否则文件身份与 manifest 元数据会
        // 携带非法名落盘，直到某个控制面入口才炸出 InvalidArgs。builder 无
        // Result 通道：debug 构建（含测试）直接断言炸出；release 回退
        // MAIN_BRANCH_NAME 并告警，非法身份运行时可见、不再静默生效。
        debug_assert!(
          is_valid_branch_name(&branch_name),
          "with_branch 收到非法 branch 名 {branch_name:?}：合法字符集 [a-zA-Z0-9-]",
        );
        log::warn!("with_branch 收到非法 branch 名 {branch_name:?}，回退 {MAIN_BRANCH_NAME}");
        MAIN_BRANCH_NAME.to_owned()
      }
    };
    self.io.set_file_identity(&normalized, term);
    self.branch_name = normalized;
    self.term = term;
    self
  }

  pub fn with_pages_per_file_shift(mut self, pages_per_file_shift: u8) -> Self {
    self.pages_per_file_shift = pages_per_file_shift;
    self
  }

  /// attach 时更新 active term；后续 manifest / archive 记录使用该 term。
  pub fn set_term(&mut self, term: u64) {
    self.term = term;
    self.io.set_file_identity(&self.branch_name, term);
  }

  /// 当前 Store 采纳的 term（分支 manifest / 文件身份用）。
  pub fn term(&self) -> u64 {
    self.term
  }

  /// 启用 Store 级大 value：`value.len() >= threshold` 的 upsert 写入 segment 文件。
  pub fn with_large_value(mut self, layout: SegmentLayout, threshold: usize) -> Self {
    self.large_value_layout = Some(layout);
    self.large_value_threshold = threshold;
    self
  }

  pub fn page_size(&self) -> usize {
    self.page_size
  }

  pub async fn get(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<Vec<u8>>> {
    self.get_at(tbl, key, now_ms()).await
  }

  pub async fn get_at(&self, tbl: &TableIdent, key: &[u8], now_ms: u64) -> Result<Option<Vec<u8>>> {
    // 一次请求取一次快照：点读全程（含 overflow 链）只用它，不受并发写影响。
    let Some(snapshot) = self.snapshot(tbl) else {
      return Ok(None);
    };
    if snapshot.root_id() == NULL_PAGE {
      return Ok(None);
    }
    let Some(entry) = read_entry(
      &self.io,
      tbl,
      key,
      self.page_size,
      self.read_ctx_at(tbl, &snapshot, now_ms),
    )
    .await?
    else {
      return Ok(None);
    };
    // large value 的 `entry.value` 是 segment 指针 + metadata 编码，不是用户数据；
    // 绝不能透传给 `get` / `read`（issue 15）。用 `get_large*` 读取。
    if entry.is_large_value {
      return Err(KvError::LargeValueUnsupported);
    }
    Ok(Some(entry.value))
  }

  pub async fn read(&self, tbl: &TableIdent, key: &[u8]) -> Result<Vec<u8>> {
    self.read_at(tbl, key, now_ms()).await
  }

  pub async fn read_at(&self, tbl: &TableIdent, key: &[u8], now_ms: u64) -> Result<Vec<u8>> {
    self
      .get_at(tbl, key, now_ms)
      .await?
      .ok_or(KvError::NotFound)
  }

  pub async fn scan(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    self.scan_at(tbl, begin, end, max_entries, now_ms()).await
  }

  pub async fn scan_at(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    now_ms: u64,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    self
      .scan_opt(tbl, begin, end, max_entries, now_ms, ScanOption::default())
      .await
  }

  pub async fn scan_with_option(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    option: ScanOption,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    self
      .scan_opt(tbl, begin, end, max_entries, now_ms(), option)
      .await
  }

  pub async fn scan_page(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    option: ScanOption,
    page: PageOption,
  ) -> Result<ScanPageResult> {
    self
      .scan_page_at(tbl, begin, end, now_ms(), option, page)
      .await
  }

  pub async fn scan_page_at(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    now_ms: u64,
    option: ScanOption,
    page: PageOption,
  ) -> Result<ScanPageResult> {
    let page = self
      .scan_entry_page_at(tbl, begin, end, now_ms, option, page)
      .await?;
    Ok(ScanPageResult {
      entry_li: page
        .entry_li
        .into_iter()
        .map(|entry| (entry.key, entry.value))
        .collect(),
      has_remaining: page.has_remaining,
    })
  }

  /// 分页扫描，保留每条 entry 的 `expire_ts`。
  pub async fn scan_entry_page(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    option: ScanOption,
    page: PageOption,
  ) -> Result<KvEntryPage> {
    self
      .scan_entry_page_at(tbl, begin, end, now_ms(), option, page)
      .await
  }

  pub async fn scan_entry_page_at(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    now_ms: u64,
    option: ScanOption,
    page: PageOption,
  ) -> Result<KvEntryPage> {
    let Some(snapshot) = self.snapshot(tbl) else {
      return Ok(KvEntryPage::default());
    };
    if snapshot.root_id() == NULL_PAGE {
      return Ok(KvEntryPage::default());
    }
    let state = self.table_map.get(tbl);
    let (dict, index_cache) = (
      state.and_then(|state| state.compression.as_ref()),
      state.and_then(|state| state.index_cache.as_ref()),
    );
    // TTL 过滤在叶子迭代器层完成：过期条目不读 overflow、不占分页配额。
    // 单次扫描从 root 下降到叶子后顺序推进，不再因过滤/字节超限重查整棵树。
    let task_page = scan_page_with_budget(
      &self.io,
      ScanArg {
        tbl,
        snapshot: &snapshot,
        tree: ScanTree::Data,
        begin,
        end,
        page_size: self.page_size,
        max_entries: 0,
        dict,
        index_cache,
        ttl_filter_now_ms: self.ttl_read_filter.then_some(now_ms),
      },
      option,
      page,
      self.yield_budget_us,
    )
    .await?;
    Ok(KvEntryPage {
      entry_li: task_page.entry_li.into_iter().map(KvEntry::from).collect(),
      has_remaining: task_page.has_remaining,
    })
  }

  pub async fn scan_opt(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    now_ms: u64,
    option: ScanOption,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    Ok(
      self
        .scan_entry_opt(tbl, begin, end, max_entries, now_ms, option)
        .await?
        .into_iter()
        .map(|entry| (entry.key, entry.value))
        .collect(),
    )
  }

  /// 范围扫描，保留每条 entry 的 `expire_ts`；TTL 过滤口径与 `scan` 一致。
  pub async fn scan_entry(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
  ) -> Result<Vec<KvEntry>> {
    self
      .scan_entry_at(tbl, begin, end, max_entries, now_ms())
      .await
  }

  pub async fn scan_entry_at(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    now_ms: u64,
  ) -> Result<Vec<KvEntry>> {
    self
      .scan_entry_opt(tbl, begin, end, max_entries, now_ms, ScanOption::default())
      .await
  }

  pub async fn scan_entry_with_option(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    option: ScanOption,
  ) -> Result<Vec<KvEntry>> {
    self
      .scan_entry_opt(tbl, begin, end, max_entries, now_ms(), option)
      .await
  }

  pub async fn scan_entry_opt(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    now_ms: u64,
    option: ScanOption,
  ) -> Result<Vec<KvEntry>> {
    let Some(snapshot) = self.snapshot(tbl) else {
      return Ok(Vec::new());
    };
    if snapshot.root_id() == NULL_PAGE {
      return Ok(Vec::new());
    }
    let state = self.table_map.get(tbl);
    // 两个上限都下推到叶子迭代器：`max_entries` 逐条限量（不整区间物化），
    // TTL 过滤在解 overflow / 解压之前完成，过期条目不读 value、不占配额。
    // 与 `scan_entry_page_at` 同一口径，见 eng-52 / eng-64 / perf-84。
    let entry_li = scan_entries_with_budget(
      &self.io,
      ScanArg {
        tbl,
        snapshot: &snapshot,
        tree: ScanTree::Data,
        begin,
        end,
        page_size: self.page_size,
        max_entries,
        dict: state.and_then(|state| state.compression.as_ref()),
        index_cache: state.and_then(|state| state.index_cache.as_ref()),
        ttl_filter_now_ms: self.ttl_read_filter.then_some(now_ms),
      },
      option,
      self.yield_budget_us,
    )
    .await?;
    Ok(entry_li.into_iter().map(KvEntry::from).collect())
  }

  /// 原地刷新该表的 branch 元数据：以恢复出来的元数据为基线，保留父分支 /
  /// 旧 term 的 `file_range_li` 历史链，末尾同 (branch, term) 区间原地合并。
  fn refresh_branch_metadata(&mut self, tbl: &TableIdent) {
    let branch_name = self.branch_name.clone();
    let term = self.term;
    let page_shift = self.pages_per_file_shift;
    let segment_shift = self.segment_shift();
    let Some(state) = self.table_map.get_mut(tbl) else {
      return;
    };
    if state.mapper.is_none() && state.segment_mapper.is_none() {
      return;
    }
    let max_file_id = state
      .mapper
      .as_ref()
      .map_or(0, |mapper| mapper.max_fp_id() >> page_shift);
    let max_segment_file_id = state
      .segment_mapper
      .as_ref()
      .map_or(0, |mapper| mapper.max_fp_id() >> segment_shift);
    let range = BranchFileRange {
      branch_name: branch_name.clone().into(),
      term,
      max_file_id,
      max_segment_file_id,
    };
    // 只在写 manifest 时借出引用，不再整条克隆回写。
    let metadata = state
      .branch_metadata
      .get_or_insert_with(|| BranchManifestMetadata::new(branch_name, term));
    match metadata.file_range_li.last_mut() {
      Some(last)
        if last.branch_name.as_ref() == range.branch_name.as_ref() && last.term == range.term =>
      {
        last.max_file_id = last.max_file_id.max(range.max_file_id);
        last.max_segment_file_id = last.max_segment_file_id.max(range.max_segment_file_id);
      }
      _ => metadata.file_range_li.push(range),
    }
  }

  /// segment 文件的 file id 位移；未启用大 value 时用后端默认槽位数。
  fn segment_shift(&self) -> u8 {
    self
      .large_value_layout
      .map_or(DEFAULT_SEGMENTS_PER_FILE_SHIFT, |layout| {
        layout.segments_per_file_shift()
      })
  }

  /// 构造 snapshot record。
  ///
  /// segment 段是可选段：只有 `segment_mapping != nullptr` 时才追加
  /// `max_segment_fp_id(varint64) | seg_len(4B) | seg varints`，否则整段省略。
  /// 因此本函数是否写 segment 段只由「该表是否有活跃 segment mapping」决定：
  /// 空的 `PageMapper`（未写过 large value）不算活跃，避免 branch/archive
  /// manifest 多出一段空 segment。
  ///
  /// `max_segment_fp_id` 只在存在活跃 segment mapping 时生效（branch 用它写
  /// 预留给子分支的 segment 起点）。
  fn snapshot_record(
    &self,
    tbl: &TableIdent,
    root_id: PageId,
    ttl_root_id: PageId,
    max_fp_id: Option<FilePageId>,
    max_segment_fp_id: Option<FilePageId>,
    branch_override: Option<&BranchManifestMetadata>,
  ) -> Result<Vec<u8>> {
    let state = self.table_map.get(tbl);
    let mapper = state.and_then(|state| state.mapper.as_ref());
    // 与 C++ `GetSegmentMapper`（replayer.cpp）一致：空 mapping 且高水位为 0
    // 等价于「没有 segment mapper」。`put_large` 会为未命中阈值的批次建出
    // 空 mapper，这类 mapper 不应写进 manifest。
    let segment_mapper = state
      .and_then(|state| state.segment_mapper.as_ref())
      .filter(|mapper| !mapper.tbl().is_empty() || mapper.max_fp_id() > 0);
    let default_metadata: BranchManifestMetadata;
    let branch_metadata = match branch_override {
      Some(branch) => branch,
      None => match state.and_then(|state| state.branch_metadata.as_ref()) {
        Some(branch) => branch,
        None => {
          default_metadata = BranchManifestMetadata::new(self.branch_name.clone(), self.term);
          &default_metadata
        }
      },
    };
    // 直接流式编码 mapping，跳过整表摊平的临时 `Vec<u64>`（eng-76）。
    ManifestBuilder::new().snapshot_with(
      SnapshotArg {
        root_id,
        ttl_root: ttl_root_id,
        max_fp_id: max_fp_id.unwrap_or_else(|| mapper.map_or(0, PageMapper::max_fp_id)),
        dict_bytes: self.dict_bytes(tbl),
        branch_metadata,
        max_segment_fp_id: max_segment_fp_id
          .unwrap_or_else(|| segment_mapper.map_or(0, PageMapper::max_fp_id)),
      },
      mapper
        .into_iter()
        .flat_map(|mapper| mapper.tbl().iter_value_li()),
      segment_mapper.map(|mapper| mapper.tbl().iter_value_li()),
    )
  }

  pub fn page_io(&self) -> &I {
    &self.io
  }

  pub fn mapper(&self, tbl: &TableIdent) -> Option<&PageMapper> {
    self
      .table_map
      .get(tbl)
      .and_then(|state| state.mapper.as_ref())
  }

  pub fn segment_mapper(&self, tbl: &TableIdent) -> Option<&PageMapper> {
    self
      .table_map
      .get(tbl)
      .and_then(|state| state.segment_mapper.as_ref())
  }

  /// 取一份读请求可跨 `await` 持有的 mapper 快照并登记到 GC 保留表。
  ///
  /// 快照固定调用时刻的 data / segment 映射、`root_id` / `ttl_root_id` 与两侧
  /// 分配器高水位；创建代价是 O(1) 共享 chunk 指针表（不再逐 chunk 复制），
  /// 不随库容量增长。快照 `drop` 后弱引用失效，注册表在下一轮 GC 取 retained
  /// set 时惰性摘链，旧页从下一轮 GC 起可回收。
  ///
  /// 返回 `None` 表示该表尚未加载 mapper（无映射可读）。多份快照各自独立登记，
  /// 共享同一份底层 chunk 的只是数据，不是生命周期。
  ///
  /// 所有读入口（`get` / `get_large*` / `floor` / `scan*` 及写路径内部扫描）都在
  /// 请求开始时调用本方法取一次快照，之后整个请求只读该快照（27b）。
  pub fn snapshot(&self, tbl: &TableIdent) -> Option<MappingSnapshot> {
    let state = self.table_map.get(tbl)?;
    let data = state.mapper.as_ref()?;
    Some(self.snapshots.open(
      tbl,
      SnapshotSource {
        data,
        segment: state.segment_mapper.as_ref(),
        root_id: state.root.unwrap_or(NULL_PAGE),
        ttl_root_id: state.ttl_root,
      },
    ))
  }

  /// 该表当前仍被读请求持有的快照数（诊断 / 测试）。
  pub fn snapshot_num(&self, tbl: &TableIdent) -> usize {
    self.snapshots.live_num(tbl)
  }

  pub fn root_id(&self, tbl: &TableIdent) -> Option<PageId> {
    self.table_map.get(tbl).and_then(|state| state.root)
  }

  pub fn ttl_root_id(&self, tbl: &TableIdent) -> Option<PageId> {
    self.table_map.get(tbl).map(|state| state.ttl_root)
  }

  pub fn branch_file_limit(&self) -> Option<FilePageId> {
    self.branch_file_limit
  }

  pub fn branch_segment_limit(&self) -> Option<FilePageId> {
    self.branch_segment_limit
  }

  pub fn into_io(self) -> I {
    self.io
  }
}

/// 本层真正新建的 mapper journal。
///
/// `PageMapper::begin_batch` 在已有外层批次时返回 `false`（嵌套忽略）。结算必须
/// 只处理自己开的 journal：否则外层 `put_large` 的段批次会被内层 `put` 或 TTL
/// 清理的 abort 一起回滚，出现“manifest 已持久、段映射已回滚”的撕裂状态。
#[derive(Debug, Clone, Copy, Default)]
struct MapperBatch {
  data: bool,
  segment: bool,
}

impl<I: PageIo + ManifestIo> Store<I> {
  /// 维护写批次开场：`mapper` / `segment_mapper` 的 mapping、分配器与脏页记录
  /// 整体可回滚，供 compact / truncate / clean_expired 对齐 `put_at` 的提交时机。
  fn begin_mapper_batch(&mut self, tbl: &TableIdent) -> MapperBatch {
    let mut batch = MapperBatch::default();
    let Some(state) = self.table_map.get_mut(tbl) else {
      return batch;
    };
    if let Some(mapper) = state.mapper.as_mut() {
      batch.data = mapper.begin_batch();
    }
    if let Some(segment_mapper) = state.segment_mapper.as_mut() {
      batch.segment = segment_mapper.begin_batch();
    }
    batch
  }

  /// 维护写批次结算：manifest 落盘成功才提交，失败回滚到 `begin_mapper_batch` 前状态。
  ///
  /// 必须先 `append_mapping_log` 再提交：manifest 失败时 journal 仍在，
  /// `abort_batch` 才能把 mapping / 分配器 / 脏页长度恢复到本批之前（core-25）。
  /// 只结算 `batch` 里由本层新建的 journal，嵌套外层批次保持不动。
  fn settle_mapper_batch(&mut self, tbl: &TableIdent, batch: MapperBatch, ok: bool) {
    let Some(state) = self.table_map.get_mut(tbl) else {
      return;
    };
    if batch.data
      && let Some(mapper) = state.mapper.as_mut()
    {
      if ok {
        mapper.commit_batch();
      } else {
        mapper.abort_batch();
      }
    }
    if batch.segment
      && let Some(segment_mapper) = state.segment_mapper.as_mut()
    {
      if ok {
        segment_mapper.commit_batch();
      } else {
        segment_mapper.abort_batch();
      }
    }
    // 回滚后映射内容回到批次前，但代际已继续前进：清空条目并把 baseline 抬到
    // 该 live 代际。在途旧快照（代际早于它）此后的回填全部被缓存拒绝，不会
    // 在下一次 `retag` 被当成 baseline 交给新读者（r7a-core-01）。
    if !ok && batch.data {
      let generation = state.mapper.as_ref().map_or(0, PageMapper::generation);
      if let Some(cache) = state.index_cache.as_ref() {
        cache.borrow_mut().reset(generation);
      }
    }
  }

  fn note_next_expire(&mut self, tbl: &TableIdent, expire_ts: u64) {
    if expire_ts == 0 {
      return;
    }
    let state = self.table_state(tbl);
    if state.next_expire == 0 || expire_ts < state.next_expire {
      state.next_expire = expire_ts;
    }
  }

  fn record_next_expire(&mut self, tbl: &TableIdent, next_expire_ts: u64) {
    if let Some(state) = self.table_map.get_mut(tbl) {
      state.next_expire = next_expire_ts;
    }
  }

  fn note_ttl_batch(&mut self, tbl: &TableIdent, ttl_li: &[BatchEntry]) {
    for entry in ttl_li {
      if entry.op == WriteOp::Upsert
        && let Ok((expire_ts, _)) = parse_ttl_key(&entry.key)
      {
        self.note_next_expire(tbl, expire_ts);
      }
    }
  }

  /// 写后触发：只在已知最早过期时间已到时扫描 TTL 树，对齐 C++ TriggerTTL。
  async fn maybe_clean_expired(&mut self, tbl: &TableIdent, now_ms: u64) -> Result<()> {
    let next = self.table_map.get(tbl).map_or(0, |state| state.next_expire);
    if next == 0 || next > now_ms {
      return Ok(());
    }
    self.clean_expired(tbl, now_ms).await?;
    Ok(())
  }

  /// 执行所有已标记 table 的 TTL 清理（对齐 C++ 独立 CleanExpired 任务）。
  ///
  /// 逐表清理成功后才清该表的 pending 标记；中途失败时失败表及其后的表
  /// 保留 pending，下一轮 drain 继续，不会因一次错误丢失待清理状态。
  ///
  /// 单次 `clean_expired` 有 [`CLEAN_EXPIRED_MAX_ENTRIES`] 上界，命中上界时它
  /// 返回 `next_expire_ts = now`：只有 `next_expire_ts` 仍未到期（或清理无进展）
  /// 才清 pending，否则继续下一轮，避免写停止后过期项滞留在 TTL 树里。
  pub async fn drain_pending_ttl_at(&mut self, now_ms: u64) -> Result<usize> {
    let table_li: Vec<TableIdent> = self
      .table_map
      .iter()
      .filter(|(_, state)| state.ttl_pending)
      .map(|(tbl, _)| tbl.clone())
      .collect();
    let mut cleaned = 0;
    for tbl in table_li {
      loop {
        let stat = self.clean_expired(&tbl, now_ms).await?;
        let more = stat.next_expire_ts != 0 && stat.next_expire_ts <= now_ms;
        // 无进展保护：理论上命中上界必删条目，异常实现下也绝不空转。
        let progressed = stat.ttl_deleted != 0 || stat.deleted != 0;
        if let Some(state) = self.table_map.get_mut(&tbl) {
          state.ttl_pending = more;
        }
        if !more || !progressed {
          break;
        }
      }
      cleaned += 1;
    }
    Ok(cleaned)
  }

  pub async fn drain_pending_ttl(&mut self) -> Result<usize> {
    self.drain_pending_ttl_at(now_ms()).await
  }

  /// 写入口的 key 校验：非空且不超过数据页大小的一半（对齐 C++ `SetBatch`
  /// 的 `data_page_size >> 1` 约束）。整批拒绝，避免错误暴露在页构建深处。
  ///
  /// 所有公开写路径（`put` / `put_at` / `put_large*` / `put_with_term`）
  /// 都汇入 `put_at`，只在此处校验一次。
  fn check_batch_key_li(&self, batch_li: &[BatchEntry]) -> Result<()> {
    let max_len = self.page_size >> 1;
    if batch_li
      .iter()
      .any(|entry| entry.key.is_empty() || entry.key.len() > max_len)
    {
      return Err(KvError::InvalidArgs);
    }
    Ok(())
  }

  pub async fn put(&mut self, tbl: &TableIdent, batch_li: &[BatchEntry]) -> Result<()> {
    self.put_at(tbl, batch_li, now_ms()).await
  }

  /// 带时间注入的写入：对齐 C++ BatchWriteTask，`expire_ts <= now_ms` 的
  /// upsert 不写主树，只删除对应 TTL 索引项。
  ///
  /// 写入过程可回滚：中途失败（如 `OutOfMem`）时 `PageMapper::abort_batch`
  /// 恢复 mapping / 分配器，保证重试从干净状态开始（对齐 C++ `Abort`）。
  ///
  /// data mapper 与 segment mapper 同批（rev-01）：底页上的大 value 被覆盖 / 删除时
  /// 会在 `write_batch` 内释放 segment 页，失败必须一并回滚，否则 live key 指向已回收
  /// 的段，或本批新分配（`put_large`）的段泄漏。
  pub async fn put_at(
    &mut self,
    tbl: &TableIdent,
    batch_li: &[BatchEntry],
    now_ms: u64,
  ) -> Result<()> {
    self.put_at_with_ttl(tbl, batch_li, now_ms, true).await
  }

  /// `put_at` 主体；`inline_ttl = false` 时把写后清理留给调用方在结算外层
  /// journal 之后执行（`put_large` 的段批次尚未结算，内联清理会嵌套回滚）。
  async fn put_at_with_ttl(
    &mut self,
    tbl: &TableIdent,
    batch_li: &[BatchEntry],
    now_ms: u64,
    inline_ttl: bool,
  ) -> Result<()> {
    self.check_batch_key_li(batch_li)?;
    self.touch_table(tbl);
    // `begin_mapper_batch` 只处理已加载的表状态，首次写先建空 mapper。
    self
      .table_state(tbl)
      .mapper
      .get_or_insert_with(|| PageMapper::new(STORE_ALLOC_MODE, 0));
    let batch = self.begin_mapper_batch(tbl);
    let result = self.put_at_inner(tbl, batch_li, now_ms).await;
    self.settle_mapper_batch(tbl, batch, result.is_ok());
    result?;
    // TTL 清理是批次结算后的维护动作；它自带 mapper 批次，不能嵌套在外层 journal 内。
    if inline_ttl {
      self.dispatch_ttl_clean(tbl, now_ms).await;
    }
    // 写入会扩充 data / segment mapping：收尾补记账并按预算驱逐其它表。
    // index 缓存的增长只来自读路径（`&self`），由下一次触摸补记。
    self.refresh_table_bytes(tbl);
    self.evict_over_root_meta(tbl);
    Ok(())
  }

  /// 写后 TTL 维护：延迟模式只标记 pending，内联模式立即清理且失败仅告警。
  async fn dispatch_ttl_clean(&mut self, tbl: &TableIdent, now_ms: u64) {
    if self.defer_ttl {
      self.mark_ttl_pending(tbl, now_ms);
      return;
    }
    if let Err(err) = self.maybe_clean_expired(tbl, now_ms).await {
      // 清理是写入提交后的维护动作：失败不回滚、不影响本次写入结果，
      // 但必须可见（含 Corrupted），不能静默丢弃。
      log::warn!("TTL 内联清理失败（写入已提交） tbl={tbl} now_ms={now_ms} err={err}");
    }
  }

  async fn put_at_inner(
    &mut self,
    tbl: &TableIdent,
    batch_li: &[BatchEntry],
    now_ms: u64,
  ) -> Result<()> {
    let PreparedWriteBatch {
      live_li,
      mut ttl_li,
      expired_num: _,
    } = prepare_write_batch(batch_li, now_ms);
    if live_li.is_empty() && ttl_li.is_empty() {
      return Ok(());
    }
    // 启用压缩时，首次写为该表建立 DictCompression（无字典，standalone 路径）。
    if let Some(level) = self.compression_level {
      self
        .table_map
        .entry(tbl.clone())
        .or_default()
        .compression
        .get_or_insert_with(|| DictCompression::new(level));
      // 自动字典训练（对齐 C++ `SampleAndBuildDictionaryIfNeeded`）：
      // 样本够了就建字典，同批写入即可走 Dictionary 压缩；字典字节随 manifest 持久化。
      let sample_li: Vec<&[u8]> = live_li
        .iter()
        .filter(|entry| !entry.op.is_delete() && !entry.large_value)
        .map(|entry| entry.value.as_slice())
        .collect();
      if let Some(compression) = self
        .table_map
        .get_mut(tbl)
        .and_then(|state| state.compression.as_mut())
      {
        // 对齐 C++ `BuildDictionary`：首次尝试后即置 `training_attempted`，
        // 失败也不重试、样本随 `take` 丢弃；写路径继续按 standalone/None 压缩。
        let _ = compression.maybe_build_dictionary(&sample_li);
      }
    }
    self.ensure_index_cache(tbl);
    let state = self.table_map.entry(tbl.clone()).or_default();
    let root_id = state.root.unwrap_or(NULL_PAGE);
    let ttl_root_id = state.ttl_root;
    let first_manifest = !state.manifest_started;
    let (new_root, new_ttl_root) = {
      let state = self.table_map.entry(tbl.clone()).or_default();
      let mapper = state
        .mapper
        .get_or_insert_with(|| PageMapper::new(STORE_ALLOC_MODE, 0));
      let mut segment_mapper = state.segment_mapper.as_mut();
      let dict = state.compression.as_ref();
      let segment_size = self
        .large_value_layout
        .map_or(0, |layout| layout.segment_size());
      // 底页归并时收集到的旧 TTL 索引删除项（eng-50）。
      let mut ttl_delete_li = Vec::new();
      let new_root = if live_li.is_empty() {
        root_id
      } else {
        write_batch(
          &self.io,
          WriteArg {
            tbl,
            root_id,
            mapper,
            batch_li: &live_li,
            layout: PageLayout::new(
              self.page_size,
              self.data_restart_interval,
              self.index_restart_interval,
              dict,
              self.overflow_pointers,
            ),
            segment_mapper: reborrow_segment(&mut segment_mapper),
            segment_size,
            ttl_delete_sink: Some(&mut ttl_delete_li),
          },
        )
        .await?
      };
      ttl_li.extend(ttl_delete_li);
      let new_ttl_root = if ttl_li.is_empty() {
        ttl_root_id
      } else {
        write_batch(
          &self.io,
          WriteArg {
            tbl,
            root_id: ttl_root_id,
            mapper,
            batch_li: &ttl_li,
            layout: PageLayout::new(
              self.page_size,
              self.data_restart_interval,
              self.index_restart_interval,
              dict,
              self.overflow_pointers,
            ),
            segment_mapper: reborrow_segment(&mut segment_mapper),
            segment_size,
            ttl_delete_sink: None,
          },
        )
        .await?
      };
      (new_root, new_ttl_root)
    };
    // 精确检查分支预留区间：本批实际分配的页越界即整体回滚（eng-54）。
    if let Some(limit) = self.branch_file_limit {
      let mapper_max = self
        .table_map
        .get(tbl)
        .and_then(|state| state.mapper.as_ref())
        .map_or(0, PageMapper::max_fp_id);
      if mapper_max > limit {
        return Err(KvError::OutOfSpace);
      }
    }
    if let Some(limit) = self.branch_segment_limit {
      let segment_max = self
        .table_map
        .get(tbl)
        .and_then(|state| state.segment_mapper.as_ref())
        .map_or(0, PageMapper::max_fp_id);
      if segment_max > limit {
        return Err(KvError::OutOfSpace);
      }
    }
    // 先让数据页落盘，再写 manifest（对齐 C++ WaitWrite → SyncData → FlushManifest），
    // 保证 manifest 引用的页在断电后一定可读。
    self.io.sync_data(tbl).await?;

    self.refresh_branch_metadata(tbl);
    if first_manifest {
      let record = self.snapshot_record(tbl, new_root, new_ttl_root, None, None, None)?;
      self.write_manifest_record(tbl, &record).await?;
      if let Some(state) = self.table_map.get_mut(tbl) {
        state.manifest_size = record.len();
        state.manifest_started = true;
        if let Some(mapper) = state.mapper.as_mut() {
          mapper.clear_dirty_page_li();
        }
        if let Some(segment_mapper) = state.segment_mapper.as_mut() {
          segment_mapper.clear_dirty_page_li();
        }
      }
      self.clear_dict_dirty(tbl);
    } else {
      self.append_mapping_log(tbl, new_root, new_ttl_root).await?;
    }
    // index page 已被 COW 重写，缓存作废。必须等 manifest 落盘成功再失效 / 重标：
    // 失败路径会把本批 mapping 回滚到批次前（`abort_batch`），此时失效既白丢
    // 仍然有效的旧条目，也把缓存 baseline 提前抬到了回滚后并不会被用到的代际。
    // 失败时 `settle_mapper_batch` 已清空缓存并把 baseline 抬到新的 live 代际，
    // 在途旧快照的回填被拒，无需这里兜底。
    self.invalidate_index_cache(tbl);
    if let Some(state) = self.table_map.get_mut(tbl) {
      state.root = Some(new_root);
      state.ttl_root = new_ttl_root;
    }
    self.note_ttl_batch(tbl, &ttl_li);
    Ok(())
  }

  async fn append_mapping_log(
    &mut self,
    tbl: &TableIdent,
    root_id: PageId,
    ttl_root_id: PageId,
  ) -> Result<()> {
    self.refresh_branch_metadata(tbl);
    // 脏页可能覆盖整表（首次写 / truncate 后），manifest 编码是纯 CPU 段，
    // 按固定步长检查段预算，避免一次写请求长时间独占 shard。
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    let record = {
      let state = self.table_map.get(tbl).ok_or(KvError::ResourceMissing)?;
      let mapper = state.mapper.as_ref().ok_or(KvError::ResourceMissing)?;
      let mut builder = ManifestBuilder::new();
      for (seq, &page_id) in mapper.dirty_page_li().iter().enumerate() {
        yield_budget.maybe_yield_every(seq).await;
        builder.update_mapping(page_id, mapper.tbl().get(page_id)?);
      }
      builder.append_branch_metadata(
        state
          .branch_metadata
          .as_ref()
          .ok_or(KvError::ResourceMissing)?,
      )?;
      let segment_entries = if let Some(segment_mapper) = state.segment_mapper.as_ref() {
        let mut bytes = Vec::new();
        for (seq, &page_id) in segment_mapper.dirty_page_li().iter().enumerate() {
          yield_budget.maybe_yield_every(seq).await;
          append_varint32(&mut bytes, page_id);
          append_varint64(&mut bytes, segment_mapper.tbl().get(page_id)?);
        }
        bytes
      } else {
        Vec::new()
      };
      let mut segment_delta = Vec::with_capacity(4 + segment_entries.len());
      segment_delta.extend_from_slice(&(segment_entries.len() as u32).to_le_bytes());
      segment_delta.extend_from_slice(&segment_entries);
      builder.append_segment_mapping(&segment_delta);
      builder.finalize(root_id, ttl_root_id)?
    };
    let (current_size, dict_dirty) = self.table_map.get(tbl).map_or((0, false), |state| {
      (
        state.manifest_size,
        state
          .compression
          .as_ref()
          .is_some_and(DictCompression::dirty),
      )
    });
    // 字典刚训练出来时必须写 snapshot 持久化 dict_bytes，
    // 否则日志增量不含字典，reopen 后无法解压 Dictionary 页（对齐 C++ `dict_dirty`）。
    if dict_dirty
      || (self.manifest_limit != 0
        && current_size.saturating_add(record.len()) > self.manifest_limit)
    {
      let snapshot = self.snapshot_record(tbl, root_id, ttl_root_id, None, None, None)?;
      self.write_manifest_record(tbl, &snapshot).await?;
      if let Some(state) = self.table_map.get_mut(tbl) {
        state.manifest_size = snapshot.len();
      }
      self.clear_dict_dirty(tbl);
    } else {
      self.append_manifest_record(tbl, &record).await?;
      if let Some(state) = self.table_map.get_mut(tbl) {
        state.manifest_size = current_size.saturating_add(record.len());
      }
    }
    // 只有 manifest 成功落盘后才清脏页；失败时保留供上层重试。
    if let Some(state) = self.table_map.get_mut(tbl) {
      if let Some(mapper) = state.mapper.as_mut() {
        mapper.clear_dirty_page_li();
      }
      if let Some(segment_mapper) = state.segment_mapper.as_mut() {
        segment_mapper.clear_dirty_page_li();
      }
    }
    Ok(())
  }

  pub async fn delete(&mut self, tbl: &TableIdent, key: &[u8], timestamp: u64) -> Result<()> {
    self
      .put(tbl, &[BatchEntry::delete(key.to_vec(), timestamp)])
      .await
  }

  /// 切换到一个 branch manifest 并打开独立 Store 状态。
  /// Attach 前重置 Store：清空所有 table 的缓存状态，并设置 branch manifest 模式。
  pub fn prepare_attach(&mut self, branch_name: &str, term: u64) -> Result<()> {
    let normalized = normalize_branch_name(branch_name).ok_or(KvError::InvalidArgs)?;
    self.reset_table_li();
    self.adopt_attach(normalized.as_ref(), term)?;
    self.io.set_file_identity(&self.branch_name, term);
    Ok(())
  }

  /// 新分区 Store 采纳 shard 当前的 attach 身份。
  ///
  /// 与 [`Self::prepare_attach`] 的区别：不清理共享 IO 的路径 / fd 缓存。
  /// shard 的每分区 Store 共用同一后端，attach 时已由首个 Store 统一失效过；
  /// 每个新分区再清一次会让其它分区的缓存反复失效。字段语义与
  /// `prepare_attach` 保持一致（main 无具名 manifest，branch 用固定文件名）。
  pub fn adopt_attach(&mut self, branch_name: &str, term: u64) -> Result<()> {
    let normalized = normalize_branch_name(branch_name).ok_or(KvError::InvalidArgs)?;
    if normalized.as_ref() == MAIN_BRANCH_NAME {
      self.branch_name = MAIN_BRANCH_NAME.to_owned();
      self.manifest_file = None;
    } else {
      self.manifest_file = Some(manifest_file_name(&normalized, 0));
      self.branch_name = normalized.into_string();
    }
    self.branch_file_limit = None;
    self.branch_segment_limit = None;
    self.term = term;
    Ok(())
  }

  fn reset_table_li(&mut self) {
    self.table_map.clear();
    self.root_meta_used = 0;
    self.table_order_li.clear();
    self.touch_seq.clear();
    self.touch_clock = 0;
    self.io.invalidate_path_cache();
  }

  /// 打开具名 branch manifest；`term` 是 attach 后的活跃 term（不是 manifest 名）。
  ///
  /// branch manifest 文件名固定为创建时的 `manifest_<branch>_0`（对齐
  /// `prepare_attach` / `create_branch`），attach term 只决定后续写对象与
  /// `file_range_li` 区间归属。必须在 `install_replay_state` 之前采纳 attach
  /// term：install 依据 term 变化抬升文件边界，并把新区间记到当前 identity；
  /// 若先用 0 回放，term bump 永不触发，区间永远记 0，远端读 `_0` 找不到
  /// attach 后写出的 `_T` 对象。
  pub async fn open_branch(
    &mut self,
    tbl: &TableIdent,
    branch_name: &str,
    term: u64,
  ) -> Result<()> {
    let normalized = normalize_branch_name(branch_name).ok_or(KvError::InvalidArgs)?;
    let manifest_file = manifest_file_name(&normalized, 0);
    let bytes = self
      .io
      .read_manifest_file(tbl, &manifest_file)
      .await?
      .ok_or(KvError::ResourceMissing)?;
    // branch manifest 同样以逻辑 replay size 为追加基准：撕裂 / 损坏尾部不计入，
    // 否则下一次写会落在垃圾之后，reopen 见「损坏 + 完好」直接 Corrupted。
    let (state, size) = fkv_task_reopen::replay_manifest_bytes_with_len(&bytes)?;
    let branch_base_file = (state.max_fp_id >> self.pages_per_file_shift)
      .div_euclid(BRANCH_FILE_RESERVE_FILE_NUM)
      * BRANCH_FILE_RESERVE_FILE_NUM;
    let branch_limit_file = branch_base_file
      .checked_add(BRANCH_FILE_RESERVE_FILE_NUM)
      .ok_or(KvError::InvalidArgs)?;
    let branch_file_limit = branch_limit_file
      .checked_shl(u32::from(self.pages_per_file_shift))
      .ok_or(KvError::InvalidArgs)?;
    let segment_shift = self
      .large_value_layout
      .map_or(DEFAULT_SEGMENTS_PER_FILE_SHIFT, |layout| {
        layout.segments_per_file_shift()
      });
    let branch_segment_limit = if state.max_segment_fp_id > 0 {
      let base_file = (state.max_segment_fp_id >> segment_shift)
        .div_euclid(BRANCH_FILE_RESERVE_FILE_NUM)
        * BRANCH_FILE_RESERVE_FILE_NUM;
      let limit_file = base_file
        .checked_add(BRANCH_FILE_RESERVE_FILE_NUM)
        .ok_or(KvError::InvalidArgs)?;
      Some(
        limit_file
          .checked_shl(u32::from(segment_shift))
          .ok_or(KvError::InvalidArgs)?,
      )
    } else {
      None
    };
    self.branch_name = normalized.clone().into_string();
    self.term = term;
    self.io.set_file_identity(normalized.as_ref(), term);
    self.manifest_file = Some(manifest_file);
    self.branch_file_limit = Some(branch_file_limit);
    self.branch_segment_limit = branch_segment_limit;
    self.install_replay_state(tbl, state, size)?;
    let name = self
      .manifest_file
      .as_deref()
      .ok_or(KvError::ResourceMissing)?;
    self.io.reset_manifest_file(tbl, name, size as u64).await?;
    Ok(())
  }

  /// 写整份 snapshot：branch 模式写具名文件，main 模式 switch 活跃 manifest。
  async fn write_manifest_record(&mut self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    match self.manifest_file.clone() {
      Some(name) => self.io.write_manifest_file(tbl, &name, record).await,
      None => self.io.switch_manifest(tbl, record).await,
    }
  }

  /// 追加 log record：branch 模式追加到具名文件，main 模式追加活跃 manifest。
  async fn append_manifest_record(&mut self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    match self.manifest_file.clone() {
      Some(name) => self.io.append_manifest_file(tbl, &name, record).await,
      None => self.io.append_manifest(tbl, record).await,
    }
  }

  async fn write_snapshot_manifest(
    &mut self,
    tbl: &TableIdent,
    root_id: PageId,
    ttl_root_id: PageId,
  ) -> Result<()> {
    self.refresh_branch_metadata(tbl);
    let record = self.snapshot_record(tbl, root_id, ttl_root_id, None, None, None)?;
    self.write_manifest_record(tbl, &record).await?;
    if let Some(state) = self.table_map.get_mut(tbl) {
      state.manifest_size = record.len();
      state.manifest_started = true;
      if let Some(mapper) = state.mapper.as_mut() {
        mapper.clear_dirty_page_li();
      }
      if let Some(segment_mapper) = state.segment_mapper.as_mut() {
        segment_mapper.clear_dirty_page_li();
      }
    }
    self.clear_dict_dirty(tbl);
    Ok(())
  }

  /// 读取并安装本地 manifest；失败时清空该表内存状态，避免后续写把 log
  /// record 写成 manifest 第一条（远端 manifest 缺失 / 损坏后重试路径）。
  pub async fn reopen(&mut self, tbl: &TableIdent) -> Result<()> {
    let result = self.reopen_inner(tbl).await;
    if result.is_err() {
      self.evict_table(tbl);
    }
    result
  }

  async fn reopen_inner(&mut self, tbl: &TableIdent) -> Result<()> {
    let (state, size) = match self.manifest_file.clone() {
      Some(name) => {
        let bytes = self
          .io
          .read_manifest_file(tbl, &name)
          .await?
          .ok_or(KvError::ResourceMissing)?;
        fkv_task_reopen::replay_manifest_bytes_with_len(&bytes)?
      }
      None => fkv_task_reopen::reopen_with_size(&self.io, tbl).await?,
    };
    self.install_replay_state(tbl, state, size)?;
    match self.manifest_file.clone() {
      Some(name) => self.io.reset_manifest_file(tbl, &name, size as u64).await?,
      None => self.io.reset_manifest(tbl, size as u64).await?,
    }
    self.touch_table(tbl);
    Ok(())
  }

  /// 采纳外部（远端）当前快照：强制刷新 manifest → replay → 原子安装。
  ///
  /// 对齐 C++ `PageManager::InstallExternalSnapshot`：
  /// - 远端有 manifest：复用 [`Self::reopen`] 的 replay + `install_replay_state`；
  /// - 远端无 manifest：只删**本地** manifest 并清空内存表状态，返回
  ///   [`InstallOutcome::WaitingRemote`]，绝不以本地空状态起跑、绝不删远端对象。
  pub async fn install_external_snapshot(&mut self, tbl: &TableIdent) -> Result<InstallOutcome> {
    match self.io.refresh_manifest(tbl).await {
      Ok(()) => {}
      Err(KvError::NotFound | KvError::ResourceMissing) => {
        return self.clear_local_snapshot(tbl).await;
      }
      Err(err) => return Err(err),
    }
    match self.reopen(tbl).await {
      Ok(()) => Ok(InstallOutcome::Installed),
      // refresh 与 replay 之间远端 manifest 消失：同样只清本地。
      Err(KvError::ResourceMissing) => self.clear_local_snapshot(tbl).await,
      Err(err) => Err(err),
    }
  }

  /// 清空本地 manifest 与内存表状态，等待远端状态重新出现。
  async fn clear_local_snapshot(&mut self, tbl: &TableIdent) -> Result<InstallOutcome> {
    self.io.drop_local_manifest(tbl).await?;
    self.evict_table(tbl);
    Ok(InstallOutcome::WaitingRemote)
  }

  fn install_replay_state(
    &mut self,
    tbl: &TableIdent,
    replay: fkv_recovery::ReplayState,
    size: usize,
  ) -> Result<()> {
    // term 变化 = 新文件边界起步（对齐 C++ `Replayer::GetMapper`）：旧 term 的页
    // 按 (branch, term) 寻址，新数据不得复用同一 file id 的旧对象 key 空间。
    let bump = self.term != 0 && replay.branch_metadata.term != self.term;
    // term bump 若会撞到 branch 预留区间上限，就保留当前高水位：文件名带
    // term，不会与旧 term 对象冲突；否则最后一次 bump 会占满区间，后续写
    // 永久 OutOfSpace。
    let bumped_fp_id = next_file_boundary(replay.max_fp_id, self.pages_per_file_shift);
    let max_fp_id = if bump
      && self
        .branch_file_limit
        .is_none_or(|limit| bumped_fp_id < limit)
    {
      bumped_fp_id
    } else {
      replay.max_fp_id
    };
    let bumped_segment_fp_id = next_file_boundary(replay.max_segment_fp_id, self.segment_shift());
    let max_segment_fp_id = if bump
      && self
        .branch_segment_limit
        .is_none_or(|limit| bumped_segment_fp_id < limit)
    {
      bumped_segment_fp_id
    } else {
      replay.max_segment_fp_id
    };
    let mut mapper = PageMapper::with_tbl(replay.mapping_tbl, STORE_ALLOC_MODE, max_fp_id);
    // 由活跃映射反推 `min_file_id` / `empty_file_cnt`，对齐 C++ `Replayer::GetMapper`：
    // 否则 `space_size` 在 reopen 后按 0 起算，`needs_maintain` 口径失真。
    mapper.update_stat_by_scan(self.pages_per_file_shift);
    // 旧缓存能否复用（eng-67）：只有同一 (branch, term) 才可能是同一物理页体系；
    // 跨分支 / 跨 term 的 file id 空间不同，且 term 变化本身已按新文件边界起步。
    let same_identity = !bump
      && self.table_map.get(tbl).is_some_and(|state| {
        state.cache_branch.as_deref() == Some(self.branch_name.as_str())
          && state.cache_term == self.term
      });
    // 恢复字典：即便当前未启用压缩，也要能读回旧盘面里字典压缩的页。
    // 先于任何状态改写执行：字典损坏必须 fail-fast，且不留下半套新状态。
    let dict = if replay.dict_bytes.is_empty() {
      None
    } else {
      let level = self.compression_level.unwrap_or_default();
      let mut dict = DictCompression::new(level);
      // 字典损坏必须 fail-fast：静默丢字典会让后续 snapshot 写出无字典 manifest，
      // 旧压缩页永久不可解压。
      dict
        .load_dictionary(replay.dict_bytes.clone())
        .map_err(|_| KvError::Corrupted)?;
      Some(dict)
    };
    let old_cache = self
      .table_map
      .get_mut(tbl)
      .and_then(|state| state.index_cache.take());
    let mut cache = match old_cache {
      Some(old) if same_identity => old.into_inner(),
      _ => self.new_index_cache(),
    };
    if same_identity {
      // 逐条按新映射校验：逻辑页仍指向条目记录的物理页才保留，等价 C++ 的
      // swizzle 复用。append-only 分配下物理页内容写后不变，因此物理页相同
      // 即缓存页仍有效；映射缺失 / 换页 / 非 baseline 代条目一律丢弃。
      cache.retain_mapped(|page_id| mapper.file_page_id(page_id).ok());
    }
    // 重建后的 mapper 取全新代际（进程内单调，不复用旧值）：baseline 一律
    // 对齐到该代。复用路径靠它让保留条目继续命中（27b）；新建路径靠它挡住
    // 旧化身在途快照的回填（baseline 留在 0 等于接受所有代际，r7a-core-02）。
    cache.retag(mapper.generation());
    // 把父分支 / 旧 term 的 per-file 归属交给后端：远端对象 key 必须按写时解析。
    // 放在字典校验之后：字典损坏时 fail-fast，后端与内存状态都不留半套新值。
    self
      .io
      .set_file_range_li(tbl, &replay.branch_metadata.file_range_li);
    let branch = self.branch_name.clone();
    let term = self.term;
    let state = self.table_state(tbl);
    state.mapper = Some(mapper);
    state.index_cache = Some(RefCell::new(cache));
    state.cache_branch = Some(branch.clone());
    state.cache_term = term;
    // 新状态整体替换：replay 没有字典就必须清空，不能沿用上一分支 / 归档的
    // 字典，否则新写入的页按未持久化字典压缩，重开无法解压。
    state.compression = dict;
    // segment 侧同理：空 mapping + 高水位 0 表示该状态没有 segment 体系，
    // 残留的旧 mapper 会让后续大 value 写入落到上一身份的分段空间。
    state.segment_mapper = (!replay.segment_mapping.is_empty() || replay.max_segment_fp_id > 0)
      .then(|| PageMapper::with_tbl(replay.segment_mapping, STORE_ALLOC_MODE, max_segment_fp_id));
    // 活跃身份以当前安装上下文为准：term 不同步会让下次 reopen 每次判定
    // 「term 变化」而反复抬文件边界；branch 不同步会把新区间挂到旧分支。
    let mut metadata = replay.branch_metadata;
    metadata.branch_name = branch.into();
    metadata.term = term;
    state.branch_metadata = Some(metadata);
    state.root = Some(replay.root_id);
    state.ttl_root = replay.ttl_root_id;
    state.manifest_started = true;
    state.manifest_size = size;
    // 整体替换状态：旧表残留的延迟 TTL 清理标记必须清掉，否则 pending_ttl_len
    // 虚高，下一轮 drain 会做无谓清理。
    state.ttl_pending = false;
    // 对齐 C++ `page_manager.cpp` reopen：TTL 树非空时 next_expire_ts=1，
    // 强制重启后的第一次写触发一次过期清理（而不是等新的 TTL 写入）。
    state.next_expire = u64::from(replay.ttl_root_id != NULL_PAGE);
    // 挂载的缓存改变了实例数：均分额度同步收缩，不能等下一次写才回收。
    self.rebalance_cache_budget();
    // open_branch 不走 `touch_table`：安装后立即补记账，避免 attach 表漏计预算。
    self.refresh_table_bytes(tbl);
    Ok(())
  }

  /// 整表清空：释放两棵树占用的物理页，root / ttl_root 归零，并重置 manifest snapshot。
  ///
  /// data 文件不立即删除，留给 `gc` 回收；逻辑 page id 表保留为 INVALID，
  /// 避免重启后逻辑 id 与旧文件冲突。
  pub async fn truncate(&mut self, tbl: &TableIdent) -> Result<()> {
    self.refresh_branch_metadata(tbl);
    // 整表释放包成一个可回滚批次：manifest 写失败时 mapping / 分配器 / root 全部复原，
    // 调用方重试从干净状态开始（core-25）。
    let batch = self.begin_mapper_batch(tbl);
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    let free_result = {
      let Some(state) = self.table_map.get_mut(tbl) else {
        return Ok(());
      };
      match state.mapper.as_mut() {
        None => {
          state.next_expire = 0;
          // 空表没有页面要释放，但本批已开场：必须结算，否则 journal 悬挂（R2-5）。
          None
        }
        Some(mapper) => {
          let segment = state.segment_mapper.as_mut();
          let result = match free_all_pages(mapper, &mut yield_budget).await {
            Ok(()) => match segment {
              Some(segment) => free_all_pages(segment, &mut yield_budget).await,
              None => Ok(()),
            },
            Err(err) => Err(err),
          };
          Some(result)
        }
      }
    };
    match free_result {
      None => {
        self.settle_mapper_batch(tbl, batch, true);
        return Ok(());
      }
      Some(Ok(())) => {}
      Some(Err(err)) => {
        self.settle_mapper_batch(tbl, batch, false);
        return Err(err);
      }
    }
    let record = match self.snapshot_record(tbl, NULL_PAGE, NULL_PAGE, None, None, None) {
      Ok(record) => record,
      Err(err) => {
        self.settle_mapper_batch(tbl, batch, false);
        return Err(err);
      }
    };
    let write_result = self.write_manifest_record(tbl, &record).await;
    self.settle_mapper_batch(tbl, batch, write_result.is_ok());
    write_result?;
    // manifest 成功：本批脏页 / 变更记录已随 snapshot 落盘，直接丢弃。
    if let Some(state) = self.table_map.get_mut(tbl) {
      state.root = Some(NULL_PAGE);
      state.ttl_root = NULL_PAGE;
      state.manifest_started = true;
      state.manifest_size = record.len();
      state.next_expire = 0;
      if let Some(mapper) = state.mapper.as_mut() {
        mapper.clear_dirty_page_li();
        // 末尾 `clear_index_cache` 整表清空，物理释放无需逐页失效（eng-68）。
        mapper.clear_changed_page_li();
      }
      if let Some(segment_mapper) = state.segment_mapper.as_mut() {
        segment_mapper.clear_dirty_page_li();
        segment_mapper.clear_changed_page_li();
      }
    }
    self.clear_index_cache(tbl);
    Ok(())
  }

  /// 删除所有 key >= position；position 为空等价整表 truncate。
  ///
  /// 只需 key 定位，扫描走 `keys_only`：不读 overflow 链、不解压 value。
  /// 大值 segment / overflow 页的释放由写路径在 merge 旧 entry 时完成。
  ///
  /// 按 [`TRUNCATE_FROM_BATCH`] 分批「扫描 → 删除 → 从 position 续扫」，
  /// 驻留内存与表大小无关（R2-1）：删除已提交，下一轮扫描自然从更靠后的 key 开始。
  pub async fn truncate_from(&mut self, tbl: &TableIdent, position: &[u8]) -> Result<usize> {
    if position.is_empty() {
      self.truncate(tbl).await?;
      return Ok(0);
    }
    let mut deleted = 0usize;
    let mut prev_first: Option<Vec<u8>> = None;
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    loop {
      // 每轮是一批 scan + 删除；批之间也检查段预算，避免超大表连续占线程。
      yield_budget.maybe_yield().await;
      let entry_li = {
        let Some(snapshot) = self.snapshot(tbl) else {
          return Ok(deleted);
        };
        if snapshot.root_id() == NULL_PAGE {
          return Ok(deleted);
        }
        let state = self.table_map.get(tbl);
        scan_entries_with_budget(
          &self.io,
          ScanArg {
            tbl,
            snapshot: &snapshot,
            tree: ScanTree::Data,
            begin: position,
            end: &[],
            page_size: self.page_size,
            max_entries: TRUNCATE_FROM_BATCH,
            dict: state.and_then(|state| state.compression.as_ref()),
            index_cache: state.and_then(|state| state.index_cache.as_ref()),
            ttl_filter_now_ms: None,
          },
          ScanOption {
            reject_large_value: false,
            keys_only: true,
            ..ScanOption::default()
          },
          self.yield_budget_us,
        )
        .await?
      };
      let Some(first) = entry_li.first() else {
        break;
      };
      // 删除未生效时首 key 不会前进：中止而不是空转（防御写路径回归）。
      if prev_first.as_deref() == Some(first.key.as_slice()) {
        log::warn!(
          "truncate_from 扫描无进展，提前结束 tbl={tbl} key_len={}",
          first.key.len()
        );
        break;
      }
      prev_first = Some(first.key.clone());
      let round = entry_li.len();
      let delete_li: Vec<BatchEntry> = entry_li
        .into_iter()
        .map(|entry| BatchEntry::delete(entry.key, u64::MAX))
        .collect();
      self.put(tbl, &delete_li).await?;
      deleted += round;
      // 未扫满一轮说明已到末尾，省掉一次空扫。
      if round < TRUNCATE_FROM_BATCH {
        break;
      }
    }
    Ok(deleted)
  }

  /// 在 `[begin, end)` 内取首条 TTL 条目的过期时间；区间为空返回 `None`。
  ///
  /// 命中上限后的二次探测（是否还有过期项）与未命中后的下一条过期时间
  /// 复用同一份扫描逻辑，避免两处边界条件漂移。
  ///
  /// 坏 TTL key（长度不足 8 字节）不参与定界：把它当 `expire_ts = 0` 会让
  /// `next_expire_ts` 归零、清理永久停摆。这里跳过该条继续向后探测，跳过的
  /// key 收进 `bad_key_li` 由调用方删除：主扫描只覆盖 `[空, expired_end)`，
  /// 出界的坏 key 若只跳过不删除就会永久留在 TTL 树里，每轮探测都为它多读
  /// 一页（r7a-core-05）。连续跳过 [`TTL_BAD_KEY_SKIP_MAX`] 条即放弃本轮。
  ///
  /// `bad_key_li` 的上界是**两阶段累计**的：`clean_expired` 命中扫描上界时会
  /// 先后用同一个 `Vec` 探测「是否还有过期项」与「下一条到期时间」，两段各自
  /// 可能跳过坏 key，合计达到 [`TTL_BAD_KEY_SKIP_MAX`] 即返回
  /// [`TtlPeek::Truncated`]。所以上界不是单次探测的条数，早先一段已跳过的
  /// 坏 key 会让后一段提前截断——这是有意为之：本批坏 key 已收进
  /// `bad_key_li`，下一轮会删掉它们并从新位置继续。
  async fn peek_ttl_expire(
    &self,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
    begin: &[u8],
    end: &[u8],
    bad_key_li: &mut Vec<Vec<u8>>,
  ) -> Result<TtlPeek> {
    let state = self.table_map.get(tbl).ok_or(KvError::ResourceMissing)?;
    let page_opts = PageOption {
      page_entries: 1,
      ..PageOption::default()
    };
    // 跳过坏 key 后 `begin` 严格前进；再加上跳过计数上界，两者共同保证终止。
    let mut begin = begin.to_vec();
    loop {
      let page = scan_page_with_budget(
        &self.io,
        ScanArg {
          tbl,
          snapshot,
          tree: ScanTree::Ttl,
          begin: &begin,
          end,
          page_size: self.page_size,
          max_entries: 1,
          dict: state.compression.as_ref(),
          index_cache: state.index_cache.as_ref(),
          ttl_filter_now_ms: None,
        },
        ScanOption::default(),
        page_opts,
        self.yield_budget_us,
      )
      .await?;
      let Some(entry) = page.entry_li.first() else {
        return Ok(TtlPeek::Empty);
      };
      match parse_ttl_key(&entry.key) {
        Ok((expire_ts, _)) => return Ok(TtlPeek::Expire(expire_ts)),
        Err(_) => {
          bad_key_li.push(entry.key.clone());
          if bad_key_li.len() >= TTL_BAD_KEY_SKIP_MAX {
            return Ok(TtlPeek::Truncated);
          }
          // `key + 0` 是紧邻其后继的最小 key。
          begin.clone_from(&entry.key);
          begin.push(0);
        }
      }
    }
  }

  /// 探测 `expired_end` 之后的首个到期时间；无更多条目返回 `0`。
  ///
  /// 坏 key 长尾命中跳过上界时压回 `now`：本轮已删掉一批坏 key，下一轮从
  /// 新位置继续，直到长尾清空。
  async fn peek_ttl_after(
    &self,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
    expired_end: &[u8],
    now_ms: u64,
    bad_key_li: &mut Vec<Vec<u8>>,
  ) -> Result<u64> {
    Ok(
      match self
        .peek_ttl_expire(tbl, snapshot, expired_end, &[], bad_key_li)
        .await?
      {
        TtlPeek::Expire(expire_ts) => expire_ts,
        TtlPeek::Empty => 0,
        TtlPeek::Truncated => now_ms.max(1),
      },
    )
  }

  /// 清理 TTL tree 中 `expire_ts <= now_ms` 的条目，并删除对应主树 entry。
  ///
  /// 只有主树 entry 的 `expire_ts` 与 TTL key 一致时才删除，避免误删被覆盖的新值。
  /// 单次最多处理 [`CLEAN_EXPIRED_MAX_ENTRIES`] 条，未清完时返回的 `next_expire_ts`
  /// 为 `now_ms`，由下一轮清理继续。
  pub async fn clean_expired(&mut self, tbl: &TableIdent, now_ms: u64) -> Result<CleanExpiredStat> {
    let Some(snapshot) = self.snapshot(tbl) else {
      return Ok(CleanExpiredStat::default());
    };
    let root_id = snapshot.root_id();
    if root_id == NULL_PAGE {
      self.record_next_expire(tbl, 0);
      return Ok(CleanExpiredStat::default());
    }
    let ttl_root_id = snapshot.ttl_root_id();
    if ttl_root_id == NULL_PAGE {
      self.record_next_expire(tbl, 0);
      return Ok(CleanExpiredStat::default());
    }
    // TTL key = `expire_ts(BE) ++ user_key`，所有 `expire_ts <= now` 的条目都
    // 落在 `[空, ttl_key(now+1))`，无需扫整棵 TTL 树（对齐 C++ CleanExpiredKeys）。
    // 扫描与逐条主树复核共用同一快照，整轮清理看到一致的映射。
    let expired_end = ttl_key(now_ms.saturating_add(1), &[]);
    let entry_li = {
      let state = self.table_map.get(tbl).ok_or(KvError::ResourceMissing)?;
      scan_entries(
        &self.io,
        ScanArg {
          tbl,
          snapshot: &snapshot,
          tree: ScanTree::Ttl,
          begin: &[],
          end: &expired_end,
          page_size: self.page_size,
          max_entries: CLEAN_EXPIRED_MAX_ENTRIES,
          dict: state.compression.as_ref(),
          index_cache: state.index_cache.as_ref(),
          ttl_filter_now_ms: None,
        },
      )
      .await?
    };
    // 扫满上界说明可能还有过期项留在 TTL 树里：把 next_expire 压到当前时刻，
    // 让写路径 / drain 的下一次调用继续清理，而不是错误地跳到上界之外的下一条。
    let hit_limit = entry_li.len() >= CLEAN_EXPIRED_MAX_ENTRIES;
    // 恰好扫满不等于还有剩余：从最后一条之后的 key 二次探测。确无剩余时
    // 与未扫满路径一样 peek 一条未过期 TTL 维护 next_expire。
    // 探测阶段跳过的坏 key 收进 `bad_key_li`，本轮一并删除（r7a-core-05）。
    let mut bad_key_li = Vec::new();
    let next_expire_ts = if hit_limit {
      let mut probe_begin = entry_li
        .last()
        .map(|entry| entry.key.clone())
        .ok_or(KvError::Corrupted)?;
      probe_begin.push(0);
      match self
        .peek_ttl_expire(tbl, &snapshot, &probe_begin, &expired_end, &mut bad_key_li)
        .await?
      {
        // 还有过期项（或坏 key 长尾）：下一轮继续清理。
        TtlPeek::Expire(_) | TtlPeek::Truncated => now_ms.max(1),
        TtlPeek::Empty => {
          self
            .peek_ttl_after(tbl, &snapshot, &expired_end, now_ms, &mut bad_key_li)
            .await?
        }
      }
    } else {
      self
        .peek_ttl_after(tbl, &snapshot, &expired_end, now_ms, &mut bad_key_li)
        .await?
    };
    let mut data_delete_li = Vec::new();
    let mut ttl_delete_li = Vec::new();
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    for (seq, entry) in entry_li.into_iter().enumerate() {
      // 每条过期项要做一次主树点读复核，按步长检查段预算。
      yield_budget.maybe_yield_every(seq).await;
      let Ok((expire_ts, user_key)) = parse_ttl_key(&entry.key) else {
        // 坏 TTL key 直接清掉，避免永久占空间。
        // TTL 索引项由引擎写入、版本号恒为 0，墙钟即可压过。
        ttl_delete_li.push(BatchEntry::delete(entry.key, expired_delete_ts(0)));
        continue;
      };
      if expire_ts > now_ms {
        continue;
      }
      if let Some(data_entry) = read_entry(
        &self.io,
        tbl,
        user_key,
        self.page_size,
        // 清理路径需要拿到过期项本身，不能下推过滤。
        self.read_ctx(tbl, &snapshot),
      )
      .await?
        && data_entry.expire_ts == expire_ts
      {
        // 删除时间戳必须压过被删条目的版本号：用户 ts 可能是 µs / ns 量级，
        // 直接用毫秒 `now_ms` 会让 delete 在折叠中落败、过期数据永不物理删除。
        data_delete_li.push(BatchEntry::delete(
          user_key.to_vec(),
          expired_delete_ts(data_entry.timestamp),
        ));
      }
      ttl_delete_li.push(BatchEntry::delete(entry.key, expired_delete_ts(0)));
    }
    // 探测阶段跳过的坏 key 落在主扫描上界之外，只跳过不删除就会永久留在
    // TTL 树里，每轮探测都为它多读一页。这里与过期项同批物理删除，逐轮推进
    // 直到长尾清空（r7a-core-05）。
    ttl_delete_li.extend(
      bad_key_li
        .into_iter()
        .map(|key| BatchEntry::delete(key, expired_delete_ts(0))),
    );
    // 读阶段结束：释放快照，后续批次改写当前 mapper。
    drop(snapshot);
    if ttl_delete_li.is_empty() {
      self.record_next_expire(tbl, next_expire_ts);
      return Ok(CleanExpiredStat {
        deleted: 0,
        ttl_deleted: 0,
        next_expire_ts,
      });
    }
    // data 树与 TTL 树两次 write_batch 作为一个批次：任一次失败都回滚
    // mapper 分配，避免 OOM 重试时半修改状态。
    // 提交推迟到 manifest 落盘成功之后，失败时 `abort_batch` 才能真正回滚（core-25）。
    let batch = self.begin_mapper_batch(tbl);
    let roots_result = {
      let state = self
        .table_map
        .get_mut(tbl)
        .ok_or(KvError::ResourceMissing)?;
      let mapper = state.mapper.as_mut().ok_or(KvError::ResourceMissing)?;
      let mut segment_mapper = state.segment_mapper.as_mut();
      let dict = state.compression.as_ref();
      let segment_size = self
        .large_value_layout
        .map_or(0, |layout| layout.segment_size());
      let result: Result<(PageId, PageId)> = async {
        let new_root = if data_delete_li.is_empty() {
          root_id
        } else {
          write_batch(
            &self.io,
            WriteArg {
              tbl,
              root_id,
              mapper,
              batch_li: &data_delete_li,
              layout: PageLayout::new(
                self.page_size,
                self.data_restart_interval,
                self.index_restart_interval,
                dict,
                self.overflow_pointers,
              ),
              segment_mapper: reborrow_segment(&mut segment_mapper),
              segment_size,
              ttl_delete_sink: None,
            },
          )
          .await?
        };
        let new_ttl_root = write_batch(
          &self.io,
          WriteArg {
            tbl,
            root_id: ttl_root_id,
            mapper,
            batch_li: &ttl_delete_li,
            layout: PageLayout::new(
              self.page_size,
              self.data_restart_interval,
              self.index_restart_interval,
              dict,
              self.overflow_pointers,
            ),
            segment_mapper: reborrow_segment(&mut segment_mapper),
            segment_size,
            ttl_delete_sink: None,
          },
        )
        .await?;
        Ok((new_root, new_ttl_root))
      }
      .await;
      result
    };
    let (new_root, new_ttl_root) = match roots_result {
      Ok(roots) => roots,
      Err(err) => {
        self.settle_mapper_batch(tbl, batch, false);
        return Err(err);
      }
    };
    let log_result = async {
      self.io.sync_data(tbl).await?;
      self.append_mapping_log(tbl, new_root, new_ttl_root).await
    }
    .await;
    self.settle_mapper_batch(tbl, batch, log_result.is_ok());
    log_result?;
    // manifest 成功后再失效：失败时缓存保持原代际，不会出现回滚后 baseline 悬空。
    self.invalidate_index_cache(tbl);
    if let Some(state) = self.table_map.get_mut(tbl) {
      state.root = Some(new_root);
      state.ttl_root = new_ttl_root;
    }
    self.record_next_expire(tbl, next_expire_ts);
    Ok(CleanExpiredStat {
      deleted: data_delete_li.len(),
      ttl_deleted: ttl_delete_li.len(),
      next_expire_ts,
    })
  }

  /// 列举该表目录下全部归档 manifest 的 `(tag, term)`，按 `(tag, term)` 升序去重。
  ///
  /// 语义对齐 C++ `HandleGlobalListArchiveTagsRequest`（`src/eloq_store.cpp:2133`）：
  /// - **不按 branch 过滤**：归档清单是表级 inventory，跨 branch 的同名 tag 也要
  ///   出现在结果里（按 `(tag, term)` 全等去重，与 C++ `sort` + `unique` 一致）；
  /// - 只解析文件名，不读 manifest 内容，也不需要表已打开；
  /// - `Cloud` 后端下 `list_manifest_file_name_li` 会合并本地 cache 与远端对象
  ///   （只列举 manifest，不下载 data / segment）。
  ///
  /// 与 `prune_archive_li` 的「按 tag 倒序保留最近 N 份」不同：保留策略只决定删
  /// 谁，控制面列举给的是完整清单，因此两者排序方向不共享。
  pub async fn list_archive_tags(&self, tbl: &TableIdent) -> Result<Vec<ArchiveTagInfo>> {
    let mut entry_li = Vec::new();
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    for name in self.io.list_manifest_file_name_li(tbl).await? {
      yield_budget.maybe_yield().await;
      let Some((_, term, Some(tag))) = parse_manifest_file_name(&name) else {
        continue;
      };
      entry_li.push(ArchiveTagInfo {
        tag: tag.to_owned(),
        term,
      });
    }
    entry_li.sort();
    entry_li.dedup();
    Ok(entry_li)
  }

  /// [`Self::list_archive_tags`] 的 `_li` 命名别名（fkv 内部 Vec 后缀习惯）。
  pub async fn list_archive_tag_li(&self, tbl: &TableIdent) -> Result<Vec<ArchiveTagInfo>> {
    self.list_archive_tags(tbl).await
  }

  /// 生成一份 archive manifest，并按 tag 倒序保留最近 `num_retained` 份。
  pub async fn create_archive(
    &mut self,
    tbl: &TableIdent,
    tag: &str,
    num_retained: u32,
  ) -> Result<ArchiveInfo> {
    if !is_safe_tag(tag) {
      return Err(KvError::InvalidArgs);
    }
    // 表未打开 / 已被 drop：C++ `CreateArchive` 对 `FindRoot == NotFound`
    // 直接成功且不落文件；返回空回执，避免归档线程“复活”已删表。
    let Some(state) = self.table_map.get(tbl) else {
      return Ok(ArchiveInfo {
        name: String::new(),
        bytes: 0,
        removed_num: 0,
      });
    };
    // `open_empty` 的空表没有真实 manifest：C++ `CreateArchive` 对
    // `FindRoot == NotFound` 直接成功且不落文件，不能写空归档。
    if !state.manifest_started {
      return Ok(ArchiveInfo {
        name: String::new(),
        bytes: 0,
        removed_num: 0,
      });
    }
    let Some(root_id) = state.root else {
      return Err(KvError::ResourceMissing);
    };
    let ttl_root_id = state.ttl_root;
    let branch = normalize_branch_name(&self.branch_name).ok_or(KvError::InvalidArgs)?;
    self.refresh_branch_metadata(tbl);
    // 对齐 C++ `CreateArchive`：`segment_mapper_ == nullptr` 时传 nullptr，
    // snapshot 不写可选 segment 段；有活跃 segment mapping 时由
    // `snapshot_record` 统一写入。
    let record = self.snapshot_record(tbl, root_id, ttl_root_id, None, None, None)?;
    let name = archive_file_name(&branch, self.term, tag);
    self.io.write_manifest_file(tbl, &name, &record).await?;
    let removed_num = self.prune_archive_li(tbl, &branch, num_retained).await?;
    Ok(ArchiveInfo {
      name,
      bytes: record.len(),
      removed_num,
    })
  }

  /// 按 tag 重新打开归档快照。
  ///
  /// - `tag` 为空：等价 [`Self::reopen`]（C++ `ReopenRequest`：empty = latest
  ///   available snapshot），不做归档选择。
  /// - `tag` 非空：在当前分支里选该 tag 的归档 manifest，replay 后把该快照
  ///   **提升为活跃状态**（写回活跃 manifest），后续读写继续基于它。
  ///   多 term 时取最高 term —— C++ Cloud 用当前 process term 精确匹配
  ///   （`CloudStoreMgr::RefreshManifest`），但 fkv 的归档跨 term 保留且归档名
  ///   自带 term，取最高 term 可在重启 / term 推进后仍按 tag 恢复旧快照。
  ///
  /// `ResourceMissing` 表示该 tag 没有归档（或 manifest 已被并发删除），由
  /// shard 的 reopen 延迟队列决定是否重试。
  pub async fn reopen_archive(&mut self, tbl: &TableIdent, tag: &str) -> Result<ArchiveInfo> {
    let branch = normalize_branch_name(&self.branch_name).ok_or(KvError::InvalidArgs)?;
    if tag.is_empty() {
      self.reopen(tbl).await?;
      let state = self.table_map.get(tbl).ok_or(KvError::ResourceMissing)?;
      let name = self
        .manifest_file
        .clone()
        .unwrap_or_else(|| manifest_file_name(MAIN_BRANCH_NAME, 0));
      return Ok(ArchiveInfo {
        name,
        bytes: state.manifest_size,
        removed_num: 0,
      });
    }
    if !is_safe_tag(tag) {
      return Err(KvError::InvalidArgs);
    }
    let mut selected: Option<(u64, String)> = None;
    for name in self.io.list_manifest_file_name_li(tbl).await? {
      let Some((archive_branch, term, Some(archive_tag))) = parse_manifest_file_name(&name) else {
        continue;
      };
      if archive_branch != branch.as_ref() || archive_tag != tag {
        continue;
      }
      if selected
        .as_ref()
        .is_none_or(|(best_term, _)| term >= *best_term)
      {
        selected = Some((term, name));
      }
    }
    let Some((_, name)) = selected else {
      return Err(KvError::ResourceMissing);
    };
    let bytes = self
      .io
      .read_manifest_file(tbl, &name)
      .await?
      .ok_or(KvError::ResourceMissing)?;
    let (replay, size) = fkv_task_reopen::replay_manifest_bytes_with_len(&bytes)?;
    self.install_replay_state(tbl, replay, size)?;
    // 归档快照来自旧 term 时，把活跃身份改写为当前 (branch, term)：file range
    // 历史链保留旧 term 的对象 key，新写入从当前 term 继续（install 已按 term
    // 变化抬过文件边界）。
    if let Some(metadata) = self
      .table_map
      .get_mut(tbl)
      .and_then(|state| state.branch_metadata.as_mut())
    {
      metadata.branch_name = branch.as_ref().into();
      metadata.term = self.term;
    }
    let (root, ttl_root) = {
      let state = self.table_map.get(tbl).ok_or(KvError::ResourceMissing)?;
      (state.root.ok_or(KvError::ResourceMissing)?, state.ttl_root)
    };
    if let Err(err) = self.write_snapshot_manifest(tbl, root, ttl_root).await {
      // 内存已切到归档快照，但活跃 manifest 发布失败；清空表状态，避免下一次
      // 写以归档 root 给旧 manifest 追加 log（或写 log-only manifest）。
      self.evict_table(tbl);
      return Err(err);
    }
    Ok(ArchiveInfo {
      name,
      bytes: bytes.len(),
      removed_num: 0,
    })
  }

  /// 删除当前分支下指定 tag 的归档 manifest（全部 term），返回删除条数。
  ///
  /// 对齐 C++ `ArchiveRequest::Action::Delete` 的幂等删除语义（文件不存在不算
  /// 失败）；与 `delete_branch` 一致按 tag 扫全部 term，这样 term 推进后仍能删掉
  /// 旧归档（C++ 只按当前 process term 定位）。
  pub async fn delete_archive(&mut self, tbl: &TableIdent, tag: &str) -> Result<usize> {
    if tag.is_empty() || !is_safe_tag(tag) {
      return Err(KvError::InvalidArgs);
    }
    let branch = normalize_branch_name(&self.branch_name).ok_or(KvError::InvalidArgs)?;
    let mut removed = 0;
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    for name in self.io.list_manifest_file_name_li(tbl).await? {
      yield_budget.maybe_yield().await;
      let Some((archive_branch, _, Some(archive_tag))) = parse_manifest_file_name(&name) else {
        continue;
      };
      if archive_branch != branch.as_ref() || archive_tag != tag {
        continue;
      }
      self.io.remove_manifest_file(tbl, &name).await?;
      removed += 1;
    }
    Ok(removed)
  }

  /// 删除 branch 的所有 manifest / archive 文件（不允许 main 或 active branch）。
  pub async fn delete_branch(&mut self, tbl: &TableIdent, branch_name: &str) -> Result<usize> {
    let normalized = normalize_branch_name(branch_name).ok_or(KvError::InvalidArgs)?;
    let active = normalize_branch_name(&self.branch_name).ok_or(KvError::InvalidArgs)?;
    if normalized.as_ref() == MAIN_BRANCH_NAME || normalized == active {
      return Err(KvError::InvalidArgs);
    }
    let mut removed = 0;
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    for name in self.io.list_manifest_file_name_li(tbl).await? {
      yield_budget.maybe_yield().await;
      let Some((branch, ..)) = parse_manifest_file_name(&name) else {
        continue;
      };
      if branch == normalized.as_ref() {
        self.io.remove_manifest_file(tbl, &name).await?;
        removed += 1;
      }
    }
    Ok(removed)
  }

  async fn prune_archive_li(
    &self,
    tbl: &TableIdent,
    branch: &str,
    num_retained: u32,
  ) -> Result<usize> {
    if num_retained == 0 {
      return Ok(0);
    }
    let mut archive_li: Vec<(String, u64, String)> = self
      .io
      .list_manifest_file_name_li(tbl)
      .await?
      .into_iter()
      .filter_map(|name| {
        let (archive_branch, term, tag) = parse_manifest_file_name(&name)?;
        let tag = tag?.to_owned();
        if archive_branch == branch {
          Some((tag, term, name))
        } else {
          None
        }
      })
      .collect();
    // 同一 tag 可能有多个 term：保留最高 term（与 `reopen_archive` 选择一致），
    // 再按 tag 倒序保留最近 `num_retained` 个标签。
    archive_li.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    let mut removed = 0;
    let mut kept_tag_set = HashSet::default();
    for (tag, _, name) in archive_li {
      yield_budget.maybe_yield().await;
      let keep = if kept_tag_set.contains(&tag) {
        false
      } else if kept_tag_set.len() < num_retained as usize {
        kept_tag_set.insert(tag);
        true
      } else {
        false
      };
      if !keep {
        self.io.remove_manifest_file(tbl, &name).await?;
        removed += 1;
      }
    }
    Ok(removed)
  }

  pub async fn compact_file(
    &mut self,
    tbl: &TableIdent,
    file_id: fkv_types::FileId,
    pages_per_file_shift: u8,
  ) -> Result<usize> {
    self
      .compact_file_li(tbl, &[file_id], pages_per_file_shift)
      .await
  }

  /// 一次 compaction 搬多个文件：只开一个 mapper 批次、落一次 manifest，
  /// 对齐 C++ `DoCompactDataFile` 覆盖 `begin_file_id..end_file_id` 的行为。
  async fn compact_file_li(
    &mut self,
    tbl: &TableIdent,
    file_li: &[fkv_types::FileId],
    pages_per_file_shift: u8,
  ) -> Result<usize> {
    let page_size = self.page_size;
    let (moved, batch) = {
      let mapper = self
        .table_map
        .get_mut(tbl)
        .and_then(|state| state.mapper.as_mut())
        .ok_or(KvError::ResourceMissing)?;
      let batch = MapperBatch {
        data: mapper.begin_batch(),
        segment: false,
      };
      let moved = match fkv_task_compact::compact_file_li(
        &self.io,
        tbl,
        mapper,
        file_li,
        page_size,
        pages_per_file_shift,
      )
      .await
      {
        // 提交推迟到 manifest 落盘成功之后（core-25）：失败时 journal 仍在，
        // `settle_mapper_batch` 的 abort 才能回滚这次物理搬迁。
        Ok(count) => count,
        Err(err) => {
          if batch.data {
            mapper.abort_batch();
          }
          return Err(err);
        }
      };
      // 搬空的文件并入分配器空间统计，对齐 C++ `UpdateStat(min_file_id, empty_file_cnt)`。
      mapper.update_stat_by_scan(pages_per_file_shift);
      (moved, batch)
    };
    // 没有搬迁就没有 mapping 变化：不要为 open_empty 的空表追加一条 log
    // （manifest 首条记录必须是 snapshot，否则 reopen 解析失败）。
    if moved == 0 {
      self.settle_mapper_batch(tbl, batch, true);
      return Ok(0);
    }
    let state = self.table_map.get(tbl);
    let root_id = state.and_then(|state| state.root).unwrap_or(NULL_PAGE);
    let ttl_root_id = state.map_or(NULL_PAGE, |state| state.ttl_root);
    // 与 put / clean_expired 同序：搬迁页先落盘，再追加 manifest（eng-89）。
    // 否则 manifest 已持久而新页仍在 page cache，崩溃后存活页会指向垃圾 / 零页，
    // 且随后的 gc 可能已删掉旧文件里唯一已落盘的副本。
    let log_result = async {
      self.io.sync_data(tbl).await?;
      self.append_mapping_log(tbl, root_id, ttl_root_id).await
    }
    .await;
    // compact 只换物理 `FilePageId`，逻辑页内容不变，索引缓存仍有效；
    // 若留到下一次 put，`invalidate_index_cache` 会把搬迁过的热点页成批逐出（eng-68）。
    // 搬迁使 mapper 代际前进：提交成功后把仍有效条目重标到新代（27b）。
    if log_result.is_ok()
      && let Some(state) = self.table_map.get_mut(tbl)
    {
      let generation = state.mapper.as_ref().map(PageMapper::generation);
      if let Some(mapper) = state.mapper.as_mut() {
        mapper.clear_changed_page_li();
      }
      if let (Some(generation), Some(cache)) = (generation, state.index_cache.as_ref()) {
        cache.borrow_mut().retag(generation);
      }
    }
    self.settle_mapper_batch(tbl, batch, log_result.is_ok());
    log_result?;
    Ok(moved)
  }
}

/// 大 value 写入目标：plain 直接按 segment 写；fixed 先落调用方注册 buffer 再写。
///
/// 两条路径只在“value 从哪取”上不同，提交 / 回滚骨架由 `put_large_impl` 统一。
trait LargeSink<I: SegmentIo> {
  /// 写入前的长度校验与搬运；plain 路径为 no-op。
  fn prepare(&mut self, value: &[u8]) -> Result<()>;

  async fn write(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    mapper: &mut PageMapper,
    layout: SegmentLayout,
    value: &[u8],
    metadata: &[u8],
  ) -> Result<WrittenLargeValue>;
}

struct PlainSink;

impl<I: SegmentIo> LargeSink<I> for PlainSink {
  fn prepare(&mut self, _value: &[u8]) -> Result<()> {
    Ok(())
  }

  async fn write(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    mapper: &mut PageMapper,
    layout: SegmentLayout,
    value: &[u8],
    metadata: &[u8],
  ) -> Result<WrittenLargeValue> {
    write_large_value(io, tbl, mapper, layout, value, metadata).await
  }
}

/// fixed buffer 写入目标：value 先拷入 buffer，尾部保留区不写出。
struct FixedSink<'a> {
  buf: &'a mut FixedBuffer,
  value_len: usize,
}

impl<I: FixedSegmentIo> LargeSink<I> for FixedSink<'_> {
  fn prepare(&mut self, value: &[u8]) -> Result<()> {
    let dst = self.buf.as_mut_slice().ok_or(KvError::Busy)?;
    if value.len() > dst.len() {
      return Err(KvError::InvalidArgs);
    }
    dst[..value.len()].copy_from_slice(value);
    self.value_len = value.len();
    Ok(())
  }

  async fn write(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    mapper: &mut PageMapper,
    layout: SegmentLayout,
    _value: &[u8],
    metadata: &[u8],
  ) -> Result<WrittenLargeValue> {
    write_value_fixed_with_len(io, tbl, mapper, layout, self.buf, self.value_len, metadata).await
  }
}

/// 命中 large path 的 entry：只替换 value / metadata，避免再复制整份原始大 value。
fn large_entry(entry: &BatchEntry, content: Vec<u8>) -> BatchEntry {
  BatchEntry {
    key: entry.key.clone(),
    value: content,
    timestamp: entry.timestamp,
    expire_ts: entry.expire_ts,
    op: entry.op,
    large_value: true,
    overflow: entry.overflow,
    metadata: Vec::new(),
  }
}

impl<I: PageIo + ManifestIo + SegmentIo> Store<I> {
  /// 按 key 折叠本批写入，保留 timestamp 最大的版本（相等时后出现者胜）。
  ///
  /// 与 `prepare_write_batch` 的主树折叠语义一致；`put_large` 必须在落 segment
  /// 之前先折叠，否则被折叠条目的段链既无树引用也不会 `free_value`，映射与空间
  /// 要泄漏到下一次段压缩（R4-05）。
  fn fold_latest(batch_li: &[BatchEntry]) -> Vec<&BatchEntry> {
    let mut winner_map: BTreeMap<&[u8], &BatchEntry> = BTreeMap::new();
    for entry in batch_li {
      match winner_map.entry(entry.key.as_slice()) {
        Entry::Occupied(mut slot) => {
          if entry.timestamp >= slot.get().timestamp {
            slot.insert(entry);
          }
        }
        Entry::Vacant(slot) => {
          slot.insert(entry);
        }
      }
    }
    winner_map.into_values().collect()
  }

  /// Store 级大 value 写入：超过阈值的 upsert 先写 segment 文件，再写 data page。
  pub async fn put_large(&mut self, tbl: &TableIdent, batch_li: &[BatchEntry]) -> Result<()> {
    self.put_large_impl(tbl, batch_li, &mut PlainSink).await
  }

  /// 两条大 value 写路径共享的骨架：开批、逐条落 segment、统一提交 / 回滚。
  async fn put_large_impl<S: LargeSink<I>>(
    &mut self,
    tbl: &TableIdent,
    batch_li: &[BatchEntry],
    sink: &mut S,
  ) -> Result<()> {
    let Some(layout) = self.large_value_layout else {
      return Err(KvError::InvalidArgs);
    };
    // 与 `put_at` 同一校验点前移：空 / 超长 key 必须在落 segment 之前拒绝，
    // 否则会在写完整批段页后才 abort，白写且可能留下孤儿段。
    self.check_batch_key_li(batch_li)?;
    // 与 `put_at` 用同一时间基准判定过期，避免折叠后仍为过期 upsert 落段。
    let now_ms = now_ms();
    let folded_li = Self::fold_latest(batch_li);
    let mut write_li = Vec::with_capacity(folded_li.len());
    // segment 分配与 data page 写入作为一个批次回滚：`self.put` 失败时
    // 释放本次分配的 segment 映射。
    let batch = {
      let segment_mapper = self
        .table_map
        .entry(tbl.clone())
        .or_default()
        .segment_mapper
        .get_or_insert_with(|| PageMapper::new(STORE_ALLOC_MODE, 0));
      MapperBatch {
        data: false,
        segment: segment_mapper.begin_batch(),
      }
    };
    let mut result = Ok(());
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    for (seq, entry) in folded_li.into_iter().enumerate() {
      // 大 value 批次可能很多条，每条都可能写整条 segment 链；按步长让出。
      yield_budget.maybe_yield_every(seq).await;
      // 过期 upsert 由 `put_at` 折叠成 delete（并清 TTL 索引），不落段。
      if entry.op == WriteOp::Upsert
        && !entry.large_value
        && entry.value.len() >= self.large_value_threshold
        && !is_expired(entry.expire_ts, now_ms)
      {
        if let Err(err) = sink.prepare(&entry.value) {
          result = Err(err);
          break;
        }
        // 比树中版本更旧的 upsert 会被 merge 丢弃；提前跳过，避免为落败版本
        // 分配 segment（映射 / 磁盘 / branch file id 预留区间永久泄漏）。
        let base_ts = match self.base_timestamp(tbl, &entry.key).await {
          Ok(ts) => ts,
          Err(err) => {
            // 不能 `?` 直接返回：segment MapperBatch 已开场，必须走到统一结算点。
            result = Err(err);
            break;
          }
        };
        if let Some(base_ts) = base_ts
          && entry.timestamp < base_ts
        {
          continue;
        }
        let Some(segment_mapper) = self
          .table_map
          .get_mut(tbl)
          .and_then(|state| state.segment_mapper.as_mut())
        else {
          result = Err(KvError::ResourceMissing);
          break;
        };
        match sink
          .write(
            &self.io,
            tbl,
            segment_mapper,
            layout,
            &entry.value,
            &entry.metadata,
          )
          .await
        {
          Ok(written) => write_li.push(large_entry(entry, written.content)),
          Err(err) => {
            result = Err(err);
            break;
          }
        }
      } else {
        // 未命中 large path 的 value 必小于阈值（或已是编码后的 content），
        // 复制成本有界；命中的大 value 走 `large_entry` 不复制。
        write_li.push(entry.clone());
      }
    }
    if result.is_ok()
      && let Some(limit) = self.branch_segment_limit
    {
      let segment_max = self
        .table_map
        .get(tbl)
        .and_then(|state| state.segment_mapper.as_ref())
        .map_or(0, PageMapper::max_fp_id);
      if segment_max > limit {
        result = Err(KvError::OutOfSpace);
      }
    }
    if result.is_ok() {
      // 段 journal 尚未结算：TTL 清理交回本函数在结算后执行。
      result = self.put_at_with_ttl(tbl, &write_li, now_ms, false).await;
    }
    self.settle_mapper_batch(tbl, batch, result.is_ok());
    // 段批次已结算，此时清理自带 mapper journal 不会与外层交织回滚。
    if result.is_ok() {
      self.dispatch_ttl_clean(tbl, now_ms).await;
    }
    result
  }
}

impl<I: PageIo + ManifestIo + FixedSegmentIo> Store<I> {
  /// 与 `put_large` 相同，但大 value 通过调用方注册的 fixed buffer 写出。
  pub async fn put_large_fixed(
    &mut self,
    tbl: &TableIdent,
    batch_li: &[BatchEntry],
    buf: &mut FixedBuffer,
  ) -> Result<()> {
    let mut sink = FixedSink { buf, value_len: 0 };
    self.put_large_impl(tbl, batch_li, &mut sink).await
  }
}

/// 大 value 读取目标：owned 分配 Vec 返回，buffer 直接读进调用方 FixedBuffer。
trait ValueDst<I: SegmentIo> {
  type Out;

  /// 非 large value：直接交付 data page 里的原始 value（owned 路径零拷贝搬运）。
  fn inline(&mut self, value: Vec<u8>) -> Result<Self::Out>;

  /// large value：解析编码并按 segment 映射还原。
  async fn large(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    mapper: &PageMapper,
    layout: SegmentLayout,
    content: &[u8],
  ) -> Result<Self::Out>;
}

struct OwnedDst;

impl<I: SegmentIo> ValueDst<I> for OwnedDst {
  type Out = LargeValue;

  fn inline(&mut self, value: Vec<u8>) -> Result<LargeValue> {
    Ok(LargeValue {
      value,
      metadata: Vec::new(),
    })
  }

  async fn large(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    mapper: &PageMapper,
    layout: SegmentLayout,
    content: &[u8],
  ) -> Result<LargeValue> {
    let read = read_large_value(io, tbl, mapper, layout, content).await?;
    Ok(LargeValue {
      value: read.value,
      metadata: read.metadata,
    })
  }
}

struct BufferDst<'a>(&'a mut FixedBuffer);

impl<I: FixedSegmentIo> ValueDst<I> for BufferDst<'_> {
  type Out = LargeValueInto;

  fn inline(&mut self, value: Vec<u8>) -> Result<LargeValueInto> {
    let dst = self.0.as_mut_slice().ok_or(KvError::Busy)?;
    if value.len() > dst.len() {
      return Err(KvError::InvalidArgs);
    }
    dst[..value.len()].copy_from_slice(&value);
    Ok(LargeValueInto {
      value_len: value.len(),
      metadata: Vec::new(),
    })
  }

  async fn large(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    mapper: &PageMapper,
    layout: SegmentLayout,
    content: &[u8],
  ) -> Result<LargeValueInto> {
    let read: ManagedReadResult =
      read_value_fixed(io, tbl, mapper, layout, content, self.0).await?;
    Ok(LargeValueInto {
      value_len: read.header.actual_len as usize,
      metadata: read.metadata,
    })
  }
}

impl<I: PageIo + SegmentIo> Store<I> {
  /// 点读 entry（TTL 下推）；表未初始化返回 `None`。
  ///
  /// 快照由调用方取一次并在 entry / large value 解析期间复用。
  async fn read_entry_at(
    &self,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
    key: &[u8],
    now_ms: u64,
  ) -> Result<Option<ReadEntry>> {
    read_entry(
      &self.io,
      tbl,
      key,
      self.page_size,
      self.read_ctx_at(tbl, snapshot, now_ms),
    )
    .await
  }

  /// 当前树中该 key 的版本时间戳（忽略 TTL 过滤，`now=0`）：大 value 写入前
  /// 用它判断本批是否会被 merge 丢弃，避免为落败版本分配 segment 造成泄漏。
  async fn base_timestamp(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<u64>> {
    let Some(snapshot) = self.snapshot(tbl) else {
      return Ok(None);
    };
    Ok(
      self
        .read_entry_at(tbl, &snapshot, key, 0)
        .await?
        .map(|entry| entry.timestamp),
    )
  }

  /// 统一 entry → value 还原：非 large 直接交付，large 按快照的 segment 映射读回。
  async fn resolve_value<D: ValueDst<I>>(
    &self,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
    value: Vec<u8>,
    is_large_value: bool,
    dst: &mut D,
  ) -> Result<D::Out> {
    if !is_large_value {
      return dst.inline(value);
    }
    let Some(layout) = self.large_value_layout else {
      return Err(KvError::InvalidArgs);
    };
    let Some(segment_mapper) = snapshot.segment_mapper() else {
      return Err(KvError::ResourceMissing);
    };
    dst
      .large(&self.io, tbl, segment_mapper, layout, &value)
      .await
  }

  /// 最大且 <= key 的 entry；不存在返回 `NotFound`。
  pub async fn floor(&self, tbl: &TableIdent, key: &[u8]) -> Result<FloorValue> {
    let Some(snapshot) = self.snapshot(tbl) else {
      return Err(KvError::NotFound);
    };
    let Some(entry) = read_floor_entry(
      &self.io,
      tbl,
      key,
      self.page_size,
      self.read_ctx_at(tbl, &snapshot, now_ms()),
    )
    .await?
    else {
      return Err(KvError::NotFound);
    };
    let ReadFloorEntry {
      key,
      value,
      timestamp,
      expire_ts,
      is_large_value,
    } = entry;
    let read = self
      .resolve_value(tbl, &snapshot, value, is_large_value, &mut OwnedDst)
      .await?;
    Ok(FloorValue {
      key,
      value: read.value,
      metadata: read.metadata,
      timestamp,
      expire_ts,
    })
  }

  pub async fn get_large(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<LargeValue>> {
    self.get_large_at(tbl, key, now_ms()).await
  }

  pub async fn get_large_at(
    &self,
    tbl: &TableIdent,
    key: &[u8],
    now_ms: u64,
  ) -> Result<Option<LargeValue>> {
    let Some(snapshot) = self.snapshot(tbl) else {
      return Ok(None);
    };
    let Some(ReadEntry {
      value,
      is_large_value,
      ..
    }) = self.read_entry_at(tbl, &snapshot, key, now_ms).await?
    else {
      return Ok(None);
    };
    Ok(Some(
      self
        .resolve_value(tbl, &snapshot, value, is_large_value, &mut OwnedDst)
        .await?,
    ))
  }
}

impl<I: PageIo + FixedSegmentIo> Store<I> {
  pub async fn get_large_into(
    &self,
    tbl: &TableIdent,
    key: &[u8],
    buf: &mut FixedBuffer,
  ) -> Result<Option<LargeValueInto>> {
    self.get_large_into_at(tbl, key, now_ms(), buf).await
  }

  /// 大 value 直接读入 fixed buffer；返回有效长度与 metadata。
  pub async fn get_large_into_at(
    &self,
    tbl: &TableIdent,
    key: &[u8],
    now_ms: u64,
    buf: &mut FixedBuffer,
  ) -> Result<Option<LargeValueInto>> {
    let Some(snapshot) = self.snapshot(tbl) else {
      return Ok(None);
    };
    let Some(ReadEntry {
      value,
      is_large_value,
      ..
    }) = self.read_entry_at(tbl, &snapshot, key, now_ms).await?
    else {
      return Ok(None);
    };
    Ok(Some(
      self
        .resolve_value(tbl, &snapshot, value, is_large_value, &mut BufferDst(buf))
        .await?,
    ))
  }
}

impl<I: PageIo + ManifestIo + FileIo + SegmentIo> Store<I> {
  /// 创建 branch manifest：继承当前 mapping，max_fp_id 跳到下一个文件避免与父 branch 冲突。
  ///
  /// 对齐 C++ `BackgroundWrite::CreateBranch`：
  /// - 先做一次 pre-compact，branch 继承稠密 mapping，不携带父的碎片文件；
  /// - 分区尚未打开（无 root）或 mapper 缺失时视为空分区，直接成功且不落 branch manifest。
  pub async fn create_branch(&mut self, tbl: &TableIdent, branch_name: &str) -> Result<BranchInfo> {
    let normalized = normalize_branch_name(branch_name).ok_or(KvError::InvalidArgs)?;
    let active = normalize_branch_name(&self.branch_name).ok_or(KvError::InvalidArgs)?;
    if normalized == active || normalized.as_ref() == MAIN_BRANCH_NAME {
      return Err(KvError::InvalidArgs);
    }
    let opened = self.table_map.get(tbl).is_some_and(|state| {
      state.manifest_started && state.root.is_some() && state.mapper.is_some()
    });
    if !opened {
      // 空分区 / 未打开表（含 Runtime `open_empty`）：没有可继承的 mapping，
      // 不创建 active / branch manifest（对齐 C++ NotFound 直接成功）。
      return Ok(BranchInfo {
        name: String::new(),
        bytes: 0,
      });
    }
    self.compact_before_branch(tbl).await?;
    let state = self.table_map.get(tbl).ok_or(KvError::ResourceMissing)?;
    let root_id = state.root.ok_or(KvError::ResourceMissing)?;
    let ttl_root_id = state.ttl_root;
    let parent_max_fp_id = state.mapper.as_ref().map_or(0, PageMapper::max_fp_id);
    let next_file_id = (parent_max_fp_id >> self.pages_per_file_shift)
      .checked_add(1)
      .ok_or(KvError::InvalidArgs)?;
    let branch_base_file = next_file_id
      .div_ceil(BRANCH_FILE_RESERVE_FILE_NUM)
      .checked_mul(BRANCH_FILE_RESERVE_FILE_NUM)
      .ok_or(KvError::InvalidArgs)?;
    let branch_limit_file = branch_base_file
      .checked_add(BRANCH_FILE_RESERVE_FILE_NUM)
      .ok_or(KvError::InvalidArgs)?;
    let new_max_fp_id = branch_base_file
      .checked_shl(u32::from(self.pages_per_file_shift))
      .ok_or(KvError::InvalidArgs)?;
    let branch_limit_fp_id = branch_limit_file
      .checked_shl(u32::from(self.pages_per_file_shift))
      .ok_or(KvError::InvalidArgs)?;
    let segment_shift = self
      .large_value_layout
      .map_or(DEFAULT_SEGMENTS_PER_FILE_SHIFT, |layout| {
        layout.segments_per_file_shift()
      });
    // 是否启用 segment 侧：显式配置了大 value，或该表已有 segment mapping。
    // 两者都不成立时（plain store）不创建 segment mapper，branch snapshot
    // 与 C++ `BackgroundWrite::CreateBranch` 一样完全省略可选 segment 段。
    let use_segment = state.segment_mapper.is_some() || self.large_value_layout.is_some();
    let parent_segment_max_fp = state
      .segment_mapper
      .as_ref()
      .map_or(0, PageMapper::max_fp_id);
    let next_segment_file_id = (parent_segment_max_fp >> segment_shift)
      .checked_add(1)
      .ok_or(KvError::InvalidArgs)?;
    let branch_segment_base_file = next_segment_file_id
      .div_ceil(BRANCH_FILE_RESERVE_FILE_NUM)
      .checked_mul(BRANCH_FILE_RESERVE_FILE_NUM)
      .ok_or(KvError::InvalidArgs)?;
    let branch_segment_limit_file = branch_segment_base_file
      .checked_add(BRANCH_FILE_RESERVE_FILE_NUM)
      .ok_or(KvError::InvalidArgs)?;
    let branch_segment_fp_id = branch_segment_base_file
      .checked_shl(u32::from(segment_shift))
      .ok_or(KvError::InvalidArgs)?;
    let branch_segment_limit_fp_id = branch_segment_limit_file
      .checked_shl(u32::from(segment_shift))
      .ok_or(KvError::InvalidArgs)?;
    // 子分支继承父分支的 file_range_li 链，再追加自己的区间。
    let mut metadata = state
      .branch_metadata
      .clone()
      .unwrap_or_else(|| BranchManifestMetadata::new(normalized.clone(), 0));
    metadata.branch_name = normalized.clone();
    metadata.term = 0;
    metadata.file_range_li.push(BranchFileRange {
      branch_name: normalized.clone(),
      term: 0,
      max_file_id: new_max_fp_id >> self.pages_per_file_shift,
      // 无 segment 侧时填 0：与 C++ branch metadata 一致，避免 GC / cloud
      // range lookup 把这个区间误当成已存在的 segment 文件区间。
      max_segment_file_id: if use_segment {
        branch_segment_base_file
      } else {
        0
      },
    });
    // 父 branch 先跳过整个 segment 预留区间，再写 branch snapshot：
    // 只有 use_segment 时才需要；plain store 不凭空创建 segment mapper。
    // 预留先落盘即使 branch manifest 写失败也只是浪费空间，不会重叠。
    if use_segment {
      self
        .table_map
        .entry(tbl.clone())
        .or_default()
        .segment_mapper
        .get_or_insert_with(|| PageMapper::new(STORE_ALLOC_MODE, 0))
        .reserve_until(branch_segment_limit_fp_id);
    }
    let record = self.snapshot_record(
      tbl,
      root_id,
      ttl_root_id,
      Some(new_max_fp_id),
      use_segment.then_some(branch_segment_fp_id),
      Some(&metadata),
    )?;
    let name = manifest_file_name(&normalized, 0);
    // 父 branch 跳过整个 data 预留区间，先把新 max 持久化；
    // 即使随后 branch manifest 写入失败，也只是浪费预留空间，不会重叠。
    if let Some(mapper) = self
      .table_map
      .get_mut(tbl)
      .and_then(|state| state.mapper.as_mut())
    {
      mapper.reserve_until(branch_limit_fp_id);
    }
    self
      .write_snapshot_manifest(tbl, root_id, ttl_root_id)
      .await?;
    self.io.write_manifest_file(tbl, &name, &record).await?;
    Ok(BranchInfo {
      name,
      bytes: record.len(),
    })
  }

  /// branch 创建前的全量 pre-compact（对齐 C++ `BackgroundWrite::Compact`）：
  /// data / segment 两侧各自按放大率把 live page 搬到文件尾，搬空的旧文件
  /// 随后由 GC 删除，branch 继承的 mapping 因此是稠密的。
  ///
  /// 正在追加的尾文件未写满，按容量算放大率会误判，跳过（同 `compact_if_needed`）。
  /// 两个放大率阈值都为 0 表示关闭自动 compact，直接跳过。
  async fn compact_before_branch(&mut self, tbl: &TableIdent) -> Result<()> {
    let data_factor = self.file_amplify_factor;
    let segment_factor = self.segment_file_amplify_factor;
    if data_factor == 0 && segment_factor == 0 {
      return Ok(());
    }
    if data_factor != 0
      && let Some(mapper) = self
        .table_map
        .get(tbl)
        .and_then(|state| state.mapper.as_ref())
    {
      let plan_li = {
        let live_map = fkv_gc::file_live_count(mapper.tbl(), self.pages_per_file_shift);
        let candidate_li = self.io.list_file_id_li(tbl).await?;
        let pages_per_file = 1u64 << self.pages_per_file_shift;
        // 尾文件按分配器高水位算：盘上最大 id 可能因空文件被 GC 删除而更小。
        fkv_gc::compact_plan(
          &live_map,
          &candidate_li,
          pages_per_file,
          data_factor,
          mapper.max_fp_id() >> self.pages_per_file_shift,
        )
      };
      if !plan_li.is_empty() {
        self
          .compact_file_li(tbl, &plan_li, self.pages_per_file_shift)
          .await?;
      }
    }
    if segment_factor != 0 {
      let plan_li = self.segment_compact_plan(tbl).await?;
      if !plan_li.is_empty() {
        self.compact_segment_file_li(tbl, &plan_li).await?;
      }
    }
    // C++ `Compact()` 末尾无条件 `RunFileGc`：即使没有文件达到 compact 阈值，
    // 也回收无引用的旧文件，保证 branch 继承的文件集合与 C++ 一致。
    self.file_gc(tbl, self.pages_per_file_shift).await?;
    self.refresh_space_stat(tbl);
    Ok(())
  }

  /// 删除整表：只删当前 identity 的活跃 manifest，随后清空内存状态。
  ///
  /// 对齐 C++ `BatchWriteTask::Drop` → `DropManifest`：manifest 名是
  /// `manifest_<active_branch>_<process_term>`，只删本 shard 当前身份这一份。
  /// data / segment 文件不在这里硬删——其它 branch / archive manifest 可能仍
  /// 引用它们，直接删会让那些快照变成悬空引用。无引用的文件由调用方发起的
  /// `FileGc` 按 retained set 回收（与 C++ Drop 末尾 `TryAddFileGc` 一致）。
  ///
  /// 无论 manifest 删除是否成功都先 `evict_table`：避免失败后留下
  /// `manifest_started=true` 的脏状态，下一次写把 log record 写成 manifest
  /// 第一条记录（恢复侧会按 snapshot 解析而损坏）。
  pub async fn drop_table(&mut self, tbl: &TableIdent) -> Result<()> {
    // branch attach 后活跃 manifest 是显式名字（`manifest_<branch>_0`），
    // 不走 main 的固定路径；main / 未指定 branch 时才走 `drop_manifest`。
    let drop_result = match self.manifest_file.clone() {
      Some(name) => self.io.remove_manifest_file(tbl, &name).await,
      None => self.io.drop_manifest(tbl).await,
    };
    self.evict_table(tbl);
    drop_result?;
    // 物理文件不在这里同步删除：C++ `Drop` 只 `DropManifest` 并触发后台
    // `FileGc`，由 GC 依据全部 manifest 的 retained set 决定哪些文件无引用。
    // 调用方需要立即回收时显式发起 `FileGc` / `maintain`。
    Ok(())
  }
}

impl<I: PageIo + ManifestIo + FileIo + SegmentIo> Store<I> {
  /// 写后维护的 O(1) 预判（对齐 C++ `WriteTask::MapperExceedsAmplification`）：
  /// 只在放大率可能超阈值时返回 true，避免每次写都做 mapping 全量扫描 + 目录列举。
  ///
  /// 触发条件（`file_amplify_factor > 0` 时）：
  /// - 活映射已清空但仍占 file page 空间（全死文件）→ 交给 `maintain` 的 `gc` 回收；
  /// - 已分配空间不少于一个文件（`space >= pages_per_file`，尾文件未写满不计）
  ///   且 `space > mapped * factor`。
  ///
  /// `mapped` 用 `MappingCount()`（活跃映射数），`space` 用 `SpaceSize()`
  /// （扣除整文件空隙），对齐 C++ `WriteTask::MapperExceedsAmplification`：
  /// 稀疏 mapping 里已释放的逻辑页不会把放大率稀释。
  pub fn needs_maintain(&self, tbl: &TableIdent) -> bool {
    let factor = u64::from(self.file_amplify_factor);
    if factor == 0 {
      return false;
    }
    let Some(mapper) = self
      .table_map
      .get(tbl)
      .and_then(|state| state.mapper.as_ref())
    else {
      return false;
    };
    let mapped = mapper.mapping_count() as u64;
    let space = mapper.space_size(self.pages_per_file_shift);
    if mapped == 0 {
      return space != 0;
    }
    space >= 1u64 << self.pages_per_file_shift && space > mapped.saturating_mul(factor)
  }

  /// 对齐 C++ `WriteTask::CompactIfNeeded`：按 `file_amplify_factor` 选出
  /// 全部超阈值文件并一次性搬完；返回被 compact 的文件列表。
  ///
  /// 活映射为空时不再搬页，而是把分配器 high-water 推到下一文件边界
  /// （对齐 C++ `DoCompactDataFile` 的 `fp_ids.empty()` 分支），
  /// 避免 `needs_maintain` 每轮都为全死空间触发一次全表扫描。
  pub async fn compact_if_needed(&mut self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    if !self.needs_maintain(tbl) {
      return Ok(Vec::new());
    }
    let factor = self.file_amplify_factor;
    let pages_per_file = 1u64 << self.pages_per_file_shift;
    let plan_li = {
      let Some(mapper) = self
        .table_map
        .get(tbl)
        .and_then(|state| state.mapper.as_ref())
      else {
        return Ok(Vec::new());
      };
      if mapper.mapping_count() == 0 {
        // 全死映射：跳到下一文件边界，剩余死文件由随后的 `gc` 回收。
        let mapper = self
          .table_map
          .get_mut(tbl)
          .and_then(|state| state.mapper.as_mut())
          .ok_or(KvError::ResourceMissing)?;
        mapper.skip_empty_space(self.pages_per_file_shift);
        return Ok(Vec::new());
      }
      let live_map = fkv_gc::file_live_count(mapper.tbl(), self.pages_per_file_shift);
      let candidate_li = self.io.list_file_id_li(tbl).await?;
      // 当前追加尾文件尚未写满，不能按容量算放大率，否则 compact 出的稀疏
      // 尾页会再次触发抖动。尾 id 按分配器高水位算，不依赖盘上最大 id。
      fkv_gc::compact_plan(
        &live_map,
        &candidate_li,
        pages_per_file,
        factor,
        mapper.max_fp_id() >> self.pages_per_file_shift,
      )
    };
    if plan_li.is_empty() {
      return Ok(Vec::new());
    }
    self
      .compact_file_li(tbl, &plan_li, self.pages_per_file_shift)
      .await?;
    Ok(plan_li)
  }

  /// segment 侧的写后 O(1) 预判，规则同 [`Self::needs_maintain`]。
  ///
  /// 空间与活跃映射都按 segment 槽位计（`segments_per_file_shift`），
  /// 未启用大 value 或阈值为 0 时恒为 false。
  pub fn needs_segment_maintain(&self, tbl: &TableIdent) -> bool {
    let factor = u64::from(self.segment_file_amplify_factor);
    if factor == 0 {
      return false;
    }
    let Some(layout) = self.large_value_layout else {
      return false;
    };
    let Some(mapper) = self
      .table_map
      .get(tbl)
      .and_then(|state| state.segment_mapper.as_ref())
    else {
      return false;
    };
    let shift = layout.segments_per_file_shift();
    let mapped = mapper.mapping_count() as u64;
    let space = mapper.space_size(shift);
    if mapped == 0 {
      return space != 0;
    }
    space >= 1u64 << shift && space > mapped.saturating_mul(factor)
  }

  /// segment 文件压缩候选：按 segment 槽位算放大率，跳过追加尾文件。
  async fn segment_compact_plan(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    if self.segment_file_amplify_factor == 0 {
      return Ok(Vec::new());
    }
    let Some(layout) = self.large_value_layout else {
      return Ok(Vec::new());
    };
    let Some(mapper) = self
      .table_map
      .get(tbl)
      .and_then(|state| state.segment_mapper.as_ref())
    else {
      return Ok(Vec::new());
    };
    let shift = layout.segments_per_file_shift();
    let live_map = fkv_gc::file_live_count(mapper.tbl(), shift);
    let candidate_li = self.io.list_segment_file_id_li(tbl).await?;
    // 尾文件按分配器高水位算：`max_fp_id` 是下一个待分配 id，
    // 它落在的文件才是当前追加目标；盘上最大 id 可能更小（空文件已被 GC 删）。
    Ok(fkv_gc::segment_compact_plan(
      &live_map,
      &candidate_li,
      1u64 << shift,
      self.segment_file_amplify_factor,
      mapper.max_fp_id() >> shift,
    ))
  }

  /// 对齐 C++ `DoCompactSegmentFile`：把超阈值 segment 文件里的 live 段搬到文件尾，
  /// 返回被 compact 的 segment file 列表。
  ///
  /// 全死映射没有可搬的段，直接交给 `maintain` 随后的 `gc` 回收旧文件。
  pub async fn compact_segment_if_needed(&mut self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    if !self.needs_segment_maintain(tbl) {
      return Ok(Vec::new());
    }
    let plan_li = self.segment_compact_plan(tbl).await?;
    if plan_li.is_empty() {
      return Ok(Vec::new());
    }
    self.compact_segment_file_li(tbl, &plan_li).await?;
    Ok(plan_li)
  }

  /// 一次 compaction 搬多个 segment 文件：一个 mapper 批次、落一次 manifest。
  ///
  /// 段内容不变，只换物理 `FilePageId`，manifest 只追加 segment mapping 增量；
  /// 搬空的旧文件由下一次 `gc` 回收，仍被其它 branch manifest 引用的文件会保留。
  async fn compact_segment_file_li(
    &mut self,
    tbl: &TableIdent,
    file_li: &[FileId],
  ) -> Result<usize> {
    let Some(layout) = self.large_value_layout else {
      return Ok(0);
    };
    let segment_size = layout.segment_size() as usize;
    let segment_shift = layout.segments_per_file_shift();
    let (moved, batch) = {
      let mapper = self
        .table_map
        .get_mut(tbl)
        .and_then(|state| state.segment_mapper.as_mut())
        .ok_or(KvError::ResourceMissing)?;
      let batch = MapperBatch {
        data: false,
        segment: mapper.begin_batch(),
      };
      let moved = match fkv_task_compact::compact_segment_file_li(
        &self.io,
        tbl,
        mapper,
        file_li,
        segment_size,
        segment_shift,
      )
      .await
      {
        // 提交推迟到 manifest 落盘成功之后（core-25）：失败时 abort 才能回滚搬迁。
        Ok(count) => count,
        Err(err) => {
          if batch.segment {
            mapper.abort_batch();
          }
          return Err(err);
        }
      };
      mapper.update_stat_by_scan(segment_shift);
      (moved, batch)
    };
    let state = self.table_map.get(tbl);
    let root_id = state.and_then(|state| state.root).unwrap_or(NULL_PAGE);
    let ttl_root_id = state.map_or(NULL_PAGE, |state| state.ttl_root);
    // 同 compact_file_li / put：段搬迁先 sync_data 再落 manifest（eng-89）。
    let log_result = async {
      self.io.sync_data(tbl).await?;
      self.append_mapping_log(tbl, root_id, ttl_root_id).await
    }
    .await;
    // 段内容未变，只换物理位置：不能当成内容变更把相关缓存成批逐出。
    if log_result.is_ok()
      && let Some(mapper) = self
        .table_map
        .get_mut(tbl)
        .and_then(|state| state.segment_mapper.as_mut())
    {
      mapper.clear_changed_page_li();
    }
    self.settle_mapper_batch(tbl, batch, log_result.is_ok());
    log_result?;
    Ok(moved)
  }

  /// 写后维护钩子：把放大文件 compact 一轮，再回收无引用文件。
  /// Runtime / shard 应在写批次之后调用（C++ 由 WriteTask 内部触发）。
  pub async fn maintain(&mut self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    let compacted = self.compact_if_needed(tbl).await?;
    self.compact_segment_if_needed(tbl).await?;
    self.file_gc(tbl, self.pages_per_file_shift).await?;
    self.refresh_space_stat(tbl);
    self.cleanup_empty_partition(tbl).await?;
    Ok(compacted)
  }
}

impl<I: PageIo + FileIo + ManifestIo> Store<I> {
  /// 表逻辑为空时收尾分区（对齐 C++ `TryCleanupLocalPartitionDir`）：
  /// 删除活跃 manifest、重置 manifest 状态，再移除已无任何文件的分区目录。
  ///
  /// 只处理「truncate 后 GC 已删光 data / segment」的表：root 归零、两套 mapper
  /// 无活跃映射、目录内 data / segment 文件列表为空，缺一不可。其它 branch /
  /// archive 的 manifest 不在此删除，`try_remove_empty_partition_dir` 遇到非空
  /// 目录会原样保留，因此绝不会误删仍被引用的分区。
  pub async fn cleanup_empty_partition(&mut self, tbl: &TableIdent) -> Result<bool> {
    if !self.table_is_logically_empty(tbl).await? {
      return Ok(false);
    }
    match self.manifest_file.clone() {
      Some(name) => self.io.remove_manifest_file(tbl, &name).await?,
      None => self.io.drop_manifest(tbl).await?,
    }
    if let Some(state) = self.table_map.get_mut(tbl) {
      // 下一次写入走完整 snapshot 路径，避免按已删除 manifest 的追加偏移续写。
      state.manifest_started = false;
      state.manifest_size = 0;
    }
    self.io.try_remove_empty_partition_dir(tbl).await
  }

  /// 逻辑空表判定：已打开、树根为空、两套映射无活跃项、目录无数据文件。
  async fn table_is_logically_empty(&self, tbl: &TableIdent) -> Result<bool> {
    let Some(state) = self.table_map.get(tbl) else {
      return Ok(false);
    };
    // `manifest_started == false` 表示已经收尾过或从未写过 manifest，无需重做。
    if !state.manifest_started || state.root != Some(NULL_PAGE) {
      return Ok(false);
    }
    if state
      .mapper
      .as_ref()
      .is_none_or(|mapper| mapper.mapping_count() != 0)
    {
      return Ok(false);
    }
    if state
      .segment_mapper
      .as_ref()
      .is_some_and(|mapper| mapper.mapping_count() != 0)
    {
      return Ok(false);
    }
    if !self.io.list_file_id_li(tbl).await?.is_empty()
      || !self.io.list_segment_file_id_li(tbl).await?.is_empty()
    {
      return Ok(false);
    }
    Ok(true)
  }

  /// GC 删掉整文件后同步分配器空间统计，供下一轮 `needs_maintain` /
  /// `needs_segment_maintain` 使用。全空映射直接跳界，避免下一轮再为
  /// 空空间触发扫描。
  fn refresh_space_stat(&mut self, tbl: &TableIdent) {
    let shift = self.pages_per_file_shift;
    if let Some(mapper) = self
      .table_map
      .get_mut(tbl)
      .and_then(|state| state.mapper.as_mut())
    {
      if mapper.mapping_count() == 0 {
        mapper.skip_empty_space(shift);
      } else {
        mapper.update_stat_by_scan(shift);
      }
    }
    let segment_shift = self.segment_shift();
    if let Some(mapper) = self
      .table_map
      .get_mut(tbl)
      .and_then(|state| state.segment_mapper.as_mut())
    {
      if mapper.mapping_count() == 0 {
        mapper.skip_empty_space(segment_shift);
      } else {
        mapper.update_stat_by_scan(segment_shift);
      }
    }
  }

  /// 文件 GC（对齐 C++ `FileGcRequest` → `WriteTask::RunFileGc`）：按 IO 后端的
  /// 语义删除无引用文件 —— 本地 / 内存后端删本地文件，云端后端删远端对象并清
  /// 对应本地 cache。data 与 segment 在同一轮处理。
  ///
  /// retained set 以所有 manifest（active + branch + archive）为准，再并上当前
  /// 内存 mapping（含未 flush 的 page）与**所有活跃 mapper 快照**，并按 per-branch
  /// 高水位保护「已写数据页、manifest 尚未提交」的文件。
  ///
  /// 快照并集保证：读请求跨 `await` 持旧映射时，旧快照独占的 file page 不会被
  /// 删除。无活跃快照时该分支只是一次空表遍历，不产生额外分配。快照释放后
  /// 旧页从**下一轮** GC 起可回收，GC 不会因一次 drop 当场重跑。
  pub async fn file_gc(&self, tbl: &TableIdent, pages_per_file_shift: u8) -> Result<usize> {
    self
      .gc_with_scope(tbl, pages_per_file_shift, GcScope::File)
      .await
  }

  /// 本地 GC（对齐 C++ `LocalGcRequest` → `WriteTask::RunLocalFileGc`）：引用集
  /// 判定同 [`Self::file_gc`]，但只列举 / 删除本地文件与本地 cache，**绝不删除
  /// 远端对象**（C++ `ExecuteLocalGC` 只走本地 directory + IouringMgr）。
  ///
  /// 云端后端不剪枝远端 manifest：旧 term manifest 的回收仍由 `FileGc` 的
  /// `prune_superseded_manifest_file_li` 负责，本地 GC 只处理 data / segment。
  pub async fn local_gc(&self, tbl: &TableIdent, pages_per_file_shift: u8) -> Result<usize> {
    self
      .gc_with_scope(tbl, pages_per_file_shift, GcScope::Local)
      .await
  }

  async fn gc_with_scope(
    &self,
    tbl: &TableIdent,
    pages_per_file_shift: u8,
    scope: GcScope,
  ) -> Result<usize> {
    let segment_shift = self.segment_shift();
    let (mut data_set, mut segment_set, mut guard) =
      fkv_task_gc::manifest_retained_file_page_set_budget(
        &self.io,
        tbl,
        pages_per_file_shift,
        segment_shift,
        self.yield_budget_us,
      )
      .await?;
    if let Some(mapper) = self
      .table_map
      .get(tbl)
      .and_then(|state| state.mapper.as_ref())
    {
      // 内存 mapping 的引用不带 (branch, term)：并入 retained 的同时登记 pin，
      // 变体级 GC 对同 id 全部变体整体保留（rev-39）。
      guard.add_pinned_data_mapping(mapper.tbl(), &mut data_set, pages_per_file_shift);
      guard.seed(
        &self.branch_name,
        self.term,
        mapper.max_fp_id(),
        pages_per_file_shift,
      );
    }
    if let Some(segment_mapper) = self
      .table_map
      .get(tbl)
      .and_then(|state| state.segment_mapper.as_ref())
    {
      guard.add_pinned_segment_mapping(segment_mapper.tbl(), &mut segment_set, segment_shift);
      guard.seed_segment(
        &self.branch_name,
        self.term,
        segment_mapper.max_fp_id(),
        segment_shift,
      );
    }
    guard.set_process_term(self.term);
    // 对齐 C++ `file_gc.cpp:1553-1560`：看到任一新 owner 的 manifest term
    // 高于本进程 term，整轮 GC 返回 ExpiredTerm，绝不删除它的对象。
    if guard.term_above(self.term) {
      return Err(KvError::ExpiredTerm);
    }
    // 旧快照独占的页不在当前 mapper / 最新 manifest 里，必须显式并集；
    // 快照读者可能按旧变体读取，同 id 全部变体一并 pin（rev-39）。
    let mut snapshot_data_set = HashSet::default();
    self
      .snapshots
      .data_retained_into(tbl, &mut snapshot_data_set);
    if !snapshot_data_set.is_empty() {
      guard.pin_data_page_set(&snapshot_data_set, pages_per_file_shift);
      data_set.extend(snapshot_data_set);
    }
    let mut snapshot_segment_set = HashSet::default();
    self
      .snapshots
      .segment_retained_into(tbl, &mut snapshot_segment_set);
    if !snapshot_segment_set.is_empty() {
      guard.pin_segment_page_set(&snapshot_segment_set, segment_shift);
      segment_set.extend(snapshot_segment_set);
    }
    // 有 manifest 损坏 / 不可读时引用集合不完整：本轮不删文件、不剪枝，
    // 只记录，等坏文件恢复或修复后再回收，绝不误删仍被引用的数据。
    if guard.is_damaged() {
      log::warn!(
        "GC 跳过删除：存在损坏/不可读 manifest {:?}",
        guard.damaged_li()
      );
      return Ok(0);
    }
    let mut removed = match scope {
      GcScope::File => {
        fkv_task_gc::gc_files_with_retained_budget(
          &self.io,
          tbl,
          &data_set,
          &guard,
          pages_per_file_shift,
          self.yield_budget_us,
        )
        .await?
      }
      GcScope::Local => {
        fkv_task_gc::gc_local_files_with_retained_budget(
          &self.io,
          tbl,
          &data_set,
          &guard,
          pages_per_file_shift,
          self.yield_budget_us,
        )
        .await?
      }
    };
    if let Some(layout) = self.large_value_layout {
      let segment_shift = layout.segments_per_file_shift();
      removed += match scope {
        GcScope::File => {
          fkv_task_gc::gc_segment_files_with_retained_budget(
            &self.io,
            tbl,
            &segment_set,
            &guard,
            segment_shift,
            self.yield_budget_us,
          )
          .await?
        }
        GcScope::Local => {
          fkv_task_gc::gc_local_segment_files_with_retained_budget(
            &self.io,
            tbl,
            &segment_set,
            &guard,
            segment_shift,
            self.yield_budget_us,
          )
          .await?
        }
      };
    }
    // 剪枝放在数据文件删除之后：本轮的 retained set 仍包含旧 manifest 的引用，
    // 只被旧版本引用的文件要等下一轮 GC 才回收，避免「剪 manifest → 还有读者
    // 按旧版本判定」的窗口。云端后端只在远端最新 term 就是本实例 term 时
    // 才删更旧的无 tag manifest（见 `CloudStore::prune_superseded_manifest_file_li`）。
    // `LocalGc` 跳过：该实现会删远端对象，而本地 GC 只处理本地文件
    // （C++ `ExecuteLocalGC` 也不剪 manifest）。
    if scope == GcScope::File {
      self
        .io
        .prune_superseded_manifest_file_li(tbl, &self.branch_name, self.term)
        .await?;
    }
    // 收尾对齐 C++ `TryCleanupLocalPartitionDir`：目录内已无任何条目才移除；
    // 还有 manifest / 临时文件 / 未知残留时原样保留（绝不递归删除）。
    self.io.try_remove_empty_partition_dir(tbl).await?;
    Ok(removed)
  }
}

impl<I: PageIo + ManifestIo + TermIo> Store<I> {
  pub async fn claim_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<()> {
    self.io.upsert_term(branch, shard_id, term).await?;
    Ok(())
  }

  pub async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>> {
    self.io.read_term(branch, shard_id).await
  }

  pub async fn put_with_term(
    &mut self,
    tbl: &TableIdent,
    batch_li: &[BatchEntry],
    branch: &str,
    shard_id: u32,
    term: u64,
  ) -> Result<()> {
    if self
      .io
      .read_term(branch, shard_id)
      .await?
      .is_some_and(|current| current > term)
    {
      return Err(KvError::ExpiredTerm);
    }
    self.put(tbl, batch_li).await
  }
}
