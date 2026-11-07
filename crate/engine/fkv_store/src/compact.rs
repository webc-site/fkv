//! data / segment 文件 compaction 与写后维护入口。

use fkv_error::{KvError, Result};
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_mapping::{AllocMode, PageMapper};
use fkv_types::{FileId, NULL_PAGE, TableIdent};

use crate::{Store, batch::MapperBatch};

impl<I: PageIo + ManifestIo> Store<I> {
  pub async fn compact_file(
    &mut self,
    tbl: &TableIdent,
    file_id: fkv_types::FileId,
    pages_per_file_shift: u8,
  ) -> Result<usize> {
    self
      .compact_file_li(tbl, &[file_id], pages_per_file_shift)
      .await
  }

  /// 一次 compaction 搬多个文件：只开一个 mapper 批次、落一次 manifest，
  /// 对齐 C++ `DoCompactDataFile` 覆盖 `begin_file_id..end_file_id` 的行为。
  pub(crate) async fn compact_file_li(
    &mut self,
    tbl: &TableIdent,
    file_li: &[fkv_types::FileId],
    pages_per_file_shift: u8,
  ) -> Result<usize> {
    let page_size = self.page_size;
    // 搬页走裸 IO（不过写路径装饰器）写新物理页，Pooled 下可能落到缓存里仍
    // 有条目的 id 上；本调用内部也没有写路径读，因此**在开批前**整体作废、
    // 覆盖成功 / 中途 IO 失败 / 零搬迁全部出口，不留一条陈旧条目。
    self.clear_write_cache(tbl);
    let (moved, batch) = {
      let mapper = self
        .table_map
        .get_mut(tbl)
        .and_then(|state| state.mapper.as_mut())
        .ok_or(KvError::ResourceMissing)?;
      let batch = MapperBatch {
        data: mapper.begin_batch(),
        segment: false,
      };
      let moved = match fkv_task_compact::compact_file_li_budget(
        &self.io,
        tbl,
        mapper,
        file_li,
        page_size,
        pages_per_file_shift,
        self.yield_budget_us,
      )
      .await
      {
        // 提交推迟到 manifest 落盘成功之后（core-25）：失败时 journal 仍在，
        // `settle_mapper_batch` 的 abort 才能回滚这次物理搬迁。
        Ok(count) => count,
        Err(err) => {
          if batch.data {
            mapper.abort_batch();
          }
          return Err(err);
        }
      };
      // 搬空的文件并入分配器空间统计，对齐 C++ `UpdateStat(min_file_id, empty_file_cnt)`。
      mapper.update_stat_by_scan(pages_per_file_shift);
      (moved, batch)
    };
    // 没有搬迁就没有 mapping 变化：不要为 open_empty 的空表追加一条 log
    // （manifest 首条记录必须是 snapshot，否则 reopen 解析失败）。
    if moved == 0 {
      self.settle_mapper_batch(tbl, batch, true);
      return Ok(0);
    }
    let state = self.table_map.get(tbl);
    let root_id = state.and_then(|state| state.root).unwrap_or(NULL_PAGE);
    let ttl_root_id = state.map_or(NULL_PAGE, |state| state.ttl_root);
    // 与 put / clean_expired 同序：搬迁页先落盘，再追加 manifest（eng-89）。
    // 否则 manifest 已持久而新页仍在 page cache，崩溃后存活页会指向垃圾 / 零页，
    // 且随后的 gc 可能已删掉旧文件里唯一已落盘的副本。
    let log_result = async {
      self.io.sync_data(tbl).await?;
      self.append_mapping_log(tbl, root_id, ttl_root_id).await
    }
    .await;
    // compact 只换物理 `FilePageId`，逻辑页内容不变，索引缓存仍有效；
    // 若留到下一次 put，`invalidate_index_cache` 会把搬迁过的热点页成批逐出（eng-68）。
    // 搬迁使 mapper 代际前进：提交成功后把仍有效条目重标到新代（27b）。
    if log_result.is_ok()
      && let Some(state) = self.table_map.get_mut(tbl)
    {
      let generation = state.mapper.as_ref().map(PageMapper::generation);
      if let Some(mapper) = state.mapper.as_mut() {
        mapper.clear_changed_page_li();
      }
      if let (Some(generation), Some(cache)) = (generation, state.index_cache.as_ref()) {
        cache.borrow_mut().retag(generation);
      }
    }
    self.settle_mapper_batch(tbl, batch, log_result.is_ok());
    log_result?;
    // 搬迁把旧物理页 retire 出活跃映射；批次已结算、manifest 已落盘，按快照
    // 水位回池（pooled-04）。放在这里而不是只靠 `maintain`：显式
    // `Request::Compact` 不保证随后有人驱动维护，纯 compact 负载会让
    // `retired_li` 只增不减、`max_fp_id` 每轮翻倍。
    self.reclaim_retired_now(tbl);
    Ok(moved)
  }
}

