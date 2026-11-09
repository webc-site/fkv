//! Data page：header(19B) + entries + restart array + restart_num(2B) + padding。

use std::cmp::Ordering;

use fkv_codec::{
  decode_i64_delta, encode_i64_delta, get_varint32, get_varint64, put_fixed32, put_varint32,
  put_varint64, varint32_len, varint64_len,
};
use fkv_error::{KvError, Result};

use crate::{
  header::{
    DATA_HEADER_SIZE, DATA_NEXT_PAGE_OFFSET, DATA_PREV_PAGE_OFFSET, PageType, content_len,
    data_restart_offset, new_page, page_type, restart_num, set_checksum, set_content_len,
  },
  restart::{self, cmp_split, common_prefix},
};

pub(crate) const VAL_LEN_BITS: u8 = 4;

/// 对齐 C++ `DataPageBuilder::IsOverflowKV`：估算单条 KV 在**空页**上是否放不下，
/// 放不下即需要拆 overflow 页。估算用的 `stored_val_len` 不含 flag 位、`last_key` 为空。
pub fn is_overflow_kv(
  key: &[u8],
  val_size: usize,
  ts: u64,
  expire_ts: u64,
  page_size: usize,
) -> bool {
  let reserved = DATA_HEADER_SIZE + 4;
  if reserved + val_size > page_size {
    return true;
  }
  let stored_val_len = (val_size as u64) << VAL_LEN_BITS;
  // ts 已收敛到 [0, i64::MAX]，zigzag 必然可编码。
  let ts_delta = encode_i64_delta(clamp_ts_to_i64(ts)).unwrap_or_default();
  let addition = varint32_len(0)
    + varint32_len(key.len() as u32)
    + varint32_len(stored_val_len as u32)
    + key.len()
    + val_size
    + usize::from(expire_ts != 0) * varint64_len(expire_ts)
    + varint64_len(ts_delta);
  reserved + addition > page_size
}

/// 对齐 C++ `ClampTimestampToInt64`：超过 i64::MAX 的 ts 饱和，避免 `as i64` 回绕。
fn clamp_ts_to_i64(ts: u64) -> i64 {
  ts.min(i64::MAX as u64) as i64
}

/// 单条 entry 编码后的尺寸拆分。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntrySize {
  /// 与页内前一条的共享前缀长度（页首 / restart 为 0）。
  pub shared: usize,
  /// 与前一条的 zigzag 时间戳增量（页首 / restart 以 0 为基点）。
  pub ts_delta: u64,
  /// 写入 entries 区的字节数，不含 restart 数组里本条的 2 字节偏移。
  pub len: usize,
  /// value 长度与标志位打包后的 varint 值（`add` 复用，避免重算）。
  stored_val_len: u32,
}

/// 整页内容长度：页头 + entries 编码 + restart 偏移数组 + restart 计数。
/// `restart_num` 含页首隐含的那一项，与 `DataPageBuilder` 的容量口径一致。
pub const fn page_content_len(entry_len: usize, restart_num: usize) -> usize {
  DATA_HEADER_SIZE + entry_len + restart_num * 2 + 2
}

/// 写入 / 估算一条 data page entry 的参数（`DataPageBuilder::add` 与
/// `entry_size` 共用同一口径）。
///
/// 借用 `key` / `value`，全程零拷贝；命名字段替代原先的 7 个位置参数，
/// 避免相邻 bool（`overflow` / `large_value`）在调用点传错位。
#[derive(Debug, Clone, Copy)]
pub struct EntrySpec<'a> {
  pub key: &'a [u8],
  pub value: &'a [u8],
  pub ts: u64,
  /// 0 表示无过期时间。
  pub expire_ts: u64,
  /// value 是否为 overflow 指针数组占位。
  pub overflow: bool,
  pub compression: CompressionType,
  /// value 是否整段外置（大 value 段，不带 compression flag）。
  pub large_value: bool,
}

