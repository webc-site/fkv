//! HTTP 状态码分类与重试策略测试（被测项以 `#[doc(hidden)]` pub 暴露）。
//!
//! 旧版依赖 `object_store::Error` 的显示文本做正则匹配；改用自研客户端后
//! 状态码直接可得，这里改为直接断言分类表与退避曲线。

use std::time::Duration;

use fkv_error::KvError;
use fkv_object_s3::{RetryPolicy, map_status};

#[test]
fn status_mapping_matches_cpp_contract() {
  assert_eq!(map_status(404), KvError::ResourceMissing);
  assert_eq!(map_status(403), KvError::ResourceMissing);
  assert_eq!(map_status(409), KvError::TryAgain);
  assert_eq!(map_status(412), KvError::ExpiredTerm);
  assert_eq!(map_status(429), KvError::TryAgain);
  assert_eq!(map_status(503), KvError::TryAgain);
  assert_eq!(map_status(507), KvError::OssInsufficientStorage);
  assert_eq!(map_status(400), KvError::CloudErr);
  assert_eq!(map_status(418), KvError::CloudErr);
}

#[test]
fn retry_policy_backoff_is_exponential_and_capped() {
  let policy = RetryPolicy {
    max_retries: 6,
    init_backoff: Duration::from_millis(100),
    max_backoff: Duration::from_millis(500),
    multiplier: 2.0,
    retry_timeout: Duration::from_secs(30),
  };
  let first = policy.backoff_for(0);
  let second = policy.backoff_for(1);
  let fourth = policy.backoff_for(3);
  assert!(first <= Duration::from_millis(125), "首次退避 {first:?}");
  assert!(
    second >= Duration::from_millis(150),
    "第二次退避 {second:?}"
  );
  assert!(fourth <= policy.max_backoff, "退避必须封顶: {fourth:?}");
  // 抖动用确定性散列，同一 attempt 结果稳定（便于断言）。
  assert_eq!(policy.backoff_for(2), policy.backoff_for(2));
}

#[test]
fn env_overrides_are_parsed() {
  let policy = RetryPolicy::from_lookup(|key| match key {
    "FKV_S3_MAX_RETRIES" => Some("5".to_owned()),
    "FKV_S3_RETRY_INIT_BACKOFF_MS" => Some("7".to_owned()),
    _ => None,
  });
  assert_eq!(policy.max_retries, 5);
  assert_eq!(policy.init_backoff, Duration::from_millis(7));
}
