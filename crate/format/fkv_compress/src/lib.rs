//! zstd 字典 / 单值压缩，格式对齐 eloqstore `compression.cpp`：
//! `varint32(original_len) | zstd_frame`。
//!
//! 编解码走 `mzstd`（纯安全 Rust、与 zstd 1.5.7 字节一致）；字典训练仍用
//! `zstd` 的 `ZDICT_trainFromBuffer`（mzstd 无等价能力）。

use std::{cell::RefCell, fmt};

use fkv_codec::{get_varint32, put_varint32, varint32_len};
use fkv_error::{KvError, Result};
use mzstd::{
  Compressor, Decompressor, EncoderConfig, Level,
  dictionary::{DictionaryBuilder, PreparedDictionary},
};
use zstd::dict::from_continuous;

pub const SKIP_COMPRESSION_THRESHOLD: usize = 100;
pub const STANDALONE_COMPRESSION_THRESHOLD: usize = 10 * 1024 * 1024;
pub const MAX_DICT_BYTES: usize = 16 * 1024;
pub const SAMPLE_TARGET_BYTES: usize = 100 * MAX_DICT_BYTES;
pub const MIN_SAMPLES: usize = 5;

thread_local! {
  /// 对齐 C++ `kCompressCtx`：每线程复用同一 CCtx，避免每次建上下文。
  static RAW_COMPRESS: RefCell<Option<(i32, Compressor)>> = const { RefCell::new(None) };
  /// 对齐 C++ `kDecompressCtx`。
  static RAW_DECOMPRESS: RefCell<Option<Decompressor>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompressionType {
  None = 0,
  Dictionary = 1,
  Standalone = 2,
}

#[derive(Debug, Clone)]
pub struct PreparedValue {
  pub data: Vec<u8>,
  pub kind: CompressionType,
}

/// 压缩级别转 mzstd `Level`；越界按默认级别处理（C++ level 0 也是默认）。
fn level_of(level: i32) -> Level {
  Level::try_from(level).unwrap_or(Level::DEFAULT)
}

/// 拼 `varint32(原始长度) | 压缩体`；压后不小于原值则放弃。
fn frame(input: &[u8], compressed: &[u8]) -> Option<Vec<u8>> {
  if compressed.is_empty() || compressed.len() >= input.len() {
    return None;
  }
  let prefix_len = varint32_len(input.len() as u32);
  let mut out = vec![0u8; prefix_len + compressed.len()];
  put_varint32(&mut out, input.len() as u32)?;
  out[prefix_len..].copy_from_slice(compressed);
  Some(out)
}

/// 无字典 raw 压缩，线程内复用 CCtx。压后不更小返回 `None`。
pub fn compress_raw(input: &[u8], level: i32) -> Result<Option<Vec<u8>>> {
  if input.is_empty() {
    return Ok(None);
  }
  let compressed = RAW_COMPRESS.with(|cell| -> Result<Vec<u8>> {
    let mut slot = cell.borrow_mut();
    if slot.is_none() {
      let config = EncoderConfig::new().with_level(level_of(level));
      *slot = Some((level, Compressor::new(config).map_err(|_| KvError::IoFail)?));
    }
    let (cur_level, compressor) = slot.as_mut().ok_or(KvError::IoFail)?;
    if *cur_level != level {
      let config = EncoderConfig::new().with_level(level_of(level));
      compressor
        .reconfigure(config)
        .map_err(|_| KvError::IoFail)?;
      *cur_level = level;
    }
    let mut out = Vec::new();
    compressor
      .compress_into(input, &mut out)
      .map_err(|_| KvError::IoFail)?;
    Ok(out)
  })?;
  Ok(frame(input, &compressed))
}

/// 无字典 raw 解压：读 varint 前缀长度，再用复用 DCtx 解码。
pub fn decompress_raw(input: &[u8]) -> Result<Vec<u8>> {
  let (original_len, read) = get_varint32(input)?;
  let body = input.get(read..).ok_or(KvError::EndOfFile)?;
  RAW_DECOMPRESS.with(|cell| {
    let mut slot = cell.borrow_mut();
    if slot.is_none() {
      *slot = Some(Decompressor::default());
    }
    let out = slot
      .as_mut()
      .ok_or(KvError::IoFail)?
      .decompress(body)
      .map_err(|_| KvError::Corrupted)?;
    if out.len() != original_len as usize {
      return Err(KvError::Corrupted);
    }
    Ok(out)
  })
}

/// 无字典写路径：`>= SKIP_COMPRESSION_THRESHOLD` 的值尝试 standalone zstd，
/// 只有压缩后更小才采用。格式与 C++ `CompressRaw` 一致。
pub fn prepare_standalone(value: &[u8], level: i32) -> PreparedValue {
  if value.len() >= SKIP_COMPRESSION_THRESHOLD
    && let Ok(Some(data)) = compress_raw(value, level)
  {
    return PreparedValue {
      data,
      kind: CompressionType::Standalone,
    };
  }
  PreparedValue {
    data: value.to_vec(),
    kind: CompressionType::None,
  }
}

#[derive(Default)]
pub struct DictCompression {
  dict: Vec<u8>,
  prepared: Option<PreparedDictionary>,
  dirty: bool,
  sample_data: Vec<u8>,
  sample_size_li: Vec<usize>,
  level: i32,
}

impl DictCompression {
  pub fn new(level: i32) -> Self {
    Self {
      level,
      ..Self::default()
    }
  }

  pub fn has_dictionary(&self) -> bool {
    self.prepared.is_some()
  }

  pub fn dictionary_bytes(&self) -> &[u8] {
    &self.dict
  }

  /// 加载已存在的字典；预构建 CDict/DDict，避免每次压缩重建。
  pub fn load_dictionary(&mut self, dict: Vec<u8>) -> Result<()> {
    if dict.is_empty() || dict.len() > MAX_DICT_BYTES {
      return Err(KvError::Corrupted);
    }
    let prepared = DictionaryBuilder::new(&dict)
      .with_config(EncoderConfig::new().with_level(level_of(self.level)))
      .build()
      .map_err(|_| KvError::Corrupted)?;
    self.dict = dict;
    self.prepared = Some(prepared);
    Ok(())
  }

  pub fn dirty(&self) -> bool {
    self.dirty
  }

  pub fn clear_dirty(&mut self) {
    self.dirty = false;
  }

  pub fn clear_samples(&mut self) {
    self.sample_data.clear();
    self.sample_size_li.clear();
  }

  pub fn add_sample(&mut self, sample: &[u8]) {
    self.sample_data.extend_from_slice(sample);
    self.sample_size_li.push(sample.len());
  }

  pub fn sample_count(&self) -> usize {
    self.sample_size_li.len()
  }

  /// 用累计样本训练字典（`ZDICT_trainFromBuffer` 语义）。
  pub fn build_dictionary(&mut self) -> Result<()> {
    if self.sample_size_li.len() < MIN_SAMPLES || self.sample_data.is_empty() {
      return Err(KvError::InvalidArgs);
    }
    let dict = from_continuous(&self.sample_data, &self.sample_size_li, MAX_DICT_BYTES)
      .map_err(|_| KvError::IoFail)?;
    self.clear_samples();
    if dict.is_empty() {
      return Err(KvError::IoFail);
    }
    self.load_dictionary(dict)?;
    self.dirty = true;
    Ok(())
  }

  /// 与 `SampleAndBuildDictionaryIfNeeded` 对齐：样本达到目标或补齐 MIN_SAMPLES 后训练。
  pub fn maybe_build_dictionary(&mut self, sample_li: &[&[u8]]) -> Result<()> {
    if self.has_dictionary() {
      return Ok(());
    }
    for sample in sample_li {
      if (SKIP_COMPRESSION_THRESHOLD..=STANDALONE_COMPRESSION_THRESHOLD).contains(&sample.len()) {
        self.add_sample(sample);
      }
    }
    if self.sample_data.len() >= SAMPLE_TARGET_BYTES && self.sample_count() >= MIN_SAMPLES {
      self.build_dictionary()?;
    }
    Ok(())
  }

  pub fn compress(&self, input: &[u8]) -> Result<Option<Vec<u8>>> {
    let Some(prepared) = &self.prepared else {
      return Ok(None);
    };
    if input.is_empty() {
      return Ok(None);
    }
    // 字典压缩复用函数式构造（mzstd 的 with_dictionary 需按字典绑定压缩器）。
    let mut compressor = Compressor::new(EncoderConfig::new().with_level(level_of(self.level)))
      .map_err(|_| KvError::IoFail)?;
    let mut out = Vec::new();
    compressor
      .compress_with_dictionary_into(prepared, input, &mut out)
      .map_err(|_| KvError::IoFail)?;
    Ok(frame(input, &out))
  }

  pub fn decompress(&self, input: &[u8]) -> Result<Vec<u8>> {
    let Some(prepared) = &self.prepared else {
      return Err(KvError::Corrupted);
    };
    let (original_len, read) = get_varint32(input)?;
    let body = input.get(read..).ok_or(KvError::EndOfFile)?;
    let mut decompressor = Decompressor::default();
    let out = decompressor
      .decompress_with_dictionary(prepared, body)
      .map_err(|_| KvError::Corrupted)?;
    if out.len() != original_len as usize {
      return Err(KvError::Corrupted);
    }
    Ok(out)
  }

  /// 与 `Prepare` 对齐：短值不压缩，大值 standalone，其余走字典。
  pub fn prepare(&self, value: &[u8]) -> PreparedValue {
    if value.len() < SKIP_COMPRESSION_THRESHOLD {
      return PreparedValue {
        data: value.to_vec(),
        kind: CompressionType::None,
      };
    }
    if value.len() > STANDALONE_COMPRESSION_THRESHOLD {
      if let Ok(Some(data)) = compress_raw(value, self.level) {
        return PreparedValue {
          data,
          kind: CompressionType::Standalone,
        };
      }
      return PreparedValue {
        data: value.to_vec(),
        kind: CompressionType::None,
      };
    }
    if let Ok(Some(data)) = self.compress(value) {
      return PreparedValue {
        data,
        kind: CompressionType::Dictionary,
      };
    }
    PreparedValue {
      data: value.to_vec(),
      kind: CompressionType::None,
    }
  }
}
impl fmt::Debug for DictCompression {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("DictCompression")
      .field("dict_len", &self.dict.len())
      .field("has_dictionary", &self.has_dictionary())
      .field("dirty", &self.dirty)
      .field("level", &self.level)
      .finish()
  }
}
