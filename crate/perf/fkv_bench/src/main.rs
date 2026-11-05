//! 端到端 workload + gxhash 微基准。
//!
//! # 时钟口径
//!
//! 本 crate 有三类时间用途，时钟源刻意分开，勿混用：
//!
//! 1. **输出时间戳**（JSON `date` / `generated_at`、临时目录名）→
//!    [`coarsetime::Clock`]，秒级墙钟，对齐 C++ `utils::UnixTs`
//!    （`include/utils.h` 的 `system_clock`）。见 [`now_secs`]。
//! 2. **吞吐窗口**（写 / 读 / scan / fullscan 整段、`bench_gxhash` 的 300ms 窗口）→
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

use std::{
  env,
  error::Error,
  fs,
  hint::black_box,
  mem::size_of,
  path::{Path, PathBuf},
  process::{Command as ProcessCommand, id},
  sync::atomic::{AtomicU64, Ordering},
  time::{Duration, Instant},
};

use clap_args::{ArgAction, arg, clap::Command};
use coarsetime::Clock;
use compio::runtime::Runtime;
use fastrand::Rng;
use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_store::{PageOption, ScanOption, Store};
use fkv_types::TableIdent;
use futures::future::join_all;
use rusty_alloc_default as _;
use serde::{Deserialize, Serialize};

/// key 编码宽度：8 字节大端 uint64，对齐 C++ `EncodeFixed64(ToBigEndian(key))`。
const KEY_BYTES: usize = size_of::<u64>();
/// 点读随机序列的固定种子，保证 bench 可复现。
const READ_SEED: u64 = 0x5eed_0071_c01d_0001;
/// scan 随机区间（起点 / 窗口）的固定种子，与点读种子分开。
const SCAN_SEED: u64 = 0x5eed_0073_ca11_0001;
/// 延迟直方图线性区容量（微秒）：`[0, LAT_LINEAR_US)` 每微秒一个桶。
const LAT_LINEAR_US: usize = 1 << 16;
/// 线性区的位宽，用于计算溢出桶下标。
const LAT_LINEAR_BITS: u32 = LAT_LINEAR_US.trailing_zeros();
/// 线性区之上的 2 的幂溢出桶数，覆盖到约 2.8e14µs。
const LAT_LOG_NUM: usize = 32;
/// 输出的延迟分位：`(percent, 指标名后缀)`。
const LAT_PERCENT_LI: [(u32, &str); 5] = [
  (50, "p50"),
  (90, "p90"),
  (99, "p99"),
  (999, "p999"),
  (9999, "p9999"),
];
/// 各计量阶段的默认时间窗（秒）：对齐 C++ `simple_bench.cpp:31`
/// `DEFINE_uint32(test_secs, 60, "read/scan test time")`；`0`＝固定次数模式
/// （历史行为），供既有调用回退。
const DEFAULT_TEST_SECS: f64 = 60.0;
/// `--kv-size` 缺省：整对 KV 宽度（含 8B key），对齐 C++ `simple_bench.cpp:24`
/// `DEFINE_uint32(kv_size, 128, "size of a pair of KV")` 的默认值
/// （rev-01 落地前的默认 64 是 value 字节口径，已断代）。
const DEFAULT_KV_SIZE: usize = 128;
/// 每个 scan 请求的页大小：对齐 C++ `Scanner::page_size = 256`
/// （`SetPagination(256, 0)`），即一次请求一页。
const SCAN_PAGE_ENTRIES: usize = 256;
/// `--scan-bytes` 缺省：0＝扫到表尾，对齐 C++ `DEFINE_uint64(scan_bytes, 0, ...)`。
const DEFAULT_SCAN_BYTES: u64 = 0;
/// `--write-batchs` 缺省：0＝把 `--rows` 写一遍（历史行为），对齐 C++ 前不改写入量。
const DEFAULT_WRITE_BATCHS: usize = 0;

/// 每请求延迟直方图。
///
/// 线性区 `[0, 65536)` 微秒每 µs 一桶，其上按 2 的幂分桶。`record` 为 O(1)、
/// 无每请求分配；分位只在阶段结束时遍历一次，线性区误差 ≤ 1µs。
struct LatencyHist {
  linear: Box<[u32]>,
  overflow: [u32; LAT_LOG_NUM],
  count: u64,
  sum_us: u64,
  max_us: u64,
}

impl LatencyHist {
  fn new() -> Self {
    Self {
      linear: vec![0u32; LAT_LINEAR_US].into_boxed_slice(),
      overflow: [0; LAT_LOG_NUM],
      count: 0,
      sum_us: 0,
      max_us: 0,
    }
  }

  /// 记录一次请求耗时；亚微秒按 1µs 计，避免分位出现 0。
  fn record(&mut self, elapsed: Duration) {
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

  fn avg_us(&self) -> f64 {
    if self.count == 0 {
      return 0.0;
    }
    self.sum_us as f64 / self.count as f64
  }

  /// 归并另一份直方图：并发读链各自记录自己的直方图，阶段结束时合并为一份。
  ///
  /// 桶值逐项相加，`count` / `sum_us` 累加，`max_us` 取大；归并后各分位与
  /// 单份记录等价（桶边界相同）。
  fn merge(&mut self, other: &Self) {
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

  /// 分位值（微秒）：线性区返回桶的 µs 值（误差 <1µs），溢出区返回桶上界，
  /// 所有结果钳制到实测 `max_us`，避免分位大于 max 的怪异输出。
  fn percentile_us(&self, percent: u32) -> u64 {
    if self.count == 0 {
      return 0;
    }
    // 目标排名（1 基，向上取整）；钳制到样本数，避免 p9999 在小样本下溢出排名。
    let rank = (self.count as u128 * percent as u128)
      .div_ceil(100)
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
/// average / p50 / p90 / p99 / p999 / p9999 / max。
fn push_latency(
  result_li: &mut Vec<(String, f64, &'static str)>,
  prefix: &str,
  hist: &LatencyHist,
) {
  result_li.push((format!("{prefix}_avg_us"), hist.avg_us(), "us"));
  for (percent, name) in LAT_PERCENT_LI {
    let value = hist.percentile_us(percent) as f64;
    result_li.push((format!("{prefix}_{name}_us"), value, "us"));
  }
  result_li.push((format!("{prefix}_max_us"), hist.max_us as f64, "us"));
}

// 记录基线信息及本次 bench 的所有指标。新增字段用于对齐 C++ simple_bench 的参数口径。
#[derive(Default, Deserialize, Serialize)]
struct PerfData {
  baseline: String,
  generated_at: i64,
  // 新增的 CLI 参数对应的字段，供外部对比使用。
  workload: String,
  // `--kv-size`：整对 KV 宽度（含 8B key）。rev-01（2026-11）前的旧 history
  // 是 value 字节数（默认 64），同字段异义，跨代对比见 doc/metrics.md §6。
  kv_size: usize,
  batch_size: usize,
  max_key: usize,
  read_per_part: usize,
  partitions: usize,
  // 点读随机种子；旧 history 缺此字段时取默认值。
  #[serde(default)]
  read_seed: u64,
  // 写路径口径（稀疏 key / delete 比例），旧 history 缺此字段时取默认值。
  #[serde(default)]
  key_interval: usize,
  #[serde(default)]
  del_ratio: usize,
  // 各计量阶段的时间窗（秒）；0 表固定次数口径，旧 history 缺此字段时取默认值。
  #[serde(default)]
  test_secs: f64,
  // 写阶段总批数（C++ `write_batchs` 语义，全系统口径）；0＝把 `--rows` 写一遍。
  #[serde(default)]
  write_batchs: usize,
  // 每个 scan 请求的字节窗口；0＝扫到表尾（C++ `scan_bytes`）。
  #[serde(default)]
  scan_bytes: u64,
  /// 与 `perf.json` / web 看板契约一致，序列化名是 `runs`；旧 history 的
  /// `run_li` 仍可反序列化。
  #[serde(default, rename = "runs", alias = "run_li")]
  run_li: Vec<Run>,
}

#[derive(Deserialize, Serialize)]
struct Run {
  commit: String,
  date: i64,
  platform: String,
  #[serde(rename = "impl")]
  impl_name: String,
  metric: String,
  value: f64,
  unit: String,
}

/// 从 `/proc/self/status` 文本里取 `VmHWM`（进程峰值 RSS，kB）。
fn parse_vmhwm(status: &str) -> Option<u64> {
  status.lines().find_map(|line| {
    let rest = line.strip_prefix("VmHWM:")?;
    rest.split_whitespace().next()?.parse().ok()
  })
}

/// 当前进程峰值 RSS（kB）；这是「RSS ≤ 1.1×」验收的数据源。
///
/// 只有 Linux 才有 `/proc`；CI bench 跑在 Linux，其它平台返回 `None` 而不是
/// 编造数字。
fn peak_rss_kb() -> Option<u64> {
  parse_vmhwm(&fs::read_to_string("/proc/self/status").ok()?)
}

fn main() -> Result<(), Box<dyn Error>> {
  let Some(arg_match) = clap_args::parse!(|cmd: Command| {
    cmd
      .arg(arg!(--history <FILE> "perf.json 历史文件").action(ArgAction::Set))
      .arg(arg!(--commit <COMMIT> "提交号").action(ArgAction::Set))
      .arg(arg!(--baseline <BASELINE> "兼容基线").action(ArgAction::Set))
      .arg(arg!(--"impl" <IMPL> "实现名：rust / cpp").action(ArgAction::Set))
      // 兼容现有参数
      .arg(arg!(--rows <ROWS> "端到端 benchmark key 数，默认等于 --max-key").action(ArgAction::Set))
      .arg(
        arg!(--"batch-size" <N> "单分区单请求条数（对齐 C++ simple_bench 的 batch_size），默认 100")
          .action(ArgAction::Set),
      )
      .arg(arg!(--root <DIR> "benchmark 数据目录（例如 NVMe 挂载点）").action(ArgAction::Set))
      // 新增参数，保持默认行为与原实现一致
      .arg(
        arg!(--workload <MODE> "write-read|write-only|read-only|write-scan|scan，默认 write-read")
          .action(ArgAction::Set),
      )
      .arg(
        // 帮助文本的默认值与校验下界由 `DEFAULT_KV_SIZE` / `KEY_BYTES` 生成，
        // 单一事实源，防止文案与代码再漂移（rev-01 的教训）。
        arg!(--"kv-size" <BYTES>)
          .help(format!(
            "整对 KV 宽度（含 {KEY_BYTES}B key），value 字节 = kv_size - {KEY_BYTES}；默认 {DEFAULT_KV_SIZE}，对齐 C++ simple_bench"
          ))
          .action(ArgAction::Set),
      )
      .arg(arg!(--"max-key" <N> "最大 key 数，默认 10000000").action(ArgAction::Set))
      .arg(arg!(--"read-per-part" <N> "每分区读取次数，默认 4").action(ArgAction::Set))
      .arg(
        arg!(--partitions <N> "分区数，默认 1；分区是全局键空间的分片（C++ simple_bench 为每分区一份全量副本，严格对等需 1）")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"key-interval" <N> "key 稀疏间隔；C++ simple_bench 用 4，默认 1")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"del-ratio" <N> "每 N 个写把最后一个改成 delete；C++ simple_bench 用 4，0 表示不删，默认 0")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"write-batchs" <N> "写阶段全系统总批数；每批 = 单分区 batch_size 条，超过分区键数回绕覆盖（对齐 C++ write_batchs）；0＝把 --rows 写一遍，默认 0")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"scan-bytes" <BYTES> "每个 scan 请求的字节窗口；0＝扫到表尾（对齐 C++ scan_bytes），默认 0")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"test-secs" <SECS> "读/scan 阶段时间窗（秒），窗口内循环执行、分位按全部样本算；写阶段按 --write-batchs/--rows 次数驱动；默认 60 对齐 C++ simple_bench，0＝固定次数模式")
          .action(ArgAction::Set),
      )
  }) else {
    return Ok(());
  };

