//! 整表 / 范围 truncate。

use fkv_btree::BatchEntry;
use fkv_error::Result;
use fkv_io::{ManifestIo, PageIo};
use fkv_mapping::PageMapper;
use fkv_task_read::YieldBudget;
use fkv_task_scan::{ScanArg, ScanOption, ScanTree, scan_entries_with_budget};
use fkv_types::{NULL_PAGE, PageId, TableIdent};

use crate::{Store, TRUNCATE_FROM_BATCH};

/// 释放一个 mapper 的全部逻辑页；长表按 [`YieldBudget`] 定期让出。
///
/// 整表 truncate 的页数与表大小同阶，不让出会把整个 shard 线程占满、
/// 拖住同线程上的读请求。
async fn free_all_pages(mapper: &mut PageMapper, budget: &mut YieldBudget) -> Result<()> {
  let page_num = mapper.tbl().len();
  for page_id in 0..page_num as PageId {
    let _ = mapper.free_page(page_id)?;
    budget.maybe_yield_every(page_id as usize).await;
  }
  Ok(())
}

impl<I: PageIo + ManifestIo> Store<I> {
  /// 整表清空：释放两棵树占用的物理页，root / ttl_root 归零，并重置 manifest snapshot。
  ///
  /// 已加载且两棵树均空的分区直接早退（对齐 C++ `root_id_ == MaxPageId` 门，
  /// sem-58）：不释放页、不写 manifest、不创建 manifest 对象。
  ///
  /// data 文件不立即删除，留给 `gc` 回收；逻辑 page id 表保留为 INVALID，
  /// 避免重启后逻辑 id 与旧文件冲突。
  ///
  /// 与 C++ `BatchWriteTask::Truncate` 的**有意差异**（sem-43）：C++ 整表
  /// truncate 只释放 data mapper 的页，`segment_mapper_` 原样保留，manifest
  /// 仍写回 truncate 前的 segment FilePageId 集合，旧 segment 文件由 GC
  /// 继续保留；本实现同时释放 segment 逻辑页，manifest segment 段变为空闲
  /// 链，旧 segment 文件在无在途快照后可由 GC 回收。两侧都保留
  /// `max_segment_fp_id`，续写不会覆盖旧段；差异只影响 truncate 后的
  /// manifest segment 段取值与可见 segment 文件集，数据 / 互读语义一致。
  /// 对拍时该字段与 truncate 后的 segment 文件集不作为等价目标。
  pub async fn truncate(&mut self, tbl: &TableIdent) -> Result<()> {
    // 对齐 C++ `BatchWriteTask::Truncate` 的空树早退（batch_write_task.cpp:1849-1852）：
    // 已加载且两棵树均空的分区，整表 truncate 零释放、零 manifest 写（sem-58
    // Form A）。C++ 在 trunc_pos 分派之前按 `root_id_ == MaxPageId` 无条件早退：
    // 懒开空表不得凭空建 manifest、已清空分区不得反复重写空快照、cleanup 收尾
    // 后不得重建 manifest。`next_expire` 与 C++ 早退路径一致地保持原值，留给
    // 后续 `clean_expired` 在 TTL 树为空时自行归零。
    if let Some(state) = self.table_map.get(tbl)
      && state.root == Some(NULL_PAGE)
      && state.ttl_root == NULL_PAGE
    {
      return Ok(());
    }
    self.refresh_branch_metadata(tbl);
    // 整表释放包成一个可回滚批次：manifest 写失败时 mapping / 分配器 / root 全部复原，
    // 调用方重试从干净状态开始（core-25）。
    let batch = self.begin_mapper_batch(tbl);
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    let free_result = {
      let Some(state) = self.table_map.get_mut(tbl) else {
        return Ok(());
      };
      match state.mapper.as_mut() {
        None => {
          state.next_expire = 0;
          // 空表没有页面要释放，但本批已开场：必须结算，否则 journal 悬挂（R2-5）。
          None
        }
        Some(mapper) => {
          let segment = state.segment_mapper.as_mut();
          let result = match free_all_pages(mapper, &mut yield_budget).await {
            Ok(()) => match segment {
              Some(segment) => free_all_pages(segment, &mut yield_budget).await,
              None => Ok(()),
            },
            Err(err) => Err(err),
          };
          Some(result)
        }
      }
    };
    match free_result {
      None => {
        self.settle_mapper_batch(tbl, batch, true);
        return Ok(());
      }
      Some(Ok(())) => {}
      Some(Err(err)) => {
        self.settle_mapper_batch(tbl, batch, false);
        return Err(err);
      }
    }
    let record = match self.snapshot_record(tbl, NULL_PAGE, NULL_PAGE, None, None, None) {
      Ok(record) => record,
      Err(err) => {
        self.settle_mapper_batch(tbl, batch, false);
        return Err(err);
      }
    };
    let write_result = self.write_manifest_record(tbl, &record).await;
    self.settle_mapper_batch(tbl, batch, write_result.is_ok());
    write_result?;
    // manifest 成功：本批脏页 / 变更记录已随 snapshot 落盘，直接丢弃。
    if let Some(state) = self.table_map.get_mut(tbl) {
      state.root = Some(NULL_PAGE);
      state.ttl_root = NULL_PAGE;
      state.manifest_started = true;
      state.manifest_size = record.len();
      state.next_expire = 0;
      if let Some(mapper) = state.mapper.as_mut() {
        mapper.clear_dirty_page_li();
        // 末尾 `clear_index_cache` 整表清空，物理释放无需逐页失效（eng-68）。
        mapper.clear_changed_page_li();
      }
      if let Some(segment_mapper) = state.segment_mapper.as_mut() {
        segment_mapper.clear_dirty_page_li();
        segment_mapper.clear_changed_page_li();
      }
    }
    // 整表释放退役了全部 data 物理页；manifest 已落盘、批次已结算，此刻按
    // 快照水位回池（pooled-04）。不留给后续 `put` / `maintain` 兜底：纯
    // 「写满 → truncate → 重写」负载下否则每轮都会抬一次分配高位。
    self.reclaim_retired_now(tbl);
    self.clear_index_cache(tbl);
    // 两棵树全部页已释放、root 归零：写缓存留着只会是死条目（下批首次分配
    // 即覆盖其 id），整体丢弃。
    self.clear_write_cache(tbl);
    Ok(())
  }

