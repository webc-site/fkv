//! eng-34b：`ShardConfig::yield_budget_us` 端到端接线。
//!
//! 段预算只影响长循环内部的让出时机，不改变 scan / get 结果；`0` 关闭时
//! 与未插桩路径一致。让出行为本身由 `fkv_store/tests/yield.rs` 的标记任务测试
//! 证明，这里验证 shard 配置链路与端到端结果。
use std::path::PathBuf;

use fkv_api::{Request, Response, ScanPageOption};
use fkv_btree::BatchEntry;
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const TOTAL: u32 = 2000;

fn config(root: PathBuf, yield_budget_us: u64) -> ShardConfig {
  let mut cfg = ShardConfig::for_tests(root);
  cfg.queue_size = 256;
  cfg.backend = Backend::Mem;
  cfg.yield_budget_us = yield_budget_us;
  cfg
}

fn key(index: u32) -> Vec<u8> {
  format!("key-{index:05}").into_bytes()
}

async fn scan_and_get(yield_budget_us: u64) {
  let root = TempRoot::new("yield_budget");
  let shard = Shard::start(config(root.path().to_path_buf(), yield_budget_us)).unwrap();
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
}

#[compio::test]
async fn scan_and_get_keep_results_with_yield_budget() {
  scan_and_get(fkv_config::DEFAULT_YIELD_BUDGET_US).await;
  scan_and_get(0).await;
}
