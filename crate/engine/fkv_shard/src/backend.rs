//! IO 后端枚举：按运行模式在 shard 线程内构造 Local / Mem / Cloud 之一。
//!
//! 用 enum 分派而非 `dyn`：`Store` 对 IO 的 trait bound 是六件套叠加，
//! 枚举转发保持编译期单态化，热路径无虚调用。

use std::{path::PathBuf, rc::Rc, sync::Arc};

use fkv_cloud::{CacheGroup, CloudStore};
use fkv_error::{KvError, Result};
use fkv_io::{
  FileIo, FileVariant, FixedBuffer, FixedSegmentIo, ManifestIo, PageIo, SegmentIo, TermIo,
};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_metrics::ShardMetric;
use fkv_object::{ListPage, ObjectMeta, ObjectStore};
use fkv_object_s3::{S3Config, S3ObjectStore};
use fkv_task_prewarm::PrewarmStat;
use fkv_types::{BranchFileRange, FileId, FilePageId, TableIdent};

use super::roots::{RootedConfig, RootedStore};

/// 本地文件后端：按 `ShardConfig` 的盘面 / IO 参数装配一个 `CompioStore`。
///
/// 单根与多根（`RootedStore`）共用本函数，避免两处装配参数漂移。
pub(crate) fn build_compio_store(config: &super::ShardConfig, root: PathBuf) -> CompioStore {
  CompioStore::new(root, config.page_size, config.pages_per_file_shift)
    .with_segment_config(config.segment_size as usize, config.segments_per_file_shift)
    .with_managed_segment_read()
    .with_max_write_batch_pages(config.max_write_batch_pages)
    .with_max_inflight_write(config.max_inflight_write)
    .with_fd_cache_limit(config.fd_limit)
    // 对齐 C++ `IouringMgr::CreateFile` / `OpenFile`：data / segment 文件统一
    // O_DIRECT，读路径不依赖内核 page cache。manifest 仍走 buffered。
    .with_direct_io(true)
}

/// 本地 cache 根目录与云参数：只在 `Backend::Cloud` 下使用。
#[derive(Debug, Clone)]
pub struct CloudBackend {
  pub bucket: String,
  pub prefix: String,
  pub endpoint: String,
  pub region: String,
  pub access_key: String,
  pub secret_key: String,
  pub allow_http: bool,
  /// 本地 cache 预算组（12d）：同一运行时的全部 shard 与运行时侧预热线程
  /// 共享同一个 `local_space_limit`，不再是每个实例一份上界。
  /// `CacheGroup::new(0)` 表示不限预算。
  pub cache_group: Arc<CacheGroup>,
  /// 是否复用启动前已存在的本地 cache（对齐 C++ `allow_reuse_local_caches`
  /// 默认关闭）。由运行时从 `KvOption` 投影，shard 装配时显式传给
  /// [`CloudStore::with_reuse_local_caches`]；不在这里透传会让该配置成为死旋钮，
  /// 且默认值与 C++ 相反。
  pub allow_reuse_local_caches: bool,
  /// S3 飞行中请求上限（`KvOption::max_cloud_concurrency`；0 = 环境/默认）。
  pub max_cloud_concurrency: usize,
  /// 共享同一份 `max_cloud_concurrency` 的 `S3ObjectStore` 实例数。
  ///
  /// 每个 shard 各起一个 S3 后台线程（`CloudStore` 含 `RefCell`，不能跨线程
  /// 共享），运行时侧另有长驻全局列举、预热 crond、归档 crond 三类实例
  /// （后两者按配置启用），实例数 = `shard_num + 1 + 预热 + 归档`，与
  /// `fkv_runtime::cloud_s3_instance_num` 的四类计数一致。若每个实例都按完整
  /// 上限放行，实际并发会被放大到 `实例数 × 上限`；这里按实例数均分，保证整机
  /// 飞行中请求不超过配置值。1 = 不拆分（单实例）。
  pub s3_instance_num: usize,
  /// 是否校验 S3 服务端 TLS 证书（`KvOption::cloud_verify_ssl`）。
  pub verify_ssl: bool,
}

