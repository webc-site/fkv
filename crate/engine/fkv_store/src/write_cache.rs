//! 写路径页缓存：把 COW 写路径刚写出的页留给下一批读回，命中即免一次磁盘读。
//!
//! 背景（perf 归因）：每批写入都是 COW——路由读 root → 内部 index → leaf index
//! → data 页，重写后再写下新的物理页。批出口 `invalidate_index_cache` 只服务
//! 读路径的 index 缓存，写路径一个页都不缓存，于是**每一批都要把上一批刚写下
//! 的同一路径重新读一遍**。单键批（batch-size 1）下这一点被放大到 14.7 次磁盘
//! 页读 / put，是 fkv 相对 C++ EloqStore 慢数倍的主因（C++ `WriteTask` 通过
//! `old_mapping_->GetSwizzlingHandle(page_id)` 走 128 MiB buffer pool，从不为此
//! 触盘）。
//!
//! 结构：`CachedWriteIo` 是 `PageIo` 装饰器，包住真实 IO 并在**写路径**上使用。
//! 读先探缓存、未命中读盘后回填；写成功后把写出的字节放进缓存；页释放由上层
//! 逐条失效（见下）。
//!
//! ## 键的选择：物理 `FilePageId`
//!
//! 缓存键是物理页 id，而不是逻辑 `PageId`，理由：
//!
//! - 装饰器位于 [`PageIo`] 边界，`read_page*` / `write_page*` 只暴露物理页 id；
//!   逻辑 id 与物理 id 的换算由 `PageMapper::file_page_id` 在调用点完成，装饰器
//!   拿不到 mapper（写路径同时可变借用它）。
//! - 写路径的每次读都在调用点用**当前**映射解出物理页，因此 COW 后的页读到的是
//!   新物理页——正好是上一批写出的那个；键与「写出的落点」天然一致。
//! - C++ 的 buffer pool 同样按物理落点缓存（swizzling 句柄挂在 mapping 条目上），
//!   安全性来自 `UpdateMapping` / `FreePage` 覆盖句柄。
//!
//! ## 硬不变量
//!
//! 对缓存里任意 `FilePageId X`：`cache[X]` 等于 `X` 当前盘上内容。
//!
//! 盘上内容只可能被以下路径改写，逐条对账：
//!
//! - **本装饰器写出**：写完即回填，缓存与盘面同步；
//! - **页释放 / 复用**：`PageMapper` 把 `free_page` 释放的物理页与 `remap_page`
//!   退役的旧页记入 `freed_file_page_li`，Store 在批次出口 `take_freed_file_page_li`
//!   逐条 `remove`。Pooled 分配器此后把这些 id 复用给新页时，新页必须先被写出
//!   （回填覆盖旧条目）才会被读到，因此复用页永不可能命中陈旧内容；
//! - **compaction**：`compact_file_li` 走裸 IO 写新物理页，Store 在 compaction
//!   路径整体清空该表缓存；
//! - **reopen / 截断 / 回滚**：映射被整体替换或回退，Store 在相应出口清空缓存
//!   （`install_replay_state` / `truncate*` / `settle_mapper_batch` 失败分支）；
//! - **旁路写**：任何绕过 `Store` 直接对 `page_io()` 落盘的写（外部工具 / 测试）
//!   必须调用 [`Store::invalidate_write_cache`] 告知，与
//!   [`Store::invalidate_path_cache`] 同一契约。
//!
//! 缓存只影响**读到的字节内容**，不改变写出的字节、写顺序、fsync 顺序与
//! manifest 形态：开启与关闭缓存产出逐字节相同的盘面（见 `tests/write_cache.rs`）。
//!
//! ## 实测（macOS，rows=200000、batch-size=1、128B 值，`fkv_bench`）
//!
//! 缓存把写路径磁盘页读从 14.7 次 / put 降到 0（`io_read_at_num` 2,932,554 → 0，
//! 命中率 1.00）。但只修读盘不足以达到吞吐目标：游标下降当时每跨一个子项就
//! 重读并重解码整张 index 页（`RouteCursor` 的按键二分下降同时消除了这部分 CPU，
//! 见 `fkv_task_write::batch`），且每批还有 ~115us 的 fsync / manifest 固定开销。
//! 三项合计让单键批 `put_avg_us` 从 416us 降到 153us（`put_kv_s` 2397 → 6506）。

