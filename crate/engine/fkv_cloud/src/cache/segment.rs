//! segment 文件记账：resident 登记、LRU 触达、干净 / 注销与组内脏标记同步。

use core::mem::replace;
use std::collections::hash_map::Entry;

use fkv_types::{FileId, TableIdent};

use super::state::{
  CacheState, LRU_NIL, SegmentState, drop_if_empty, lru_move_tail, lru_push, lru_unlink, table_slot,
};
use crate::CloudStore;

impl<I, O> CloudStore<I, O> {
  pub(crate) fn segment_contains(&self, tbl: &TableIdent, file_id: FileId) -> bool {
    self
      .state
      .borrow()
      .table_map
      .get(tbl)
      .is_some_and(|cache| cache.segment.contains_key(&file_id))
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn touch_segment(&self, tbl: &TableIdent, file_id: FileId) {
    let state = &mut *self.state.borrow_mut();
    let CacheState {
      table_map,
      node_li,
      lru_head,
      lru_tail,
      ..
    } = state;
    let Some(entry) = table_map
      .get_mut(tbl)
      .and_then(|cache| cache.segment.get_mut(&file_id))
    else {
      return;
    };
    if entry.lru_idx != LRU_NIL {
      lru_move_tail(node_li, lru_head, lru_tail, entry.lru_idx);
    }
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn upsert_segment_resident(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    size: u64,
    dirty: bool,
    ident: Option<(String, u64)>,
  ) {
    if dirty {
      self
        .owner
        .set_segment_dirty(self.owner.id, tbl, file_id, true);
    }
    let state = &mut *self.state.borrow_mut();
    let CacheState {
      table_map,
      table_li,
      free_table_li,
      node_li,
      free_node,
      lru_head,
      lru_tail,
      resident_bytes,
      ..
    } = state;
    let cache = table_slot(table_map, table_li, free_table_li, tbl);
    let table_idx = cache.table_idx;
    match cache.segment.entry(file_id) {
      Entry::Vacant(vac) => {
        let lru_idx = if dirty {
          LRU_NIL
        } else {
          lru_push(
            node_li, free_node, lru_head, lru_tail, table_idx, file_id, true,
          )
        };
        vac.insert(SegmentState {
          size,
          dirty,
          ident,
          lru_idx,
        });
        *resident_bytes += size;
        self.owner.reserve(size);
      }
      Entry::Occupied(mut occ) => {
        let entry = occ.get_mut();
        let old_size = replace(&mut entry.size, size);
        entry.dirty |= dirty;
        if entry.dirty {
          if entry.lru_idx != LRU_NIL {
            lru_unlink(node_li, free_node, lru_head, lru_tail, entry.lru_idx);
            entry.lru_idx = LRU_NIL;
          }
        } else if entry.lru_idx == LRU_NIL {
          entry.lru_idx = lru_push(
            node_li, free_node, lru_head, lru_tail, table_idx, file_id, true,
          );
        } else {
          lru_move_tail(node_li, lru_head, lru_tail, entry.lru_idx);
        }
        *resident_bytes = *resident_bytes - old_size + size;
        self.owner.adjust(old_size, size);
      }
    }
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn mark_segment_clean(&self, tbl: &TableIdent, file_id: FileId, size: u64) {
    // 先清组内 owner：本地表 / 条目缺失时也必须清，否则该文件在组内永久
    // busy，别的实例的 scheme 回填永远跳过它。
    self
      .owner
      .set_segment_dirty(self.owner.id, tbl, file_id, false);
    let ident = self.current_ident();
    let state = &mut *self.state.borrow_mut();
    let CacheState {
      table_map,
      node_li,
      free_node,
      lru_head,
      lru_tail,
      resident_bytes,
      ..
    } = state;
    let Some(cache) = table_map.get_mut(tbl) else {
      return;
    };
    let table_idx = cache.table_idx;
    let Some(entry) = cache.segment.get_mut(&file_id) else {
      return;
    };
    entry.dirty = false;
    entry.ident = Some(ident);
    let old_size = replace(&mut entry.size, size);
    if entry.lru_idx == LRU_NIL {
      entry.lru_idx = lru_push(
        node_li, free_node, lru_head, lru_tail, table_idx, file_id, true,
      );
    } else {
      lru_move_tail(node_li, lru_head, lru_tail, entry.lru_idx);
    }
    *resident_bytes = *resident_bytes - old_size + size;
    self.owner.adjust(old_size, size);
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn forget_segment(&self, tbl: &TableIdent, file_id: FileId) {
    self
      .owner
      .set_segment_dirty(self.owner.id, tbl, file_id, false);
    let mut state = self.state.borrow_mut();
    {
      let CacheState {
        table_map,
        node_li,
        free_node,
        lru_head,
        lru_tail,
        resident_bytes,
        ..
      } = &mut *state;
      let removed = table_map
        .get_mut(tbl)
        .and_then(|cache| cache.segment.remove(&file_id));
      if let Some(entry) = removed {
        *resident_bytes -= entry.size;
        self.owner.release(entry.size);
        if entry.lru_idx != LRU_NIL {
          lru_unlink(node_li, free_node, lru_head, lru_tail, entry.lru_idx);
        }
      }
    }
    drop_if_empty(&mut state, tbl);
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn dirty_segment_li(&self, tbl: &TableIdent) -> Vec<FileId> {
    let mut file_id_li: Vec<FileId> = self
      .state
      .borrow()
      .table_map
      .get(tbl)
      .map(|cache| {
        cache
          .segment
          .iter()
          .filter(|(_, entry)| entry.dirty)
          .map(|(file_id, _)| *file_id)
          .collect()
      })
      .unwrap_or_default();
    file_id_li.sort_unstable();
    file_id_li
  }
}