impl CloudBackend {
  /// 按实例数拆分后的 S3 配置；运行时侧复用同一规则，避免默认值分叉。
  pub fn s3_config(&self) -> S3Config {
    // 0 表示「用环境变量 / 默认值」，不做拆分；否则按实例数均分。
    let max_concurrency = match self.max_cloud_concurrency {
      0 => 0,
      n => s3_quota_share(n, self.s3_instance_num),
    };
    S3Config {
      bucket: self.bucket.clone(),
      endpoint: self.endpoint.clone(),
      region: self.region.clone(),
      access_key: self.access_key.clone(),
      secret_key: self.secret_key.clone(),
      allow_http: self.allow_http,
      max_concurrency,
      verify_ssl: self.verify_ssl,
    }
  }
}

/// 单实例的飞行中请求额度。
///
/// `instances` 必须是**真实 `CloudStore` 构造点数**（shard 数 + 运行时侧实际
/// 启用的列举 / 预热 / 归档实例），否则整机并发会被放大成
/// `实例数 × 单实例额度`。
///
/// 额度按 `n / instances` 向下取整；`n < instances` 时每个实例至少 1
/// （`S3Config::max_concurrency == 0` 表示「用环境默认」，不能表达 0 额度），
/// 此时整机上界是 `instances` 而不是 `instances × n`。
fn s3_quota_share(n: usize, instances: usize) -> usize {
  (n / instances.max(1)).max(1)
}

/// `ShardConfig.backend`：决定 shard 线程内装配哪种 IO。
#[derive(Debug, Clone, Default)]
pub enum Backend {
  /// 本地文件（`CompioStore`）。
  #[default]
  Local,
  /// 多根本地文件：分区按 LUT 选根（对齐 C++ `store_path` / `store_path_lut`）。
  LocalRouted(RootedConfig),
  /// 纯内存（`MemStore`），无 durability。
  Mem,
  /// 本地 cache + S3（`CloudStore<CompioStore, S3ObjectStore>`）。
  Cloud(CloudBackend),
}

#[derive(Clone)]
pub enum StoreBackend {
  // 各后端结构体内联 KB 级状态，统一装箱，避免枚举被最大变体撑大。
  //
  // `Rc` 让每分区 Store 共享同一后端（fd 缓存 / 路径索引 / 在飞写闸门），
  // 克隆只增引用计数，不复制缓存；单 shard 单线程，不引入跨线程共享。
  Local(Rc<RootedStore>),
  Mem(Rc<MemStore>),
  Cloud {
    /// bucket 级对象存储 + 本地 cache 的共享实例。
    store: Rc<CloudStore<CompioStore, S3ObjectStore>>,
    /// store root（`cloud_path.prefix`，忽略首尾 `/`）。
    ///
    /// `Request::ListObject` 的 `prefix` 相对它解释（对齐 C++
    /// `AsyncHttpManager::ComposeKeyFromRemote` 拼接 `cloud_path_.prefix`）；
    /// 返回名也按它 / 用户 prefix 剥离。与 `CloudStore::tbl_prefix` 同口径。
    prefix: String,
  },
}

impl StoreBackend {
  pub fn local(config: &super::ShardConfig) -> Self {
    Self::Local(Rc::new(RootedStore::new(
      config,
      &RootedConfig::single(config.root.clone()),
    )))
  }

  /// 多根本地后端：根列表与 LUT 来自 `Backend::LocalRouted`。
  pub fn local_routed(config: &super::ShardConfig, rooted: &RootedConfig) -> Self {
    Self::Local(Rc::new(RootedStore::new(config, rooted)))
  }

  pub fn mem(config: &super::ShardConfig) -> Self {
    Self::Mem(Rc::new(
      MemStore::with_pages_per_file(config.page_size, config.pages_per_file_shift)
        .with_segment_config(config.segment_size as usize, config.segments_per_file_shift),
    ))
  }

