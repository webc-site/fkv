//! 分区目录生命周期：建目录 / fsync、预加载、按类枚举与删除、table 状态清理。

use std::{fs::create_dir_all, io::ErrorKind, path::Path, sync::atomic::Ordering};

use compio::fs::{OpenOptions, remove_file};
use fkv_error::{KvError, Result};
use fkv_failpoint::points::SYNC_DIR;
use fkv_io::FileVariant;
use fkv_types::{FileId, TableIdent};

use crate::{
  CompioStore, FileKind,
  fd_cache::{remove_stale_tmp, spawn_dir_io},
  map_io_err,
};

impl CompioStore {
  /// 列出某类文件在目录中出现的全部 file id（当前与其它 identity）。
  pub(crate) async fn list_file_id_by_kind(
    &self,
    tbl: &TableIdent,
    kind: FileKind,
  ) -> Result<Vec<FileId>> {
    self.refresh_dir_index_async(tbl, true).await;
    let mut file_id_li: Vec<FileId> =
      self
        .dir_index_map
        .borrow()
        .get(tbl)
        .map_or_else(Vec::new, |index| {
          // 当前 (branch, term) 与其它 term 的文件都要列出（GC 需要全量）。
          index
            .primary(kind)
            .keys()
            .chain(index.other(kind).keys())
            .copied()
            .collect()
        });
    file_id_li.sort_unstable();
    file_id_li.dedup();
    Ok(file_id_li)
  }

  /// 列出某类文件的全部物理变体（rev-39）：identity 命中的 primary 条目带当前
  /// `(branch, term)`，other 条目带文件名解析出的 `(branch, term)`。
  pub(crate) async fn list_file_variants_by_kind(
    &self,
    tbl: &TableIdent,
    kind: FileKind,
  ) -> Result<Vec<FileVariant>> {
    self.refresh_dir_index_async(tbl, true).await;
    let (identity_branch, identity_term) = {
      let identity = self.identity.borrow();
      (identity.branch.clone(), identity.term)
    };
    let mut variant_li: Vec<FileVariant> =
      self
        .dir_index_map
        .borrow()
        .get(tbl)
        .map_or_else(Vec::new, |index| {
          let primary_li = index.primary(kind).keys().map(|file_id| FileVariant {
            file_id: *file_id,
            identity: Some((identity_branch.as_str().into(), identity_term)),
          });
          let other_li = index.other(kind).iter().flat_map(|(file_id, li)| {
            li.iter().map(|variant| FileVariant {
              file_id: *file_id,
              identity: Some((variant.branch.clone(), variant.term)),
            })
          });
          primary_li.chain(other_li).collect()
        });
    variant_li.sort_unstable();
    variant_li.dedup();
    Ok(variant_li)
  }

  /// 删除某类 file id 的全部 identity 变体，并失效句柄 / 脏标记 / 路径缓存。
  pub(crate) async fn remove_file_by_kind(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    kind: FileKind,
  ) -> Result<()> {
    self.ensure_dir_index(tbl).await;
    let mut result = Ok(());
    for path in self.variant_path_li(tbl, file_id, kind) {
      if let Err(err) = remove_file(&path).await
        && err.kind() != ErrorKind::NotFound
      {
        result = Err(KvError::IoFail);
      }
      self.forget_fd(&path);
      self.forget_dirty(&path);
    }
    self.forget_path(tbl, file_id, kind);
    self.dirty_dir.borrow_mut().insert(tbl.clone());
    result
  }

  /// 只删除一个 `(file_id, branch, term)` 物理变体（rev-39）。
  ///
  /// 同 id 的其它 `(branch, term)` 变体不动；索引只清对应条目，目录脏标记
  /// 照常登记（删除后的目录项需要随 durability barrier 落盘）。
  pub(crate) async fn remove_file_variant_by_kind(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    kind: FileKind,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self.ensure_dir_index(tbl).await;
    let mut result = Ok(());
    for path in self.variant_paths_of(tbl, file_id, kind, branch, term) {
      if let Err(err) = remove_file(&path).await
        && err.kind() != ErrorKind::NotFound
      {
        result = Err(KvError::IoFail);
      }
      self.forget_fd(&path);
      self.forget_dirty(&path);
    }
    self.forget_variant(tbl, file_id, kind, branch, term);
    self.dirty_dir.borrow_mut().insert(tbl.clone());
    result
  }

  /// 预加载单个 table 目录：一次扫描建立 data / segment 的 file id -> path 索引。
  /// 同步扫描，只适合 attach / reopen 阶段；运行期请走异步路径。
  /// 返回被索引的文件数。
  pub fn preload(&self, tbl: &TableIdent) -> usize {
    // attach / reopen 阶段没有并发写：清掉上次崩溃残留的 `*.tmp` 半成品，
    // 避免磁盘长期泄漏（正常写入的 tmp 会紧接 rename 消失）。
    remove_stale_tmp(&self.part_dir(tbl));
    self.refresh_dir_index(tbl, true);
    self
      .dir_index_map
      .borrow()
      .get(tbl)
      .map_or(0, |index| index.data.len() + index.segment.len())
  }

