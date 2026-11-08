//! manifest / sidecar 文件读写与原子替换。

use std::{
  fs::read_dir,
  io::{self, ErrorKind},
  path::Path,
  sync::atomic::Ordering,
};

use compio::{
  buf::BufResult,
  fs::{File, OpenOptions, remove_file, rename},
  io::AsyncWriteAt,
};
use fkv_error::{KvError, Result};
use fkv_failpoint::points::{
  APPEND_SYNC, APPEND_SYNC_FAIL, APPEND_WRITE, ATOMIC_RENAME, ATOMIC_SYNC, ATOMIC_SYNC_DIR,
};
use fkv_io::ManifestIo;
use fkv_types::{FILE_NAME_MANIFEST, PAGE_ALIGN, TMP_SUFFIX, TableIdent, parse_manifest_file_name};

use crate::{CompioStore, DirectBuf, WriteChunk, fd_cache::spawn_dir_io, map_io_err, tmp_path};

impl CompioStore {
  /// 写整个文件并校验写入长度。
  pub(crate) async fn write_whole(
    &self,
    file: &mut File,
    bytes: &[u8],
    direct: bool,
  ) -> Result<()> {
    if direct && !bytes.len().is_multiple_of(PAGE_ALIGN) {
      return Err(KvError::InvalidArgs);
    }
    let chunk = if direct {
      WriteChunk::Direct(DirectBuf::from_slice(
        self.take_direct_buf(bytes.len()),
        bytes,
      ))
    } else {
      let mut buf = self.take_write_buf(bytes.len());
      debug_assert_eq!(buf.len(), bytes.len());
      buf.copy_from_slice(bytes);
      WriteChunk::Plain(buf)
    };
    let write_len = self.write_chunk(file, chunk, 0).await.map_err(map_io_err)?;
    if write_len != bytes.len() {
      return Err(KvError::IoFail);
    }
    Ok(())
  }

  /// manifest 族文件在既有偏移后追加一条记录并 fdatasync。
  /// 偏移缓存 / FD 缓存 / 写缓冲与活跃 manifest 共用（io-05）。
  async fn append_file_at(&self, dir: &Path, path: &Path, record: &[u8]) -> Result<()> {
    self.manifest_append_num.fetch_add(1, Ordering::Relaxed);
    let mut file = self.open_write_file(path).await.map_err(map_io_err)?;
    // 偏移来自缓存；只有首次访问该文件才 fstat（io-05）。
    let cached_pos = self.manifest_offset.borrow().get(path).copied();
    let pos = match cached_pos {
      Some(pos) => pos,
      None => file.metadata().await.map_err(map_io_err)?.len(),
    };
    fkv_failpoint::fail_point!(APPEND_WRITE);
    // 测试注入：只写前 N 字节，走与真实短写完全相同的回滚分支。
    let write_len_limit = self
      .short_write_bytes
      .take()
      .map_or(record.len(), |len| len.min(record.len()));
    let mut buf = self.take_write_buf(write_len_limit);
    debug_assert_eq!(buf.len(), write_len_limit);
    buf.copy_from_slice(&record[..write_len_limit]);
    let BufResult(result, buf) = file.write_at(buf, pos).await;
    let write_len = result.map_err(map_io_err)?;
    self.put_write_buf(write_len_limit, buf);
    if write_len != record.len() {
      // 半写 record 必须截掉再报错：否则重试按 fstat 追加到撕裂前缀之后，
      // 盘面形成 valid/corrupt/valid，reopen 永远判 Corrupted。
      file.set_len(pos).await.map_err(map_io_err)?;
      return Err(KvError::IoFail);
    }
    self
      .manifest_offset
      .borrow_mut()
      .insert(path.to_path_buf(), pos + record.len() as u64);
    fkv_failpoint::kill_point(APPEND_WRITE);
    // 生产环境 no-op；测试用 `AppendManifest:SyncFail` 在真实 sync_data 之后
    // 注入失败，确保走下面的截断 / 偏移回滚分支（`AppendManifest:Sync` 留给
    // 掉电 kill 语义，位于 sync 之前）。
    self.manifest_sync_num.fetch_add(1, Ordering::Relaxed);
    let mut sync_result = file.sync_data().await;
    if fkv_failpoint::fail_point(APPEND_SYNC_FAIL).is_some() {
      // 注入必须落到下面的回滚分支，而不是 `fail_point!` 的提前 return。
      sync_result = Err(io::Error::other("injected manifest sync failure"));
    }
    if let Err(err) = sync_result {
      // fsync 失败：record 可能已落入页缓存 / 盘面，但事务未提交。调用方会
      // 回滚本批 mapping 并复用 FilePageId，因此必须把文件截回 pos 且把偏移
      // 缓存退回 pos；否则下一次 append 会写在 stale record 之后，reopen 会
      // replay 这条未提交记录，导致已确认数据丢失。
      self
        .manifest_offset
        .borrow_mut()
        .insert(path.to_path_buf(), pos);
      match file.set_len(pos).await {
        Ok(()) => {
          // 截断也要落盘：否则“fsync 失败 + 立即掉电”会让未提交 record 复活。
          if let Err(trunc_sync_err) = file.sync_data().await {
            log::warn!(
              target: "fkv_io_compio",
              "manifest 回滚截断 fsync 失败 path={} pos={} err={}",
              path.display(),
              pos,
              trunc_sync_err
            );
          }
        }
        Err(trunc_err) => log::warn!(
          target: "fkv_io_compio",
          "manifest append 回滚截断失败 path={} pos={} err={}",
          path.display(),
          pos,
          trunc_err
        ),
      }
      return Err(map_io_err(err));
    }
    fkv_failpoint::kill_point(APPEND_SYNC);
    // 首次创建文件后落盘一次父目录项；append 本身不改目录项。文件内容已经
    // fsync 成功，目录项失败只影响掉电后的可见性，不能让调用方回滚内存状态
    // （那样会与盘上已提交的 record 分叉）。
    if let Err(err) = self.sync_manifest_dir_once(dir).await {
      log::warn!(
        target: "fkv_io_compio",
        "manifest 目录 fsync 失败（记录已写入） dir={} err={}",
        dir.display(),
        err
      );
    }
    Ok(())
  }

