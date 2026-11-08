//! rsync / ssh 同步骨架：命令构造、`rsync --list-only` 解析、可注入执行器。

use std::{
  fs::{self, read},
  path::Path,
  process::{self, Command},
  str,
  sync::Arc,
};

use fkv_error::{KvError, Result};
use fkv_types::{DATA_FILE_PREFIX, SEGMENT_FILE_PREFIX, TableIdent};

use crate::{RemoteFile, RemoteSource, block, is_safe_file_name};

/// rsync 归档模式：保留属性、递归。
const RSYNC_ARCHIVE: &str = "-a";
/// 原地写：staging 每轮清空，省掉 rsync 自建临时文件再 rename 的开销。
const RSYNC_INPLACE: &str = "--inplace";
/// 与本地已有副本逐文件 quick-check，相同则不传输（增量追赶的关键）。
const RSYNC_COMPARE_DEST: &str = "--compare-dest=";
/// 只放行 data_ / segment_，其余（含 manifest）一律排除。
const RSYNC_EXCLUDE_ALL: &str = "--exclude=*";
const RSYNC_INCLUDE: &str = "--include=";
/// 按内容而非 `(size, mtime)` 比较（manifest 安装前复核）。
const RSYNC_CHECKSUM: &str = "--checksum";
/// rsync 可执行文件固定取 PATH 中的 `rsync`；部署侧通过 PATH 覆盖。
const RSYNC_BIN: &str = "rsync";

/// 子进程执行结果：退出码 + stdout。
///
/// rsync 的错误分类依赖退出码（源目录缺失是 23/24，ssh 连接失败是 255），
/// 不能把非零退出压平成同一种错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
  /// `None` 表示进程被信号杀死（`ExitStatus::code()` 的语义）。
  pub code: Option<i32>,
  pub stdout: Vec<u8>,
}

impl CommandOutput {
  /// rsync 退出码映射，对齐 C++ `StandbyService::InterpretRsyncStatus`。
  ///
  /// 23 / 24 是「源文件或目录缺失 / 传输不完整」：master 已删分区时远端
  /// `rsync` 正是这个退出码，必须映射成 `NotFound` 才能触发 standby 的本地
  /// 状态清理（C++ `ReopenTask::clear_local_state`）。其余非零退出（含 ssh
  /// 连接失败的 255）保持 `IoFail`，不清仍可用的本地副本。
  fn into_rsync_stdout(self) -> Result<Vec<u8>> {
    match self.code {
      Some(0) => Ok(self.stdout),
      Some(23 | 24) => Err(KvError::NotFound),
      _ => Err(KvError::IoFail),
    }
  }

  /// 非 rsync 命令（`ls`）：任何非零退出都是 `IoFail`，对齐 C++
  /// `StandbyService::InterpretCommandStatus`。
  fn into_stdout(self) -> Result<Vec<u8>> {
    if self.code == Some(0) {
      Ok(self.stdout)
    } else {
      Err(KvError::IoFail)
    }
  }
}

pub trait CommandRunner {
  fn output(&self, program: &str, args: &[String]) -> Result<CommandOutput>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StdCommandRunner;

impl CommandRunner for StdCommandRunner {
  fn output(&self, program: &str, args: &[String]) -> Result<CommandOutput> {
    let output = Command::new(program)
      .args(args)
      .output()
      .map_err(|_| KvError::IoFail)?;
    Ok(CommandOutput {
      code: output.status.code(),
      stdout: output.stdout,
    })
  }
}

#[derive(Debug, Clone)]
pub struct RsyncShell<R> {
  runner: R,
  ssh_addr: String,
}

impl<R> RsyncShell<R> {
  /// `ssh_addr` 为空时走本机 rsync；否则 list 用 ssh，fetch 用 rsync-over-ssh。
  pub fn new(runner: R, ssh_addr: impl Into<String>) -> Self {
    Self {
      runner,
      ssh_addr: ssh_addr.into(),
    }
  }