  /// 确保分区目录存在：`create_dir_all` 走阻塞线程池，成功后记住不再重建。
  ///
  /// 新建目录必须 fsync 父目录（对齐 C++ `mkdirat` 后 fdatasync 目录）：否则
  /// 断电后 root 里没有 `demo.1` 这个 entry，目录内的 manifest / data 即使已
  /// fsync 也随目录一起消失。root 本身也是懒创建时，root 的父目录一并同步。
  pub(crate) async fn ensure_dir(&self, tbl: &TableIdent) -> Result<()> {
    let dir = self.part_dir(tbl);
    if self.dir_ready.borrow().contains(&dir) {
      return Ok(());
    }
    let target = dir.clone();
    let root = self.root.clone();
    let created = spawn_dir_io(move || {
      // 目录项同步的范围取决于 root 是否本就存在：root 由本次 create 建立时，
      // 它的 entry 记在更上一层目录里。
      let root_existed = root.is_dir();
      create_dir_all(&target).ok().map(|()| root_existed)
    })
    .await?;
    let Some(root_existed) = created else {
      return Err(KvError::IoFail);
    };
    self.sync_dir_entry(&self.root).await?;
    if !root_existed
      && let Some(parent) = self
        .root
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
      self.sync_dir_entry(parent).await?;
    }
    self.dir_ready.borrow_mut().insert(dir);
    Ok(())
  }

  /// fsync 一个目录 fd。调用方负责决定是否需要记入 `manifest_dir_synced`。
  ///
  /// 目录句柄不进 `fd_cache`：FD 缓存只统计 data / segment / manifest 文件，
  /// 目录句柄一次 fsync 即弃，缓存它只会挤占 `fd_limit` 并污染 `fd_open_num`。
  pub(crate) async fn sync_dir_entry(&self, dir: &Path) -> Result<()> {
    fkv_failpoint::fail_point!(SYNC_DIR);
    // 目录项未落盘的窗口：`sync_data` 的 dirty 目录分支、`ensure_dir`、
    // `append_file_at` 都走这里，因此「文件已耐久、目录项未持久」可被注入（gap-12）。
    fkv_failpoint::kill_point(SYNC_DIR);
    let mut opts = OpenOptions::new();
    opts.read(true);
    let file = opts.open(dir).await.map_err(map_io_err)?;
    file.sync_data().await.map_err(map_io_err)?;
    self.dir_fsync_num.fetch_add(1, Ordering::Relaxed);
    Ok(())
  }

  /// fsync 目录项：打开目录 fd 后 `sync_data`（Linux fdatasync / macOS fsync）。
  ///
  /// 用于 manifest 目录：首次创建 manifest 后落盘一次父目录项。
  pub(crate) async fn sync_dir(&self, dir: &Path) -> Result<()> {
    self.sync_dir_entry(dir).await?;
    self
      .manifest_dir_synced
      .borrow_mut()
      .insert(dir.to_path_buf());
    Ok(())
  }

  /// manifest 目录项持久化：首次创建 manifest 文件后 fsync 一次父目录。
  pub(crate) async fn sync_manifest_dir_once(&self, dir: &Path) -> Result<()> {
    if self.manifest_dir_synced.borrow().contains(dir) {
      return Ok(());
    }
    self.sync_dir(dir).await
  }

  /// 目录被外部同步工具（standby / rsync）改动后主动失效；下一次访问会重新扫描。
  pub fn invalidate_path_cache(&self) {
    self.clear_caches();
  }

  /// 清空路径索引 / 目录就绪标记 / FD 缓存。
  pub(crate) fn clear_caches(&self) {
    self.dir_index_map.borrow_mut().clear();
    self.dir_ready.borrow_mut().clear();
    self.fd_cache.borrow_mut().clear();
  }

  /// 清掉某表在 IO 层的全部缓存状态：FD / 路径索引 / 目录就绪标记 / 脏集合 /
  /// manifest 追加偏移 / segment 预分配标记 / 目录 fsync 标记。
  ///
  /// evict（detach / LRU 驱逐）与空目录删除共用同一份清理集合，避免两处漂移。
  /// 写路径在落 manifest 前已 `sync_data`，这里丢弃的只是未提交页的脏标记。
  pub(crate) fn forget_table_state(&self, tbl: &TableIdent) {
    let dir = self.part_dir(tbl);
    self.fd_cache.borrow_mut().forget_under(&dir);
    self.dir_index_map.borrow_mut().remove(tbl);
    self.dir_ready.borrow_mut().remove(&dir);
    self.dirty.borrow_mut().remove(tbl);
    self.dirty_dir.borrow_mut().remove(tbl);
    // 目录可能被外部工具（standby / 运维）整体重建：路径前缀下的偏移与
    // 预分配标记一并作废，后续 append 重新走 fstat。
    self
      .manifest_offset
      .borrow_mut()
      .retain(|path, _| !path.starts_with(&dir));
    self
      .segment_preallocated
      .borrow_mut()
      .retain(|path| !path.starts_with(&dir));
    self.manifest_dir_synced.borrow_mut().remove(&dir);
  }
}

impl CompioStore {
  pub fn path_scan_num(&self) -> (u64, u64) {
    (
      self.data_scan_num.load(Ordering::Relaxed),
      self.segment_scan_num.load(Ordering::Relaxed),
    )
  }
}
