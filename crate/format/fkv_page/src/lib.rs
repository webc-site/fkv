//! Data / Index / Overflow 页编解码与迭代。

pub mod data;
pub mod header;
pub mod index;
pub mod overflow;

pub use data::{
  CompressionType, DataPage, DataPageBuilder, DataPageIter, set_next_page_id, set_prev_page_id,
};
pub use header::{
  CHECKSUM_BYTES, DATA_HEADER_SIZE, INDEX_HEADER_SIZE, OVERFLOW_HEADER_SIZE, PageType, content_len,
  data_restart_offset, page_type, restart_num, set_checksum, set_page_type, validate_checksum,
};
pub use index::{IndexPage, IndexPageBuilder, IndexPageIter};
pub use overflow::{OverflowPage, capacity as overflow_capacity};
