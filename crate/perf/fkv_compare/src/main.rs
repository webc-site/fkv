//! fkv / RocksDB / fjall 三方对比评测。
//!
//! 三方共用同一份阶段驱动与同一套 key 编码（末尾 8 字节大端下标），缓存额度
//! 与持久化口径见 `doc/perf-compare.md`。RocksDB 通过 `cpp/rocksdb_ffi.cpp`
//! 的 C ABI 以 dlopen 加载，避免把 C++ 依赖塞进 Rust 构建。

use std::{
  ffi::{CStr, CString, c_char, c_int, c_void},
  path::{Path, PathBuf},
  process::ExitCode,
  time::Instant,
};

use compio::runtime::Runtime;
use fkv_btree::BatchEntry;
use fkv_config::KvOption;
use fkv_io_compio::CompioStore;
use fkv_store::{PageOption, ScanOption, Store};
use fkv_types::TableIdent;
use futures::stream::{FuturesUnordered, StreamExt};
use serde::Serialize;

const KEY_INDEX_BYTES: usize = 8;

/// 线程模式下单个读者线程的在飞窗口默认值。窗口让一次点读的「排队 / 提交 /
/// 等待」流水起来，而不是 1 线程 1 IO 串行等待。
const READ_WINDOW_DEFAULT: usize = 16;
/// 页缓存总量固定时允许并行的最大读者线程数。超过它的读者档位只增加排队，
/// 不再切碎缓存：实测 4 线程后吞吐已饱和，而 32 份缓存只剩 8 MiB、热页互踢。
const READ_MAX_CONCURRENCY: usize = 4;

#[derive(Debug, Clone)]
struct Cfg {
  workdir: PathBuf,
  rows: u64,
  bulk_rows: u64,
  batch_size: usize,
  kv_size: usize,
  key_size: usize,
  cache_bytes: u64,
  individual_writes: u64,
  batch_writes: u64,
  removals: u64,
  reads: u64,
  scans: u64,
  scan_len: usize,
  readers: Vec<usize>,
  rocksdb_lib: Option<PathBuf>,
  /// 是否强制 RocksDB / fjall 也逐批 fsync。**默认关闭**：对照列用 RocksDB 的
  /// 默认配置（`WriteOptions.sync=false`），不为了对齐去改它的默认行为。fkv 侧
  /// 的同步无法关闭（与 C++ EloqStore 相同），所以写阶段的差异必须在页面上写明
  /// 是持久化强度差异，而不是实现差异。打开它只用于分析「同样的同步强度下谁快」。
  align_fsync: bool,
  /// 进程内直连驱动下，点读是否每读者一个 OS 线程（缓存平分）；关闭时退化为
  /// 单 runtime 上 N 条异步链交错。多 shard 运行时驱动不看这个开关。
  fkv_read_threads: bool,
  /// 线程模式下每个读者线程保持的在飞点读请求数（窗口）。默认 16；与
  /// `--no-fkv-read-threads` 的单 runtime 多路交错路径无关。
  read_window: usize,
  /// 固定缓存预算下允许并行的最大读线程数（默认 4）。调大只在本机核数更多时
  /// 有意义；CI 的 4 vCPU 口径用默认值。
  read_max_concurrency: usize,
  /// 复用 `--workdir` 里已有的库：跳过 reset 与全部写/删/计数阶段，只跑点读与
  /// 范围读。
  /// 默认关闭，CI 的「从头造库」口径不受影响；打开后可对同一份数据集反复调参。
  reuse_data: bool,
  /// fkv 驱动：store（进程内直连，默认）/ runtime（多 shard 部署形态）。
  fkv_driver: String,
  /// fkv 的 shard 线程数（`RuntimeConfig::num_threads`）：一个进程 N 个 shard、
  /// 共享一份缓存预算，请求按 key 路由。0 表示跟随 `readers` 的最大值。
  fkv_shards: usize,
  /// `--json` 指定的输出文件；放在 Cfg 里而不是在解析到该参数时立刻跑，
  /// 否则写在它后面的参数（例如 `--rocksdb-lib`）会被静默忽略。
  json_out: Option<PathBuf>,
}

impl Cfg {
  fn value_size(&self) -> usize {
    self.kv_size - self.key_size
  }
}

fn key_of(key_size: usize, index: u64) -> Vec<u8> {
  let mut key = vec![0u8; key_size];
  let offset = key_size - KEY_INDEX_BYTES;
  for i in 0..KEY_INDEX_BYTES {
    key[offset + i] = ((index >> (8 * (KEY_INDEX_BYTES - 1 - i))) & 0xff) as u8;
  }
  key
}

fn value_of(cfg: &Cfg) -> Vec<u8> {
  vec![b'v'; cfg.value_size()]
}

#[derive(Debug, Clone, Copy, Serialize)]
struct Stat {
  ops: u64,
  secs: f64,
}

#[derive(Debug, Serialize)]
struct Report {
  config: CfgSummary,
  phases: Vec<PhaseRow>,
  sizes: SizeRow,
}

#[derive(Debug, Serialize)]
struct CfgSummary {
  rows: u64,
  bulk_rows: u64,
  batch_size: usize,
  kv_size: usize,
  key_size: usize,
  cache_bytes: u64,
  /// 线程模式的在飞窗口（`--read-window`）。
  read_window: usize,
  /// 固定缓存预算下允许并行读的最大线程数（超过的读者档位只排队）。
  read_max_concurrency: usize,
  individual_writes: u64,
  batch_writes: u64,
  removals: u64,
  reads: u64,
  scans: u64,
  scan_len: usize,
  readers: Vec<usize>,
}

/// 点读阶段实际采用的并发形态。请求的读者档位与真正开出的线程数分开记录：
/// `readers` 超过 `read_max_concurrency` 时只增加排队，缓存总预算保持一份。
#[derive(Debug, Clone, Serialize)]
struct ReadPlan {
  /// `--readers` 里本阶段的档位。
  readers: usize,
  /// 实际开出的 fkv 读线程数（单 runtime 交错路径为 1）。
  threads: usize,
  /// 每个线程保持的在飞请求数（交错路径为 1）。
  window: usize,
  /// 每线程页缓存字节数；`threads * thread_cache_bytes <= cache_bytes`，
  /// 整除余数补给 0 号线程，总预算不随读者档位变化。
  thread_cache_bytes: u64,
  /// 并发形态：threads / interleaved / runtime。
  mode: String,
}

