//! Standby：Master 导出本地文件列表，Replica 通过远端 list + 整文件拉取追赶。
//!
//! 第一版同步流程：
//! 1. 列出远端 table 目录，挑最新 manifest；
//! 2. 先拉 data / segment 文件到 staging 再原子 rename；
//! 3. 最后拉 manifest 到 staging，校验完整 record 链后**按本地活跃名安装**
//!    （`manifest_main_0`，对齐 C++ `FinalizeRsync` 的 rename），清掉旧的无 tag
//!    main manifest，并 fsync 分区目录让目录项落盘。
//! 4. 同时安装远端已有的 main 归档 manifest（`manifest_main_<term>_<tag>`），
//!    保证副本上的 `ReopenArchive` 能按 tag 命中；也可用
//!    [`StandbyService::sync_table_archive`] 按 tag 定向同步。
//!
//! 这样任何时刻本地 manifest 都不会指向尚未下载的数据文件。

use core::{fmt, future::Future};
use std::{
  ffi::OsStr,
  fs, io,
  io::{Error, ErrorKind},
  path::{Path, PathBuf},
  time::UNIX_EPOCH,
};

use compio::runtime::Runtime;
use fkv_config::KvOption;
use fkv_error::{KvError, Result};
use fkv_manifest::{HEADER_BYTES, manifest_align, parse_header, validate_record};
use fkv_types::{
  DATA_FILE_PREFIX, MAIN_BRANCH_NAME, SEGMENT_FILE_PREFIX, StoreMode, TMP_SUFFIX, TableIdent,
  is_valid_branch_name, manifest_file_name, parse_manifest_file_name,
};
use futures::future::join_all;

pub mod rsync;

pub use rsync::{CommandRunner, RsyncShell, RsyncSource, StdCommandRunner};

pub const STAGING_DIR: &str = ".standby";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandbyRole {
  Master,
  Replica,
}

impl StandbyRole {
  pub fn from_mode(mode: StoreMode) -> Result<Self> {
    match mode {
      StoreMode::StandbyMaster => Ok(Self::Master),
      StoreMode::StandbyReplica => Ok(Self::Replica),
      StoreMode::Local | StoreMode::Cloud => Err(KvError::InvalidArgs),
    }
  }
}

#[derive(Debug, Clone)]
pub struct StandbyConfig {
  pub role: StandbyRole,
  pub local_root: PathBuf,
  pub master_addr: String,
  pub master_path_li: Vec<String>,
  pub max_concurrency: u16,
  /// replica 当前 process term；0 表示不做 fencing。远端 manifest term
  /// 大于该值时拒绝安装（对齐 C++ `RunPrepareManifestJob`）。
  pub target_term: u64,
}

impl StandbyConfig {
  pub fn from_option(option: &KvOption, local_root: impl Into<PathBuf>) -> Result<Self> {
    let config = Self {
      role: StandbyRole::from_mode(option.derive_mode())?,
      local_root: local_root.into(),
      master_addr: option.standby_master_addr.clone(),
      master_path_li: option.standby_master_store_paths.clone(),
      max_concurrency: option.standby_max_concurrency,
      target_term: 0,
    };
    config.validate()?;
    Ok(config)
  }

  pub fn validate(&self) -> Result<()> {
    if self.local_root.as_os_str().is_empty() || self.max_concurrency == 0 {
      return Err(KvError::InvalidArgs);
    }
    Ok(())
  }

  pub fn table_dir(&self, tbl: &TableIdent) -> PathBuf {
    table_dir(&self.local_root, tbl)
  }

  /// rsync 远端前缀：`""` 表示本机 rsync。
  ///
  /// 对齐 C++ `StandbyService::RemoteSpec`：`standby_master_addr` 为空或字面量
  /// `"local"` 都不加 `addr:` 前缀——单机 standby（C++ `tests/standby.cpp` 的形态）
  /// 就靠这个哨兵跑通，不需要 ssh。
  pub fn source_addr(&self) -> &str {
    if self.master_addr.is_empty() || self.master_addr.eq_ignore_ascii_case("local") {
      return "";
    }
    &self.master_addr
  }

  /// staging 目录：`<local_root>/.standby/<tbl>.<partition>`。
  pub fn staging_dir(&self, tbl: &TableIdent) -> PathBuf {
    self.local_root.join(STAGING_DIR).join(tbl.dir_name())
  }
}

