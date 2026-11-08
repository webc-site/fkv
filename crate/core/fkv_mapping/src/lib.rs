//! 逻辑页 → 物理页映射与文件页分配器。
//!
//! 值格式：
//! 低 3 位为 `ValType`，高位为 `FilePageId` / `PageId`。

mod allocator;
mod chunk;
mod generation;
mod mapper;
mod mapping_tbl;
mod snapshot;

pub use allocator::{AllocMode, AppendAllocator, FilePageAllocator, FilePagePool};
pub use chunk::{CHUNK_CACHE_MAX, CHUNK_SIZE, chunk_alloc_num, chunk_dir_copy_num};
pub use generation::GenerationSource;
pub use mapper::{BranchPageOwner, PageMapper};
pub use mapping_tbl::{MappingTbl, add_mapping_retained};
pub use snapshot::{MappingSnapshot, SnapshotRegistry, SnapshotSource};
