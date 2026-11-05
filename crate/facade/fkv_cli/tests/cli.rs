use std::{
  fs::write,
  path::Path,
  process::{Command, Output},
};

use fkv_testkit::TempRoot;

fn run(root: &Path, args: &[&str]) -> Output {
  Command::new(env!("CARGO_BIN_EXE_fkv"))
    .arg("--root")
    .arg(root)
    .args(args)
    .output()
    .unwrap()
}

#[test]
fn cli_put_get_scan_floor_truncate() {
  let root = TempRoot::new("cli");
  assert!(
    run(
      root.path(),
      &["put", "--table", "demo.1", "--key", "a", "--value", "A"]
    )
    .status
    .success()
  );
  assert!(
    run(
      root.path(),
      &["put", "--table", "demo.1", "--key", "b", "--value", "B"]
    )
    .status
    .success()
  );
  let get = run(root.path(), &["get", "--table", "demo.1", "--key", "a"]);
  assert!(get.status.success());
  assert_eq!(String::from_utf8_lossy(&get.stdout).trim(), "A");
  let scan = run(root.path(), &["scan", "--table", "demo.1"]);
  assert!(scan.status.success());
  assert_eq!(String::from_utf8_lossy(&scan.stdout).lines().count(), 2);
  let floor = run(root.path(), &["floor", "--table", "demo.1", "--key", "c"]);
  assert!(floor.status.success());
  assert_eq!(String::from_utf8_lossy(&floor.stdout), "b\tB\n");
  assert!(
    run(
      root.path(),
      &["truncate", "--table", "demo.1", "--position", "b"]
    )
    .status
    .success()
  );
  let scan = run(root.path(), &["scan", "--table", "demo.1"]);
  assert_eq!(String::from_utf8_lossy(&scan.stdout).lines().count(), 1);
}

/// Local 数据面（SDK）与控制面（Runtime）必须共用 `--term`。
///
/// 归档 / 建分支 / 重开归档后仍要读回原值：读侧虽有 (branch, term) 变体
/// 回退兜底，但写身份分裂会让新旧写入落不同 term 的对象/文件名，归档
/// CAS 与 GC 归属也依赖统一 term，故两边必须同 term。
#[test]
fn cli_local_term_identity_survives_archive_and_branch() {
  let root = TempRoot::new("cli_term");
  let term_arg = ["--term", "3"];
  let put = run(
    root.path(),
    &[
      &term_arg[..],
      &["put", "--table", "demo.1", "--key", "a", "--value", "A"],
    ]
    .concat(),
  );
  assert!(
    put.status.success(),
    "put stderr: {}",
    String::from_utf8_lossy(&put.stderr)
  );
  for args in [
    vec!["archive", "--tag", "t1"],
    vec!["create-branch", "--branch", "b1"],
    vec!["reopen-archive", "--tag", "t1"],
  ] {
    let out = run(root.path(), &[&term_arg[..], &args].concat());
    assert!(
      out.status.success(),
      "{args:?} stderr: {}",
      String::from_utf8_lossy(&out.stderr)
    );
  }
  let get = run(
    root.path(),
    &[&term_arg[..], &["get", "--table", "demo.1", "--key", "a"]].concat(),
  );
  assert!(
    get.status.success(),
    "get stderr: {}",
    String::from_utf8_lossy(&get.stderr)
  );
  assert_eq!(String::from_utf8_lossy(&get.stdout).trim(), "A");
}

/// Local 之外的模式必须真的走 Runtime：Standby 落本地盘，跨进程可读。
#[test]
fn cli_standby_mode_put_get_and_global_archive() {
  let root = TempRoot::new("cli_standby");
  let put = run(
    root.path(),
    &[
      "--mode", "standby", "put", "--table", "demo.1", "--key", "a", "--value", "A",
    ],
  );
  assert!(
    put.status.success(),
    "put stderr: {}",
    String::from_utf8_lossy(&put.stderr)
  );
  let get = run(
    root.path(),
    &[
      "--mode", "standby", "get", "--table", "demo.1", "--key", "a",
    ],
  );
  assert!(
    get.status.success(),
    "get stderr: {}",
    String::from_utf8_lossy(&get.stderr)
  );
  assert_eq!(String::from_utf8_lossy(&get.stdout).trim(), "A");

  // 控制面命令走 Runtime 的 global_*，同时验证 4+1 全局操作已进生产入口。
  let archive = run(
    root.path(),
    &["--mode", "standby", "archive", "--tag", "t1"],
  );
  assert!(
    archive.status.success(),
    "archive stderr: {}",
    String::from_utf8_lossy(&archive.stderr)
  );
  let tags = run(
    root.path(),
    &["--mode", "standby", "list-archive-tags", "--prefix", "t"],
  );
  assert!(
    tags.status.success(),
    "list stderr: {}",
    String::from_utf8_lossy(&tags.stderr)
  );
  assert!(
    String::from_utf8_lossy(&tags.stdout).contains("t1\t1"),
    "stdout: {}",
    String::from_utf8_lossy(&tags.stdout)
  );
  let reopen = run(
    root.path(),
    &["--mode", "standby", "reopen-archive", "--tag", "t1"],
  );
  assert!(
    reopen.status.success(),
    "reopen stderr: {}",
    String::from_utf8_lossy(&reopen.stderr)
  );
}

