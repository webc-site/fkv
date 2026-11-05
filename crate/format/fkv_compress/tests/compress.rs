use std::borrow::Cow;

use fkv_compress::{
  CompressionType, DictCompression, SKIP_COMPRESSION_THRESHOLD, compress_raw, decompress_raw,
};
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

/// 解压长度与前缀不一致必须报错，不能静默返回短数据。
#[test]
fn length_prefix_mismatch_is_rejected() {
  let input = vec![7u8; 4096];
  let compressed = compress_raw(&input, 3).unwrap().unwrap();
  let (orig_len, body_off) = fkv_codec::get_varint32(&compressed).unwrap();
  assert_eq!(orig_len as usize, input.len());

  let mut wrong = Vec::new();
  fkv_codec::append_varint32(&mut wrong, orig_len + 1);
  wrong.extend_from_slice(&compressed[body_off..]);
  assert!(decompress_raw(&wrong).is_err());

  // 前缀被截断 / 空输入。
  assert!(decompress_raw(&compressed[..1]).is_err());
  assert!(decompress_raw(&[]).is_err());
}

/// 帧头声明的 content size 远大于前缀长度时必须报错，不得按帧头放大分配。
#[test]
fn decompress_caps_output_at_prefix_len() {
  let input = vec![7u8; 1 << 20];
  let frame = compress(&input, 3).unwrap();
  let mut payload = Vec::new();
  // 只声明 16 字节，但帧实际展开 1 MiB。
  fkv_codec::append_varint32(&mut payload, 16);
  payload.extend_from_slice(&frame);
  assert!(decompress_raw(&payload).is_err());
}

/// 未压缩必须借用原值，不额外拷贝一份。
#[test]
fn prepare_short_value_is_uncompressed() {
  let dict = DictCompression::new(3);
  let prepared = dict.prepare(b"tiny");
  assert_eq!(prepared.kind, CompressionType::None);
  assert!(matches!(prepared.data, Cow::Borrowed(_)));
  assert_eq!(prepared.data.as_ref(), b"tiny");
}

/// 阈值边界：低于 `SKIP_COMPRESSION_THRESHOLD` 不压；超出字典上限的值走 standalone。
#[test]
fn prepare_threshold_boundary() {
  let dict = DictCompression::new(3);
  let below = vec![b'a'; SKIP_COMPRESSION_THRESHOLD - 1];
  assert_eq!(dict.prepare(&below).kind, CompressionType::None);
  // 无字典时中间区间不压（对齐 C++ Prepare），所以恰好等于阈值也只能透传。
  let at_threshold = vec![b'a'; SKIP_COMPRESSION_THRESHOLD];
  let at = dict.prepare(&at_threshold);
  assert_eq!(at.kind, CompressionType::None);
  assert!(matches!(at.data, Cow::Borrowed(_)));

  let big = vec![b'b'; fkv_compress::STANDALONE_COMPRESSION_THRESHOLD + 1];
  let prepared = dict.prepare(&big);
  assert_eq!(prepared.kind, CompressionType::Standalone);
  assert!(prepared.data.len() < big.len());
  assert_eq!(decompress_raw(&prepared.data).unwrap(), big);
}

/// 采纳压缩后落盘字节必须真的比原值小（长度前缀也算在内）。
#[test]
fn adopted_compression_is_always_smaller() {
  for len in [SKIP_COMPRESSION_THRESHOLD, 128, 1000, 10_000] {
    let value: Vec<u8> = (0..len as u32)
      .map(|idx| (idx.wrapping_mul(2_654_435_761) >> 24) as u8)
      .collect();
    let prepared = DictCompression::new(3).prepare(&value);
    if prepared.kind != CompressionType::None {
      assert!(
        prepared.data.len() < value.len(),
        "len={len} 压缩后反而更大"
      );
      assert_eq!(decompress_raw(&prepared.data).unwrap(), value);
    }
  }
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

/// 训练（无论成败）后必须释放样本，且不再重复累积 / 训练。
#[test]
fn dictionary_training_does_not_retry_or_keep_samples() {
  let mut dict = DictCompression::new(3);
  let sample = vec![b'y'; 16 * 1024];
  let sample_li: Vec<&[u8]> = (0..200).map(|_| sample.as_slice()).collect();
  dict.maybe_build_dictionary(&sample_li).unwrap();
  assert_eq!(dict.sample_count(), 0);
  let before = dict.estimated_memory_bytes();
  dict.maybe_build_dictionary(&sample_li).unwrap();
  assert_eq!(dict.sample_count(), 0);
  assert_eq!(dict.estimated_memory_bytes(), before);
}
