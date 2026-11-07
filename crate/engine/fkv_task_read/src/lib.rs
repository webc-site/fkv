//! 点读：root index → data page，最多 64 层。
//!
//! 传入 [`IndexRead`] 的页缓存时，index 层下降走内存 `IndexNode`（对齐 C++
//! 常驻 index page），命中后一次点读只剩最后 1 次 data page IO；同时给出
//! 首访在飞表时，未命中的首访按 `(tbl, page_id, generation)` 合并读盘。

use std::{cell::RefCell, future::poll_fn, mem::take, task::Poll, time::Instant};

use fkv_btree::{IndexCache, IndexNode, decompress_value};
use fkv_compress::DictCompression;
use fkv_error::{KvError, Result};
use fkv_io::PageIo;
use fkv_mapping::MappingSnapshot;
use fkv_page::{
  DataPage, DataPageIter, IndexPage, IndexPageIter, OverflowPage, PageType,
  overflow::decode_pointers, page_type,
};
use fkv_types::{FilePageId, MAX_TREE_DEPTH, NULL_PAGE, PageId, TableIdent, is_expired, slow_read};
use gxhash::HashSet;

use crate::{
  flight::{LoadGuard, LoadStep},
  page_buf::{PageBuf, acquire_buf, release_buf},
};

pub mod flight;
pub mod page_buf;

pub use flight::PageFlight;

/// 廉价 CPU 循环的让出检查步长：每 N 次迭代读一次时钟，摊薄 `Instant::now`。
pub const YIELD_CHECK_STRIDE: usize = 256;

/// 协作式让出预算。
///
/// 长循环只在段边界调用 [`Self::maybe_yield`]：自上次让出（或创建）以来挂钟
/// 超过 `budget_us` 时让出一次。`0` = 关闭，不读时钟、不改变任务调度。
///
/// 计时口径是挂钟而非 CPU 时钟：IO 等待也会计入，最多导致一次多余让出，
/// 不影响结果；换 CPU 时钟需要平台 syscall，代价大于收益。
#[derive(Debug, Clone, Copy)]
pub struct YieldBudget {
  budget_us: u64,
  start: Instant,
}

impl YieldBudget {
  pub fn new(budget_us: u64) -> Self {
    Self {
      budget_us,
      start: Instant::now(),
    }
  }

  pub fn budget_us(&self) -> u64 {
    self.budget_us
  }

  pub fn reset(&mut self) {
    self.start = Instant::now();
  }

  /// 超过段预算就让出一次；返回是否真的让出。
  pub async fn maybe_yield(&mut self) -> bool {
    if self.budget_us == 0 {
      return false;
    }
    if self.start.elapsed().as_micros() < u128::from(self.budget_us) {
      return false;
    }
    yield_now().await;
    // 让出期间的时间不计入下一段，避免恢复后立刻再次让出。
    self.reset();
    true
  }

  /// 每 [`YIELD_CHECK_STRIDE`] 次迭代检查一次，用于迭代体很轻的 CPU 循环。
  pub async fn maybe_yield_every(&mut self, seq: usize) -> bool {
    if !seq.is_multiple_of(YIELD_CHECK_STRIDE) {
      return false;
    }
    self.maybe_yield().await
  }
}

/// 让出一次：自唤醒后返回 `Pending`，把执行权交回调度器。
///
/// compio 0.19 没有公开的 `yield_now`；`sleep(0)` 只是新增一个 timer 让出点，
/// 不会把当前任务排到其它就绪任务之后。自唤醒把任务重新挂到执行器 ready
/// 队列尾部，与其它任务轮转；对 `pollster` 等单 future 执行器等价于立即重 poll。
pub async fn yield_now() {
  let mut yielded = false;
  poll_fn(move |cx| {
    if yielded {
      return Poll::Ready(());
    }
    yielded = true;
    cx.waker().wake_by_ref();
    Poll::Pending
  })
  .await;
}

/// 一批 overflow 页缓冲，Drop 时逐块归还线程本地池。
struct BufLi(Vec<Vec<u8>>);