use core::{cell::Cell, fmt, mem::take};
use std::{cell::RefCell, collections::VecDeque};

use fkv_error::{KvError, Result};
use fkv_io::PageIo;
use fkv_types::{FilePageId, TableIdent};
use gxhash::{HashMap, HashSet};

/// 写路径页缓存统计快照（与 `fkv_btree::CacheStat` 同形，便于 bench 汇总）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WriteCacheStat {
  /// 命中次数（免掉一次磁盘页读）。
  pub hit: u64,
  /// 未命中次数（回源读到磁盘）。
  pub miss: u64,
  /// 逐出次数。
  pub evict: u64,
  /// 当前常驻近似字节数。
  pub bytes: usize,
  /// 当前缓存页数。
  pub len: usize,
  /// 字节上限；`0` 表示缓存关闭。
  pub capacity: usize,
}

#[derive(Debug)]
struct CacheEntry {
  page: Vec<u8>,
  /// Clock 引用位：命中过的页在逐出时获得一次保留机会。
  referenced: Cell<bool>,
}

/// 写路径页缓存：`FilePageId -> 整页字节`，按近似字节数做 Clock 淘汰。
///
/// `capacity == 0` 表示**显式关闭**（差分测试的对照组）：`insert` 直接丢弃，
/// `get` 恒 miss，不再产生任何分配与记录。
pub(crate) struct WritePageCache {
  page_map: HashMap<FilePageId, CacheEntry>,
  /// FIFO 顺序（惰性清理，允许残留已被 `remove` 的陈旧项）。
  order_li: VecDeque<FilePageId>,
  bytes: usize,
  capacity: usize,
  hit: Cell<u64>,
  miss: Cell<u64>,
  evict: Cell<u64>,
}

impl fmt::Debug for WritePageCache {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // gxhash 的 `HashMap` 不实现 `Debug`；只打账目，不打页字节。
    f.debug_struct("WritePageCache")
      .field("len", &self.page_map.len())
      .field("bytes", &self.bytes)
      .field("capacity", &self.capacity)
      .field("hit", &self.hit.get())
      .field("miss", &self.miss.get())
      .field("evict", &self.evict.get())
      .finish()
  }
}

impl WritePageCache {
  pub(crate) fn new(capacity: usize) -> Self {
    Self {
      page_map: HashMap::default(),
      order_li: VecDeque::new(),
      bytes: 0,
      capacity,
      hit: Cell::new(0),
      miss: Cell::new(0),
      evict: Cell::new(0),
    }
  }

  /// `true` 表示缓存启用；关闭时装饰器退化为直通。
  pub(crate) fn enabled(&self) -> bool {
    self.capacity != 0
  }

  fn limit(&self) -> Option<usize> {
    (self.capacity != 0).then_some(self.capacity)
  }

  fn over_limit(&self) -> bool {
    self.limit().is_some_and(|limit| self.bytes > limit)
  }

  /// 探测一页；命中返回整页字节并置引用位、记一次 hit，未命中记一次 miss。
  pub(crate) fn get(&self, file_page_id: FilePageId) -> Option<&[u8]> {
    if !self.enabled() {
      return None;
    }
    let Some(entry) = self.page_map.get(&file_page_id) else {
      self.miss.set(self.miss.get() + 1);
      return None;
    };
    entry.referenced.set(true);
    self.hit.set(self.hit.get() + 1);
    Some(entry.page.as_slice())
  }

  /// 回填 / 覆盖一页；容量不足时按 Clock 逐出到预算内。
  pub(crate) fn insert(&mut self, file_page_id: FilePageId, page: Vec<u8>) {
    if !self.enabled() {
      return;
    }
    if let Some(old) = self.page_map.get_mut(&file_page_id) {
      // 同页覆盖：不重复入队（队里已有本页项），只是换字节。
      self.bytes = self.bytes - old.page.len() + page.len();
      old.page = page;
      old.referenced.set(true);
      self.evict_to_fit();
      return;
    }
    // 陈旧队列项（`remove` 后残留、同 id 重插）会累积；超阈值时压实去重，
    // 保证 `order_li` 与条目数同阶、摊销 O(1)。
    if self.order_li.len() > self.page_map.len().saturating_mul(2).saturating_add(64) {
      let page_map = &self.page_map;
      let mut seen = HashSet::default();
      self
        .order_li
        .retain(|file_page_id| page_map.contains_key(file_page_id) && seen.insert(*file_page_id));
    }
    self.order_li.push_back(file_page_id);
    self.bytes += page.len();
    self.page_map.insert(
      file_page_id,
      CacheEntry {
        page,
        referenced: Cell::new(true),
      },
    );
    self.evict_to_fit();
  }