  pub fn runner(&self) -> &R {
    &self.runner
  }
}

impl<R: CommandRunner> RsyncShell<R> {
  pub fn list(&self, remote_dir: &str) -> Result<Vec<RemoteFile>> {
    let dir = format!("{}/", remote_dir.trim_end_matches('/'));
    let stdout = if self.ssh_addr.is_empty() {
      self
        .runner
        .output(RSYNC_BIN, &["--list-only".to_owned(), dir])
        .and_then(CommandOutput::into_rsync_stdout)?
    } else {
      self
        .runner
        .output(
          "ssh",
          &[
            self.ssh_addr.clone(),
            RSYNC_BIN.to_owned(),
            "--list-only".to_owned(),
            dir,
          ],
        )
        .and_then(CommandOutput::into_rsync_stdout)?
    };
    parse_rsync_list(&stdout)
  }

  /// 一次 rsync 拉整个分区目录：只取 `data_` / `segment_`，与 `compare`
  /// 相同者不传输。对齐 C++ `StartRsyncJob` 的「每分区一次 rsync」，
  /// 避免每个文件一次进程 + 一次 ssh 握手。
  ///
  /// 必须带 `--checksum`：segment 文件定长，同尺寸同秒的原地覆写用默认
  /// `(size, mtime)` quick-check 会被误判成未变更而永久漏传。
  ///
  /// `dst` 为 staging 目录：原地写只发生在 staging，安装仍靠后续 rename
  /// 保证原子。
  pub fn sync_dir(&self, remote_dir: &str, dst: &Path, compare: &Path) -> Result<()> {
    let src = dir_spec(&format!("{}/", remote_dir.trim_end_matches('/')));
    let src = if self.ssh_addr.is_empty() {
      src
    } else {
      format!("{}:{src}", self.ssh_addr)
    };
    let args = vec![
      RSYNC_ARCHIVE.to_owned(),
      RSYNC_INPLACE.to_owned(),
      RSYNC_CHECKSUM.to_owned(),
      format!(
        "{RSYNC_COMPARE_DEST}{}",
        dir_spec(&compare.to_string_lossy())
      ),
      format!("{RSYNC_INCLUDE}{DATA_FILE_PREFIX}*"),
      format!("{RSYNC_INCLUDE}{SEGMENT_FILE_PREFIX}*"),
      RSYNC_EXCLUDE_ALL.to_owned(),
      src,
      dir_spec(&dst.to_string_lossy()),
    ];
    self
      .runner
      .output(RSYNC_BIN, &args)
      .and_then(CommandOutput::into_rsync_stdout)?;
    Ok(())
  }

  pub fn fetch(&self, remote_file: &str, dst: &Path) -> Result<()> {
    let dst = dst.to_string_lossy().into_owned();
    let args = if self.ssh_addr.is_empty() {
      vec![RSYNC_ARCHIVE.to_owned(), remote_file.to_owned(), dst]
    } else {
      vec![
        RSYNC_ARCHIVE.to_owned(),
        format!("{}:{remote_file}", self.ssh_addr),
        dst,
      ]
    };
    self
      .runner
      .output(RSYNC_BIN, &args)
      .and_then(CommandOutput::into_rsync_stdout)?;
    Ok(())
  }

