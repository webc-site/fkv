//! Cloud 模式预热接线：`prewarm_cloud_cache` 打开后由 attach 触发后台预热。
//!
//! 断言分两层：
//! - 装配与开关语义：不需要 S3，覆盖「默认不开预热」「开了但未 attach 无回执」「shutdown 正常」。
//! - 端到端（列举远端分区 → 落到本地 cache）需要可用的 S3：设置 `EKV_S3_ENDPOINT`
//!   后启用，否则跳过（同 `fkv_cloud/tests/s3_rustfs.rs` 的约定）。

use std::{
  fs,
  path::{Path, PathBuf},
  sync::atomic::{AtomicU64, Ordering},
  thread::sleep,
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_config::KvOption;
use fkv_io_mem::MemStore;
use fkv_object::ObjectStore;
use fkv_object_s3::{S3Config, S3ObjectStore};
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_store::Store;
use fkv_testkit::{S3Env, TempRoot, s3_env};
use fkv_types::TableIdent;

fn config(root: PathBuf) -> RuntimeConfig {
  RuntimeConfig::for_tests(root)
}

/// 本地 cache 根目录下的文件总数（预热落盘的可观测证据）。
fn file_num(root: &Path) -> usize {
  let Ok(entry_li) = fs::read_dir(root) else {
    return 0;
  };
  entry_li
    .flatten()
    .map(|entry| match entry.file_type() {
      Ok(kind) if kind.is_dir() => file_num(&entry.path()),
      Ok(_) => 1,
      Err(_) => 0,
    })
    .sum()
}

fn s3_config() -> Option<S3Config> {
  let S3Env {
    bucket,
    endpoint,
    region,
    access_key,
    secret_key,
    allow_http,
  } = s3_env()?;
  Some(S3Config {
    bucket,
    endpoint,
    region,
    access_key,
    secret_key,
    allow_http,
    ..S3Config::default()
  })
}

#[test]
fn cloud_prewarm_off_by_default_has_no_report_and_shuts_down() {
  let root = TempRoot::new("runtime_prewarm_off");
  let option = KvOption {
    cloud_store_path: "fkv-test/prefix".to_owned(),
    cloud_endpoint: "http://127.0.0.1:1".to_owned(),
    ..KvOption::default()
  };
  assert!(!option.prewarm_cloud_cache);
  pollster::block_on(async {
    let runtime = Runtime::start_with_option(config(root.path().to_path_buf()), &option).unwrap();
    // 未开预热：不起后台线程，也没有回执。
    assert!(runtime.prewarm_report().is_none());
    runtime.shutdown().await.unwrap();
  });
}

#[test]
fn cloud_prewarm_enabled_without_attach_has_no_report() {
  let root = TempRoot::new("runtime_prewarm_idle");
  let option = KvOption {
    cloud_store_path: "fkv-test/prefix".to_owned(),
    cloud_endpoint: "http://127.0.0.1:1".to_owned(),
    prewarm_cloud_cache: true,
    prewarm_task_count: 2,
    ..KvOption::default()
  };
  pollster::block_on(async {
    let runtime = Runtime::start_with_option(config(root.path().to_path_buf()), &option).unwrap();
    // 预热只在 attach 后跑：没有 attach 就没有回执，也没有本地文件。
    assert!(runtime.prewarm_report().is_none());
    assert_eq!(file_num(root.path()), 0);
    runtime.shutdown().await.unwrap();
  });
}

/// reopen 成功后按表触发后台预热（对齐 C++ `ReopenTask::Reopen` →
/// `PrewarmService::Prewarm`）：attach 之后远端新增的分区不因 `done_li`
/// 已置位而永远漏预热。需要可用的 S3，否则跳过。
#[test]
fn s3_reopen_triggers_per_table_prewarm() {
  let Some(s3) = s3_config() else {
    return;
  };
  let root = TempRoot::new("runtime_prewarm_reopen");
  let tbl = TableIdent::new("late", 1).unwrap();
  let prefix = format!("fkv-prewarm-reopen/{}", unique_tag());
  let store_path = format!("{}/{prefix}", s3.bucket);

  pollster::block_on(async {
    let option = KvOption {
      cloud_store_path: store_path.clone(),
      cloud_endpoint: s3.endpoint.clone(),
      cloud_region: s3.region.clone(),
      cloud_access_key: s3.access_key.clone(),
      cloud_secret_key: s3.secret_key.clone(),
      prewarm_cloud_cache: true,
      prewarm_task_count: 2,
      ..KvOption::default()
    };
    // attach 时远端为空：预热轮没有目标，也不产生回执。
    let runtime = Runtime::start_with_option(config(root.path().to_path_buf()), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    sleep(Duration::from_millis(300));
    assert!(runtime.prewarm_report().is_none());

    // attach 之后、reopen 之前由别的 writer 创建新分区。
    {
      let remote = S3ObjectStore::start(s3.clone()).unwrap();
      // 种子只用于把对象上传到远端；MemStore 不需要 compio runtime。
      let local = MemStore::new(512);
      let mut store = Store::new(
        CloudStore::new(
          local,
          remote,
          prefix.to_owned(),
          fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
        ),
        512,
        16,
      );
      store.prepare_attach("main", 1).unwrap();
      store
        .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
        .await
        .unwrap();
    }

    // reopen 回执不等待预热：只入队 + 唤醒 crond。
    runtime.reopen(&tbl).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let report = loop {
      if let Some(report) = runtime.prewarm_report()
        && report.table_num == 1
        && report.pulled_file_num >= 2
      {
        break report;
      }
      assert!(Instant::now() < deadline, "reopen 未触发 per-table 预热");
      sleep(Duration::from_millis(50));
    };
    assert!(!report.listing_failed, "{report:?}");
    runtime.shutdown().await.unwrap();
  });

  // 清理远端对象。
  pollster::block_on(async {
    if let Ok(remote) = S3ObjectStore::start(s3)
      && let Ok(meta_li) = remote.list(&format!("{prefix}/")).await
    {
      for meta in meta_li {
        let _ = remote.delete(&meta.key).await;
      }
    }
  });
}

#[test]
fn s3_attach_triggers_prewarm() {
  let Some(s3) = s3_config() else {
    return;
  };
  let root = TempRoot::new("runtime_prewarm_s3");
  let tbl = TableIdent::new("demo", 1).unwrap();
  // `cloud_store_path` = `bucket/prefix`；bucket 必须与 S3Config 一致。
  let prefix = format!("fkv-prewarm-it/{}", unique_tag());
  let store_path = format!("{}/{prefix}", s3.bucket);

  // 远端种子：以 term=1 写入 manifest + data（prewarm 按 attach 身份列举）。
  pollster::block_on(async {
    let remote = S3ObjectStore::start(s3.clone()).unwrap();
    // 种子只用于把对象上传到远端；MemStore 不需要 compio runtime。
    let local = MemStore::new(512);
    let mut store = Store::new(
      CloudStore::new(
        local,
        remote,
        prefix.to_owned(),
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    store.prepare_attach("main", 1).unwrap();
    store
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
      .await
      .unwrap();
  });

  pollster::block_on(async {
    let option = KvOption {
      cloud_store_path: store_path.clone(),
      cloud_endpoint: s3.endpoint.clone(),
      cloud_region: s3.region.clone(),
      cloud_access_key: s3.access_key.clone(),
      cloud_secret_key: s3.secret_key.clone(),
      prewarm_cloud_cache: true,
      prewarm_task_count: 2,
      ..KvOption::default()
    };
    let runtime = Runtime::start_with_option(config(root.path().to_path_buf()), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    // attach 只投递信号：预热在后台线程跑，轮询回执。
    let deadline = Instant::now() + Duration::from_secs(30);
    let report = loop {
      if let Some(report) = runtime.prewarm_report() {
        break report;
      }
      assert!(Instant::now() < deadline, "prewarm report timeout");
      sleep(Duration::from_millis(50));
    };
    assert!(!report.listing_failed, "{report:?}");
    assert_eq!(report.table_num, 1, "{report:?}");
    assert!(report.pulled_file_num >= 2, "{report:?}");
    assert!(
      file_num(root.path()) >= 2,
      "local cache should hold pulled files"
    );
    runtime.shutdown().await.unwrap();
  });

  // 清理远端对象。
  pollster::block_on(async {
    if let Ok(remote) = S3ObjectStore::start(s3)
      && let Ok(meta_li) = remote.list(&format!("{prefix}/")).await
    {
      for meta in meta_li {
        let _ = remote.delete(&meta.key).await;
      }
    }
  });
}

fn unique_tag() -> String {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  format!(
    "{}-{seq}",
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos()
  )
}

/// per-table `Prewarm` / `ListObject` 请求的 Cloud 端到端：
/// 1. attach 后、任何本地 cache 之前，`ListObject` 能直接列到远端对象；
/// 2. 显式 `Prewarm` 把 manifest / data 拉到本地 cache。
///
/// 需要可用的 S3（设置 `EKV_S3_ENDPOINT`），否则跳过。
#[test]
fn s3_per_table_prewarm_and_list_object_requests() {
  let Some(s3) = s3_config() else {
    return;
  };
  let root = TempRoot::new("runtime_prewarm_req");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let prefix = format!("fkv-prewarm-req/{}", unique_tag());
  let store_path = format!("{}/{prefix}", s3.bucket);

  pollster::block_on(async {
    // 远端种子：以 term=1 写入 manifest + data（预热按 attach 身份列举）。
    {
      let remote = S3ObjectStore::start(s3.clone()).unwrap();
      // 种子只用于把对象上传到远端；MemStore 不需要 compio runtime。
      let local = MemStore::new(512);
      let mut store = Store::new(
        CloudStore::new(
          local,
          remote,
          prefix.to_owned(),
          fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
        ),
        512,
        16,
      );
      store.prepare_attach("main", 1).unwrap();
      store
        .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
        .await
        .unwrap();
    }

    let option = KvOption {
      cloud_store_path: store_path.clone(),
      cloud_endpoint: s3.endpoint.clone(),
      cloud_region: s3.region.clone(),
      cloud_access_key: s3.access_key.clone(),
      cloud_secret_key: s3.secret_key.clone(),
      // 关掉后台 crond，确保本地文件来自显式 Prewarm。
      prewarm_cloud_cache: false,
      ..KvOption::default()
    };
    let runtime = Runtime::start_with_option(config(root.path().to_path_buf()), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let before = file_num(root.path());

    // ListObject 是远端列举，不依赖本地 cache；分区目录下的文件都在同一层，
    // 非递归（C++ 默认）即可列全。`prefix` 相对 store root（`cloud_store_path`
    // 的 prefix），返回名也相对它——不带 store root、不含 `/`。
    let page = runtime
      .list_object(&tbl, &format!("{tbl}/"), None, 100, false)
      .await
      .unwrap();
    assert!(!page.meta_li.is_empty(), "{page:?}");
    assert!(
      page
        .meta_li
        .iter()
        .any(|meta| meta.key.contains("manifest_main_1")),
      "{page:?}"
    );
    assert!(
      page
        .meta_li
        .iter()
        .any(|meta| meta.key.contains("data_0_main_1")),
      "{page:?}"
    );
    assert!(
      page.meta_li.iter().all(|meta| !meta.key.contains('/')),
      "返回名必须剥掉 store root 与分区前缀: {page:?}"
    );

    // 显式按分区预热：manifest + data 落本地 cache。
    runtime.prewarm(&tbl).await.unwrap();
    assert!(
      file_num(root.path()) >= before + 2,
      "before={before} after={}",
      file_num(root.path())
    );
    runtime.shutdown().await.unwrap();
  });

  // 清理远端对象。
  pollster::block_on(async {
    if let Ok(remote) = S3ObjectStore::start(s3)
      && let Ok(meta_li) = remote.list(&format!("{prefix}/")).await
    {
      for meta in meta_li {
        let _ = remote.delete(&meta.key).await;
      }
    }
  });
}
