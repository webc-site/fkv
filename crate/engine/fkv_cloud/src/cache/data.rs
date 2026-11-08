//! data 文件记账：resident 登记、脏区间、LRU 触达与注销。

use core::mem::replace;
use std::{collections::hash_map::Entry, mem::take};

use fkv_types::{FileId, TableIdent};

use super::state::{
  CacheState, FileState, LRU_NIL, drop_if_empty, lru_move_tail, lru_push, lru_unlink, table_slot,
};
use crate::CloudStore;

impl<I, O> CloudStore<I, O> {
  pub(crate) fn contains(&self, tbl: &TableIdent, file_id: FileId) -> bool {
    self
      .state
      .borrow()
      .table_map
      .get(tbl)
      .is_some_and(|cache| cache.data.contains_key(&file_id))
  }
}

impl<I, O> CloudStore<I, O> {
  /// 本实例记账的 data 文件长度是否覆盖写入起点 `start`。
  ///
  /// 只读内存记账、不 stat：磁盘上真实长度由 [`Self::ensure_resident`] 的
  /// 单飞 owner 复核一次，快路径不必为每个写页付系统调用。
  pub(crate) fn resident_covers(&self, tbl: &TableIdent, file_id: FileId, start: u64) -> bool {
    self
      .state
      .borrow()
      .table_map
      .get(tbl)
      .and_then(|cache| cache.data.get(&file_id))
      .is_some_and(|entry| entry.size >= start)
  }
}

impl<I, O> CloudStore<I, O> {
  /// 该 data / segment 文件是否已在本实例 cache 记账中，读取无需回源。
  ///
  /// 预热据此在磁盘余量判定之前跳过已热文件：本地已有副本时不会新增占用，
  /// 预算已满也不该因此中止整轮。
  pub fn is_file_resident(&self, tbl: &TableIdent, file_id: FileId, segment: bool) -> bool {
    if segment {
      self.segment_contains(tbl, file_id)
    } else {
      self.contains(tbl, file_id)
    }
  }
}

