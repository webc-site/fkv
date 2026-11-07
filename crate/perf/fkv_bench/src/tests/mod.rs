//! fkv_bench 内联测试：纯函数口径、指标形状与端到端 smoke。

use std::{
  env, fs,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use fastrand::Rng;
use fkv_types::now_ms;

use crate::{
  cli::{DEFAULT_KV_SIZE, resolve_kv_size, value_len},
  hist::LatencyHist,
  lane::{
    WriteLane, claim_batch, delete_batch_num, key_bytes, once_batch_num, partition_key_num,
    probe_key_bytes, write_entry, write_lane_li,
  },
  read::{lane_seed, read_lane_li},
  report::{PerfData, aggregate_repeat, now_secs, parse_vmhwm},
  resource::{dir_stats, parse_proc_io},
  rng::{CppKeyGen, Mt19937},
  runner::{RunControl, RunMode, bench_workload, bench_workload_ext, window_duration},
  scan::{decode_key, scan_kvs, scan_lane_li, scan_range},
  workload::{
    BenchArgs, ReadPlan, RngMode, Workload, WriteMix, delete_target, effective_rows, is_deleted,
    is_live, live_after, live_key_num, ttl_live_key_num,
  },
};
mod cli;
mod compact;
mod hist;
mod lane;
mod read;
mod report;
mod resource;
mod rng;
mod runner;
mod scan;
mod ttl;
mod workload;

/// 每个阶段的延迟指标后缀，对齐 C++ `LatencyRecorder` 输出。
const LAT_SUFFIX_LI: [&str; 8] = [
  "avg_us", "p50_us", "p90_us", "p95_us", "p99_us", "p999_us", "p9999_us", "max_us",
];

/// 每秒窗口指标后缀（无 `_us` 的吞吐项单独计）。
const WINDOW_SUFFIX_LI: [&str; 6] = [
  "window_num",
  "window_qps",
  "window_kv_s",
  "window_p50_us",
  "window_p90_us",
  "window_p99_us",
];

fn metric_name_li<'a>(metric_li: &'a [(String, f64, &'static str)]) -> Vec<&'a str> {
  metric_li.iter().map(|(name, ..)| name.as_str()).collect()
}

/// 断言某阶段输出了全部延迟分位指标，且值为正、单位为 `us`。
fn assert_latency_metric_li(metric_li: &[(String, f64, &'static str)], prefix: &str) {
  for suffix in LAT_SUFFIX_LI {
    let name = format!("{prefix}_{suffix}");
    let Some(item) = metric_li.iter().find(|item| item.0 == name) else {
      panic!("缺少延迟指标 {name}");
    };
    assert!(item.1 > 0.0, "{name} = {}", item.1);
    assert_eq!(item.2, "us");
  }
}

/// 吞吐指标（以 `_s` 结尾），延迟指标单位是 `_us`，不会混入。
fn throughput_name_li<'a>(metric_li: &'a [(String, f64, &'static str)]) -> Vec<&'a str> {
  metric_name_li(metric_li)
    .into_iter()
    .filter(|name| name.ends_with("_s"))
    .collect()
}
