//! 归档水位：块位上界累计，以及活跃 / 冷态下 data / segment 追加水位的重建。

use fkv_error::{KvError, Result};
use fkv_io::{ManifestIo, PageIo};
use fkv_manifest::BranchManifestMetadata;
use fkv_mapping::PageMapper;
use fkv_recovery::ReplayState;
use fkv_task_read::YieldBudget;
use fkv_types::{
  FileId, FilePageId, MAIN_BRANCH_NAME, TableIdent, decode_id, is_file_page_id,
  parse_manifest_file_name,
};

use super::Store;
use crate::{
  BRANCH_FILE_RESERVE_FILE_NUM,
  manifest::{
    branch_block_base, branch_block_end, manifest_data_ceiling, manifest_segment_ceiling,
  },
};

/// 把单份 manifest 已承诺的 data / segment 块上界并入累计值。
///
/// 解析失败不能静默跳过（无法判断它占了哪些块），直接 fail-fast 返回
/// `Corrupted`；legacy 无区间链时按高水位上取整并告警，比现状多推进一个块，
/// 宁浪费不重叠。
pub(super) fn accumulate_manifest_ceiling(
  name: &str,
  bytes: &[u8],
  shift: u8,
  segment_shift: u8,
  data_ceiling: &mut FileId,
  segment_ceiling: &mut Option<FileId>,
) -> Result<()> {
  let Some((name_branch, _term, _tag)) = parse_manifest_file_name(name) else {
    return Ok(());
  };
  let (state, _) = match fkv_task_reopen::replay_manifest_bytes_with_len(bytes) {
    Ok(replayed) => replayed,
    Err(err) => {
      log::warn!("块位分配: manifest 解析失败 name={name} err={err}");
      return Err(KvError::Corrupted);
    }
  };
  let metadata = &state.branch_metadata;
  // legacy（区间链里没有父前缀）只能按高水位回退；这不是正常路径，不能
  // 静默按 0 处理。main 的 manifest 本来就没有父前缀，不告警。
  if branch_block_end(metadata, metadata.branch_name.as_ref(), false).is_none()
    && name_branch != MAIN_BRANCH_NAME
  {
    log::warn!(
      "块位分配: legacy branch manifest 缺少可推导的区间链，按高水位上取整 \
       name={name} max_fp_id={}",
      state.max_fp_id
    );
  }
  let data_end =
    manifest_data_ceiling(metadata, state.max_fp_id, shift).ok_or(KvError::InvalidArgs)?;
  *data_ceiling = (*data_ceiling).max(data_end);
  if let Some(segment_end) =
    manifest_segment_ceiling(metadata, state.max_segment_fp_id, segment_shift)
  {
    *segment_ceiling = Some(segment_ceiling.map_or(segment_end, |end| end.max(segment_end)));
  }
  Ok(())
}

/// 本分支预留块 `[块基址, 块末尾)`（页 id 半开区间，`segment` 选 data / 段
/// 侧面）。区间链推不出块基址（legacy 无父前缀 / main）或移位溢出时 `None`，
/// 由调用方回退保守口径。
fn branch_block_span(
  metadata: &BranchManifestMetadata,
  branch: &str,
  shift: u8,
  segment: bool,
) -> Option<(FilePageId, FilePageId)> {
  let base = branch_block_base(metadata, branch, segment)?;
  let end = base.checked_add(BRANCH_FILE_RESERVE_FILE_NUM)?;
  let shift = u32::from(shift);
  Some((base.checked_shl(shift)?, end.checked_shl(shift)?))
}

/// 冷态水位扫描的 data / segment 侧面。
#[derive(Clone, Copy)]
enum WatermarkSide {
  Data,
  Segment,
}

