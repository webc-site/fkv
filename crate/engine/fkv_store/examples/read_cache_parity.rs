//! `enable_data_page_cache` on / off 的点读吞吐对比（数据页缓存的真实收益）。
//!
//! 造一个 200 万行库（key 16 B、value 100 B、4 KiB 页），再用同一串确定性
//! 随机下标做 20 万次点读，分别在开关 on / off 下各跑 `--samples` 轮：
//!
//! - `off`（对齐 C++ `KvOption::enable_data_page_cache=false`）：index 页命中
//!   页缓存，data 页每次直读盘；
//! - `on`：index 页与 data 页共用同一个页缓存池（对齐 C++ 单一 buffer pool）。
//!
//! 每轮打印 ops/s、`read_at_num` 增量（真实页读次数）与 `page_cache_stat`
//! 增量（hit / miss / len / bytes）。最后按模式取中位数并给出 off 相对 on 的
//! 吞吐跌幅。
//!
//! ```text
//! cargo run --release -p fkv_store --example read_cache_parity -- \
//!   --dir /tmp/fkv-read-parity --rows 2000000 --reads 200000 --samples 3
//! ```
//!
//! ## 读数口径（macOS）
//!
//! - `ops/s` 是单线程、单 Store 的端到端点读（`get`，含 index 下降 + data 页
//!   读取 + value 解压/物化），不是纯 IOPS。
//! - `read_at_num` 是 `CompioStore` 的页读 syscall 计数增量：命中页缓存的读
//!   不产生 syscall，因此它直接反映「有多少页真的从文件读」。
//! - macOS 无法在进程内清空统一缓冲缓存（需要 `sudo purge`），同一目录的
//!   后续轮次可能命中 OS 文件缓存。程序用「每轮新建 Store + 相同随机序列 +
//!   轮换 on/off 顺序 + 取中位数」缩小该偏差；绝对数字会随机器 / 文件缓存
//!   状态变化，**跨模式倍率**才是可复现的口径。
//! - 建库与读库分两个 Store；建库固定走默认（关 data 页缓存），不参与计时。

use std::{
  path::{Path, PathBuf},
  time::Instant,
};

use compio::runtime::Runtime;
use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_store::{PageCacheStat, Store};
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, TableIdent};

const KEY_INDEX_BYTES: usize = 8;
const KEY_SIZE: usize = 16;
const VALUE_SIZE: usize = 100;
const PAGE_SIZE: usize = 4096;
const DEFAULT_CACHE_BYTES: usize = 128 << 20;

fn key_of(index: u64) -> Vec<u8> {
  let mut key = vec![0u8; KEY_SIZE];
  let offset = KEY_SIZE - KEY_INDEX_BYTES;
  for i in 0..KEY_INDEX_BYTES {
    key[offset + i] = ((index >> (8 * (KEY_INDEX_BYTES - 1 - i))) & 0xff) as u8;
  }
  key
}

/// 确定性 xorshift64*：同一 seed 下 on / off 读到完全相同的 key 序列。
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

fn io(dir: &Path) -> CompioStore {
  CompioStore::new(dir, PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT)
}

fn build(rt: &Runtime, dir: &Path, rows: u64, batch_size: usize) {
  let _ = std::fs::remove_dir_all(dir);
  std::fs::create_dir_all(dir).unwrap();
  let tbl = TableIdent::new("bench", 1).unwrap();
  let mut store = Store::new(io(dir), PAGE_SIZE, 16);
  let value = vec![b'v'; VALUE_SIZE];
  let start = Instant::now();
  let mut first = 0u64;
  while first < rows {
    let count = batch_size.min((rows - first) as usize) as u64;
    let batch: Vec<BatchEntry> = (0..count)
      .map(|i| BatchEntry::upsert(key_of(first + i), value.clone(), 1))
      .collect();
    rt.block_on(store.put(&tbl, &batch)).unwrap();
    first += count;
  }
  let secs = start.elapsed().as_secs_f64();
  eprintln!(
    "build: rows={rows} secs={secs:.3} ops/s={:.0} dir={}",
    rows as f64 / secs,
    dir.display()
  );
}

#[derive(Clone, Copy)]
struct Run {
  ops_s: f64,
  page_reads: u64,
  hit: u64,
  miss: u64,
  len: usize,
  bytes: usize,
}

fn stat_delta(before: PageCacheStat, after: PageCacheStat) -> (u64, u64, usize, usize) {
  (
    after.hit - before.hit,
    after.miss - before.miss,
    after.len,
    after.bytes,
  )
}

