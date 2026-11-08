//! S3 backend：`fkv_s3`（SigV4 + compio HTTP/1.1）+ 独立后台线程。
//!
//! shard 线程只发命令 + 等 oneshot，不直接跑网络 IO。
//!
//! 后台线程跑一个 compio runtime，为每条命令派发独立 future（`FuturesUnordered`
//! 限制在飞数量），因此单条命令的等待（重试退避、大对象传输）不会队头阻塞
//! 其它命令；飞行中请求数由 [`S3Config::max_concurrency`] 限制（生产由
//! `KvOption::max_cloud_concurrency` 投影而来）。
//!
//! 瞬态错误（连接/读写超时、HTTP 408/429/5xx）按 [`RetryPolicy`] 重试
//! （默认最多 3 次、100ms 起、乘 2、上限 15s、带抖动）；4xx 立即返回。
//!
//! 本模块**不依赖 tokio / object_store**：协议层在 `fkv_s3`，传输层只用 compio。

use std::{
  any::Any,
  env,
  panic::AssertUnwindSafe,
  str::FromStr,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  thread::{self, JoinHandle},
  time::{Duration, Instant},
};

use bytes::Bytes;
use compio::runtime::Runtime;
use crossfire::{
  MAsyncRx, MAsyncTx,
  mpmc::{Array, bounded_async},
};
use fkv_error::{KvError, Result};
use fkv_object::{ObjectMeta, ObjectStore};
use fkv_s3::client::{ListedObject, S3Client, S3Config as ClientConfig};
use futures::{
  FutureExt, StreamExt, channel::oneshot, executor::block_on, stream::FuturesUnordered,
};

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

  /// 第 `attempt` 次失败后的退避时长（指数增长 + ±25% 抖动，封顶）。
  pub fn backoff_for(&self, attempt: u32) -> Duration {
    let base = if self.multiplier > 0.0 {
      self.multiplier
    } else {
      DEFAULT_BACKOFF_BASE
    };
    let factor = base.powi(attempt.min(16) as i32);
    let raw = self.init_backoff.mul_f64(factor);
    let capped = raw.min(self.max_backoff);
    // 抖动：0.75x ~ 1.25x，避免重试风暴同步。
    let jitter = 0.75 + (rand_unit(attempt) * 0.5);
    capped.mul_f64(jitter).min(self.max_backoff)
  }
}

/// 确定性伪随机 [0,1)（按重试序号散列，避免引入 rand 依赖）。
fn rand_unit(attempt: u32) -> f64 {
  let mut x = u64::from(attempt).wrapping_mul(0x9E37_79B9_7F4A_7C15);
  x ^= x >> 30;
  x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
  x ^= x >> 27;
  (x % 10_000) as f64 / 10_000.0
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
  /// 测试注入：命中该对象 key 的命令在 worker 任务内 panic，用于验证单条
  /// 命令 panic 不会伪装成 `NotRunning` 且 worker 继续服务。生产必须为 `None`。
  #[doc(hidden)]
  pub panic_key: Option<String>,
}