  let commit = arg_match
    .get_one::<String>("commit")
    .cloned()
    .or_else(|| env::var("GITHUB_SHA").ok())
    .unwrap_or_else(git_head);
  let baseline = arg_match
    .get_one::<String>("baseline")
    .cloned()
    .unwrap_or_default();
  let history = arg_match.get_one::<String>("history").cloned();
  let impl_name = arg_match
    .get_one::<String>("impl")
    .cloned()
    .unwrap_or_else(|| "rust".to_owned());
  let max_key = arg_match
    .get_one::<String>("max-key")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(10_000_000)
    .max(1);
  let rows = arg_match
    .get_one::<String>("rows")
    .and_then(|value| value.parse::<usize>().ok())
    // 默认与 C++ `max_key` 同量级，否则数据规模与冷读比例不可比。
    .unwrap_or(max_key)
    .max(1);
  let batch_size = arg_match
    .get_one::<String>("batch-size")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(100)
    .max(1);
  // `--kv-size` 是整对 KV 宽度（含 8B key），`<= 8` 报参数错误；value 取差值，
  // 对齐 C++ `simple_bench.cpp:113` `value.resize(FLAGS_kv_size - sizeof(uint64_t))`。
  let kv_size = resolve_kv_size(arg_match.get_one::<String>("kv-size").map(String::as_str))?;
  let value_size = value_len(kv_size);
  let read_per_part = arg_match
    .get_one::<String>("read-per-part")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(4)
    .max(1);
  let partitions = arg_match
    .get_one::<String>("partitions")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(1)
    .max(1);
  // 写路径口径：默认「间隔 1、不删」＝历史行为；本仓使用固定步长。
  let key_interval = arg_match
    .get_one::<String>("key-interval")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(1)
    .max(1);
  let del_ratio = arg_match
    .get_one::<String>("del-ratio")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(0);
  // 写阶段总批数：0＝按 `--rows` 写一遍（历史行为），>0 按 C++ `write_batchs`
  // 的全系统批数驱动（每批 `batch_size` 条，越界回绕）。
  let write_batchs = arg_match
    .get_one::<String>("write-batchs")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(DEFAULT_WRITE_BATCHS);
  // scan 请求的字节窗口：0＝扫到表尾（C++ 同口径）。
  let scan_bytes = arg_match
    .get_one::<String>("scan-bytes")
    .and_then(|value| value.parse::<u64>().ok())
    .unwrap_or(DEFAULT_SCAN_BYTES);
  // 时间窗（秒）：默认对齐 C++ `simple_bench` 的 `test_secs`（60）；
  // 0＝固定次数模式（历史行为），负值按 0 处理。
  let test_secs = arg_match
    .get_one::<String>("test-secs")
    .and_then(|value| value.parse::<f64>().ok())
    .unwrap_or(DEFAULT_TEST_SECS)
    .max(0.0);
  let workload = arg_match
    .get_one::<String>("workload")
    .map(|value| Workload::parse(value))
    .unwrap_or(Workload::WriteRead);
  // 实际 key 数受 max_key 约束。
  let rows = effective_rows(rows, max_key);
  let bench_root = arg_match.get_one::<String>("root").map(PathBuf::from);
  let now = now_secs();
  let mut data = history
    .as_deref()
    .and_then(|path| fs::read(path).ok())
    .and_then(|bytes| sonic_rs::from_slice::<PerfData>(&bytes).ok())
    .unwrap_or_default();

  data.baseline = baseline;
  data.generated_at = now;
  data.workload = workload.as_str().to_owned();
  data.kv_size = kv_size;
  data.batch_size = batch_size;
  data.max_key = max_key;
  data.read_per_part = read_per_part;
  data.partitions = partitions;
  data.read_seed = READ_SEED;
  data.key_interval = key_interval;
  data.del_ratio = del_ratio;
  data.test_secs = test_secs;
  data.write_batchs = write_batchs;
  data.scan_bytes = scan_bytes;

  let platform = format!("{}-{}", env::consts::OS, env::consts::ARCH);
  data.run_li.push(Run {
    commit: commit.clone(),
    date: now,
    platform: platform.clone(),
    impl_name: impl_name.clone(),
    metric: "gxhash_4096B_gib_s".to_owned(),
    value: bench_gxhash(4096),
    unit: "GiB/s".to_owned(),
  });

  let workload_li = bench_workload(
    BenchArgs {
      rows,
      value_size,
      batch_size,
      workload,
      mix: WriteMix {
        key_interval,
        del_ratio,
      },
      plan: ReadPlan {
        partitions,
        read_per_part,
      },
      test_secs,
      write_batchs,
      scan_bytes,
    },
    bench_root.as_deref(),
  );
  for (metric, value, unit) in workload_li {
    println!("{metric}: {value:.2} {unit}");
    data.run_li.push(Run {
      commit: commit.clone(),
      date: now,
      platform: platform.clone(),
      impl_name: impl_name.clone(),
      metric,
      value,
      unit: unit.to_owned(),
    });
  }

  // `VmHWM` 是自进程启动以来的峰值，放在 workload 之后读即为本次 bench 的峰值。
  if let Some(rss_kb) = peak_rss_kb() {
    println!("rss_peak_kb: {rss_kb} kB");
    data.run_li.push(Run {
      commit: commit.clone(),
      date: now,
      platform: platform.clone(),
      impl_name: impl_name.clone(),
      metric: "rss_peak_kb".to_owned(),
      value: rss_kb as f64,
      unit: "kB".to_owned(),
    });
  }

