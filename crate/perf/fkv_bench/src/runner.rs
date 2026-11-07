//! 端到端 workload 编排：装配 store、按 workload 调度各阶段、清理目录。

use std::{
  env, fs,
  path::{Path, PathBuf},
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{Duration, Instant},
};

use compio::runtime::Runtime;
use fkv_config::KvOption;
use fkv_io_compio::CompioStore;
use fkv_store::{AllocMode, Store};
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, TableIdent};
use serde::{Deserialize, Serialize};

use crate::{
  compact::{COMPACT_LANE_PAGES_PER_FILE_SHIFT, bench_compact_lane},
  hist::{push_latency, push_window_metrics},
  lane::{PartState, WriteMode, WritePhase, delete_batch_num, partition_key_num, run_write_phase},
  read::{ReadPhase, run_read_phase},
  report::now_secs,
  resource::{
    collect_cache_stats, collect_write_cache_stats, dir_size_bytes, dir_stats, drain_io_counters,
    push_cache_stats, push_resource_metrics, push_store_breakdown, push_write_cache_stats,
  },
  scan::{ScanPhase, run_scan_phase},
  ttl::clean_ttl_lane,
  workload::{BenchArgs, RngMode, Workload, WriteMix},
};

/// 一次 bench 的运行形态（对齐 C++ 的独立 load / read 两段跑法）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RunMode {
  /// 写 + 读 + scan 一气跑完，结束时删目录（历史行为）。
  #[default]
  Normal,
  /// `--load-only`：只写数据集，保留 store 目录后退出。
  LoadOnly,
  /// `--read-only-existing`：打开已存在的 store，可选 drop index cache 后只读。
  ReadOnlyExisting,
}

/// CLI 之外的控制项：保持 [`crate::workload::BenchArgs`] 仍可 `Copy`，
/// 既有测试调用点不用改结构体字面量。
#[derive(Debug, Clone, Default)]
pub(crate) struct RunControl {
  /// 预热窗口（秒）；0＝不预热。
  pub(crate) warmup_secs: f64,
  pub(crate) mode: RunMode,
  /// load / read-only-existing 的固定 store 目录；`None` 用临时目录缺省名。
  pub(crate) store_dir: Option<PathBuf>,
  /// 是否输出资源 / IO 计数指标（store 字节、`/proc/self/io`、CompioStore 计数）。
  pub(crate) resource: bool,
  /// 读 / scan 阶段独立重复次数；写阶段只跑一次。
  pub(crate) trials: usize,
}

/// 阶段开关：load-only 只写，read-only-existing 不写，其余跟随 workload。
#[derive(Debug, Clone, Copy)]
struct PhasePlan {
  read: bool,
  scan: bool,
  /// 是否输出写指标。load-only 把「装载」本身当成被测操作。
  measure_write: bool,
  /// 是否输出 delete 指标（`delete-only` 的计量阶段）。
  measure_delete: bool,
  /// 是否做 TTL 回收计量；load-only 不能顺手把刚写的 TTL 条目清掉。
  ttl_cleanup: bool,
}

impl PhasePlan {
  fn from_workload(workload: Workload) -> Self {
    Self {
      read: workload.measures_read(),
      scan: workload.measures_scan(),
      measure_write: workload.measures_write(),
      measure_delete: workload.measures_delete(),
      ttl_cleanup: true,
    }
  }

  fn load_only() -> Self {
    Self {
      read: false,
      scan: false,
      measure_write: true,
      measure_delete: false,
      ttl_cleanup: false,
    }
  }
}

/// 把 CLI 的 `--test-secs`（秒）折算成各计量阶段的时间窗；`0`＝固定次数模式。
///
/// 0 / 负值 / NaN 一律按固定次数模式处理（NaN 不满足 `> 0.0`）；正值溢出按
/// [`Duration::MAX`] 饱和——请求「跑到天荒地老」不应悄悄退化成固定次数模式。
pub(crate) fn window_duration(test_secs: f64) -> Duration {
  if test_secs > 0.0 {
    Duration::try_from_secs_f64(test_secs).unwrap_or(Duration::MAX)
  } else {
    Duration::ZERO
  }
}

