//! data page merge 状态机：旧页 / batch 两路归并、overflow 拆分与 TTL upsert 收集。
//!
//! 入口 [`merge_data_pages_with_options`]；页配置与 TTL 收集器见 [`MergeOption`]。
//! 纯算法、无 IO，写路径由 `fkv_task_write` 编排。

use core::{
  cell::RefCell,
  mem::{replace, take},
};
use std::{
  borrow::{Borrow, Cow},
  cmp::Ordering,
};

use fkv_compress::{CompressionType as CompressKind, DictCompression};
use fkv_error::{KvError, Result};
use fkv_page::{
  CompressionType, DataPageBuilder, DataPageIter, EntrySpec, overflow::first_pointer_count,
};
use fkv_types::is_expired;

use crate::BatchEntry;

/// `fkv_compress::CompressionType` → 页内 compression flag。
fn page_kind(kind: CompressKind) -> CompressionType {
  match kind {
    CompressKind::None => CompressionType::None,
    CompressKind::Standalone => CompressionType::Standalone,
    CompressKind::Dictionary => CompressionType::Dictionary,
  }
}

/// 待写入新页的单条 entry：value 借用旧页切片或 batch，压缩值才分配。
struct MergeVal<'a> {
  value: Cow<'a, [u8]>,
  compression: CompressionType,
  timestamp: u64,
  expire_ts: u64,
  large_value: bool,
  overflow: bool,
}

impl<'a> MergeVal<'a> {
  /// 页内已有 entry：保持原值与原 compression flag，绝不二次压缩。
  fn page(iter: &'a DataPageIter<'_>) -> Self {
    Self {
      value: Cow::Borrowed(iter.value()),
      compression: iter.compression(),
      timestamp: iter.timestamp(),
      expire_ts: iter.expire_ts(),
      large_value: iter.is_large_value(),
      overflow: iter.is_overflow(),
    }
  }

  /// batch entry：大 value / 指针数组不压缩；其余按字典 `Prepare`，未压缩时零拷贝。
  fn batch(entry: &'a BatchEntry, dict: Option<&DictCompression>) -> Self {
    let (value, compression) = match dict {
      Some(dict) if !entry.large_value && !entry.overflow => {
        let prepared = dict.prepare(&entry.value);
        (prepared.data, page_kind(prepared.kind))
      }
      _ => (Cow::Borrowed(entry.value.as_slice()), CompressionType::None),
    };
    Self {
      value,
      compression,
      timestamp: entry.timestamp,
      expire_ts: entry.expire_ts,
      large_value: entry.large_value,
      overflow: entry.overflow,
    }
  }
}

/// 旧 data page 序列的流式游标：跨页按 key 升序推进，不物化 entry。
struct OldPageCursor<'p, 'y> {
  page_li: &'p [&'y [u8]],
  page_idx: usize,
  iter: Option<DataPageIter<'y>>,
}

impl<'p, 'y> OldPageCursor<'p, 'y> {
  fn new(page_li: &'p [&'y [u8]]) -> Self {
    Self {
      page_li,
      page_idx: 0,
      iter: None,
    }
  }

  /// 推进到下一条 entry；跨页时自动打开下一页。
  fn advance(&mut self) -> Result<bool> {
    loop {
      if let Some(iter) = self.iter.as_mut()
        && iter.advance()?
      {
        return Ok(true);
      }
      let Some(page) = self.page_li.get(self.page_idx) else {
        self.iter = None;
        return Ok(false);
      };
      self.page_idx += 1;
      self.iter = Some(DataPageIter::new(page)?);
    }
  }

  /// 当前 entry 的迭代器；调用前必须确认 `advance` 返回过 true。
  fn current(&self) -> Result<&DataPageIter<'y>> {
    self.iter.as_ref().ok_or(KvError::Corrupted)
  }
}

/// batch 的有序视图：已排序时直接借用，乱序时只排下标（不拷贝 key/value）。
///
/// `B` 允许拥有 entry 的切片与引用切片（写路径排序去重后的 `&[&BatchEntry]`），
/// 两者都零拷贝。稳定排序保证同 key 的批内相对顺序不变，重复 key 的
/// timestamp 折叠语义与原 BTreeMap 路径一致。
enum BatchView<'a, B> {
  Sorted(&'a [B]),
  Indexed {
    batch: &'a [B],
    index_li: Vec<usize>,
  },
}