/// 估算一条 entry 的页内编码尺寸（不实际编码）。
///
/// `add` 的容量判断与写路径的四分页切分点试探共用此口径，保证估算与实际
/// 编码逐字节一致。`prev` 为页内前一条的 `(key, ts)`；`None` 或 `restart`
/// 表示按页首 / restart 编码（共享前缀归零、时间戳基点归零）。
pub fn entry_size(
  spec: EntrySpec<'_>,
  prev: Option<(&[u8], u64)>,
  restart: bool,
) -> Result<EntrySize> {
  let EntrySpec {
    key,
    value,
    ts,
    expire_ts,
    overflow,
    compression,
    large_value,
  } = spec;
  if large_value && overflow {
    return Err(KvError::InvalidArgs);
  }
  let value_len = u64::try_from(value.len()).map_err(|_| KvError::InvalidArgs)?;
  if value_len > u64::from(u32::MAX >> VAL_LEN_BITS) {
    return Err(KvError::InvalidArgs);
  }
  let mut stored_val_len = value_len << VAL_LEN_BITS;
  if overflow {
    stored_val_len |= 1;
  }
  if large_value {
    stored_val_len |= 0b11 << 2;
  } else {
    stored_val_len |= u64::from(compression as u8) << 2;
  }
  if expire_ts != 0 {
    stored_val_len |= 1 << 1;
  }
  let stored_val_len = u32::try_from(stored_val_len).map_err(|_| KvError::InvalidArgs)?;

  let (shared, last_ts) = match prev {
    Some((prev_key, prev_ts)) if !restart => {
      (common_prefix(prev_key, key), clamp_ts_to_i64(prev_ts))
    }
    _ => (0, 0),
  };
  let non_shared = key.len() - shared;
  let ts_delta = encode_i64_delta(clamp_ts_to_i64(ts) - last_ts).ok_or(KvError::InvalidArgs)?;
  let len = varint32_len(shared as u32)
    + varint32_len(non_shared as u32)
    + varint32_len(stored_val_len)
    + non_shared
    + value.len()
    + usize::from(expire_ts != 0) * varint64_len(expire_ts)
    + varint64_len(ts_delta);
  Ok(EntrySize {
    shared,
    ts_delta,
    len,
    stored_val_len,
  })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompressionType {
  None = 0,
  Dictionary = 1,
  Standalone = 2,
}

impl CompressionType {
  pub fn from_bits(bits: u8) -> Result<Self> {
    match bits {
      0 => Ok(Self::None),
      1 => Ok(Self::Dictionary),
      2 => Ok(Self::Standalone),
      _ => Err(KvError::Corrupted),
    }
  }
}

#[derive(Debug, Clone)]
pub struct DataPageBuilder {
  page_size: usize,
  restart_interval: u16,
  buf: Vec<u8>,
  restart_li: Vec<u16>,
  counter: u16,
  cnt: u16,
  /// 是否已写入至少一条 entry。空首 key 是合法输入，不能用
  /// `last_key.is_empty()` 兼作「无前驱」哨兵。
  has_prev: bool,
  last_key: Vec<u8>,
  last_ts: i64,
  finished: bool,
}

impl DataPageBuilder {
  pub fn new(page_size: usize, restart_interval: u16) -> Result<Self> {
    if restart_interval == 0 {
      return Err(KvError::InvalidArgs);
    }
    let buf = new_page(page_size, PageType::Data, DATA_HEADER_SIZE)?;
    Ok(Self {
      page_size,
      restart_interval,
      buf,
      restart_li: vec![DATA_HEADER_SIZE as u16],
      counter: 0,
      cnt: 0,
      has_prev: false,
      last_key: Vec::new(),
      last_ts: 0,
      finished: false,
    })
  }

  pub fn is_empty(&self) -> bool {
    self.cnt == 0
  }

  pub fn current_size_estimate(&self) -> usize {
    self.buf.len() + self.restart_li.len() * 2 + 2
  }

  pub fn add(&mut self, spec: EntrySpec<'_>) -> Result<bool> {
    if self.finished {
      return Err(KvError::InvalidArgs);
    }
    // 乱序 / 重复 key 是调用方错误（C++ 侧只有 debug assert），显式报错而不是
    // 与“页满”共用 `Ok(false)`。
    if self.has_prev && spec.key <= self.last_key.as_slice() {
      return Err(KvError::InvalidArgs);
    }
    let restart = self.counter >= self.restart_interval;
    let prev = if self.has_prev {
      Some((self.last_key.as_slice(), self.last_ts as u64))
    } else {
      None
    };
    let size = entry_size(spec, prev, restart)?;
    let addition = size.len + if restart { 2 } else { 0 };

    if self.current_size_estimate() + addition > self.page_size {
      return Ok(false);
    }

    if restart {
      self
        .restart_li
        .push(u16::try_from(self.buf.len()).map_err(|_| KvError::InvalidArgs)?);
      self.counter = 0;
      self.last_ts = 0;
    }

    let mut scratch = [0u8; 10];
    let n = put_varint32(&mut scratch, size.shared as u32).ok_or(KvError::InvalidArgs)?;
    self.buf.extend_from_slice(&scratch[..n]);
    let n = put_varint32(&mut scratch, (spec.key.len() - size.shared) as u32)
      .ok_or(KvError::InvalidArgs)?;
    self.buf.extend_from_slice(&scratch[..n]);
    let n = put_varint32(&mut scratch, size.stored_val_len).ok_or(KvError::InvalidArgs)?;
    self.buf.extend_from_slice(&scratch[..n]);
    self.buf.extend_from_slice(&spec.key[size.shared..]);
    self.buf.extend_from_slice(spec.value);
    if spec.expire_ts != 0 {
      let n = put_varint64(&mut scratch, spec.expire_ts).ok_or(KvError::InvalidArgs)?;
      self.buf.extend_from_slice(&scratch[..n]);
    }
    let n = put_varint64(&mut scratch, size.ts_delta).ok_or(KvError::InvalidArgs)?;
    self.buf.extend_from_slice(&scratch[..n]);

    self.last_key.clear();
    self.last_key.extend_from_slice(spec.key);
    self.has_prev = true;
    self.last_ts = clamp_ts_to_i64(spec.ts);
    self.counter += 1;
    self.cnt += 1;
    Ok(true)
  }

  /// 与 C++ `DataPageBuilder::Finish` 一致的输出：不写 checksum（前 8 字节为 0），
  /// 供 V2 格式差分逐字节对比；落盘前需调用方 `set_checksum`。
  pub fn finish_without_checksum(self) -> Result<Vec<u8>> {
    self.finish_inner(false)
  }

  pub fn finish(self) -> Result<Vec<u8>> {
    self.finish_inner(true)
  }

  fn finish_inner(mut self, with_checksum: bool) -> Result<Vec<u8>> {
    self.finished = true;
    for offset in &self.restart_li {
      let mut dst = [0u8; 2];
      fkv_codec::put_fixed16(&mut dst, *offset);
      self.buf.extend_from_slice(&dst);
    }
    let restart_count = u16::try_from(self.restart_li.len()).map_err(|_| KvError::InvalidArgs)?;
    let mut dst = [0u8; 2];
    fkv_codec::put_fixed16(&mut dst, restart_count);
    self.buf.extend_from_slice(&dst);

    let content_size = u16::try_from(self.buf.len()).map_err(|_| KvError::InvalidArgs)?;
    set_content_len(&mut self.buf, content_size)?;
    self.buf.resize(self.page_size, 0);
    if with_checksum {
      set_checksum(&mut self.buf)?;
    }
    Ok(self.buf)
  }
}

#[derive(Debug, Clone)]
pub struct DataPage<'a> {
  page: &'a [u8],
}

