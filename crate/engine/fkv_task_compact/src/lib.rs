//! Compaction 执行：把指定文件里的 live page 搬到新物理页。
//!
//! 逻辑 PageId 不变，只换 `FilePageId`；搬空后由 `fkv_task_gc` 删除文件。
//! 搬页按批聚合，对齐 C++ `DoCompactDataFile` 的 `max_read_pages_batch`：
//! 一批先批量读，再统一 remap + 批量写，避免逐页 IO 往返。
//!
//! segment 文件同构：`compact_segment_file_li` 搬运的是整段（`segment_size`），
//! 逻辑 segment id 不变，搬空后的旧 segment 文件由 `fkv_task_gc` 回收。

use core::time::Duration;

use fkv_error::{KvError, Result};
use fkv_io::{PageIo, SegmentIo};
use fkv_mapping::PageMapper;
use fkv_task_gc::live_page_li_by_file;
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
}

async fn read_page_li_retry<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  file_page_id_li: &[FilePageId],
  page_buf_li: &mut [Vec<u8>],
) -> Result<()> {
  let mut retry = Retry::new();
  loop {
    match io.read_page_li(tbl, file_page_id_li, page_buf_li).await {
      Ok(()) => return Ok(()),
      Err(err) => {
        if !retry.wait(err).await {
          return Err(err);
        }
      }
    }
  }
}

async fn write_page_li_retry<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  write_li: &[(FilePageId, Vec<u8>)],
) -> Result<()> {
  let mut retry = Retry::new();
  loop {
    match io.write_page_li(tbl, write_li).await {
      Ok(()) => return Ok(()),
      Err(err) => {
        if !retry.wait(err).await {
          return Err(err);
        }
      }
    }
  }
}

async fn read_segment_retry<I: SegmentIo>(
  io: &I,
  tbl: &TableIdent,
  segment_fp_id: FilePageId,
  buf: &mut [u8],
) -> Result<()> {
  let mut retry = Retry::new();
  loop {
    match io.read_segment(tbl, segment_fp_id, buf).await {
      Ok(()) => return Ok(()),
      Err(err) => {
        if !retry.wait(err).await {
          return Err(err);
        }
      }
    }
  }
}

async fn write_segment_retry<I: SegmentIo>(
  io: &I,
  tbl: &TableIdent,
  segment_fp_id: FilePageId,
  buf: &[u8],
) -> Result<()> {
  let mut retry = Retry::new();
  loop {
    match io.write_segment(tbl, segment_fp_id, buf).await {
      Ok(()) => return Ok(()),
      Err(err) => {
        if !retry.wait(err).await {
          return Err(err);
        }
      }
    }
  }
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
  let own = mapper.begin_batch();
  let result =
    compact_file_li_inner(io, tbl, mapper, file_li, page_size, pages_per_file_shift).await;
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
) -> Result<usize> {
  let mut page_map = live_page_li_by_file(mapper, file_li, pages_per_file_shift);
  let mut moved = 0;
  for file_id in file_li {
    let Some(page_li) = page_map.remove(file_id) else {
      continue;
    };
    for chunk in page_li.chunks(usize::from(MAX_READ_PAGES_BATCH)) {
      moved += compact_page_batch(io, tbl, mapper, chunk, page_size).await?;
    }
  }
  Ok(moved)
}

/// 搬一批逻辑页：批量读旧物理页，再统一分配新物理页并批量写回。
async fn compact_page_batch<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  page_li: &[PageId],
  page_size: usize,
) -> Result<usize> {
  let old_fp_li: Vec<FilePageId> = page_li
    .iter()
    .map(|page_id| mapper.file_page_id(*page_id))
    .collect::<Result<_>>()?;
  let mut page_buf_li: Vec<Vec<u8>> = page_li.iter().map(|_| vec![0u8; page_size]).collect();
  read_page_li_retry(io, tbl, &old_fp_li, &mut page_buf_li).await?;

  let mut write_li: Vec<(FilePageId, Vec<u8>)> = Vec::with_capacity(page_li.len());
  for (page_id, page) in page_li.iter().zip(page_buf_li) {
    let new_fp = mapper.remap_page(*page_id)?;
    write_li.push((new_fp, page));
  }
  write_page_li_retry(io, tbl, &write_li).await?;
  Ok(page_li.len())
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
) -> Result<usize> {
  let mut page_map = live_page_li_by_file(mapper, file_li, segments_per_file_shift);
  let mut moved = 0;
  for file_id in file_li {
    let Some(page_li) = page_map.remove(file_id) else {
      continue;
    };
    for chunk in page_li.chunks(usize::from(MAX_SEGMENTS_BATCH)) {
      moved += compact_segment_batch(io, tbl, mapper, chunk, segment_size).await?;
    }
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
async fn compact_segment_batch<I: SegmentIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  segment_id_li: &[PageId],
  segment_size: usize,
) -> Result<usize> {
  let old_fp_li: Vec<FilePageId> = segment_id_li
    .iter()
    .map(|segment_id| mapper.file_page_id(*segment_id))
    .collect::<Result<_>>()?;
  let mut buf_li: Vec<Vec<u8>> = segment_id_li
    .iter()
    .map(|_| vec![0u8; segment_size])
    .collect();
  try_join_all(old_fp_li.iter().copied().zip(buf_li.iter_mut()).map(
    |(segment_fp_id, buf)| async move { read_segment_retry(io, tbl, segment_fp_id, buf).await },
  ))
  .await?;

  let mut new_fp_li = Vec::with_capacity(segment_id_li.len());
  for segment_id in segment_id_li {
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
  Ok(segment_id_li.len())
}