  let json = sonic_rs::to_string_pretty(&data)?;
  if let Some(path) = history {
    if let Some(parent) = Path::new(&path).parent() {
      fs::create_dir_all(parent)?;
    }
    fs::write(path, json + "\n")?;
  } else {
    println!("{json}");
  }
  Ok(())
}

fn bench_gxhash(size: usize) -> f64 {
  let data = vec![0u8; size];
  // 300ms 定长窗口：粗粒度时钟（1–4ms）会让窗口量化误差达 1%，用单调 µs 时钟。
  let start = Instant::now();
  let mut count = 0u64;
  let mut acc = 0u64;
  while start.elapsed() < Duration::from_millis(300) {
    acc ^= gxhash::gxhash64(black_box(&data), count as i64);
    count += 1;
  }
  black_box(acc);
  let secs = start.elapsed().as_secs_f64();
  (count as f64 * size as f64) / secs / (1u64 << 30) as f64
}

fn git_head() -> String {
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
fn now_secs() -> i64 {
  Clock::now_since_epoch().as_secs() as i64
}

/// bench workload 模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Workload {
  WriteRead,
  WriteOnly,
  ReadOnly,
  WriteScan,
  Scan,
}

impl Workload {
  fn parse(value: &str) -> Self {
    match value {
      "write-only" => Self::WriteOnly,
      "read-only" => Self::ReadOnly,
      "write-scan" => Self::WriteScan,
      "scan" => Self::Scan,
      _ => Self::WriteRead,
    }
  }

  fn as_str(self) -> &'static str {
    match self {
      Self::WriteRead => "write-read",
      Self::WriteOnly => "write-only",
      Self::ReadOnly => "read-only",
      Self::WriteScan => "write-scan",
      Self::Scan => "scan",
    }
  }

  /// 是否计量写阶段（`read-only` / `scan` 的写入只作数据准备）。
  fn measures_write(self) -> bool {
    !matches!(self, Self::ReadOnly | Self::Scan)
  }

  /// 是否计量点读。
  fn measures_read(self) -> bool {
    matches!(self, Self::WriteRead | Self::ReadOnly)
  }

  /// 是否计量 scan（对齐 C++ 的 `scan`/`write-scan` workload）。
  fn measures_scan(self) -> bool {
    matches!(
      self,
      Self::WriteRead | Self::ReadOnly | Self::WriteScan | Self::Scan
    )
  }
}

/// 写路径口径（`key_interval` / `del_ratio`）。
///
/// key 模型是**均匀网格**：第 `index` 个 key 为 `index * key_interval`。
/// 同参数下键密度 / 树形状与随机步进不同。跨实现比较只在
/// `key_interval = 1`（两侧都退化为顺序写）时严格可比，其余按趋势参考。
/// 默认间隔 1、不删，与历史数据可比。
#[derive(Debug, Clone, Copy)]
struct WriteMix {
  /// key 稀疏间隔：第 `index` 个 key 是 `index * key_interval`。
  key_interval: usize,
  /// 每 `del_ratio` 个写请求把最后一个改成 delete；`0` 表示不删。
  del_ratio: usize,
}

impl Default for WriteMix {
  fn default() -> Self {
    Self {
      key_interval: 1,
      del_ratio: 0,
    }
  }
}

/// 第 `index` 个写请求是否按 `del_ratio` 判定为 delete。
///
/// `index` 是**分区内序列位**（不是全局 key 下标）。`del_ratio < 2` 视为「不删」：
/// 这样 delete 删的是 `del_ratio - 1` 步之前已写过的 key，写路径真的走到既有项的
/// tombstone 覆盖（而不是删一个不存在的 key）。
fn is_deleted(index: usize, del_ratio: usize) -> bool {
  del_ratio >= 2 && index % del_ratio == del_ratio - 1
}

/// 第 `index` 个写入若是 delete，它删掉的**分区内序列位**。
///
/// delete 位 `index % del_ratio == del_ratio - 1` 保证 `index >= del_ratio - 1`，
/// 不会下溢；目标与本条写同属一个分区。
fn delete_target(index: usize, del_ratio: usize) -> usize {
  index - (del_ratio - 1)
}

/// 一个分区写完一遍后，第 `index` 个序列位上的 key 是否还存在。
///
/// 存活 = 该位是 upsert，且它没有被后续 delete 摘掉。delete 位自身不写 key；
/// 倍数额 `k * del_ratio` 会在 `k * del_ratio + del_ratio - 1` 被 delete 摘掉——
/// 只有该 delete 位落在本分区序列内（`index + del_ratio - 1 < count`）才真的发生，
/// 尾部的倍数额在写完一遍后仍然存活。
fn is_live(index: usize, count: usize, del_ratio: usize) -> bool {
  del_ratio < 2
    || (!is_deleted(index, del_ratio)
      && !(index.is_multiple_of(del_ratio) && index + del_ratio - 1 < count))
}

/// 一个分区写完一遍后的存活 key 数（闭式，避免对 `rows` 全量遍历）。
///
/// 存活 = 总数 - delete 位数 - 被摘掉的倍数额；delete 位有 `count / del_ratio` 个，
/// 倍数额 `k * del_ratio` 被摘掉当且仅当 `k * del_ratio + del_ratio - 1 < count`。
fn live_key_num(count: usize, del_ratio: usize) -> usize {
  if del_ratio < 2 {
    return count;
  }
  let deleted = count / del_ratio;
  let tomb = if count >= del_ratio {
    (count - del_ratio) / del_ratio + 1
  } else {
    0
  };
  count - deleted - tomb
}

/// 分区 `part` 的序列位再写 `written` 条后，第 `pos` 位的 key 是否存活。
///
/// 覆盖写时每条写都在分区自有序列上推进、越界回绕（对齐 C++ `writing_key_`
/// 到 `max_key` 归零）。最后一遍可能只写了前缀 `rem = written % count`：
///
/// - `rem == 0`：最后一遍完整，按整遍语义判定；
/// - `pos < rem`：最后一遍重写了该位，按前缀 `rem` 判定；
/// - 其余位：最后一遍没碰，仍是一整遍写完的状态。
fn live_after(written: usize, count: usize, pos: usize, del_ratio: usize) -> bool {
  if written == 0 || count == 0 {
    return false;
  }
  let rem = written % count;
  if rem > 0 && pos < rem {
    is_live(pos, rem, del_ratio)
  } else if written >= count {
    is_live(pos, count, del_ratio)
  } else {
    false
  }
}

/// 读侧分区分读参数，对齐 C++ `simple_bench` 的 `partitions` / `read_per_part`。
#[derive(Debug, Clone, Copy)]
struct ReadPlan {
  /// 分区数，每分区一棵独立表。
  partitions: usize,
  /// 每分区点读遍数。
  read_per_part: usize,
}

/// `bench_workload` 的输入参数：CLI 解析后的端到端计量口径。
#[derive(Debug, Clone, Copy)]
struct BenchArgs {
  rows: usize,
  value_size: usize,
  batch_size: usize,
  workload: Workload,
  mix: WriteMix,
  plan: ReadPlan,
  test_secs: f64,
  /// 写阶段全系统总批数；0＝按 `rows` 写一遍。
  write_batchs: usize,
  /// scan 请求的字节窗口；0＝扫到表尾。
  scan_bytes: u64,
}

/// 分区 `part` 的写状态：key 数与累计写入条数（覆盖写后按 `written % count` 回绕）。
#[derive(Debug, Clone, Copy, Default)]
struct PartState {
  /// 分区 key 数（序列位上界）。
  count: usize,
  /// 该分区累计写入条数（可大于 `count`）。
  written: usize,
}

/// 分区 `part` 写入的 key 下标：分区按 `index % partitions` 分片，序列位是分区内下标。
fn shard_index(part: usize, step: usize, pos: usize) -> usize {
  part + step * pos
}

/// 第 `pos` 个序列位的写请求：按 [`WriteMix`] 决定稀疏间隔与 upsert/delete。
///
/// delete 目标按**分区内序列位**回退 `del_ratio - 1`，保证 tombstone 落在同一分区的
/// 表上（旧实现按全局下标回退，`partitions > 1` 时会跨分区删不存在的 key）。
fn write_entry(part: usize, step: usize, pos: usize, mix: WriteMix, value: &[u8]) -> BatchEntry {
  if is_deleted(pos, mix.del_ratio) {
    let target = delete_target(pos, mix.del_ratio);
    return BatchEntry::delete(
      key_bytes(shard_index(part, step, target) * mix.key_interval).to_vec(),
      1,
    );
  }
  BatchEntry::upsert(
    key_bytes(shard_index(part, step, pos) * mix.key_interval).to_vec(),
    value.to_vec(),
    1,
  )
}

