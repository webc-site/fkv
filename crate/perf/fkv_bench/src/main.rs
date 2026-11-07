//! 端到端 workload（吞吐、延迟分位、资源与 IO 计数）。
//!
//! # 时钟口径
//!
//! 本 crate 有三类时间用途，时钟源刻意分开，勿混用：
//!
//! 1. **输出时间戳**（JSON `date` / `generated_at`、临时目录名）→
//!    [`coarsetime::Clock`]，秒级墙钟，对齐 C++ `utils::UnixTs`
//!    （`include/utils.h` 的 `system_clock`）。见 `now_secs`。
//! 2. **吞吐窗口**（写 / 读 / scan / fullscan 整段）→
//!    `std::time::Instant`（Linux `CLOCK_MONOTONIC`，µs 级）。
//! 3. **per-request 延迟采样**（`req_start` → `record`）→ `std::time::Instant`。
//!
//! 第 2、3 类**不用** `coarsetime::Instant`：它在 Linux 走
//! `CLOCK_MONOTONIC_COARSE`（分辨率 1–4ms），µs 分位会被量化成 0 / 1000µs，
//! p99 验收直接失效；小规模跑法（如 `--rows 64`）单个 phase 可短于 1ms，
//! 粗钟会返回 0 → 吞吐除零为 `inf`，且与同阶段 `max_us > 0` 自相矛盾。
//! 吞吐窗口的分辨率不得低于它所包住的延迟采样。`std::time::Instant` 在 Linux
//! 是 `CLOCK_MONOTONIC` + vdso，精度反而高于 COARSE，且每阶段只读 2 次，
//! coarsetime 的「读缓存原子值」收益在这里为零。
//!
//! 与 C++ 的对照：`simple_bench` 的吞吐窗口用 `high_resolution_clock`
//! （libstdc++ 上是 `system_clock` 别名，墙钟），每请求延迟用
//! `utils::UnixTs<microseconds>()`（同样是 `system_clock` 墙钟）。fkv 侧用单调
//! 时钟是**改进**而非偏差：墙钟会被 NTP 回拨，C++ 侧 p99 反而可能失真。跨实现
//! 对比按比值看即可，不要把墙钟跳变当成 fkv 的抖动。
//!
//! # 时间窗口径
//!
//! `--test-secs`（默认 60，对齐 C++ `simple_bench` 的 `test_secs`；0＝固定次数
//! 模式）给读 / scan 阶段一个稳态窗口：读链与 scan 链逐请求循环到窗口耗尽，
//! 分位与吞吐按窗口内全部样本计算。写阶段保持次数驱动（`--write-batchs`，
//! 缺省 0＝把 `--rows` 写一遍），不随窗口放大。固定次数模式的窗口长度 =
//! 固定次数 × 单请求延迟，冷启动等瞬态主导分位；时间窗模式把瞬态稀释掉，
//! 跨实现 p99 才可比。
//!
//! # 负载形状（对齐 C++ simple_bench）
//!
//! - 写：每个分区一条独立流水（`partitions` 个 put 并发在飞，**无轮间
//!   barrier**，对齐 C++ `WriteLoop` 的完成即补发）。`--batch-size` 是
//!   **单分区单请求**条数（C++ `batch_size`），不再是全轮合计；`--write-batchs`
//!   是**全系统**总批数（C++ 同名 flag），0＝按 `--rows` 写成一遍（历史行为）。
//!   写入量超过分区键数时按 C++ 回绕覆盖，配合 `--del-ratio` / `--key-interval`
//!   复刻覆盖写 + tombstone churn。
//! - 读：`--read-per-part × --partitions` 条链并发、随机 key（对齐 C++
//!   `ReadLoop`）。
//! - scan：同为 `--read-per-part × --partitions` 条并发链，每个请求一页
//!   （256 条，对齐 C++ `Scanner::page_size`）、随机起点；`--scan-bytes` 限定
//!   窗口（0＝扫到表尾，对齐 C++ `scan_bytes`）。主指标 `scan_*` 与 C++ 的
//!   页请求是同一物理量；顺序全表遍历保留为附加指标 `fullscan_*`。
//! - 分区是**全局键空间的分片**（`index % partitions`），C++ 是每分区一份
//!   `[0, max_key)` 全量副本；跨实现严格对等需 `--partitions 1`，详见
//!   `doc/metrics.md` §6。

use std::{env, error::Error, fs, path::Path};

use rusty_alloc_default as _;