impl<'a, B: Borrow<BatchEntry>> BatchView<'a, B> {
  fn new(batch: &'a [B]) -> Self {
    if batch
      .windows(2)
      .all(|pair| pair[0].borrow().key <= pair[1].borrow().key)
    {
      return Self::Sorted(batch);
    }
    let mut index_li: Vec<usize> = (0..batch.len()).collect();
    index_li.sort_by(|&left, &right| batch[left].borrow().key.cmp(&batch[right].borrow().key));
    Self::Indexed { batch, index_li }
  }

  fn len(&self) -> usize {
    match self {
      Self::Sorted(batch) => batch.len(),
      Self::Indexed { index_li, .. } => index_li.len(),
    }
  }

  fn get(&self, index: usize) -> &'a BatchEntry {
    match self {
      Self::Sorted(batch) => batch[index].borrow(),
      Self::Indexed { batch, index_li } => batch[index_li[index]].borrow(),
    }
  }

  fn key(&self, index: usize) -> &'a [u8] {
    self.get(index).key.as_slice()
  }

  /// 折叠同 key 的一组 batch entry，返回胜者与下一组起点。
  ///
  /// 对齐旧实现：后项 timestamp >= 当前胜者才覆盖（相等也覆盖），删除项参与折叠。
  fn group(&self, start: usize) -> (&'a BatchEntry, usize) {
    let mut kept = self.get(start);
    let mut idx = start + 1;
    while idx < self.len() && self.key(idx) == kept.key.as_slice() {
      let entry = self.get(idx);
      if entry.timestamp >= kept.timestamp {
        kept = entry;
      }
      idx += 1;
    }
    (kept, idx)
  }
}

/// 新页写入器：逐条喂 `DataPageBuilder`，页满即切页，overflow 值回填指针占位。
struct PageSink<'d> {
  option: MergeOption<'d>,
  builder: DataPageBuilder,
  page_li: Vec<Vec<u8>>,
  overflow_li: Vec<OverflowSlot>,
}

impl<'d> PageSink<'d> {
  fn new(option: MergeOption<'d>) -> Result<Self> {
    Ok(Self {
      option,
      builder: DataPageBuilder::new(option.page_size, option.restart_interval)?,
      page_li: Vec::new(),
      overflow_li: Vec::new(),
    })
  }

  /// 写入一条从旧页带过来的 entry。
  ///
  /// 写批重写该页时，`expire_ts != 0 && expire_ts <= now_ms` 的旧条目直接丢弃，
  /// 不进入新页（对齐 C++ `BatchWriteTask::add_to_page` 的过期分支）。其 overflow
  /// 链 / large value 段与 TTL 索引项由 `fkv_task_write` 在同一个 key 位置释放
  /// （见 `rewrite_page_values`），合并结果本身不需要知道释放出口。
  fn push_old(&mut self, iter: &DataPageIter<'_>) -> Result<()> {
    if is_expired(iter.expire_ts(), self.option.now_ms) {
      return Ok(());
    }
    self.push(iter.key(), MergeVal::page(iter))
  }

