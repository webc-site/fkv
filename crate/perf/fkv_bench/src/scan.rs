//! 并发 scan 链与 scan / fullscan 计量。

use std::time::{Duration, Instant};

use fastrand::Rng;
use fkv_io_compio::CompioStore;
use fkv_store::{PageOption, ScanOption, Store};
use fkv_types::TableIdent;
use futures::future::join_all;

use crate::{
  hist::{LatencyHist, push_latency},
  lane::{KEY_BYTES, PartState, partition_key_num, probe_key_bytes},
  workload::{WriteMix, live_after, live_key_num},
};

/// scan 随机区间（起点 / 窗口）的固定种子，与点读种子分开。
const SCAN_SEED: u64 = 0x5eed_0073_ca11_0001;
/// 每个 scan 请求的页大小：对齐 C++ `Scanner::page_size = 256`
/// （`SetPagination(256, 0)`），即一次请求一页。
const SCAN_PAGE_ENTRIES: usize = 256;
/// 一条并发 scan 链：绑定分区、独立 RNG 与独立直方图（join 后合并）。
///
/// 链数 = `partitions * read_per_part`，对齐 C++ `ScanLoop` 的
/// `num_scanners = read_per_part * partitions`；每链完成一页请求即发下一请求。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ScanLane {
  /// 分区下标（`store_li` 下标），按 `id % partitions` 轮转，对齐 C++ `Scanner`。
  pub(crate) part: usize,
  /// 固定 RNG 种子，按链编号派生，保证并发下仍可复现。
  pub(crate) seed: u64,
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
pub(crate) fn scan_lane_li(partitions: usize, read_per_part: usize) -> Vec<ScanLane> {
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
pub(crate) fn scan_kvs(scan_bytes: u64, value_size: usize) -> Option<usize> {
  (scan_bytes > 0).then(|| (scan_bytes / value_size as u64).max(1) as usize)
}

/// 一次 scan 请求的起点（全局 key 下标）。
///
/// 窗口模式对齐 C++ `send_req`：`max_start = rows - scan_kvs`，
/// `start = max_start > 0 ? rand % max_start : 0`（注意 C++ 也取不到 `max_start`
/// 本身，这里保持一致）；`end = start + scan_kvs`。扫到表尾时 `start`
/// 取 `[0, rows)`、`end` 为 `None`。
pub(crate) fn scan_range(rows: usize, kvs: Option<usize>, rng: &mut Rng) -> (usize, Option<usize>) {
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
pub(crate) fn decode_key(key: &[u8]) -> Option<usize> {
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
    let begin = probe_key_bytes(start, mix);
    let end_key = end.map(|end| probe_key_bytes(end, mix));
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
    //
    // `--rng cpp` 的分区是 C++ 语义的「每分区一份全量键空间副本」，key 不满足
    // `global % partitions == part`；写状态未知（跨进程读）时也无法推导存活，
    // 两种情况下只校验值长度与分页上界。
    let strict = state.exact && mix.rng.is_grid();
    if cfg!(debug_assertions) {
      for (key, value) in &page.entry_li {
        let index = decode_key(key).expect("scan 返回的 key 必须是 8B 大端编码");
        assert_eq!(value.len(), plan.value_size);
        if !strict {
          continue;
        }
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

/// scan 阶段参数。
pub(crate) struct ScanPhase<'a> {
  pub(crate) store_li: &'a [(TableIdent, Store<CompioStore>)],
  pub(crate) rows: usize,
  pub(crate) partitions: usize,
  pub(crate) read_per_part: usize,
  pub(crate) part_state_li: &'a [PartState],
  pub(crate) value_size: usize,
  pub(crate) mix: WriteMix,
  pub(crate) window: Duration,
  pub(crate) scan_bytes: u64,
  pub(crate) write_batchs: usize,
}

/// 跑完 scan 阶段：主计量（随机窗口页请求）与附加的全表遍历指标。
pub(crate) async fn run_scan_phase(phase: ScanPhase<'_>) -> Vec<(String, f64, &'static str)> {
  let ScanPhase {
    store_li,
    rows,
    partitions,
    read_per_part,
    part_state_li,
    value_size,
    mix,
    window,
    scan_bytes,
    write_batchs,
  } = phase;
  let mut result_li = Vec::new();
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
  let expected_live: Option<usize> = (write_batchs == 0
    && mix.rng.is_grid()
    && part_state_li.iter().all(|state| state.exact))
  .then(|| {
    (0..partitions)
      .map(|part| live_key_num(partition_key_num(rows, partitions, part), mix.del_ratio))
      .sum()
  });
  let mut fullscan_hist = LatencyHist::new();
  let started = Instant::now();
  let mut scanned = 0usize;
  let mut first_round: Option<usize> = None;
  // 首轮口径：`fullscan_num` = 首轮存活条数（各分区求和），
  // `fullscan_total_us` = 首轮整轮墙钟时间（含所有分区）。固定次数模式只有
  // 一轮，两者即整段；窗口模式只取首轮，使调用方能按「存活数 / 首轮耗时」
  // 推导 `len()` 遍历吞吐，而不被窗口内重复轮次稀释。
  let mut first_round_num: Option<usize> = None;
  let mut first_round_us: Option<u64> = None;
  let mut rounds = 0usize;
  loop {
    let round_started = Instant::now();
    let mut round_num = 0usize;
    for (tbl, store) in store_li {
      let req_start = Instant::now();
      // 计数路径：与 `scan(tbl, b"", b"", 0).len()` 同口径（同一叶子链遍历、
      // TTL 过滤与删除存活语义），但不物化 key/value、不读 overflow 链，
      // 全表 `len()` 只累加 entry 数。
      round_num += store.scan_count(tbl, b"", b"").await.unwrap();
      fullscan_hist.record(req_start.elapsed());
    }
    let round_us = round_started.elapsed().as_micros() as u64;
    if rounds == 0 {
      first_round_num = Some(round_num);
      first_round_us = Some(round_us);
    }
    scanned += round_num;
    rounds += 1;
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
  // 首轮至少执行一次，`expect` 不可能失败（固定次数与窗口模式都保证一轮）。
  let first_round_num = first_round_num.expect("fullscan 至少执行一轮");
  let first_round_us = first_round_us.expect("fullscan 至少执行一轮");
  result_li.push(("fullscan_num".to_owned(), first_round_num as f64, "num"));
  result_li.push(("fullscan_total_us".to_owned(), first_round_us as f64, "us"));
  result_li.push(("fullscan_rounds".to_owned(), rounds as f64, "num"));
  push_latency(&mut result_li, "fullscan", &fullscan_hist);
  result_li
}