  /// 原子替换：写 tmp → fdatasync → rename → 失效句柄 → fsync 父目录。
  async fn atomic_write_file(&self, dir: &Path, path: &Path, record: &[u8]) -> Result<()> {
    let tmp = tmp_path(path);
    let mut file = File::create(&tmp).await.map_err(map_io_err)?;
    self.write_whole(&mut file, record, false).await?;
    file.sync_data().await.map_err(map_io_err)?;
    // 每个名字只对应**一个**两阶段窗口（gap-12），这样按名字枚举的 fuzz 驱动
    // 才有区分度：
    // - `AtomicWriteFile:Sync`：tmp 内容已耐久、尚未改名（旧 manifest 仍是权威）；
    // - `AtomicWriteFile:Rename`：已改名、父目录项尚未 fsync（新 snapshot 已可见，
    //   但掉电后可能回退）；
    // - `AtomicWriteFile:SyncDir`：目录项已 fsync，提交完成。
    fkv_failpoint::kill_point(ATOMIC_SYNC);
    fkv_failpoint::fail_point!(ATOMIC_SYNC);
    rename(&tmp, path).await.map_err(map_io_err)?;
    fkv_failpoint::kill_point(ATOMIC_RENAME);
    fkv_failpoint::fail_point!(ATOMIC_RENAME);
    // rename 后旧句柄指向被替换的 inode，必须失效；目录项也要落盘。
    self.forget_fd(path);
    // rename 已生效：新 manifest 已经是命名空间里的权威状态。此时目录 fsync
    // 失败若返回 Err，调用方会回滚内存 root/mapping，但盘上已是新快照，下一次
    // append 会把旧 root 写成新 snapshot 的 log，reopen 后树不可读。这里按
    // 「已提交、目录项 durability 降级」处理：告警但返回成功。
    if let Err(err) = self.sync_dir(dir).await {
      log::warn!(
        target: "fkv_io_compio",
        "manifest rename 后目录 fsync 失败（新快照已生效） dir={} err={}",
        dir.display(),
        err
      );
    }
    fkv_failpoint::kill_point(ATOMIC_SYNC_DIR);
    Ok(())
  }
}

impl ManifestIo for CompioStore {
  async fn reset_manifest(&self, tbl: &TableIdent, size: u64) -> Result<()> {
    let path = self.manifest_path(tbl);
    self.manifest_offset.borrow_mut().insert(path.clone(), size);
    let file = self.open_write_file(&path).await.map_err(map_io_err)?;
    // 截掉撕裂尾部，后续 append 从有效边界开始。
    //
    // 注意：`set_len` 本身**不 fsync**（对齐 C++ 的 `Reopen` 路径）。即"截断未持久"
    // 是一个真实窗口：掉电后撕裂尾可能复活，reopen 必须幂等地再切分并再截断一次
    // （gap-12）。此处保持不 fsync 是为了不在 reopen 热路径上多一次盘同步。
    file.set_len(size).await.map_err(map_io_err)?;
    Ok(())
  }

