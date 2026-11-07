//! B+tree 基础算法：index 下降与 data page 合并 / 分裂。
//!
//! 完整的 COW 写路径在 `fkv_task_write` 里编排；这里保持无 IO、可单测。

pub mod comparator;
mod merge;

use core::{cell::Cell, mem::take};
use std::{
  borrow::Cow,
  collections::VecDeque,
  mem::size_of,
  sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering as AtomicOrdering},
  },
};

use fkv_compress::{DictCompression, decompress_raw};
use fkv_error::{KvError, Result};
use fkv_page::{CompressionType, IndexPage, IndexPageIter};
use fkv_types::{FilePageId, PageId, WriteOp, slow_read};
use gxhash::{HashMap, HashSet};
pub use merge::{
  MergeOption, MergeOutput, OverflowSlot, TtlUpsertSink, merge_data_pages_with_options,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchEntry {
  pub key: Vec<u8>,
  pub value: Vec<u8>,
  pub timestamp: u64,
  pub expire_ts: u64,
  pub op: WriteOp,
  /// value 是 `fkv_large` 编码后的 segment id + metadata trailer。
  pub large_value: bool,
  /// value 是 overflow 页的首批指针数组（每个 PageId 小端 4 字节）。
  pub overflow: bool,
  /// 仅写入大 value 时使用；编码后不再保留。
  pub metadata: Vec<u8>,
}

impl BatchEntry {
  pub fn upsert(key: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>, timestamp: u64) -> Self {
    Self {
      key: key.into(),
      value: value.into(),
      timestamp,
      expire_ts: 0,
      op: WriteOp::Upsert,
      large_value: false,
      overflow: false,
      metadata: Vec::new(),
    }
  }

  pub fn delete(key: impl Into<Vec<u8>>, timestamp: u64) -> Self {
    Self {
      key: key.into(),
      value: Vec::new(),
      timestamp,
      expire_ts: 0,
      op: WriteOp::Delete,
      large_value: false,
      overflow: false,
      metadata: Vec::new(),
    }
  }

  /// 大 value 写入时携带 metadata trailer。
  pub fn with_metadata(mut self, metadata: impl Into<Vec<u8>>) -> Self {
    self.metadata = metadata.into();
    self
  }
}

/// 缓存中的 index page：常驻原始页字节（对齐 C++ `MemCachedPage`），
/// seek 时 restart 二分 + 区间惰性解码，不物化每条 entry。
///
/// 额外记录页所属物理页 `file_page_id`：reopen / attach 后只有该物理页仍被
/// 逻辑页引用时才能复用（eng-67）。append-only 分配下物理页内容写后不变，
/// 物理页相同即页字节相同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexNode {
  leftmost: PageId,
  file_page_id: FilePageId,
  page: Vec<u8>,
}

impl IndexNode {
  /// 插入缓存时构造：校验页头/类型与 restart 升序（O(restart_num)，零 entry 分配），
  /// entry 结构留到 seek 触达区间时解码。`page` 是复用 IO buffer，必须拷贝持有。
  pub fn new(file_page_id: FilePageId, page: &[u8]) -> Result<Self> {
    let leftmost = IndexPage::new(page)?.leftmost_ptr()?;
    drop(IndexPageIter::new(page)?);
    Ok(Self {
      leftmost,
      file_page_id,
      page: page.to_vec(),
    })
  }

  /// 最大 key <= search 的子页；小于首 key 时返回 leftmost。
  ///
  /// 页已由 [`Self::new`] 校验，命中路径跳过重复的 restart 升序校验。
  pub fn seek(&self, key: &[u8]) -> Result<PageId> {
    let mut iter = IndexPageIter::new_prevalidated(&self.page)?;
    Ok(if iter.seek_floor(key)? {
      iter.page_id()
    } else {
      self.leftmost
    })
  }

  /// 近似内存占用：结构体 + 常驻页字节（按分配粒度取整） + 堆块头分摊。
  pub fn byte_len(&self) -> usize {
    // 页字节按分配粒度取整，并计入堆块头 / 散列表桶分摊；否则预算系统性低估。
    size_of::<Self>() + alloc_len(self.page.capacity()) + ENTRY_ALLOC_OVERHEAD
  }
}