/// 远端目录里的一个普通文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
  pub name: String,
  pub size: u64,
  pub modified: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncStat {
  pub data_fetched: usize,
  pub data_skipped: usize,
  pub manifest_installed: bool,
  /// 安装时清掉的旧 term 无 tag main manifest 条数（对齐 C++ `FinalizeRsync` 清理）。
  pub stale_manifest_removed: usize,
  /// 本轮安装的归档 manifest 条数（`manifest_<branch>_<term>_<tag>`）。
  pub archive_installed: usize,
  /// 本轮是否 fsync 了分区目录（目录项落盘）。
  pub dir_synced: bool,
}

/// 远端 list / 分区级同步 / 单文件 fetch 抽象；生产实现是 rsync / ssh，
/// 测试用本地目录或内存实现。
pub trait RemoteSource {
  fn list<'a>(&'a self, tbl: &'a TableIdent) -> impl Future<Output = Result<Vec<RemoteFile>>> + 'a;

  /// 把分区里所有与 `compare` 不同的 data / segment 文件落到 `dst`（staging）：
  /// 一轮一次分区级传输，不逐文件起进程。`compare` 通常是本地分区目录，
  /// 相同者不传（增量追赶）。
  fn sync_dir<'a>(
    &'a self,
    tbl: &'a TableIdent,
    dst: &'a Path,
    compare: &'a Path,
  ) -> impl Future<Output = Result<()>> + 'a;

  /// 取单个文件（manifest 专用）。
  fn fetch<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
    dst: &'a Path,
  ) -> impl Future<Output = Result<()>> + 'a;

  /// 复核本地 `local` 与远端同名文件内容一致（manifest 安装前的最终校验）。
  ///
  /// 仅比 `(name, size, mtime)` 无法识别同秒等长覆写；实现方可返回
  /// `Ok(false)` 让本轮重试。无法做内容级校验的来源默认返回 `Ok(true)`。
  fn verify_same<'a>(
    &'a self,
    _tbl: &'a TableIdent,
    _name: &'a str,
    _local: &'a Path,
  ) -> impl Future<Output = Result<bool>> + 'a {
    async move { Ok(true) }
  }

  /// 远端根下已有的表（`<tbl>.<partition>` 目录）。
  ///
  /// replica 靠它知道本轮要追赶哪些分区：C++ 侧由 master 广告分区列表
  /// （`list_standby_partition`），Rust 侧直接列 master 的 store 根。
  fn list_table_li(&self) -> impl Future<Output = Result<Vec<TableIdent>>> + '_;
}

/// 本地目录作为远端源：用于 Master 自测 / 单机 standby。
#[derive(Debug, Clone)]
pub struct LocalDirSource {
  root: PathBuf,
}

impl LocalDirSource {
  pub fn new(root: impl Into<PathBuf>) -> Self {
    Self { root: root.into() }
  }

  pub fn root(&self) -> &Path {
    &self.root
  }
}

impl RemoteSource for LocalDirSource {
  async fn list(&self, tbl: &TableIdent) -> Result<Vec<RemoteFile>> {
    let dir = table_dir(&self.root, tbl);
    block(move || list_local_file_li(&dir)).await?
  }

  async fn list_table_li(&self) -> Result<Vec<TableIdent>> {
    let root = self.root.clone();
    block(move || list_table_li(&root)).await?
  }

  async fn sync_dir(&self, tbl: &TableIdent, dst: &Path, compare: &Path) -> Result<()> {
    let src_dir = table_dir(&self.root, tbl);
    let dst = dst.to_path_buf();
    let compare = compare.to_path_buf();
    block(move || copy_changed_data_file(&src_dir, &dst, &compare)).await?
  }

  async fn fetch(&self, tbl: &TableIdent, name: &str, dst: &Path) -> Result<()> {
    if !is_safe_file_name(name) {
      return Err(KvError::InvalidArgs);
    }
    let src = table_dir(&self.root, tbl).join(name);
    let dst = dst.to_path_buf();
    block(move || match fs::copy(&src, &dst) {
      Ok(_) => Ok(()),
      Err(err) if err.kind() == ErrorKind::NotFound => Err(KvError::ResourceMissing),
      Err(_) => Err(KvError::IoFail),
    })
    .await?
  }

  async fn verify_same(&self, tbl: &TableIdent, name: &str, local: &Path) -> Result<bool> {
    if !is_safe_file_name(name) {
      return Err(KvError::InvalidArgs);
    }
    let src = table_dir(&self.root, tbl).join(name);
    let local = local.to_path_buf();
    block(move || -> Result<bool> {
      let source = fs::read(&src).map_err(map_io)?;
      let fetched = fs::read(&local).map_err(map_io)?;
      Ok(source == fetched)
    })
    .await?
  }
}

