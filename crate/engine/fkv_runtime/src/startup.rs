//! 运行时装配与生命周期：`Runtime` 构造、shard 拉起、后台服务与 shutdown。

use std::{
  mem::take,
  path::PathBuf,
  sync::{Arc, Mutex},
  thread,
  time::Duration,
};

use fkv_config::KvOption;
use fkv_error::{KvError, Result};
use fkv_io_compio::CompioStore;
use fkv_metrics::MetricsSnapshot;
use fkv_object_s3::S3ObjectStore;
use fkv_shard::{AllocMode, Backend, CloudBackend, Shard, ShardConfig};
use fkv_standby::RsyncStandbyService;
use fkv_types::{DEFAULT_MANIFEST_LIMIT, DEFAULT_MAX_PROCESSING_TIME_US, StoreMode};
use futures::lock::Mutex as AsyncMutex;
use log::{error, info};

use crate::{
  DEFAULT_MAX_GLOBAL_REQUEST_BATCH, PrewarmReport, Runtime, RuntimeConfig,
  archive::{ArchiveCrond, ArchiveState, ArchiveTarget},
  attach::{AttachEpoch, AttachIdent, ShardViews},
  backend::{BackendState, assemble_backend},
  cloud_env::{CloudStoreConfig, open_runtime_cloud_store},
  config::{ResolvedSettings, resolve_settings, write_limits},
  prewarm::{PrewarmCrond, PrewarmState, lock_prewarm_state},
  route::ShardRoute,
};

/// 运行时级后台服务：Cloud 列举参数、预热 crond 与归档 crond。
pub(crate) struct RuntimeServices {
  pub(crate) cloud_enum: Option<(CloudStoreConfig, CloudBackend)>,
  pub(crate) max_global_request_batch: usize,
  pub(crate) archive_state: Option<Arc<ArchiveState>>,
  pub(crate) prewarm_crond: Option<PrewarmCrond<CompioStore, S3ObjectStore>>,
  pub(crate) prewarm_state: Option<Arc<Mutex<PrewarmState>>>,
  pub(crate) attach_ident_li: Arc<Mutex<Vec<Option<AttachIdent>>>>,
  pub(crate) attach_epoch: Arc<AttachEpoch>,
  pub(crate) views: ShardViews,
  pub(crate) archive_crond: Option<ArchiveCrond>,
}

