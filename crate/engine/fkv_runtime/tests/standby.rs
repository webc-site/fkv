//! StandbyReplica 端到端：单机 rsync 追赶。
//!
//! 对齐 C++ `tests/standby.cpp`：master `enable_local_standby=true` +
//! replica `standby_master_addr="local"` + `standby_master_store_paths={master 目录}`，
//! 两个本地 store 之间用真 rsync 同步；本次只在 replica 侧配置 standby
//! （master 用普通 Local store，盘面布局与 C++ StandbyMaster 相同）。
//!
//! 触发点对齐 C++ `ReopenTask::Reopen`：replica 的 `reopen` 先把该分区从 master
//! 追一次，再打开本地 manifest。

use std::{
  env::temp_dir,
  fs::{create_dir_all, read, remove_dir_all},
  path::{Path, PathBuf},
  process::{Command, id},
  sync::atomic::{AtomicU64, Ordering},
  thread::sleep,
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_config::KvOption;
use fkv_error::KvError;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_standby::{list_local_file_li, table_dir};
use fkv_types::TableIdent;

fn temp_root(tag: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_runtime_standby_{tag}_{}_{}_{}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos(),
    seq,
  ));
  create_dir_all(&root).unwrap();
  root
}

fn config(root: PathBuf) -> RuntimeConfig {
  RuntimeConfig {
    num_threads: 1,
    root,
    page_size: 512,
    restart_interval: 16,
    pages_per_file_shift: 18,
    segment_size: 4096,
    segments_per_file_shift: 2,
    large_value_threshold: 1024,
    fixed_buffer_size: 0,
    queue_size: 64,
    auto_reopen_retry_times: 3,
    auto_reopen_pending_time_us: 10_000,
    archive_interval_secs: 0,
    archive_num_retained: 100,
  }
}

/// replica 选项：`"local"` 哨兵 → 本机 rsync（对齐 C++ `RemoteSpec`）。
fn replica_options(replica: &Path, master: &Path) -> KvOption {
  KvOption {
    store_path: vec![replica.to_string_lossy().into_owned()],
    enable_local_standby: true,
    standby_master_addr: "local".to_owned(),
    standby_master_store_paths: vec![master.to_string_lossy().into_owned()],
    // 单机 standby 必须追加写（`validate` 也会自动开，这里显式表达意图）。
    data_append_mode: true,
    ..KvOption::default()
  }
}

/// replica 选项（多根 master）：路径顺序即 LUT 的根序，未给权重时等权。
fn replica_options_multi(replica: &Path, master_li: &[&Path]) -> KvOption {
  KvOption {
    store_path: vec![replica.to_string_lossy().into_owned()],
    enable_local_standby: true,
    standby_master_addr: "local".to_owned(),
    standby_master_store_paths: master_li
      .iter()
      .map(|master| master.to_string_lossy().into_owned())
      .collect(),
    data_append_mode: true,
    ..KvOption::default()
  }
}

/// master 选项（多根本地存储）：等权时 LUT = `[0, 1]`，分区按 `partition_id % 2` 分根。
fn master_multi_root_options(master_li: &[&Path]) -> KvOption {
  KvOption {
    store_path: master_li
      .iter()
      .map(|master| master.to_string_lossy().into_owned())
      .collect(),
    ..KvOption::default()
  }
}

/// 环境没有 rsync 就跳过（与 C++ 的 standby 测试同一前提）。
fn rsync_available() -> bool {
  Command::new("rsync")
    .arg("--version")
    .output()
    .is_ok_and(|output| output.status.success())
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

/// `TryAgain` 是同步期间 master 继续写导致的一致性保护；生产 auto-reopen 会
/// 重试，测试也按同一语义做有界重试。
async fn reopen_until_ok(runtime: &Runtime, tbl: &TableIdent) {
  for _ in 0..20 {
    match runtime.reopen(tbl).await {
      Ok(()) => return,
      Err(KvError::TryAgain) => sleep(Duration::from_millis(20)),
      Err(err) => panic!("reopen failed: {err:?}"),
    }
  }
  runtime.reopen(tbl).await.unwrap();
}

#[test]
fn replica_catches_up_master_on_reopen() {
  if !rsync_available() {
    eprintln!("skip: rsync not available");
    return;
  }
  let master_root = temp_root("master");
  let replica_root = temp_root("replica");
  let tbl = TableIdent::new("standby_tbl", 0).unwrap();
  pollster::block_on(async {
    let master = Runtime::start(config(master_root.clone())).unwrap();
    master.attach_shard(0, "main", 1).await.unwrap();
    master
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      )
      .await
      .unwrap();

    let replica = Runtime::start_with_option(
      config(replica_root.clone()),
      &replica_options(&replica_root, &master_root),
    )
    .unwrap();
    replica.attach_shard(0, "main", 1).await.unwrap();
    // `ListStandbyPartition` 走 standby source 的直接列举：master 已写过的分区
    // 必须出现在清单里（对齐 C++ `StandbyService::ListRemotePartitions`）。
    let partition_li = replica.list_standby_partition().await.unwrap();
    assert!(partition_li.contains(&tbl), "{partition_li:?}");
    // reopen 先把该分区追过来，再打开本地 manifest（对齐 C++ `ReopenTask`）。
    reopen_until_ok(&replica, &tbl).await;
    assert_eq!(replica.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    // 副本本地确实落了 data 文件，且与 master 逐字节一致。
    assert_same_files(&master_root, &replica_root, &tbl);

    // master 再写一个 key：replica 再 reopen 一次即增量追上。
    master
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)],
      )
      .await
      .unwrap();
    reopen_until_ok(&replica, &tbl).await;
    assert_eq!(replica.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
    assert_eq!(replica.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

    // master 停掉后，replica 读已同步的数据不能丢（数据面本地）。
    master.shutdown().await.unwrap();
    assert_eq!(replica.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(replica.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));

    replica.shutdown().await.unwrap();
  });
  remove_dir_all(master_root).unwrap();
  remove_dir_all(replica_root).unwrap();
}

