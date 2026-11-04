use std::{
  cell::RefCell,
  env::temp_dir,
  fs::{create_dir_all, read, remove_dir_all, write},
  path::{Path, PathBuf},
  process::{Command, id},
  rc::Rc,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_config::KvOptions;
use fkv_error::KvError;
use fkv_io_compio::CompioStore;
use fkv_standby::{
  CommandRunner, DATA_PREFIX, LocalDirSource, RemoteSource, RsyncSource, SEGMENT_PREFIX,
  StandbyConfig, StandbyRole, StandbyService, StdCommandRunner, list_local_file_li,
  rsync::parse_rsync_list, table_dir, validate_manifest_bytes,
};
use fkv_store::Store;
use fkv_types::{StoreMode, TableIdent};

fn temp_root(tag: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_standby_{tag}_{}_{}_{seq}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos()
  ));
  create_dir_all(&root).unwrap();
  root
}

fn replica_config(local_root: &Path, master_root: &Path) -> StandbyConfig {
  StandbyConfig {
    role: StandbyRole::Replica,
    local_root: local_root.to_path_buf(),
    master_addr: String::new(),
    master_paths: vec![master_root.to_string_lossy().into_owned()],
    max_concurrency: 2,
  }
}

fn is_data_file(name: &str) -> bool {
  name.starts_with(DATA_PREFIX) || name.starts_with(SEGMENT_PREFIX)
}

async fn write_master(root: &Path, tbl: &TableIdent) {
  let mut store = Store::new(CompioStore::new(root, 512, 18), 512, 16);
  let batch = [
    BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
    BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
    BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 1),
  ];
  store.put(tbl, &batch).await.unwrap();
}

fn assert_same_files(master: &Path, replica: &Path, tbl: &TableIdent) {
  let master_dir = table_dir(master, tbl);
  let replica_dir = table_dir(replica, tbl);
  let file_li = list_local_file_li(&master_dir).unwrap();
  assert!(!file_li.is_empty());
  for file in file_li {
    let left = read(master_dir.join(&file.name)).unwrap();
    let right = read(replica_dir.join(&file.name)).unwrap();
    assert_eq!(left, right, "file {} differs", file.name);
  }
}

#[compio::test]
async fn replica_syncs_real_store() {
  let master = temp_root("master");
  let replica = temp_root("replica");
  let tbl = TableIdent::new("demo", 1).unwrap();
  write_master(&master, &tbl).await;

  let master_file_li = list_local_file_li(&table_dir(&master, &tbl)).unwrap();
  let data_num = master_file_li
    .iter()
    .filter(|file| is_data_file(&file.name))
    .count();
  assert!(data_num >= 1);

  let service = StandbyService::new(
    replica_config(&replica, &master),
    LocalDirSource::new(master.clone()),
  )
  .unwrap();
  let stats = service.sync_table(&tbl).await.unwrap();
  assert_eq!(stats.data_fetched, data_num);
  assert_eq!(stats.data_skipped, 0);
  assert!(stats.manifest_installed);
  assert_same_files(&master, &replica, &tbl);
  assert!(!replica.join(".standby").join("demo.1").exists());

  let mut store = Store::new(CompioStore::new(replica.clone(), 512, 18), 512, 16);
  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(store.get(&tbl, b"c").await.unwrap(), Some(b"C".to_vec()));

  let stats = service.sync_table(&tbl).await.unwrap();
  assert_eq!(stats.data_fetched, 0);
  assert_eq!(stats.data_skipped, data_num);
  assert!(stats.manifest_installed);

  remove_dir_all(master).unwrap();
  remove_dir_all(replica).unwrap();
}

