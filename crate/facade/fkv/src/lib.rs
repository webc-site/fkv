//! fkv curated facade：只导出稳定入口，内部 crate 仍可单独使用。
//!
//! # 三层入口
//!
//! - [`KvStore`]（`fkv_sdk`）：**Local-only** 的 put/get/scan 便捷 API，
//!   单文件后端、无需 attach。配置为 Cloud/Standby 时显式 `InvalidArgs`。
//! - [`Runtime`]（`fkv_runtime`）：多 shard、模式化 IO 后端（Local / Cloud /
//!   Standby / Mem）与全局控制面（`global_archive` / `global_reopen` /
//!   `global_create_branch` / `global_drop_table` / `global_list_archive_tags`）。
//!   Cloud / Standby 模式由 [`KvOption::derive_mode`] 选择；纯内存用
//!   [`start_mem_runtime`]。
//! - [`start_runtime`]：以 [`KvOption`] 为唯一配置源启动 [`Runtime`] 的
//!   便捷入口，对应 C++ 嵌入方「持有一个 Store 句柄」的使用形态。
//!
//! 需要直接下发请求 / 匹配回执的一方使用 [`Runtime::request`] 与
//! [`Request`] / [`Response`]。

use std::path::PathBuf;

pub use fkv_api::{
  ArchiveReply, BranchReply, CleanExpiredReply, FloorReply, LargeValueReply, Request, Response,
  ScanPageOption, ScanReply,
};
pub use fkv_config::KvOption;
pub use fkv_error::{KvError, Result};
pub use fkv_runtime::{ArchiveEntry, GlobalReply, PrewarmReport, Runtime, RuntimeConfig};
pub use fkv_sdk::{BatchEntry, KvStore};
pub use fkv_store::ScanOption;
pub use fkv_types::{MAIN_BRANCH_NAME, StoreMode, TableIdent, now_ms};

/// 按 [`KvOption`] 派生模式（Local / Cloud / Standby）启动多 shard [`Runtime`]。
///
/// `root` 是本地数据根（Cloud 模式为本地 cache 根）。模式、盘面与重试参数
/// 全部来自 `option`；`RuntimeConfig` 只填默认值，不会与 `KvOption` 冲突。
///
/// 数据面默认超时同样来自 `option`：`request_timeout_us` /
/// `request_queue_timeout_us` 接到 [`Runtime::with_request_timeout`] /
/// [`Runtime::with_queue_timeout`]。两键为 `0` 时 Local / Standby 保持旧行为
/// （不设 deadline、无限排队），Cloud 回退到 S3 重试窗口量级的默认值。
/// 控制面 attach / detach / shutdown 不套用（见 `Runtime::control_route`）。
pub fn start_runtime(root: impl Into<PathBuf>, option: &KvOption) -> Result<Runtime> {
  let mut runtime = Runtime::start_with_option(RuntimeConfig::new(root), option)?;
  if let Some(timeout) = option.request_timeout() {
    runtime = runtime.with_request_timeout(timeout);
  }
  if let Some(timeout) = option.queue_timeout() {
    runtime = runtime.with_queue_timeout(timeout);
  }
  Ok(runtime)
}

/// 启动纯内存 [`Runtime`]（无 durability，进程退出即丢）。
///
/// `root` 只用于日志与本地 cache 约定，本模式不落盘。
pub fn start_mem_runtime(root: impl Into<PathBuf>) -> Result<Runtime> {
  Runtime::start_mem(RuntimeConfig::new(root))
}
