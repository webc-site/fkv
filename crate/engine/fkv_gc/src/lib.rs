//! 文件 GC 的纯算法：候选过滤。

use fkv_mapping::MappingTbl;
use fkv_types::{FileId, decode_id, is_file_page_id};
use gxhash::HashMap;

pub fn should_compact(live: u64, total: u64, factor: u8) -> bool {
  factor > 0 && total > live.saturating_mul(u64::from(factor))
}

/// 一遍 mapping 统计每个文件的 live page 数。
pub fn file_live_count(mapping_tbl: &MappingTbl, pages_per_file_shift: u8) -> HashMap<FileId, u64> {
  let mut count = HashMap::default();
  for value in mapping_tbl.iter_value_li() {
    if is_file_page_id(value) {
      let file_id = decode_id(value) >> pages_per_file_shift;
      *count.entry(file_id).or_insert(0) += 1;
    }
  }
  count
}

/// 一次遍历选出所有放大率超阈值的文件，按 file_id 升序排列。
///
/// 对齐 C++ `DoCompactDataFile`：单次 compaction 覆盖 `begin_file_id..end_file_id`
/// 内全部超阈值文件，而不是每轮只搬一个文件；逐文件按文件号升序处理。
///
/// `tail_file_id` 是分配器当前正在追加的文件，必须跳过：它尚未写满，按容量
/// 算放大率必然误判，搬进去的新页会再次触发抖动。该值由调用方按分配器高
/// 水位（`max_fp_id >> pages_per_file_shift`）算出，不能用候选列表最大 id
/// 代替：GC 删空文件后，盘上最大 id 可能远小于分配器尾。
pub fn compact_plan(
  live_map: &HashMap<FileId, u64>,
  candidate_li: &[FileId],
  pages_per_file: u64,
  factor: u8,
  tail_file_id: FileId,
) -> Vec<FileId> {
  compact_plan_where(live_map, candidate_li, pages_per_file, factor, |file_id| {
    file_id != tail_file_id
  })
}

/// [`compact_plan`] 的过滤核心：`keep` 决定候选文件是否参与比较。
///
/// 用闭包而非先筛出临时 `Vec`，segment 侧跳尾文件时零额外分配。
fn compact_plan_where(
  live_map: &HashMap<FileId, u64>,
  candidate_li: &[FileId],
  pages_per_file: u64,
  factor: u8,
  keep: impl Fn(FileId) -> bool,
) -> Vec<FileId> {
  if factor == 0 {
    return Vec::new();
  }
  let mut plan_li: Vec<FileId> = candidate_li
    .iter()
    .filter(|file_id| keep(**file_id))
    .filter(|file_id| {
      let live = live_map.get(file_id).copied().unwrap_or(0);
      // 空文件交给 GC，不 compact。
      live != 0 && should_compact(live, pages_per_file, factor)
    })
    .copied()
    .collect();
  // 按文件号升序搬，对齐 C++ `DoCompactDataFile` / `DoCompactSegmentFile` 的
  // `for (file_id = begin_file_id; file_id < end_file_id; ++file_id)` 升序循环
  // （background_write.cpp:232 / :409）：搬迁顺序决定 append 分配器的新
  // `FilePageId` 发号与 manifest record 内 delta 条目顺序（sem-45 操作序），
  // 顺序不同则映射值不同。旧口径「放大率高的先搬」没有收益：同一轮
  // `maintain` 末尾的 file_gc 与搬迁顺序无关，本轮回收集合不变（sem-61）。
  plan_li.sort_unstable();
  plan_li
}

/// segment 文件的压缩候选：放大率按「每文件槽位数」计算。
///
/// 与 data 文件同规则，唯一差别是跳过 `tail_file_id`：那是分配器当前正在
/// 追加的文件，尚未写满，按容量算放大率必然误判，且搬进去的新段会再次触发抖动。
///
/// 尾文件必须由调用方按分配器高水位算出（`max_fp_id >> shift`），不能用
/// 候选列表里的最大 file id 代替：GC 删空文件后，盘上最大 id 可能远小于
/// 分配器尾，那个已写满的文件会因此永久免于 compact。
pub fn segment_compact_plan(
  live_map: &HashMap<FileId, u64>,
  candidate_li: &[FileId],
  segments_per_file: u64,
  factor: u8,
  tail_file_id: FileId,
) -> Vec<FileId> {
  compact_plan_where(
    live_map,
    candidate_li,
    segments_per_file,
    factor,
    |file_id| file_id != tail_file_id,
  )
}