#[compio::test]
async fn corrupt_manifest_is_not_installed() {
  let master = temp_root("corrupt_master");
  let replica = temp_root("corrupt_replica");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let master_dir = table_dir(&master, &tbl);
  create_dir_all(&master_dir).unwrap();
  write(master_dir.join("data_0_main_0"), vec![1u8; 512]).unwrap();
  write(master_dir.join("manifest_main_0"), vec![0u8; 4096]).unwrap();

  let service = StandbyService::new(
    replica_config(&replica, &master),
    LocalDirSource::new(master.clone()),
  )
  .unwrap();
  assert_eq!(
    service.sync_table(&tbl).await.unwrap_err(),
    KvError::Corrupted
  );
  let replica_dir = table_dir(&replica, &tbl);
  // data 先落地、manifest 最后安装；校验失败时不能出现 manifest。
  assert!(!replica_dir.join("manifest_main_0").exists());
  assert!(!replica.join(".standby").join("demo.1").exists());

  remove_dir_all(master).unwrap();
  remove_dir_all(replica).unwrap();
}

#[test]
fn role_from_mode_and_config_validation() {
  assert_eq!(
    StandbyRole::from_mode(StoreMode::StandbyMaster).unwrap(),
    StandbyRole::Master
  );
  assert_eq!(
    StandbyRole::from_mode(StoreMode::StandbyReplica).unwrap(),
    StandbyRole::Replica
  );
  assert_eq!(
    StandbyRole::from_mode(StoreMode::Local).unwrap_err(),
    KvError::InvalidArgs
  );
  assert_eq!(
    StandbyRole::from_mode(StoreMode::Cloud).unwrap_err(),
    KvError::InvalidArgs
  );

  let mut options = KvOptions {
    enable_local_standby: true,
    standby_master_addr: "master:22".to_owned(),
    standby_master_store_paths: vec!["/data/fkv".to_owned()],
    ..KvOptions::default()
  };
  let config = StandbyConfig::from_options(&options, "/tmp/replica").unwrap();
  assert_eq!(config.role, StandbyRole::Replica);
  assert_eq!(config.master_addr, "master:22");
  assert_eq!(config.master_paths, vec!["/data/fkv".to_owned()]);

  options.standby_master_addr.clear();
  let config = StandbyConfig::from_options(&options, "/tmp/master").unwrap();
  assert_eq!(config.role, StandbyRole::Master);

  let invalid = StandbyConfig {
    role: StandbyRole::Replica,
    local_root: PathBuf::new(),
    master_addr: String::new(),
    master_paths: Vec::new(),
    max_concurrency: 0,
  };
  assert_eq!(invalid.validate().unwrap_err(), KvError::InvalidArgs);
}

#[test]
fn master_export_lists_files() {
  let master = temp_root("export");
  let tbl = TableIdent::new("demo", 2).unwrap();
  let dir = table_dir(&master, &tbl);
  create_dir_all(&dir).unwrap();
  write(dir.join("manifest_main_0"), vec![0u8; 4096]).unwrap();
  write(dir.join("data_0_main_0"), b"abcd").unwrap();
  let config = StandbyConfig {
    role: StandbyRole::Master,
    local_root: master.clone(),
    master_addr: String::new(),
    master_paths: Vec::new(),
    max_concurrency: 2,
  };
  let service = StandbyService::new(config, LocalDirSource::new(master.clone())).unwrap();
  let file_li = service.export_table(&tbl).unwrap();
  assert_eq!(file_li.len(), 2);
  assert_eq!(file_li[0].name, "data_0_main_0");
  assert_eq!(file_li[0].size, 4);
  assert_eq!(file_li[1].name, "manifest_main_0");
  remove_dir_all(master).unwrap();
}

type CallLog = Rc<RefCell<Vec<(String, Vec<String>)>>>;

#[derive(Default)]
struct RecordingRunner {
  calls: CallLog,
  stdout: Vec<u8>,
}

impl CommandRunner for RecordingRunner {
  fn output(&self, program: &str, args: &[String]) -> Result<Vec<u8>, KvError> {
    self
      .calls
      .borrow_mut()
      .push((program.to_owned(), args.to_vec()));
    Ok(self.stdout.clone())
  }
}

