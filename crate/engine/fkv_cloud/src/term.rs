//! term 文件的远端 CAS 读写。

use std::str::from_utf8;

use bytes::Bytes;
use fkv_error::{KvError, Result};
use fkv_io::TermIo;
use fkv_object::ObjectStore;
use fkv_types::current_term_file_name;

use crate::CloudStore;

/// term CAS 的最大尝试次数（含首次）。
pub(crate) const TERM_CAS_RETRY_TIMES: u32 = 10;

impl<I, O: ObjectStore> TermIo for CloudStore<I, O> {
  async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>> {
    let key = self.root_object_key(&current_term_file_name(branch, shard_id));
    let Some(bytes) = self.remote.get(&key).await? else {
      return Ok(None);
    };
    let text = from_utf8(&bytes).map_err(|_| KvError::Corrupted)?;
    text
      .trim()
      .parse()
      .map(Some)
      .map_err(|_| KvError::Corrupted)
  }

  async fn upsert_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<u64> {
    let key = self.root_object_key(&current_term_file_name(branch, shard_id));
    let value = Bytes::from(term.to_string());
    for _ in 0..TERM_CAS_RETRY_TIMES {
      // 对齐 C++ `UpsertTermFile`：value 与 etag 必须来自同一次 GET，
      // 否则 GET 与 HEAD 之间被推进的 term 会被旧 writer 用新 etag 回退。
      let etag = if let Some((bytes, etag)) = self.remote.get_versioned(&key).await? {
        let text = from_utf8(&bytes).map_err(|_| KvError::Corrupted)?;
        let current: u64 = text.trim().parse().map_err(|_| KvError::Corrupted)?;
        if current > term {
          return Err(KvError::ExpiredTerm);
        }
        if current == term {
          return Ok(term);
        }
        Some(etag)
      } else {
        None
      };
      let result = match &etag {
        Some(etag) => {
          self
            .remote
            .put(&key, value.clone(), Some(etag), false)
            .await
        }
        None => self.remote.put(&key, value.clone(), None, true).await,
      };
      match result {
        Ok(_) => return Ok(term),
        // `is_retryable()` 覆盖 Busy 等瞬时冲突；`AlreadyExists` 是 CAS 创建冲突，
        // 不属于瞬时错误分类，但同样应重新读取 etag 后再试。
        Err(err) if err.is_retryable() || err == KvError::AlreadyExists => continue,
        Err(err) => return Err(err),
      }
    }
    // 尝试全部耗尽（每次都输掉 CAS 竞争）：返回**可重试**的 `TryAgain`，
    // 与 C++ `UpsertTermFile`（`src/async_io_manager.cpp:5169`）收尾的不可重试
    // `CloudErr` 是有意差异（登记于 `COMPAT_BASELINE` sem-80）：竞争耗尽只是
    // 瞬时窗口、云服务本身可用，`TryAgain` 既如实（"Try again later"）又让
    // 宿主可在退避后重投递；`concurrent_writers_never_roll_back_term` 依赖该
    // 分类做外层重来。
    Err(KvError::TryAgain)
  }

  async fn remove_term(&self, branch: &str, shard_id: u32) -> Result<()> {
    let key = self.root_object_key(&current_term_file_name(branch, shard_id));
    match self.remote.delete(&key).await {
      Ok(()) => Ok(()),
      // 对象不存在时 delete 可能返回 NotFound；删除语义幂等。
      Err(KvError::NotFound) => Ok(()),
      Err(err) => Err(err),
    }
  }
}
