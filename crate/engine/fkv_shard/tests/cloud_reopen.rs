//! Cloud / StandbyReplica 模式：manifest 暂不可见时读请求挂等待重排（对齐 C++
//! `auto_reopen` + `AutoReopenRetry`），且不做本地空表懒创建。
//!
//! 用 Local IO 后端 + `StoreMode::Cloud`：模式只决定 `ResourceMissing` 的语义，
//! 与 IO 后端正交，这样能在本地确定性地复现「manifest 延迟可见」而无需对象存储。

use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use compio::{runtime::spawn, time::sleep};
use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io_compio::CompioStore;
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_store::Store;
use fkv_types::{InstallOutcome, StoreMode, TableIdent};

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_cloud_reopen_{}_{}_{}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos(),
    seq,
  ));
  create_dir_all(&root).unwrap();
  root
}

fn config(root: PathBuf, mode: StoreMode, retry_times: u8, pending_us: u64) -> ShardConfig {
  ShardConfig {
    shard_id: 0,
    root,
    page_size: 512,
    restart_interval: 16,
    index_restart_interval: 16,
    pages_per_file_shift: fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    segment_size: 4096,
    segments_per_file_shift: 2,
    large_value_threshold: 1024,
    fixed_buffer_size: 0,
    queue_size: 128,
    auto_reopen_retry_times: retry_times,
    auto_reopen_pending_time_us: pending_us,
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
    mode,
    standby: None,
    max_write_concurrency: 0,
    max_processing_time_us: 0,
    yield_budget_us: 0,
  }
}

async fn attach(shard: &Shard) {
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
}

fn get_req(tbl: &TableIdent) -> Request {
  Request::Get {
    tbl: tbl.clone(),
    key: b"a".to_vec(),
  }
}

/// 另一个 store 稍后落盘：模拟远端 manifest 可见性延迟。
async fn write_later(root: PathBuf, tbl: TableIdent, delay: Duration) {
  sleep(delay).await;
  let mut store = Store::new(
    CompioStore::new(root, 512, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT),
    512,
    16,
  );
  store
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
}

/// Cloud 模式读缺失表：首次尝试立即失败，driver 按 `pending_time` 延迟尝试一次
/// 并拿到迟到的 manifest，读请求被重排后返回真实数据。
#[compio::test]
async fn cloud_read_waits_for_late_manifest() {
  let root = temp_root();
  // driver 单次尝试：`pending_time` 必须晚于 manifest 出现时刻。留足余量，
  // 避免并行测试把写线程压到 driver 尝试之后。
  let shard = Shard::start(config(root.clone(), StoreMode::Cloud, 1, 2_000_000)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("late", 1).unwrap();
  let writer = spawn(write_later(
    root.clone(),
    tbl.clone(),
    Duration::from_millis(100),
  ));

  let started = Instant::now();
  let response = shard.request(get_req(&tbl)).await.unwrap();
  assert!(matches!(response, Response::Get(Some(ref value)) if value == b"A"));
  assert!(
    started.elapsed() >= Duration::from_millis(100),
    "读请求没有等待 manifest：{:?}",
    started.elapsed()
  );
  writer.await.unwrap();
  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}

/// 同表多个读请求挂成同一 driver 的 waiter，manifest 出现后一起成功。
#[compio::test]
async fn cloud_reads_share_driver_and_all_wake() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), StoreMode::Cloud, 1, 2_000_000)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("shared", 1).unwrap();
  let writer = spawn(write_later(
    root.clone(),
    tbl.clone(),
    Duration::from_millis(100),
  ));

  let mut handle_li = Vec::new();
  for _ in 0..4 {
    let sender = shard.sender();
    let tbl = tbl.clone();
    handle_li.push(spawn(async move { sender.request(get_req(&tbl)).await }));
  }
  for handle in handle_li {
    let response = handle.await.unwrap().unwrap();
    assert!(matches!(response, Response::Get(Some(ref value)) if value == b"A"));
  }
  writer.await.unwrap();
  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}

