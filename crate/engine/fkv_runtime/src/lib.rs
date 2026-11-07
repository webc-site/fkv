//! Thread-per-core runtime：N 个 Shard，按 TableIdent 路由请求。

use std::{
  path::PathBuf,
  sync::{Arc, Mutex},
  time::Duration,
};

use fkv_cloud::CloudStore;
use fkv_config::KvOption;
use fkv_error::KvError;
use fkv_io_compio::CompioStore;
use fkv_object_s3::S3ObjectStore;
use fkv_shard::{CloudBackend, Shard};
use fkv_types::TableIdent;
use futures::lock::Mutex as AsyncMutex;

use crate::{
  archive::{ArchiveCrond, ArchiveState},
  attach::{AttachEpoch, AttachIdent, ShardViews},
  cloud_env::CloudStoreConfig,
  prewarm::{PrewarmCrond, PrewarmState},
};

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
  pub num_threads: u16,
  pub root: PathBuf,
  /// 未给 `KvOption` 时使用；给了且 `data_page_size` 非默认则以 option 为准。
  pub page_size: usize,
  /// 未给 `KvOption` 时 data / index 页共用的 restart 间隔回退值。
  pub restart_interval: u16,
  /// 未给 `KvOption` 时使用；给了且 `pages_per_file_shift` 非默认则以 option 为准。
  pub pages_per_file_shift: u8,
  /// 未给 `KvOption` 时使用；给了且 `segment_size` 非默认则以 option 为准。
  pub segment_size: u32,
  /// 未给 `KvOption` 时使用；给了且 `segments_per_file_shift` 非默认则以 option 为准。
  pub segments_per_file_shift: u8,
  pub large_value_threshold: usize,
  pub fixed_buffer_size: usize,
  pub queue_size: usize,
  /// 未给 `KvOption` 时使用；给了且 `auto_reopen_*` 非默认则以 option 为准。
  pub auto_reopen_retry_times: u8,
  /// 未给 `KvOption` 时使用；给了且 `auto_reopen_*` 非默认则以 option 为准。
  pub auto_reopen_pending_time_us: u64,
  /// 0 表示关闭定时归档；另外需 `archive_num_retained > 0` 才会启动归档
  /// crond（对齐 C++ `eloq_store.cpp:898`）。
  pub archive_interval_secs: u64,
  /// 自动归档 crond 的启用条件之一：0 表示不自动归档（对齐 C++）；
  /// 显式 `create_archive` 仍可调用，只是按「不剪枝」执行。
  pub archive_num_retained: u32,
}

impl Default for RuntimeConfig {
  /// 与 `KvOption` 默认值对齐：字段值等于 `KvOption::default()` 时视为
  /// 「未显式指定」，`start_with_option` 会以 `KvOption` 为准，不会触发
  /// 两侧冲突。`large_value_threshold = 0` 表示按 `page_size / 2` 自动取界。
  fn default() -> Self {
    let option = KvOption::default();
    Self {
      num_threads: option.num_threads,
      root: PathBuf::new(),
      page_size: option.data_page_size as usize,
      restart_interval: option.data_page_restart_interval,
      pages_per_file_shift: option.pages_per_file_shift,
      segment_size: option.segment_size,
      segments_per_file_shift: option.segments_per_file_shift,
      large_value_threshold: 0,
      fixed_buffer_size: 0,
      queue_size: 64,
      auto_reopen_retry_times: option.auto_reopen_retry_times,
      auto_reopen_pending_time_us: option.auto_reopen_pending_time_us,
      archive_interval_secs: u64::from(option.archive_interval_secs),
      archive_num_retained: option.num_retained_archives,
    }
  }
}

impl RuntimeConfig {
  /// 以 `root` 为数据根、其余字段取 [`Self::default`] 的配置。
  ///
  /// 配合 [`Runtime::start_with_option`] 使用时，盘面 / 重试 / 归档字段由
  /// `KvOption` 决定，调用方不必重复填十几个字段。
  pub fn new(root: impl Into<PathBuf>) -> Self {
    Self {
      root: root.into(),
      ..Self::default()
    }
  }
}

/// 归档标签条目（`manifest_<branch>_<term>_<tag>` 里的 `(tag, term)`）。
///
/// 对齐 C++ `GlobalListArchiveTagsRequest::ArchiveEntry`。字段顺序 = `Ord` 顺序
/// （先 `tag` 后 `term`），直接 `sort()` 即 C++ 比较器语义。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArchiveEntry {
  pub tag: String,
  pub term: u64,
}

/// 全局请求（archive / reopen / create_branch / drop_table）回执：
/// 成功条数 + 逐表失败（`TableIdent`, `KvError`），以及需要回传给调用方的
/// 全局命名结果。
///
/// 单表失败必须可见：不能把「N 个分区里挂了 1 个」回成整体成功。
///
/// `tag` / `name` 对齐 C++ 的结果字段：
/// - `global_archive`：`tag` = 实际使用的归档 tag（空 tag 由本层生成一次并
///   广播给全部分区），`name` = 归档 manifest 名（C++ `ResultArchive`）。
///   0 目标 / 全部失败时 `name` 仍按 attach 身份构造，不为空。
/// - `global_delete_archive`：同走 C++ `HandleGlobalArchiveRequest`（Delete），
///   `tag` + `name` 与 create 同语义返回。
/// - `global_create_branch`：`name` = 规范化并加盐后的内部 branch 名
///   （C++ `ResultBranch`），调用方必须用它继续操作该分支。
///
/// `global_reopen` / `global_drop_table` 不适用（有意留空）：
/// - C++ `GlobalReopenRequest` / `DropTableRequest` 的回执只有每分区成功与否，
///   不带 `ResultArchive` / `ResultBranch` 结果字段；fkv 对应的 per-shard
///   `Reopen` / `ReopenArchive` / `Drop` 响应同样不携带名字，本层无从构造。
/// - `tag` 也不回填请求参数：reopen 的 tag 只是输入（空 = 重开活跃快照），
///   回填会被读成「本层生成的归档 tag」，与归档回执语义混淆。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlobalReply {
  pub ok_num: usize,
  pub failed_li: Vec<(TableIdent, KvError)>,
  pub tag: String,
  pub name: String,
}

