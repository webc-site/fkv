//! `FileIo` 实现：整文件读写、空分区目录移除。

use std::{
  fs::{read_dir, remove_dir},
  io::{self, ErrorKind},
};

use compio::fs::{File, remove_file, rename};
use fkv_error::{KvError, Result};
use fkv_failpoint::points::{WRITE_FILE_RENAME, WRITE_FILE_SYNC_FAIL};
use fkv_io::{FileIo, FileVariant};
use fkv_types::{FileId, TableIdent};

use crate::{CompioStore, FileKind, fd_cache::spawn_dir_io, map_io_err, tmp_path};

impl FileIo for CompioStore {
  async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>> {
    match self.open_data_file(tbl, file_id).await? {
      Some(file) => self.read_whole(&file, self.direct_io.get()).await.map(Some),
      None => Ok(None),
    }
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()> {
    if !bytes.len().is_multiple_of(self.page_size) {
      return Err(KvError::InvalidArgs);
    }
    self.ensure_dir(tbl).await?;
    let path = self.data_path(tbl, file_id << self.pages_per_file_shift);
    // tmp + rename 原子替换：崩溃只可能留下 `.tmp`，不会让可见文件出现
    // 「前半段新内容、后半段旧内容 / 零洞」的中间态。
    let tmp = tmp_path(&path);
    let mut file = File::create(&tmp).await.map_err(map_io_err)?;
    if let Err(err) = self.write_whole(&mut file, bytes, false).await {
      drop(file);
      let _ = remove_file(&tmp).await;
      return Err(err);
    }
    // 对齐 C++ `WriteSnapshot`：rename 提交目录项前必须先 fdatasync tmp，
    // 否则掉电后可见文件可能是 0 长 / 稀疏洞，而 cloud 已按 bytes.len()
    // 把它登记为 resident，后续读不再回源。
    let mut sync_result = file.sync_data().await;
    if fkv_failpoint::fail_point(WRITE_FILE_SYNC_FAIL).is_some() {
      sync_result = Err(io::Error::other("injected write_file sync failure"));
    }
    if let Err(err) = sync_result {
      drop(file);
      let _ = remove_file(&tmp).await;
      return Err(map_io_err(err));
    }
    drop(file);
    rename(&tmp, &path).await.map_err(map_io_err)?;
    // 已改名、但目录项 fsync 仍推迟到下一次 `sync_data`（见函数尾）：这是掉电后
    // 文件可能"消失"的窗口。补 kill point 让它可注入（gap-12）。
    fkv_failpoint::kill_point(WRITE_FILE_RENAME);
    // rename 后旧句柄指向被替换的 inode，必须失效。
    self.forget_fd(&path);
    self.mark_dirty(tbl, path);
    // 新文件名首次出现的目录项同步交给下一次 sync_data。
    self.dirty_dir.borrow_mut().insert(tbl.clone());
    Ok(())
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>> {
    match self.open_data_file(tbl, file_id).await? {
      Some(file) => file
        .metadata()
        .await
        .map(|meta| Some(meta.len()))
        .map_err(map_io_err),
      None => Ok(None),
    }
  }

  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.list_file_id_by_kind(tbl, FileKind::Data).await
  }

  async fn list_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    self.list_file_variants_by_kind(tbl, FileKind::Data).await
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.remove_file_by_kind(tbl, file_id, FileKind::Data).await
  }

  async fn remove_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .remove_file_variant_by_kind(tbl, file_id, FileKind::Data, branch, term)
      .await
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.list_file_id_by_kind(tbl, FileKind::Segment).await
  }

  async fn list_segment_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    self
      .list_file_variants_by_kind(tbl, FileKind::Segment)
      .await
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self
      .remove_file_by_kind(tbl, file_id, FileKind::Segment)
      .await
  }

  async fn remove_segment_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .remove_file_variant_by_kind(tbl, file_id, FileKind::Segment, branch, term)
      .await
  }

  /// 空分区目录移除：目录里还有任何条目（data / segment / manifest / 临时
  /// 文件 / 未知残留）就原样保留，只 `remove_dir` 不递归删除。
  async fn try_remove_empty_partition_dir(&self, tbl: &TableIdent) -> Result<bool> {
    let dir = self.part_dir(tbl);
    let removed = spawn_dir_io({
      let dir = dir.clone();
      move || -> Result<bool> {
        // 注入一次目录收尾瞬时失败：等价 read_dir EMFILE / EACCES，或
        // read_dir 见空后 rmdir 的 ENOTEMPTY 竞态 / EACCES（io-31）。放在
        // 入口而非 rmdir 之前，是因为带 manifest 的活跃分区 rmdir 永远到
        // 不了（read_dir 先见条目），维护路径可达的失败类只有 read_dir 侧；
        // 两类失败在 GC 收尾的 warning-only 口径下等价。
        fkv_failpoint::fail_point!(TRY_REMOVE_EMPTY_DIR_FAIL);
        let mut entry_li = match read_dir(&dir) {
          Ok(entry_li) => entry_li,
          Err(err) if err.kind() == ErrorKind::NotFound => return Ok(false),
          Err(_) => return Err(KvError::IoFail),
        };
        // 只要还有条目（含读不出名的残留）就不删，保守优先。
        if entry_li.next().is_some() {
          return Ok(false);
        }
        match remove_dir(&dir) {
          Ok(()) => Ok(true),
          Err(err) if rmdir_keep_dir(&err) => Ok(false),
          Err(_) => Err(KvError::IoFail),
        }
      }
    })
    .await??;
    if removed {
      self.forget_table_state(tbl);
    }
    Ok(removed)
  }
}

/// remove_dir 失败是否按「目录仍在 / 非空」吞掉（返回未移除而非错误）：
/// `-ENOENT`（已被并发移除）与 `-ENOTEMPTY`（read_dir 见空到 remove_dir
/// 之间又有条目出现的 TOCTOU 竞态，等价于「目录非空」）都不算错误，对齐
/// C++ `IouringMgr::TryCleanupLocalPartitionDir` 对 `-ENOENT / -ENOTEMPTY`
/// 返回 `NoError` 的口径（`async_io_manager.cpp:1470-1476`）；其余错误
/// （EACCES / EIO 等）仍回 `Err`，由 GC 收尾告警吞掉（C++ 同分支也只
/// WARNING + `NoError`，`file_gc.cpp:364-371`）。
fn rmdir_keep_dir(err: &io::Error) -> bool {
  matches!(
    err.kind(),
    ErrorKind::NotFound | ErrorKind::DirectoryNotEmpty
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  /// rmdir 错误映射（io-31）：-ENOENT / -ENOTEMPTY 吞掉，其余 errno 仍回
  /// `Err(IoFail)`。用真实 errno 构造，保证 OS 错误确实落进对应 kind 分支。
  #[test]
  fn rmdir_keep_dir_only_for_notfound_and_notempty() {
    assert!(rmdir_keep_dir(&io::Error::from_raw_os_error(libc::ENOENT)));
    assert!(rmdir_keep_dir(&io::Error::from_raw_os_error(
      libc::ENOTEMPTY
    )));
    assert!(!rmdir_keep_dir(&io::Error::from_raw_os_error(libc::EACCES)));
    assert!(!rmdir_keep_dir(&io::Error::from_raw_os_error(libc::EIO)));
  }
}