#[derive(Debug, Serialize)]
struct PhaseRow {
  phase: String,
  fkv: Option<Stat>,
  rocksdb: Option<Stat>,
  fjall: Option<Stat>,
  /// 仅点读阶段记录 fkv 侧实际线程数 / 窗口 / 每线程缓存，便于复核口径。
  fkv_read: Option<ReadPlan>,
}

#[derive(Debug, Serialize)]
struct SizeRow {
  fkv: Option<u64>,
  rocksdb: Option<u64>,
  fjall: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
  BulkLoad,
  IndividualWrites,
  BatchWrites,
  Len,
  RandomReads(usize),
  RangeReads,
  Removals,
}

impl Phase {
  fn label(&self) -> String {
    match self {
      Phase::BulkLoad => "bulk_load".to_owned(),
      Phase::IndividualWrites => "individual_writes".to_owned(),
      Phase::BatchWrites => "batch_writes".to_owned(),
      Phase::Len => "len".to_owned(),
      Phase::RandomReads(readers) => format!("random_reads_{readers}"),
      Phase::RangeReads => "range_reads".to_owned(),
      Phase::Removals => "removals".to_owned(),
    }
  }
}

fn phase_li(readers: &[usize]) -> Vec<Phase> {
  let mut out = vec![
    Phase::BulkLoad,
    Phase::IndividualWrites,
    Phase::BatchWrites,
    Phase::Len,
  ];
  out.extend(readers.iter().map(|r| Phase::RandomReads(*r)));
  out.push(Phase::RangeReads);
  out.push(Phase::Removals);
  out
}

/// 点读阶段实际并发口径。与 `FkvEngine::phase` 的线程/窗口选择保持一致，
/// 保证 JSON 里记录的是真正跑出来的线程数与窗口，而不是请求的参数。
fn read_plan(cfg: &Cfg, readers: usize) -> ReadPlan {
  if cfg.fkv_driver == "runtime" {
    // 多 shard 运行时：页缓存由 shard 共享，客户端读者数就是请求线程数。
    return ReadPlan {
      readers,
      threads: readers,
      window: 1,
      thread_cache_bytes: 0,
      mode: "runtime".to_owned(),
    };
  }
  if cfg.fkv_read_threads && readers > 1 {
    let threads = readers.min(cfg.read_max_concurrency.max(1));
    ReadPlan {
      readers,
      threads,
      window: cfg.read_window.max(1),
      thread_cache_bytes: cfg.cache_bytes / threads as u64,
      mode: "threads".to_owned(),
    }
  } else {
    // 单 runtime 多路交错：一个 Store 独占整份缓存，窗口固定为 1。
    ReadPlan {
      readers,
      threads: 1,
      window: 1,
      thread_cache_bytes: cfg.cache_bytes,
      mode: "interleaved".to_owned(),
    }
  }
}

fn make_batch(cfg: &Cfg, first: u64, count: u64, delete: bool) -> Vec<BatchEntry> {
  let value = value_of(cfg);
  (0..count)
    .map(|i| {
      let key = key_of(cfg.key_size, first + i);
      if delete {
        BatchEntry::delete(key, 1)
      } else {
        BatchEntry::upsert(key, value.clone(), 1)
      }
    })
    .collect()
}

trait Engine {
  fn phase(&mut self, phase: Phase, cfg: &Cfg) -> Result<Stat, String>;
  fn size(&mut self) -> Result<u64, String>;
}

// ---------------------------------------------------------------------------
// fkv
// ---------------------------------------------------------------------------

/// 进程内直连：单个 `Store`，与 C++ 侧 `Store` 对象同层（不经请求队列）。
struct FkvEngine {
  root: PathBuf,
  store: Store<CompioStore>,
  /// compio 运行时：`CompioStore` 的 IO 必须在这个运行时里驱动。
  rt: Runtime,
  tbl: TableIdent,
  page_size: usize,
}