  /// 失效一页（页释放 / 复用 / 显式失效）。不存在时是空操作。
  pub(crate) fn remove(&mut self, file_page_id: FilePageId) {
    if let Some(entry) = self.page_map.remove(&file_page_id) {
      self.bytes -= entry.page.len();
      // 队列项惰性清理：淘汰时发现条目不存在即丢弃，避免 O(N) retain。
    }
  }

  /// 清空全部条目（compaction / reopen / 截断 / 回滚）。
  pub(crate) fn clear(&mut self) {
    self.page_map.clear();
    self.order_li.clear();
    self.bytes = 0;
  }

  pub(crate) fn stat(&self) -> WriteCacheStat {
    WriteCacheStat {
      hit: self.hit.get(),
      miss: self.miss.get(),
      evict: self.evict.get(),
      bytes: self.bytes,
      len: self.page_map.len(),
      capacity: self.capacity,
    }
  }

  pub(crate) fn byte_len(&self) -> usize {
    self.bytes
  }

  /// 逐出到预算内：Clock 至多两整轮（引用位给一次保留机会），仍超限则按 FIFO
  /// 强制淘汰，保证硬上限（对齐 `IndexCache::evict_to_fit`，不会被全命中卡住）。
  fn evict_to_fit(&mut self) {
    if self.limit().is_none() {
      return;
    }
    let mut budget = self.order_li.len().saturating_mul(2);
    while budget > 0 && self.over_limit() {
      budget -= 1;
      let Some(oldest) = self.order_li.pop_front() else {
        break;
      };
      let Some(entry) = self.page_map.get_mut(&oldest) else {
        continue;
      };
      if entry.referenced.replace(false) {
        self.order_li.push_back(oldest);
        continue;
      }
      self.drop_page(oldest);
    }
    while self.over_limit() {
      let Some(oldest) = self.order_li.pop_front() else {
        break;
      };
      self.drop_page(oldest);
    }
  }

  fn drop_page(&mut self, file_page_id: FilePageId) {
    if let Some(entry) = self.page_map.remove(&file_page_id) {
      self.bytes -= entry.page.len();
      self.evict.set(self.evict.get() + 1);
    }
  }
}

/// 写路径 `PageIo` 装饰器：读先探缓存，写成功后回填。
///
/// 缓存句柄来自目标表的 `TableState`，因此装饰器天然是 per-table 的：调用方
/// 传进来的 `tbl` 必须与句柄所属表一致（由 `Store` 的构造点保证）。
pub(crate) struct CachedWriteIo<'a, I> {
  inner: &'a I,
  cache: &'a RefCell<WritePageCache>,
  page_size: usize,
}

impl<'a, I: PageIo> CachedWriteIo<'a, I> {
  pub(crate) fn new(inner: &'a I, cache: &'a RefCell<WritePageCache>, page_size: usize) -> Self {
    Self {
      inner,
      cache,
      page_size,
    }
  }

  /// 目标缓冲不足整页时按底层契约报 `InvalidArgs`（避免把短页缓存进来）。
  fn check_dst(&self, len: usize) -> Result<()> {
    if len < self.page_size {
      return Err(KvError::InvalidArgs);
    }
    Ok(())
  }

  /// 命中缓存时把整页拷进 `dst`；返回是否命中。
  fn fill_from_cache(&self, file_page_id: FilePageId, dst: &mut [u8]) -> bool {
    let cache = self.cache.borrow();
    let Some(page) = cache.get(file_page_id) else {
      return false;
    };
    // 命中快照下 `dst` 长度已校验 ≥ page_size；缓存条目恒为整页。
    let len = page.len().min(dst.len());
    dst[..len].copy_from_slice(&page[..len]);
    true
  }

