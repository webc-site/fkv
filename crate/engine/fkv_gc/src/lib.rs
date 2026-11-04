//! 文件 GC 的纯算法：retained set、放大率、候选过滤。

use fkv_mapping::{MappingTbl, decode_id, is_file_page_id};
use fkv_types::FilePageId;
use gxhash::HashSet;

pub fn retained_file_page_set(mapping_tbl: &MappingTbl) -> HashSet<FilePageId> {
  let mut set = HashSet::default();
  for value in mapping_tbl.value_li() {
    if is_file_page_id(*value) {
      set.insert(decode_id(*value));
    }
  }
  set
}

pub fn unreferenced_file_page_li(
  candidate_li: impl IntoIterator<Item = FilePageId>,
  retained: &HashSet<FilePageId>,
) -> Vec<FilePageId> {
  candidate_li
    .into_iter()
    .filter(|file_page_id| !retained.contains(file_page_id))
    .collect()
}

pub fn file_amplification(live: u64, total: u64) -> f64 {
  if live == 0 {
    0.0
  } else {
    total as f64 / live as f64
  }
}

pub fn should_compact(live: u64, total: u64, factor: u8) -> bool {
  factor > 0 && total > live.saturating_mul(u64::from(factor))
}