impl<I, O> CloudStore<I, O> {
  /// 该 data 文件是否含未上传区间；脏文件不能从远端回源覆盖。
  pub(crate) fn is_dirty(&self, tbl: &TableIdent, file_id: FileId) -> bool {
    self
      .state
      .borrow()
      .table_map
      .get(tbl)
      .and_then(|cache| cache.data.get(&file_id))
      .is_some_and(|entry| entry.dirty)
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn touch(&self, tbl: &TableIdent, file_id: FileId) {
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
      .and_then(|cache| cache.data.get_mut(&file_id))
    else {
      return;
    };
    // 脏文件不在链上（不可淘汰），命中只更新链表位置。
    if entry.lru_idx != LRU_NIL {
      lru_move_tail(node_li, lru_head, lru_tail, entry.lru_idx);
    }
  }
}

impl<I, O> CloudStore<I, O> {
  /// 登记一个 data 文件为 resident。
  ///
  /// `ident` 只在新建条目时记录：干净回填传回源对象的写时 `(branch, term)`；
  /// 写入路径传 `None`（随后由 [`Self::mark_dirty_range`] 记成当前 identity）。
  /// 已存在条目保留原归属——登记本身不改变本地内容。
  pub(crate) fn upsert_resident(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    size: u64,
    dirty: bool,
    ident: Option<(String, u64)>,
  ) {
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
    match cache.data.entry(file_id) {
      Entry::Vacant(vac) => {
        let lru_idx = if dirty {
          LRU_NIL
        } else {
          lru_push(
            node_li, free_node, lru_head, lru_tail, table_idx, file_id, false,
          )
        };
        vac.insert(FileState {
          dirty_range_li: Vec::new(),
          ident,
          dirty_all: false,
          size,
          dirty,
          lru_idx,
        });
        *resident_bytes += size;
        self.owner.reserve(size);
      }
      Entry::Occupied(mut occ) => {
        let entry = occ.get_mut();
        // 本次写入可能只覆盖前几页：驻留字节只能取 max，否则预算低估。
        let old_size = entry.size;
        entry.size = old_size.max(size);
        entry.dirty |= dirty;
        if entry.dirty {
          // 转脏即摘链：dirty 文件不可被淘汰（对齐 C++ 不进 closed_files_）。
          if entry.lru_idx != LRU_NIL {
            lru_unlink(node_li, free_node, lru_head, lru_tail, entry.lru_idx);
            entry.lru_idx = LRU_NIL;
          }
        } else if entry.lru_idx == LRU_NIL {
          entry.lru_idx = lru_push(
            node_li, free_node, lru_head, lru_tail, table_idx, file_id, false,
          );
        } else {
          lru_move_tail(node_li, lru_head, lru_tail, entry.lru_idx);
        }
        *resident_bytes = *resident_bytes - old_size + entry.size;
        self.owner.adjust(old_size, entry.size);
      }
    }
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn mark_clean(&self, tbl: &TableIdent, file_id: FileId, size: u64) {
    // 本地内容此刻与本次上传的 key 一致；记为「干净副本所属 identity」，
    // 后续 identity 变更后的首个写区间必须整传，否则会在新 key 上留零洞。
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
    let Some(entry) = cache.data.get_mut(&file_id) else {
      return;
    };
    entry.dirty = false;
    entry.ident = Some(ident);
    entry.dirty_all = false;
    let old_size = replace(&mut entry.size, size);
    if entry.lru_idx == LRU_NIL {
      // 刚上传完成：按最新使用入链尾。
      entry.lru_idx = lru_push(
        node_li, free_node, lru_head, lru_tail, table_idx, file_id, false,
      );
    } else {
      lru_move_tail(node_li, lru_head, lru_tail, entry.lru_idx);
    }
    *resident_bytes = *resident_bytes - old_size + size;
    self.owner.adjust(old_size, size);
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn mark_dirty_range(&self, tbl: &TableIdent, file_id: FileId, offset: u64, len: u64) {
    if len == 0 {
      return;
    }
    let ident = self.current_ident();
    let state = &mut *self.state.borrow_mut();
    let CacheState {
      table_map,
      node_li,
      free_node,
      lru_head,
      lru_tail,
      ..
    } = state;
    let Some(entry) = table_map
      .get_mut(tbl)
      .and_then(|cache| cache.data.get_mut(&file_id))
    else {
      return;
    };
    entry.dirty = true;
    if entry.lru_idx != LRU_NIL {
      lru_unlink(node_li, free_node, lru_head, lru_tail, entry.lru_idx);
      entry.lru_idx = LRU_NIL;
    }
    // identity 变更（换 term / branch）后旧区间属于旧对象 key：不能带到新 key 上，
    // 否则 put_range 会用零前缀覆盖旧 term 数据。旧脏页仍留在本地文件里，
    // 新 identity 的首个对象必须整文件上传（`dirty_all`），只写新区间会让
    // 新对象前段成为零洞。
    if entry.ident.as_ref() != Some(&ident) {
      // 已有旧 identity 的脏区间 / 干净副本：旧页仍在本地文件里，新 key 必须
      // 整对象上传。全新写入条目（无 ident）的本地内容只由本次写构成，
      // 区间可覆盖。
      if entry.ident.is_some() {
        entry.dirty_all = true;
      }
      entry.dirty_range_li.clear();
      entry.ident = Some(ident);
    }
    entry.dirty_range_li.push((offset, len));
  }
}

impl<I, O> CloudStore<I, O> {
  /// 取出待上传区间；`None` 表示必须整对象上传。
  ///
  /// identity 变更 / `dirty_all` 置位 / 区间归属与本次 flush 目标不一致时，
  /// 丢弃区间并返回 `None`，由调用方整文件 PUT 兜底。
  pub(crate) fn take_dirty_range_li(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    ident: &(String, u64),
  ) -> Option<Vec<(u64, u64)>> {
    let mut state = self.state.borrow_mut();
    let Some(entry) = state
      .table_map
      .get_mut(tbl)
      .and_then(|cache| cache.data.get_mut(&file_id))
    else {
      return Some(Vec::new());
    };
    if entry.dirty_all || entry.ident.as_ref() != Some(ident) {
      entry.dirty_range_li.clear();
      entry.ident = None;
      entry.dirty_all = false;
      return None;
    }
    let mut range_li = take(&mut entry.dirty_range_li);
    range_li.sort_unstable();
    let mut merged_range_li: Vec<(u64, u64)> = Vec::with_capacity(range_li.len());
    for (start, len) in range_li {
      match merged_range_li.last_mut() {
        Some((last_start, last_len)) if start <= *last_start + *last_len => {
          let end = (*last_start + *last_len).max(start + len);
          *last_len = end - *last_start;
        }
        _ => merged_range_li.push((start, len)),
      }
    }
    Some(merged_range_li)
  }
}

impl<I, O> CloudStore<I, O> {
  /// 上传失败 / 部分跳过时把未覆盖区间还回 dirty 记账，下一轮重试。
  ///
  /// `None` 表示本轮原计划整对象上传：保持空区间即可，下一轮仍会整传。
  pub(crate) fn restore_dirty_range_li(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    range_li: Option<Vec<(u64, u64)>>,
  ) {
    if let Some(range_li) = range_li {
      for (offset, len) in range_li {
        self.mark_dirty_range(tbl, file_id, offset, len);
      }
    }
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn forget(&self, tbl: &TableIdent, file_id: FileId) {
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
        .and_then(|cache| cache.data.remove(&file_id));
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
  pub(crate) fn dirty_file_li(&self, tbl: &TableIdent) -> Vec<FileId> {
    let mut file_id_li: Vec<FileId> = self
      .state
      .borrow()
      .table_map
      .get(tbl)
      .map(|cache| {
        cache
          .data
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
