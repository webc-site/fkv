//! perf-64 复现程序：造库后逐条 put（每批 1 条），测单条写入吞吐与固定开销。
//!
//! 口径对齐 `crate/perf/fkv_compare` 的 `individual_writes`：key 16 字节
//! （末尾 8 字节大端下标），value 116 字节，逐条 `store.put`（每批 1 条），
//! 每条都走两次同步（data + manifest）。造库用 1000 条一批。
//!
//! 除吞吐外打印「两次 fsync 之外」的可观测成本：页写 / manifest 追加与
//! 字节增量、fd 打开与目录 fsync 次数、用户态 / 内核态 CPU 时间
//! （`getrusage(RUSAGE_SELF)`，不引入额外依赖）。
//!
//! ```text
//! cargo run --release -p fkv_store --example single_put_bench -- \
//!   --rows 2000000 --puts 1000 --samples 3 --dir /tmp/fkv-single-put-bench
//! ```
//!
//! `--put-batch N` 保持「每批 N 条、每批仍只同步一次」，把单条成本拆成
//! 「每批固定成本 + 每条边际成本」：固定成本随 N 摊薄，边际成本不变。
//! `--keep` 保留库目录，便于盘面对拍（同一输入跑两次后 `cmp -r`）。
//! `--sleep-before-puts <secs>` 用于外部采样器（`sample <pid>`）就位。

use std::{path::PathBuf, time::Instant};

use compio::runtime::Runtime;
use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_store::Store;
use fkv_types::TableIdent;

const KEY_INDEX_BYTES: usize = 8;
const KEY_SIZE: usize = 16;
const VALUE_SIZE: usize = 116;
const PAGE_SIZE: usize = 4096;

fn key_of(index: u64) -> Vec<u8> {
  let mut key = vec![0u8; KEY_SIZE];
  let offset = KEY_SIZE - KEY_INDEX_BYTES;
  for i in 0..KEY_INDEX_BYTES {
    key[offset + i] = ((index >> (8 * (KEY_INDEX_BYTES - 1 - i))) & 0xff) as u8;
  }
  key
}

#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Timeval {
  tv_sec: i64,
  tv_usec: i64,
}

#[cfg(not(target_os = "linux"))]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Timeval {
  tv_sec: i64,
  tv_usec: i32,
  _pad: i32,
}

/// `struct rusage` 前缀：只需要 `ru_utime` / `ru_stime`，其余用足够大的
/// 尾部占位（macOS 144B / Linux 144B，160B 缓冲不会越界）。
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Rusage {
  utime: Timeval,
  stime: Timeval,
  _rest: [u64; 16],
}

const RUSAGE_SELF: i32 = 0;

unsafe extern "C" {
  fn getrusage(who: i32, usage: *mut Rusage) -> i32;
}

/// 进程累计 (用户态, 内核态) CPU 秒。
fn cpu_secs() -> (f64, f64) {
  // SAFETY: `getrusage` 只写入调用方提供的 `rusage`，不保留指针。
  unsafe {
    let mut usage = Rusage::default();
    if getrusage(RUSAGE_SELF, &mut usage) != 0 {
      return (0.0, 0.0);
    }
    let secs = |tv: Timeval| tv.tv_sec as f64 + tv.tv_usec as f64 / 1e6;
    (secs(usage.utime), secs(usage.stime))
  }
}

/// 分区目录下 manifest 文件总字节数（manifest 记录的实际盘面增量）。
fn manifest_bytes(root: &std::path::Path) -> u64 {
  fn walk(dir: &std::path::Path, sum: &mut u64) {
    let Ok(entry_li) = std::fs::read_dir(dir) else {
      return;
    };
    for entry in entry_li.flatten() {
      let path = entry.path();
      let Ok(meta) = entry.metadata() else {
        continue;
      };
      if meta.is_dir() {
        walk(&path, sum);
      } else if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("manifest"))
      {
        *sum += meta.len();
      }
    }
  }
  let mut sum = 0;
  walk(root, &mut sum);
  sum
}

fn median(mut value_li: Vec<f64>) -> f64 {
  value_li.sort_by(|a, b| a.partial_cmp(b).unwrap());
  if value_li.is_empty() {
    return 0.0;
  }
  value_li[value_li.len() / 2]
}