impl<I: PageIo + ManifestIo> Store<I> {
  /// 活跃状态当前的段（大 value）追加水位，即下一次段分配的 fp id。
  ///
  /// 取两个来源的较大者：本 Store 内存 segment mapper（表已打开时的权威值）
  /// 与盘上活跃 manifest（刚 attach / 另一 owner 可能留下更高水位）。具名
  /// branch 读固定文件；main 的活跃 manifest 不在具名文件里，先读本地 cache，
  /// Cloud 冷进程 cache 为空时回源远端一次（`ExpiredTerm` 表示本实例已是
  /// 过期 owner，必须向上返回，不能把失败伪装成「没有水位」）。
  ///
  /// `None` 表示两处都没有活跃状态（例如 `drop_table` 之后）：调用方不能用
  /// 归档水位代替——归档 / 子分支 manifest 仍可能引用更高水位的段页。
  /// 冷态（`None`）的段页证据由 [`Self::cold_segment_watermark`] 重建。
  pub(super) async fn active_segment_watermark(
    &self,
    tbl: &TableIdent,
  ) -> Result<Option<FilePageId>> {
    let mapper_max = self
      .table_map
      .get(tbl)
      .and_then(|state| state.segment_mapper.as_ref())
      .map(PageMapper::max_fp_id);
    let bytes = if self.manifest_file.is_some() {
      // branch 活跃名带 attach term（sem-60）：首写发布前远端只有旧 term
      // 对象，必须走回退链才能读到采纳的内容，且不能把回退 miss 伪装成
      // 「没有水位」——种子被读路径逐出后还能重新装上。
      self
        .io
        .read_branch_manifest(tbl, &self.branch_name, self.term)
        .await?
    } else {
      match self.io.read_local_manifest(tbl).await? {
        Some(bytes) => Some(bytes),
        None => self.io.read_manifest(tbl).await?,
      }
    };
    let manifest_max = bytes
      .map(|bytes| {
        fkv_task_reopen::replay_manifest_bytes_with_len(&bytes)
          .map(|(replay, _)| replay.max_segment_fp_id)
      })
      .transpose()?;
    Ok(match (mapper_max, manifest_max) {
      (Some(mapper_max), Some(manifest_max)) => Some(mapper_max.max(manifest_max)),
      (mapper_max, manifest_max) => mapper_max.or(manifest_max),
    })
  }

  /// 冷态重建本分支的段追加水位：只承认**存活 manifest 仍引用**的段页。
  ///
  /// [`Self::active_segment_watermark`] 两处都为 `None` 时（新进程、分区被
  /// 驱逐、`delete_branch` 已删 branch manifest），归档自带的
  /// `max_segment_fp_id` 不能单独作为「归档引用了段页」的证据：生产分区恒
  /// 配置 large value，`create_branch` 会把子分支段块基址写进 branch
  /// manifest，**纯数据**分支的归档因此也带 `> 0` 的段水位（= 段块基址、
  /// segment mapping 为空）。只看 `> 0` 会让 sem-28 留下的孤儿归档在默认
  /// 配置下永远不可恢复（pooled-13）。
  ///
  /// 判据改为段页证据本身，扫描本表全部存活 manifest：
  /// - 本分支自己的 manifest（更晚的归档 / 重建的 branch manifest）的
  ///   `max_segment_fp_id`；
  /// - 任意 manifest 的 segment mapping 里**物理段页 id** 落在本分支段块
  ///   `[块基址, 块末尾)` 的段页——子 / 孙分支继承父分支的大 value 段引用，
  ///   这些页仍被它们的 manifest 引用。
  ///
  /// 返回值是下次段分配的下界，`0` 表示没有任何段页证据。证据越过本分支段块
  /// 末尾（越界 / legacy manifest）返回 `ResourceMissing`：既不能当水位（后续
  /// 段写会被 `branch_segment_limit` 永久挡成 `OutOfSpace`），也无法证明越界页
  /// 不再被引用。区间链推不出本分支段块时保持既有口径：归档自身带段体系即
  /// fail-stop。main 没有预留块（`drop_table` 后的 fail-stop 见 pooled-12），
  /// 调用方不会走到这里。
  pub(super) async fn cold_segment_watermark(
    &self,
    tbl: &TableIdent,
    replay: &ReplayState,
    branch: &str,
  ) -> Result<FilePageId> {
    let Some(span) = branch_block_span(&replay.branch_metadata, branch, self.segment_shift(), true)
    else {
      return if replay.max_segment_fp_id > 0 || !replay.segment_mapping.is_empty() {
        Err(KvError::ResourceMissing)
      } else {
        Ok(0)
      };
    };
    let (block_start, _) = span;
    let watermark = self
      .cold_watermark_scan(tbl, replay, branch, span, WatermarkSide::Segment)
      .await?;
    // 段页只能落在本分支预留块内（`branch_segment_limit` 是上界）：归档没有
    // 段段（如 C++ 产物）而存活 manifest 有本块内引用时，水位至少抬到块基址，
    // 后续分配才不会落进上一个块（父 / 兄弟的段空间）。
    let has_segment =
      replay.max_segment_fp_id > 0 || !replay.segment_mapping.is_empty() || watermark > 0;
    Ok(if has_segment {
      watermark.max(block_start)
    } else {
      0
    })
  }

