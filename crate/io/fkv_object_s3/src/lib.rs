//! S3 backend：`object_store::aws` + 独立 tokio 线程。
//!
//! shard 线程只发命令 + 等 oneshot，不直接跑 tokio/http。
//!
//! 后台线程在一个 `current_thread` runtime 上为每条命令 `tokio::spawn` 一个任务，
//! 因此单条命令的等待（重试退避、32 MiB 大对象传输）不会阻塞其它命令；
//! 飞行中请求数由 [`RuntimeConfig::max_concurrency`] 限制
//! （对齐 C++ `AsyncHttpManager::max_cloud_concurrency`，默认 64）。
//!
//! 瞬态错误（连接/读写超时、HTTP 408/429/5xx）由 `object_store::RetryConfig`
//! 按 [`RetryPolicy`]（默认开启，最多 3 次、100ms 起、乘 2、上限 15s、装饰抖动）
//! 自动重试；不可重试错误（4xx 等）立即返回，不消耗退避时间。

use std::{
  env,
  error::Error as StdError,
  result,
  str::FromStr,
  sync::Arc,
  thread::{self, JoinHandle},
  time::Duration,
};

use bytes::Bytes;
use crossfire::{
  MAsyncRx, MAsyncTx,
  mpmc::{Array, bounded_async},
};
use fkv_error::{KvError, Result};
use fkv_object::{ObjectMeta, ObjectStore};
use futures::{StreamExt, channel::oneshot, executor::block_on};
use object_store::{
  BackoffConfig, Error, ObjectMeta as StoreMeta, ObjectStoreExt, PutMode, RetryConfig,
  UpdateVersion,
  aws::{AmazonS3, AmazonS3Builder},
  client::{ClientOptions, HttpError, HttpErrorKind},
  path::Path as OsPath,
};
use tokio::{runtime::Builder, task::JoinSet};

/// 默认最大重试次数（不含首次尝试）。
const DEFAULT_MAX_RETRIES: u32 = 3;
/// 默认首次退避时长。
const DEFAULT_INIT_BACKOFF: Duration = Duration::from_millis(100);
/// 默认退避上限。
const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(15);
/// 默认退避倍数。
const DEFAULT_BACKOFF_BASE: f64 = 2.0;
/// 默认单次操作的重试窗口上限（含各次请求耗时）。
const DEFAULT_RETRY_TIMEOUT: Duration = Duration::from_secs(30);
/// 默认最大飞行中请求数。
const DEFAULT_MAX_CONCURRENCY: usize = 64;

/// 重试策略：指数退避 + 抖动。
///
/// 只对瞬态错误生效：连接/读写超时、HTTP 408/429/5xx（含 S3 `SlowDown`）。
/// 4xx 等不可重试错误立即返回。默认开启。
#[derive(Debug, Clone)]
pub struct RetryPolicy {
  /// 最大重试次数（不含首次尝试）；`0` 表示关闭重试。
  pub max_retries: u32,
  /// 首次退避时长；后续退避按 [`RetryPolicy::multiplier`] 增长（带抖动）。
  pub init_backoff: Duration,
  /// 单次退避上限。
  pub max_backoff: Duration,
  /// 退避倍数，必须为正。
  pub multiplier: f64,
  /// 重试窗口上限：从首次请求起超过该时长不再重试。
  pub retry_timeout: Duration,
}

impl Default for RetryPolicy {
  fn default() -> Self {
    Self {
      max_retries: DEFAULT_MAX_RETRIES,
      init_backoff: DEFAULT_INIT_BACKOFF,
      max_backoff: DEFAULT_MAX_BACKOFF,
      multiplier: DEFAULT_BACKOFF_BASE,
      retry_timeout: DEFAULT_RETRY_TIMEOUT,
    }
  }
}

