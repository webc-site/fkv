//! B+tree 基础算法：index 下降与 data page 合并 / 分裂。
//!
//! 完整的 COW 写路径在 `fkv_task_write` 里编排；这里保持无 IO、可单测。

use std::{borrow::Cow, collections::BTreeMap};

use fkv_compress::{CompressionType as CompressKind, DictCompression, decompress_raw};
use fkv_error::{KvError, Result};
use fkv_page::{CompressionType, DataPageBuilder, DataPageIter, IndexPage, IndexPageIter};
use fkv_types::{PageId, WriteOp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchEntry {
  pub key: Vec<u8>,
  pub value: Vec<u8>,
  pub timestamp: u64,
  pub expire_ts: u64,
  pub op: WriteOp,
  /// value 是 `fkv_large` 编码后的 segment id + metadata trailer。
  pub large_value: bool,
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
      metadata: Vec::new(),
    }
  }

  /// 大 value 写入时携带 metadata trailer。
  pub fn with_metadata(mut self, metadata: impl Into<Vec<u8>>) -> Self {
    self.metadata = metadata.into();
    self
  }
}

/// 在 index page 中查找目标 key 应该进入的子页。
///
/// `leftmost_ptr` 覆盖 `< first_key`；后续 entry 的 `page_id` 覆盖
/// `[key_i, key_{i+1})`。目标大于最后一个 key 时返回最后一个子页。
pub fn seek_index(page: &[u8], key: &[u8]) -> Result<PageId> {
  let index = IndexPage::new(page)?;
  let mut child = index.leftmost_ptr()?;
  let mut iter = IndexPageIter::new(page)?;
  while iter.advance()? {
    if key < iter.key() {
      return Ok(child);
    }
    child = iter.page_id();
    if key == iter.key() {
      return Ok(child);
    }
  }
  Ok(child)
}

/// 读路径还原 entry value：`None` 直接透传，`Standalone` 走自带 varint 前缀的
/// raw zstd，`Dictionary` 用 table 字典解压。对齐 C++ `GetValue`。
pub fn decompress_value(
  value: &[u8],
  compression: CompressionType,
  dict: Option<&DictCompression>,
) -> Result<Vec<u8>> {
  match compression {
    CompressionType::None => Ok(value.to_vec()),
    CompressionType::Standalone => decompress_raw(value),
    CompressionType::Dictionary => dict
      .ok_or(KvError::Corrupted)
      .and_then(|dict| dict.decompress(value)),
  }
}

/// 合并旧 data page 与 batch，按 key 排序、按 timestamp 过滤，必要时拆成多页。
struct MergedEntry {
  entry: BatchEntry,
  compression: CompressionType,
  from_page: bool,
}

fn merged_value<'a>(
  entry: &'a MergedEntry,
  dict: Option<&DictCompression>,
) -> (Cow<'a, [u8]>, CompressionType) {
  if entry.from_page || entry.entry.large_value {
    return (
      Cow::Borrowed(entry.entry.value.as_slice()),
      entry.compression,
    );
  }
  let Some(dict) = dict else {
    return (
      Cow::Borrowed(entry.entry.value.as_slice()),
      CompressionType::None,
    );
  };
  // 短值、大值 standalone、其余走字典；和 C++ `Prepare` 一致。
  let prepared = dict.prepare(&entry.entry.value);
  let kind = match prepared.kind {
    CompressKind::None => {
      return (
        Cow::Borrowed(entry.entry.value.as_slice()),
        CompressionType::None,
      );
    }
    CompressKind::Standalone => CompressionType::Standalone,
    CompressKind::Dictionary => CompressionType::Dictionary,
  };
  (Cow::Owned(prepared.data), kind)
}

pub fn merge_data_pages(
  old_page_li: &[&[u8]],
  batch_li: &[BatchEntry],
  page_size: usize,
  restart_interval: u16,
) -> Result<Vec<Vec<u8>>> {
  merge_data_pages_with_compression(old_page_li, batch_li, page_size, restart_interval, None)
}

pub fn merge_data_pages_with_compression(
  old_page_li: &[&[u8]],
  batch_li: &[BatchEntry],
  page_size: usize,
  restart_interval: u16,
  dict: Option<&DictCompression>,
) -> Result<Vec<Vec<u8>>> {
  let mut entry_map: BTreeMap<Vec<u8>, MergedEntry> = BTreeMap::new();
  for page in old_page_li {
    let mut iter = DataPageIter::new(page)?;
    while iter.advance()? {
      entry_map.insert(
        iter.key().to_vec(),
        MergedEntry {
          entry: BatchEntry {
            key: iter.key().to_vec(),
            value: iter.value().to_vec(),
            timestamp: iter.timestamp(),
            expire_ts: iter.expire_ts(),
            op: WriteOp::Upsert,
            large_value: iter.is_large_value(),
            metadata: Vec::new(),
          },
          compression: iter.compression(),
          from_page: true,
        },
      );
    }
  }
  for entry in batch_li {
    if let Some(old) = entry_map.get(&entry.key)
      && entry.timestamp < old.entry.timestamp
    {
      continue;
    }
    if entry.op.is_delete() {
      entry_map.remove(&entry.key);
    } else {
      entry_map.insert(
        entry.key.clone(),
        MergedEntry {
          entry: entry.clone(),
          compression: CompressionType::None,
          from_page: false,
        },
      );
    }
  }

  let mut builder = DataPageBuilder::new(page_size, restart_interval)?;
  let mut page_li = Vec::new();
  for entry in entry_map.values() {
    let (value, compression) = merged_value(entry, dict);
    let added = builder.add(
      &entry.entry.key,
      &value,
      false,
      entry.entry.timestamp,
      entry.entry.expire_ts,
      compression,
      entry.entry.large_value,
    )?;
    if added {
      continue;
    }
    if builder.is_empty() {
      return Err(KvError::InvalidArgs);
    }
    page_li.push(builder.finish()?);
    builder = DataPageBuilder::new(page_size, restart_interval)?;
    if !builder.add(
      &entry.entry.key,
      &value,
      false,
      entry.entry.timestamp,
      entry.entry.expire_ts,
      compression,
      entry.entry.large_value,
    )? {
      return Err(KvError::InvalidArgs);
    }
  }
  if !builder.is_empty() {
    page_li.push(builder.finish()?);
  }
  Ok(page_li)
}
