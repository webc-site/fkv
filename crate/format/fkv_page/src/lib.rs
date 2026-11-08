//! Data / Index / Overflow 页编解码与迭代。
//!
//! 比较器约定：只支持**默认字节序 comparator**（等价 C++ `Comparator::DefaultComparator`）。
//! C++ 允许注入自定义 comparator 的 store 不保证可读（fmt-20）。

pub mod data;
pub mod header;
pub mod index;
pub mod overflow;
mod region;
mod restart;

pub use data::{
  CompressionType, DataPage, DataPageBuilder, DataPageIter, EntrySize, EntrySpec, entry_size,
  is_overflow_kv, page_content_len, set_next_page_id, set_prev_page_id,
};
// CHECKSUM_BYTES 的唯一来源是 `fkv_codec`；page 头部布局常量由它推导。
pub use fkv_codec::CHECKSUM_BYTES;
pub use header::{
  DATA_HEADER_SIZE, INDEX_HEADER_SIZE, OVERFLOW_HEADER_SIZE, PageType, content_len,
  data_restart_offset, page_type, restart_num, set_checksum, set_page_type, validate_checksum,
};
pub use index::{IndexPage, IndexPageBuilder, IndexPageIter};
pub use overflow::{
  DEFAULT_OVERFLOW_POINTERS, OverflowPage,
  build_without_checksum as build_overflow_without_checksum, capacity as overflow_capacity,
  decode_pointers, encode_pointers, first_pointer_count,
};
pub use region::{FastIndexPageBuilder, PageRegionIter, region_entry_cnt};