impl<'a> DataPage<'a> {
  pub fn new(page: &'a [u8]) -> Result<Self> {
    if page_type(page)? != PageType::Data {
      return Err(KvError::Corrupted);
    }
    Ok(Self { page })
  }

  pub fn content_len(&self) -> Result<u16> {
    content_len(self.page)
  }

  pub fn prev_page_id(&self) -> Result<u32> {
    fkv_codec::get_fixed32(
      self
        .page
        .get(DATA_PREV_PAGE_OFFSET..)
        .ok_or(KvError::EndOfFile)?,
    )
  }

  pub fn next_page_id(&self) -> Result<u32> {
    fkv_codec::get_fixed32(
      self
        .page
        .get(DATA_NEXT_PAGE_OFFSET..)
        .ok_or(KvError::EndOfFile)?,
    )
  }
}

#[derive(Debug)]
pub struct DataPageIter<'a> {
  page: &'a [u8],
  restart_num: u16,
  restart_offset: usize,
  curr_offset: usize,
  curr_restart_idx: u16,
  key: Vec<u8>,
  value_range: (usize, usize),
  timestamp: u64,
  expire_ts: u64,
  overflow: bool,
  large_value: bool,
  compression: CompressionType,
  valid: bool,
}

impl<'a> DataPageIter<'a> {
  pub fn new(page: &'a [u8]) -> Result<Self> {
    if page_type(page)? != PageType::Data {
      return Err(KvError::Corrupted);
    }
    let restart_offset = data_restart_offset(page)?;
    if restart_offset < DATA_HEADER_SIZE || restart_offset > page.len() {
      return Err(KvError::Corrupted);
    }
    let iter = Self {
      page,
      restart_num: restart_num(page)?,
      restart_offset,
      curr_offset: DATA_HEADER_SIZE,
      curr_restart_idx: 0,
      key: Vec::new(),
      value_range: (0, 0),
      timestamp: 0,
      expire_ts: 0,
      overflow: false,
      large_value: false,
      compression: CompressionType::None,
      valid: false,
    };
    restart::validate_offsets(page, DATA_HEADER_SIZE, restart_offset, iter.restart_num)?;
    restart::validate_order(&iter)?;
    Ok(iter)
  }

