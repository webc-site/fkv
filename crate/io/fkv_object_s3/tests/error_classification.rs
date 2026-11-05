//! HTTP 错误分类与重试策略测试（自 src 内联迁出；被测项以 `#[doc(hidden)]`
//! pub 暴露）。

use std::{collections::HashMap, io::Error as IoError, time::Duration};

use fkv_error::KvError;
use fkv_object_s3::{RetryPolicy, http_status, map_err, status_error};
use object_store::{
  Error,
  client::{HttpError, HttpErrorKind},
};

/// 构造一个带指定传输错误类型的 `object_store::Generic` 错误。
fn transport_error(kind: HttpErrorKind) -> Error {
  Error::Generic {
    store: "S3",
    source: Box::new(HttpError::new(kind, IoError::other("boom"))),
  }
}

#[test]
fn timeout_transport_error_maps_to_timeout() {
  assert_eq!(
    map_err(&transport_error(HttpErrorKind::Timeout)),
    KvError::Timeout
  );
  assert_eq!(
    map_err(&transport_error(HttpErrorKind::Connect)),
    KvError::CloudErr
  );
}

#[test]
fn http_status_parsing() {
  let with_status = |text: &str| Error::Generic {
    store: "S3",
    source: Box::new(IoError::other(text.to_owned())),
  };
  assert_eq!(http_status(&with_status("no marker here")), None);
  assert_eq!(
    http_status(&with_status(
      "Server returned non-2xx status code: 503 Service Unavailable: x"
    )),
    Some(503)
  );
  assert_eq!(
    map_err(&with_status(
      "Server returned non-2xx status code: 507 Insufficient Storage: x"
    )),
    KvError::OssInsufficientStorage
  );
  assert_eq!(
    map_err(&with_status(
      "Server returned non-2xx status code: 400 Bad Request: x"
    )),
    KvError::CloudErr
  );
}

/// 版本锁：状态码只能从 `object_store::Error::Generic` 的显示文本里取（其
/// `source` 是私有 `RetryError`，无法 downcast）。这里钉死 0.14.x 的实际格式，
/// 含重试包装前缀；升级依赖后文本漂移会让本用例先失败，而不是线上静默退化。
#[test]
fn object_store_status_text_format_is_locked() {
  let wrapped = Error::Generic {
    store: "S3",
    source: Box::new(IoError::other(
      "Error performing PUT https://bucket.example/key in 2.1s, after 3 retries, \
       max_retries: 4, retry_timeout: 10s  - Server returned non-2xx status code: \
       507 Insufficient Storage: quota exceeded",
    )),
  };
  assert_eq!(http_status(&wrapped), Some(507));
  assert_eq!(map_err(&wrapped), KvError::OssInsufficientStorage);

  let unavailable = Error::Generic {
    store: "S3",
    source: Box::new(IoError::other(
      "Error performing GET https://bucket.example/key in 1ms  - \
       Server returned non-2xx status code: 503 Service Unavailable: ",
    )),
  };
  assert_eq!(http_status(&unavailable), Some(503));
  assert_eq!(map_err(&unavailable), KvError::Timeout);
}

#[test]
fn status_error_matches_cpp_classification() {
  // 对齐 C++ `AsyncHttpManager::ClassifyHttpError`。
  for status in [408u16, 429, 500, 502, 503, 504, 505] {
    assert_eq!(status_error(status), KvError::Timeout, "status {status}");
  }
  assert_eq!(status_error(507), KvError::OssInsufficientStorage);
  for status in [400u16, 401, 403, 409, 501, 508] {
    assert_eq!(status_error(status), KvError::CloudErr, "status {status}");
  }
}

#[test]
fn retry_policy_defaults_and_env_overrides() {
  // 无环境变量时取 Default 常量；env 覆盖路径单独验证。
  let overrides: HashMap<&str, &str> = HashMap::from_iter([
    ("FKV_S3_MAX_RETRIES", "7"),
    ("FKV_S3_RETRY_INIT_BACKOFF_MS", "5"),
    ("FKV_S3_RETRY_MAX_BACKOFF_MS", "50"),
    ("FKV_S3_RETRY_BACKOFF_BASE", "1.5"),
    ("FKV_S3_RETRY_TIMEOUT_MS", "250"),
  ]);
  let policy = RetryPolicy::from_lookup(|key| overrides.get(key).map(|value| (*value).to_owned()));
  assert_eq!(policy.max_retries, 7);
  assert_eq!(policy.init_backoff, Duration::from_millis(5));
  assert_eq!(policy.max_backoff, Duration::from_millis(50));
  assert_eq!(policy.multiplier, 1.5);
  assert_eq!(policy.retry_timeout, Duration::from_millis(250));
  let config = policy.to_retry_config();
  assert_eq!(config.max_retries, 7);
  assert_eq!(config.backoff.init_backoff, Duration::from_millis(5));
  assert_eq!(config.backoff.max_backoff, Duration::from_millis(50));
  assert_eq!(config.backoff.base, 1.5);

  // 非正倍数（<=0）在 to_retry_config 里回落默认 2.0，不透传坏配置。
  let bad: HashMap<&str, &str> = HashMap::from_iter([("FKV_S3_RETRY_BACKOFF_BASE", "0")]);
  let policy = RetryPolicy::from_lookup(|key| bad.get(key).map(|value| (*value).to_owned()));
  assert_eq!(policy.multiplier, 0.0);
  assert_eq!(policy.to_retry_config().backoff.base, 2.0);
}
