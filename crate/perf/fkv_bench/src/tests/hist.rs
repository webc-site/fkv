//! 延迟直方图测试。

use super::*;

#[test]
fn latency_hist_empty_reports_zero() {
  let hist = LatencyHist::new();
  assert_eq!(hist.percentile_us(9_900), 0);
  assert_eq!(hist.avg_us(), 0.0);
  assert_eq!(hist.max_us, 0);
}

#[test]
fn latency_hist_linear_percentiles() {
  let mut hist = LatencyHist::new();
  for us in 1..=100u64 {
    hist.record(Duration::from_micros(us));
  }
  // 100 个样本：p50 落在 50µs 桶，p99 落在 99µs 桶，p100（万分位 10_000）为最大值。
  assert_eq!(hist.percentile_us(5_000), 50);
  assert_eq!(hist.percentile_us(9_900), 99);
  assert_eq!(hist.percentile_us(10_000), 100);
  assert_eq!(hist.max_us, 100);
  assert_eq!(hist.avg_us(), 50.5);
}

#[test]
fn latency_hist_overflow_bucket() {
  let mut hist = LatencyHist::new();
  hist.record(Duration::from_micros(100_000));
  // 100ms 落在 [65536, 131072)µs 桶，单一最大值时钳制到实测 max。
  assert_eq!(hist.percentile_us(5_000), 100_000);
  assert_eq!(hist.max_us, 100_000);
}

#[test]
fn latency_hist_percentiles_are_monotonic() {
  let mut hist = LatencyHist::new();
  for us in [3u64, 7, 20, 900, 5_000, 70_000, 2_000_000] {
    hist.record(Duration::from_micros(us));
  }
  let mut last = 0;
  for per_10000 in [5_000u32, 9_000, 9_900, 9_990, 9_999, 10_000] {
    let value = hist.percentile_us(per_10000);
    assert!(value >= last, "p{per_10000} = {value} < {last}");
    last = value;
  }
}

#[test]
fn latency_hist_merge_combines_counts() {
  let mut hist = LatencyHist::new();
  hist.record(Duration::from_micros(10));
  hist.record(Duration::from_micros(20));
  let mut other = LatencyHist::new();
  other.record(Duration::from_micros(30));
  other.record(Duration::from_micros(100_000));
  hist.merge(&other);
  // 归并后与单份记录 4 个样本等价：count / sum / max 一致，分位仍单调。
  assert_eq!(hist.count, 4);
  assert_eq!(hist.sum_us, 10 + 20 + 30 + 100_000);
  assert_eq!(hist.max_us, 100_000);
  assert_eq!(hist.avg_us(), hist.sum_us as f64 / 4.0);
  assert_eq!(hist.percentile_us(10_000), 100_000);
  // 归并到空直方图等于复制。
  let mut empty = LatencyHist::new();
  empty.merge(&other);
  assert_eq!(empty.count, 2);
  assert_eq!(empty.max_us, 100_000);
}
