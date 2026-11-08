//! `compact` 诊断 lane：维护分区碎片写 + compact，其余分区并发点读。

use std::{
  path::Path,
  sync::atomic::AtomicU64,
  time::{Duration, Instant},
};

use fastrand::Rng;
use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_store::Store;
use fkv_types::TableIdent;
use futures::future::{join, join_all};

use crate::{
  hist::{LatencyHist, push_latency},
  lane::{PartState, WriteMode, once_batch_num, run_write_lane, write_entry, write_lane_li},
  read::{ReadLane, read_lane_li, run_read_lane},
  resource::dir_size_bytes,
  workload::{BenchArgs, WriteMix},
};

/// `compact` lane 的 data 文件粒度：`2^8 = 256` 页（4 KiB 页下一文件 1 MiB）。
///
/// 线上默认是 `2^18` 页（1 GiB）：bench 规模（百万级 KV）永远只写满尾文件，
/// 而 `compact_plan` 跳过尾文件，`compact_if_needed` 恒为空，lane 无法成立。
/// 该粒度只用于本诊断 lane，实际值以 `compact_pages_per_file` 指标输出，
/// 不与线上文件大小混读。
pub(crate) const COMPACT_LANE_PAGES_PER_FILE_SHIFT: u8 = 8;

/// `compact` lane 维护分区随机覆盖写的固定种子，保证可复现。
const COMPACT_SEED: u64 = 0x5eed_0075_c0de_0001;
/// `compact` lane 维护分区的一次压力循环参数。
struct CompactArm<'a> {
  tbl: &'a TableIdent,
  store: &'a mut Store<CompioStore>,
  /// 分区下标（`store_li` 下标）。
  part: usize,
  /// 分区内 key 的全局步长（＝分区数）。
  step: usize,
  /// 分区 key 数（随机取位上界）。
  count: usize,
  batch_size: usize,
  /// 固定次数模式的轮数；`window` 非零时忽略。
  fixed_round: u64,
  window: Duration,
  mix: WriteMix,
  value: &'a [u8],
}

/// 维护分区：每轮「随机覆盖写一批 + `compact_if_needed`」，直到窗口耗尽。
///
/// 随机（而非顺序整遍）覆盖是刻意的：顺序整遍会把旧文件写成全死，
/// `compact_plan` 把 `live == 0` 的文件留给 GC、不进入搬运，lane 反而看不到
/// compact；随机更新让每个中间文件留下部分活页，才有可搬运的候选。
async fn run_compact_arm(arm: CompactArm<'_>) -> Vec<(String, f64, &'static str)> {
  let CompactArm {
    tbl,
    store,
    part,
    step,
    count,
    batch_size,
    fixed_round,
    window,
    mix,
    value,
  } = arm;
  let mut rng = Rng::with_seed(COMPACT_SEED);
  let mut put_hist = LatencyHist::new();
  let mut compact_hist = LatencyHist::new();
  let mut file_num = 0u64;
  let mut run_num = 0u64;
  let mut kv_num = 0u64;
  let started = Instant::now();
  loop {
    let batch: Vec<BatchEntry> = (0..batch_size)
      .map(|_| write_entry(part, step, rng.usize(0..count), mix, value))
      .collect();
    let req_start = Instant::now();
    store.put(tbl, &batch).await.unwrap();
    put_hist.record(req_start.elapsed());
    kv_num += batch_size as u64;
    let req_start = Instant::now();
    file_num += store.compact_if_needed(tbl).await.unwrap().len() as u64;
    compact_hist.record(req_start.elapsed());
    run_num += 1;
    if window.is_zero() {
      if run_num >= fixed_round {
        break;
      }
    } else if started.elapsed() >= window {
      break;
    }
  }
  let elapsed = started.elapsed().as_secs_f64();
  let compact_secs = compact_hist.sum_us as f64 / 1e6;
  let mut result_li = vec![
    (
      "compact_pages_per_file".to_owned(),
      (1u64 << COMPACT_LANE_PAGES_PER_FILE_SHIFT) as f64,
      "page",
    ),
    ("compact_run_num".to_owned(), run_num as f64, "call"),
    ("compact_file_num".to_owned(), file_num as f64, "file"),
    (
      "compact_file_s".to_owned(),
      file_num as f64 / compact_secs.max(f64::EPSILON),
      "file/s",
    ),
    (
      "compact_put_kv_s".to_owned(),
      kv_num as f64 / elapsed,
      "kv/s",
    ),
    (
      "compact_put_batch_s".to_owned(),
      run_num as f64 / elapsed,
      "batch/s",
    ),
  ];
  // compact 调用本身的分位：包含空转（未达放大率）的快速返回样本。
  push_latency(&mut result_li, "compact", &compact_hist);
  push_latency(&mut result_li, "compact_put", &put_hist);
  result_li
}

/// `compact` lane 前台读臂参数。
struct CompactReadArm<'a> {
  /// 读分区（全局 1..N）的 store；下标 `part - 1`。
  read_li: &'a [(TableIdent, Store<CompioStore>)],
  part_state_li: &'a [PartState],
  rows: usize,
  partitions: usize,
  read_per_part: usize,
  value_size: usize,
  mix: WriteMix,
  window: Duration,
}

