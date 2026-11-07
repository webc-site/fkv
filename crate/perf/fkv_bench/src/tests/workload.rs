//! workload 端到端指标与存活模型测试。

use super::*;

#[test]
fn workload_parses_redb_style_lanes() {
  assert_eq!(Workload::parse("delete-only"), Workload::DeleteOnly);
  assert_eq!(Workload::parse("compact-only"), Workload::CompactOnly);
  assert_eq!(Workload::DeleteOnly.as_str(), "delete-only");
  assert_eq!(Workload::CompactOnly.as_str(), "compact-only");
  assert!(Workload::DeleteOnly.measures_delete());
  assert!(Workload::CompactOnly.is_compact_only());
  assert!(!Workload::DeleteOnly.measures_read());
  assert!(!Workload::CompactOnly.measures_write());
}

#[test]
fn workload_reports_all_metrics() {
  let metric_li = bench_workload(
    BenchArgs {
      rows: 64,
      value_size: 32,
      batch_size: 16,
      workload: Workload::WriteRead,
      mix: WriteMix::default(),
      plan: ReadPlan {
        partitions: 1,
        read_per_part: 4,
      },
      test_secs: 0.0,
      write_batchs: 0,
      scan_bytes: 0,
      ttl_ms: 0,
    },
    None,
  );
  assert_eq!(
    throughput_name_li(&metric_li),
    vec![
      "put_kv_s",
      "put_batch_s",
      "put_window_kv_s",
      "get_kv_s",
      "get_window_kv_s",
      "scan_kv_s",
      "scan_page_s",
      "fullscan_kv_s"
    ]
  );
  for prefix in ["put", "get", "scan", "fullscan"] {
    assert_latency_metric_li(&metric_li, prefix);
  }
  for (name, value, unit) in &metric_li {
    assert!(*value > 0.0, "{name} = {value}");
    assert!(!unit.is_empty());
  }
}

#[test]
fn write_only_reports_write_metrics() {
  let metric_li = bench_workload(
    BenchArgs {
      rows: 32,
      value_size: 16,
      batch_size: 8,
      workload: Workload::WriteOnly,
      mix: WriteMix::default(),
      plan: ReadPlan {
        partitions: 1,
        read_per_part: 1,
      },
      test_secs: 0.0,
      write_batchs: 0,
      scan_bytes: 0,
      ttl_ms: 0,
    },
    None,
  );
  assert_eq!(
    throughput_name_li(&metric_li),
    vec!["put_kv_s", "put_batch_s", "put_window_kv_s"]
  );
  assert_latency_metric_li(&metric_li, "put");
  assert_eq!(
    metric_li.len(),
    2 + LAT_SUFFIX_LI.len() + WINDOW_SUFFIX_LI.len()
  );
}

#[test]
fn read_only_reports_read_metrics() {
  let metric_li = bench_workload(
    BenchArgs {
      rows: 32,
      value_size: 16,
      batch_size: 8,
      workload: Workload::ReadOnly,
      mix: WriteMix::default(),
      plan: ReadPlan {
        partitions: 1,
        read_per_part: 2,
      },
      test_secs: 0.0,
      write_batchs: 0,
      scan_bytes: 0,
      ttl_ms: 0,
    },
    None,
  );
  assert_eq!(
    throughput_name_li(&metric_li),
    vec![
      "get_kv_s",
      "get_window_kv_s",
      "scan_kv_s",
      "scan_page_s",
      "fullscan_kv_s"
    ]
  );
  for prefix in ["get", "scan", "fullscan"] {
    assert_latency_metric_li(&metric_li, prefix);
  }
}

#[test]
fn partitions_split_rows_across_tables() {
  // 两个分区、rows=16：写入与两种 scan 都应覆盖全部 16 条。
  let metric_li = bench_workload(
    BenchArgs {
      rows: 16,
      value_size: 16,
      batch_size: 4,
      workload: Workload::WriteRead,
      mix: WriteMix::default(),
      plan: ReadPlan {
        partitions: 2,
        read_per_part: 1,
      },
      test_secs: 0.0,
      write_batchs: 0,
      scan_bytes: 0,
      ttl_ms: 0,
    },
    None,
  );
  assert_eq!(throughput_name_li(&metric_li).len(), 8);
  assert_eq!(
    metric_li.len(),
    // 6 个纯吞吐名 + 4 阶段延迟 + put/get 两组窗口（含 `_window_kv_s`）
    // + 写后 reopen_us + fullscan 首轮诊断 3 项
    // （`fullscan_num` / `fullscan_total_us` / `fullscan_rounds`）。
    6 + 4 * LAT_SUFFIX_LI.len() + 2 * WINDOW_SUFFIX_LI.len() + 1 + 3
  );
}