/// 单个 table 的 index 缓存默认上限（近似字节）。
///
/// 数值唯一来源是 `fkv_config::DEFAULT_INDEX_CACHE_BYTES`（u64，INI 解析用）；
/// 这里只做 u64 → usize 投影，不再另设常量。
pub const DEFAULT_INDEX_CACHE_BYTES: usize = fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize;

/// 堆分配对齐粒度：页字节按该粒度向上取整估算真实占用。
const ALLOC_GRANULE: usize = 16;

/// 单条缓存项的额外开销估值：`Vec` 堆块头 + `HashMap` 桶分摊。
const ENTRY_ALLOC_OVERHEAD: usize = 64;

/// 向上取整到 [`ALLOC_GRANULE`]。
const fn alloc_len(len: usize) -> usize {
  (len + ALLOC_GRANULE - 1) & !(ALLOC_GRANULE - 1)
}

/// 跨 [`IndexCache`] 共享的字节账本：一个 Store / shard 内所有表共用一份，
/// 实现 index 缓存的聚合上限（对齐 C++ 单池记账）。
///
/// shard 线程独占访问，`relaxed` 原子只为让 `Arc` 满足 `Send + Sync`。
/// `slot` 是挂在该账本上的缓存实例数：聚合上限按实例均分，避免先写满的
/// 表独占整池把后开的表饿死。
#[derive(Debug, Default)]
pub struct CacheLedger {
  used: AtomicUsize,
  slot: AtomicUsize,
}

impl CacheLedger {
  /// 当前各缓存合计占用（近似字节）。
  pub fn used(&self) -> usize {
    self.used.load(AtomicOrdering::Relaxed)
  }

  /// 当前挂载的缓存实例数。
  pub fn slot(&self) -> usize {
    self.slot.load(AtomicOrdering::Relaxed)
  }

  fn add(&self, bytes: usize) {
    self.used.fetch_add(bytes, AtomicOrdering::Relaxed);
  }

  fn sub(&self, bytes: usize) {
    self.used.fetch_sub(bytes, AtomicOrdering::Relaxed);
  }

  fn attach(&self) {
    self.slot.fetch_add(1, AtomicOrdering::Relaxed);
  }

  fn detach(&self) {
    self.slot.fetch_sub(1, AtomicOrdering::Relaxed);
  }
}

/// 每个缓存项的访问标记（Clock 算法，`get` 走 `&self` 也能置位）与所属
/// 映射代际：只有代际与读者快照一致时才可命中（27b）。
#[derive(Debug)]
struct CacheEntry {
  node: IndexNode,
  referenced: Cell<bool>,
  /// 非 baseline 代插入时的显式代际；`None` 表示跟随缓存 baseline。
  gen_override: Option<u64>,
}

/// index 缓存计数：单线程累加，可跨多个 [`IndexCache`] 实例共享。
///
/// shard 线程独占访问，用 relaxed 原子是为了让 `Arc` 满足 `Send + Sync`；
/// 共享计数器让 store 在 reopen / 表状态驱逐替换缓存实例后，累计命中数仍然单调。
#[derive(Debug, Default)]
pub struct CacheCounter {
  hit: AtomicU64,
  miss: AtomicU64,
  evict: AtomicU64,
}

impl CacheCounter {
  pub fn hit(&self) -> u64 {
    self.hit.load(AtomicOrdering::Relaxed)
  }

  pub fn miss(&self) -> u64 {
    self.miss.load(AtomicOrdering::Relaxed)
  }

  pub fn evict(&self) -> u64 {
    self.evict.load(AtomicOrdering::Relaxed)
  }

  fn inc_hit(&self) {
    self.hit.fetch_add(1, AtomicOrdering::Relaxed);
  }

  fn inc_miss(&self) {
    self.miss.fetch_add(1, AtomicOrdering::Relaxed);
  }

  fn inc_evict(&self) {
    self.evict.fetch_add(1, AtomicOrdering::Relaxed);
  }
}

