//! 每分区独立 Store：跨分区写可并发，同分区仍由 `WriteGate` 串行。
//!
//! 分区之间在盘面上本来就互不共享：data / segment / manifest 都在
//! `part_dir(tbl)` 下，main 分支每表一份 manifest。因此把表状态按分区拆成
//! 独立 `Store`，共享同一个 IO 后端（`MeteredIo<StoreBackend>` 内部是 `Rc`
//! 克隆，fd 缓存 / 路径索引 / 写闸门仍是全 shard 一份）与 index 缓存字节
//! 账本（聚合上限仍按全部缓存实例均分），就能让不同分区的写在各自的
//! `&mut Store` 上真并发，而不需要把 `Store` 改成内部可变或拆 per-table 锁。
//!
//! 读 / 写边界：
//! - 读任务持 `Rc<ShardStore>` 克隆，多个读共享同一分区 Store；
//! - 写任务需要 `&mut Store`：先 drain 在飞读，再把 `Rc` 拆成独占值交给
//!   写任务（见 shard 的 `WriteTask`），完成后放回；写进行中该分区没有 Store
//!   可借，同分区的新读 / 写都要等它结束（同分区 FIFO 语义本来如此）。
//!
//! term / branch 身份仍是 shard 级：attach 时统一失效共享缓存，新分区
//! Store 只采纳字段（[`adopt_attach`](fkv_store::Store::adopt_attach)），
//! 不重复清理其它分区的 IO 缓存。

use std::{rc::Rc, sync::Arc};

use fkv_btree::{CacheLedger, CacheStat};
use fkv_error::{KvError, Result};
use fkv_io::PageIo;
use fkv_large::SegmentLayout;
use fkv_metrics::{MeteredIo, ShardMetric};
use fkv_store::Store;
use fkv_types::TableIdent;
use gxhash::HashMap;

use super::backend::StoreBackend;

/// shard 线程内的单分区 Store 类型（读任务与写任务共用同一实例）。
pub(crate) type ShardStore = Store<MeteredIo<StoreBackend>>;

/// 每分区 Store 的构造参数：shard 启动时从 `ShardConfig` 固化。
#[derive(Clone, Copy)]
pub(crate) struct StoreCfg {
  pub page_size: usize,
  pub restart_interval: u16,
  pub index_restart_interval: u16,
  pub layout: SegmentLayout,
  pub large_value_threshold: usize,
  pub verify_checksum: bool,
  pub overflow_pointers: u8,
  pub file_amplify_factor: u8,
  pub segment_file_amplify_factor: u8,
  pub manifest_limit: usize,
  pub index_cache_bytes: usize,
  pub index_cache_total_bytes: usize,
  pub pages_per_file_shift: u8,
  pub yield_budget_us: u64,
  pub compression_level: Option<i32>,
}

/// 单个分区的槽位。
struct Slot {
  /// 写任务运行期间由任务独占（`None` = 该分区写进行中）。
  store: Option<Rc<ShardStore>>,
  /// 上次上报给指标的 index 缓存累计值（增量对账基线）。
  cache_stat: CacheStat,
  /// 最近使用代际（root meta 预算驱逐用）。
  touch: u64,
  /// 最近一次记账的 root meta 字节（增量预算判定的单槽基线）。
  root_meta_bytes: usize,
}

/// 分区 → Store 映射。全部方法只在 shard 线程内调用。
pub(crate) struct PartitionStoreMap {
  /// 共享 IO 后端句柄：新分区 Store 复制一个 `Rc` 克隆。
  backend: MeteredIo<StoreBackend>,
  cfg: StoreCfg,
  /// index 缓存字节账本：所有分区 Store 共享，聚合上限不被分区数放大。
  ledger: Arc<CacheLedger>,
  map: HashMap<TableIdent, Slot>,
  /// 单调触摸代际，用于 root meta 预算的近似 LRU。
  clock: u64,
  /// 全 shard root meta 字节预算（`0` = 不限制）。
  root_meta_budget: usize,
  /// 各槽位 [`Slot::root_meta_bytes`] 之和。
  ///
  /// 预算判定只做一次比较，不再每次新建分区都遍历全部槽位重算
  /// （旧实现冷启动 n 个分区是 O(n² log n)）。增量来源：
  /// - 新建 / 写回 / 触摸时重算该分区；
  /// - 驱逐时按槽位基线扣减；
  /// - [`Self::sync_cache_metrics`] 顺带全量对账。
  root_meta_total: usize,
}

