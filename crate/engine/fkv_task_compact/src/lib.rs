//! Compaction 执行：把指定文件里的 live page 搬到新物理页。
//!
//! 逻辑 PageId 不变，只换 `FilePageId`；搬空后由 `fkv_task_gc` 删除文件。
//! 搬页按批聚合，对齐 C++ `DoCompactDataFile` 的 `max_read_pages_batch`：
//! 一批先批量读，再统一 remap + 批量写，避免逐页 IO 往返。
//!
//! 文件内搬移序按**旧 fp 升序**（`(fp, page)` 对排序，对齐 C++
//! `HeapSortFpIdsWithYield` + `FilePageLess`，background_write.cpp:66-98）：
//! 删除把逻辑页压回空闲链表头（LIFO），后续写入复用该 page id 却拿到更高的
//! fp，同文件内「逻辑页序 ≠ fp 序」可达；此状态下 append 分配器按搬移序发
//! 新 fp，页序与 fp 序任选其一作序会分叉出不同的 mapping 槽值与 record 顺序
//! （sem-60c）。
//!
//! segment 文件同构：`compact_segment_file_li` 搬运的是整段（`segment_size`），
//! 逻辑 segment id 不变，搬空后的旧 segment 文件由 `fkv_task_gc` 回收。

use core::{ops::AsyncFnMut, time::Duration};

use fkv_error::{KvError, Result};
use fkv_io::{PageIo, SegmentIo};
use fkv_mapping::PageMapper;
use fkv_task_gc::live_page_li_by_file_budget;
use fkv_task_read::YieldBudget;
use fkv_types::{FileId, FilePageId, MAX_READ_PAGES_BATCH, MAX_SEGMENTS_BATCH, PageId, TableIdent};
use futures::future::try_join_all;
use futures_timer::Delay;

// 单批上限唯一来源是 `fkv_types`（对齐 C++ `KvOption::max_read_pages_batch` /
// `max_segments_batch`）；u16 → usize 投影在各使用点完成。

/// 单次搬页读 / 写遇到可重试错误时最多尝试的次数（含首次）。
///
/// 云后端读段会先在共享组内抢安装独占权：同组另一实例正在写同一段时返回
/// `TryAgain`。这里不重试的话，段批量里一个 `TryAgain` 会让 `try_join_all`
/// 整体 abort，压缩在首次冷读时系统性失败。
const IO_RETRY_TIMES: u32 = 8;
const IO_RETRY_INIT_DELAY_MS: u64 = 1;
const IO_RETRY_MAX_DELAY_MS: u64 = 50;

/// 有界退避重试游标：读 / 写循环共用，避免每处各抄一遍计数与退避。
struct Retry {
  attempt: u32,
  delay_ms: u64,
}

impl Retry {
  const fn new() -> Self {
    Self {
      attempt: 1,
      delay_ms: IO_RETRY_INIT_DELAY_MS,
    }
  }

  /// 还有重试预算且错误可重试时睡完退避返回 `true`；否则 `false`。
  async fn wait(&mut self, err: KvError) -> bool {
    if !err.is_retryable() || self.attempt >= IO_RETRY_TIMES {
      return false;
    }
    Delay::new(Duration::from_millis(self.delay_ms)).await;
    self.delay_ms = (self.delay_ms * 2).min(IO_RETRY_MAX_DELAY_MS);
    self.attempt += 1;
    true
  }

  /// 反复执行 `f` 直到成功 / 遇不可重试错误 / 预算耗尽：读 / 写包装共用
  /// 这一循环骨架，谓词与退避全部收在 [`Self::wait`]。`f` 为 async 闭包，
  /// 每次调用产生的 future 允许借用闭包捕获的 `&mut` IO 参数。
  async fn run<T, F>(&mut self, mut f: F) -> Result<T>
  where
    F: AsyncFnMut() -> Result<T>,
  {
    loop {
      match f().await {
        Ok(v) => return Ok(v),
        Err(err) => {
          if !self.wait(err).await {
            return Err(err);
          }
        }
      }
    }
  }
}

async fn read_page_li_retry<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  file_page_id_li: &[FilePageId],
  page_buf_li: &mut [Vec<u8>],
) -> Result<()> {
  Retry::new()
    .run(async || io.read_page_li(tbl, file_page_id_li, page_buf_li).await)
    .await
}

async fn write_page_li_retry<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  write_li: &[(FilePageId, Vec<u8>)],
) -> Result<()> {
  Retry::new()
    .run(async || io.write_page_li(tbl, write_li).await)
    .await
}

