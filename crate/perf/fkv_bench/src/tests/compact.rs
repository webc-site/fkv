//! compact lane 端到端测试。

use super::*;

/// `compact` lane：维护分区有 compact 调用样本且**确实搬了文件**，读分区有
/// 前台读分位；文件粒度以 `compact_pages_per_file` 显式输出。固定次数模式下
/// 轮数由分区 0 的写遍数决定。
///
/// `value_size` 取 4096：准备阶段每分区 128 条 × 2 遍即可写满多个非尾文件
/// （256 页/文件），`compact_plan` 才有候选；小 value（如 32B）在同规模下
/// 只有几十页，`compact_file_num` 恒为 0，测不到搬运。
#[test]
fn compact_lane_reports_pressure_and_read_metrics() {
  let metric_li = bench_workload(
    BenchArgs {
      rows: 256,
      value_size: 4096,
      batch_size: 16,
      workload: Workload::Compact,
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
  let names = metric_name_li(&metric_li);
  for name in [
    "compact_pages_per_file",
    "compact_run_num",
    "compact_file_num",
    "compact_file_s",
    "compact_put_kv_s",
    "compact_put_batch_s",
    "get_kv_s",
  ] {
    assert!(names.contains(&name), "缺少指标 {name}: {names:?}");
  }
  for prefix in ["compact", "compact_put", "get"] {
    assert_latency_metric_li(&metric_li, prefix);
  }
  let metric = |name: &str| {
    metric_li
      .iter()
      .find(|item| item.0 == name)
      .map(|item| item.1)
      .unwrap_or_else(|| panic!("缺少指标 {name}"))
  };
  assert_eq!(metric("compact_pages_per_file"), 256.0);
  assert!(metric("compact_run_num") > 0.0);
  // 断言 lane 确实发生 compact：只查调用次数 / 吞吐无法区分「空转」。
  assert!(
    metric("compact_file_num") > 0.0,
    "compact lane 未搬任何文件（compact_file_num = 0）"
  );
  assert!(metric("compact_put_kv_s") > 0.0);
  assert!(metric("get_kv_s") > 0.0);
}