  /// 远端文件与本地副本内容是否一致：`--checksum` 干跑，无变更则无输出。
  pub fn verify_same(&self, remote_file: &str, local: &Path) -> Result<bool> {
    // 本机 rsync 直接比字节：macOS 自带 openrsync 的 `--itemize-changes`
    // 输出与 GNU rsync 不同，依赖「无输出」会误判为内容不一致。
    if self.ssh_addr.is_empty() {
      let remote = read(remote_file).map_err(|_| KvError::IoFail)?;
      let local = read(local).map_err(|_| KvError::IoFail)?;
      return Ok(remote == local);
    }
    let src = if self.ssh_addr.is_empty() {
      remote_file.to_owned()
    } else {
      format!("{}:{remote_file}", self.ssh_addr)
    };
    let stdout = self
      .runner
      .output(
        RSYNC_BIN,
        &[
          RSYNC_ARCHIVE.to_owned(),
          RSYNC_CHECKSUM.to_owned(),
          "--dry-run".to_owned(),
          "--itemize-changes".to_owned(),
          src,
          local.to_string_lossy().into_owned(),
        ],
      )
      .and_then(CommandOutput::into_rsync_stdout)?;
    // 只看「需要传输」的行（以 `>` 开头）；元数据行不算内容变化。
    if !stdout
      .split(|byte| *byte == b'\n')
      .any(|line| line.starts_with(b">"))
    {
      return Ok(true);
    }
    // macOS 自带 openrsync 的 `--dry-run` 不保证按 `--checksum` 比较，会把
    // 仅 mtime 不同的同内容文件报成待传输；远端路径也无法走上面的本地直读。
    // 报「有变化」时再抓一次远端并逐字节确认，避免把正常 manifest 永久判成
    // TryAgain（Linux GNU rsync 命中这里只发生在真实并发写）。
    let probe = format!("{}.verify-{}", local.to_string_lossy(), process::id());
    let result = (|| {
      self.fetch(remote_file, Path::new(&probe))?;
      let remote = read(&probe).map_err(|_| KvError::IoFail)?;
      let local = read(local).map_err(|_| KvError::IoFail)?;
      Ok(remote == local)
    })();
    let _ = fs::remove_file(&probe);
    result
  }

  /// 远端根下已有的表目录（`<tbl>.<partition>`）：一次 `ls -1`，不逐表探测。
  ///
  /// 用 `ls` 而不是 `rsync --list-only`：后者输出只保留普通文件行（`-` 开头），
  /// 目录会被过滤掉，拿不到分区目录名。
  pub fn list_table_li(&self, remote_root: &str) -> Result<Vec<TableIdent>> {
    let root = remote_root.trim_end_matches('/').to_owned();
    let stdout = if self.ssh_addr.is_empty() {
      self
        .runner
        .output("ls", &["-1".to_owned(), root])
        .and_then(CommandOutput::into_stdout)?
    } else {
      self
        .runner
        .output(
          "ssh",
          &[
            self.ssh_addr.clone(),
            "ls".to_owned(),
            "-1".to_owned(),
            root,
          ],
        )
        .and_then(CommandOutput::into_stdout)?
    };
    let text = str::from_utf8(&stdout).map_err(|_| KvError::Corrupted)?;
    Ok(crate::table_li_of_name_li(text.lines()))
  }
}

/// rsync 的目录参数带尾 `/` 表示「目录内容」。
fn dir_spec(text: &str) -> String {
  if text.ends_with('/') {
    text.to_owned()
  } else {
    format!("{text}/")
  }
}

/// rsync 输出形如：
/// `-rw-r--r--          1,234 2026/11/04 12:00:00 data_0_main_0`
pub fn parse_rsync_list(stdout: &[u8]) -> Result<Vec<RemoteFile>> {
  let text = str::from_utf8(stdout).map_err(|_| KvError::Corrupted)?;
  let mut file_li = Vec::new();
  for line in text.lines() {
    let part_li: Vec<&str> = line.split_whitespace().collect();
    if part_li.len() < 5 || !part_li[0].starts_with('-') {
      continue;
    }
    let Some(name) = part_li.last() else {
      continue;
    };
    if !is_safe_file_name(name) {
      continue;
    }
    let Ok(size) = part_li[1].replace(',', "").parse::<u64>() else {
      continue;
    };
    let modified = parse_modified(part_li[2], part_li[3]).unwrap_or(0);
    file_li.push(RemoteFile {
      name: (*name).to_owned(),
      size,
      modified,
    });
  }
  file_li.sort_by(|left, right| left.name.cmp(&right.name));
  Ok(file_li)
}

/// rsync 输出的时间戳是 **sender 本地时区**，这里按 UTC 换算，因此绝对值
/// 与真实 epoch 可能差一个时区偏移。该字段只用于同一份列表内的相对比较
/// （`select_manifest` 的同 term 平局），不参与跨来源的 mtime 判定；
/// 增量判定以 rsync `--compare-dest` / 本地拷贝为准。
fn parse_modified(date: &str, time: &str) -> Option<u64> {
  let mut date_li = date.split('/');
  let year = date_li.next()?.parse::<i64>().ok()?;
  let month = date_li.next()?.parse::<u32>().ok()?;
  let day = date_li.next()?.parse::<u32>().ok()?;
  let mut time_li = time.split(':');
  let hour = time_li.next()?.parse::<u64>().ok()?;
  let minute = time_li.next()?.parse::<u64>().ok()?;
  let second = time_li.next()?.parse::<u64>().ok()?;
  if !(1..=12).contains(&month)
    || !(1..=31).contains(&day)
    || hour > 23
    || minute > 59
    || second > 59
  {
    return None;
  }
  let days = days_from_civil(year, month, day);
  u64::try_from(days * 86_400 + hour as i64 * 3_600 + minute as i64 * 60 + second as i64).ok()
}

/// Howard Hinnant `days_from_civil`。
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
  let year = if month <= 2 { year - 1 } else { year };
  let era = if year >= 0 { year } else { year - 399 } / 400;
  let year_of_era = year - era * 400;
  let month = month as i64;
  let day = day as i64;
  let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
  let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
  era * 146_097 + day_of_era - 719_468
}