  pub fn key(&self) -> &[u8] {
    &self.key
  }

  pub fn value(&self) -> &[u8] {
    &self.page[self.value_range.0..self.value_range.1]
  }

  /// 当前 entry value 在页内的字节区间；写路径回填 overflow 指针用。
  pub fn value_range(&self) -> (usize, usize) {
    self.value_range
  }

  pub fn timestamp(&self) -> u64 {
    self.timestamp
  }

  pub fn expire_ts(&self) -> u64 {
    self.expire_ts
  }

  pub fn is_overflow(&self) -> bool {
    self.overflow
  }

  pub fn is_large_value(&self) -> bool {
    self.large_value
  }

  pub fn compression(&self) -> CompressionType {
    self.compression
  }

  pub fn valid(&self) -> bool {
    self.valid
  }

  pub fn advance(&mut self) -> Result<bool> {
    if self.curr_offset >= self.restart_offset {
      self.valid = false;
      return Ok(false);
    }
    let is_restart = self.curr_offset == self.restart_offset_at(self.curr_restart_idx)?;
    let (shared, non_shared, value_len, flags, mut pos) =
      decode_entry(self.page, self.curr_offset, self.restart_offset)?;
    let key_start = pos;
    pos = pos.checked_add(non_shared).ok_or(KvError::Corrupted)?;
    let value_start = pos;
    pos = pos.checked_add(value_len).ok_or(KvError::Corrupted)?;
    if pos > self.restart_offset {
      return Err(KvError::Corrupted);
    }
    if shared > self.key.len() {
      return Err(KvError::Corrupted);
    }
    self.key.truncate(shared);
    self.key.extend_from_slice(
      self
        .page
        .get(key_start..key_start + non_shared)
        .ok_or(KvError::Corrupted)?,
    );
    self.value_range = (value_start, value_start + value_len);
    self.overflow = flags & 1 != 0;
    self.expire_ts = if flags & 2 != 0 {
      let (value, read) = get_varint64(&self.page[pos..self.restart_offset])?;
      pos += read;
      value
    } else {
      0
    };
    let (delta, read) = get_varint64(&self.page[pos..self.restart_offset])?;
    pos += read;
    let delta = decode_i64_delta(delta);
    self.timestamp = if is_restart {
      delta as u64
    } else {
      (self.timestamp as i64).wrapping_add(delta) as u64
    };
    self.compression = if flags >> 2 == 0b11 {
      self.large_value = true;
      CompressionType::None
    } else {
      self.large_value = false;
      CompressionType::from_bits(flags >> 2)?
    };
    self.curr_offset = pos;
    if self.curr_restart_idx + 1 < self.restart_num
      && pos >= self.restart_offset_at(self.curr_restart_idx + 1)?
    {
      self.curr_restart_idx += 1;
    }
    self.valid = true;
    Ok(true)
  }

