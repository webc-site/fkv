//! 范围扫描：从 begin 下降到 data page，再沿 next 链遍历。
//!
//! 遍历来源契约：与 C++ `ScanTask`（沿 `index_stack_` 推导叶子顺序）不同，
//! 这里以 data page 的 `next_page_id` 单向链表为准，链的一致性由写路径保证。
//! 因此链只能预取 1 页，且需要环检测兜底（见 `scan_page_with`）。

use std::{
  borrow::Cow,
  cell::RefCell,
  future::Future,
  mem::swap,
  pin::{Pin, pin},
  task::{Context, Poll, Waker},
};

use fkv_btree::{IndexCache, decompress_value};
use fkv_compress::DictCompression;
use fkv_error::{KvError, Result};
use fkv_io::PageIo;
use fkv_mapping::MappingSnapshot;
use fkv_page::{DataPage, DataPageIter};
use fkv_task_read::{Descend, YieldBudget, descend_step};
use fkv_types::{MAX_TREE_DEPTH, NULL_PAGE, PageId, TableIdent, is_expired};

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
  pub index_cache: Option<&'a RefCell<IndexCache>>,
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
  pub expire_ts: u64,
}

pub async fn scan_range<I: PageIo>(io: &I, args: ScanArg<'_>) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
  Ok(
    scan_entries(io, args)
      .await?
      .into_iter()
      .map(|entry| (entry.key, entry.value))
      .collect(),
  )
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
  let root_id = args.root_id();
  let ScanArg {
    tbl,
    snapshot,
    begin,
    end,
    page_size,
    max_entries,
    dict,
    index_cache,
    ttl_filter_now_ms,
    ..
  } = args;
  if root_id == NULL_PAGE {
    return Ok(ScanPage::default());
  }
  let mut page_id = root_id;
  let mut page_buf = vec![0u8; page_size];
  for _ in 0..MAX_TREE_DEPTH {
    match descend_step(
      io,
      tbl,
      snapshot,
      index_cache,
      page_id,
      begin,
      &mut page_buf,
    )
    .await?
    {
      Descend::Data => break,
      Descend::Child(child) => page_id = child,
    }
  }

  let mut result_li = Vec::new();
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
  // 双缓冲：读当前页时提前提交下一页 IO，把 IO 与当前页解码重叠。
  // 数据页是单向链表，next id 只能从当前页 header 得到，故预取深度上界为 1 页；
  // C++ `ScanTask` 预取 6 页是因为它沿 index 栈推导叶子顺序（见 eng-65）。
  let mut spare = vec![0u8; page_size];
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
    let mut prefetch_done: Option<Result<()>> = None;
    let mut stop = false;
    // 预取 future 固定在栈上（`pin!`），避免每页 `Box::pin` 堆分配与 dyn 虚调用。
    // 单独作用域：块结束即释放对 `spare` 的借用，之后才能 `swap`。
    {
      let mut prefetch = pin!(async {
        match next_file_page_id {
          Some(file_page_id) => io.read_page(tbl, file_page_id, &mut spare).await,
          None => Ok(()),
        }
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
        poll_page_io(prefetch.as_mut(), &mut prefetch_done);
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
        if entry_limit != 0 && result_li.len() == entry_limit {
          if page_limit_applies {
            has_remaining = true;
          }
          stop = true;
          break;
        }
        if option.reject_large_value && !option.keys_only && iter.is_large_value() {
          // 提前返回前必须收割在飞的 prefetch，否则会取消 compio op 并借坏 spare。
          let _ = drain_prefetch(prefetch.as_mut(), prefetch_done.take()).await;
          return Err(KvError::LargeValueUnsupported);
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
              let _ = drain_prefetch(prefetch.as_mut(), prefetch_done.take()).await;
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
        result_li.push(ScannedEntry {
          key: key.to_vec(),
          value,
          expire_ts: iter.expire_ts(),
        });
        result_size += entry_size;
        has = match iter.advance() {
          Ok(has) => has,
          Err(err) => {
            let _ = drain_prefetch(prefetch.as_mut(), prefetch_done.take()).await;
            return Err(err);
          }
        };
      }
      // 已判定没有下一页时预取是空操作，没有在飞 IO，直接丢弃。
      if next_page != NULL_PAGE {
        if stop {
          // 提前结束（end / 配额）时本页结果已完整：等预取落地只为不取消
          // compio op（借用了 `spare`），其错误与本请求无关，不传播。
          let _ = drain_prefetch(prefetch.as_mut(), prefetch_done.take()).await;
        } else {
          // 还要继续翻页：预取结果就是下一页的前置条件，错误必须上抛。
          drain_prefetch(prefetch.as_mut(), prefetch_done.take()).await?;
        }
      }
    }
    if stop || next_page == NULL_PAGE {
      break;
    }
    first_page = false;
    swap(&mut page_buf, &mut spare);
    page_id = next_page;
  }
  Ok(ScanPage {
    entry_li: result_li,
    has_remaining,
  })
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