/// 远端 master 的 rsync 来源。
///
/// 单根时行为与历史一致（LUT 恒 `[0]`）。cld-58：多根 master 按
/// `master_path_lut_li[partition_id % lut.len()]` 选根，与本地多根
/// `RootedConfig::root_index` / C++ `TableIdent::StorePathIndex` 同式——
/// 配置面生成 LUT 后运行面必须消费它，否则不在 root[0] 的分区会被当成
/// 「master 上不存在」进而触发 replica 的 `drop_manifest`。
#[derive(Debug, Clone)]
pub struct RsyncSource<R> {
  shell: Arc<RsyncShell<R>>,
  master_root_li: Vec<String>,
  /// 分区 -> `master_root_li` 下标；槽位按 `partition_id % len` 取。
  master_path_lut_li: Vec<u32>,
}

impl<R> RsyncSource<R> {
  pub fn new(runner: R, master_root: impl Into<String>, ssh_addr: impl Into<String>) -> Self {
    Self {
      shell: Arc::new(RsyncShell::new(runner, ssh_addr)),
      master_root_li: vec![master_root.into()],
      master_path_lut_li: vec![0],
    }
  }

  /// 多根 master：`master_path_lut_li` 由
  /// `fkv_config::compute_store_path_lut(standby_master_store_path_weights)` 生成。
  ///
  /// 空根列表 / 空 LUT / LUT 索引越界都在构造时拒绝（配置错误应在启动时
  /// 暴露，而不是运行期静默退回 root[0]）。
  pub fn new_rooted(
    runner: R,
    master_root_li: Vec<String>,
    master_path_lut_li: Vec<u32>,
    ssh_addr: impl Into<String>,
  ) -> Result<Self> {
    if master_root_li.is_empty() || master_path_lut_li.is_empty() {
      return Err(KvError::InvalidArgs);
    }
    let root_num = master_root_li.len();
    if master_path_lut_li
      .iter()
      .any(|&idx| usize::try_from(idx).map_or(true, |idx| idx >= root_num))
    {
      return Err(KvError::InvalidArgs);
    }
    Ok(Self {
      shell: Arc::new(RsyncShell::new(runner, ssh_addr)),
      master_root_li,
      master_path_lut_li,
    })
  }

