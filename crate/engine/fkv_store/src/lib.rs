//! 单线程 Store：可接 `MemStore` / `CompioStore`，打通 put / get / scan / delete，
//! 并在写路径后追加 manifest snapshot / log。

use std::{cell::RefCell, collections::VecDeque};

use fkv_compress::DictCompression;
use fkv_large::SegmentLayout;
use fkv_manifest::BranchManifestMetadata;
pub use fkv_mapping::AllocMode;
use fkv_mapping::{CHUNK_SIZE, GenerationSource, PageMapper, SnapshotRegistry};
use fkv_task_scan::ScannedEntry;
pub use fkv_task_scan::{PageOption, ScanOption};
use fkv_types::{BranchFileRange, FilePageId, NULL_PAGE, PageId, TableIdent};
use gxhash::HashMap;
use page_cache::PageCache;

mod archive;
mod batch;
mod compact;
mod config;
mod gc;
mod manifest;
mod page_cache;
mod read;
mod reuse_floor;
mod state;
mod term;
mod truncate;
mod ttl;
mod write;

pub use page_cache::PageCacheStat;

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

/// `truncate_from` 单轮扫描 / 删除的 key 上界。
///
/// 整表物化后一次性删除会让大表的内存随表大小线性增长；按批扫描、删除、
/// 再从中断位置续扫，驻留内存只与本常量成正比（R2-1）。
pub const TRUNCATE_FROM_BATCH: usize = 4096;

/// 完整大 value 的读取结果（`get_large*`）。
///
/// 对应 C++ `ReadRequest` 的目标缓冲区形态：value 与 metadata 一并返回，
/// `ts_` / `expire_ts_` 与 inline read 一样来自 entry 头。C++ 的
/// `large_value_only_` 只是「不把 metadata 写进 `value_`」的省拷贝开关；
/// 这里始终返回 metadata 字段（trailer 上界 64 KiB），调用方按需忽略即可。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeValue {
  pub value: Vec<u8>,
  pub metadata: Vec<u8>,
  pub timestamp: u64,
  pub expire_ts: u64,
}

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

/// 扫描结果 entry：对齐 C++ `KvEntry` 的 `key_` / `value_` /
/// `timestamp_` / `expire_ts_`；`expire_ts == 0` 表示不过期。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KvEntry {
  pub key: Vec<u8>,
  pub value: Vec<u8>,
  pub timestamp: u64,
  pub expire_ts: u64,
}

impl From<ScannedEntry> for KvEntry {
  fn from(entry: ScannedEntry) -> Self {
    Self {
      key: entry.key,
      value: entry.value,
      timestamp: entry.timestamp,
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
  pub timestamp: u64,
  pub expire_ts: u64,
}

/// 点读 entry：对齐 C++ `ReadRequest` 的公开输出 `value_` / `ts_` /
/// `expire_ts_`（`eloqstore/include/eloq_store.h:228-230`）。
///
/// - `is_large_value == false`：`value` 是完整用户 value，`metadata` 为空；
/// - `is_large_value == true`：**不读取 segment 文件**，对齐 C++ 默认
///   `std::monostate` 的 metadata-only 形态；`value` 为空，`metadata` 是
///   metadata trailer（无 metadata 时为空）。需要完整 value 时用
///   `Store::get_large` / `get_large_into`。
///
/// `Store::get` / `Store::read` 对 large value 仍返回
/// `KvError::LargeValueUnsupported`（issue 15 的有意分层，避免把 segment
/// 指针编码当用户数据透传）；本入口是迁移 C++ `ReadRequest` 默认读法的
/// 显式替代。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GetEntry {
  pub value: Vec<u8>,
  pub metadata: Vec<u8>,
  pub timestamp: u64,
  pub expire_ts: u64,
  pub is_large_value: bool,
}

/// 重新借用 `Option<&mut PageMapper>`，供同一作用域内多次 `write_batch` 使用。
fn reborrow_segment<'a>(segment: &'a mut Option<&mut PageMapper>) -> Option<&'a mut PageMapper> {
  segment.as_deref_mut()
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
  /// 读写共用页缓存（`FilePageId -> 整页字节`，见 [`page_cache`]）：写路径回填、
  /// 读路径命中，两侧共用同一实例与同一份预算。`None` 表示预算为 `0`
  /// （显式关闭）或该表尚未打开。
  page_cache: Option<RefCell<PageCache>>,
  /// 写后延迟 TTL 清理时标记待清理。
  ttl_pending: bool,
  /// 最近一次记账的近似常驻字节（[`table_state_bytes`] 结果）。
  ///
  /// 只在触摸 / 写批次收尾 / 驱逐时刷新；读路径填充页缓存带来的增长
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
      page_cache: None,
      ttl_pending: false,
      bytes: 0,
    }
  }
}

/// 单表 root 状态的近似常驻字节：结构体本身、data / segment 映射 chunk、
/// 页缓存、压缩字典与 branch 元数据。
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
    // 页缓存计入表级常驻字节：`root_meta_cache_size` 超限驱逐时一并归还，
    // 否则页缓存会成为预算外的隐形驻留。
    + state
      .page_cache
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
  /// 表级 root 状态字节预算；0 表示不限制。
  ///
  /// 对齐 C++ `KvOption::root_meta_cache_size` 经 `RootMetaMgr` 实施的字节上限：
  /// data / segment mapping、页缓存、字典与 branch 元数据合计超限时，
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
  /// 每表页缓存（读写共用）字节预算；`0` 表示**关闭**页缓存。
  ///
  /// 对齐 C++ `PageManager` 的单一 buffer pool：index 页与 data 页共用一个
  /// 实例与一份预算。index 页始终经过本缓存；data 页是否入池由
  /// [`Self::enable_data_page_cache`] 决定（默认关闭，对齐 C++
  /// `KvOption::enable_data_page_cache`）。预算为 `0` 时读写路径都直连 IO。
  page_cache_bytes: usize,
  /// data 页（含 overflow 页）是否进页缓存；对齐 C++
  /// `KvOption::enable_data_page_cache`，默认 `false`。
  ///
  /// index 页始终走页缓存（C++ `FindPage`），不受本开关影响；开关关闭时
  /// data 页的读 / 写都退化为 `LoadDataPageStorage` 的直读直写语义：
  /// 不查缓存计账、不回填，因此 `page_cache_stat` 的 hit / miss / len
  /// 不随 data 页访问变化。开关只影响「字节从哪里来」，不改变写出的字节、
  /// 写顺序与 fsync 顺序。
  enable_data_page_cache: bool,
  /// 活跃 mapper 快照登记表：GC retained set 必须并集所有仍被读请求持有的
  /// 旧映射，否则旧快照独占的 file page 会被提前回收（27a）。
  snapshots: SnapshotRegistry,
  /// 本 Store 的映射代际来源：所有表 mapper 与重建化身共享，跨 Store 不共享。
  ///
  /// 代际只需在单 Store 内唯一（快照 / page cache 均为 per-Store），这里按
  /// Store 归属后各 shard 分区互不争用（perf-06）。
  generation_source: GenerationSource,
  /// 长循环（scan 页链 / overflow 链 / GC 文件遍历 / manifest 脏页）的
  /// 协作让出段预算（微秒；`0` = 关闭）。由 shard 从 `ShardConfig` 注入。
  yield_budget_us: u64,
  /// 物理页分配策略：`Append`（默认）/ `Pooled`（对齐 C++ `data_append_mode=false`）。
  alloc_mode: AllocMode,
}
