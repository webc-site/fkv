//! sem-16：任务化写 / 维护任务返回 `ExpiredTerm` 时，事件循环回收 `WriteDone`
//! 必须把整个 shard 置 `Expired`（对齐 C++ `Shard::OnTaskFinished` 对任意任务
//! `ExpiredTerm` 调 `MarkShardExpired`），后续普通请求快速失败。
//!
//! 构造：shard 以 term 5 attach 并建立分区；另起一个 `Store` 以更高 term 7
//! 回放同一本地 manifest 后追加快照（模拟新 owner 接管盘面），随后 shard 的
//! 任务化 `FileGc` 在遍历 manifest 时命中 `term_above` 熔断。非 term 错误
//! （`CreateBranch` 的非法 branch 名）必须保持生命周期不变。
use std::{
  path::PathBuf,
  time::{Duration, Instant},
};

use compio::time::sleep;
use fkv_api::Request;
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io_compio::CompioStore;
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_store::{AllocMode, Store};
use fkv_testkit::TempRoot;
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, StoreMode, TableIdent};

fn config(root: PathBuf) -> ShardConfig {
  ShardConfig {
    alloc_mode: AllocMode::Append,
    shard_id: 0,
    root,
    page_size: 512,
    restart_interval: 16,
    index_restart_interval: 16,
    pages_per_file_shift: DEFAULT_PAGES_PER_FILE_SHIFT,
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

/// 回执先于 `WriteDone` 归还 Store 发出：等写门清零，确保事件循环已收尾上一条
/// 任务（Store 已放回槽位、`WriteDone` 结果已观察完）。
async fn wait_write_running_zero(shard: &Shard) {
  for _ in 0..10_000 {
    if shard.metrics_snapshot().write_running == 0 {
      return;
    }
    sleep(Duration::from_micros(200)).await;
  }
  panic!("写门在跑数始终未归零");
}

#[compio::test]
async fn spawned_file_gc_expired_term_marks_shard_expired() {
  let root = TempRoot::new("spawned-expired");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  let tbl = TableIdent::new("spawned", 1).unwrap();

  shard.request(attach("main", 0, 5)).await.unwrap();
  shard.request(put(&tbl, b"a", b"A", 5)).await.unwrap();
  wait_write_running_zero(&shard).await;

  // 对照：任务化 `CreateBranch` 返回非 term 错误（非法 branch 名）不改变生命周期。
  assert_eq!(
    shard
      .request(Request::CreateBranch {
        tbl: tbl.clone(),
        branch: "Bad_Name!".to_owned(),
      })
      .await
      .unwrap_err(),
    KvError::InvalidArgs
  );
  wait_write_running_zero(&shard).await;
  assert!(
    shard.lifecycle().is_attached(),
    "非 term 任务错误不得把 shard 置 Expired"
  );

  // 更高 term owner 接管：同一本地盘面上回放并追加 term 7 的 snapshot。
  let mut higher = Store::new(
    CompioStore::new(root.path().to_path_buf(), 512, DEFAULT_PAGES_PER_FILE_SHIFT)
      .with_segment_config(4096, 2),
    512,
    16,
  )
  .with_branch("main", 7);
  higher.reopen(&tbl).await.unwrap();
  higher
    .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1)])
    .await
    .unwrap();

  // 任务化维护：GC 看到更高 term 的 manifest，任务返回 ExpiredTerm。
  assert_eq!(
    shard
      .request(Request::FileGc { tbl: tbl.clone() })
      .await
      .unwrap_err(),
    KvError::ExpiredTerm
  );

  // 回执由任务在 `WriteDone` 之前发送：等事件循环回收完成（生命周期镜像翻转）。
  // 修复前这里永不翻转，测试以超时失败。
  let deadline = Instant::now() + Duration::from_secs(5);
  while shard.lifecycle().is_attached() {
    assert!(
      Instant::now() < deadline,
      "任务化 FileGc 的 ExpiredTerm 未把 shard 置 Expired"
    );
    sleep(Duration::from_millis(5)).await;
  }

  // 后续普通读写经 `admit` 快速失败，而不是继续以旧 term 放行。
  assert_eq!(
    shard.request(get(&tbl, b"b")).await.unwrap_err(),
    KvError::ExpiredTerm
  );
  assert_eq!(
    shard.request(put(&tbl, b"c", b"C", 5)).await.unwrap_err(),
    KvError::ExpiredTerm
  );

  shard.shutdown().await.unwrap();
}
