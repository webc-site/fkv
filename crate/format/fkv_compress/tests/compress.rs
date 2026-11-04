use fkv_compress::{CompressionType, DictCompression, compress_raw, decompress_raw};
use zstd::bulk::{compress, decompress};

#[test]
fn raw_roundtrip() {
  let input = vec![7u8; 4096];
  let compressed = compress_raw(&input, 3).unwrap().unwrap();
  assert!(compressed.len() < input.len());
  assert_eq!(decompress_raw(&compressed).unwrap(), input);
}

#[test]
fn dictionary_roundtrip() {
  let sample = vec![b'x'; 1024];
  let sample_li: Vec<&[u8]> = (0..64).map(|_| sample.as_slice()).collect();
  let mut dict = DictCompression::new(3);
  dict.maybe_build_dictionary(&sample_li).unwrap();
  if !dict.has_dictionary() {
    return;
  }
  let input = vec![b'x'; 2048];
  let compressed = dict.compress(&input).unwrap();
  if let Some(compressed) = compressed {
    assert_eq!(dict.decompress(&compressed).unwrap(), input);
  }
}

#[test]
fn prepare_short_value_is_uncompressed() {
  let dict = DictCompression::new(3);
  let prepared = dict.prepare(b"tiny");
  assert_eq!(prepared.kind, CompressionType::None);
  assert_eq!(prepared.data, b"tiny");
}

/// mzstd 与 zstd(C) 字节层互操作：两边写的帧互相能解。
#[test]
fn mzstd_and_zstd_interoperate() {
  // 结构化 + 随机混合，覆盖字面量/匹配/熵表。
  let mut input = Vec::new();
  for i in 0..4000u32 {
    input.extend_from_slice(format!("user:{}\tcity:Oslo\tactive:true\n", i % 97).as_bytes());
  }
  let ours = compress_raw(&input, 3).unwrap().unwrap();
  let (orig_len, body_off) = fkv_codec::get_varint32(&ours).unwrap();
  assert_eq!(orig_len as usize, input.len());

  // Rust 写 → C++ zstd 读。
  let decoded = decompress(&ours[body_off..], input.len()).unwrap();
  assert_eq!(decoded, input);

  // C++ zstd 写 → Rust 读（同样的 varint 前缀格式）。
  let c_frame = compress(&input, 3).unwrap();
  let mut with_prefix = Vec::new();
  fkv_codec::append_varint32(&mut with_prefix, input.len() as u32);
  with_prefix.extend_from_slice(&c_frame);
  assert_eq!(decompress_raw(&with_prefix).unwrap(), input);
}