  /// `metrics` 必须是**本 shard** 的 `Arc<ShardMetric>`（cld-55）：不传的话
  /// cloud 本地 cache 的 hit / miss / evict / bytes 四项指标恒 0，且从外部
  /// 看不出「指标没接」和「cache 一直没命中」的区别。
  pub fn cloud(
    config: &super::ShardConfig,
    cloud: &CloudBackend,
    metrics: Arc<ShardMetric>,
  ) -> Result<Self> {
    let local = build_compio_store(config, config.root.clone());
    let remote = S3ObjectStore::start(cloud.s3_config())?;
    let store = CloudStore::new(
      local,
      remote,
      cloud.prefix.clone(),
      config.pages_per_file_shift,
    )
    .with_segment_config(config.segment_size as usize, config.segments_per_file_shift)
    .with_cache_group(cloud.cache_group.clone())
    .with_reuse_local_caches(cloud.allow_reuse_local_caches)
    // shard 是分区写实例：跨进程共用本地 root 时，只有它能启用 / 回填表级
    // segment scheme；运行时侧预热 / 列举 / 归档实例统一置只读。
    .with_read_only(false)
    .with_metrics(metrics);
    // 装配点自检：配置必须真的走到 CloudStore，禁止再回落默认值。
    debug_assert_eq!(
      store.reuse_local_caches(),
      cloud.allow_reuse_local_caches,
      "CloudStore 未按 KvOption 接线 allow_reuse_local_caches"
    );
    debug_assert!(
      !store.read_only(),
      "shard 写实例必须 read_only(false)，否则 scheme 回填无人负责"
    );
    Ok(Self::Cloud {
      store: Rc::new(store),
      // 与 `CloudStore::tbl_prefix` 同口径：忽略首尾 `/`，ListObject 以它为
      // 相对基准（空串 = 整个 bucket，对齐 C++ 未配置 cloud path 的情形）。
      prefix: cloud.prefix.trim_matches('/').to_owned(),
    })
  }
}

impl StoreBackend {
  /// 按分区预热本地 cache（仅 Cloud）。
  ///
  /// 直接复用 `fkv_task_prewarm::prewarm_table`：先 manifest，再 data，最后
  /// segment；磁盘余量不足 / 列举失败 / 下载失败都由该函数返回统计或错误。
  /// Local / Mem / Standby 没有远端对象可拉，显式返回 `InvalidArgs`，不静默
  /// 降级为成功（否则调用方会误以为 cache 已就绪）。
  pub async fn prewarm_table(&self, tbl: &TableIdent) -> Result<PrewarmStat> {
    match self {
      Self::Cloud { store, .. } => fkv_task_prewarm::prewarm_table(store, tbl).await,
      Self::Local(_) | Self::Mem(_) => Err(KvError::InvalidArgs),
    }
  }

  /// 列举 store root 下的远端对象（仅 Cloud）。
  ///
  /// `prefix` 是**相对 store root**（`cloud_path.prefix`）的路径，对齐 C++
  /// `ListObjectRequest::SetRemotePath`：内部按 `ComposeKeyFromRemote` 拼成
  /// 绝对前缀下推，并把返回名按 `StripPrefixFromKey` 剥成相对 `prefix` 的名字
  /// （不是完整 bucket key）。空 `prefix` 列 store root（store root 也为空时
  /// 才是整个 bucket）；前导 `/` 忽略。`tbl` 只用于路由。
  ///
  /// `token` 是上一页的 `next_token`，对调用方不透明（实现上是后端绝对 key），
  /// 只能原样回传。C++ `ListObjectRequest::SetContinuationToken` 拿的是
  /// provider continuation token，形态与本方法的 token 不同，禁止跨实现复用
  /// 或逐项对拍。
  ///
  /// `recursive == false`（对齐 C++ `ListObjectRequest` 默认）只列当前层对象
  /// 与 common prefixes（`ObjectStore::list_level_page`）；`true` 列全部后代。
  /// 回执保留 `last_modified`（C++ `CloudObjectInfo::mod_time`）；common
  /// prefix 合成条目与不记录时间的后端（Mem）为 `None`。`path` 可由
  /// `<store root>/<remote_path>/<返回名>` 重建；`is_dir` 用 common prefix
  /// 合成条目（`/` 结尾 + 空 `etag` + `size == 0`）表达。
  /// 非 Cloud 后端返回 `InvalidArgs`：空列表会被误读成「远端没有对象」。
  pub async fn list_object(
    &self,
    prefix: &str,
    token: Option<&str>,
    max_keys: usize,
    recursive: bool,
  ) -> Result<ListPage> {
    match self {
      Self::Cloud {
        store,
        prefix: root,
      } => list_remote_page(store.remote(), root, prefix, token, max_keys, recursive).await,
      Self::Local(_) | Self::Mem(_) => Err(KvError::InvalidArgs),
    }
  }
}

