//! 点读分段采样：一次 `get` 里「取快照 / index 下降 / data 页 IO / 解码」各占多少。
//!
//! 只在 `--features read-prof` 下产出分段数字（`fkv_task_read::prof` 在
//! `descend_step` / `read_entry` 内按 `Instant` 累加）；不带该 feature 时只报
//! 端到端 ops/s 与页缓存命中，不会编译任何计时分支。
//!
//! ```text
//! cargo run --release -p fkv_store --features read-prof --example read_profile -- \
//!   --dir /tmp/fkv-perf-data/store --rows 25000000 --reads 200000 \
//!   --cache-bytes 268435456 --lanes 32
//! ```
//!
//! 口径：
//! - 与 `fkv_bench --read-only-existing` 相同：先 `open_branch` 打开既有库，
//!   `data page cache` 默认关闭（对齐 C++ `enable_data_page_cache=false`）；
//!   `--data-cache` 打开后 data / index 共用同一池，用于对照 C++ 开启数据
//!   页缓存时的 IO 次数。
//! - `snapshot_ns` 在 example 内计时（`Store::snapshot`），其余分段来自
//!   `ReadProf` 的线程本地累加。
//! - `page_reads_per_op` 是真实下盘页数 / 请求：关闭 data 缓存时约 1.00
//!   （每次点读一个 data 页，index 页已常驻），打开后按 data 命中率下降。
//! - `cpu_us_per_op` 读 `/proc/self/stat` 的 user+sys 增量（Linux）。
//! - `--lanes N` 在同一个 runtime 上并发 N 条读链，复现 bench 的并发口径；
//!   单链时 wall 就是单次 IO 延迟，多链时受设备 IOPS 上限约束。

use std::{path::PathBuf, time::Instant};

use compio::runtime::Runtime;
use fkv_io_compio::CompioStore;
use fkv_store::Store;
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, TableIdent};
use futures::future::join_all;

const PAGE_SIZE: usize = 4096;

/// 未启用 `read-prof` feature 时的零值替身，保持字段形状与分段输出一致。
#[cfg(not(feature = "read-prof"))]
#[derive(Default)]
struct ReadProf {
  index_steps: u64,
  index_io_ns: u64,
  index_cpu_ns: u64,
  data_io_ns: u64,
  data_seek_ns: u64,
  decode_ns: u64,
}

fn key_of(index: u64, key_size: usize) -> Vec<u8> {
  let mut key = vec![0u8; key_size];
  let offset = key_size - 8;
  key[offset..].copy_from_slice(&index.to_be_bytes());
  key
}

/// 确定性 xorshift64*：同一 seed 下与 bench 的随机序列形状一致。
struct Rng(u64);

impl Rng {
  fn new(seed: u64) -> Self {
    Self(seed.max(1))
  }

  fn next(&mut self) -> u64 {
    let mut x = self.0;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    self.0 = x;
    x.wrapping_mul(0x2545_f491_4f6c_dd1d)
  }
}

#[derive(Clone, Copy, Default)]
struct Run {
  ops_s: f64,
  cpu_ns: u64,
  snapshot_ns: u64,
  index_steps: u64,
  index_io_ns: u64,
  index_cpu_ns: u64,
  data_io_ns: u64,
  data_seek_ns: u64,
  decode_ns: u64,
  hit: u64,
  miss: u64,
  cache_capacity: usize,
  cache_bytes: usize,
  cache_len: usize,
  page_reads: u64,
}

/// 进程累计 CPU（user+sys，纳秒）。Linux 读 `/proc/self/stat`；其它平台返回 0。
fn cpu_ns() -> u64 {
  let Ok(stat) = std::fs::read_to_string("/proc/self/stat") else {
    return 0;
  };
  let Some((_, rest)) = stat.rsplit_once(") ") else {
    return 0;
  };
  // comm 之后的下标 11/12 是 utime/stime（1-based 的 14/15）。
  let fields: Vec<&str> = rest.split_whitespace().collect();
  let ticks: u64 = fields
    .get(11)
    .and_then(|value| value.parse().ok())
    .unwrap_or(0)
    + fields
      .get(12)
      .and_then(|value| value.parse().ok())
      .unwrap_or(0);
  // Linux USER_HZ 固定 100：1 tick = 10 ms。
  ticks * 10_000_000
}