impl FkvEngine {
  fn new(root: PathBuf, cfg: &Cfg) -> Self {
    let option = KvOption::default();
    let page_size = option.data_page_size as usize;
    let rt = Runtime::new().expect("compio runtime");
    let io = CompioStore::new(
      root.clone(),
      page_size,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    // 缓存额度与 RocksDB 列同口径（同一份 cache_bytes 直接作为页缓存预算）。
    let store = Store::new(io, page_size, option.data_page_restart_interval.max(1))
      .with_page_cache_bytes(cfg.cache_bytes as usize);
    Self {
      root,
      store,
      rt,
      tbl: TableIdent::new("bench", 1).unwrap(),
      page_size,
    }
  }
}

impl Engine for FkvEngine {
  fn phase(&mut self, phase: Phase, cfg: &Cfg) -> Result<Stat, String> {
    let tbl = self.tbl.clone();
    let page_size = self.page_size;
    let _ = page_size;
    // 拆开字段借用：`rt` 与 `store` 互不重叠，才能在 runtime 里驱动 store。
    let FkvEngine { store, rt, .. } = self;
    match phase {
      Phase::BulkLoad => {
        let start = Instant::now();
        let mut first = 0u64;
        while first < cfg.bulk_rows {
          let count = cfg.batch_size.min((cfg.bulk_rows - first) as usize) as u64;
          let batch = make_batch(cfg, first, count, false);
          rt.block_on(store.put(&tbl, &batch))
            .map_err(|e| format!("put: {e:?}"))?;
          first += count;
        }
        Ok(Stat {
          ops: cfg.bulk_rows,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::IndividualWrites => {
        let start = Instant::now();
        for i in 0..cfg.individual_writes {
          let batch = make_batch(cfg, cfg.rows + i, 1, false);
          rt.block_on(store.put(&tbl, &batch))
            .map_err(|e| format!("put: {e:?}"))?;
        }
        Ok(Stat {
          ops: cfg.individual_writes,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::BatchWrites => {
        let start = Instant::now();
        for b in 0..cfg.batch_writes {
          let first = cfg.rows + b * cfg.batch_size as u64;
          let batch = make_batch(cfg, first, cfg.batch_size as u64, false);
          rt.block_on(store.put(&tbl, &batch))
            .map_err(|e| format!("put: {e:?}"))?;
        }
        Ok(Stat {
          ops: cfg.batch_writes * cfg.batch_size as u64,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::Len => {
        let start = Instant::now();
        let count = rt
          .block_on(store.scan_count(&tbl, b"", b""))
          .map_err(|e| format!("scan_count: {e:?}"))?;
        Ok(Stat {
          ops: count as u64,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::RandomReads(readers) => {
        // 线程模型对齐：每读者一个 OS 线程、每线程一个 Store（Store 是单线程
        // `!Sync`，多线程读只能各开一个），但两处口径按 perf-60 收紧：
        // 1) 缓存总预算固定。实际线程数 = min(readers, read_max_concurrency)，
        //    默认上限 4：超出的读者档位只增加排队；每线程缓存 = cache_bytes /
        //    threads，不再按请求的 readers 切碎（32 读者 = 每份 8 MiB 会互踢）。
        // 2) 每线程保持 `--read-window` 个在飞请求（FuturesUnordered 完成一个
        //    补一个），而不是 1 线程 1 IO 串行等待。
        // 单 runtime 多路交错路径（`--no-fkv-read-threads`）保持不变，见下。
        if cfg.fkv_read_threads && readers > 1 {
          let root = self.root.clone();
          let threads = readers.min(cfg.read_max_concurrency.max(1));
          let per_lane = cfg.reads.div_ceil(threads as u64);
          let cache_base = cfg.cache_bytes / threads as u64;
          let cache_rem = cfg.cache_bytes % threads as u64;
          let window = cfg.read_window.max(1);
          let start = Instant::now();
          let handle_li: Vec<_> = (0..threads)
            .map(|lane| {
              let root = root.clone();
              // 整除余数补给 0 号线程：threads 份缓存之和恰为 cache_bytes。
              // 只有 cache_bytes < threads 的极端档位才会因每线程至少 1 字节而略超。
              let cache = (cache_base + if lane == 0 { cache_rem } else { 0 }).max(1) as usize;
              let key_size = cfg.key_size;
              let rows = cfg.rows;
              std::thread::spawn(move || -> Result<u64, String> {
                let rt = Runtime::new().map_err(|e| format!("compio runtime: {e}"))?;
                let option = KvOption::default();
                let page_size = option.data_page_size as usize;
                let restart = option.data_page_restart_interval.max(1);
                let io = CompioStore::new(root, page_size, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT);
                let mut store = Store::new(io, page_size, restart).with_page_cache_bytes(cache);
                let tbl = TableIdent::new("bench", 1).unwrap();
                rt.block_on(async move {
                  store
                    .open_branch(&tbl, "main", 0)
                    .await
                    .map_err(|e| format!("open_branch: {e:?}"))?;
                  let store_ref = &store;
                  let tbl_ref = &tbl;
                  // 窗口化：预填 window 个请求，之后每完成一个再补一个。
                  let mut inflight = FuturesUnordered::new();
                  let mut next = 0u64;
                  let mut done = 0u64;
                  loop {
                    while next < per_lane && inflight.len() < window {
                      let idx = (lane as u64 * 7919 + next * 104729) % rows;
                      next += 1;
                      let key = key_of(key_size, idx);
                      inflight.push(async move {
                        store_ref
                          .get(tbl_ref, &key)
                          .await
                          .map(|_| ())
                          .map_err(|e| format!("get: {e:?}"))
                      });
                    }
                    if inflight.is_empty() {
                      break;
                    }
                    match inflight.next().await {
                      Some(Ok(())) => done += 1,
                      Some(Err(err)) => return Err(err),
                      None => break,
                    }
                  }
                  Ok(done)
                })
              })
            })
            .collect();
          let mut total = 0u64;
          for handle in handle_li {
            total += handle.join().map_err(|_| "读线程 panic".to_owned())??;
          }
          return Ok(Stat {
            ops: total,
            secs: start.elapsed().as_secs_f64(),
          });
        }
        // 单 runtime、多路交错：并发来自流水线深度而非线程，窗口固定为 1。
        let per_lane = cfg.reads.div_ceil(readers as u64);
        let start = Instant::now();
        let done = rt.block_on(async {
          // 复用已有库（`--reuse-data`）时没有走过 bulk_load，这里显式打开
          // branch；全新库上重复打开是幂等的。
          store
            .open_branch(&tbl, "main", 0)
            .await
            .map_err(|e| format!("open_branch: {e:?}"))?;
          let lane_li: Vec<Vec<Vec<u8>>> = (0..readers)
            .map(|lane| {
              (0..per_lane)
                .map(|i| {
                  let idx = (lane as u64 * 7919 + i * 104729) % cfg.rows;
                  key_of(cfg.key_size, idx)
                })
                .collect()
            })
            .collect();
          let store_ref: &Store<CompioStore> = store;
          let tbl_ref = &tbl;
          let fut_li = lane_li.into_iter().map(|keys| async move {
            let mut n = 0u64;
            for key in keys {
              store_ref
                .get(tbl_ref, &key)
                .await
                .map_err(|e| format!("get: {e:?}"))?;
              n += 1;
            }
            Ok::<u64, String>(n)
          });
          let mut total = 0u64;
          for result in futures::future::join_all(fut_li).await {
            total += result?;
          }
          Ok::<u64, String>(total)
        })?;
        Ok(Stat {
          ops: done,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::RangeReads => {
        let start = Instant::now();
        let mut rng: u64 = 0x2545F4914F6CDD1D;
        for _ in 0..cfg.scans {
          rng ^= rng << 13;
          rng ^= rng >> 7;
          rng ^= rng << 17;
          let begin = key_of(cfg.key_size, rng % cfg.rows);
          let end = key_of(cfg.key_size, (rng % cfg.rows) + cfg.scan_len as u64);
          rt.block_on(store.scan_page(
            &tbl,
            &begin,
            &end,
            ScanOption::default(),
            PageOption {
              page_entries: cfg.scan_len,
              page_size: 0,
            },
          ))
          .map_err(|e| format!("scan_page: {e:?}"))?;
        }
        Ok(Stat {
          ops: cfg.scans,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::Removals => {
        let start = Instant::now();
        let mut first = 0u64;
        while first < cfg.removals {
          let count = cfg.batch_size.min((cfg.removals - first) as usize) as u64;
          let batch = make_batch(cfg, first, count, true);
          rt.block_on(store.put(&tbl, &batch))
            .map_err(|e| format!("delete: {e:?}"))?;
          first += count;
        }
        Ok(Stat {
          ops: cfg.removals,
          secs: start.elapsed().as_secs_f64(),
        })
      }
    }
  }

  fn size(&mut self) -> Result<u64, String> {
    dir_bytes(&self.root)
  }
}

struct FkvRuntimeEngine {
  root: PathBuf,
  /// 生产路径的多 shard 运行时：`num_threads` 个 shard 线程、每 shard 一个 store，
  /// 请求按 key 路由。与 C++ EloqStore 的 `num_threads` 同构，也是唯一能拿
  /// 「一份缓存 + 真并发」的形态（`Store` 本身是单线程 `!Sync`，多线程读只能
  /// 各开一个 store，等于把缓存切开）。
  rt: fkv_runtime::Runtime,
  /// 客户端驱动：shard 侧有自己的 runtime，这里只用来 block_on 请求 future。
  drive: Runtime,
  tbl: TableIdent,
}

impl FkvRuntimeEngine {
  fn new(root: PathBuf, cfg: &Cfg) -> Result<Self, String> {
    let shards = cfg.fkv_shards.max(1);
    let mut option = KvOption::default();
    // 与 C++ 侧 `buffer_pool_size` 同语义：每 shard 一份预算，总量对齐 cache_bytes。
    option.page_cache_bytes = (cfg.cache_bytes / shards as u64).max(1);
    let runtime_cfg = fkv_runtime::RuntimeConfig {
      num_threads: shards as u16,
      root: root.clone(),
      ..Default::default()
    };
    let rt = fkv_runtime::Runtime::start_with_option(runtime_cfg, &option)
      .map_err(|e| format!("fkv runtime start: {e:?}"))?;
    let drive = Runtime::new().map_err(|e| format!("compio runtime: {e}"))?;
    // 每个 shard 必须显式 attach 到 (branch, term) 才能接数据面请求；生产路径由
    // facade 的 open 流程做，这里照做一遍。
    for shard in 0..shards as u32 {
      drive
        .block_on(rt.attach_shard(shard, "main", 0))
        .map_err(|e| format!("attach shard {shard}: {e:?}"))?;
    }
    Ok(Self {
      root,
      rt,
      drive,
      tbl: TableIdent::new("bench", 1).unwrap(),
    })
  }
}

impl Engine for FkvRuntimeEngine {
  fn phase(&mut self, phase: Phase, cfg: &Cfg) -> Result<Stat, String> {
    let tbl = self.tbl.clone();
    let drive = &self.drive;
    let rt = &self.rt;
    match phase {
      Phase::BulkLoad => {
        let start = Instant::now();
        let mut first = 0u64;
        while first < cfg.bulk_rows {
          let count = cfg.batch_size.min((cfg.bulk_rows - first) as usize) as u64;
          let batch = make_batch(cfg, first, count, false);
          drive
            .block_on(rt.put(&tbl, batch))
            .map_err(|e| format!("put: {e:?}"))?;
          first += count;
        }
        Ok(Stat {
          ops: cfg.bulk_rows,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::IndividualWrites => {
        let start = Instant::now();
        for i in 0..cfg.individual_writes {
          let batch = make_batch(cfg, cfg.rows + i, 1, false);
          drive
            .block_on(rt.put(&tbl, batch))
            .map_err(|e| format!("put: {e:?}"))?;
        }
        Ok(Stat {
          ops: cfg.individual_writes,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::BatchWrites => {
        let start = Instant::now();
        for b in 0..cfg.batch_writes {
          let first = cfg.rows + b * cfg.batch_size as u64;
          let batch = make_batch(cfg, first, cfg.batch_size as u64, false);
          drive
            .block_on(rt.put(&tbl, batch))
            .map_err(|e| format!("put: {e:?}"))?;
        }
        Ok(Stat {
          ops: cfg.batch_writes * cfg.batch_size as u64,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::Len => {
        // 没有 O(1) 计数接口：分页扫一遍、只数条数（与 C++ 全表扫描计数同义）。
        let start = Instant::now();
        let mut total = 0u64;
        let mut begin = Vec::new();
        loop {
          let reply = drive
            .block_on(rt.scan_page(
              &tbl,
              &begin,
              b"",
              fkv_api::ScanPageOption {
                begin_inclusive: begin.is_empty(),
                end_inclusive: false,
                page_entries: 100_000,
                page_size: 0,
              },
            ))
            .map_err(|e| format!("scan_page: {e:?}"))?;
          let n = reply.entry_li.len() as u64;
          total += n;
          if !reply.has_remaining || n == 0 {
            break;
          }
          begin = reply
            .entry_li
            .last()
            .map(|(k, _)| k.clone())
            .unwrap_or_default();
        }
        Ok(Stat {
          ops: total,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::RandomReads(readers) => {
        let per_lane = cfg.reads.div_ceil(readers as u64);
        let start = Instant::now();
        // 客户端并发：`readers` 个 OS 线程向同一个运行时提交请求（shard 侧并行），
        // 缓存仍是完整一份，不会被读者数切碎。
        let total = std::thread::scope(|scope| -> Result<u64, String> {
          let mut handle_li = Vec::with_capacity(readers);
          for lane in 0..readers {
            let key_size = cfg.key_size;
            let rows = cfg.rows;
            let tbl = tbl.clone();
            handle_li.push(scope.spawn(move || -> Result<u64, String> {
              // compio Runtime 是 !Send，只能在线程内部创建。
              let drive = Runtime::new().map_err(|e| format!("compio runtime: {e}"))?;
              for i in 0..per_lane {
                let idx = (lane as u64 * 7919 + i * 104729) % rows;
                drive
                  .block_on(rt.get(&tbl, &key_of(key_size, idx)))
                  .map_err(|e| format!("get: {e:?}"))?;
              }
              Ok(per_lane)
            }));
          }
          let mut total = 0u64;
          for handle in handle_li {
            total += handle.join().map_err(|_| "读线程 panic".to_owned())??;
          }
          Ok(total)
        })?;
        Ok(Stat {
          ops: total,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::RangeReads => {
        let start = Instant::now();
        let mut rng: u64 = 0x2545F4914F6CDD1D;
        for _ in 0..cfg.scans {
          rng ^= rng << 13;
          rng ^= rng >> 7;
          rng ^= rng << 17;
          let begin = key_of(cfg.key_size, rng % cfg.rows);
          let end = key_of(cfg.key_size, (rng % cfg.rows) + cfg.scan_len as u64);
          drive
            .block_on(rt.scan_page(
              &tbl,
              &begin,
              &end,
              fkv_api::ScanPageOption {
                begin_inclusive: true,
                end_inclusive: false,
                page_entries: cfg.scan_len,
                page_size: 0,
              },
            ))
            .map_err(|e| format!("scan_page: {e:?}"))?;
        }
        Ok(Stat {
          ops: cfg.scans,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::Removals => {
        let start = Instant::now();
        let mut first = 0u64;
        while first < cfg.removals {
          let count = cfg.batch_size.min((cfg.removals - first) as usize) as u64;
          let batch = make_batch(cfg, first, count, true);
          drive
            .block_on(rt.put(&tbl, batch))
            .map_err(|e| format!("put: {e:?}"))?;
          first += count;
        }
        Ok(Stat {
          ops: cfg.removals,
          secs: start.elapsed().as_secs_f64(),
        })
      }
    }
  }

  fn size(&mut self) -> Result<u64, String> {
    dir_bytes(&self.root)
  }
}

// ---------------------------------------------------------------------------
// RocksDB（dlopen 同一 C ABI）
// ---------------------------------------------------------------------------

#[repr(C)]
struct RocksConfig {
  rows: u32,
  bulk_rows: u32,
  batch_size: u32,
  kv_size: u32,
  partitions: u32,
  key_size: u32,
  num_threads: u32,
  buffer_pool_size: u64,
  individual_writes: u32,
  batch_writes: u32,
  removals: u32,
  reads: u32,
  range_scans: u32,
  range_len: u32,
}

#[repr(C)]
#[derive(Default)]
struct RocksResult {
  ops: u64,
  secs: f64,
  p99_us: f64,
}

struct RocksApi {
  open: unsafe extern "C" fn(*const c_char, *const RocksConfig) -> *mut c_void,
  close: unsafe extern "C" fn(*mut c_void),
  run: unsafe extern "C" fn(*mut c_void, *const c_char, u32, f64, *mut RocksResult) -> c_int,
  last_error: unsafe extern "C" fn(*mut c_void) -> *const c_char,
}

fn dl_err() -> String {
  unsafe {
    let err = libc::dlerror();
    if err.is_null() {
      "unknown".to_owned()
    } else {
      CStr::from_ptr(err).to_string_lossy().into_owned()
    }
  }
}

impl RocksApi {
  fn load(path: &Path) -> Result<Self, String> {
    let cpath = CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    let lib = unsafe { libc::dlopen(cpath.as_ptr(), libc::RTLD_NOW) };
    if lib.is_null() {
      return Err(format!("dlopen {}: {}", path.display(), dl_err()));
    }
    unsafe fn sym<T>(lib: *mut c_void, name: &str) -> Result<T, String> {
      let cname = CString::new(name).unwrap();
      let ptr = unsafe { libc::dlsym(lib, cname.as_ptr()) };
      if ptr.is_null() {
        return Err(format!("dlsym {name} 失败"));
      }
      Ok(unsafe { std::mem::transmute_copy(&ptr) })
    }
    unsafe {
      let abi: unsafe extern "C" fn() -> u32 = sym(lib, "fkv_bench_abi_version")?;
      if abi() != 2 {
        return Err("rocksdb FFI ABI 版本不是 2".to_owned());
      }
      Ok(Self {
        open: sym(lib, "fkv_bench_open")?,
        close: sym(lib, "fkv_bench_close")?,
        run: sym(lib, "fkv_bench_run")?,
        last_error: sym(lib, "fkv_bench_last_error")?,
      })
    }
  }
}

struct RocksEngine {
  api: RocksApi,
  handle: *mut c_void,
  root: PathBuf,
}

impl RocksEngine {
  fn new(root: PathBuf, cfg: &Cfg) -> Result<Self, String> {
    let lib = cfg
      .rocksdb_lib
      .as_ref()
      .ok_or_else(|| "缺少 --rocksdb-lib".to_owned())?;
    let api = RocksApi::load(lib)?;
    let croot = CString::new(root.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    let raw = RocksConfig {
      rows: cfg.rows as u32,
      bulk_rows: cfg.bulk_rows as u32,
      batch_size: cfg.batch_size as u32,
      kv_size: cfg.kv_size as u32,
      partitions: 1,
      key_size: cfg.key_size as u32,
      num_threads: 1,
      buffer_pool_size: cfg.cache_bytes,
      individual_writes: cfg.individual_writes as u32,
      batch_writes: cfg.batch_writes as u32,
      removals: cfg.removals as u32,
      reads: cfg.reads as u32,
      range_scans: cfg.scans as u32,
      range_len: cfg.scan_len as u32,
    };
    let handle = unsafe { (api.open)(croot.as_ptr(), &raw) };
    if handle.is_null() {
      let msg = unsafe { CStr::from_ptr((api.last_error)(std::ptr::null_mut())) };
      return Err(format!("rocksdb open: {}", msg.to_string_lossy()));
    }
    Ok(Self { api, handle, root })
  }
}

impl Engine for RocksEngine {
  fn phase(&mut self, phase: Phase, _cfg: &Cfg) -> Result<Stat, String> {
    let (name, readers) = match &phase {
      Phase::BulkLoad => ("bulk_load", 1u32),
      Phase::IndividualWrites => ("individual_writes", 1),
      Phase::BatchWrites => ("batch_writes", 1),
      Phase::Len => ("len", 1),
      Phase::RandomReads(n) => ("random_reads", *n as u32),
      Phase::RangeReads => ("range_reads", 1),
      Phase::Removals => ("removals", 1),
    };
    let cname = CString::new(name).unwrap();
    let mut out = RocksResult::default();
    let rc = unsafe { (self.api.run)(self.handle, cname.as_ptr(), readers, 0.0, &mut out) };
    if rc != 0 {
      let msg = unsafe { CStr::from_ptr((self.api.last_error)(self.handle)) };
      return Err(format!("rocksdb {name}: {}", msg.to_string_lossy()));
    }
    Ok(Stat {
      ops: out.ops,
      secs: out.secs,
    })
  }

  fn size(&mut self) -> Result<u64, String> {
    dir_bytes(&self.root)
  }
}

impl Drop for RocksEngine {
  fn drop(&mut self) {
    unsafe { (self.api.close)(self.handle) };
  }
}

// ---------------------------------------------------------------------------
// fjall
// ---------------------------------------------------------------------------

struct FjallEngine {
  db: fjall::Database,
  keyspace: fjall::Keyspace,
  root: PathBuf,
}

impl FjallEngine {
  fn new(root: PathBuf, cfg: &Cfg) -> Result<Self, String> {
    let db = fjall::Database::builder(root.clone())
      .cache_size(cfg.cache_bytes)
      .open()
      .map_err(|e| format!("fjall open: {e}"))?;
    let keyspace = db
      .keyspace("bench", fjall::KeyspaceCreateOptions::default)
      .map_err(|e| format!("fjall keyspace: {e}"))?;
    Ok(Self { db, keyspace, root })
  }
}

impl Engine for FjallEngine {
  fn phase(&mut self, phase: Phase, cfg: &Cfg) -> Result<Stat, String> {
    let value = value_of(cfg);
    match phase {
      Phase::BulkLoad => {
        let start = Instant::now();
        let mut first = 0u64;
        while first < cfg.bulk_rows {
          let count = cfg.batch_size.min((cfg.bulk_rows - first) as usize) as u64;
          let mut batch = self.db.batch();
          if cfg.align_fsync {
            batch = batch.durability(Some(fjall::PersistMode::SyncAll));
          }
          for i in 0..count {
            batch.insert(
              &self.keyspace,
              key_of(cfg.key_size, first + i),
              value.clone(),
            );
          }
          batch.commit().map_err(|e| format!("fjall insert: {e}"))?;
          first += count;
        }
        Ok(Stat {
          ops: cfg.bulk_rows,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::IndividualWrites => {
        let start = Instant::now();
        for i in 0..cfg.individual_writes {
          let key = key_of(cfg.key_size, cfg.rows + i);
          if cfg.align_fsync {
            // `keyspace.insert` 内部 batch 用默认 Buffer 持久化；对齐口径下必须
            // 显式走带 SyncAll 的 batch，否则这一列根本没 fsync。
            let mut batch = self.db.batch();
            batch = batch.durability(Some(fjall::PersistMode::SyncAll));
            batch.insert(&self.keyspace, key, value.clone());
            batch.commit().map_err(|e| format!("fjall insert: {e}"))?;
          } else {
            self
              .keyspace
              .insert(key, value.clone())
              .map_err(|e| format!("fjall insert: {e}"))?;
          }
        }
        Ok(Stat {
          ops: cfg.individual_writes,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::BatchWrites => {
        let start = Instant::now();
        for b in 0..cfg.batch_writes {
          let first = cfg.rows + b * cfg.batch_size as u64;
          let mut batch = self.db.batch();
          if cfg.align_fsync {
            batch = batch.durability(Some(fjall::PersistMode::SyncAll));
          }
          for i in 0..cfg.batch_size as u64 {
            batch.insert(
              &self.keyspace,
              key_of(cfg.key_size, first + i),
              value.clone(),
            );
          }
          batch.commit().map_err(|e| format!("fjall insert: {e}"))?;
        }
        Ok(Stat {
          ops: cfg.batch_writes * cfg.batch_size as u64,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::Len => {
        let start = Instant::now();
        let count = self.keyspace.len().map_err(|e| format!("fjall len: {e}"))?;
        Ok(Stat {
          ops: count as u64,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::RandomReads(_) => {
        let start = Instant::now();
        let mut rng: u64 = 0x9E3779B97F4A7C15;
        for _ in 0..cfg.reads {
          rng ^= rng << 13;
          rng ^= rng >> 7;
          rng ^= rng << 17;
          let _ = self
            .keyspace
            .get(key_of(cfg.key_size, rng % cfg.rows))
            .map_err(|e| format!("fjall get: {e}"))?;
        }
        Ok(Stat {
          ops: cfg.reads,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::RangeReads => {
        let start = Instant::now();
        let mut rng: u64 = 0x2545F4914F6CDD1D;
        for _ in 0..cfg.scans {
          rng ^= rng << 13;
          rng ^= rng >> 7;
          rng ^= rng << 17;
          let begin = key_of(cfg.key_size, rng % cfg.rows);
          for item in self.keyspace.range(begin..).take(cfg.scan_len) {
            // `into_inner` 一次解出 key/value（Guard 的 key()/value() 都消费 self）。
            let (key, value) = item.into_inner().map_err(|e| format!("fjall range: {e}"))?;
            let _ = key.len() + value.len();
          }
        }
        Ok(Stat {
          ops: cfg.scans,
          secs: start.elapsed().as_secs_f64(),
        })
      }
      Phase::Removals => {
        let start = Instant::now();
        let mut first = 0u64;
        while first < cfg.removals {
          let count = cfg.batch_size.min((cfg.removals - first) as usize) as u64;
          let mut batch = self.db.batch();
          if cfg.align_fsync {
            batch = batch.durability(Some(fjall::PersistMode::SyncAll));
          }
          for i in 0..count {
            batch.remove(&self.keyspace, key_of(cfg.key_size, first + i));
          }
          batch.commit().map_err(|e| format!("fjall remove: {e}"))?;
          first += count;
        }
        Ok(Stat {
          ops: cfg.removals,
          secs: start.elapsed().as_secs_f64(),
        })
      }
    }
  }

  fn size(&mut self) -> Result<u64, String> {
    dir_bytes(&self.root)
  }
}

fn dir_bytes(path: &Path) -> Result<u64, String> {
  let mut total = 0u64;
  let mut stack = vec![path.to_path_buf()];
  while let Some(dir) = stack.pop() {
    let entries = match std::fs::read_dir(&dir) {
      Ok(entries) => entries,
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
      Err(err) => return Err(format!("read_dir {}: {err}", dir.display())),
    };
    for entry in entries.flatten() {
      let path = entry.path();
      match entry.metadata() {
        Ok(meta) if meta.is_dir() => stack.push(path),
        Ok(meta) => total += meta.len(),
        Err(_) => {}
      }
    }
  }
  Ok(total)
}

fn reset_dir(path: &Path) -> Result<(), String> {
  let _ = std::fs::remove_dir_all(path);
  std::fs::create_dir_all(path).map_err(|e| format!("create {}: {e}", path.display()))
}

fn run(cfg: &Cfg) -> Result<Report, String> {
  let mut report = Report {
    config: CfgSummary {
      rows: cfg.rows,
      bulk_rows: cfg.bulk_rows,
      batch_size: cfg.batch_size,
      kv_size: cfg.kv_size,
      key_size: cfg.key_size,
      cache_bytes: cfg.cache_bytes,
      read_window: cfg.read_window,
      read_max_concurrency: cfg.read_max_concurrency,
      individual_writes: cfg.individual_writes,
      batch_writes: cfg.batch_writes,
      removals: cfg.removals,
      reads: cfg.reads,
      scans: cfg.scans,
      scan_len: cfg.scan_len,
      readers: cfg.readers.clone(),
    },
    phases: Vec::new(),
    sizes: SizeRow {
      fkv: None,
      rocksdb: None,
      fjall: None,
    },
  };

  // 每个引擎只开一个库，按阶段顺序在同一个库上推进：bulk load 造数据，
  // 后续读 / scan / 删都作用在同一份数据集上（与 C++ 侧单进程多阶段一致）。
  let fkv_root = cfg.workdir.join("fkv");
  let fjall_root = cfg.workdir.join("fjall");
  if cfg.reuse_data {
    // 复用模式：目录必须已存在，且刻意不删除任何数据。
    std::fs::create_dir_all(&fkv_root)
      .map_err(|e| format!("create {}: {e}", fkv_root.display()))?;
    std::fs::create_dir_all(&fjall_root)
      .map_err(|e| format!("create {}: {e}", fjall_root.display()))?;
  } else {
    reset_dir(&fkv_root)?;
    reset_dir(&fjall_root)?;
  }
  let mut fkv: Box<dyn Engine> = if cfg.fkv_driver == "runtime" {
    Box::new(FkvRuntimeEngine::new(fkv_root, cfg)?)
  } else {
    Box::new(FkvEngine::new(fkv_root, cfg))
  };
  let mut rocks = match &cfg.rocksdb_lib {
    Some(_) => {
      let rocks_root = cfg.workdir.join("rocks");
      if cfg.reuse_data {
        std::fs::create_dir_all(&rocks_root)
          .map_err(|e| format!("create {}: {e}", rocks_root.display()))?;
      } else {
        reset_dir(&rocks_root)?;
      }
      Some(RocksEngine::new(rocks_root, cfg)?)
    }
    None => None,
  };
  let mut fjall = FjallEngine::new(fjall_root, cfg)?;

  // 复用模式下没有 bulk_load 阶段，体积在开库后立即量一次，报告结构不变。
  if cfg.reuse_data {
    report.sizes = SizeRow {
      fkv: Some(fkv.size()?),
      rocksdb: match rocks.as_mut() {
        Some(rocks) => Some(rocks.size()?),
        None => None,
      },
      fjall: Some(fjall.size()?),
    };
  }

  for phase in phase_li(&cfg.readers) {
    // 复用模式严格只读：数据已经由上一次从头造库的运行写好，bulk_load / 写 /
    // 删除会改数据集，len 会整表扫描，都不是读路径调参的目标。
    if cfg.reuse_data
      && matches!(
        phase,
        Phase::BulkLoad
          | Phase::IndividualWrites
          | Phase::BatchWrites
          | Phase::Len
          | Phase::Removals
      )
    {
      continue;
    }
    eprintln!("[compare] {} ...", phase.label());
    let fkv_stat = fkv.phase(phase.clone(), cfg)?;
    let rocks_stat = match rocks.as_mut() {
      Some(rocks) => Some(rocks.phase(phase.clone(), cfg)?),
      None => None,
    };
    let fjall_stat = fjall.phase(phase.clone(), cfg)?;

    if matches!(phase, Phase::BulkLoad) {
      report.sizes = SizeRow {
        fkv: Some(fkv.size()?),
        rocksdb: match rocks.as_mut() {
          Some(rocks) => Some(rocks.size()?),
          None => None,
        },
        fjall: Some(fjall.size()?),
      };
    }
    let fkv_read = match &phase {
      Phase::RandomReads(readers) => Some(read_plan(cfg, *readers)),
      _ => None,
    };
    report.phases.push(PhaseRow {
      phase: phase.label(),
      fkv: Some(fkv_stat),
      rocksdb: rocks_stat,
      fjall: Some(fjall_stat),
      fkv_read,
    });
  }
  Ok(report)
}

fn parse_args() -> Result<Cfg, String> {
  let mut cfg = Cfg {
    workdir: PathBuf::from("/tmp/fkv-compare"),
    rows: 25_000_000,
    bulk_rows: 25_000_000,
    batch_size: 1000,
    kv_size: 116,
    key_size: 16,
    cache_bytes: 256 << 20,
    individual_writes: 1000,
    batch_writes: 100,
    removals: 500_000,
    reads: 1_000_000,
    scans: 500_000,
    scan_len: 10,
    readers: vec![1, 4, 8, 16, 32],
    rocksdb_lib: None,
    align_fsync: false,
    fkv_read_threads: true,
    read_window: READ_WINDOW_DEFAULT,
    read_max_concurrency: READ_MAX_CONCURRENCY,
    reuse_data: false,
    fkv_driver: "store".to_owned(),
    fkv_shards: 4,
    json_out: None,
  };
  let mut args = std::env::args().skip(1);
  while let Some(arg) = args.next() {
    match arg.as_str() {
      "--help" | "-h" => {
        println!(
          "用法: fkv_compare [选项]
  --workdir <dir>          工作目录
  --rows <n>               读/scan/len 数据集行数
  --bulk-rows <n>          bulk load 行数
  --batch-size <n>         每批条数
  --kv-size <n>            整对宽度（key+value）
  --key-size <n>           key 宽度
  --cache-bytes <n>        每引擎缓存额度
  --individual-writes <n>  单条写入次数
  --batch-writes <n>       批量写批数
  --removals <n>           删除条数
  --reads <n>              点读总数
  --scans <n>              范围读请求数
  --scan-len <n>           每次范围读条数
  --readers <a,b,...>      点读并发档位
  --rocksdb-lib <so>       RocksDB C ABI 适配器
  --align-fsync            强制 RocksDB / fjall 也逐批 fsync（默认关闭：对照列用
                           RocksDB 默认配置，不逐次 fsync）
  --fkv-shards <n>         fkv shard 线程数（默认 4，与评测机 vCPU 对齐）
  --fkv-driver <name>      fkv 驱动：store（进程内直连，默认）/ runtime（多 shard）
  --read-window <k>        线程模式每读者在飞点读请求数（默认 {}）
  --read-max-concurrency <n> 固定缓存预算下最大读线程数（默认 {}）
  --no-fkv-read-threads    store 驱动下点读退化到单 runtime 多路交错
  --reuse-data             复用 workdir 已有库，跳过 reset 与全部写/删阶段（调参用）
  --json <path>            输出 JSON",
          READ_WINDOW_DEFAULT, READ_MAX_CONCURRENCY
        );
        std::process::exit(0);
      }
      "--workdir" => cfg.workdir = PathBuf::from(next(&mut args, &arg)?),
      "--rows" => cfg.rows = parse_num(&arg, next(&mut args, &arg)?)?,
      "--bulk-rows" => cfg.bulk_rows = parse_num(&arg, next(&mut args, &arg)?)?,
      "--batch-size" => cfg.batch_size = parse_num(&arg, next(&mut args, &arg)?)?,
      "--kv-size" => cfg.kv_size = parse_num(&arg, next(&mut args, &arg)?)?,
      "--key-size" => cfg.key_size = parse_num(&arg, next(&mut args, &arg)?)?,
      "--cache-bytes" => cfg.cache_bytes = parse_num(&arg, next(&mut args, &arg)?)?,
      "--individual-writes" => cfg.individual_writes = parse_num(&arg, next(&mut args, &arg)?)?,
      "--batch-writes" => cfg.batch_writes = parse_num(&arg, next(&mut args, &arg)?)?,
      "--removals" => cfg.removals = parse_num(&arg, next(&mut args, &arg)?)?,
      "--reads" => cfg.reads = parse_num(&arg, next(&mut args, &arg)?)?,
      "--scans" => cfg.scans = parse_num(&arg, next(&mut args, &arg)?)?,
      "--scan-len" => cfg.scan_len = parse_num(&arg, next(&mut args, &arg)?)?,
      "--readers" => {
        cfg.readers = next(&mut args, &arg)?
          .split(',')
          .map(|s| s.trim().parse::<usize>().map_err(|e| format!("{e}")))
          .collect::<Result<Vec<_>, _>>()?
      }
      "--rocksdb-lib" => cfg.rocksdb_lib = Some(PathBuf::from(next(&mut args, &arg)?)),
      "--align-fsync" => cfg.align_fsync = true,
      "--no-align-fsync" => cfg.align_fsync = false,
      "--fkv-shards" => cfg.fkv_shards = parse_num(&arg, next(&mut args, &arg)?)?,
      "--fkv-driver" => cfg.fkv_driver = next(&mut args, &arg)?,
      "--read-window" => cfg.read_window = parse_num(&arg, next(&mut args, &arg)?)?,
      "--read-max-concurrency" => {
        cfg.read_max_concurrency = parse_num(&arg, next(&mut args, &arg)?)?
      }
      "--no-fkv-read-threads" => cfg.fkv_read_threads = false,
      "--reuse-data" => cfg.reuse_data = true,
      "--json" => cfg.json_out = Some(PathBuf::from(next(&mut args, &arg)?)),
      other => return Err(format!("未知参数 {other}")),
    }
  }
  Ok(cfg)
}

fn next(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
  args.next().ok_or_else(|| format!("{flag} 缺少参数值"))
}

fn parse_num<T: std::str::FromStr>(flag: &str, value: String) -> Result<T, String>
where
  T::Err: std::fmt::Display,
{
  value.parse().map_err(|e| format!("{flag}: {e}"))
}

fn main() -> ExitCode {
  let cfg = match parse_args() {
    Ok(cfg) => cfg,
    Err(err) => {
      eprintln!("fkv_compare: {err}");
      return ExitCode::FAILURE;
    }
  };
  // RocksDB 适配器在 `fkv_bench_open` 时读这个环境变量决定 `WriteOptions.sync`；
  // 必须在 dlopen / open 之前设置，所以放在 run() 之前。
  if cfg.align_fsync {
    unsafe { std::env::set_var("FKV_BENCH_SYNC", "1") };
  }
  match run(&cfg) {
    Ok(report) => {
      let text = match sonic_rs::to_string_pretty(&report) {
        Ok(text) => text,
        Err(err) => {
          eprintln!("fkv_compare: serialize: {err}");
          return ExitCode::FAILURE;
        }
      };
      match &cfg.json_out {
        None => println!("{text}"),
        Some(path) => {
          if let Err(err) = std::fs::write(path, text) {
            eprintln!("fkv_compare: write {}: {err}", path.display());
            return ExitCode::FAILURE;
          }
        }
      }
      ExitCode::SUCCESS
    }
    Err(err) => {
      eprintln!("fkv_compare: {err}");
      ExitCode::FAILURE
    }
  }
}
