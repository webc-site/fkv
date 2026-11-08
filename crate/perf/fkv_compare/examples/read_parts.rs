//! 读路径开销拆解：把一次点读拆成「取快照」「点读整条」两段，分别计时，
//! 用来判断 CPU 时间到底花在哪个环节（快照分配 / 映射查找 / 页缓存 / 解码）。
//!
//! ```bash
//! cargo run --release -p fkv_compare --example read_parts -- /path/to/workdir/fkv 200000
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
  let rounds: u64 = args.next().expect("rounds").parse().expect("rounds");
  let rows: u64 = args
    .next()
    .map(|s| s.parse().expect("rows"))
    .unwrap_or(1_000_000);

  let option = KvOption::default();
  let page_size = option.data_page_size as usize;
  let rt = Runtime::new().unwrap();
  let io = CompioStore::new(root, page_size, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT);
  let mut store = Store::new(io, page_size, option.data_page_restart_interval.max(1));
  let tbl = TableIdent::new("bench", 1).unwrap();
  rt.block_on(store.open_branch(&tbl, "main", 0)).unwrap();

  let key_li: Vec<Vec<u8>> = (0..rounds)
    .map(|i| key_of(16, (i * 104729) % rows))
    .collect();

  // 预热：把页缓存 / 分支状态拉到稳定。
  for key in &key_li {
    rt.block_on(store.get(&tbl, key)).unwrap();
  }

  let start = Instant::now();
  for key in &key_li {
    let snapshot = store.snapshot(&tbl).expect("snapshot");
    std::hint::black_box(snapshot.root_id());
  }
  let snapshot_secs = start.elapsed().as_secs_f64();

  let start = Instant::now();
  let mut found = 0u64;
  for key in &key_li {
    if rt.block_on(store.get(&tbl, key)).unwrap().is_some() {
      found += 1;
    }
  }
  let get_secs = start.elapsed().as_secs_f64();

  println!("rounds={rounds} rows={rows} 命中={found}");
  println!("snapshot  {:.0} ns/次", snapshot_secs * 1e9 / rounds as f64);
  println!("get 整条  {:.0} ns/次", get_secs * 1e9 / rounds as f64);
}