  /// 删除所有 key >= position；position 为空等价整表 truncate。
  ///
  /// 只需 key 定位，扫描走 `keys_only`：不读 overflow 链、不解压 value。
  /// 大值 segment / overflow 页的释放由写路径在 merge 旧 entry 时完成。
  ///
  /// 按 [`TRUNCATE_FROM_BATCH`] 分批「扫描 → 删除 → 从 position 续扫」，
  /// 驻留内存与表大小无关（R2-1）：删除已提交，下一轮扫描自然从更靠后的 key 开始。
  pub async fn truncate_from(&mut self, tbl: &TableIdent, position: &[u8]) -> Result<usize> {
    if position.is_empty() {
      self.truncate(tbl).await?;
      return Ok(0);
    }
    let mut deleted = 0usize;
    let mut prev_first: Option<Vec<u8>> = None;
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    loop {
      // 每轮是一批 scan + 删除；批之间也检查段预算，避免超大表连续占线程。
      yield_budget.maybe_yield().await;
      let entry_li = {
        let Some(snapshot) = self.snapshot(tbl) else {
          return Ok(deleted);
        };
        if snapshot.root_id() == NULL_PAGE {
          return Ok(deleted);
        }
        let state = self.table_map.get(tbl);
        scan_entries_with_budget(
          &self.io,
          ScanArg {
            tbl,
            snapshot: &snapshot,
            tree: ScanTree::Data,
            begin: position,
            end: &[],
            page_size: self.page_size,
            max_entries: TRUNCATE_FROM_BATCH,
            dict: state.and_then(|state| state.compression.as_ref()),
            index_cache: state.and_then(|state| state.index_cache.as_ref()),
            index_flight: Some(&self.index_flight),
            ttl_filter_now_ms: None,
          },
          ScanOption {
            reject_large_value: false,
            keys_only: true,
            ..ScanOption::default()
          },
          self.yield_budget_us,
        )
        .await?
      };
      let Some(first) = entry_li.first() else {
        break;
      };
      // 删除未生效时首 key 不会前进：中止而不是空转（防御写路径回归）。
      if prev_first.as_deref() == Some(first.key.as_slice()) {
        log::warn!(
          "truncate_from 扫描无进展，提前结束 tbl={tbl} key_len={}",
          first.key.len()
        );
        break;
      }
      prev_first = Some(first.key.clone());
      let round = entry_li.len();
      let delete_li: Vec<BatchEntry> = entry_li
        .into_iter()
        .map(|entry| BatchEntry::delete(entry.key, u64::MAX))
        .collect();
      self.put(tbl, &delete_li).await?;
      deleted += round;
      // 未扫满一轮说明已到末尾，省掉一次空扫。
      if round < TRUNCATE_FROM_BATCH {
        break;
      }
    }
    Ok(deleted)
  }
}