fn main() {
  let mut rows: u64 = 2_000_000;
  let mut puts: u64 = 1_000;
  let mut put_batch: usize = 1;
  let mut bulk_batch: usize = 1_000;
  let mut samples: usize = 3;
  let mut dir = PathBuf::from("/tmp/fkv-single-put-bench");
  let mut keep = false;
  let mut sleep_before_puts = 0u64;
  let mut args = std::env::args().skip(1);
  while let Some(arg) = args.next() {
    let mut next = || args.next().expect("缺少参数值");
    match arg.as_str() {
      "--rows" => rows = next().parse().unwrap(),
      "--puts" => puts = next().parse().unwrap(),
      "--put-batch" => put_batch = next().parse().unwrap(),
      "--bulk-batch" => bulk_batch = next().parse().unwrap(),
      "--samples" => samples = next().parse().unwrap(),
      "--dir" => dir = PathBuf::from(next()),
      "--sleep-before-puts" => sleep_before_puts = next().parse().unwrap(),
      "--keep" => keep = true,
      other => panic!("未知参数 {other}"),
    }
  }
  let root = if keep {
    dir.clone()
  } else {
    dir.join(format!("run-{}", std::process::id()))
  };
  let _ = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap();

  let rt = Runtime::new().unwrap();
  let io = CompioStore::new(
    root.clone(),
    PAGE_SIZE,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  let mut store = Store::new(io, PAGE_SIZE, 16).with_page_cache_bytes(256 << 20);
  let tbl = TableIdent::new("bench", 1).unwrap();

  let value = vec![b'v'; VALUE_SIZE];
  let bulk_start = Instant::now();
  let mut first = 0u64;
  while first < rows {
    let count = bulk_batch.min((rows - first) as usize) as u64;
    let batch: Vec<BatchEntry> = (0..count)
      .map(|i| BatchEntry::upsert(key_of(first + i), value.clone(), 1))
      .collect();
    rt.block_on(store.put(&tbl, &batch)).unwrap();
    first += count;
  }
  let bulk_secs = bulk_start.elapsed().as_secs_f64();
  eprintln!(
    "bulk: rows={rows} secs={bulk_secs:.3} ops/s={:.0} data_sync={} fsync={} \
     manifest_append={} manifest_sync={} manifest_bytes={}",
    rows as f64 / bulk_secs,
    store.page_io().data_sync_num(),
    store.page_io().data_fsync_num(),
    store.page_io().manifest_append_num(),
    store.page_io().manifest_sync_num(),
    manifest_bytes(&root),
  );

  if sleep_before_puts > 0 {
    eprintln!("pid={} sleeping {}", std::process::id(), sleep_before_puts);
    std::thread::sleep(std::time::Duration::from_secs(sleep_before_puts));
  }
  eprintln!(
    "pid={} starting puts (attach sampler now)",
    std::process::id()
  );

  let mut ops_per_sec_li = Vec::new();
  for sample in 0..samples {
    let io_before = (
      store.page_io().data_sync_num(),
      store.page_io().manifest_sync_num(),
      store.page_io().manifest_append_num(),
      store.page_io().write_at_num(),
      store.page_io().fd_open_num(),
      store.page_io().dir_fsync_num(),
      store.page_io().data_fsync_num(),
    );
    let manifest_before = manifest_bytes(&root);
    let (user_before, sys_before) = cpu_secs();
    let start = Instant::now();
    let mut wrote = 0u64;
    let mut next_key = rows + (sample as u64) * puts;
    while wrote < puts {
      let count = (put_batch as u64).min(puts - wrote);
      let batch: Vec<BatchEntry> = (0..count)
        .map(|i| BatchEntry::upsert(key_of(next_key + i), value.clone(), 1))
        .collect();
      rt.block_on(store.put(&tbl, &batch)).unwrap();
      next_key += count;
      wrote += count;
    }
    let secs = start.elapsed().as_secs_f64();
    let (user_after, sys_after) = cpu_secs();
    let io_after = (
      store.page_io().data_sync_num(),
      store.page_io().manifest_sync_num(),
      store.page_io().manifest_append_num(),
      store.page_io().write_at_num(),
      store.page_io().fd_open_num(),
      store.page_io().dir_fsync_num(),
      store.page_io().data_fsync_num(),
    );
    let manifest_after = manifest_bytes(&root);
    let ops = puts as f64 / secs;
    ops_per_sec_li.push(ops);
    let batches = puts.div_ceil(put_batch as u64);
    eprintln!(
      "put[{sample}]: puts={puts} batch={put_batch} batches={batches} secs={secs:.3} \
       ops/s={ops:.0} ms/op={:.3} ms/batch={:.3} data_sync={} manifest_sync={} \
       manifest_append={} manifest_bytes_delta={} write_at={} fd_open={} \
       dir_fsync={} data_fsync={} user_cpu_ms={:.1} sys_cpu_ms={:.1} \
       user_us_per_batch={:.1} cpu_ms_per_op={:.3}",
      secs * 1e3 / puts as f64,
      secs * 1e3 / batches as f64,
      io_after.0 - io_before.0,
      io_after.1 - io_before.1,
      io_after.2 - io_before.2,
      manifest_after.saturating_sub(manifest_before),
      io_after.3 - io_before.3,
      io_after.4 - io_before.4,
      io_after.5 - io_before.5,
      io_after.6 - io_before.6,
      (user_after - user_before) * 1e3,
      (sys_after - sys_before) * 1e3,
      (user_after - user_before) * 1e6 / batches as f64,
      (user_after - user_before + sys_after - sys_before) * 1e3 / puts as f64,
    );
  }
  eprintln!(
    "summary: puts={puts} samples={samples} ops/s median={:.0} min={:.0} max={:.0}",
    median(ops_per_sec_li.clone()),
    ops_per_sec_li.iter().copied().fold(f64::INFINITY, f64::min),
    ops_per_sec_li.iter().copied().fold(0.0, f64::max),
  );
  if keep {
    eprintln!("keep: {}", root.display());
  } else {
    let _ = std::fs::remove_dir_all(&root);
  }
}