/// 实际 key 数受 `max_key` 约束。
fn effective_rows(rows: usize, max_key: usize) -> usize {
  rows.min(max_key).max(1)
}

/// key 编码：8 字节大端 uint64；无需分配，且字典序与数值序一致。
fn key_bytes(index: usize) -> [u8; KEY_BYTES] {
  (index as u64).to_be_bytes()
}

/// 分区 `part` 分到的 key 数：`[0, rows)` 中 `index % partitions == part` 的个数。
///
/// 读链按分区拆分读次数时用它做采样上界，保证各分区总读次数仍等于 `rows`。
fn partition_key_num(rows: usize, partitions: usize, part: usize) -> usize {
  let full = rows / partitions;
  let rem = rows % partitions;
  full + usize::from(part < rem)
}

/// 一条写流水：绑定一个分区，在分区自有 key 序列上推进游标。
///
/// 分区是全局键空间的分片（`index % partitions`），写游标只在分片内推进；
/// 覆盖写模式下越过分片末尾回绕到 0，对齐 C++ `writing_key_ > max_key` 归零。
#[derive(Debug, Clone, Copy)]
struct WriteLane {
  /// 分区下标（`store_li` 下标）。
  part: usize,
  /// 分区内 key 的全局步长（＝分区数）。
  step: usize,
  /// 该分区的 key 数，也是游标回绕上界。
  count: usize,
  /// 下一个要写的分区内序列位。
  pos: usize,
}

impl WriteLane {
  /// 取本分区接下来 `len` 条写请求；越过序列末尾按 C++ 回绕。
  fn next_batch(&mut self, len: usize, mix: WriteMix, value: &[u8]) -> Vec<BatchEntry> {
    let mut batch_li = Vec::with_capacity(len);
    for _ in 0..len {
      batch_li.push(write_entry(self.part, self.step, self.pos, mix, value));
      self.pos += 1;
      if self.pos == self.count {
        self.pos = 0;
      }
    }
    batch_li
  }
}

/// 写阶段驱动口径。
#[derive(Debug, Clone, Copy)]
enum WriteMode {
  /// 历史口径：每个分区把自有 `count` 个 key 顺序写一遍（每批最多 `batch_size` 条）。
  Once { batch_size: usize },
  /// 对齐 C++：全系统共 `batch_num` 个请求，每请求恰好 `batch_size` 条，越界回绕。
  Churn { batch_size: usize, batch_num: u64 },
}

/// 构造每分区的写流水，下标与 `store_li` 一一对应。
///
/// `rows < partitions` 的空分区也保留流水（`count == 0`），runner 直接跳过，
/// 便于用 `zip` 把流水与各自 store 配对。
fn write_lane_li(rows: usize, partitions: usize) -> Vec<WriteLane> {
  (0..partitions)
    .map(|part| WriteLane {
      part,
      step: partitions,
      count: partition_key_num(rows, partitions, part),
      pos: 0,
    })
    .collect()
}

/// 写完一遍时每分区的批数：`ceil(count / batch_size)`。
fn once_batch_num(count: usize, batch_size: usize) -> usize {
  count.div_ceil(batch_size)
}

/// 一条写流水的产出：直方图 + 批数 + 条数（条数即该分区的写状态，供存活断言）。
struct WriteLaneResult {
  hist: LatencyHist,
  batch_num: u64,
  kv_num: u64,
}

/// 从全系统预算里认领一个 batch；返回 `false` 表示预算已用尽。
///
/// `write_batchs` 是**全系统**总批数（C++ `WriteLoop` 的完成计数跨全部 writer），
/// 各分区流水按完成顺序动态认领，不预先均分。
fn claim_batch(budget: &AtomicU64, batch_num: u64) -> bool {
  budget.fetch_add(1, Ordering::Relaxed) < batch_num
}

/// 跑一条写流水：完成即取本分区下一批，不与其它分区做轮间同步。
///
/// `Once` 模式写满本分区全部 key；`Churn` 模式每请求 `batch_size` 条、从共享预算
/// 认领批数，写满预算即停（对齐 C++ `WriteLoop` 的 `finished` 队列收割）。
async fn run_write_lane(
  store: &mut Store<CompioStore>,
  tbl: &TableIdent,
  lane: WriteLane,
  mode: WriteMode,
  mix: WriteMix,
  value: &[u8],
  budget: &AtomicU64,
) -> WriteLaneResult {
  let mut lane = lane;
  let mut result = WriteLaneResult {
    hist: LatencyHist::new(),
    batch_num: 0,
    kv_num: 0,
  };
  if lane.count == 0 {
    return result;
  }
  let mut written = 0usize;
  loop {
    let len = match mode {
      WriteMode::Once { batch_size } => {
        let remaining = lane.count - written;
        if remaining == 0 {
          break;
        }
        remaining.min(batch_size)
      }
      WriteMode::Churn {
        batch_size,
        batch_num,
      } => {
        if !claim_batch(budget, batch_num) {
          break;
        }
        batch_size
      }
    };
    let batch = lane.next_batch(len, mix, value);
    // per-request 采样：µs 分位必须用 µs 级单调时钟，禁用 COARSE 时钟。
    let req_start = Instant::now();
    store.put(tbl, &batch).await.unwrap();
    result.hist.record(req_start.elapsed());
    result.batch_num += 1;
    result.kv_num += len as u64;
    written += len;
  }
  result
}

/// 一条并发读链：绑定一个分区、独立 RNG 与独立直方图（join 后合并）。
///
/// 链数 = `partitions * read_per_part`，对齐 C++ `simple_bench` 的并发读者数
/// `read_per_part * partitions`；每链负责把本分区的 key 随机读一遍。
#[derive(Debug, Clone, Copy)]
struct ReadLane {
  /// 分区下标（`store_li` 下标）。
  part: usize,
  /// 分区内 key 的全局步长（＝分区数）：第 `i` 个 key 在全局是 `part + step * i`。
  step: usize,
  /// 该分区的 key 数，也是本链的随机采样范围与读次数。
  count: usize,
  /// 固定 RNG 种子，按链编号派生，保证并发下仍可复现。
  seed: u64,
}

/// 读链 RNG 种子：从全局 [`READ_SEED`] 按链编号派生，链间互不相同。
fn lane_seed(lane_id: u64) -> u64 {
  READ_SEED ^ lane_id.wrapping_mul(0x9e37_79b9_7f4a_7c15)
}

/// 构造并发读链：每分区 `read_per_part` 条链；`rows < partitions` 时空分区不建链。
///
/// 总读次数 = `read_per_part * Σ count`，而 `Σ count == rows`，故与串行版
/// `rows * read_per_part` 完全一致，只改变在飞并发度。
fn read_lane_li(rows: usize, partitions: usize, read_per_part: usize) -> Vec<ReadLane> {
  let mut lane_li = Vec::with_capacity(partitions.saturating_mul(read_per_part));
  for part in 0..partitions {
    let count = partition_key_num(rows, partitions, part);
    if count == 0 {
      continue;
    }
    for lane in 0..read_per_part {
      lane_li.push(ReadLane {
        part,
        step: partitions,
        count,
        seed: lane_seed((part * read_per_part + lane) as u64),
      });
    }
  }
  lane_li
}

/// 跑一条读链：在本分区内随机取 key 读，逐请求记入本链直方图。
///
/// key 用 [`key_bytes`] 在栈上编码，零分配；读回结果只借 `&Store`，与其它读链
/// 在同一 poll 栈内重叠推进（页 IO 在 ring 内并发）。
///
/// 判停口径：`window` 为零 → 固定次数模式，读满 `lane.count` 次；否则 →
/// 时间窗模式（对齐 C++ `ReadLoop` 收割即回填、主线程 sleep `FLAGS_test_secs`
/// 后停机），持续随机读直到窗口耗尽，窗口内全部样本进直方图。
///
/// 两种模式都至少完成一次读：窗口极短时 `Instant::now` 在首轮之前就已过期，
/// 也必须留下一个样本，否则该链的直方图为空、吞吐分母为 0。
///
/// `state` 是本分区的写状态：命中/未命中按 [`live_after`] 判定，覆盖写
/// （`--write-batchs`）后也能精确断言。
async fn run_read_lane(
  store: &Store<CompioStore>,
  tbl: &TableIdent,
  lane: ReadLane,
  state: PartState,
  value_size: usize,
  mix: WriteMix,
  window: Duration,
) -> LatencyHist {
  let mut hist = LatencyHist::new();
  let mut rng = Rng::with_seed(lane.seed);
  let started = Instant::now();
  let timed = !window.is_zero();
  let mut done = 0usize;
  loop {
    let pos = rng.usize(0..lane.count);
    let index = shard_index(lane.part, lane.step, pos);
    let req_start = Instant::now();
    let got = store
      .get(tbl, &key_bytes(index * mix.key_interval))
      .await
      .unwrap();
    hist.record(req_start.elapsed());
    // 被 delete 摘掉（或从未写到）的 key 读回 None，其余必须命中且长度一致。
    assert_eq!(
      got.is_some(),
      live_after(state.written, state.count, pos, mix.del_ratio),
      "part {} pos {pos} written {} count {}",
      lane.part,
      state.written,
      state.count
    );
    if let Some(got) = got {
      assert_eq!(got.len(), value_size);
    }
    done += 1;
    if timed {
      if started.elapsed() >= window {
        break;
      }
    } else if done >= lane.count {
      break;
    }
  }
  hist
}

