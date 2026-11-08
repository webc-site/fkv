//! 写路径模块：写门排空 / Put 组合并派发 / 任务化执行体 / 完成回收与排队清理。
//!
//! 由 struct-20 从 `write.rs` 按内聚域搬移，逻辑未改。

mod finish;
mod flight;
mod gate;
mod merge;
mod spawn;

pub(crate) use finish::{WriteDone, finish_write};
pub(crate) use flight::WriteFlight;
pub use flight::poll_ready_once;
pub(crate) use gate::{DrainCx, drain_write_gate, fail_pending, write_queue_stale};
// `merge_put_batch_li` 仅由 crate 内联测试（`lib.rs`）经 `crate::write::` 使用。
#[cfg(test)]
pub(crate) use merge::merge_put_batch_li;
pub use merge::merge_put_batches_for_test;
pub(crate) use spawn::put_large_body;