/// 放大率尾部判定（[`Store::needs_maintain`] / [`Store::needs_segment_maintain`]
/// 共用）：活映射清空时任何已占空间都触发；否则需已分配空间不少于一个
/// 文件（尾文件未写满不计）且严格超过 `mapped * factor`。
fn amplify_exceeds(mapper: &PageMapper, shift: u8, factor: u64) -> bool {
  let mapped = mapper.mapping_count() as u64;
  let space = mapper.space_size(shift);
  if mapped == 0 {
    return space != 0;
  }
  space >= 1u64 << shift && space > mapped.saturating_mul(factor)
}

impl<I: PageIo + ManifestIo + FileIo + SegmentIo> Store<I> {
  /// 自动放大率 compaction 是否对本 Store 生效：只在 `Append` 分配模式下。
  ///
  /// 对齐 C++：`WriteTask::UpdateMeta` 只在 `data_append_mode` 下评估 data 侧
  /// 放大率，`BackgroundWrite::Compact` 在非 append 模式跳过
  /// `DoCompactDataFile` —— 其 `min_file_id` / `empty_file_cnt` 统计模型依赖
  /// 分配器高水位只增不减且能按文件边界回落，Pooled 分配器不满足。
  ///
  /// fkv 的 Pooled `space_size()` 退化为 `max_fp_id`，且 `update_stat` /
  /// `skip_to_next_file` 对它都是 no-op（没有任何缩减路径）。若不做该 gate，
  /// 清空 / 大量删除后放大率判定恒为真，每次写都会跑全量 `maintain`
  /// （data compact + segment compact + file_gc + 空分区清理，pooled-03）。
  /// Pooled 的空闲页复用由 reclaim 承担，自动 compaction 的主要收益
  /// （append 模式的放大率收敛）在 Pooled 下不存在。
  pub(crate) fn amplify_compact_enabled(&self) -> bool {
    self.alloc_mode == AllocMode::Append
  }

  /// 写后维护的 O(1) 预判（对齐 C++ `WriteTask::MapperExceedsAmplification`）：
  /// 只在放大率可能超阈值时返回 true，避免每次写都做 mapping 全量扫描 + 目录列举。
  ///
  /// 触发条件（`file_amplify_factor > 0` 且分配模式为 `Append` 时）：
  /// - 活映射已清空但仍占 file page 空间（全死文件）→ 交给 `maintain` 的 `gc` 回收；
  /// - 已分配空间不少于一个文件（`space >= pages_per_file`，尾文件未写满不计）
  ///   且 `space > mapped * factor`。
  ///
  /// `mapped` 用 `MappingCount()`（活跃映射数），`space` 用 `SpaceSize()`
  /// （扣除整文件空隙），对齐 C++ `WriteTask::MapperExceedsAmplification`：
  /// 稀疏 mapping 里已释放的逻辑页不会把放大率稀释。
  pub fn needs_maintain(&self, tbl: &TableIdent) -> bool {
    let factor = u64::from(self.file_amplify_factor);
    if factor == 0 || !self.amplify_compact_enabled() {
      return false;
    }
    let Some(mapper) = self
      .table_map
      .get(tbl)
      .and_then(|state| state.mapper.as_ref())
    else {
      return false;
    };
    amplify_exceeds(mapper, self.pages_per_file_shift, factor)
  }