pub struct StandbyService<S> {
  config: StandbyConfig,
  source: S,
}

/// 来源（rsync 子进程）不实现 `Debug`，配置里只需看 `StandbyConfig`。
impl<S> fmt::Debug for StandbyService<S> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("StandbyService")
      .field("config", &self.config)
      .finish_non_exhaustive()
  }
}

/// 生产形态的 replica 服务：来源是 rsync/ssh 子进程。
///
/// `StandbyConfig::source_addr()` 为空时走本机 rsync（单机 standby），否则走
/// `rsync`/`ssh <addr>`。
pub type RsyncStandbyService = StandbyService<RsyncSource<StdCommandRunner>>;

impl<S: RemoteSource> StandbyService<S> {
  pub fn new(config: StandbyConfig, source: S) -> Result<Self> {
    config.validate()?;
    Ok(Self { config, source })
  }

  pub fn config(&self) -> &StandbyConfig {
    &self.config
  }

  pub fn role(&self) -> StandbyRole {
    self.config.role
  }

  pub async fn list_remote(&self, tbl: &TableIdent) -> Result<Vec<RemoteFile>> {
    self.source.list(tbl).await
  }

  /// 列出远端 master 上已有的分区（`<tbl>.<partition>` 目录）。
  ///
  /// 对齐 C++ `StandbyService::ListRemotePartitions` / `RequestType::ListStandbyPartition`：
  /// 这是观察接口（master 侧的分区广告），不是 replica 追赶的驱动；
  /// 追赶仍走 [`Self::catch_up`] / [`Self::sync_table`]。
  ///
  /// 多根 master 的结果按 `(table_name, partition_id)` 排序去重（由
  /// `RemoteSource::list_table_li` 保证），单个空根不使整轮失败。
  pub async fn list_remote_partition_li(&self) -> Result<Vec<TableIdent>> {
    self.source.list_table_li().await
  }

  /// Master：列出本地 table 目录，供 rsync / standby client 拉取。
  pub fn export_table(&self, tbl: &TableIdent) -> Result<Vec<RemoteFile>> {
    if self.config.role != StandbyRole::Master {
      return Err(KvError::InvalidArgs);
    }
    list_local_file_li(&self.config.table_dir(tbl))
  }

  /// Replica：追赶一次某个 table。data / segment 先落 staging 再原子 rename，
  /// manifest 最后安装；任何时刻本地 manifest 都不会指向尚未下载的数据文件。
  ///
  /// 一轮只起 **一次** 分区级传输（rsync `--include=data_*` / `segment_*`
  /// `--exclude=*` `--compare-dest`，本地后端为等价拷贝），与 C++
  /// `StartRsyncJob` 每分区一次一致；逐文件 fetch 只留给 manifest。
  pub async fn sync_table(&self, tbl: &TableIdent) -> Result<SyncStat> {
    self
      .sync_table_with_term(tbl, self.config.target_term)
      .await
  }

  /// 同 [`Self::sync_table`]，但由调用方给出 process term 做 fencing。
  ///
  /// 对齐 C++ `ReopenTask` 用 `store_->Term()` 作为 `target_term` 的做法：副本自身
  /// 的 term 来自是否已 attach，不能固定在服务构造时。
  pub async fn sync_table_with_term(&self, tbl: &TableIdent, target_term: u64) -> Result<SyncStat> {
    self.sync_table_inner(tbl, target_term, None).await
  }

  /// 追赶指定归档快照（`manifest_<branch>_<term>_<tag>`）。
  ///
  /// `ReopenArchive` 带 tag 时使用：普通 [`Self::sync_table`] 也会顺手安装远端
  /// 已有的归档 manifest，但按 tag 定向同步能避免为无关归档付出列举 / 校验成本。
  pub async fn sync_table_archive(
    &self,
    tbl: &TableIdent,
    target_term: u64,
    branch: &str,
    tag: &str,
  ) -> Result<SyncStat> {
    if !is_safe_file_name(tag) || !is_valid_branch_name(branch) {
      return Err(KvError::InvalidArgs);
    }
    self
      .sync_table_inner(tbl, target_term, Some((branch, tag)))
      .await
  }

