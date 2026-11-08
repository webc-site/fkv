//! 读写共用页缓存：`FilePageId -> 整页字节`，写路径回填、读路径命中。
//!
//! 背景（perf 归因）：fkv 原先只有写路径页缓存。COW 写批每批都要把上一批刚写
//! 出的 root → 内部 index → leaf index → data 路径重读一遍，写缓存把这段固定
//! 页读降到 0；但读路径（点读 / 范围扫描 / 全表计数）每一页仍是 O_DIRECT 真读
//! 盘。C++ EloqStore 只有一个 128 MiB buffer pool，索引页 / 数据页在读路径反
//! 复命中内存。本模块把写缓存推广成**读写共用**的单实例页缓存：写路径刚写出
//! 的页，紧接着的读请求直接命中（写后读、批量写后扫描是收益最大的两条链）。
//!
//! 结构：`PageCache` 是缓存本体；`CachedWriteIo` / `CachedReadIo` 是 [`PageIo`]
//! 装饰器，共用同一个 `RefCell<PageCache>` 实例：
//!
//! - 写路径（[`CachedWriteIo`]）：读先探缓存、命中即免盘读；写成功后回填。
//! - 读路径（[`CachedReadIo`]）：全部 `read_page*` 先探缓存，未命中读盘后回填；
//!   写类方法直通内层（读装饰器不得改变写语义）。
//!
//! 命中 / 未命中按读写两侧分别累计（`CacheSide`，仅用于内部计数），对外的
//! `page_cache_*` 指标是两者之和；占用与逐出是同一个池的公共账目。
//!
//! ## 键的选择：物理 `FilePageId`
//!
//! 缓存键是物理页 id，而不是逻辑 `PageId`，理由：
//!
//! - 装饰器位于 [`PageIo`] 边界，`read_page*` / `write_page*` 只暴露物理页 id；
//!   逻辑 id 与物理 id 的换算由 `PageMapper::file_page_id` 在调用点完成，装饰器
//!   拿不到 mapper（写路径同时可变借用它）。装饰器天然是 per-table 的（缓存句柄
//!   来自目标表的 `TableState`），`FilePageId` 在该表文件空间内唯一。
//! - 写路径的每次读都在调用点用**当前**映射解出物理页，因此 COW 后的页读到的是
//!   新物理页——正好是上一批写出的那个；键与「写出的落点」天然一致。
//! - 读路径用快照映射解出物理页：COW 之下旧物理页的内容在该页退役前不变，
//!   因此快照读命中的字节就是快照可见的字节。
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
//!   必须调用 [`Store::invalidate_page_cache`] 告知，与
//!   [`Store::invalidate_path_cache`] 同一契约。
//!
//! 读路径的失败（EOF / 校验失败 / 不存在的页）一律不回填：只有 `Ok` 的整页才
//! 进缓存，避免把「没有字节」当成一个可命中的页。
//!
//! 缓存只影响**读到的字节来源**，不改变写出的字节、写顺序、fsync 顺序与
//! manifest 形态：开启与关闭缓存产出逐字节相同的盘面（见 `tests/page_cache.rs`）。
//!
//! 缓存只覆盖 `PageIo` 的 data 页 / overflow 页（两者都由 data mapper 分配、
//! 退役时进 `freed_file_page_li`）。大 value 的 segment 读走 `SegmentIo`，不在
//! 本缓存内：segment 文件按槽位顺序写、单次读即用，缓存收益低且需要另一套
//! 退役对账，因此明确不缓存（读到的字节仍直连后端）。
//!
//! ## 实测（macOS，rows=200000、batch-size=1、128B 值，`fkv_bench`）
//!
//! 写缓存把写路径磁盘页读从 14.7 次 / put 降到 0（`io_read_at_num` 2,932,554 → 0，
//! 命中率 1.00）。读写共用后点读 / 范围扫描的页读同样命中，见 bench
//! `page_cache_hit_ratio` 与 `io_read_at_num`。

use core::{cell::Cell, fmt, mem::take};
use std::{cell::RefCell, collections::VecDeque, sync::Arc};

use fkv_error::{KvError, Result};
use fkv_io::{PageIo, PageRef};
use fkv_types::{FilePageId, TableIdent};
use gxhash::{HashMap, HashSet};

