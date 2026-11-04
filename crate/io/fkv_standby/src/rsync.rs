//! rsync / ssh 同步骨架：命令构造、`rsync --list-only` 解析、可注入执行器。

use std::{path::Path, process::Command, str};

use fkv_error::{KvError, Result};
use fkv_types::TableIdent;

use crate::{RemoteFile, RemoteSource, is_safe_file_name};

pub trait CommandRunner {
  fn output(&self, program: &str, args: &[String]) -> Result<Vec<u8>>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StdCommandRunner;

impl CommandRunner for StdCommandRunner {
  fn output(&self, program: &str, args: &[String]) -> Result<Vec<u8>> {
    let output = Command::new(program)
      .args(args)
      .output()
      .map_err(|_| KvError::IoFail)?;
    if !output.status.success() {
      return Err(KvError::IoFail);
    }
    Ok(output.stdout)
  }
}

#[derive(Debug, Clone)]
pub struct RsyncShell<R> {
  runner: R,
  ssh_addr: String,
  rsync_bin: String,
}

impl<R> RsyncShell<R> {
  /// `ssh_addr` 为空时走本机 rsync；否则 list 用 ssh，fetch 用 rsync-over-ssh。
  pub fn new(runner: R, ssh_addr: impl Into<String>) -> Self {
    Self {
      runner,
      ssh_addr: ssh_addr.into(),
      rsync_bin: "rsync".to_owned(),
    }
  }

  pub fn with_rsync_bin(mut self, rsync_bin: impl Into<String>) -> Self {
    self.rsync_bin = rsync_bin.into();
    self
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
        .output(&self.rsync_bin, &["--list-only".to_owned(), dir])
    } else {
      self.runner.output(
        "ssh",
        &[
          self.ssh_addr.clone(),
          "rsync".to_owned(),
          "--list-only".to_owned(),
          dir,
        ],
      )
    }?;
    parse_rsync_list(&stdout)
  }

  pub fn fetch(&self, remote_file: &str, dst: &Path) -> Result<()> {
    let dst = dst.to_string_lossy().into_owned();
    let args = if self.ssh_addr.is_empty() {
      vec!["-a".to_owned(), remote_file.to_owned(), dst]
    } else {
      vec![
        "-a".to_owned(),
        format!("{}:{remote_file}", self.ssh_addr),
        dst,
      ]
    };
    self.runner.output(&self.rsync_bin, &args)?;
    Ok(())
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

#[derive(Debug)]
pub struct RsyncSource<R> {
  shell: RsyncShell<R>,
  master_root: String,
}

impl<R> RsyncSource<R> {
  pub fn new(runner: R, master_root: impl Into<String>, ssh_addr: impl Into<String>) -> Self {
    Self {
      shell: RsyncShell::new(runner, ssh_addr),
      master_root: master_root.into(),
    }
  }

  pub fn shell(&self) -> &RsyncShell<R> {
    &self.shell
  }

  fn remote_dir(&self, tbl: &TableIdent) -> String {
    format!(
      "{}/{}.{}",
      self.master_root.trim_end_matches('/'),
      tbl.tbl_name,
      tbl.partition_id
    )
  }
}

impl<R: CommandRunner> RemoteSource for RsyncSource<R> {
  async fn list(&self, tbl: &TableIdent) -> Result<Vec<RemoteFile>> {
    self.shell.list(&self.remote_dir(tbl))
  }

  async fn fetch(&self, tbl: &TableIdent, name: &str, dst: &Path) -> Result<()> {
    if !is_safe_file_name(name) {
      return Err(KvError::InvalidArgs);
    }
    self
      .shell
      .fetch(&format!("{}/{name}", self.remote_dir(tbl)), dst)
  }
}