  async fn sync_table_inner(
    &self,
    tbl: &TableIdent,
    target_term: u64,
    archive: Option<(&str, &str)>,
  ) -> Result<SyncStat> {
    if self.config.role != StandbyRole::Replica {
      return Err(KvError::InvalidArgs);
    }
    // 表名会拼进目录名；含 `/`、`\`、`..` 会逃逸 local_root。staging 目录
    // 每轮删除重建，必须先挡住逃逸路径。
    if !is_safe_file_name(&tbl.dir_name()) {
      return Err(KvError::InvalidArgs);
    }
    let remote_li = self.list_remote(tbl).await?;
    let manifest = match archive {
      Some((branch, tag)) => select_archive_manifest(&remote_li, branch, tag),
      None => select_manifest(&remote_li, MAIN_BRANCH_NAME),
    }
    .ok_or(KvError::ResourceMissing)?
    .clone();
    check_manifest_term(&manifest.name, target_term)?;
    if !is_safe_file_name(&manifest.name) {
      return Err(KvError::Corrupted);
    }

    let local_dir = self.config.table_dir(tbl);
    let staging_dir = self.config.staging_dir(tbl);
    // staging 每轮清空：`--inplace` 只在空目录内原地写，崩溃残留不会带进本轮。
    remove_dir_async(staging_dir.clone()).await?;
    block({
      let local_dir = local_dir.clone();
      let staging_dir = staging_dir.clone();
      move || -> io::Result<()> {
        fs::create_dir_all(&local_dir)?;
        fs::create_dir_all(&staging_dir)
      }
    })
    .await?
    .map_err(map_io)?;

    if let Err(err) = self.source.sync_dir(tbl, &staging_dir, &local_dir).await {
      let _ = remove_dir_async(staging_dir.clone()).await;
      return Err(err);
    }

    let mut data_li: Vec<&RemoteFile> = remote_li
      .iter()
      .filter(|file| is_data_file(&file.name))
      .collect();
    data_li.sort_by(|left, right| left.name.cmp(&right.name));
    let mut stat = SyncStat::default();
    for file in data_li {
      if !is_safe_file_name(&file.name) {
        return Err(KvError::Corrupted);
      }
      // staging 里有没有就是传输端（rsync / 本地拷贝）的增量判定结果：
      // 缺席说明与本地已有副本相同，无需更新。
      let staged = staging_dir.join(&file.name);
      let Some(fetched_len) = local_len_async(staged.clone()).await? else {
        stat.data_skipped += 1;
        continue;
      };
      // data / segment 文件只追加：master 并发写入时可以比列表里的更大，但不能更小。
      if fetched_len < file.size {
        log::warn!(
          target: "fkv_standby",
          "sync retry: short staged file name={} remote_len={} staged_len={}",
          file.name,
          file.size,
          fetched_len
        );
        #[cfg(debug_assertions)]
        eprintln!(
          "fkv_standby short staged file {} remote={} staged={}",
          file.name, file.size, fetched_len
        );
        let _ = remove_dir_async(staging_dir.clone()).await;
        return Err(KvError::TryAgain);
      }
      rename_async(staged, local_dir.join(&file.name)).await?;
      stat.data_fetched += 1;
    }

    // `remote_li` 是同步开始前的快照；分区级传输期间 master 可能已经切换
    // manifest（等长内容也可能被覆写）。安装前必须重新列举，确认 manifest
    // 名 / size / modified 都还是同一版本，否则本轮可能装上一份引用尚未
    // 下载 data 的 manifest。下一轮重新做增量同步即可。
    let fresh_li = self.list_remote(tbl).await?;
    let fresh_ok = match archive {
      // 归档 manifest 创建后不再改写：只确认同名文件仍在且版本一致。
      Some(_) => fresh_li.iter().any(|file| {
        file.name == manifest.name
          && file.size == manifest.size
          && file.modified == manifest.modified
      }),
      None => select_manifest(&fresh_li, MAIN_BRANCH_NAME).is_some_and(|fresh| {
        fresh.name == manifest.name
          && fresh.size == manifest.size
          && fresh.modified == manifest.modified
      }),
    };
    if !fresh_ok {
      log::warn!(
        target: "fkv_standby",
        "sync retry: manifest changed name={} size={} modified={} fresh={fresh_li:?}",
        manifest.name,
        manifest.size,
        manifest.modified
      );
      #[cfg(debug_assertions)]
      eprintln!(
        "fkv_standby manifest changed {} size={} modified={} fresh={fresh_li:?}",
        manifest.name, manifest.size, manifest.modified
      );
      let _ = remove_dir_async(staging_dir.clone()).await;
      return Err(KvError::TryAgain);
    }

    let active_name = manifest_file_name(MAIN_BRANCH_NAME, 0);
    match archive {
      // 归档按**远端原名**安装（`manifest_<branch>_<term>_<tag>`）：归档是不可变
      // 历史快照，`reopen_archive(tag)` 按 tag 选中它；它不是本地活跃 manifest，
      // 所以不走「落活跃名 `manifest_main_0`」的规则（该规则只适用于活跃
      // manifest，避免 C++ master 的 `manifest_main_<term>` 打不开）。
      // POSIX rename 原子替换同名目标，不留半装窗口。
      Some(_) => {
        if let Err(err) = self
          .install_manifest_file(tbl, &manifest, &staging_dir, &local_dir, &manifest.name)
          .await
        {
          let _ = remove_dir_async(staging_dir.clone()).await;
          return Err(err);
        }
        stat.archive_installed = 1;
      }
      None => {
        // 归档先装、活跃 manifest 最后装：活跃 manifest 是本轮提交点。
        // 这样 ReopenArchive 在普通 reopen 同步之后也能找到归档，不需要
        // 调用方额外传 tag（shard 的 ReopenArchive 路径只做一次普通同步）。
        for file in fresh_li
          .iter()
          .filter(|file| is_main_archive_manifest(&file.name))
        {
          if !is_safe_file_name(&file.name) {
            return Err(KvError::Corrupted);
          }
          match self
            .install_manifest_file(tbl, file, &staging_dir, &local_dir, &file.name)
            .await
          {
            Ok(()) => stat.archive_installed += 1,
            // 归档是旁路状态：被剪枝 / 损坏 / 传输中变化都不应阻断活跃 manifest
            // 的提交；对应 tag 的 reopen 会按缺档重试。
            Err(err) => log::warn!(
              "standby 归档 manifest 安装跳过 name={} err={err}",
              file.name
            ),
          }
        }
        if let Err(err) = self
          .install_manifest_file(tbl, &manifest, &staging_dir, &local_dir, &active_name)
          .await
        {
          let _ = remove_dir_async(staging_dir.clone()).await;
          return Err(err);
        }
        // 清掉同分区里其它名字的无 tag main manifest（C++ 只留 target term 一份）。
        stat.stale_manifest_removed =
          remove_stale_main_manifest_li(local_dir.clone(), &active_name).await?;
      }
    }
    stat.manifest_installed = true;
    // 本轮 rename / unlink 只改了目录项，必须 fsync 目录才算落盘
    // （对齐 C++ `fdatasync(dir_fd)`；它按 mutated 决定，Rust 每轮都 rename 过，恒 mutated）。
    sync_dir_async(local_dir.clone()).await?;
    stat.dir_synced = true;
    let _ = remove_dir_async(staging_dir).await;
    Ok(stat)
  }