/// `KvOption` → `ShardConfig` 的唯一投影点：除 `shard_id` / `fd_limit` 外
/// 所有字段与 shard 无关，先解一次再逐 shard 克隆。
fn build_shard_cfg(
  config: &RuntimeConfig,
  option: Option<&KvOption>,
  settings: &ResolvedSettings,
  backend: Backend,
  mode: StoreMode,
  standby: Option<Arc<RsyncStandbyService>>,
) -> ShardConfig {
  let num_threads = settings.num_threads;
  let page_size = settings.page_size;
  let pages_per_file_shift = settings.pages_per_file_shift;
  let segment_size = settings.segment_size;
  let segments_per_file_shift = settings.segments_per_file_shift;
  let auto_reopen_retry_times = settings.auto_reopen_retry_times;
  let auto_reopen_pending_time_us = settings.auto_reopen_pending_time_us;
  let option_default = KvOption::default();
  // FD 上限按 shard 均分（对齐 C++ `store_fd_limit / num_threads`）；0 保留为不限制。
  let store_fd_limit = option.map_or(fkv_config::DEFAULT_FD_LIMIT as usize, |opts| {
    opts.fd_limit as usize
  });
  let shard_fd_limit = if store_fd_limit == 0 {
    0
  } else {
    (store_fd_limit / usize::from(num_threads)).max(1)
  };
  // root meta 字节预算同样按 shard 均分（对齐 C++ `root_meta.cpp` 的
  // `capacity_bytes_ = root_meta_cache_size / num_threads`）；0 保留为不限制。
  let store_root_meta_cache_bytes = option
    .map_or(fkv_config::DEFAULT_ROOT_META_CACHE_BYTES, |opts| {
      opts.root_meta_cache_size
    });
  let shard_root_meta_cache_bytes = if store_root_meta_cache_bytes == 0 {
    0
  } else {
    usize::try_from(store_root_meta_cache_bytes / u64::from(num_threads))
      .unwrap_or(usize::MAX)
      .max(1)
  };
  // 分配模式对齐 C++ `data_append_mode`（默认 false = PooledFilePages）；
  // 未给 KvOption 时按 `KvOption::default()`，与显式传入默认值同源。
  let alloc_mode = if option.map_or(option_default.data_append_mode, |opts| {
    opts.data_append_mode
  }) {
    AllocMode::Append
  } else {
    AllocMode::Pooled
  };
  // SQ 深度对齐 C++ `io_queue_size`；未给 KvOption 时用默认值。
  let io_queue_size = option.map_or(fkv_config::DEFAULT_IO_QUEUE_SIZE, |opts| opts.io_queue_size);
  // 写 IO 两个上限同源投影，避免默认值 / 配置值在 runtime 内再分叉。
  let (max_inflight_write, max_write_batch_pages) = write_limits(option);
  // `KvOption` → `ShardConfig` 的唯一投影点：除 `shard_id` / `fd_limit` 外
  // 所有字段与 shard 无关，先解一次再逐 shard 克隆，回退默认值只在此处出现。
  ShardConfig {
    shard_id: 0,
    root: config.root.clone(),
    page_size,
    // 两旋钮各取各的；未给 KvOption 时回落到 RuntimeConfig 的共用值。
    restart_interval: option.map_or(config.restart_interval, |opts| {
      opts.data_page_restart_interval
    }),
    index_restart_interval: option.map_or(config.restart_interval, |opts| {
      opts.index_page_restart_interval
    }),
    pages_per_file_shift,
    segment_size,
    segments_per_file_shift,
    large_value_threshold: config.large_value_threshold,
    fixed_buffer_size: config.fixed_buffer_size,
    queue_size: config.queue_size,
    auto_reopen_retry_times,
    auto_reopen_pending_time_us,
    oom_retry_times: option.map_or(
      u32::from(fkv_config::DEFAULT_AUTO_OOM_RETRY_TIMES),
      |opts| u32::from(opts.auto_oom_retry_times),
    ),
    compression_level: option
      .filter(|opts| opts.enable_compression)
      .map(|opts| opts.zstd_compression_level),
    verify_checksum: option.is_none_or(|opts| !opts.skip_verify_checksum),
    overflow_pointers: option.map_or(fkv_page::DEFAULT_OVERFLOW_POINTERS, |opts| {
      opts.overflow_pointers
    }),
    // 未给 KvOption 时回落到 KvOption 默认值，与 `start_with_option(default)`
    // 同源；否则默认路径关闭压缩、显式路径默认开启，同默认两套行为。
    file_amplify_factor: option.map_or(option_default.file_amplify_factor, |opts| {
      opts.file_amplify_factor
    }),
    segment_file_amplify_factor: option
      .map_or(option_default.segment_file_amplify_factor, |opts| {
        opts.segment_file_amplify_factor
      }),
    manifest_limit: option.map_or(DEFAULT_MANIFEST_LIMIT as usize, |opts| {
      opts.manifest_limit as usize
    }),
    max_write_batch_pages,
    max_inflight_write,
    fd_limit: shard_fd_limit,
    alloc_mode,
    // 扩展键 `0` = 跟随 C++ `buffer_pool_size`（per shard，索引/数据共享）；
    // index 缓存与读写共用页缓存（page_cache）同取该值、各自独立计量
    // （partition.rs `with_page_cache_bytes` 同源接线）；读路径由页缓存覆盖。
    index_cache_bytes: {
      let bytes = option.map_or(option_default.index_cache_bytes, |opts| {
        opts.index_cache_bytes
      });
      (if bytes == 0 {
        option.map_or(option_default.buffer_pool_size, |opts| {
          opts.buffer_pool_size
        })
      } else {
        bytes
      }) as usize
    },
    index_cache_total_bytes: {
      let bytes = option.map_or(option_default.index_cache_total_bytes, |opts| {
        opts.index_cache_total_bytes
      });
      (if bytes == 0 {
        option.map_or(option_default.buffer_pool_size, |opts| {
          opts.buffer_pool_size
        })
      } else {
        bytes
      }) as usize
    },
    root_meta_cache_bytes: shard_root_meta_cache_bytes,
    io_queue_size,
    backend,
    mode,
    standby,
    max_write_concurrency: option.map_or(0, |opts| opts.max_write_concurrency as usize),
    max_processing_time_us: option.map_or(DEFAULT_MAX_PROCESSING_TIME_US, |opts| {
      opts.max_processing_time_us
    }),
    yield_budget_us: option.map_or(fkv_config::DEFAULT_YIELD_BUDGET_US, |opts| {
      opts.yield_budget_us
    }),
  }
}