async fn read_segment_retry<I: SegmentIo>(
  io: &I,
  tbl: &TableIdent,
  segment_fp_id: FilePageId,
  buf: &mut [u8],
) -> Result<()> {
  Retry::new()
    .run(async || io.read_segment(tbl, segment_fp_id, buf).await)
    .await
}

async fn write_segment_retry<I: SegmentIo>(
  io: &I,
  tbl: &TableIdent,
  segment_fp_id: FilePageId,
  buf: &[u8],
) -> Result<()> {
  Retry::new()
    .run(async || io.write_segment(tbl, segment_fp_id, buf).await)
    .await
}

/// 一次搬多个文件：只扫一遍 mapping 分组 live 页，再逐文件批式搬。
///
/// 入口自带 mapper 批次：直接调用时中途 IO 失败会回滚已完成的 remap，
/// 不会留下「一半页已搬、manifest 未写」的中间态。外层已开批次（store
/// 为延后 manifest 提交而开）时沿用外层日志，由外层结算。
pub async fn compact_file_li<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  file_li: &[FileId],
  page_size: usize,
  pages_per_file_shift: u8,
) -> Result<usize> {
  compact_file_li_budget(io, tbl, mapper, file_li, page_size, pages_per_file_shift, 0).await
}

/// 同 [`compact_file_li`]，但为两个 CPU 段（mapping 扫描 / 逐文件循环）接入
/// [`YieldBudget`]（l2-09）。
///
/// 让出点只落在**文件边界**与**纯读的映射扫描**上：扫描期间 `page_map` 只在
/// 本函数内增量构建；逐文件循环的让出发生在「上一个文件整批搬完、下一个文件
/// 尚未开始」处，不会插进半页搬移的中间态。`0` 时不读时钟、行为与
/// [`compact_file_li`] 完全一致。
pub async fn compact_file_li_budget<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  file_li: &[FileId],
  page_size: usize,
  pages_per_file_shift: u8,
  yield_budget_us: u64,
) -> Result<usize> {
  let own = mapper.begin_batch();
  let result = compact_file_li_inner(
    io,
    tbl,
    mapper,
    file_li,
    page_size,
    pages_per_file_shift,
    yield_budget_us,
  )
  .await;
  settle_batch(mapper, own, result.is_ok());
  result
}

async fn compact_file_li_inner<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  file_li: &[FileId],
  page_size: usize,
  pages_per_file_shift: u8,
  yield_budget_us: u64,
) -> Result<usize> {
  let mut page_map =
    live_page_li_by_file_budget(mapper, file_li, pages_per_file_shift, yield_budget_us).await;
  let mut yield_budget = YieldBudget::new(yield_budget_us);
  let mut moved = 0;
  for file_id in file_li {
    let Some(mut pair_li) = page_map.remove(file_id) else {
      continue;
    };
    // 文件内搬移序对齐 C++ `FilePageLess`：`(旧 fp, page)` 对升序，tuple
    // 字典序与 C++ 堆排比较子逐条等价（fp 全局唯一，page 仅作并列键）。
    pair_li.sort_unstable();
    for chunk in pair_li.chunks(usize::from(MAX_READ_PAGES_BATCH)) {
      moved += compact_page_batch(io, tbl, mapper, chunk, page_size).await?;
    }
    // 文件边界让出：本文件整批已搬完并记入 mapper 批次日志，下一个文件尚未
    // 开始，恢复后从同一 `file_li` 位置继续，不存在半页状态。
    yield_budget.maybe_yield().await;
  }
  Ok(moved)
}

/// 搬一批逻辑页：批量读旧物理页，再统一分配新物理页并批量写回。
///
/// `pair_li` 已按 `(旧 fp, page)` 升序排好：`remap_page` 按该序发新 fp，
/// 与 C++ 逐对 `AllocatePage` 的发号序一致（sem-60c）。
async fn compact_page_batch<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  pair_li: &[(FilePageId, PageId)],
  page_size: usize,
) -> Result<usize> {
  let old_fp_li: Vec<FilePageId> = pair_li.iter().map(|(fp, _)| *fp).collect();
  let mut page_buf_li: Vec<Vec<u8>> = pair_li.iter().map(|_| vec![0u8; page_size]).collect();
  read_page_li_retry(io, tbl, &old_fp_li, &mut page_buf_li).await?;

  let mut write_li: Vec<(FilePageId, Vec<u8>)> = Vec::with_capacity(pair_li.len());
  for ((_, page_id), page) in pair_li.iter().zip(page_buf_li) {
    let new_fp = mapper.remap_page(*page_id)?;
    write_li.push((new_fp, page));
  }
  write_page_li_retry(io, tbl, &write_li).await?;
  Ok(pair_li.len())
}