impl RetryPolicy {
  /// 从环境变量读取覆盖项，未设置或无法解析时保持默认值：
  ///
  /// - `FKV_S3_MAX_RETRIES`：最大重试次数
  /// - `FKV_S3_RETRY_INIT_BACKOFF_MS`：首次退避毫秒
  /// - `FKV_S3_RETRY_MAX_BACKOFF_MS`：退避上限毫秒
  /// - `FKV_S3_RETRY_BACKOFF_BASE`：退避倍数（浮点）
  /// - `FKV_S3_RETRY_TIMEOUT_MS`：重试窗口毫秒
  pub fn from_env() -> Self {
    Self::from_lookup(|key| env::var(key).ok())
  }

  /// [`RetryPolicy::from_env`] 的纯函数版本，便于测试。
  #[doc(hidden)]
  pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
    let mut policy = Self::default();
    if let Some(value) = parse_value(&lookup, "FKV_S3_MAX_RETRIES") {
      policy.max_retries = value;
    }
    if let Some(value) = parse_value(&lookup, "FKV_S3_RETRY_INIT_BACKOFF_MS") {
      policy.init_backoff = Duration::from_millis(value);
    }
    if let Some(value) = parse_value(&lookup, "FKV_S3_RETRY_MAX_BACKOFF_MS") {
      policy.max_backoff = Duration::from_millis(value);
    }
    if let Some(value) = parse_value(&lookup, "FKV_S3_RETRY_BACKOFF_BASE") {
      policy.multiplier = value;
    }
    if let Some(value) = parse_value(&lookup, "FKV_S3_RETRY_TIMEOUT_MS") {
      policy.retry_timeout = Duration::from_millis(value);
    }
    policy
  }

  /// 转换为 `object_store` 的重试/退避配置；#[doc(hidden)] 仅供集成测试断言。
  #[doc(hidden)]
  pub fn to_retry_config(&self) -> RetryConfig {
    RetryConfig {
      max_retries: self.max_retries as usize,
      retry_timeout: self.retry_timeout,
      backoff: BackoffConfig {
        init_backoff: self.init_backoff,
        // 非正的倍数会让退避时长退化为 0，这里回退到默认倍数。
        base: if self.multiplier > 0.0 {
          self.multiplier
        } else {
          DEFAULT_BACKOFF_BASE
        },
        max_backoff: self.max_backoff,
      },
    }
  }
}

/// 读取并解析一个环境变量；缺失或解析失败时返回 `None`（保持默认值）。
fn parse_value<T: FromStr>(lookup: &impl Fn(&str) -> Option<String>, key: &str) -> Option<T> {
  lookup(key).and_then(|value| value.parse().ok())
}

/// 后台线程的运行时参数。
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
  /// 瞬态错误重试策略。
  pub retry: RetryPolicy,
  /// 最大飞行中请求数（至少 1）。
  pub max_concurrency: usize,
}

impl Default for RuntimeConfig {
  fn default() -> Self {
    Self {
      retry: RetryPolicy::default(),
      max_concurrency: DEFAULT_MAX_CONCURRENCY,
    }
  }
}

impl RuntimeConfig {
  /// 默认参数 + 环境变量覆盖（`FKV_S3_MAX_CONCURRENCY` 与 `FKV_S3_RETRY_*`）。
  pub fn from_env() -> Self {
    let max_concurrency = parse_value(&|key| env::var(key).ok(), "FKV_S3_MAX_CONCURRENCY")
      .filter(|value: &usize| *value > 0)
      .unwrap_or(DEFAULT_MAX_CONCURRENCY);
    Self {
      retry: RetryPolicy::from_env(),
      max_concurrency,
    }
  }
}