#[test]
fn cli_mem_mode_is_process_local() {
  let root = TempRoot::new("cli_mem");
  let put = run(
    root.path(),
    &[
      "--mode", "mem", "put", "--table", "demo.1", "--key", "a", "--value", "A",
    ],
  );
  assert!(
    put.status.success(),
    "put stderr: {}",
    String::from_utf8_lossy(&put.stderr)
  );
  // Mem 无 durability：新进程读不到上一个进程写的数据。
  let get = run(
    root.path(),
    &["--mode", "mem", "get", "--table", "demo.1", "--key", "a"],
  );
  assert!(!get.status.success());
}

/// 显式 Cloud 但没有 `cloud_store_path`：启动必须失败，不能静默按 Local 跑。
#[test]
fn cli_cloud_mode_without_config_fails() {
  let root = TempRoot::new("cli_cloud_missing");
  let out = run(
    root.path(),
    &[
      "--mode", "cloud", "put", "--table", "demo.1", "--key", "a", "--value", "A",
    ],
  );
  assert!(!out.status.success());
}

#[test]
fn cli_unknown_mode_fails() {
  let root = TempRoot::new("cli_bad_mode");
  let out = run(
    root.path(),
    &[
      "--mode", "bogus", "put", "--table", "demo.1", "--key", "a", "--value", "A",
    ],
  );
  assert!(!out.status.success());
}

/// Mem 模式没有 `KvOption` 装载路径：显式 `--config` 不能被静默丢弃。
#[test]
fn cli_mem_mode_rejects_config() {
  let root = TempRoot::new("cli_mem_config");
  let ini = root.path().join("kv.ini");
  write(
    &ini,
    "[run]\nnum_threads = 1\n[permanent]\ndata_page_size = 4KB\n",
  )
  .unwrap();
  let out = run(
    root.path(),
    &[
      "--mode",
      "mem",
      "--config",
      ini.to_str().unwrap(),
      "put",
      "--table",
      "demo.1",
      "--key",
      "a",
      "--value",
      "A",
    ],
  );
  assert!(!out.status.success());
  assert!(
    String::from_utf8_lossy(&out.stderr).contains("--config"),
    "stderr: {}",
    String::from_utf8_lossy(&out.stderr)
  );
}

/// `store_path` 是配置驱动的数据根：Local 数据面（put/get）与控制面（archive）
/// 必须都走 Runtime。此前数据面走 SDK，`store_path[0] != --root` 时直接
/// `InvalidArgs`，同一份配置里 `archive` 成功、`put` 失败。
#[test]
fn cli_local_store_path_config_runs_data_and_archive() {
  let root = TempRoot::new("cli_store_path");
  let data = root.path().join("data");
  let ini = root.path().join("kv.ini");
  write(
    &ini,
    format!(
      "[run]\nnum_threads = 1\n[permanent]\nstore_path = {}\n",
      data.display()
    ),
  )
  .unwrap();
  let config = ini.to_str().unwrap();
  let put = run(
    root.path(),
    &[
      "--config", config, "put", "--table", "demo.1", "--key", "a", "--value", "A",
    ],
  );
  assert!(
    put.status.success(),
    "put stderr: {}",
    String::from_utf8_lossy(&put.stderr)
  );
  let get = run(
    root.path(),
    &["--config", config, "get", "--table", "demo.1", "--key", "a"],
  );
  assert!(
    get.status.success(),
    "get stderr: {}",
    String::from_utf8_lossy(&get.stderr)
  );
  assert_eq!(String::from_utf8_lossy(&get.stdout).trim(), "A");
  // 数据必须落在 `store_path`（不是 `--root` 的默认目录）。
  assert!(data.exists(), "store_path 未被用作数据根: {data:?}");
  let archive = run(root.path(), &["--config", config, "archive", "--tag", "t1"]);
  assert!(
    archive.status.success(),
    "archive stderr: {}",
    String::from_utf8_lossy(&archive.stderr)
  );
  // 归档后按 tag 重开，数据仍可读（数据面 / 控制面同一套身份）。
  let reopen = run(
    root.path(),
    &["--config", config, "reopen-archive", "--tag", "t1"],
  );
  assert!(
    reopen.status.success(),
    "reopen-archive stderr: {}",
    String::from_utf8_lossy(&reopen.stderr)
  );
  let get = run(
    root.path(),
    &["--config", config, "get", "--table", "demo.1", "--key", "a"],
  );
  assert!(get.status.success());
  assert_eq!(String::from_utf8_lossy(&get.stdout).trim(), "A");
}

/// 多根 `store_path` 同样走 Runtime 的 `RootedStore`：SDK 单根入口会拒绝，
/// 但 CLI 数据面不能因此不可用。
#[test]
fn cli_local_multi_root_store_path_put_get() {
  let root = TempRoot::new("cli_multi_root");
  let data_a = root.path().join("a");
  let data_b = root.path().join("b");
  let ini = root.path().join("kv.ini");
  write(
    &ini,
    format!(
      "[run]\nnum_threads = 1\n[permanent]\nstore_path = {},{}\n",
      data_a.display(),
      data_b.display()
    ),
  )
  .unwrap();
  let config = ini.to_str().unwrap();
  let put = run(
    root.path(),
    &[
      "--config", config, "put", "--table", "demo.1", "--key", "a", "--value", "A",
    ],
  );
  assert!(
    put.status.success(),
    "put stderr: {}",
    String::from_utf8_lossy(&put.stderr)
  );
  let get = run(
    root.path(),
    &["--config", config, "get", "--table", "demo.1", "--key", "a"],
  );
  assert!(
    get.status.success(),
    "get stderr: {}",
    String::from_utf8_lossy(&get.stderr)
  );
  assert_eq!(String::from_utf8_lossy(&get.stdout).trim(), "A");
}
