//! EloqStore 兼容的错误模型。
//!
//! 枚举值与 `include/error.h` 对齐；保持顺序稳定，方便日志、测试与迁移账本引用。

use core::{fmt, result};
use std::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KvError {
  NoError,
  InvalidArgs,
  NotFound,
  ResourceMissing,
  NotRunning,
  Corrupted,
  EndOfFile,
  OutOfSpace,
  OutOfMem,
  OpenFileLimit,
  TryAgain,
  Busy,
  Timeout,
  NoPermission,
  CloudErr,
  IoFail,
  ExpiredTerm,
  OssInsufficientStorage,
  AlreadyExists,
  LargeValueUnsupported,
}

impl KvError {
  pub const fn as_str(&self) -> &'static str {
    match self {
      Self::NoError => "Succeed",
      Self::InvalidArgs => "Invalid arguments",
      Self::NotFound => "Resource not found",
      Self::ResourceMissing => "Expected resource missing",
      Self::NotRunning => "EloqStore is not running",
      Self::Corrupted => "Disk data corrupted",
      Self::EndOfFile => "End of file",
      Self::OutOfSpace => "Out of disk space",
      Self::OutOfMem => "Out of memory",
      Self::OpenFileLimit => "Too many opened files",
      Self::TryAgain => "Try again later",
      Self::Busy => "Device or resource busy",
      Self::Timeout => "Operation timeout",
      Self::NoPermission => "Operation not permitted",
      Self::CloudErr => "Cloud service is unavailable",
      Self::IoFail => "I/O failure",
      Self::ExpiredTerm => "Expired term",
      Self::OssInsufficientStorage => "Object storage insufficient storage",
      Self::AlreadyExists => "Resource already exists",
      Self::LargeValueUnsupported => "Operation does not support very large value",
    }
  }

  pub const fn is_retryable(&self) -> bool {
    matches!(self, Self::OpenFileLimit | Self::Busy | Self::TryAgain)
  }

  pub const fn is_ok(&self) -> bool {
    matches!(self, Self::NoError)
  }
}

impl fmt::Display for KvError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

impl Error for KvError {}

pub type Result<T> = result::Result<T, KvError>;
