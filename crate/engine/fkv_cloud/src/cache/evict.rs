//! 淘汰：驻留字节收敛到预算、待删队列阈值、victim 摘链与表级清账。

use fkv_types::{FileId, TableIdent};

use super::state::{
  CacheState, CacheVictim, EVICT_DRAIN_FILE_NUM, EVICT_DRAIN_SHIFT, EVICT_FAIL_PAUSE,
  EVICT_PENDING_FILE_MAX, LRU_NIL, drop_if_empty, free_table_slot, lru_unlink,
};
use crate::CloudStore;

impl<I, O> CloudStore<I, O> {
  /// 清空该表的 resident 记账与 LRU 节点（不触碰本地文件）。
  pub(crate) fn forget_table_cache(&self, tbl: &TableIdent) {
    let mut state = self.state.borrow_mut();
    // 按表嵌套后，detach 直接删一个外层条目；连带摘掉该表在 LRU 链上的节点。
    if let Some(cache) = state.table_map.remove(tbl) {
      // 本实例对该表已无脏文件：清掉组内 owner 标记，别让别的实例回填永久跳过。
      // 一次加锁批量清，避免 O(n) 次 mutex 往返。
      let file_id_li: Vec<FileId> = cache.segment.keys().copied().collect();
      self
        .owner
        .clear_segment_dirty_li(self.owner.id, tbl, &file_id_li);
      let bytes = cache.bytes();
      state.resident_bytes -= bytes;
      self.owner.release(bytes);
      let CacheState {
        table_li,
        free_table_li,
        node_li,
        free_node,
        lru_head,
        lru_tail,
        ..
      } = &mut *state;
      for entry in cache.data.values() {
        if entry.lru_idx != LRU_NIL {
          lru_unlink(node_li, free_node, lru_head, lru_tail, entry.lru_idx);
        }
      }
      for entry in cache.segment.values() {
        if entry.lru_idx != LRU_NIL {
          lru_unlink(node_li, free_node, lru_head, lru_tail, entry.lru_idx);
        }
      }
      free_table_slot(table_li, free_table_li, cache.table_idx);
    }
  }
}

impl<I, O> CloudStore<I, O> {
  /// 把驻留字节同步压回预算：只做 LRU 摘链 / 记账，不碰本地文件。
  ///
  /// 返回摘链条数；真正的 `remove_file` / `remove_segment_file` 由
  /// [`Self::drain_evictions`] 批量执行，读写路径只在本批超过阈值时触发。
  ///
  /// 共享预算组时按整组用量收敛：别的实例写超预算，本实例也会从自己的 LRU
  /// 摘条目，直到整组回到 `limit` 或本实例无可淘汰项。本实例没有 victim 时
  /// 不越权删别的实例的文件，只把自己刚写入的条目摘回（磁盘短暂多一份，
  /// `drain_evictions` 后回落到预算内）。
  pub(crate) fn trim_cache(&self) -> usize {
    let limit = self.owner.limit();
    if limit == 0 {
      return 0;
    }
    let evicted = {
      let mut state = self.state.borrow_mut();
      let mut evicted = 0;
      while self.owner.resident_bytes() > limit {
        let Some(victim) = take_victim(&mut state) else {
          break;
        };
        // 条目已从本实例摘链：同步清组内本 owner 的脏登记。不清的话 owner
        // 随被淘汰的 file id 永久残留，该文件在组内对别的实例永远 busy
        // （scheme 回填永远跳过），且 file id 轮转时无界增长。
        if victim.segment {
          self
            .owner
            .set_segment_dirty(self.owner.id, &victim.tbl, victim.file_id, false);
        }
        // `take_victim` 已从本实例 resident 扣减，共享记账同步扣减。
        self.owner.release(victim.bytes);
        let foreign_key = (victim.tbl.clone(), victim.segment, victim.file_id);
        if self.foreign_file_set.borrow_mut().remove(&foreign_key) {
          // 借用文件由另一实例拥有：只注销本实例记账，绝不 unlink 共享路径
          // （shard 可能正在写它且尚未 flush）。
          continue;
        }
        // 失败项回队 / 反复淘汰都可能顶到上限：超限不再入队，文件留在本地
        // 由下一次全量清理（flush / 运维 drain）处理，避免内存无界增长。
        if state.pending_delete_li.len() < EVICT_PENDING_FILE_MAX {
          state.pending_delete_bytes += victim.bytes;
          state.evict_seq += 1;
          state.pending_delete_li.push(victim);
        }
        evicted += 1;
      }
      evicted
    };
    if evicted > 0
      && let Some(metrics) = &self.metrics
    {
      for _ in 0..evicted {
        metrics.inc_cache_evict();
      }
    }
    // 与旧路径一致：每次读写后都刷新 gauge，下载 / 写入增长也能及时反映。
    self.sync_cache_bytes();
    evicted
  }
}

impl<I, O> CloudStore<I, O> {
  /// 待删队列是否达到批量删除阈值。
  ///
  /// `evict_seq == drain_seq` 表示这批淘汰已经尝试过删除（失败项已回队），
  /// 没有新的淘汰就等显式 drain 或下一次淘汰再重试，避免坏盘时每个 IO 都重试。
  pub(crate) fn evict_drain_due(&self) -> bool {
    let state = self.state.borrow();
    !state.pending_delete_li.is_empty()
      && state.evict_seq != state.drain_seq
      && state.drain_fail_streak < EVICT_FAIL_PAUSE
      && (state.pending_delete_bytes >= self.owner.limit() >> EVICT_DRAIN_SHIFT
        || state.pending_delete_li.len() >= EVICT_DRAIN_FILE_NUM)
  }
}

impl<I, O> CloudStore<I, O> {
  /// 淘汰对象是否又回到 cache（重新下载 / 写入）：是则取消删除。
  pub(crate) fn is_resident(&self, victim: &CacheVictim) -> bool {
    if victim.segment {
      self.segment_contains(&victim.tbl, victim.file_id)
    } else {
      self.contains(&victim.tbl, victim.file_id)
    }
  }
}

/// 从全局 LRU 头摘下一个最久未用的干净文件并注销记账。
///
/// 链表头即 O(1) victim；摘链后 `resident_bytes` 立即扣减，语义与「已淘汰」
/// 一致，本地文件删除延后到批量 drain。
pub(crate) fn take_victim(state: &mut CacheState) -> Option<CacheVictim> {
  let head = state.lru_head;
  if head == LRU_NIL {
    return None;
  }
  // 链表头节点与表槽位由本结构维护，越界即状态损坏，按无候选处理。
  let node = *state.node_li.get(head as usize)?;
  // `table_li` 槽位在表内条目清空前不会被释放。
  let tbl = state.table_li.get(node.table_idx as usize)?.clone()?;
  let bytes = {
    let CacheState {
      table_map,
      node_li,
      free_node,
      lru_head,
      lru_tail,
      resident_bytes,
      ..
    } = state;
    let cache = table_map.get_mut(&tbl)?;
    let bytes = if node.segment {
      cache.segment.remove(&node.file_id)?.size
    } else {
      cache.data.remove(&node.file_id)?.size
    };
    *resident_bytes -= bytes;
    lru_unlink(node_li, free_node, lru_head, lru_tail, head);
    bytes
  };
  drop_if_empty(state, &tbl);
  Some(CacheVictim {
    tbl,
    file_id: node.file_id,
    segment: node.segment,
    bytes,
  })
}
