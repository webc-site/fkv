use fkv_codec::{
  append_varint64, checksum, get_fixed32, get_fixed64, get_varint32, get_varint64, put_fixed32,
  put_fixed64, put_varint64, varint64_len,
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
      fkv_codec::decode_i64_delta(fkv_codec::encode_i64_delta(delta)),
      delta
    );
  }
  for delta in [0i32, 1, -1, 12345, -12345, i32::MAX, i32::MIN + 1] {
    assert_eq!(
      fkv_codec::decode_i32_delta(fkv_codec::encode_i32_delta(delta)),
      delta
    );
  }
}
