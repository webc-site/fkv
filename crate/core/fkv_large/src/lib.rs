//! 大 value：segment 映射、segment 文件读写计划、metadata trailer。
//!
//! data page 里的编码：
//! `word0(4B) | K × logical_segment_id(4B) | [metadata_len(2B) | metadata]`。
//! K 由 `ceil(actual_len / segment_size)` 推导，不单独存储；bit31 表示有 metadata。

use fkv_codec::{get_fixed16, get_fixed32};
use fkv_error::{KvError, Result};
use fkv_io::SegmentIo;
use fkv_mapping::PageMapper;
use fkv_types::{FileId, FilePageId, PAGE_ALIGN, PageId, TableIdent};

mod batch;
pub mod managed;

use batch::{allocate_and_write, finish_written, read_batch};
pub use managed::{
  ManagedBuffer, ManagedReadResult, PinnedBuffer, read_value_fixed, read_value_into,
  read_value_pinned, write_value_fixed, write_value_fixed_with_len, write_value_pinned,
};

pub const HAS_METADATA_BIT: u32 = 0x8000_0000;
pub const LENGTH_MASK: u32 = 0x7fff_ffff;
pub const MAX_METADATA_LEN: usize = u16::MAX as usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LargeValueHeader {
  pub actual_len: u32,
  pub segment_num: u32,
  pub metadata_len: u16,
}

/// segment 文件布局：`segment_fp_id -> (file_id, offset)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentLayout {
  segment_size: u32,
  segments_per_file_shift: u8,
}

impl SegmentLayout {
  pub fn new(segment_size: u32, segments_per_file_shift: u8) -> Result<Self> {
    if segment_size == 0
      || !segment_size.is_multiple_of(PAGE_ALIGN as u32)
      || segments_per_file_shift >= 63
    {
      return Err(KvError::InvalidArgs);
    }
    let layout = Self {
      segment_size,
      segments_per_file_shift,
    };
    layout.file_size()?;
    Ok(layout)
  }

  pub fn segment_size(&self) -> u32 {
    self.segment_size
  }

  pub fn segments_per_file_shift(&self) -> u8 {
    self.segments_per_file_shift
  }

  pub fn segment_num(&self, actual_len: u32) -> u32 {
    if actual_len == 0 {
      0
    } else {
      actual_len.div_ceil(self.segment_size)
    }
  }

  pub fn file_id(&self, segment_fp_id: FilePageId) -> FileId {
    segment_fp_id >> self.segments_per_file_shift
  }

  pub fn offset(&self, segment_fp_id: FilePageId) -> u64 {
    let mask = (1u64 << self.segments_per_file_shift) - 1;
    (segment_fp_id & mask) * u64::from(self.segment_size)
  }

  pub fn file_size(&self) -> Result<u64> {
    u64::from(self.segment_size)
      .checked_shl(u32::from(self.segments_per_file_shift))
      .ok_or(KvError::InvalidArgs)
  }
}

/// 编码放入 data page 的 large-value content。
pub fn encode(actual_len: u32, segment_id_li: &[PageId], metadata: &[u8]) -> Result<Vec<u8>> {
  if actual_len > LENGTH_MASK || metadata.len() > MAX_METADATA_LEN {
    return Err(KvError::InvalidArgs);
  }
  let mut out = Vec::with_capacity(
    4 + segment_id_li.len() * 4
      + if metadata.is_empty() {
        0
      } else {
        2 + metadata.len()
      },
  );
  let mut word0 = actual_len;
  if !metadata.is_empty() {
    word0 |= HAS_METADATA_BIT;
  }
  out.extend_from_slice(&word0.to_le_bytes());
  for segment_id in segment_id_li {
    out.extend_from_slice(&segment_id.to_le_bytes());
  }
  if !metadata.is_empty() {
    out.extend_from_slice(&(metadata.len() as u16).to_le_bytes());
    out.extend_from_slice(metadata);
  }
  Ok(out)
}

/// 只解析头（含 metadata 长度），不读取 segment id 数组。
pub fn decode_header(encoded: &[u8], segment_size: u32) -> Result<LargeValueHeader> {
  if segment_size == 0 {
    return Err(KvError::InvalidArgs);
  }
  if encoded.len() < 4 {
    return Err(KvError::Corrupted);
  }
  let word0 = get_fixed32(encoded)?;
  let actual_len = word0 & LENGTH_MASK;
  let segment_num = if actual_len == 0 {
    0
  } else {
    actual_len.div_ceil(segment_size)
  };
  let segments_end = 4usize
    .checked_add(segment_num as usize * 4)
    .ok_or(KvError::Corrupted)?;
  let has_metadata = word0 & HAS_METADATA_BIT != 0;
  let metadata_len = if has_metadata {
    if encoded.len() < segments_end + 2 {
      return Err(KvError::Corrupted);
    }
    let metadata_len = get_fixed16(&encoded[segments_end..])?;
    let expected = segments_end
      .checked_add(2 + metadata_len as usize)
      .ok_or(KvError::Corrupted)?;
    if encoded.len() != expected {
      return Err(KvError::Corrupted);
    }
    metadata_len
  } else {
    if encoded.len() != segments_end {
      return Err(KvError::Corrupted);
    }
    0
  };
  Ok(LargeValueHeader {
    actual_len,
    segment_num,
    metadata_len,
  })
}

