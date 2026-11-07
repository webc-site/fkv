//! 配置合并：`RuntimeConfig` / `KvOption` 字段解析与写 IO 上限投影。

use core::fmt;

use fkv_config::KvOption;
use fkv_error::{KvError, Result};
use log::{error, info};

use crate::RuntimeConfig;

/// 写 IO 的两个上限：`(max_inflight_write, max_write_batch_pages)`。
///
/// 未给 `KvOption` 时回落到 `fkv_config` 常量；默认值与 C++ `kv_options.h`
/// 字段初始值一致（`32 << 10` / `32`）。
pub(crate) fn write_limits(option: Option<&KvOption>) -> (usize, usize) {
  option.map_or(
    (
      fkv_config::DEFAULT_MAX_INFLIGHT_WRITE as usize,
      fkv_config::DEFAULT_MAX_WRITE_BATCH_PAGES as usize,
    ),
    |option| {
      (
        option.max_inflight_write as usize,
        option.max_write_batch_pages as usize,
      )
    },
  )
}

/// Cloud 模式 S3 实例数：每个 shard 一个，加运行时侧全局列举（长驻 1 个）、
/// 预热 crond 与归档 crond（按配置启用，未启用不占额度）。
///
/// [`CloudBackend::s3_config`] 按该数均分 `max_cloud_concurrency`：漏算运行时侧
/// 实例会把整机飞行请求放大到 `实际实例数 × 每实例额度`。
pub(crate) fn cloud_s3_instance_num(num_threads: u16, archive_on: bool, prewarm_on: bool) -> usize {
  usize::from(num_threads) + 1 + usize::from(archive_on) + usize::from(prewarm_on)
}

/// `RuntimeConfig` 与 `KvOption` 双处存在的盘面 / 重试字段合并。
///
/// 只有偏离 `KvOption` 默认值的才算「有语义」：两侧都显式且不一致直接拒绝，
/// 避免同名字段静默分叉；否则显式值优先，未显式时沿用 `RuntimeConfig`。
pub(crate) fn merge_field<T>(
  name: &'static str,
  from_config: T,
  from_option: Option<T>,
  default: T,
) -> Result<T>
where
  T: Copy + PartialEq + fmt::Debug,
{
  let Some(value) = from_option else {
    return Ok(from_config);
  };
  if value != default && from_config != default && value != from_config {
    error!(
      target: "fkv_runtime",
      "runtime config conflict field={name} config={from_config:?} option={value:?}"
    );
    return Err(KvError::InvalidArgs);
  }
  Ok(if value == default { from_config } else { value })
}

/// `RuntimeConfig` 与 `KvOption` 合并后的盘面 / 重试 / 归档参数。
pub(crate) struct ResolvedSettings {
  pub(crate) num_threads: u16,
  pub(crate) page_size: usize,
  pub(crate) pages_per_file_shift: u8,
  pub(crate) segment_size: u32,
  pub(crate) segments_per_file_shift: u8,
  pub(crate) auto_reopen_retry_times: u8,
  pub(crate) auto_reopen_pending_time_us: u64,
  pub(crate) archive_interval_secs: u64,
  pub(crate) archive_num_retained: u32,
  /// 当前分配模式是否为 append；C++ 只有 append 分区才跑自动归档。
  pub(crate) data_append_mode: bool,
}

impl ResolvedSettings {
  /// 自动归档 crond 的启动谓词：对齐 C++ `eloq_store.cpp:898`
  /// （`data_append_mode && num_retained_archives > 0 && archive_interval_secs > 0`）。
  /// `num_retained = 0` 只关闭自动归档，显式 `create_archive` 仍按「不剪枝」执行。
  pub(crate) fn archive_crond_on(&self) -> bool {
    self.data_append_mode && self.archive_interval_secs > 0 && self.archive_num_retained > 0
  }
}

/// 合并 `RuntimeConfig` 与 `KvOption` 的盘面 / 重试 / 归档字段。
///
/// 只做取值与校验，不触碰后端；`archive_interval_secs` 必须在后端装配前解出
/// （S3 实例数要把归档 crond 计进去）。
pub(crate) fn resolve_settings(
  config: &RuntimeConfig,
  option: Option<&KvOption>,
) -> Result<ResolvedSettings> {
  let option_default = KvOption::default();
  let num_threads = merge_field(
    "num_threads",
    config.num_threads,
    option.map(|opts| opts.num_threads),
    option_default.num_threads,
  )?;
  let page_size = merge_field(
    "page_size",
    config.page_size,
    option.map(|opts| opts.data_page_size as usize),
    option_default.data_page_size as usize,
  )?;
  let pages_per_file_shift = merge_field(
    "pages_per_file_shift",
    config.pages_per_file_shift,
    option.map(|opts| opts.pages_per_file_shift),
    option_default.pages_per_file_shift,
  )?;
  let segment_size = merge_field(
    "segment_size",
    config.segment_size,
    option.map(|opts| opts.segment_size),
    option_default.segment_size,
  )?;
  let segments_per_file_shift = merge_field(
    "segments_per_file_shift",
    config.segments_per_file_shift,
    option.map(|opts| opts.segments_per_file_shift),
    option_default.segments_per_file_shift,
  )?;
  let auto_reopen_retry_times = merge_field(
    "auto_reopen_retry_times",
    config.auto_reopen_retry_times,
    option.map(|opts| opts.auto_reopen_retry_times),
    option_default.auto_reopen_retry_times,
  )?;
  let auto_reopen_pending_time_us = merge_field(
    "auto_reopen_pending_time_us",
    config.auto_reopen_pending_time_us,
    option.map(|opts| opts.auto_reopen_pending_time_us),
    option_default.auto_reopen_pending_time_us,
  )?;
  if num_threads == 0 {
    error!(target: "fkv_runtime", "runtime start rejected: num_threads=0");
    return Err(KvError::InvalidArgs);
  }
  info!(
    target: "fkv_runtime",
    "runtime start threads={} root={} page_size={} pages_per_file_shift={}",
    num_threads,
    config.root.display(),
    page_size,
    pages_per_file_shift
  );
  // 归档参数与盘面字段同一套合并规则：`KvOption` 显式（非默认）才覆盖
  // `RuntimeConfig`，两侧都显式且冲突直接拒绝，避免传一份 KvOption 就把
  // 定时归档静默关掉（`archive_interval_secs` 默认 0）。提前到后端装配前：
  // S3 实例数要把归档 crond 计进去，且启动谓词依赖合并后的 retained 值。
  let archive_interval_secs = merge_field(
    "archive_interval_secs",
    config.archive_interval_secs,
    option.map(|opts| u64::from(opts.archive_interval_secs)),
    u64::from(option_default.archive_interval_secs),
  )?;
  let archive_num_retained = merge_field(
    "num_retained_archives",
    config.archive_num_retained,
    option.map(|opts| opts.num_retained_archives),
    option_default.num_retained_archives,
  )?;
  Ok(ResolvedSettings {
    num_threads,
    page_size,
    pages_per_file_shift,
    segment_size,
    segments_per_file_shift,
    auto_reopen_retry_times,
    auto_reopen_pending_time_us,
    archive_interval_secs,
    archive_num_retained,
    data_append_mode: option.map_or(option_default.data_append_mode, |opts| {
      opts.data_append_mode
    }),
  })
}
