//! 文件句柄打开 / 失效与 segment 预分配。

use std::{
  io::{self, ErrorKind},
  path::Path,
  sync::atomic::Ordering,
};

use compio::fs::{File, OpenOptions};
use fkv_error::{KvError, Result};
use fkv_types::{FILE_NAME_DATA, FILE_NAME_SEGMENT, FileId, FilePageId, PAGE_ALIGN, TableIdent};

use crate::{Access, CompioStore, FileKind, apply_direct_flag, map_io_err};

/// 打开句柄的用途：O_DIRECT 回退重试时据此重建同语义的 `OpenOptions`。
#[derive(Clone, Copy)]
enum OpenMode {
  Read,
  Write,
  Sync,
}

impl OpenMode {
  fn options(self) -> OpenOptions {
    let mut opts = OpenOptions::new();
    match self {
      Self::Read => {
        opts.read(true);
      }
      Self::Write => {
        opts.read(true).write(true).create(true).truncate(false);
      }
      Self::Sync => {
        opts.read(true).write(true).truncate(false);
      }
    }
    opts
  }
}

/// 文件系统不支持 O_DIRECT：`EINVAL`（tmpfs / 旧 overlay 等）或
/// `EOPNOTSUPP`（网络 / 特殊文件系统）。其它错误（`ENOENT` / `EMFILE` 等）
/// 与 O_DIRECT 无关，不能触发回退。
fn direct_unsupported(err: &io::Error) -> bool {
  matches!(
    err.raw_os_error(),
    Some(code) if code == libc::EINVAL || code == libc::EOPNOTSUPP
  )
}

impl CompioStore {
  /// 该路径属于哪类 O_DIRECT 文件；该类开关关闭或对齐不满足时返回 `None`。
  ///
  /// data 文件由 `page_size` 对齐保证；segment 文件必须 `segment_size`
  /// 为页大小整数倍（否则整段定长读写无法对齐，保持 buffered）。
  fn direct_kind(&self, path: &Path) -> Option<FileKind> {
    let name = path.file_name().and_then(|name| name.to_str())?;
    if name.starts_with(FILE_NAME_DATA) {
      self.direct_io.get().then_some(FileKind::Data)
    } else if name.starts_with(FILE_NAME_SEGMENT) {
      self.direct_segment().then_some(FileKind::Segment)
    } else {
      None
    }
  }

  /// segment 是否可用 O_DIRECT：`segment_size` 必须是页大小整数倍。
  pub(crate) fn direct_segment(&self) -> bool {
    self.direct_seg.get() && self.segment_size.is_multiple_of(PAGE_ALIGN)
  }

  /// 按用途打开句柄；O_DIRECT 不被文件系统支持时按路径类别回退 buffered。
  async fn open_path(&self, path: &Path, mode: OpenMode) -> io::Result<File> {
    let Some(kind) = self.direct_kind(path) else {
      return mode.options().open(path).await;
    };
    // 测试注入：模拟文件系统在 O_DIRECT open 上返回 EINVAL。
    if self.direct_open_err.get() & kind.bit() != 0
      || self.direct_open_err_path.borrow().as_deref() == Some(path)
    {
      self.fallback_direct(kind, path, &io::Error::from_raw_os_error(libc::EINVAL));
      return mode.options().open(path).await;
    }
    let mut opts = mode.options();
    apply_direct_flag(&mut opts, true);
    match opts.open(path).await {
      Ok(file) => {
        // io-21：open 在飞期间，同类别另一条 open 可能已 EINVAL 并关闭开关。
        // 复查开关；已关闭则丢弃这个 O_DIRECT 句柄、不置位 `direct_opened`，
        // 按 buffered 重开——否则「开关 false + 缓存 direct 句柄」会让后续
        // 非对齐缓冲读写持续 EINVAL。复查与 fd 缓存插入之间没有 await。
        if self.direct_kind(path) != Some(kind) {
          drop(file);
          self.direct_discard_num.fetch_add(1, Ordering::Relaxed);
          return mode.options().open(path).await;
        }
        self
          .direct_opened
          .set(self.direct_opened.get() | kind.bit());
        Ok(file)
      }
      Err(err) if direct_unsupported(&err) => {
        self.fallback_direct(kind, path, &err);
        mode.options().open(path).await
      }
      Err(err) => Err(err),
    }
  }

  /// 处理一次 O_DIRECT open 失败（io-18）。
  ///
  /// 该类还没有任何 direct 句柄时关闭开关，后续 open 直接 buffered；已有
  /// direct 句柄时只让本次 open buffered——已打开的 O_DIRECT fd 仍按对齐缓冲
  /// 读写，若此时把开关置 false，后续用非对齐缓冲写对齐 fd 会 `EINVAL`。
  fn fallback_direct(&self, kind: FileKind, path: &Path, err: &io::Error) {
    let bit = kind.bit();
    self.direct_fallback_num.fetch_add(1, Ordering::Relaxed);
    let closed = self.direct_opened.get() & bit == 0;
    if closed {
      match kind {
        FileKind::Data => self.direct_io.set(false),
        FileKind::Segment => self.direct_seg.set(false),
      }
    }
    if self.direct_fallback_warned.get() & bit == 0 {
      self
        .direct_fallback_warned
        .set(self.direct_fallback_warned.get() | bit);
      log::warn!(
        target: "fkv_io_compio",
        "O_DIRECT 不可用，按路径类别回退 buffered kind={:?} path={} error={} scope={}",
        kind,
        path.display(),
        err,
        if closed { "category" } else { "this-open" }
      );
    }
  }
}