#[derive(Debug, Clone)]
pub struct S3Config {
  pub bucket: String,
  pub endpoint: String,
  pub region: String,
  pub access_key: String,
  pub secret_key: String,
  pub allow_http: bool,
  /// 飞行中 HTTP 请求上限（`KvOption::max_cloud_concurrency`）；`0` 时沿用
  /// `FKV_S3_MAX_CONCURRENCY` / 默认 64。
  pub max_concurrency: usize,
  /// 是否校验服务端 TLS 证书（`KvOption::cloud_verify_ssl`）。默认 `true`；
  /// `false` 仅用于自签名测试端点。
  pub verify_ssl: bool,
}

impl Default for S3Config {
  fn default() -> Self {
    Self {
      bucket: String::new(),
      endpoint: String::new(),
      region: String::new(),
      access_key: String::new(),
      secret_key: String::new(),
      allow_http: false,
      max_concurrency: 0,
      verify_ssl: true,
    }
  }
}

struct Command {
  kind: CommandKind,
  reply: oneshot::Sender<Result<CommandReply>>,
}

enum CommandKind {
  Get(String),
  GetVersioned(String),
  Head(String),
  Put {
    key: String,
    /// 调用方移交的 [`Bytes`]，入队与 `PutPayload` 构造都不再拷贝。
    data: Bytes,
    if_match: Option<String>,
    if_none_match: bool,
  },
  Delete(String),
  List(String),
  ListPage {
    prefix: String,
    token: Option<String>,
    max_keys: usize,
  },
  Shutdown,
}

enum CommandReply {
  Get(Option<Bytes>),
  GetVersioned(Option<(Bytes, String)>),
  Head(Option<ObjectMeta>),
  Put(String),
  Unit,
  List(Vec<ObjectMeta>),
  ListPage(fkv_object::ListPage),
}

pub struct S3ObjectStore {
  tx: MAsyncTx<Array<Command>>,
  join: Option<JoinHandle<()>>,
}

impl S3ObjectStore {
  /// 用默认运行时参数启动（`FKV_S3_*` 环境变量可覆盖重试与并发）。
  pub fn start(config: S3Config) -> Result<Self> {
    let mut runtime = RuntimeConfig::from_env();
    if config.max_concurrency != 0 {
      runtime.max_concurrency = config.max_concurrency;
    }
    Self::start_with(config, runtime)
  }

  /// 用显式运行时参数启动，便于调用方/测试控制重试与并发。
  pub fn start_with(config: S3Config, mut runtime: RuntimeConfig) -> Result<Self> {
    if config.max_concurrency != 0 {
      runtime.max_concurrency = config.max_concurrency;
    }
    // 先构建 store 再起线程：配置错误必须同步返回给调用方，而不是让
    // 后台线程静默退出、后续每条命令都只报 `CloudErr` / `NotRunning`。
    let store = Arc::new(build_store(&config, &runtime.retry)?);
    let (tx, rx) = bounded_async::<Command>(64);
    let join = thread::Builder::new()
      .name("fkv-s3".to_owned())
      .spawn(move || worker(store, runtime, rx))
      .map_err(|_| KvError::IoFail)?;
    Ok(Self {
      tx,
      join: Some(join),
    })
  }

  pub fn shutdown(mut self) -> Result<()> {
    let _ = block_on(self.send(CommandKind::Shutdown));
    if let Some(join) = self.join.take() {
      join.join().map_err(|_| KvError::IoFail)?;
    }
    Ok(())
  }
}

impl Drop for S3ObjectStore {
  /// 只做非阻塞投递：Drop 可能发生在 compio poll 栈内，绝不能 `block_on` 等
  /// 队列空位或 join 线程（会停摆 worker，违反 thread-per-core 纪律）。
  ///
  /// 投递 `Shutdown` 让 worker 先收尾在飞任务；通道已满 / 已关闭则跳过，
  /// 本结构体随后释放 `tx`，worker 在 `recv()` 返回 `Err` 后自然退出。
  /// 需要等待线程真正退出时，请**在 reactor 外**显式调用
  /// [`S3ObjectStore::shutdown`]。
  fn drop(&mut self) {
    if self.join.is_none() {
      return;
    }
    let (reply, _rx) = oneshot::channel();
    let _ = self.tx.try_send(Command {
      kind: CommandKind::Shutdown,
      reply,
    });
  }
}