/// 把用户提供的相对 `RemotePath` 组合到 store root 之后，对齐 C++
/// `AsyncHttpManager::ComposeKeyFromRemote` + `SetupListRequest`：
///
/// - 空 `remote_path` 列 store root；store root 非空时补 `/` 保住边界，
///   避免 `tenant` 把 `tenant2/...` 也列进来；
/// - 非空 `remote_path` 去掉前导 `/` 后按组件拼接，空组件跳过、不重复 `/`；
/// - store root 为空时，组合结果就是 `remote_path` 本身（空串 = 整个 bucket）。
fn compose_list_prefix(root: &str, remote_path: &str) -> String {
  fn append(key: &mut String, part: &str) {
    if part.is_empty() {
      return;
    }
    if !key.is_empty() && !key.ends_with('/') {
      key.push('/');
    }
    key.push_str(part);
  }

  let mut key = String::new();
  append(&mut key, root.trim_matches('/'));
  append(&mut key, remote_path.trim_start_matches('/'));
  if remote_path.is_empty() && !key.is_empty() && !key.ends_with('/') {
    key.push('/');
  }
  key
}

/// 剥掉列举前缀，返回相对 `RemotePath` 的名字（对齐 C++ `StripPrefixFromKey`）：
/// 先按完整前缀匹配，再退化为去掉末尾 `/` 的前缀，最后去掉剩余的前导 `/`。
fn strip_list_prefix<'k>(key: &'k str, prefix: &str) -> &'k str {
  let rest = if prefix.is_empty() {
    key
  } else {
    key
      .strip_prefix(prefix)
      .or_else(|| {
        prefix
          .strip_suffix('/')
          .filter(|trimmed| !trimmed.is_empty())
          .and_then(|trimmed| key.strip_prefix(trimmed))
      })
      .unwrap_or(key)
  };
  rest.trim_start_matches('/')
}

/// 一页 `ListObject`：组合 store root 后下推，名字剥成相对 `RemotePath`。
///
/// `token` 原样透传给后端（绝对 key），因此调用方拿到的 `next_token` 也是
/// 绝对 key——对调用方不透明，只能回传。整页名字都被剥空（对象名恰好等于
/// 列举前缀，C++ `AppendParsedEntry` 同样跳过这类条目）时继续翻页，
/// 避免返回空页让分页提前结束。
async fn list_remote_page<O: ObjectStore>(
  remote: &O,
  root: &str,
  remote_path: &str,
  token: Option<&str>,
  max_keys: usize,
  recursive: bool,
) -> Result<ListPage> {
  let scan_prefix = compose_list_prefix(root, remote_path);
  let mut cursor = token.map(str::to_owned);
  loop {
    let page = if recursive {
      remote
        .list_page(&scan_prefix, cursor.as_deref(), max_keys)
        .await?
    } else {
      remote
        .list_level_page(&scan_prefix, cursor.as_deref(), max_keys)
        .await?
    };
    let ListPage {
      meta_li,
      next_token,
    } = page;
    let meta_li: Vec<ObjectMeta> = meta_li
      .into_iter()
      .filter_map(|mut meta| {
        let name = strip_list_prefix(&meta.key, &scan_prefix);
        if name.is_empty() {
          return None;
        }
        if name.len() != meta.key.len() {
          meta.key = name.to_owned();
        }
        Some(meta)
      })
      .collect();
    if !meta_li.is_empty() || next_token.is_none() {
      return Ok(ListPage {
        meta_li,
        next_token,
      });
    }
    cursor = next_token;
  }
}

