//! 范围扫描：从 begin 下降到 data page，再沿 next 链遍历。
//!
//! 遍历来源契约：与 C++ `ScanTask`（沿 `index_stack_` 推导叶子顺序）不同，
//! 这里以 data page 的 `next_page_id` 单向链表为准，链的一致性由写路径保证。
//! 因此链只能预取 1 页，且需要环检测兜底（见 `scan_page_with`）。
//!
//! 叶子链的连续页窗口（[`ChainWindow`]）是纯投机：深度 > 1 时按连续物理页号
//! 批量读，深度为 1 时**整条窗口旁路**——不建窗口、不取页缓冲、不记
//! `taken`/`wasted`，下一页仍用窗口引入前的 `read_page_owned` 逐页直读。
//! 旁路路径的页内容、错误码、条目/TTL/配额语义与投机路径完全一致，只少了
//! 投机簿记，因此「不退化为投机」的负载（散乱布局的长整表扫描）不会为窗口
//! 机制付每页固定开销。

use std::{
  borrow::Cow,
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
use fkv_page::{DataPage, DataPageIter};
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
  let mut page_id = root_id;
  // 叶子链要沿 `next_page_id` 逐页搬动并复用同一块缓冲（窗口换入 / 单页直读
  // 都写回它），所以下降阶段直接把页读进这块池化缓冲：命中页缓存时复制一次
  // （每请求一次，与 `read_page_owned` 同量级），未命中时零额外拷贝。
  let mut page_buf = PageBuf::new(page_size);
  // 页缓存命中时拿到的是共享句柄（借不到 `page_buf`），循环结束后再复制进来：
  // 循环体内借用结束才能再次可变借用同一块缓冲。
  let mut shared_page: Option<Arc<[u8]>> = None;
  for _ in 0..MAX_TREE_DEPTH {
    match descend_step(io, tbl, snapshot, page_id, begin, page_buf.as_mut_vec()).await? {
      Descend::Data(PageRef::Shared(page)) => {
        shared_page = Some(page);
        break;
      }
      // 未命中：页已经在 `page_buf` 里，直接用。
      Descend::Data(PageRef::Borrowed(_)) => break,
      Descend::Child(child) => page_id = child,
    }
  }
  if let Some(page) = shared_page {
    page_buf.as_mut_vec().clear();
    page_buf.as_mut_vec().extend_from_slice(&page);
  }

  let mut result_li = Vec::new();
  // count-only 路径的命中计数；与 `result_li.len()` 二者按 `count_only` 互斥使用。
  let mut count = 0usize;
  let mut result_size = 0usize;
  let mut has_remaining = false;
  // entry 上限统一为一个：`page_entries`（分页）与 `max_entries`（一次性扫描）
  // 取较小非零值。0 表示不限制（Rust 扩展；C++ 要求 > 0）。
  let entry_limit = match (page.page_entries, max_entries) {
    (0, 0) => 0,
    (0, limit) | (limit, 0) => limit,
    (paged, max) => paged.min(max),
  };
  let paged = page.page_entries != 0;
  let page_limit_applies = paged && (max_entries == 0 || page.page_entries <= max_entries);
  // 叶子链块窗口：把「链上后续 K 页」一次批量读进来（见 [`ChainWindow`]）。
  //
  // 数据页的 next id 只能从当前页 header 得到，单看链只能预取 1 页（C++
  // `ScanTask` 预取 6 页是因为它沿 index 栈推导叶子顺序，见 eng-65）。这里走
  // 另一条路：读到当前页就知道链的下一**物理**页号，而实测同一段链在盘上
  // 连续（相邻页物理页号 +1，见 `read_page_span`），于是按连续页号投机读入
  // 一个窗口。命中的页零额外 IO，未命中（跨段跳转）退回单页直读——窗口只
  // 决定「一次读几页」，页内容与错误仍完全来自读盘，链始终是遍历的权威。
  //
  // 深度退化到 1 后整条窗口旁路，走窗口引入前的逐页直读路径。现实入口只有
  // 一条：refill 的退化（散乱布局白读多于有用读，或批量读失败）置 `depth = 1`，
  // 随后调用方丢弃（`window = None`，见块尾）；`ChainWindow::new` 构造公式的
  // 下界为 2，其 `depth <= 1` 返回 `None` 现状不可达，只是防御分支
  // （见 [`ChainWindow`] 文档）。
  // `end` 为空表示扫到表尾，链会一直延伸，投机预取才有收益。
  let mut window = ChainWindow::new(page_size, page, entry_limit, end.is_empty());
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
    // 单独作用域：块结束即释放对 `window` 的借用，之后才能 `take_into`。
    {
      // 叠在 entry 迭代上的预取：窗口没覆盖下一页时才真读（见 [`ChainWindow`]）。
      // 只在确实要下一页时提交，链尾（无下一页）不读；窗口已旁路（`None` 或
      // 深度退化到 1）时不投机，下一页由块尾逐页直读。
      let mut refill = pin!(async {
        if let (Some(file_page_id), Some(window)) = (next_file_page_id, window.as_mut())
          && !window.bypass()
          && !window.covers(file_page_id)
        {
          window.refill(io, tbl, file_page_id).await?;
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
          has = match iter.advance() {
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
    // 取下一页字节：窗口命中零 IO；未命中（跨段跳转）退回单页直读，复用当前页
    // 缓冲的分配。窗口旁路（深度 ≤ 1）时一律走这条单页直读，即窗口引入前的
    // 逐页路径，不碰缓冲池 / `taken` / `wasted`。
    let next_file_page_id = next_file_page_id.ok_or(KvError::Corrupted)?;
    let taken = match window.as_mut() {
      Some(window) if !window.bypass() => {
        window.take_into(next_file_page_id, page_buf.as_mut_vec())
      }
      _ => false,
    };
    if !taken {
      io.read_page_owned(tbl, next_file_page_id, page_buf.as_mut_vec())
        .await?;
    }
    // 本窗已判定不划算（自适应退化 / 批量读失败）：丢弃窗口，后续页逐页直读。
    if window.as_ref().is_some_and(ChainWindow::bypass) {
      window = None;
    }
    page_id = next_page;
  }
  Ok(ScanCoreOut {
    item_li: result_li,
    count,
    has_remaining,
  })
}

/// 叶子链块窗口：一次批量读入「链上后续若干页」，链走到窗口内的页时零 IO。
///
/// # 为什么可以按连续页号投机
///
/// 遍历权威仍是 data page 的 `next_page_id` 单向链表：读到当前页才知道下一页
/// 的逻辑页号。但**物理页号**在同一段链上是连续的（页分配按段顺序落盘，实测
/// 相邻链页物理页号 +1 占 99% 以上），因此「从链要求的下一页起连续读 K 页」
/// 是高命中率的投机：
///
/// - 命中：该页字节已在窗口里，零额外 IO；
/// - 未命中（链跨段跳转）：退回单页直读，语义不变；
/// - 白读：窗口里没被取走的页只是多读了盘，不影响结果。
///
/// 于是「每页一次页 IO」被压成「每窗一次页 IO」，后端还能把连续页合并成一次
/// `pread`（见 [`fkv_io::PageIo::read_page_span`]）。散乱布局下白读会超过有用
/// 读，此时把深度降到 1（等价于逐页预取），不会有额外的读放大。
///
/// # 缓冲
///
/// 槽位是线程本地池的整页缓冲；[`Self::take_into`] 把某页的缓冲与调用方当前
/// 页缓冲交换，并在 `taken` 位图里标记该页已取走（槽里换成交换进来的旧缓冲，
/// 下次重填会被整段读覆写）；`Drop` 时整批归还池。
struct ChainWindow {
  page_size: usize,
  /// 窗口首物理页号；`None` 表示窗口为空。
  base: Option<FilePageId>,
  /// 窗口内每页缓冲，下标 `i` 对应物理页号 `base + i`。
  buf_li: Vec<Vec<u8>>,
  /// 位图：下标 `i` 的页已被取走（该槽里是交换进来的旧缓冲，不再是窗口内容）。
  taken: u32,
  /// 投机深度（页）：首次由 [`Self::new`] 定，散乱布局下降为 1。
  depth: usize,
  /// 已被取走的页数（累计），用于判断投机是否划算。
  consumed: usize,
  /// 白读页数（窗口被换掉时仍未取走）累计。
  wasted: usize,
}

/// 块窗口默认深度（页）。
///
/// 4 KiB 页时深 16 = 64 KiB/请求常驻：一次页请求（256 条、约 15 页）正好一窗
/// 一次 IO；整表顺序遍历（`len()`）每 16 页一次 IO。再深则每次跨段跳转白读更多。
/// 深度上界必须 ≤ 32（`taken` 位图的位数）。
const CHAIN_WINDOW_DEPTH: usize = 16;
/// [`ChainWindow::taken`] 位图宽度。
const CHAIN_WINDOW_MAX: usize = 32;

/// 编译期钉住两条窗口不变量：
///
/// - 深度不超过 `taken` 位图宽度；
/// - 当前页 1 块 + 窗口 `depth` 块都能从整页缓冲池里取到（池上限与窗口工作集
///   必须一起调整），否则每个窗口请求都要重新分配整页缓冲。
const _: () = {
  assert!(CHAIN_WINDOW_DEPTH <= CHAIN_WINDOW_MAX);
  assert!(CHAIN_WINDOW_DEPTH < PAGE_BUF_POOL_MAX);
};

impl ChainWindow {
  /// `entry_limit` 是本请求最多能返回的 entry 数（0 = 不限），`page.page_size`
  /// 是字节配额（0 = 不限）：两者都给出「本请求最多可能用到多少页」的上界，
  /// 据此收敛深度，避免只返回几条的请求也读满整窗。
  ///
  /// - entry 上限：一页至少放 1 条，典型密度在两位数量级，按 `entry_limit / 8`
  ///   （下界 2 页）估计上界；
  /// - 字节配额：配额限的是结果字节，而一页能贡献的字节上限就是一页容量，
  ///   按 `quota / page_size` 估计上界。
  ///
  /// 两者都缺（计数 / 整表扫描）时用满 [`CHAIN_WINDOW_DEPTH`]。
  ///
  /// 深度 ≤ 1 等价于「不投机」：返回 `None`，调用方据此完全不建窗口、逐页直读
  /// （窗口引入前的旧路径），避免为零投机负载付每页一次 `read_page_span(1)`
  /// + 缓冲池租借 + `taken` 位图 + `take_into` 交换的固定开销。
  ///
  /// 两条 min 的下界都是 2，现状构造出的深度不会 ≤ 1——该分支是公式 / 常量
  /// 未来变化保留的防御分支，不是现实入口。
  fn new(page_size: usize, page: PageOption, entry_limit: usize, speculate: bool) -> Option<Self> {
    // 有 `end` 边界的范围扫描不投机：请求本身可能只覆盖一页（典型 10 条），
    // 读之前无从知道后面还有没有下一页，按 `page_entries` 推出来的 16 页窗口
    // 会整块读进来——冷缓存（数据 ≫ 缓存）下就是纯浪费，实测 range 读因此比
    // C++ 慢一倍。只有扫到表尾（`end` 为空，全表 / len）才值得预取。
    if !speculate {
      return None;
    }
    let mut depth = CHAIN_WINDOW_DEPTH;
    if entry_limit != 0 {
      depth = depth.min((entry_limit / 8).max(2));
    }
    if page.page_size != 0 {
      depth = depth.min((page.page_size / page_size).max(2));
    }
    if depth <= 1 {
      return None;
    }
    Some(Self {
      page_size,
      base: None,
      buf_li: Vec::new(),
      taken: 0,
      depth,
      consumed: 0,
      wasted: 0,
    })
  }

  /// 窗口已退化到「不投机」：调用方应停止使用窗口并丢弃它，后续页走逐页直读。
  fn bypass(&self) -> bool {
    self.depth <= 1
  }

  /// 窗口是否覆盖该物理页（该页字节现可从窗口取用，且尚未被取走）。
  fn covers(&self, file_page_id: FilePageId) -> bool {
    let Some(base) = self.base else {
      return false;
    };
    let Some(offset) = file_page_id.checked_sub(base) else {
      return false;
    };
    match usize::try_from(offset) {
      Ok(offset) => offset < self.buf_li.len() && self.taken & (1u32 << offset) == 0,
      Err(_) => false,
    }
  }

  /// 取走该页的整页字节（与 `dst` 交换缓冲）；未覆盖返回 `false`。
  ///
  /// 槽里留下 `dst` 的旧缓冲（不再被读，下次重填会被整段覆盖），只是标记为
  /// 已取走，不再算窗口内容。
  fn take_into(&mut self, file_page_id: FilePageId, dst: &mut Vec<u8>) -> bool {
    if !self.covers(file_page_id) {
      return false;
    }
    let offset = file_page_id - self.base.expect("covers 已确认 base 存在");
    let index = usize::try_from(offset).expect("covers 已确认下标在界内");
    swap(&mut self.buf_li[index], dst);
    self.taken |= 1u32 << index;
    true
  }

  /// 以 `start` 为首页重填窗口：一次 [`fkv_io::PageIo::read_page_span`] 批量读入
  /// 最多 `depth` 个连续物理页。
  ///
  /// 退化（上一窗白读多于有用读，或批量读失败）时**不再自己读首页**：只把
  /// `depth` 置 1、缓冲归还池并返回 `Ok(())`，由调用方丢弃窗口后用
  /// `read_page_owned` 逐页直读（与窗口引入前的路径一致）。`read_page_span`
  /// 单页读与 `read_page_owned` 同源，所以首页的字节与错误码不变。
  async fn refill<I: PageIo>(&mut self, io: &I, tbl: &TableIdent, start: FilePageId) -> Result<()> {
    // 上一窗没被取走的页就是白读：白读多于有用读时降到深度 1（逐页预取）。
    let used = self.taken.count_ones() as usize;
    self.wasted += self.buf_li.len() - used;
    self.consumed += used;
    if self.wasted > self.consumed {
      self.depth = 1;
      release_buf_li(take(&mut self.buf_li));
      self.base = None;
      return Ok(());
    }
    let depth = self.depth;
    debug_assert!(depth > 1, "深度 1 的窗口不重填，调用方直接逐页直读");
    // 复用上一窗的缓冲：非空槽直接用，空槽（上一窗已取走）与新增槽整批从池里取。
    let mut old_li = take(&mut self.buf_li);
    if old_li.len() > depth {
      release_buf_li(old_li.split_off(depth));
    }
    let fresh_num = depth - old_li.iter().filter(|buf| !buf.is_empty()).count();
    let mut fresh_li = acquire_buf_li(fresh_num, self.page_size);
    let mut buf_li: Vec<Vec<u8>> = Vec::with_capacity(depth);
    for mut buf in old_li {
      if buf.is_empty() {
        buf = fresh_li.pop().expect("空槽数已按池取数对齐");
      }
      buf_li.push(buf);
    }
    buf_li.extend(fresh_li);
    self.base = Some(start);
    self.buf_li = buf_li;
    // 新窗全部槽位都刚被整页读覆写，重新计入可命中。
    self.taken = 0;
    match io.read_page_span(tbl, start, &mut self.buf_li).await {
      Ok(()) => Ok(()),
      // 整段里有读不到的页（典型是段尾）：退化到逐页直读。多取的缓冲必须归还
      // 池，否则这次失败会让池永久少若干块，之后每个请求都要重新分配整页缓冲
      // （perf-10 的零分配口径）。
      Err(_) => {
        self.depth = 1;
        release_buf_li(take(&mut self.buf_li));
        self.base = None;
        Ok(())
      }
    }
  }
}

impl Drop for ChainWindow {
  fn drop(&mut self) {
    release_buf_li(take(&mut self.buf_li));
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
