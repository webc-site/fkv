//! 启动期 IO 后端装配：本地 / 多根 / Cloud / standby 与本地根集合。

use std::{
  path::{Path, PathBuf},
  sync::Arc,
};

use fkv_cloud::CacheGroup;
use fkv_config::KvOption;
use fkv_error::{KvError, Result};
use fkv_shard::{Backend, CloudBackend, RootedConfig, split_cloud_store_path};
use fkv_standby::{RsyncSource, RsyncStandbyService, StandbyConfig, StdCommandRunner};
use fkv_types::StoreMode;
use log::{error, info};

use crate::{
  RuntimeConfig,
  config::{ResolvedSettings, cloud_s3_instance_num},
};

/// 本地数据根列表：`store_path` 非空时以其为准，否则保持 `RuntimeConfig::root`。
///
/// `store_path_lut` 由 `KvOption::validate` 构建；这里只检查它已就绪，
/// 不重新计算权重分布（索引越界由 `RootedConfig::new` 校验）。Cloud 模式
/// 不使用该列表（本地 cache 仍是 `RuntimeConfig::root`）。
fn local_root_li(option: Option<&KvOption>, root: &Path) -> Result<Vec<PathBuf>> {
  let Some(option) = option else {
    return Ok(vec![root.to_path_buf()]);
  };
  if option.store_path.is_empty() {
    return Ok(vec![root.to_path_buf()]);
  }
  if option.store_path_lut_li.is_empty() {
    error!(
      target: "fkv_runtime",
      "store_path configured but store_path_lut is empty (validate not called)"
    );
    return Err(KvError::InvalidArgs);
  }
  Ok(
    option
      .store_path
      .iter()
      .map(|path| PathBuf::from(path.as_str()))
      .collect(),
  )
}

/// 按 `KvOption` 的运行模式选择 shard IO 后端。
///
/// - `Local`：本地 `CompioStore`；`store_path` 多根时按 LUT 选根。
/// - `Cloud`：本地 cache + S3（`cloud_store_path` = `bucket/prefix`）。
/// - `StandbyMaster` / `StandbyReplica`：数据面仍是本地 `CompioStore`（standby 是
///   「本地存储之上的复制」）；replica 的分区追赶挂在 reopen 路径上，
///   见 `standby_service()` 与 `fkv_shard::sync_standby_partition`。
///
/// 多根 standby 暂不支持：rsync 追赶服务按单 `local_root` 工作，允许多根会让
/// 副本只追第一个根、其余根静默落后；显式拒绝而不是假装成功。
pub(crate) fn backend_of(option: Option<&KvOption>, root: &Path) -> Result<Backend> {
  let Some(option) = option else {
    return Ok(Backend::Local);
  };
  match option.derive_mode() {
    StoreMode::Local => local_backend(Some(option), root),
    StoreMode::Cloud => {
      // 只支持 S3 兼容后端（`aws` / `s3`）。gcs 等既无实现也无对照，显式拒绝而不是
      // 静默按 S3 起（否则凭据 / endpoint 语义错配会以超时表现，难定位）。
      if !option.cloud_provider.eq_ignore_ascii_case("aws")
        && !option.cloud_provider.eq_ignore_ascii_case("s3")
      {
        error!(
          target: "fkv_runtime",
          "cloud provider unsupported provider={}",
          option.cloud_provider
        );
        return Err(KvError::InvalidArgs);
      }
      let (bucket, prefix) = split_cloud_store_path(&option.cloud_store_path)?;
      Ok(Backend::Cloud(CloudBackend {
        bucket,
        prefix,
        endpoint: option.cloud_endpoint.clone(),
        region: option.cloud_region.clone(),
        access_key: option.cloud_access_key.clone(),
        secret_key: option.cloud_secret_key.clone(),
        allow_http: option.cloud_endpoint.starts_with("http://"),
        // 本地 cache 预算取 `local_space_limit`（0 = 不限），对齐 C++ 的本地空间
        // 上限；整组由全部 shard 与运行时侧预热共享（12d）。
        cache_group: Arc::new(CacheGroup::new(option.local_space_limit)),
        // 复用旧 cache 默认关闭（对齐 C++）；必须显式投影，否则该旋钮无效。
        allow_reuse_local_caches: option.allow_reuse_local_caches,
        max_cloud_concurrency: option.max_cloud_concurrency as usize,
        // 运行时启动后按 `num_threads` 覆写为真实实例数，这里先按单实例。
        s3_instance_num: 1,
        verify_ssl: option.cloud_verify_ssl,
      }))
    }
    StoreMode::StandbyMaster | StoreMode::StandbyReplica => {
      // 数据面仍旧是本地存储（standby 是「本地存储之上的复制」，对齐 C++
      // `StoreMode` 语义）；replica 的分区追赶挂在 reopen 路径上，
      // 见 `standby_service()` 与 `fkv_shard::sync_standby_partition`。
      let root_li = local_root_li(Some(option), root)?;
      if root_li.len() > 1 {
        error!(
          target: "fkv_runtime",
          "standby multi-root store_path unsupported roots={}",
          root_li.len()
        );
        return Err(KvError::InvalidArgs);
      }
      local_backend(Some(option), root)
    }
  }
}