/// [`IndexCache`] 的统计快照：计数器累计值 + 当前占用。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStat {
  pub hit: u64,
  pub miss: u64,
  pub evict: u64,
  /// 当前常驻近似字节数。
  pub bytes: usize,
  /// 当前缓存页数。
  pub len: usize,
  /// 字节上限；`0` 表示不限制。
  pub capacity: usize,
}

/// 每 table 的 index page 缓存：`PageId -> IndexNode`，命中则跳过 index IO。
///
/// 按近似字节数做 Clock（二次机会）淘汰：命中过的页在淘汰时获得一次保留机会，
/// 避免 FIFO 把根页 / 上层内部索引页优先踢出。
/// `capacity == 0` 表示本表不限制；挂了 [`CacheLedger`] 时仍受聚合上限约束。
#[derive(Debug)]
pub struct IndexCache {
  node_map: HashMap<PageId, CacheEntry>,
  order_li: VecDeque<PageId>,
  /// 缓存级 baseline 代际：绝大多数条目直接跟随它。
  base_gen: u64,
  /// 自上次 `retag` 以来以非 baseline 代插入的例外条目。
  ///
  /// `retag` 只逐条清这些例外，不再遍历整张缓存（R3）。同一页在两次 `retag`
  /// 之间至多登记一次（`insert` 只在 `gen_override` 由 `None` 变为 `Some` 时
  /// 追加）；逐出 / 覆盖残留的陈旧项超过阈值时按 `node_map` 压实一次，压实后
  /// 长度不超过条目数，与缓存规模同阶、摊销 O(1)。
  exception_li: Vec<PageId>,
  bytes: usize,
  capacity: usize,
  counter: Arc<CacheCounter>,
  /// shard 级共享字节账本（可选）。
  ledger: Option<Arc<CacheLedger>>,
  /// 聚合上限；`0` 表示不限制聚合。实际生效上限是它与 `capacity` 的较小者。
  total_capacity: usize,
}

impl Default for IndexCache {
  fn default() -> Self {
    Self::with_capacity_bytes(DEFAULT_INDEX_CACHE_BYTES)
  }
}

impl IndexCache {
  pub fn with_capacity_bytes(capacity: usize) -> Self {
    Self::with_counter(capacity, Arc::new(CacheCounter::default()))
  }

  /// 复用外部计数器创建缓存（store 级共享，替换缓存实例不清零）。
  pub fn with_counter(capacity: usize, counter: Arc<CacheCounter>) -> Self {
    Self {
      node_map: HashMap::default(),
      order_li: VecDeque::new(),
      base_gen: 0,
      exception_li: Vec::new(),
      bytes: 0,
      capacity,
      counter,
      ledger: None,
      total_capacity: 0,
    }
  }

  /// 绑定 shard 级共享字节账本：聚合上限按挂载实例数均分。
  ///
  /// 均分而非先到先得，避免最早写满的表独占整池、后开的表完全拿不到缓存。
  /// `total_capacity == 0` 表示不做聚合约束。
  pub fn with_shared_budget(mut self, ledger: Arc<CacheLedger>, total_capacity: usize) -> Self {
    ledger.attach();
    self.ledger = Some(ledger);
    self.total_capacity = total_capacity;
    self
  }

  /// 命中要求条目代际与读者快照的映射代际一致；代际不符视为 miss，
  /// 旧快照不会拿到新代节点。代际不符的条目留给后续 `insert` 覆盖，
  /// 不在此处驱逐（`&self` 且避免旧快照反向冲掉新代条目）。
  pub fn get(&self, page_id: PageId, generation: u64) -> Option<&IndexNode> {
    let entry = self.node_map.get(&page_id)?;
    if entry.gen_override.unwrap_or(self.base_gen) != generation {
      return None;
    }
    entry.referenced.set(true);
    self.counter.inc_hit();
    Some(&entry.node)
  }