/// 前台读臂：只读非维护分区，与维护分区的 compact / 覆盖写在同一运行时内
/// 并发推进，分位覆盖 compact 进行中的读。
async fn run_compact_read_arm(arm: CompactReadArm<'_>) -> Vec<(String, f64, &'static str)> {
  let CompactReadArm {
    read_li,
    part_state_li,
    rows,
    partitions,
    read_per_part,
    value_size,
    mix,
    window,
  } = arm;
  if read_li.is_empty() {
    return Vec::new();
  }
  // 0 号分区归维护臂；读链按全局分区下标筛掉它。
  let lane_li: Vec<ReadLane> = read_lane_li(rows, partitions, read_per_part)
    .into_iter()
    .filter(|lane| lane.part != 0)
    .collect();
  let started = Instant::now();
  let hist_li = join_all(lane_li.iter().map(|lane| {
    let (tbl, store) = &read_li[lane.part - 1];
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
  let elapsed = started.elapsed().as_secs_f64();
  let mut hist = LatencyHist::new();
  let mut read_num = 0u64;
  for lane_result in &hist_li {
    hist.merge(&lane_result.hist);
    read_num += lane_result.hist.count;
  }
  let mut result_li = vec![("get_kv_s".to_owned(), read_num as f64 / elapsed, "kv/s")];
  push_latency(&mut result_li, "get", &hist);
  result_li
}

/// `compact` 诊断 lane：先写两遍把每个分区写成「有死页」的碎片 store，再让
/// 0 号分区持续随机覆盖写 + compact，其余分区并发点读。
///
/// 并发口径：`Store` 的 put / compact 都要 `&mut self`，同一分区在 API 层必然
/// 串行；线上争用来自不同分区的维护任务与前台读共享 compio 运行时与设备
/// （`write_gate` 是 per-Store 的，不跨 Store 共享）。本 lane 用
/// `split_at_mut` 让维护分区与读分区真实并发，读 p99 覆盖 compact 搬运期间。
/// `--partitions 1` 时没有前台读臂，只输出 compact 自身指标。
pub(crate) async fn bench_compact_lane(
  mut store_li: Vec<(TableIdent, Store<CompioStore>)>,
  args: BenchArgs,
  window: Duration,
  root: &Path,
) -> Vec<(String, f64, &'static str)> {
  let BenchArgs {
    rows,
    value_size,
    batch_size,
    mix,
    plan,
    write_batchs,
    ..
  } = args;
  let partitions = plan.partitions.max(1);
  let read_per_part = plan.read_per_part.max(1);
  let batch_size = batch_size.max(1);
  let value = vec![0x5au8; value_size];
  let lane_li = write_lane_li(rows, partitions, mix);
  // 准备阶段（不计指标）：每个分区顺序写两遍。第二遍让第一遍分配的页变成
  // 死页，读分区在并发阶段仍是完整 key 集，读侧断言口径与 read-only 一致。
  let budget = AtomicU64::new(0);
  for _ in 0..2 {
    join_all(
      store_li
        .iter_mut()
        .zip(&lane_li)
        .map(|((tbl, store), lane)| {
          run_write_lane(
            store,
            tbl,
            *lane,
            WriteMode::Once { batch_size },
            mix,
            &value,
            &budget,
          )
        }),
    )
    .await;
  }
  // redb 的 uncompacted / compacted size 口径：准备阶段写完后的目录大小，
  // 与 compact 压力跑完后的目录大小。两者都是落盘字节，不是存活数据估算。
  let store_bytes_before = dir_size_bytes(root);
  let part_state_li: Vec<PartState> = lane_li
    .iter()
    .map(|lane| PartState {
      count: lane.count,
      written: lane.count,
      exact: true,
    })
    .collect();
  // 读分区丢弃 page cache：点读走真实物理页读取（冷路径）。
  for (tbl, store) in store_li.iter_mut().skip(1) {
    store.reopen(tbl).await.unwrap();
  }
  // 维护分区只用覆盖写（del_ratio = 0）：这里要的是碎片，不是 tombstone。
  let pressure_mix = WriteMix {
    del_ratio: 0,
    ttl_ratio: 0,
    ttl_expire_ts: 0,
    ..mix
  };
  let fixed_round = if write_batchs > 0 {
    write_batchs as u64
  } else {
    once_batch_num(lane_li[0].count, batch_size) as u64
  }
  .max(1);
  let (head, tail) = store_li.split_at_mut(1);
  let compact_fut = run_compact_arm(CompactArm {
    tbl: &head[0].0,
    store: &mut head[0].1,
    part: 0,
    step: partitions,
    count: lane_li[0].count,
    batch_size,
    fixed_round,
    window,
    mix: pressure_mix,
    value: &value,
  });
  let read_fut = run_compact_read_arm(CompactReadArm {
    read_li: tail,
    part_state_li: &part_state_li,
    rows,
    partitions,
    read_per_part,
    value_size,
    mix,
    window,
  });
  let (mut result_li, read_li) = join(compact_fut, read_fut).await;
  result_li.extend(read_li);
  let store_bytes_after = dir_size_bytes(root);
  result_li.push((
    "compact_store_bytes_before".to_owned(),
    store_bytes_before as f64,
    "byte",
  ));
  result_li.push((
    "compact_store_bytes_after".to_owned(),
    store_bytes_after as f64,
    "byte",
  ));
  result_li.push((
    "compact_reclaim_ratio".to_owned(),
    1.0 - store_bytes_after as f64 / store_bytes_before.max(1) as f64,
    "ratio",
  ));
  result_li
}