impl GlobalReply {
  /// 成功 + 失败总数（= 本轮投递的分区数）。
  pub fn total(&self) -> usize {
    self.ok_num + self.failed_li.len()
  }
}

/// 一轮预热的汇总回执（`Runtime::prewarm_report` 观测）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PrewarmReport {
  /// 本轮实际列举的分区数。
  pub table_num: usize,
  /// 拉取成功的文件数（manifest + data + segment）。
  pub pulled_file_num: usize,
  /// 单文件失败数。
  pub files_failed: usize,
  /// 因本地 cache 余量不足中止的表数。
  pub disk_full_num: usize,
  /// 远端不存在的文件数。
  pub missing_num: usize,
  /// 本轮不完整：整轮列举失败（网络 / 权限等）、单 shard 列举失败，或某张表以
  /// `DownloadError` / `DiskFull` 结束。失败分区会重新入队，下一轮重试；回执不会
  /// 被后续轮次抹掉。
  pub listing_failed: bool,
}

pub struct Runtime {
  pub(crate) shard_li: Vec<Shard>,
  pub(crate) archive_crond: Option<ArchiveCrond>,
  pub(crate) archive_state: Option<Arc<ArchiveState>>,
  pub(crate) prewarm_crond: Option<PrewarmCrond<CompioStore, S3ObjectStore>>,
  pub(crate) prewarm_state: Option<Arc<Mutex<PrewarmState>>>,
  /// 全局请求的目标集合来源：无 = Local/Mem 列盘，有 = Cloud 列远端。
  pub(crate) cloud_enum: Option<(CloudStoreConfig, CloudBackend)>,
  /// 列举用长生命周期 `CloudStore`：首次全局操作时装配，之后复用。
  ///
  /// 每次 `enumerate_target_li` 新建实例都会各起一个 S3 后台线程，全局归档 /
  /// 重开 / 删除频繁调用时线程数随之增长。实例只做远端列举、不写本地 cache，
  /// 串行调用复用同一实例；并发调用各自临时建、用完丢弃。
  pub(crate) cloud_enum_store: Mutex<Option<Box<CloudStore<CompioStore, S3ObjectStore>>>>,
  /// 本地数据根（Local/Mem/Standby 的全局列举与归档 crond 目标）。
  pub(crate) root_li: Vec<PathBuf>,
  /// `StandbyReplica`：全局重开的目标集合要合并 master 远端分区（对齐 C++
  /// `HandleGlobalReopenRequest` 的 `Mode() == StandbyReplica` 分支）。
  pub(crate) standby_replica: bool,
  /// 各 shard 的 attach 身份；`None` = 未 attach。
  ///
  /// 只用于全局结果名：`global_archive` / `global_delete_archive` 在 0 目标或
  /// 全部失败时按最早 shard 的身份构造 `manifest_<branch>_<term>_<tag>`。
  pub(crate) attach_ident_li: Arc<Mutex<Vec<Option<AttachIdent>>>>,
  /// 每 shard 的 attach 代际（`ShardRoute` 共享）。
  pub(crate) attach_epoch: Arc<AttachEpoch>,
  /// 本层 attach 视图集合（归档目标 / 预热路由 / attach 身份），`ShardRoute`
  /// 在错误收尾时据此清理；`ArchiveCrond` 持同一份。
  pub(crate) views: ShardViews,
  /// 全局 fanout 单批上限（`max_global_request_batch`）。
  pub(crate) max_global_request_batch: usize,
  /// 数据面默认请求 deadline；`None` = 不设（保持旧行为）。
  pub(crate) request_timeout: Option<Duration>,
  /// 数据面默认入队等待上限；`None` = 无限等待（保持旧行为）。
  pub(crate) queue_timeout: Option<Duration>,
  /// per-shard 控制面串行锁：bump 代际、发送请求与落地视图必须在同一临界区
  /// 内，否则并发 attach / detach 会让已在 shard 生效的 attach 因代际过期而
  /// 丢视图（shard `Attached`、本层视图空）。
  pub(crate) control_li: Vec<AsyncMutex<()>>,
}

/// 未给 `KvOption` 时的全局 fanout 单批上限（对齐 `fkv_config` 默认值）。
const DEFAULT_MAX_GLOBAL_REQUEST_BATCH: u32 = 1000;

/// 观测类请求（`ListStandbyPartition`）的默认 deadline。
///
/// 该类请求在 shard 线程内同步跑 `ssh` / `ls`（见 `fkv_standby::rsync`），
/// master 卡死时没有 deadline 会让调用方永久挂起；`with_request_timeout`
/// 显式配置时以配置为准。
const DEFAULT_OBSERVE_TIMEOUT: Duration = Duration::from_secs(30);

mod admin;
mod archive;
mod attach;
mod backend;
mod cloud_env;
mod config;
mod prewarm;
mod request;
mod route;
mod standby;
mod startup;

#[cfg(test)]
mod attach_view_tests;
#[cfg(test)]
mod branch_salt_tests;
#[cfg(test)]
mod global_tests;
#[cfg(test)]
mod prewarm_tests;
#[cfg(test)]
mod s3_instance_tests;
