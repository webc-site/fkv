use std::{
  fs::{File, create_dir_all, read, rename, write},
  path::{Path, PathBuf},
  process::Command,
  sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
  },
  time::SystemTime,
};

use fkv_btree::BatchEntry;
use fkv_config::KvOption;
use fkv_error::KvError;
use fkv_io_compio::CompioStore;
use fkv_standby::{
  CommandRunner, LocalDirSource, RemoteFile, RemoteSource, RsyncSource, StandbyConfig, StandbyRole,
  StandbyService, StdCommandRunner, check_manifest_term, list_local_file_li, list_table_li,
  rsync::{CommandOutput, RsyncShell, parse_rsync_list},
  select_manifest, table_dir, validate_manifest_bytes,
};
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::{DATA_FILE_PREFIX, SEGMENT_FILE_PREFIX, StoreMode, TableIdent};

fn replica_config(local_root: &Path, master_root: &Path) -> StandbyConfig {
  StandbyConfig {
    role: StandbyRole::Replica,
    local_root: local_root.to_path_buf(),
    master_addr: String::new(),
    master_path_li: vec![master_root.to_string_lossy().into_owned()],
    max_concurrency: 2,
    target_term: 0,
  }
}

fn is_data_file(name: &str) -> bool {
  name.starts_with(DATA_FILE_PREFIX) || name.starts_with(SEGMENT_FILE_PREFIX)
}

