//! 逻辑页 → 编码值的分块表 [`MappingTbl`]：chunk 指针表 COW 共享。

use std::{mem::take, sync::Arc};

use fkv_codec::{append_varint64, get_varint64};
use fkv_error::{KvError, Result};
use fkv_types::{FileId, FilePageId, INVALID_VALUE, PageId, decode_id, is_file_page_id};
use gxhash::HashSet;

use crate::chunk::{CHUNK_DIR_COPY_NUM, CHUNK_SHIFT, CHUNK_SIZE, Chunk, take_chunk};

/// 逻辑页 → 编码值的分块表。chunk 指针表整体用 `Arc` 共享：`clone` 只递增一次
/// 引用计数（O(1)，不复制指针数组），修改时对指针表与受影响 chunk 做写时复制
/// （`Arc::make_mut`，为 COW 快照服务）；释放的 chunk 归还线程内池复用。
#[derive(Debug, Clone, Default)]
pub struct MappingTbl {
  chunk_li: Arc<Vec<Arc<Chunk>>>,
  len: usize,
  /// 活跃映射数（`ValType::FilePageId` 槽位数），对齐 C++ `MappingCount()`。
  /// 随 `set` / `push` / `resize` 增量维护，读取 O(1)。
  live_cnt: usize,
}

impl PartialEq for MappingTbl {
  fn eq(&self, other: &Self) -> bool {
    self.len == other.len && self.iter_value_li().eq(other.iter_value_li())
  }
}

impl Eq for MappingTbl {}

impl MappingTbl {
  pub fn new() -> Self {
    Self::default()
  }

  /// 可变 chunk 指针表：被快照 / clone 共享时先整体复制指针数组（写时复制），
  /// 保证旧快照仍看到取出时刻的 chunk 集合。
  pub(crate) fn chunk_li_mut(&mut self) -> &mut Vec<Arc<Chunk>> {
    if Arc::strong_count(&self.chunk_li) > 1 {
      let _ = CHUNK_DIR_COPY_NUM.try_with(|num| num.set(num.get() + 1));
    }
    Arc::make_mut(&mut self.chunk_li)
  }

  /// chunk 指针表当前共享份数（含自身）；测试 / 诊断用。
  pub fn chunk_dir_share_cnt(&self) -> usize {
    Arc::strong_count(&self.chunk_li)
  }

  pub fn len(&self) -> usize {
    self.len
  }

  /// 活跃映射数，对齐 C++ `MappingCount()`：只计指向物理页的槽位，
  /// 不含空闲逻辑页（`PageId` 链节）与 `Invalid` 槽位。
  pub fn mapping_count(&self) -> usize {
    self.live_cnt
  }

  pub fn is_empty(&self) -> bool {
    self.len == 0
  }

  pub fn get(&self, page_id: PageId) -> Result<u64> {
    let index = page_id as usize;
    if index >= self.len {
      return Err(KvError::InvalidArgs);
    }
    // 每个 chunk 恒为 `CHUNK_SIZE` 长（`push`/`resize`/`deserialize` 都填满），
    // 上界已检查，这里直接索引，省掉一次 `Option` 分支。
    let chunk = &self.chunk_li[index >> CHUNK_SHIFT];
    debug_assert_eq!(chunk.value_li.len(), CHUNK_SIZE);
    Ok(chunk.value_li[index & (CHUNK_SIZE - 1)])
  }

  /// 逻辑页当前映射的物理页；非活跃槽位（空闲链表节点 / `Invalid`）报缺失。
  pub fn file_page_id(&self, page_id: PageId) -> Result<FilePageId> {
    let value = self.get(page_id)?;
    if !is_file_page_id(value) {
      return Err(KvError::ResourceMissing);
    }
    Ok(decode_id(value))
  }