/// cld-58：master 双根时 replica 必须按分区 LUT 选根，而不是只追 root[0]。
///
/// 等权 LUT = `[0, 1]`：分区 1 落 `master_b`，分区 2 落 `master_a`。修复前
/// source 只取 `standby_master_store_paths[0]`，分区 1 在 root[0] 下不存在 →
/// `ResourceMissing` + `drop_manifest`，reopen 永久失败。
#[test]
fn replica_catches_up_multi_root_master_by_lut() {
  if !rsync_available() {
    eprintln!("skip: rsync not available");
    return;
  }
  let master_a = temp_root("multi_master_a");
  let master_b = temp_root("multi_master_b");
  let replica_root = temp_root("multi_replica");
  let tbl_a = TableIdent::new("standby_tbl", 2).unwrap();
  let tbl_b = TableIdent::new("standby_tbl", 1).unwrap();
  pollster::block_on(async {
    let master = Runtime::start_with_option(
      config(master_a.clone()),
      &master_multi_root_options(&[&master_a, &master_b]),
    )
    .unwrap();
    master.attach_shard(0, "main", 1).await.unwrap();
    master
      .put(
        &tbl_b,
        vec![BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1)],
      )
      .await
      .unwrap();
    master
      .put(
        &tbl_a,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 2)],
      )
      .await
      .unwrap();

    // 先证明 master 侧确实分根：否则本用例退化成单根，测不出 cld-58。
    assert!(table_dir(&master_b, &tbl_b).is_dir());
    assert!(table_dir(&master_a, &tbl_a).is_dir());
    assert!(!table_dir(&master_a, &tbl_b).exists());
    assert!(!table_dir(&master_b, &tbl_a).exists());

    let replica = Runtime::start_with_option(
      config(replica_root.clone()),
      &replica_options_multi(&replica_root, &[&master_a, &master_b]),
    )
    .unwrap();
    replica.attach_shard(0, "main", 1).await.unwrap();
    // 分区发现同样要跨全部 master 根：只列 root[0] 会漏掉 master_b 上的分区 1。
    let partition_li = replica.list_standby_partition().await.unwrap();
    assert!(partition_li.contains(&tbl_b), "{partition_li:?}");
    assert!(partition_li.contains(&tbl_a), "{partition_li:?}");
    // 修复前分区 1（master_b）会在这里报 ResourceMissing 并 drop manifest。
    reopen_until_ok(&replica, &tbl_b).await;
    reopen_until_ok(&replica, &tbl_a).await;
    assert_eq!(
      replica.get(&tbl_b, b"b").await.unwrap(),
      Some(b"B".to_vec())
    );
    assert_eq!(
      replica.get(&tbl_a, b"a").await.unwrap(),
      Some(b"A".to_vec())
    );
    // 两个根各自的分区都与副本逐字节一致。
    assert_same_files(&master_b, &replica_root, &tbl_b);
    assert_same_files(&master_a, &replica_root, &tbl_a);

    replica.shutdown().await.unwrap();
    master.shutdown().await.unwrap();
  });
  remove_dir_all(master_a).unwrap();
  remove_dir_all(master_b).unwrap();
  remove_dir_all(replica_root).unwrap();
}

#[test]
fn replica_with_missing_master_fails_reopen() {
  let master_root = temp_root("absent_master");
  let replica_root = temp_root("absent_replica");
  let tbl = TableIdent::new("standby_tbl", 0).unwrap();
  pollster::block_on(async {
    let replica = Runtime::start_with_option(
      config(replica_root.clone()),
      &replica_options(&replica_root, &master_root),
    )
    .unwrap();
    replica.attach_shard(0, "main", 1).await.unwrap();
    assert_eq!(
      replica.reopen(&tbl).await.unwrap_err(),
      KvError::ResourceMissing
    );
    replica.shutdown().await.unwrap();
  });
  remove_dir_all(master_root).unwrap();
  remove_dir_all(replica_root).unwrap();
}

