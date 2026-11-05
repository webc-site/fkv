//! `Backend::Mem`：枚举分派下的纯内存后端可完成 attach / put / get / reopen。

use std::{
  env::temp_dir,
  fs::{create_dir_all, read_dir, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io::ManifestIo;
use fkv_shard::{Backend, Shard, ShardConfig, StoreBackend};
use fkv_types::{StoreMode, TableIdent};

fn config(root: PathBuf) -> ShardConfig {
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
    backend: Backend::Mem,
    mode: StoreMode::Local,
    standby: None,
    max_write_concurrency: 0,
    max_processing_time_us: 0,
    yield_budget_us: 0,
  }
}

fn temp_root() -> PathBuf {
  // 并发测试可能在同一时钟刻度取到相同 nanos；序号保证目录唯一。
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let root = temp_dir().join(format!(
    "fkv-mem-backend-{}-{}-{}",
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

#[compio::test]
async fn mem_backend_put_get_reopen() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone())).unwrap();
  let tbl = TableIdent::new("mem", 1).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
  let batch_li = vec![
    BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
    BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
  ];
  assert!(matches!(
    shard
      .request(Request::Put {
        tbl: tbl.clone(),
        batch_li,
        term: None,
      })
      .await
      .unwrap(),
    Response::Put
  ));
  assert!(matches!(
    shard
      .request(Request::Get {
        tbl: tbl.clone(),
        key: b"a".to_vec(),
      })
      .await
      .unwrap(),
    Response::Get(Some(value)) if value == b"A"
  ));
  assert!(matches!(
    shard.request(Request::Reopen { tbl }).await.unwrap(),
    Response::Reopen
  ));
  // Mem 无 durability：不落盘，root 下不应产生任何数据文件。
  assert_eq!(read_dir(&root).unwrap().count(), 0);
  let _ = remove_dir_all(&root);
  shard.shutdown().await.unwrap();
}

/// sto-69：attach 的 branch 名必须归一化——`"MAIN"` 与 `"main"` 是同一个 owner，
/// 非法名（含 `/` 等）直接拒绝，避免写出 `CURRENT_TERM_MAIN_0` 这类分裂名字。
#[compio::test]
async fn attach_normalizes_branch_name() {
  let root = temp_root();
  {
    let shard = Shard::start(config(root.clone())).unwrap();
    assert!(matches!(
      shard
        .request(Request::AttachShard {
          branch: "MAIN".to_owned(),
          shard_id: 0,
          term: 1,
        })
        .await
        .unwrap(),
      Response::AttachShard
    ));
    // 归一后是同一 owner：同 shard / term 再 attach 应幂等成功
    // （不归一的话 branch 字符串不等 → `admit_attach` 报 InvalidArgs）。
    assert!(matches!(
      shard
        .request(Request::AttachShard {
          branch: "main".to_owned(),
          shard_id: 0,
          term: 1,
        })
        .await
        .unwrap(),
      Response::AttachShard
    ));
    // 非法字符直接拒绝，不进状态机。
    assert!(matches!(
      shard
        .request(Request::AttachShard {
          branch: "bad/branch".to_owned(),
          shard_id: 0,
          term: 1,
        })
        .await,
      Err(KvError::InvalidArgs)
    ));
    shard.shutdown().await.unwrap();
  }
  let _ = remove_dir_all(&root);
}

/// io-40：ShardConfig 的 `max_inflight_write` 必须投影到 CompioStore；
/// 0 由 store 归一为 1，避免闸门容量为 0 导致写永远挂起。
#[test]
fn local_backend_carries_inflight_write_limit() {
  // 只验装配投影，不落盘：CompioStore::new 不触碰路径。
  let mut cfg = config(PathBuf::from("fkv-inflight-wiring"));
  cfg.max_inflight_write = 7;
  let StoreBackend::Local(store) = StoreBackend::local(&cfg) else {
    panic!("local backend 必须构造 CompioStore");
  };
  assert_eq!(store.max_inflight_write(), 7);

  cfg.max_inflight_write = 0;
  let StoreBackend::Local(store) = StoreBackend::local(&cfg) else {
    panic!("local backend 必须构造 CompioStore");
  };
  assert_eq!(store.max_inflight_write(), 1);
}

/// 并发额度按**真实 CloudStore 构造点**均分：`s3_instance_num` 由装配点写入
/// shard + 长驻列举 + 已启用的预热 / 归档 crond；`n < instances` 时不低于 1，
/// 整机上界收敛到实例数而不是 `实例数 × n`。
#[test]
fn cloud_s3_quota_is_split_by_construction_points() {
  use std::sync::Arc;

  use fkv_cloud::CacheGroup;
  use fkv_shard::CloudBackend;

  let cloud = |max_cloud_concurrency: usize, s3_instance_num: usize| CloudBackend {
    bucket: "b".to_owned(),
    prefix: "p".to_owned(),
    endpoint: "http://127.0.0.1:1".to_owned(),
    region: "r".to_owned(),
    access_key: String::new(),
    secret_key: String::new(),
    allow_http: true,
    cache_group: Arc::new(CacheGroup::new(0)),
    allow_reuse_local_caches: false,
    max_cloud_concurrency,
    s3_instance_num,
    verify_ssl: true,
  };

  // 0 = 用环境 / 默认值，不拆分。
  assert_eq!(cloud(0, 3).s3_config().max_concurrency, 0);
  // 64 / 3 = 21。
  assert_eq!(cloud(64, 3).s3_config().max_concurrency, 21);
  assert_eq!(cloud(10, 1).s3_config().max_concurrency, 10);
  // n < instances：每实例至少 1，不能被整数除法压成 0（0 会退回默认 64）。
  assert_eq!(cloud(4, 5).s3_config().max_concurrency, 1);
}

/// `StoreBackend` 必须按当前后端转发旁路文件能力，否则 segment 完成标记
/// 在枚举 / drop 清理时静默失效。
#[compio::test]
async fn store_backend_forwards_sidecar_file_li() {
  let root = temp_root();
  let config = config(root.clone());
  let store = StoreBackend::mem(&config);
  let tbl = TableIdent::new("sidecar", 1).unwrap();

  ManifestIo::write_manifest_file(&store, &tbl, "segcomplete_3", b"magic")
    .await
    .unwrap();
  let name_li = store.list_sidecar_file_name_li(&tbl).await.unwrap();
  assert!(
    name_li.iter().any(|name| name == "segcomplete_3"),
    "{name_li:?}"
  );

  store
    .remove_sidecar_file_name(&tbl, "segcomplete_3")
    .await
    .unwrap();
  assert!(
    store
      .list_sidecar_file_name_li(&tbl)
      .await
      .unwrap()
      .is_empty(),
    "旁路删除必须转发到当前后端"
  );
  let _ = remove_dir_all(root);
}
