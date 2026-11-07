use std::str::from_utf8;

use fkv_codec::{
  append_varint32, append_varint64, checksum, get_fixed32, get_fixed64, get_varint32, get_varint64,
  put_fixed16, put_fixed32, put_fixed64, put_varint32, put_varint64, varint32_len, varint64_len,
};
use fkv_error::KvError;

#[test]
fn fixed_roundtrip() {
  let mut buf = [0u8; 8];
  assert_eq!(put_fixed32(&mut buf, 0xdead_beef), Some(4));
  assert_eq!(get_fixed32(&buf).unwrap(), 0xdead_beef);

  assert_eq!(put_fixed64(&mut buf, 0x0123_4567_89ab_cdef), Some(8));
  assert_eq!(get_fixed64(&buf).unwrap(), 0x0123_4567_89ab_cdef);
}

#[test]
fn fixed_truncated() {
  assert_eq!(get_fixed32(&[1, 2, 3]).unwrap_err(), KvError::EndOfFile);
  assert_eq!(get_fixed64(&[1, 2, 3]).unwrap_err(), KvError::EndOfFile);
}

#[test]
fn varint_roundtrip() {
  let value_li = [
    0u64,
    1,
    127,
    128,
    300,
    16_384,
    u64::from(u32::MAX),
    u64::MAX,
  ];
  for value in value_li {
    let mut buf = [0u8; 10];
    let len = put_varint64(&mut buf, value).unwrap();
    assert_eq!(len, varint64_len(value));
    let (decoded, read) = get_varint64(&buf[..len]).unwrap();
    assert_eq!(decoded, value);
    assert_eq!(read, len);

    let mut out = Vec::new();
    append_varint64(&mut out, value);
    assert_eq!(&out[..], &buf[..len]);
  }
}

#[test]
fn varint_truncated_and_overflow() {
  assert_eq!(get_varint64(&[0x80]).unwrap_err(), KvError::EndOfFile);
  assert_eq!(get_varint64(&[0xff; 10]).unwrap_err(), KvError::Corrupted);
  assert_eq!(
    get_varint32(&[0xff, 0xff, 0xff, 0xff, 0x1f])
      .unwrap_err()
      .to_string(),
    "Disk data corrupted"
  );
}

#[test]
fn checksum_is_stable() {
  assert_eq!(checksum(b""), checksum(&[]));
  assert_eq!(checksum(b"fkv"), checksum(b"fkv"));
  assert_ne!(checksum(b"fkv"), checksum(b"fkw"));
}

#[test]
fn delta_roundtrip() {
  for delta in [0i64, 1, -1, 12345, -12345, i32::MAX as i64, i32::MIN as i64] {
    assert_eq!(
      fkv_codec::decode_i64_delta(fkv_codec::encode_i64_delta(delta).unwrap()),
      delta
    );
  }
  for delta in [0i32, 1, -1, 12345, -12345, i32::MAX, i32::MIN + 1] {
    assert_eq!(
      fkv_codec::decode_i32_delta(fkv_codec::encode_i32_delta(delta).unwrap()),
      delta
    );
  }
}

