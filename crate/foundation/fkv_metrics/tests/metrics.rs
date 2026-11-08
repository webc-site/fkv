use std::time::Duration;

use fkv_metrics::{
  BUCKET_NUM, BUCKET_UPPER_US, HistogramSnapshot, METRIC_REQUEST_TYPE_LI, MetricsSnapshot, Op,
  ReadBatchSnapshot, ShardMetric,
};

#[test]
fn histogram_buckets_min_max_and_errors() {
  let metrics = ShardMetric::new(3);
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
  let first = ShardMetric::new(0);
  let second = ShardMetric::new(1);
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

/// 名字不同的映射与 Gc/Drop 的拆分逐条钉死。
#[test]
fn op_to_request_type_mapping() {
  assert_eq!(Op::Put.request_type_li(), &["batch_write"]);
  assert_eq!(Op::Get.request_type_li(), &["read"]);
  assert_eq!(Op::CreateArchive.request_type_li(), &["archive"]);
  assert_eq!(Op::FileGc.request_type_li(), &["file_gc"]);
  assert_eq!(Op::LocalGc.request_type_li(), &["local_gc"]);
  assert_eq!(Op::Drop.request_type_li(), &["drop", "drop_table"]);
  assert_eq!(Op::ListObject.request_type_li(), &["list_object"]);
  assert_eq!(
    Op::ListStandbyPartition.request_type_li(),
    &["list_standby_partition"]
  );
  // 控制面请求没有独立标签。
  assert!(Op::Prewarm.request_type_li().is_empty());
  assert!(Op::ListArchiveTags.request_type_li().is_empty());
  assert!(Op::Shutdown.request_type_li().is_empty());

  assert_eq!(Op::from_request_type("batch_write"), Some(Op::Put));
  assert_eq!(Op::from_request_type("read"), Some(Op::Get));
  assert_eq!(Op::from_request_type("archive"), Some(Op::CreateArchive));
  assert_eq!(Op::from_request_type("local_gc"), Some(Op::LocalGc));
  assert_eq!(Op::from_request_type("file_gc"), Some(Op::FileGc));
  assert_eq!(Op::from_request_type("drop_table"), Some(Op::Drop));
  assert_eq!(Op::from_request_type("list_object"), Some(Op::ListObject));
  assert_eq!(
    Op::from_request_type("list_standby_partition"),
    Some(Op::ListStandbyPartition)
  );
  assert_eq!(Op::from_request_type("global_archive"), None);
  assert_eq!(Op::from_request_type("global_list_archive_tags"), None);
}

/// 注册的 19 个标签全部能反查；每个标签只属于一个 `Op`。
#[test]
fn metric_tags_are_covered_once() {
  for tag in METRIC_REQUEST_TYPE_LI {
    assert!(Op::from_request_type(tag).is_some(), "tag {tag}");
  }
  for op in Op::ALL {
    for &tag in op.request_type_li() {
      assert!(METRIC_REQUEST_TYPE_LI.contains(&tag), "tag {tag}");
      assert_eq!(Op::from_request_type(tag), Some(op), "tag {tag}");
      let owners = Op::ALL
        .iter()
        .filter(|other| other.request_type_li().contains(&tag))
        .count();
      assert_eq!(owners, 1, "tag {tag} 被多个 Op 映射");
    }
  }
  assert_eq!(Op::ALL.len(), 21);
}

/// 口径锁定（l2-04）：C++ `RequestType` 里的 4 个全局操作名与 `unknown` **不**登记为
/// 指标标签。
///
/// 它们在 C++ 侧只是 `RequestTypeToString` 的名字：全局请求由 `EloqStore` 在 store
/// 层 fanout 出每分区子请求，顶层请求永不进入 `Shard::OnTaskFinished` 的埋点
/// （`eloqstore/src/storage/shard.cpp:1513-1520`），因此这 4 个名字从不产出
/// `eloqstore_request_latency` / `eloqstore_requests_completed` 样本，`unknown`
/// 只是 default 分支。fkv 的全局操作同样在 runtime 层 fanout，指标落在每分区的
/// `archive` / `reopen` / `create_branch` 上，不额外注册全局标签（登记见
/// `eloqstore2fkv/COMPAT_BASELINE`「全局操作不计入 shard 指标」）。若有人补上这些
/// 标签，跨实现对账会多出 C++ 侧不存在的序列，本用例负责挡住该回归。
#[test]
fn global_request_labels_are_not_registered() {
  for tag in [
    "global_archive",
    "global_reopen",
    "global_list_archive_tags",
    "global_create_branch",
    "unknown",
  ] {
    assert!(
      !METRIC_REQUEST_TYPE_LI.contains(&tag),
      "tag {tag} 不应登记为 shard 指标标签"
    );
    assert_eq!(Op::from_request_type(tag), None, "tag {tag}");
  }
}

/// C++ 的 index buffer pool 与 local space 是两个 gauge，快照必须分开可读。
#[test]
fn cache_gauges_split_cloud_and_index() {
  let metrics = ShardMetric::new(0);
  metrics.set_cache_bytes(4096);
  metrics.set_page_cache_bytes(1024);

  let snapshot = metrics.snapshot();
  assert_eq!(snapshot.cloud_cache_bytes, 4096);
  assert_eq!(snapshot.page_cache_bytes, 1024);
  assert_eq!(snapshot.cache_bytes, 5120);
}

/// 读侧在飞 gauge：当前值随写入变化，峰值保持单调（eng-32c）。
#[test]
fn read_in_flight_gauge_tracks_current_and_peak() {
  let metrics = ShardMetric::new(0);
  metrics.set_read_in_flight(3);
  metrics.set_read_in_flight(7);
  metrics.set_read_in_flight(2);

  let snapshot = metrics.snapshot();
  assert_eq!(snapshot.read_in_flight, 2);
  assert_eq!(snapshot.read_in_flight_max, 7);
}

/// 读批大小走计数型桶，空批不记；`avg` / `max` 与桶边界可读（eng-32c）。
#[test]
fn read_batch_histogram_records_batch_sizes() {
  let metrics = ShardMetric::new(0);
  metrics.record_read_batch(1);
  metrics.record_read_batch(4);
  metrics.record_read_batch(32);
  metrics.record_read_batch(0);

  let batch = metrics.snapshot().read_batch_size;
  assert_eq!(batch.count, 3);
  assert_eq!(batch.sum, 37);
  assert_eq!(batch.max, 32);
  assert_eq!(batch.avg(), 12);
  // 桶上界 [1, 2, 4, 8, 16, 32] + Inf：1 / 4 / 32 各落一桶。
  assert_eq!(batch.bucket_count[0], 1);
  assert_eq!(batch.bucket_count[2], 1);
  assert_eq!(batch.bucket_count[5], 1);
  assert_eq!(ReadBatchSnapshot::bucket_upper(5), Some(32));
  assert_eq!(ReadBatchSnapshot::bucket_upper(6), None);
}