/// 后台线程入口：单线程 runtime + 逐命令并发派发。
///
/// 不在 `rx.recv()` 循环栈上内联 `.await` 网络 IO，否则一条慢命令（大对象传输、
/// 重试退避）会队头阻塞后续所有命令。
fn worker(store: Arc<RemoteStore>, runtime: RuntimeConfig, rx: MAsyncRx<Array<Command>>) {
  let Ok(runtime_handle) = Builder::new_current_thread().enable_all().build() else {
    return;
  };
  runtime_handle.block_on(async move {
    let max_concurrency = runtime.max_concurrency.max(1);
    let mut tasks: JoinSet<()> = JoinSet::new();
    while let Ok(command) = rx.recv().await {
      if matches!(command.kind, CommandKind::Shutdown) {
        // 先等飞行中的命令收尾，再回复关闭请求，避免调用方拿到 NotRunning。
        while tasks.join_next().await.is_some() {}
        let _ = command.reply.send(Ok(CommandReply::Unit));
        break;
      }
      // 达到并发上限时先回收一个任务，避免无界堆积；不做全局串行等待。
      if tasks.len() >= max_concurrency {
        let _ = tasks.join_next().await;
      }
      let store = store.clone();
      tasks.spawn(async move {
        let reply = handle(&store, command.kind).await;
        let _ = command.reply.send(reply);
      });
    }
  });
}

/// `object_store::list` 把 prefix 当目录并自动补 `/`；fkv 的 `prefix` 是原始
/// 递归字符串前缀。这里取最后一个 `/` 之前的目录部分下推，剩余部分由调用方
/// 用 `starts_with` 过滤。空前缀 / 无斜杠前缀返回 `None`（列整桶）。
fn raw_list_dir(prefix: &str) -> Option<OsPath> {
  if prefix.ends_with('/') {
    let dir = prefix.trim_end_matches('/');
    return (!dir.is_empty()).then(|| OsPath::from(dir));
  }
  if prefix.is_empty() {
    return None;
  }
  let idx = prefix.rfind('/')?;
  let dir = &prefix[..idx];
  (!dir.is_empty()).then(|| OsPath::from(dir))
}

/// S3 store 类型。对象 key 的命名空间（bucket / prefix）全部由引擎层拼好，
/// 这里不再叠加第二层 prefix，避免双前缀。
type RemoteStore = AmazonS3;

fn build_store(config: &S3Config, retry: &RetryPolicy) -> result::Result<RemoteStore, KvError> {
  // `object_store` 的 builder 对空 bucket / 非 URL endpoint 不做同步校验（endpoint
  // 直到发请求才报错），这里先挡住必然失败的配置，让 `start_with` 立即返回。
  if config.bucket.is_empty() {
    return Err(KvError::InvalidArgs);
  }
  if !config.endpoint.is_empty()
    && !(config.endpoint.starts_with("http://") || config.endpoint.starts_with("https://"))
  {
    return Err(KvError::InvalidArgs);
  }
  let mut builder = AmazonS3Builder::new()
    .with_bucket_name(&config.bucket)
    .with_region(&config.region)
    .with_allow_http(config.allow_http)
    .with_virtual_hosted_style_request(false)
    .with_retry(retry.to_retry_config());
  // 显式 key 为空时不要写入空字符串：否则会覆盖 object_store 的默认凭据链
  // （env / IMDS / profile），`cloud_auto_credentials=true` 永远失效。
  if !config.access_key.is_empty() {
    builder = builder.with_access_key_id(&config.access_key);
  }
  if !config.secret_key.is_empty() {
    builder = builder.with_secret_access_key(&config.secret_key);
  }
  if !config.verify_ssl {
    // `with_client_options` 会替换整组客户端选项；不显式带上 `allow_http` 会
    // 把前面的 `with_allow_http(true)` 覆盖回默认 false（http endpoint 直接
    // BadScheme）。
    builder = builder.with_client_options(
      ClientOptions::new()
        .with_allow_invalid_certificates(true)
        .with_allow_http(config.allow_http),
    );
  }
  if !config.endpoint.is_empty() {
    builder = builder.with_endpoint(&config.endpoint);
  }
  builder.build().map_err(|_| KvError::CloudErr)
}