/// 把 CLI 的 `--test-secs`（秒）折算成各计量阶段的时间窗；`0`＝固定次数模式。
///
/// 0 / 负值 / NaN 一律按固定次数模式处理（NaN 不满足 `> 0.0`）；正值溢出按
/// [`Duration::MAX`] 饱和——请求「跑到天荒地老」不应悄悄退化成固定次数模式。
fn window_duration(test_secs: f64) -> Duration {
  if test_secs > 0.0 {
    Duration::try_from_secs_f64(test_secs).unwrap_or(Duration::MAX)
  } else {
    Duration::ZERO
  }
}

/// 一条并发 scan 链：绑定分区、独立 RNG 与独立直方图（join 后合并）。
///
/// 链数 = `partitions * read_per_part`，对齐 C++ `ScanLoop` 的
/// `num_scanners = read_per_part * partitions`；每链完成一页请求即发下一请求。
#[derive(Debug, Clone, Copy)]
struct ScanLane {
  /// 分区下标（`store_li` 下标），按 `id % partitions` 轮转，对齐 C++ `Scanner`。
  part: usize,
  /// 固定 RNG 种子，按链编号派生，保证并发下仍可复现。
  seed: u64,
}

/// scan 链 RNG 种子：从 [`SCAN_SEED`] 按链编号派生。
fn scan_lane_seed(lane_id: u64) -> u64 {
  SCAN_SEED ^ lane_id.wrapping_mul(0x9e37_79b9_7f4a_7c15)
}

/// 构造并发 scan 链：链数与分区轮转对齐 C++ `num_scanners`。
///
/// 空分区（`rows < partitions`）仍建链：C++ 每个分区都持有全量键空间，
/// 扫描空表会得到空页，但延迟样本与并发度口径一致。`partitions` 由调用方
/// 钳到 `>= 1`（与 [`partition_key_num`] 同前提）。
fn scan_lane_li(partitions: usize, read_per_part: usize) -> Vec<ScanLane> {
  let num = partitions.saturating_mul(read_per_part);
  (0..num)
    .map(|id| ScanLane {
      part: id % partitions,
      seed: scan_lane_seed(id as u64),
    })
    .collect()
}

/// `--scan-bytes` 折算成窗口内的 key 个数，对齐 C++
/// `max(1, scan_bytes / (kv_size - sizeof(uint64_t)))`。
///
/// 返回 `None` 表示 `scan_bytes == 0`：end 为空、扫到表尾（C++ 同口径）。
fn scan_kvs(scan_bytes: u64, value_size: usize) -> Option<usize> {
  (scan_bytes > 0).then(|| (scan_bytes / value_size as u64).max(1) as usize)
}

/// 一次 scan 请求的起点（全局 key 下标）。
///
/// 窗口模式对齐 C++ `send_req`：`max_start = rows - scan_kvs`，
/// `start = max_start > 0 ? rand % max_start : 0`（注意 C++ 也取不到 `max_start`
/// 本身，这里保持一致）；`end = start + scan_kvs`。扫到表尾时 `start`
/// 取 `[0, rows)`、`end` 为 `None`。
fn scan_range(rows: usize, kvs: Option<usize>, rng: &mut Rng) -> (usize, Option<usize>) {
  let Some(kvs) = kvs else {
    return (rng.usize(0..rows), None);
  };
  let max_start = rows.saturating_sub(kvs);
  let start = if max_start > 0 {
    rng.usize(0..max_start)
  } else {
    0
  };
  (start, Some(start + kvs))
}

/// 把 8 字节大端 key 解回全局下标；非本 bench 编码的 key 返回 `None`。
fn decode_key(key: &[u8]) -> Option<usize> {
  let bytes: [u8; KEY_BYTES] = key.try_into().ok()?;
  usize::try_from(u64::from_be_bytes(bytes)).ok()
}

/// scan 阶段的固定参数（每链共享）。
#[derive(Debug, Clone, Copy)]
struct ScanPlan {
  rows: usize,
  partitions: usize,
  key_interval: usize,
  value_size: usize,
  /// `--scan-bytes` 折算的窗口；`None`＝扫到表尾。
  kvs: Option<usize>,
  window: Duration,
}

/// 一条 scan 链的产出：直方图 + 页数 / 条数。
struct ScanLaneResult {
  hist: LatencyHist,
  page_num: u64,
  kv_num: u64,
}

/// 跑一条 scan 链：每请求一页（256 条）、随机起点，窗口内持续发请求。
///
/// 判停口径与读链一致：`window` 为零 → 固定次数模式，每链只发 1 个页请求
/// （最小样本，历史固定次数模式没有等价物理量）；否则按 C++ `ScanLoop`
/// 的 `test_secs` 时间窗持续循环。窗口模式至少完成一次请求。
///
/// 每个返回 key 都校验：属于本分区、落在请求区间内、解码后是存活 key。
/// 分区是同键空间的分片，随机起点可能落在本分区最后一个 key 之后，此时
/// 返回空页——这是分片语义的真实反映，不是错误。
async fn run_scan_lane(
  store: &Store<CompioStore>,
  tbl: &TableIdent,
  lane: ScanLane,
  plan: ScanPlan,
  state: PartState,
  mix: WriteMix,
) -> ScanLaneResult {
  let mut result = ScanLaneResult {
    hist: LatencyHist::new(),
    page_num: 0,
    kv_num: 0,
  };
  let mut rng = Rng::with_seed(lane.seed);
  let started = Instant::now();
  let timed = !plan.window.is_zero();
  loop {
    let (start, end) = scan_range(plan.rows, plan.kvs, &mut rng);
    let begin = key_bytes(start * plan.key_interval);
    let end_key = end.map(|end| key_bytes(end * plan.key_interval));
    let end_slice: &[u8] = match &end_key {
      Some(key) => key,
      None => &[],
    };
    let req_start = Instant::now();
    let page = store
      .scan_page(
        tbl,
        &begin,
        end_slice,
        ScanOption::default(),
        PageOption {
          page_entries: SCAN_PAGE_ENTRIES,
          page_size: 0,
        },
      )
      .await
      .unwrap();
    result.hist.record(req_start.elapsed());
    result.page_num += 1;
    result.kv_num += page.entry_li.len() as u64;
    assert!(page.entry_li.len() <= SCAN_PAGE_ENTRIES);
    // 逐条校验只在 debug 构建执行（cargo test 跑 debug）；release bench 不该为
    // 断言付每页 256 次的解码/取模开销，跨实现分位才干净。
    if cfg!(debug_assertions) {
      for (key, value) in &page.entry_li {
        let index = decode_key(key).expect("scan 返回的 key 必须是 8B 大端编码");
        assert_eq!(value.len(), plan.value_size);
        assert!(index.is_multiple_of(plan.key_interval));
        let global = index / plan.key_interval;
        assert_eq!(
          global % plan.partitions,
          lane.part,
          "scan 返回了其它分区的 key：global {global}"
        );
        assert!((start..end.unwrap_or(usize::MAX)).contains(&global));
        let pos = (global - lane.part) / plan.partitions;
        assert!(
          live_after(state.written, state.count, pos, mix.del_ratio),
          "scan 返回了不存在的 key：part {} pos {pos}",
          lane.part
        );
      }
    }
    if timed {
      if started.elapsed() >= plan.window {
        break;
      }
    } else {
      break;
    }
  }
  result
}

/// 解析 `--kv-size`：整对 KV 宽度（含 8B key），缺省取 [`DEFAULT_KV_SIZE`]。
/// `kv_size <= 8` 拒绝并报参数错误，对齐 C++ `simple_bench.cpp:550`
/// `CHECK(FLAGS_kv_size > sizeof(uint64_t))`（pair 口径下 value = kv_size - 8，
/// 再小就下溢）。非法数值沿用本文件其余参数的口径：按缺省值处理。
fn resolve_kv_size(arg: Option<&str>) -> Result<usize, String> {
  let kv_size = arg
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(DEFAULT_KV_SIZE);
  if kv_size <= KEY_BYTES {
    return Err(format!(
      "--kv-size {kv_size} 非法：kv_size 是整对 KV 宽度（含 {KEY_BYTES}B key），必须 > {KEY_BYTES}（对齐 C++ simple_bench 的 CHECK）"
    ));
  }
  Ok(kv_size)
}