/// 按分区装配 store：每个分区一棵独立表，key 按 `index % partitions` 分散。
///
/// Store 参数取 `KvOption::default()`，与 C++ `KvOptions` 默认值同源：
/// pooled 分配、校验和开启、128MB 缓存、fd 10000、manifest 8MB、
/// overflow 16、file amplify 2。`compact` lane 是例外：自动 compaction 只在
/// `Append` 分配模式下启用（`Store::compact_if_needed` 的放大率统计依赖追加
/// 分配器），Pooled 下恒为空；因此该 lane 强制用 `Append`，并把放大率阈值
/// 降到 1，保证 bench 规模下能观测到文件搬运。
fn open_store_li(
  root: PathBuf,
  partitions: usize,
  pages_per_file_shift: u8,
  workload: Workload,
) -> Vec<(TableIdent, Store<CompioStore>)> {
  let opts = KvOption::default();
  let page_size = opts.data_page_size as usize;
  (0..partitions)
    .map(|part| {
      let tbl = TableIdent::new("bench", part as u32 + 1).unwrap();
      let mut store = Store::new(
        CompioStore::new(root.clone(), page_size, pages_per_file_shift)
          .with_fd_cache_limit(opts.fd_limit as usize)
          // C++ `IouringMgr` 对 data / segment 文件统一 O_DIRECT；bench 必须
          // 同语义，否则 Rust 走内核 page cache，读吞吐不可比。
          .with_direct_io(true),
        page_size,
        opts.data_page_restart_interval,
      )
      .with_index_restart_interval(opts.index_page_restart_interval)
      .with_pages_per_file_shift(pages_per_file_shift)
      // compact lane 必须用 Append：`compact_if_needed` 的放大率统计与
      // `skip_empty_space` 都只在 Append 分配器下成立，Pooled 下恒返回空，
      // lane 会静默输出 `compact_file_num = 0`。
      .with_alloc_mode(
        if matches!(workload, Workload::Compact | Workload::CompactOnly) || opts.data_append_mode {
          AllocMode::Append
        } else {
          AllocMode::Pooled
        },
      )
      .with_verify_checksum(!opts.skip_verify_checksum)
      .with_overflow_pointers(opts.overflow_pointers)
      .with_file_amplify_factor(opts.file_amplify_factor)
      .with_segment_amplify_factor(opts.segment_file_amplify_factor)
      .with_manifest_limit(opts.manifest_limit as usize)
      .with_index_cache_bytes(opts.buffer_pool_size as usize)
      .with_index_cache_total_bytes(opts.buffer_pool_size as usize)
      // 写路径页缓存（COW 路径免盘读）：与 index 缓存同源预算。
      .with_write_cache_bytes(opts.buffer_pool_size as usize);
      if matches!(workload, Workload::Compact | Workload::CompactOnly) {
        // 放大率阈值 1：只要文件里有死页就成为 compact 候选（lane 需要可观测的搬运）。
        store = store.with_file_amplify_factor(1);
      }
      (tbl, store)
    })
    .collect()
}

/// 端到端 workload 计量（历史入口，等价于 `RunControl::default()`；仅测试使用）。
#[cfg(test)]
pub(crate) fn bench_workload(
  args: BenchArgs,
  base_dir: Option<&Path>,
) -> Vec<(String, f64, &'static str)> {
  bench_workload_ext(args, base_dir, RunControl::default())
}

