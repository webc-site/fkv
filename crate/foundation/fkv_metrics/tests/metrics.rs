use std::{sync::Arc, time::Duration};

use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_io_mem::MemStore;
use fkv_metrics::{
  BUCKET_NUM, BUCKET_UPPER_US, HistogramSnapshot, MeteredIo, MetricsSnapshot, Op, ShardMetrics,
};
use fkv_types::TableIdent;

#[test]
fn histogram_buckets_min_max_and_errors() {
  let metrics = ShardMetrics::new(3);
  metrics.observe(Op::Get, Duration::from_micros(49), false);
  metrics.observe(Op::Get, Duration::from_micros(50), false);
  metrics.observe(Op::Get, Duration::from_micros(51), true);
  metrics.observe(Op::Get, Duration::from_micros(10_000), false);
  metrics.observe(Op::Get, Duration::from_micros(6_000_000), false);

  let snapshot = metrics.snapshot();
  assert_eq!(snapshot.shard_id, 3);
  let get = snapshot.op(Op::Get);
  assert_eq!(get.total, 5);
  assert_eq!(get.error, 1);
  assert_eq!(get.latency.count, 5);
  assert_eq!(get.latency.bucket_count[0], 2); // 49us, 50us
  assert_eq!(get.latency.bucket_count[1], 1); // 51us
  assert_eq!(get.latency.bucket_count[7], 1); // 10_000us
  assert_eq!(get.latency.bucket_count[BUCKET_NUM - 1], 1); // +Inf
  assert_eq!(get.latency.min_ns, 49_000);
  assert_eq!(get.latency.max_ns, 6_000_000_000);
  assert_eq!(
    get.latency.sum_ns,
    49_000 + 50_000 + 51_000 + 10_000_000 + 6_000_000_000
  );
  assert_eq!(get.latency.avg_ns(), get.latency.sum_ns / 5);
  assert_eq!(HistogramSnapshot::bucket_upper_us(0), Some(50));
  assert_eq!(HistogramSnapshot::bucket_upper_us(BUCKET_NUM - 1), None);
  assert_eq!(BUCKET_UPPER_US[BUCKET_NUM - 2], 5_000_000);
}

#[test]
fn metrics_aggregate_across_shards() {
  let first = ShardMetrics::new(0);
  let second = ShardMetrics::new(1);
  first.observe(Op::Put, Duration::from_millis(1), false);
  first.observe(Op::Get, Duration::from_millis(1), false);
  second.observe(Op::Put, Duration::from_millis(1), true);

  let snapshot = MetricsSnapshot {
    shard_li: vec![first.snapshot(), second.snapshot()],
  };
  assert_eq!(snapshot.shard_li.len(), 2);
  assert_eq!(snapshot.total_requests(Op::Put), 2);
  assert_eq!(snapshot.total_errors(Op::Put), 1);
  assert_eq!(snapshot.total_requests(Op::Get), 1);
}

#[compio::test]
async fn metered_io_counts_pages_manifests_segments_and_files() {
  let metrics = Arc::new(ShardMetrics::new(0));
  let io = MeteredIo::new(
    MemStore::new(8).with_segment_config(4096, 2),
    metrics.clone(),
  );
  let tbl = TableIdent::new("demo", 1).unwrap();

  io.write_page(&tbl, 0, b"12345678").await.unwrap();
  let mut page = [0u8; 8];
  io.read_page(&tbl, 0, &mut page).await.unwrap();
  io.append_manifest(&tbl, b"abc").await.unwrap();
  io.switch_manifest(&tbl, b"xy").await.unwrap();
  assert_eq!(io.read_manifest(&tbl).await.unwrap().unwrap(), b"xy");
  io.write_segment(&tbl, 0, b"seg").await.unwrap();
  let mut segment = [0u8; 3];
  io.read_segment(&tbl, 0, &mut segment).await.unwrap();
  io.write_file(&tbl, 0, b"12345678").await.unwrap();
  assert!(io.read_file(&tbl, 0).await.unwrap().is_some());
  io.remove_file(&tbl, 0).await.unwrap();

  let snapshot = metrics.snapshot();
  assert_eq!(snapshot.page_reads, 1);
  assert_eq!(snapshot.page_writes, 1);
  assert_eq!(snapshot.manifest_reads, 1);
  assert_eq!(snapshot.manifest_writes, 2);
  assert_eq!(snapshot.segment_reads, 1);
  assert_eq!(snapshot.segment_writes, 1);
  assert_eq!(snapshot.file_reads, 1);
  assert_eq!(snapshot.file_writes, 1);
  assert_eq!(snapshot.file_removes, 1);
  assert_eq!(snapshot.bytes_written, 8 + 3 + 2 + 3 + 8);
  assert_eq!(snapshot.bytes_read, 8 + 2 + 3 + 8);
}