  fn push(&mut self, key: &[u8], val: MergeVal<'_>) -> Result<()> {
    let MergeVal {
      value,
      compression,
      timestamp,
      expire_ts,
      large_value,
      overflow,
    } = val;
    let mut added = self.builder.add(EntrySpec {
      key,
      value: &value,
      ts: timestamp,
      expire_ts,
      overflow,
      compression,
      large_value,
    })?;
    // 对齐 C++ `add_to_page`：Add 失败时先用 `IsOverflowKV` 估算；
    // 判定需 overflow 则把指针写进**当前页**，不先 finish 当前页。
    if !added
      && !overflow
      && !large_value
      && fkv_page::is_overflow_kv(
        key,
        value.len(),
        timestamp,
        expire_ts,
        self.option.page_size,
      )
    {
      let pointer_num = first_pointer_count(
        value.len(),
        self.option.page_size,
        self.option.overflow_pointers,
      )?;
      let placeholder = vec![0u8; pointer_num * 4];
      if self.builder.add(EntrySpec {
        key,
        value: &placeholder,
        ts: timestamp,
        expire_ts,
        overflow: true,
        compression,
        large_value: false,
      })? {
        self.overflow_li.push(OverflowSlot {
          page_idx: self.page_li.len(),
          key: key.to_vec(),
          value: value.to_vec(),
        });
        return Ok(());
      }
    }
    if !added && !self.builder.is_empty() {
      let builder = replace(
        &mut self.builder,
        DataPageBuilder::new(self.option.page_size, self.option.restart_interval)?,
      );
      self.page_li.push(builder.finish()?);
      added = self.builder.add(EntrySpec {
        key,
        value: &value,
        ts: timestamp,
        expire_ts,
        overflow,
        compression,
        large_value,
      })?;
    }
    if added {
      return Ok(());
    }
    // 空页仍放不下（估算/精确编码边界兜底）：只有非 large、非指针 value 才拆链。
    if large_value || overflow {
      return Err(KvError::InvalidArgs);
    }
    let pointer_num = first_pointer_count(
      value.len(),
      self.option.page_size,
      self.option.overflow_pointers,
    )?;
    let placeholder = vec![0u8; pointer_num * 4];
    if !self.builder.add(EntrySpec {
      key,
      value: &placeholder,
      ts: timestamp,
      expire_ts,
      overflow: true,
      compression,
      large_value: false,
    })? {
      return Err(KvError::InvalidArgs);
    }
    self.overflow_li.push(OverflowSlot {
      page_idx: self.page_li.len(),
      key: key.to_vec(),
      value: value.to_vec(),
    });
    Ok(())
  }

  fn finish(self) -> Result<MergeOutput> {
    let mut page_li = self.page_li;
    if !self.builder.is_empty() {
      page_li.push(self.builder.finish()?);
    }
    Ok(MergeOutput {
      page_li,
      overflow_li: self.overflow_li,
    })
  }
}

/// 写出一组同 key batch entry 的胜者；删除项不出页。返回下一组起点。
fn emit_batch<B: Borrow<BatchEntry>>(
  sink: &mut PageSink<'_>,
  batch: &BatchView<'_, B>,
  start: usize,
) -> Result<usize> {
  let (kept, next) = batch.group(start);
  if !kept.op.is_delete() {
    sink.push(kept.key.as_slice(), MergeVal::batch(kept, sink.option.dict))?;
    // C++ `cmp > 0` 的 INSERT 分支：无基页，live upsert 直接 `UpdateTTL(Upsert)`。
    sink
      .option
      .record_ttl_upsert(kept.key.as_slice(), kept.expire_ts);
  }
  Ok(next)
}

/// merge 结果中需要落盘 overflow 链的 entry。
///
/// `page_idx` 指向 `MergeOutput::page_li` 里的目标 data page；`value` 是已压缩
/// （若启用）的真实字节，回溯到对应 overflow 页。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverflowSlot {
  pub page_idx: usize,
  pub key: Vec<u8>,
  pub value: Vec<u8>,
}

/// merge 输出：data page 列表（overflow entry 已写入等长指针占位）
/// + 待 `fkv_task_write` 落盘并回填指针的 entry。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MergeOutput {
  pub page_li: Vec<Vec<u8>>,
  pub overflow_li: Vec<OverflowSlot>,
}

/// data 页 merge / 重建共用的页配置（`merge_data_pages_with_options` 入参）。
///
/// 收拢原先的四个位置参数；调用方通常直接从同一份 layout / option 抄录。
#[derive(Debug, Clone, Copy)]
pub struct MergeOption<'d> {
  pub page_size: usize,
  /// data 页 restart 间隔。
  pub restart_interval: u16,
  pub overflow_pointers: u8,
  /// 字典压缩上下文；`None` 表示不压缩。
  pub dict: Option<&'d DictCompression>,
  /// 写批的墙钟（毫秒）：旧页 entry 的 `expire_ts != 0 && expire_ts <= now_ms`
  /// 即已过期，重写该页时直接丢弃（对齐 C++ `add_to_page`）。`0` 表示不过滤。
  pub now_ms: u64,
  /// TTL upsert 收集器（sem-55）：`None` 表示本次 merge 不产出 TTL 索引变更。
  ///
  /// TTL 索引只在能看到基页 `expire_ts` 的 merge 阶段才能判定是否需要
  /// `UpdateTTL(Upsert)`（C++ `BatchWriteTask::ApplyOnePage` 的 `cmp == 0` /
  /// `cmp > 0` 分支），因此由写路径把该收集器挂进页配置；用共享引用 +
  /// 内部可变性承载以保持本结构 `Copy`（页组按值传递布局）。
  pub ttl_upsert_sink: Option<&'d TtlUpsertSink>,
}

