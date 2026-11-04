//! Reopen：读取 manifest 文件，切分对齐 record，replay 成 `ReplayState`。

use core::{future::Future, time::Duration};
use std::thread::sleep;

use fkv_error::{KvError, Result};
use fkv_io::ManifestIo;
use fkv_manifest::{HEADER_BYTES, MANIFEST_ALIGN, parse_header, validate_record};
use fkv_recovery::ReplayState;
use fkv_types::TableIdent;

/// manifest 文件是 `align(record, 4096)` 的连续 record；末尾 torn tail 直接停止。
pub fn split_manifest(bytes: &[u8]) -> Result<Vec<&[u8]>> {
  let mut record_li = Vec::new();
  let mut pos = 0usize;
  while pos + HEADER_BYTES <= bytes.len() {
    let header = match parse_header(&bytes[pos..]) {
      Ok(header) => header,
      Err(_) => break,
    };
    let record_end = pos
      .checked_add(HEADER_BYTES)
      .and_then(|value| value.checked_add(header.payload_len as usize))
      .ok_or(KvError::Corrupted)?;
    if record_end > bytes.len() {
      break;
    }
    let record = &bytes[pos..record_end];
    if !validate_record(record).unwrap_or(false) {
      break;
    }
    record_li.push(record);
    let next = (record_end + MANIFEST_ALIGN - 1) & !(MANIFEST_ALIGN - 1);
    if next <= pos {
      break;
    }
    pos = next;
  }
  Ok(record_li)
}

pub async fn reopen<I: ManifestIo>(io: &I, tbl: &TableIdent) -> Result<ReplayState> {
  reopen_with_size(io, tbl).await.map(|(state, _)| state)
}

pub async fn reopen_with_size<I: ManifestIo>(
  io: &I,
  tbl: &TableIdent,
) -> Result<(ReplayState, usize)> {
  let bytes = io
    .read_manifest(tbl)
    .await?
    .ok_or(KvError::ResourceMissing)?;
  Ok((replay_manifest_bytes(&bytes)?, bytes.len()))
}

/// auto-reopen 的重试策略：manifest 暂不可见时等待并重试。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoReopenOptions {
  pub retry_times: u8,
  pub pending_time: Duration,
}

impl AutoReopenOptions {
  pub fn new(retry_times: u8, pending_time: Duration) -> Self {
    Self {
      retry_times,
      pending_time,
    }
  }
}

pub trait Sleeper {
  fn sleep<'a>(&'a self, duration: Duration) -> impl Future<Output = ()> + 'a;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ThreadSleeper;

impl Sleeper for ThreadSleeper {
  async fn sleep(&self, duration: Duration) {
    sleep(duration);
  }
}

/// 按 `retry_times` 重试 `ResourceMissing`；其它错误立即返回。
pub async fn auto_reopen_with_sleeper<I: ManifestIo, S: Sleeper>(
  io: &I,
  tbl: &TableIdent,
  options: AutoReopenOptions,
  sleeper: &S,
) -> Result<ReplayState> {
  auto_reopen_with_size_with_sleeper(io, tbl, options, sleeper)
    .await
    .map(|(state, _)| state)
}

pub async fn auto_reopen_with_size_with_sleeper<I: ManifestIo, S: Sleeper>(
  io: &I,
  tbl: &TableIdent,
  options: AutoReopenOptions,
  sleeper: &S,
) -> Result<(ReplayState, usize)> {
  let mut remaining = options.retry_times;
  loop {
    match reopen_with_size(io, tbl).await {
      Ok(result) => return Ok(result),
      Err(KvError::ResourceMissing) if remaining > 0 => {
        sleeper.sleep(options.pending_time).await;
        remaining -= 1;
      }
      Err(err) => return Err(err),
    }
  }
}

/// 默认使用阻塞 sleeper；runtime 后续可换成 compio timer。
pub async fn auto_reopen<I: ManifestIo>(
  io: &I,
  tbl: &TableIdent,
  options: AutoReopenOptions,
) -> Result<ReplayState> {
  auto_reopen_with_sleeper(io, tbl, options, &ThreadSleeper).await
}

/// 直接从 manifest 文件字节 replay（branch manifest / 内存镜像）。
pub fn replay_manifest_bytes(bytes: &[u8]) -> Result<ReplayState> {
  let record_li = split_manifest(bytes)?;
  let mut iter = record_li.into_iter();
  let Some(snapshot) = iter.next() else {
    return Err(KvError::ResourceMissing);
  };
  let mut state = ReplayState::from_snapshot(snapshot)?;
  for record in iter {
    state.apply_log(record)?;
  }
  Ok(state)
}