/// driver 只延迟尝试一次：失败即连坐全部 waiter，不按 `retry_times` 多轮空转。
#[compio::test]
async fn cloud_reads_fail_together_after_single_driver_attempt() {
  let root = temp_root();
  // 10 轮预算 × 100ms：若 driver 自己轮询，最坏要 1s；对齐 C++ 后只等一轮。
  let shard = Shard::start(config(root.clone(), StoreMode::Cloud, 10, 100_000)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("missing", 1).unwrap();

  let started = Instant::now();
  let first = {
    let sender = shard.sender();
    let tbl = tbl.clone();
    spawn(async move { sender.request(get_req(&tbl)).await })
  };
  let second = {
    let sender = shard.sender();
    let tbl = tbl.clone();
    spawn(async move { sender.request(get_req(&tbl)).await })
  };
  assert_eq!(first.await.unwrap().unwrap_err(), KvError::ResourceMissing);
  assert_eq!(second.await.unwrap().unwrap_err(), KvError::ResourceMissing);
  let elapsed = started.elapsed();
  assert!(
    elapsed >= Duration::from_millis(90),
    "没有等待一轮延迟尝试：{elapsed:?}"
  );
  assert!(
    elapsed < Duration::from_millis(600),
    "driver 仍在多轮重试：{elapsed:?}"
  );
  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}

/// 外部 `Reopen` 不无条件 park：无同表 driver 时立即回 `ResourceMissing`，
/// 不消耗 `retry_times × pending_time` 的长尾（对齐 C++ 外部 `ReopenRequest`）。
#[compio::test]
async fn cloud_external_reopen_missing_manifest_fails_fast() {
  let root = temp_root();
  // 10 轮 × 500ms：若被 park，调用方会等 5s。
  let shard = Shard::start(config(root.clone(), StoreMode::Cloud, 10, 500_000)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("missing", 1).unwrap();

  let started = Instant::now();
  let err = shard.request(Request::Reopen { tbl }).await.unwrap_err();
  assert_eq!(err, KvError::ResourceMissing);
  assert!(
    started.elapsed() < Duration::from_millis(200),
    "外部 Reopen 被挂起：{:?}",
    started.elapsed()
  );
  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}

/// 同表已有读 driver 时，外部 `Reopen` 搭 waiter（eng-39a 语义），
/// driver 成功后被一起唤醒，而不是立即报错。
#[compio::test]
async fn cloud_external_reopen_joins_read_driver() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), StoreMode::Cloud, 1, 2_000_000)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("join", 1).unwrap();
  let writer = spawn(write_later(
    root.clone(),
    tbl.clone(),
    Duration::from_millis(100),
  ));

  let read = {
    let sender = shard.sender();
    let tbl = tbl.clone();
    spawn(async move { sender.request(get_req(&tbl)).await })
  };
  // 等读请求首次失败并挂起（driver 尚未到期）。Reopen 在 driver 出现前
  // 会立即回 `ResourceMissing`，轮询到 driver 建立再断言。
  let deadline = Instant::now() + Duration::from_secs(5);
  let response = loop {
    match shard.request(Request::Reopen { tbl: tbl.clone() }).await {
      Ok(response) => break response,
      Err(KvError::ResourceMissing) if Instant::now() < deadline => {
        sleep(Duration::from_millis(10)).await;
      }
      Err(err) => panic!("Reopen 失败：{err:?}"),
    }
  };
  assert!(matches!(response, Response::Reopen));

  let response = read.await.unwrap().unwrap();
  assert!(matches!(response, Response::Get(Some(ref value)) if value == b"A"));
  writer.await.unwrap();
  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}

/// 挂起的读只占延迟队列，不阻塞同 shard 其它 table 的读。
#[compio::test]
async fn cloud_pending_read_does_not_block_other_table() {
  let root = temp_root();
  let other = TableIdent::new("other", 1).unwrap();
  {
    let mut store = Store::new(
      CompioStore::new(root.clone(), 512, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT),
      512,
      16,
    );
    store
      .put(
        &other,
        &[BatchEntry::upsert(b"a".to_vec(), b"v".to_vec(), 1)],
      )
      .await
      .unwrap();
  }
  let shard = Shard::start(config(root.clone(), StoreMode::Cloud, 10, 500_000)).unwrap();
  attach(&shard).await;
  let missing = TableIdent::new("missing", 1).unwrap();

  let pending = {
    let sender = shard.sender();
    let tbl = missing.clone();
    spawn(async move { sender.request(get_req(&tbl)).await })
  };
  sleep(Duration::from_millis(50)).await;
  let started = Instant::now();
  let response = shard.request(get_req(&other)).await.unwrap();
  assert!(matches!(response, Response::Get(Some(ref value)) if value == b"v"));
  assert!(
    started.elapsed() < Duration::from_millis(100),
    "其它 table 的读被挂起的 driver 阻塞：{:?}",
    started.elapsed()
  );

  // 清理挂起读：shutdown 以 `NotRunning` 结束它。
  shard.shutdown().await.unwrap();
  assert_eq!(pending.await.unwrap().unwrap_err(), KvError::NotRunning);
  remove_dir_all(root).unwrap();
}

/// detach 时挂起的读立即以 `NotRunning` 返回，不悬挂、不 panic。
#[compio::test]
async fn cloud_detach_fails_pending_read() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), StoreMode::Cloud, 10, 500_000)).unwrap();
  attach(&shard).await;
  let missing = TableIdent::new("missing", 1).unwrap();

  let pending = {
    let sender = shard.sender();
    spawn(async move { sender.request(get_req(&missing)).await })
  };
  sleep(Duration::from_millis(50)).await;
  shard
    .request(Request::DetachShard { shard_id: 0 })
    .await
    .unwrap();
  let started = Instant::now();
  assert_eq!(pending.await.unwrap().unwrap_err(), KvError::NotRunning);
  assert!(
    started.elapsed() < Duration::from_millis(200),
    "detach 后挂起读没有立即失败：{:?}",
    started.elapsed()
  );
  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}

