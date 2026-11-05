//! 用本地 mock S3（脚本化 HTTP 响应）验证重试、错误映射与并发派发。
//!
//! mock 服务端走真实 HTTP，因此覆盖的是 `object_store::aws` 的完整请求路径
//! （签名、连接、重试与错误包装），而不是只测桩函数。

use std::{
  collections::HashMap,
  future::Future,
  io::{BufRead, BufReader, Read, Write},
  net::{SocketAddr, TcpListener, TcpStream},
  sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
  },
  thread,
  time::{Duration, Instant},
};

use bytes::Bytes;
use fkv_error::KvError;
use fkv_object::ObjectStore;
use fkv_object_s3::{RetryPolicy, RuntimeConfig, S3Config, S3ObjectStore};
use futures::future::{join, join_all};
use tokio::runtime::Builder as RtBuilder;

/// 单条脚本响应。
#[derive(Clone)]
struct Reply {
  status: u16,
  reason: &'static str,
  body: String,
  delay: Duration,
}

impl Reply {
  fn ok(body: &str) -> Self {
    Self {
      status: 200,
      reason: "OK",
      body: body.to_owned(),
      delay: Duration::ZERO,
    }
  }

  fn status(status: u16, reason: &'static str) -> Self {
    Self {
      status,
      reason,
      body: String::new(),
      delay: Duration::ZERO,
    }
  }

  fn delayed(mut self, delay: Duration) -> Self {
    self.delay = delay;
    self
  }
}

/// 响应脚本：`(路径, 该路径第几次请求) -> 响应`。
type Script = Box<dyn Fn(&str, usize) -> Reply + Send + Sync>;

/// mock 服务端共享状态。
struct State {
  /// 收到的请求行，按到达顺序。
  hits: Mutex<Vec<String>>,
  /// 每条路径已命中的次数（用于按路径脚本化响应）。
  per_path: Mutex<HashMap<String, usize>>,
  /// 当前飞行中的请求数。
  live: AtomicUsize,
  /// 飞行中请求数的峰值。
  peak: AtomicUsize,
  /// 脚本：`(路径, 该路径第几次请求) -> 响应`。
  script: Script,
}

/// 本地 mock S3：按脚本返回响应，并统计请求数与并发峰值。
struct MockS3 {
  addr: SocketAddr,
  state: Arc<State>,
}

impl MockS3 {
  /// 启动 mock，`script` 决定每个请求的响应。
  fn start<F>(script: F) -> Self
  where
    F: Fn(&str, usize) -> Reply + Send + Sync + 'static,
  {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let state = Arc::new(State {
      hits: Mutex::new(Vec::new()),
      per_path: Mutex::new(HashMap::new()),
      live: AtomicUsize::new(0),
      peak: AtomicUsize::new(0),
      script: Box::new(script),
    });
    let accept_state = Arc::clone(&state);
    thread::spawn(move || {
      for stream in listener.incoming() {
        let Ok(stream) = stream else { break };
        let state = Arc::clone(&accept_state);
        thread::spawn(move || serve(stream, &state));
      }
    });
    Self { addr, state }
  }

  fn config(&self) -> S3Config {
    S3Config {
      bucket: "bucket".to_owned(),
      endpoint: format!("http://{}", self.addr),
      region: "us-east-1".to_owned(),
      access_key: "access".to_owned(),
      secret_key: "secret".to_owned(),
      allow_http: true,
      ..S3Config::default()
    }
  }

  fn store(&self, retry: RetryPolicy, max_concurrency: usize) -> S3ObjectStore {
    S3ObjectStore::start_with(
      self.config(),
      RuntimeConfig {
        retry,
        max_concurrency,
      },
    )
    .unwrap()
  }

  fn hits(&self) -> usize {
    self.state.hits.lock().unwrap().len()
  }

  fn request_lines(&self) -> Vec<String> {
    self.state.hits.lock().unwrap().clone()
  }

  fn peak(&self) -> usize {
    self.state.peak.load(Ordering::SeqCst)
  }
}