impl Default for RuntimeConfig {
  fn default() -> Self {
    Self {
      retry: RetryPolicy::default(),
      max_concurrency: DEFAULT_MAX_CONCURRENCY,
      panic_key: None,
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
      panic_key: None,
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
  /// 非递归列举：下推 S3 `delimiter=/`，只返回当前层条目。
  ListLevelPage {
    prefix: String,
    token: Option<String>,
    max_keys: usize,
  },
  Shutdown,
}

impl CommandKind {
  /// 命令操作的对象 key；列举 / 关闭命令没有单一对象 key。
  fn object_key(&self) -> Option<&str> {
    match self {
      Self::Get(key)
      | Self::GetVersioned(key)
      | Self::Head(key)
      | Self::Put { key, .. }
      | Self::Delete(key) => Some(key),
      Self::List(_) | Self::ListPage { .. } | Self::ListLevelPage { .. } | Self::Shutdown => None,
    }
  }
}

enum CommandReply {
  Get(Option<Bytes>),
  GetVersioned(Option<(Bytes, String)>),
  Head(Option<ObjectMeta>),
  Put(String),
  Unit,
  List(Vec<ObjectMeta>),
  ListPage(fkv_object::ListPage),
  ListLevelPage(fkv_object::ListPage),
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
    let client = Arc::new(build_client(&config)?);
    let (tx, rx) = bounded_async::<Command>(64);
    let join = thread::Builder::new()
      .name("fkv-s3".to_owned())
      .spawn(move || worker(client, runtime.retry.clone(), runtime, rx))
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

/// 后台命令任务中观测到的 panic 次数（进程内累计）。供日志之外的诊断与测试
/// 断言；生产侧可直接用 `log` 收集。
static PANIC_COUNT: AtomicU64 = AtomicU64::new(0);

/// 返回累积的 worker 任务 panic 次数；`#[doc(hidden)]` 仅供集成测试断言。
#[doc(hidden)]
pub fn panic_count() -> u64 {
  PANIC_COUNT.load(Ordering::Relaxed)
}

/// 记录一次 worker 任务 panic：计数 + 日志（payload 尽量还原为原始字符串）。
fn record_panic(payload: &(dyn Any + Send)) {
  PANIC_COUNT.fetch_add(1, Ordering::Relaxed);
  let detail = payload
    .downcast_ref::<&str>()
    .copied()
    .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
    .unwrap_or("<non-string panic payload>");
  log::error!("s3 worker command task panicked: {detail}");
}

/// 后台线程入口：compio runtime + 限制在飞的命令 future。
///
/// 不在 `rx.recv()` 循环栈上内联 `.await` 网络 IO，否则一条慢命令（大对象传输、
/// 重试退避）会队头阻塞后续所有命令。
fn worker(
  client: Arc<S3Client>,
  retry: RetryPolicy,
  runtime: RuntimeConfig,
  rx: MAsyncRx<Array<Command>>,
) {
  let Ok(rt) = Runtime::new() else {
    log::error!("s3 worker: 创建 compio runtime 失败，后续命令将得到 NotRunning");
    return;
  };
  let panic_key = runtime.panic_key.clone();
  rt.block_on(async move {
    let max_concurrency = runtime.max_concurrency.max(1);
    let mut inflight: FuturesUnordered<_> = FuturesUnordered::new();
    let mut shutdown_reply = None;
    loop {
      let command = if inflight.len() >= max_concurrency {
        // 在飞已满：先推进一个任务，不阻塞收命令。
        let _ = inflight.next().await;
        continue;
      } else if inflight.is_empty() {
        match rx.recv().await {
          Ok(command) => command,
          Err(_) => break,
        }
      } else {
        futures::select! {
          maybe = rx.recv().fuse() => match maybe {
            Ok(command) => command,
            Err(_) => break,
          },
          _ = inflight.next().fuse() => continue,
        }
      };
      if matches!(command.kind, CommandKind::Shutdown) {
        // 先等飞行中的命令收尾，再回复关闭请求，避免调用方拿到 NotRunning。
        shutdown_reply = Some(command.reply);
        break;
      }
      let client = client.clone();
      let retry = retry.clone();
      let panic_key = panic_key.clone();
      inflight.push(async move {
        let Command {
          kind,
          reply: reply_tx,
        } = command;
        // 单条命令的 panic 必须在 future 内转成错误回复：否则 reply 发送端随
        // 栈展开被 drop，调用方只会看到 `NotRunning`（worker 其实仍在运行）。
        let reply = match AssertUnwindSafe(async move {
          if panic_key
            .as_deref()
            .is_some_and(|key| kind.object_key() == Some(key))
          {
            panic!("injected s3 worker panic");
          }
          handle(&client, &retry, kind).await
        })
        .catch_unwind()
        .await
        {
          Ok(reply) => reply,
          Err(payload) => {
            record_panic(payload.as_ref());
            Err(KvError::IoFail)
          }
        };
        let _ = reply_tx.send(reply);
      });
    }
    while inflight.next().await.is_some() {}
    if let Some(reply) = shutdown_reply {
      let _ = reply.send(Ok(CommandReply::Unit));
    }
  });
}

/// 由引擎配置构造客户端；配置错误必须同步返回（不能留给后台线程静默失败）。
fn build_client(config: &S3Config) -> Result<S3Client> {
  if config.bucket.is_empty() || config.endpoint.is_empty() {
    return Err(KvError::InvalidArgs);
  }
  let endpoint = fkv_s3::http::Endpoint::parse(&config.endpoint)?;
  if !endpoint.tls && !config.allow_http {
    log::error!("s3: endpoint 是明文 http 但未显式 allow_http");
    return Err(KvError::InvalidArgs);
  }
  if !config.verify_ssl {
    // 自签名端点只在测试里出现；fkv_s3 的 TLS 配置固定校验证书。
    log::warn!("s3: verify_ssl=false 当前不生效（TLS 始终校验证书）");
  }
  // AWS 官方域名用 virtual-host 风格，其余（MinIO / rustfs / 自建）用 path-style。
  let path_style = !endpoint.host.ends_with("amazonaws.com");
  Ok(S3Client::new(ClientConfig {
    endpoint,
    bucket: config.bucket.clone(),
    region: if config.region.is_empty() {
      "us-east-1".to_owned()
    } else {
      config.region.clone()
    },
    access_key: config.access_key.clone(),
    secret_key: config.secret_key.clone(),
    path_style,
  }))
}

/// 错误是否值得重试：连接 / 超时 / 服务端限流都归到 `TryAgain` / `IoFail`。
fn is_retryable(err: &KvError) -> bool {
  matches!(err, KvError::TryAgain | KvError::IoFail)
}

/// 指数退避 + 抖动重试。
async fn with_retry<T, F, Fut>(retry: &RetryPolicy, mut op: F) -> Result<T>
where
  F: FnMut() -> Fut,
  Fut: std::future::Future<Output = Result<T>>,
{
  let started = Instant::now();
  let mut attempt = 0u32;
  loop {
    match op().await {
      Ok(value) => return Ok(value),
      Err(err) => {
        if attempt >= retry.max_retries
          || !is_retryable(&err)
          || started.elapsed() >= retry.retry_timeout
        {
          return Err(err);
        }
        let backoff = retry.backoff_for(attempt);
        attempt += 1;
        log::warn!("s3: {err:?} 第 {attempt} 次重试，退避 {backoff:?}");
        compio::time::sleep(backoff).await;
      }
    }
  }
}

fn to_meta(object: &ListedObject) -> ObjectMeta {
  ObjectMeta {
    key: object.key.clone(),
    etag: object.etag.clone(),
    size: object.size,
    last_modified: None,
  }
}

/// 递归列举：S3 的 `prefix` 本身就是字符串前缀，直接透传。
async fn list_all(client: &S3Client, retry: &RetryPolicy, prefix: &str) -> Result<Vec<ObjectMeta>> {
  let mut out = Vec::new();
  let mut token: Option<String> = None;
  loop {
    let token_ref = token.clone();
    let page = with_retry(retry, || {
      client.list_page(prefix, token_ref.as_deref(), 1000, None)
    })
    .await?;
    out.extend(page.object_li.iter().map(to_meta));
    match page.next_token {
      Some(next) if page.truncated => token = Some(next),
      _ => break,
    }
  }
  Ok(out)
}

async fn list_page_of(
  client: &S3Client,
  retry: &RetryPolicy,
  prefix: &str,
  token: Option<&str>,
  max_keys: usize,
  delimiter: Option<&str>,
) -> Result<fkv_object::ListPage> {
  let page = with_retry(retry, || {
    client.list_page(prefix, token, max_keys, delimiter)
  })
  .await?;
  let mut meta_li: Vec<ObjectMeta> = page.object_li.iter().map(to_meta).collect();
  // 非递归列举的 common prefix 是合成条目：size 0、没有 ETag / 修改时间。
  for prefix_item in &page.prefix_li {
    meta_li.push(ObjectMeta {
      key: prefix_item.clone(),
      etag: String::new(),
      size: 0,
      last_modified: None,
    });
  }
  Ok(fkv_object::ListPage {
    meta_li,
    next_token: page.next_token,
  })
}

async fn handle(client: &S3Client, retry: &RetryPolicy, kind: CommandKind) -> Result<CommandReply> {
  match kind {
    CommandKind::Get(key) => Ok(CommandReply::Get(
      with_retry(retry, || client.get(&key)).await?,
    )),
    CommandKind::GetVersioned(key) => Ok(CommandReply::GetVersioned(
      with_retry(retry, || client.get_with_etag(&key)).await?,
    )),
    CommandKind::Head(key) => {
      let meta = with_retry(retry, || client.head(&key)).await?;
      Ok(CommandReply::Head(meta.map(|meta| ObjectMeta {
        key: key.clone(),
        etag: meta.etag,
        size: meta.size,
        last_modified: None,
      })))
    }
    CommandKind::Put {
      key,
      data,
      if_match,
      if_none_match,
    } => {
      let etag = with_retry(retry, || {
        client.put(&key, data.clone(), if_match.as_deref(), if_none_match)
      })
      .await?;
      Ok(CommandReply::Put(etag.unwrap_or_default()))
    }
    CommandKind::Delete(key) => {
      with_retry(retry, || client.delete(&key)).await?;
      Ok(CommandReply::Unit)
    }
    CommandKind::List(prefix) => Ok(CommandReply::List(list_all(client, retry, &prefix).await?)),
    CommandKind::ListPage {
      prefix,
      token,
      max_keys,
    } => Ok(CommandReply::ListPage(
      list_page_of(client, retry, &prefix, token.as_deref(), max_keys, None).await?,
    )),
    CommandKind::ListLevelPage {
      prefix,
      token,
      max_keys,
    } => Ok(CommandReply::ListLevelPage(
      list_page_of(
        client,
        retry,
        &prefix,
        token.as_deref(),
        max_keys,
        Some("/"),
      )
      .await?,
    )),
    CommandKind::Shutdown => Ok(CommandReply::Unit),
  }
}

/**
 * HTTP 状态码到引擎错误码的映射（与 C++ `ToKvError` 同口径）。
 *
 * `#[doc(hidden)]` 仅供集成测试直接断言分类，生产调用方只看 [`KvError`]。
 */
#[doc(hidden)]
pub fn map_status(status: u16) -> KvError {
  fkv_s3::client::map_status(status)
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

  /// 覆盖默认实现：用 S3 原生 `delimiter=/` 分页，避免客户端扫描全部后代。
  async fn list_level_page(
    &self,
    prefix: &str,
    token: Option<&str>,
    max_keys: usize,
  ) -> Result<fkv_object::ListPage> {
    match self
      .send(CommandKind::ListLevelPage {
        prefix: prefix.to_owned(),
        token: token.map(str::to_owned),
        max_keys,
      })
      .await?
    {
      CommandReply::ListLevelPage(page) => Ok(page),
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
      // 通道关闭 = worker 线程已退出 / 从未启动。
      .map_err(|_| KvError::NotRunning)?;
    // 单条命令的 panic 已由任务内 `catch_unwind` 转成 `IoFail` 回复（见
    // `worker`）；这里收不到回复只剩「worker 提前退出」一种解释，才映射为
    // `NotRunning`。
    rx.await.map_err(|_| KvError::NotRunning)?
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn retry_backoff_grows_and_is_capped() {
    let policy = RetryPolicy {
      max_retries: 5,
      init_backoff: Duration::from_millis(100),
      max_backoff: Duration::from_millis(400),
      multiplier: 2.0,
      retry_timeout: Duration::from_secs(30),
    };
    let first = policy.backoff_for(0);
    let second = policy.backoff_for(1);
    let third = policy.backoff_for(2);
    assert!(first <= Duration::from_millis(125), "{first:?}");
    assert!(second > Duration::from_millis(150), "{second:?}");
    assert!(third <= policy.max_backoff, "{third:?}");
  }

  #[test]
  fn client_config_uses_path_style_for_custom_endpoints() {
    let config = S3Config {
      bucket: "b".to_owned(),
      endpoint: "http://127.0.0.1:9000".to_owned(),
      allow_http: true,
      ..S3Config::default()
    };
    assert!(build_client(&config).is_ok());
    let aws = S3Config {
      bucket: "b".to_owned(),
      endpoint: "https://s3.us-east-1.amazonaws.com".to_owned(),
      ..S3Config::default()
    };
    assert!(build_client(&aws).is_ok());
  }

  #[test]
  fn rejects_plain_http_without_allow_http() {
    let config = S3Config {
      bucket: "b".to_owned(),
      endpoint: "http://127.0.0.1:9000".to_owned(),
      ..S3Config::default()
    };
    assert!(matches!(build_client(&config), Err(KvError::InvalidArgs)));
  }
}