impl PartitionStoreMap {
  pub(crate) fn new(
    backend: MeteredIo<StoreBackend>,
    cfg: StoreCfg,
    root_meta_budget: usize,
  ) -> Self {
    Self {
      backend,
      cfg,
      ledger: Arc::new(CacheLedger::default()),
      map: HashMap::default(),
      clock: 0,
      root_meta_budget,
      root_meta_total: 0,
    }
  }

  /// 共享 IO 后端（非表状态操作：预热 / 列举 / term 文件 / 缓存失效）。
  pub(crate) fn backend(&self) -> &MeteredIo<StoreBackend> {
    &self.backend
  }

  /// 表是否已打开。槽位不存在 / 写任务持有 Store 都按未就绪处理。
  pub(crate) fn is_open(&self, tbl: &TableIdent) -> bool {
    self
      .map
      .get(tbl)
      .and_then(|slot| slot.store.as_ref())
      .is_some_and(|store| store.is_open(tbl))
  }

  /// 该分区的写任务是否正持有 Store。
  pub(crate) fn is_writing(&self, tbl: &TableIdent) -> bool {
    self.map.get(tbl).is_some_and(|slot| slot.store.is_none())
  }

  /// 分区槽位是否存在（表不一定已打开）。
  pub(crate) fn has_slot(&self, tbl: &TableIdent) -> bool {
    self.map.contains_key(tbl)
  }

  /// 读路径共享 Store（只读；可被多个读任务同时持有）。
  pub(crate) fn store(&self, tbl: &TableIdent) -> Result<&ShardStore> {
    match self.map.get(tbl) {
      Some(slot) => match slot.store.as_deref() {
        Some(store) => Ok(store),
        None => Err(KvError::Busy),
      },
      None => Err(KvError::ResourceMissing),
    }
  }

  /// 读任务提交用：克隆一份 `Rc`。
  pub(crate) fn store_rc(&self, tbl: &TableIdent) -> Result<Rc<ShardStore>> {
    match self.map.get(tbl) {
      Some(slot) => slot.store.clone().ok_or(KvError::Busy),
      None => Err(KvError::ResourceMissing),
    }
  }

  /// 串行路径独占 Store。
  ///
  /// 读任务持有克隆时 `Rc::get_mut` 失败 → `Busy`；调用方必须先 drain 读。
  pub(crate) fn store_mut(&mut self, tbl: &TableIdent) -> Result<&mut ShardStore> {
    let slot = self.map.get_mut(tbl).ok_or(KvError::ResourceMissing)?;
    match slot.store.as_mut() {
      Some(rc) => Rc::get_mut(rc).ok_or(KvError::Busy),
      None => Err(KvError::Busy),
    }
  }

  /// 取（必要时创建）该分区的 Store 独占引用；返回前登记 root meta 预算。
  pub(crate) fn ensure_store(
    &mut self,
    tbl: &TableIdent,
    branch: &str,
    term: u64,
  ) -> Result<&mut ShardStore> {
    if !self.map.contains_key(tbl) {
      let store = self.new_store(branch, term)?;
      // 新 Store 还没有表状态；调用方随后 `reopen` / `open_empty`。
      let root_meta_bytes = store.root_meta_bytes();
      let touch = self.next_touch();
      self.map.insert(
        tbl.clone(),
        Slot {
          store: Some(Rc::new(store)),
          cache_stat: CacheStat::default(),
          touch,
          root_meta_bytes,
        },
      );
      self.root_meta_total = self.root_meta_total.saturating_add(root_meta_bytes);
      self.enforce_root_meta_budget(tbl);
    }
    self.store_mut(tbl)
  }