/// 多分区 × 多读链：每条读链绑定各自分区，读侧断言在全部分区上仍成立。
#[test]
fn concurrent_read_lanes_cover_all_partitions() {
  let metric_li = bench_workload(
    BenchArgs {
      rows: 24,
      value_size: 16,
      batch_size: 8,
      workload: Workload::WriteRead,
      mix: WriteMix::default(),
      plan: ReadPlan {
        partitions: 3,
        read_per_part: 2,
      },
      test_secs: 0.0,
      write_batchs: 0,
      scan_bytes: 0,
      ttl_ms: 0,
    },
    None,
  );
  assert_eq!(throughput_name_li(&metric_li).len(), 8);
  assert_latency_metric_li(&metric_li, "get");
}

/// `scan` 只计量扫描：写入作数据准备，不计入指标。
#[test]
fn scan_only_workload_measures_scan_not_write() {
  let metric_li = bench_workload(
    BenchArgs {
      rows: 24,
      value_size: 16,
      batch_size: 8,
      workload: Workload::Scan,
      mix: WriteMix::default(),
      plan: ReadPlan {
        partitions: 1,
        read_per_part: 1,
      },
      test_secs: 0.0,
      write_batchs: 0,
      scan_bytes: 0,
      ttl_ms: 0,
    },
    None,
  );
  assert_eq!(
    throughput_name_li(&metric_li),
    vec!["scan_kv_s", "scan_page_s", "fullscan_kv_s"]
  );
  for prefix in ["scan", "fullscan"] {
    assert_latency_metric_li(&metric_li, prefix);
  }
}

/// `write-scan` + 稀疏 key + delete：扫描条数为存活 key 数，写入仍计量。
#[test]
fn sparse_keys_and_deletes_stay_consistent() {
  let mix = WriteMix {
    key_interval: 2,
    del_ratio: 4,
    ..WriteMix::default()
  };
  let metric_li = bench_workload(
    BenchArgs {
      rows: 32,
      value_size: 16,
      batch_size: 8,
      workload: Workload::WriteScan,
      mix,
      plan: ReadPlan {
        partitions: 1,
        read_per_part: 1,
      },
      test_secs: 0.0,
      write_batchs: 0,
      scan_bytes: 0,
      ttl_ms: 0,
    },
    None,
  );
  assert_eq!(
    throughput_name_li(&metric_li),
    vec![
      "put_kv_s",
      "put_batch_s",
      "put_window_kv_s",
      "scan_kv_s",
      "scan_page_s",
      "fullscan_kv_s"
    ]
  );
  assert_eq!(live_key_num(32, 4), 16);
}

/// 覆盖写 + 随机 scan 窗口：`--write-batchs 8 --batch-size 8` 写 64 条、
/// `--scan-bytes 320`（value 16B）→ 每请求 20 条窗口；读 / scan 的存活断言
/// 在部分覆盖的键空间上仍成立。
#[test]
fn churn_and_scan_bytes_smoke() {
  let metric_li = bench_workload(
    BenchArgs {
      rows: 32,
      value_size: 16,
      batch_size: 8,
      workload: Workload::WriteRead,
      mix: WriteMix {
        key_interval: 1,
        del_ratio: 4,
        ..WriteMix::default()
      },
      plan: ReadPlan {
        partitions: 2,
        read_per_part: 2,
      },
      test_secs: 0.0,
      write_batchs: 8,
      scan_bytes: 320,
      ttl_ms: 0,
    },
    None,
  );
  assert_eq!(
    throughput_name_li(&metric_li),
    vec![
      "put_kv_s",
      "put_batch_s",
      "put_window_kv_s",
      "get_kv_s",
      "get_window_kv_s",
      "scan_kv_s",
      "scan_page_s",
      "fullscan_kv_s"
    ]
  );
  for prefix in ["put", "get", "scan", "fullscan"] {
    assert_latency_metric_li(&metric_li, prefix);
  }
}

