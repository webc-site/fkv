//! 12c：`store_path` / `store_path_weights` / `store_path_lut` 接入 runtime。
//!
//! 覆盖三类验收：
//! - 两路径等权：分区按 LUT 分布到两个根（盘上目录可观测）。
//! - 单根：保持现行为（所有分区落唯一根）。
//! - 权重 0 / 个数不符：启动前拒绝。
//!
//! 归档 crond 的 Local 目标集合也要覆盖全部根（不是只看 `RuntimeConfig::root`）。

use std::{
  fs::{self, read_dir},
  path::{Path, PathBuf},
  thread::sleep,
  time::{Duration, Instant},
};

use fkv_btree::BatchEntry;
use fkv_config::KvOption;
use fkv_error::KvError;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn config(root: PathBuf, archive_interval_secs: u64) -> RuntimeConfig {
  let mut config = RuntimeConfig::for_tests(root);
  config.archive_interval_secs = archive_interval_secs;
  config
}

fn partition_dir(root: &Path, tbl: &TableIdent) -> PathBuf {
  root.join(format!("{}.{}", tbl.tbl_name, tbl.partition_id))
}

fn store_path_options(root_li: &[&Path], weight_li: &[u64]) -> KvOption {
  let path_li = root_li
    .iter()
    .map(|root| root.to_string_lossy().into_owned())
    .collect();
  KvOption {
    store_path: path_li,
    store_path_weights: weight_li.to_vec(),
    // 归档 crond 与 C++ 一样只对 append 分区启动。
    data_append_mode: true,
    ..KvOption::default()
  }
}