  /// 读盘后回填（只取整页前缀，忽略调用方缓冲的尾部余量）。
  fn refill(&self, file_page_id: FilePageId, buf: &[u8]) {
    let len = buf.len().min(self.page_size);
    self
      .cache
      .borrow_mut()
      .insert(file_page_id, buf[..len].to_vec());
  }

  fn insert_page(&self, file_page_id: FilePageId, page: &[u8]) {
    let len = page.len().min(self.page_size);
    self
      .cache
      .borrow_mut()
      .insert(file_page_id, page[..len].to_vec());
  }
}

impl<I: PageIo> PageIo for CachedWriteIo<'_, I> {
  fn set_file_identity(&self, branch: &str, term: u64) {
    self.inner.set_file_identity(branch, term);
  }

  fn set_file_range_li(&self, tbl: &TableIdent, range_li: &[fkv_types::BranchFileRange]) {
    self.inner.set_file_range_li(tbl, range_li);
  }

  fn preload_path_li(&self, tbl: &TableIdent) -> usize {
    self.inner.preload_path_li(tbl)
  }

  fn invalidate_path_cache(&self) {
    self.inner.invalidate_path_cache();
  }

  fn set_verify_checksum(&self, enabled: bool) {
    self.inner.set_verify_checksum(enabled);
  }

  fn evict_table(&self, tbl: &TableIdent) {
    self.inner.evict_table(tbl);
  }

  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    self.check_dst(dst.len())?;
    if self.fill_from_cache(file_page_id, dst) {
      return Ok(());
    }
    self.inner.read_page(tbl, file_page_id, dst).await?;
    self.refill(file_page_id, dst);
    Ok(())
  }

  async fn read_page_owned(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    buf: &mut Vec<u8>,
  ) -> Result<()> {
    self.check_dst(buf.len())?;
    if self.fill_from_cache(file_page_id, buf) {
      return Ok(());
    }
    // 转发给内层：本地后端可直接把页读进调用方 `Vec`（零整页拷贝）。
    self.inner.read_page_owned(tbl, file_page_id, buf).await?;
    self.refill(file_page_id, buf);
    Ok(())
  }

  async fn read_page_li(
    &self,
    tbl: &TableIdent,
    file_page_id_li: &[FilePageId],
    page_buf_li: &mut [Vec<u8>],
  ) -> Result<()> {
    if file_page_id_li.len() != page_buf_li.len() {
      return Err(KvError::InvalidArgs);
    }
    // 命中项直接拷贝；未命中项收集成一批交给内层（保留后端的批量读语义）。
    let mut miss_id_li: Vec<FilePageId> = Vec::new();
    let mut miss_idx_li: Vec<usize> = Vec::new();
    for (idx, file_page_id) in file_page_id_li.iter().copied().enumerate() {
      let Some(buf) = page_buf_li.get_mut(idx) else {
        return Err(KvError::InvalidArgs);
      };
      self.check_dst(buf.len())?;
      if self.fill_from_cache(file_page_id, buf) {
        continue;
      }
      miss_id_li.push(file_page_id);
      miss_idx_li.push(idx);
    }
    if miss_id_li.is_empty() {
      return Ok(());
    }
    let mut miss_buf_li: Vec<Vec<u8>> = miss_idx_li
      .iter()
      .map(|idx| take(&mut page_buf_li[*idx]))
      .collect();
    let result = self
      .inner
      .read_page_li(tbl, &miss_id_li, &mut miss_buf_li)
      .await;
    for ((idx, file_page_id), buf) in miss_idx_li.iter().zip(miss_id_li).zip(miss_buf_li) {
      if result.is_ok() {
        self.refill(file_page_id, &buf);
      }
      page_buf_li[*idx] = buf;
    }
    result
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    self.inner.write_page(tbl, file_page_id, src).await?;
    // 写成功后才回填：缓存的语义是「盘上已有这份字节」。
    self.insert_page(file_page_id, src);
    Ok(())
  }

  async fn write_page_li(&self, tbl: &TableIdent, page_li: &[(FilePageId, Vec<u8>)]) -> Result<()> {
    // 必须整批转发：内层会把连续页合并成一次写（perf 关键路径）。
    self.inner.write_page_li(tbl, page_li).await?;
    for (file_page_id, page) in page_li {
      self.insert_page(*file_page_id, page);
    }
    Ok(())
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.inner.sync_data(tbl).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const PAGE: usize = 512;

  fn page(fill: u8) -> Vec<u8> {
    vec![fill; PAGE]
  }

  #[test]
  fn disabled_cache_never_serves_or_stores() {
    let mut cache = WritePageCache::new(0);
    assert!(!cache.enabled());
    cache.insert(7, page(1));
    assert!(cache.get(7).is_none());
    assert_eq!(cache.stat().len, 0);
    assert_eq!(cache.byte_len(), 0);
  }

  #[test]
  fn insert_get_remove_roundtrip() {
    let mut cache = WritePageCache::new(8 * PAGE);
    cache.insert(3, page(0xA));
    assert_eq!(cache.get(3), Some(page(0xA).as_slice()));
    assert_eq!(cache.stat().len, 1);
    assert_eq!(cache.byte_len(), PAGE);
    assert_eq!(cache.stat().hit, 1);
    cache.remove(3);
    assert!(cache.get(3).is_none());
    assert_eq!(cache.stat().len, 0);
    assert_eq!(cache.byte_len(), 0);
  }

  /// 预算硬上限：反复插入远超预算的页，常驻字节与条目数都不越过上限。
  #[test]
  fn budget_is_hard_cap() {
    let cap = 4;
    let mut cache = WritePageCache::new(cap * PAGE);
    for idx in 0..1000u64 {
      cache.insert(idx, page(idx as u8));
      assert!(
        cache.byte_len() <= cap * PAGE,
        "bytes 越界: {}",
        cache.byte_len()
      );
      assert!(cache.stat().len <= cap, "len 越界: {}", cache.stat().len);
    }
    assert_eq!(cache.byte_len(), cap * PAGE);
    assert!(cache.stat().evict >= 1000 - cap as u64);
  }

  /// 清空后不留任何条目或字节账。
  #[test]
  fn clear_resets_everything() {
    let mut cache = WritePageCache::new(4 * PAGE);
    for idx in 0..4u64 {
      cache.insert(idx, page(1));
    }
    cache.clear();
    assert_eq!(cache.byte_len(), 0);
    assert_eq!(cache.stat().len, 0);
    for idx in 0..4u64 {
      assert!(cache.get(idx).is_none());
    }
  }

  /// Pooled 复用链路（真实 mapper API：allocate → free → reclaim → allocate）：
  /// 被回收再分配的同一 `FilePageId` 绝不可能从缓存里读到陈旧字节。
  ///
  /// 这是写缓存唯一的危险点：键是物理页 id，而 Pooled 会把释放过的页复用给
  /// 新页。测试走「定位复用确实发生 → 验证装饰器读到新内容」的全链路，并先用
  /// 一次未失效的读做阳性对照（证明本用例真能检出陈旧）。
  #[test]
  fn pooled_reuse_after_free_cannot_serve_stale_bytes() {
    use fkv_btree::BatchEntry;
    use fkv_io_mem::MemStore;
    use fkv_mapping::AllocMode;
    use fkv_types::TableIdent;

    use crate::Store;

    pollster::block_on(async {
      let page_size = PAGE;
      let tbl = TableIdent::new("reuse", 1).unwrap();
      let mut store = Store::new(MemStore::new(page_size), page_size, 16)
        .with_alloc_mode(AllocMode::Pooled)
        .with_write_cache_bytes(16 * page_size);
      // 先写一批：建出表状态 / mapper / 写缓存。
      store
        .put(
          &tbl,
          &[BatchEntry::upsert(b"a".to_vec(), vec![b'v'; 60], 1)],
        )
        .await
        .unwrap();

      // 手工走一遍「上一批已写出页」：分配逻辑页并直写盘，再手工回填缓存。
      let (page_a, fp_a) = {
        let state = store.table_map.get_mut(&tbl).unwrap();
        let mapper = state.mapper.as_mut().unwrap();
        let page_a = mapper.allocate_page();
        let fp_a = mapper.file_page_id(page_a).unwrap();
        (page_a, fp_a)
      };
      let bytes_a = vec![0xAAu8; page_size];
      store.io.write_page(&tbl, fp_a, &bytes_a).await.unwrap();
      store
        .table_map
        .get(&tbl)
        .unwrap()
        .write_cache
        .as_ref()
        .unwrap()
        .borrow_mut()
        .insert(fp_a, bytes_a.clone());

      let cache = store
        .table_map
        .get(&tbl)
        .unwrap()
        .write_cache
        .as_ref()
        .unwrap();
      let io = CachedWriteIo::new(&store.io, cache, page_size);
      let mut buf = vec![0u8; page_size];
      io.read_page_owned(&tbl, fp_a, &mut buf).await.unwrap();
      assert_eq!(buf, bytes_a, "阳性对照：缓存里有条目时命中的就是它");

      // 释放该页 → 批次结算消费释放记录 → 回池 → 复用给新页。
      {
        let state = store.table_map.get_mut(&tbl).unwrap();
        let mapper = state.mapper.as_mut().unwrap();
        assert_eq!(mapper.free_page(page_a).unwrap(), Some(fp_a));
        assert_eq!(mapper.freed_file_page_li(), &[fp_a]);
      }
      store.invalidate_freed_write_cache(&tbl);
      assert!(
        store
          .table_map
          .get(&tbl)
          .unwrap()
          .write_cache
          .as_ref()
          .unwrap()
          .borrow()
          .get(fp_a)
          .is_none(),
        "释放的物理页必须已从缓存失效"
      );

      // 退役页回池后重新分配：确认确实是同一个物理页被复用。
      let page_b = {
        let state = store.table_map.get_mut(&tbl).unwrap();
        let mapper = state.mapper.as_mut().unwrap();
        mapper.reclaim_retired(u64::MAX);
        mapper.allocate_page()
      };
      let fp_b = {
        let state = store.table_map.get(&tbl).unwrap();
        state.mapper.as_ref().unwrap().file_page_id(page_b).unwrap()
      };
      assert_eq!(fp_b, fp_a, "Pooled 必须把释放过的物理页复用给新页");

      // 新页写出后读回：必须是新字节，而不是缓存里残留的旧字节。
      let bytes_b = vec![0xBBu8; page_size];
      store.io.write_page(&tbl, fp_b, &bytes_b).await.unwrap();
      let cache = store
        .table_map
        .get(&tbl)
        .unwrap()
        .write_cache
        .as_ref()
        .unwrap();
      let io = CachedWriteIo::new(&store.io, cache, page_size);
      let mut buf = vec![0u8; page_size];
      io.read_page_owned(&tbl, fp_b, &mut buf).await.unwrap();
      assert_eq!(buf, bytes_b, "复用页不得返回上一世的字节");
    });
  }

  /// 回滚撤销批次内的释放记录：`abort_batch` 后这些页重新被活跃映射引用，
  /// 不得被上层当作已释放页失效（否则会白丢仍然有效的缓存条目）。
  #[test]
  fn abort_batch_restores_freed_records() {
    use fkv_mapping::{AllocMode, PageMapper};

    let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
    let page_id = mapper.allocate_page();
    let fp = mapper.file_page_id(page_id).unwrap();
    assert!(mapper.freed_file_page_li().is_empty());
    assert!(mapper.begin_batch());
    assert_eq!(mapper.free_page(page_id).unwrap(), Some(fp));
    assert_eq!(mapper.freed_file_page_li(), &[fp]);
    assert!(mapper.abort_batch());
    assert!(
      mapper.freed_file_page_li().is_empty(),
      "回滚后的释放记录必须被撤销"
    );
  }

  /// `remap_page` 退役的旧物理页同样入释放表（COW 的每一次覆盖都会走到）。
  #[test]
  fn remap_page_records_retired_fp() {
    use fkv_mapping::{AllocMode, PageMapper};

    let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
    let page_id = mapper.allocate_page();
    let old_fp = mapper.file_page_id(page_id).unwrap();
    let new_fp = mapper.remap_page(page_id).unwrap();
    assert_ne!(new_fp, old_fp, "COW 必须换物理页");
    assert_eq!(mapper.freed_file_page_li(), &[old_fp]);
  }
}
