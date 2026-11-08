//! 读路径模块：分发 / 在飞任务与槽位 / 批量提交 / 完成回执 / 读窗口准入。
//!
//! 由 struct-17 从 `read.rs` 按内聚域搬移，逻辑未改。

mod dispatch;
mod finish;
mod flight;
mod submit;
mod window;

pub(crate) use dispatch::dispatch_read;
pub(crate) use finish::{finish_read_li, finish_read_partition};
pub(crate) use flight::ReadFlight;
pub(crate) use submit::submit_read_li;
pub(crate) use window::{ReadEvent, WindowDeferred, read_window};
