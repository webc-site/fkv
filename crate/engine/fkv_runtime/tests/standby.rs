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
  fs::{read, remove_file, write},
  path::{Path, PathBuf},
  process::Command,
  thread::sleep,
  time::Duration,
};

/// sem-21：本机没有 sshd 时用假 `ssh` 把远端调用还原成本机命令。
///
/// `rsync` 的 remote spec（`addr:path`）自己也会从 PATH 拉起 `ssh`，
/// 因此同一个假脚本既覆盖 `RsyncShell::list` 的 `ssh <addr> rsync ...`，
/// 也覆盖数据 / manifest 传输的 server 端协议。目录不存在时真实 rsync
/// 以 23 退出（source missing），与远端 master 删分区同码。
#[cfg(unix)]
mod fake_ssh {
  use std::{
    env,
    ffi::OsString,
    fs::{self, write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
  };

  /// 安装假 ssh 脚本，返回「注入退出码」标记文件路径。
  pub fn install(bin_dir: &Path) -> PathBuf {
    let marker = bin_dir.join("exit_code");
    let script = format!(
      "#!/bin/sh\n\
       marker=\"{}\"\n\
       if [ -f \"$marker\" ]; then exit \"$(cat \"$marker\")\"; fi\n\
       while [ $# -gt 0 ]; do\n\
         case \"$1\" in\n\
           -l|-o|-p|-i|-F) shift 2 ;;\n\
           -*) shift ;;\n\
           *) shift; break ;;\n\
         esac\n\
       done\n\
       exec \"$@\"\n",
      marker.display()
    );
    let path = bin_dir.join("ssh");
    write(&path, script).unwrap();
    let mut perm = fs::metadata(&path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&path, perm).unwrap();
    marker
  }

  /// 把 `bin_dir` 放到 PATH 最前，Drop 时恢复。
  ///
  /// nextest 每个用例一个进程；同一进程内的 libtest 线程只影响本文件里
  /// 使用 ssh 的用例，其余用例即使并发也只调用 PATH 里未被遮蔽的 rsync。
  pub struct PathGuard {
    old: Option<OsString>,
  }

  impl PathGuard {
    pub fn prepend(dir: &Path) -> Self {
      let old = env::var_os("PATH");
      let mut path_li = vec![dir.to_path_buf()];
      if let Some(old) = &old {
        path_li.extend(env::split_paths(old));
      }
      let joined = env::join_paths(path_li).unwrap();
      // SAFETY: 仅测试进程内改环境变量；见上方并发说明。
      unsafe { env::set_var("PATH", joined) };
      Self { old }
    }
  }

  impl Drop for PathGuard {
    fn drop(&mut self) {
      // SAFETY: 同上。
      unsafe {
        match &self.old {
          Some(old) => env::set_var("PATH", old),
          None => env::remove_var("PATH"),
        }
      }
    }
  }
}

use fkv_btree::BatchEntry;
use fkv_config::KvOption;
use fkv_error::KvError;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_standby::{list_local_file_li, table_dir};
use fkv_testkit::TempRoot;
use fkv_types::{SEGMENT_FILE_PREFIX, TableIdent, manifest_file_name};

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

/// replica 选项（远端 ssh）：地址必须含 `@`（C++ `username@addr` 约束）。
fn replica_options_remote(replica: &Path, master: &Path, master_addr: &str) -> KvOption {
  KvOption {
    store_path: vec![replica.to_string_lossy().into_owned()],
    enable_local_standby: true,
    standby_master_addr: master_addr.to_owned(),
    standby_master_store_paths: vec![master.to_string_lossy().into_owned()],
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

/// 分区目录下的 `segment_*` 文件名（有序），供 rsync 文件集断言。
fn segment_file_li(root: &Path, tbl: &TableIdent) -> Vec<String> {
  let mut name_li = list_local_file_li(&table_dir(root, tbl))
    .unwrap()
    .into_iter()
    .map(|file| file.name)
    .filter(|name| name.starts_with(SEGMENT_FILE_PREFIX))
    .collect::<Vec<_>>();
  name_li.sort();
  name_li
}

/// `TryAgain` 是同步期间 master 继续写导致的一致性保护；生产 auto-reopen 会
/// 重试，测试也按同一语义做有界重试。
async fn reopen_until_ok(runtime: &Runtime, tbl: &TableIdent) {
  // 慢 runner 上 rsync/fake-ssh 追赶可能持续 >400ms；有界重试 2s 后再判失败。
  for _ in 0..100 {
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
  let master_root = TempRoot::new("runtime_standby_master");
  let replica_root = TempRoot::new("runtime_standby_replica");
  let tbl = TableIdent::new("standby_tbl", 0).unwrap();
  pollster::block_on(async {
    let master = Runtime::start(config(master_root.path().to_path_buf())).unwrap();
    master.attach_shard(0, "main", 1).await.unwrap();
    master
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      )
      .await
      .unwrap();

    let replica = Runtime::start_with_option(
      config(replica_root.path().to_path_buf()),
      &replica_options(replica_root.path(), master_root.path()),
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
    assert_same_files(master_root.path(), replica_root.path(), &tbl);

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
  let master_a = TempRoot::new("runtime_standby_multi_master_a");
  let master_b = TempRoot::new("runtime_standby_multi_master_b");
  let replica_root = TempRoot::new("runtime_standby_multi_replica");
  let tbl_a = TableIdent::new("standby_tbl", 2).unwrap();
  let tbl_b = TableIdent::new("standby_tbl", 1).unwrap();
  pollster::block_on(async {
    let master = Runtime::start_with_option(
      config(master_a.path().to_path_buf()),
      &master_multi_root_options(&[master_a.path(), master_b.path()]),
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
    assert!(table_dir(master_b.path(), &tbl_b).is_dir());
    assert!(table_dir(master_a.path(), &tbl_a).is_dir());
    assert!(!table_dir(master_a.path(), &tbl_b).exists());
    assert!(!table_dir(master_b.path(), &tbl_a).exists());

    let replica = Runtime::start_with_option(
      config(replica_root.path().to_path_buf()),
      &replica_options_multi(replica_root.path(), &[master_a.path(), master_b.path()]),
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
    assert_same_files(master_b.path(), replica_root.path(), &tbl_b);
    assert_same_files(master_a.path(), replica_root.path(), &tbl_a);

    replica.shutdown().await.unwrap();
    master.shutdown().await.unwrap();
  });
}

/// sem-61-standby-rsync-segment-include：rsync 文件集放行 `segment_*`，
/// replica 本地存在 segment 文件且 `get_large` 可用。
///
/// C++ `StandbyService::StartRsyncJob` 的 rsync 只 `--include=data_*`
/// （`src/standby_service.cpp:706-710`，全文件零处 segment），replica 本地
/// 永远没有 segment 文件，`GetLarge`（`src/tasks/task.cpp:238-253` 本地
/// `ReadSegments`）因文件缺失失败；fkv `sync_dir` 额外放行 `segment_*`
/// （`fkv_standby/src/rsync.rs` 的 include 列表），属 Rust 单侧加强（见
/// eloqstore2fkv COMPAT_BASELINE「standby rsync 文件集」条）。本用例锁定
/// 现行为：master 写大 value → replica 追赶后本地存在 `segment_*` 且
/// `get_large` 成功；若有人把 include 收窄回 `data_*`（向 C++ 反向对齐）
/// 会立红。
#[test]
fn replica_catches_up_segment_files_and_serves_get_large() {
  if !rsync_available() {
    eprintln!("skip: rsync not available");
    return;
  }
  let master_root = TempRoot::new("runtime_standby_segment_master");
  let replica_root = TempRoot::new("runtime_standby_segment_replica");
  let tbl = TableIdent::new("standby_tbl", 0).unwrap();
  pollster::block_on(async {
    let master = Runtime::start(config(master_root.path().to_path_buf())).unwrap();
    master.attach_shard(0, "main", 1).await.unwrap();
    // 9000 B > large_value_threshold(1024)：master 把 value 落进 segment 文件。
    let big: Vec<u8> = (0..9000).map(|index| (index % 251) as u8).collect();
    master
      .put_large(
        &tbl,
        vec![BatchEntry::upsert(b"big".to_vec(), big.clone(), 1)],
      )
      .await
      .unwrap();
    let master_seg_li = segment_file_li(master_root.path(), &tbl);
    assert!(
      !master_seg_li.is_empty(),
      "master 写大 value 后应有 segment 文件"
    );

    let replica = Runtime::start_with_option(
      config(replica_root.path().to_path_buf()),
      &replica_options(replica_root.path(), master_root.path()),
    )
    .unwrap();
    replica.attach_shard(0, "main", 1).await.unwrap();
    reopen_until_ok(&replica, &tbl).await;

    // rsync include 放行 segment_*：副本文件集与 master 逐字节一致（含大
    // value 的 segment），这与 C++ 只同步 data_* 形成单侧差异。
    assert_same_files(master_root.path(), replica_root.path(), &tbl);
    let replica_seg_li = segment_file_li(replica_root.path(), &tbl);
    assert!(
      !replica_seg_li.is_empty(),
      "replica 本地必须存在 segment_* 文件（C++ 只同步 data_*）"
    );
    assert_eq!(replica_seg_li, master_seg_li);

    // replica 读面：get_large 直接可用（C++ replica 缺 segment 文件会失败）。
    let read = replica.get_large(&tbl, b"big").await.unwrap().unwrap();
    assert_eq!(read.value, big);

    replica.shutdown().await.unwrap();
    master.shutdown().await.unwrap();
  });
}

/// sem-12：显式 `reopen` 对齐 C++ `ReopenTask::Reopen` + `InstallExternalSnapshot`
/// —— master 没有该分区时清本地状态并回成功（C++ 的 `cleared_local_state` 分支
/// 归 `NoError`）。清理不等于装空表：随后的读仍按缺失回 `ResourceMissing`。
///
/// 同一用例锁定非空 tag 的有意差异：C++ 对缺失归档同样「清状态 + 成功」，fkv
/// 保持显式失败可见（见 eloqstore2fkv/COMPAT_BASELINE 条目建议）。
#[test]
fn replica_with_missing_master_reopen_succeeds_after_clear() {
  let master_root = TempRoot::new("runtime_standby_absent_master");
  let replica_root = TempRoot::new("runtime_standby_absent_replica");
  let tbl = TableIdent::new("standby_tbl", 0).unwrap();
  pollster::block_on(async {
    let replica = Runtime::start_with_option(
      config(replica_root.path().to_path_buf()),
      &replica_options(replica_root.path(), master_root.path()),
    )
    .unwrap();
    replica.attach_shard(0, "main", 1).await.unwrap();
    // C++ 同路径：NotFound / ResourceMissing → DropLocalManifest + 清 RootMeta。
    replica.reopen(&tbl).await.unwrap();
    assert!(
      !table_dir(replica_root.path(), &tbl)
        .join(manifest_file_name("main", 0))
        .exists(),
      "缺失快照的重开不得留下本地活跃 manifest"
    );
    // 清理后是「未打开」，不是空表：读仍等 master 的 manifest 可见。
    assert_eq!(
      replica.get(&tbl, b"a").await.unwrap_err(),
      KvError::ResourceMissing
    );
    // 非空 tag：显式 `reopen_archive` 保持 `ResourceMissing`（有意差异）。
    assert_eq!(
      replica.reopen_archive(&tbl, "gone").await.unwrap_err(),
      KvError::ResourceMissing
    );
    // 空 tag = 重开活跃快照，与 `reopen` 同一收尾。
    replica.reopen_archive(&tbl, "").await.unwrap();
    replica.shutdown().await.unwrap();
  });
}

/// sem-15：表**曾经打开过**（内存 `Store` 持有 root / mapper）时，master 删除
/// 该分区后的显式 `reopen` 也必须清内存状态，不能只 `drop_manifest`。
///
/// C++ `ReopenTask` 的 `clear_local_state` 对 `sync_err == NotFound /
/// ResourceMissing` 不区分 `Clean()`，一律清 RootMeta（`reopen_task.cpp:63-110`）。
/// 修复前 fkv 只在 clean 分支清 `Store` / `table_open`，而显式 `reopen` 发
/// `clean: false`：`Runtime::reopen` 把 `ResourceMissing` 收敛成 `Ok` 后，
/// 下一次读仍命中 `table_open.ready`，直接返回删除前的旧值。
#[test]
fn replica_reopen_after_master_drop_clears_open_table() {
  if !rsync_available() {
    eprintln!("skip: rsync not available");
    return;
  }
  let master_root = TempRoot::new("runtime_standby_open_drop_master");
  let replica_root = TempRoot::new("runtime_standby_open_drop_replica");
  let tbl = TableIdent::new("standby_tbl", 0).unwrap();
  pollster::block_on(async {
    let master = Runtime::start(config(master_root.path().to_path_buf())).unwrap();
    master.attach_shard(0, "main", 1).await.unwrap();
    master
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"old".to_vec(), 1)],
      )
      .await
      .unwrap();

    let replica = Runtime::start_with_option(
      config(replica_root.path().to_path_buf()),
      &replica_options(replica_root.path(), master_root.path()),
    )
    .unwrap();
    replica.attach_shard(0, "main", 1).await.unwrap();
    // 先正常打开并读到值：内存 `Store` 与 `table_open` 登记都已建立。
    reopen_until_ok(&replica, &tbl).await;
    assert_eq!(
      replica.get(&tbl, b"a").await.unwrap(),
      Some(b"old".to_vec())
    );

    // master 删除分区（目录随 GC 移除），副本上只剩本地旧副本。
    master.drop_table(&tbl).await.unwrap();
    assert!(
      !table_dir(master_root.path(), &tbl).exists(),
      "master 删表并 GC 后分区目录应被移除"
    );

    // 显式 reopen 按 C++ `clear_local_state` 回成功，且必须清内存 Store。
    replica.reopen(&tbl).await.unwrap();
    assert!(
      !table_dir(replica_root.path(), &tbl)
        .join(manifest_file_name("main", 0))
        .exists(),
      "master 已删分区的本地 manifest 必须被删除"
    );
    // 读必须回缺失错误，不能命中 `table_open.ready` 返回删除前的旧值。
    assert_eq!(
      replica.get(&tbl, b"a").await.unwrap_err(),
      KvError::ResourceMissing,
      "已打开表在 reopen 清理后不得返回旧值"
    );

    // 再次 reopen 仍收敛成功、仍不可读（内存状态保持已清）。
    replica.reopen(&tbl).await.unwrap();
    assert_eq!(
      replica.get(&tbl, b"a").await.unwrap_err(),
      KvError::ResourceMissing
    );

    replica.shutdown().await.unwrap();
    master.shutdown().await.unwrap();
  });
}

/// sem-21：远端（ssh）master 删分区时 rsync 以 23 退出，必须映射为
/// `NotFound`，才能复用 sem-15 的清理路径；非 23/24 的失败不得清本地副本。
///
/// 用假 ssh 在本机执行远端命令：`RsyncShell::list` 的 `ssh <addr> rsync
/// --list-only` 与 rsync 数据面的 server 协议都经同一 PATH 上的 ssh 还原，
/// 因此走的是与生产一致的 `ssh_addr` 非空分支。
#[test]
#[cfg(unix)]
fn replica_ssh_master_drop_clears_open_table() {
  if !rsync_available() {
    eprintln!("skip: rsync not available");
    return;
  }
  let fake_bin = TempRoot::new("runtime_standby_fake_ssh");
  let marker = fake_ssh::install(fake_bin.path());
  let _path_guard = fake_ssh::PathGuard::prepend(fake_bin.path());

  let master_root = TempRoot::new("runtime_standby_ssh_master");
  let replica_root = TempRoot::new("runtime_standby_ssh_replica");
  let tbl = TableIdent::new("standby_tbl", 0).unwrap();
  let absent_tbl = TableIdent::new("standby_never", 0).unwrap();
  pollster::block_on(async {
    let master = Runtime::start(config(master_root.path().to_path_buf())).unwrap();
    master.attach_shard(0, "main", 1).await.unwrap();
    master
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"old".to_vec(), 1)],
      )
      .await
      .unwrap();

    let replica = Runtime::start_with_option(
      config(replica_root.path().to_path_buf()),
      &replica_options_remote(replica_root.path(), master_root.path(), "s21@fakehost"),
    )
    .unwrap();
    replica.attach_shard(0, "main", 1).await.unwrap();
    reopen_until_ok(&replica, &tbl).await;
    assert_eq!(
      replica.get(&tbl, b"a").await.unwrap(),
      Some(b"old".to_vec())
    );

    // 非 23/24（这里注入 ssh 失败 1）：保持 IoFail，不清本地副本。
    write(&marker, b"1").unwrap();
    assert_eq!(replica.reopen(&tbl).await.unwrap_err(), KvError::IoFail);
    assert_eq!(
      replica.get(&tbl, b"a").await.unwrap(),
      Some(b"old".to_vec()),
      "非缺失类 rsync 失败不得清掉仍可用的本地副本"
    );
    remove_file(&marker).unwrap();

    // master 删分区：远端 list 现在以 rsync 23 退出。
    master.drop_table(&tbl).await.unwrap();
    assert!(
      !table_dir(master_root.path(), &tbl).exists(),
      "master 删表并 GC 后分区目录应被移除"
    );
    // 与 sem-15 同一收尾：reopen 收敛成功、内存表被清、读回缺失而非旧值。
    replica.reopen(&tbl).await.unwrap();
    assert!(
      !table_dir(replica_root.path(), &tbl)
        .join(manifest_file_name("main", 0))
        .exists(),
      "远端已删分区的本地 manifest 必须被删除"
    );
    assert_eq!(
      replica.get(&tbl, b"a").await.unwrap_err(),
      KvError::ResourceMissing,
      "已打开表在远端缺失清理后不得返回旧值"
    );
    replica.reopen(&tbl).await.unwrap();
    assert_eq!(
      replica.get(&tbl, b"a").await.unwrap_err(),
      KvError::ResourceMissing
    );

    // 首次打开：远端没有该分区时必须进 auto-reopen 等待（ResourceMissing），
    // 不能以 IoFail 直接失败，也不能在本地创建空表目录。
    assert_eq!(
      replica.get(&absent_tbl, b"a").await.unwrap_err(),
      KvError::ResourceMissing
    );
    assert!(
      !table_dir(replica_root.path(), &absent_tbl).exists(),
      "远端缺失的懒开不得创建本地空表目录"
    );

    replica.shutdown().await.unwrap();
    master.shutdown().await.unwrap();
  });
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
  let master_root = TempRoot::new("runtime_standby_lazy_master");
  let replica_root = TempRoot::new("runtime_standby_lazy_replica");
  let tbl = TableIdent::new("standby_tbl", 0).unwrap();
  pollster::block_on(async {
    let master = Runtime::start(config(master_root.path().to_path_buf())).unwrap();
    master.attach_shard(0, "main", 1).await.unwrap();
    master
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      )
      .await
      .unwrap();

    let replica = Runtime::start_with_option(
      config(replica_root.path().to_path_buf()),
      &replica_options(replica_root.path(), master_root.path()),
    )
    .unwrap();
    replica.attach_shard(0, "main", 1).await.unwrap();
    // 不调用 reopen：第一次 get 走懒开路径。
    assert_eq!(replica.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_same_files(master_root.path(), replica_root.path(), &tbl);

    replica.shutdown().await.unwrap();
    master.shutdown().await.unwrap();
  });
}

/// rev-46：副本重启后本地 manifest 是旧的，懒开首读必须追平 master。
#[test]
fn replica_lazy_open_after_restart_reads_current_master() {
  if !rsync_available() {
    eprintln!("skip: rsync not available");
    return;
  }
  let master_root = TempRoot::new("runtime_standby_restart_master");
  let replica_root = TempRoot::new("runtime_standby_restart_replica");
  let tbl = TableIdent::new("standby_tbl", 0).unwrap();
  pollster::block_on(async {
    let master = Runtime::start(config(master_root.path().to_path_buf())).unwrap();
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
        config(replica_root.path().to_path_buf()),
        &replica_options(replica_root.path(), master_root.path()),
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
      config(replica_root.path().to_path_buf()),
      &replica_options(replica_root.path(), master_root.path()),
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
}

/// sem-05：standby 全局重开的目标集合 = master 远端 ∪ 本地扫描，且
/// 「master 已删、仅本地存在」的分区按 C++ `SetClean(true)` 语义清理成功。
///
/// 对齐 C++ `HandleGlobalReopenRequest` 的 standby 分支：
/// 1. master 独有的分区也要进入目标集合（修复前只扫本地，远端分区不会被拉取）；
/// 2. master 删掉 P2 后，replica 的全局重开必须删除本地 manifest 且
///    `failed_li` 为空（修复前记为 `ResourceMissing` 失败）；
/// 3. master 新增的 P4 在全局重开后本地可读。
#[test]
fn global_reopen_merges_master_partitions_and_cleans_local_only() {
  if !rsync_available() {
    eprintln!("skip: rsync not available");
    return;
  }
  let master_root = TempRoot::new("runtime_standby_global_master");
  let replica_root = TempRoot::new("runtime_standby_global_replica");
  let p2 = TableIdent::new("standby_tbl", 2).unwrap();
  let p3 = TableIdent::new("standby_tbl", 3).unwrap();
  let p4 = TableIdent::new("standby_tbl", 4).unwrap();
  pollster::block_on(async {
    let master = Runtime::start(config(master_root.path().to_path_buf())).unwrap();
    master.attach_shard(0, "main", 1).await.unwrap();
    master
      .put(
        &p2,
        vec![BatchEntry::upsert(b"k".to_vec(), b"v2".to_vec(), 1)],
      )
      .await
      .unwrap();
    master
      .put(
        &p3,
        vec![BatchEntry::upsert(b"k".to_vec(), b"old".to_vec(), 1)],
      )
      .await
      .unwrap();

    let replica = Runtime::start_with_option(
      config(replica_root.path().to_path_buf()),
      &replica_options(replica_root.path(), master_root.path()),
    )
    .unwrap();
    replica.attach_shard(0, "main", 1).await.unwrap();

    // 修复前目标集合只来自本地扫描（空），这里 ok_num 会是 0，P2 / P3 不会被拉取。
    let reply = replica.global_reopen("").await.unwrap();
    assert!(reply.failed_li.is_empty(), "{:?}", reply.failed_li);
    assert_eq!(reply.ok_num, 2, "{reply:?}");
    assert_eq!(replica.get(&p2, b"k").await.unwrap(), Some(b"v2".to_vec()));
    assert_eq!(replica.get(&p3, b"k").await.unwrap(), Some(b"old".to_vec()));

    // master 删 P2、更新 P3、新增 P4。Drop 后的 FileGc 会移除空分区目录，
    // master 的分区清单里不再有 P2（这是 clean 判定的前提）。
    master.drop_table(&p2).await.unwrap();
    assert!(
      !table_dir(master_root.path(), &p2).exists(),
      "master 删表并 GC 后分区目录应被移除"
    );
    master
      .put(
        &p3,
        vec![BatchEntry::upsert(b"k".to_vec(), b"new".to_vec(), 2)],
      )
      .await
      .unwrap();
    master
      .put(
        &p4,
        vec![BatchEntry::upsert(b"k".to_vec(), b"v4".to_vec(), 3)],
      )
      .await
      .unwrap();
    let remote_li = replica.list_standby_partition().await.unwrap();
    assert!(!remote_li.contains(&p2), "{remote_li:?}");
    assert!(
      remote_li.contains(&p3) && remote_li.contains(&p4),
      "{remote_li:?}"
    );

    // 目标集合 = 远端(P3, P4) ∪ 本地(P2, P3)：P2 走 clean 清理，其余走 rsync。
    let reply = replica.global_reopen("").await.unwrap();
    assert!(
      reply.failed_li.is_empty(),
      "clean 分区不应计入失败: {:?}",
      reply.failed_li
    );
    assert_eq!(reply.ok_num, 3, "{reply:?}");

    // P2：本地活跃 manifest 已被 clean 语义删除（data 文件由后续 GC 回收，
    // 与 C++ `DropLocalManifest` + `TryAddLocalGc` 同路径）。
    assert!(
      !table_dir(replica_root.path(), &p2)
        .join(manifest_file_name("main", 0))
        .exists(),
      "master 已删分区的本地 manifest 必须被删除"
    );
    // P3：追到 master 新值；P4：远端独有分区已被拉到本地可读。
    assert_eq!(replica.get(&p3, b"k").await.unwrap(), Some(b"new".to_vec()));
    assert_eq!(replica.get(&p4, b"k").await.unwrap(), Some(b"v4".to_vec()));

    replica.shutdown().await.unwrap();
    master.shutdown().await.unwrap();
  });
}

/// sem-12（sem-05 残差）：standby **带 tag** 的全局重开对「master 已删、仅本地
/// 存在」的分区同样按 C++ `SetClean(true)` 清理成功，不再进 `failed_li`
/// （C++ 的 clean 置位对 `req->Tag()` 是否为空一视同仁）。
///
/// 同一用例锁定 master 仍有该分区、但该 tag 的归档缺失时的有意差异：C++
/// `InstallExternalSnapshot` 会清本地状态并成功，fkv 保持 `ResourceMissing`
/// 可见（COMPAT_BASELINE 条目建议）。
#[test]
fn global_reopen_with_tag_cleans_local_only_partition() {
  if !rsync_available() {
    eprintln!("skip: rsync not available");
    return;
  }
  let master_root = TempRoot::new("runtime_standby_tag_master");
  let replica_root = TempRoot::new("runtime_standby_tag_replica");
  let tbl = TableIdent::new("standby_tbl", 5).unwrap();
  pollster::block_on(async {
    let master = Runtime::start(config(master_root.path().to_path_buf())).unwrap();
    master.attach_shard(0, "main", 1).await.unwrap();
    master
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      )
      .await
      .unwrap();

    let replica = Runtime::start_with_option(
      config(replica_root.path().to_path_buf()),
      &replica_options(replica_root.path(), master_root.path()),
    )
    .unwrap();
    replica.attach_shard(0, "main", 1).await.unwrap();

    // master 有分区、没有该 tag：fkv 记 `ResourceMissing`（显式失败可见）。
    let reply = replica.global_reopen("snap").await.unwrap();
    assert_eq!(reply.ok_num, 0, "{reply:?}");
    assert_eq!(
      reply.failed_li,
      vec![(tbl.clone(), KvError::ResourceMissing)],
      "{reply:?}"
    );

    // master 删掉整个分区（目录随 GC 移除），replica 上只剩本地副本。
    master.drop_table(&tbl).await.unwrap();
    assert!(
      !table_dir(master_root.path(), &tbl).exists(),
      "master 删表并 GC 后分区目录应被移除"
    );
    assert!(
      table_dir(replica_root.path(), &tbl).exists(),
      "replica 上应仍有本地副本（仅本地存在的 clean 判定前提）"
    );

    // sem-05 残差：带 tag 的全局重开也必须把「仅本地存在」分区清理成功。
    let reply = replica.global_reopen("snap").await.unwrap();
    assert!(
      reply.failed_li.is_empty(),
      "clean 分区不应计入失败: {:?}",
      reply.failed_li
    );
    assert_eq!(reply.ok_num, 1, "{reply:?}");
    assert!(
      !table_dir(replica_root.path(), &tbl)
        .join(manifest_file_name("main", 0))
        .exists(),
      "master 已删分区的本地 manifest 必须被删除"
    );
    // 清理后是「未打开」，不是空表：读仍按缺失回错误。
    assert_eq!(
      replica.get(&tbl, b"k").await.unwrap_err(),
      KvError::ResourceMissing
    );

    replica.shutdown().await.unwrap();
    master.shutdown().await.unwrap();
  });
}

/// rev-46：master 无该分区时懒开不得装空表，按既有挂起重排后回
/// `ResourceMissing`（对齐 eng-39c）。
#[test]
fn replica_lazy_open_missing_master_does_not_create_empty_table() {
  if !rsync_available() {
    eprintln!("skip: rsync not available");
    return;
  }
  let master_root = TempRoot::new("runtime_standby_lazy_absent_master");
  let replica_root = TempRoot::new("runtime_standby_lazy_absent_replica");
  let tbl = TableIdent::new("standby_tbl", 0).unwrap();
  pollster::block_on(async {
    let master = Runtime::start(config(master_root.path().to_path_buf())).unwrap();
    master.attach_shard(0, "main", 1).await.unwrap();

    let replica = Runtime::start_with_option(
      config(replica_root.path().to_path_buf()),
      &replica_options(replica_root.path(), master_root.path()),
    )
    .unwrap();
    replica.attach_shard(0, "main", 1).await.unwrap();
    assert_eq!(
      replica.get(&tbl, b"a").await.unwrap_err(),
      KvError::ResourceMissing
    );
    assert!(
      !table_dir(replica_root.path(), &tbl).exists(),
      "懒开失败不得创建本地空表目录"
    );

    replica.shutdown().await.unwrap();
    master.shutdown().await.unwrap();
  });
}