  /// 写任务取走分区 Store（`Rc` 必须唯一，调用前已 drain 在飞读）。
  pub(crate) fn take(&mut self, tbl: &TableIdent) -> Result<ShardStore> {
    let slot = self.map.get_mut(tbl).ok_or(KvError::ResourceMissing)?;
    let rc = slot.store.take().ok_or(KvError::Busy)?;
    match Rc::try_unwrap(rc) {
      Ok(store) => Ok(store),
      Err(rc) => {
        // 仍有读任务持有克隆：放回并让调用方按 Busy 重试。
        slot.store = Some(rc);
        Err(KvError::Busy)
      }
    }
  }

  /// 写任务完成：放回 Store。
  ///
  /// 保留槽位里上一次上报的缓存计数基线，由随后的
  /// [`Self::sync_cache_metrics`] 统一算增量——写任务期间累积的
  /// hit / miss / evict 不会因为换了一次持有者而漏报。
  pub(crate) fn put_back(&mut self, tbl: &TableIdent, store: ShardStore) {
    let root_meta_bytes = store.root_meta_bytes();
    let touch = self.next_touch();
    let slot = self.map.entry(tbl.clone()).or_insert_with(|| Slot {
      store: None,
      cache_stat: CacheStat::default(),
      touch,
      root_meta_bytes: 0,
    });
    slot.store = Some(Rc::new(store));
    slot.touch = touch;
    self.root_meta_total = self
      .root_meta_total
      .saturating_sub(slot.root_meta_bytes)
      .saturating_add(root_meta_bytes);
    slot.root_meta_bytes = root_meta_bytes;
  }

  /// 写任务 panic：Store 已随任务丢失，只能丢槽位，后续访问按需 reopen。
  pub(crate) fn lose(&mut self, tbl: &TableIdent) {
    if let Some(slot) = self.map.remove(tbl) {
      self.root_meta_total = self.root_meta_total.saturating_sub(slot.root_meta_bytes);
    }
    self.backend.evict_table(tbl);
  }

  /// 标记最近使用（读 / 写 / 打开时调用）。
  pub(crate) fn touch(&mut self, tbl: &TableIdent) {
    let touch = self.next_touch();
    if let Some(slot) = self.map.get_mut(tbl) {
      slot.touch = touch;
    }
    // 读 / 写可能改变了该分区的 root 状态：只重算当前分区，不做全表扫描。
    self.refresh_slot_bytes(tbl);
  }

  /// 驱逐单表并丢弃槽位（`Drop` / detach / root meta 预算）。
  pub(crate) fn evict_table(&mut self, tbl: &TableIdent) {
    if let Some(mut slot) = self.map.remove(tbl) {
      self.root_meta_total = self.root_meta_total.saturating_sub(slot.root_meta_bytes);
      if let Some(rc) = slot.store.take()
        && let Ok(mut store) = Rc::try_unwrap(rc)
      {
        store.evict_table(tbl);
      }
    }
    self.backend.evict_table(tbl);
  }

  /// 清空全部分区（attach / detach 的身份切换）。
  pub(crate) fn clear(&mut self) {
    let tbl_li: Vec<TableIdent> = self.map.keys().cloned().collect();
    for tbl in tbl_li {
      self.evict_table(&tbl);
    }
  }

  /// 全 shard 是否还有待清理 TTL。
  pub(crate) fn pending_ttl_len(&self) -> usize {
    self
      .map
      .values()
      .filter_map(|slot| slot.store.as_ref())
      .map(|store| store.pending_ttl_len())
      .sum()
  }

  /// 该分区是否需要写后维护（compact / GC）。
  pub(crate) fn needs_maintain(&self, tbl: &TableIdent) -> bool {
    self
      .map
      .get(tbl)
      .and_then(|slot| slot.store.as_ref())
      // data / segment 两侧任一超放大阈值都要触发写后维护：只查 data 侧会让
      // `segment_file_amplify_factor` 在 Runtime 下永不生效。
      .is_some_and(|store| store.needs_maintain(tbl) || store.needs_segment_maintain(tbl))
  }