impl BufLi {
  fn with_len(len: usize, page_size: usize) -> Self {
    let mut buf_li = Vec::with_capacity(len);
    buf_li.extend((0..len).map(|_| acquire_buf(page_size)));
    Self(buf_li)
  }
}

impl Drop for BufLi {
  fn drop(&mut self) {
    for buf in take(&mut self.0) {
      release_buf(buf);
    }
  }
}

/// TTL 下推判定：只依赖 entry 头，先于 overflow / 解压执行。
#[inline]
fn expired(ctx: ReadCtx<'_>, expire_ts: u64) -> bool {
  ctx
    .ttl_filter_now_ms
    .is_some_and(|now| is_expired(expire_ts, now))
}

/// 取当前 entry 的用户 value：overflow 链拼回后解压，否则页内直接解压。
async fn iter_value<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  snapshot: &MappingSnapshot,
  page_size: usize,
  iter: &DataPageIter<'_>,
  ctx: ReadCtx<'_>,
) -> Result<Vec<u8>> {
  if iter.is_overflow() {
    let overflow_start = slow_read::start();
    let raw = read_overflow_budget(
      io,
      tbl,
      snapshot,
      page_size,
      iter.value(),
      ctx.yield_budget_us,
    )
    .await?;
    slow_read::stop("overflow", overflow_start);
    let decode_start = slow_read::start();
    let value = decompress_value(&raw, iter.compression(), ctx.dict)?.into_owned();
    slow_read::stop("decompress", decode_start);
    Ok(value)
  } else {
    let decode_start = slow_read::start();
    let value = decompress_value(iter.value(), iter.compression(), ctx.dict)?.into_owned();
    slow_read::stop("decompress", decode_start);
    Ok(value)
  }
}

/// 沿 overflow 页链拼回用户 value：leaf entry 存首批页指针，每批末页尾部存
/// 下一批指针。对齐 C++ `GetOverflowValue`。
pub async fn read_overflow<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  snapshot: &MappingSnapshot,
  page_size: usize,
  encoded: &[u8],
) -> Result<Vec<u8>> {
  read_overflow_budget(io, tbl, snapshot, page_size, encoded, 0).await
}

/// 同 [`read_overflow`]，但每个 overflow 批次检查一次段预算并让出。
pub async fn read_overflow_budget<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  snapshot: &MappingSnapshot,
  page_size: usize,
  encoded: &[u8],
  yield_budget_us: u64,
) -> Result<Vec<u8>> {
  let mut yield_budget = YieldBudget::new(yield_budget_us);
  let mut value = Vec::new();
  let mut pointer_li = decode_pointers(encoded)?;
  // 损坏盘面可能让链成环或重复指向同一页：以逻辑页总数为访问上界，
  // 再用 visited 拒绝重复页，保证任意输入下有限步结束。
  let page_cnt = snapshot.data_tbl().len();
  let mut visited = HashSet::default();
  // 一批页缓冲来自线程本地池；`read_page` 整页覆写，故不预清零。
  while !pointer_li.is_empty() {
    // 每批 overflow 页是一次批量 IO + 拼接，属长段边界，检查段预算。
    yield_budget.maybe_yield().await;
    if visited.len() + pointer_li.len() > page_cnt
      || pointer_li.iter().any(|pointer| !visited.insert(*pointer))
    {
      return Err(KvError::Corrupted);
    }
    let file_page_id_li = pointer_li
      .iter()
      .map(|pointer| snapshot.data_tbl().file_page_id(*pointer))
      .collect::<Result<Vec<_>>>()?;
    let mut buf_li = BufLi::with_len(file_page_id_li.len(), page_size);
    io.read_page_li(tbl, &file_page_id_li, &mut buf_li.0)
      .await?;
    for buf in &buf_li.0 {
      value.extend_from_slice(OverflowPage::new(buf)?.value()?);
    }
    // 只有一批的末页带指向下一批的指针。
    pointer_li = OverflowPage::new(buf_li.0.last().ok_or(KvError::Corrupted)?)?.pointer_li()?;
  }
  Ok(value)
}

