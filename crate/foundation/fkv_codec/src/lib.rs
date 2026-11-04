//! 定长 / varint / XXH3 编解码。
//!
//! 与 eloqstore `include/coding.h` 保持同一套 LEB128 varint 与 LE 定长格式。

use fkv_error::{KvError, Result};
use xxhash_rust::xxh3::xxh3_64;

pub const CHECKSUM_BYTES: usize = 8;

pub fn put_fixed16(dst: &mut [u8], value: u16) -> Option<usize> {
  let out = dst.get_mut(..2)?;
  out.copy_from_slice(&value.to_le_bytes());
  Some(2)
}

pub fn get_fixed16(src: &[u8]) -> Result<u16> {
  let bytes: [u8; 2] = src
    .get(..2)
    .ok_or(KvError::EndOfFile)?
    .try_into()
    .map_err(|_| KvError::EndOfFile)?;
  Ok(u16::from_le_bytes(bytes))
}

pub fn put_fixed32(dst: &mut [u8], value: u32) -> Option<usize> {
  let out = dst.get_mut(..4)?;
  out.copy_from_slice(&value.to_le_bytes());
  Some(4)
}

pub fn put_fixed64(dst: &mut [u8], value: u64) -> Option<usize> {
  let out = dst.get_mut(..8)?;
  out.copy_from_slice(&value.to_le_bytes());
  Some(8)
}

pub fn get_fixed32(src: &[u8]) -> Result<u32> {
  let bytes: [u8; 4] = src
    .get(..4)
    .ok_or(KvError::EndOfFile)?
    .try_into()
    .map_err(|_| KvError::EndOfFile)?;
  Ok(u32::from_le_bytes(bytes))
}

pub fn get_fixed64(src: &[u8]) -> Result<u64> {
  let bytes: [u8; 8] = src
    .get(..8)
    .ok_or(KvError::EndOfFile)?
    .try_into()
    .map_err(|_| KvError::EndOfFile)?;
  Ok(u64::from_le_bytes(bytes))
}

pub const fn varint64_len(mut value: u64) -> usize {
  let mut len = 1;
  while value >= 0x80 {
    value >>= 7;
    len += 1;
  }
  len
}

pub const fn varint32_len(value: u32) -> usize {
  varint64_len(value as u64)
}

pub fn put_varint64(dst: &mut [u8], mut value: u64) -> Option<usize> {
  let mut pos = 0;
  while value >= 0x80 {
    *dst.get_mut(pos)? = (value as u8) | 0x80;
    value >>= 7;
    pos += 1;
  }
  *dst.get_mut(pos)? = value as u8;
  Some(pos + 1)
}

pub fn put_varint32(dst: &mut [u8], value: u32) -> Option<usize> {
  put_varint64(dst, value as u64)
}

pub fn append_varint64(out: &mut Vec<u8>, mut value: u64) {
  while value >= 0x80 {
    out.push((value as u8) | 0x80);
    value >>= 7;
  }
  out.push(value as u8);
}

pub fn append_varint32(out: &mut Vec<u8>, value: u32) {
  append_varint64(out, value as u64);
}

pub fn get_varint64(src: &[u8]) -> Result<(u64, usize)> {
  let mut result = 0u64;
  for (idx, byte) in src.iter().copied().take(10).enumerate() {
    if idx == 9 && byte > 1 {
      return Err(KvError::Corrupted);
    }
    result |= u64::from(byte & 0x7f) << (idx * 7);
    if byte < 0x80 {
      return Ok((result, idx + 1));
    }
  }
  if src.len() < 10 {
    Err(KvError::EndOfFile)
  } else {
    Err(KvError::Corrupted)
  }
}

pub fn get_varint32(src: &[u8]) -> Result<(u32, usize)> {
  let (value, read) = get_varint64(src)?;
  u32::try_from(value)
    .map(|value| (value, read))
    .map_err(|_| KvError::Corrupted)
}

/// 与 `XXH3_64bits` 一致。
pub fn checksum(bytes: &[u8]) -> u64 {
  xxh3_64(bytes)
}

/// 页 / manifest 的 checksum 覆盖范围：跳过前 8 字节 checksum。
pub fn checksum_excluding_prefix(bytes: &[u8]) -> u64 {
  checksum(bytes.get(CHECKSUM_BYTES..).unwrap_or_default())
}

/// 与 `EncodeInt64Delta` / `DecodeInt64Delta` 一致：最低位是符号位。
pub const fn encode_i64_delta(delta: i64) -> u64 {
  if delta >= 0 {
    (delta as u64) << 1
  } else {
    ((-delta) as u64) << 1 | 1
  }
}

pub const fn decode_i64_delta(value: u64) -> i64 {
  if value & 1 == 1 {
    -((value >> 1) as i64)
  } else {
    (value >> 1) as i64
  }
}

pub const fn encode_i32_delta(delta: i32) -> u32 {
  if delta >= 0 {
    (delta as u32) << 1
  } else {
    ((-delta) as u32) << 1 | 1
  }
}

pub const fn decode_i32_delta(value: u32) -> i32 {
  if value & 1 == 1 {
    -((value >> 1) as i32)
  } else {
    (value >> 1) as i32
  }
}