  /// 增量同步 index 缓存计数与占用到 shard 指标。
  ///
  /// 写任务进行中的分区保持上次数值（Store 在任务手里，读不到）。
  pub(crate) fn sync_cache_metrics(&mut self, metrics: &ShardMetric) {
    let mut bytes = 0usize;
    for slot in self.map.values_mut() {
      let Some(store) = slot.store.as_ref() else {
        bytes += slot.cache_stat.bytes;
        continue;
      };
      let now = store.index_cache_stat();
      metrics.add_cache_counters(
        now.hit.saturating_sub(slot.cache_stat.hit),
        now.miss.saturating_sub(slot.cache_stat.miss),
        now.evict.saturating_sub(slot.cache_stat.evict),
      );
      slot.cache_stat = now;
      bytes += now.bytes;
      // 顺带对账 root meta 增量记账（索引缓存也计入 root 状态字节）。
      let root_meta_bytes = store.root_meta_bytes();
      self.root_meta_total = self
        .root_meta_total
        .saturating_sub(slot.root_meta_bytes)
        .saturating_add(root_meta_bytes);
      slot.root_meta_bytes = root_meta_bytes;
    }
    metrics.set_index_cache_bytes(u64::try_from(bytes).unwrap_or(u64::MAX));
  }

  /// 重算单个槽位的 root meta 字节并修正总量（O(该分区表状态数)）。
  fn refresh_slot_bytes(&mut self, tbl: &TableIdent) {
    let Some(slot) = self.map.get_mut(tbl) else {
      return;
    };
    let Some(store) = slot.store.as_ref() else {
      return;
    };
    let root_meta_bytes = store.root_meta_bytes();
    if root_meta_bytes == slot.root_meta_bytes {
      return;
    }
    self.root_meta_total = self
      .root_meta_total
      .saturating_sub(slot.root_meta_bytes)
      .saturating_add(root_meta_bytes);
    slot.root_meta_bytes = root_meta_bytes;
  }

  fn next_touch(&mut self) -> u64 {
    self.clock = self.clock.wrapping_add(1);
    self.clock
  }

  fn new_store(&self, branch: &str, term: u64) -> Result<ShardStore> {
    let cfg = &self.cfg;
    let mut store = Store::new(self.backend.clone(), cfg.page_size, cfg.restart_interval)
      .with_index_restart_interval(cfg.index_restart_interval)
      .with_large_value(cfg.layout, cfg.large_value_threshold)
      .with_verify_checksum(cfg.verify_checksum)
      .with_overflow_pointers(cfg.overflow_pointers)
      .with_file_amplify_factor(cfg.file_amplify_factor)
      .with_segment_amplify_factor(cfg.segment_file_amplify_factor)
      // TTL 清理走写后维护（对齐 C++ TriggerTTL -> 独立 CleanExpired）。
      .with_deferred_ttl(true)
      .with_manifest_limit(cfg.manifest_limit)
      .with_index_cache_bytes(cfg.index_cache_bytes)
      .with_index_cache_total_bytes(cfg.index_cache_total_bytes)
      .with_index_cache_ledger(self.ledger.clone())
      .with_pages_per_file_shift(cfg.pages_per_file_shift)
      .with_yield_budget_us(cfg.yield_budget_us)
      // root meta 预算由 shard 级 LRU 统一执行：单表 Store 内部无法驱逐自己。
      .with_root_meta_cache_bytes(0);
    if let Some(level) = cfg.compression_level {
      store = store.with_compression_level(level);
    }
    store.adopt_attach(branch, term)?;
    Ok(store)
  }

