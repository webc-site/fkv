use std::{fs, path::Path, process::Command};

use crate::error::{LedgerError, Result};

#[derive(Debug, Clone)]
pub struct Baseline {
  pub commit: String,
  pub date: String,
  pub level: String,
}

pub fn read(path: &Path) -> Result<Option<Baseline>> {
  let Ok(text) = fs::read_to_string(path) else {
    return Ok(None);
  };
  let mut line_li = text.lines();
  let Some(first) = line_li.next() else {
    return Ok(None);
  };
  let part_li: Vec<&str> = first.split_whitespace().collect();
  if part_li.len() < 3 {
    return Ok(None);
  }
  let level = line_li
    .find_map(|line| line.strip_prefix("兼容等级："))
    .unwrap_or("L1")
    .to_owned();
  Ok(Some(Baseline {
    commit: part_li[2].to_owned(),
    date: part_li.get(3).copied().unwrap_or("").to_owned(),
    level,
  }))
}

pub fn latest(repo: &Path, reference: &str, fetch: bool) -> Result<(String, String)> {
  if fetch {
    git(repo, &["fetch", "origin", reference])?;
  }
  let candidate_li = [format!("origin/{reference}"), reference.to_owned()];
  let mut commit = None;
  for candidate in candidate_li {
    if let Ok(value) = git(repo, &["rev-parse", &candidate]) {
      commit = Some(value);
      break;
    }
  }
  let commit = commit
    .ok_or_else(|| LedgerError::Invalid(format!("cannot resolve {reference}")))?
    .trim()
    .to_owned();
  let date = git(repo, &["show", "-s", "--format=%cI", &commit])?
    .trim()
    .to_owned();
  Ok((commit, date))
}

pub fn write(path: &Path, reference: &str, commit: &str, date: &str, level: &str) -> Result<()> {
  fs::write(
    path,
    format!("eloqstore {reference} {commit} {date}\n兼容等级：{level}\n"),
  )?;
  Ok(())
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
  let output = Command::new("git")
    .arg("-C")
    .arg(repo)
    .args(args)
    .output()?;
  if !output.status.success() {
    return Err(LedgerError::Invalid(format!(
      "git {} failed: {}",
      args.join(" "),
      String::from_utf8_lossy(&output.stderr).trim()
    )));
  }
  Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