  /// 取单个 manifest 到 staging、校验长度 / 内容 / 记录链，再原子 rename 到本地名。
  async fn install_manifest_file(
    &self,
    tbl: &TableIdent,
    file: &RemoteFile,
    staging_dir: &Path,
    local_dir: &Path,
    dst_name: &str,
  ) -> Result<()> {
    let tmp = staging_dir.join(format!("{}{TMP_SUFFIX}", file.name));
    let result = async {
      self.source.fetch(tbl, &file.name, &tmp).await?;
      let bytes = read_async(tmp.clone()).await?;
      if bytes.len() as u64 != file.size {
        // 列表与内容不一致说明 master 正在追写 manifest，下一轮重试。
        log::warn!(
          target: "fkv_standby",
          "sync retry: manifest size mismatch name={} listed={} fetched={}",
          file.name,
          file.size,
          bytes.len()
        );
        #[cfg(debug_assertions)]
        eprintln!(
          "fkv_standby manifest size mismatch {} listed={} fetched={}",
          file.name,
          file.size,
          bytes.len()
        );
        return Err(KvError::TryAgain);
      }
      // 同秒等长覆写靠 (name, size, mtime) 识别不出来：再按内容核对一次。
      if !self.source.verify_same(tbl, &file.name, &tmp).await? {
        log::warn!(
          target: "fkv_standby",
          "sync retry: manifest content mismatch name={}",
          file.name
        );
        #[cfg(debug_assertions)]
        eprintln!("fkv_standby manifest content mismatch {}", file.name);
        return Err(KvError::TryAgain);
      }
      validate_manifest_bytes(&bytes)
    }
    .await;
    if result.is_err() {
      let _ = remove_file_async(tmp).await;
      return result;
    }
    rename_async(tmp, local_dir.join(dst_name)).await
  }

