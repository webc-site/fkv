//! test-01：shard 层「页写 IoFail」错误回执 + 旧数据完好 + 可续写。
//!
//! `Local` 后端（`CompioStore`）在 `write_page_li` 的逐页前置检查中命中
//! `WritePage@2`，dispatch 必须把 `IoFail` 原样回给调用方，而不是 abort shard；
//! 失败批次的 mapping 已由 `settle_mapper_batch` 回滚，后续 `put` 必须成功。
//!
//! 对照 `oom_retry.rs`：那里只覆盖可重试的 `OutOfMem`，且失败后直接 shutdown，
//! 不验证错误回执与失败后的可用性。

use std::path::PathBuf;

use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_failpoint::{reset, set_spec};
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_store::AllocMode;
use fkv_testkit::TempRoot;
use fkv_types::{StoreMode, TableIdent};

fn config(root: PathBuf) -> ShardConfig {
  ShardConfig {
    alloc_mode: AllocMode::Append,
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
    queue_size: 16,
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

async fn put(shard: &Shard, tbl: &TableIdent, batch_li: Vec<BatchEntry>) -> fkv_error::Result<()> {
  shard
    .request(Request::Put {
      tbl: tbl.clone(),
      batch_li,
      term: Some(1),
    })
    .await
    .map(|_| ())
}

async fn get(shard: &Shard, tbl: &TableIdent, key: &[u8]) -> Option<Vec<u8>> {
  match shard
    .request(Request::Get {
      tbl: tbl.clone(),
      key: key.to_vec(),
    })
    .await
    .unwrap()
  {
    Response::Get(value) => value,
    other => panic!("unexpected get response: {other:?}"),
  }
}

#[compio::test]
async fn page_write_error_returns_io_fail_and_recovers() {
  let root = TempRoot::new("page_write_fail");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
  let tbl = TableIdent::new("demo", 1).unwrap();

  // 1. 先提交 a。
  put(
    &shard,
    &tbl,
    vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
  )
  .await
  .unwrap();
  assert_eq!(get(&shard, &tbl, b"a").await, Some(b"A".to_vec()));

  // 2. 第 2 次 WritePage 注入不可重试的 IoFail（value 各占一页，保证 >=2 页）。
  set_spec("WritePage@2!err=IoFail");
  let err = put(
    &shard,
    &tbl,
    vec![
      BatchEntry::upsert(b"b".to_vec(), b"B".repeat(400), 2),
      BatchEntry::upsert(b"c".to_vec(), b"C".repeat(400), 2),
    ],
  )
  .await
  .unwrap_err();
  reset();
  assert_eq!(err, KvError::IoFail, "dispatch 必须回传 IoFail 错误回执");

  // 3. shard 未被 abort：旧数据仍可读，失败批次的新键不可见。
  assert_eq!(get(&shard, &tbl, b"a").await, Some(b"A".to_vec()));
  assert_eq!(get(&shard, &tbl, b"b").await, None);
  assert_eq!(get(&shard, &tbl, b"c").await, None);

  // 4. reset 后续写成功（失败不污染下一批）。
  put(
    &shard,
    &tbl,
    vec![
      BatchEntry::upsert(b"b".to_vec(), b"B".repeat(400), 3),
      BatchEntry::upsert(b"c".to_vec(), b"C".repeat(400), 3),
    ],
  )
  .await
  .unwrap();
  assert_eq!(get(&shard, &tbl, b"a").await, Some(b"A".to_vec()));
  assert_eq!(get(&shard, &tbl, b"b").await, Some(b"B".repeat(400)));
  assert_eq!(get(&shard, &tbl, b"c").await, Some(b"C".repeat(400)));

  shard.shutdown().await.unwrap();
}
