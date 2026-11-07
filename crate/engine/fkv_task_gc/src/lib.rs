//! File GC 执行：删除没有任何 retained page 的 data 文件。
//!
//! 文件内仍有 live page 时保留；等 compaction 把 live page 移走后再删。

mod gc;
mod inflight;
mod retained;

pub use gc::{
  GcScope, gc_files_with_retained, gc_files_with_retained_budget,
  gc_local_files_with_retained_budget, gc_local_segment_files_with_retained_budget,
  gc_segment_files_with_retained, gc_segment_files_with_retained_budget,
};
pub use inflight::{BranchGuard, InflightGuard};
pub use retained::{
  add_mapping_retained, live_page_li_by_file, live_page_li_by_file_budget,
  manifest_retained_file_page_set, manifest_retained_file_page_set_budget,
};