  pub fn insert(&mut self, page_id: PageId, node: IndexNode, generation: u64) {
    // 旧快照（代际早于当前 baseline）在 retag 之后回填的页来自旧映射，
    // 不能进入缓存：否则下一次 retag 会把它当成本代条目提升给新读者。
    //
    // 这条守卫成立的唯一前提是 `base_gen` 单调不减、且代际值永不复用
    // （见 `fkv_mapping::GenerationSource`）：早于 baseline 的代际必然
    // 属于已提交的旧映射、已回滚的批次或上一个 mapper 化身。
    if generation < self.base_gen {
      return;
    }
    let node_bytes = node.byte_len();
    let base_gen = self.base_gen;
    let gen_override = (generation != base_gen).then_some(generation);
    let old_override = self.node_map.get(&page_id).map(|entry| entry.gen_override);
    let old_gen = old_override.map(|old| old.unwrap_or(base_gen));
    if gen_override.is_some() {
      // 去重：条目已带显式代际说明本页自上次 `retag` 起已登记过，不再追加。
      if !matches!(old_override, Some(Some(_))) {
        // 上界：`remove` / 逐出不清清单，同页反复「删除 → 以旧代回填」会留下
        // 陈旧项；超阈值时按 `node_map` 压实去重（同 `order_li` 的摊销策略），
        // 压实后只剩仍带显式代际的存活条目，长度不超过缓存条目数。
        if self.exception_li.len() > self.node_map.len().saturating_mul(2).saturating_add(64) {
          let compact_start = slow_read::start();
          let node_map = &self.node_map;
          let mut seen = HashSet::default();
          self.exception_li.retain(|page_id| {
            node_map
              .get(page_id)
              .is_some_and(|entry| entry.gen_override.is_some())
              && seen.insert(*page_id)
          });
          slow_read::stop("cache_compact", compact_start);
        }
        self.exception_li.push(page_id);
      }
    }
    if let Some(old) = self.node_map.get_mut(&page_id) {
      // 代际不符说明 `get` 已按 miss 处理，这里是一次真实回填；同代重插
      // 视为覆盖，不重复计 miss（读路径同代不会走到这里）。
      if old_gen != Some(generation) {
        self.counter.inc_miss();
      }
      let old_bytes = old.node.byte_len();
      old.node = node;
      old.referenced.set(true);
      old.gen_override = gen_override;
      self.account_sub(old_bytes);
      self.account_add(node_bytes);
      self.evict_to_fit();
      return;
    }
    // miss 按回填计数：读路径先 `get` 探测，data 页不写缓存，若在 `get`
    // 里计 miss 会让每次点读都多记一次未命中，命中率失去意义。
    self.counter.inc_miss();
    // 陈旧队列项（remove 后残留、同 id 重插）会累积；超阈值时压实去重，
    // 保证 `order_li` 与条目数同阶，摊销 O(1)。
    if self.order_li.len() > self.node_map.len().saturating_mul(2).saturating_add(64) {
      let compact_start = slow_read::start();
      let node_map = &self.node_map;
      let mut seen = HashSet::default();
      self
        .order_li
        .retain(|page_id| node_map.contains_key(page_id) && seen.insert(*page_id));
      slow_read::stop("cache_compact", compact_start);
    }
    self.order_li.push_back(page_id);
    self.node_map.insert(
      page_id,
      CacheEntry {
        node,
        referenced: Cell::new(true),
        gen_override,
      },
    );
    self.account_add(node_bytes);
    self.evict_to_fit();
  }

  /// 实际生效字节上限：本表额度与聚合均分额度的较小者；`None` 表示无约束。
  fn limit(&self) -> Option<usize> {
    let table = (self.capacity != 0).then_some(self.capacity);
    let total = self.ledger.as_ref().and_then(|ledger| {
      (self.total_capacity != 0).then(|| self.total_capacity / ledger.slot().max(1))
    });
    match (table, total) {
      (Some(a), Some(b)) => Some(a.min(b)),
      (Some(limit), None) | (None, Some(limit)) => Some(limit),
      (None, None) => None,
    }
  }

  fn over_limit(&self) -> bool {
    self.limit().is_some_and(|limit| self.bytes > limit)
  }

  fn account_add(&mut self, bytes: usize) {
    self.bytes += bytes;
    if let Some(ledger) = &self.ledger {
      ledger.add(bytes);
    }
  }