  /// 冷态重建本分支的 data 追加水位（仅 Append 需要）。
  ///
  /// Append 没有空闲池：`reopen_archive` 只回滚可见状态，data 追加水位必须
  /// 留在盘上已写完的位置之后（pooled-11）。活跃状态不存在时（branch
  /// manifest 已被 `delete_branch` 删除后的冷重开），从存活 manifest 重建
  /// 同口径的保守水位：
  /// - 本分支自己的 manifest（更晚的归档 / 重建的 branch manifest）的
  ///   `max_fp_id`；
  /// - 任意 manifest 的 data mapping 里物理页落在本分支 data 块内的引用
  ///   （子 / 孙分支继承父分支的页）。
  ///
  /// 证据越过本分支 data 块末尾（越界 / legacy manifest）返回
  /// `ResourceMissing`。块推不出来（legacy / main）时返回 `None`，调用方
  /// 保持既有口径（main 交给 `rebuild_reuse_floor` 的块末尾 reserve）。
  pub(super) async fn cold_data_watermark(
    &self,
    tbl: &TableIdent,
    replay: &ReplayState,
    branch: &str,
  ) -> Result<Option<FilePageId>> {
    let Some(span) = branch_block_span(
      &replay.branch_metadata,
      branch,
      self.pages_per_file_shift,
      false,
    ) else {
      return Ok(None);
    };
    Ok(Some(
      self
        .cold_watermark_scan(tbl, replay, branch, span, WatermarkSide::Data)
        .await?,
    ))
  }

  /// 扫描本表全部存活 manifest，按 `side` 统计冷态水位证据。
  ///
  /// 返回该侧的下一次分配下界（调用方再决定是否抬到块基址）：本分支
  /// manifest 自报水位与 `[min(块基址, 归档水位), 块末尾)` 内任意 manifest
  /// 引用页的 `页 + 1` 取最大。本分支 manifest 越过块末尾视为越界，返回
  /// `ResourceMissing`。
  async fn cold_watermark_scan(
    &self,
    tbl: &TableIdent,
    replay: &ReplayState,
    branch: &str,
    (block_start, block_limit): (FilePageId, FilePageId),
    side: WatermarkSide,
  ) -> Result<FilePageId> {
    let archive_max = match side {
      WatermarkSide::Data => replay.max_fp_id,
      WatermarkSide::Segment => replay.max_segment_fp_id,
    };
    // 证据窗口下界不能固定为派生块基址（sem-36）：C++ / legacy 区间链未按
    // 预留块对齐时，归档水位可能低于派生块基址，子 / 孙分支继承的页落在
    // `[归档水位, 块基址)`。低于归档水位的页不会被续写（分配水位只前进）。
    let evidence_start = block_start.min(archive_max);
    let mut watermark = 0u64;
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    for name in self.io.list_manifest_file_name_li(tbl).await? {
      yield_budget.maybe_yield().await;
      // 列举与读取之间被删除：已无引用，不参与水位。
      let Some(bytes) = self.io.read_manifest_file(tbl, &name).await? else {
        continue;
      };
      let (state, _) = fkv_task_reopen::replay_manifest_bytes_with_len(&bytes)?;
      let (max_fp_id, mapping) = match side {
        WatermarkSide::Data => (state.max_fp_id, &state.mapping_tbl),
        WatermarkSide::Segment => (state.max_segment_fp_id, &state.segment_mapping),
      };
      // 本分支的 manifest 与归档共用同一个块（区间链前缀在 branch 创建时
      // 固定），水位可以直接作为候选；越过块末尾说明越界，不能当水位。
      if parse_manifest_file_name(&name).is_some_and(|(name_branch, ..)| name_branch == branch) {
        if max_fp_id > block_limit {
          match side {
            WatermarkSide::Data => log::warn!(
              "冷态 data 水位: 本分支 manifest 水位越过预留块 name={name} branch={branch} \
               max_fp_id={max_fp_id} block_end={block_limit}"
            ),
            WatermarkSide::Segment => log::warn!(
              "冷态段水位: 本分支 manifest 段水位越过预留块 name={name} branch={branch} \
               seg_max={max_fp_id} block_end={block_limit}"
            ),
          }
          return Err(KvError::ResourceMissing);
        }
        watermark = watermark.max(max_fp_id);
      }
      for page in mapping
        .iter_value_li()
        .filter(|value| is_file_page_id(*value))
        .map(decode_id)
        .filter(|page| (evidence_start..block_limit).contains(page))
      {
        watermark = watermark.max(page.saturating_add(1));
      }
    }
    Ok(watermark)
  }
}
