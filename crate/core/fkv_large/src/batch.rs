//! 多段 IO 批量提交。
//!
//! C++ 每次向 io_uring 提交一批（`max_segments_batch`）段读写 SQE；Rust 侧让同一
//! 批的段 future 由 `try_join_all` 并发驱动：首次 poll 时各段全部 submit 进 ring，
//! 队列深度等于批大小，然后再等完成。批间串行，限制同一时刻的在飞缓冲数量。

use core::future::Future;

use fkv_error::{KvError, Result};
use fkv_io::{FixedBuffer, FixedSegmentIo, FixedSegmentRead, SegmentIo};
use fkv_mapping::PageMapper;
use fkv_types::{MAX_SEGMENTS_BATCH, PageId, TableIdent};
use futures::future::try_join_all;

use crate::{WrittenLargeValue, encode};

// 单批段数上限唯一来源是 `fkv_types::MAX_SEGMENTS_BATCH`；
// u16 → usize 投影在各使用点完成。
/// 批内并发、批间串行执行段 IO。
async fn run_batch<T, F, Fut>(item_li: impl IntoIterator<Item = T>, run: F) -> Result<()>
where
  F: Fn(T) -> Fut,
  Fut: Future<Output = Result<()>>,
{
  let mut iter = item_li.into_iter();
  let mut batch = Vec::with_capacity(usize::from(MAX_SEGMENTS_BATCH));
  loop {
    batch.clear();
    batch.extend(iter.by_ref().take(usize::from(MAX_SEGMENTS_BATCH)));
    if batch.is_empty() {
      return Ok(());
    }
    try_join_all(batch.drain(..).map(&run)).await?;
  }
}

/// 按 `segment_size` 把 `dst` 切成互不重叠的切片，逐批并发读入。
pub(crate) async fn read_batch<I: SegmentIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &PageMapper,
  segment_id_li: &[PageId],
  dst: &mut [u8],
  segment_size: usize,
) -> Result<()> {
  if segment_id_li.len() < dst.len().div_ceil(segment_size) {
    return Err(KvError::Corrupted);
  }
  run_batch(
    segment_id_li
      .iter()
      .copied()
      .zip(dst.chunks_mut(segment_size)),
    |(segment_id, chunk)| async move {
      io.read_segment(tbl, mapper.file_page_id(segment_id)?, chunk)
        .await
    },
  )
  .await
}

/// 按 `segment_size` 把 `src` 切成段，逐批并发写出。
pub(crate) async fn write_batch<I: SegmentIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &PageMapper,
  segment_id_li: &[PageId],
  src: &[u8],
  segment_size: usize,
) -> Result<()> {
  if segment_id_li.len() != src.len().div_ceil(segment_size) {
    return Err(KvError::InvalidArgs);
  }
  run_batch(
    segment_id_li.iter().copied().zip(src.chunks(segment_size)),
    |(segment_id, chunk)| async move {
      io.write_segment(tbl, mapper.file_page_id(segment_id)?, chunk)
        .await
    },
  )
  .await
}

/// 只读 `buf[..value_len]`，切成互不重叠的区间逐批 fixed 读入。
///
/// 与 [`write_batch_fixed`] 同构：批内整表交给 `read_segment_fixed_li`（后端
/// 对已注册缓冲一次提交多个 READ_FIXED 并发等待），批间串行，限制同一时刻
/// 的在飞缓冲数量。
pub(crate) async fn read_batch_fixed<I: FixedSegmentIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &PageMapper,
  segment_id_li: &[PageId],
  buf: &mut FixedBuffer,
  value_len: usize,
  segment_size: usize,
) -> Result<()> {
  if segment_id_li.len() < value_len.div_ceil(segment_size) {
    return Err(KvError::Corrupted);
  }
  let mut iter = segment_id_li
    .iter()
    .copied()
    .zip((0..value_len).step_by(segment_size));
  let mut batch = Vec::with_capacity(usize::from(MAX_SEGMENTS_BATCH));
  loop {
    batch.clear();
    for (segment_id, offset) in iter.by_ref().take(usize::from(MAX_SEGMENTS_BATCH)) {
      batch.push(FixedSegmentRead {
        segment_fp_id: mapper.file_page_id(segment_id)?,
        offset,
        len: (value_len - offset).min(segment_size),
      });
    }
    if batch.is_empty() {
      return Ok(());
    }
    io.read_segment_fixed_li(tbl, &batch, buf).await?;
  }
}

/// 只读 `buf[..value_len]`，逐批并发 fixed 写出；`buf` 共享借用，段间互不重叠。
pub(crate) async fn write_batch_fixed<I: FixedSegmentIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &PageMapper,
  segment_id_li: &[PageId],
  buf: &FixedBuffer,
  value_len: usize,
  segment_size: usize,
) -> Result<()> {
  if value_len > buf.len() || segment_id_li.len() != value_len.div_ceil(segment_size) {
    return Err(KvError::InvalidArgs);
  }
  run_batch(
    segment_id_li
      .iter()
      .copied()
      .zip((0..value_len).step_by(segment_size)),
    |(segment_id, offset)| {
      let len = (value_len - offset).min(segment_size);
      async move {
        io.write_segment_fixed(tbl, mapper.file_page_id(segment_id)?, buf, offset, len)
          .await
      }
    },
  )
  .await
}

/// 分配逻辑段并按批并发写出；任一段失败则释放本次分配的全部段。
pub(crate) async fn allocate_and_write<I: SegmentIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  segment_size: usize,
  value: &[u8],
) -> Result<Vec<PageId>> {
  let segment_id_li = allocate_segment_li(mapper, value.len().div_ceil(segment_size));
  match write_batch(io, tbl, mapper, &segment_id_li, value, segment_size).await {
    Ok(()) => Ok(segment_id_li),
    Err(err) => {
      free_segment_li(mapper, &segment_id_li);
      Err(err)
    }
  }
}

/// fixed 版本的 [`allocate_and_write`]。
pub(crate) async fn allocate_and_write_fixed<I: FixedSegmentIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  segment_size: usize,
  buf: &FixedBuffer,
  value_len: usize,
) -> Result<Vec<PageId>> {
  let segment_id_li = allocate_segment_li(mapper, value_len.div_ceil(segment_size));
  match write_batch_fixed(
    io,
    tbl,
    mapper,
    &segment_id_li,
    buf,
    value_len,
    segment_size,
  )
  .await
  {
    Ok(()) => Ok(segment_id_li),
    Err(err) => {
      free_segment_li(mapper, &segment_id_li);
      Err(err)
    }
  }
}

fn allocate_segment_li(mapper: &mut PageMapper, seg_num: usize) -> Vec<PageId> {
  let mut segment_id_li = Vec::with_capacity(seg_num);
  segment_id_li.extend((0..seg_num).map(|_| mapper.allocate_page()));
  segment_id_li
}

fn free_segment_li(mapper: &mut PageMapper, segment_id_li: &[PageId]) {
  for segment_id in segment_id_li {
    let _ = mapper.free_page(*segment_id);
  }
}

/// 段写完后编码 page content；编码失败回收本次分配的段。
pub(crate) fn finish_written(
  mapper: &mut PageMapper,
  actual_len: u32,
  segment_id_li: Vec<PageId>,
  metadata: &[u8],
) -> Result<WrittenLargeValue> {
  match encode(actual_len, &segment_id_li, metadata) {
    Ok(content) => Ok(WrittenLargeValue {
      content,
      segment_id_li,
    }),
    Err(err) => {
      free_segment_li(mapper, &segment_id_li);
      Err(err)
    }
  }
}
