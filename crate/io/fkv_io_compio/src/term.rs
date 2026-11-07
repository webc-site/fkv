//! 本地 `CURRENT_TERM_<branch>_<shard_id>` 文件：读取与 CAS 递增。
//!
//! 内容格式：十进制 ASCII，读时容忍首尾空白。
//! 全部为同步 IO，只允许在阻塞线程池调用。

use std::{
  fs,
  fs::{File, create_dir_all, read, rename},
  io::{Error, ErrorKind, Write},
  path::Path,
};

use fkv_error::{KvError, Result};
use fkv_io::TermIo;
use fkv_types::TMP_SUFFIX;
use itoa::Buffer;

use crate::{CompioStore, fd_cache::spawn_dir_io};

fn io_fail(_: Error) -> KvError {
  KvError::IoFail
}

/// 解析十进制 term；空内容、非数字、溢出均视为损坏。
fn parse(bytes: &[u8]) -> Result<u64> {
  let digit_li = bytes.trim_ascii();
  if digit_li.is_empty() {
    return Err(KvError::Corrupted);
  }
  digit_li
    .iter()
    .try_fold(0u64, |acc, &byte| {
      byte
        .is_ascii_digit()
        .then(|| acc.checked_mul(10)?.checked_add(u64::from(byte - b'0')))
        .flatten()
    })
    .ok_or(KvError::Corrupted)
}

/// 读 term 文件；不存在返回 `None`。
pub(crate) fn load(path: &Path) -> Result<Option<u64>> {
  match read(path) {
    Ok(bytes) => parse(&bytes).map(Some),
    Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
    Err(_) => Err(KvError::IoFail),
  }
}

/// CAS 递增：持有 `root` 目录的独占 flock，读 → 比较 → 写临时文件 → fsync → rename → fsync 目录。
///
/// 单调：已存 term `>` 请求 → `ExpiredTerm`；不存在、相等或更小 → 写入新值。
/// 相等时幂等成功，与 C++ `UpsertTermFile` 及 Cloud/Mem 后端一致。
///
/// flock 作用于打开的文件描述，同进程多个任务 / 多进程之间都互斥。
pub(crate) fn upsert(root: &Path, name: &str, term: u64) -> Result<u64> {
  create_dir_all(root).map_err(io_fail)?;
  let dir = File::open(root).map_err(io_fail)?;
  // 锁随 `dir` drop 释放。
  dir.lock().map_err(io_fail)?;
  let path = root.join(name);
  match load(&path)? {
    Some(stored) if stored > term => return Err(KvError::ExpiredTerm),
    _ => {}
  }
  let tmp = root.join(format!("{name}{TMP_SUFFIX}"));
  let mut file = File::create(&tmp).map_err(io_fail)?;
  file
    .write_all(Buffer::new().format(term).as_bytes())
    .map_err(io_fail)?;
  file.sync_all().map_err(io_fail)?;
  drop(file);
  rename(&tmp, &path).map_err(io_fail)?;
  // rename 只改目录项：fsync 目录保证掉电后新 term 可见。
  dir.sync_all().map_err(io_fail)?;
  Ok(term)
}

impl TermIo for CompioStore {
  async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>> {
    let path = self
      .root
      .join(fkv_types::current_term_file_name(branch, shard_id));
    spawn_dir_io(move || self::load(&path)).await?
  }

  /// 同步 IO（flock + fsync + rename）放在阻塞线程池，不阻塞 reactor。
  async fn upsert_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<u64> {
    let root = self.root.clone();
    let name = fkv_types::current_term_file_name(branch, shard_id);
    spawn_dir_io(move || self::upsert(&root, &name, term)).await?
  }

  async fn remove_term(&self, branch: &str, shard_id: u32) -> Result<()> {
    let path = self
      .root
      .join(fkv_types::current_term_file_name(branch, shard_id));
    spawn_dir_io(move || match fs::remove_file(&path) {
      Ok(()) => Ok(()),
      Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
      Err(_) => Err(KvError::IoFail),
    })
    .await?
  }
}
