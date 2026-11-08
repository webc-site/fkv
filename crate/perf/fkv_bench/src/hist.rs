//! 每请求延迟直方图与分位指标输出。

use std::time::{Duration, Instant};

/// 延迟直方图线性区容量（微秒）：`[0, LAT_LINEAR_US)` 每微秒一个桶。
const LAT_LINEAR_US: usize = 1 << 16;
/// 线性区的位宽，用于计算溢出桶下标。
const LAT_LINEAR_BITS: u32 = LAT_LINEAR_US.trailing_zeros();
/// 线性区之上的 2 的幂溢出桶数，覆盖到约 2.8e14µs。
const LAT_LOG_NUM: usize = 32;
/// 输出的延迟分位：`(万分位刻度, 指标名后缀)`。刻度统一为「每 10000 份」，
/// 修正 perf-21：此前的 `(999, "p999")` 表按百分位（/100）参与排名，
/// `count × 999 / 100 ≥ count` 恒被钳到最后一个样本，p999/p9999 输出的
/// 一直是 max 而非真实千分位。
const LAT_PERCENT_LI: [(u32, &str); 6] = [
  (5_000, "p50"),
  (9_000, "p90"),
  (9_500, "p95"),
  (9_900, "p99"),
  (9_990, "p999"),
  (9_999, "p9999"),
];
/// 每请求延迟直方图。
///
/// 线性区 `[0, 65536)` 微秒每 µs 一桶，其上按 2 的幂分桶。`record` 为 O(1)、
/// 无每请求分配；分位只在阶段结束时遍历一次，线性区误差 ≤ 1µs。
pub(crate) struct LatencyHist {
  linear: Box<[u32]>,
  overflow: [u32; LAT_LOG_NUM],
  pub(crate) count: u64,
  pub(crate) sum_us: u64,
  pub(crate) max_us: u64,
}

impl LatencyHist {
  pub(crate) fn new() -> Self {
    Self {
      linear: vec![0u32; LAT_LINEAR_US].into_boxed_slice(),
      overflow: [0; LAT_LOG_NUM],
      count: 0,
      sum_us: 0,
      max_us: 0,
    }
  }

  /// 记录一次请求耗时；亚微秒按 1µs 计，避免分位出现 0。
  pub(crate) fn record(&mut self, elapsed: Duration) {
    let us = elapsed.as_micros().max(1) as u64;
    if let Some(count) = self.linear.get_mut(us as usize) {
      *count += 1;
    } else {
      // 溢出区：桶 `i` 覆盖 [`LAT_LINEAR_US << i`, `LAT_LINEAR_US << (i + 1)`)。
      let index = (us.ilog2() - LAT_LINEAR_BITS) as usize;
      self.overflow[index.min(LAT_LOG_NUM - 1)] += 1;
    }
    self.count += 1;
    self.sum_us += us;
    self.max_us = self.max_us.max(us);
  }

  pub(crate) fn avg_us(&self) -> f64 {
    if self.count == 0 {
      return 0.0;
    }
    self.sum_us as f64 / self.count as f64
  }

  /// 归并另一份直方图：并发读链各自记录自己的直方图，阶段结束时合并为一份。
  ///
  /// 桶值逐项相加，`count` / `sum_us` 累加，`max_us` 取大；归并后各分位与
  /// 单份记录等价（桶边界相同）。
  pub(crate) fn merge(&mut self, other: &Self) {
    for (dst, src) in self.linear.iter_mut().zip(other.linear.iter()) {
      *dst += *src;
    }
    for (dst, src) in self.overflow.iter_mut().zip(other.overflow.iter()) {
      *dst += *src;
    }
    self.count += other.count;
    self.sum_us += other.sum_us;
    self.max_us = self.max_us.max(other.max_us);
  }

  /// 分位值（微秒）：`per_10000` 为万分位刻度（如 9_990 = p999）。线性区
  /// 返回桶的 µs 值（误差 <1µs），溢出区返回桶上界，所有结果钳制到实测
  /// `max_us`，避免分位大于 max 的怪异输出。
  pub(crate) fn percentile_us(&self, per_10000: u32) -> u64 {
    if self.count == 0 {
      return 0;
    }
    // 目标排名（1 基，向上取整）；钳制到样本数，避免 p9999 在小样本下溢出排名。
    let rank = (self.count as u128 * u128::from(per_10000))
      .div_ceil(10_000)
      .min(u128::from(self.count));
    let mut cumulative = 0u64;
    for (index, count) in self.linear.iter().enumerate() {
      cumulative += u64::from(*count);
      if u128::from(cumulative) >= rank {
        return index as u64;
      }
    }
    for (index, count) in self.overflow.iter().enumerate() {
      cumulative += u64::from(*count);
      if u128::from(cumulative) >= rank {
        return ((LAT_LINEAR_US as u64) << (index + 1)).min(self.max_us);
      }
    }
    self.max_us
  }
}