async fn write_master(root: &Path, tbl: &TableIdent) {
  let mut store = Store::new(
    CompioStore::new(root, 512, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT),
    512,
    16,
  );
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

/// 记录调用次数：cld-45 要证明「每分区一次传输」而不是「每文件一次」。
#[derive(Clone, Default)]
struct CallCount {
  list: Arc<AtomicU64>,
  sync_dir: Arc<AtomicU64>,
  fetch: Arc<AtomicU64>,
}

impl CallCount {
  fn snapshot(&self) -> (u64, u64, u64) {
    (
      self.list.load(Ordering::Relaxed),
      self.sync_dir.load(Ordering::Relaxed),
      self.fetch.load(Ordering::Relaxed),
    )
  }
}

struct CountingSource<S> {
  inner: S,
  count: CallCount,
}

impl<S: RemoteSource> RemoteSource for CountingSource<S> {
  async fn list(&self, tbl: &TableIdent) -> Result<Vec<RemoteFile>, KvError> {
    self.count.list.fetch_add(1, Ordering::Relaxed);
    self.inner.list(tbl).await
  }

  async fn sync_dir(&self, tbl: &TableIdent, dst: &Path, compare: &Path) -> Result<(), KvError> {
    self.count.sync_dir.fetch_add(1, Ordering::Relaxed);
    self.inner.sync_dir(tbl, dst, compare).await
  }

  async fn fetch(&self, tbl: &TableIdent, name: &str, dst: &Path) -> Result<(), KvError> {
    self.count.fetch.fetch_add(1, Ordering::Relaxed);
    self.inner.fetch(tbl, name, dst).await
  }

  async fn list_table_li(&self) -> Result<Vec<TableIdent>, KvError> {
    self.inner.list_table_li().await
  }
}

/// 首次 `sync_dir` 后改写 master manifest，模拟同步期间 master 切换 manifest。
struct ManifestMoveSource {
  inner: LocalDirSource,
  master_root: PathBuf,
  moved: Arc<AtomicU64>,
  tbl: TableIdent,
}

impl RemoteSource for ManifestMoveSource {
  async fn list(&self, tbl: &TableIdent) -> Result<Vec<RemoteFile>, KvError> {
    self.inner.list(tbl).await
  }

  async fn sync_dir(&self, tbl: &TableIdent, dst: &Path, compare: &Path) -> Result<(), KvError> {
    self.inner.sync_dir(tbl, dst, compare).await?;
    if self.moved.fetch_add(1, Ordering::Relaxed) == 0 {
      let path = table_dir(&self.master_root, &self.tbl).join("manifest_main_0");
      let mut bytes = read(&path).unwrap();
      bytes.push(0);
      write(&path, bytes).unwrap();
    }
    Ok(())
  }

  async fn fetch(&self, tbl: &TableIdent, name: &str, dst: &Path) -> Result<(), KvError> {
    self.inner.fetch(tbl, name, dst).await
  }

  async fn list_table_li(&self) -> Result<Vec<TableIdent>, KvError> {
    self.inner.list_table_li().await
  }
}

/// 一轮同步只起一次分区级传输 + 一次 manifest fetch，与 data 文件数无关；
/// list 两次（同步前取 data 清单、安装前复核 manifest 版本）。
#[compio::test]
async fn one_partition_transfer_per_sync() {
  let master = TempRoot::new("standby_count_master");
  let replica = TempRoot::new("standby_count_replica");
  let tbl = TableIdent::new("demo", 1).unwrap();
  write_master(master.path(), &tbl).await;
  let master_dir = table_dir(master.path(), &tbl);
  // 额外灌 5 个 data 文件：进程 / 会话数不应随文件数增长。
  for file_id in 100..105 {
    write(
      master_dir.join(format!("data_{file_id}_main_0")),
      vec![7u8; 512],
    )
    .unwrap();
  }

  let count = CallCount::default();
  let source = CountingSource {
    inner: LocalDirSource::new(master.path().to_path_buf()),
    count: count.clone(),
  };
  let service = StandbyService::new(replica_config(replica.path(), master.path()), source).unwrap();
  let stats = service.sync_table(&tbl).await.unwrap();
  assert!(
    stats.data_fetched >= 6,
    "data_fetched={}",
    stats.data_fetched
  );
  assert!(stats.manifest_installed);
  assert_eq!(count.snapshot(), (2, 1, 1));
}

/// manifest 在数据同步期间被改写：安装前复核失败，返回 `TryAgain` 且不落盘。
#[compio::test]
async fn manifest_move_during_sync_retries_before_install() {
  let master = TempRoot::new("standby_move_master");
  let replica = TempRoot::new("standby_move_replica");
  let tbl = TableIdent::new("demo", 1).unwrap();
  write_master(master.path(), &tbl).await;
  let source = ManifestMoveSource {
    inner: LocalDirSource::new(master.path().to_path_buf()),
    master_root: master.path().to_path_buf(),
    moved: Arc::new(AtomicU64::new(0)),
    tbl: tbl.clone(),
  };
  let service = StandbyService::new(replica_config(replica.path(), master.path()), source).unwrap();
  assert!(matches!(
    service.sync_table(&tbl).await,
    Err(KvError::TryAgain)
  ));
  // 本地活跃 manifest 未安装（staging 已清理）。
  assert!(read(table_dir(replica.path(), &tbl).join("manifest_main_0")).is_err());
}

/// `sync_all` 按 `max_concurrency` 分批并发追赶多个分区。
#[compio::test]
async fn sync_all_catches_up_every_partition() {
  let master = TempRoot::new("standby_all_master");
  let replica = TempRoot::new("standby_all_replica");
  let tbl_li: Vec<TableIdent> = (1..=3)
    .map(|partition| TableIdent::new("demo", partition).unwrap())
    .collect();
  for tbl in &tbl_li {
    write_master(master.path(), tbl).await;
  }

  let mut config = replica_config(replica.path(), master.path());
  config.max_concurrency = 2;
  let service =
    StandbyService::new(config, LocalDirSource::new(master.path().to_path_buf())).unwrap();
  let stats_li = service.sync_all(&tbl_li).await.unwrap();
  assert_eq!(stats_li.len(), tbl_li.len());
  assert!(stats_li.iter().all(|stats| stats.manifest_installed));
  for tbl in &tbl_li {
    assert_same_files(master.path(), replica.path(), tbl);
    assert!(
      !replica
        .join(".standby")
        .join(format!("{}.{}", tbl.tbl_name, tbl.partition_id))
        .exists()
    );
  }

  let mut store = Store::new(
    CompioStore::new(
      replica.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store.reopen(&tbl_li[2]).await.unwrap();
  assert_eq!(
    store.get(&tbl_li[2], b"b").await.unwrap(),
    Some(b"B".to_vec())
  );
}

#[compio::test]
async fn replica_syncs_real_store() {
  let master = TempRoot::new("standby_master");
  let replica = TempRoot::new("standby_replica");
  let tbl = TableIdent::new("demo", 1).unwrap();
  write_master(master.path(), &tbl).await;

  let master_file_li = list_local_file_li(&table_dir(master.path(), &tbl)).unwrap();
  let data_num = master_file_li
    .iter()
    .filter(|file| is_data_file(&file.name))
    .count();
  assert!(data_num >= 1);

  let service = StandbyService::new(
    replica_config(replica.path(), master.path()),
    LocalDirSource::new(master.path().to_path_buf()),
  )
  .unwrap();
  let stats = service.sync_table(&tbl).await.unwrap();
  assert_eq!(stats.data_fetched, data_num);
  assert_eq!(stats.data_skipped, 0);
  assert!(stats.manifest_installed);
  assert_same_files(master.path(), replica.path(), &tbl);
  assert!(!replica.join(".standby").join("demo.1").exists());

  let mut store = Store::new(
    CompioStore::new(
      replica.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(store.get(&tbl, b"c").await.unwrap(), Some(b"C".to_vec()));

  let stats = service.sync_table(&tbl).await.unwrap();
  assert_eq!(stats.data_fetched, 0);
  assert_eq!(stats.data_skipped, data_num);
  assert!(stats.manifest_installed);
}

/// term 必须按数值比较，且排除 archive / `.tmp`。
#[test]
fn select_manifest_uses_numeric_term_and_skips_archive() {
  let file = |name: &str, modified: u64| fkv_standby::RemoteFile {
    name: name.to_owned(),
    size: 4096,
    modified,
  };
  let file_li = vec![
    file("manifest_main_9", 10),
    file("manifest_main_10", 10),
    file("manifest_main_11_archive", 20),
    file("manifest_main_12.tmp", 30),
    file("manifest_dev_99", 40),
  ];
  let selected = select_manifest(&file_li, "main").unwrap();
  assert_eq!(selected.name, "manifest_main_10");
}

/// cld-44：replica 拒绝 term 高于自身的 manifest；archive 不受限。
#[test]
fn manifest_term_fencing() {
  assert!(check_manifest_term("manifest_main_7", 7).is_ok());
  assert!(check_manifest_term("manifest_main_7", 0).is_ok());
  assert_eq!(
    check_manifest_term("manifest_main_8", 7).unwrap_err(),
    KvError::ExpiredTerm
  );
  assert!(check_manifest_term("manifest_main_9_archive", 7).is_ok());
}

#[compio::test]
async fn corrupt_manifest_is_not_installed() {
  let master = TempRoot::new("standby_corrupt_master");
  let replica = TempRoot::new("standby_corrupt_replica");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let master_dir = table_dir(master.path(), &tbl);
  create_dir_all(&master_dir).unwrap();
  write(master_dir.join("data_0_main_0"), vec![1u8; 512]).unwrap();
  write(master_dir.join("manifest_main_0"), vec![0u8; 4096]).unwrap();

  let service = StandbyService::new(
    replica_config(replica.path(), master.path()),
    LocalDirSource::new(master.path().to_path_buf()),
  )
  .unwrap();
  assert_eq!(
    service.sync_table(&tbl).await.unwrap_err(),
    KvError::Corrupted
  );
  let replica_dir = table_dir(replica.path(), &tbl);
  // data 先落地、manifest 最后安装；校验失败时不能出现 manifest。
  assert!(!replica_dir.join("manifest_main_0").exists());
  assert!(!replica.join(".standby").join("demo.1").exists());
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

  let mut options = KvOption {
    enable_local_standby: true,
    // C++ `ValidateOptions` 只接受 `local` 或 `username@addr`；这里用合法形式
    // 构造，避免测试固化非 C++ 地址（`master:22` 会被 ssh 当成含端口的主机名）。
    standby_master_addr: "user@master".to_owned(),
    standby_master_store_paths: vec!["/data/fkv".to_owned()],
    ..KvOption::default()
  };
  let config = StandbyConfig::from_option(&options, "/tmp/replica").unwrap();
  assert_eq!(config.role, StandbyRole::Replica);
  assert_eq!(config.master_addr, "user@master");
  assert_eq!(config.master_path_li, vec!["/data/fkv".to_owned()]);

  // 双开关同时置位且未 validate：standby 优先（对齐 C++ `DeriveStoreMode`），
  // 不能误判成 Cloud 后让 from_mode 返回 InvalidArgs。
  options.cloud_store_path = "bucket/prefix".to_owned();
  let config = StandbyConfig::from_option(&options, "/tmp/replica").unwrap();
  assert_eq!(config.role, StandbyRole::Replica);
  options.cloud_store_path.clear();

  options.standby_master_addr.clear();
  let config = StandbyConfig::from_option(&options, "/tmp/master").unwrap();
  assert_eq!(config.role, StandbyRole::Master);

  let invalid = StandbyConfig {
    role: StandbyRole::Replica,
    local_root: PathBuf::new(),
    master_addr: String::new(),
    master_path_li: Vec::new(),
    max_concurrency: 0,
    target_term: 0,
  };
  assert_eq!(invalid.validate().unwrap_err(), KvError::InvalidArgs);
}

#[test]
fn master_export_lists_files() {
  let master = TempRoot::new("standby_export");
  let tbl = TableIdent::new("demo", 2).unwrap();
  let dir = table_dir(master.path(), &tbl);
  create_dir_all(&dir).unwrap();
  write(dir.join("manifest_main_0"), vec![0u8; 4096]).unwrap();
  write(dir.join("data_0_main_0"), b"abcd").unwrap();
  let config = StandbyConfig {
    role: StandbyRole::Master,
    local_root: master.path().to_path_buf(),
    master_addr: String::new(),
    master_path_li: Vec::new(),
    max_concurrency: 2,
    target_term: 0,
  };
  let service =
    StandbyService::new(config, LocalDirSource::new(master.path().to_path_buf())).unwrap();
  let file_li = service.export_table(&tbl).unwrap();
  assert_eq!(file_li.len(), 2);
  assert_eq!(file_li[0].name, "data_0_main_0");
  assert_eq!(file_li[0].size, 4);
  assert_eq!(file_li[1].name, "manifest_main_0");
}

type CallLog = Arc<Mutex<Vec<(String, Vec<String>)>>>;

#[derive(Default)]
struct RecordingRunner {
  calls: CallLog,
  stdout: Vec<u8>,
}

impl CommandRunner for RecordingRunner {
  fn output(&self, program: &str, args: &[String]) -> Result<CommandOutput, KvError> {
    self
      .calls
      .lock()
      .unwrap()
      .push((program.to_owned(), args.to_vec()));
    Ok(CommandOutput {
      code: Some(0),
      stdout: self.stdout.clone(),
    })
  }
}

/// cld-58：多根 master 按 LUT 选根（`partition_id % lut.len()`）；单根恒用唯一根。
#[test]
fn rsync_source_selects_master_root_by_lut() {
  let runner = RecordingRunner::default();
  let calls = runner.calls.clone();
  let source = RsyncSource::new_rooted(
    runner,
    vec!["/m0".to_owned(), "/m1".to_owned()],
    vec![0, 1],
    "master:22",
  )
  .unwrap();
  let tbl_odd = TableIdent::new("demo", 1).unwrap();
  let tbl_even = TableIdent::new("demo", 2).unwrap();
  pollster::block_on(source.list(&tbl_odd)).unwrap();
  pollster::block_on(source.list(&tbl_even)).unwrap();
  let call_li = calls.lock().unwrap();
  // 分区 1 -> LUT[1] = 1 -> /m1；分区 2 -> LUT[0] = 0 -> /m0。
  assert_eq!(call_li[0].1[3], "/m1/demo.1/");
  assert_eq!(call_li[1].1[3], "/m0/demo.2/");
  drop(call_li);

  // 非等权 LUT（1:2，LUT = [0, 1, 1]）：分区 2 -> LUT[2] = 1 -> /w1。
  // 若误用 `partition_id % master_root_li.len()` 会得到 /w0，这条能区分。
  let runner = RecordingRunner::default();
  let calls = runner.calls.clone();
  let source = RsyncSource::new_rooted(
    runner,
    vec!["/w0".to_owned(), "/w1".to_owned()],
    vec![0, 1, 1],
    "master:22",
  )
  .unwrap();
  pollster::block_on(source.list(&tbl_even)).unwrap();
  let call_li = calls.lock().unwrap();
  assert_eq!(call_li[0].1[3], "/w1/demo.2/");
  drop(call_li);

  // 单根保持现行为：LUT = [0]，任何分区都落唯一根。
  let runner = RecordingRunner::default();
  let calls = runner.calls.clone();
  let source = RsyncSource::new(runner, "/only", "master:22");
  pollster::block_on(source.list(&tbl_odd)).unwrap();
  let call_li = calls.lock().unwrap();
  assert_eq!(call_li[0].1[3], "/only/demo.1/");
}

/// 多根配置在构造期拒绝空根 / 空 LUT / 越界 LUT，不在运行期静默退回 root[0]。
#[test]
fn rsync_source_rejects_invalid_master_lut() {
  for (root_li, lut_li) in [
    (Vec::new(), vec![0]),
    (vec!["/m0".to_owned()], Vec::new()),
    (vec!["/m0".to_owned()], vec![1]),
  ] {
    match RsyncSource::new_rooted(RecordingRunner::default(), root_li, lut_li, "master:22") {
      Ok(_) => panic!("invalid master root lut must be rejected"),
      Err(err) => assert_eq!(err, KvError::InvalidArgs),
    }
  }
}

/// 表发现合并全部 master 根（按 `(表名, 分区号)` 定序）。
#[compio::test]
async fn rsync_source_lists_tables_across_master_roots() {
  let master_a = TempRoot::new("standby_multi_list_a");
  let master_b = TempRoot::new("standby_multi_list_b");
  let tbl_a = TableIdent::new("demo", 2).unwrap();
  let tbl_b = TableIdent::new("demo", 1).unwrap();
  create_dir_all(table_dir(master_a.path(), &tbl_a)).unwrap();
  create_dir_all(table_dir(master_b.path(), &tbl_b)).unwrap();
  let source = RsyncSource::new_rooted(
    StdCommandRunner,
    vec![
      master_a.path().to_string_lossy().into_owned(),
      master_b.path().to_string_lossy().into_owned(),
    ],
    vec![0, 1],
    "",
  )
  .unwrap();
  let tbl_li = source.list_table_li().await.unwrap();
  assert_eq!(tbl_li, vec![tbl_b, tbl_a]);
}

#[test]
fn rsync_list_parse_and_ssh_command_args() {
  let stdout = b"drwxr-xr-x 160 2026/11/04 12:00:00 .\n\
-rw-r--r-- 1,234 2026/11/04 12:00:00 manifest_main_0\n\
-rw-r--r-- 512 2026/11/04 12:00:01 data_0_main_0\n\
-rw-r--r-- 512 2026/11/04 12:00:02 sub\n";
  let runner = RecordingRunner {
    calls: Arc::default(),
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
  let call_li = calls.lock().unwrap();
  assert_eq!(call_li[0].0, "ssh");
  assert_eq!(call_li[0].1[0], "master:22");
  assert_eq!(call_li[0].1[1], "rsync");
  assert_eq!(call_li[0].1[2], "--list-only");
  assert_eq!(call_li[0].1[3], "/srv/fkv/demo.1/");
  drop(call_li);

  let dst = Path::new("/tmp/stage/data_0_main_0");
  pollster::block_on(source.fetch(&tbl, "data_0_main_0", dst)).unwrap();
  let call_li = calls.lock().unwrap();
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

/// cld-45：分区级同步只起一条 rsync 命令
/// （含 `--inplace` / `--checksum` / `--compare-dest`）。
#[test]
fn rsync_sync_dir_command_shape() {
  let runner = RecordingRunner::default();
  let calls = runner.calls.clone();
  let source = RsyncSource::new(runner, "/srv/fkv", "master:22");
  let tbl = TableIdent::new("demo", 1).unwrap();
  pollster::block_on(source.sync_dir(&tbl, Path::new("/tmp/stage"), Path::new("/tmp/local")))
    .unwrap();
  let call_li = calls.lock().unwrap();
  assert_eq!(call_li.len(), 1);
  assert_eq!(call_li[0].0, "rsync");
  assert_eq!(
    call_li[0].1,
    vec![
      "-a".to_owned(),
      "--inplace".to_owned(),
      "--checksum".to_owned(),
      "--compare-dest=/tmp/local/".to_owned(),
      "--include=data_*".to_owned(),
      "--include=segment_*".to_owned(),
      "--exclude=*".to_owned(),
      "master:22:/srv/fkv/demo.1/".to_owned(),
      "/tmp/stage/".to_owned(),
    ]
  );
}

/// sem-21：可注入退出码的执行器，验证 `CommandRunner` 不再丢弃子进程状态。
struct ExitCodeRunner {
  code: Option<i32>,
  stdout: Vec<u8>,
}

impl CommandRunner for ExitCodeRunner {
  fn output(&self, _program: &str, _args: &[String]) -> Result<CommandOutput, KvError> {
    Ok(CommandOutput {
      code: self.code,
      stdout: self.stdout.clone(),
    })
  }
}

/// sem-21：rsync 退出码映射对齐 C++ `InterpretRsyncStatus`。
///
/// 23 / 24 是远端源目录 / 文件缺失（master 已删分区），必须成为 `NotFound`
/// 才能触发 standby 清理；0 成功；1 / 255（ssh 连接失败）/ 信号仍是 `IoFail`。
/// `ls` 走的 `InterpretCommandStatus` 不参与 23/24 的语义，任何非零都是 `IoFail`。
#[test]
fn rsync_exit_code_mapping_matches_cpp() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let shell_of = |code| {
    RsyncShell::new(
      ExitCodeRunner {
        code,
        stdout: Vec::new(),
      },
      "user@master",
    )
  };
  for (code, expected) in [
    (Some(0), Ok(())),
    (Some(23), Err(KvError::NotFound)),
    (Some(24), Err(KvError::NotFound)),
    (Some(1), Err(KvError::IoFail)),
    (Some(255), Err(KvError::IoFail)),
    (None, Err(KvError::IoFail)),
  ] {
    let shell = shell_of(code);
    assert_eq!(
      shell.list("/srv/fkv/demo.1").map(|_| ()),
      expected,
      "list code={code:?}"
    );
    assert_eq!(
      shell
        .sync_dir(
          "/srv/fkv/demo.1",
          Path::new("/tmp/dst"),
          Path::new("/tmp/cmp")
        )
        .map(|_| ()),
      expected,
      "sync_dir code={code:?}"
    );
    assert_eq!(
      shell
        .fetch("/srv/fkv/demo.1/manifest_main_0", Path::new("/tmp/m.tmp"))
        .map(|_| ()),
      expected,
      "fetch code={code:?}"
    );
    assert_eq!(
      shell
        .verify_same("/srv/fkv/demo.1/manifest_main_0", Path::new("/tmp/local_m"))
        .map(|_| ()),
      expected,
      "verify_same code={code:?}"
    );
    // `ls` 是独立命令：23 也不能当成 NotFound（C++ `InterpretCommandStatus`）。
    assert_eq!(
      shell.list_table_li("/srv/fkv").map(|_| ()),
      match code {
        Some(0) => Ok(()),
        _ => Err(KvError::IoFail),
      },
      "list_table_li code={code:?}"
    );
    // 本机 rsync 与 ssh 分支同一映射。
    let local = RsyncShell::new(
      ExitCodeRunner {
        code,
        stdout: Vec::new(),
      },
      "",
    );
    assert_eq!(
      local
        .list(&format!("/srv/fkv/{}", tbl.dir_name()))
        .map(|_| ()),
      expected,
      "local list code={code:?}"
    );
  }
}

/// sem-21：ssh 远端 list 返回 23 时，`sync_table_with_term` 必须把
/// 「master 已删分区」暴露为 `NotFound`（上层 `sync_standby_partition`
/// 转 `ResourceMissing` 并清本地状态）；1 / 255 仍是 `IoFail`。
#[compio::test]
async fn ssh_missing_source_maps_to_not_found() {
  let replica = TempRoot::new("standby_sem21_replica");
  let tbl = TableIdent::new("demo", 1).unwrap();
  for (code, expected) in [
    (23, KvError::NotFound),
    (24, KvError::NotFound),
    (1, KvError::IoFail),
    (255, KvError::IoFail),
  ] {
    let source = RsyncSource::new(
      ExitCodeRunner {
        code: Some(code),
        stdout: Vec::new(),
      },
      "/srv/master",
      "user@master",
    );
    let config = replica_config(replica.path(), Path::new("/srv/master"));
    let service = StandbyService::new(config, source).unwrap();
    assert_eq!(
      service.sync_table_with_term(&tbl, 1).await.unwrap_err(),
      expected,
      "rsync exit {code}"
    );
  }
}

/// 本地源同样不能只比 `(size, mtime)`：segment 定长，同尺寸同秒原地覆写
/// 也必须按内容判定为已变更并重传。
#[test]
fn local_source_retransfers_same_size_segment_overwrite() {
  let root = TempRoot::new("standby-seg-checksum");
  let src_root = root.path().join("src");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let src = table_dir(&src_root, &tbl);
  let dst = root.path().join("dst");
  let compare = root.path().join("compare");
  for dir in [&src, &dst, &compare] {
    create_dir_all(dir).unwrap();
  }
  let name = "segment_0_main_0";
  write(src.join(name), vec![1u8; 4096]).unwrap();
  write(dst.join(name), vec![0u8; 4096]).unwrap();
  // compare 与源同尺寸、mtime 不旧，但内容不同。
  write(compare.join(name), vec![2u8; 4096]).unwrap();
  let now = SystemTime::now();
  for dir in [&src, &compare] {
    let file = File::options().write(true).open(dir.join(name)).unwrap();
    file.set_modified(now).unwrap();
  }

  let source = LocalDirSource::new(&src_root);
  pollster::block_on(source.sync_dir(&tbl, &dst, &compare)).unwrap();
  assert_eq!(read(dst.join(name)).unwrap(), vec![1u8; 4096]);
}

/// manifest 安装前的内容复核：`--checksum --dry-run` 无输出即一致。
#[test]
fn rsync_verify_same_uses_checksum_dry_run() {
  let runner = RecordingRunner::default();
  let calls = runner.calls.clone();
  let source = RsyncSource::new(runner, "/srv/fkv", "master:22");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let local = Path::new("/tmp/stage/manifest_main_0.tmp");
  assert!(pollster::block_on(source.verify_same(&tbl, "manifest_main_0", local)).unwrap());
  let call_li = calls.lock().unwrap();
  assert_eq!(call_li[0].0, "rsync");
  assert_eq!(
    call_li[0].1,
    vec![
      "-a".to_owned(),
      "--checksum".to_owned(),
      "--dry-run".to_owned(),
      "--itemize-changes".to_owned(),
      "master:22:/srv/fkv/demo.1/manifest_main_0".to_owned(),
      "/tmp/stage/manifest_main_0.tmp".to_owned(),
    ]
  );
}

/// openrsync 的 `--dry-run` 可能只按 mtime 报「待传输」；此时必须再抓一次
/// 远端逐字节确认，不能把同内容 manifest 永久判成 `TryAgain`。
#[test]
fn rsync_verify_same_confirms_itemized_difference_by_content() {
  struct ItemizeRunner {
    calls: CallLog,
    payload: Vec<u8>,
  }

  impl CommandRunner for ItemizeRunner {
    fn output(&self, program: &str, args: &[String]) -> Result<CommandOutput, KvError> {
      self
        .calls
        .lock()
        .unwrap()
        .push((program.to_owned(), args.to_vec()));
      if args.iter().any(|arg| arg == "--dry-run") {
        return Ok(CommandOutput {
          code: Some(0),
          stdout: b">f+++++++++ manifest_main_0\n".to_vec(),
        });
      }
      let dst = args.last().expect("fetch destination");
      write(dst, &self.payload).unwrap();
      Ok(CommandOutput {
        code: Some(0),
        stdout: Vec::new(),
      })
    }
  }

  let dir = TempRoot::new("rsync_verify_content");
  let local = dir.path().join("manifest_main_0.tmp");
  write(&local, b"same").unwrap();
  let runner = ItemizeRunner {
    calls: CallLog::default(),
    payload: b"same".to_vec(),
  };
  let source = RsyncSource::new(runner, "/srv/fkv", "master:22");
  let tbl = TableIdent::new("demo", 1).unwrap();
  assert!(pollster::block_on(source.verify_same(&tbl, "manifest_main_0", &local)).unwrap());

  let runner = ItemizeRunner {
    calls: CallLog::default(),
    payload: b"changed".to_vec(),
  };
  let source = RsyncSource::new(runner, "/srv/fkv", "master:22");
  assert!(!pollster::block_on(source.verify_same(&tbl, "manifest_main_0", &local)).unwrap());
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
  let master = TempRoot::new("standby_rsync_master");
  let replica = TempRoot::new("standby_rsync_replica");
  let tbl = TableIdent::new("demo", 1).unwrap();
  write_master(master.path(), &tbl).await;

  let source = RsyncSource::new(
    StdCommandRunner,
    master.path().to_string_lossy().into_owned(),
    "",
  );
  let service = StandbyService::new(replica_config(replica.path(), master.path()), source).unwrap();
  let stats = service.sync_table(&tbl).await.unwrap();
  assert!(stats.data_fetched >= 1);
  assert!(stats.manifest_installed);
  assert_same_files(master.path(), replica.path(), &tbl);

  // `--compare-dest` 命中：第二轮不应再传 data。
  let data_num = list_local_file_li(&table_dir(master.path(), &tbl))
    .unwrap()
    .iter()
    .filter(|file| is_data_file(&file.name))
    .count();
  let stats = service.sync_table(&tbl).await.unwrap();
  assert_eq!(stats.data_fetched, 0);
  assert_eq!(stats.data_skipped, data_num);

  let mut store = Store::new(
    CompioStore::new(
      replica.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
}

/// 一轮 `catch_up` 自己发现 master 上所有表并全部同步（含 replica 还没有的表）。
#[compio::test]
async fn catch_up_discovers_and_syncs_every_table() {
  let master = TempRoot::new("standby_catchup_master");
  let replica = TempRoot::new("standby_catchup_replica");
  let tbl_a = TableIdent::new("demo", 1).unwrap();
  let tbl_b = TableIdent::new("demo", 2).unwrap();
  write_master(master.path(), &tbl_a).await;
  write_master(master.path(), &tbl_b).await;

  let service = StandbyService::new(
    replica_config(replica.path(), master.path()),
    LocalDirSource::new(master.path().to_path_buf()),
  )
  .unwrap();
  let synced = service.catch_up().await.unwrap();
  let tbl_li: Vec<TableIdent> = synced.iter().map(|(tbl, _)| tbl.clone()).collect();
  assert_eq!(tbl_li, vec![tbl_a.clone(), tbl_b.clone()]);
  for (tbl, stats) in &synced {
    assert!(stats.manifest_installed, "{tbl} manifest must be installed");
    assert_same_files(master.path(), replica.path(), tbl);
  }
  // replica 侧真的能读出来（不需要 master 指路）。
  let mut store = Store::new(
    CompioStore::new(
      replica.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store.reopen(&tbl_b).await.unwrap();
  assert_eq!(store.get(&tbl_b, b"a").await.unwrap(), Some(b"A".to_vec()));

  // 第二轮：没有新数据，data 全跳过，manifest 仍安装（幂等）。
  let again = service.catch_up().await.unwrap();
  assert_eq!(again.len(), 2);
  for (_, stats) in &again {
    assert_eq!(stats.data_fetched, 0);
    assert!(stats.manifest_installed);
  }
}

/// 表发现只认 `<tbl>.<partition>` 目录：跳过 staging、普通文件与怪名字。
#[test]
fn list_table_li_skips_staging_and_non_table_dirs() {
  let root = TempRoot::new("standby_discover");
  for name in ["demo.1", "demo.2", ".standby", "no_partition", "demo.x"] {
    create_dir_all(root.join(name)).unwrap();
  }
  write(root.join("stray_file"), b"x").unwrap();

  assert_eq!(
    list_table_li(root.path()).unwrap(),
    vec![
      TableIdent::new("demo", 1).unwrap(),
      TableIdent::new("demo", 2).unwrap()
    ]
  );
  // 根不存在视为空，不报错。
  assert!(
    list_table_li(&root.path().join("missing"))
      .unwrap()
      .is_empty()
  );
}

/// master 身份不能跑 replica 的追赶（C++ 侧 Master 不发起同步）。
#[compio::test]
async fn catch_up_rejects_master_role() {
  let master = TempRoot::new("standby_catchup_role");
  let config = StandbyConfig {
    role: StandbyRole::Master,
    local_root: master.path().to_path_buf(),
    master_addr: String::new(),
    master_path_li: Vec::new(),
    max_concurrency: 1,
    target_term: 0,
  };
  let service =
    StandbyService::new(config, LocalDirSource::new(master.path().to_path_buf())).unwrap();
  assert!(matches!(
    service.catch_up().await,
    Err(KvError::InvalidArgs)
  ));
}

/// rsync 后端用一次 `ls -1`（配了 ssh_addr 就走 ssh）拿表清单，不逐表探测。
#[test]
fn rsync_list_table_li_uses_single_ls() {
  struct LsRunner(Arc<Mutex<Vec<String>>>);
  impl CommandRunner for LsRunner {
    fn output(&self, program: &str, args: &[String]) -> Result<CommandOutput, KvError> {
      self
        .0
        .lock()
        .unwrap()
        .push(format!("{program} {}", args.join(" ")));
      Ok(CommandOutput {
        code: Some(0),
        stdout: b"demo.2\ndemo.1\n.standby\nnoise\n".to_vec(),
      })
    }
  }
  let calls = Arc::new(Mutex::new(Vec::new()));
  let shell = RsyncShell::new(LsRunner(Arc::clone(&calls)), "user@host");
  assert_eq!(
    shell.list_table_li("/data/store/").unwrap(),
    vec![
      TableIdent::new("demo", 1).unwrap(),
      TableIdent::new("demo", 2).unwrap()
    ]
  );
  assert_eq!(
    calls.lock().unwrap().as_slice(),
    ["ssh user@host ls -1 /data/store".to_owned()]
  );
}

/// sto-70：远端是 C++ 命名（`manifest_main_<term>`）时，安装必须落到**本地活跃名**
/// `manifest_main_0`（本地读侧只认这个名字），旧的无 tag main manifest 要清掉，
/// 归档 / branch manifest 不能碰，安装后 fsync 分区目录。
///
/// 反向验证：把安装改回「落远端原名」时，本地 `manifest_main_0` 不存在 →
/// 下面的 `reopen` 直接 `ResourceMissing`。
#[compio::test]
async fn installs_cxx_named_manifest_as_local_active_name() {
  let master = TempRoot::new("standby_cxx_name_master");
  let replica = TempRoot::new("standby_cxx_name_replica");
  let tbl = TableIdent::new("demo", 1).unwrap();
  write_master(master.path(), &tbl).await;
  let master_dir = table_dir(master.path(), &tbl);
  // Rust master 的活跃 manifest 固定 `manifest_main_0`；C++ master 带真实 term。
  rename(
    master_dir.join("manifest_main_0"),
    master_dir.join("manifest_main_5"),
  )
  .unwrap();

  // 副本目录预置噪声：旧 term main manifest（要删）、branch manifest（要留）、
  // 归档 manifest（要留）。
  let replica_dir = table_dir(replica.path(), &tbl);
  create_dir_all(&replica_dir).unwrap();
  write(replica_dir.join("manifest_main_3"), b"stale").unwrap();
  write(replica_dir.join("manifest_branchX_0"), b"branch").unwrap();
  write(replica_dir.join("manifest_main_0_001"), b"archive").unwrap();

  let mut config = replica_config(replica.path(), master.path());
  config.target_term = 5;
  let service =
    StandbyService::new(config, LocalDirSource::new(master.path().to_path_buf())).unwrap();
  let stats = service.sync_table(&tbl).await.unwrap();

  assert!(stats.manifest_installed, "{stats:?}");
  assert_eq!(stats.stale_manifest_removed, 1, "{stats:?}");
  assert!(stats.dir_synced, "{stats:?}");

  let name_li: Vec<String> = list_local_file_li(&replica_dir)
    .unwrap()
    .into_iter()
    .map(|file| file.name)
    .collect();
  assert!(
    name_li.contains(&"manifest_main_0".to_owned()),
    "{name_li:?}"
  );
  assert!(
    !name_li.contains(&"manifest_main_5".to_owned()),
    "{name_li:?}"
  );
  assert!(
    !name_li.contains(&"manifest_main_3".to_owned()),
    "{name_li:?}"
  );
  assert!(
    name_li.contains(&"manifest_branchX_0".to_owned()),
    "{name_li:?}"
  );
  assert!(
    name_li.contains(&"manifest_main_0_001".to_owned()),
    "{name_li:?}"
  );

  // 本地读侧（活跃名 + term 0 的 data 文件）能真正读出来。
  let mut store = Store::new(
    CompioStore::new(replica.path(), 512, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT),
    512,
    16,
  );
  store.prepare_attach("main", 0).unwrap();
  store.reopen(&tbl).await.unwrap();
  assert_eq!(
    store.get(&tbl, b"a").await.unwrap().as_deref(),
    Some(&b"A"[..])
  );
}

/// 归档 manifest 必须按本地归档名安装：否则 `reopen_archive(tag)` 在副本上
/// 必然 `ResourceMissing`。普通同步顺手安装全部 main 归档；带 tag 的入口
/// 只装目标归档。
#[compio::test]
async fn sync_installs_archive_manifests_for_tag_reopen() {
  let master = TempRoot::new("standby_archive_master");
  let replica = TempRoot::new("standby_archive_replica");
  let tbl = TableIdent::new("demo", 1).unwrap();
  write_master(master.path(), &tbl).await;
  let master_dir = table_dir(master.path(), &tbl);
  let replica_dir = table_dir(replica.path(), &tbl);
  rename(
    master_dir.join("manifest_main_0"),
    master_dir.join("manifest_main_5"),
  )
  .unwrap();
  let active = read(master_dir.join("manifest_main_5")).unwrap();
  write(master_dir.join("manifest_main_5_snap"), &active).unwrap();

  let mut config = replica_config(replica.path(), master.path());
  config.target_term = 5;
  let service =
    StandbyService::new(config, LocalDirSource::new(master.path().to_path_buf())).unwrap();
  let stats = service.sync_table(&tbl).await.unwrap();
  assert_eq!(stats.archive_installed, 1, "{stats:?}");
  assert_eq!(
    read(replica_dir.join("manifest_main_5_snap")).unwrap(),
    active
  );

  // 定向 tag：只装 `manifest_main_5_snap2`。
  write(master_dir.join("manifest_main_5_snap2"), &active).unwrap();
  let stats = service
    .sync_table_archive(&tbl, 5, "main", "snap2")
    .await
    .unwrap();
  assert_eq!(stats.archive_installed, 1, "{stats:?}");
  assert!(stats.manifest_installed, "{stats:?}");
  assert_eq!(
    read(replica_dir.join("manifest_main_5_snap2")).unwrap(),
    active
  );
}