/// C++ oracle 逐字节向量：直接链接 eloqstore `src/coding.cpp` 的驱动产出。
///
/// 覆盖 `EncodeVarint32/64`、`Varint32Size` / `Varint64Size`、`PutFixed*`
/// 的边界值编码，以及 `GetVarint32/64` 对规范 / 非规范输入的裁决。
#[test]
fn cpp_oracle_encoding_vectors() {
  const V32: &[(u32, &str)] = &[
    (0, "00"),
    (1, "01"),
    (127, "7f"),
    (128, "8001"),
    (16_383, "ff7f"),
    (16_384, "808001"),
    (2_097_151, "ffff7f"),
    (2_097_152, "80808001"),
    (268_435_455, "ffffff7f"),
    (268_435_456, "8080808001"),
    (u32::MAX, "ffffffff0f"),
  ];
  for &(value, want) in V32 {
    let mut buf = [0u8; 10];
    let len = put_varint32(&mut buf, value).unwrap();
    assert_eq!(&buf[..len], hex(want).as_slice(), "varint32 {value}");
    assert_eq!(len, varint32_len(value), "Varint32Size({value})");
  }

  const V64: &[(u64, &str)] = &[
    (0, "00"),
    (127, "7f"),
    (128, "8001"),
    (16_383, "ff7f"),
    (16_384, "808001"),
    (2_097_151, "ffff7f"),
    (2_097_152, "80808001"),
    (268_435_455, "ffffff7f"),
    (268_435_456, "8080808001"),
    (34_359_738_367, "ffffffff7f"),
    (34_359_738_368, "808080808001"),
    (4_398_046_511_103, "ffffffffff7f"),
    (4_398_046_511_104, "80808080808001"),
    (562_949_953_421_311, "ffffffffffff7f"),
    (562_949_953_421_312, "8080808080808001"),
    (72_057_594_037_927_935, "ffffffffffffff7f"),
    (72_057_594_037_927_936, "808080808080808001"),
    (9_223_372_036_854_775_808, "80808080808080808001"),
    (u64::MAX, "ffffffffffffffffff01"),
  ];
  for &(value, want) in V64 {
    let mut buf = [0u8; 10];
    let len = put_varint64(&mut buf, value).unwrap();
    assert_eq!(&buf[..len], hex(want).as_slice(), "varint64 {value}");
    assert_eq!(len, varint64_len(value), "Varint64Size({value})");
  }

  let mut buf = [0u8; 8];
  assert_eq!(put_fixed16(&mut buf, 0x1234), Some(2));
  assert_eq!(&buf[..2], hex("3412").as_slice());
  assert_eq!(put_fixed32(&mut buf, 0x89ab_cdef), Some(4));
  assert_eq!(&buf[..4], hex("efcdab89").as_slice());
  assert_eq!(put_fixed64(&mut buf, 0x0123_4567_89ab_cdef), Some(8));
  assert_eq!(&buf[..8], hex("efcdab8967452301").as_slice());

  // `PutVarint32` + `PutVarint64` + `PutFixed*` 的追加序列。
  let mut appended = Vec::new();
  append_varint32(&mut appended, 300);
  append_varint64(&mut appended, 300);
  let n = put_fixed16(&mut buf, 0x1234).unwrap();
  appended.extend_from_slice(&buf[..n]);
  let n = put_fixed32(&mut buf, 0x89ab_cdef).unwrap();
  appended.extend_from_slice(&buf[..n]);
  let n = put_fixed64(&mut buf, 0x0123_4567_89ab_cdef).unwrap();
  appended.extend_from_slice(&buf[..n]);
  assert_eq!(appended, hex("ac02ac023412efcdab89efcdab8967452301"));
}

/// C++ `GetVarint32/64` 对非规范编码静默截断；Rust 按文档拒绝（更严格）。
#[test]
fn cpp_oracle_decode_vectors() {
  assert_eq!(get_varint32(&hex("ffffffff0f")).unwrap(), (u32::MAX, 5));
  assert_eq!(
    get_varint64(&hex("ffffffff1f")).unwrap(),
    (8_589_934_591, 5)
  );
  assert_eq!(
    get_varint32(&hex("ffffffff1f")).unwrap_err(),
    KvError::Corrupted
  );
  assert_eq!(
    get_varint64(&hex("ffffffffffffffffff01")).unwrap(),
    (u64::MAX, 10)
  );
  assert_eq!(
    get_varint64(&hex("ffffffffffffffffff7f")).unwrap_err(),
    KvError::Corrupted
  );
  assert_eq!(get_varint32(&hex("ac02")).unwrap(), (300, 2));
  assert_eq!(get_varint64(&hex("80")).unwrap_err(), KvError::EndOfFile);
}

fn hex(text: &str) -> Vec<u8> {
  text
    .as_bytes()
    .as_chunks::<2>()
    .0
    .iter()
    .map(|pair| u8::from_str_radix(from_utf8(pair).unwrap(), 16).unwrap())
    .collect()
}