#[test]
fn rsync_list_parse_and_ssh_command_args() {
  let stdout = b"drwxr-xr-x 160 2026/11/04 12:00:00 .\n\
-rw-r--r-- 1,234 2026/11/04 12:00:00 manifest_main_0\n\
-rw-r--r-- 512 2026/11/04 12:00:01 data_0_main_0\n\
-rw-r--r-- 512 2026/11/04 12:00:02 sub\n";
  let runner = RecordingRunner {
    calls: Rc::default(),
    stdout: stdout.to_vec(),
  };
  let calls = runner.calls.clone();
  let source = RsyncSource::new(runner, "/srv/fkv", "master:22");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let file_li = pollster::block_on(source.list(&tbl)).unwrap();
  assert_eq!(file_li.len(), 3);
  assert_eq!(file_li[0].name, "data_0_main_0");
  assert_eq!(file_li[0].size, 512);
  assert_eq!(file_li[1].name, "manifest_main_0");
  assert_eq!(file_li[1].size, 1234);
  assert!(file_li[0].modified > 0 && file_li[1].modified > 0);
  assert_ne!(file_li[0].modified, file_li[1].modified);
  let call_li = calls.borrow();
  assert_eq!(call_li[0].0, "ssh");
  assert_eq!(call_li[0].1[0], "master:22");
  assert_eq!(call_li[0].1[1], "rsync");
  assert_eq!(call_li[0].1[2], "--list-only");
  assert_eq!(call_li[0].1[3], "/srv/fkv/demo.1/");
  drop(call_li);

  let dst = Path::new("/tmp/stage/data_0_main_0");
  pollster::block_on(source.fetch(&tbl, "data_0_main_0", dst)).unwrap();
  let call_li = calls.borrow();
  assert_eq!(call_li[1].0, "rsync");
  assert_eq!(
    call_li[1].1,
    vec![
      "-a".to_owned(),
      "master:22:/srv/fkv/demo.1/data_0_main_0".to_owned(),
      "/tmp/stage/data_0_main_0".to_owned()
    ]
  );
}

#[test]
fn rsync_parse_rejects_unsafe_names() {
  let stdout = b"-rw-r--r-- 1 2026/11/04 12:00:00 ../escape\n\
-rw-r--r-- 1 2026/11/04 12:00:00 ok\n";
  let file_li = parse_rsync_list(stdout).unwrap();
  assert_eq!(file_li.len(), 1);
  assert_eq!(file_li[0].name, "ok");
}

#[test]
fn validate_manifest_rejects_torn_and_empty() {
  assert_eq!(
    validate_manifest_bytes(&[]).unwrap_err(),
    KvError::Corrupted
  );
  assert_eq!(
    validate_manifest_bytes(&[0u8; 100]).unwrap_err(),
    KvError::Corrupted
  );
}

#[compio::test]
async fn rsync_local_source_syncs_real_store() {
  if Command::new("rsync")
    .arg("--version")
    .output()
    .map_or(true, |output| !output.status.success())
  {
    eprintln!("rsync not available, skip");
    return;
  }
  let master = temp_root("rsync_master");
  let replica = temp_root("rsync_replica");
  let tbl = TableIdent::new("demo", 1).unwrap();
  write_master(&master, &tbl).await;

  let source = RsyncSource::new(StdCommandRunner, master.to_string_lossy().into_owned(), "");
  let service = StandbyService::new(replica_config(&replica, &master), source).unwrap();
  let stats = service.sync_table(&tbl).await.unwrap();
  assert!(stats.data_fetched >= 1);
  assert!(stats.manifest_installed);
  assert_same_files(&master, &replica, &tbl);

  let mut store = Store::new(CompioStore::new(replica.clone(), 512, 18), 512, 16);
  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));

  remove_dir_all(master).unwrap();
  remove_dir_all(replica).unwrap();
}
