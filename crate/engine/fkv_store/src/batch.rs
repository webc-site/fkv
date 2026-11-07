//! mapper 批次 journal：开场 / 结算与失败回滚。

use fkv_io::{ManifestIo, PageIo};
use fkv_mapping::PageMapper;
use fkv_types::TableIdent;

use crate::Store;

/// 本层真正新建的 mapper journal。
///
/// `PageMapper::begin_batch` 在已有外层批次时返回 `false`（嵌套忽略）。结算必须
/// 只处理自己开的 journal：否则外层 `put_large` 的段批次会被内层 `put` 或 TTL
/// 清理的 abort 一起回滚，出现“manifest 已持久、段映射已回滚”的撕裂状态。
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MapperBatch {
  pub(crate) data: bool,
  pub(crate) segment: bool,
}

impl<I: PageIo + ManifestIo> Store<I> {
  /// 维护写批次开场：`mapper` / `segment_mapper` 的 mapping、分配器与脏页记录
  /// 整体可回滚，供 compact / truncate / clean_expired 对齐 `put_at` 的提交时机。
  pub(crate) fn begin_mapper_batch(&mut self, tbl: &TableIdent) -> MapperBatch {
    let mut batch = MapperBatch::default();
    let Some(state) = self.table_map.get_mut(tbl) else {
      return batch;
    };
    if let Some(mapper) = state.mapper.as_mut() {
      batch.data = mapper.begin_batch();
    }
    if let Some(segment_mapper) = state.segment_mapper.as_mut() {
      batch.segment = segment_mapper.begin_batch();
    }
    batch
  }

  /// 维护写批次结算：manifest 落盘成功才提交，失败回滚到 `begin_mapper_batch` 前状态。
  ///
  /// 必须先 `append_mapping_log` 再提交：manifest 失败时 journal 仍在，
  /// `abort_batch` 才能把 mapping / 分配器 / 脏页长度恢复到本批之前（core-25）。
  /// 只结算 `batch` 里由本层新建的 journal，嵌套外层批次保持不动。
  pub(crate) fn settle_mapper_batch(&mut self, tbl: &TableIdent, batch: MapperBatch, ok: bool) {
    let Some(state) = self.table_map.get_mut(tbl) else {
      return;
    };
    if batch.data
      && let Some(mapper) = state.mapper.as_mut()
    {
      if ok {
        mapper.commit_batch();
      } else {
        mapper.abort_batch();
      }
    }
    if batch.segment
      && let Some(segment_mapper) = state.segment_mapper.as_mut()
    {
      if ok {
        segment_mapper.commit_batch();
      } else {
        segment_mapper.abort_batch();
      }
    }
    // 回滚后映射内容回到批次前，但代际已继续前进：清空条目并把 baseline 抬到
    // 该 live 代际。在途旧快照（代际早于它）此后的回填全部被缓存拒绝，不会
    // 在下一次 `retag` 被当成 baseline 交给新读者（r7a-core-01）。
    if !ok && batch.data {
      let generation = state.mapper.as_ref().map_or(0, PageMapper::generation);
      if let Some(cache) = state.index_cache.as_ref() {
        cache.borrow_mut().reset(generation);
      }
    }
    // 写缓存收尾：先消费 mapper 记录的已释放物理页（无论本层是否开场，嵌套时
    // 先失效更保守），失败批次额外整体清空——回滚后本批已落盘但已失效的字节
    // 不再对应任何活跃映射，清空比逐页对账更稳。
    self.invalidate_freed_page_cache(tbl);
    if !ok && batch.data {
      self.clear_page_cache(tbl);
    }
  }
}
