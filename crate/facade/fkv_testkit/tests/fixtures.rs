//! fkv_testkit 夹具自测：warn 捕获窗口与 direct fd 分类。

use fkv_testkit::capture_warn;
#[cfg(target_os = "linux")]
use fkv_testkit::{TempRoot, direct_io_probe};

#[test]
fn capture_warn_filters_by_needle() {
  let warn = capture_warn();
  log::warn!(target: "fkv_testkit_fixture", "fixture-needle-alpha");
  assert!(warn.warned_with(&["fixture-needle-alpha"]));
  assert!(!warn.warned_with(&["fixture-needle-beta"]));
  // 空条件不能让任意日志命中。
  assert!(!warn.warned_with(&[]));
}

#[test]
fn capture_warn_only_sees_records_after_creation() {
  let first = capture_warn();
  log::warn!(target: "fkv_testkit_fixture", "fixture-before-window");
  assert!(first.warned_with(&["fixture-before-window"]));

  let second = capture_warn();
  assert!(!second.warned_with(&["fixture-before-window"]));
  log::warn!(target: "fkv_testkit_fixture", "fixture-after-window");
  assert!(second.warned_with(&["fixture-after-window"]));
  // 先创建的句柄观察窗口更大，后发生的记录仍在窗内。
  assert!(first.warned_with(&["fixture-after-window"]));
}

/// 只有目标根目录下、真实带 `O_DIRECT` 的句柄才计入分类。
#[cfg(target_os = "linux")]
#[test]
fn direct_io_probe_counts_only_open_direct_fds_under_root() {
  use std::{fs, os::unix::fs::OpenOptionsExt};

  let root = TempRoot::new("probe_fd");
  let probe = direct_io_probe(root.path());
  if !probe.supported() {
    eprintln!("跳过：临时目录不支持 O_DIRECT");
    return;
  }
  let data = root.join(fkv_types::FILE_NAME_DATA);
  fs::write(&data, [0u8; 4096]).unwrap();

  // buffered 句柄不计入。
  let buffered = fs::File::open(&data).unwrap();
  assert_eq!(probe.fd_kind_num(), (0, 0));

  // direct 句柄按 data 类别计数；关闭后归零。
  let direct = fs::OpenOptions::new()
    .read(true)
    .custom_flags(libc::O_DIRECT)
    .open(&data)
    .unwrap();
  assert_eq!(probe.fd_kind_num(), (1, 0));
  drop(direct);
  assert_eq!(probe.fd_kind_num(), (0, 0));
  drop(buffered);
}
