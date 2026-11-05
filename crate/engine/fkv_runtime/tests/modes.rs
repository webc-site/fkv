//! `Runtime::start_with_option` 按 `KvOption` 的运行模式装配 IO 后端。
use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_config::KvOption;
use fkv_error::KvError;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_types::TableIdent;

fn temp_root(tag: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_runtime_mode_{tag}_{}_{}_{}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos(),
    seq
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
    auto_reopen_retry_times: 0,
    auto_reopen_pending_time_us: 0,
    archive_interval_secs: 0,
    archive_num_retained: 100,
    queue_size: 64,
  }
}

#[test]
fn local_mode_put_get_reopen() {
  let root = temp_root("local");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let option = KvOption::default();
  pollster::block_on(async {
    let runtime = Runtime::start_with_option(config(root.clone()), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"k").await.unwrap(), Some(b"v".to_vec()));
    // 两类 GC 请求都经 shard 往返：Local 模式下没有远端，两者都只回收本地文件。
    runtime.file_gc(&tbl).await.unwrap();
    runtime.local_gc(&tbl).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"k").await.unwrap(), Some(b"v".to_vec()));
    runtime.reopen(&tbl).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"k").await.unwrap(), Some(b"v".to_vec()));
    runtime.shutdown().await.unwrap();
  });
  let _ = remove_dir_all(&root);
}

#[test]
fn cloud_mode_starts_and_shuts_down() {
  let root = temp_root("cloud");
  let option = KvOption {
    cloud_store_path: "fkv-test/prefix".to_owned(),
    cloud_endpoint: "http://127.0.0.1:1".to_owned(),
    ..KvOption::default()
  };
  pollster::block_on(async {
    // 只验证按 Cloud 后端完成装配（S3 worker 启动、本地 cache 就绪），
    // 真实读写需外部 S3，见 `fkv_cloud/tests/s3_rustfs.rs`。
    let runtime = Runtime::start_with_option(config(root.clone()), &option).unwrap();
    runtime.shutdown().await.unwrap();
  });
  let _ = remove_dir_all(&root);
}

#[test]
fn standby_modes_start_and_shut_down() {
  let root = temp_root("standby");
  let mut option = KvOption {
    enable_local_standby: true,
    store_path: vec![root.to_string_lossy().into_owned()],
    ..KvOption::default()
  };
  // master 地址为空 -> StandbyMaster。
  assert!(option.validate().is_ok());
  pollster::block_on(async {
    let runtime = Runtime::start_with_option(config(root.clone()), &option).unwrap();
    runtime.shutdown().await.unwrap();
  });
  // 有 master 路径 -> StandbyReplica；真正 rsync 发生在 reopen，不在这里。
  option.standby_master_addr = "local".to_owned();
  option.standby_master_store_paths = vec![root.join("master").to_string_lossy().into_owned()];
  assert!(option.validate().is_ok());
  pollster::block_on(async {
    let runtime = Runtime::start_with_option(config(root.clone()), &option).unwrap();
    runtime.shutdown().await.unwrap();
  });
  let _ = remove_dir_all(&root);
}

#[test]
fn standby_replica_requires_master_paths() {
  let root = temp_root("standby_no_paths");
  let option = KvOption {
    enable_local_standby: true,
    store_path: vec![root.to_string_lossy().into_owned()],
    standby_master_addr: "local".to_owned(),
    ..KvOption::default()
  };
  // 副本没有 master store 路径就无从同步：启动即报错，不假装成功。
  let err = match Runtime::start_with_option(config(root.clone()), &option) {
    Ok(_) => panic!("expected start failure"),
    Err(err) => err,
  };
  assert_eq!(err, KvError::InvalidArgs);
  let _ = remove_dir_all(&root);
}

#[test]
fn mem_mode_put_get_without_disk() {
  let root = temp_root("mem");
  let tbl = TableIdent::new("demo", 1).unwrap();
  pollster::block_on(async {
    // `start_mem` 是 `KvOption` 之外唯一的 Mem 入口：`StoreMode` 没有 Mem。
    let runtime = Runtime::start_mem(RuntimeConfig::new(root.clone())).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"k").await.unwrap(), Some(b"v".to_vec()));
    // Mem 没有本地根：全局列举目标集合为空，不扫 `RuntimeConfig::root`。
    assert!(
      runtime
        .global_list_archive_tags("")
        .await
        .unwrap()
        .is_empty()
    );
    runtime.shutdown().await.unwrap();
  });
  assert!(!root.join("demo.1").exists());
  let _ = remove_dir_all(&root);
}