/// 转换 `object_store` 元数据为引擎元数据。
fn to_meta(meta: StoreMeta) -> ObjectMeta {
  ObjectMeta {
    key: meta.location.to_string(),
    etag: meta.e_tag.unwrap_or_default(),
    size: meta.size,
  }
}

async fn handle<T: object_store::ObjectStore>(
  store: &T,
  kind: CommandKind,
) -> Result<CommandReply> {
  match kind {
    CommandKind::Head(key) => match store.head(&OsPath::from(key.as_str())).await {
      Ok(meta) => Ok(CommandReply::Head(Some(to_meta(meta)))),
      Err(Error::NotFound { .. }) => Ok(CommandReply::Head(None)),
      Err(err) => Err(map_err(&err)),
    },
    CommandKind::Get(key) => match store.get(&OsPath::from(key.as_str())).await {
      // `GetResult::bytes()` 已是引用计数的 `Bytes`，直接回传，整对象零拷贝。
      Ok(result) => Ok(CommandReply::Get(Some(
        result.bytes().await.map_err(|err| map_err(&err))?,
      ))),
      Err(Error::NotFound { .. }) => Ok(CommandReply::Get(None)),
      Err(err) => Err(map_err(&err)),
    },
    CommandKind::GetVersioned(key) => match store.get(&OsPath::from(key.as_str())).await {
      Ok(result) => {
        // etag 与 body 同源，且 `bytes()` 会消费 `result`，故先取 etag。
        let etag = result.meta.e_tag.clone().unwrap_or_default();
        match result.bytes().await {
          Ok(bytes) => Ok(CommandReply::GetVersioned(Some((bytes, etag)))),
          Err(err) => Err(map_err(&err)),
        }
      }
      Err(Error::NotFound { .. }) => Ok(CommandReply::GetVersioned(None)),
      Err(err) => Err(map_err(&err)),
    },
    CommandKind::Put {
      key,
      data,
      if_match,
      if_none_match,
    } => {
      let mode = if if_none_match {
        PutMode::Create
      } else if let Some(etag) = if_match {
        PutMode::Update(UpdateVersion {
          e_tag: Some(etag),
          version: None,
        })
      } else {
        PutMode::Overwrite
      };
      match store
        .put_opts(&OsPath::from(key.as_str()), data.into(), mode.into())
        .await
      {
        Ok(result) => Ok(CommandReply::Put(result.e_tag.unwrap_or_default())),
        Err(Error::AlreadyExists { .. }) => Err(KvError::AlreadyExists),
        Err(Error::Precondition { .. }) => Err(KvError::Busy),
        Err(err) => Err(map_err(&err)),
      }
    }
    CommandKind::Delete(key) => match store.delete(&OsPath::from(key.as_str())).await {
      Ok(()) => Ok(CommandReply::Unit),
      Err(Error::NotFound { .. }) => Ok(CommandReply::Unit),
      Err(err) => Err(map_err(&err)),
    },
    CommandKind::Shutdown => Ok(CommandReply::Unit),
    CommandKind::List(prefix) => {
      let mut meta_li = Vec::new();
      // `object_store` 的 list 把 prefix 当目录，自动补 `/`；fkv 契约是原始
      // 字符串递归前缀（`.../manifest_` 必须匹配 `.../manifest_main_0`）。
      // 因此只下推所在目录，剩余前缀在客户端过滤。
      let dir = raw_list_dir(&prefix);
      let mut stream = match &dir {
        Some(dir) => store.list(Some(dir)),
        None => store.list(None),
      };
      while let Some(item) = stream.next().await {
        let meta = to_meta(item.map_err(|err| map_err(&err))?);
        if meta.key.starts_with(&prefix) {
          meta_li.push(meta);
        }
      }
      Ok(CommandReply::List(meta_li))
    }
    CommandKind::ListPage {
      prefix,
      token,
      max_keys,
    } => {
      let mut meta_li: Vec<ObjectMeta> = Vec::new();
      let mut next_token = None;
      // 同 `List`：object_store 只接受目录 prefix，原始前缀的剩余部分在客户端
      // 过滤。token 是上一页最后一个 key，作为 `start-after` 下推。
      let dir = raw_list_dir(&prefix);
      let mut stream = match (&token, &dir) {
        (Some(token), Some(dir)) => {
          store.list_with_offset(Some(dir), &OsPath::from(token.as_str()))
        }
        (Some(token), None) => store.list_with_offset(None, &OsPath::from(token.as_str())),
        (None, Some(dir)) => store.list(Some(dir)),
        (None, None) => store.list(None),
      };
      while let Some(item) = stream.next().await {
        let meta = to_meta(item.map_err(|err| map_err(&err))?);
        // 防御：后端未下推 offset 时仍保证「严格大于 token」契约。
        if let Some(token) = &token
          && meta.key.as_str() <= token.as_str()
        {
          continue;
        }
        if !meta.key.starts_with(&prefix) {
          continue;
        }
        if max_keys != 0 && meta_li.len() == max_keys {
          // 已攒满一页，当前条目作为下一页的起点。
          next_token = meta_li.last().map(|meta| meta.key.clone());
          break;
        }
        meta_li.push(meta);
      }
      Ok(CommandReply::ListPage(fkv_object::ListPage {
        meta_li,
        next_token,
      }))
    }
  }
}