/// 按 `ShardConfig` 逐个拉起 shard 线程。
///
/// 线程创建在 runner 资源紧张时可能瞬时返回 EAGAIN（映射为 `IoFail`）：
/// 重试几次，其它错误原样上抛。
fn start_shard_li(shard_cfg: &ShardConfig, num_threads: u16) -> Result<Vec<Shard>> {
  let mut shard_li = Vec::with_capacity(usize::from(num_threads));
  for shard_id in 0..num_threads {
    let per_shard_cfg = ShardConfig {
      shard_id: u32::from(shard_id),
      ..shard_cfg.clone()
    };
    // 线程创建在 runner 资源紧张时可能瞬时返回 EAGAIN（映射为 `IoFail`）。
    // 重试几次，避免把瞬时资源压力报成启动失败；其它错误原样返回。
    let mut attempt = 0;
    let shard = loop {
      match Shard::start(per_shard_cfg.clone()) {
        Ok(shard) => break shard,
        Err(KvError::IoFail) if attempt < 10 => {
          attempt += 1;
          error!(
            target: "fkv_runtime",
            "runtime shard start retry shard_id={} attempt={} error=IoFail",
            shard_id,
            attempt
          );
          thread::sleep(Duration::from_millis(50));
        }
        Err(err) => {
          error!(
            target: "fkv_runtime",
            "runtime shard start failed shard_id={} error={}",
            shard_id,
            err
          );
          return Err(err);
        }
      }
    };
    shard_li.push(shard);
  }
  Ok(shard_li)
}

/// 启动运行时级后台服务：Cloud 列举参数、预热 crond 与归档 crond。
///
/// 归档 crond 的启动谓词（`archive_interval_secs > 0 && num_retained > 0`）与
/// 参数合并在 [`resolve_settings`] 完成：后端装配的 `s3_instance_num` 要用同一
/// 谓词，避免多算一个不会创建的列举实例。
fn start_services(
  option: Option<&KvOption>,
  settings: &ResolvedSettings,
  shard_cfg: &ShardConfig,
  shard_li: &[Shard],
  root_li: &[PathBuf],
) -> Result<RuntimeServices> {
  // 运行时侧 Cloud 装配参数：预热线程 / 归档 crond / 全局列举共用同一份
  // （`CloudStore` 非 `Sync`，但装配参数一致，只有实例不同）。
  let cloud_enum = match &shard_cfg.backend {
    Backend::Cloud(cloud) => Some((
      CloudStoreConfig::from_shard(shard_cfg, cloud),
      cloud.clone(),
    )),
    _ => None,
  };
  let max_global_request_batch =
    usize::try_from(option.map_or(DEFAULT_MAX_GLOBAL_REQUEST_BATCH, |opts| {
      opts.max_global_request_batch
    }))
    .unwrap_or(1)
    .max(1);
  let archive_num_retained = settings.archive_num_retained;
  let archive_state = settings.archive_crond_on().then(|| {
    // Cloud 模式归档目标以**远端**为准：本地 cache 只看得见已 cache 的分区，
    // 否则远端已有、本地未 cache 的分区会永远不被归档。
    let target = match &cloud_enum {
      Some((cfg, cloud)) => ArchiveTarget::Cloud(Box::new(cfg.clone()), Box::new(cloud.clone())),
      None => ArchiveTarget::Local(root_li.to_vec()),
    };
    Arc::new(ArchiveState::new(
      settings.num_threads,
      target,
      Duration::from_secs(settings.archive_interval_secs),
      archive_num_retained,
      u32::try_from(max_global_request_batch).unwrap_or(u32::MAX),
    ))
  });
  // Cloud 模式预热：`prewarm_cloud_cache` 打开时按 `prewarm_task_count` 起后台线程。
  // attach 只向 crond 投递信号就返回，不在请求路径上同步等下载（对齐 C++
  // `AttachShard` 排队 + `PrewarmService::PrewarmLoop`）。
  //
  // 先于归档 crond 建：归档线程的 `ShardRoute` 也带预热视图，shard 过期时两边
  // 一起失效。
  let (prewarm_crond, prewarm_state) = match (option, cloud_enum.as_ref()) {
    (Some(opts), Some((cfg, cloud))) if opts.prewarm_cloud_cache => {
      let store = open_runtime_cloud_store(cfg, cloud)?;
      let state = Arc::new(Mutex::new(PrewarmState::new(settings.num_threads)));
      let task_count = usize::from(opts.prewarm_task_count).max(1);
      let crond = PrewarmCrond::start(store, state.clone(), task_count)?;
      info!(
        target: "fkv_runtime",
        "prewarm crond start task_count={}",
        task_count
      );
      (Some(crond), Some(state))
    }
    _ => (None, None),
  };
  let attach_ident_li = Arc::new(Mutex::new(vec![None; usize::from(settings.num_threads)]));
  let attach_epoch = Arc::new(AttachEpoch::new(settings.num_threads));
  let views = ShardViews {
    archive: archive_state.clone(),
    prewarm: prewarm_state.clone(),
    ident: Some(Arc::clone(&attach_ident_li)),
  };
  let archive_crond = if let Some(state) = &archive_state {
    // 归档 crond 的 sender 不套数据面 timeout：归档本身可能远慢于默认 deadline。
    let route_li: Vec<ShardRoute> = shard_li
      .iter()
      .map(|shard| ShardRoute::new(shard, shard.sender(), &attach_epoch, &views))
      .collect();
    // 配置了归档就必须有定时任务：启动失败上抛，不能静默降级为「归档关闭」。
    let crond = ArchiveCrond::start(state.clone(), route_li)?;
    info!(
      target: "fkv_runtime",
      "archive crond start interval_secs={} retain={}",
      settings.archive_interval_secs,
      archive_num_retained
    );
    Some(crond)
  } else {
    None
  };
  Ok(RuntimeServices {
    cloud_enum,
    max_global_request_batch,
    archive_state,
    prewarm_crond,
    prewarm_state,
    attach_ident_li,
    attach_epoch,
    views,
    archive_crond,
  })
}

