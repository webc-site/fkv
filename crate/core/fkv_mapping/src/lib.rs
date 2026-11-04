//! 逻辑页 → 物理页映射与文件页分配器。
//!
//! 值格式与 eloqstore `MappingSnapshot` 一致：
//! 低 3 位为 `ValType`，高位为 `FilePageId` / `PageId`。

use std::mem::take;

use fkv_codec::{append_varint64, get_varint64};
use fkv_error::{KvError, Result};
use fkv_types::{FilePageId, PageId};

pub const TYPE_BITS: u8 = 3;
pub const TYPE_MASK: u64 = (1 << TYPE_BITS) - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ValType {
  SwizzlingPointer = 0,
  FilePageId = 1,
  PageId = 2,
  Invalid = 7,
}

pub const INVALID_VALUE: u64 = ValType::Invalid as u64;

pub fn val_type(value: u64) -> ValType {
  match value & TYPE_MASK {
    0 => ValType::SwizzlingPointer,
    1 => ValType::FilePageId,
    2 => ValType::PageId,
    _ => ValType::Invalid,
  }
}

pub fn encode_file_page_id(file_page_id: FilePageId) -> u64 {
  (file_page_id << TYPE_BITS) | ValType::FilePageId as u64
}

pub fn encode_page_id(page_id: PageId) -> u64 {
  (u64::from(page_id) << TYPE_BITS) | ValType::PageId as u64
}

pub fn decode_id(value: u64) -> u64 {
  value >> TYPE_BITS
}

