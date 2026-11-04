use fkv_error::KvError;

#[test]
fn retryable_matches_cpp() {
  assert!(KvError::TryAgain.is_retryable());
  assert!(KvError::Busy.is_retryable());
  assert!(KvError::OpenFileLimit.is_retryable());
  assert!(!KvError::NotFound.is_retryable());
  assert!(!KvError::OutOfMem.is_retryable());
}

#[test]
fn display_is_stable() {
  assert_eq!(KvError::NoError.to_string(), "Succeed");
  assert_eq!(KvError::ExpiredTerm.to_string(), "Expired term");
}
