use std::{
  env::temp_dir,
  fs::{remove_file, write},
  process::id,
};

use fkv_config::KvOptions;
use fkv_error::KvError;
use fkv_types::StoreMode;

#[test]
fn default_is_local() {
  let opts = KvOptions::default();
  assert_eq!(opts.derive_mode(), StoreMode::Local);
  assert_eq!(opts.data_file_size(), 4096 * (1 << 18));
}

#[test]
fn cloud_mode_clears_standby() {
  let mut opts = KvOptions {
    cloud_store_path: "bucket/prefix/".to_owned(),
    enable_local_standby: true,
    standby_master_addr: "host".to_owned(),
    ..KvOptions::default()
  };
  opts.validate().unwrap();
  assert_eq!(opts.cloud_store_path, "bucket/prefix");
  assert_eq!(opts.derive_mode(), StoreMode::Cloud);
  assert!(!opts.enable_local_standby);
}

#[test]
fn standby_modes() {
  let mut opts = KvOptions {
    enable_local_standby: true,
    store_path: vec!["/tmp/fkv".to_owned()],
    ..KvOptions::default()
  };
  assert_eq!(opts.derive_mode(), StoreMode::StandbyMaster);
  opts.standby_master_addr = "host".to_owned();
  assert_eq!(opts.derive_mode(), StoreMode::StandbyReplica);
}

#[test]
fn invalid_options() {
  let mut opts = KvOptions {
    num_threads: 0,
    ..KvOptions::default()
  };
  assert_eq!(opts.validate().unwrap_err(), KvError::InvalidArgs);

  let mut opts = KvOptions {
    standby_master_addr: "host".to_owned(),
    ..KvOptions::default()
  };
  assert_eq!(opts.validate().unwrap_err(), KvError::InvalidArgs);
}

#[test]
fn load_ini() {
  let path = temp_dir().join(format!("fkv_options_{}.ini", id()));
  write(
        &path,
        "[run]\nnum_threads = 4\nbuffer_pool_size = 400MB\ndata_append_mode = true\n[permanent]\nstore_path = /tmp/a, /tmp/b\ndata_page_size = 4KB\ndata_file_size = 8MB\n",
    )
    .unwrap();
  let mut opts = KvOptions::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.num_threads, 4);
  assert_eq!(opts.buffer_pool_size, 400 * 1024 * 1024);
  assert!(opts.data_append_mode);
  assert_eq!(
    opts.store_path,
    vec!["/tmp/a".to_owned(), "/tmp/b".to_owned()]
  );
  assert_eq!(opts.data_page_size, 4096);
  assert_eq!(opts.pages_per_file_shift, 11);
  remove_file(path).unwrap();
}
