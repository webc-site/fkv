//! retained 集合汇总：mapping / manifest replay 与 active branch guard 折入。

/// 活跃 manifest 在损坏记录里的占位名（它不一定有具名文件）。
const ACTIVE_MANIFEST_TAG: &str = "<active>";

use fkv_error::Result;
use fkv_io::ManifestIo;
use fkv_mapping::{MappingTbl, PageMapper};
use fkv_task_read::YieldBudget;
use fkv_types::{FileId, FilePageId, PageId, TableIdent, decode_id, is_file_page_id};
use gxhash::{HashMap, HashSet};

use crate::inflight::{FileKind, InflightGuard};

/// 单个 mapping 槽位的分组判定：非 file page 或不属于候选文件则丢弃。
///
/// 同步 / 带预算两条扫描路径共用，避免分组规则在让出版里分叉。
///
/// 分组值为 `(旧 fp, 逻辑页)` 对而非裸 `PageId`：compaction 的批内搬移序
/// 按旧 fp 升序排（对齐 C++ `FilePageLess` 堆排），fp 必须随分组一并产出，
/// 避免调用方再查一遍 mapping（sem-60c）。
#[inline]
fn push_live_page(
  page_map: &mut HashMap<FileId, Vec<(FilePageId, PageId)>>,
  wanted: &HashSet<FileId>,
  page_id: PageId,
  value: FilePageId,
  pages_per_file_shift: u8,
) {
  if !is_file_page_id(value) {
    return;
  }
  let fp_id = decode_id(value);
  let file_id = fp_id >> pages_per_file_shift;
  if wanted.contains(&file_id) {
    page_map.entry(file_id).or_default().push((fp_id, page_id));
  }
}

/// 一次遍历 mapping，按候选文件分组 live 逻辑页（组内按 mapping 迭代序，
/// 即逻辑页升序；compaction 侧使用前需重排为旧 fp 升序）。
///
/// compaction 计划覆盖多文件时，避免每个文件各扫一遍 mapping（O(F*N) → O(N)）。
pub fn live_page_li_by_file(
  mapper: &PageMapper,
  file_li: &[FileId],
  pages_per_file_shift: u8,
) -> HashMap<FileId, Vec<(FilePageId, PageId)>> {
  let wanted: HashSet<FileId> = file_li.iter().copied().collect();
  let mut page_map: HashMap<FileId, Vec<(FilePageId, PageId)>> = HashMap::default();
  for (page_id, value) in mapper.tbl().iter() {
    push_live_page(&mut page_map, &wanted, page_id, value, pages_per_file_shift);
  }
  page_map
}

/// 同 [`live_page_li_by_file`]，但按 [`YieldBudget`] 在扫描中途协作让出。
///
/// 全表 mapping 可达数十万页，纯 CPU 的扫描段会长时间占住 shard 事件循环，
/// 拖慢同 shard 的高优先读（l2-09）。扫描是纯读：`page_map` 只在本函数内增量
/// 构建，让出点前后不存在可被外部观测的半成品状态，恢复后继续同一迭代器即可。
pub async fn live_page_li_by_file_budget(
  mapper: &PageMapper,
  file_li: &[FileId],
  pages_per_file_shift: u8,
  yield_budget_us: u64,
) -> HashMap<FileId, Vec<(FilePageId, PageId)>> {
  let wanted: HashSet<FileId> = file_li.iter().copied().collect();
  let mut page_map: HashMap<FileId, Vec<(FilePageId, PageId)>> = HashMap::default();
  let mut yield_budget = YieldBudget::new(yield_budget_us);
  for (seq, (page_id, value)) in mapper.tbl().iter().enumerate() {
    // retained / 全表 mapping 可能非常大，按步长检查段预算（同 GC）。
    yield_budget.maybe_yield_every(seq).await;
    push_live_page(&mut page_map, &wanted, page_id, value, pages_per_file_shift);
  }
  page_map
}

pub fn add_mapping_retained(mapping: &MappingTbl, file_page_set: &mut HashSet<FilePageId>) {
  // 统一实现在 fkv_mapping：GC / 快照注册表共用同一份活跃槽位判定。
  fkv_mapping::add_mapping_retained(mapping, file_page_set);
}

