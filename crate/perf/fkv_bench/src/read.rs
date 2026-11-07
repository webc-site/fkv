//! 并发点读链与读阶段计量。

use std::time::{Duration, Instant};

use fastrand::Rng;
use fkv_io_compio::CompioStore;
use fkv_store::Store;
use fkv_types::TableIdent;
use futures::future::join_all;

use crate::{
  hist::{LatencyHist, WindowRecorder, push_latency, push_window_metrics},
  lane::{PartState, partition_key_num, probe_key_bytes, shard_index},
  workload::{WriteMix, live_after},
};

/// 点读随机序列的固定种子，保证 bench 可复现。
pub(crate) const READ_SEED: u64 = 0x5eed_0071_c01d_0001;
/// 一条并发读链：绑定一个分区、独立 RNG 与独立直方图（join 后合并）。
///
/// 链数 = `partitions * read_per_part`，对齐 C++ `simple_bench` 的并发读者数
/// `read_per_part * partitions`；每链负责把本分区的 key 随机读一遍。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ReadLane {
  /// 分区下标（`store_li` 下标）。
  pub(crate) part: usize,
  /// 分区内 key 的全局步长（＝分区数）：第 `i` 个 key 在全局是 `part + step * i`。
  pub(crate) step: usize,
  /// 该分区的 key 数，也是本链的随机采样范围与读次数。
  pub(crate) count: usize,
  /// 固定 RNG 种子，按链编号派生，保证并发下仍可复现。
  pub(crate) seed: u64,
}

/// 读链 RNG 种子：从全局 [`READ_SEED`] 按链编号派生，链间互不相同。
pub(crate) fn lane_seed(lane_id: u64) -> u64 {
  READ_SEED ^ lane_id.wrapping_mul(0x9e37_79b9_7f4a_7c15)
}

/// 构造并发读链：每分区 `read_per_part` 条链；`rows < partitions` 时空分区不建链。
///
/// 总读次数 = `read_per_part * Σ count`，而 `Σ count == rows`，故与串行版
/// `rows * read_per_part` 完全一致，只改变在飞并发度。
pub(crate) fn read_lane_li(rows: usize, partitions: usize, read_per_part: usize) -> Vec<ReadLane> {
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
/// `state` 是本分区的写状态：`exact` 且网格口径时，命中/未命中按
/// [`live_after`] 判定，覆盖写（`--write-batchs`）后也能精确断言；C++
/// 随机游走（`--rng cpp`）或跨进程读不到写状态时只校验读回值长度。
pub(crate) async fn run_read_lane(
  store: &Store<CompioStore>,
  tbl: &TableIdent,
  lane: ReadLane,
  state: PartState,
  value_size: usize,
  mix: WriteMix,
  window: Duration,
) -> ReadLaneResult {
  let mut hist = LatencyHist::new();
  let mut window_rec = WindowRecorder::new();
  let mut rng = Rng::with_seed(lane.seed);
  let started = Instant::now();
  let timed = !window.is_zero();
  let mut done = 0usize;
  loop {
    let pos = rng.usize(0..lane.count);
    let index = shard_index(lane.part, lane.step, pos);
    let req_start = Instant::now();
    let got = store.get(tbl, &probe_key_bytes(index, mix)).await.unwrap();
    let req_elapsed = req_start.elapsed();
    hist.record(req_elapsed);
    window_rec.record(req_elapsed, 1);
    // 被 delete 摘掉（或从未写到）的 key 读回 None，其余必须命中且长度一致。
    if state.exact && mix.rng.is_grid() {
      assert_eq!(
        got.is_some(),
        live_after(state.written, state.count, pos, mix.del_ratio),
        "part {} pos {pos} written {} count {}",
        lane.part,
        state.written,
        state.count
      );
    }
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
  ReadLaneResult {
    hist,
    window: window_rec,
  }
}

/// 一条读链的产出：累计直方图 + 逐秒窗口记录（阶段结束时合并）。
pub(crate) struct ReadLaneResult {
  pub(crate) hist: LatencyHist,
  pub(crate) window: WindowRecorder,
}

/// 读阶段参数。
pub(crate) struct ReadPhase<'a> {
  pub(crate) store_li: &'a [(TableIdent, Store<CompioStore>)],
  pub(crate) rows: usize,
  pub(crate) partitions: usize,
  pub(crate) read_per_part: usize,
  pub(crate) part_state_li: &'a [PartState],
  pub(crate) value_size: usize,
  pub(crate) mix: WriteMix,
  pub(crate) window: Duration,
}

/// 跑完并发点读阶段：`partitions * read_per_part` 条链并发在飞，返回吞吐与分位指标。
pub(crate) async fn run_read_phase(phase: ReadPhase<'_>) -> Vec<(String, f64, &'static str)> {
  let ReadPhase {
    store_li,
    rows,
    partitions,
    read_per_part,
    part_state_li,
    value_size,
    mix,
    window,
  } = phase;
  // 点读：`partitions * read_per_part` 条读链并发在飞，对齐 C++ 的
  // `read_per_part * partitions` 个并发读者。每链绑定一个分区、独立 RNG，
  // 各链结果先记入本链直方图再合并；固定次数模式总读次数 `rows * read_per_part`，
  // 时间窗模式持续随机读直到窗口耗尽（对齐 C++ `test_secs` 稳态口径）。
  let lane_li = read_lane_li(rows, partitions, read_per_part);
  let started = Instant::now();
  let lane_result_li = join_all(lane_li.iter().map(|lane| {
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
  let mut get_window = WindowRecorder::new();
  for result in &lane_result_li {
    get_hist.merge(&result.hist);
    get_window.merge(&result.window);
  }
  let elapsed = started.elapsed().as_secs_f64();
  // 分子用窗口内实际完成的请求数：每个完成请求恰好记一个直方图样本
  // （链回填无丢失），固定次数模式下仍等于 `rows * read_per_part`。
  let read_num: u64 = lane_result_li.iter().map(|result| result.hist.count).sum();
  let mut result_li = vec![("get_kv_s".to_owned(), read_num as f64 / elapsed, "kv/s")];
  push_latency(&mut result_li, "get", &get_hist);
  push_window_metrics(&mut result_li, "get", &get_window, elapsed);
  result_li
}
