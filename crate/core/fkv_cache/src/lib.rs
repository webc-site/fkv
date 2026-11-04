//! Shard 本地页缓存：槽位 + generation + LRU + pin。
//!
//! 只允许在 shard 线程使用；内部使用 `gxhash::HashMap` 与 `AlignedBuf`。

use fkv_buf::AlignedBuf;
use fkv_types::{FilePageId, PageId};
use gxhash::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRef {
  pub slot: u32,
  pub generation: u32,
}

#[derive(Debug)]
pub struct CachedPage {
  pub page_id: PageId,
  pub file_page_id: FilePageId,
  pub buf: AlignedBuf,
  pub pin_cnt: u32,
  prev: Option<u32>,
  next: Option<u32>,
}

#[derive(Debug)]
pub struct PageCache {
  page_size: usize,
  capacity: usize,
  slot_li: Vec<Option<CachedPage>>,
  generation_li: Vec<u32>,
  index: HashMap<PageId, u32>,
  free_li: Vec<u32>,
  head: Option<u32>,
  tail: Option<u32>,
}

impl PageCache {
  pub fn new(page_size: usize, capacity: usize) -> Self {
    Self {
      page_size,
      capacity,
      slot_li: Vec::with_capacity(capacity),
      generation_li: Vec::with_capacity(capacity),
      index: HashMap::default(),
      free_li: Vec::new(),
      head: None,
      tail: None,
    }
  }

  pub fn page_size(&self) -> usize {
    self.page_size
  }

  pub fn capacity(&self) -> usize {
    self.capacity
  }

  pub fn len(&self) -> usize {
    self.index.len()
  }

  pub fn is_empty(&self) -> bool {
    self.index.is_empty()
  }

  pub fn get(&self, page_id: PageId) -> Option<PageRef> {
    let slot = *self.index.get(&page_id)?;
    Some(PageRef {
      slot,
      generation: self.generation_li[slot as usize],
    })
  }

  pub fn pin(&mut self, page_id: PageId) -> Option<PageRef> {
    let slot = *self.index.get(&page_id)?;
    let page = self.slot_li[slot as usize].as_mut()?;
    page.pin_cnt += 1;
    self.move_front(slot);
    Some(PageRef {
      slot,
      generation: self.generation_li[slot as usize],
    })
  }

  pub fn unpin(&mut self, page_ref: PageRef) -> bool {
    let slot = page_ref.slot as usize;
    if self.generation_li.get(slot).copied() != Some(page_ref.generation) {
      return false;
    }
    let Some(page) = self.slot_li.get_mut(slot).and_then(Option::as_mut) else {
      return false;
    };
    page.pin_cnt = page.pin_cnt.saturating_sub(1);
    true
  }

  pub fn insert(&mut self, page_id: PageId, file_page_id: FilePageId) -> PageRef {
    if let Some(page_ref) = self.pin(page_id) {
      return page_ref;
    }
    let slot = match self.free_li.pop() {
      Some(slot) => {
        let slot_usize = slot as usize;
        self.generation_li[slot_usize] = self.generation_li[slot_usize].wrapping_add(1);
        slot
      }
      None => {
        let slot = self.slot_li.len() as u32;
        self.slot_li.push(None);
        self.generation_li.push(0);
        slot
      }
    };
    let page = CachedPage {
      page_id,
      file_page_id,
      buf: AlignedBuf::new(self.page_size),
      pin_cnt: 1,
      prev: None,
      next: None,
    };
    self.slot_li[slot as usize] = Some(page);
    self.index.insert(page_id, slot);
    self.push_front(slot);
    PageRef {
      slot,
      generation: self.generation_li[slot as usize],
    }
  }

  pub fn resolve(&self, page_ref: PageRef) -> Option<&CachedPage> {
    let slot = page_ref.slot as usize;
    if self.generation_li.get(slot).copied() != Some(page_ref.generation) {
      return None;
    }
    self.slot_li.get(slot).and_then(Option::as_ref)
  }

  pub fn resolve_mut(&mut self, page_ref: PageRef) -> Option<&mut CachedPage> {
    let slot = page_ref.slot as usize;
    if self.generation_li.get(slot).copied() != Some(page_ref.generation) {
      return None;
    }
    self.slot_li.get_mut(slot).and_then(Option::as_mut)
  }

  /// 释放一个未 pin 的页，返回需要回收的物理页。
  pub fn evict_one(&mut self) -> Option<(PageId, FilePageId)> {
    let mut slot = self.tail;
    while let Some(current) = slot {
      let (page_id, file_page_id, pin_cnt, prev) = {
        let page = self.slot_li[current as usize].as_ref()?;
        (page.page_id, page.file_page_id, page.pin_cnt, page.prev)
      };
      if pin_cnt == 0 {
        self.remove_slot(current);
        return Some((page_id, file_page_id));
      }
      slot = prev;
    }
    None
  }

  pub fn remove(&mut self, page_id: PageId) -> Option<FilePageId> {
    let slot = self.index.remove(&page_id)?;
    let file_page_id = self.slot_li[slot as usize]
      .as_ref()
      .map(|page| page.file_page_id)?;
    self.detach(slot);
    self.slot_li[slot as usize] = None;
    self.generation_li[slot as usize] = self.generation_li[slot as usize].wrapping_add(1);
    self.free_li.push(slot);
    Some(file_page_id)
  }

  fn remove_slot(&mut self, slot: u32) {
    let Some(page) = self.slot_li[slot as usize].as_ref() else {
      return;
    };
    let page_id = page.page_id;
    self.index.remove(&page_id);
    self.detach(slot);
    self.slot_li[slot as usize] = None;
    self.generation_li[slot as usize] = self.generation_li[slot as usize].wrapping_add(1);
    self.free_li.push(slot);
  }

  fn detach(&mut self, slot: u32) {
    let (prev, next) = match self.slot_li[slot as usize].as_ref() {
      Some(page) => (page.prev, page.next),
      None => return,
    };
    if let Some(prev) = prev {
      if let Some(page) = self.slot_li[prev as usize].as_mut() {
        page.next = next;
      }
    } else {
      self.head = next;
    }
    if let Some(next) = next {
      if let Some(page) = self.slot_li[next as usize].as_mut() {
        page.prev = prev;
      }
    } else {
      self.tail = prev;
    }
  }

  fn push_front(&mut self, slot: u32) {
    let old_head = self.head;
    if let Some(page) = self.slot_li[slot as usize].as_mut() {
      page.prev = None;
      page.next = old_head;
    }
    if let Some(old_head) = old_head {
      if let Some(page) = self.slot_li[old_head as usize].as_mut() {
        page.prev = Some(slot);
      }
    } else {
      self.tail = Some(slot);
    }
    self.head = Some(slot);
  }

  fn move_front(&mut self, slot: u32) {
    if self.head == Some(slot) {
      return;
    }
    self.detach(slot);
    self.push_front(slot);
  }
}
