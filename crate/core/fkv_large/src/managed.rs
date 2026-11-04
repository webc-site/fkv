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
use fkv_types::{PageId, TableIdent};

use crate::{
  LENGTH_MASK, LargeValueHeader, MAX_METADATA_LEN, SEGMENT_ALIGN, SegmentLayout, WrittenLargeValue,
  decode_header, decode_segment_id_li, encode, metadata,
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

/// managed read：逐 segment 读入 `dst` 指定的调用方 buffer。
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
  let segment_size = layout.segment_size() as usize;
  let mut remaining = value_len;
  let mut offset = 0usize;
  for segment_id in segment_id_li {
    if remaining == 0 {
      break;
    }
    let chunk_len = remaining.min(segment_size);
    let segment_fp_id = mapper.file_page_id(segment_id)?;
    io.read_segment(
      tbl,
      segment_fp_id,
      &mut dst_bytes[offset..offset + chunk_len],
    )
    .await?;
    offset += chunk_len;
    remaining -= chunk_len;
  }
  if remaining != 0 {
    return Err(KvError::Corrupted);
  }
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
  if !dst.is_managed_aligned(SEGMENT_ALIGN as usize) {
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
  if !src.is_pinned_aligned(SEGMENT_ALIGN as usize) {
    return Err(KvError::InvalidArgs);
  }
  let value = src.as_pinned_bytes();
  let actual_len = u32::try_from(value.len()).map_err(|_| KvError::InvalidArgs)?;
  if actual_len > LENGTH_MASK || metadata.len() > MAX_METADATA_LEN {
    return Err(KvError::InvalidArgs);
  }
  let segment_size = layout.segment_size() as usize;
  let mut segment_id_li: Vec<PageId> = Vec::with_capacity(layout.segment_num(actual_len) as usize);
  let mut result = Ok(());
  for chunk in value.chunks(segment_size) {
    let segment_id = mapper.allocate_page();
    segment_id_li.push(segment_id);
    let write_result = async {
      let segment_fp_id = mapper.file_page_id(segment_id)?;
      io.write_segment(tbl, segment_fp_id, chunk).await
    }
    .await;
    if let Err(err) = write_result {
      result = Err(err);
      break;
    }
  }
  if let Err(err) = result {
    for segment_id in &segment_id_li {
      let _ = mapper.free_page(*segment_id);
    }
    return Err(err);
  }
  let content = match encode(actual_len, &segment_id_li, metadata) {
    Ok(content) => content,
    Err(err) => {
      for segment_id in &segment_id_li {
        let _ = mapper.free_page(*segment_id);
      }
      return Err(err);
    }
  };
  Ok(WrittenLargeValue {
    content,
    segment_id_li,
  })
}

/// fixed-buffer 读：要求 `buf` 已由后端注册；逐 segment 使用 READ_FIXED 直读。
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
  let segment_size = layout.segment_size() as usize;
  let mut remaining = value_len;
  let mut offset = 0usize;
  for segment_id in segment_id_li {
    if remaining == 0 {
      break;
    }
    let chunk_len = remaining.min(segment_size);
    let segment_fp_id = mapper.file_page_id(segment_id)?;
    io.read_segment_fixed(tbl, segment_fp_id, buf, offset, chunk_len)
      .await?;
    offset += chunk_len;
    remaining -= chunk_len;
  }
  if remaining != 0 {
    return Err(KvError::Corrupted);
  }
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
  let segment_size = layout.segment_size() as usize;
  let mut segment_id_li: Vec<PageId> = Vec::with_capacity(layout.segment_num(actual_len) as usize);
  let mut offset = 0usize;
  let mut result = Ok(());
  for chunk in value.chunks(segment_size) {
    let segment_id = mapper.allocate_page();
    segment_id_li.push(segment_id);
    let write_result = async {
      let segment_fp_id = mapper.file_page_id(segment_id)?;
      io.write_segment_fixed(tbl, segment_fp_id, buf, offset, chunk.len())
        .await
    }
    .await;
    if let Err(err) = write_result {
      result = Err(err);
      break;
    }
    offset += chunk.len();
  }
  if let Err(err) = result {
    for segment_id in &segment_id_li {
      let _ = mapper.free_page(*segment_id);
    }
    return Err(err);
  }
  let content = match encode(actual_len, &segment_id_li, metadata) {
    Ok(content) => content,
    Err(err) => {
      for segment_id in &segment_id_li {
        let _ = mapper.free_page(*segment_id);
      }
      return Err(err);
    }
  };
  Ok(WrittenLargeValue {
    content,
    segment_id_li,
  })
}