/// merge 阶段产出的 TTL upsert 请求：`(user_key, expire_ts)`。
///
/// 对齐 C++ `UpdateTTL(expire_ts, key, WriteOp::Upsert)`：只有 merge 真正落地、
/// 且新值 live 的条目才会进入，且**仅当基页没有该 key 或基页 `expire_ts` 与
/// 新值不同**时记录。相同 expire 的覆盖不产生索引项，被 merge 丢弃的旧
/// timestamp 写入也不产生（sem-55）。
#[derive(Debug, Default)]
pub struct TtlUpsertSink {
  entry_li: RefCell<Vec<(Vec<u8>, u64)>>,
}

impl TtlUpsertSink {
  pub fn new() -> Self {
    Self::default()
  }

  fn push(&self, key: &[u8], expire_ts: u64) {
    self.entry_li.borrow_mut().push((key.to_vec(), expire_ts));
  }

  /// 取走已收集的 `(user_key, expire_ts)`。
  pub fn take(&self) -> Vec<(Vec<u8>, u64)> {
    take(&mut *self.entry_li.borrow_mut())
  }
}

impl MergeOption<'_> {
  /// 记录一条需要新建的 TTL 索引项。
  ///
  /// 调用方已确认 merge 保留该新值；这里只按 C++ `UpdateTTL` 的前置条件过滤：
  /// `expire_ts == 0`（无 TTL）与已过期的新值都不写索引。
  fn record_ttl_upsert(&self, key: &[u8], expire_ts: u64) {
    if expire_ts != 0
      && !is_expired(expire_ts, self.now_ms)
      && let Some(sink) = self.ttl_upsert_sink
    {
      sink.push(key, expire_ts);
    }
  }
}

pub fn merge_data_pages_with_options<B: Borrow<BatchEntry>>(
  old_page_li: &[&[u8]],
  batch_li: &[B],
  option: MergeOption<'_>,
) -> Result<MergeOutput> {
  let mut sink = PageSink::new(option)?;
  let batch = BatchView::new(batch_li);
  let mut old = OldPageCursor::new(old_page_li);
  let mut old_has = old.advance()?;
  let mut batch_idx = 0usize;
  // 两路归并：旧页（按 key 升序）与 batch（有序视图）各持一个游标，
  // 直接喂 PageSink，不落中间容器；value 全程借用，压缩值才分配。
  while old_has || batch_idx < batch.len() {
    if !old_has {
      batch_idx = emit_batch(&mut sink, &batch, batch_idx)?;
      continue;
    }
    if batch_idx >= batch.len() {
      let iter = old.current()?;
      sink.push_old(iter)?;
      old_has = old.advance()?;
      continue;
    }
    match old.current()?.key().cmp(batch.key(batch_idx)) {
      Ordering::Less => {
        let iter = old.current()?;
        sink.push_old(iter)?;
        old_has = old.advance()?;
      }
      Ordering::Greater => {
        batch_idx = emit_batch(&mut sink, &batch, batch_idx)?;
      }
      Ordering::Equal => {
        let (kept, next) = batch.group(batch_idx);
        batch_idx = next;
        let iter = old.current()?;
        // timestamp 较旧的 batch 写入被丢弃；相等时 batch 覆盖（对齐旧实现）。
        if kept.timestamp < iter.timestamp() {
          // C++ `change_ts < base_ts`：整条 change 被丢弃，完全不调用 UpdateTTL。
          sink.push_old(iter)?;
        } else if !kept.op.is_delete() {
          sink.push(kept.key.as_slice(), MergeVal::batch(kept, option.dict))?;
          // C++ `cmp == 0` / `change_ts >= base_ts` 的 UPDATE 分支：只有
          // `base_expire != expire_ts` 才 `UpdateTTL(Delete old)` +
          // `UpdateTTL(Upsert new)`；相同 expire 不写 TTL 树。
          if kept.expire_ts != iter.expire_ts() {
            sink
              .option
              .record_ttl_upsert(kept.key.as_slice(), kept.expire_ts);
          }
        }
        old_has = old.advance()?;
      }
    }
  }
  sink.finish()
}