#[test]
fn equal_weights_spread_partitions_across_roots() {
  let runtime_root = TempRoot::new("runtime_store_path_cfg");
  let root_a = TempRoot::new("runtime_store_path_a");
  let root_b = TempRoot::new("runtime_store_path_b");
  let option = store_path_options(&[root_a.path(), root_b.path()], &[]);
  pollster::block_on(async {
    let runtime =
      Runtime::start_with_option(config(runtime_root.path().to_path_buf(), 0), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    for partition in [0u32, 1] {
      let tbl = TableIdent::new("demo", partition).unwrap();
      runtime
        .put(
          &tbl,
          vec![BatchEntry::upsert(b"k".to_vec(), vec![partition as u8], 1)],
        )
        .await
        .unwrap();
    }
    runtime.shutdown().await.unwrap();
  });

  // 等权 LUT = [0, 1]：偶数分区根 A、奇数分区根 B；RuntimeConfig::root 不落数据。
  assert!(partition_dir(root_a.path(), &TableIdent::new("demo", 0).unwrap()).is_dir());
  assert!(partition_dir(root_b.path(), &TableIdent::new("demo", 1).unwrap()).is_dir());
  assert!(!partition_dir(root_b.path(), &TableIdent::new("demo", 0).unwrap()).exists());
  assert!(!partition_dir(root_a.path(), &TableIdent::new("demo", 1).unwrap()).exists());
  assert_eq!(fs_file_num(runtime_root.path()), 0);
}

#[test]
fn single_store_path_keeps_single_root_behavior() {
  let runtime_root = TempRoot::new("runtime_store_path_cfg");
  let root_a = TempRoot::new("runtime_store_path_only");
  let option = store_path_options(&[root_a.path()], &[]);
  pollster::block_on(async {
    let runtime =
      Runtime::start_with_option(config(runtime_root.path().to_path_buf(), 0), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    for partition in [0u32, 1] {
      let tbl = TableIdent::new("demo", partition).unwrap();
      runtime
        .put(
          &tbl,
          vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
        )
        .await
        .unwrap();
    }
    runtime.shutdown().await.unwrap();
  });
  for partition in [0u32, 1] {
    assert!(
      partition_dir(root_a.path(), &TableIdent::new("demo", partition).unwrap()).is_dir(),
      "单根时全部分区落 store_path"
    );
  }
}

#[test]
fn invalid_weights_rejected_before_start() {
  let runtime_root = TempRoot::new("runtime_store_path_cfg");
  let root_a = TempRoot::new("runtime_store_path_a");
  let root_b = TempRoot::new("runtime_store_path_b");
  for option in [
    store_path_options(&[root_a.path(), root_b.path()], &[1, 0]),
    store_path_options(&[root_a.path(), root_b.path()], &[1]),
  ] {
    let err =
      match Runtime::start_with_option(config(runtime_root.path().to_path_buf(), 0), &option) {
        Ok(_) => panic!("权重非法必须在启动前拒绝"),
        Err(err) => err,
      };
    assert_eq!(err, KvError::InvalidArgs);
  }
}

/// 归档 crond 的 Local 目标：两个根上的分区都要进目标集合并各落归档 manifest。
#[test]
fn archive_crond_scans_all_store_paths() {
  let runtime_root = TempRoot::new("runtime_store_path_cfg");
  let root_a = TempRoot::new("runtime_store_path_a");
  let root_b = TempRoot::new("runtime_store_path_b");
  let option = store_path_options(&[root_a.path(), root_b.path()], &[1, 1]);
  pollster::block_on(async {
    let runtime =
      Runtime::start_with_option(config(runtime_root.path().to_path_buf(), 1), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    for partition in [0u32, 1] {
      let tbl = TableIdent::new("demo", partition).unwrap();
      runtime
        .put(
          &tbl,
          vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
        )
        .await
        .unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
      let archived = archive_file_num(&partition_dir(
        root_a.path(),
        &TableIdent::new("demo", 0).unwrap(),
      )) + archive_file_num(&partition_dir(
        root_b.path(),
        &TableIdent::new("demo", 1).unwrap(),
      ));
      if archived >= 2 {
        break;
      }
      assert!(Instant::now() < deadline, "archive crond timeout");
      sleep(Duration::from_millis(100));
    }
    // 控制面列举同样覆盖全部根；同一归档周期两个分区使用同一个 tag，
    // C++ (tag, term) unique 语义下列表只保留一条。
    let tag_li = runtime.global_list_archive_tag_li("").await.unwrap();
    assert_eq!(tag_li.len(), 1, "{tag_li:?}");
    runtime.shutdown().await.unwrap();
  });
}

/// 根存在但为空（首次部署、尚未写入）：正常空根，全局归档 0 目标成功。
#[test]
fn global_archive_empty_root_is_empty_success() {
  let runtime_root = TempRoot::new("runtime_store_path_cfg");
  let root_a = TempRoot::new("runtime_store_path_empty");
  let option = store_path_options(&[root_a.path()], &[]);
  pollster::block_on(async {
    let runtime =
      Runtime::start_with_option(config(runtime_root.path().to_path_buf(), 0), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let reply = runtime.global_archive("20250101", 100).await.unwrap();
    assert_eq!(reply.total(), 0, "{reply:?}");
    assert_eq!(reply.ok_num, 0, "{reply:?}");
    assert!(reply.failed_li.is_empty(), "{reply:?}");
    assert_eq!(reply.tag, "20250101", "{reply:?}");
    runtime.shutdown().await.unwrap();
  });
}

/// 单根不存在：本仓启动不建根（分区目录懒创建），首次部署 / 根尚未落盘是
/// 正常场景，全局归档按 0 目标成功（C++ 在 `InitStoreSpace` 建根，无此分支）。
#[test]
fn global_archive_missing_root_is_empty_success() {
  let runtime_root = TempRoot::new("runtime_store_path_cfg");
  // 「缺失根」用活 TempRoot 下从未创建的子路径表达：attach 会经 term upsert
  // 的 create_dir_all 重建根，挂在 runtime_root 内可随 Drop 清理（struct-43：
  // 此前 into_path + remove_dir_all 的手法会让重建的根逃逸清理、逐轮泄漏）。
  let missing = runtime_root.path().join("missing");
  let option = store_path_options(&[missing.as_path()], &[]);
  pollster::block_on(async {
    let runtime =
      Runtime::start_with_option(config(runtime_root.path().to_path_buf(), 0), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let reply = runtime.global_archive("20250101", 100).await.unwrap();
    assert_eq!(reply.total(), 0, "{reply:?}");
    assert_eq!(reply.ok_num, 0, "{reply:?}");
    assert!(reply.failed_li.is_empty(), "{reply:?}");
    // tag 列举同样把缺失根当空根（两侧一致地静默跳过）。
    assert!(
      runtime
        .global_list_archive_tag_li("")
        .await
        .unwrap()
        .is_empty()
    );
    runtime.shutdown().await.unwrap();
  });
}

/// 多根里一个根不存在：其余根照常归档，整轮不失败（保留的「NotFound = 空根」
/// 口径）；缺失根不产出目标，回执里也不应出现它的分区。
#[test]
fn global_archive_missing_secondary_root_scans_readable_root() {
  let runtime_root = TempRoot::new("runtime_store_path_cfg");
  let root_a = TempRoot::new("runtime_store_path_a");
  // 缺失根挂在 runtime_root 下（不挂在 root_a 下，避免被根扫描当作分区目录），
  // attach 重建后随 runtime_root 的 Drop 清理（struct-43）。
  let missing = runtime_root.path().join("missing");
  let option = store_path_options(&[root_a.path(), missing.as_path()], &[1, 1]);
  pollster::block_on(async {
    let runtime =
      Runtime::start_with_option(config(runtime_root.path().to_path_buf(), 0), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("demo", 0).unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      )
      .await
      .unwrap();
    let reply = runtime.global_archive("20250101", 100).await.unwrap();
    // LUT [1, 1] = [0, 1]：分区 0 落根 A，缺失根无目标。
    assert_eq!(reply.total(), 1, "{reply:?}");
    assert_eq!(reply.ok_num, 1, "{reply:?}");
    assert!(reply.failed_li.is_empty(), "{reply:?}");
    assert_eq!(archive_file_num(&partition_dir(root_a.path(), &tbl)), 1);
    runtime.shutdown().await.unwrap();
  });
}

/// 多根里一个根存在但不可读：对齐 C++ `directory_iterator` 失败整轮回错，
/// 全局 archive / reopen / drop 都返回 `NoPermission`，绝不回「成功但漏根」。
/// tag 列举是唯一例外，两侧都静默跳过不可读根。
#[cfg(unix)]
#[test]
fn global_ops_unreadable_root_return_no_permission() {
  use std::os::unix::fs::PermissionsExt;

  let runtime_root = TempRoot::new("runtime_store_path_cfg");
  let root_a = TempRoot::new("runtime_store_path_a");
  let root_b = TempRoot::new("runtime_store_path_b");
  fs::set_permissions(root_b.path(), fs::Permissions::from_mode(0o000)).unwrap();
  // 以 root 运行时 chmod 000 仍可读，该用例没有意义：恢复权限后跳过。
  if read_dir(root_b.path()).is_ok() {
    fs::set_permissions(root_b.path(), fs::Permissions::from_mode(0o755)).unwrap();
    return;
  }
  let option = store_path_options(&[root_a.path(), root_b.path()], &[1, 1]);
  pollster::block_on(async {
    let runtime =
      Runtime::start_with_option(config(runtime_root.path().to_path_buf(), 0), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    for err in [
      runtime.global_archive("20250101", 100).await.unwrap_err(),
      runtime.global_reopen("").await.unwrap_err(),
      runtime.global_drop_table("demo").await.unwrap_err(),
    ] {
      assert_eq!(err, KvError::NoPermission);
    }
    assert!(
      runtime
        .global_list_archive_tag_li("")
        .await
        .unwrap()
        .is_empty()
    );
    runtime.shutdown().await.unwrap();
  });
  fs::set_permissions(root_b.path(), fs::Permissions::from_mode(0o755)).unwrap();
}

fn fs_file_num(root: &Path) -> usize {
  let Ok(entry_li) = read_dir(root) else {
    return 0;
  };
  entry_li
    .flatten()
    .map(|entry| match entry.file_type() {
      Ok(kind) if kind.is_dir() => fs_file_num(&entry.path()),
      Ok(_) => 1,
      Err(_) => 0,
    })
    .sum()
}

/// 归档 manifest 形如 `manifest_main_<term>_<tag>`（4 段）。
fn archive_file_num(dir: &Path) -> usize {
  let Ok(entry_li) = read_dir(dir) else {
    return 0;
  };
  entry_li
    .flatten()
    .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
    .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
    .filter(|name| {
      fkv_types::parse_manifest_file_name(name).is_some_and(|(_, _, tag)| tag.is_some())
    })
    .count()
}