/// 页缓存统计快照（与 bench 指标汇总口径同形，便于直接累加）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageCacheStat {
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

/// 探缓存的一侧：命中 / 未命中分别记在写路径与读路径的账上。
///
/// 占用（`bytes` / `len` / `capacity`）与逐出（`evict`）是同一个池的公共账目，
/// 两侧上报同一份值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CacheSide {
  Read,
  Write,
}

#[derive(Debug)]
struct CacheEntry {
  /// 整页字节，按 `Arc` 共享：读路径命中时零拷贝拿到句柄（对齐 C++ buffer
  /// pool 返回页句柄，而不是把页拷进调用方缓冲）。
  page: Arc<[u8]>,
  /// Clock 引用位：命中过的页在逐出时获得一次保留机会。
  referenced: Cell<bool>,
}

/// 读写共用页缓存：`FilePageId -> 整页字节`，按近似字节数做 Clock 淘汰。
///
/// `capacity == 0` 表示**显式关闭**（差分测试的对照组）：`insert` 直接丢弃，
/// `get` 恒 miss，不再产生任何分配与记录。
pub(crate) struct PageCache {
  page_map: HashMap<FilePageId, CacheEntry>,
  /// FIFO 顺序（惰性清理，允许残留已被 `remove` 的陈旧项）。
  order_li: VecDeque<FilePageId>,
  bytes: usize,
  capacity: usize,
  /// 写路径装饰器的命中 / 未命中。
  write_hit: Cell<u64>,
  write_miss: Cell<u64>,
  /// 读路径装饰器的命中 / 未命中。
  read_hit: Cell<u64>,
  read_miss: Cell<u64>,
  /// 两侧共用的逐出计数。
  evict: Cell<u64>,
}

impl fmt::Debug for PageCache {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // gxhash 的 `HashMap` 不实现 `Debug`；只打账目，不打页字节。
    f.debug_struct("PageCache")
      .field("len", &self.page_map.len())
      .field("bytes", &self.bytes)
      .field("capacity", &self.capacity)
      .field("write_hit", &self.write_hit.get())
      .field("write_miss", &self.write_miss.get())
      .field("read_hit", &self.read_hit.get())
      .field("read_miss", &self.read_miss.get())
      .field("evict", &self.evict.get())
      .finish()
  }
}