/// index 层读取的共享句柄：页缓存 + 首访在飞表。
///
/// 两者是同一个决策的两半（缓存命中后仍然要落到在飞表），因此收拢成一个
/// 类型、字段不公开：调用方只能经 [`Self::new`] / [`Self::none`] 显式表态，
/// 不会出现「要了缓存、漏写在飞表」这种编译得过但静默失去去重的组合
/// （l2-13）。字段都属 shard 线程独占状态，`None` 表示对应能力关闭。
#[derive(Clone, Copy)]
pub struct IndexRead<'a> {
  cache: Option<&'a RefCell<IndexCache>>,
  flight: Option<&'a RefCell<PageFlight>>,
}

impl<'a> IndexRead<'a> {
  /// 关闭页缓存与首访去重：每次 index 下降都读盘。
  ///
  /// 只用于「本就不读 index 层」或纯差分测试；生产读路径必须显式给出
  /// 缓存 / 去重（见 [`Self::new`]）。
  pub const fn none() -> Self {
    Self {
      cache: None,
      flight: None,
    }
  }

  /// 显式指定两项能力；缓存命中按快照的映射代际校验，只有缓存未命中的
  /// 首访才登记在飞表。
  pub const fn new(
    cache: Option<&'a RefCell<IndexCache>>,
    flight: Option<&'a RefCell<PageFlight>>,
  ) -> Self {
    Self { cache, flight }
  }
}

/// 读路径的共享上下文：快照 + 字典 + index 层能力 + 点读 TTL 过滤阈值。
///
/// 快照由请求入口取一次，整个点读 / floor 复用；index 缓存命中按快照的
/// 映射代际校验，旧快照不会读到新代节点。
#[derive(Clone, Copy)]
pub struct ReadCtx<'a> {
  pub snapshot: &'a MappingSnapshot,
  pub dict: Option<&'a DictCompression>,
  /// index 层的缓存与首访在飞去重（见 [`IndexRead`]）。
  ///
  /// 无默认值：构造 [`ReadCtx`] 必须显式选择，避免新增读路径用结构体更新
  /// 语法意外继承「关闭」而静默失去去重（l2-13）。
  pub index: IndexRead<'a>,
  /// `Some(now_ms)` 时在解码 entry 头后立刻判过期，命中即返回 `None`，跳过
  /// overflow 链 IO 与解压；`None` 时不过滤，由调用方决定过期语义。
  pub ttl_filter_now_ms: Option<u64>,
  /// overflow / prev 页链的协作让出预算（微秒；`0` = 关闭）。
  pub yield_budget_us: u64,
}

