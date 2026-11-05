//! 12c：`store_path` / `store_path_weights` / `store_path_lut` 接入 runtime。
//!
//! 覆盖三类验收：
//! - 两路径等权：分区按 LUT 分布到两个根（盘上目录可观测）。
//! - 单根：保持现行为（所有分区落唯一根）。
//! - 权重 0 / 个数不符：启动前拒绝。
//!
//! 归档 crond 的 Local 目标集合也要覆盖全部根（不是只看 `RuntimeConfig::root`）。

use std::{
  env::temp_dir,
  fs::{create_dir_all, read_dir, remove_dir_all},
  path::{Path, PathBuf},
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  thread::sleep,
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_config::KvOption;
use fkv_error::KvError;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_types::TableIdent;

fn temp_root(tag: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let root = temp_dir().join(format!(
    "fkv_runtime_store_path_{tag}_{}_{}_{}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos(),
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
  ));
  create_dir_all(&root).unwrap();
  root
}

fn config(root: PathBuf, archive_interval_secs: u64) -> RuntimeConfig {
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
    auto_reopen_retry_times: 0,
    auto_reopen_pending_time_us: 0,
    archive_interval_secs,
    archive_num_retained: 100,
    queue_size: 64,
  }
}

fn partition_dir(root: &Path, tbl: &TableIdent) -> PathBuf {
  root.join(format!("{}.{}", tbl.tbl_name, tbl.partition_id))
}

fn store_path_options(root_li: &[&PathBuf], weight_li: &[u64]) -> KvOption {
  let path_li = root_li
    .iter()
    .map(|root| root.to_string_lossy().into_owned())
    .collect();
  KvOption {
    store_path: path_li,
    store_path_weights: weight_li.to_vec(),
    ..KvOption::default()
  }
}

#[test]
fn equal_weights_spread_partitions_across_roots() {
  let runtime_root = temp_root("cfg");
  let root_a = temp_root("a");
  let root_b = temp_root("b");
  let option = store_path_options(&[&root_a, &root_b], &[]);
  pollster::block_on(async {
    let runtime = Runtime::start_with_option(config(runtime_root.clone(), 0), &option).unwrap();
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
  assert!(partition_dir(&root_a, &TableIdent::new("demo", 0).unwrap()).is_dir());
  assert!(partition_dir(&root_b, &TableIdent::new("demo", 1).unwrap()).is_dir());
  assert!(!partition_dir(&root_b, &TableIdent::new("demo", 0).unwrap()).exists());
  assert!(!partition_dir(&root_a, &TableIdent::new("demo", 1).unwrap()).exists());
  assert_eq!(fs_file_num(&runtime_root), 0);
  let _ = remove_dir_all(&runtime_root);
  let _ = remove_dir_all(&root_a);
  let _ = remove_dir_all(&root_b);
}

#[test]
fn single_store_path_keeps_single_root_behavior() {
  let runtime_root = temp_root("cfg");
  let root_a = temp_root("only");
  let option = store_path_options(&[&root_a], &[]);
  pollster::block_on(async {
    let runtime = Runtime::start_with_option(config(runtime_root.clone(), 0), &option).unwrap();
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
      partition_dir(&root_a, &TableIdent::new("demo", partition).unwrap()).is_dir(),
      "单根时全部分区落 store_path"
    );
  }
  let _ = remove_dir_all(&runtime_root);
  let _ = remove_dir_all(&root_a);
}

#[test]
fn invalid_weights_rejected_before_start() {
  let runtime_root = temp_root("cfg");
  let root_a = temp_root("a");
  let root_b = temp_root("b");
  for option in [
    store_path_options(&[&root_a, &root_b], &[1, 0]),
    store_path_options(&[&root_a, &root_b], &[1]),
  ] {
    let err = match Runtime::start_with_option(config(runtime_root.clone(), 0), &option) {
      Ok(_) => panic!("权重非法必须在启动前拒绝"),
      Err(err) => err,
    };
    assert_eq!(err, KvError::InvalidArgs);
  }
  let _ = remove_dir_all(&runtime_root);
  let _ = remove_dir_all(&root_a);
  let _ = remove_dir_all(&root_b);
}

/// 归档 crond 的 Local 目标：两个根上的分区都要进目标集合并各落归档 manifest。
#[test]
fn archive_crond_scans_all_store_paths() {
  let runtime_root = temp_root("cfg");
  let root_a = temp_root("a");
  let root_b = temp_root("b");
  let option = store_path_options(&[&root_a, &root_b], &[1, 1]);
  pollster::block_on(async {
    let runtime = Runtime::start_with_option(config(runtime_root.clone(), 1), &option).unwrap();
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
        &root_a,
        &TableIdent::new("demo", 0).unwrap(),
      )) + archive_file_num(&partition_dir(
        &root_b,
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
  let _ = remove_dir_all(&runtime_root);
  let _ = remove_dir_all(&root_a);
  let _ = remove_dir_all(&root_b);
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