  /// 写入一个槽位；越界自动按 chunk 扩容。没有失败路径，故不返回 `Result`。
  pub fn set(&mut self, page_id: PageId, value: u64) {
    let index = page_id as usize;
    if index >= self.len {
      self.resize(index + 1, INVALID_VALUE);
    }
    // 先写槽位并取旧值是否活跃，结束对 chunk_li 的借用后再改 live_cnt
    // （`chunk_li_mut` 借用整个 self，不能与字段写入同时存活）。
    let was_live = {
      let chunk_li = self.chunk_li_mut();
      let slot =
        &mut Arc::make_mut(&mut chunk_li[index >> CHUNK_SHIFT]).value_li[index & (CHUNK_SIZE - 1)];
      let was_live = is_file_page_id(*slot);
      *slot = value;
      was_live
    };
    let is_live = is_file_page_id(value);
    if was_live != is_live {
      if is_live {
        self.live_cnt += 1;
      } else {
        self.live_cnt -= 1;
      }
    }
  }

  pub fn push(&mut self, value: u64) -> PageId {
    let page_id = self.len as PageId;
    let index = self.len;
    let chunk_idx = index >> CHUNK_SHIFT;
    let chunk_li = self.chunk_li_mut();
    if chunk_idx == chunk_li.len() {
      chunk_li.push(Arc::new(Chunk::filled(INVALID_VALUE)));
    }
    Arc::make_mut(&mut chunk_li[chunk_idx]).value_li[index & (CHUNK_SIZE - 1)] = value;
    if is_file_page_id(value) {
      self.live_cnt += 1;
    }
    self.len += 1;
    page_id
  }

  pub fn resize(&mut self, len: usize, value: u64) {
    if len <= self.len {
      if len < self.len {
        // 截断区间内的活跃槽位要从计数里扣掉（`abort_batch` 回滚尾部页）。
        let removed_live = self
          .iter_value_li()
          .skip(len)
          .filter(|value| is_file_page_id(*value))
          .count();
        self.live_cnt -= removed_live;
      }
      // chunk 保持满长（含 len 之后的陈旧 slot），只收缩 chunk 数与逻辑长度。
      self.chunk_li_mut().truncate(len.div_ceil(CHUNK_SIZE));
      self.len = len;
      return;
    }
    let chunk_num = len.div_ceil(CHUNK_SIZE);
    let old_len = self.len;
    let chunk_li = self.chunk_li_mut();
    while chunk_li.len() < chunk_num {
      chunk_li.push(Arc::new(Chunk::filled(INVALID_VALUE)));
    }
    // 填充新增区间 [old_len, len) 为 value。
    let mut index = old_len;
    while index < len {
      let chunk_idx = index >> CHUNK_SHIFT;
      let offset = index & (CHUNK_SIZE - 1);
      let end = (offset + (len - index)).min(CHUNK_SIZE);
      Arc::make_mut(&mut chunk_li[chunk_idx]).value_li[offset..end].fill(value);
      index += end - offset;
    }
    if is_file_page_id(value) {
      self.live_cnt += len - old_len;
    }
    self.len = len;
  }

