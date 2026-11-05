//! zstd 字典 / 单值压缩：
//! `varint32(original_len) | zstd_frame`。
//!
//! 编解码走 `mzstd`（纯安全 Rust、与 zstd 1.5.7 字节一致）；字典训练仍用
//! `zstd` 的 `ZDICT_trainFromBuffer`（mzstd 无等价能力）。

use core::mem::size_of;
use std::{borrow::Cow, cell::RefCell, fmt, mem::take};

use fkv_codec::{get_varint32, put_varint32, varint32_len};
use fkv_error::{KvError, Result};
use mzstd::{
  Compressor, Decompressor, EncoderConfig, Level,
  dictionary::{DictionaryBuilder, PreparedDictionary},
};
use zstd::dict::from_continuous;

pub const SKIP_COMPRESSION_THRESHOLD: usize = 100;
pub const STANDALONE_COMPRESSION_THRESHOLD: usize = 10 * 1024 * 1024;
/// 预构建字典（CDict + DDict）相对原始字典字节的保守倍数。
const PREPARED_DICT_FACTOR: usize = 3;

const MAX_DICT_BYTES: usize = 16 * 1024;
pub const SAMPLE_TARGET_BYTES: usize = 100 * MAX_DICT_BYTES;
pub(crate) const MIN_SAMPLES: usize = 5;
/// u32 varint 最长字节数。
const MAX_VARINT32_BYTES: usize = 5;

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
pub struct PreparedValue<'a> {
  pub data: Cow<'a, [u8]>,
  pub kind: CompressionType,
}

impl<'a> PreparedValue<'a> {
  /// 不压缩：直接借用原值，避免多拷贝一次。
  fn raw(value: &'a [u8]) -> Self {
    Self {
      data: Cow::Borrowed(value),
      kind: CompressionType::None,
    }
  }
}

/// 压缩级别转 mzstd `Level`；越界按默认级别处理（C++ level 0 也是默认）。
fn level_of(level: i32) -> Level {
  Level::try_from(level).unwrap_or(Level::DEFAULT)
}

/// 借出线程内复用的 CCtx，级别不同则重配。raw 与字典压缩共用同一上下文。
fn with_compressor<T>(level: i32, f: impl FnOnce(&mut Compressor) -> Result<T>) -> Result<T> {
  RAW_COMPRESS.with(|cell| {
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
    f(compressor)
  })
}

/// 借出线程内复用的 DCtx。
fn with_decompressor<T>(f: impl FnOnce(&mut Decompressor) -> Result<T>) -> Result<T> {
  RAW_DECOMPRESS.with(|cell| {
    let mut slot = cell.borrow_mut();
    if slot.is_none() {
      *slot = Some(Decompressor::default());
    }
    f(slot.as_mut().ok_or(KvError::IoFail)?)
  })
}

/// 拼 `varint32(原始长度) | 压缩体`；含长度前缀后仍不小于原值则放弃，
/// 避免为 1B 的收益多存一个前缀反而变大。
///
/// 注意与 C++ 的细微差异：C++ `CompressRaw` 只比较压缩体本身
/// （`written >= input.size()` 就放弃），不管前缀；Rust 用「前缀 + 压缩体」比较，
/// 更严格（绝不写出膨胀帧）。分叉只发生在 `input.len()-prefix_len <=
/// compressed.len() < input.len()` 的 1~5 字节边界，**不影响双向互读**
/// （解压侧先读 varint 长度再解帧，与保留决策无关）。
fn frame(input: &[u8], compressed: &[u8]) -> Option<Vec<u8>> {
  // 长度前缀是 u32；超长输入放弃压缩，绝不截断写出损坏帧。
  let original_len = u32::try_from(input.len()).ok()?;
  let prefix_len = varint32_len(original_len);
  if compressed.is_empty() || prefix_len + compressed.len() >= input.len() {
    return None;
  }
  let mut scratch = [0u8; MAX_VARINT32_BYTES];
  let written = put_varint32(&mut scratch, original_len)?;
  let mut out = Vec::with_capacity(prefix_len + compressed.len());
  out.extend_from_slice(&scratch[..written]);
  out.extend_from_slice(compressed);
  Some(out)
}

/// 无字典 raw 压缩，线程内复用 CCtx。压后不更小返回 `None`。
pub fn compress_raw(input: &[u8], level: i32) -> Result<Option<Vec<u8>>> {
  if input.is_empty() {
    return Ok(None);
  }
  let mut out = Vec::new();
  with_compressor(level, |compressor| {
    compressor
      .compress_into(input, &mut out)
      .map_err(|_| KvError::IoFail)?;
    Ok(())
  })?;
  Ok(frame(input, &out))
}