/// Cloud main 首写：远端没有 manifest 等价空表，惰性创建并立即成功
/// （对齐 C++ `MakeCowRoot` 的 NotFound → stub）；读请求仍走 auto-reopen。
#[compio::test]
async fn cloud_write_missing_table_lazily_creates_main_branch() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), StoreMode::Cloud, 50, 20_000)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("missing", 1).unwrap();

  let started = Instant::now();
  let response = shard
    .request(Request::Put {
      tbl: tbl.clone(),
      batch_li: vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      term: None,
    })
    .await
    .unwrap();
  assert!(matches!(response, Response::Put));
  assert!(
    started.elapsed() < Duration::from_millis(500),
    "首写不应挂起：{:?}",
    started.elapsed()
  );
  let response = shard
    .request(Request::Get {
      tbl: tbl.clone(),
      key: b"k".to_vec(),
    })
    .await
    .unwrap();
  assert!(matches!(response, Response::Get(Some(value)) if value == b"v"));
  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}

/// Local 模式保持懒创建：缺失表读不到不是错误，也不挂起。
#[compio::test]
async fn local_read_missing_table_is_empty_and_immediate() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), StoreMode::Local, 50, 20_000)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("missing", 1).unwrap();

  let started = Instant::now();
  let response = shard.request(get_req(&tbl)).await.unwrap();
  assert!(matches!(response, Response::Get(None)));
  assert!(started.elapsed() < Duration::from_millis(50));
  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}

/// Local 模式没有远端状态：`ReopenExternal` 显式拒绝，不静默降级成 reopen。
#[compio::test]
async fn local_reopen_external_rejected() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), StoreMode::Local, 50, 20_000)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("local", 1).unwrap();
  assert_eq!(
    shard
      .request(Request::ReopenExternal { tbl })
      .await
      .unwrap_err(),
    KvError::InvalidArgs
  );
  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}

/// Cloud 模式缺 manifest：清本地状态并回 `WaitingRemote`，之后远端出现时读请求
/// 仍能拿到数据（不以空表起跑）。
#[compio::test]
async fn cloud_reopen_external_waits_then_reads_late_manifest() {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), StoreMode::Cloud, 1, 2_000_000)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("adopt", 1).unwrap();
  assert!(matches!(
    shard
      .request(Request::ReopenExternal { tbl: tbl.clone() })
      .await
      .unwrap(),
    Response::ReopenExternal(InstallOutcome::WaitingRemote)
  ));

  let writer = spawn(write_later(
    root.clone(),
    tbl.clone(),
    Duration::from_millis(50),
  ));
  let response = shard.request(get_req(&tbl)).await.unwrap();
  assert!(matches!(response, Response::Get(Some(ref value)) if value == b"A"));
  writer.await.unwrap();
  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}

/// Fresh attach（Expired 后换 term 恢复）会清掉挂在旧 (branch, term) 上的
/// 读：旧请求不能拿新身份重跑，否则可能读到另一 branch 的数据。
#[compio::test]
async fn fresh_attach_fails_parked_read_with_old_identity() {
  let root = temp_root();
  // 5 轮 × 2s：若旧读未被清掉，调用方要等到 10s 后才会重跑。
  let shard = Shard::start(config(root.clone(), StoreMode::Cloud, 5, 2_000_000)).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("stale", 1).unwrap();
  let read = {
    let sender = shard.sender();
    let tbl = tbl.clone();
    spawn(async move { sender.request(get_req(&tbl)).await })
  };
  // 等读请求首次失败并挂进延迟队列。
  sleep(Duration::from_millis(50)).await;
  // 旧 term 重新 attach 把 shard 标记为 Expired，再以更高 term 恢复 = Fresh。
  assert_eq!(
    shard
      .request(Request::AttachShard {
        branch: "main".to_owned(),
        shard_id: 0,
        term: 0,
      })
      .await
      .unwrap_err(),
    KvError::ExpiredTerm
  );
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 2,
    })
    .await
    .unwrap();
  let started = Instant::now();
  assert_eq!(read.await.unwrap().unwrap_err(), KvError::NotRunning);
  assert!(
    started.elapsed() < Duration::from_millis(500),
    "旧身份的挂起读没有被立即清掉：{:?}",
    started.elapsed()
  );
  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}

/// 已有 manifest：`ReopenExternal` 复用 reopen 安装状态并返回 `Installed`。
#[compio::test]
async fn cloud_reopen_external_installs_existing_manifest() {
  let root = temp_root();
  let tbl = TableIdent::new("existing", 1).unwrap();
  let mut writer = Store::new(
    CompioStore::new(root.clone(), 512, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT),
    512,
    16,
  );
  writer
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  let shard = Shard::start(config(root.clone(), StoreMode::Cloud, 50, 20_000)).unwrap();
  attach(&shard).await;
  assert!(matches!(
    shard
      .request(Request::ReopenExternal { tbl: tbl.clone() })
      .await
      .unwrap(),
    Response::ReopenExternal(InstallOutcome::Installed)
  ));
  let response = shard.request(get_req(&tbl)).await.unwrap();
  assert!(matches!(response, Response::Get(Some(ref value)) if value == b"A"));
  shard.shutdown().await.unwrap();
  remove_dir_all(root).unwrap();
}