impl Drop for Runtime {
  fn drop(&mut self) {
    if let Some(mut crond) = self.archive_crond.take() {
      crond.stop();
    }
    if let Some(mut crond) = self.prewarm_crond.take() {
      crond.stop();
    }
  }
}

impl Runtime {
  pub fn start(config: RuntimeConfig) -> Result<Self> {
    Self::start_impl(config, None, None)
  }

  /// 用 `KvOption` 驱动压缩 / checksum / overflow / compaction / OOM 重试等配置。
  pub fn start_with_option(config: RuntimeConfig, option: &KvOption) -> Result<Self> {
    let mut option = option.clone();
    option.validate()?;
    Self::start_impl(config, Some(&option), None)
  }

  /// 纯内存模式：数据只存在 `MemStore`，进程退出即丢，无 durability。
  ///
  /// `KvOption::derive_mode()` 没有 Mem（C++ `StoreMode` 同样只有
  /// Local/Cloud/Standby），因此单独提供入口，而不是让调用方去碰
  /// `fkv_shard` 的内部 `Backend`。`config.root` 仅用于日志与本地 cache
  /// 约定，本模式不落盘；全局列举 / 定时归档的目标集合为空。
  pub fn start_mem(config: RuntimeConfig) -> Result<Self> {
    Self::start_impl(config, None, Some(Backend::Mem))
  }

  /// 启动运行时：解析配置 → 装配后端 → 投影 shard 配置 → 拉起 shard →
  /// 启动后台服务。
  fn start_impl(
    config: RuntimeConfig,
    option: Option<&KvOption>,
    backend_override: Option<Backend>,
  ) -> Result<Self> {
    let settings = resolve_settings(&config, option)?;
    let BackendState {
      backend,
      root_li,
      mode,
      standby,
    } = assemble_backend(&config, option, backend_override, &settings)?;
    // standby 服务随 shard 配置下沉给 shard 线程；runtime 只需保留模式标记，
    // 供 `global_reopen` 决定是否合并 master 远端分区。
    let standby_replica = standby.is_some();
    let shard_cfg = build_shard_cfg(&config, option, &settings, backend, mode, standby);
    let shard_li = start_shard_li(&shard_cfg, settings.num_threads)?;
    let services = start_services(option, &settings, &shard_cfg, &shard_li, &root_li)?;
    Ok(Self {
      shard_li,
      archive_crond: services.archive_crond,
      archive_state: services.archive_state,
      prewarm_crond: services.prewarm_crond,
      prewarm_state: services.prewarm_state,
      cloud_enum: services.cloud_enum,
      cloud_enum_store: Mutex::new(None),
      root_li,
      standby_replica,
      attach_ident_li: services.attach_ident_li,
      attach_epoch: services.attach_epoch,
      views: services.views,
      max_global_request_batch: services.max_global_request_batch,
      request_timeout: None,
      queue_timeout: None,
      control_li: (0..usize::from(settings.num_threads))
        .map(|_| AsyncMutex::new(()))
        .collect(),
    })
  }

