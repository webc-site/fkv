//! data store shard 生命周期（sto-61）：attach / detach 的幂等与非法转换。
//!
//! 对齐 C++ `AttachShard` / `DetachShard` 的状态要求：已托管时换 shard / branch
//! 必须先 detach；`Detached` 下 detach 幂等；旧 term 重新 attach 让整 shard 过期。
use std::{
  env::temp_dir,
  fs::{create_dir_all, read_to_string, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicUsize, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_types::{StoreMode, TableIdent, current_term_file_name};

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_lifecycle_{}_{}_{}",
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
    queue_size: 64,
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
    fd_limit: 10_000,
    index_cache_bytes: fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize,
    index_cache_total_bytes: fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize,
    root_meta_cache_bytes: 0,
    io_queue_size: 4096,
    backend: Backend::default(),
    mode: StoreMode::Local,
    standby: None,
    max_write_concurrency: 0,
    max_processing_time_us: 0,
    yield_budget_us: 0,
  }
}

fn attach(branch: &str, shard_id: u32, term: u64) -> Request {
  Request::AttachShard {
    branch: branch.to_owned(),
    shard_id,
    term,
  }
}

fn detach(shard_id: u32) -> Request {
  Request::DetachShard { shard_id }
}

fn put(tbl: &TableIdent, key: &[u8], value: &[u8], term: u64) -> Request {
  Request::Put {
    tbl: tbl.clone(),
    batch_li: vec![BatchEntry::upsert(key.to_vec(), value.to_vec(), 1)],
    term: Some(term),
  }
}

fn get(tbl: &TableIdent, key: &[u8]) -> Request {
  Request::Get {
    tbl: tbl.clone(),
    key: key.to_vec(),
  }
}

/// 未 attach 拒绝普通请求；同 term 重复 attach 幂等且不驱逐已写数据。
#[compio::test]
async fn attach_is_idempotent_and_keeps_state() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone())).unwrap();
  let tbl = TableIdent::new("lifecycle", 1).unwrap();

  assert_eq!(
    shard.request(get(&tbl, b"a")).await.unwrap_err(),
    KvError::NotRunning
  );
  assert!(matches!(
    shard.request(attach("main", 0, 7)).await.unwrap(),
    Response::AttachShard
  ));
  shard.request(put(&tbl, b"a", b"A", 7)).await.unwrap();
  // 重复 attach 不得重置 store 状态（否则需要重新打开 manifest）。
  assert!(matches!(
    shard.request(attach("main", 0, 7)).await.unwrap(),
    Response::AttachShard
  ));
  assert!(matches!(
    shard.request(get(&tbl, b"a")).await.unwrap(),
    Response::Get(Some(value)) if value == b"A"
  ));
  // detach 幂等：第二次直接返回成功。
  assert!(matches!(
    shard.request(detach(0)).await.unwrap(),
    Response::DetachShard
  ));
  assert!(matches!(
    shard.request(detach(0)).await.unwrap(),
    Response::DetachShard
  ));
  assert_eq!(
    shard.request(get(&tbl, b"a")).await.unwrap_err(),
    KvError::NotRunning
  );
  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}

/// 已托管时换 branch / shard 是非法转换；旧 term 让整 shard 过期。
#[compio::test]
async fn illegal_transitions_are_rejected() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone())).unwrap();
  let tbl = TableIdent::new("demo", 1).unwrap();

  shard.request(attach("main", 0, 5)).await.unwrap();
  assert_eq!(
    shard.request(attach("feature", 0, 5)).await.unwrap_err(),
    KvError::InvalidArgs
  );
  assert_eq!(
    shard.request(attach("main", 1, 5)).await.unwrap_err(),
    KvError::InvalidArgs
  );
  assert_eq!(
    shard.request(attach("main", 0, 6)).await.unwrap_err(),
    KvError::InvalidArgs
  );
  assert_eq!(
    shard.request(detach(1)).await.unwrap_err(),
    KvError::InvalidArgs
  );
  // 旧 term attach：整 shard 过期，普通请求快速失败。
  assert_eq!(
    shard.request(attach("main", 0, 4)).await.unwrap_err(),
    KvError::ExpiredTerm
  );
  assert_eq!(
    shard.request(get(&tbl, b"a")).await.unwrap_err(),
    KvError::ExpiredTerm
  );
  // 同 term 重新 attach 恢复。
  shard.request(attach("main", 0, 5)).await.unwrap();
  shard.request(put(&tbl, b"a", b"A", 5)).await.unwrap();
  assert!(matches!(
    shard.request(get(&tbl, b"a")).await.unwrap(),
    Response::Get(Some(value)) if value == b"A"
  ));
  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}

