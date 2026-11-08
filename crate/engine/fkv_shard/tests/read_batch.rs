//! 读批并发：连续只读请求共享 `&Store`，`join_all` 同轮推进（eng-32a）。
use std::path::PathBuf;

use compio::runtime::spawn;
use fkv_api::{Request, Response, ScanPageOption};
use fkv_btree::BatchEntry;
use fkv_shard::{Shard, ShardConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const KEY_NUM: u32 = 32;

fn config(root: PathBuf) -> ShardConfig {
  ShardConfig::for_tests(root)
}

fn key(index: u32) -> Vec<u8> {
  format!("key-{index:03}").into_bytes()
}

fn val(index: u32) -> Vec<u8> {
  format!("value-{index:03}").into_bytes()
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

fn put_batch(tbl: TableIdent, index_li: impl IntoIterator<Item = u32>) -> Request {
  Request::Put {
    tbl,
    batch_li: index_li
      .into_iter()
      .map(|index| BatchEntry::upsert(key(index), val(index), 1))
      .collect(),
    term: Some(1),
  }
}

fn expect_value(response: Result<Response, fkv_error::KvError>, index: u32) {
  match response.unwrap() {
    Response::Get(Some(got)) => assert_eq!(got, val(index)),
    other => panic!("unexpected response for {index}: {other:?}"),
  }
}

/// 成批只读请求（`Get` + `Scan`）同轮并发推进，结果按请求逐条正确回执。
#[compio::test]
async fn read_batch_serves_concurrent_gets_and_scan() {
  let root = TempRoot::new("read_batch");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("batch", 1).unwrap();
  shard
    .request(put_batch(tbl.clone(), 0..KEY_NUM))
    .await
    .unwrap();

  let mut handle_li = Vec::new();
  for index in 0..KEY_NUM {
    let sender = shard.sender();
    let tbl = tbl.clone();
    handle_li.push(spawn(async move {
      sender
        .request(Request::Get {
          tbl,
          key: key(index),
        })
        .await
    }));
  }
  {
    let sender = shard.sender();
    handle_li.push(spawn(async move {
      sender
        .request(Request::Scan {
          tbl,
          begin: key(0),
          end: b"key-zzz".to_vec(),
          max_entries: 0,
          option: ScanPageOption::default(),
        })
        .await
    }));
  }

  for (index, handle) in handle_li.into_iter().enumerate() {
    if index == KEY_NUM as usize {
      match handle.await.unwrap().unwrap() {
        Response::Scan(reply) => assert_eq!(reply.entry_li.len(), KEY_NUM as usize),
        other => panic!("unexpected scan response: {other:?}"),
      }
    } else {
      expect_value(handle.await.unwrap(), index as u32);
    }
  }
  shard.shutdown().await.unwrap();
}

/// 读批不会丢写：只读批后仍按原顺序处理排队的写请求，最终值可读。
#[compio::test]
async fn read_batch_does_not_lose_queued_writes() {
  let root = TempRoot::new("read_batch");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("mixed", 1).unwrap();

  // 交替投递读与写：读批在队列里成批推进，写请求靠 carry 槽位保持顺序。
  let mut handle_li = Vec::new();
  for index in 0..KEY_NUM {
    let sender = shard.sender();
    let put_tbl = tbl.clone();
    handle_li.push(spawn(async move {
      sender.request(put_batch(put_tbl, [index])).await
    }));
    let sender = shard.sender();
    let tbl = tbl.clone();
    handle_li.push(spawn(async move {
      sender
        .request(Request::Get {
          tbl,
          key: key(index),
        })
        .await
    }));
  }
  for handle in handle_li {
    handle.await.unwrap().unwrap();
  }

  for index in 0..KEY_NUM {
    let response = shard
      .request(Request::Get {
        tbl: tbl.clone(),
        key: key(index),
      })
      .await;
    expect_value(response, index);
  }
  shard.shutdown().await.unwrap();
}