/// 处理单个连接：读请求（含请求体）后按脚本回一个响应，然后关闭连接。
fn serve(mut stream: TcpStream, state: &State) {
  let Ok(clone) = stream.try_clone() else {
    return;
  };
  let mut reader = BufReader::new(clone);
  let mut line = String::new();
  if reader.read_line(&mut line).unwrap_or(0) == 0 {
    return;
  }
  let request_line = line.trim_end().to_owned();
  let mut content_length = 0usize;
  loop {
    let mut header = String::new();
    if reader.read_line(&mut header).unwrap_or(0) == 0 {
      break;
    }
    let header = header.trim_end().to_owned();
    if header.is_empty() {
      break;
    }
    if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length: ") {
      content_length = value.trim().parse().unwrap_or(0);
    }
  }
  if content_length > 0 {
    let mut body = vec![0u8; content_length];
    let _ = reader.read_exact(&mut body);
  }
  // 请求行形如 `GET /bucket/obj HTTP/1.1`，第二段即路径（list 会带 query）。
  let path = request_line
    .split(' ')
    .nth(1)
    .unwrap_or_default()
    .to_owned();
  state.hits.lock().unwrap().push(request_line);
  let index = {
    let mut per_path = state.per_path.lock().unwrap();
    let next = per_path.entry(path.clone()).or_insert(0);
    let index = *next;
    *next += 1;
    index
  };
  let reply = (state.script)(&path, index);

  // 统计并发峰值：从读完请求到写完响应之间算作飞行中。
  let live = state.live.fetch_add(1, Ordering::SeqCst) + 1;
  state.peak.fetch_max(live, Ordering::SeqCst);
  if !reply.delay.is_zero() {
    thread::sleep(reply.delay);
  }
  let raw = format!(
    "HTTP/1.1 {} {}\r\ncontent-length: {}\r\netag: \"etag-{index}\"\r\nconnection: close\r\n\r\n{}",
    reply.status,
    reply.reason,
    reply.body.len(),
    reply.body
  );
  let _ = stream.write_all(raw.as_bytes());
  let _ = stream.flush();
  state.live.fetch_sub(1, Ordering::SeqCst);
}

/// 在带 IO 驱动的单线程 runtime 上执行（reqwest 需要 reactor）。
fn run<F: Future>(fut: F) -> F::Output {
  RtBuilder::new_current_thread()
    .enable_all()
    .build()
    .unwrap()
    .block_on(fut)
}

/// 测试用短退避策略：退避时长压缩到毫秒级，重试次数仍由 `max_retries` 控制。
fn fast_retry(max_retries: u32) -> RetryPolicy {
  RetryPolicy {
    max_retries,
    init_backoff: Duration::from_millis(20),
    max_backoff: Duration::from_millis(200),
    multiplier: 2.0,
    retry_timeout: Duration::from_secs(5),
  }
}

/// 一次最简的 `ListBucketResult`（单对象）。
const LIST_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult><Name>bucket</Name><Prefix></Prefix><KeyCount>1</KeyCount><MaxKeys>1000</MaxKeys><IsTruncated>false</IsTruncated><Contents><Key>obj</Key><LastModified>2026-01-01T00:00:00.000Z</LastModified><ETag>"etag"</ETag><Size>3</Size><StorageClass>STANDARD</StorageClass></Contents></ListBucketResult>"#;

/// `DeleteObjects` 成功响应（删除一个对象）。
const DELETE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?><DeleteResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Deleted><Key>obj</Key></Deleted></DeleteResult>"#;

#[test]
fn get_retries_transient_error_then_succeeds() {
  // 前两次 503，第三次 200：验证「重试 N 次后成功」。
  let mock = MockS3::start(|_path, index| {
    if index < 2 {
      Reply::status(503, "Service Unavailable")
    } else {
      Reply::ok("payload")
    }
  });
  let store = mock.store(fast_retry(3), 4);
  let start = Instant::now();
  let value = run(store.get("obj")).unwrap();
  let elapsed = start.elapsed();
  store.shutdown().unwrap();

  assert_eq!(value.as_deref(), Some(&b"payload"[..]));
  assert_eq!(mock.hits(), 3, "503 x2 + 200 应产生 3 次请求");
  // 退避不阻塞：首次退避固定 20ms，第二次在 [20ms, 40ms] 内，总耗时下限 40ms。
  assert!(
    elapsed >= Duration::from_millis(40),
    "两次指数退避应有等待，实际 {elapsed:?}"
  );
  assert!(
    elapsed < Duration::from_secs(1),
    "退避应受 max_backoff 约束，实际 {elapsed:?}"
  );
}

/// cld-41：`get_versioned` 必须在同一次 GET 响应里同时拿到 body 与 ETag
/// （对齐 C++ `ReadTermFile`），不能再额外发一次 HEAD。
#[test]
fn get_versioned_reads_body_and_etag_in_one_request() {
  let mock = MockS3::start(|_path, _index| Reply::ok("payload"));
  let store = mock.store(fast_retry(3), 4);
  let pair = run(store.get_versioned("obj")).unwrap();
  store.shutdown().unwrap();

  assert_eq!(
    pair,
    Some((Bytes::from_static(b"payload"), "\"etag-0\"".to_owned()))
  );
  assert_eq!(mock.hits(), 1, "value 与 etag 必须来自同一次 GET");
  assert!(mock.request_lines()[0].starts_with("GET "));
}