  /// 并发追赶多个分区：每批最多 `max_concurrency` 个分区同时同步，
  /// 对齐 C++ supervisor 的 ready queue。`tbl_li` 必须互不相同，
  /// 同一分区并发同步会争用同一 staging 目录。
  pub async fn sync_all(&self, tbl_li: &[TableIdent]) -> Result<Vec<SyncStat>> {
    let width = usize::from(self.config.max_concurrency.max(1));
    let mut stats_li = Vec::with_capacity(tbl_li.len());
    for chunk in tbl_li.chunks(width) {
      let fut_li = chunk.iter().map(|tbl| self.sync_table(tbl));
      for stat in join_all(fut_li).await {
        stats_li.push(stat?);
      }
    }
    Ok(stats_li)
  }

  /// Replica：一轮完整追赶 —— 先从远端发现有哪些表，再按 `max_concurrency` 限流同步。
  ///
  /// 返回本轮的表与统计（顺序同 [`RemoteSource::list_table_li`]），调用方拿到结果后
  /// 负责让读写侧看到新 manifest（`Reopen`）。单张表失败会整轮返回错误，由调用方
  /// 决定重试节奏——副本静默半同步比直接报错更危险。
  pub async fn catch_up(&self) -> Result<Vec<(TableIdent, SyncStat)>> {
    if self.config.role != StandbyRole::Replica {
      return Err(KvError::InvalidArgs);
    }
    let tbl_li = self.source.list_table_li().await?;
    let stats_li = self.sync_all(&tbl_li).await?;
    Ok(tbl_li.into_iter().zip(stats_li).collect())
  }
}

/// manifest 文件由 `align(record, OS page size)` 连续记录组成；必须完整、无 torn tail。
pub fn validate_manifest_bytes(bytes: &[u8]) -> Result<()> {
  let align = manifest_align();
  if bytes.is_empty() || !bytes.len().is_multiple_of(align) {
    return Err(KvError::Corrupted);
  }
  let mut pos = 0usize;
  let mut record_num = 0usize;
  while pos < bytes.len() {
    let header = parse_header(&bytes[pos..]).map_err(|_| KvError::Corrupted)?;
    let record_end = pos
      .checked_add(HEADER_BYTES)
      .and_then(|value| value.checked_add(header.payload_len as usize))
      .ok_or(KvError::Corrupted)?;
    if record_end > bytes.len()
      || !validate_record(&bytes[pos..record_end]).map_err(|_| KvError::Corrupted)?
    {
      return Err(KvError::Corrupted);
    }
    let next = (record_end + align - 1) & !(align - 1);
    if next <= pos || next > bytes.len() {
      return Err(KvError::Corrupted);
    }
    pos = next;
    record_num += 1;
  }
  if record_num == 0 || pos != bytes.len() {
    return Err(KvError::Corrupted);
  }
  Ok(())
}

pub fn select_manifest<'a>(file_li: &'a [RemoteFile], branch: &str) -> Option<&'a RemoteFile> {
  file_li
    .iter()
    .filter(|file| {
      matches!(
        parse_manifest_file_name(&file.name),
        Some((file_branch, _, None)) if file_branch == branch
      )
    })
    .max_by(|left, right| {
      let term_of = |file: &RemoteFile| {
        parse_manifest_file_name(&file.name)
          .map(|(_, term, _)| term)
          .unwrap_or(0)
      };
      (term_of(left), left.modified).cmp(&(term_of(right), right.modified))
    })
}

/// 选定指定 branch / tag 的归档 manifest；多 term 时取最高 term（同 term 取最新 mtime）。
pub fn select_archive_manifest<'a>(
  file_li: &'a [RemoteFile],
  branch: &str,
  tag: &str,
) -> Option<&'a RemoteFile> {
  file_li
    .iter()
    .filter(|file| {
      matches!(
        parse_manifest_file_name(&file.name),
        Some((file_branch, _, Some(archive_tag))) if file_branch == branch && archive_tag == tag
      )
    })
    .max_by(|left, right| {
      let key_of = |file: &RemoteFile| {
        parse_manifest_file_name(&file.name)
          .map(|(_, term, _)| term)
          .unwrap_or(0)
      };
      (key_of(left), left.modified).cmp(&(key_of(right), right.modified))
    })
}

/// main 分支的带 tag 归档 manifest（`manifest_main_<term>_<tag>`）。
fn is_main_archive_manifest(name: &str) -> bool {
  matches!(
    parse_manifest_file_name(name),
    Some((branch, _, Some(_))) if branch == MAIN_BRANCH_NAME
  )
}