/// 把 `object_store` 错误细分映射为引擎错误码。
///
/// 除既有语义（NotFound / AlreadyExists / Precondition 在调用点单独处理）外，这里
/// 额外区分：
/// - 传输超时/中断 -> [`KvError::Timeout`]
/// - HTTP 408/429/5xx -> [`KvError::Timeout`]
/// - HTTP 507（对象存储空间不足）-> [`KvError::OssInsufficientStorage`]
/// - 其余云端错误 -> [`KvError::CloudErr`]
#[doc(hidden)]
pub fn map_err(err: &Error) -> KvError {
  if let Some(kind) = transport_kind(err) {
    return match kind {
      HttpErrorKind::Timeout | HttpErrorKind::Interrupted => KvError::Timeout,
      _ => KvError::CloudErr,
    };
  }
  http_status(err).map_or(KvError::CloudErr, status_error)
}

/// HTTP 状态码 -> 引擎错误码，对齐 C++ `AsyncHttpManager::ClassifyHttpError`；
/// #[doc(hidden)] 暴露给集成测试与诊断。
#[doc(hidden)]
pub const fn status_error(status: u16) -> KvError {
  match status {
    507 => KvError::OssInsufficientStorage,
    408 | 429 | 500 | 502..=505 => KvError::Timeout,
    _ => KvError::CloudErr,
  }
}

