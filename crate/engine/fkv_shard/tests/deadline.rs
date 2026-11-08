//! sto-62b：请求级 deadline。过期请求在开始执行前回 `Timeout`，不进入
//! `admit` / `prepare_table` / store；正常请求不受影响。

use std::{
  path::PathBuf,
  time::{Duration, Instant},
};

use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn config(root: PathBuf) -> ShardConfig {
  let mut cfg = ShardConfig::for_tests(root);
  cfg.queue_size = 16;
  cfg.fd_limit = 0;
  cfg.backend = Backend::Mem;
  cfg
}

async fn start_shard(root: PathBuf) -> Shard {
  let shard = Shard::start(config(root)).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
  shard
}

fn put(tbl: &TableIdent, key: &[u8], value: &[u8]) -> Request {
  Request::Put {
    tbl: tbl.clone(),
    batch_li: vec![BatchEntry::upsert(key.to_vec(), value.to_vec(), 1)],
    term: None,
  }
}

fn get(tbl: &TableIdent, key: &[u8]) -> Request {
  Request::Get {
    tbl: tbl.clone(),
    key: key.to_vec(),
  }
}

/// 已过期的写请求：shard 出队即回 `Timeout`，不写 store；随后正常请求仍可服务。
#[compio::test]
async fn expired_put_returns_timeout_without_applying() {
  let root = TempRoot::new("deadline");
  let shard = start_shard(root.path().to_path_buf()).await;
  let tbl = TableIdent::new("deadline", 1).unwrap();
  let expired = Instant::now() - Duration::from_millis(1);
  let err = shard
    .sender()
    .request_at(put(&tbl, b"late", b"x"), expired)
    .await
    .unwrap_err();
  assert_eq!(err, KvError::Timeout);
  assert!(matches!(
    shard.request(get(&tbl, b"late")).await.unwrap(),
    Response::Get(None)
  ));

  // 过期请求不污染 shard：正常请求继续执行。
  shard.request(put(&tbl, b"ok", b"1")).await.unwrap();
  assert!(matches!(
    shard.request(get(&tbl, b"ok")).await.unwrap(),
    Response::Get(Some(value)) if value == b"1"
  ));

  shard.shutdown().await.unwrap();
}

/// 同一轮读批里混有过期与有效请求：过期的回 `Timeout`，有效的照常返回数据。
#[compio::test]
async fn read_batch_splits_expired_and_live_requests() {
  let root = TempRoot::new("deadline");
  let shard = start_shard(root.path().to_path_buf()).await;
  let tbl = TableIdent::new("deadline", 1).unwrap();
  shard.request(put(&tbl, b"k", b"v")).await.unwrap();

  let sender = shard.sender();
  let expired = Instant::now() - Duration::from_millis(1);
  let (late, live) = futures::join!(
    sender.request_at(get(&tbl, b"k"), expired),
    sender.request_at(get(&tbl, b"k"), Instant::now() + Duration::from_secs(30)),
  );
  assert_eq!(late.unwrap_err(), KvError::Timeout);
  assert!(matches!(
    live.unwrap(),
    Response::Get(Some(value)) if value == b"v"
  ));

  shard.shutdown().await.unwrap();
}

/// `request_deadline` 的本端兜底：超时回 `Timeout`，请求未落盘。
#[compio::test]
async fn request_deadline_honors_timeout_and_skips_store() {
  let root = TempRoot::new("deadline");
  let shard = start_shard(root.path().to_path_buf()).await;
  let tbl = TableIdent::new("deadline", 1).unwrap();
  // 0 时长：deadline 在构造时即到期，shard 侧开始执行前必判过期。
  let err = shard
    .sender()
    .request_deadline(put(&tbl, b"zero", b"x"), Duration::ZERO)
    .await
    .unwrap_err();
  assert_eq!(err, KvError::Timeout);
  assert!(matches!(
    shard.request(get(&tbl, b"zero")).await.unwrap(),
    Response::Get(None)
  ));

  shard.shutdown().await.unwrap();
}

/// 未过期的 deadline 不影响执行：deadline 只在开始执行前检查。
#[compio::test]
async fn live_deadline_request_executes_normally() {
  let root = TempRoot::new("deadline");
  let shard = start_shard(root.path().to_path_buf()).await;
  let tbl = TableIdent::new("deadline", 1).unwrap();
  shard
    .sender()
    .request_at(
      put(&tbl, b"live", b"v"),
      Instant::now() + Duration::from_secs(30),
    )
    .await
    .unwrap();
  assert!(matches!(
    shard.request(get(&tbl, b"live")).await.unwrap(),
    Response::Get(Some(value)) if value == b"v"
  ));

  shard.shutdown().await.unwrap();
}
