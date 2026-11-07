//! scan 流水与区间折算测试。

use super::*;

/// scan 链数 = `partitions * read_per_part`，分区按 `id % partitions` 轮转。
#[test]
fn scan_lanes_match_cpp_scanner_num() {
  let lane_li = scan_lane_li(4, 3);
  assert_eq!(lane_li.len(), 12);
  assert_eq!(lane_li.iter().filter(|lane| lane.part == 2).count(), 3);
  let mut seed_li: Vec<u64> = lane_li.iter().map(|lane| lane.seed).collect();
  seed_li.sort_unstable();
  seed_li.dedup();
  assert_eq!(seed_li.len(), 12);
  // 空表也建满 C++ 口径的 scanner 数。
  assert_eq!(scan_lane_li(8, 2).len(), 16);
}

/// redb 口径：总页请求数固定为 `--scan-count`，跨全部 scan 链均分。
#[test]
fn scan_count_is_shared_across_lanes() {
  let lane_li = scan_lane_li_with_count(1, 4, 500_000);
  let total: u64 = lane_li.iter().filter_map(|lane| lane.scans).sum();
  assert_eq!(total, 500_000);
  assert_eq!(
    lane_li.iter().map(|lane| lane.scans).collect::<Vec<_>>(),
    vec![Some(125_000); 4]
  );
  // None 保持历史语义（固定次数模式每链一页）。
  assert!(scan_lane_li(1, 4).iter().all(|lane| lane.scans.is_none()));
}

/// `--scan-bytes` 折算：0＝扫尾；否则 `max(1, bytes / value_size)`。
#[test]
fn scan_kvs_matches_cpp_conversion() {
  assert_eq!(scan_kvs(0, 120), None);
  assert_eq!(scan_kvs(1, 120), Some(1));
  assert_eq!(scan_kvs(119, 120), Some(1));
  assert_eq!(scan_kvs(120, 120), Some(1));
  assert_eq!(scan_kvs(239, 120), Some(1));
  assert_eq!(scan_kvs(240, 120), Some(2));
  assert_eq!(scan_kvs(1200, 120), Some(10));
}

/// 随机 scan 区间对齐 C++：扫尾时起点在 `[0, rows)`；窗口内
/// `start ∈ [0, rows - kvs)`、`end = start + kvs`（不含 `rows - kvs` 本身）。
#[test]
fn scan_range_truncates_window() {
  let mut rng = Rng::with_seed(0x5eed);
  for _ in 0..1000 {
    let (start, end) = scan_range(1000, Some(100), &mut rng);
    assert_eq!(end, Some(start + 100));
    assert!(start < 900, "start = {start}");
    assert!(end.unwrap() <= 999);
  }
  // 窗口大于键空间：C++ 回退为 start = 0，end 可越过 rows。
  assert_eq!(scan_range(10, Some(50), &mut rng), (0, Some(50)));
  // 扫到表尾：start ∈ [0, rows)，end 为空。
  for _ in 0..1000 {
    let (start, end) = scan_range(10, None, &mut rng);
    assert!(start < 10);
    assert_eq!(end, None);
  }
}

/// 固定次数模式（`test_secs = 0`）下 fullscan 只跑一轮：`fullscan_num`
/// 等于存活 key 数、`fullscan_total_us > 0`、`fullscan_rounds == 1`；
/// 既有的 `fullscan_kv_s` / `fullscan_*_us` 不受影响。
#[test]
fn fullscan_emits_first_round_num_and_us() {
  let metric_li = bench_workload(
    BenchArgs {
      rows: 64,
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
  let metric = |name: &str| -> (f64, &'static str) {
    metric_li
      .iter()
      .find(|(metric, ..)| metric == name)
      .map(|(.., value, unit)| (*value, *unit))
      .unwrap_or_else(|| panic!("缺少指标 {name}"))
  };
  // 网格无删除、单分区：一个全扫轮 = rows 条存活 key。
  assert_eq!(expected_live_grid(64, 1, 0), 64);
  assert_eq!(metric("fullscan_num"), (64.0, "num"));
  let (total_us, unit) = metric("fullscan_total_us");
  assert_eq!(unit, "us");
  assert!(total_us > 0.0, "fullscan_total_us = {total_us}");
  assert_eq!(metric("fullscan_rounds"), (1.0, "num"));
  // 既有指标保持原口径。
  assert!(metric("fullscan_kv_s").0 > 0.0);
  assert_eq!(metric("fullscan_kv_s").1, "kv/s");
  assert_latency_metric_li(&metric_li, "fullscan");
}

/// 稀疏 key + 多分区 + delete：`fullscan_num` 是「存活 key 数」而非 `rows`，
/// 与闭式 `live_key_num(partition_key_num(...))` 求和一致。
#[test]
fn fullscan_num_counts_live_keys_with_deletes() {
  let rows = 48usize;
  let partitions = 3usize;
  let mix = WriteMix {
    key_interval: 2,
    del_ratio: 4,
    ..WriteMix::default()
  };
  let expected_live = expected_live_grid(rows, partitions, mix.del_ratio);
  assert_eq!(
    expected_live,
    (0..partitions)
      .map(|part| live_key_num(partition_key_num(rows, partitions, part), mix.del_ratio))
      .sum::<usize>()
  );
  let metric_li = bench_workload(
    BenchArgs {
      rows,
      value_size: 16,
      batch_size: 8,
      workload: Workload::Scan,
      mix,
      plan: ReadPlan {
        partitions,
        read_per_part: 1,
      },
      test_secs: 0.0,
      write_batchs: 0,
      scan_bytes: 0,
      ttl_ms: 0,
    },
    None,
  );
  let num = metric_li
    .iter()
    .find(|(name, ..)| name == "fullscan_num")
    .map(|item| item.1)
    .expect("缺少指标 fullscan_num");
  assert_eq!(num, expected_live as f64);
  assert!(num < rows as f64, "fullscan_num 应是存活数而非 rows");
}

/// 网格 workload 的存活 key 闭式：各分区 `partition_key_num` 经
/// `live_key_num` 求和（`del_ratio < 2` 即无删除）。
fn expected_live_grid(rows: usize, partitions: usize, del_ratio: usize) -> usize {
  (0..partitions)
    .map(|part| live_key_num(partition_key_num(rows, partitions, part), del_ratio))
    .sum()
}
