//! `Runtime::start_with_option`：KvOption 驱动 checksum / 压缩等配置。
use std::{
  env::temp_dir,
  fs::{create_dir_all, read, read_dir, remove_dir_all, write},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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
    "fkv_runtime_opts_{tag}_{}_{}_{}",
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
fn kv_options_skip_verify_checksum_reaches_store() {
  let root = temp_root("checksum");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let option = KvOption {
    skip_verify_checksum: true,
    ..KvOption::default()
  };
  pollster::block_on(async {
    let runtime = Runtime::start_with_option(config(root.clone()), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      )
      .await
      .unwrap();
    runtime.shutdown().await.unwrap();
  });

  // 破坏 data page 前缀 checksum（term 参与文件名，按目录查找）。
  let dir = root.join("demo.1");
  let path = read_dir(&dir)
    .unwrap()
    .flatten()
    .map(|entry| entry.path())
    .find(|path| {
      path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("data_"))
    })
    .expect("data file must exist");
  let mut bytes = read(&path).unwrap();
  bytes[0] ^= 0xff;
  write(&path, &bytes).unwrap();

  pollster::block_on(async {
    // skip_verify_checksum = true：可读。
    let runtime = Runtime::start_with_option(config(root.clone()), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    runtime.shutdown().await.unwrap();
  });
  pollster::block_on(async {
    // 默认校验开启：读取失败。
    let strict = KvOption::default();
    let runtime = Runtime::start_with_option(config(root.clone()), &strict).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    assert!(runtime.get(&tbl, b"a").await.is_err());
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}

/// eng-69c：`index_cache_total_bytes` 从 `KvOption` 透传到 shard/store，
/// 聚合上限独立于每表预算生效。
#[test]
fn index_cache_total_bytes_reaches_store() {
  let root = temp_root("index_total");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let option = KvOption {
    // 每表预算维持默认（64 MiB），聚合上限压到 512 字节：必须触发逐出。
    index_cache_total_bytes: 512,
    ..KvOption::default()
  };
  let batch: Vec<BatchEntry> = (0..2000)
    .map(|idx| BatchEntry::upsert(format!("k{idx:04}").into_bytes(), vec![b'v'; 8], 1))
    .collect();
  pollster::block_on(async {
    let runtime = Runtime::start_with_option(config(root.clone()), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    runtime.put(&tbl, batch).await.unwrap();
    for idx in (0..2000).step_by(7) {
      assert_eq!(
        runtime
          .get(&tbl, format!("k{idx:04}").as_bytes())
          .await
          .unwrap(),
        Some(vec![b'v'; 8])
      );
    }
    let snap = &runtime.metrics().shard_li[0];
    // 未接通时只有每表 64 MiB 预算，逐出为 0、占用远超页大小。
    assert!(snap.cache_evict > 0, "聚合上限必须触发逐出: {snap:?}");
    assert!(snap.cache_bytes <= 2048, "聚合上限必须约束占用: {snap:?}");
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}

/// fmt-18：`index_page_restart_interval` 与 `data_page_restart_interval` 各自透传到
/// shard/store。两值不等（index 更密）时写入 / 扫描 / reopen 语义不变。
#[test]
fn restart_intervals_reach_shard_store() {
  let root = temp_root("restart");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let option = KvOption {
    data_page_restart_interval: 8,
    index_page_restart_interval: 2,
    ..KvOption::default()
  };
  let batch: Vec<BatchEntry> = (0..200)
    .map(|idx| BatchEntry::upsert(format!("k{idx:04}").into_bytes(), vec![b'v'; 8], 1))
    .collect();
  pollster::block_on(async {
    let runtime = Runtime::start_with_option(config(root.clone()), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    runtime.put(&tbl, batch.clone()).await.unwrap();
    assert_eq!(runtime.scan(&tbl, b"", b"", 0).await.unwrap().len(), 200);
    runtime.shutdown().await.unwrap();
  });
  pollster::block_on(async {
    // 重新打开：index 页按 ri=2 编码，读路径照常定位。
    let runtime = Runtime::start_with_option(config(root.clone()), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let all = runtime.scan(&tbl, b"", b"", 0).await.unwrap();
    assert_eq!(all.len(), 200);
    assert_eq!(
      runtime.get(&tbl, b"k0199").await.unwrap(),
      Some(vec![b'v'; 8])
    );
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}

/// option 的 `auto_reopen_*` 非默认值覆盖 `RuntimeConfig` 的同名默认值：
/// 缺失表按 option 的 1 次重试 / 10ms 间隔快速失败，不会落到 config 的 10s 间隔。
#[test]
fn kv_options_auto_reopen_overrides_runtime_config() {
  let root = temp_root("auto_reopen");
  let default = KvOption::default();
  let runtime_cfg = RuntimeConfig {
    auto_reopen_retry_times: default.auto_reopen_retry_times,
    auto_reopen_pending_time_us: default.auto_reopen_pending_time_us,
    ..config(root.clone())
  };
  let option = KvOption {
    auto_reopen_retry_times: 1,
    auto_reopen_pending_time_us: 10_000,
    ..default
  };
  pollster::block_on(async {
    let runtime = Runtime::start_with_option(runtime_cfg, &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let started = Instant::now();
    assert_eq!(
      runtime
        .reopen(&TableIdent::new("missing", 1).unwrap())
        .await
        .unwrap_err(),
      KvError::ResourceMissing
    );
    assert!(
      started.elapsed() < Duration::from_secs(1),
      "option 的 auto_reopen 未生效：{:?}",
      started.elapsed()
    );
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}

/// 两侧都显式且不一致直接拒绝，不静默择一。
#[test]
fn kv_options_conflicting_page_size_rejected() {
  let root = temp_root("conflict");
  let option = KvOption {
    data_page_size: 8192,
    ..KvOption::default()
  };
  // `config()` 的 page_size=512 非默认，与 option 的 8192 冲突。
  match Runtime::start_with_option(config(root.clone()), &option) {
    Err(err) => assert_eq!(err, KvError::InvalidArgs),
    Ok(_) => panic!("显式冲突必须拒绝"),
  }
  remove_dir_all(root).unwrap();
}
