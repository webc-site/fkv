//! RustFS / S3 集成测试。设置 `EKV_S3_ENDPOINT` 后启用，否则跳过。

use std::{
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_error::KvError;
use fkv_io::TermIo;
use fkv_io_mem::MemStore;
use fkv_object::ObjectStore;
use fkv_object_s3::{S3Config, S3ObjectStore};
use fkv_store::Store;
use fkv_testkit::{S3Env, s3_env};
use fkv_types::TableIdent;

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

fn test_prefix() -> String {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  format!(
    "fkv-it/{}-{}-{seq}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos()
  )
}

async fn cleanup(remote: &S3ObjectStore, prefix: &str) {
  for meta in remote
    .list(&format!("{prefix}/"))
    .await
    .expect("list objects for cleanup")
  {
    remote
      .delete(&meta.key)
      .await
      .expect("delete object for cleanup");
  }
}

#[test]
fn s3_whole_file_roundtrip_and_cache_redownload() {
  pollster::block_on(async {
    let Some(config) = s3_config() else {
      eprintln!("EKV_S3_ENDPOINT not set, skip RustFS integration test");
      return;
    };
    let prefix = test_prefix();
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = S3ObjectStore::start(config.clone()).unwrap();
    let mut store = Store::new(
      CloudStore::new(
        MemStore::with_pages_per_file(512, 2),
        remote,
        prefix.clone(),
        2,
      ),
      512,
      16,
    );
    let entry_li: Vec<BatchEntry> = (0..64)
      .map(|index| {
        BatchEntry::upsert(
          format!("k{index:04}").into_bytes(),
          format!("v{index:04}").repeat(4).into_bytes(),
          1,
        )
      })
      .collect();
    store.put(&tbl, &entry_li).await.unwrap();
    for entry in &entry_li {
      assert_eq!(
        store.get(&tbl, &entry.key).await.unwrap(),
        Some(entry.value.clone())
      );
    }
    let (_, first_remote) = store.into_io().into_parts();
    let data_li: Vec<_> = first_remote
      .list(&format!("{prefix}/demo.1/"))
      .await
      .unwrap()
      .into_iter()
      .filter(|meta| {
        meta
          .key
          .rsplit('/')
          .next()
          .is_some_and(|name| name.starts_with("data_"))
      })
      .collect();
    assert!(!data_li.is_empty(), "data files must be uploaded");
    assert!(
      data_li.iter().all(|meta| meta.size.is_multiple_of(512)),
      "objects must be whole data files"
    );
    assert!(
      first_remote
        .list(&format!("{prefix}/demo.1/page/"))
        .await
        .unwrap()
        .is_empty(),
      "per-page objects must not be uploaded"
    );

    // 全新本地 cache + 重新 reopen，读取时按整文件从 S3 拉回。
    let remote = S3ObjectStore::start(config).unwrap();
    let mut cached = Store::new(
      CloudStore::new(
        MemStore::with_pages_per_file(512, 2),
        remote,
        prefix.clone(),
        2,
      )
      .with_cache_budget(4096),
      512,
      16,
    );
    cached.reopen(&tbl).await.unwrap();
    for entry in &entry_li {
      assert_eq!(
        cached.get(&tbl, &entry.key).await.unwrap(),
        Some(entry.value.clone())
      );
    }
    assert!(cached.page_io().local().page_count() >= 2);
    let (_, last_remote) = cached.into_io().into_parts();
    cleanup(&last_remote, &prefix).await;
    last_remote.shutdown().unwrap();
    first_remote.shutdown().unwrap();
  });
}

#[test]
fn s3_conditional_put_and_term_cas() {
  pollster::block_on(async {
    let Some(config) = s3_config() else {
      eprintln!("EKV_S3_ENDPOINT not set, skip RustFS integration test");
      return;
    };
    let prefix = test_prefix();
    let key = format!("{prefix}/cas/object");
    let remote = S3ObjectStore::start(config).unwrap();
    let etag1 = remote
      .put(&key, Bytes::from_static(b"one"), None, true)
      .await
      .unwrap();
    assert_eq!(
      remote
        .put(&key, Bytes::from_static(b"again"), None, true)
        .await
        .unwrap_err(),
      KvError::AlreadyExists
    );
    assert_eq!(
      remote
        .put(
          &key,
          Bytes::from_static(b"stale"),
          Some("stale-etag"),
          false
        )
        .await
        .unwrap_err(),
      KvError::Busy
    );
    let etag2 = remote
      .put(&key, Bytes::from_static(b"two"), Some(&etag1), false)
      .await
      .unwrap();
    assert_ne!(etag1, etag2);
    assert_eq!(remote.get(&key).await.unwrap().unwrap().as_ref(), b"two");
    assert_eq!(remote.head(&key).await.unwrap().unwrap().size, 3);

    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      prefix.clone(),
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    cloud.upsert_term("main", 1, 1).await.unwrap();
    cloud.upsert_term("main", 1, 2).await.unwrap();
    assert_eq!(cloud.read_term("main", 1).await.unwrap(), Some(2));
    assert_eq!(
      cloud.upsert_term("main", 1, 1).await.unwrap_err(),
      KvError::ExpiredTerm
    );
    let (_, remote) = cloud.into_parts();
    cleanup(&remote, &prefix).await;
    remote.shutdown().unwrap();
  });
}
