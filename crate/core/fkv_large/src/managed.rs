//! managed / pinned buffer 读写路径骨架。
//!
//! - managed read：把 segment 数据直接读入调用方提供的 buffer，不构造中间 Vec。
//! - pinned write：从调用方提供的 4 KiB 对齐 buffer 直接按 segment 切片写出。
//!
//! 真正的 io_uring registered buffer（`buf_index` / fixed I/O）在 IO 层接入，
//! 这里先固定 API 与对齐 / 长度校验。

use fkv_buf::AlignedBuf;
use fkv_error::{KvError, Result};
use fkv_io::{FixedBuffer, FixedSegmentIo, SegmentIo};
use fkv_mapping::PageMapper;
use fkv_types::{PAGE_ALIGN, TableIdent};

use crate::{
  LENGTH_MASK, LargeValueHeader, MAX_METADATA_LEN, SegmentLayout, WrittenLargeValue,
  batch::{
    allocate_and_write, allocate_and_write_fixed, finish_written, read_batch, read_batch_fixed,
  },
  decode_header, decode_segment_id_li, metadata,
};

pub trait ManagedBuffer {
  fn as_managed_bytes(&mut self) -> &mut [u8];

  fn is_managed_aligned(&self, align: usize) -> bool;
}

impl ManagedBuffer for AlignedBuf {
  fn as_managed_bytes(&mut self) -> &mut [u8] {
    self
  }

  fn is_managed_aligned(&self, align: usize) -> bool {
    self.is_aligned(align)
  }
}

impl ManagedBuffer for &mut [u8] {
  fn as_managed_bytes(&mut self) -> &mut [u8] {
    self
  }

  fn is_managed_aligned(&self, align: usize) -> bool {
    (self.as_ptr() as usize).is_multiple_of(align)
  }
}

pub trait PinnedBuffer {
  fn as_pinned_bytes(&self) -> &[u8];

  fn is_pinned_aligned(&self, align: usize) -> bool;
}

impl PinnedBuffer for AlignedBuf {
  fn as_pinned_bytes(&self) -> &[u8] {
    self
  }

  fn is_pinned_aligned(&self, align: usize) -> bool {
    self.is_aligned(align)
  }
}

impl PinnedBuffer for &[u8] {
  fn as_pinned_bytes(&self) -> &[u8] {
    self
  }

  fn is_pinned_aligned(&self, align: usize) -> bool {
    (self.as_ptr() as usize).is_multiple_of(align)
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedReadResult {
  pub header: LargeValueHeader,
  pub metadata: Vec<u8>,
}

/// managed read：按批并发把各 segment 读入 `dst` 指定的调用方 buffer。
pub async fn read_value_into<I: SegmentIo, B: ManagedBuffer>(
  io: &I,
  tbl: &TableIdent,
  mapper: &PageMapper,
  layout: SegmentLayout,
  content: &[u8],
  dst: &mut B,
) -> Result<ManagedReadResult> {
  let header = decode_header(content, layout.segment_size())?;
  let segment_id_li = decode_segment_id_li(content, &header)?;
  let value_len = header.actual_len as usize;
  let dst_bytes = dst.as_managed_bytes();
  if dst_bytes.len() < value_len {
    return Err(KvError::InvalidArgs);
  }
  read_batch(
    io,
    tbl,
    mapper,
    &segment_id_li,
    &mut dst_bytes[..value_len],
    layout.segment_size() as usize,
  )
  .await?;
  Ok(ManagedReadResult {
    header,
    metadata: metadata(content, &header)?.to_vec(),
  })
}

/// pinned read：要求 buffer 4 KiB 对齐，其余同 managed read。
pub async fn read_value_pinned<I: SegmentIo, B: ManagedBuffer>(
  io: &I,
  tbl: &TableIdent,
  mapper: &PageMapper,
  layout: SegmentLayout,
  content: &[u8],
  dst: &mut B,
) -> Result<ManagedReadResult> {
  if !dst.is_managed_aligned(PAGE_ALIGN) {
    return Err(KvError::InvalidArgs);
  }
  read_value_into(io, tbl, mapper, layout, content, dst).await
}

/// pinned write：直接从调用方对齐 buffer 按 segment 切片写入。
pub async fn write_value_pinned<I: SegmentIo, B: PinnedBuffer>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  layout: SegmentLayout,
  src: &B,
  metadata: &[u8],
) -> Result<WrittenLargeValue> {
  if !src.is_pinned_aligned(PAGE_ALIGN) {
    return Err(KvError::InvalidArgs);
  }
  let value = src.as_pinned_bytes();
  let actual_len = u32::try_from(value.len()).map_err(|_| KvError::InvalidArgs)?;
  if actual_len > LENGTH_MASK || metadata.len() > MAX_METADATA_LEN {
    return Err(KvError::InvalidArgs);
  }
  let segment_id_li =
    allocate_and_write(io, tbl, mapper, layout.segment_size() as usize, value).await?;
  finish_written(mapper, actual_len, segment_id_li, metadata)
}

/// fixed-buffer 读：要求 `buf` 已由后端注册；按批并发 READ_FIXED 直读。
///
/// 批内并发、批间串行（对齐 `batch.rs` 既有模式，rev-44 残余
/// 计划 1）：同一 `FixedBuffer` 上各段区间互不重叠，后端以 `share_disjoint`
/// 区间租约同时提交；段数超过 `MAX_SEGMENTS_BATCH` 时分批，限制同一时刻的
/// 在飞缓冲数量。默认实现逐段回退，compio 覆盖批量提交。
pub async fn read_value_fixed<I: FixedSegmentIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &PageMapper,
  layout: SegmentLayout,
  content: &[u8],
  buf: &mut FixedBuffer,
) -> Result<ManagedReadResult> {
  let header = decode_header(content, layout.segment_size())?;
  let segment_id_li = decode_segment_id_li(content, &header)?;
  let value_len = header.actual_len as usize;
  if buf.len() < value_len {
    return Err(KvError::InvalidArgs);
  }
  read_batch_fixed(
    io,
    tbl,
    mapper,
    &segment_id_li,
    buf,
    value_len,
    layout.segment_size() as usize,
  )
  .await?;
  Ok(ManagedReadResult {
    header,
    metadata: metadata(content, &header)?.to_vec(),
  })
}

/// fixed-buffer 写：`buf` 全长作为 value，按 segment 使用 WRITE_FIXED 写出。
pub async fn write_value_fixed<I: FixedSegmentIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  layout: SegmentLayout,
  buf: &FixedBuffer,
  metadata: &[u8],
) -> Result<WrittenLargeValue> {
  write_value_fixed_with_len(io, tbl, mapper, layout, buf, buf.len(), metadata).await
}

/// 指定 buffer 内的有效 value 长度；尾部保留区不写出。
pub async fn write_value_fixed_with_len<I: FixedSegmentIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  layout: SegmentLayout,
  buf: &FixedBuffer,
  value_len: usize,
  metadata: &[u8],
) -> Result<WrittenLargeValue> {
  if value_len > buf.len() {
    return Err(KvError::InvalidArgs);
  }
  let value = &buf.as_slice()[..value_len];
  let actual_len = u32::try_from(value.len()).map_err(|_| KvError::InvalidArgs)?;
  if actual_len > LENGTH_MASK || metadata.len() > MAX_METADATA_LEN {
    return Err(KvError::InvalidArgs);
  }
  let segment_id_li = allocate_and_write_fixed(
    io,
    tbl,
    mapper,
    layout.segment_size() as usize,
    buf,
    value.len(),
  )
  .await?;
  finish_written(mapper, actual_len, segment_id_li, metadata)
}