  /// 对齐 C++ `WriteTask::CompactIfNeeded`：按 `file_amplify_factor` 选出
  /// 全部超阈值文件并一次性搬完；返回被 compact 的文件列表。
  ///
  /// 仅 `Append` 模式生效（见 [`Self::amplify_compact_enabled`]）；Pooled 下
  /// 恒返回空，显式 [`Self::compact_file`] 请求不受影响。
  ///
  /// 活映射为空时不再搬页，而是把分配器 high-water 推到下一文件边界
  /// （对齐 C++ `DoCompactDataFile` 的 `fp_ids.empty()` 分支），
  /// 避免 `needs_maintain` 每轮都为全死空间触发一次全表扫描。
  pub async fn compact_if_needed(&mut self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    if !self.needs_maintain(tbl) {
      return Ok(Vec::new());
    }
    let factor = self.file_amplify_factor;
    let pages_per_file = 1u64 << self.pages_per_file_shift;
    let plan_li = {
      let Some(mapper) = self
        .table_map
        .get(tbl)
        .and_then(|state| state.mapper.as_ref())
      else {
        return Ok(Vec::new());
      };
      if mapper.mapping_count() == 0 {
        // 全死映射：跳到下一文件边界，剩余死文件由随后的 `gc` 回收。
        let mapper = self
          .table_map
          .get_mut(tbl)
          .and_then(|state| state.mapper.as_mut())
          .ok_or(KvError::ResourceMissing)?;
        mapper.skip_empty_space(self.pages_per_file_shift);
        return Ok(Vec::new());
      }
      let live_map = fkv_gc::file_live_count(mapper.tbl(), self.pages_per_file_shift);
      let candidate_li = self.io.list_file_id_li(tbl).await?;
      // 当前追加尾文件尚未写满，不能按容量算放大率，否则 compact 出的稀疏
      // 尾页会再次触发抖动。尾 id 按分配器高水位算，不依赖盘上最大 id。
      fkv_gc::compact_plan(
        &live_map,
        &candidate_li,
        pages_per_file,
        factor,
        mapper.max_fp_id() >> self.pages_per_file_shift,
      )
    };
    if plan_li.is_empty() {
      return Ok(Vec::new());
    }
    self
      .compact_file_li(tbl, &plan_li, self.pages_per_file_shift)
      .await?;
    Ok(plan_li)
  }

  /// segment 侧的写后 O(1) 预判，规则同 [`Self::needs_maintain`]。
  ///
  /// 空间与活跃映射都按 segment 槽位计（`segments_per_file_shift`），
  /// 未启用大 value 或阈值为 0 时恒为 false。
  ///
  /// **不受 data 侧 `alloc_mode` 门控**：段 mapper 恒为 `Append`
  /// （pooled-02；构造点 `write.rs` / `archive.rs` / `manifest.rs`），其
  /// `space_size` / `update_stat_by_scan` 的 append 口径成立。C++ 的段 mapper
  /// 恒为 `AppendAllocator`（`EnsureSegmentMapper`），段 compaction 只按
  /// `segment_file_amplify_factor` 评估，与 `data_append_mode` 无关
  /// （`WriteTask::UpdateMeta` / `BackgroundWrite::Compact`）。
  ///
  /// 仍按段 mapper 自身的分配模式做一次防御：若将来误把段 mapper 构造为
  /// `Pooled`，`space_size()` 退化为 `max_fp_id` 且无缩减路径，放大率会恒真
  /// （pooled-03 的成因），此时宁可关闭段 compaction。
  pub fn needs_segment_maintain(&self, tbl: &TableIdent) -> bool {
    let factor = u64::from(self.segment_file_amplify_factor);
    if factor == 0 {
      return false;
    }
    let Some(layout) = self.large_value_layout else {
      return false;
    };
    let Some(mapper) = self
      .table_map
      .get(tbl)
      .and_then(|state| state.segment_mapper.as_ref())
    else {
      return false;
    };
    if mapper.alloc_mode() != AllocMode::Append {
      return false;
    }
    amplify_exceeds(mapper, layout.segments_per_file_shift(), factor)
  }

  /// segment 文件压缩候选：按 segment 槽位算放大率，跳过追加尾文件。
  pub(crate) async fn segment_compact_plan(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    if self.segment_file_amplify_factor == 0 {
      return Ok(Vec::new());
    }
    let Some(layout) = self.large_value_layout else {
      return Ok(Vec::new());
    };
    let Some(mapper) = self
      .table_map
      .get(tbl)
      .and_then(|state| state.segment_mapper.as_ref())
    else {
      return Ok(Vec::new());
    };
    let shift = layout.segments_per_file_shift();
    let live_map = fkv_gc::file_live_count(mapper.tbl(), shift);
    let candidate_li = self.io.list_segment_file_id_li(tbl).await?;
    // 尾文件按分配器高水位算：`max_fp_id` 是下一个待分配 id，
    // 它落在的文件才是当前追加目标；盘上最大 id 可能更小（空文件已被 GC 删）。
    Ok(fkv_gc::segment_compact_plan(
      &live_map,
      &candidate_li,
      1u64 << shift,
      self.segment_file_amplify_factor,
      mapper.max_fp_id() >> shift,
    ))
  }

  /// 对齐 C++ `DoCompactSegmentFile`：把超阈值 segment 文件里的 live 段搬到文件尾，
  /// 返回被 compact 的 segment file 列表。
  ///
  /// 全死映射没有可搬的段，直接交给 `maintain` 随后的 `gc` 回收旧文件。
  pub async fn compact_segment_if_needed(&mut self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    if !self.needs_segment_maintain(tbl) {
      return Ok(Vec::new());
    }
    let plan_li = self.segment_compact_plan(tbl).await?;
    if plan_li.is_empty() {
      return Ok(Vec::new());
    }
    self.compact_segment_file_li(tbl, &plan_li).await?;
    Ok(plan_li)
  }

