//! fkv_cli：bench 与检查工具
//!
//! 本 crate 只放可单测的编排逻辑（attach 重放），命令解析与执行留在
//! `main.rs`。

use std::{future::Future, time::Duration};

use compio::time::sleep;
use fkv::{KvError, Result};

/// attach 遇到 `Busy` 时的最大尝试次数（含首次）。
pub const ATTACH_MAX_ATTEMPTS: usize = 3;

/// 重放退避基数：第 `n` 次重试等待 `n × ATTACH_RETRY_BACKOFF`。
pub const ATTACH_RETRY_BACKOFF: Duration = Duration::from_millis(20);

/// 有界重放 attach：只在 `Busy` 时退避重试，其它错误立即上抛。
///
/// `Busy` 表示本次 attach 被更晚的控制操作接管（`Runtime::attach_shard` 的
/// superseded 分支），请求本身没有失败，重放一次即可恢复视图；无界重试会在
/// 控制面持续抖动时把 CLI 挂死，因此封顶 [`ATTACH_MAX_ATTEMPTS`]。
///
/// 端到端造不出 `Busy`：CLI 串行 attach，而该分支要求另一路控制操作在
/// `control_li` 之外推进代际（生产路径不可达，只有 runtime 内部测试能手工
/// 构造）。因此用注入 `Busy` 的闭包覆盖重试次数与终止条件。
pub async fn attach_with_retry<F, Fut>(mut attach: F) -> Result<()>
where
  F: FnMut() -> Fut,
  Fut: Future<Output = Result<()>>,
{
  let mut attempt = 1;
  loop {
    match attach().await {
      Ok(()) => return Ok(()),
      Err(KvError::Busy) if attempt < ATTACH_MAX_ATTEMPTS => {
        let backoff = ATTACH_RETRY_BACKOFF * u32::try_from(attempt).unwrap_or(u32::MAX);
        sleep(backoff).await;
        attempt += 1;
      }
      Err(err) => return Err(err),
    }
  }
}

#[cfg(test)]
mod tests {
  use std::cell::Cell;

  use super::*;

  /// `Busy` 未达上限时重放，成功即停。
  #[compio::test]
  async fn retries_busy_until_success() {
    let calls = Cell::new(0usize);
    let result = attach_with_retry(|| {
      let attempt = calls.get() + 1;
      calls.set(attempt);
      async move {
        if attempt < ATTACH_MAX_ATTEMPTS {
          Err(KvError::Busy)
        } else {
          Ok(())
        }
      }
    })
    .await;
    assert!(result.is_ok());
    assert_eq!(calls.get(), ATTACH_MAX_ATTEMPTS);
  }

  /// 持续 `Busy`：恰好尝试上限次后返回 `Busy`，不得无界重试。
  #[compio::test]
  async fn busy_stops_at_attempt_limit() {
    let calls = Cell::new(0usize);
    let err = attach_with_retry(|| {
      calls.set(calls.get() + 1);
      async { Err(KvError::Busy) }
    })
    .await
    .unwrap_err();
    assert_eq!(err, KvError::Busy);
    assert_eq!(calls.get(), ATTACH_MAX_ATTEMPTS);
  }

  /// 非 `Busy` 错误不重试（例如 `InvalidArgs` 重放也不会成功）。
  #[compio::test]
  async fn non_busy_error_is_not_retried() {
    let calls = Cell::new(0usize);
    let err = attach_with_retry(|| {
      calls.set(calls.get() + 1);
      async { Err(KvError::InvalidArgs) }
    })
    .await
    .unwrap_err();
    assert_eq!(err, KvError::InvalidArgs);
    assert_eq!(calls.get(), 1);
  }
}
