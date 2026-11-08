use std::path::PathBuf;

use fkv_btree::BatchEntry;
use fkv_metrics::Op;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn config(root: PathBuf) -> RuntimeConfig {
  let mut config = RuntimeConfig::for_tests(root);
  config.segments_per_file_shift = 1;
  config.fixed_buffer_size = 65536;
  config
}

fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

#[test]
fn runtime_large_value_put_get_gc_and_reopen() {
  let root = TempRoot::new("runtime_large");
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.path().to_path_buf())).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    let big = pattern(9000);
    let small = b"small-inline".to_vec();
    runtime
      .put_large(
        &tbl,
        vec![
          BatchEntry::upsert(b"big".to_vec(), big.clone(), 1).with_metadata(b"meta"),
          BatchEntry::upsert(b"small".to_vec(), small.clone(), 1),
        ],
      )
      .await
      .unwrap();

    let read = runtime.get_large(&tbl, b"big").await.unwrap().unwrap();
    assert_eq!(read.value, big);
    assert_eq!(read.metadata, b"meta");
    let read = runtime.get_large(&tbl, b"small").await.unwrap().unwrap();
    assert_eq!(read.value, small);
    assert!(read.metadata.is_empty());
    assert_eq!(runtime.get_large(&tbl, b"missing").await.unwrap(), None);

    assert!(root.join("demo.1").join("segment_0_main_1").exists());
    assert!(root.join("demo.1").join("segment_1_main_1").exists());
    let metrics = runtime.metrics();
    assert!(
      metrics
        .shard_li
        .iter()
        .map(|shard| shard.segment_writes)
        .sum::<u64>()
        > 0
    );
    assert!(
      metrics
        .shard_li
        .iter()
        .map(|shard| shard.segment_reads)
        .sum::<u64>()
        > 0
    );
    assert_eq!(metrics.total_requests(Op::Put), 1);
    assert_eq!(metrics.total_requests(Op::Get), 3);

    // 覆盖为较小的大 value，旧 segment 文件应被 runtime.gc 回收（segment 1 仍被引用）。
    let big2 = pattern(3000);
    runtime
      .put_large(
        &tbl,
        vec![BatchEntry::upsert(b"big".to_vec(), big2.clone(), 2)],
      )
      .await
      .unwrap();
    assert_eq!(
      runtime
        .get_large(&tbl, b"big")
        .await
        .unwrap()
        .unwrap()
        .value,
      big2
    );
    // `Runtime::start` 默认开启放大因子 2：写后维护可能已回收旧段，
    // 显式 `file_gc` 只做幂等补扫。
    runtime.file_gc(&tbl).await.unwrap();
    assert!(!root.join("demo.1").join("segment_0_main_1").exists());
    assert!(root.join("demo.1").join("segment_1_main_1").exists());

    // TTL：过期的大 value 读取返回 None。
    let mut expired = BatchEntry::upsert(b"expired".to_vec(), pattern(3000), 3);
    expired.expire_ts = 1;
    runtime.put_large(&tbl, vec![expired]).await.unwrap();
    assert_eq!(runtime.get_large(&tbl, b"expired").await.unwrap(), None);

    runtime.shutdown().await.unwrap();

    let runtime = Runtime::start(config(root.path().to_path_buf())).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    runtime.reopen(&tbl).await.unwrap();
    let read = runtime.get_large(&tbl, b"big").await.unwrap().unwrap();
    assert_eq!(read.value, big2);
    let read = runtime.get_large(&tbl, b"small").await.unwrap().unwrap();
    assert_eq!(read.value, small);
    runtime.shutdown().await.unwrap();
  });
}
