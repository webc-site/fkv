//! perf-61 复现程序：批量 upsert 造数据后，按 1000 条一批删除，测删除阶段吞吐。
//!
//! 口径对齐 `crate/perf/fkv_compare`：key 16 字节（末尾 8 字节大端下标），
//! value 116 字节，批大小 1000。默认 200 万行 / 删除 2 万条，可用参数覆盖。
//!
//! ```text
//! cargo run --release -p fkv_store --example delete_bench -- \
//!   --rows 2000000 --removals 20000 --batch-size 1000 --dir /tmp/fkv-del-bench
//! ```

use std::{path::PathBuf, time::Instant};

use compio::runtime::Runtime;
use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_store::Store;
use fkv_types::TableIdent;

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

fn main() {
  let mut rows: u64 = 2_000_000;
  let mut removals: u64 = 20_000;
  let mut batch_size: usize = 1000;
  let mut dir = PathBuf::from("/tmp/fkv-del-bench");
  let mut samples: usize = 3;
  // `--keep`：保留库目录（盘面对拍 / 事后分析用；默认跑完删除）。
  let mut keep = false;
  let mut args = std::env::args().skip(1);
  while let Some(arg) = args.next() {
    let mut next = || args.next().expect("缺少参数值");
    match arg.as_str() {
      "--rows" => rows = next().parse().unwrap(),
      "--removals" => removals = next().parse().unwrap(),
      "--batch-size" => batch_size = next().parse().unwrap(),
      "--samples" => samples = next().parse().unwrap(),
      "--dir" => dir = PathBuf::from(next()),
      "--keep" => keep = true,
      "--help" | "-h" => {
        println!(
          "用法: delete_bench [选项]\n  --rows <n>        建库行数\n  --batch-size <n>  每批条数\n  --samples <n>     采样轮数\n  --dir <dir>       工作目录\n  --keep            保留目录（做盘面对拍）"
        );
        std::process::exit(0);
      }
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
    let count = batch_size.min((rows - first) as usize) as u64;
    let batch: Vec<BatchEntry> = (0..count)
      .map(|i| BatchEntry::upsert(key_of(first + i), value.clone(), 1))
      .collect();
    rt.block_on(store.put(&tbl, &batch)).unwrap();
    first += count;
  }
  let bulk_secs = bulk_start.elapsed().as_secs_f64();
  eprintln!(
    "bulk: rows={rows} secs={bulk_secs:.3} ops/s={:.0} sync={} fsync={} manifest_append={}",
    rows as f64 / bulk_secs,
    store.page_io().data_sync_num(),
    store.page_io().data_fsync_num(),
    store.page_io().manifest_append_num(),
  );

  for sample in 0..samples {
    let before = (
      store.page_io().data_sync_num(),
      store.page_io().manifest_sync_num(),
    );
    let start = Instant::now();
    let mut batch_secs_li = Vec::new();
    let mut first = 0u64;
    while first < removals {
      let count = batch_size.min((removals - first) as usize) as u64;
      let batch: Vec<BatchEntry> = (0..count)
        .map(|i| BatchEntry::delete(key_of(first + i), 1))
        .collect();
      let batch_start = Instant::now();
      rt.block_on(store.put(&tbl, &batch)).unwrap();
      batch_secs_li.push(batch_start.elapsed().as_secs_f64() * 1e3);
      first += count;
    }
    let secs = start.elapsed().as_secs_f64();
    let after = (
      store.page_io().data_sync_num(),
      store.page_io().manifest_sync_num(),
    );
    batch_secs_li.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pick = |q: f64| {
      let idx = ((batch_secs_li.len() as f64 - 1.0) * q) as usize;
      batch_secs_li[idx]
    };
    eprintln!(
      "delete[{sample}]: removals={removals} secs={secs:.3} ops/s={:.0} \
       data_sync={} manifest_sync={} data_fsync={} batch_ms p50={:.2} p95={:.2} p99={:.2} max={:.2}",
      removals as f64 / secs,
      after.0 - before.0,
      after.1 - before.1,
      store.page_io().data_fsync_num(),
      pick(0.50),
      pick(0.95),
      pick(0.99),
      batch_secs_li.last().copied().unwrap_or_default(),
    );
    // 后续采样重建相同 key，保证每轮删除的页分布一致。
    if sample + 1 < samples {
      let mut first = 0u64;
      while first < removals {
        let count = batch_size.min((removals - first) as usize) as u64;
        let batch: Vec<BatchEntry> = (0..count)
          .map(|i| BatchEntry::upsert(key_of(first + i), value.clone(), 2 + sample as u64))
          .collect();
        rt.block_on(store.put(&tbl, &batch)).unwrap();
        first += count;
      }
    }
  }
  if !keep {
    let _ = std::fs::remove_dir_all(&root);
  } else {
    eprintln!("keep: {}", root.display());
  }
}