#[test]
fn put_retries_transient_error_then_succeeds() {
  let mock = MockS3::start(|_path, index| {
    if index < 2 {
      Reply::status(503, "Service Unavailable")
    } else {
      Reply::ok("")
    }
  });
  let store = mock.store(fast_retry(3), 4);
  let etag = run(store.put("obj", Bytes::from_static(b"hello"), None, false)).unwrap();
  store.shutdown().unwrap();

  // S3 的 ETag 响应头带引号，`object_store` 原样透传。
  assert_eq!(etag, "\"etag-2\"");
  assert_eq!(mock.hits(), 3);
}

#[test]
fn head_retries_then_maps_404_to_none() {
  let mock = MockS3::start(|_path, index| {
    if index < 2 {
      Reply::status(503, "Service Unavailable")
    } else {
      Reply::status(404, "Not Found")
    }
  });
  let store = mock.store(fast_retry(3), 4);
  let meta = run(store.head("obj")).unwrap();
  store.shutdown().unwrap();

  assert_eq!(meta, None, "404 对 head 表示对象不存在");
  assert_eq!(mock.hits(), 3, "503 x2 被重试，404 不重试");
}

#[test]
fn delete_retries_transient_error_then_succeeds() {
  let mock = MockS3::start(|_path, index| {
    if index < 1 {
      Reply::status(503, "Service Unavailable")
    } else {
      // 单对象 delete 走 DeleteObjects 批量接口（POST ?delete），成功响应需要 XML 体。
      Reply {
        status: 200,
        reason: "OK",
        body: DELETE_XML.to_owned(),
        delay: Duration::ZERO,
      }
    }
  });
  let store = mock.store(fast_retry(3), 4);
  run(store.delete("obj")).unwrap();
  store.shutdown().unwrap();

  assert_eq!(mock.hits(), 2);
  assert!(mock.request_lines()[0].starts_with("POST "));
}

#[test]
fn list_retries_transient_error_then_succeeds() {
  let mock = MockS3::start(|_path, index| {
    if index < 1 {
      Reply::status(429, "Too Many Requests")
    } else {
      Reply {
        status: 200,
        reason: "OK",
        body: LIST_XML.to_owned(),
        delay: Duration::ZERO,
      }
    }
  });
  let store = mock.store(fast_retry(3), 4);
  let meta_li = run(store.list("")).unwrap();
  store.shutdown().unwrap();

  assert_eq!(meta_li.len(), 1, "解析 ListObjectsV2 结果");
  assert_eq!(meta_li[0].key, "obj");
  assert_eq!(mock.hits(), 2, "429 应被重试");
}

#[test]
fn non_retryable_4xx_fails_immediately() {
  let mock = MockS3::start(|_path, _index| Reply::status(400, "Bad Request"));
  // 把首次退避拉长到 200ms：若发生重试/退避，耗时会被断言捕获。
  let policy = RetryPolicy {
    init_backoff: Duration::from_millis(200),
    ..fast_retry(3)
  };
  let store = mock.store(policy, 4);
  let start = Instant::now();
  let err = run(store.get("obj")).unwrap_err();
  let elapsed = start.elapsed();
  store.shutdown().unwrap();

  assert_eq!(err, KvError::CloudErr);
  assert_eq!(mock.hits(), 1, "4xx 不可重试：只应发起一次请求");
  assert!(
    elapsed < Duration::from_millis(200),
    "不可重试错误不应等待退避，实际 {elapsed:?}"
  );
}

#[test]
fn insufficient_storage_maps_to_oss_error() {
  let mock = MockS3::start(|_path, _index| Reply::status(507, "Insufficient Storage"));
  let store = mock.store(fast_retry(1), 4);
  let err = run(store.put("obj", Bytes::from_static(b"x"), None, false)).unwrap_err();
  store.shutdown().unwrap();

  assert_eq!(err, KvError::OssInsufficientStorage);
}

#[test]
fn exhausted_server_error_maps_to_timeout() {
  // 503 先被重试，耗尽后按 C++ `ClassifyHttpError` 归类为 Timeout 而非 CloudErr。
  let mock = MockS3::start(|_path, _index| Reply::status(503, "Service Unavailable"));
  let store = mock.store(fast_retry(1), 4);
  let err = run(store.get("obj")).unwrap_err();
  store.shutdown().unwrap();

  assert_eq!(err, KvError::Timeout);
  assert_eq!(mock.hits(), 2, "1 次首试 + 1 次重试");
}

#[test]
fn request_timeout_status_maps_to_timeout() {
  let mock = MockS3::start(|_path, _index| Reply::status(408, "Request Timeout"));
  let store = mock.store(fast_retry(0), 4);
  let err = run(store.get("obj")).unwrap_err();
  store.shutdown().unwrap();

  assert_eq!(err, KvError::Timeout);
  assert_eq!(mock.hits(), 1);
}