  pub fn num_threads(&self) -> u16 {
    self.shard_li.len() as u16
  }

  /// 数据面请求默认 deadline：所有经 [`Runtime::request`] 的便捷方法生效。
  ///
  /// 控制面 `attach_shard` / `detach_shard` 不套用：这两个必须拿到确定性回执，
  /// 超时会把 shard 状态留在半途。
  pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
    self.request_timeout = Some(timeout);
    self
  }

  /// 数据面请求的队列背压上限：队列满时最长等待 `timeout`，超时回 `Busy`。
  ///
  /// 不设时保持旧行为（无限等待）。
  pub fn with_queue_timeout(mut self, timeout: Duration) -> Self {
    self.queue_timeout = Some(timeout);
    self
  }

  pub fn metrics(&self) -> MetricsSnapshot {
    MetricsSnapshot {
      shard_li: self.shard_li.iter().map(Shard::metrics_snapshot).collect(),
    }
  }

  /// 最近一轮 Cloud 预热回执；未开启预热（非 Cloud，或 `prewarm_cloud_cache=false`）
  /// 时为 `None`。预热在后台线程跑，本查询不进请求路径。
  pub fn prewarm_report(&self) -> Option<PrewarmReport> {
    self
      .prewarm_state
      .as_ref()
      .and_then(|state| lock_prewarm_state(state).last_report)
  }

  pub async fn shutdown(mut self) -> Result<()> {
    info!(
      target: "fkv_runtime",
      "runtime shutdown begin threads={}",
      self.shard_li.len()
    );
    if let Some(mut crond) = self.archive_crond.take() {
      crond.stop();
      info!(target: "fkv_runtime", "archive crond stopped");
    }
    if let Some(mut crond) = self.prewarm_crond.take() {
      crond.stop();
      info!(target: "fkv_runtime", "prewarm crond stopped");
    }
    // 逐个 shutdown 全部 shard 后再统一返回首个错误：首错即 `?` 会让余下 shard
    // 落到 `Drop`（只告警不 join），丢失确定性清理时机。
    let mut first_err = None;
    for shard in take(&mut self.shard_li) {
      if let Err(err) = shard.shutdown().await
        && first_err.is_none()
      {
        first_err = Some(err);
      }
    }
    if let Some(err) = first_err {
      error!(target: "fkv_runtime", "runtime shutdown failed error={err}");
      return Err(err);
    }
    info!(target: "fkv_runtime", "runtime shutdown complete");
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn config() -> RuntimeConfig {
    RuntimeConfig {
      num_threads: 1,
      root: PathBuf::from("/tmp/fkv-startup-defaults"),
      page_size: 4096,
      restart_interval: 16,
      pages_per_file_shift: 18,
      segment_size: 256 * 1024,
      segments_per_file_shift: 7,
      large_value_threshold: 0,
      fixed_buffer_size: 0,
      queue_size: 64,
      auto_reopen_retry_times: 0,
      auto_reopen_pending_time_us: 0,
      archive_interval_secs: 0,
      archive_num_retained: 100,
    }
  }

  /// 未给 `KvOption` 时，`ShardConfig` 必须与 C++ `KvOptions` 默认值一致。
  #[test]
  fn default_option_matches_cpp_defaults() {
    let config = config();
    let settings = resolve_settings(&config, None).unwrap();
    let cfg = build_shard_cfg(
      &config,
      None,
      &settings,
      Backend::Mem,
      StoreMode::Local,
      None,
    );
    assert_eq!(cfg.alloc_mode, AllocMode::Pooled);
    assert!(cfg.verify_checksum);
    assert_eq!(cfg.file_amplify_factor, 2);
    assert_eq!(cfg.segment_file_amplify_factor, 2);
    assert_eq!(cfg.manifest_limit, 8 * 1024 * 1024);
    assert_eq!(cfg.overflow_pointers, 16);
    assert_eq!(cfg.index_cache_bytes, 128 * 1024 * 1024);
    assert_eq!(cfg.index_cache_total_bytes, 128 * 1024 * 1024);
    assert_eq!(cfg.fd_limit, 10_000);
  }
}
