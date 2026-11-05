use fkv_error::KvError;

#[test]
fn retryable_matches_cpp() {
  // 与 C++ `IsRetryableErr` 对齐：仅这三类瞬时错误可重试。
  let retryable_li = [KvError::TryAgain, KvError::Busy, KvError::OpenFileLimit];
  for err in retryable_li {
    assert!(err.is_retryable(), "{err:?}");
  }
  // 其余变体一律不可重试（逐个列出，新增变体时测试会提醒复核分类）。
  let other_li = [
    KvError::NoError,
    KvError::InvalidArgs,
    KvError::NotFound,
    KvError::ResourceMissing,
    KvError::NotRunning,
    KvError::Corrupted,
    KvError::EndOfFile,
    KvError::OutOfSpace,
    KvError::OutOfMem,
    KvError::Timeout,
    KvError::NoPermission,
    KvError::CloudErr,
    KvError::IoFail,
    KvError::ExpiredTerm,
    KvError::OssInsufficientStorage,
    KvError::AlreadyExists,
    KvError::LargeValueUnsupported,
  ];
  for err in other_li {
    assert!(!err.is_retryable(), "{err:?}");
  }
}

#[test]
fn display_is_stable() {
  assert_eq!(KvError::NoError.to_string(), "Succeed");
  assert_eq!(KvError::ExpiredTerm.to_string(), "Expired term");
}