  /// 按 PageId 升序遍历编码值。
  pub fn iter_value_li(&self) -> impl Iterator<Item = u64> + '_ {
    self
      .chunk_li
      .iter()
      .flat_map(|chunk| chunk.value_li.iter().copied())
      .take(self.len)
  }

  /// 按 `(PageId, value)` 升序遍历。
  pub fn iter(&self) -> impl Iterator<Item = (PageId, u64)> + '_ {
    self
      .iter_value_li()
      .enumerate()
      .map(|(index, value)| (index as PageId, value))
  }

  pub fn serialize(&self) -> Vec<u8> {
    let mut out = Vec::with_capacity(self.len * 5);
    for value in self.iter_value_li() {
      append_varint64(&mut out, value);
    }
    out
  }

  pub fn deserialize(data: &[u8]) -> Result<Self> {
    // 直接按 chunk 边解码边填充：省掉整表中间 `Vec<u64>`，也避免对满 chunk
    // 先 `vec![INVALID; CHUNK_SIZE]` 再整块覆盖的冗余零填。
    let mut chunk_li: Vec<Arc<Chunk>> = Vec::new();
    let mut value_li = take_chunk();
    let mut len = 0usize;
    let mut live_cnt = 0usize;
    let mut pos = 0usize;
    while pos < data.len() {
      let (value, read) = get_varint64(&data[pos..])?;
      if is_file_page_id(value) {
        live_cnt += 1;
      }
      value_li.push(value);
      len += 1;
      pos += read;
      if value_li.len() == CHUNK_SIZE {
        chunk_li.push(Arc::new(Chunk {
          value_li: take(&mut value_li),
        }));
        value_li = take_chunk();
      }
    }
    // 末尾不足一个 chunk：补 `INVALID` 到定长，与 C++ `MappingTbl` 一致。
    if !value_li.is_empty() {
      value_li.resize(CHUNK_SIZE, INVALID_VALUE);
      chunk_li.push(Arc::new(Chunk { value_li }));
    }
    Ok(Self {
      chunk_li: Arc::new(chunk_li),
      len,
      live_cnt,
    })
  }

  /// 由活跃槽位反推文件级空间统计，对齐 C++ `Replayer`（由 `using_fp_ids` 反推
  /// `min_file_id_` / `empty_file_cnt_`），让 `space_size` 在 reopen 后即可用。
  ///
  /// 返回 `(最老非空 file id, [min_file, max_live_file] 之间的空文件数, 最大 live fp id)`；
  /// 空表返回 `(None, 0, None)`。只统计 `is_file_page_id` 的活跃页。
  ///
  /// 第一遍只求 `min_file` / `max_file` / `max_fp_id`，第二遍用位图标记占用，
  /// 内存为 `O(文件数/64)`（而非 `O(活跃页数)` 或每文件 1 字节）。
  pub fn file_stat(&self, pages_per_file_shift: u8) -> (Option<FileId>, u32, Option<FilePageId>) {
    let mut min_file = FileId::MAX;
    let mut max_file = 0u64;
    let mut max_fp_id = 0u64;
    let mut has_live = false;
    for value in self.iter_value_li() {
      if !is_file_page_id(value) {
        continue;
      }
      has_live = true;
      let fp_id = decode_id(value);
      max_fp_id = max_fp_id.max(fp_id);
      let file_id = fp_id >> pages_per_file_shift;
      min_file = min_file.min(file_id);
      max_file = max_file.max(file_id);
    }
    if !has_live {
      return (None, 0, None);
    }
    let file_cnt = (max_file - min_file + 1) as usize;
    let word_bits = u64::BITS as usize;
    let mut bits = vec![0u64; file_cnt.div_ceil(word_bits)];
    for value in self.iter_value_li() {
      if is_file_page_id(value) {
        let slot = ((decode_id(value) >> pages_per_file_shift) - min_file) as usize;
        bits[slot / word_bits] |= 1 << (slot % word_bits);
      }
    }
    let occupied_cnt: u32 = bits.iter().map(|word| word.count_ones()).sum();
    (
      Some(min_file),
      file_cnt as u32 - occupied_cnt,
      Some(max_fp_id),
    )
  }
}

/// 把一个 mapping 中的活跃 `FilePageId` 并入保留集合。
///
/// GC 的 retained set 由 manifest、当前 mapper 与活跃快照三部分并集而成；
/// 本函数是三处共用的一份实现，避免「哪些槽位算活跃」的判定漂移。
pub fn add_mapping_retained(mapping: &MappingTbl, file_page_set: &mut HashSet<FilePageId>) {
  for value in mapping.iter_value_li() {
    if is_file_page_id(value) {
      file_page_set.insert(decode_id(value));
    }
  }
}