/// 应用一份 manifest 的 retained page 与区间 / 高水位信息。
fn apply_manifest(
  bytes: &[u8],
  data_set: &mut HashSet<FilePageId>,
  segment_set: &mut HashSet<FilePageId>,
  guard: &mut InflightGuard,
  pages_per_file_shift: u8,
  segments_per_file_shift: u8,
) -> Result<()> {
  let state = fkv_task_reopen::replay_manifest_bytes(bytes)?;
  // 引用归属先于并集：本 manifest 的 retained file id 经**本 manifest**的
  // 区间链解析 (branch, term)（对齐 C++ `GetRetainedFiles` 的 per-manifest
  // 归属；各 manifest 的链视图不同，跨 manifest 混用会归错变体）。
  let mut data_pages = HashSet::default();
  add_mapping_retained(&state.mapping_tbl, &mut data_pages);
  guard.note_refs(
    FileKind::Data,
    &state.branch_metadata.file_range_li,
    data_pages.iter().map(|fp| fp >> pages_per_file_shift),
  );
  data_set.extend(data_pages);
  let mut segment_pages = HashSet::default();
  add_mapping_retained(&state.segment_mapping, &mut segment_pages);
  guard.note_refs(
    FileKind::Segment,
    &state.branch_metadata.file_range_li,
    segment_pages.iter().map(|fp| fp >> segments_per_file_shift),
  );
  segment_set.extend(segment_pages);
  let metadata = &state.branch_metadata;
  guard.add_range_li(&metadata.file_range_li);
  guard.fold(
    &metadata.branch_name,
    metadata.term,
    Some(state.max_fp_id >> pages_per_file_shift),
    (state.max_segment_fp_id > 0).then_some(state.max_segment_fp_id >> segments_per_file_shift),
  );
  Ok(())
}

/// 汇总 active manifest + 所有 branch / archive manifest 的 retained page。
///
/// 单个 manifest 损坏 / 不可读只记入 `InflightGuard::damaged_li` 并跳过，
/// 不再让整轮 GC 返回 `Err`；此时引用集合不完整，`retain` 会保守保留全部
/// 候选文件，绝不误删仍被引用的数据。
///
/// 列举得到、随后读取为 `None` 的名字同样按 damaged 处理：文件在列举与读取
/// 之间消失，可能是别的 writer 正常删除，也可能是后端临时不可见；此时该
/// manifest 的 retained / guard 贡献同时缺失，按竞态跳过会误删仍被引用的
/// 活文件，只有全量保留才是安全侧。
pub async fn manifest_retained_file_page_set<I: ManifestIo>(
  io: &I,
  tbl: &TableIdent,
  pages_per_file_shift: u8,
  segments_per_file_shift: u8,
) -> Result<(HashSet<FilePageId>, HashSet<FilePageId>, InflightGuard)> {
  manifest_retained_file_page_set_budget(io, tbl, pages_per_file_shift, segments_per_file_shift, 0)
    .await
}

/// 同 [`manifest_retained_file_page_set`]，但每 replay 一份 manifest 检查段预算。
///
/// manifest replay 本身是同步 CPU 段，只能在 manifest 之间让出；单个超大
/// manifest 仍会占满一个段，拆分 replay 需要改 `fkv_task_reopen` 的接口。
pub async fn manifest_retained_file_page_set_budget<I: ManifestIo>(
  io: &I,
  tbl: &TableIdent,
  pages_per_file_shift: u8,
  segments_per_file_shift: u8,
  yield_budget_us: u64,
) -> Result<(HashSet<FilePageId>, HashSet<FilePageId>, InflightGuard)> {
  let mut yield_budget = YieldBudget::new(yield_budget_us);
  let mut data_set = HashSet::default();
  let mut segment_set = HashSet::default();
  let mut guard = InflightGuard::default();
  let active = match io.read_manifest(tbl).await {
    Ok(bytes) => bytes,
    Err(_) => {
      guard.mark_damaged(ACTIVE_MANIFEST_TAG);
      None
    }
  };
  if let Some(bytes) = &active
    && apply_manifest(
      bytes,
      &mut data_set,
      &mut segment_set,
      &mut guard,
      pages_per_file_shift,
      segments_per_file_shift,
    )
    .is_err()
  {
    guard.mark_damaged(ACTIVE_MANIFEST_TAG);
  }
  for name in io.list_manifest_file_name_li(tbl).await? {
    // 每份 manifest 的一次读 + replay 是长段边界；0 预算时无额外开销。
    yield_budget.maybe_yield().await;
    let bytes = match io.read_manifest_file(tbl, &name).await {
      Ok(Some(bytes)) => bytes,
      // 刚列举到却读不到：retained / guard 同时缺席，按损坏处理（全量保留）。
      Ok(None) => {
        guard.mark_damaged(&name);
        continue;
      }
      // 不可读：记录并跳过，本轮不删除任何文件。
      Err(_) => {
        guard.mark_damaged(&name);
        continue;
      }
    };
    // 本地 / 云端后端的活跃 manifest 也作为具名文件出现在列表里。内容与上面
    // 第一遍完全一致时，对 retained 集合与 guard 的贡献重复（`apply_manifest`
    // 幂等），跳过第二遍 replay；不按文件名硬编码，避免命名约定变化导致少算 retained。
    if active.as_deref() == Some(bytes.as_slice()) {
      continue;
    }
    if apply_manifest(
      &bytes,
      &mut data_set,
      &mut segment_set,
      &mut guard,
      pages_per_file_shift,
      segments_per_file_shift,
    )
    .is_err()
    {
      guard.mark_damaged(&name);
    }
  }
  Ok((data_set, segment_set, guard))
}
