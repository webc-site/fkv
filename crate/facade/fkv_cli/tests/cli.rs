use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::{Command, Output, id},
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_cli_{}_{}_{seq}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos()
  ));
  create_dir_all(&root).unwrap();
  root
}

fn run(root: &PathBuf, args: &[&str]) -> Output {
  Command::new(env!("CARGO_BIN_EXE_fkv"))
    .arg("--root")
    .arg(root)
    .args(args)
    .output()
    .unwrap()
}

#[test]
fn cli_put_get_scan_floor_truncate() {
  let root = temp_root();
  assert!(
    run(
      &root,
      &["put", "--table", "demo.1", "--key", "a", "--value", "A"]
    )
    .status
    .success()
  );
  assert!(
    run(
      &root,
      &["put", "--table", "demo.1", "--key", "b", "--value", "B"]
    )
    .status
    .success()
  );
  let get = run(&root, &["get", "--table", "demo.1", "--key", "a"]);
  assert!(get.status.success());
  assert_eq!(String::from_utf8_lossy(&get.stdout).trim(), "A");
  let scan = run(&root, &["scan", "--table", "demo.1"]);
  assert!(scan.status.success());
  assert_eq!(String::from_utf8_lossy(&scan.stdout).lines().count(), 2);
  let floor = run(&root, &["floor", "--table", "demo.1", "--key", "c"]);
  assert!(floor.status.success());
  assert_eq!(String::from_utf8_lossy(&floor.stdout), "b\tB\n");
  assert!(
    run(&root, &["truncate", "--table", "demo.1", "--position", "b"])
      .status
      .success()
  );
  let scan = run(&root, &["scan", "--table", "demo.1"]);
  assert_eq!(String::from_utf8_lossy(&scan.stdout).lines().count(), 1);
  remove_dir_all(root).unwrap();
}