use crate::{
  cli::Cli,
  read::READ_SEED,
  report::{PerfData, Run, aggregate_repeat, now_secs, peak_rss_kb},
  runner::{RunControl, RunMode, bench_workload_ext},
  workload::{BenchArgs, ReadPlan, WriteMix},
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

#[cfg(test)]
mod tests;

fn main() -> Result<(), Box<dyn Error>> {
  let Some(opt) = cli::parse()? else {
    return Ok(());
  };
  run(opt)
}

/// 组装报告并写出：端到端 workload + 峰值 RSS。
fn run(opt: Cli) -> Result<(), Box<dyn Error>> {
  let now = now_secs();
  let mut data = opt
    .history
    .as_deref()
    .and_then(|path| fs::read(path).ok())
    .and_then(|bytes| sonic_rs::from_slice::<PerfData>(&bytes).ok())
    .unwrap_or_default();

  data.baseline = opt.baseline;
  data.generated_at = now;
  data.workload = opt.workload.as_str().to_owned();
  data.kv_size = opt.kv_size;
  data.batch_size = opt.batch_size;
  data.max_key = opt.max_key;
  data.read_per_part = opt.read_per_part;
  data.partitions = opt.partitions;
  data.read_seed = READ_SEED;
  data.key_interval = opt.key_interval;
  data.del_ratio = opt.del_ratio;
  data.test_secs = opt.test_secs;
  data.write_batchs = opt.write_batchs;
  data.scan_bytes = opt.scan_bytes;
  data.rng = opt.rng.as_str().to_owned();
  let platform = format!("{}-{}", env::consts::OS, env::consts::ARCH);

  let mode = if opt.load_only {
    RunMode::LoadOnly
  } else if opt.read_only_existing {
    RunMode::ReadOnlyExisting
  } else {
    RunMode::Normal
  };
  let bench_args = BenchArgs {
    rows: opt.rows,
    value_size: opt.value_size,
    batch_size: opt.batch_size,
    workload: opt.workload,
    mix: WriteMix {
      key_interval: opt.key_interval,
      del_ratio: opt.del_ratio,
      ttl_ratio: opt.ttl_ratio,
      ttl_expire_ts: opt.ttl_expire_ts,
      rng: opt.rng,
    },
    plan: ReadPlan {
      partitions: opt.partitions,
      read_per_part: opt.read_per_part,
    },
    test_secs: opt.test_secs,
    write_batchs: opt.write_batchs,
    scan_bytes: opt.scan_bytes,
    ttl_ms: opt.ttl_ms,
  };
  let control = RunControl {
    warmup_secs: opt.warmup_secs,
    trials: opt.trials,
    mode,
    store_dir: opt.store_dir.clone(),
    resource: true,
  };
  let mut repeat_li = Vec::with_capacity(opt.repeat);
  for _ in 0..opt.repeat {
    repeat_li.push(bench_workload_ext(
      bench_args,
      opt.bench_root.as_deref(),
      control.clone(),
    ));
  }
  let mut workload_li = aggregate_repeat(repeat_li);
  let metric_value = |name: &str| {
    workload_li
      .iter()
      .find(|(metric, ..)| metric == name)
      .map(|(.., value, _)| *value)
  };
  let mut derived_li = Vec::new();
  let kv_size = opt.kv_size as f64;
  for (src, dst) in [
    ("put_kv_s", "put_mib_s"),
    ("delete_kv_s", "delete_mib_s"),
    ("get_kv_s", "get_mib_s"),
    ("scan_kv_s", "scan_mib_s"),
    ("fullscan_kv_s", "fullscan_mib_s"),
  ] {
    if let Some(value) = metric_value(src) {
      derived_li.push((
        dst.to_owned(),
        value * kv_size / (1u64 << 20) as f64,
        "MiB/s",
      ));
    }
  }
  let write_kv = if opt.write_batchs > 0 {
    opt.write_batchs.saturating_mul(opt.batch_size)
  } else {
    opt.rows
  } as f64;
  let logical_write_bytes = write_kv * kv_size;
  let logical_nominal_bytes = opt.rows as f64 * kv_size;
  if logical_write_bytes > 0.0
    && let Some(written) = metric_value("proc_io_write_bytes")
  {
    derived_li.push(("write_amp".to_owned(), written / logical_write_bytes, "x"));
  }
  if logical_nominal_bytes > 0.0
    && let Some(store_bytes) = metric_value("store_bytes")
  {
    derived_li.push((
      "space_amp_nominal".to_owned(),
      store_bytes / logical_nominal_bytes,
      "x",
    ));
  }
  let batch_num = if opt.write_batchs > 0 {
    opt.write_batchs as f64
  } else {
    (opt.rows as f64 / opt.batch_size as f64).ceil()
  };
  if batch_num > 0.0 {
    for (src, dst) in [
      ("io_write_at_num", "write_at_per_batch"),
      ("io_data_sync_num", "sync_per_batch"),
      ("io_manifest_append_num", "manifest_append_per_batch"),
      ("io_manifest_sync_num", "manifest_sync_per_batch"),
    ] {
      if let Some(value) = metric_value(src) {
        derived_li.push((dst.to_owned(), value / batch_num, "num/batch"));
      }
    }
  }
  workload_li.extend(derived_li);
  for (metric, value, unit) in workload_li {
    println!("{metric}: {value:.2} {unit}");
    data.run_li.push(Run {
      commit: opt.commit.clone(),
      date: now,
      platform: platform.clone(),
      impl_name: opt.impl_name.clone(),
      metric,
      value,
      unit: unit.to_owned(),
    });
  }

  // `VmHWM` 是自进程启动以来的峰值，放在 workload 之后读即为本次 bench 的峰值。
  if let Some(rss_kb) = peak_rss_kb() {
    println!("rss_peak_kb: {rss_kb} kB");
    data.run_li.push(Run {
      commit: opt.commit.clone(),
      date: now,
      platform: platform.clone(),
      impl_name: opt.impl_name.clone(),
      metric: "rss_peak_kb".to_owned(),
      value: rss_kb as f64,
      unit: "kB".to_owned(),
    });
  }

  let json = sonic_rs::to_string_pretty(&data)?;
  if let Some(path) = opt.history {
    if let Some(parent) = Path::new(&path).parent() {
      fs::create_dir_all(parent)?;
    }
    fs::write(path, json + "\n")?;
  } else {
    println!("{json}");
  }
  Ok(())
}