pub fn is_file_page_id(value: u64) -> bool {
  val_type(value) == ValType::FilePageId
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MappingTbl {
  value_li: Vec<u64>,
}

impl MappingTbl {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn with_capacity(capacity: usize) -> Self {
    Self {
      value_li: Vec::with_capacity(capacity),
    }
  }

  pub fn len(&self) -> usize {
    self.value_li.len()
  }

  pub fn is_empty(&self) -> bool {
    self.value_li.is_empty()
  }

  pub fn get(&self, page_id: PageId) -> Result<u64> {
    self
      .value_li
      .get(page_id as usize)
      .copied()
      .ok_or(KvError::InvalidArgs)
  }

  pub fn set(&mut self, page_id: PageId, value: u64) -> Result<()> {
    let index = page_id as usize;
    if index >= self.value_li.len() {
      self.value_li.resize(index + 1, crate::INVALID_VALUE);
    }
    self.value_li[index] = value;
    Ok(())
  }

  pub fn push(&mut self, value: u64) -> PageId {
    let page_id = self.value_li.len() as PageId;
    self.value_li.push(value);
    page_id
  }

  pub fn resize(&mut self, len: usize, value: u64) {
    self.value_li.resize(len, value);
  }

  pub fn value_li(&self) -> &[u64] {
    &self.value_li
  }

  pub fn serialize(&self) -> Vec<u8> {
    let mut out = Vec::with_capacity(self.value_li.len() * 5);
    for value in &self.value_li {
      append_varint64(&mut out, *value);
    }
    out
  }

  pub fn deserialize(data: &[u8]) -> Result<Self> {
    let mut value_li = Vec::new();
    let mut pos = 0usize;
    while pos < data.len() {
      let (value, read) = get_varint64(&data[pos..])?;
      value_li.push(value);
      pos += read;
    }
    Ok(Self { value_li })
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocMode {
  Append,
  Pooled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendAllocator {
  max_fp_id: FilePageId,
}

impl AppendAllocator {
  pub fn new(max_fp_id: FilePageId) -> Self {
    Self { max_fp_id }
  }

  pub fn allocate(&mut self) -> FilePageId {
    let id = self.max_fp_id;
    self.max_fp_id += 1;
    id
  }

  pub fn free(&mut self, _file_page_id: FilePageId) {}

  pub fn max_fp_id(&self) -> FilePageId {
    self.max_fp_id
  }

  /// 预留到指定 file page id，后续分配从该位置开始。
  pub fn reserve_until(&mut self, file_page_id: FilePageId) {
    self.max_fp_id = self.max_fp_id.max(file_page_id);
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PooledFilePages {
  free_li: Vec<FilePageId>,
  max_fp_id: FilePageId,
}

impl PooledFilePages {
  pub fn new(max_fp_id: FilePageId) -> Self {
    Self {
      free_li: Vec::new(),
      max_fp_id,
    }
  }

  pub fn allocate(&mut self) -> FilePageId {
    if let Some(id) = self.free_li.pop() {
      return id;
    }
    let id = self.max_fp_id;
    self.max_fp_id += 1;
    id
  }

  pub fn free(&mut self, file_page_id: FilePageId) {
    self.free_li.push(file_page_id);
  }

  pub fn max_fp_id(&self) -> FilePageId {
    self.max_fp_id
  }

  pub fn free_cnt(&self) -> usize {
    self.free_li.len()
  }

  pub fn reserve_until(&mut self, file_page_id: FilePageId) {
    self.max_fp_id = self.max_fp_id.max(file_page_id);
    self.free_li.retain(|id| *id >= file_page_id);
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilePageAllocator {
  Append(AppendAllocator),
  Pooled(PooledFilePages),
}

impl FilePageAllocator {
  pub fn new(mode: AllocMode, max_fp_id: FilePageId) -> Self {
    match mode {
      AllocMode::Append => Self::Append(AppendAllocator::new(max_fp_id)),
      AllocMode::Pooled => Self::Pooled(PooledFilePages::new(max_fp_id)),
    }
  }

  pub fn allocate(&mut self) -> FilePageId {
    match self {
      Self::Append(allocator) => allocator.allocate(),
      Self::Pooled(allocator) => allocator.allocate(),
    }
  }

  pub fn free(&mut self, file_page_id: FilePageId) {
    match self {
      Self::Append(allocator) => allocator.free(file_page_id),
      Self::Pooled(allocator) => allocator.free(file_page_id),
    }
  }

  pub fn max_fp_id(&self) -> FilePageId {
    match self {
      Self::Append(allocator) => allocator.max_fp_id(),
      Self::Pooled(allocator) => allocator.max_fp_id(),
    }
  }

  pub fn reserve_until(&mut self, file_page_id: FilePageId) {
    match self {
      Self::Append(allocator) => allocator.reserve_until(file_page_id),
      Self::Pooled(allocator) => allocator.reserve_until(file_page_id),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageMapper {
  tbl: MappingTbl,
  allocator: FilePageAllocator,
  dirty_li: Vec<PageId>,
  garbage_file_page_li: Vec<FilePageId>,
}

impl PageMapper {
  pub fn new(mode: AllocMode, max_fp_id: FilePageId) -> Self {
    Self {
      tbl: MappingTbl::new(),
      allocator: FilePageAllocator::new(mode, max_fp_id),
      dirty_li: Vec::new(),
      garbage_file_page_li: Vec::new(),
    }
  }

  pub fn with_tbl(tbl: MappingTbl, mode: AllocMode, max_fp_id: FilePageId) -> Self {
    Self {
      tbl,
      allocator: FilePageAllocator::new(mode, max_fp_id),
      dirty_li: Vec::new(),
      garbage_file_page_li: Vec::new(),
    }
  }

  pub fn tbl(&self) -> &MappingTbl {
    &self.tbl
  }

  pub fn tbl_mut(&mut self) -> &mut MappingTbl {
    &mut self.tbl
  }

  pub fn take_dirty_page_li(&mut self) -> Vec<PageId> {
    take(&mut self.dirty_li)
  }

  pub fn take_garbage_file_page_li(&mut self) -> Vec<FilePageId> {
    take(&mut self.garbage_file_page_li)
  }

  pub fn allocate_page(&mut self) -> PageId {
    let file_page_id = self.allocator.allocate();
    let page_id = self.tbl.push(encode_file_page_id(file_page_id));
    self.dirty_li.push(page_id);
    page_id
  }

  pub fn allocate_file_page(&mut self) -> FilePageId {
    self.allocator.allocate()
  }

  /// 逻辑 PageId 不变，换一个新的 FilePageId（COW）。
  pub fn remap_page(&mut self, page_id: PageId) -> Result<FilePageId> {
    let old = self.tbl.get(page_id)?;
    if is_file_page_id(old) {
      let old_file_page_id = decode_id(old);
      self.allocator.free(old_file_page_id);
      self.garbage_file_page_li.push(old_file_page_id);
    }
    let file_page_id = self.allocator.allocate();
    self.tbl.set(page_id, encode_file_page_id(file_page_id))?;
    self.dirty_li.push(page_id);
    Ok(file_page_id)
  }

  pub fn set_file_page(&mut self, page_id: PageId, file_page_id: FilePageId) -> Result<()> {
    self.tbl.set(page_id, encode_file_page_id(file_page_id))?;
    self.dirty_li.push(page_id);
    Ok(())
  }

  pub fn file_page_id(&self, page_id: PageId) -> Result<FilePageId> {
    let value = self.tbl.get(page_id)?;
    if !is_file_page_id(value) {
      return Err(KvError::ResourceMissing);
    }
    Ok(decode_id(value))
  }

  pub fn free_page(&mut self, page_id: PageId) -> Result<Option<FilePageId>> {
    let value = self.tbl.get(page_id)?;
    if is_file_page_id(value) {
      let file_page_id = decode_id(value);
      self.allocator.free(file_page_id);
      self.garbage_file_page_li.push(file_page_id);
      self.tbl.set(page_id, INVALID_VALUE)?;
      self.dirty_li.push(page_id);
      Ok(Some(file_page_id))
    } else {
      Ok(None)
    }
  }

  pub fn max_fp_id(&self) -> FilePageId {
    self.allocator.max_fp_id()
  }

  /// 跳过一段 file page id 区间（branch 预留）。
  pub fn reserve_until(&mut self, file_page_id: FilePageId) {
    self.allocator.reserve_until(file_page_id);
  }

  pub fn serialize(&self) -> Vec<u8> {
    self.tbl.serialize()
  }
}