  /// 一次 compaction 搬多个 segment 文件：一个 mapper 批次、落一次 manifest。
  ///
  /// 段内容不变，只换物理 `FilePageId`，manifest 只追加 segment mapping 增量；
  /// 搬空的旧文件由下一次 `gc` 回收，仍被其它 branch manifest 引用的文件会保留。
  pub(crate) async fn compact_segment_file_li(
    &mut self,
    tbl: &TableIdent,
    file_li: &[FileId],
  ) -> Result<usize> {
    let Some(layout) = self.large_value_layout else {
      return Ok(0);
    };
    let segment_size = layout.segment_size() as usize;
    let segment_shift = layout.segments_per_file_shift();
    let (moved, batch) = {
      let mapper = self
        .table_map
        .get_mut(tbl)
        .and_then(|state| state.segment_mapper.as_mut())
        .ok_or(KvError::ResourceMissing)?;
      let batch = MapperBatch {
        data: false,
        segment: mapper.begin_batch(),
      };
      let moved = match fkv_task_compact::compact_segment_file_li_budget(
        &self.io,
        tbl,
        mapper,
        file_li,
        segment_size,
        segment_shift,
        self.yield_budget_us,
      )
      .await
      {
        // 提交推迟到 manifest 落盘成功之后（core-25）：失败时 abort 才能回滚搬迁。
        Ok(count) => count,
        Err(err) => {
          if batch.segment {
            mapper.abort_batch();
          }
          return Err(err);
        }
      };
      mapper.update_stat_by_scan(segment_shift);
      (moved, batch)
    };
    let state = self.table_map.get(tbl);
    let root_id = state.and_then(|state| state.root).unwrap_or(NULL_PAGE);
    let ttl_root_id = state.map_or(NULL_PAGE, |state| state.ttl_root);
    // 同 compact_file_li / put：段搬迁先 sync_data 再落 manifest（eng-89）。
    let log_result = async {
      self.io.sync_data(tbl).await?;
      self.append_mapping_log(tbl, root_id, ttl_root_id).await
    }
    .await;
    // 段内容未变，只换物理位置：不能当成内容变更把相关缓存成批逐出。
    if log_result.is_ok()
      && let Some(mapper) = self
        .table_map
        .get_mut(tbl)
        .and_then(|state| state.segment_mapper.as_mut())
    {
      mapper.clear_changed_page_li();
    }
    self.settle_mapper_batch(tbl, batch, log_result.is_ok());
    log_result?;
    Ok(moved)
  }

  /// 写后维护钩子：把放大文件 compact 一轮，再回收无引用文件。
  /// Runtime / shard 应在写批次之后调用（C++ 由 WriteTask 内部触发）。
  ///
  /// Pooled 分配模式下 data 侧自动放大 compaction 被 gate 掉（见
  /// [`Self::amplify_compact_enabled`]），但 `file_gc` / 空分区清理仍会执行：
  /// 显式 `FileGc`、`truncate` 之外的维护触发（如 TTL 清理）与归档路径
  /// 仍需要它们回收零引用文件。段侧不受该 gate 影响（段 mapper 恒为
  /// `Append`，pooled-02）。
  ///
  /// 末尾补一次 retired 回收（pooled-04）：data compaction 搬迁页会 retire
  /// 旧物理页，而维护负载下不保证随后有 `put` 来驱动回收；本步在
  /// `compact_*` 的 mapper 批次结算之后执行，满足「批次之外」约束。
  pub async fn maintain(&mut self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    let compacted = self.compact_if_needed(tbl).await?;
    self.compact_segment_if_needed(tbl).await?;
    self.file_gc(tbl, self.pages_per_file_shift).await?;
    self.refresh_space_stat(tbl);
    // 空分区收尾失败只告警不上抛（io-31）：与 `file_gc` 收尾同口径（C++
    // `ExecuteLocalGC` 对目录清理失败仅 WARNING，file_gc.cpp:364-371），
    // 收尾类目录 IO 失败推迟到下一轮即可，不得中断其后的 retired 页回池。
    // `cleanup_empty_partition` 本身保持 `Result<bool>`：错误是否上抛由
    // 调用方按语义决定。
    if let Err(err) = self.cleanup_empty_partition(tbl).await {
      log::warn!("maintain 空分区收尾失败 tbl={tbl} err={err}");
    }
    self.reclaim_retired_now(tbl);
    Ok(compacted)
  }
}
