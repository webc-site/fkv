//! 报告模型（JSON schema）与进程 / 版本信息探测。

use std::{fs, process::Command as ProcessCommand};

use coarsetime::Clock;
use serde::{Deserialize, Serialize};

// 记录基线信息及本次 bench 的所有指标。新增字段用于对齐 C++ simple_bench 的参数口径。
#[derive(Default, Deserialize, Serialize)]
pub(crate) struct PerfData {
  pub(crate) baseline: String,
  pub(crate) generated_at: i64,
  // 新增的 CLI 参数对应的字段，供外部对比使用。
  pub(crate) workload: String,
  // `--kv-size`：整对 KV 宽度（含 8B key）。rev-01（2026-11）前的旧 history
  // 是 value 字节数（默认 64），同字段异义，跨代对比见 doc/metrics.md §6。
  pub(crate) kv_size: usize,
  pub(crate) batch_size: usize,
  pub(crate) max_key: usize,
  pub(crate) read_per_part: usize,
  pub(crate) partitions: usize,
  // 点读随机种子；旧 history 缺此字段时取默认值。
  #[serde(default)]
  pub(crate) read_seed: u64,
  // 写路径口径（稀疏 key / delete 比例），旧 history 缺此字段时取默认值。
  #[serde(default)]
  pub(crate) key_interval: usize,
  #[serde(default)]
  pub(crate) del_ratio: usize,
  // key / delete 生成口径：`cpp`（simple_bench 兼容随机游走）或 `grid`（历史网格）。
  #[serde(default)]
  pub(crate) rng: String,
  // 各计量阶段的时间窗（秒）；0 表固定次数口径，旧 history 缺此字段时取默认值。
  #[serde(default)]
  pub(crate) test_secs: f64,
  // 写阶段总批数（C++ `write_batchs` 语义，全系统口径）；0＝把 `--rows` 写一遍。
  #[serde(default)]
  pub(crate) write_batchs: usize,
  // 每个 scan 请求的字节窗口；0＝扫到表尾（C++ `scan_bytes`）。
  #[serde(default)]
  pub(crate) scan_bytes: u64,
  /// 与 `perf.json` / web 看板契约一致，序列化名是 `runs`；旧 history 的
  /// `run_li` 仍可反序列化。
  #[serde(default, rename = "runs", alias = "run_li")]
  pub(crate) run_li: Vec<Run>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Run {
  pub(crate) commit: String,
  pub(crate) date: i64,
  pub(crate) platform: String,
  #[serde(rename = "impl")]
  pub(crate) impl_name: String,
  pub(crate) metric: String,
  pub(crate) value: f64,
  pub(crate) unit: String,
}
/// 从 `/proc/self/status` 文本里取 `VmHWM`（进程峰值 RSS，kB）。
pub(crate) fn parse_vmhwm(status: &str) -> Option<u64> {
  status.lines().find_map(|line| {
    let rest = line.strip_prefix("VmHWM:")?;
    rest.split_whitespace().next()?.parse().ok()
  })
}

/// 当前进程峰值 RSS（kB）；这是「RSS ≤ 1.1×」验收的数据源。
///
/// 只有 Linux 才有 `/proc`；CI bench 跑在 Linux，其它平台返回 `None` 而不是
/// 编造数字。
pub(crate) fn peak_rss_kb() -> Option<u64> {
  parse_vmhwm(&fs::read_to_string("/proc/self/status").ok()?)
}
pub(crate) fn git_head() -> String {
  ProcessCommand::new("git")
    .args(["rev-parse", "HEAD"])
    .output()
    .ok()
    .filter(|out| out.status.success())
    .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    .unwrap_or_else(|| "unknown".to_owned())
}

/// 当前 Unix 秒（墙钟），用于 JSON `date` / `generated_at` 与临时目录名。
///
/// 走 `coarsetime`（todo.md §4.1「取时间戳用 coarsetime」），底层仍是系统墙钟，
/// 语义与 C++ `utils::UnixTs`（`system_clock`）一致；秒级精度足够，不需要 µs。
pub(crate) fn now_secs() -> i64 {
  Clock::now_since_epoch().as_secs() as i64
}

/// `--repeat` 聚合：逐指标取中位数，并追加 `<metric>_min` / `<metric>_max`。
///
/// 各次运行的指标名与顺序必须一致（同一 CLI 参数、同一 lane 集合）；不一致
/// 说明 bench 本身有随机分支，直接返回第一次的结果并保留原名，避免错位聚合。
pub(crate) fn aggregate_repeat(
  run_li: Vec<Vec<(String, f64, &'static str)>>,
) -> Vec<(String, f64, &'static str)> {
  let mut iter = run_li.into_iter();
  let Some(first) = iter.next() else {
    return Vec::new();
  };
  let mut runs = vec![first];
  for run in iter {
    if run.len() != runs[0].len() || run.iter().zip(&runs[0]).any(|(lhs, rhs)| lhs.0 != rhs.0) {
      return runs.remove(0);
    }
    runs.push(run);
  }
  if runs.len() == 1 {
    return runs.remove(0);
  }
  let mut result_li = Vec::with_capacity(runs[0].len() * 3);
  for index in 0..runs[0].len() {
    let (name, _, unit) = &runs[0][index];
    let mut value_li: Vec<f64> = runs.iter().map(|run| run[index].1).collect();
    let min = value_li.iter().copied().fold(f64::INFINITY, f64::min);
    let max = value_li.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    value_li.sort_by(f64::total_cmp);
    let median = value_li[value_li.len() / 2];
    result_li.push((name.clone(), median, *unit));
    result_li.push((format!("{name}_min"), min, *unit));
    result_li.push((format!("{name}_max"), max, *unit));
  }
  result_li
}