/// 追加一个阶段的延迟分位指标，命名对齐 C++ `LatencyRecorder` 的
/// average / p50 / p90 / p95 / p99 / p999 / p9999 / max。
pub(crate) fn push_latency(
  result_li: &mut Vec<(String, f64, &'static str)>,
  prefix: &str,
  hist: &LatencyHist,
) {
  result_li.push((format!("{prefix}_avg_us"), hist.avg_us(), "us"));
  for (per_10000, name) in LAT_PERCENT_LI {
    let value = hist.percentile_us(per_10000) as f64;
    result_li.push((format!("{prefix}_{name}_us"), value, "us"));
  }
  result_li.push((format!("{prefix}_max_us"), hist.max_us as f64, "us"));
}

/// 秒级窗口直方图的线性区（微秒）：`[0, 16384)` 每 µs 一桶。
///
/// 比累计直方图小 4 倍，单窗口 64KB；逐秒窗口 × 长窗口仍是可控内存。
/// 累计分位仍走 [`LatencyHist`]（256KB、误差 ≤1µs），窗口分位只用于对齐
/// C++ 的「每秒一段」粒度，`>16ms` 的尾部按 2 的幂分桶、误差不超过 2 倍。
const WIN_LINEAR_US: usize = 1 << 14;
/// 窗口直方图线性区的位宽。
const WIN_LINEAR_BITS: u32 = 14;
/// 窗口直方图的幂次溢出桶数，覆盖到约 2.7e13µs。
const WIN_LOG_NUM: usize = 32;

/// 单个秒级窗口的延迟直方图（比 [`LatencyHist`] 小，逐秒各一份）。
#[derive(Clone)]
pub(crate) struct WindowHist {
  linear: Box<[u32]>,
  overflow: [u32; WIN_LOG_NUM],
  count: u64,
  max_us: u64,
}

impl WindowHist {
  fn new() -> Self {
    Self {
      linear: vec![0u32; WIN_LINEAR_US].into_boxed_slice(),
      overflow: [0; WIN_LOG_NUM],
      count: 0,
      max_us: 0,
    }
  }

  fn record(&mut self, elapsed: Duration) {
    let us = elapsed.as_micros().max(1) as u64;
    if let Some(count) = self.linear.get_mut(us as usize) {
      *count += 1;
    } else {
      let index = (us.ilog2() - WIN_LINEAR_BITS) as usize;
      self.overflow[index.min(WIN_LOG_NUM - 1)] += 1;
    }
    self.count += 1;
    self.max_us = self.max_us.max(us);
  }

  fn merge(&mut self, other: &Self) {
    for (dst, src) in self.linear.iter_mut().zip(other.linear.iter()) {
      *dst += *src;
    }
    for (dst, src) in self.overflow.iter_mut().zip(other.overflow.iter()) {
      *dst += *src;
    }
    self.count += other.count;
    self.max_us = self.max_us.max(other.max_us);
  }

  /// `per_10000` 为万分位刻度（与 [`LatencyHist::percentile_us`] 同口径）。
  fn percentile_us(&self, per_10000: u32) -> u64 {
    if self.count == 0 {
      return 0;
    }
    let rank = (self.count as u128 * u128::from(per_10000))
      .div_ceil(10_000)
      .min(u128::from(self.count));
    let mut cumulative = 0u64;
    for (index, count) in self.linear.iter().enumerate() {
      cumulative += u64::from(*count);
      if u128::from(cumulative) >= rank {
        return index as u64;
      }
    }
    for (index, count) in self.overflow.iter().enumerate() {
      cumulative += u64::from(*count);
      if u128::from(cumulative) >= rank {
        return ((WIN_LINEAR_US as u64) << (index + 1)).min(self.max_us);
      }
    }
    self.max_us
  }
}

/// 逐秒窗口记录器：把请求按「距本链启动的整秒」分桶。
///
/// 对齐 C++ `simple_bench` 的 1s 窗口粒度：每个窗口独立统计请求数 / kv 数 /
/// 延迟分位。各链在 `join_all` 中几乎同时启动，桶下标可直接按秒对齐后合并。
pub(crate) struct WindowRecorder {
  started: Instant,
  bucket_li: Vec<(u64, WindowHist)>,
}

impl WindowRecorder {
  pub(crate) fn new() -> Self {
    Self {
      started: Instant::now(),
      bucket_li: Vec::new(),
    }
  }

  /// 记录一个已完成请求：`kv` 为该请求条数（点读恒 1，写为 batch 条数）。
  pub(crate) fn record(&mut self, elapsed: Duration, kv: u64) {
    let index = self.started.elapsed().as_secs() as usize;
    if self.bucket_li.len() <= index {
      self
        .bucket_li
        .resize_with(index + 1, || (0, WindowHist::new()));
    }
    let (kv_num, hist) = &mut self.bucket_li[index];
    *kv_num += kv;
    hist.record(elapsed);
  }

  /// 归并另一条链的窗口记录；窗口下标按启动时刻对齐（各链同批启动）。
  pub(crate) fn merge(&mut self, other: &Self) {
    if self.bucket_li.len() < other.bucket_li.len() {
      self
        .bucket_li
        .resize_with(other.bucket_li.len(), || (0, WindowHist::new()));
    }
    for (index, (kv, hist)) in other.bucket_li.iter().enumerate() {
      let (dst_kv, dst_hist) = &mut self.bucket_li[index];
      *dst_kv += kv;
      dst_hist.merge(hist);
    }
  }

  /// 是否有任何窗口采集到样本。
  pub(crate) fn is_empty(&self) -> bool {
    self.bucket_li.iter().all(|(_, hist)| hist.count == 0)
  }
}

/// 中位数（偶数个取中间偏上的那个，样本足够时差异可忽略）。
fn median(mut value_li: Vec<f64>) -> f64 {
  value_li.sort_by(f64::total_cmp);
  value_li[value_li.len() / 2]
}

/// 追加秒级窗口指标：`{prefix}_window_num/qps/kv_s/p50_us/p90_us/p99_us`。
///
/// 各窗口先算自己的吞吐与分位，再对窗口序列取中位数——这样个别慢窗口
/// （GC / 调度抖动）不会像整段平均那样被摊平，与 C++ 逐秒输出的观察口径一致。
/// 末窗口按实际长度（`elapsed_secs - index`，上限 1s）折算，避免把半截窗口
/// 当成完整秒拉低吞吐。
pub(crate) fn push_window_metrics(
  result_li: &mut Vec<(String, f64, &'static str)>,
  prefix: &str,
  recorder: &WindowRecorder,
  elapsed_secs: f64,
) {
  if recorder.is_empty() || elapsed_secs <= 0.0 {
    return;
  }
  let mut qps_li = Vec::new();
  let mut kv_s_li = Vec::new();
  let mut p50_li = Vec::new();
  let mut p90_li = Vec::new();
  let mut p99_li = Vec::new();
  for (index, (kv, hist)) in recorder.bucket_li.iter().enumerate() {
    if hist.count == 0 {
      continue;
    }
    let secs = (elapsed_secs - index as f64).clamp(f64::MIN_POSITIVE, 1.0);
    qps_li.push(hist.count as f64 / secs);
    kv_s_li.push(*kv as f64 / secs);
    p50_li.push(hist.percentile_us(5_000) as f64);
    p90_li.push(hist.percentile_us(9_000) as f64);
    p99_li.push(hist.percentile_us(9_900) as f64);
  }
  if qps_li.is_empty() {
    return;
  }
  result_li.push((format!("{prefix}_window_num"), qps_li.len() as f64, "num"));
  result_li.push((format!("{prefix}_window_qps"), median(qps_li), "req/s"));
  result_li.push((format!("{prefix}_window_kv_s"), median(kv_s_li), "kv/s"));
  result_li.push((format!("{prefix}_window_p50_us"), median(p50_li), "us"));
  result_li.push((format!("{prefix}_window_p90_us"), median(p90_li), "us"));
  result_li.push((format!("{prefix}_window_p99_us"), median(p99_li), "us"));
}

#[cfg(test)]
mod tests {
  use super::*;

  /// perf-21 锁定：千分位不得退化为 max。100 万个小样本 + 1 个大样本下，
  /// p999/p9999 必须仍落在线性区小值桶；修复前的排名公式（千分位刻度按
  /// /100 参与）把 rank 恒钳到最后一个样本，两个分位都会返回 max。
  #[test]
  fn percentile_permille_scale_is_not_clamped_to_max() {
    let mut hist = LatencyHist::new();
    let tiny = Duration::from_micros(3);
    for _ in 0..1_000_000 {
      hist.record(tiny);
    }
    hist.record(Duration::from_micros(10_000_000));
    assert_eq!(hist.percentile_us(9_900), 3, "p99 应在小值桶");
    assert_eq!(hist.percentile_us(9_990), 3, "p999 不得退化为 max");
    assert_eq!(hist.percentile_us(9_999), 3, "p9999 不得退化为 max");
    assert_eq!(hist.percentile_us(5_000), 3);
    assert_eq!(hist.max_us, 10_000_000);
  }
}