/// 按方法清单生成 `StoreBackend` 的三臂转发（`Local` / `Mem` / `Cloud`）。
///
/// 只在 impl 体内展开方法项，因此宏调用可以与手写方法并列：
/// - `async fn` 生成 `async fn`，对应 trait 的 `impl Future` 返回；
/// - `fn` 生成同步转发；两种都支持 `&self` / `&mut self`；
/// - 参数与返回类型原样透传，返回类型可省略（即 `()`）。
///
/// 六个 IO trait 没有泛型参数 / `where` 子句；宏不展开这两种语法，
/// 出现这类签名时保留手写并在 impl 内注明。
///
/// 三个臂都是无条件转发，错误映射与 side-effect 由内层后端负责，宏不介入。
macro_rules! forward_io_enum {
  () => {};

  ($(#[$m:meta])* async fn $name:ident(&self $(, $arg:ident: $ty:ty)* $(,)?) $(-> $ret:ty)?; $($rest:tt)*) => {
    $(#[$m])*
    #[inline]
    async fn $name(&self $(, $arg: $ty)*) $(-> $ret)? {
      match self {
        Self::Local(io) => io.$name($($arg),*).await,
        Self::Mem(io) => io.$name($($arg),*).await,
        Self::Cloud { store: io, .. } => io.$name($($arg),*).await,
      }
    }
    forward_io_enum!($($rest)*);
  };

  ($(#[$m:meta])* async fn $name:ident(&mut self $(, $arg:ident: $ty:ty)* $(,)?) $(-> $ret:ty)?; $($rest:tt)*) => {
    $(#[$m])*
    #[inline]
    async fn $name(&mut self $(, $arg: $ty)*) $(-> $ret)? {
      match self {
        Self::Local(io) => io.$name($($arg),*).await,
        Self::Mem(io) => io.$name($($arg),*).await,
        Self::Cloud { store: io, .. } => io.$name($($arg),*).await,
      }
    }
    forward_io_enum!($($rest)*);
  };

  ($(#[$m:meta])* fn $name:ident(&self $(, $arg:ident: $ty:ty)* $(,)?) $(-> $ret:ty)?; $($rest:tt)*) => {
    $(#[$m])*
    #[inline]
    fn $name(&self $(, $arg: $ty)*) $(-> $ret)? {
      match self {
        Self::Local(io) => io.$name($($arg),*),
        Self::Mem(io) => io.$name($($arg),*),
        Self::Cloud { store: io, .. } => io.$name($($arg),*),
      }
    }
    forward_io_enum!($($rest)*);
  };

  ($(#[$m:meta])* fn $name:ident(&mut self $(, $arg:ident: $ty:ty)* $(,)?) $(-> $ret:ty)?; $($rest:tt)*) => {
    $(#[$m])*
    #[inline]
    fn $name(&mut self $(, $arg: $ty)*) $(-> $ret)? {
      match self {
        Self::Local(io) => io.$name($($arg),*),
        Self::Mem(io) => io.$name($($arg),*),
        Self::Cloud { store: io, .. } => io.$name($($arg),*),
      }
    }
    forward_io_enum!($($rest)*);
  };
}

impl PageIo for StoreBackend {
  forward_io_enum! {
    fn set_file_identity(&self, branch: &str, term: u64);
    fn set_file_range_li(&self, tbl: &TableIdent, range_li: &[BranchFileRange]);
    fn preload_path_li(&self, tbl: &TableIdent) -> usize;
    fn invalidate_path_cache(&self);
    fn set_verify_checksum(&self, enabled: bool);
    fn evict_table(&self, tbl: &TableIdent);
    async fn read_page(&self, tbl: &TableIdent, file_page_id: FilePageId, dst: &mut [u8]) -> Result<()>;
    async fn read_page_owned(&self, tbl: &TableIdent, file_page_id: FilePageId, buf: &mut Vec<u8>) -> Result<()>;
    async fn read_page_li(&self, tbl: &TableIdent, file_page_id_li: &[FilePageId], page_buf_li: &mut [Vec<u8>]) -> Result<()>;
    async fn read_page_span(&self, tbl: &TableIdent, file_page_id: FilePageId, page_buf_li: &mut [Vec<u8>]) -> Result<()>;
    async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()>;
    async fn write_page_li(&self, tbl: &TableIdent, page_li: &[(FilePageId, Vec<u8>)]) -> Result<()>;
    async fn sync_data(&self, tbl: &TableIdent) -> Result<()>;
  }
}

impl ManifestIo for StoreBackend {
  forward_io_enum! {
    async fn reset_manifest(&self, tbl: &TableIdent, size: u64) -> Result<()>;
    async fn reset_manifest_file(&self, tbl: &TableIdent, name: &str, size: u64) -> Result<()>;
    async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()>;
    async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()>;
    async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>>;
    async fn read_local_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>>;
    async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()>;
    async fn refresh_manifest(&self, tbl: &TableIdent) -> Result<()>;
    async fn drop_local_manifest(&self, tbl: &TableIdent) -> Result<()>;
    async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()>;
    async fn append_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()>;
    async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>>;
    /// 旁路文件（segment 完成标记）必须按当前后端显式转发，不做空实现兜底。
    async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>>;
    async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()>;
    async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>>;
    /// 晋升种子必须显式转发：默认走 `write_manifest_file` 会把 Cloud 的晋升
    /// 对象提前发布到远端（sem-60 写时晋升）。
    async fn seed_local_manifest_file(&self, tbl: &TableIdent, name: &str, bytes: &[u8]) -> Result<()>;
    async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()>;
    async fn prune_superseded_manifest_file_li(&self, tbl: &TableIdent, branch: &str, term: u64) -> Result<usize>;
  }
}

impl TermIo for StoreBackend {
  forward_io_enum! {
    async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>>;
    async fn upsert_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<u64>;
    async fn remove_term(&self, branch: &str, shard_id: u32) -> Result<()>;
  }
}

impl SegmentIo for StoreBackend {
  forward_io_enum! {
    fn write_segment_len(&self, src_len: usize) -> u64;
    async fn read_segment(&self, tbl: &TableIdent, segment_fp_id: FilePageId, dst: &mut [u8]) -> Result<()>;
    async fn write_segment(&self, tbl: &TableIdent, segment_fp_id: FilePageId, src: &[u8]) -> Result<()>;
  }
}

impl FileIo for StoreBackend {
  forward_io_enum! {
    async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>>;
    async fn list_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>>;
    async fn list_local_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>>;
    async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>>;
    async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()>;
    async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>>;
    async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()>;
    async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>>;
    async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()>;
    async fn list_local_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>>;
    async fn remove_local_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()>;
    async fn list_local_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>>;
    async fn remove_local_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()>;
    async fn remove_file_variant(&self, tbl: &TableIdent, file_id: FileId, branch: &str, term: u64) -> Result<()>;
    async fn remove_local_file_variant(&self, tbl: &TableIdent, file_id: FileId, branch: &str, term: u64) -> Result<()>;
    async fn remove_segment_file_variant(&self, tbl: &TableIdent, file_id: FileId, branch: &str, term: u64) -> Result<()>;
    async fn remove_local_segment_file_variant(&self, tbl: &TableIdent, file_id: FileId, branch: &str, term: u64) -> Result<()>;
    async fn list_segment_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>>;
    async fn list_local_segment_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>>;
    async fn try_remove_empty_partition_dir(&self, tbl: &TableIdent) -> Result<bool>;
  }
}

impl FixedSegmentIo for StoreBackend {
  forward_io_enum! {
    fn register_fixed_buffer(&self, len: usize) -> Result<FixedBuffer>;
    async fn read_segment_fixed(&self, tbl: &TableIdent, segment_fp_id: FilePageId, buf: &mut FixedBuffer, offset: usize, len: usize) -> Result<()>;
    async fn write_segment_fixed(&self, tbl: &TableIdent, segment_fp_id: FilePageId, buf: &FixedBuffer, offset: usize, len: usize) -> Result<()>;
  }
}

/// 校验 `KvOption::cloud_store_path`（`bucket/prefix`）并拆成两段。
pub fn split_cloud_store_path(path: &str) -> Result<(String, String)> {
  let path = path.strip_suffix('/').unwrap_or(path);
  let (bucket, prefix) = path.split_once('/').unwrap_or((path, ""));
  if bucket.is_empty() {
    return Err(KvError::InvalidArgs);
  }
  Ok((bucket.to_owned(), prefix.to_owned()))
}

#[cfg(test)]
mod tests {
  use fkv_object::MemObjectStore;

  use super::*;

  /// Mem 后端没有远端对象：Prewarm / ListObject 必须显式回 `InvalidArgs`，
  /// 不能静默成功（否则调用方会把「没做」当成「已完成」）。
  #[compio::test]
  async fn mem_backend_rejects_cloud_only_operations() {
    let backend = StoreBackend::Mem(MemStore::new(512).into());
    let tbl = TableIdent::new("demo", 1).unwrap();
    assert_eq!(
      backend.prewarm_table(&tbl).await.unwrap_err(),
      KvError::InvalidArgs
    );
    assert_eq!(
      backend
        .list_object("demo.1/", None, 100, false)
        .await
        .unwrap_err(),
      KvError::InvalidArgs
    );
  }

  /// 造一个带 store root 的远端：root 内两个分区，root 外另有 `root.txt`，
  /// 以及一对容易混淆的租户前缀（`tenant` / `tenant2`）。
  async fn seeded_remote() -> fkv_object::MemObjectStore {
    let remote = MemObjectStore::new();
    for key in [
      "fkv/demo.1/data_0_main_1",
      "fkv/demo.1/manifest_main_1",
      "fkv/demo.1/sub/nested",
      "fkv/demo.2/data_0_main_1",
      "root.txt",
      "tenant/other/a",
      "tenant2/other/a",
    ] {
      remote.put_range(key, 0, b"x").await.unwrap();
    }
    remote
  }

  fn names(page: &ListPage) -> Vec<&str> {
    page.meta_li.iter().map(|meta| meta.key.as_str()).collect()
  }

  /// 对齐 C++ `ComposeKeyFromRemote` + `SetupListRequest`：空 `RemotePath`
  /// 列 store root（补 `/` 保住边界），非空前缀去掉前导 `/` 后拼接。
  #[test]
  fn compose_list_prefix_matches_cpp_compose_key_from_remote() {
    assert_eq!(compose_list_prefix("", ""), "");
    assert_eq!(compose_list_prefix("/", ""), "");
    assert_eq!(compose_list_prefix("fkv", ""), "fkv/");
    assert_eq!(compose_list_prefix("fkv/", ""), "fkv/");
    assert_eq!(compose_list_prefix("", "demo.1/"), "demo.1/");
    assert_eq!(compose_list_prefix("", "/demo.1/"), "demo.1/");
    assert_eq!(compose_list_prefix("fkv", "demo.1/"), "fkv/demo.1/");
    assert_eq!(compose_list_prefix("fkv", "/demo.1/"), "fkv/demo.1/");
    assert_eq!(compose_list_prefix("fkv", "demo.1"), "fkv/demo.1");
    assert_eq!(compose_list_prefix("a/b", "c.1/"), "a/b/c.1/");
  }

  /// 对齐 C++ `StripPrefixFromKey`：完整前缀优先，退化为去掉末尾 `/` 的前缀，
  /// 最后去掉剩余的前导 `/`。
  #[test]
  fn strip_list_prefix_matches_cpp_strip_prefix_from_key() {
    assert_eq!(strip_list_prefix("fkv/demo.1/a", "fkv/demo.1/"), "a");
    assert_eq!(strip_list_prefix("fkv/demo.1/a", "fkv/demo.1"), "a");
    assert_eq!(strip_list_prefix("demo.1/a", "demo."), "1/a");
    assert_eq!(strip_list_prefix("fkv/demo.1/", "fkv/demo.1"), "");
    assert_eq!(strip_list_prefix("other/a", "fkv/"), "other/a");
    assert_eq!(strip_list_prefix("root.txt", ""), "root.txt");
  }

  /// 空 `RemotePath` 列 store root：返回名相对 store root，
  /// 且 `tenant` 不会把 `tenant2/...` 一起列出来（C++ 补 `/` 的边界）。
  #[compio::test]
  async fn empty_remote_path_lists_store_root_only() {
    let remote = seeded_remote().await;
    let page = list_remote_page(&remote, "tenant", "", None, 0, true)
      .await
      .unwrap();
    assert_eq!(names(&page), ["other/a"], "tenant2 不属于 tenant/ 边界");
    assert_eq!(page.next_token, None);

    let page = list_remote_page(&remote, "fkv", "", None, 0, true)
      .await
      .unwrap();
    assert_eq!(
      names(&page),
      [
        "demo.1/data_0_main_1",
        "demo.1/manifest_main_1",
        "demo.1/sub/nested",
        "demo.2/data_0_main_1",
      ]
    );
    assert_eq!(page.next_token, None);
  }

  /// store root 为空时，空 `RemotePath` 才是整个 bucket（C++ 未配置
  /// `cloud_path.prefix` 的情形）；这是唯一会看到 root 外对象的口径。
  #[compio::test]
  async fn empty_root_lists_whole_bucket() {
    let remote = seeded_remote().await;
    let page = list_remote_page(&remote, "", "", None, 0, true)
      .await
      .unwrap();
    let all = names(&page);
    assert!(all.contains(&"root.txt"), "{all:?}");
    assert!(all.contains(&"tenant2/other/a"), "{all:?}");
  }

  /// 相对前缀 + 递归：返回名是相对 `RemotePath` 的名字（不是完整 key），
  /// 前导 `/` 与末尾 `/` 两种写法等价。
  #[compio::test]
  async fn relative_prefix_returns_relative_names() {
    let remote = seeded_remote().await;
    let page = list_remote_page(&remote, "fkv", "/demo.1/", None, 0, true)
      .await
      .unwrap();
    assert_eq!(
      names(&page),
      ["data_0_main_1", "manifest_main_1", "sub/nested"]
    );

    // C++ `DropTable` 用无末尾 `/` 的相对前缀（`table.`）枚举目录：剥离后
    // 剩余名的前导 `/` 要去掉，语义与带 `/` 一致。
    let page = list_remote_page(&remote, "fkv", "demo.1", None, 0, true)
      .await
      .unwrap();
    assert_eq!(
      names(&page),
      ["data_0_main_1", "manifest_main_1", "sub/nested"]
    );
  }

  /// 非递归：只列当前层对象 + common prefixes，名字仍相对 `RemotePath`。
  /// Mem 后端不记录修改时间，`last_modified` 全为 `None`（sem-41 的
  /// `mod_time` 透传只在 S3 后端成立，见 `fkv_object_s3` 的 mock 用例）。
  #[compio::test]
  async fn level_listing_is_relative_to_remote_path() {
    let remote = seeded_remote().await;
    let page = list_remote_page(&remote, "fkv", "demo.1/", None, 0, false)
      .await
      .unwrap();
    assert_eq!(names(&page), ["data_0_main_1", "manifest_main_1", "sub/"]);
    assert_eq!(page.meta_li[2].etag, "");
    assert_eq!(page.meta_li[2].size, 0);
    assert!(page.meta_li.iter().all(|meta| meta.last_modified.is_none()));
  }

  /// 非递归 + 空 `RemotePath`：store root 的非递归视图只含当前层目录
  /// （common prefix），不展开到分区内的对象；空 store root 时列整个 bucket
  /// 的当前层，分页 token 跨页不重不漏。
  #[compio::test]
  async fn level_listing_at_empty_remote_path_covers_store_root() {
    let remote = seeded_remote().await;
    let page = list_remote_page(&remote, "fkv", "", None, 0, false)
      .await
      .unwrap();
    assert_eq!(names(&page), ["demo.1/", "demo.2/"]);
    assert_eq!(page.next_token, None);

    let first = list_remote_page(&remote, "", "", None, 2, false)
      .await
      .unwrap();
    assert_eq!(names(&first), ["fkv/", "root.txt"]);
    assert_eq!(first.next_token.as_deref(), Some("root.txt"));

    let second = list_remote_page(&remote, "", "", first.next_token.as_deref(), 2, false)
      .await
      .unwrap();
    assert_eq!(names(&second), ["tenant/", "tenant2/"]);
    assert_eq!(second.next_token, None);
  }

  /// 递归分页：token 只回传不解释（后端绝对 key），翻页不重不漏。
  #[compio::test]
  async fn recursive_pagination_token_round_trips() {
    let remote = seeded_remote().await;
    let mut token = None;
    let mut all = Vec::new();
    loop {
      let page = list_remote_page(&remote, "fkv", "demo.1/", token.as_deref(), 1, true)
        .await
        .unwrap();
      all.extend(page.meta_li.into_iter().map(|meta| meta.key));
      token = page.next_token;
      if token.is_none() {
        break;
      }
    }
    assert_eq!(all, ["data_0_main_1", "manifest_main_1", "sub/nested"]);
  }

  /// 非递归分页：token 落在条目上，common prefix 的下一页不重不漏。
  #[compio::test]
  async fn level_pagination_continues_after_last_entry() {
    let remote = seeded_remote().await;
    let first = list_remote_page(&remote, "fkv", "demo.1/", None, 2, false)
      .await
      .unwrap();
    assert_eq!(names(&first), ["data_0_main_1", "manifest_main_1"]);
    assert_eq!(
      first.next_token.as_deref(),
      Some("fkv/demo.1/manifest_main_1"),
      "next_token 是不透明的后端绝对 key"
    );

    let second = list_remote_page(
      &remote,
      "fkv",
      "demo.1/",
      first.next_token.as_deref(),
      2,
      false,
    )
    .await
    .unwrap();
    assert_eq!(names(&second), ["sub/"]);
    assert_eq!(second.next_token, None);
  }
}