async fn read_lane(
  store: &Store<CompioStore>,
  tbl: &TableIdent,
  rows: u64,
  reads: usize,
  seed: u64,
  key_size: usize,
) -> usize {
  let mut rng = Rng::new(seed);
  let mut observed = 0usize;
  for _ in 0..reads {
    let key = key_of(rng.next() % rows, key_size);
    let value = store.get(tbl, &key).await.unwrap();
    let value = value.expect("随机下标必须命中已建行的 key");
    observed = observed.wrapping_add(value.len());
  }
  observed
}

#[allow(clippy::too_many_arguments)]
fn measure(
  rt: &Runtime,
  dir: &std::path::Path,
  rows: u64,
  reads: usize,
  seed: u64,
  cache_bytes: usize,
  key_size: usize,
  direct_io: bool,
  lanes: usize,
  data_cache: bool,
) -> Run {
  let tbl = TableIdent::new("bench", 1).unwrap();
  let io = CompioStore::new(dir, PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT).with_direct_io(direct_io);
  let mut store = Store::new(io, PAGE_SIZE, 16)
    .with_page_cache_bytes(cache_bytes)
    .with_enable_data_page_cache(data_cache);
  rt.block_on(store.open_branch(&tbl, "main", 0)).unwrap();

  #[cfg(feature = "read-prof")]
  let _ = fkv_task_read::prof::take();
  let base_stat = store.page_cache_stat();
  let base_reads = store.page_io().read_at_num();
  let cpu_start = cpu_ns();
  let lanes = lanes.max(1);
  let base = reads / lanes;
  let rem = reads % lanes;
  let start = Instant::now();
  let observed: usize = rt.block_on(async {
    let lane_fut_li = (0..lanes).map(|lane| {
      read_lane(
        &store,
        &tbl,
        rows,
        base + usize::from(lane < rem),
        seed ^ (lane as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
        key_size,
      )
    });
    join_all(lane_fut_li).await.into_iter().sum()
  });
  let secs = start.elapsed().as_secs_f64();
  let cpu_ns = cpu_ns().saturating_sub(cpu_start);
  std::hint::black_box(observed);
  // 单独的取快照一轮：不与 get 混跑，避免重复取快照改变吞吐口径。
  let snap_start = Instant::now();
  for _ in 0..reads {
    let snapshot = store.snapshot(&tbl);
    std::hint::black_box(snapshot.is_some());
  }
  let snapshot_ns = u64::try_from(snap_start.elapsed().as_nanos()).unwrap_or(u64::MAX);

  let stat = store.page_cache_stat();
  let page_reads = store.page_io().read_at_num() - base_reads;
  #[cfg(feature = "read-prof")]
  let prof = fkv_task_read::prof::take();
  #[cfg(not(feature = "read-prof"))]
  let prof: ReadProf = Default::default();
  let result = Run {
    ops_s: reads as f64 / secs,
    cpu_ns,
    snapshot_ns,
    index_steps: prof.index_steps,
    index_io_ns: prof.index_io_ns,
    index_cpu_ns: prof.index_cpu_ns,
    data_io_ns: prof.data_io_ns,
    data_seek_ns: prof.data_seek_ns,
    decode_ns: prof.decode_ns,
    hit: stat.hit - base_stat.hit,
    miss: stat.miss - base_stat.miss,
    cache_capacity: stat.capacity,
    cache_bytes: stat.bytes,
    cache_len: stat.len,
    page_reads,
  };
  eprintln!(
    "run: ops/s={:.0} reads={reads} page_reads={} cache_hit_ratio={:.3}",
    result.ops_s,
    result.page_reads,
    result.hit as f64 / (result.hit + result.miss).max(1) as f64
  );
  eprintln!(
    "run: cpu_us_per_op={:.1}",
    result.cpu_ns as f64 / 1000.0 / reads as f64
  );
  result
}

fn median(values: &mut [f64]) -> f64 {
  values.sort_by(|a, b| a.partial_cmp(b).unwrap());
  values[values.len() / 2]
}

fn main() {
  let mut rows: u64 = 25_000_000;
  let mut reads: usize = 200_000;
  let mut samples: usize = 3;
  let mut cache_bytes: usize = 256 << 20;
  let mut key_size: usize = 16;
  let mut seed: u64 = 0x5eed_0071_c01d_0001;
  let mut dir = PathBuf::from("/tmp/fkv-perf-data/store");
  let mut direct_io = true;
  let mut lanes: usize = 1;
  let mut data_cache = false;
  let mut args = std::env::args().skip(1);
  while let Some(arg) = args.next() {
    let mut next = || args.next().expect("缺少参数值");
    match arg.as_str() {
      "--rows" => rows = next().parse().unwrap(),
      "--reads" => reads = next().parse().unwrap(),
      "--samples" => samples = next().parse().unwrap(),
      "--cache-bytes" => cache_bytes = next().parse().unwrap(),
      "--key-size" => key_size = next().parse().unwrap(),
      "--seed" => seed = next().parse().unwrap(),
      "--dir" => dir = PathBuf::from(next()),
      "--no-direct-io" => direct_io = false,
      "--lanes" => lanes = next().parse().unwrap(),
      "--data-cache" => data_cache = true,
      "--help" | "-h" => {
        println!(
          "用法: read_profile [选项]\n  --dir <dir>        已有 store 目录（由 fkv_bench --load-only 建）\n  --rows <n>         读随机下标范围（默认 25000000）\n  --reads <n>        每轮点读次数（默认 200000）\n  --samples <n>      轮数（默认 3）\n  --cache-bytes <n>  页缓存预算（默认 256 MiB）\n  --key-size <n>     key 宽度（默认 16）\n  --seed <n>         随机序列种子\n  --no-direct-io     关闭 O_DIRECT（仅本地调试）\n  --lanes <n>        同进程并发读链数（默认 1）\n  --data-cache       让 data 页也进页缓存（默认关闭，对齐 C++ 默认）"
        );
        std::process::exit(0);
      }
      other => panic!("未知参数 {other}"),
    }
  }

  let rt = Runtime::new().unwrap();
  let mut ops = Vec::with_capacity(samples);
  let mut last = Run::default();
  for _ in 0..samples {
    let run = measure(
      &rt,
      &dir,
      rows,
      reads,
      seed,
      cache_bytes,
      key_size,
      direct_io,
      lanes,
      data_cache,
    );
    ops.push(run.ops_s);
    last = run;
  }
  let ops_s = median(&mut ops);
  let n = reads as f64;
  println!("ops_s: {ops_s:.0}");
  println!("cpu_us_per_op: {:.1}", last.cpu_ns as f64 / 1000.0 / n);
  println!("snapshot_ns_per_op: {:.1}", last.snapshot_ns as f64 / n);
  println!("index_steps_per_op: {:.2}", last.index_steps as f64 / n);
  println!("index_io_ns_per_op: {:.1}", last.index_io_ns as f64 / n);
  println!("index_cpu_ns_per_op: {:.1}", last.index_cpu_ns as f64 / n);
  println!("data_io_ns_per_op: {:.1}", last.data_io_ns as f64 / n);
  println!("data_seek_ns_per_op: {:.1}", last.data_seek_ns as f64 / n);
  println!("decode_ns_per_op: {:.1}", last.decode_ns as f64 / n);
  println!(
    "cache_hit_ratio: {:.3}",
    last.hit as f64 / (last.hit + last.miss).max(1) as f64
  );
  println!(
    "cache_capacity: {} cache_bytes: {} cache_len: {}",
    last.cache_capacity, last.cache_bytes, last.cache_len
  );
  println!("page_reads_per_op: {:.3}", last.page_reads as f64 / n);
  println!(
    "profiled_ns_per_op: {:.1}",
    (last.index_io_ns + last.index_cpu_ns + last.data_io_ns + last.data_seek_ns + last.decode_ns)
      as f64
      / n
  );
  println!("wall_ns_per_op: {:.1}", 1e9 / ops_s);
}