impl CompioStore {
  /// 打开只读句柄；命中缓存时零系统调用返回（对齐 C++ `OpenOrCreateFD`）。
  ///
  /// 缓存失效依赖显式路径：`remove_file`/`remove_segment_file`/`switch_manifest`/
  /// `write_manifest_file`/`evict_table`/`invalidate_path_cache` 都会 `forget_fd`。
  /// 不再在命中路径做 `stat` 校验 inode——那会把每次页读变成 `statx`+`pread`
  /// 两次串行系统调用（io-01）。
  pub(crate) async fn open_read_file(&self, path: &Path) -> io::Result<File> {
    if let Some(file) = self.fd_cache.borrow_mut().get_read(path) {
      return Ok(file);
    }
    let file = self.open_path(path, OpenMode::Read).await?;
    self.fd_open_num.fetch_add(1, Ordering::Relaxed);
    self
      .fd_cache
      .borrow_mut()
      .insert(path.to_path_buf(), Access::Read, file.clone());
    Ok(file)
  }

  /// 打开写句柄（带 create、不 truncate）；命中缓存直接复用。
  pub(crate) async fn open_write_file(&self, path: &Path) -> io::Result<File> {
    if let Some(file) = self.fd_cache.borrow_mut().get_write(path) {
      return Ok(file);
    }
    let file = self.open_path(path, OpenMode::Write).await?;
    self.fd_open_num.fetch_add(1, Ordering::Relaxed);
    self
      .fd_cache
      .borrow_mut()
      .insert(path.to_path_buf(), Access::Write, file.clone());
    Ok(file)
  }

  /// 失效一条路径的缓存句柄（删除 / 替换 / rename 后调用）。
  pub(crate) fn forget_fd(&self, path: &Path) {
    self.fd_cache.borrow_mut().remove(path);
    // 句柄失效后偏移也不再可信（文件可能被替换 / 删除）。
    self.manifest_offset.borrow_mut().remove(path);
    self.segment_preallocated.borrow_mut().remove(path);
  }

  /// 首次使用某个 segment 文件时预分配一次，后续段写入不再 `ftruncate`。
  pub(crate) async fn preallocate_segment_file(&self, path: &Path, file: &File) -> Result<()> {
    if self.segment_preallocated.borrow().contains(path) {
      return Ok(());
    }
    file
      .set_len(self.sequence_file_size()?)
      .await
      .map_err(map_io_err)?;
    self
      .segment_preallocated
      .borrow_mut()
      .insert(path.to_path_buf());
    Ok(())
  }

  /// sync 用句柄：优先复用缓存句柄；不 create，文件已删除时返回 `NotFound`。
  pub(crate) async fn open_for_sync(&self, path: &Path) -> io::Result<File> {
    if let Some(file) = self.fd_cache.borrow_mut().get_write(path) {
      return Ok(file);
    }
    let file = self.open_path(path, OpenMode::Sync).await?;
    self.fd_open_num.fetch_add(1, Ordering::Relaxed);
    Ok(file)
  }
}

impl CompioStore {
  pub(crate) async fn open_segment_file(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
  ) -> Result<File> {
    let path = self
      .cached_segment_path(tbl, segment_fp_id)
      .await
      .ok_or(KvError::ResourceMissing)?;
    match self.open_read_file(&path).await {
      Ok(file) => Ok(file),
      Err(err) if err.kind() == ErrorKind::NotFound => {
        self.forget_path(
          tbl,
          segment_fp_id >> self.segments_per_file_shift,
          FileKind::Segment,
        );
        let path = self
          .cached_segment_path(tbl, segment_fp_id)
          .await
          .ok_or(KvError::ResourceMissing)?;
        match self.open_read_file(&path).await {
          Ok(file) => Ok(file),
          // 重查后仍不存在：段确实缺失。
          Err(err) if err.kind() == ErrorKind::NotFound => Err(KvError::ResourceMissing),
          // 权限 / fd 耗尽 / EIO 等底层错误按 io 契约映射，便于上层区分
          // 可重试错误与真正的 cache miss。
          Err(err) => Err(map_io_err(err)),
        }
      }
      Err(err) => Err(map_io_err(err)),
    }
  }

  /// 按 file id 打开 data 文件；`NotFound` 时清路径缓存并重查一次目录。
  pub(crate) async fn open_data_file(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
  ) -> Result<Option<File>> {
    let file_page_id = file_id << self.pages_per_file_shift;
    let Some(path) = self.cached_data_path(tbl, file_page_id).await else {
      return Ok(None);
    };
    match self.open_read_file(&path).await {
      Ok(file) => Ok(Some(file)),
      Err(err) if err.kind() == ErrorKind::NotFound => {
        self.forget_path(tbl, file_id, FileKind::Data);
        let Some(path) = self.cached_data_path(tbl, file_page_id).await else {
          return Ok(None);
        };
        match self.open_read_file(&path).await {
          Ok(file) => Ok(Some(file)),
          Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
          Err(err) => Err(map_io_err(err)),
        }
      }
      Err(err) => Err(map_io_err(err)),
    }
  }
}