/// 本地后端：`store_path` 为空保持单 `RuntimeConfig::root`；非空时单根 / 多根
/// 都走 `RootedStore`（单根 LUT = `[0]`，固定 buffer 与旧行为一致）。
fn local_backend(option: Option<&KvOption>, root: &Path) -> Result<Backend> {
  let Some(option) = option.filter(|option| !option.store_path.is_empty()) else {
    return Ok(Backend::Local);
  };
  let root_li = local_root_li(Some(option), root)?;
  Ok(Backend::LocalRouted(RootedConfig::new(
    root_li,
    option.store_path_lut_li.clone(),
  )?))
}

/// `StandbyReplica` 的追赶服务；其余模式返回 `None`。
///
/// 来源是全部 `standby_master_store_paths`，按
/// `standby_master_store_path_lut_li[partition_id % lut.len()]` 选根（cld-58，
/// 与本地多根 `RootedConfig::root_index` / C++ `TableIdent::StorePathIndex`
/// 同式）；单根时 LUT 恒 `[0]`，行为不变。`StandbyConfig::source_addr()` 为空
/// （`standby_master_addr` 为空或 `"local"`）时走**本机 rsync**，否则走
/// `rsync`/`ssh <addr>`——与 C++ `StandbyService::RemoteSpec` 同一判定。
fn standby_service(
  option: &KvOption,
  local_root: &Path,
) -> Result<Option<Arc<RsyncStandbyService>>> {
  if option.derive_mode() != StoreMode::StandbyReplica {
    return Ok(None);
  }
  if option.standby_master_store_paths.is_empty()
    || option.standby_master_store_path_lut_li.is_empty()
  {
    error!(
      target: "fkv_runtime",
      "standby replica requires standby_master_store_paths and its lut"
    );
    return Err(KvError::InvalidArgs);
  }
  let config = StandbyConfig::from_option(option, local_root)?;
  // 多根 master：路径与 LUT 都由 `KvOption::validate` 从
  // `standby_master_store_path_weights` 生成；这里只消费，不再自行推导。
  let source = RsyncSource::new_rooted(
    StdCommandRunner,
    option.standby_master_store_paths.clone(),
    option.standby_master_store_path_lut_li.clone(),
    config.source_addr().to_owned(),
  )?;
  Ok(Some(Arc::new(RsyncStandbyService::new(config, source)?)))
}

/// 启动期后端装配结果：shard 配置与 runtime 后台服务共用。
pub(crate) struct BackendState {
  pub(crate) backend: Backend,
  pub(crate) root_li: Vec<PathBuf>,
  pub(crate) mode: StoreMode,
  pub(crate) standby: Option<Arc<RsyncStandbyService>>,
}

/// 构造 IO 后端，并解析运行模式 / standby 服务与本地根集合。
pub(crate) fn assemble_backend(
  config: &RuntimeConfig,
  option: Option<&KvOption>,
  backend_override: Option<Backend>,
  settings: &ResolvedSettings,
) -> Result<BackendState> {
  let mut backend = match backend_override {
    Some(backend) => backend,
    None => backend_of(option, &config.root)?,
  };
  // 每个 shard 各一个 S3ObjectStore 线程；运行时侧另有全局列举（长驻 1 个）、
  // 预热 crond（可选）与归档 crond（可选，惰性创建）。按真实构造点计数，
  // 未启用的 crond 不占额度；归档 crond 的启用谓词与 `start_services` 共用
  // `ResolvedSettings::archive_crond_on`（`interval > 0 && num_retained > 0`）。
  // 并发上限按实例数均分，避免整机实际并发被放大到
  // `实例数 × max_cloud_concurrency`。
  if let Backend::Cloud(cloud) = &mut backend {
    cloud.s3_instance_num = cloud_s3_instance_num(
      settings.num_threads,
      settings.archive_crond_on(),
      option.is_some_and(|opts| opts.prewarm_cloud_cache),
    );
  }
  // 本地全局列举 / 归档 crond 的根集合：Local 用 store_path（单根或多根），
  // Cloud 的本地 cache 仍是 RuntimeConfig::root；Mem 不落盘，目标集合为空。
  let root_li = match &backend {
    Backend::Cloud(_) => vec![config.root.clone()],
    Backend::Mem => Vec::new(),
    _ => local_root_li(option, &config.root)?,
  };
  // 运行模式透传到 shard：决定 `ResourceMissing` 是本地懒创建还是等 manifest 可见。
  let mode = option.map_or(StoreMode::Local, |opts| opts.derive_mode());
  // `StandbyReplica`：reopen 前追赶 master 的服务（`standby_master_addr` 为空或
  // `"local"` 时走本机 rsync）。启动失败必须冒泡——副本没在追赶却报成功是最坏结果。
  let standby = match option {
    // standby 的本地根与数据根一致（`store_path` 单根时用它，否则 RuntimeConfig::root）。
    Some(opts) => standby_service(opts, root_li.first().ok_or(KvError::InvalidArgs)?)?,
    None => None,
  };
  if standby.is_some() {
    info!(
      target: "fkv_runtime",
      "standby replica enabled master_paths={:?} master_path_lut={:?}",
      option.map(|opts| opts.standby_master_store_paths.clone()),
      option.map(|opts| opts.standby_master_store_path_lut_li.clone())
    );
  }
  Ok(BackendState {
    backend,
    root_li,
    mode,
    standby,
  })
}
