//! B+tree 基础算法：index 下降与 data page 合并 / 分裂。
//!
//! 完整的 COW 写路径在 `fkv_task_write` 里编排；这里保持无 IO、可单测。

pub mod comparator;
mod merge;

use std::borrow::Cow;

use fkv_compress::{DictCompression, decompress_raw};
use fkv_error::{KvError, Result};
use fkv_page::CompressionType;
use fkv_types::WriteOp;
pub use merge::{
  MergeOption, MergeOutput, OverflowSlot, TtlUpsertSink, merge_data_pages_with_options,
  merge_sorted_unique_data_pages_with_options,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchEntry {
  pub key: Vec<u8>,
  pub value: Vec<u8>,
  pub timestamp: u64,
  pub expire_ts: u64,
  pub op: WriteOp,
  /// value 是 `fkv_large` 编码后的 segment id + metadata trailer。
  pub large_value: bool,
  /// value 是 overflow 页的首批指针数组（每个 PageId 小端 4 字节）。
  pub overflow: bool,
  /// 仅写入大 value 时使用；编码后不再保留。
  pub metadata: Vec<u8>,
}

impl BatchEntry {
  pub fn upsert(key: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>, timestamp: u64) -> Self {
    Self {
      key: key.into(),
      value: value.into(),
      timestamp,
      expire_ts: 0,
      op: WriteOp::Upsert,
      large_value: false,
      overflow: false,
      metadata: Vec::new(),
    }
  }

  pub fn delete(key: impl Into<Vec<u8>>, timestamp: u64) -> Self {
    Self {
      key: key.into(),
      value: Vec::new(),
      timestamp,
      expire_ts: 0,
      op: WriteOp::Delete,
      large_value: false,
      overflow: false,
      metadata: Vec::new(),
    }
  }

  /// 大 value 写入时携带 metadata trailer。
  pub fn with_metadata(mut self, metadata: impl Into<Vec<u8>>) -> Self {
    self.metadata = metadata.into();
    self
  }
}

/// 读路径还原 entry value：`None` 直接透传，`Standalone` 走自带 varint 前缀的
/// raw zstd，`Dictionary` 用 table 字典解压。对齐 C++ `GetValue`。
/// 未压缩值借用页内切片，零拷贝。
pub fn decompress_value<'a>(
  value: &'a [u8],
  compression: CompressionType,
  dict: Option<&DictCompression>,
) -> Result<Cow<'a, [u8]>> {
  match compression {
    CompressionType::None => Ok(Cow::Borrowed(value)),
    CompressionType::Standalone => decompress_raw(value).map(Cow::Owned),
    CompressionType::Dictionary => dict
      .ok_or(KvError::Corrupted)
      .and_then(|dict| dict.decompress(value))
      .map(Cow::Owned),
  }
}