  /// 超出 root meta 预算时按近似 LRU 驱逐空闲分区。
  ///
  /// 只在新分区 Store 创建后调用：分区内单表状态随写入增长的部分由
  /// [`PartitionStoreMap::touch`] 记账，下次新建分区时统一收敛。
  fn enforce_root_meta_budget(&mut self, keep: &TableIdent) {
    if self.root_meta_budget == 0 {
      return;
    }
    // 增量总量：预算内直接返回，不再逐槽累加（旧实现每次新建分区全表扫描）。
    let mut total = self.root_meta_total;
    if total <= self.root_meta_budget {
      return;
    }
    let mut cand_li: Vec<(u64, TableIdent)> = self
      .map
      .iter()
      .filter(|(tbl, slot)| {
        // 只驱逐空闲分区：写任务持有（None）或仍被读任务借用（count > 1）都不能动。
        *tbl != keep
          && slot
            .store
            .as_ref()
            .is_some_and(|store| Rc::strong_count(store) == 1)
      })
      .map(|(tbl, slot)| (slot.touch, tbl.clone()))
      .collect();
    cand_li.sort_unstable_by_key(|(touch, _)| *touch);
    for (_, tbl) in cand_li {
      if total <= self.root_meta_budget {
        break;
      }
      let bytes = self.map.get(&tbl).map_or(0, |slot| slot.root_meta_bytes);
      self.evict_table(&tbl);
      total = total.saturating_sub(bytes);
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{rc::Rc, sync::Arc};

  use fkv_io_mem::MemStore;
  use fkv_metrics::{MeteredIo, ShardMetric};
  use fkv_page::DEFAULT_OVERFLOW_POINTERS;
  use fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT;

  use super::*;

  fn partition_map(root_meta_budget: usize) -> PartitionStoreMap {
    let metrics = Arc::new(ShardMetric::new(0));
    let io = MeteredIo::new(
      StoreBackend::Mem(Rc::new(
        MemStore::with_pages_per_file(512, DEFAULT_PAGES_PER_FILE_SHIFT)
          .with_segment_config(4096, 2),
      )),
      metrics,
    );
    let cfg = StoreCfg {
      page_size: 512,
      restart_interval: 16,
      index_restart_interval: 16,
      layout: SegmentLayout::new(4096, 2).unwrap(),
      large_value_threshold: 1024,
      verify_checksum: true,
      overflow_pointers: DEFAULT_OVERFLOW_POINTERS,
      file_amplify_factor: 0,
      segment_file_amplify_factor: 0,
      manifest_limit: 8 * 1024 * 1024,
      index_cache_bytes: 1024 * 1024,
      index_cache_total_bytes: 1024 * 1024,
      pages_per_file_shift: DEFAULT_PAGES_PER_FILE_SHIFT,
      yield_budget_us: 0,
      compression_level: None,
    };
    PartitionStoreMap::new(io, cfg, root_meta_budget)
  }

  fn slot_sum(map: &PartitionStoreMap) -> usize {
    map.map.values().map(|slot| slot.root_meta_bytes).sum()
  }

  /// 增量总量必须与逐槽基线一致：新建 / 写回 / 驱逐后都不漂移。
  #[test]
  fn root_meta_total_tracks_slots() {
    let mut map = partition_map(0);
    let a = TableIdent::new("acc", 1).unwrap();
    let b = TableIdent::new("acc", 2).unwrap();
    // 空 Store 的 root 状态为 0：先打开空表再触摸，才有可记账的字节。
    map.ensure_store(&a, "main", 1).unwrap().open_empty(&a);
    map.touch(&a);
    map.ensure_store(&b, "main", 1).unwrap().open_empty(&b);
    map.touch(&b);
    assert!(map.root_meta_total > 0);
    assert_eq!(map.root_meta_total, slot_sum(&map));

    let store = map.take(&a).unwrap();
    map.put_back(&a, store);
    assert_eq!(map.root_meta_total, slot_sum(&map));

    map.touch(&a);
    assert_eq!(map.root_meta_total, slot_sum(&map));

    map.evict_table(&a);
    assert_eq!(map.root_meta_total, slot_sum(&map));
    assert!(!map.has_slot(&a));
  }

  /// 超预算只按 LRU 驱逐，不动本轮新建（`keep`）的分区。
  #[test]
  fn root_meta_budget_evicts_lru_only() {
    let mut map = partition_map(1);
    let a = TableIdent::new("acc", 1).unwrap();
    let b = TableIdent::new("acc", 2).unwrap();
    map.ensure_store(&a, "main", 1).unwrap().open_empty(&a);
    map.touch(&a);
    assert!(map.root_meta_total > 1, "A 必须已记账");
    map.ensure_store(&b, "main", 1).unwrap().open_empty(&b);
    map.touch(&b);
    assert!(!map.has_slot(&a), "超预算未驱逐最久未用的分区");
    assert!(map.has_slot(&b), "本轮新建的分区被自己驱逐");
    assert_eq!(map.map.len(), 1);
    assert_eq!(map.root_meta_total, slot_sum(&map));
  }
}