impl PageCache {
  pub(crate) fn new(capacity: usize) -> Self {
    Self {
      page_map: HashMap::default(),
      order_li: VecDeque::new(),
      bytes: 0,
      capacity,
      write_hit: Cell::new(0),
      write_miss: Cell::new(0),
      read_hit: Cell::new(0),
      read_miss: Cell::new(0),
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

  fn hit_counter(&self, side: CacheSide) -> &Cell<u64> {
    match side {
      CacheSide::Read => &self.read_hit,
      CacheSide::Write => &self.write_hit,
    }
  }

  fn miss_counter(&self, side: CacheSide) -> &Cell<u64> {
    match side {
      CacheSide::Read => &self.read_miss,
      CacheSide::Write => &self.write_miss,
    }
  }

  /// 探测一页；命中返回整页字节并置引用位、记一次 hit，未命中记一次 miss。
  pub(crate) fn get(&self, file_page_id: FilePageId, side: CacheSide) -> Option<Arc<[u8]>> {
    if !self.enabled() {
      return None;
    }
    let Some(entry) = self.page_map.get(&file_page_id) else {
      let miss = self.miss_counter(side);
      miss.set(miss.get() + 1);
      return None;
    };
    entry.referenced.set(true);
    let hit = self.hit_counter(side);
    hit.set(hit.get() + 1);
    Some(Arc::clone(&entry.page))
  }

  /// 回填 / 覆盖一页；容量不足时按 Clock 逐出到预算内。
  pub(crate) fn insert(&mut self, file_page_id: FilePageId, page: Arc<[u8]>) {
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

  /// 指定一侧的统计快照；占用与逐出是公共账目。
  /// 整个池的统计：读写共用一份预算，命中 / 未命中按两路调用点合计上报
  /// （对齐 C++ `PageManager` 单池口径，不再分侧出指标）。
  pub(crate) fn stat(&self) -> PageCacheStat {
    PageCacheStat {
      hit: self.hit_counter(CacheSide::Read).get() + self.hit_counter(CacheSide::Write).get(),
      miss: self.miss_counter(CacheSide::Read).get() + self.miss_counter(CacheSide::Write).get(),
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
  /// 强制淘汰，保证硬上限（不会被全命中卡住）。
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

/// 缓存装饰器本体：读先探缓存、未命中回填；写成功后按角色回填或失效。
///
/// 读写两个公开装饰器只是角色不同的薄壳（[`CachedWriteIo`] / [`CachedReadIo`]），
/// `PageIo` 实现完全共用一份，避免两套读缓存逻辑漂移。
struct CachedIo<'a, I> {
  inner: &'a I,
  /// `None` = 该表没有缓存实例（预算为 `0` 或表未打开），退化为直通。
  cache: Option<&'a RefCell<PageCache>>,
  page_size: usize,
  side: CacheSide,
  /// 写成功后是否把写出的字节回填进缓存。写路径 `true`；读路径 `false`
  /// （读装饰器不参与写，若真被写到也只做保守失效，见 [`Self::write_page`]）。
  refill_on_write: bool,
}

impl<'a, I> CachedIo<'a, I> {
  fn new(
    inner: &'a I,
    cache: Option<&'a RefCell<PageCache>>,
    page_size: usize,
    side: CacheSide,
    refill_on_write: bool,
  ) -> Self {
    Self {
      inner,
      cache,
      page_size,
      side,
      refill_on_write,
    }
  }

  /// 目标缓冲不足整页时按底层契约报 `InvalidArgs`（避免把短页缓存进来）。
  fn check_dst(&self, len: usize) -> Result<()> {
    if len < self.page_size {
      return Err(KvError::InvalidArgs);
    }
    Ok(())
  }

  /// 命中缓存时把整页拷进 `dst`；返回是否命中。缓存条目恒为整页。
  fn fill_from_cache(&self, file_page_id: FilePageId, dst: &mut [u8]) -> bool {
    let Some(cache) = self.cache else {
      return false;
    };
    let cache = cache.borrow();
    let Some(page) = cache.get(file_page_id, self.side) else {
      return false;
    };
    // 命中路径下 `dst` 长度已校验 ≥ page_size；`min` 只防御条目异常变短。
    let len = page.len().min(dst.len());
    dst[..len].copy_from_slice(&page[..len]);
    true
  }

  /// 读盘成功后回填（只取整页前缀，忽略调用方缓冲的尾部余量）。
  ///
  /// 调用点只在 `Ok` 之后调用本方法：EOF / 校验失败 / 不存在的页不会进缓存。
  fn refill(&self, file_page_id: FilePageId, buf: &[u8]) {
    let Some(cache) = self.cache else {
      return;
    };
    let len = buf.len().min(self.page_size);
    cache
      .borrow_mut()
      .insert(file_page_id, Arc::from(&buf[..len]));
  }

  /// 写成功后回填写出的整页（写路径）。
  fn insert_page(&self, file_page_id: FilePageId, page: &[u8]) {
    let Some(cache) = self.cache else {
      return;
    };
    let len = page.len().min(self.page_size);
    cache
      .borrow_mut()
      .insert(file_page_id, Arc::from(&page[..len]));
  }

  /// 读装饰器被写时的保守失效：盘上字节已变，旧条目不能再命中。
  fn invalidate_page(&self, file_page_id: FilePageId) {
    let Some(cache) = self.cache else {
      return;
    };
    cache.borrow_mut().remove(file_page_id);
  }
}

/// `PageIo` 装饰器：写路径用。读先探缓存，写成功后回填，与读路径共用缓存实例。
pub(crate) struct CachedWriteIo<'a, I>(CachedIo<'a, I>);

impl<'a, I: PageIo> CachedWriteIo<'a, I> {
  pub(crate) fn new(inner: &'a I, cache: &'a RefCell<PageCache>, page_size: usize) -> Self {
    Self(CachedIo::new(
      inner,
      Some(cache),
      page_size,
      CacheSide::Write,
      true,
    ))
  }
}

/// `PageIo` 装饰器：读路径用（只读）。所有 `read_page*` 先探缓存、未命中回填；
/// 写类方法直通内层，不改变写语义。
///
/// `cache == None`（预算为 `0` 或表未打开）时完全直通，与未引入本缓存时逐字节
/// 一致，供差分测试做对照组。
pub(crate) struct CachedReadIo<'a, I>(CachedIo<'a, I>);

impl<'a, I: PageIo> CachedReadIo<'a, I> {
  pub(crate) fn new(inner: &'a I, cache: Option<&'a RefCell<PageCache>>, page_size: usize) -> Self {
    Self(CachedIo::new(
      inner,
      cache,
      page_size,
      CacheSide::Read,
      false,
    ))
  }
}

macro_rules! delegate_page_io {
  ($ty:ident) => {
    impl<I: PageIo> PageIo for $ty<'_, I> {
      fn set_file_identity(&self, branch: &str, term: u64) {
        self.0.set_file_identity(branch, term);
      }

      fn set_file_range_li(&self, tbl: &TableIdent, range_li: &[fkv_types::BranchFileRange]) {
        self.0.set_file_range_li(tbl, range_li);
      }

      fn preload_path_li(&self, tbl: &TableIdent) -> usize {
        self.0.preload_path_li(tbl)
      }

      fn invalidate_path_cache(&self) {
        self.0.invalidate_path_cache();
      }

      fn set_verify_checksum(&self, enabled: bool) {
        self.0.set_verify_checksum(enabled);
      }

      fn evict_table(&self, tbl: &TableIdent) {
        self.0.evict_table(tbl);
      }

      async fn read_page(
        &self,
        tbl: &TableIdent,
        file_page_id: FilePageId,
        dst: &mut [u8],
      ) -> Result<()> {
        self.0.read_page(tbl, file_page_id, dst).await
      }

      async fn read_page_owned(
        &self,
        tbl: &TableIdent,
        file_page_id: FilePageId,
        buf: &mut Vec<u8>,
      ) -> Result<()> {
        self.0.read_page_owned(tbl, file_page_id, buf).await
      }

      async fn read_page_ref<'a>(
        &'a self,
        tbl: &'a TableIdent,
        file_page_id: FilePageId,
        buf: &'a mut Vec<u8>,
      ) -> Result<PageRef<'a>> {
        self.0.read_page_ref(tbl, file_page_id, buf).await
      }

      async fn read_page_li(
        &self,
        tbl: &TableIdent,
        file_page_id_li: &[FilePageId],
        page_buf_li: &mut [Vec<u8>],
      ) -> Result<()> {
        self.0.read_page_li(tbl, file_page_id_li, page_buf_li).await
      }

      async fn read_page_span(
        &self,
        tbl: &TableIdent,
        file_page_id: FilePageId,
        page_buf_li: &mut [Vec<u8>],
      ) -> Result<()> {
        self.0.read_page_span(tbl, file_page_id, page_buf_li).await
      }

      async fn write_page(
        &self,
        tbl: &TableIdent,
        file_page_id: FilePageId,
        src: &[u8],
      ) -> Result<()> {
        self.0.write_page(tbl, file_page_id, src).await
      }

      async fn write_page_li(
        &self,
        tbl: &TableIdent,
        page_li: &[(FilePageId, Vec<u8>)],
      ) -> Result<()> {
        self.0.write_page_li(tbl, page_li).await
      }

      async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
        self.0.sync_data(tbl).await
      }
    }
  };
}

