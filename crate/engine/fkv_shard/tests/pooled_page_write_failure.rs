//! test-11：Pooled（生产默认）下的页写失败回执、失败后可用性与重开。
//!
//! 对照 `page_write_failure.rs`（Append）：
//! - 失败批次**覆盖已提交的 key**，强制 COW 把旧物理页 retire 后再注入
//!   `IoFail`。Pooled 的 `abort_batch` 必须同时恢复 mapping、分配器与
//!   retired 队列；否则重试分配可能复用仍被旧映射引用的物理页。
//! - 失败后同一 shard 必须继续可写，重开（同一 root）后数据仍是最后提交的轮次。
//!
//! Append 模式下旧页本就不复用，覆盖失败后的「旧值完好」测不出 retired
//! 队列的回滚；本用例把该窗口挪到生产默认路径上。

use std::path::PathBuf;

use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_failpoint::{reset, set_spec};
use fkv_shard::{Shard, ShardConfig};
use fkv_store::AllocMode;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const KEYS: u32 = 8;
const VALUE_LEN: usize = 400;

fn config(root: PathBuf) -> ShardConfig {
  let mut cfg = ShardConfig::for_tests(root);
  cfg.alloc_mode = AllocMode::Pooled;
  cfg.queue_size = 16;
  cfg
}

fn key(index: u32) -> Vec<u8> {
  format!("k{index:04}").into_bytes()
}

/// 每个 value 首 4 字节是 key 序号，其余为轮次 tag：串页 / 混轮会直接暴露。
fn value(tag: u8, index: u32) -> Vec<u8> {
  let mut value = vec![tag; VALUE_LEN];
  value[..4].copy_from_slice(&index.to_be_bytes());
  value
}

fn batch(tag: u8, index_li: impl IntoIterator<Item = u32>, ts: u64) -> Vec<BatchEntry> {
  index_li
    .into_iter()
    .map(|index| BatchEntry::upsert(key(index), value(tag, index), ts))
    .collect()
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

async fn get(shard: &Shard, tbl: &TableIdent, index: u32) -> Option<Vec<u8>> {
  match shard
    .request(Request::Get {
      tbl: tbl.clone(),
      key: key(index),
    })
    .await
    .unwrap()
  {
    Response::Get(value) => value,
    other => panic!("unexpected get response: {other:?}"),
  }
}

fn expect_value(got: Option<Vec<u8>>, tag: u8, index: u32) {
  assert_eq!(got, Some(value(tag, index)), "key={index} tag={tag}");
}

#[compio::test]
async fn pooled_page_write_error_keeps_old_values_and_reopen_consistent() {
  let root = TempRoot::new("pooled_page_write_fail");
  let cfg = config(root.path().to_path_buf());
  let shard = Shard::start(cfg.clone()).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("pooled_write_fail", 1).unwrap();

  // 1. 先提交第一轮，作为失败回滚的基线。
  put(&shard, &tbl, batch(b'A', 0..KEYS, 1)).await.unwrap();
  for index in 0..KEYS {
    expect_value(get(&shard, &tbl, index).await, b'A', index);
  }

  // 2. 覆盖同一批 key + 新增两个 key，第 2 次 WritePage 注入不可重试 IoFail。
  set_spec("WritePage@2!err=IoFail");
  let err = put(
    &shard,
    &tbl,
    batch(b'B', 0..KEYS, 2)
      .into_iter()
      .chain(batch(b'B', KEYS..KEYS + 2, 2))
      .collect(),
  )
  .await
  .unwrap_err();
  reset();
  assert_eq!(err, KvError::IoFail, "dispatch 必须回传 IoFail 错误回执");

  // 3. 失败批次的覆盖 / 新增都不可见：旧值完好，新 key 缺失。
  for index in 0..KEYS {
    expect_value(get(&shard, &tbl, index).await, b'A', index);
  }
  assert_eq!(get(&shard, &tbl, KEYS).await, None);
  assert_eq!(get(&shard, &tbl, KEYS + 1).await, None);

  // 4. 失败不污染下一批：同一 shard 重试成功。
  put(
    &shard,
    &tbl,
    batch(b'B', 0..KEYS, 3)
      .into_iter()
      .chain(batch(b'B', KEYS..KEYS + 2, 3))
      .collect(),
  )
  .await
  .unwrap();
  for index in 0..KEYS + 2 {
    expect_value(get(&shard, &tbl, index).await, b'B', index);
  }

  // 5. 再覆盖一轮触发退役页回收 / 复用，验证数据仍是最后提交轮次。
  put(&shard, &tbl, batch(b'C', 0..KEYS / 2, 4))
    .await
    .unwrap();
  for index in 0..KEYS / 2 {
    expect_value(get(&shard, &tbl, index).await, b'C', index);
  }
  for index in KEYS / 2..KEYS + 2 {
    expect_value(get(&shard, &tbl, index).await, b'B', index);
  }

  // 6. 正常关闭后用同一 root 重开：manifest 重放 + Pooled 空闲池按映射空洞
  //    重建后，最后提交状态必须完整可见。
  shard.shutdown().await.unwrap();
  let reopened = Shard::start(cfg).unwrap();
  attach(&reopened).await;
  for index in 0..KEYS / 2 {
    expect_value(get(&reopened, &tbl, index).await, b'C', index);
  }
  for index in KEYS / 2..KEYS + 2 {
    expect_value(get(&reopened, &tbl, index).await, b'B', index);
  }
  reopened.shutdown().await.unwrap();
}