/// rev-46：懒开（无显式 `reopen`）也必须先追 master，不能 replay 本地旧 manifest。
///
/// 首次访问直接 `get`：`ensure_table_ready` 在 `StandbyReplica` 下先
/// `sync_standby_partition` 再 `store.reopen`，读到 master 当前数据。
#[test]
fn replica_lazy_open_syncs_master_before_replaying_local_manifest() {
  if !rsync_available() {
    eprintln!("skip: rsync not available");
    return;
  }
  let master_root = temp_root("lazy_master");
  let replica_root = temp_root("lazy_replica");
  let tbl = TableIdent::new("standby_tbl", 0).unwrap();
  pollster::block_on(async {
    let master = Runtime::start(config(master_root.clone())).unwrap();
    master.attach_shard(0, "main", 1).await.unwrap();
    master
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      )
      .await
      .unwrap();

    let replica = Runtime::start_with_option(
      config(replica_root.clone()),
      &replica_options(&replica_root, &master_root),
    )
    .unwrap();
    replica.attach_shard(0, "main", 1).await.unwrap();
    // 不调用 reopen：第一次 get 走懒开路径。
    assert_eq!(replica.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_same_files(&master_root, &replica_root, &tbl);

    replica.shutdown().await.unwrap();
    master.shutdown().await.unwrap();
  });
  remove_dir_all(master_root).unwrap();
  remove_dir_all(replica_root).unwrap();
}

/// rev-46：副本重启后本地 manifest 是旧的，懒开首读必须追平 master。
#[test]
fn replica_lazy_open_after_restart_reads_current_master() {
  if !rsync_available() {
    eprintln!("skip: rsync not available");
    return;
  }
  let master_root = temp_root("restart_master");
  let replica_root = temp_root("restart_replica");
  let tbl = TableIdent::new("standby_tbl", 0).unwrap();
  pollster::block_on(async {
    let master = Runtime::start(config(master_root.clone())).unwrap();
    master.attach_shard(0, "main", 1).await.unwrap();
    master
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"old".to_vec(), 1)],
      )
      .await
      .unwrap();

    // 第一个副本进程：同步一次后关闭（本地留下一份旧 manifest）。
    {
      let replica = Runtime::start_with_option(
        config(replica_root.clone()),
        &replica_options(&replica_root, &master_root),
      )
      .unwrap();
      replica.attach_shard(0, "main", 1).await.unwrap();
      reopen_until_ok(&replica, &tbl).await;
      assert_eq!(
        replica.get(&tbl, b"a").await.unwrap(),
        Some(b"old".to_vec())
      );
      replica.shutdown().await.unwrap();
    }

    // master 在副本停机期间更新。
    master
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"new".to_vec(), 2)],
      )
      .await
      .unwrap();

    // 新副本进程（同一盘面）：懒开首读必须看到 master 当前值。
    let replica = Runtime::start_with_option(
      config(replica_root.clone()),
      &replica_options(&replica_root, &master_root),
    )
    .unwrap();
    replica.attach_shard(0, "main", 1).await.unwrap();
    assert_eq!(
      replica.get(&tbl, b"a").await.unwrap(),
      Some(b"new".to_vec())
    );

    replica.shutdown().await.unwrap();
    master.shutdown().await.unwrap();
  });
  remove_dir_all(master_root).unwrap();
  remove_dir_all(replica_root).unwrap();
}

/// rev-46：master 无该分区时懒开不得装空表，按既有挂起重排后回
/// `ResourceMissing`（对齐 eng-39c）。
#[test]
fn replica_lazy_open_missing_master_does_not_create_empty_table() {
  if !rsync_available() {
    eprintln!("skip: rsync not available");
    return;
  }
  let master_root = temp_root("lazy_absent_master");
  let replica_root = temp_root("lazy_absent_replica");
  let tbl = TableIdent::new("standby_tbl", 0).unwrap();
  pollster::block_on(async {
    let master = Runtime::start(config(master_root.clone())).unwrap();
    master.attach_shard(0, "main", 1).await.unwrap();

    let replica = Runtime::start_with_option(
      config(replica_root.clone()),
      &replica_options(&replica_root, &master_root),
    )
    .unwrap();
    replica.attach_shard(0, "main", 1).await.unwrap();
    assert_eq!(
      replica.get(&tbl, b"a").await.unwrap_err(),
      KvError::ResourceMissing
    );
    assert!(
      !table_dir(&replica_root, &tbl).exists(),
      "懒开失败不得创建本地空表目录"
    );

    replica.shutdown().await.unwrap();
    master.shutdown().await.unwrap();
  });
  remove_dir_all(master_root).unwrap();
  remove_dir_all(replica_root).unwrap();
}