/// 无字典 raw 解压：读 varint 前缀长度，再用复用 DCtx 解码。
pub fn decompress_raw(input: &[u8]) -> Result<Vec<u8>> {
  let (original_len, read) = get_varint32(input)?;
  let body = input.get(read..).ok_or(KvError::EndOfFile)?;
  // 按前缀长度预分配并用固定切片解压：mzstd 默认 output budget 关闭，
  // 损坏帧若声明超大 content size 会放大分配；固定切片让超限直接报错。
  let original_len = original_len as usize;
  let mut out = vec![0u8; original_len];
  let written = with_decompressor(|decompressor| {
    decompressor
      .decompress_to_slice(body, &mut out)
      .map_err(|_| KvError::Corrupted)
  })?;
  if written != original_len {
    return Err(KvError::Corrupted);
  }
  Ok(out)
}

#[derive(Default)]
pub struct DictCompression {
  dict: Vec<u8>,
  prepared: Option<PreparedDictionary>,
  dirty: bool,
  sample_data: Vec<u8>,
  sample_size_li: Vec<usize>,
  level: i32,
  /// 已尝试过训练（无论成败），对齐 C++ `has_dictionary_ = true` 的副作用，避免
  /// 训练失败后每次写入重复跑全量 `ZDICT_trainFromBuffer`。
  training_attempted: bool,
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

  pub fn level(&self) -> i32 {
    self.level
  }

  pub fn dictionary_bytes(&self) -> &[u8] {
    &self.dict
  }

  /// 估算内存占用：字典 + 样本缓冲容量 + 预构建 CDict/DDict。
  ///
  /// mzstd 不暴露 `ZSTD_sizeof_CDict`，按字典字节的固定倍数保守估算。
  pub fn estimated_memory_bytes(&self) -> usize {
    let prepared = if self.prepared.is_some() {
      self.dict.len() * PREPARED_DICT_FACTOR
    } else {
      0
    };
    self.dict.len()
      + prepared
      + self.sample_data.capacity()
      + self.sample_size_li.capacity() * size_of::<usize>()
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
    // 训练一旦开始就释放样本并记下"已尝试"，失败后不再重复训练。
    let sample_data = take(&mut self.sample_data);
    let sample_size_li = take(&mut self.sample_size_li);
    self.training_attempted = true;
    let dict = from_continuous(&sample_data, &sample_size_li, MAX_DICT_BYTES)
      .map_err(|_| KvError::IoFail)?;
    if dict.is_empty() {
      return Err(KvError::IoFail);
    }
    self.load_dictionary(dict)?;
    self.dirty = true;
    Ok(())
  }

  /// 与 `SampleAndBuildDictionaryIfNeeded` 对齐：样本达到目标或补齐 MIN_SAMPLES 后训练。
  ///
  /// 已知差异：Rust 把批内所有合法样本整批加入，C++ 对超目标批次做随机
  /// 抽样；语料量/字典字节可能不同，但 zstd 帧自包含，不影响双向可读。
  pub fn maybe_build_dictionary(&mut self, sample_li: &[&[u8]]) -> Result<()> {
    if self.has_dictionary() || self.training_attempted {
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
    // `prepared` 是 `load_dictionary` 时一次性编译的 CDict/DDict 等价物（对齐 C++
    // `EnsureZstdObjects`）；每次调用只把它挂到复用的 CCtx 上，不重新编译字典。
    let mut out = Vec::new();
    with_compressor(self.level, |compressor| {
      compressor
        .compress_with_dictionary_into(prepared, input, &mut out)
        .map_err(|_| KvError::IoFail)?;
      Ok(())
    })?;
    Ok(frame(input, &out))
  }

  pub fn decompress(&self, input: &[u8]) -> Result<Vec<u8>> {
    let Some(prepared) = &self.prepared else {
      return Err(KvError::Corrupted);
    };
    let (original_len, read) = get_varint32(input)?;
    let body = input.get(read..).ok_or(KvError::EndOfFile)?;
    // 同 `decompress_raw`：输出上限收紧到前缀声明的长度。
    let original_len = original_len as usize;
    let mut out = vec![0u8; original_len];
    let written = with_decompressor(|decompressor| {
      decompressor
        .decompress_with_dictionary_to_slice(prepared, body, &mut out)
        .map_err(|_| KvError::Corrupted)
    })?;
    if written != original_len {
      return Err(KvError::Corrupted);
    }
    Ok(out)
  }

  /// 与 `Prepare` 对齐：短值不压缩；超上限走 standalone；有字典的中间区间走
  /// 字典；无字典的中间区间暂不压缩（对齐 C++ `Prepare`；自动训练在 `fkv_store` 写路径接入）。
  pub fn prepare<'a>(&self, value: &'a [u8]) -> PreparedValue<'a> {
    if value.len() < SKIP_COMPRESSION_THRESHOLD {
      return PreparedValue::raw(value);
    }
    let (compressed, kind) = if value.len() > STANDALONE_COMPRESSION_THRESHOLD {
      (compress_raw(value, self.level), CompressionType::Standalone)
    } else if self.has_dictionary() {
      (self.compress(value), CompressionType::Dictionary)
    } else {
      return PreparedValue::raw(value);
    };
    match compressed {
      Ok(Some(data)) => PreparedValue {
        data: Cow::Owned(data),
        kind,
      },
      _ => PreparedValue::raw(value),
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
