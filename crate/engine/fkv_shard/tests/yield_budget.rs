//! eng-34b：`ShardConfig::yield_budget_us` 端到端接线。
//!
//! 段预算只影响长循环内部的让出时机，不改变 scan / get 结果；`0` 关闭时
//! 与未插桩路径一致。让出行为本身由 `fkv_store/tests/yield.rs` 的标记任务测试
//! 证明，这里验证 shard 配置链路与端到端结果。
use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_api::{Request, Response, ScanPageOption};
use fkv_btree::BatchEntry;
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_types::{StoreMode, TableIdent};

const TOTAL: u32 = 2000;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_yield_budget_{}_{}_{}",
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

fn config(root: PathBuf, yield_budget_us: u64) -> ShardConfig {
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
    queue_size: 256,
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
    backend: Backend::Mem,
    mode: StoreMode::Local,
    standby: None,
    max_write_concurrency: 0,
    max_processing_time_us: 0,
    yield_budget_us,
  }
}

fn key(index: u32) -> Vec<u8> {
  format!("key-{index:05}").into_bytes()
}

async fn scan_and_get(yield_budget_us: u64) {
  let root = temp_root();
  let shard = Shard::start(config(root.clone(), yield_budget_us)).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
  let tbl = TableIdent::new("yield", 1).unwrap();
  let batch_li: Vec<BatchEntry> = (0..TOTAL)
    .map(|index| BatchEntry::upsert(key(index), vec![b'v'; 32], 1))
    .collect();
  shard
    .request(Request::Put {
      tbl: tbl.clone(),
      batch_li,
      term: Some(1),
    })
    .await
    .unwrap();

  let Response::Scan(reply) = shard
    .request(Request::Scan {
      tbl: tbl.clone(),
      begin: Vec::new(),
      end: Vec::new(),
      max_entries: 0,
      option: ScanPageOption::default(),
    })
    .await
    .unwrap()
  else {
    panic!("expected scan reply");
  };
  assert_eq!(reply.entry_li.len(), TOTAL as usize);
  assert_eq!(reply.entry_li[0].0, key(0));
  assert_eq!(reply.entry_li[TOTAL as usize - 1].0, key(TOTAL - 1));

  let Response::Get(value) = shard
    .request(Request::Get {
      tbl: tbl.clone(),
      key: key(TOTAL / 2),
    })
    .await
    .unwrap()
  else {
    panic!("expected get reply");
  };
  assert_eq!(value, Some(vec![b'v'; 32]));

  shard.shutdown().await.unwrap();
  let _ = remove_dir_all(root);
}

#[compio::test]
async fn scan_and_get_keep_results_with_yield_budget() {
  scan_and_get(fkv_config::DEFAULT_YIELD_BUDGET_US).await;
  scan_and_get(0).await;
}