/// 端到端 workload 计量（可带预热 / load-only / read-only-existing）。
///
/// `test_secs` > 0 时为读 / scan 的时间窗模式（对齐 C++ `simple_bench` 的
/// `test_secs` 稳态口径）：读链与 scan 链逐请求在窗口内循环，分位与吞吐按
/// 窗口内全部样本计算。写阶段按次数驱动（`--write-batchs`，缺省 0＝把
/// `--rows` 写一遍），不随窗口放大。`test_secs` = 0 时读 / scan 为固定次数
/// 模式（读满每分区一遍、scan 每链一个页请求）。
///
/// `control.warmup_secs` > 0 时先跑一遍同形状负载（写 / 读 / scan 都跑，
/// 写阶段按预热窗口持续写），指标整份丢弃，再跑正式计量。
pub(crate) fn bench_workload_ext(
  args: BenchArgs,
  base_dir: Option<&Path>,
  control: RunControl,
) -> Vec<(String, f64, &'static str)> {
  let runtime = Runtime::new().unwrap();
  runtime.block_on(async move {
    let partitions = args.plan.partitions.max(1);
    let window = window_duration(args.test_secs);
    let warmup_window = window_duration(control.warmup_secs);
    // compact lane 用更细的文件粒度（`2^8` 页），否则 bench 规模下永远只有
    // 尾文件、`compact_plan` 恒空；其余 lane 保持线上默认粒度，行为不变。
    let mut pages_per_file_shift = bench_pages_per_file_shift(args.workload);
    let root = match control.mode {
      RunMode::Normal => bench_root(base_dir),
      RunMode::LoadOnly | RunMode::ReadOnlyExisting => {
        control.store_dir.clone().unwrap_or_else(|| {
          base_dir
            .map_or_else(env::temp_dir, Path::to_path_buf)
            .join(DEFAULT_STORE_DIR)
        })
      }
    };
    match control.mode {
      RunMode::LoadOnly => {
        // load-only 每次重建，保证写出来的是本次参数的数据集。
        let _ = fs::remove_dir_all(&root);
      }
      RunMode::ReadOnlyExisting => {
        assert!(
          root.is_dir(),
          "--read-only-existing 的 store 目录不存在：{}（先跑 --load-only --store-dir 同一路径）",
          root.display()
        );
      }
      RunMode::Normal => {}
    }
    // `compact-only` 对已有 store 重开时沿用装载阶段落盘的文件粒度，
    // 避免大 store 因 shift 不一致把页归到错误的 data 文件。
    if control.mode == RunMode::ReadOnlyExisting
      && args.workload.is_compact_only()
      && let Some(shift) = load_pages_per_file_shift(&root)
    {
      pages_per_file_shift = shift;
    }
    let mut store_li = open_store_li(
      root.clone(),
      partitions,
      pages_per_file_shift,
      args.workload,
    );

    // compact lane 有独立的准备 / 并发 / 计量流程，不经过通用写读阶段。
    if args.workload == Workload::Compact {
      let mut result_li = bench_compact_lane(store_li, args, window, &root).await;
      if control.resource {
        // compact lane 自己吃掉 store 句柄，IO 计数器无法回收；落盘字节仍可测。
        let stats = dir_stats(&root);
        result_li.push(("store_bytes".to_owned(), stats.total_bytes as f64, "byte"));
        push_store_breakdown(&mut result_li, stats);
      }
      if control.mode != RunMode::LoadOnly {
        let _ = fs::remove_dir_all(root);
      }
      return result_li;
    }

    // `compact-only`：对已有（或刚装载的）store 逐分区调用
    // `compact_if_needed`，只计量 compaction 本身与前后目录字节。
    // 不经过 `bench_compact_lane` 的并发维护 / 前台读诊断流程。
    if args.workload.is_compact_only() {
      if control.mode == RunMode::LoadOnly {
        let (mut metric_li, state_li) = run_pipeline(
          &mut store_li,
          &args,
          window,
          false,
          None,
          PhasePlan::load_only(),
          1,
        )
        .await;
        save_state(&root, &args, &state_li);
        if control.resource {
          let cache_stats = collect_cache_stats(&store_li);
          let write_cache_stats = collect_write_cache_stats(&store_li);
          let counters = drain_io_counters(store_li);
          let stats = dir_stats(&root);
          push_resource_metrics(&mut metric_li, counters, stats.total_bytes);
          push_store_breakdown(&mut metric_li, stats);
          push_cache_stats(&mut metric_li, cache_stats);
          push_write_cache_stats(&mut metric_li, write_cache_stats);
        }
        return metric_li;
      }
      let mut result_li = Vec::new();
      let bytes_before = dir_size_bytes(&root);
      let started = Instant::now();
      let mut file_num = 0u64;
      for (tbl, store) in store_li.iter_mut() {
        file_num += store.compact_if_needed(tbl).await.unwrap().len() as u64;
      }
      let total_us = started.elapsed().as_secs_f64() * 1e6;
      let bytes_after = dir_size_bytes(&root);
      result_li.push(("compact_total_us".to_owned(), total_us, "us"));
      result_li.push(("compact_file_num".to_owned(), file_num as f64, "file"));
      result_li.push((
        "store_bytes_before_compact".to_owned(),
        bytes_before as f64,
        "byte",
      ));
      result_li.push((
        "store_bytes_after_compact".to_owned(),
        bytes_after as f64,
        "byte",
      ));
      result_li.push((
        "compact_reclaim_ratio".to_owned(),
        1.0 - bytes_after as f64 / bytes_before.max(1) as f64,
        "ratio",
      ));
      if control.resource {
        push_store_breakdown(&mut result_li, dir_stats(&root));
      }
      if control.mode == RunMode::Normal {
        let _ = fs::remove_dir_all(root);
      }
      return result_li;
    }

    let mut result_li = Vec::new();
    match control.mode {
      RunMode::LoadOnly => {
        let (metric_li, state_li) = run_pipeline(
          &mut store_li,
          &args,
          window,
          false,
          None,
          PhasePlan::load_only(),
          1,
        )
        .await;
        result_li.extend(metric_li);
        save_state(&root, &args, &state_li);
      }
      RunMode::ReadOnlyExisting => {
        let state_li = load_state(&root, &args);
        if !warmup_window.is_zero() {
          let _ = run_pipeline(
            &mut store_li,
            &args,
            warmup_window,
            true,
            Some(&state_li),
            PhasePlan::from_workload(args.workload),
            1,
          )
          .await;
        }
        let (metric_li, _) = run_pipeline(
          &mut store_li,
          &args,
          window,
          false,
          Some(&state_li),
          PhasePlan::from_workload(args.workload),
          control.trials,
        )
        .await;
        result_li.extend(metric_li);
      }
      RunMode::Normal => {
        let plan = PhasePlan::from_workload(args.workload);
        if !warmup_window.is_zero() {
          if matches!(args.workload, Workload::TtlWrite) {
            eprintln!(
              "fkv_bench: --warmup-secs 对 ttl-write lane 无效（TTL 回收计量会重复计数），已忽略"
            );
          } else {
            let _ = run_pipeline(&mut store_li, &args, warmup_window, true, None, plan, 1).await;
          }
        }
        let (metric_li, _) = run_pipeline(
          &mut store_li,
          &args,
          window,
          false,
          None,
          plan,
          control.trials,
        )
        .await;
        result_li.extend(metric_li);
      }
    }

    let cache_stats = collect_cache_stats(&store_li);
    let write_cache_stats = collect_write_cache_stats(&store_li);
    let counters = drain_io_counters(store_li);
    if control.resource {
      let stats = dir_stats(&root);
      push_resource_metrics(&mut result_li, counters, stats.total_bytes);
      push_store_breakdown(&mut result_li, stats);
      push_cache_stats(&mut result_li, cache_stats);
      push_write_cache_stats(&mut result_li, write_cache_stats);
    }
    if control.mode == RunMode::Normal {
      let _ = fs::remove_dir_all(root);
    }
    result_li
  })
}

