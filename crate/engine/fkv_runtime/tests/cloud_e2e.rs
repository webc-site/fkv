//! Cloud 模式真 S3 端到端：归档 crond 的远端目标 + `global_*` 回执。
//!
//! 设置 `EKV_S3_ENDPOINT`（本地 rustfs / minio，见 `docker/docker-compose.yml`）
//! 后启用；未设置时明确打印 skip 并返回，不构造假 S3。
//!
//! 覆盖 12c 的两条 Cloud 路径：
//! - `ArchiveTarget::Cloud`：远端已 seed、**本地未 cache** 的分区也要被归档
//!   crond 列举并写出远端归档 manifest。
//! - `Runtime::global_archive` / `global_create_branch` / `global_drop_table`
//!   的目标集合来自远端列举，回执条数等于远端分区数。

use std::{
  env::{temp_dir, var},
  fs::{create_dir_all, remove_dir_all},
  path::{Path, PathBuf},
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  thread::sleep,
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use compio::runtime;
use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_config::KvOption;
use fkv_io_compio::CompioStore;
use fkv_object::ObjectStore;
use fkv_object_s3::{S3Config, S3ObjectStore};
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_store::Store;
use fkv_types::{InstallOutcome, TableIdent};

fn s3_config() -> Option<S3Config> {
  let endpoint = var("EKV_S3_ENDPOINT").ok()?;
  Some(S3Config {
    bucket: var("EKV_S3_BUCKET").unwrap_or_else(|_| "fkv-test".to_owned()),
    endpoint,
    region: var("EKV_S3_REGION").unwrap_or_else(|_| "us-east-1".to_owned()),
    access_key: var("EKV_S3_ACCESS_KEY").unwrap_or_else(|_| "minioadmin".to_owned()),
    secret_key: var("EKV_S3_SECRET_KEY").unwrap_or_else(|_| "minioadmin".to_owned()),
    allow_http: true,
    ..S3Config::default()
  })
}

fn temp_root(tag: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let root = temp_dir().join(format!(
    "fkv_runtime_cloud_e2e_{tag}_{}_{}_{}",
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

fn config(root: PathBuf) -> RuntimeConfig {
  RuntimeConfig {
    num_threads: 1,
    root,
    page_size: 512,
    restart_interval: 16,
    pages_per_file_shift: fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    segment_size: 4096,
    segments_per_file_shift: 2,
    large_value_threshold: 1024,
    fixed_buffer_size: 0,
    auto_reopen_retry_times: 0,
    auto_reopen_pending_time_us: 0,
    // 1 秒一轮：测试直接等远端出现归档 manifest。
    archive_interval_secs: 1,
    archive_num_retained: 100,
    queue_size: 64,
  }
}

fn unique_tag() -> String {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  format!(
    "{}-{}",
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos(),
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
  )
}

/// 远端 seed：两个分区各写一份 manifest + data，本地 seed 目录随后丢弃。
///
/// `CompioStore` 的页 IO 必须在 compio runtime 内驱动；测试外层的 pollster
/// 只能驱动纯 future，不能提供 compio 的 thread-local runtime。
fn seed_remote(s3: &S3Config, prefix: &str, seed_root: &Path, tbl_li: &[TableIdent]) {
  runtime::Runtime::new().unwrap().block_on(async {
    let remote = S3ObjectStore::start(s3.clone()).unwrap();
    let local = CompioStore::new(
      seed_root.to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
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
    for tbl in tbl_li {
      store
        .put(tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
        .await
        .unwrap();
    }
  });
}

async fn remote_key_li(s3: &S3Config, prefix: &str) -> Vec<String> {
  let remote = S3ObjectStore::start(s3.clone()).unwrap();
  remote
    .list(&format!("{prefix}/"))
    .await
    .unwrap()
    .into_iter()
    .map(|meta| meta.key)
    .collect()
}

/// 指定分区的远端归档 manifest 数（`manifest_<branch>_<term>_<tag>`）。
fn archive_manifest_num(key_li: &[String], prefix: &str, tbl: &TableIdent) -> usize {
  let dir = format!("{prefix}/{}.{}/", tbl.tbl_name, tbl.partition_id);
  key_li
    .iter()
    .filter_map(|key| key.strip_prefix(&dir))
    .filter(|name| {
      fkv_types::parse_manifest_file_name(name).is_some_and(|(_, _, tag)| tag.is_some())
    })
    .count()
}

async fn cleanup(s3: &S3Config, prefix: &str) {
  let Ok(remote) = S3ObjectStore::start(s3.clone()) else {
    return;
  };
  let Ok(meta_li) = remote.list(&format!("{prefix}/")).await else {
    return;
  };
  for meta in meta_li {
    let _ = remote.delete(&meta.key).await;
  }
}

#[test]
fn cloud_archive_crond_and_global_requests_hit_remote_targets() {
  let Some(s3) = s3_config() else {
    eprintln!("EKV_S3_ENDPOINT not set, skip cloud e2e test");
    return;
  };
  let prefix = format!("fkv-cloud-e2e/{}", unique_tag());
  let store_path = format!("{}/{prefix}", s3.bucket);
  let runtime_root = temp_root("runtime");
  let seed_root = temp_root("seed");
  let tbl_li = [
    TableIdent::new("demo", 0).unwrap(),
    TableIdent::new("demo", 1).unwrap(),
  ];

  pollster::block_on(async {
    seed_remote(&s3, &prefix, &seed_root, &tbl_li);

    let option = KvOption {
      cloud_store_path: store_path.clone(),
      cloud_endpoint: s3.endpoint.clone(),
      cloud_region: s3.region.clone(),
      cloud_access_key: s3.access_key.clone(),
      cloud_secret_key: s3.secret_key.clone(),
      ..KvOption::default()
    };
    let runtime = Runtime::start_with_option(config(runtime_root.clone()), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();

    // 1) 归档 crond：远端列举两个分区（本地 cache 尚未打开/下载），逐个写远端
    //    archive manifest。轮询到两个分区各 1 条为止。
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
      let key_li = remote_key_li(&s3, &prefix).await;
      let archived = tbl_li
        .iter()
        .map(|tbl| archive_manifest_num(&key_li, &prefix, tbl))
        .sum::<usize>();
      if archived >= tbl_li.len() {
        break;
      }
      assert!(
        Instant::now() < deadline,
        "archive crond timeout keys={key_li:?}"
      );
      sleep(Duration::from_millis(200));
    }

    // 2) global_archive：远端目标集合 = 2，回执条数一致且全部成功；调用方拿到
    //    归档名（同一 tag 广播到全部分区）。
    let reply = runtime.global_archive("e2e-tag", 100).await.unwrap();
    assert_eq!(reply.total(), tbl_li.len(), "{reply:?}");
    assert_eq!(reply.ok_num, tbl_li.len(), "{reply:?}");
    assert_eq!(reply.tag, "e2e-tag");
    assert!(reply.name.ends_with("_e2e-tag"), "{reply:?}");

    // 3) global_create_branch：同样按远端分区 fanout，返回归一化 + salt 的内部名。
    let reply = runtime.global_create_branch("snap").await.unwrap();
    assert_eq!(reply.total(), tbl_li.len(), "{reply:?}");
    assert_eq!(reply.ok_num, tbl_li.len(), "{reply:?}");
    assert!(reply.name.starts_with("snap-"), "{reply:?}");
    assert_eq!(reply.name.len(), "snap-".len() + 8, "{reply:?}");

    // 4) 全局按 tag 重开归档 + 单分区强制回源（本地 cache 可能只有旧状态）。
    let reply = runtime.global_reopen("e2e-tag").await.unwrap();
    assert_eq!(reply.total(), tbl_li.len(), "{reply:?}");
    assert_eq!(reply.ok_num, tbl_li.len(), "{reply:?}");
    assert_eq!(
      runtime.reopen_external(&tbl_li[0]).await.unwrap(),
      InstallOutcome::Installed
    );

    // 5) global_drop_table：按表名逐分区 Drop，回执条数仍与远端分区数一致。
    let reply = runtime.global_drop_table("demo").await.unwrap();
    assert_eq!(reply.total(), tbl_li.len(), "{reply:?}");
    assert_eq!(reply.ok_num, tbl_li.len(), "{reply:?}");

    runtime.shutdown().await.unwrap();
  });

  pollster::block_on(cleanup(&s3, &prefix));
  let _ = remove_dir_all(&runtime_root);
  let _ = remove_dir_all(&seed_root);
}
