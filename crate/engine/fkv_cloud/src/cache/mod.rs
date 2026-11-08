//! 本地 cache 记账模块：状态与 LRU / 内容归属 / data 记账 / segment 记账 / 淘汰。
//!
//! 由 struct-17 从 `cache.rs` 按内聚域搬移，逻辑未改。

mod data;
mod evict;
mod ident;
mod segment;
mod state;

pub use state::EVICT_PENDING_FILE_MAX;
pub(crate) use state::{CacheState, TableCache};
