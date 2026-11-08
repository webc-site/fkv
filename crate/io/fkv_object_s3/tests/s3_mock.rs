//! 用本地脚本化 HTTP mock 验证 S3 适配器：重试、状态码映射、条件写、列举。
//!
//! mock 是真实 TCP + HTTP/1.1 服务端，因此覆盖的是 `fkv_s3` 的完整请求路径
//! （SigV4 头、连接、响应解析），而不是只测桩函数。

use std::{
  collections::HashMap,
  future::Future,
  io::{BufRead, BufReader, Read, Write},
  net::{SocketAddr, TcpListener, TcpStream},
  sync::{Arc, Mutex},
  thread,
  time::Duration,
};

use bytes::Bytes;
use compio::runtime::Runtime;
use fkv_error::KvError;
use fkv_object::ObjectStore;
use fkv_object_s3::{RetryPolicy, RuntimeConfig, S3Config, S3ObjectStore};
use futures::future::{join, join_all};

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

type Script = Box<dyn Fn(&str, usize) -> Reply + Send + Sync>;

struct State {
  hits: Mutex<Vec<String>>,
  per_path: Mutex<HashMap<String, usize>>,
  script: Script,
}

struct MockS3 {
  addr: SocketAddr,
  state: Arc<State>,
}

impl MockS3 {
  fn start<F>(script: F) -> Self
  where
    F: Fn(&str, usize) -> Reply + Send + Sync + 'static,
  {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let state = Arc::new(State {
      hits: Mutex::new(Vec::new()),
      per_path: Mutex::new(HashMap::new()),
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

  fn hits(&self) -> usize {
    self.state.hits.lock().unwrap().len()
  }
}

fn serve(stream: TcpStream, state: &State) {
  let mut reader = BufReader::new(stream.try_clone().unwrap());
  let mut request_line = String::new();
  if reader.read_line(&mut request_line).is_err() {
    return;
  }
  let request_line = request_line.trim_end().to_owned();
  let mut content_length = 0usize;
  loop {
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
      return;
    }
    let line = line.trim_end();
    if line.is_empty() {
      break;
    }
    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
      content_length = value.trim().parse().unwrap_or(0);
    }
  }
  if content_length > 0 {
    let mut body = vec![0u8; content_length];
    let _ = reader.read_exact(&mut body);
  }
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
  if !reply.delay.is_zero() {
    thread::sleep(reply.delay);
  }
  let mut stream = stream;
  let raw = format!(
    "HTTP/1.1 {} {}\r\ncontent-length: {}\r\netag: \"etag-{index}\"\r\nconnection: close\r\n\r\n{}",
    reply.status,
    reply.reason,
    reply.body.len(),
    reply.body
  );
  let _ = stream.write_all(raw.as_bytes());
  let _ = stream.flush();
}

fn run<F: Future>(fut: F) -> F::Output {
  Runtime::new().unwrap().block_on(fut)
}

fn fast_retry(max_retries: u32) -> RetryPolicy {
  RetryPolicy {
    max_retries,
    init_backoff: Duration::from_millis(20),
    max_backoff: Duration::from_millis(200),
    multiplier: 2.0,
    retry_timeout: Duration::from_secs(5),
  }
}

fn store(mock: &MockS3, retry: RetryPolicy) -> S3ObjectStore {
  S3ObjectStore::start_with(
    mock.config(),
    RuntimeConfig {
      retry,
      max_concurrency: 4,
      panic_key: None,
    },
  )
  .expect("start")
}

#[test]
fn get_retries_transient_errors_then_succeeds() {
  let mock = MockS3::start(|_path, index| {
    if index < 2 {
      Reply::status(503, "Service Unavailable")
    } else {
      Reply::ok("payload")
    }
  });
  let store = store(&mock, fast_retry(3));
  let value = run(store.get("obj")).expect("get");
  assert_eq!(value.as_deref(), Some(&b"payload"[..]));
  assert_eq!(mock.hits(), 3, "503 两次 + 成功一次");
  store.shutdown().unwrap();
}

#[test]
fn get_404_is_none_and_not_retried() {
  let mock = MockS3::start(|_path, _index| Reply::status(404, "Not Found"));
  let store = store(&mock, fast_retry(3));
  assert_eq!(run(store.get("missing")).expect("get"), None);
  assert_eq!(mock.hits(), 1, "404 不可重试");
  store.shutdown().unwrap();
}

#[test]
fn head_returns_size_and_etag() {
  let mock = MockS3::start(|_path, _index| Reply::ok("hello"));
  let store = store(&mock, fast_retry(1));
  let meta = run(store.head("obj")).expect("head").expect("exists");
  assert_eq!(meta.key, "obj");
  assert_eq!(meta.size, 5);
  // 客户端会把 ETag 两侧的引号去掉（与 C++ 侧一致）。
  assert_eq!(meta.etag, "etag-0");
  store.shutdown().unwrap();
}

#[test]
fn put_condition_failure_maps_to_expired_term() {
  let mock = MockS3::start(|_path, _index| Reply::status(412, "Precondition Failed"));
  let store = store(&mock, fast_retry(1));
  let err =
    run(store.put("obj", Bytes::from_static(b"v"), Some("etag-x"), false)).expect_err("412");
  assert_eq!(err, KvError::ExpiredTerm);
  assert_eq!(mock.hits(), 1, "412 不可重试");
  store.shutdown().unwrap();
}

#[test]
fn insufficient_storage_maps_to_dedicated_error() {
  let mock = MockS3::start(|_path, _index| Reply::status(507, "Insufficient Storage"));
  let store = store(&mock, fast_retry(0));
  let err = run(store.put("obj", Bytes::from_static(b"v"), None, false)).expect_err("507");
  assert_eq!(err, KvError::OssInsufficientStorage);
  store.shutdown().unwrap();
}

#[test]
fn delete_accepts_204() {
  let mock = MockS3::start(|_path, _index| Reply::status(204, "No Content"));
  let store = store(&mock, fast_retry(1));
  run(store.delete("obj")).expect("delete");
  assert_eq!(mock.hits(), 1);
  store.shutdown().unwrap();
}

#[test]
fn list_page_parses_objects() {
  let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult>
  <IsTruncated>false</IsTruncated>
  <Contents><Key>obj</Key><Size>3</Size><ETag>"e1"</ETag></Contents>
</ListBucketResult>"#;
  let mock = MockS3::start(move |_path, _index| Reply::ok(body));
  let store = store(&mock, fast_retry(1));
  let page = run(store.list_page("", None, 100)).expect("list");
  assert_eq!(page.meta_li.len(), 1);
  assert_eq!(page.meta_li[0].key, "obj");
  assert_eq!(page.meta_li[0].size, 3);
  assert_eq!(page.meta_li[0].etag, "e1");
  store.shutdown().unwrap();
}

#[test]
fn commands_run_concurrently() {
  let mock = MockS3::start(|_path, _index| Reply::ok("x").delayed(Duration::from_millis(120)));
  let store = store(&mock, fast_retry(0));
  let started = std::time::Instant::now();
  let (a, b) = run(join(store.get("a"), store.get("b")));
  a.expect("a");
  b.expect("b");
  assert_eq!(mock.hits(), 2);
  assert!(
    started.elapsed() < Duration::from_millis(220),
    "两条命令应并发执行，实际 {:?}",
    started.elapsed()
  );
  store.shutdown().unwrap();
}

#[test]
fn many_parallel_gets_all_reply() {
  let mock = MockS3::start(|_path, _index| Reply::ok("x"));
  let store = store(&mock, fast_retry(0));
  let key_li: Vec<String> = (0..16).map(|i| format!("k{i}")).collect();
  let result_li = run(join_all(key_li.iter().map(|key| store.get(key))));
  for result in result_li {
    assert!(result.expect("get").is_some());
  }
  assert_eq!(mock.hits(), 16);
  store.shutdown().unwrap();
}