/// value 字节数 = 整对 KV 宽度 - key 宽度，对齐 C++ `simple_bench.cpp:113`
/// `value.resize(FLAGS_kv_size - sizeof(uint64_t))`；调用方先经
/// [`resolve_kv_size`] 保证 `kv_size > KEY_BYTES`，不会下溢。
fn value_len(kv_size: usize) -> usize {
  kv_size - KEY_BYTES
}

/// 端到端 workload 计量。
///
/// `test_secs` > 0 时为读 / scan 的时间窗模式（对齐 C++ `simple_bench` 的
/// `test_secs` 稳态口径）：读链与 scan 链逐请求在窗口内循环，分位与吞吐按
/// 窗口内全部样本计算。写阶段按次数驱动（`--write-batchs`，缺省 0＝把
/// `--rows` 写一遍），不随窗口放大。`test_secs` = 0 时读 / scan 为固定次数
/// 模式（读满每分区一遍、scan 每链一个页请求）。
fn bench_workload(args: BenchArgs, base_dir: Option<&Path>) -> Vec<(String, f64, &'static str)> {
  let BenchArgs {
    rows,
    value_size,
    batch_size,
    workload,
    mix,
    plan,
    test_secs,
    write_batchs,
    scan_bytes,
  } = args;
  let runtime = Runtime::new().unwrap();
  runtime.block_on(async move {
    let root = bench_root(base_dir);
    let partitions = plan.partitions.max(1);
    let batch_size = batch_size.max(1);
    let read_per_part = plan.read_per_part.max(1);
    // 各阶段时间窗：0（或非法值）＝固定次数模式。
    let window = window_duration(test_secs);
    // 每个分区一棵独立表；key 按 `index % partitions` 分散。
    let mut store_li: Vec<(TableIdent, Store<CompioStore>)> = (0..partitions)
      .map(|part| {
        let tbl = TableIdent::new("bench", part as u32 + 1).unwrap();
        let store = Store::new(
          CompioStore::new(root.clone(), 4096, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT),
          4096,
          16,
        );
        (tbl, store)
      })
      .collect();

    let value = vec![0x5au8; value_size];

    let mut result_li = Vec::new();

    // 写阶段。`read-only` / `scan` 时静默写入作为数据准备，不计入指标。
    let measure_write = workload.measures_write();
    let do_read = workload.measures_read();
    let do_scan = workload.measures_scan();
    // 每分区一条独立写流水：完成即补发，无轮间 barrier（对齐 C++ `WriteLoop`）。
    let lane_li = write_lane_li(rows, partitions);
    let mode = if write_batchs > 0 {
      WriteMode::Churn {
        batch_size,
        batch_num: write_batchs as u64,
      }
    } else {
      WriteMode::Once { batch_size }
    };
    // 覆盖写模式的全系统批数预算；各分区完成即认领，不预先均分。
    let budget = AtomicU64::new(0);
    // 吞吐窗口：单调 µs 时钟，分辨率不低于其包住的 per-request 采样（见模块注释）。
    let started = Instant::now();
    let lane_result_li = join_all(
      store_li
        .iter_mut()
        .zip(&lane_li)
        .map(|((tbl, store), lane)| run_write_lane(store, tbl, *lane, mode, mix, &value, &budget)),
    )
    .await;
    let elapsed = started.elapsed().as_secs_f64();
    let mut put_hist = LatencyHist::new();
    let mut batch_num = 0u64;
    let mut kv_num = 0u64;
    let mut part_state_li = vec![PartState::default(); partitions];
    for (lane, result) in lane_li.iter().zip(&lane_result_li) {
      put_hist.merge(&result.hist);
      batch_num += result.batch_num;
      kv_num += result.kv_num;
      part_state_li[lane.part] = PartState {
        count: lane.count,
        written: result.kv_num as usize,
      };
    }
    // 写入量口径：`Once` 恰好覆盖 `rows` 个 key；`Churn` 恰好
    // `write_batchs * batch_size` 条（对齐 C++ 的全系统批数）。
    match mode {
      WriteMode::Once { batch_size } => {
        assert_eq!(kv_num, rows as u64);
        let expect_batch: u64 = lane_li
          .iter()
          .map(|lane| once_batch_num(lane.count, batch_size))
          .sum::<usize>() as u64;
        assert_eq!(batch_num, expect_batch);
      }
      WriteMode::Churn {
        batch_size,
        batch_num: total_batch,
      } => assert_eq!(kv_num, total_batch * batch_size as u64),
    }
    if measure_write {
      result_li.push(("put_kv_s".to_owned(), kv_num as f64 / elapsed, "kv/s"));
      result_li.push((
        "put_batch_s".to_owned(),
        batch_num as f64 / elapsed,
        "batch/s",
      ));
      push_latency(&mut result_li, "put", &put_hist);
    }

    if do_read || do_scan {
      // 写后 reopen 丢弃 index cache，让点读走真实 index 页读取（冷路径）。
      for (tbl, store) in &mut store_li {
        store.reopen(tbl).await.unwrap();
      }
    }

    if do_read {
      // 点读：`partitions * read_per_part` 条读链并发在飞，对齐 C++ 的
      // `read_per_part * partitions` 个并发读者。每链绑定一个分区、独立 RNG，
      // 各链结果先记入本链直方图再合并；固定次数模式总读次数 `rows * read_per_part`，
      // 时间窗模式持续随机读直到窗口耗尽（对齐 C++ `test_secs` 稳态口径）。
      let lane_li = read_lane_li(rows, partitions, read_per_part);
      let started = Instant::now();
      let hist_li = join_all(lane_li.iter().map(|lane| {
        let (tbl, store) = &store_li[lane.part];
        run_read_lane(
          store,
          tbl,
          *lane,
          part_state_li[lane.part],
          value_size,
          mix,
          window,
        )
      }))
      .await;
      let mut get_hist = LatencyHist::new();
      for hist in &hist_li {
        get_hist.merge(hist);
      }
      let elapsed = started.elapsed().as_secs_f64();
      // 分子用窗口内实际完成的请求数：每个完成请求恰好记一个直方图样本
      // （链回填无丢失），固定次数模式下仍等于 `rows * read_per_part`。
      let read_num: u64 = hist_li.iter().map(|hist| hist.count).sum();
      result_li.push(("get_kv_s".to_owned(), read_num as f64 / elapsed, "kv/s"));
      push_latency(&mut result_li, "get", &get_hist);
    }

    if do_scan {
      // 主计量：`partitions * read_per_part` 条 scan 链并发，每请求一页（256 条）、
      // 随机起点（对齐 C++ `ScanLoop` 的 Scanner / SetPagination(256, 0)）。
      // 时间窗模式持续发请求；固定次数模式每链只发 1 个页请求。
      let plan = ScanPlan {
        rows,
        partitions,
        key_interval: mix.key_interval,
        value_size,
        kvs: scan_kvs(scan_bytes, value_size),
        window,
      };
      let lane_li = scan_lane_li(partitions, read_per_part);
      let started = Instant::now();
      let lane_result_li = join_all(lane_li.iter().map(|lane| {
        let (tbl, store) = &store_li[lane.part];
        run_scan_lane(store, tbl, *lane, plan, part_state_li[lane.part], mix)
      }))
      .await;
      let elapsed = started.elapsed().as_secs_f64();
      let mut scan_hist = LatencyHist::new();
      let mut page_num = 0u64;
      let mut kv_num = 0u64;
      for result in &lane_result_li {
        scan_hist.merge(&result.hist);
        page_num += result.page_num;
        kv_num += result.kv_num;
      }
      result_li.push(("scan_kv_s".to_owned(), kv_num as f64 / elapsed, "kv/s"));
      result_li.push((
        "scan_page_s".to_owned(),
        page_num as f64 / elapsed,
        "page/s",
      ));
      push_latency(&mut result_li, "scan", &scan_hist);

      // 附加指标：顺序全表遍历（旧 `scan_kv_s` 口径）。固定次数模式扫一遍，
      // 时间窗模式整轮重复。C++ 无对应物，仅供诊断，不可与 `scan_*` 混比。
      //
      // 存活数断言：写完一遍时精确已知；覆盖写模式下状态只由写入顺序决定，
      // 每轮条数恒定，故断言各轮相等。
      let expected_live: Option<usize> = (write_batchs == 0).then(|| {
        (0..partitions)
          .map(|part| live_key_num(partition_key_num(rows, partitions, part), mix.del_ratio))
          .sum()
      });
      let mut fullscan_hist = LatencyHist::new();
      let started = Instant::now();
      let mut scanned = 0usize;
      let mut first_round: Option<usize> = None;
      loop {
        let mut round_num = 0usize;
        for (tbl, store) in &store_li {
          let req_start = Instant::now();
          round_num += store.scan(tbl, b"", b"", 0).await.unwrap().len();
          fullscan_hist.record(req_start.elapsed());
        }
        scanned += round_num;
        match expected_live {
          Some(total) => assert_eq!(round_num, total),
          None => match first_round {
            Some(first) => assert_eq!(round_num, first),
            None => first_round = Some(round_num),
          },
        }
        if window.is_zero() || started.elapsed() >= window {
          break;
        }
      }
      let elapsed = started.elapsed().as_secs_f64();
      result_li.push(("fullscan_kv_s".to_owned(), scanned as f64 / elapsed, "kv/s"));
      push_latency(&mut result_li, "fullscan", &fullscan_hist);
    }

    let _ = fs::remove_dir_all(root);
    result_li
  })
}

