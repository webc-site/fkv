//! 范围扫描：从 begin 下降到 data page，再沿 next 链遍历。
//!
//! 遍历权威仍是 data page 的 `next_page_id` 单向链表（链的一致性由写路径
//! 保证，成环会报 `Corrupted`）。但预取不再依赖「链页物理页号连续」这一
//! 猜测：对齐 C++ `ScanTask::PrefetchFromStack()`，本模块在 index 树上维护
//! 一个游标栈（[`LeafCursor`]），沿页内 `advance` 逐层推导接下来 K 个叶子页
//! 的**逻辑** PageId，再用 [`PageIo::read_page_li`] 一次批量提交——冷缓存下
//! 多请求在飞，每页不必等一个 4 KiB 读的往返。
//!
//! 预取只决定「一次读哪几页」，链仍是页序的权威：链的下一页若正好是索引栈
//! 给出的下一页，就直接消费请求内缓冲；一旦两者不一致（索引损坏 / 索引
//! 不完整 / 已到树尾），本请求后续全部退回逐页直读，绝不拿错页。页缓冲是
//! 请求内的，消费完即回收进线程本地池，没有任何跨请求缓存层。

use std::{
  borrow::Cow,
  collections::VecDeque,
  future::Future,
  mem::{swap, take},
  pin::{Pin, pin},
  sync::Arc,
  task::{Context, Poll, Waker},
};

use fkv_btree::decompress_value;
use fkv_compress::DictCompression;
use fkv_error::{KvError, Result};
use fkv_io::{PageIo, PageRef};
use fkv_mapping::MappingSnapshot;
use fkv_page::{DataPage, DataPageIter, IndexPage, IndexPageIter, PageType, page_type};
use fkv_task_read::{
  Descend, YieldBudget, descend_step,
  page_buf::{PAGE_BUF_POOL_MAX, PageBuf, acquire_buf_li, release_buf_li},
};
use fkv_types::{FilePageId, MAX_TREE_DEPTH, NULL_PAGE, PageId, TableIdent, is_expired};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanOption {
  pub begin_inclusive: bool,
  pub end_inclusive: bool,
  /// 对齐 ScanTask：命中超大 value 返回 `LargeValueUnsupported`。
  pub reject_large_value: bool,
  /// 只物化 key / `expire_ts`，跳过 overflow 链读取与解压，返回的 value 为空。
  ///
  /// 用于只按 key 定位的后台任务（如 `truncate_from`）。此时 `value` 不计入
  /// `PageOption::page_size` 配额（真实长度未知），调用方不应同时依赖字节分页。
  pub keys_only: bool,
}

impl Default for ScanOption {
  fn default() -> Self {
    Self {
      begin_inclusive: true,
      end_inclusive: false,
      reject_large_value: true,
      keys_only: false,
    }
  }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageOption {
  /// 0 表示不限制 entry 数。
  pub page_entries: usize,
  /// 0 表示不限制字节数。字节口径：key + value + 16（两个 u64）。
  pub page_size: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanPage {
  pub entry_li: Vec<ScannedEntry>,
  pub has_remaining: bool,
}

/// 扫描的树：主树或 TTL 树，两个 root 都由快照固定。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScanTree {
  #[default]
  Data,
  Ttl,
}

pub struct ScanArg<'a> {
  pub tbl: &'a TableIdent,
  /// 请求入口取一次的映射快照；root / 映射全部来自它。
  pub snapshot: &'a MappingSnapshot,
  pub tree: ScanTree,
  pub begin: &'a [u8],
  pub end: &'a [u8],
  pub page_size: usize,
  pub max_entries: usize,
  pub dict: Option<&'a DictCompression>,
  /// `Some(now_ms)` 时在叶子迭代器层过滤过期条目（不读 overflow、不占分页配额）。
  pub ttl_filter_now_ms: Option<u64>,
}

