//! Reopen：读取 manifest 文件，切分对齐 record，replay 成 `ReplayState`。

use core::time::Duration;

use fkv_error::{KvError, Result};
use fkv_io::ManifestIo;
use fkv_manifest::{HEADER_BYTES, manifest_align, parse_header, validate_record};
use fkv_recovery::ReplayState;
use fkv_types::TableIdent;

/// 切分 manifest 为对齐 record，语义对齐 C++ `Replayer::ParseNextRecord`：
/// - record body 不完整（torn body）→ 停止（EndOfFile）；
/// - checksum 失败 → 跳过该 record + padding 继续；首个 record 损坏直接报错；
/// - 损坏后又出现完好 record → `Corrupted`；
/// - body 完好但 padding 缺失（torn tail）→ 丢弃该 record 并停止。
pub fn split_manifest(bytes: &[u8]) -> Result<Vec<&[u8]>> {
  split_manifest_with_len(bytes).map(|(record_li, _)| record_li)
}

/// 同时返回最后一个完整 record 的结束位置（有效长度）；尾部撕裂字节不计入。
pub fn split_manifest_with_len(bytes: &[u8]) -> Result<(Vec<&[u8]>, usize)> {
  let mut record_li = Vec::new();
  let mut pos = 0usize;
  let mut valid_len = 0usize;
  let mut corrupted = false;
  while pos + HEADER_BYTES <= bytes.len() {
    let header = parse_header(&bytes[pos..])?;
    let record_end = pos
      .checked_add(HEADER_BYTES)
      .and_then(|value| value.checked_add(header.payload_len as usize))
      .ok_or(KvError::Corrupted)?;
    if record_end > bytes.len() {
      // torn body：尾部未写完整，保留之前 record。
      break;
    }
    let next = align_up(record_end);
    let record = &bytes[pos..record_end];
    if !validate_record(record).unwrap_or(false) {
      if record_li.is_empty() {
        return Err(KvError::Corrupted);
      }
      corrupted = true;
      if next <= pos {
        break;
      }
      pos = next;
      continue;
    }
    if next > bytes.len() {
      // body 完好但 padding 未落盘：视为未提交，丢弃本 record。
      break;
    }
    if corrupted {
      return Err(KvError::Corrupted);
    }
    record_li.push(record);
    valid_len = next;
    if next <= pos {
      break;
    }
    pos = next;
  }
  // 文件存在但一个完整 record 都没有（0 字节 / 超短 header）：按损坏处理，
  // 不能当成新表让上层 open_empty 后 rename 覆写。
  if record_li.is_empty() && !bytes.is_empty() {
    return Err(KvError::Corrupted);
  }
  Ok((record_li, valid_len))
}

fn align_up(value: usize) -> usize {
  let align = manifest_align();
  (value + align - 1) & !(align - 1)
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
  // 文件存在但 0 字节：不能当成“新表”去懒创建并覆写，按损坏处理。
  if bytes.is_empty() {
    return Err(KvError::Corrupted);
  }
  replay_manifest_bytes_with_len(&bytes)
}

/// auto-reopen 的重试策略：manifest 暂不可见时等待并重试。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoReopenOption {
  pub retry_times: u8,
  pub pending_time: Duration,
}

impl AutoReopenOption {
  pub fn new(retry_times: u8, pending_time: Duration) -> Self {
    Self {
      retry_times,
      pending_time,
    }
  }
}

/// 直接从 manifest 文件字节 replay（branch manifest / 内存镜像）。
pub fn replay_manifest_bytes(bytes: &[u8]) -> Result<ReplayState> {
  replay_manifest_bytes_with_len(bytes).map(|(state, _)| state)
}

/// 同 [`replay_manifest_bytes`]，并返回最后一个完好 record 的对齐结束偏移
/// （等价 C++ `file_size_before_corrupted_log_`）：尾部撕裂 / 损坏字节不计入，
/// 调用方据此截断后续追加位置。
pub fn replay_manifest_bytes_with_len(bytes: &[u8]) -> Result<(ReplayState, usize)> {
  let (record_li, valid_len) = split_manifest_with_len(bytes)?;
  Ok((replay_record_li(record_li)?, valid_len))
}

fn replay_record_li(record_li: Vec<&[u8]>) -> Result<ReplayState> {
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