  fn account_sub(&mut self, bytes: usize) {
    self.bytes -= bytes;
    if let Some(ledger) = &self.ledger {
      ledger.sub(bytes);
    }
  }

  fn drop_node(&mut self, page_id: PageId) {
    if let Some(entry) = self.node_map.remove(&page_id) {
      self.account_sub(entry.node.byte_len());
      self.counter.inc_evict();
    }
  }

  /// 逐出到预算内。
  ///
  /// Clock 至多两整轮：被引用过的条目获得一次保留机会；两轮后仍超限（例如
  /// 条目全部 referenced）则按 FIFO 强制淘汰，保证硬上限（对齐 C++ LRU 尾
  /// 严格推进，不会被全命中卡住）。单条超预算的条目因此也会被逐出，
  /// 即「放弃缓存该条」。
  fn evict_to_fit(&mut self) {
    if self.limit().is_none() {
      return;
    }
    let evict_start = slow_read::start();
    let mut budget = self.order_li.len().saturating_mul(2);
    while budget > 0 && self.over_limit() {
      budget -= 1;
      let Some(oldest) = self.order_li.pop_front() else {
        break;
      };
      let Some(entry) = self.node_map.get_mut(&oldest) else {
        // 已被移除的陈旧队列项，直接丢弃。
        continue;
      };
      if entry.referenced.replace(false) {
        self.order_li.push_back(oldest);
        continue;
      }
      self.drop_node(oldest);
    }
    // 两轮后仍超限：忽略引用位强制逐出。
    while self.over_limit() {
      let Some(oldest) = self.order_li.pop_front() else {
        break;
      };
      self.drop_node(oldest);
    }
    slow_read::stop("cache_evict", evict_start);
  }

  /// 重新计算均分额度后立即裁剪（缓存实例数变化时由 store 调用）。
  pub fn trim_to_budget(&mut self) {
    self.evict_to_fit();
  }

  pub fn remove(&mut self, page_id: PageId) {
    if let Some(entry) = self.node_map.remove(&page_id) {
      self.account_sub(entry.node.byte_len());
      // 队列项惰性清理：淘汰时发现节点不存在即丢弃，避免 O(N) retain。
    }
  }

  /// 写提交 / reopen 后把仍存活的条目重新标记为当前映射代际。
  ///
  /// 调用前提：变更过的逻辑页已被 `remove` 清掉（或 compact 只换物理页、
  /// 页内容不变），剩余条目的映射未变，可继续服务新代读者。旧快照读者的
  /// 代际仍小于新值，因此不会命中这些条目。
  ///
  /// 实现为 baseline 代际 + 例外清单：跟随 baseline 的条目无需逐条改，
  /// 只把自上次 `retag` 起以旧代插入的少数例外回落，整体 O(例外数)，
  /// 常规写批次没有例外即 O(1)，不再遍历整张缓存。
  ///
  /// `insert` 已拒绝早于 baseline 的代际，因此例外清单里的条目代际都
  /// `>=` 旧 baseline；代际值不复用，故这些条目读到的映射要么已提交、要么
  /// 属于当前在途批次（其提交由调用时机保证），可安全跟随新 baseline。
  ///
  /// 调用契约：`generation` 必然 ≥ 当前 `base_gen`——调用方取的是 mapper 的
  /// live 代际，写批次只会推进代际（见 `fkv_mapping::GenerationSource`）。
  /// debug 构建用断言锁住该不变量；release 下仍取 `max` 兜底，即使契约被破坏
  /// 也不会把 baseline 回退（回退会让高代例外被降级成 baseline 交给新读者）。
  pub fn retag(&mut self, generation: u64) {
    debug_assert!(
      generation >= self.base_gen,
      "retag 代际回退: generation={generation} base_gen={}",
      self.base_gen
    );
    self.base_gen = self.base_gen.max(generation);
    for page_id in take(&mut self.exception_li) {
      if let Some(entry) = self.node_map.get_mut(&page_id) {
        entry.gen_override = None;
      }
    }
  }