/// 跑一遍完整阶段序列；`warmup` 为真时写阶段按 `window` 持续写、指标由调用方丢弃。
///
/// `preset` 为 `Some` 时跳过写阶段（`--read-only-existing`），直接用保存的
/// 写状态做读侧存活判定。
async fn run_pipeline(
  store_li: &mut [(TableIdent, Store<CompioStore>)],
  args: &BenchArgs,
  window: Duration,
  warmup: bool,
  preset: Option<&[PartState]>,
  plan: PhasePlan,
  trials: usize,
) -> (Vec<(String, f64, &'static str)>, Vec<PartState>) {
  let BenchArgs {
    rows,
    value_size,
    batch_size,
    workload,
    mix,
    plan: read_plan,
    write_batchs,
    scan_bytes,
    ttl_ms,
    ..
  } = *args;
  let partitions = read_plan.partitions.max(1);
  let batch_size = batch_size.max(1);
  let read_per_part = read_plan.read_per_part.max(1);
  let value = vec![0x5au8; value_size];
  let mut result_li = Vec::new();

  // `delete-only` 的准备阶段：不计时地顺序 upsert 全部 key。删除阶段必须
  // 面对既有 key，因此这里强制网格序列 + del_ratio = 0，只发 upsert。
  let mut prepopulated = None;
  if workload.measures_delete() && preset.is_none() {
    let prep_mix = WriteMix {
      del_ratio: 0,
      ttl_ratio: 0,
      ttl_expire_ts: 0,
      rng: RngMode::Grid,
      ..mix
    };
    let prep = run_write_phase(
      store_li,
      rows,
      partitions,
      WriteMode::Once { batch_size },
      prep_mix,
      &value,
    )
    .await;
    prepopulated = Some(prep.part_state_li);
  }

  // 写阶段。`read-only` / `scan` 时静默写入作为数据准备，不计入指标。
  let part_state_li = if workload.measures_delete() {
    let mut state_li = if let Some(state_li) = preset {
      state_li.to_vec()
    } else {
      prepopulated.unwrap_or_else(|| {
        (0..partitions)
          .map(|part| PartState {
            count: partition_key_num(rows, partitions, part),
            written: 0,
            exact: true,
          })
          .collect()
      })
    };
    // 计量阶段：只发 delete，全系统共享批预算，不混入 upsert。
    if !warmup && plan.measure_delete {
      let batch_num = if write_batchs > 0 {
        write_batchs as u64
      } else {
        delete_batch_num(rows, partitions, batch_size)
      };
      let WritePhase {
        hist: delete_hist,
        window: delete_window,
        batch_num,
        kv_num,
        elapsed,
        ..
      } = run_write_phase(
        store_li,
        rows,
        partitions,
        WriteMode::DeleteAll {
          batch_size,
          batch_num,
        },
        mix,
        &value,
      )
      .await;
      result_li.push(("delete_kv_s".to_owned(), kv_num as f64 / elapsed, "kv/s"));
      result_li.push((
        "delete_batch_s".to_owned(),
        batch_num as f64 / elapsed,
        "batch/s",
      ));
      push_latency(&mut result_li, "delete", &delete_hist);
      push_window_metrics(&mut result_li, "delete", &delete_window, elapsed);
      for state in &mut state_li {
        state.written = 0;
      }
    }
    state_li
  } else if let Some(state_li) = preset {
    state_li.to_vec()
  } else {
    let mode = if warmup {
      WriteMode::Warmup { batch_size, window }
    } else if write_batchs > 0 {
      WriteMode::Churn {
        batch_size,
        batch_num: write_batchs as u64,
      }
    } else {
      WriteMode::Once { batch_size }
    };
    // 每分区一条独立写流水：完成即补发，无轮间 barrier（对齐 C++ `WriteLoop`）。
    let WritePhase {
      hist: put_hist,
      window: put_window,
      batch_num,
      kv_num,
      part_state_li,
      elapsed,
    } = run_write_phase(store_li, rows, partitions, mode, mix, &value).await;
    if !warmup && plan.measure_write {
      // TTL lane 的写指标单独前缀 `put_ttl`：它测的是「主树 + TTL 树两次
      // write_batch + 每条 ttl_key 分配」的成本，不能与无 TTL 的 put 混比。
      let put = if workload == Workload::TtlWrite {
        "put_ttl"
      } else {
        "put"
      };
      result_li.push((format!("{put}_kv_s"), kv_num as f64 / elapsed, "kv/s"));
      result_li.push((
        format!("{put}_batch_s"),
        batch_num as f64 / elapsed,
        "batch/s",
      ));
      push_latency(&mut result_li, put, &put_hist);
      push_window_metrics(&mut result_li, put, &put_window, elapsed);
    }

    // TTL lane 的回收计量：把注入时钟推到统一到期时刻，逐分区清空 TTL 树。
    if !warmup && plan.ttl_cleanup && workload == Workload::TtlWrite {
      result_li.push(("ttl_ratio".to_owned(), mix.ttl_ratio as f64, "ratio"));
      result_li.push(("ttl_ms".to_owned(), ttl_ms as f64, "ms"));
      result_li.extend(clean_ttl_lane(store_li, &part_state_li, mix).await);
    }
    part_state_li
  };

  if plan.read || plan.scan {
    // 写后 reopen 丢弃 index cache，让点读走真实 index 页读取（冷路径）。
    let reopen_start = Instant::now();
    for (tbl, store) in store_li.iter_mut() {
      store.reopen(tbl).await.unwrap();
    }
    result_li.push((
      "reopen_us".to_owned(),
      reopen_start.elapsed().as_micros() as f64,
      "us",
    ));
  }

  if plan.read {
    let mut trial_li = Vec::with_capacity(trials.max(1));
    for _ in 0..trials.max(1) {
      trial_li.push(
        run_read_phase(ReadPhase {
          store_li,
          rows,
          partitions,
          read_per_part,
          part_state_li: &part_state_li,
          value_size,
          mix,
          window,
        })
        .await,
      );
    }
    result_li.extend(median_metric_li(trial_li));
  }

  if plan.scan {
    let mut trial_li = Vec::with_capacity(trials.max(1));
    for _ in 0..trials.max(1) {
      trial_li.push(
        run_scan_phase(ScanPhase {
          store_li,
          rows,
          partitions,
          read_per_part,
          part_state_li: &part_state_li,
          value_size,
          mix,
          window,
          scan_bytes,
          write_batchs,
        })
        .await,
      );
    }
    result_li.extend(median_metric_li(trial_li));
  }

  (result_li, part_state_li)
}

/// 对同一阶段的多轮指标逐项取中位数。各轮的指标集合相同，顺序以第一轮为准。
fn median_metric_li(
  trial_li: Vec<Vec<(String, f64, &'static str)>>,
) -> Vec<(String, f64, &'static str)> {
  if trial_li.len() <= 1 {
    return trial_li.into_iter().next().unwrap_or_default();
  }
  let mut out = Vec::with_capacity(trial_li[0].len());
  for (name, _, unit) in &trial_li[0] {
    let mut values = trial_li
      .iter()
      .filter_map(|metric_li| {
        metric_li
          .iter()
          .find(|(metric, ..)| metric == name)
          .map(|(.., value, _)| *value)
      })
      .collect::<Vec<_>>();
    if values.is_empty() {
      continue;
    }
    values.sort_by(f64::total_cmp);
    out.push((name.clone(), values[values.len() / 2], *unit));
  }
  out
}

/// load-only 的缺省目录名（`--store-dir` 未指定时放在 `--root`/临时目录下）。
const DEFAULT_STORE_DIR: &str = "fkv_bench_store";

/// `compact` / `compact-only` 的 data 文件粒度：`2^8 = 256` 页（4 KiB 页下 1 MiB）。
///
/// 线上默认 `2^18` 页（1 GiB）在 bench 规模下永远只有尾文件，`compact_plan`
/// 跳过尾文件，诊断 lane 看不到搬运；只对这两个 lane 使用细粒度。
fn bench_pages_per_file_shift(workload: Workload) -> u8 {
  if matches!(workload, Workload::Compact | Workload::CompactOnly) {
    COMPACT_LANE_PAGES_PER_FILE_SHIFT
  } else {
    DEFAULT_PAGES_PER_FILE_SHIFT
  }
}

/// load / read 两段共享的写状态文件，放在 store 目录**旁边**，不污染数据库目录。
#[derive(Debug, Deserialize, Serialize)]
struct PersistedState {
  rows: usize,
  partitions: usize,
  write_batchs: usize,
  key_interval: usize,
  del_ratio: usize,
  rng: String,
  /// 装载阶段使用的 data 文件粒度；旧 state 文件缺省按线上默认值。
  #[serde(default = "default_pages_per_file_shift")]
  pages_per_file_shift: u8,
  part_li: Vec<(usize, usize)>,
}

fn default_pages_per_file_shift() -> u8 {
  DEFAULT_PAGES_PER_FILE_SHIFT
}

fn state_path(store_root: &Path) -> PathBuf {
  let name = store_root
    .file_name()
    .map(|name| name.to_string_lossy().into_owned())
    .unwrap_or_else(|| DEFAULT_STORE_DIR.to_owned());
  store_root.with_file_name(format!("{name}.state.json"))
}

/// `--load-only` 收尾：把行数 / 分区 / 每分区写状态落盘，供 read 阶段复用。
fn save_state(store_root: &Path, args: &BenchArgs, state_li: &[PartState]) {
  let state = PersistedState {
    rows: args.rows,
    partitions: args.plan.partitions.max(1),
    write_batchs: args.write_batchs,
    key_interval: args.mix.key_interval,
    del_ratio: args.mix.del_ratio,
    rng: args.mix.rng.as_str().to_owned(),
    pages_per_file_shift: bench_pages_per_file_shift(args.workload),
    part_li: state_li
      .iter()
      .map(|state| (state.count, state.written))
      .collect(),
  };
  let path = state_path(store_root);
  if let Ok(bytes) = sonic_rs::to_vec(&state)
    && let Err(err) = fs::write(&path, bytes)
  {
    eprintln!("fkv_bench: 写状态文件失败 {}: {err}", path.display());
  }
}

/// 读 state 文件里的文件粒度；缺失 / 未落盘时返回 `None`。
fn load_pages_per_file_shift(store_root: &Path) -> Option<u8> {
  let bytes = fs::read(state_path(store_root)).ok()?;
  let state = sonic_rs::from_slice::<PersistedState>(&bytes).ok()?;
  Some(state.pages_per_file_shift)
}

/// `--read-only-existing` 的写状态：优先读 load-only 落盘的文件。
///
/// 文件缺失或参数不一致时退回闭式推导：`--write-batchs 0` 视为每分区写完
/// 一遍（精确）；覆盖写模式无法还原分区归属，置 `exact: false`，读 / scan
/// 只校验值长度、不做存活断言。
fn load_state(store_root: &Path, args: &BenchArgs) -> Vec<PartState> {
  let partitions = args.plan.partitions.max(1);
  if let Ok(bytes) = fs::read(state_path(store_root)) {
    if let Ok(state) = sonic_rs::from_slice::<PersistedState>(&bytes) {
      let same = state.rows == args.rows
        && state.partitions == partitions
        && state.write_batchs == args.write_batchs
        && state.key_interval == args.mix.key_interval
        && state.del_ratio == args.mix.del_ratio
        && state.rng == args.mix.rng.as_str();
      if same && state.part_li.len() == partitions {
        return state
          .part_li
          .iter()
          .map(|&(count, written)| PartState {
            count,
            written,
            exact: true,
          })
          .collect();
      }
      eprintln!(
        "fkv_bench: 写状态文件与本次参数不一致，改用闭式推导（存活断言降级）：{}",
        state_path(store_root).display()
      );
    }
  } else {
    eprintln!(
      "fkv_bench: 未找到写状态文件 {}，改用闭式推导（存活断言降级）",
      state_path(store_root).display()
    );
  }
  let exact = args.write_batchs == 0;
  (0..partitions)
    .map(|part| {
      let count = partition_key_num(args.rows, partitions, part);
      PartState {
        count,
        written: if exact { count } else { 0 },
        exact,
      }
    })
    .collect()
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