impl<'a> ReadCtx<'a> {
  /// 构造读上下文：index 层能力必须显式给出（`IndexRead::none()` = 关闭），
  /// 字典 / TTL / 让出预算取默认关闭。
  ///
  /// 刻意不提供「只传快照」的构造：那样新增读路径会把 `index` 留成默认关闭
  /// 且编译通过，并发首访退化成每人一次读盘，且无人会发现（l2-13）。
  pub fn new(snapshot: &'a MappingSnapshot, index: IndexRead<'a>) -> Self {
    Self {
      snapshot,
      dict: None,
      index,
      ttl_filter_now_ms: None,
      yield_budget_us: 0,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadEntry {
  pub value: Vec<u8>,
  pub timestamp: u64,
  pub expire_ts: u64,
  pub is_large_value: bool,
}

/// 下降一步的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Descend {
  /// 已落在 data page，页内容在调用方的 `page` 缓冲。
  Data,
  /// 子页 id（index 层）。
  Child(PageId),
}

/// 沿 index 下降一层：缓存命中直接 `seek`，未命中读盘、构造节点并回填。
///
/// 单次 `get`：命中只计一次 hit，避免先探测再在下降里二次查表。
/// 返回 `Data` 时 `page` 已装入 data 页；返回 `Child` 时 `page` 内容无意义。
/// 子页等于当前页视作环，直接报 `Corrupted`。
///
/// `page` 以 `Vec` 借入，未命中时走 [`PageIo::read_page_owned`]（本地后端
/// 内核直读，省一次整页拷贝与一次读池槽周转）。
///
/// 未命中且 `index` 里给了首访在飞表时走 single-flight（对齐 C++ `WaitingZone`）：
/// 同一 `(tbl, page_id, generation)` 只让 owner 读盘，其余等待并复用广播的整页
/// 字节；等待时长记入慢读探针的 `flight_wait` 段，合并路径的 IO 时间仍可归因。
/// 失败按同一错误语义传播；等待者与 owner 同代，不会拿到失效代际的页。
pub async fn descend_step<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  snapshot: &MappingSnapshot,
  index: IndexRead<'_>,
  page_id: PageId,
  key: &[u8],
  page: &mut Vec<u8>,
) -> Result<Descend> {
  let IndexRead { cache, flight } = index;
  let generation = snapshot.mapping_generation();
  if let Some(cache) = cache {
    let hit_start = slow_read::start();
    let cache_ref = cache.borrow();
    let node = cache_ref.get(page_id, generation);
    slow_read::stop("index_hit", hit_start);
    if let Some(node) = node {
      let seek_start = slow_read::start();
      let child = node.seek(key)?;
      slow_read::stop("index_seek", seek_start);
      return descend_child(child, page_id);
    }
  }
  let file_page_id = snapshot.data_tbl().file_page_id(page_id)?;
  // 首访登记：owner 读盘，等待者挂起等广播。`loaded` 表示页字节已由等待者
  // 从广播填好，此时不再读盘、也不回填缓存（owner 已回填）。
  let (guard, loaded) = match flight {
    Some(flight) => {
      let step = { flight.borrow_mut().enter(tbl, page_id, generation) };
      match step {
        LoadStep::Waiter { rx } => {
          // 等待者不读盘，把「等待在飞 owner」的时长单独成段：慢读报告里的
          // IO 时间仍可归因（owner 的 io 段留在 owner 自己的报告）。失败路径
          // 同样计等待时长——等待确实发生了，错误语义由下面的 `?` 传播。
          let wait_start = slow_read::start();
          let payload = rx.await.map_err(|_| KvError::TryAgain);
          slow_read::stop("flight_wait", wait_start);
          let payload = payload??;
          page.clear();
          page.extend_from_slice(payload.as_slice());
          (None, true)
        }
        LoadStep::Owner { ticket } => (
          Some(LoadGuard::new(flight, tbl, page_id, generation, ticket)),
          false,
        ),
      }
    }
    None => (None, false),
  };
  let io_us = if loaded {
    None
  } else {
    let io_start = slow_read::start();
    if let Err(err) = io.read_page_owned(tbl, file_page_id, page).await {
      // 读盘失败：同一错误广播给等待者。
      if let Some(guard) = guard {
        guard.finish_err(err);
      }
      return Err(err);
    }
    io_start.map(|start| start.elapsed())
  };
  let outcome = process_page(page, key, page_id, generation, file_page_id, cache, !loaded);
  match outcome {
    Ok(descend) => {
      if let Some(guard) = guard {
        guard.finish_ok(page);
      }
      if !loaded {
        match page_type(page) {
          Ok(PageType::Data) => slow_read::record("data_io", io_us.unwrap_or_default()),
          Ok(_) => slow_read::record("index_io", io_us.unwrap_or_default()),
          Err(_) => {}
        }
      }
      Ok(descend)
    }
    Err(err) => {
      if let Some(guard) = guard {
        guard.finish_err(err);
      }
      Err(err)
    }
  }
}

/// 处理已装入 `page` 的一页：data 页直接返回，index 页 seek 并按需回填缓存。
///
/// `cache_fill` 为 false（single-flight 等待者）时即使有缓存也不回填：owner
/// 已插入同代节点，重复插入只会重复记账并触发淘汰。
fn process_page(
  page: &[u8],
  key: &[u8],
  page_id: PageId,
  generation: u64,
  file_page_id: FilePageId,
  index_cache: Option<&RefCell<IndexCache>>,
  cache_fill: bool,
) -> Result<Descend> {
  match page_type(page)? {
    PageType::Data => Ok(Descend::Data),
    PageType::LeafIndex | PageType::NonLeafIndex => {
      let Some(cache) = (if cache_fill { index_cache } else { None }) else {
        // 无缓存 / 等待者：就地在页上 seek，不构造 `IndexNode`（省一次整页拷贝）。
        let mut iter = IndexPageIter::new(page)?;
        let child = if iter.seek_floor(key)? {
          iter.page_id()
        } else {
          IndexPage::new(page)?.leftmost_ptr()?
        };
        return descend_child(child, page_id);
      };
      // 同一份页只构造一次：seek 后 move 进缓存，避免重复校验与页拷贝。
      let node = IndexNode::new(file_page_id, page)?;
      let child = node.seek(key)?;
      let insert_start = slow_read::start();
      cache.borrow_mut().insert(page_id, node, generation);
      slow_read::stop("cache_insert", insert_start);
      descend_child(child, page_id)
    }
    PageType::Overflow | PageType::Deleted => Err(KvError::Corrupted),
  }
}

/// 环检测：子页不能指向自身。
#[inline]
fn descend_child(child: PageId, page_id: PageId) -> Result<Descend> {
  if child == page_id {
    return Err(KvError::Corrupted);
  }
  Ok(Descend::Child(child))
}

/// 点读 entry。root / 映射全部取自 `ctx.snapshot`；TTL 下推见
/// `ReadCtx::ttl_filter_now_ms`。
pub async fn read_entry<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  key: &[u8],
  page_size: usize,
  ctx: ReadCtx<'_>,
) -> Result<Option<ReadEntry>> {
  let snapshot = ctx.snapshot;
  let root_id = snapshot.root_id();
  if root_id == NULL_PAGE {
    return Ok(None);
  }
  let mut page_id = root_id;
  let mut page = PageBuf::new(page_size);
  for _ in 0..MAX_TREE_DEPTH {
    match descend_step(
      io,
      tbl,
      snapshot,
      ctx.index,
      page_id,
      key,
      page.as_mut_vec(),
    )
    .await?
    {
      Descend::Data => {
        let mut iter = DataPageIter::new(&page)?;
        if iter.seek(key)? && iter.key() == key {
          // 过期判定只依赖 entry 头，必须先于 overflow / 解压做掉，
          // 否则过期大 value 会白付整条 IO 链。
          if expired(ctx, iter.expire_ts()) {
            return Ok(None);
          }
          let value = iter_value(io, tbl, snapshot, page_size, &iter, ctx).await?;
          return Ok(Some(ReadEntry {
            value,
            timestamp: iter.timestamp(),
            expire_ts: iter.expire_ts(),
            is_large_value: iter.is_large_value(),
          }));
        }
        return Ok(None);
      }
      Descend::Child(child) => page_id = child,
    }
  }
  Err(KvError::Corrupted)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadFloorEntry {
  pub key: Vec<u8>,
  pub value: Vec<u8>,
  pub timestamp: u64,
  pub expire_ts: u64,
  pub is_large_value: bool,
}

/// 最大且 <= `key` 的未过期 entry；不存在返回 `None`。
///
/// TTL 语义与 scan 一致：过期项跳过，继续向更小 key 回退（本页内重扫，
/// 仍无候选则沿 prev 页链继续），因此不会因叶子末尾过期而漏掉存活区间。
pub async fn read_floor_entry<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  key: &[u8],
  page_size: usize,
  ctx: ReadCtx<'_>,
) -> Result<Option<ReadFloorEntry>> {
  let snapshot = ctx.snapshot;
  let root_id = snapshot.root_id();
  if root_id == NULL_PAGE {
    return Ok(None);
  }
  let mut page_id = root_id;
  let mut page = PageBuf::new(page_size);
  for _ in 0..MAX_TREE_DEPTH {
    match descend_step(
      io,
      tbl,
      snapshot,
      ctx.index,
      page_id,
      key,
      page.as_mut_vec(),
    )
    .await?
    {
      Descend::Data => break,
      Descend::Child(child) => page_id = child,
    }
  }
  // 超过最大树深仍未落到 data 页：树结构异常。
  if page_type(&page)? != PageType::Data {
    return Err(KvError::Corrupted);
  }
  // 删除后 data page 首 key 可能大于父 index 的分隔 key，key 落在其间时
  // 本页无 floor，需回退到前一页；本页候选全部过期时同样继续回退，直到
  // 找到存活项或链尾。单页树的 prev 可能默认指回自身，视为链尾；其余
  // 跨页成环以逻辑页槽位数为步数上界，报 `Corrupted`。
  let page_cnt = snapshot.data_tbl().len();
  let mut yield_budget = YieldBudget::new(ctx.yield_budget_us);
  let mut candidate = page_floor(io, tbl, snapshot, page_size, &page, key, ctx).await?;
  let mut prev_page_id = DataPage::new(&page)?.prev_page_id()?;
  let mut step = 0usize;
  while candidate.is_none() && prev_page_id != NULL_PAGE && prev_page_id != page_id {
    // 每个 prev 页是一次 IO；TTL 回退链可能跨越整个表，按页检查段预算。
    yield_budget.maybe_yield().await;
    step += 1;
    if step > page_cnt {
      return Err(KvError::Corrupted);
    }
    let file_page_id = snapshot.data_tbl().file_page_id(prev_page_id)?;
    io.read_page_owned(tbl, file_page_id, page.as_mut_vec())
      .await?;
    if page_type(&page)? != PageType::Data {
      return Err(KvError::Corrupted);
    }
    page_id = prev_page_id;
    candidate = page_floor(io, tbl, snapshot, page_size, &page, key, ctx).await?;
    prev_page_id = DataPage::new(&page)?.prev_page_id()?;
  }
  Ok(candidate)
}