  /// 计数专用推进：只解析到「下一个 entry 的起点」，不拼 key、不累计时间戳。
  ///
  /// 全表计数（`len()`）每页几十条，逐条 `advance` 会把 key 复制进 `self.key`
  /// 再丢弃，25M 行就是 25M 次无谓的 memcpy + Vec 截断。这里保留解码合法性检查
  /// （长度 / 越界 / restart 推进）与 `expire_ts`（计数要过滤过期条目），只跳过
  /// key 物化与 timestamp 累加，遍历顺序与计数结果与 [`Self::advance`] 完全一致。
  pub fn advance_skip_key(&mut self) -> Result<bool> {
    if self.curr_offset >= self.restart_offset {
      self.valid = false;
      return Ok(false);
    }
    let is_restart = self.curr_offset == self.restart_offset_at(self.curr_restart_idx)?;
    let (shared, non_shared, value_len, flags, mut pos) =
      decode_entry(self.page, self.curr_offset, self.restart_offset)?;
    let _ = shared;
    pos = pos.checked_add(non_shared).ok_or(KvError::Corrupted)?;
    pos = pos.checked_add(value_len).ok_or(KvError::Corrupted)?;
    if pos > self.restart_offset {
      return Err(KvError::Corrupted);
    }
    self.overflow = flags & 1 != 0;
    self.expire_ts = if flags & 2 != 0 {
      let (value, read) = get_varint64(&self.page[pos..self.restart_offset])?;
      pos += read;
      value
    } else {
      0
    };
    // timestamp 只需要跳过：计数不看它，restart 点与增量编码对两种推进等价。
    let (_delta, read) = get_varint64(&self.page[pos..self.restart_offset])?;
    pos += read;
    let _ = is_restart;
    self.compression = if flags >> 2 == 0b11 {
      self.large_value = true;
      CompressionType::None
    } else {
      self.large_value = false;
      CompressionType::from_bits(flags >> 2)?
    };
    self.curr_offset = pos;
    if self.curr_restart_idx + 1 < self.restart_num
      && pos >= self.restart_offset_at(self.curr_restart_idx + 1)?
    {
      self.curr_restart_idx += 1;
    }
    self.valid = true;
    Ok(true)
  }

  pub fn reset(&mut self) {
    self.curr_offset = DATA_HEADER_SIZE;
    self.curr_restart_idx = 0;
    self.key.clear();
    self.value_range = (0, 0);
    self.timestamp = 0;
    self.expire_ts = 0;
    self.overflow = false;
    self.large_value = false;
    self.compression = CompressionType::None;
    self.valid = false;
  }

  /// 第一个 key `>= target` 的 entry：restart 二分 + 区间内线性。
  pub fn seek(&mut self, target: &[u8]) -> Result<bool> {
    restart::seek(self, target)
  }

  /// 最后一个 key `<= target` 的 entry：restart 二分 + 单遍推进。
  ///
  /// 推进前先窥探下一 entry 的 key（前缀 + 页内后缀零拷贝比较），超出即停，
  /// 迭代器天然停在 floor entry 上，无需回退或快照。
  pub fn seek_floor(&mut self, target: &[u8]) -> Result<bool> {
    restart::seek_floor(self, target)
  }