delegate_page_io!(CachedWriteIo);
delegate_page_io!(CachedReadIo);

impl<I: PageIo> PageIo for CachedIo<'_, I> {
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

  /// 命中缓存时直接返回共享页句柄（零拷贝）；未命中读进调用方 `buf` 并回填缓存。
  async fn read_page_ref<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_page_id: FilePageId,
    buf: &'a mut Vec<u8>,
  ) -> Result<PageRef<'a>> {
    self.check_dst(buf.len())?;
    if let Some(cache) = self.cache
      && let Some(page) = cache.borrow().get(file_page_id, self.side)
    {
      return Ok(PageRef::Shared(page));
    }
    self.inner.read_page_owned(tbl, file_page_id, buf).await?;
    if let Some(cache) = self.cache {
      let len = buf.len().min(self.page_size);
      cache
        .borrow_mut()
        .insert(file_page_id, Arc::from(&buf[..len]));
    }
    Ok(PageRef::Borrowed(buf.as_slice()))
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
      // 失败批次不区分哪些页已经成功落进缓冲：整批不回填，错误照原样上抛。
      if result.is_ok() {
        self.refill(file_page_id, &buf);
      }
      page_buf_li[*idx] = buf;
    }
    result
  }

  async fn read_page_span(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    page_buf_li: &mut [Vec<u8>],
  ) -> Result<()> {
    // 先逐页探缓存（每页只记一次 hit / miss），命中页直接拷入调用方缓冲。
    let mut miss_idx_li: Vec<usize> = Vec::new();
    for (idx, buf) in page_buf_li.iter_mut().enumerate() {
      self.check_dst(buf.len())?;
      if !self.fill_from_cache(file_page_id + idx as u64, buf) {
        miss_idx_li.push(idx);
      }
    }
    // 未命中的**连续段**各发一次合并读（保留 `read_page_span` 的整段 pread
    // 语义）；全 miss 时等价于原来的一次整段读。
    let mut cursor = 0;
    while cursor < miss_idx_li.len() {
      let start = miss_idx_li[cursor];
      let mut end = cursor + 1;
      while end < miss_idx_li.len() && miss_idx_li[end] == miss_idx_li[end - 1] + 1 {
        end += 1;
      }
      let run_len = end - cursor;
      let start_fp = file_page_id + start as u64;
      let run = &mut page_buf_li[start..start + run_len];
      let result = self.inner.read_page_span(tbl, start_fp, run).await;
      if result.is_ok() {
        for (offset, buf) in run.iter().enumerate() {
          self.refill(start_fp + offset as u64, buf);
        }
      }
      result?;
      cursor = end;
    }
    Ok(())
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    self.inner.write_page(tbl, file_page_id, src).await?;
    // 写成功后才回填：缓存的语义是「盘上已有这份字节」。
    if self.refill_on_write {
      self.insert_page(file_page_id, src);
    } else {
      // 读装饰器不参与写；万一被写到，也必须让旧条目失效，保持硬不变量。
      self.invalidate_page(file_page_id);
    }
    Ok(())
  }

  async fn write_page_li(&self, tbl: &TableIdent, page_li: &[(FilePageId, Vec<u8>)]) -> Result<()> {
    // 必须整批转发：内层会把连续页合并成一次写（perf 关键路径）。
    self.inner.write_page_li(tbl, page_li).await?;
    for (file_page_id, page) in page_li {
      if self.refill_on_write {
        self.insert_page(*file_page_id, page);
      } else {
        self.invalidate_page(*file_page_id);
      }
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

  fn page(fill: u8) -> Arc<[u8]> {
    Arc::from(vec![fill; PAGE])
  }

  #[test]
  fn disabled_cache_never_serves_or_stores() {
    let mut cache = PageCache::new(0);
    assert!(!cache.enabled());
    cache.insert(7, page(1));
    assert!(cache.get(7, CacheSide::Read).is_none());
    assert!(cache.get(7, CacheSide::Write).is_none());
    let stat = cache.stat();
    assert_eq!(stat.len, 0);
    assert_eq!(stat.hit, 0);
    assert_eq!(stat.miss, 0, "关闭时不应记录未命中");
    assert_eq!(cache.byte_len(), 0);
  }

  #[test]
  fn insert_get_remove_roundtrip() {
    let mut cache = PageCache::new(8 * PAGE);
    cache.insert(3, page(0xA));
    assert_eq!(cache.get(3, CacheSide::Write), Some(page(0xA)));
    assert_eq!(cache.stat().len, 1);
    assert_eq!(cache.byte_len(), PAGE);
    assert_eq!(cache.stat().hit, 1);
    assert_eq!(cache.stat().miss, 0);
    cache.remove(3);
    assert!(cache.get(3, CacheSide::Write).is_none());
    assert_eq!(cache.stat().len, 0);
    assert_eq!(cache.byte_len(), 0);
  }

  /// 读写两侧的命中 / 未命中合并记账，占用与逐出是公共账目。
  #[test]
  fn read_and_write_hits_are_counted_together() {
    let mut cache = PageCache::new(8 * PAGE);
    cache.insert(1, page(1));
    assert_eq!(cache.get(1, CacheSide::Write), Some(page(1)));
    assert!(cache.get(2, CacheSide::Read).is_none());
    assert_eq!(cache.get(1, CacheSide::Read), Some(page(1)));
    let stat = cache.stat();
    assert_eq!((stat.hit, stat.miss), (2, 1));
    assert_eq!(stat.len, 1);
    assert_eq!(stat.bytes, PAGE);
    assert_eq!(stat.capacity, 8 * PAGE);
  }

  /// 预算硬上限：反复插入远超预算的页，常驻字节与条目数都不越过上限。
  #[test]
  fn budget_is_hard_cap() {
    let cap = 4;
    let mut cache = PageCache::new(cap * PAGE);
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
    let mut cache = PageCache::new(4 * PAGE);
    for idx in 0..4u64 {
      cache.insert(idx, page(1));
    }
    cache.clear();
    assert_eq!(cache.byte_len(), 0);
    assert_eq!(cache.stat().len, 0);
    for idx in 0..4u64 {
      assert!(cache.get(idx, CacheSide::Read).is_none());
    }
  }

  /// Pooled 复用链路（真实 mapper API：allocate → free → reclaim → allocate）：
  /// 被回收再分配的同一 `FilePageId` 绝不可能从缓存里读到陈旧字节。
  ///
  /// 这是页缓存唯一的危险点：键是物理页 id，而 Pooled 会把释放过的页复用给
  /// 新页。测试走「定位复用确实发生 → 验证读写两侧装饰器都读到新内容」的
  /// 全链路，并先用一次未失效的读做阳性对照（证明本用例真能检出陈旧）。
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
        .with_page_cache_bytes(16 * page_size);
      // 先写一批：建出表状态 / mapper / 页缓存。
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
        .page_cache
        .as_ref()
        .unwrap()
        .borrow_mut()
        .insert(fp_a, Arc::from(bytes_a.clone()));

      let cache = store
        .table_map
        .get(&tbl)
        .unwrap()
        .page_cache
        .as_ref()
        .unwrap();
      let write_io = CachedWriteIo::new(&store.io, cache, page_size);
      let mut buf = vec![0u8; page_size];
      write_io
        .read_page_owned(&tbl, fp_a, &mut buf)
        .await
        .unwrap();
      assert_eq!(buf, bytes_a, "阳性对照：缓存里有条目时命中的就是它");
      let read_io = CachedReadIo::new(&store.io, Some(cache), page_size);
      buf.fill(0);
      read_io.read_page_owned(&tbl, fp_a, &mut buf).await.unwrap();
      assert_eq!(buf, bytes_a, "读路径同样命中同一实例");

      // 释放该页 → 批次结算消费释放记录 → 回池 → 复用给新页。
      {
        let state = store.table_map.get_mut(&tbl).unwrap();
        let mapper = state.mapper.as_mut().unwrap();
        assert_eq!(mapper.free_page(page_a).unwrap(), Some(fp_a));
        assert_eq!(mapper.freed_file_page_li(), &[fp_a]);
      }
      store.invalidate_freed_page_cache(&tbl);
      assert!(
        store
          .table_map
          .get(&tbl)
          .unwrap()
          .page_cache
          .as_ref()
          .unwrap()
          .borrow()
          .get(fp_a, CacheSide::Read)
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
        .page_cache
        .as_ref()
        .unwrap();
      let write_io = CachedWriteIo::new(&store.io, cache, page_size);
      let mut buf = vec![0u8; page_size];
      write_io
        .read_page_owned(&tbl, fp_b, &mut buf)
        .await
        .unwrap();
      assert_eq!(buf, bytes_b, "复用页不得返回上一世的字节");
      let read_io = CachedReadIo::new(&store.io, Some(cache), page_size);
      buf.fill(0);
      read_io.read_page_owned(&tbl, fp_b, &mut buf).await.unwrap();
      assert_eq!(buf, bytes_b, "读路径也不得返回上一世的字节");
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
