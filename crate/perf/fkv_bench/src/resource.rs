//! 资源与 IO 计数指标：store 落盘字节、`/proc/self/io`、CompioStore 计数器。
//!
//! 全部是**观测侧**信息，不改变 workload 行为：
//! - `store_bytes`：bench 目录下普通文件字节合计（递归，不引入 `walkdir`）。
//! - `proc_io_read_bytes` / `proc_io_write_bytes`：`/proc/self/io` 的同名字段；
//!   只有 Linux 有，其它平台直接不输出（不编造数字）。
//! - CompioStore 计数器：`fd_open_num` / `write_at_num` / `read_at_num` /
//!   `data_sync_num` / `dir_fsync_num`，通过 `Store::into_io` 在收尾时读取。
//!
//! IO 层未公开 `page_reads` / `page_writes` / `bytes_read` / `bytes_written`
//! 的 store 级 getter（`fkv_metrics` 的 `OpSnapshot` 走 runtime 通道），bench
//! 不越界改引擎，只输出实际可读的计数器。

use std::{fs, path::Path};

use fkv_io_compio::CompioStore;
use fkv_store::Store;
use fkv_types::TableIdent;

/// 递归统计目录下普通文件字节数；读不到的文件按 0 计（观测指标不致命）。
pub(crate) fn dir_size_bytes(root: &Path) -> u64 {
  dir_stats(root).total_bytes
}

/// store 目录的字节 / 文件数分解，按 fkv 文件名前缀归类。
///
/// 与 `perf_local.sh` 的 sidecar `store_footprint()` 保持同一口径：
/// `data_` / `segment_` / `manifest_` 三个前缀，其余进 `other_*`。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DirStats {
  pub(crate) total_bytes: u64,
  pub(crate) file_num: u64,
  pub(crate) data_bytes: u64,
  pub(crate) data_file_num: u64,
  pub(crate) segment_bytes: u64,
  pub(crate) segment_file_num: u64,
  pub(crate) manifest_bytes: u64,
  pub(crate) manifest_file_num: u64,
  pub(crate) other_bytes: u64,
  pub(crate) other_file_num: u64,
}

/// 递归统计目录；按文件名前缀累计到 [`DirStats`]。
pub(crate) fn dir_stats(root: &Path) -> DirStats {
  let mut stats = DirStats::default();
  dir_stats_into(root, &mut stats);
  stats
}

fn dir_stats_into(root: &Path, stats: &mut DirStats) {
  let Ok(entry_li) = fs::read_dir(root) else {
    return;
  };
  for entry in entry_li.flatten() {
    let Ok(meta) = entry.metadata() else {
      continue;
    };
    if meta.is_dir() {
      dir_stats_into(&entry.path(), stats);
      continue;
    }
    if !meta.is_file() {
      continue;
    }
    let size = meta.len();
    stats.total_bytes += size;
    stats.file_num += 1;
    let name = entry.file_name();
    let name = name.to_string_lossy();
    if name.starts_with("data_") {
      stats.data_bytes += size;
      stats.data_file_num += 1;
    } else if name.starts_with("segment_") {
      stats.segment_bytes += size;
      stats.segment_file_num += 1;
    } else if name.starts_with("manifest_") {
      stats.manifest_bytes += size;
      stats.manifest_file_num += 1;
    } else {
      stats.other_bytes += size;
      stats.other_file_num += 1;
    }
  }
}

/// 从 `/proc/self/io` 文本里取 `(read_bytes, write_bytes)`。
pub(crate) fn parse_proc_io(text: &str) -> Option<(u64, u64)> {
  let mut read_bytes = None;
  let mut write_bytes = None;
  for line in text.lines() {
    if let Some(rest) = line.strip_prefix("read_bytes:") {
      read_bytes = rest.trim().parse().ok();
    } else if let Some(rest) = line.strip_prefix("write_bytes:") {
      write_bytes = rest.trim().parse().ok();
    }
  }
  Some((read_bytes?, write_bytes?))
}

/// 当前进程的 `/proc/self/io` 读写字节；非 Linux 返回 `None`。
pub(crate) fn proc_io_bytes() -> Option<(u64, u64)> {
  parse_proc_io(&fs::read_to_string("/proc/self/io").ok()?)
}

