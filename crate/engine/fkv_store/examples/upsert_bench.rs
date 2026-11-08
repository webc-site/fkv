//! perf-63 复现程序：批量加载 + 批量 upsert 吞吐。
//!
//! 口径对齐 `crate/perf/fkv_compare`：key 16 字节（末尾 8 字节大端下标），
//! value 100 字节（kv_size 116），页 4 KiB，缓存 256 MiB，批大小 1000。
//! 批量加载造 `--rows` 行后，按 `--batch-writes` 批 × `--batch-size` 条升序
//! upsert 测写吞吐；`--samples` 轮交替采样，每轮重建相同 key 区间（后续轮为
//! 覆盖写），最终打印中位数。
//!
//! `--mem` 用 `MemStore` 跑同一份驱动：没有 fsync / pwrite，用于隔离写路径
//! CPU（perf-63 的分阶段归因）。
//!
//! ```text
//! cargo run --release -p fkv_store --example upsert_bench -- \
//!   --rows 2000000 --batch-writes 100 --batch-size 1000 --samples 5 \
//!   --dir /tmp/fkv-upsert-bench
//! cargo run --release -p fkv_store --example upsert_bench -- \
//!   --rows 2000000 --batch-writes 100 --batch-size 1000 --samples 5 --mem
//! ```

use std::{path::PathBuf, time::Instant};

use fkv_btree::BatchEntry;
use fkv_io::{ManifestIo, PageIo};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_store::Store;
use fkv_types::TableIdent;

const KEY_INDEX_BYTES: usize = 8;
const KEY_SIZE: usize = 16;
const VALUE_SIZE: usize = 100;
const PAGE_SIZE: usize = 4096;
const CACHE_BYTES: usize = 256 << 20;

trait IoCounters {
  fn counters(&self) -> (u64, u64, u64);
}

impl IoCounters for CompioStore {
  fn counters(&self) -> (u64, u64, u64) {
    (
      self.data_sync_num(),
      self.manifest_sync_num(),
      self.write_at_num(),
    )
  }
}

impl IoCounters for MemStore {
  fn counters(&self) -> (u64, u64, u64) {
    (0, 0, 0)
  }
}

fn key_of(index: u64) -> Vec<u8> {
  let mut key = vec![0u8; KEY_SIZE];
  let offset = KEY_SIZE - KEY_INDEX_BYTES;
  for i in 0..KEY_INDEX_BYTES {
    key[offset + i] = ((index >> (8 * (KEY_INDEX_BYTES - 1 - i))) & 0xff) as u8;
  }
  key
}

fn median(values: &mut [f64]) -> f64 {
  values.sort_by(|a, b| a.partial_cmp(b).unwrap());
  values[values.len() / 2]
}

/// 一次 upsert 压测的规模参数（参数一多就容易传错位，收成一个结构体）。
struct RunShape {
  rows: u64,
  bulk_batch: usize,
  batch_writes: u64,
  batch_size: usize,
  samples: usize,
}