pub fn decode_segment_id_li(encoded: &[u8], header: &LargeValueHeader) -> Result<Vec<PageId>> {
  let mut segment_id_li = Vec::with_capacity(header.segment_num as usize);
  let mut pos = 4usize;
  for _ in 0..header.segment_num {
    segment_id_li.push(get_fixed32(&encoded[pos..])?);
    pos += 4;
  }
  Ok(segment_id_li)
}

pub fn metadata<'a>(encoded: &'a [u8], header: &LargeValueHeader) -> Result<&'a [u8]> {
  if header.metadata_len == 0 {
    return Ok(&[]);
  }
  let start = 4 + header.segment_num as usize * 4 + 2;
  let end = start + header.metadata_len as usize;
  encoded.get(start..end).ok_or(KvError::Corrupted)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedLargeValue {
  pub header: LargeValueHeader,
  pub segment_id_li: Vec<PageId>,
  pub metadata: Vec<u8>,
}

pub fn decode(encoded: &[u8], segment_size: u32) -> Result<DecodedLargeValue> {
  let header = decode_header(encoded, segment_size)?;
  let segment_id_li = decode_segment_id_li(encoded, &header)?;
  let metadata = metadata(encoded, &header)?.to_vec();
  Ok(DecodedLargeValue {
    header,
    segment_id_li,
    metadata,
  })
}

/// metadata-only 读取：只解析头与 trailer，不触碰 segment 文件。
pub fn decode_metadata_only(
  encoded: &[u8],
  segment_size: u32,
) -> Result<(LargeValueHeader, Vec<u8>)> {
  let header = decode_header(encoded, segment_size)?;
  let metadata = metadata(encoded, &header)?.to_vec();
  Ok((header, metadata))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenLargeValue {
  pub content: Vec<u8>,
  pub segment_id_li: Vec<PageId>,
}

/// 写入计划：按 segment 切分、分配逻辑 segment id、按批并发写入 segment 文件，返回 page content。
pub async fn write_value<I: SegmentIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  layout: SegmentLayout,
  value: &[u8],
  metadata: &[u8],
) -> Result<WrittenLargeValue> {
  let actual_len = u32::try_from(value.len()).map_err(|_| KvError::InvalidArgs)?;
  if actual_len > LENGTH_MASK || metadata.len() > MAX_METADATA_LEN {
    return Err(KvError::InvalidArgs);
  }
  let segment_id_li =
    allocate_and_write(io, tbl, mapper, layout.segment_size() as usize, value).await?;
  finish_written(mapper, actual_len, segment_id_li, metadata)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadLargeValue {
  pub header: LargeValueHeader,
  pub value: Vec<u8>,
  pub metadata: Vec<u8>,
}

/// 读取计划：解析 page content，按逻辑 id 查 mapping，按批并发读回到 `actual_len`。
pub async fn read_value<I: SegmentIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &PageMapper,
  layout: SegmentLayout,
  content: &[u8],
) -> Result<ReadLargeValue> {
  let header = decode_header(content, layout.segment_size())?;
  let segment_id_li = decode_segment_id_li(content, &header)?;
  // 一次性分配完整目标缓冲，按段切给并发读，全程零额外拷贝。
  let mut value = vec![0u8; header.actual_len as usize];
  read_batch(
    io,
    tbl,
    mapper,
    &segment_id_li,
    &mut value,
    layout.segment_size() as usize,
  )
  .await?;
  let metadata = metadata(content, &header)?.to_vec();
  Ok(ReadLargeValue {
    header,
    value,
    metadata,
  })
}

/// 删除大 value 时释放逻辑 segment id（物理 id 进入 garbage 列表）。
pub fn free_value(mapper: &mut PageMapper, content: &[u8], segment_size: u32) -> Result<usize> {
  let header = decode_header(content, segment_size)?;
  let segment_id_li = decode_segment_id_li(content, &header)?;
  let mut freed = 0;
  for segment_id in segment_id_li {
    if mapper.free_page(segment_id)?.is_some() {
      freed += 1;
    }
  }
  Ok(freed)
}
