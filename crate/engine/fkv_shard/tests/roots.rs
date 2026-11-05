//! 12c：`store_path` 多根本地后端按 LUT 放置分区。
//!
//! 两条等权根（LUT `[0, 1]`）下，`partition_id % 2` 决定根；读写都落在该根，
//! 不在根之间复制或迁移数据。

use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::{Path, PathBuf},
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io::ManifestIo;
use fkv_shard::{Backend, RootedConfig, RootedStore, Shard, ShardConfig};
use fkv_types::{StoreMode, TableIdent};

fn config(root: PathBuf, backend: Backend) -> ShardConfig {
  ShardConfig {
    shard_id: 0,
    root,
    page_size: 512,
    restart_interval: 16,
    index_restart_interval: 16,
    pages_per_file_shift: 18,
    segment_size: 4096,
    segments_per_file_shift: 2,
    large_value_threshold: 1024,
    fixed_buffer_size: 0,
    queue_size: 32,
    auto_reopen_retry_times: 0,
    auto_reopen_pending_time_us: 0,
    oom_retry_times: 0,
    compression_level: None,
    verify_checksum: true,
    overflow_pointers: fkv_page::DEFAULT_OVERFLOW_POINTERS,
    file_amplify_factor: 0,
    segment_file_amplify_factor: 0,
    manifest_limit: 8 * 1024 * 1024,
    max_write_batch_pages: 32,
    max_inflight_write: 32 * 1024,
    fd_limit: 0,
    index_cache_bytes: fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize,
    index_cache_total_bytes: fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize,
    root_meta_cache_bytes: 0,
    io_queue_size: 4096,
    backend,
    mode: StoreMode::Local,
    standby: None,
    max_write_concurrency: 0,
    max_processing_time_us: 0,
    yield_budget_us: 0,
  }
}

fn temp_root(tag: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let root = temp_dir().join(format!(
    "fkv-roots-{tag}-{}-{}-{}",
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

fn partition_dir(root: &Path, tbl: &TableIdent) -> PathBuf {
  root.join(format!("{}.{}", tbl.tbl_name, tbl.partition_id))
}

/// 空根集合 / 空 LUT 必须在装配期拒绝，运行期选根不做越界检查。
#[test]
fn rooted_config_rejects_empty_roots() {
  assert_eq!(
    RootedConfig::new(Vec::new(), vec![0]).unwrap_err(),
    KvError::InvalidArgs
  );
  assert_eq!(
    RootedConfig::new(vec![PathBuf::from("root")], Vec::new()).unwrap_err(),
    KvError::InvalidArgs
  );
}

#[compio::test]
async fn local_routed_places_partition_on_lut_root() {
  let root_a = temp_root("a");
  let root_b = temp_root("b");
  let rooted = RootedConfig::new(vec![root_a.clone(), root_b.clone()], vec![0, 1]).unwrap();
  let shard = Shard::start(config(root_a.clone(), Backend::LocalRouted(rooted))).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();

  let even = TableIdent::new("demo", 0).unwrap();
  let odd = TableIdent::new("demo", 1).unwrap();
  for (tbl, value) in [(&even, b"even".to_vec()), (&odd, b"odd".to_vec())] {
    assert!(matches!(
      shard
        .request(Request::Put {
          tbl: tbl.clone(),
          batch_li: vec![BatchEntry::upsert(b"k".to_vec(), value, 1)],
          term: None,
        })
        .await
        .unwrap(),
      Response::Put
    ));
  }

  // 分区 0 → root_a，分区 1 → root_b；另一个根没有该分区目录。
  assert!(partition_dir(&root_a, &even).is_dir());
  assert!(!partition_dir(&root_b, &even).exists());
  assert!(partition_dir(&root_b, &odd).is_dir());
  assert!(!partition_dir(&root_a, &odd).exists());

  // reopen 后仍从同一根读回：LUT 是稳定映射。
  shard
    .request(Request::Reopen { tbl: even.clone() })
    .await
    .unwrap();
  assert!(matches!(
    shard
      .request(Request::Get {
        tbl: even.clone(),
        key: b"k".to_vec(),
      })
      .await
      .unwrap(),
    Response::Get(Some(value)) if value == b"even"
  ));

  shard.shutdown().await.unwrap();
  let _ = remove_dir_all(&root_a);
  let _ = remove_dir_all(&root_b);
}

/// 包装层必须转发旁路文件能力：漏转发会让 segment 完成标记永久残留，
/// 分区目录也无法回收。
#[compio::test]
async fn rooted_store_forwards_sidecar_file_li() {
  let root = temp_root("sidecar");
  let config = config(root.clone(), Backend::Local);
  let store = RootedStore::new(&config, &RootedConfig::single(root.clone()));
  let tbl = TableIdent::new("sidecar", 1).unwrap();

  ManifestIo::write_manifest_file(&store, &tbl, "segcomplete_7", b"magic")
    .await
    .unwrap();
  let name_li = store.list_sidecar_file_name_li(&tbl).await.unwrap();
  assert!(
    name_li.iter().any(|name| name == "segcomplete_7"),
    "{name_li:?}"
  );

  store
    .remove_sidecar_file_name(&tbl, "segcomplete_7")
    .await
    .unwrap();
  assert!(
    store
      .list_sidecar_file_name_li(&tbl)
      .await
      .unwrap()
      .is_empty(),
    "旁路删除必须落到对应根"
  );
  let _ = remove_dir_all(root);
}