async fn drive<I: PageIo + ManifestIo + IoCounters>(
  store: &mut Store<I>,
  tbl: &TableIdent,
  shape: &RunShape,
  tag: &str,
) {
  let RunShape {
    rows,
    bulk_batch,
    batch_writes,
    batch_size,
    samples,
  } = *shape;
  let value = vec![b'v'; VALUE_SIZE];

  // ---- 批量加载 ----
  let bulk_start = Instant::now();
  let mut first = 0u64;
  while first < rows {
    let count = bulk_batch.min((rows - first) as usize) as u64;
    let batch: Vec<BatchEntry> = (0..count)
      .map(|i| BatchEntry::upsert(key_of(first + i), value.clone(), 1))
      .collect();
    store.put(tbl, &batch).await.unwrap();
    first += count;
  }
  let bulk_secs = bulk_start.elapsed().as_secs_f64();
  let (data_sync, manifest_sync, write_at) = store.page_io().counters();
  eprintln!(
    "bulk[{tag}]: rows={rows} secs={bulk_secs:.3} ops/s={:.0} data_sync={data_sync} \
     manifest_sync={manifest_sync} write_at={write_at}",
    rows as f64 / bulk_secs,
  );
  {
    let stat = store.page_cache_stat();
    eprintln!(
      "cache[{tag}]: hit={} miss={} evict={} len={} bytes={} capacity={}",
      stat.hit, stat.miss, stat.evict, stat.len, stat.bytes, stat.capacity
    );
  }
  // ---- 批量 upsert：每轮重放同一段 key（首轮为新增，后续为覆盖）----
  let mut ops_li = Vec::with_capacity(samples);
  for sample in 0..samples {
    let start = Instant::now();
    let base = rows;
    for b in 0..batch_writes {
      let first = base + b * batch_size as u64;
      let batch: Vec<BatchEntry> = (0..batch_size as u64)
        .map(|i| BatchEntry::upsert(key_of(first + i), value.clone(), 2 + sample as u64))
        .collect();
      store.put(tbl, &batch).await.unwrap();
    }
    let secs = start.elapsed().as_secs_f64();
    let ops = batch_writes * batch_size as u64;
    let ops_s = ops as f64 / secs;
    ops_li.push(ops_s);
    eprintln!(
      "batch[{tag}][{sample}]: ops={ops} secs={secs:.3} ops/s={ops_s:.0} \
       data_sync={} manifest_sync={} write_at={}",
      store.page_io().counters().0,
      store.page_io().counters().1,
      store.page_io().counters().2,
    );
  }
  if !ops_li.is_empty() {
    eprintln!("batch[{tag}] median: ops/s={:.0}", median(&mut ops_li));
  }
}

fn main() {
  let mut rows: u64 = 2_000_000;
  let mut bulk_batch: usize = 1000;
  let mut batch_writes: u64 = 100;
  let mut batch_size: usize = 1000;
  let mut dir = PathBuf::from("/tmp/fkv-upsert-bench");
  let mut samples: usize = 5;
  // `--keep`：保留库目录（盘面对拍用；默认跑完删除）。
  let mut keep = false;
  let mut mem = false;
  let mut args = std::env::args().skip(1);
  while let Some(arg) = args.next() {
    let mut next = || args.next().expect("缺少参数值");
    match arg.as_str() {
      "--rows" => rows = next().parse().unwrap(),
      "--bulk-batch" => bulk_batch = next().parse().unwrap(),
      "--batch-writes" => batch_writes = next().parse().unwrap(),
      "--batch-size" => batch_size = next().parse().unwrap(),
      "--samples" => samples = next().parse().unwrap(),
      "--dir" => dir = PathBuf::from(next()),
      "--keep" => keep = true,
      "--mem" => mem = true,
      "--help" | "-h" => {
        println!(
          "用法: upsert_bench [选项]\n  --rows <n>        建库行数\n  --batch-size <n>  每批条数\n  --samples <n>     采样轮数\n  --dir <dir>       工作目录\n  --keep            保留目录（做盘面对拍）"
        );
        std::process::exit(0);
      }
      other => panic!("未知参数 {other}"),
    }
  }
  let tbl = TableIdent::new("bench", 1).unwrap();

  if mem {
    let mut store =
      Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16).with_page_cache_bytes(CACHE_BYTES);
    let shape = RunShape {
      rows,
      bulk_batch,
      batch_writes,
      batch_size,
      samples,
    };
    pollster::block_on(drive(&mut store, &tbl, &shape, "mem"));
    return;
  }

  let root = if keep {
    dir.clone()
  } else {
    dir.join(format!("run-{}", std::process::id()))
  };
  let _ = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap();

  let rt = compio::runtime::Runtime::new().unwrap();
  let io = CompioStore::new(
    root.clone(),
    PAGE_SIZE,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  let mut store = Store::new(io, PAGE_SIZE, 16).with_page_cache_bytes(CACHE_BYTES);
  let shape = RunShape {
    rows,
    bulk_batch,
    batch_writes,
    batch_size,
    samples,
  };
  rt.block_on(drive(&mut store, &tbl, &shape, "disk"));

  if !keep {
    let _ = std::fs::remove_dir_all(&root);
  } else {
    eprintln!("keep: {}", root.display());
  }
}