/// 时间窗模式（`test_secs` > 0）：读 / scan / fullscan 在各自窗口内循环
/// 执行，样本数 > 0、分位字段齐全；写阶段仍按次数驱动（只写一遍）。只断言
/// 窗口下界，不做精确时长断言（CI 时钟抖动），窗口判停本身保证每阶段耗时
/// ≥ 窗口。
#[test]
fn test_secs_window_collects_samples() {
  // 三个计时阶段各 0.2s 窗口，总耗时下界 0.6s。
  let started = Instant::now();
  let metric_li = bench_workload(
    BenchArgs {
      rows: 32,
      value_size: 16,
      batch_size: 8,
      workload: Workload::WriteRead,
      mix: WriteMix::default(),
      plan: ReadPlan {
        partitions: 1,
        read_per_part: 2,
      },
      test_secs: 0.2,
      write_batchs: 0,
      scan_bytes: 0,
      ttl_ms: 0,
    },
    None,
  );
  assert!(started.elapsed() >= Duration::from_millis(600));
  // 写阶段按次数执行，仍产出完整指标；读 / scan 在窗口内产出样本。
  for prefix in ["put", "get", "scan", "fullscan"] {
    assert_latency_metric_li(&metric_li, prefix);
  }
  // 窗口内每个阶段都有样本：吞吐与全部分位值为正（样本亚微秒也按 1µs 计）。
  for (name, value, unit) in &metric_li {
    assert!(*value > 0.0, "{name} = {value}");
    assert!(!unit.is_empty());
  }
}

/// `del_ratio` 的 delete 落在 `del_ratio - 1` 步之前的 key 上，不是删不存在的 key。
#[test]
fn delete_targets_earlier_keys() {
  assert!(!is_deleted(2, 4));
  assert!(is_deleted(3, 4));
  assert_eq!(delete_target(3, 4), 0);
  assert_eq!(delete_target(7, 4), 4);
  // 存活判定：delete 位自身不写 key；倍数额被后续 delete 摘掉。
  assert!(is_live(1, 10, 4));
  assert!(!is_live(0, 10, 4));
  assert!(!is_live(3, 10, 4));
  // 尾部：count=10 时 delete 位只有 3 / 7，8 的 delete 位 11 不存在 → 8 存活。
  assert!(is_live(8, 10, 4));
  assert_eq!(live_key_num(10, 4), 6);
  assert_eq!(live_key_num(32, 4), 16);
  assert_eq!(live_key_num(4, 4), 2);
  assert_eq!(live_key_num(3, 4), 3);
  // 不删时全部存活。
  assert!(is_live(0, 10, 0));
  assert!(is_live(3, 10, 1));
  assert_eq!(live_key_num(10, 0), 10);
}

/// 覆盖写后的存活判定：未写 / 部分前缀 / 整遍 / 整遍 + 前缀。
#[test]
fn live_after_covers_partial_passes() {
  // 未写入。
  assert!(!live_after(0, 10, 1, 4));
  // written < count：只写了前缀 5（0..4），delete 位 3 已摘掉 key 0。
  assert!(!live_after(5, 10, 0, 4));
  assert!(live_after(5, 10, 1, 4));
  assert!(!live_after(5, 10, 3, 4));
  assert!(live_after(5, 10, 4, 4), "delete 位 7 还没发生");
  assert!(!live_after(5, 10, 5, 4), "前缀之外从未写入");
  // 整遍：与单遍语义一致。
  for pos in 0..10 {
    assert_eq!(live_after(10, 10, pos, 4), is_live(pos, 10, 4));
    assert_eq!(live_after(20, 10, pos, 4), is_live(pos, 10, 4));
  }
  // 整遍 + 前缀 2：位置 0,1 按新前缀判定，其余保持整遍状态。
  // 前缀 2 里 key 0 刚被 upsert、delete 位 3 还没到 → 存活。
  assert!(live_after(12, 10, 0, 4));
  assert_eq!(live_after(12, 10, 0, 4), is_live(0, 2, 4));
  assert_eq!(live_after(12, 10, 5, 4), is_live(5, 10, 4));
}

#[test]
fn effective_rows_is_capped_by_max_key() {
  assert_eq!(effective_rows(100, 10), 10);
  assert_eq!(effective_rows(10, 100), 10);
  assert_eq!(effective_rows(0, 0), 1);
}
