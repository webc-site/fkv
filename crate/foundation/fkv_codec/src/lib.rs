//! 定长 / varint / XXH3 编解码。
//!
//! LEB128 varint 与 LE 定长格式。

use fkv_error::{KvError, Result};
use xxhash_rust::xxh3::xxh3_64;

pub const CHECKSUM_BYTES: usize = 8;

pub fn put_fixed16(dst: &mut [u8], value: u16) -> Option<usize> {
  let out = dst.get_mut(..2)?;
  out.copy_from_slice(&value.to_le_bytes());
  Some(2)
}

pub fn get_fixed16(src: &[u8]) -> Result<u16> {
  let bytes = src.first_chunk::<2>().ok_or(KvError::EndOfFile)?;
  Ok(u16::from_le_bytes(*bytes))
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
  let bytes = src.first_chunk::<4>().ok_or(KvError::EndOfFile)?;
  Ok(u32::from_le_bytes(*bytes))
}

pub fn get_fixed64(src: &[u8]) -> Result<u64> {
  let bytes = src.first_chunk::<8>().ok_or(KvError::EndOfFile)?;
  Ok(u64::from_le_bytes(*bytes))
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

/// 解码 64 位 LEB128。
///
/// 比 C++ `GetVarint64Ptr` 更严格：拒绝 10 字节 varint 第 10 字节 > 1
/// 的溢出编码、以及超出 u32 的 varint32，返回 `Corrupted` 而不是静默截断。
/// 写侧 encoder 恒产出规范编码，正常盘面不受影响。
/// 解码 64 位 LEB128。
///
/// 点读每页每次查找要解几十个 varint（entry 头 + timestamp），所以前两个字节
/// 走无循环快路径：绝大多数 key 长度、page_id delta 与 timestamp 都落在 1~2
/// 字节内（与 C++ `GetVarint64Ptr` 的展开写法同构）。严格性不变：第 10 字节
/// > 1 仍按溢出拒绝。
#[inline]
pub fn get_varint64(src: &[u8]) -> Result<(u64, usize)> {
  let b0 = *src.first().ok_or(KvError::EndOfFile)?;
  let mut result = u64::from(b0 & 0x7f);
  if b0 < 0x80 {
    return Ok((result, 1));
  }
  let b1 = *src.get(1).ok_or(KvError::EndOfFile)?;
  result |= u64::from(b1 & 0x7f) << 7;
  if b1 < 0x80 {
    return Ok((result, 2));
  }
  let mut shift = 14;
  for idx in 2..10 {
    let byte = match src.get(idx) {
      Some(byte) => *byte,
      None => return Err(KvError::EndOfFile),
    };
    if idx == 9 && byte > 1 {
      return Err(KvError::Corrupted);
    }
    result |= u64::from(byte & 0x7f) << shift;
    if byte < 0x80 {
      return Ok((result, idx + 1));
    }
    shift += 7;
  }
  Err(KvError::Corrupted)
}

/// 解码 32 位 LEB128。
///
/// 独立实现而不是转调 `get_varint64`：后者的 10 字节循环对 32 位值多余，且
/// 每次 entry 头要解 3 个。语义保持严格——超过 5 字节、或第 5 字节高 4 位非零
/// （即溢出 u32）一律 `Corrupted`。
#[inline]
pub fn get_varint32(src: &[u8]) -> Result<(u32, usize)> {
  let b0 = *src.first().ok_or(KvError::EndOfFile)?;
  if b0 < 0x80 {
    return Ok((u32::from(b0), 1));
  }
  let b1 = *src.get(1).ok_or(KvError::EndOfFile)?;
  let mut value = u32::from(b0 & 0x7f) | (u32::from(b1 & 0x7f) << 7);
  if b1 < 0x80 {
    return Ok((value, 2));
  }
  let b2 = *src.get(2).ok_or(KvError::EndOfFile)?;
  value |= u32::from(b2 & 0x7f) << 14;
  if b2 < 0x80 {
    return Ok((value, 3));
  }
  let b3 = *src.get(3).ok_or(KvError::EndOfFile)?;
  value |= u32::from(b3 & 0x7f) << 21;
  if b3 < 0x80 {
    return Ok((value, 4));
  }
  let b4 = *src.get(4).ok_or(KvError::EndOfFile)?;
  if b4 > 0x0f {
    return Err(KvError::Corrupted);
  }
  value |= u32::from(b4) << 28;
  if b4 < 0x80 {
    return Ok((value, 5));
  }
  Err(KvError::Corrupted)
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
///
/// `i64::MIN` 取负会溢出，无法 zigzag 编码，返回 `None`（调用方按参数错误处理）。
pub const fn encode_i64_delta(delta: i64) -> Option<u64> {
  match delta.checked_abs() {
    Some(abs) => Some(((abs as u64) << 1) | (delta < 0) as u64),
    None => None,
  }
}

pub const fn decode_i64_delta(value: u64) -> i64 {
  if value & 1 == 1 {
    -((value >> 1) as i64)
  } else {
    (value >> 1) as i64
  }
}

/// `i32::MIN` 取负会溢出，无法 zigzag 编码，返回 `None`（调用方按参数错误处理）。
pub const fn encode_i32_delta(delta: i32) -> Option<u32> {
  match delta.checked_abs() {
    Some(abs) => Some(((abs as u32) << 1) | (delta < 0) as u32),
    None => None,
  }
}

pub const fn decode_i32_delta(value: u32) -> i32 {
  if value & 1 == 1 {
    -((value >> 1) as i32)
  } else {
    (value >> 1) as i32
  }
}