/// CompioStore 累计计数器（跨全部 bench 分区求和）。
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct IoCounters {
  pub(crate) fd_open_num: u64,
  pub(crate) write_at_num: u64,
  pub(crate) read_at_num: u64,
  pub(crate) data_sync_num: u64,
  pub(crate) data_fsync_num: u64,
  pub(crate) dir_fsync_num: u64,
  pub(crate) manifest_append_num: u64,
  pub(crate) manifest_sync_num: u64,
}

/// index page cache 的累计命中 / 淘汰计数与当前占用。
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct CacheStats {
  pub(crate) hit: u64,
  pub(crate) miss: u64,
  pub(crate) evict: u64,
  pub(crate) bytes: u64,
  pub(crate) len: u64,
  pub(crate) capacity: u64,
}

/// 汇总各分区 store 的 index cache 统计（读 store，不消耗句柄）。
pub(crate) fn collect_cache_stats(store_li: &[(TableIdent, Store<CompioStore>)]) -> CacheStats {
  let mut stats = CacheStats::default();
  for (_tbl, store) in store_li {
    let stat = store.index_cache_stat();
    stats.hit += stat.hit;
    stats.miss += stat.miss;
    stats.evict += stat.evict;
    stats.bytes += stat.bytes as u64;
    stats.len += stat.len as u64;
    stats.capacity += stat.capacity as u64;
  }
  stats
}

/// 汇总各分区 store 的写路径页缓存统计（与 index cache 分开上报）。
pub(crate) fn collect_write_cache_stats(
  store_li: &[(TableIdent, Store<CompioStore>)],
) -> CacheStats {
  let mut stats = CacheStats::default();
  for (_tbl, store) in store_li {
    let stat = store.write_cache_stat();
    stats.hit += stat.hit;
    stats.miss += stat.miss;
    stats.evict += stat.evict;
    stats.bytes += stat.bytes as u64;
    stats.len += stat.len as u64;
    stats.capacity += stat.capacity as u64;
  }
  stats
}

/// 汇总各分区 store 的读路径页缓存统计（与写路径页缓存分开上报）。
///
/// 读写两侧共用同一个缓存池：`bytes` / `len` / `capacity` / `evict` 是公共
/// 账目，读 / 写两份上报值相同；`hit` / `miss` 各自独立累计。
pub(crate) fn collect_read_cache_stats(
  store_li: &[(TableIdent, Store<CompioStore>)],
) -> CacheStats {
  let mut stats = CacheStats::default();
  for (_tbl, store) in store_li {
    let stat = store.read_cache_stat();
    stats.hit += stat.hit;
    stats.miss += stat.miss;
    stats.evict += stat.evict;
    stats.bytes += stat.bytes as u64;
    stats.len += stat.len as u64;
    stats.capacity += stat.capacity as u64;
  }
  stats
}

/// 收尾读取并求和各分区的 CompioStore 计数器（消耗 store，顺带触发关闭）。
pub(crate) fn drain_io_counters(store_li: Vec<(TableIdent, Store<CompioStore>)>) -> IoCounters {
  let mut counters = IoCounters::default();
  for (_tbl, store) in store_li {
    let io = store.into_io();
    counters.fd_open_num += io.fd_open_num();
    counters.write_at_num += io.write_at_num();
    counters.read_at_num += io.read_at_num();
    counters.data_sync_num += io.data_sync_num();
    counters.data_fsync_num += io.data_fsync_num();
    counters.dir_fsync_num += io.dir_fsync_num();
    counters.manifest_append_num += io.manifest_append_num();
    counters.manifest_sync_num += io.manifest_sync_num();
  }
  counters
}

