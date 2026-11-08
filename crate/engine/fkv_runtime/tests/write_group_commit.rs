use std::path::PathBuf;

use fkv_btree::BatchEntry;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;
use futures::future::join_all;

fn config(root: PathBuf) -> RuntimeConfig {
  let mut config = RuntimeConfig::for_tests(root);
  config.queue_size = 512;
  config
}

#[test]
fn concurrent_puts_are_group_committed() {
  let root = TempRoot::new("group_commit");
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.path().to_path_buf())).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("gc", 1).unwrap();
    let request_num = 256usize;
    let future_li = (0..request_num).map(|index| {
      let key = format!("k{index:04}").into_bytes();
      runtime.put(
        &tbl,
        vec![BatchEntry::upsert(key, vec![b'v'; 32], index as u64 + 1)],
      )
    });
    let result_li = join_all(future_li).await;
    assert!(result_li.iter().all(Result::is_ok));

    let fsync_num: u64 = runtime
      .metrics()
      .shard_li
      .iter()
      .map(|shard| shard.fsync_num)
      .sum();
    assert!(
      fsync_num < request_num as u64 / 2,
      "expected group commit, got {fsync_num} fsyncs for {request_num} puts"
    );
    assert_eq!(
      runtime.scan(&tbl, b"", b"", 0).await.unwrap().len(),
      request_num
    );
    runtime.shutdown().await.unwrap();
  });
}