/// cld-44：manifest term 不得大于 replica 自身 term。
///
/// **归档 manifest（tag 非空）显式豁免**：它是不可变历史快照，只在
/// `ReopenArchive(tag)` 显式请求时按远端原名安装，既不替换本地活跃 manifest，
/// 也不改变 replica 的 term 视图；对归档做 fencing 会拒掉用户明确点名的历史
/// 快照，且归档快照 term 高于当前 replica term 是合法状态（跨 term 回溯）。
/// 活跃 manifest（tag 为空）仍严格 fencing，防 failover 后装上未来 term。
pub fn check_manifest_term(name: &str, target_term: u64) -> Result<()> {
  let Some((_, term, tag)) = parse_manifest_file_name(name) else {
    return Err(KvError::Corrupted);
  };
  if tag.is_none() && target_term != 0 && term > target_term {
    return Err(KvError::ExpiredTerm);
  }
  Ok(())
}

fn is_data_file(name: &str) -> bool {
  name.starts_with(DATA_FILE_PREFIX) || name.starts_with(SEGMENT_FILE_PREFIX)
}

fn is_safe_file_name(name: &str) -> bool {
  !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\\')
}

pub fn table_dir(root: &Path, tbl: &TableIdent) -> PathBuf {
  root.join(tbl.dir_name())
}

/// `<tbl>.<partition>` 目录名 -> 表；名字不合法或不能原样还原时返回 `None`。
pub fn parse_table_name(name: &str) -> Option<TableIdent> {
  TableIdent::parse_dir_name(name)
}

/// 目录名清单 -> 表清单（过滤 + 定序）；本地目录列举与远端 `ls` 输出共用。
pub fn table_li_of_name_li(name_li: impl Iterator<Item = impl AsRef<str>>) -> Vec<TableIdent> {
  let mut tbl_li: Vec<TableIdent> = name_li
    .filter_map(|name| parse_table_name(name.as_ref()))
    .collect();
  // `TableIdent` 没有 `Ord`；按 (表名, 分区号) 定序，保证列举结果可复现。
  tbl_li.sort_by(|a, b| {
    (a.tbl_name.as_ref(), a.partition_id).cmp(&(b.tbl_name.as_ref(), b.partition_id))
  });
  tbl_li
}

/// 存储根下已有的表目录（`<tbl>.<partition>`）；根不存在视为空。
///
/// 跳过 `.standby` 暂存目录与非表目录（名字不匹配就不收），避免把 staging
/// 或任意子目录当成表去同步。
pub fn list_table_li(root: &Path) -> Result<Vec<TableIdent>> {
  let entry_li = match fs::read_dir(root) {
    Ok(entry_li) => entry_li,
    Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
    Err(_) => return Err(KvError::IoFail),
  };
  let name_li = entry_li
    .flatten()
    .filter(|entry| entry.metadata().is_ok_and(|meta| meta.is_dir()))
    .filter_map(|entry| entry.file_name().to_str().map(str::to_owned));
  Ok(table_li_of_name_li(name_li))
}

pub fn list_local_file_li(dir: &Path) -> Result<Vec<RemoteFile>> {
  let entry_li = match fs::read_dir(dir) {
    Ok(entry_li) => entry_li,
    Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
    Err(_) => return Err(KvError::IoFail),
  };
  let mut file_li = Vec::new();
  for entry in entry_li.flatten() {
    let path = entry.path();
    let Ok(meta) = entry.metadata() else {
      continue;
    };
    if !meta.is_file() {
      continue;
    }
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
      continue;
    };
    file_li.push(RemoteFile {
      name: name.to_owned(),
      size: meta.len(),
      modified: mtime_secs(&meta),
    });
  }
  file_li.sort_by(|left, right| left.name.cmp(&right.name));
  Ok(file_li)
}

fn map_io(_err: Error) -> KvError {
  KvError::IoFail
}

/// 在阻塞线程池执行同步 IO，避免在 compio reactor 线程上直接 syscall。
pub(crate) async fn block<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T> {
  // 非 compio 运行时（同步调用方 / 单测）直接执行，避免 panic。
  let mut f = Some(f);
  match Runtime::try_with_current(|runtime| runtime.spawn_blocking(f.take().unwrap())) {
    Ok(handle) => handle.await.map_err(|_| KvError::IoFail),
    Err(_) => Ok(f.take().unwrap()()),
  }
}

async fn local_len_async(path: PathBuf) -> Result<Option<u64>> {
  match block(move || fs::metadata(path)).await? {
    Ok(meta) => Ok(Some(meta.len())),
    Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
    Err(_) => Err(KvError::IoFail),
  }
}