/// 在错误来源链中查找底层 HTTP 传输错误类型（连接失败、超时、中断……）。
fn transport_kind(err: &Error) -> Option<HttpErrorKind> {
  let mut current: Option<&(dyn StdError + 'static)> = Some(err);
  while let Some(inner) = current {
    if let Some(http) = inner.downcast_ref::<HttpError>() {
      return Some(http.kind());
    }
    current = inner.source();
  }
  None
}

/// 从错误文本中尽力提取 HTTP 状态码。
///
/// `object_store` 会把 5xx/429/507 等响应统一折叠为 `Error::Generic`，其
/// `source` 是私有类型 `RetryError`（无法 downcast），状态码只保留在显示文本里
/// （`... - Server returned non-2xx status code: 507 Insufficient Storage: ...`）。
/// 依赖未提供带类型的访问器，这里只能按固定格式解析；解析失败返回 `None`，
/// 调用方退化为 `KvError::CloudErr`。
///
/// 版本锁：`tests/error_classification.rs` 的
/// `object_store_status_text_format_is_locked` 钉死 0.14.x 的文本格式（含重试
/// 前缀）。升级 `object_store` 后该用例会先暴露格式漂移，再决定是否换实现。
#[doc(hidden)]
pub fn http_status(err: &Error) -> Option<u16> {
  const MARKER: &[u8] = b"status code: ";
  let text = err.to_string();
  let bytes = text.as_bytes();
  let start = bytes
    .windows(MARKER.len())
    .position(|window| window == MARKER)?
    + MARKER.len();
  let mut value = 0u32;
  let mut found = false;
  for byte in bytes.iter().skip(start) {
    if !byte.is_ascii_digit() {
      break;
    }
    value = value * 10 + u32::from(byte - b'0');
    if value > u32::from(u16::MAX) {
      return None;
    }
    found = true;
  }
  found.then_some(value as u16)
}

impl ObjectStore for S3ObjectStore {
  async fn get(&self, key: &str) -> Result<Option<Bytes>> {
    match self.send(CommandKind::Get(key.to_owned())).await? {
      CommandReply::Get(value) => Ok(value),
      _ => Err(KvError::CloudErr),
    }
  }

  async fn get_versioned(&self, key: &str) -> Result<Option<(Bytes, String)>> {
    match self.send(CommandKind::GetVersioned(key.to_owned())).await? {
      CommandReply::GetVersioned(pair) => Ok(pair),
      _ => Err(KvError::CloudErr),
    }
  }

  async fn put(
    &self,
    key: &str,
    value: Bytes,
    if_match: Option<&str>,
    if_none_match: bool,
  ) -> Result<String> {
    match self
      .send(CommandKind::Put {
        key: key.to_owned(),
        data: value,
        if_match: if_match.map(str::to_owned),
        if_none_match,
      })
      .await?
    {
      CommandReply::Put(etag) => Ok(etag),
      _ => Err(KvError::CloudErr),
    }
  }

  async fn head(&self, key: &str) -> Result<Option<ObjectMeta>> {
    match self.send(CommandKind::Head(key.to_owned())).await? {
      CommandReply::Head(meta) => Ok(meta),
      _ => Err(KvError::CloudErr),
    }
  }

  async fn delete(&self, key: &str) -> Result<()> {
    match self.send(CommandKind::Delete(key.to_owned())).await? {
      CommandReply::Unit => Ok(()),
      _ => Err(KvError::CloudErr),
    }
  }

  async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>> {
    match self.send(CommandKind::List(prefix.to_owned())).await? {
      CommandReply::List(meta_li) => Ok(meta_li),
      _ => Err(KvError::CloudErr),
    }
  }

  async fn list_page(
    &self,
    prefix: &str,
    token: Option<&str>,
    max_keys: usize,
  ) -> Result<fkv_object::ListPage> {
    match self
      .send(CommandKind::ListPage {
        prefix: prefix.to_owned(),
        token: token.map(str::to_owned),
        max_keys,
      })
      .await?
    {
      CommandReply::ListPage(page) => Ok(page),
      _ => Err(KvError::CloudErr),
    }
  }
}

impl S3ObjectStore {
  async fn send(&self, kind: CommandKind) -> Result<CommandReply> {
    let (reply, rx) = oneshot::channel();
    self
      .tx
      .send(Command { kind, reply })
      .await
      .map_err(|_| KvError::NotRunning)?;
    rx.await.map_err(|_| KvError::NotRunning)?
  }
}
