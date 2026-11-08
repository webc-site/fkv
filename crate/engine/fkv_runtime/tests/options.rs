//! `Runtime::start_with_option`：KvOption 驱动 checksum / 压缩等配置。
use std::{
  fs::{read, read_dir, write},
  path::PathBuf,
  time::{Duration, Instant},
};

use fkv_btree::BatchEntry;
use fkv_config::KvOption;
use fkv_error::KvError;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn config(root: PathBuf) -> RuntimeConfig {
  RuntimeConfig::for_tests(root)
}

#[test]
fn kv_options_skip_verify_checksum_reaches_store() {
  let root = TempRoot::new("runtime_opts_checksum");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let option = KvOption {
    skip_verify_checksum: true,
    ..KvOption::default()
  };
  pollster::block_on(async {
    let runtime = Runtime::start_with_option(config(root.path().to_path_buf()), &option).unwrap();
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
    let runtime = Runtime::start_with_option(config(root.path().to_path_buf()), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    runtime.shutdown().await.unwrap();
  });
  pollster::block_on(async {
    // 默认校验开启：读取失败。
    let strict = KvOption::default();
    let runtime = Runtime::start_with_option(config(root.path().to_path_buf()), &strict).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    assert!(runtime.get(&tbl, b"a").await.is_err());
    runtime.shutdown().await.unwrap();
  });
}

/// fmt-18：`index_page_restart_interval` 与 `data_page_restart_interval` 各自透传到
/// shard/store。两值不等（index 更密）时写入 / 扫描 / reopen 语义不变。
#[test]
fn restart_intervals_reach_shard_store() {
  let root = TempRoot::new("runtime_opts_restart");
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
    let runtime = Runtime::start_with_option(config(root.path().to_path_buf()), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    runtime.put(&tbl, batch.clone()).await.unwrap();
    assert_eq!(runtime.scan(&tbl, b"", b"", 0).await.unwrap().len(), 200);
    runtime.shutdown().await.unwrap();
  });
  pollster::block_on(async {
    // 重新打开：index 页按 ri=2 编码，读路径照常定位。
    let runtime = Runtime::start_with_option(config(root.path().to_path_buf()), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let all = runtime.scan(&tbl, b"", b"", 0).await.unwrap();
    assert_eq!(all.len(), 200);
    assert_eq!(
      runtime.get(&tbl, b"k0199").await.unwrap(),
      Some(vec![b'v'; 8])
    );
    runtime.shutdown().await.unwrap();
  });
}

/// option 的 `auto_reopen_*` 非默认值覆盖 `RuntimeConfig` 的同名默认值：
/// 缺失表按 option 的 1 次重试 / 10ms 间隔快速失败，不会落到 config 的 10s 间隔。
#[test]
fn kv_options_auto_reopen_overrides_runtime_config() {
  let root = TempRoot::new("runtime_opts_auto_reopen");
  let default = KvOption::default();
  let runtime_cfg = RuntimeConfig {
    auto_reopen_retry_times: default.auto_reopen_retry_times,
    auto_reopen_pending_time_us: default.auto_reopen_pending_time_us,
    ..config(root.path().to_path_buf())
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
}

/// 两侧都显式且不一致直接拒绝，不静默择一。
#[test]
fn kv_options_conflicting_page_size_rejected() {
  let root = TempRoot::new("runtime_opts_conflict");
  let option = KvOption {
    data_page_size: 8192,
    ..KvOption::default()
  };
  // `config()` 的 page_size=512 非默认，与 option 的 8192 冲突。
  match Runtime::start_with_option(config(root.path().to_path_buf()), &option) {
    Err(err) => assert_eq!(err, KvError::InvalidArgs),
    Ok(_) => panic!("显式冲突必须拒绝"),
  }
}