  pub fn shell(&self) -> &RsyncShell<R> {
    &self.shell
  }

  pub fn master_root_li(&self) -> &[String] {
    &self.master_root_li
  }

  /// 分区归属的 master 根（构造时已校验，索引必然有效）。
  fn master_root_of(&self, tbl: &TableIdent) -> &str {
    let slot = u64::from(tbl.partition_id) % self.master_path_lut_li.len() as u64;
    &self.master_root_li[self.master_path_lut_li[slot as usize] as usize]
  }

  fn remote_dir(&self, tbl: &TableIdent) -> String {
    format!(
      "{}/{}.{}",
      self.master_root_of(tbl).trim_end_matches('/'),
      tbl.tbl_name,
      tbl.partition_id
    )
  }
}

impl<R: CommandRunner + Send + Sync + 'static> RemoteSource for RsyncSource<R> {
  async fn list(&self, tbl: &TableIdent) -> Result<Vec<RemoteFile>> {
    let shell = Arc::clone(&self.shell);
    let dir = self.remote_dir(tbl);
    // 本机 master 目录不存在：rsync 只会给非零退出（IoFail），而 reopen 依赖
    // `ResourceMissing` 判断「master 还没建表」并进重试队列。
    if shell.ssh_addr.is_empty() && !Path::new(&dir).exists() {
      return Err(KvError::ResourceMissing);
    }
    block(move || shell.list(&dir)).await?
  }

  async fn sync_dir(&self, tbl: &TableIdent, dst: &Path, compare: &Path) -> Result<()> {
    let shell = Arc::clone(&self.shell);
    let dir = self.remote_dir(tbl);
    let dst = dst.to_path_buf();
    let compare = compare.to_path_buf();
    block(move || shell.sync_dir(&dir, &dst, &compare)).await?
  }

  async fn fetch(&self, tbl: &TableIdent, name: &str, dst: &Path) -> Result<()> {
    if !is_safe_file_name(name) {
      return Err(KvError::InvalidArgs);
    }
    let shell = Arc::clone(&self.shell);
    let remote_file = format!("{}/{name}", self.remote_dir(tbl));
    let dst = dst.to_path_buf();
    block(move || shell.fetch(&remote_file, &dst)).await?
  }

  async fn verify_same(&self, tbl: &TableIdent, name: &str, local: &Path) -> Result<bool> {
    if !is_safe_file_name(name) {
      return Err(KvError::InvalidArgs);
    }
    let shell = Arc::clone(&self.shell);
    let remote_file = format!("{}/{name}", self.remote_dir(tbl));
    let local = local.to_path_buf();
    block(move || shell.verify_same(&remote_file, &local)).await?
  }

  async fn list_table_li(&self) -> Result<Vec<TableIdent>> {
    let mut tbl_li = Vec::new();
    for master_root in &self.master_root_li {
      let shell = Arc::clone(&self.shell);
      let root = master_root.trim_end_matches('/').to_owned();
      // 本地 master 的某个根还没建目录：视为该根下暂无表（与 `list` 的
      // `ResourceMissing` 同义），不因一个空根让整轮 catch-up 失败。
      // ssh 远端无法预先判断，命令失败照旧上抛。
      if shell.ssh_addr.is_empty() && !Path::new(&root).exists() {
        continue;
      }
      tbl_li.extend(block(move || shell.list_table_li(&root)).await??);
    }
    // 各根列举结果合并：同一分区按 LUT 只应落一个根，去重防御重复目录。
    tbl_li.sort_by(|left, right| {
      (left.tbl_name.as_ref(), left.partition_id)
        .cmp(&(right.tbl_name.as_ref(), right.partition_id))
    });
    tbl_li.dedup();
    Ok(tbl_li)
  }
}