async fn read_async(path: PathBuf) -> Result<Vec<u8>> {
  block(move || fs::read(path)).await?.map_err(map_io)
}

async fn rename_async(from: PathBuf, to: PathBuf) -> Result<()> {
  block(move || fs::rename(from, to)).await?.map_err(map_io)
}

async fn remove_file_async(path: PathBuf) -> Result<()> {
  match block(move || fs::remove_file(path)).await? {
    Ok(()) => Ok(()),
    Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
    Err(_) => Err(KvError::IoFail),
  }
}

/// 删除 staging 目录（含残留文件）；不存在视为成功。
async fn remove_dir_async(path: PathBuf) -> Result<()> {
  match block(move || fs::remove_dir_all(path)).await? {
    Ok(()) => Ok(()),
    Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
    Err(_) => Err(KvError::IoFail),
  }
}

/// fsync 目录项：`rename` / `unlink` 之后让目录项落盘，对齐 C++
/// `FinalizeRsync` 的 `fdatasync(dir_fd)`（macOS 上 `sync_data` 走 `F_FULLFSYNC`）。
async fn sync_dir_async(path: PathBuf) -> Result<()> {
  block(move || {
    let dir = fs::File::open(path)?;
    dir.sync_data()
  })
  .await?
  .map_err(map_io)
}

/// 删除分区目录里除 `active_name` 之外的无 tag **main** manifest，返回删除条数。
///
/// 对齐 C++ `FinalizeRsync` 的收尾清理（同分区只保留活跃 term 一份 manifest），
/// 但**收窄到 main 分支**：Rust 的 branch manifest 同样是 term 0
/// （`Store::prepare_attach` 用 `manifest_file_name(branch, 0)`），照 C++ 的
/// 「无 tag 且 term != target_term 一律删」会把 branch 快照 manifest 一起删掉。
/// 带 tag 的归档 manifest C++ 也不删，归档可继续 `reopen_archive`。
async fn remove_stale_main_manifest_li(dir: PathBuf, active_name: &str) -> Result<usize> {
  let mut stale_li: Vec<PathBuf> = Vec::new();
  for file in list_local_file_li(&dir)? {
    if file.name == active_name {
      continue;
    }
    let Some((branch, _, tag)) = parse_manifest_file_name(&file.name) else {
      continue;
    };
    if tag.is_none() && branch == MAIN_BRANCH_NAME {
      stale_li.push(dir.join(&file.name));
    }
  }
  let removed_num = stale_li.len();
  for path in stale_li {
    remove_file_async(path).await?;
  }
  Ok(removed_num)
}

/// mtime 秒数；不可用时退化为 0（与列表里的 `modified` 同口径）。
fn mtime_secs(meta: &fs::Metadata) -> u64 {
  meta
    .modified()
    .ok()
    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
    .map_or(0, |duration| duration.as_secs())
}

/// 增量判定：data 用 `(size, mtime)` quick-check，segment 额外比内容。
///
/// segment 文件定长，master 原地覆写同尺寸段时 mtime 秒数可能不变；
/// 只比 `(size, mtime)` 会把新数据永久漏传，因此 segment 必须按内容比较
/// （与 rsync 侧 `--checksum` 同口径）。
fn unchanged_sync(local: &Path, src: &Path, file: &RemoteFile) -> io::Result<bool> {
  let meta = match fs::metadata(local) {
    Ok(meta) => meta,
    Err(err) if err.kind() == ErrorKind::NotFound => return Ok(false),
    Err(err) => return Err(err),
  };
  if meta.len() != file.size || mtime_secs(&meta) < file.modified {
    return Ok(false);
  }
  if file.name.starts_with(SEGMENT_FILE_PREFIX) {
    return Ok(fs::read(local)? == fs::read(src)?);
  }
  Ok(true)
}

/// `LocalDirSource` 的 `sync_dir`：等价于 rsync `--compare-dest`，
/// 只拷贝与 `compare` 不同（或缺失）的 data / segment 文件。
fn copy_changed_data_file(src_dir: &Path, dst_dir: &Path, compare_dir: &Path) -> Result<()> {
  for file in list_local_file_li(src_dir)? {
    if !is_data_file(&file.name) || !is_safe_file_name(&file.name) {
      continue;
    }
    let src = src_dir.join(&file.name);
    if unchanged_sync(&compare_dir.join(&file.name), &src, &file).map_err(map_io)? {
      continue;
    }
    fs::copy(src, dst_dir.join(&file.name)).map_err(map_io)?;
  }
  Ok(())
}
