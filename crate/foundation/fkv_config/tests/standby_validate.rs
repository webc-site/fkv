//! sem-17：standby replica 的 master 配置校验。
//!
//! 对齐 C++ `ValidateOptions`（`eloqstore/src/eloq_store.cpp:251-297`）：
//! master 地址只能是 `local` 或 `username@addr`，master 路径必须为绝对路径。
//! master 路径数量与本地 `store_path` 解耦（cld-58 独立 LUT，runtime 只支持
//! 单本地根 + N 个 master 根），不套用 C++ 的等长约束。

use std::fs::write;

use fkv_config::KvOption;
use fkv_error::KvError;
use fkv_testkit::TempRoot;
use fkv_types::StoreMode;

fn replica_options(addr: &str, master_path_li: &[&str]) -> KvOption {
  KvOption {
    store_path: vec!["/data/replica".to_owned()],
    enable_local_standby: true,
    standby_master_addr: addr.to_owned(),
    standby_master_store_paths: master_path_li
      .iter()
      .map(|path| (*path).to_owned())
      .collect(),
    ..KvOption::default()
  }
}

#[test]
fn valid_master_config_passes() {
  for addr in ["local", "LOCAL", "user@master"] {
    let mut opts = replica_options(addr, &["/data/master"]);
    opts.validate().unwrap();
  }
  // 单本地根 + 多 master 根（cld-58）：数量不同仍合法，master 根按独立 LUT
  // 选路；validate 只告警，由 master 部署的实际根数决定。
  let mut opts = replica_options("local", &["/data/m1", "/data/m2"]);
  opts.validate().unwrap();
  assert_eq!(opts.standby_master_store_path_lut_li, vec![0, 1]);
}

#[test]
fn master_addr_must_be_local_or_username_at_addr() {
  // C++ 只接受 `local` 或 `username@addr`；`host` / `host:22` 能通过旧校验，
  // 却会在首次同步才被 ssh / rsync 拒绝。
  for addr in ["host", "master:22", "master:22/dir"] {
    let mut opts = replica_options(addr, &["/data/master"]);
    assert_eq!(
      opts.validate().unwrap_err(),
      KvError::InvalidArgs,
      "addr = {addr}"
    );
  }
}

#[test]
fn master_paths_must_be_absolute() {
  for path in ["data/master", "./master", "~/master", ""] {
    let mut opts = replica_options("local", &[path]);
    assert_eq!(
      opts.validate().unwrap_err(),
      KvError::InvalidArgs,
      "path = {path:?}"
    );
  }
  // 远程 master 同样拒绝相对路径（会按远端登录 shell 的 cwd 解释）。
  let mut opts = replica_options("user@master", &["relative/master"]);
  assert_eq!(opts.validate().unwrap_err(), KvError::InvalidArgs);
}

#[test]
fn replica_requires_master_store_paths() {
  let mut opts = replica_options("user@master", &[]);
  assert_eq!(opts.validate().unwrap_err(), KvError::InvalidArgs);
}

/// INI 解析后进入同一校验：非 C++ 形式不能在 `validate` 下静默通过。
#[test]
fn ini_master_config_is_validated() {
  let root = TempRoot::new("fkv_standby_validate");
  let path = root.join("config.ini");
  let load = |text: &str| {
    write(&path, text).unwrap();
    let mut opts = KvOption::default();
    opts.load_from_ini(&path).unwrap();
    opts
  };

  let mut opts = load(
    "[run]\nnum_threads = 1\n[permanent]\nstore_path = /data/replica\nenable_local_standby = true\nstandby_master_addr = host\nstandby_master_store_paths = /data/master\n",
  );
  assert_eq!(opts.validate().unwrap_err(), KvError::InvalidArgs);

  let mut opts = load(
    "[run]\nnum_threads = 1\n[permanent]\nstore_path = /data/replica\nenable_local_standby = true\nstandby_master_addr = local\nstandby_master_store_paths = data/master\n",
  );
  assert_eq!(opts.validate().unwrap_err(), KvError::InvalidArgs);

  let mut opts = load(
    "[run]\nnum_threads = 1\n[permanent]\nstore_path = /data/replica\nenable_local_standby = true\nstandby_master_addr = user@master\nstandby_master_store_paths = /data/master\n",
  );
  opts.validate().unwrap();
  assert_eq!(opts.derive_mode(), StoreMode::StandbyReplica);
}
