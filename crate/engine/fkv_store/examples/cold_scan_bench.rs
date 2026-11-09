//! 冷缓存 `len()`（`scan_count`）复现程序：一个进程建库，另一个进程只
//! `open_branch` + `scan_count`，避免同进程建库留下的用户态页缓存干扰。
//!
//! 口径对齐 `crate/perf/fkv_compare`：key 16 B（末尾 8 B 大端下标）、value 100 B、
//! 4096 B 页。页缓存预算默认 64 MiB，库远大于它时整表计数就是冷缓存读。
//!
//! ```text
//! # 建库（2M 行，约 270 MiB 数据）
//! cargo run --release -p fkv_store --example cold_scan_bench -- \
//!   --build --dir /tmp/fkv-cold-len --rows 2000000
//!
//! # 新进程只 open + scan_count（第 1 次用户态页缓存冷，后续是同进程暖读）
//! cargo run --release -p fkv_store --example cold_scan_bench -- \
//!   --dir /tmp/fkv-cold-len --samples 3 --cache-bytes 67108864
//! ```
//!
//! **本机读缓存状态**：macOS 的统一缓冲缓存无法在进程内清空（需要
//! `sudo purge`），所以第 1 次采样是「用户态页缓存冷、OS 文件缓存可能已热」，
//! 后续采样是同进程暖缓存。对比改动前后时两次运行都保持同一进程 / 同一
//! 文件缓存状态即可，绝对 ns/页 会受 OS 缓存影响，相对倍率才可跨机器比较。

use std::{path::PathBuf, time::Instant};

use fkv_btree::BatchEntry;
use fkv_store::Store;
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, TableIdent};

const KEY_INDEX_BYTES: usize = 8;
const KEY_SIZE: usize = 16;
const VALUE_SIZE: usize = 100;
const PAGE_SIZE: usize = 4096;

fn key_of(index: u64) -> Vec<u8> {
  let mut key = vec![0u8; KEY_SIZE];
  let offset = KEY_SIZE - KEY_INDEX_BYTES;
  for i in 0..KEY_INDEX_BYTES {
    key[offset + i] = ((index >> (8 * (KEY_INDEX_BYTES - 1 - i))) & 0xff) as u8;
  }
  key
}

fn io(root: PathBuf) -> fkv_io_compio::CompioStore {
  fkv_io_compio::CompioStore::new(root, PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT)
}

fn main() {
  let mut rows: u64 = 2_000_000;
  let mut batch_size: usize = 1000;
  let mut dir = PathBuf::from("/tmp/fkv-cold-len");
  let mut samples: usize = 3;
  let mut cache_bytes: usize = 64 << 20;
  let mut build = false;
  let mut args = std::env::args().skip(1);
  while let Some(arg) = args.next() {
    let mut next = || args.next().expect("缺少参数值");
    match arg.as_str() {
      "--rows" => rows = next().parse().unwrap(),
      "--batch-size" => batch_size = next().parse().unwrap(),
      "--dir" => dir = PathBuf::from(next()),
      "--samples" => samples = next().parse().unwrap(),
      "--cache-bytes" => cache_bytes = next().parse().unwrap(),
      "--build" => build = true,
      "--help" | "-h" => {
        println!(
          "用法: cold_scan_bench [选项]\n  --build           建库后退出（会清空 --dir）\n  --rows <n>        建库行数（默认 2000000）\n  --batch-size <n>  建库批大小（默认 1000）\n  --dir <dir>       工作目录\n  --samples <n>     scan_count 采样次数（默认 3）\n  --cache-bytes <n> 页缓存预算（默认 64 MiB）"
        );
        std::process::exit(0);
      }
      other => panic!("未知参数 {other}"),
    }
  }

  let tbl = TableIdent::new("bench", 1).unwrap();
  let rt = compio::runtime::Runtime::new().unwrap();

  if build {
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut store = Store::new(io(dir.clone()), PAGE_SIZE, 16).with_page_cache_bytes(cache_bytes);
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
    let pages = store.mapper(&tbl).map_or(0, |mapper| mapper.tbl().len());
    eprintln!(
      "build: rows={rows} secs={secs:.3} ops/s={:.0} mapping_pages={pages} dir={}",
      rows as f64 / secs,
      dir.display()
    );
    return;
  }

  let mut store = Store::new(io(dir.clone()), PAGE_SIZE, 16).with_page_cache_bytes(cache_bytes);
  rt.block_on(store.open_branch(&tbl, "main", 0)).unwrap();
  let pages = store.mapper(&tbl).map_or(0, |mapper| mapper.tbl().len());
  eprintln!(
    "open: dir={} mapping_pages={pages} cache_bytes={cache_bytes}",
    dir.display()
  );
  for sample in 0..samples {
    let start = Instant::now();
    let count = rt
      .block_on(store.scan_count(&tbl, b"", b""))
      .expect("scan_count");
    let secs = start.elapsed().as_secs_f64();
    let ns_per_page = if pages == 0 {
      0.0
    } else {
      secs * 1e9 / pages as f64
    };
    eprintln!(
      "scan_count[{sample}]: rows={count} pages={pages} secs={secs:.6} \
       ops/s={:.0} ns/page={ns_per_page:.0}",
      count as f64 / secs
    );
  }
}