#[test]
fn slow_request_does_not_block_fast_request() {
  // 串行派发时 fast 会被 slow 挡住，将排在 slow 之后完成。
  let mock = MockS3::start(|path, _index| {
    if path.ends_with("/slow") {
      Reply::ok("slow").delayed(Duration::from_millis(300))
    } else {
      Reply::ok("fast")
    }
  });
  let store = mock.store(fast_retry(0), 4);
  let (slow_elapsed, fast_elapsed) = run(async {
    let start = Instant::now();
    let slow = async {
      store.get("slow").await.unwrap();
      start.elapsed()
    };
    let fast = async {
      store.get("fast").await.unwrap();
      start.elapsed()
    };
    join(slow, fast).await
  });
  store.shutdown().unwrap();

  assert!(
    fast_elapsed < Duration::from_millis(150),
    "fast 请求被 slow 队头阻塞，耗时 {fast_elapsed:?}"
  );
  assert!(
    slow_elapsed >= Duration::from_millis(300),
    "slow 请求应真实等待服务端延迟，耗时 {slow_elapsed:?}"
  );
}

#[test]
fn max_concurrency_limits_in_flight_requests() {
  let mock = MockS3::start(|_path, _index| Reply::ok("x").delayed(Duration::from_millis(60)));
  let store = mock.store(fast_retry(0), 2);
  let keys: Vec<String> = (0..6).map(|index| format!("obj{index}")).collect();
  let values = run(async { join_all(keys.iter().map(|key| store.get(key))).await });
  store.shutdown().unwrap();

  assert_eq!(values.len(), 6);
  assert!(values.iter().all(|value| value.as_ref().unwrap().is_some()));
  assert_eq!(mock.hits(), 6);
  assert_eq!(mock.peak(), 2, "飞行中请求数应被 max_concurrency 限制在 2");
}

/// io-29：第二页必须把 token 下推为 S3 `start-after`，不能每页从头重扫。
#[test]
fn list_page_pushes_token_as_start_after() {
  // 两个对象：`max_keys = 1` 时第一页只返回 obj1，obj2 作为下一页起点被探到。
  const TWO_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult><Name>bucket</Name><Prefix></Prefix><KeyCount>2</KeyCount><MaxKeys>1000</MaxKeys><IsTruncated>false</IsTruncated><Contents><Key>obj1</Key><LastModified>2026-01-01T00:00:00.000Z</LastModified><ETag>"e1"</ETag><Size>1</Size><StorageClass>STANDARD</StorageClass></Contents><Contents><Key>obj2</Key><LastModified>2026-01-01T00:00:00.000Z</LastModified><ETag>"e2"</ETag><Size>1</Size><StorageClass>STANDARD</StorageClass></Contents></ListBucketResult>"#;
  const EMPTY_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult><Name>bucket</Name><Prefix></Prefix><KeyCount>0</KeyCount><MaxKeys>1000</MaxKeys><IsTruncated>false</IsTruncated></ListBucketResult>"#;

  let mock = MockS3::start(|path, _index| {
    if path.contains("start-after=obj1") {
      Reply::ok(EMPTY_XML)
    } else {
      Reply::ok(TWO_XML)
    }
  });
  let store = mock.store(fast_retry(3), 4);
  let page = run(store.list_page("", None, 1)).unwrap();
  assert_eq!(page.meta_li.len(), 1);
  assert_eq!(page.meta_li[0].key, "obj1");
  assert_eq!(page.next_token.as_deref(), Some("obj1"));

  let page = run(store.list_page("", Some("obj1"), 1)).unwrap();
  store.shutdown().unwrap();
  assert!(page.meta_li.is_empty(), "已越过末尾应返回空页");
  assert_eq!(page.next_token, None);

  let line_li = mock.request_lines();
  assert_eq!(line_li.len(), 2);
  assert!(
    line_li[1].contains("start-after=obj1"),
    "第二页应下推 start-after: {}",
    line_li[1]
  );
}

/// 配置错误必须在 `start_with` 同步返回 Err，而不是先起后台线程、让每条
/// 命令都只报 `NotRunning` / `CloudErr`。
#[test]
fn start_with_invalid_config_returns_error_before_spawn() {
  let config = S3Config {
    bucket: "bucket".to_owned(),
    // 非 URL 的 endpoint 会让 `AmazonS3Builder::build` 直接失败。
    endpoint: "not a valid url".to_owned(),
    region: "us-east-1".to_owned(),
    access_key: "key".to_owned(),
    secret_key: "secret".to_owned(),
    allow_http: true,
    ..S3Config::default()
  };
  match S3ObjectStore::start_with(config, RuntimeConfig::default()) {
    Ok(store) => {
      store.shutdown().unwrap();
      panic!("非法 endpoint 不应构建成功");
    }
    Err(err) => assert_eq!(err, KvError::InvalidArgs),
  }
}