  async fn reset_manifest_file(&self, tbl: &TableIdent, name: &str, size: u64) -> Result<()> {
    let path = self.part_dir(tbl).join(name);
    self.manifest_offset.borrow_mut().insert(path.clone(), size);
    let file = self.open_write_file(&path).await.map_err(map_io_err)?;
    // branch manifest 与活跃 manifest 同契约：丢弃 torn / 损坏尾部。
    // 同 `reset_manifest`：截断不 fsync，掉电后撕裂尾可能复活（gap-12）。
    file.set_len(size).await.map_err(map_io_err)?;
    Ok(())
  }

  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.ensure_dir(tbl).await?;
    let dir = self.part_dir(tbl);
    self
      .append_file_at(&dir, &self.manifest_path(tbl), record)
      .await
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.ensure_dir(tbl).await?;
    self
      .atomic_write_file(&self.part_dir(tbl), &self.manifest_path(tbl), record)
      .await
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    self.read_manifest_path(&self.manifest_path(tbl)).await
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
    let path = self.manifest_path(tbl);
    match remove_file(&path).await {
      Ok(()) => {
        self.forget_fd(&path);
        Ok(())
      }
      Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
      Err(_) => Err(KvError::IoFail),
    }
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self.ensure_dir(tbl).await?;
    let dir = self.part_dir(tbl);
    self.atomic_write_file(&dir, &dir.join(name), record).await
  }

  async fn append_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self.ensure_dir(tbl).await?;
    let dir = self.part_dir(tbl);
    self.append_file_at(&dir, &dir.join(name), record).await
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    let dir = self.part_dir(tbl);
    // read_dir 是阻塞 syscall：整段扫描放在阻塞线程池。
    spawn_dir_io(move || -> Result<Vec<String>> {
      let entry_li = match read_dir(&dir) {
        Ok(entry_li) => entry_li,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(KvError::IoFail),
      };
      let mut name_li = Vec::new();
      for entry in entry_li.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
          continue;
        };
        if !name.starts_with(FILE_NAME_MANIFEST) || name.ends_with(TMP_SUFFIX) {
          continue;
        }
        if parse_manifest_file_name(name).is_some() {
          name_li.push(name.to_owned());
        }
      }
      name_li.sort();
      Ok(name_li)
    })
    .await?
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    self
      .read_manifest_path(&self.part_dir(tbl).join(name))
      .await
  }

  /// Compio 无远端：晋升种子与具名写等价（落盘即发布）。
  async fn seed_local_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
    bytes: &[u8],
  ) -> Result<()> {
    self.ensure_dir(tbl).await?;
    let dir = self.part_dir(tbl);
    self.atomic_write_file(&dir, &dir.join(name), bytes).await
  }

  /// 只列非 manifest 的旁路文件（如云端 cache 的 segment 完成标记）。
  ///
  /// 与 [`Self::list_manifest_file_name_li`] 互补：manifest 名单由命名规则过滤，
  /// 这里返回其余普通文件；临时文件（`.tmp`）不暴露。
  async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    let dir = self.part_dir(tbl);
    spawn_dir_io(move || -> Result<Vec<String>> {
      let entry_li = match read_dir(&dir) {
        Ok(entry_li) => entry_li,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(KvError::IoFail),
      };
      let mut name_li = Vec::new();
      for entry in entry_li.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
          continue;
        };
        if name.starts_with(FILE_NAME_MANIFEST) || name.ends_with(TMP_SUFFIX) {
          continue;
        }
        name_li.push(name.to_owned());
      }
      name_li.sort_unstable();
      name_li.dedup();
      Ok(name_li)
    })
    .await?
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.remove_part_file(tbl, name).await
  }

  /// 旁路文件与 manifest 同目录存储，删除语义一致（不存在视为成功）。
  async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.remove_part_file(tbl, name).await
  }
}

impl CompioStore {
  /// 删除分区目录内的一个普通文件（manifest 与旁路文件共用）。
  async fn remove_part_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    let path = self.part_dir(tbl).join(name);
    match remove_file(&path).await {
      Ok(()) => {
        self.forget_fd(&path);
        Ok(())
      }
      Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
      Err(_) => Err(KvError::IoFail),
    }
  }
}

impl CompioStore {
  /// 按路径读整个文件；不存在返回 `None`。
  ///
  /// 不走 `FdCache`：standby 同步与 `switch_manifest` 会用 rename 替换整个文件，
  /// 缓存里的旧 fd 仍指向被替换的 inode（读旧内容、后续追加写进已删除文件）。
  /// 读前顺手清掉该路径的缓存 fd。
  async fn read_manifest_path(&self, path: &Path) -> Result<Option<Vec<u8>>> {
    self.forget_fd(path);
    let mut opts = OpenOptions::new();
    opts.read(true);
    let file = match opts.open(path).await {
      Ok(file) => file,
      Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
      Err(_) => return Err(KvError::IoFail),
    };
    self.read_whole(&file, false).await.map(Some)
  }
}