/// 单页内最近一个 key `<= key` 的未过期 entry。
///
/// `seek_floor` 命中过期项时没有反向迭代可用，回退到页首单遍推进，保留最后
/// 一个未过期候选；越过 `key` 即停，无需扫完剩余页。
async fn page_floor<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  snapshot: &MappingSnapshot,
  page_size: usize,
  page: &[u8],
  key: &[u8],
  ctx: ReadCtx<'_>,
) -> Result<Option<ReadFloorEntry>> {
  let mut iter = DataPageIter::new(page)?;
  if !iter.seek_floor(key)? {
    return Ok(None);
  }
  if !expired(ctx, iter.expire_ts()) {
    return materialize(io, tbl, snapshot, page_size, &iter, ctx)
      .await
      .map(Some);
  }
  // 命中项过期：与 scan 语义一致，回退到更小的 key。
  iter.reset();
  let mut hit: Option<Vec<u8>> = None;
  while iter.advance()? && iter.key() <= key {
    if !expired(ctx, iter.expire_ts()) {
      hit = Some(iter.key().to_vec());
    }
  }
  let Some(hit) = hit else {
    return Ok(None);
  };
  // 重新定位到命中项再物化：扫描阶段不做 overflow IO / 解压。
  iter.reset();
  if !iter.seek_floor(&hit)? {
    return Err(KvError::Corrupted);
  }
  materialize(io, tbl, snapshot, page_size, &iter, ctx)
    .await
    .map(Some)
}

/// 物化当前 floor entry。
async fn materialize<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  snapshot: &MappingSnapshot,
  page_size: usize,
  iter: &DataPageIter<'_>,
  ctx: ReadCtx<'_>,
) -> Result<ReadFloorEntry> {
  Ok(ReadFloorEntry {
    key: iter.key().to_vec(),
    value: iter_value(io, tbl, snapshot, page_size, iter, ctx).await?,
    timestamp: iter.timestamp(),
    expire_ts: iter.expire_ts(),
    is_large_value: iter.is_large_value(),
  })
}
