//! O_DIRECT 接线夹具：目录能力探测与 `/proc/self/fdinfo` 解析。
//!
//! 生产装配在 Linux 上把 data / segment 文件按 `O_DIRECT` 打开；集成测试需要
//! 在不暴露引擎内部后端的前提下确认「目标目录下指定类别的句柄真的带
//! `O_DIRECT`」。探测与解析集中在这里，避免各 crate 的测试各自手写后漂移。

use std::{
  fs,
  path::{Path, PathBuf},
};

/// 目录 O_DIRECT 能力与 direct fd 探测句柄。
///
/// 构造时对根目录做一次能力探测；`fd_kind_num` 可反复调用，统计调用时刻
/// 进程内仍指向该根目录的 direct 句柄。
#[derive(Debug, Clone)]
pub struct DirectIoProbe {
  root: PathBuf,
  supported: bool,
}

/// 对 `root` 做一次 O_DIRECT 能力探测（写入 4 KiB 探测文件后按 `O_DIRECT`
/// 打开）。根目录会先 `canonicalize`，与 `/proc/self/fd` 的链接目标口径一致。
///
/// 非 Linux 上 direct 只是逻辑开关（对齐缓冲 + 分块读），恒视为支持。
pub fn direct_io_probe(root: impl Into<PathBuf>) -> DirectIoProbe {
  let root = root.into();
  // `/proc/self/fd/<fd>` 的链接目标是解析过的绝对路径；根目录是符号链接时
  // 不规范化会让前缀过滤永远不命中（用例静默通过而不是失败）。
  let root = fs::canonicalize(&root).unwrap_or(root);
  let supported = probe_supported(&root);
  DirectIoProbe { root, supported }
}

impl DirectIoProbe {
  /// 该目录是否支持按 `O_DIRECT` 打开文件。
  pub fn supported(&self) -> bool {
    self.supported
  }

  /// 进程内指向探测根目录之下、且带 `O_DIRECT` 的 data / segment 打开句柄数。
  pub fn fd_kind_num(&self) -> (usize, usize) {
    direct_fd_kind_num(&self.root)
  }
}

#[cfg(target_os = "linux")]
fn probe_supported(root: &Path) -> bool {
  use std::{
    os::unix::fs::OpenOptionsExt,
    process,
    sync::atomic::{AtomicU64, Ordering},
  };

  /// 探测文件名序号：同一目录被多次探测时也不互撞。
  static PROBE_SEQ: AtomicU64 = AtomicU64::new(0);

  let seq = PROBE_SEQ.fetch_add(1, Ordering::Relaxed);
  let probe = root.join(format!(".fkv_direct_probe_{}_{seq}", process::id()));
  if fs::write(&probe, [0u8; 4096]).is_err() {
    return false;
  }
  let supported = fs::OpenOptions::new()
    .read(true)
    .custom_flags(libc::O_DIRECT)
    .open(&probe)
    .is_ok();
  let _ = fs::remove_file(&probe);
  supported
}

#[cfg(not(target_os = "linux"))]
fn probe_supported(_root: &Path) -> bool {
  true
}

/// 读 `/proc/self/fdinfo/<fd>` 的八进制 `flags`，判断句柄真实的 open 标志。
#[cfg(target_os = "linux")]
fn fd_is_direct(entry: &fs::DirEntry) -> bool {
  let info_path = format!("/proc/self/fdinfo/{}", entry.file_name().to_string_lossy());
  let Ok(info) = fs::read_to_string(info_path) else {
    return false;
  };
  info
    .lines()
    .filter_map(|line| line.split_once(':'))
    .find(|(name, _)| name.trim() == "flags")
    .and_then(|(_, value)| u64::from_str_radix(value.trim(), 8).ok())
    .is_some_and(|flags| flags & libc::O_DIRECT as u64 != 0)
}

/// 按盘面文件名前缀统计 direct 句柄；分类常量复用 `fkv_types`，避免手抄
/// `data` / `segment` 字面量再次漂移。
#[cfg(target_os = "linux")]
fn direct_fd_kind_num(root: &Path) -> (usize, usize) {
  let Ok(fd_li) = fs::read_dir("/proc/self/fd") else {
    return (0, 0);
  };
  let mut data = 0;
  let mut segment = 0;
  for entry in fd_li.flatten() {
    let Ok(target) = fs::read_link(entry.path()) else {
      continue;
    };
    if !target.starts_with(root) || !fd_is_direct(&entry) {
      continue;
    }
    match target.file_name().and_then(|name| name.to_str()) {
      Some(name) if name.starts_with(fkv_types::FILE_NAME_DATA) => data += 1,
      Some(name) if name.starts_with(fkv_types::FILE_NAME_SEGMENT) => segment += 1,
      _ => {}
    }
  }
  (data, segment)
}

#[cfg(not(target_os = "linux"))]
fn direct_fd_kind_num(_root: &Path) -> (usize, usize) {
  (0, 0)
}