  /// 丢弃全部条目并把 baseline 抬到 `generation`（写批次回滚 / truncate 用）。
  ///
  /// 旧实现把 `base_gen` 归 0，等于让 `insert` 的「早于 baseline 即拒绝」
  /// 守卫恒为假：回滚之后在途旧快照回填的节点会被接受，下一次 `retag` 再把
  /// 它提升成 baseline，交给新读者使用（其子页可能已被释放复用）。代际现在
  /// 本 Store 内单调，回滚后的 live 代际大于本批用过的任何值，因此把 baseline
  /// 抬到该值即可：旧快照的回填全部被拒，新读者从空缓存正常重建。
  pub fn reset(&mut self, generation: u64) {
    self.node_map.clear();
    self.order_li.clear();
    self.exception_li.clear();
    // `account_sub(self.bytes)` 已把账本归零，无需再显式赋值。
    self.account_sub(self.bytes);
    self.base_gen = self.base_gen.max(generation);
  }

  /// reopen / attach 后按新映射裁剪：逻辑页仍映射到条目记录的 `file_page_id`
  /// 才保留，其余丢弃。
  ///
  /// 对齐 C++ `PageManager::InstallExternalSnapshot` 的 swizzle 复用：物理页
  /// 不变即缓存页仍有效。`resolve` 返回 `None`（页已释放 / 无效）或不同物理页
  /// 都视为失效。不复算页内容哈希：那需要 O(n) 读盘，反而抵消复用收益；
  /// append-only 分配（`AllocMode::Append`，`free` 为 no-op）保证同一物理页
  /// 的内容写后不再改变。
  pub fn retain_mapped(&mut self, mut resolve: impl FnMut(PageId) -> Option<FilePageId>) {
    let before = self.node_map.len();
    // 只保留 baseline 条目：例外条目的代际对应旧化身的在途 / 已回滚视图，
    // 物理页相同不能证明映射在两次提交之间未变，跨化身提升不安全
    // （r7a-core-02）。baseline 条目则来自已提交映射，物理页相同即内容相同。
    self.node_map.retain(|page_id, entry| {
      entry.gen_override.is_none() && resolve(*page_id) == Some(entry.node.file_page_id)
    });
    self.exception_li.clear();
    if self.node_map.len() == before {
      return;
    }
    // 有条目被丢弃：队列惰性清理项随节点一起作废，这里一次性压实。
    let node_map = &self.node_map;
    self
      .order_li
      .retain(|page_id| node_map.contains_key(page_id));
    let new_bytes: usize = node_map.values().map(|entry| entry.node.byte_len()).sum();
    self.account_sub(self.bytes - new_bytes);
  }

  /// 缓存条目数：store 汇总各表缓存占用、诊断指标都走它。
  pub fn len(&self) -> usize {
    self.node_map.len()
  }

  /// 近似占用字节数。
  pub fn byte_len(&self) -> usize {
    self.bytes
  }

  /// 与 [`Self::len`] 配对（clippy `len_without_is_empty`），缓存测试与
  /// 「超大条目被逐出后缓存为空」的断言直接用。
  pub fn is_empty(&self) -> bool {
    self.node_map.is_empty()
  }
}

/// 缓存销毁时归还共享账本的占用与实例名额（store 驱逐表 / reopen 替换缓存）。
impl Drop for IndexCache {
  fn drop(&mut self) {
    if let Some(ledger) = &self.ledger {
      ledger.sub(self.bytes);
      ledger.detach();
    }
  }
}

/// 读路径还原 entry value：`None` 直接透传，`Standalone` 走自带 varint 前缀的
/// raw zstd，`Dictionary` 用 table 字典解压。对齐 C++ `GetValue`。
/// 未压缩值借用页内切片，零拷贝。
pub fn decompress_value<'a>(
  value: &'a [u8],
  compression: CompressionType,
  dict: Option<&DictCompression>,
) -> Result<Cow<'a, [u8]>> {
  match compression {
    CompressionType::None => Ok(Cow::Borrowed(value)),
    CompressionType::Standalone => decompress_raw(value).map(Cow::Owned),
    CompressionType::Dictionary => dict
      .ok_or(KvError::Corrupted)
      .and_then(|dict| dict.decompress(value))
      .map(Cow::Owned),
  }
}