fn bench_root(base_dir: Option<&Path>) -> PathBuf {
  // 同一进程内并发跑多个 bench 时，秒级时间戳会撞名导致互相删目录。
  static NEXT_SEQ: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_SEQ.fetch_add(1, Ordering::Relaxed);
  let base = base_dir.map_or_else(env::temp_dir, Path::to_path_buf);
  let root = base.join(format!("fkv_bench_{}_{}_{}", id(), now_secs(), seq));
  let _ = fs::remove_dir_all(&root);
  root
}

#[cfg(test)]
mod tests {
  use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
  };

  use fastrand::Rng;

  use super::{
    BenchArgs, DEFAULT_KV_SIZE, LatencyHist, PerfData, ReadPlan, Workload, WriteLane, WriteMix,
    bench_workload, claim_batch, decode_key, effective_rows, is_deleted, is_live, key_bytes,
    lane_seed, live_after, live_key_num, now_secs, once_batch_num, parse_vmhwm, partition_key_num,
    read_lane_li, resolve_kv_size, scan_kvs, scan_lane_li, scan_range, value_len, window_duration,
    write_entry, write_lane_li,
  };

  /// 时间戳必须是 Unix **秒**（不是毫秒），且与墙钟同步；
  /// C++ `utils::UnixTs<seconds>` 同一口径，跨实现对比才不会差 1000 倍。
  #[test]
  fn now_secs_is_unix_seconds() {
    let wall = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_secs() as i64;
    assert!((now_secs() - wall).abs() <= 2);
  }

  /// 每个阶段的延迟指标后缀，对齐 C++ `LatencyRecorder` 输出。
  const LAT_SUFFIX_LI: [&str; 7] = [
    "avg_us", "p50_us", "p90_us", "p99_us", "p999_us", "p9999_us", "max_us",
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
      },
      None,
    );
    assert_eq!(
      throughput_name_li(&metric_li),
      vec![
        "put_kv_s",
        "put_batch_s",
        "get_kv_s",
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
      },
      None,
    );
    assert_eq!(
      throughput_name_li(&metric_li),
      vec!["put_kv_s", "put_batch_s"]
    );
    assert_latency_metric_li(&metric_li, "put");
    assert_eq!(metric_li.len(), 2 + LAT_SUFFIX_LI.len());
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
      },
      None,
    );
    assert_eq!(
      throughput_name_li(&metric_li),
      vec!["get_kv_s", "scan_kv_s", "scan_page_s", "fullscan_kv_s"]
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
      },
      None,
    );
    assert_eq!(throughput_name_li(&metric_li).len(), 6);
    assert_eq!(metric_li.len(), 6 + 4 * LAT_SUFFIX_LI.len());
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
      },
      None,
    );
    assert_eq!(throughput_name_li(&metric_li).len(), 6);
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
      },
      None,
    );
    assert_eq!(
      throughput_name_li(&metric_li),
      vec![
        "put_kv_s",
        "put_batch_s",
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
        },
        plan: ReadPlan {
          partitions: 2,
          read_per_part: 2,
        },
        test_secs: 0.0,
        write_batchs: 8,
        scan_bytes: 320,
      },
      None,
    );
    assert_eq!(
      throughput_name_li(&metric_li),
      vec![
        "put_kv_s",
        "put_batch_s",
        "get_kv_s",
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

  /// `--test-secs` 折算：0 / 负值 / NaN 一律固定次数模式，正值换算成
  /// Duration，超出 `Duration` 域的正值饱和到 `MAX` 而非退化。
  #[test]
  fn window_duration_zero_or_invalid_is_fixed_mode() {
    assert_eq!(window_duration(0.0), Duration::ZERO);
    assert_eq!(window_duration(-1.5), Duration::ZERO);
    assert_eq!(window_duration(f64::NAN), Duration::ZERO);
    assert_eq!(window_duration(0.2), Duration::from_millis(200));
    assert_eq!(window_duration(60.0), Duration::from_secs(60));
    assert_eq!(window_duration(f64::INFINITY), Duration::MAX);
  }

  /// `--kv-size` 解析（rev-01）：整对 KV 宽度（含 8B key）口径。
  #[test]
  fn resolve_kv_size_pair_semantics() {
    // 缺省 128，对齐 C++ `DEFINE_uint32(kv_size, 128, "size of a pair of KV")`。
    assert_eq!(resolve_kv_size(None), Ok(DEFAULT_KV_SIZE));
    assert_eq!(resolve_kv_size(Some("128")), Ok(128));
    // 非法数值沿用本文件其余参数的口径：按缺省值处理。
    assert_eq!(resolve_kv_size(Some("abc")), Ok(DEFAULT_KV_SIZE));
    // `<= 8` 拒绝：对齐 C++ `CHECK(FLAGS_kv_size > sizeof(uint64_t))`。
    assert!(resolve_kv_size(Some("8")).is_err());
    assert!(resolve_kv_size(Some("0")).is_err());
    // value 长度推导：value = kv_size - 8（对齐 C++ `value.resize(FLAGS_kv_size - 8)`）。
    assert_eq!(value_len(DEFAULT_KV_SIZE), 120);
    assert_eq!(value_len(9), 1);
  }

  /// history 口径：旧 perf.json 缺 `test_secs` / `write_batchs` / `scan_bytes`
  /// 字段反序列化不破（默认 0）；新文件序列化带这三个字段且可回读。
  #[test]
  fn perf_data_new_fields_back_compat() {
    let old = r#"{
      "baseline": "",
      "generated_at": 0,
      "workload": "write-read",
      "kv_size": 1024,
      "batch_size": 100,
      "max_key": 10000000,
      "read_per_part": 4,
      "partitions": 1,
      "run_li": []
    }"#;
    let data: PerfData = sonic_rs::from_slice(old.as_bytes()).unwrap();
    assert_eq!(data.test_secs, 0.0);
    assert_eq!(data.write_batchs, 0);
    assert_eq!(data.scan_bytes, 0);
    let data = PerfData {
      test_secs: 60.0,
      write_batchs: 32768,
      scan_bytes: 1 << 20,
      ..PerfData::default()
    };
    let json = sonic_rs::to_string(&data).unwrap();
    assert!(json.contains("test_secs"));
    assert!(json.contains("write_batchs"));
    assert!(json.contains("scan_bytes"));
    let back: PerfData = sonic_rs::from_slice(json.as_bytes()).unwrap();
    assert_eq!(back.test_secs, 60.0);
    assert_eq!(back.write_batchs, 32768);
    assert_eq!(back.scan_bytes, 1 << 20);
  }

  /// `del_ratio` 的 delete 落在 `del_ratio - 1` 步之前的 key 上，不是删不存在的 key。
  #[test]
  fn delete_targets_earlier_keys() {
    assert!(!is_deleted(2, 4));
    assert!(is_deleted(3, 4));
    assert_eq!(super::delete_target(3, 4), 0);
    assert_eq!(super::delete_target(7, 4), 4);
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

  /// 写请求按**分区内序列位**编码；delete 目标与 upsert 同属一个分区。
  #[test]
  fn write_entries_stay_in_partition() {
    let mix = WriteMix {
      key_interval: 1,
      del_ratio: 4,
    };
    // 分区 1/4：序列位 3 是 delete 位，目标是序列位 0 → 全局 key 1。
    let del = write_entry(1, 4, 3, mix, b"v");
    assert_eq!(del.key, key_bytes(1).to_vec());
    assert!(del.op.is_delete());
    // 序列位 2 → 全局 key 1 + 4 * 2 = 9。
    let upsert = write_entry(1, 4, 2, mix, b"v");
    assert_eq!(upsert.key, key_bytes(9).to_vec());
    assert_eq!(upsert.value, b"v".to_vec());
    assert!(!upsert.op.is_delete());
    // 稀疏 key：序列位 2、interval 3 → key (1 + 4 * 2) * 3。
    let sparse = write_entry(
      1,
      4,
      2,
      WriteMix {
        key_interval: 3,
        del_ratio: 4,
      },
      b"v",
    );
    assert_eq!(sparse.key, key_bytes(27).to_vec());
  }

  /// `--batch-size` 是单分区单请求条数：`partitions 4 / batch 8192` 下每个
  /// 分区请求恰好 8192 条（对齐 C++ `Writer` 每请求 `batch_size` 条）。
  #[test]
  fn write_batch_is_per_partition_entries() {
    let mix = WriteMix::default();
    let mut lane = WriteLane {
      part: 2,
      step: 4,
      count: 8192,
      pos: 0,
    };
    let batch = lane.next_batch(8192, mix, b"v");
    assert_eq!(batch.len(), 8192);
    assert_eq!(lane.pos, 0, "写满一遍后游标回到 0");
    // 所有 key 都落在本分区：全局下标 % 4 == 2。
    for (i, entry) in batch.iter().enumerate() {
      assert_eq!(decode_key(&entry.key), Some(2 + 4 * i));
    }
    // 写一遍的分区批数 = ceil(count / batch_size)。
    assert_eq!(once_batch_num(8192, 8192), 1);
    assert_eq!(once_batch_num(8193, 8192), 2);
    assert_eq!(once_batch_num(0, 8192), 0);
  }

  /// 覆盖写游标越过分区末尾回绕，且每个请求仍是 `batch_size` 条。
  #[test]
  fn write_lane_wraps_within_partition() {
    let mix = WriteMix::default();
    let mut lane = WriteLane {
      part: 0,
      step: 1,
      count: 5,
      pos: 3,
    };
    let batch = lane.next_batch(7, mix, b"v");
    assert_eq!(batch.len(), 7);
    let index_li: Vec<usize> = batch
      .iter()
      .map(|entry| decode_key(&entry.key).unwrap())
      .collect();
    // 3,4 越界回绕到 0..5；同一 batch 内允许重复 key（C++ 回绕同款）。
    assert_eq!(index_li, vec![3, 4, 0, 1, 2, 3, 4]);
  }

  /// 写流水按分区切分：各分区 key 数之和 == rows，Once 模式批数按分区累加。
  #[test]
  fn write_lanes_cover_rows() {
    let lane_li = write_lane_li(10, 3);
    assert_eq!(lane_li.len(), 3);
    let total: usize = lane_li.iter().map(|lane| lane.count).sum();
    assert_eq!(total, 10);
    assert_eq!(lane_li[0].step, 3);
    let batch_num: usize = lane_li
      .iter()
      .map(|lane| once_batch_num(lane.count, 4))
      .sum();
    // 4 / 3 / 3 个 key，batch=4 → 1 + 1 + 1。
    assert_eq!(batch_num, 3);
    // rows < partitions 的空分区仍保留流水（runner 跳过）。
    assert_eq!(write_lane_li(1, 4).len(), 4);
    assert_eq!(write_lane_li(1, 4)[3].count, 0);
  }

  /// `--write-batchs` 是全系统预算：并发流水合计认领恰好 `batch_num` 个 batch。
  #[test]
  fn claim_batch_budget_is_global() {
    let budget = AtomicU64::new(0);
    assert!(claim_batch(&budget, 3));
    assert!(claim_batch(&budget, 3));
    assert!(claim_batch(&budget, 3));
    assert!(!claim_batch(&budget, 3));
    assert!(!claim_batch(&budget, 0));
    assert_eq!(budget.load(Ordering::Relaxed), 5);
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

  #[test]
  fn latency_hist_empty_reports_zero() {
    let hist = LatencyHist::new();
    assert_eq!(hist.percentile_us(99), 0);
    assert_eq!(hist.avg_us(), 0.0);
    assert_eq!(hist.max_us, 0);
  }

  #[test]
  fn latency_hist_linear_percentiles() {
    let mut hist = LatencyHist::new();
    for us in 1..=100u64 {
      hist.record(Duration::from_micros(us));
    }
    // 100 个样本：p50 落在 50µs 桶，p99 落在 99µs 桶，p100 为最大值。
    assert_eq!(hist.percentile_us(50), 50);
    assert_eq!(hist.percentile_us(99), 99);
    assert_eq!(hist.percentile_us(100), 100);
    assert_eq!(hist.max_us, 100);
    assert_eq!(hist.avg_us(), 50.5);
  }

  #[test]
  fn latency_hist_overflow_bucket() {
    let mut hist = LatencyHist::new();
    hist.record(Duration::from_micros(100_000));
    // 100ms 落在 [65536, 131072)µs 桶，单一最大值时钳制到实测 max。
    assert_eq!(hist.percentile_us(50), 100_000);
    assert_eq!(hist.max_us, 100_000);
  }

  #[test]
  fn latency_hist_percentiles_are_monotonic() {
    let mut hist = LatencyHist::new();
    for us in [3u64, 7, 20, 900, 5_000, 70_000, 2_000_000] {
      hist.record(Duration::from_micros(us));
    }
    let mut last = 0;
    for percent in [50u32, 90, 99, 999, 9999, 100] {
      let value = hist.percentile_us(percent);
      assert!(value >= last, "p{percent} = {value} < {last}");
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
    assert_eq!(hist.percentile_us(100), 100_000);
    // 归并到空直方图等于复制。
    let mut empty = LatencyHist::new();
    empty.merge(&other);
    assert_eq!(empty.count, 2);
    assert_eq!(empty.max_us, 100_000);
  }

  /// 分区 key 数按 `index % partitions` 精确划分，且各分区之和 == rows。
  #[test]
  fn partition_key_num_covers_rows() {
    let total: usize = (0..3).map(|part| partition_key_num(10, 3, part)).sum();
    assert_eq!(total, 10);
    assert_eq!(partition_key_num(10, 3, 0), 4);
    assert_eq!(partition_key_num(10, 3, 1), 3);
    assert_eq!(partition_key_num(10, 3, 2), 3);
    // key 数少于分区数时，空分区计 0。
    assert_eq!(partition_key_num(1, 4, 0), 1);
    assert_eq!(partition_key_num(1, 4, 3), 0);
  }

  /// 并发读链数 = `partitions * read_per_part`（对齐 C++ 读者数），
  /// 每链读本分区全部 key，总读次数仍为 `rows * read_per_part`。
  #[test]
  fn read_lanes_match_cpp_reader_num() {
    let lane_li = read_lane_li(16, 2, 3);
    assert_eq!(lane_li.len(), 6);
    let total: usize = lane_li.iter().map(|lane| lane.count).sum();
    assert_eq!(total, 16 * 3);
    assert!(lane_li.iter().all(|lane| lane.step == 2));
    assert_eq!(lane_li.iter().filter(|lane| lane.part == 0).count(), 3);
    // 分区内随机下标必须落在该分区：`part + step * i < rows`。
    for lane in &lane_li {
      assert!(lane.part + lane.step * (lane.count - 1) < 16);
    }
    // 链种子互不相同，并发下可复现。
    let mut seed_li: Vec<u64> = lane_li.iter().map(|lane| lane.seed).collect();
    seed_li.sort_unstable();
    seed_li.dedup();
    assert_eq!(seed_li.len(), 6);
    assert_ne!(lane_seed(0), lane_seed(1));
    // 空分区不建链。
    assert_eq!(read_lane_li(1, 4, 2).len(), 2);
  }

  #[test]
  fn effective_rows_is_capped_by_max_key() {
    assert_eq!(effective_rows(100, 10), 10);
    assert_eq!(effective_rows(10, 100), 10);
    assert_eq!(effective_rows(0, 0), 1);
  }

  #[test]
  fn key_bytes_are_big_endian_and_ordered() {
    // 大端编码保证字典序与数值序一致（小端会在 255/256 处反转）。
    assert_eq!(
      key_bytes(0x0102_0304).to_vec(),
      vec![0, 0, 0, 0, 1, 2, 3, 4]
    );
    assert!(key_bytes(1) < key_bytes(2));
    assert!(key_bytes(255) < key_bytes(256));
  }

  #[test]
  fn parse_vmhwm_reads_peak_rss_field() {
    let status = "Name:\tfkv_bench\nVmPeak:\t 123456 kB\nVmHWM:\t  7890 kB\nVmRSS:\t  4000 kB\n";
    assert_eq!(parse_vmhwm(status), Some(7890));
  }

  #[test]
  fn parse_vmhwm_absent_or_malformed_is_none() {
    assert_eq!(parse_vmhwm("Name:\tfkv_bench\n"), None);
    assert_eq!(parse_vmhwm("VmHWM:\tnot-a-number kB\n"), None);
  }
}