  /// entry 的 key 区间：`(shared, key_start, key_end)`，已做越界检查。
  fn key_span(&self, offset: usize) -> Result<(usize, usize, usize)> {
    let (shared, non_shared, _, _, key_start) =
      decode_entry(self.page, offset, self.restart_offset)?;
    let key_end = key_start + non_shared;
    if key_end > self.restart_offset {
      return Err(KvError::Corrupted);
    }
    Ok((shared, key_start, key_end))
  }

  fn restart_offset_at(&self, index: u16) -> Result<usize> {
    restart::offset_at(self.page, self.restart_offset, index)
  }
}

impl restart::Cursor for DataPageIter<'_> {
  fn restart_cnt(&self) -> u16 {
    self.restart_num
  }

  fn no_entry(&self) -> bool {
    self.restart_num == 0 || self.restart_offset <= DATA_HEADER_SIZE
  }

  fn restart_key(&self, idx: u16) -> Result<&[u8]> {
    let (shared, start, end) = self.key_span(self.restart_offset_at(idx)?)?;
    if shared != 0 {
      return Err(KvError::Corrupted);
    }
    Ok(&self.page[start..end])
  }

  fn seek_to_restart(&mut self, idx: u16) -> Result<()> {
    // restart 点 shared=0、timestamp 为绝对值（非 delta），清空前缀与 ts 基准即可独立解码。
    self.reset();
    self.curr_offset = self.restart_offset_at(idx)?;
    self.curr_restart_idx = idx;
    Ok(())
  }

  fn step(&mut self) -> Result<bool> {
    self.advance()
  }

  fn cur_key(&self) -> &[u8] {
    &self.key
  }

  fn invalidate(&mut self) {
    self.curr_offset = self.restart_offset;
    self.valid = false;
  }

  fn peek_cmp(&self, target: &[u8]) -> Result<Option<Ordering>> {
    if self.curr_offset >= self.restart_offset {
      return Ok(None);
    }
    let (shared, start, end) = self.key_span(self.curr_offset)?;
    let head = self.key.get(..shared).ok_or(KvError::Corrupted)?;
    Ok(Some(cmp_split(head, &self.page[start..end], target)))
  }
}

fn decode_entry(
  page: &[u8],
  offset: usize,
  limit: usize,
) -> Result<(usize, usize, usize, u8, usize)> {
  let bytes = page.get(offset..limit).ok_or(KvError::EndOfFile)?;
  if bytes.len() < 3 {
    return Err(KvError::EndOfFile);
  }
  let (shared, non_shared, stored_val_len, pos) =
    if bytes[0] < 0x80 && bytes[1] < 0x80 && bytes[2] < 0x80 {
      (
        usize::from(bytes[0]),
        usize::from(bytes[1]),
        u32::from(bytes[2]),
        offset + 3,
      )
    } else {
      let (shared, r1) = get_varint32(bytes)?;
      let (non_shared, r2) = get_varint32(&bytes[r1..])?;
      let (stored_val_len, r3) = get_varint32(&bytes[r1 + r2..])?;
      (
        shared as usize,
        non_shared as usize,
        stored_val_len,
        offset + r1 + r2 + r3,
      )
    };
  let flags = (stored_val_len & 0xf) as u8;
  let value_len = (stored_val_len >> VAL_LEN_BITS) as usize;
  Ok((shared, non_shared, value_len, flags, pos))
}

pub fn set_prev_page_id(page: &mut [u8], page_id: u32) -> Result<()> {
  put_fixed32(
    page
      .get_mut(DATA_PREV_PAGE_OFFSET..)
      .ok_or(KvError::EndOfFile)?,
    page_id,
  )
  .ok_or(KvError::EndOfFile)?;
  Ok(())
}

pub fn set_next_page_id(page: &mut [u8], page_id: u32) -> Result<()> {
  put_fixed32(
    page
      .get_mut(DATA_NEXT_PAGE_OFFSET..)
      .ok_or(KvError::EndOfFile)?,
    page_id,
  )
  .ok_or(KvError::EndOfFile)?;
  Ok(())
}