impl<'a> ScanArg<'a> {
  fn root_id(&self) -> PageId {
    match self.tree {
      ScanTree::Data => self.snapshot.root_id(),
      ScanTree::Ttl => self.snapshot.ttl_root_id(),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedEntry {
  pub key: Vec<u8>,
  pub value: Vec<u8>,
  /// entry 版本时间戳（对齐 C++ `KvEntry.timestamp_`）。
  pub timestamp: u64,
  pub expire_ts: u64,
}

/// 范围扫描，直接物化成 `(key, value)` 列表。
///
/// 走扫描核心的单趟映射，避免先收 `Vec<ScannedEntry>` 再二次转换成 tuple。
pub async fn scan_range<I: PageIo>(io: &I, args: ScanArg<'_>) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
  let out = scan_core(
    io,
    args,
    ScanOption::default(),
    PageOption::default(),
    0,
    false,
    |entry| (entry.key, entry.value),
  )
  .await?;
  Ok(out.item_li)
}

pub async fn scan_entries<I: PageIo>(io: &I, args: ScanArg<'_>) -> Result<Vec<ScannedEntry>> {
  scan_entries_with(io, args, ScanOption::default()).await
}

pub async fn scan_entries_with<I: PageIo>(
  io: &I,
  args: ScanArg<'_>,
  option: ScanOption,
) -> Result<Vec<ScannedEntry>> {
  scan_entries_with_budget(io, args, option, 0).await
}

/// 范围扫描计数：与 [`scan_entries`] 共用同一遍历、边界与过滤口径，但只累加
/// 命中数——不拷贝 key、不读 overflow 链、不解压 value。
///
/// 返回的存活 entry 数与 `scan_entries(...).len()` / `scan_range(...).len()`
/// 必然相等：`begin` / `end` 边界、`begin_inclusive` / `end_inclusive`、TTL
/// 过期过滤（`ttl_filter_now_ms`）与 large value 拒绝都在同一处判定。
/// 因此用一条叶子链遍历即可支撑 `len()` 计数，省掉全量 key/value 的物化、
/// 拷贝与 overflow / 解压 IO。
///
/// 计数路径不累加字节配额，故内部使用默认 [`PageOption`]（不限页）；
/// 调用方若需在含 large value 的表上计数，可传 `keys_only: true` 跳过
/// large value 拒绝（此时与 `reject_large_value: false` 的物化扫描同口径）。
pub async fn scan_count<I: PageIo>(io: &I, args: ScanArg<'_>, option: ScanOption) -> Result<usize> {
  scan_count_with_budget(io, args, option, 0).await
}

/// 同 [`scan_count`]，但叶子链页循环与 entry 循环带协作让出点。
pub async fn scan_count_with_budget<I: PageIo>(
  io: &I,
  args: ScanArg<'_>,
  option: ScanOption,
  yield_budget_us: u64,
) -> Result<usize> {
  Ok(
    scan_core(
      io,
      args,
      option,
      PageOption::default(),
      yield_budget_us,
      true,
      |_entry| (),
    )
    .await?
    .count,
  )
}

/// 同 [`scan_entries_with`]，但每条叶子页 / 每个 entry 批次检查段预算并让出。
pub async fn scan_entries_with_budget<I: PageIo>(
  io: &I,
  args: ScanArg<'_>,
  option: ScanOption,
  yield_budget_us: u64,
) -> Result<Vec<ScannedEntry>> {
  Ok(
    scan_page_with_budget(io, args, option, PageOption::default(), yield_budget_us)
      .await?
      .entry_li,
  )
}

pub async fn scan_page_with<I: PageIo>(
  io: &I,
  args: ScanArg<'_>,
  option: ScanOption,
  page: PageOption,
) -> Result<ScanPage> {
  scan_page_with_budget(io, args, option, page, 0).await
}

/// 同 [`scan_page_with`]，但叶子链页循环与 entry 循环带协作让出点。
pub async fn scan_page_with_budget<I: PageIo>(
  io: &I,
  args: ScanArg<'_>,
  option: ScanOption,
  page: PageOption,
  yield_budget_us: u64,
) -> Result<ScanPage> {
  let out = scan_core(io, args, option, page, yield_budget_us, false, |entry| {
    entry
  })
  .await?;
  Ok(ScanPage {
    entry_li: out.item_li,
    has_remaining: out.has_remaining,
  })
}

/// 扫描核心产出：物化条目列表（count-only 时为空）、命中计数与「是否有剩余」。
struct ScanCoreOut<T> {
  item_li: Vec<T>,
  count: usize,
  has_remaining: bool,
}

/// 扫描核心：每条命中的 entry 经 `map` 就地映射成 `T` 后收集。
///
/// 泛型映射让 [`scan_range`] 只保留一份结果容器（`Vec<(Vec<u8>, Vec<u8>)>`），
/// 不必先收 `Vec<ScannedEntry>` 再二次转换；[`scan_page_with_budget`] 传恒等映射。
///
/// `count_only` 为真时走计数路径：命中判定（边界 / TTL / large value 拒绝 /
/// entry 上限）与物化路径共用同一段代码，只跳过 key 拷贝、value 还原与结果入列，
/// 因此计数结果与物化路径严格一致。计数路径不累加字节配额，调用方须传
/// `page.page_size == 0`（[`scan_count_with_budget`] 固定用默认 [`PageOption`]）。
async fn scan_core<I: PageIo, T>(
  io: &I,
  args: ScanArg<'_>,
  option: ScanOption,
  page: PageOption,
  yield_budget_us: u64,
  count_only: bool,
  mut map: impl FnMut(ScannedEntry) -> T,
) -> Result<ScanCoreOut<T>> {
  let root_id = args.root_id();
  let ScanArg {
    tbl,
    snapshot,
    begin,
    end,
    page_size,
    max_entries,
    dict,
    ttl_filter_now_ms,
    ..
  } = args;
  if root_id == NULL_PAGE {
    return Ok(ScanCoreOut {
      item_li: Vec::new(),
      count: 0,
      has_remaining: false,
    });
  }
  // entry 上限统一为一个：`page_entries`（分页）与 `max_entries`（一次性扫描）
  // 取较小非零值。0 表示不限制（Rust 扩展；C++ 要求 > 0）。
  let entry_limit = match (page.page_entries, max_entries) {
    (0, 0) => 0,
    (0, limit) | (limit, 0) => limit,
    (paged, max) => paged.min(max),
  };
  let paged = page.page_entries != 0;
  let page_limit_applies = paged && (max_entries == 0 || page.page_entries <= max_entries);
  // 索引栈批量预取深度。只有扫到表尾（`end` 为空）才跨页预取：带边界的范围
  // 请求可能只覆盖一页，冷缓存下多读的都是白读（与旧 ChainWindow 的取舍
  // 一致）；分页 / 字节配额很小同样收敛深度。
  let prefetch_depth = prefetch_depth(page, entry_limit, end.is_empty(), page_size);

  // 首页缓冲：下降 / 首轮预取都直接读进它，命中页缓存时复制一次（与
  // `read_page_owned` 同量级），未命中时零额外拷贝。
  let mut page_buf = PageBuf::new(page_size);
  // 请求内的预取缓冲（消费完回收进线程本地池，不跨请求）。
  let mut supply = PageSupply::new(page_size);
  // 请求内的索引游标栈；`None` 表示本请求不预取，走逐页直读。
  let mut cursor: Option<LeafCursor> = None;
  let mut page_id = root_id;
  if prefetch_depth >= 2 {
    let mut leaf_cursor = LeafCursor::open(
      io,
      tbl,
      snapshot,
      root_id,
      begin,
      page_size,
      page_buf.as_mut_vec(),
    )
    .await?;
    match supply
      .refill_into_first(
        io,
        tbl,
        snapshot,
        &mut leaf_cursor,
        prefetch_depth,
        page_buf.as_mut_vec(),
      )
      .await?
    {
      Some(first_id) => page_id = first_id,
      // 索引栈里没有任何叶子：空表。
      None => {
        return Ok(ScanCoreOut {
          item_li: Vec::new(),
          count: 0,
          has_remaining: false,
        });
      }
    }
    cursor = Some(leaf_cursor);
  } else {
    // 不预取：逐页直读，先沿 index 下降到 `begin` 所在 data 页。
    // 页缓存命中时拿到的是共享句柄（借不到 `page_buf`），循环结束后再复制进来：
    // 循环体内借用结束才能再次可变借用同一块缓冲。
    let mut shared_page: Option<Arc<[u8]>> = None;
    for _ in 0..MAX_TREE_DEPTH {
      match descend_step(io, tbl, snapshot, page_id, begin, page_buf.as_mut_vec()).await? {
        Descend::Data(PageRef::Shared(page)) => {
          shared_page = Some(page);
          break;
        }
        Descend::Data(PageRef::Borrowed(_)) => break,
        Descend::Child(child) => page_id = child,
      }
    }
    if let Some(page) = shared_page {
      page_buf.as_mut_vec().clear();
      page_buf.as_mut_vec().extend_from_slice(&page);
    }
  }

  let mut result_li = Vec::new();
  // count-only 路径的命中计数；与 `result_li.len()` 二者按 `count_only` 互斥使用。
  let mut count = 0usize;
  let mut result_size = 0usize;
  let mut has_remaining = false;
  // 叶子链只应访问互异的逻辑页，快照槽位数即链长上界；超界说明链成环。
  let leaf_cnt_max = snapshot.data_tbl().len();
  let mut leaf_cnt = 0usize;
  // 只有首页可能含 `begin` 之前的 entry，需要 seek 区间；后续页沿叶子链
  // 首 entry 必然 > `begin`，直接从头迭代，省掉每页一次 restart 二分。
  let mut first_page = true;
  // 叶子链页循环与页内 entry 循环的协作让出预算（0 = 关闭）。
  let mut yield_budget = YieldBudget::new(yield_budget_us);
  loop {
    yield_budget.maybe_yield().await;
    let next_page = DataPage::new(&page_buf)?.next_page_id()?;
    leaf_cnt += 1;
    if leaf_cnt > leaf_cnt_max || next_page == page_id {
      return Err(KvError::Corrupted);
    }
    let next_file_page_id = if next_page == NULL_PAGE {
      None
    } else {
      Some(snapshot.data_tbl().file_page_id(next_page)?)
    };
    let mut refill_done: Option<Result<()>> = None;
    let mut stop = false;
    // 重填 future 固定在栈上（`pin!`），避免每页 `Box::pin` 堆分配与 dyn 虚调用。
    // 单独作用域：块结束即释放对 `supply` / `cursor` 的借用，之后才能取下一页。
    {
      // 叠在 entry 迭代上的预取：本批最后一页开始处理时，索引栈已指向下一批
      // 叶子，提交一次批量读。只在确实要下一页时提交，链尾不读。
      let mut refill = pin!(async {
        if let Some(cursor) = cursor.as_mut()
          && next_file_page_id.is_some()
          && supply.pending.is_empty()
          && !supply.lost
        {
          supply
            .refill(io, tbl, snapshot, cursor, prefetch_depth)
            .await?;
        }
        Ok(())
      });
      let mut iter = DataPageIter::new(&page_buf)?;
      let mut has = if first_page {
        let mut has = iter.seek(begin)?;
        if !option.begin_inclusive && has && iter.key() == begin {
          has = iter.advance()?;
        }
        has
      } else {
        iter.advance()?
      };
      // 单页 entry 很多（大页 + 小 key）时也不能一直占着线程：按固定步长
      // 读一次时钟，摊薄 `Instant::now` 开销。
      let mut entry_seq = 0usize;
      while has {
        yield_budget.maybe_yield_every(entry_seq).await;
        entry_seq += 1;
        poll_page_io(refill.as_mut(), &mut refill_done);
        let key = iter.key();
        let end_reached = !end.is_empty() && (key > end || (!option.end_inclusive && key == end));
        if end_reached {
          stop = true;
          break;
        }
        if let Some(now_ms) = ttl_filter_now_ms
          && is_expired(iter.expire_ts(), now_ms)
        {
          has = iter.advance()?;
          continue;
        }
        let emitted = if count_only { count } else { result_li.len() };
        if entry_limit != 0 && emitted == entry_limit {
          if page_limit_applies {
            has_remaining = true;
          }
          stop = true;
          break;
        }
        if option.reject_large_value && !option.keys_only && iter.is_large_value() {
          // 提前返回前必须收割在飞的重填，否则会取消 compio op 并借坏窗口缓冲。
          let _ = drain_prefetch(refill.as_mut(), refill_done.take()).await;
          return Err(KvError::LargeValueUnsupported);
        }
        // count-only：命中判定已全部通过，不物化 key/value，累加计数后前进。
        // 与物化路径共用同一 seek / 边界 / TTL 过滤 / 叶子链推进，计数必然一致。
        if count_only {
          count += 1;
          // 计数不物化 key：走只解析长度 / 过期时间的推进（`advance_skip_key`），
          // 省掉逐条 key 复制。只有 `end` 为空（全表计数 / len）时才能跳过——
          // 带边界的计数靠 `iter.key()` 判终止，跳过 key 会让边界判定失效。
          has = match if end.is_empty() {
            iter.advance_skip_key()
          } else {
            iter.advance()
          } {
            Ok(has) => has,
            Err(err) => {
              let _ = drain_prefetch(refill.as_mut(), refill_done.take()).await;
              return Err(err);
            }
          };
          continue;
        }
        // keys_only 只取 key：不读 overflow 链、不解压，物化后立即丢弃 value 的
        // 调用方（truncate）省掉这部分内存与 IO。
        let value = if option.keys_only {
          Vec::new()
        } else {
          // 先还原真实 value（解 overflow / 解压缩），再按真实长度判定分页边界，
          // 对齐 C++ `ScanTask`（ResolveValue 之后才累加 entry_size）。
          let value_res = if iter.is_overflow() {
            fkv_task_read::read_overflow_budget(
              io,
              tbl,
              snapshot,
              page_size,
              iter.value(),
              yield_budget_us,
            )
            .await
            .and_then(|raw| decompress_value(&raw, iter.compression(), dict).map(Cow::into_owned))
          } else {
            decompress_value(iter.value(), iter.compression(), dict).map(Cow::into_owned)
          };
          match value_res {
            Ok(value) => value,
            Err(err) => {
              let _ = drain_prefetch(refill.as_mut(), refill_done.take()).await;
              return Err(err);
            }
          }
        };
        let entry_size = key.len() + value.len() + 16;
        if page.page_size != 0
          && result_size > 0
          && result_size.saturating_add(entry_size) > page.page_size
        {
          has_remaining = true;
          stop = true;
          break;
        }
        result_li.push(map(ScannedEntry {
          key: key.to_vec(),
          value,
          timestamp: iter.timestamp(),
          expire_ts: iter.expire_ts(),
        }));
        result_size += entry_size;
        has = match iter.advance() {
          Ok(has) => has,
          Err(err) => {
            let _ = drain_prefetch(refill.as_mut(), refill_done.take()).await;
            return Err(err);
          }
        };
      }
      // 已判定没有下一页时重填是空操作，没有在飞 IO，直接丢弃。
      if next_page != NULL_PAGE {
        if stop {
          // 提前结束（end / 配额）时本页结果已完整：等重填落地只为不取消
          // compio op（写进了窗口缓冲），其错误与本请求无关，不传播。
          let _ = drain_prefetch(refill.as_mut(), refill_done.take()).await;
        } else {
          // 还要继续翻页：窗口就是下一页字节的来源，错误必须上抛。
          drain_prefetch(refill.as_mut(), refill_done.take()).await?;
        }
      }
    }
    if stop || next_page == NULL_PAGE {
      break;
    }
    first_page = false;
    // 取下一页字节：索引栈预取的下一页与链要求的页一致时直接消费请求内缓冲
    // （零 IO）；不一致说明索引与链不同步，本请求后续全部逐页直读，绝不拿错页。
    let next_file_page_id = next_file_page_id.ok_or(KvError::Corrupted)?;
    let taken = if cursor.is_some() && !supply.lost {
      match supply.pending.front() {
        Some((id, _)) if *id == next_page => {
          let (_, buf) = supply.pending.pop_front().expect("已确认队列非空");
          let mut old = buf;
          swap(page_buf.as_mut_vec(), &mut old);
          supply.recycle(old);
          true
        }
        _ => {
          supply.lost = true;
          supply.discard_pending();
          false
        }
      }
    } else {
      false
    };
    if !taken {
      io.read_page_owned(tbl, next_file_page_id, page_buf.as_mut_vec())
        .await?;
    }
    page_id = next_page;
  }
  Ok(ScanCoreOut {
    item_li: result_li,
    count,
    has_remaining,
  })
}

/// 索引栈批量预取深度（页），与 C++ `kDefaultScanPrefetchPageCount = 6` 对齐：
/// 一次 `ReadPages` 提交 6 个 4 KiB 请求，冷缓存下 io_uring 多请求在飞。
const SCAN_PREFETCH_PAGE_NUM: usize = 6;

/// 编译期钉住预取工作集能被整页缓冲池容纳（当前页 1 + 预取 K + 复用缓冲
/// ≤ `PAGE_BUF_POOL_MAX`），否则每批预取都要重新分配整页缓冲。
const _: () = {
  assert!(SCAN_PREFETCH_PAGE_NUM < PAGE_BUF_POOL_MAX);
};

/// 本请求的预取深度：`0` = 不预取（逐页直读）。
///
/// 只有扫到表尾（`end` 为空）才跨页预取；带边界的范围请求可能只覆盖一页，
/// 冷缓存下多读的页都是白读。分页 / 字节配额很小时按「本请求最多用几页」
/// 收敛深度（与旧 ChainWindow 的启发式一致），避免只返回几条的请求读满整批。
fn prefetch_depth(
  page: PageOption,
  entry_limit: usize,
  full_tail: bool,
  page_size: usize,
) -> usize {
  if !full_tail {
    return 0;
  }
  let mut depth = SCAN_PREFETCH_PAGE_NUM;
  if entry_limit != 0 {
    depth = depth.min((entry_limit / 8).max(1));
  }
  if page.page_size != 0 && page_size != 0 {
    depth = depth.min((page.page_size / page_size).max(1));
  }
  depth
}

/// 索引栈的一层：本页按序的全部子页，以及下一个待访问子页下标。
///
/// `child_li` 只存逻辑 `PageId`（不存键）：初始下降用 `begin` 定位后，后续
/// 前进纯粹按页内顺序推进，不需要再比较键。页内首个子页存在 header 的
/// leftmost 指针里，不在 entry 数组中（对齐 `IndexPageBuilder::add`）。
struct IndexFrame {
  /// 本 index 页自身的逻辑 id（防自环）。
  page_id: PageId,
  /// 页内 leftmost 子页。
  leftmost: PageId,
  /// 除 leftmost 外的子页，按页内索引顺序（即 key 升序）。
  child_li: Vec<PageId>,
  /// 下一个待访问子页下标，取值 `0..=child_li.len()`。
  pos: usize,
  /// 子页是否为 data 页（本页是 `LeafIndex`）。
  leaf_parent: bool,
}

impl IndexFrame {
  #[inline]
  fn child_cnt(&self) -> usize {
    self.child_li.len() + 1
  }

  #[inline]
  fn child_at(&self, index: usize) -> PageId {
    if index == 0 {
      self.leftmost
    } else {
      self.child_li[index - 1]
    }
  }
}

/// 索引栈游标：从 root 沿 `begin` 下降，之后按 key 顺序逐个产出叶子页。
///
/// 对齐 C++ `ScanIterator::AdvanceToNextLeaf()`：栈里只有 index 层；当前层
/// 子页耗尽就弹栈，由父层推进到下一个子页并重新下降。叶子页只产出逻辑
/// `PageId`，页字节由调用方批量读入请求内缓冲。
struct LeafCursor {
  stack: Vec<IndexFrame>,
  /// 已产出叶子数，防止损坏索引造成无限遍历。
  yielded: usize,
  /// 叶子数上界（快照槽位数），超界视为索引成环 / 损坏。
  max_leaves: usize,
  /// 仅在跨 index 页下降时使用的读页缓冲（懒分配，单层树不分配）。
  scratch: Option<PageBuf>,
  page_size: usize,
}

impl LeafCursor {
  /// 沿 `begin` 从 `root` 下降到所在叶子，并把游标定位在它上面。
  ///
  /// `buf` 借给 index 页读取（由调用方持有，之后复用作首页缓冲）。
  async fn open<I: PageIo>(
    io: &I,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
    root: PageId,
    begin: &[u8],
    page_size: usize,
    buf: &mut Vec<u8>,
  ) -> Result<Self> {
    let mut stack: Vec<IndexFrame> = Vec::new();
    let mut page_id = root;
    loop {
      if stack.len() >= MAX_TREE_DEPTH {
        return Err(KvError::Corrupted);
      }
      let file_page_id = snapshot.data_tbl().file_page_id(page_id)?;
      let page = io.read_page_ref(tbl, file_page_id, buf).await?;
      let ptype = page_type(&page)?;
      match ptype {
        // 单页树：root 本身就是 data 页。
        PageType::Data => {
          stack.push(IndexFrame {
            page_id,
            leftmost: page_id,
            child_li: Vec::new(),
            pos: 0,
            leaf_parent: true,
          });
          break;
        }
        PageType::LeafIndex | PageType::NonLeafIndex => {
          let leaf_parent = ptype == PageType::LeafIndex;
          let (leftmost, child_li, pos) = read_index_child_li(&page, begin)?;
          let child = if pos == 0 {
            leftmost
          } else {
            child_li[pos - 1]
          };
          if leaf_parent {
            // 游标停在 `pos`：下一个叶子就是 `begin` 所在的这一页。
            stack.push(IndexFrame {
              page_id,
              leftmost,
              child_li,
              pos,
              leaf_parent: true,
            });
            break;
          }
          // 已下降到子页 `pos`，父层的下一个子页是 `pos + 1`。
          stack.push(IndexFrame {
            page_id,
            leftmost,
            child_li,
            pos: pos + 1,
            leaf_parent: false,
          });
          if child == page_id {
            return Err(KvError::Corrupted);
          }
          page_id = child;
        }
        PageType::Overflow | PageType::Deleted => return Err(KvError::Corrupted),
      }
    }
    Ok(Self {
      stack,
      yielded: 0,
      max_leaves: snapshot.data_tbl().len().max(1),
      scratch: None,
      page_size,
    })
  }

  /// 产出按 key 顺序的下一张叶子页；栈耗尽返回 `None`。
  ///
  /// 叶子页只给逻辑 `PageId`，不读页字节；跨 index 页时读下一层索引页并把
  /// 游标停在它的 leftmost。
  async fn next_leaf<I: PageIo>(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
  ) -> Result<Option<PageId>> {
    loop {
      // 先取「当前层的下一个子页」快照，结束栈的不可变借用后才能推进游标。
      let step = match self.stack.last() {
        None => return Ok(None),
        Some(frame) if frame.pos >= frame.child_cnt() => None,
        Some(frame) => Some((frame.child_at(frame.pos), frame.leaf_parent, frame.page_id)),
      };
      let Some((child, leaf_parent, parent_id)) = step else {
        self.stack.pop();
        continue;
      };
      self.stack.last_mut().expect("上一步已确认栈非空").pos += 1;
      if leaf_parent {
        self.yielded += 1;
        if self.yielded > self.max_leaves {
          return Err(KvError::Corrupted);
        }
        return Ok(Some(child));
      }
      if child == parent_id || self.stack.len() >= MAX_TREE_DEPTH {
        return Err(KvError::Corrupted);
      }
      let file_page_id = snapshot.data_tbl().file_page_id(child)?;
      // 只在跨 index 页时借一块整页缓冲；单层树（所有帧都是 leaf parent）不分配。
      let Self {
        scratch, page_size, ..
      } = self;
      let (leaf_parent, leftmost, child_li) = {
        let buf = scratch
          .get_or_insert_with(|| PageBuf::new(*page_size))
          .as_mut_vec();
        let page = io.read_page_ref(tbl, file_page_id, buf).await?;
        let ptype = page_type(&page)?;
        match ptype {
          PageType::LeafIndex | PageType::NonLeafIndex => {
            let (leftmost, child_li, _) = read_index_child_li(&page, &[])?;
            (ptype == PageType::LeafIndex, leftmost, child_li)
          }
          PageType::Overflow | PageType::Deleted | PageType::Data => {
            return Err(KvError::Corrupted);
          }
        }
      };
      self.stack.push(IndexFrame {
        page_id: child,
        leftmost,
        child_li,
        pos: 0,
        leaf_parent,
      });
    }
  }
}

/// 读一页 index，按页内顺序取出全部子页，并返回 `begin` 的 floor 子页下标。
///
/// 返回值：`(leftmost, 除 leftmost 外的子页, pos)`。`pos` 是「键 <= begin 的
/// entry 数」，也就是 `begin` 应下降到的子页下标（0 = leftmost）。与
/// `IndexPageIter::seek_floor` + `leftmost_ptr` 的定位语义一致，但一次线性
/// 扫描同时拿到后续预取要用的全部子页。
fn read_index_child_li(page: &[u8], begin: &[u8]) -> Result<(PageId, Vec<PageId>, usize)> {
  let leftmost = IndexPage::new(page)?.leftmost_ptr()?;
  let mut child_li = Vec::new();
  let mut iter = IndexPageIter::new(page)?;
  let mut pos = 0usize;
  let mut counting = true;
  while iter.advance()? {
    if counting {
      if iter.key() <= begin {
        pos += 1;
      } else {
        counting = false;
      }
    }
    child_li.push(iter.page_id());
  }
  Ok((leftmost, child_li, pos))
}

/// 请求内的预取缓冲：已批量读入、按叶子顺序待消费的页，以及可复用的整页缓冲。
///
/// 所有缓冲在 `Drop` 时整批归还线程本地池；没有任何跨请求状态。
struct PageSupply {
  page_size: usize,
  /// `(逻辑 PageId, 整页字节)`，按 key 顺序。
  pending: VecDeque<(PageId, Vec<u8>)>,
  /// 已消费 / 未消费但被丢弃的整页缓冲，供下一次批量读复用。
  spare: Vec<Vec<u8>>,
  /// 索引游标已与叶子链不一致（或已耗尽）：停止预取，后续逐页直读。
  lost: bool,
}

impl PageSupply {
  fn new(page_size: usize) -> Self {
    Self {
      page_size,
      pending: VecDeque::new(),
      spare: Vec::new(),
      lost: false,
    }
  }

  /// 预取下一批（最多 `depth` 页）到 `pending`。
  async fn refill<I: PageIo>(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
    cursor: &mut LeafCursor,
    depth: usize,
  ) -> Result<()> {
    self
      .refill_impl(io, tbl, snapshot, cursor, depth, None)
      .await
      .map(|_| ())
  }

  /// 首批预取：首页读进调用方的 `first` 缓冲（复用首页缓冲，避免多一块整页
  /// 分配），其余页进入 `pending`。返回首页的逻辑 PageId。
  async fn refill_into_first<I: PageIo>(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
    cursor: &mut LeafCursor,
    depth: usize,
    first: &mut Vec<u8>,
  ) -> Result<Option<PageId>> {
    self
      .refill_impl(io, tbl, snapshot, cursor, depth, Some(first))
      .await
  }

  async fn refill_impl<I: PageIo>(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
    cursor: &mut LeafCursor,
    depth: usize,
    first: Option<&mut Vec<u8>>,
  ) -> Result<Option<PageId>> {
    if self.lost {
      return Ok(None);
    }
    let mut id_li: Vec<PageId> = Vec::with_capacity(depth);
    for _ in 0..depth {
      match cursor.next_leaf(io, tbl, snapshot).await? {
        Some(id) => id_li.push(id),
        None => break,
      }
    }
    if id_li.is_empty() {
      return Ok(None);
    }
    let mut fp_li: Vec<FilePageId> = Vec::with_capacity(id_li.len());
    for id in &id_li {
      fp_li.push(snapshot.data_tbl().file_page_id(*id)?);
    }

    let mut first = first;
    let has_first = first.is_some();
    let mut buf_li: Vec<Vec<u8>> = Vec::with_capacity(id_li.len());
    if let Some(first_buf) = first.as_deref_mut() {
      buf_li.push(take(first_buf));
    }
    let need = id_li.len() - buf_li.len();
    let reuse = need.min(self.spare.len());
    for _ in 0..reuse {
      buf_li.push(self.spare.pop().expect("spare 长度已核对"));
    }
    buf_li.extend(acquire_buf_li(need - reuse, self.page_size));

    // 索引栈给出的叶子页在同一段上通常物理连续（页分配按段顺序落盘），但
    // 每段之间夹着 index 页，一批 K 页可能跨段。按**最大连续段**切开，每段
    // 一次 `read_page_span`（单个 pread）；完全散乱（每段只有 1 页）时退回
    // [`PageIo::read_page_li`] 的多请求并发提交。两条路径的页内容与错误码
    // 都来自逐页读，语义一致。
    let mut run_li: Vec<(usize, usize)> = Vec::new();
    let mut run_start = 0usize;
    for index in 1..fp_li.len() {
      if fp_li[index] != fp_li[index - 1] + 1 {
        run_li.push((run_start, index));
        run_start = index;
      }
    }
    run_li.push((run_start, fp_li.len()));
    if std::env::var_os("FKV_SCAN_DEBUG").is_some() {
      eprintln!("batch: n={} runs={run_li:?} fp={fp_li:?}", fp_li.len());
    }
    let result = async {
      if run_li.len() == fp_li.len() {
        // 完全散乱：一次并发提交。
        return io.read_page_li(tbl, &fp_li, &mut buf_li).await;
      }
      for (start, end) in run_li {
        io.read_page_span(tbl, fp_li[start], &mut buf_li[start..end])
          .await?;
      }
      Ok(())
    }
    .await;
    if let Err(err) = result {
      // 失败时把缓冲全部收回，避免池永久少若干块。
      if has_first {
        **first.as_mut().expect("has_first 已确认") = buf_li.remove(0);
      }
      self.spare.extend(buf_li);
      return Err(err);
    }
    let first_id = if has_first {
      **first.as_mut().expect("has_first 已确认") = buf_li.remove(0);
      Some(id_li[0])
    } else {
      None
    };
    let skip = usize::from(has_first);
    for (id, buf) in id_li.into_iter().skip(skip).zip(buf_li) {
      self.pending.push_back((id, buf));
    }
    Ok(first_id)
  }

  fn recycle(&mut self, buf: Vec<u8>) {
    self.spare.push(buf);
  }

  /// 索引与链不一致：丢弃本批剩余页并停止预取。缓冲仍可复用，不做 IO。
  fn discard_pending(&mut self) {
    while let Some((_, buf)) = self.pending.pop_front() {
      self.spare.push(buf);
    }
  }
}

impl Drop for PageSupply {
  fn drop(&mut self) {
    if !self.pending.is_empty() {
      release_buf_li(self.pending.drain(..).map(|(_, buf)| buf).collect());
    }
    if !self.spare.is_empty() {
      release_buf_li(take(&mut self.spare));
    }
  }
}

/// 收割在飞的预取读：已完成则取结果，否则 await，避免取消 compio op。
async fn drain_prefetch<F: Future<Output = Result<()>>>(
  mut fut: Pin<&mut F>,
  done: Option<Result<()>>,
) -> Result<()> {
  match done {
    Some(result) => result,
    None => fut.as_mut().await,
  }
}

/// 手动 poll 一次 IO future（noop waker）：提前提交 compio op，
/// 后续仍用真实 `await` 等待完成。
fn poll_page_io<F: Future<Output = Result<()>>>(
  mut fut: Pin<&mut F>,
  done: &mut Option<Result<()>>,
) {
  if done.is_some() {
    return;
  }
  let mut cx = Context::from_waker(Waker::noop());
  if let Poll::Ready(result) = fut.as_mut().poll(&mut cx) {
    *done = Some(result);
  }
}
