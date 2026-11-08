//! 线程扩展性实验：同一 root 上开 N 个 Store、各自一个 compio runtime，
//! 每个线程跑同一批随机点读，看吞吐是否随线程数线性增长。
//!
//! 用途：判断「多线程点读不涨」是引擎瓶颈还是评测驱动的单线程限制。
//!
//! ```bash
//! cargo run --release -p fkv_compare --example threaded_read -- \
//!   /path/to/workdir/fkv 1000000 1,4,8,16
//! ```

use std::{path::PathBuf, time::Instant};

use compio::runtime::Runtime;
use fkv_config::KvOption;
use fkv_io_compio::CompioStore;
use fkv_store::Store;
use fkv_types::TableIdent;

const KEY_INDEX_BYTES: usize = 8;

fn key_of(key_size: usize, index: u64) -> Vec<u8> {
  let mut key = vec![0u8; key_size];
  let offset = key_size - KEY_INDEX_BYTES;
  for i in 0..KEY_INDEX_BYTES {
    key[offset + i] = ((index >> (8 * (KEY_INDEX_BYTES - 1 - i))) & 0xff) as u8;
  }
  key
}

fn main() {
  let mut args = std::env::args().skip(1);
  let root = PathBuf::from(args.next().expect("root"));
  let reads: u64 = args.next().expect("reads").parse().expect("reads");
  let thread_li: Vec<usize> = args
    .next()
    .unwrap_or_else(|| "1,4,8".to_owned())
    .split(',')
    .map(|s| s.trim().parse().expect("threads"))
    .collect();
  let rows: u64 = args
    .next()
    .map(|s| s.parse().expect("rows"))
    .unwrap_or(25_000_000);

  let option = KvOption::default();
  let page_size = option.data_page_size as usize;
  let key_size = 16usize;
  let restart = option.data_page_restart_interval.max(1);

  println!("root={:?} reads/线程={} rows={}", root, reads, rows);
  for threads in thread_li {
    let cache = 256 * 1024 * 1024 / threads;
    let start = Instant::now();
    let handle_li: Vec<_> = (0..threads)
      .map(|lane| {
        let root = root.clone();
        std::thread::spawn(move || -> Result<u64, String> {
          let rt = Runtime::new().map_err(|e| e.to_string())?;
          let io = CompioStore::new(root, page_size, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT);
          let mut store = Store::new(io, page_size, restart).with_page_cache_bytes(cache);
          let tbl = TableIdent::new("bench", 1).unwrap();
          rt.block_on(async move {
            // 新 Store 必须显式打开 branch，否则表 root 不存在，`get` 直接返回空。
            store
              .open_branch(&tbl, "main", 0)
              .await
              .map_err(|e| format!("open_branch: {e:?}"))?;
            for i in 0..reads {
              let idx = (lane as u64 * 7919 + i * 104729) % rows;
              store
                .get(&tbl, &key_of(key_size, idx))
                .await
                .map_err(|e| format!("{e:?}"))?;
            }
            Ok(reads)
          })
        })
      })
      .collect();
    let mut total = 0u64;
    for handle in handle_li {
      match handle.join().expect("join") {
        Ok(n) => total += n,
        Err(err) => {
          eprintln!("线程失败: {err}");
          return;
        }
      }
    }
    let secs = start.elapsed().as_secs_f64();
    println!(
      "threads={threads:>3} cache/线程={:>4} MiB  {total} reads in {secs:.3}s  {:.0} ops/s",
      cache / (1024 * 1024),
      total as f64 / secs
    );
  }
}