/// Local 模式 attach 落 `CURRENT_TERM_<branch>_<shard_id>`；detach 清空运行期
/// 身份但保留持久 term，旧 term 的重新 attach 仍由 CAS 拒绝；换 term 后旧数据可读，
/// 新写入落在 bump 之后的文件（与 core-22a 的 allocator bump 配合）。
#[compio::test]
async fn local_term_file_fences_and_detach_resets_state() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone())).unwrap();
  let tbl = TableIdent::new("term", 1).unwrap();
  let term_file = root.join(current_term_file_name("main", 0));

  shard.request(attach("main", 0, 3)).await.unwrap();
  assert_eq!(read_to_string(&term_file).unwrap(), "3");
  shard.request(put(&tbl, b"a", b"A", 3)).await.unwrap();

  shard.request(detach(0)).await.unwrap();
  // detach 不删持久 term：跨进程 / 重连的 fencing 依据仍在。
  assert_eq!(read_to_string(&term_file).unwrap(), "3");
  // 运行期身份归零：普通请求快速失败。
  assert_eq!(
    shard.request(get(&tbl, b"a")).await.unwrap_err(),
    KvError::NotRunning
  );
  // detach 后旧 term 重新 attach：由持久 term 文件 CAS 拒绝。
  assert_eq!(
    shard.request(attach("main", 0, 2)).await.unwrap_err(),
    KvError::ExpiredTerm
  );

  // 更高 term 接管：term 文件更新，旧 term 写入失败。
  shard.request(attach("main", 0, 4)).await.unwrap();
  assert_eq!(read_to_string(&term_file).unwrap(), "4");
  // 跨 term 读：旧 term 写入的数据仍可见。
  assert!(matches!(
    shard.request(get(&tbl, b"a")).await.unwrap(),
    Response::Get(Some(value)) if value == b"A"
  ));
  assert_eq!(
    shard.request(put(&tbl, b"a", b"A4", 3)).await.unwrap_err(),
    KvError::ExpiredTerm
  );
  // 旧 term 请求让整 shard 过期，重挂同 term 恢复后可继续写。
  shard.request(attach("main", 0, 4)).await.unwrap();
  shard.request(put(&tbl, b"b", b"B", 4)).await.unwrap();
  assert!(matches!(
    shard.request(get(&tbl, b"b")).await.unwrap(),
    Response::Get(Some(value)) if value == b"B"
  ));

  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}

/// sto-69b：shard 身份与存储布局在启动时固定——请求侧不能改绑 shard id，
/// `Gc` / `Compact` 的 `pages_per_file_shift` 必须等于 `ShardConfig` 的值。
#[compio::test]
async fn attach_id_and_layout_shift_are_validated() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone())).unwrap();
  let tbl = TableIdent::new("layout", 1).unwrap();

  // 配置 shard_id = 0：别的 id 不能在同一个 shard 上 attach / detach。
  assert_eq!(
    shard.request(attach("main", 1, 1)).await.unwrap_err(),
    KvError::InvalidArgs
  );
  shard.request(attach("main", 0, 1)).await.unwrap();
  assert_eq!(
    shard.request(detach(1)).await.unwrap_err(),
    KvError::InvalidArgs
  );
  shard.request(put(&tbl, b"a", b"A", 1)).await.unwrap();

  // Compact 的布局位宽由调用方传入：错误位宽会让 file_id 换算错位，必须拒绝。
  assert_eq!(
    shard
      .request(Request::Compact {
        tbl: tbl.clone(),
        file_id: 0,
        pages_per_file_shift: 64,
      })
      .await
      .unwrap_err(),
    KvError::InvalidArgs
  );
  assert!(matches!(
    shard
      .request(Request::FileGc { tbl: tbl.clone() })
      .await
      .unwrap(),
    Response::FileGc(_)
  ));
  assert!(matches!(
    shard.request(Request::LocalGc { tbl }).await.unwrap(),
    Response::LocalGc(_)
  ));

  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}

/// `ShardLifecycle::is_attached` 是 runtime 侧唯一的生命周期探针
/// （`get()` 已私有化）：attach / detach 必须让它翻转，过期后不再报托管。
#[compio::test]
async fn lifecycle_probe_tracks_attach_detach_and_expire() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone())).unwrap();
  let tbl = TableIdent::new("probe", 1).unwrap();
  assert!(!shard.lifecycle().is_attached(), "启动时未托管");

  shard.request(attach("main", 0, 1)).await.unwrap();
  assert!(shard.lifecycle().is_attached());

  shard.request(put(&tbl, b"a", b"A", 1)).await.unwrap();
  // 旧 term 写让整 shard 过期：探针必须翻成非托管。
  assert_eq!(
    shard.request(put(&tbl, b"b", b"B", 0)).await.unwrap_err(),
    KvError::ExpiredTerm
  );
  assert!(!shard.lifecycle().is_attached(), "过期后不再托管");

  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}