/// 追加资源指标（`store_bytes` + CompioStore 计数 + `/proc/self/io`）。
pub(crate) fn push_resource_metrics(
  result_li: &mut Vec<(String, f64, &'static str)>,
  counters: IoCounters,
  store_bytes: u64,
) {
  result_li.push(("store_bytes".to_owned(), store_bytes as f64, "byte"));
  result_li.push((
    "io_fd_open_num".to_owned(),
    counters.fd_open_num as f64,
    "num",
  ));
  result_li.push((
    "io_write_at_num".to_owned(),
    counters.write_at_num as f64,
    "num",
  ));
  result_li.push((
    "io_read_at_num".to_owned(),
    counters.read_at_num as f64,
    "num",
  ));
  result_li.push((
    "io_data_sync_num".to_owned(),
    counters.data_sync_num as f64,
    "num",
  ));
  result_li.push((
    "io_data_fsync_num".to_owned(),
    counters.data_fsync_num as f64,
    "num",
  ));
  result_li.push((
    "io_dir_fsync_num".to_owned(),
    counters.dir_fsync_num as f64,
    "num",
  ));
  result_li.push((
    "io_manifest_append_num".to_owned(),
    counters.manifest_append_num as f64,
    "num",
  ));
  result_li.push((
    "io_manifest_sync_num".to_owned(),
    counters.manifest_sync_num as f64,
    "num",
  ));
  if let Some((read_bytes, write_bytes)) = proc_io_bytes() {
    result_li.push(("proc_io_read_bytes".to_owned(), read_bytes as f64, "byte"));
    result_li.push(("proc_io_write_bytes".to_owned(), write_bytes as f64, "byte"));
  }
}

/// 追加 store 体积 / 文件数分解（`store_bytes` 已由 [`push_resource_metrics`] 输出）。
pub(crate) fn push_store_breakdown(
  result_li: &mut Vec<(String, f64, &'static str)>,
  stats: DirStats,
) {
  result_li.push(("store_file_num".to_owned(), stats.file_num as f64, "num"));
  result_li.push((
    "store_bytes_data".to_owned(),
    stats.data_bytes as f64,
    "byte",
  ));
  result_li.push((
    "store_bytes_segment".to_owned(),
    stats.segment_bytes as f64,
    "byte",
  ));
  result_li.push((
    "store_bytes_manifest".to_owned(),
    stats.manifest_bytes as f64,
    "byte",
  ));
  result_li.push((
    "store_bytes_other".to_owned(),
    stats.other_bytes as f64,
    "byte",
  ));
  result_li.push((
    "store_file_num_data".to_owned(),
    stats.data_file_num as f64,
    "num",
  ));
  result_li.push((
    "store_file_num_segment".to_owned(),
    stats.segment_file_num as f64,
    "num",
  ));
  result_li.push((
    "store_file_num_manifest".to_owned(),
    stats.manifest_file_num as f64,
    "num",
  ));
}

/// 追加 index cache 统计；`cache_hit_ratio` 无访问时输出 0，不伪造命中。
pub(crate) fn push_cache_stats(
  result_li: &mut Vec<(String, f64, &'static str)>,
  stats: CacheStats,
) {
  push_named_cache_stats(result_li, "cache", stats);
}

/// 写路径页缓存指标（`write_cache_*`）：与 index 缓存的 `cache_*` 分列。
pub(crate) fn push_write_cache_stats(
  result_li: &mut Vec<(String, f64, &'static str)>,
  stats: CacheStats,
) {
  push_named_cache_stats(result_li, "write_cache", stats);
}

/// 读路径页缓存指标（`read_cache_*`）：与 `write_cache_*` 分列。
pub(crate) fn push_read_cache_stats(
  result_li: &mut Vec<(String, f64, &'static str)>,
  stats: CacheStats,
) {
  push_named_cache_stats(result_li, "read_cache", stats);
}

fn push_named_cache_stats(
  result_li: &mut Vec<(String, f64, &'static str)>,
  prefix: &str,
  stats: CacheStats,
) {
  result_li.push((format!("{prefix}_hit_num"), stats.hit as f64, "num"));
  result_li.push((format!("{prefix}_miss_num"), stats.miss as f64, "num"));
  result_li.push((format!("{prefix}_evict_num"), stats.evict as f64, "num"));
  result_li.push((format!("{prefix}_bytes"), stats.bytes as f64, "byte"));
  result_li.push((format!("{prefix}_len"), stats.len as f64, "page"));
  result_li.push((format!("{prefix}_capacity"), stats.capacity as f64, "byte"));
  let total = stats.hit + stats.miss;
  result_li.push((
    format!("{prefix}_hit_ratio"),
    if total == 0 {
      0.0
    } else {
      stats.hit as f64 / total as f64
    },
    "ratio",
  ));
}
