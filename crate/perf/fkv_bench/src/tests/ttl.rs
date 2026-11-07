//! TTL lane 与闭式模型测试。

use super::*;

/// TTL 位置只加在 upsert 上：delete 位不带 TTL，未选中位置保持
/// `expire_ts == 0`（默认快路径不变）。
#[test]
fn write_entry_marks_ttl_positions() {
  let mix = WriteMix {
    key_interval: 1,
    del_ratio: 4,
    ttl_ratio: 2,
    ttl_expire_ts: 42,
    ..WriteMix::default()
  };
  // 序列位 1：被 ttl_ratio 选中且非 delete 位 → 带 TTL。
  assert_eq!(write_entry(0, 1, 1, mix, b"v").expire_ts, 42);
  // 序列位 0 未选中。
  assert_eq!(write_entry(0, 1, 0, mix, b"v").expire_ts, 0);
  // 序列位 3 是 delete 位：即使 3 % 2 == 1 也不带 TTL。
  let del = write_entry(0, 1, 3, mix, b"v");
  assert!(del.op.is_delete());
  assert_eq!(del.expire_ts, 0);
  // 默认 mix（ttl_ratio = 0）不写 TTL：整批借用快路径与历史口径一致。
  assert_eq!(write_entry(0, 1, 1, WriteMix::default(), b"v").expire_ts, 0);
}

/// TTL 树条数按「最后一次写仍是带 TTL 的 upsert」去重：部分前缀、整遍覆盖与
/// delete 摘除都要算对。
#[test]
fn ttl_live_key_num_counts_final_ttl_keys() {
  let mix = WriteMix {
    ttl_ratio: 2,
    ttl_expire_ts: 1,
    ..WriteMix::default()
  };
  // 10 个位置里 1/3/5/7/9 带 TTL。
  assert_eq!(ttl_live_key_num(10, 10, mix), 5);
  // 只写了前缀 4 个位置（0..3）→ 1、3。
  assert_eq!(ttl_live_key_num(4, 10, mix), 2);
  // del_ratio 4：3、7 是 delete 位；7 摘掉 4、3 摘掉 0 → TTL 只剩 1、5、9。
  let mix = WriteMix {
    del_ratio: 4,
    ..mix
  };
  assert_eq!(ttl_live_key_num(10, 10, mix), 3);
  // 未启用 TTL。
  assert_eq!(ttl_live_key_num(10, 10, WriteMix::default()), 0);
}

/// `ttl-write` lane：写阶段输出 `put_ttl_*`，写完后把注入时钟推到到期时刻，
/// `clean_expired` 应把 TTL 索引与主树条目全部回收（32 条）。
#[test]
fn ttl_write_lane_reclaims_all_entries() {
  let ttl_ms = 3_600_000u64;
  let metric_li = bench_workload(
    BenchArgs {
      rows: 64,
      value_size: 16,
      batch_size: 8,
      workload: Workload::TtlWrite,
      mix: WriteMix {
        ttl_ratio: 2,
        ttl_expire_ts: now_ms().saturating_add(ttl_ms),
        ..WriteMix::default()
      },
      plan: ReadPlan {
        partitions: 1,
        read_per_part: 1,
      },
      test_secs: 0.0,
      write_batchs: 0,
      scan_bytes: 0,
      ttl_ms,
    },
    None,
  );
  assert_eq!(
    throughput_name_li(&metric_li),
    vec![
      "put_ttl_kv_s",
      "put_ttl_batch_s",
      "put_ttl_window_kv_s",
      "ttl_clean_kv_s"
    ]
  );
  for prefix in ["put_ttl", "ttl_clean"] {
    assert_latency_metric_li(&metric_li, prefix);
  }
  let metric = |name: &str| {
    metric_li
      .iter()
      .find(|item| item.0 == name)
      .map(|item| item.1)
      .unwrap_or_else(|| panic!("缺少指标 {name}"))
  };
  assert_eq!(metric("ttl_ratio"), 2.0);
  assert_eq!(metric("ttl_ms"), ttl_ms as f64);
  assert_eq!(metric("ttl_clean_deleted"), 32.0);
  assert_eq!(metric("ttl_clean_ttl_deleted"), 32.0);
  assert!(metric("ttl_clean_call_num") >= 1.0);
  // 该 lane 不计量读 / scan。
  assert!(
    !metric_li
      .iter()
      .any(|(name, ..)| name.starts_with("get_") || name.starts_with("scan_"))
  );
}