fn measure(
  rt: &Runtime,
  dir: &Path,
  rows: u64,
  reads: usize,
  seed: u64,
  data_page_cache: bool,
  cache_bytes: usize,
) -> Run {
  let tbl = TableIdent::new("bench", 1).unwrap();
  let mut store = Store::new(io(dir), PAGE_SIZE, 16)
    .with_page_cache_bytes(cache_bytes)
    .with_enable_data_page_cache(data_page_cache);
  rt.block_on(store.open_branch(&tbl, "main", 0)).unwrap();

  let base_stat = store.page_cache_stat();
  let base_reads = store.page_io().read_at_num();
  let mut rng = Rng::new(seed);
  let mut observed = 0usize;
  let start = Instant::now();
  for _ in 0..reads {
    let index = rng.next() % rows;
    let key = key_of(index);
    let value = rt.block_on(store.get(&tbl, &key)).unwrap();
    let value = value.expect("随机下标必须命中已建行的 key");
    observed = observed.wrapping_add(value.len());
  }
  let secs = start.elapsed().as_secs_f64();
  std::hint::black_box(observed);

  let stat = store.page_cache_stat();
  let (hit, miss, len, bytes) = stat_delta(base_stat, stat);
  Run {
    ops_s: reads as f64 / secs,
    page_reads: store.page_io().read_at_num() - base_reads,
    hit,
    miss,
    len,
    bytes,
  }
}

fn median(values: &mut [f64]) -> f64 {
  values.sort_by(|a, b| a.partial_cmp(b).unwrap());
  values[values.len() / 2]
}

fn main() {
  let mut rows: u64 = 2_000_000;
  let mut reads: usize = 200_000;
  let mut samples: usize = 3;
  let mut batch_size: usize = 1000;
  let mut cache_bytes: usize = DEFAULT_CACHE_BYTES;
  let mut dir = PathBuf::from("/tmp/fkv-read-parity");
  let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
  let mut rebuild = false;
  let mut args = std::env::args().skip(1);
  while let Some(arg) = args.next() {
    let mut next = || args.next().expect("缺少参数值");
    match arg.as_str() {
      "--rows" => rows = next().parse().unwrap(),
      "--reads" => reads = next().parse().unwrap(),
      "--samples" => samples = next().parse().unwrap(),
      "--batch-size" => batch_size = next().parse().unwrap(),
      "--cache-bytes" => cache_bytes = next().parse().unwrap(),
      "--dir" => dir = PathBuf::from(next()),
      "--seed" => seed = next().parse().unwrap(),
      "--rebuild" => rebuild = true,
      "--help" | "-h" => {
        println!(
          "用法: read_cache_parity [选项]\n  --rows <n>         建库行数（默认 2000000）\n  --reads <n>        随机点读次数（默认 200000）\n  --samples <n>      每模式采样轮数（默认 3）\n  --batch-size <n>   建库批大小（默认 1000）\n  --cache-bytes <n>  页缓存预算（默认 128 MiB，对齐 C++ buffer_pool_size）\n  --dir <dir>        工作目录（默认 /tmp/fkv-read-parity）\n  --seed <n>         随机序列种子（默认固定值）\n  --rebuild          强制重建库"
        );
        std::process::exit(0);
      }
      other => panic!("未知参数 {other}"),
    }
  }

  let rt = Runtime::new().unwrap();
  let tbl = TableIdent::new("bench", 1).unwrap();
  let manifest_exists = dir.join(tbl.dir_name()).exists();
  if rebuild || !manifest_exists {
    build(&rt, &dir, rows, batch_size);
  } else {
    eprintln!("reuse: dir={}（--rebuild 可强制重建）", dir.display());
  }

  let mut on_ops = Vec::with_capacity(samples);
  let mut off_ops = Vec::with_capacity(samples);
  for sample in 0..samples {
    // 轮换顺序：抵消 macOS 文件缓存对后跑模式的单向偏置。
    let modes: [bool; 2] = if sample % 2 == 0 {
      [true, false]
    } else {
      [false, true]
    };
    for data_page_cache in modes {
      let tag = if data_page_cache { "on" } else { "off" };
      let run = measure(&rt, &dir, rows, reads, seed, data_page_cache, cache_bytes);
      println!(
        "mode={tag} sample={sample} rows={rows} reads={reads} ops_s={:.0} \
         page_reads={} cache_hit={} cache_miss={} cache_len={} cache_bytes={} capacity={}",
        run.ops_s, run.page_reads, run.hit, run.miss, run.len, run.bytes, cache_bytes,
      );
      if data_page_cache {
        on_ops.push(run.ops_s);
      } else {
        off_ops.push(run.ops_s);
      }
    }
  }

  let on = median(&mut on_ops);
  let off = median(&mut off_ops);
  println!(
    "summary rows={rows} reads={reads} cache_bytes={cache_bytes} \
     on_median_ops_s={on:.0} off_median_ops_s={off:.0} \
     off_throughput_drop_pct={:.1}",
    (on - off) / on * 100.0
  );
  println!(
    "note: macOS 统一缓冲缓存不可在进程内清空；数字是同机同文件缓存状态下的相对口径，\
     跨机器请只比较 off_throughput_drop_pct。"
  );
}