/// 一次搬多个 segment 文件：只扫一遍 segment mapping 分组 live 段，再逐文件批式搬。
///
/// 与 [`compact_file_li`] 同构：入口自带批次，直接调用失败即回滚。
pub async fn compact_segment_file_li<I: SegmentIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  file_li: &[FileId],
  segment_size: usize,
  segments_per_file_shift: u8,
) -> Result<usize> {
  compact_segment_file_li_budget(
    io,
    tbl,
    mapper,
    file_li,
    segment_size,
    segments_per_file_shift,
    0,
  )
  .await
}

/// 同 [`compact_segment_file_li`]，但接入 [`YieldBudget`]（l2-09，让出点同
/// [`compact_file_li_budget`]：段映射扫描 + segment 文件边界）。
pub async fn compact_segment_file_li_budget<I: SegmentIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  file_li: &[FileId],
  segment_size: usize,
  segments_per_file_shift: u8,
  yield_budget_us: u64,
) -> Result<usize> {
  if segment_size == 0 {
    return Err(KvError::InvalidArgs);
  }
  let own = mapper.begin_batch();
  let result = compact_segment_file_li_inner(
    io,
    tbl,
    mapper,
    file_li,
    segment_size,
    segments_per_file_shift,
    yield_budget_us,
  )
  .await;
  settle_batch(mapper, own, result.is_ok());
  result
}

async fn compact_segment_file_li_inner<I: SegmentIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  file_li: &[FileId],
  segment_size: usize,
  segments_per_file_shift: u8,
  yield_budget_us: u64,
) -> Result<usize> {
  let mut page_map =
    live_page_li_by_file_budget(mapper, file_li, segments_per_file_shift, yield_budget_us).await;
  let mut yield_budget = YieldBudget::new(yield_budget_us);
  let mut moved = 0;
  for file_id in file_li {
    let Some(mut pair_li) = page_map.remove(file_id) else {
      continue;
    };
    // 段内搬移序同 data 侧：`(旧 fp, segment)` 升序，对齐 C++
    // `DoCompactSegmentFile` 的 fp 升序堆排（background_write.cpp:393）。
    pair_li.sort_unstable();
    for chunk in pair_li.chunks(usize::from(MAX_SEGMENTS_BATCH)) {
      moved += compact_segment_batch(io, tbl, mapper, chunk, segment_size).await?;
    }
    // 段文件边界让出，状态安全性与 data 侧逐文件循环一致。
    yield_budget.maybe_yield().await;
  }
  Ok(moved)
}

/// 结算入口自建批次：只处理自己开的日志，嵌套调用交给外层（store 延后提交）。
fn settle_batch(mapper: &mut PageMapper, own: bool, ok: bool) {
  if !own {
    return;
  }
  if ok {
    mapper.commit_batch();
  } else {
    mapper.abort_batch();
  }
}

/// 搬一批逻辑段：批量读旧物理段，再统一分配新物理段并批量写回。
///
/// `pair_li` 已按 `(旧 fp, segment)` 升序排好（对齐 C++ 发号序，sem-60c）。
async fn compact_segment_batch<I: SegmentIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  pair_li: &[(FilePageId, PageId)],
  segment_size: usize,
) -> Result<usize> {
  let old_fp_li: Vec<FilePageId> = pair_li.iter().map(|(fp, _)| *fp).collect();
  let mut buf_li: Vec<Vec<u8>> = pair_li.iter().map(|_| vec![0u8; segment_size]).collect();
  try_join_all(old_fp_li.iter().copied().zip(buf_li.iter_mut()).map(
    |(segment_fp_id, buf)| async move { read_segment_retry(io, tbl, segment_fp_id, buf).await },
  ))
  .await?;

  let mut new_fp_li = Vec::with_capacity(pair_li.len());
  for (_, segment_id) in pair_li {
    new_fp_li.push(mapper.remap_page(*segment_id)?);
  }
  try_join_all(
    new_fp_li
      .iter()
      .copied()
      .zip(buf_li)
      .map(|(segment_fp_id, buf)| async move {
        write_segment_retry(io, tbl, segment_fp_id, &buf).await
      }),
  )
  .await?;
  Ok(pair_li.len())
}
