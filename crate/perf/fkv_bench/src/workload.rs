//! workload 模式、写口径与读计划。

/// bench workload 模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Workload {
  WriteRead,
  WriteOnly,
  ReadOnly,
  WriteScan,
  Scan,
  /// TTL 双树写诊断 lane：按 `--ttl-ratio` 给一批 upsert 附 `expire_ts`，
  /// 写阶段后推进时钟清理并计量回收。Rust-only，不参与 C++ 比值。
  TtlWrite,
  /// compact 并发诊断 lane：一个分区持续碎片写 + compact，其余分区并发点读。
  /// Rust-only，不参与 C++ 比值。
  Compact,
  /// redb 风格 delete-only：先不计量地顺序 upsert 全部 key，再计量纯 delete。
  DeleteOnly,
  /// redb 风格 compact-only：对已有 store 逐分区调用 `compact_if_needed`，
  /// 计量 compaction 前后目录大小与回收率。Rust-only，不参与 C++ 比值。
  CompactOnly,
}

impl Workload {
  pub(crate) fn parse(value: &str) -> Self {
    match value {
      "write-only" => Self::WriteOnly,
      "read-only" => Self::ReadOnly,
      "write-scan" => Self::WriteScan,
      "scan" => Self::Scan,
      "ttl-write" => Self::TtlWrite,
      "compact" => Self::Compact,
      "delete-only" => Self::DeleteOnly,
      "compact-only" => Self::CompactOnly,
      _ => Self::WriteRead,
    }
  }

  pub(crate) fn as_str(self) -> &'static str {
    match self {
      Self::WriteRead => "write-read",
      Self::WriteOnly => "write-only",
      Self::ReadOnly => "read-only",
      Self::WriteScan => "write-scan",
      Self::Scan => "scan",
      Self::TtlWrite => "ttl-write",
      Self::Compact => "compact",
      Self::DeleteOnly => "delete-only",
      Self::CompactOnly => "compact-only",
    }
  }

  /// 是否计量写阶段（`read-only` / `scan` 的写入只作数据准备）。
  pub(crate) fn measures_write(self) -> bool {
    matches!(
      self,
      Self::WriteRead | Self::WriteOnly | Self::WriteScan | Self::TtlWrite
    )
  }

  /// 是否计量点读。
  pub(crate) fn measures_read(self) -> bool {
    matches!(self, Self::WriteRead | Self::ReadOnly)
  }

  /// 是否计量 scan（对齐 C++ 的 `scan`/`write-scan` workload）。
  pub(crate) fn measures_scan(self) -> bool {
    matches!(
      self,
      Self::WriteRead | Self::ReadOnly | Self::WriteScan | Self::Scan
    )
  }

  /// 是否计量 delete 阶段（`delete-only` 的 delete 指标前缀是 `delete`）。
  pub(crate) fn measures_delete(self) -> bool {
    matches!(self, Self::DeleteOnly)
  }

  /// 是否走 `compact-only` 专用分支（逐分区 `compact_if_needed`）。
  pub(crate) fn is_compact_only(self) -> bool {
    matches!(self, Self::CompactOnly)
  }
}

/// 写 key 序列生成口径。
///
/// - [`RngMode::Cpp`]（默认）：逐位复刻 C++ `simple_bench` 的
///   `std::mt19937(0)` 随机游走 + 每 4 条一次 delete，key / tombstone 序列与
///   C++ 完全一致；此时 `key_interval` / `del_ratio` 不参与生成。
/// - [`RngMode::Grid`]：历史行为，均匀网格 key + 按序列位周期性 delete。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RngMode {
  Grid,
  Cpp,
}

impl RngMode {
  pub(crate) fn parse(value: &str) -> Self {
    match value {
      "grid" => Self::Grid,
      _ => Self::Cpp,
    }
  }

  pub(crate) fn as_str(self) -> &'static str {
    match self {
      Self::Grid => "grid",
      Self::Cpp => "cpp",
    }
  }

  /// 网格模式下读侧能按写入序列位精确推导 key 存活；随机游走模式不能。
  pub(crate) fn is_grid(self) -> bool {
    self == Self::Grid
  }
}

/// 写路径口径（`key_interval` / `del_ratio` / key 生成器）。
///
/// `Grid` 的 key 模型是**均匀网格**：第 `index` 个 key 为 `index * key_interval`。
/// 同参数下键密度 / 树形状与随机步进不同。跨实现比较只在
/// `key_interval = 1`（两侧都退化为顺序写）时严格可比，其余按趋势参考。
/// 默认间隔 1、不删，与历史数据可比；[`RngMode::Cpp`] 下这两个旋钮被
/// C++ 兼容生成器取代。
#[derive(Debug, Clone, Copy)]
pub(crate) struct WriteMix {
  /// key 稀疏间隔：第 `index` 个 key 是 `index * key_interval`；仅 `Grid` 生效。
  pub(crate) key_interval: usize,
  /// 每 `del_ratio` 个写请求把最后一个改成 delete；`0` 表示不删；仅 `Grid` 生效。
  pub(crate) del_ratio: usize,
  /// 每 `ttl_ratio` 个 upsert 附带 `ttl_expire_ts`；`0` 表示不写 TTL
  /// （默认，`expire_ts == 0` 整批借用快路径与历史口径一致）。
  pub(crate) ttl_ratio: usize,
  /// TTL 绝对到期时间（Unix ms）；仅 `ttl_ratio > 0` 时生效。
  pub(crate) ttl_expire_ts: u64,
  /// key / delete 序列生成口径。
  pub(crate) rng: RngMode,
}

impl Default for WriteMix {
  fn default() -> Self {
    Self {
      key_interval: 1,
      del_ratio: 0,
      ttl_ratio: 0,
      ttl_expire_ts: 0,
      // 内部默认保持历史网格口径；CLI 的 `--rng` 缺省是 `cpp`，由 `main` 覆盖。
      rng: RngMode::Grid,
    }
  }
}

/// 第 `index` 个写请求是否按 `del_ratio` 判定为 delete。
///
/// `index` 是**分区内序列位**（不是全局 key 下标）。`del_ratio < 2` 视为「不删」：
/// 这样 delete 删的是 `del_ratio - 1` 步之前已写过的 key，写路径真的走到既有项的
/// tombstone 覆盖（而不是删一个不存在的 key）。
pub(crate) fn is_deleted(index: usize, del_ratio: usize) -> bool {
  del_ratio >= 2 && index % del_ratio == del_ratio - 1
}

/// 第 `index` 个写入若是 delete，它删掉的**分区内序列位**。
///
/// delete 位 `index % del_ratio == del_ratio - 1` 保证 `index >= del_ratio - 1`，
/// 不会下溢；目标与本条写同属一个分区。
pub(crate) fn delete_target(index: usize, del_ratio: usize) -> usize {
  index - (del_ratio - 1)
}

/// 一个分区写完一遍后，第 `index` 个序列位上的 key 是否还存在。
///
/// 存活 = 该位是 upsert，且它没有被后续 delete 摘掉。delete 位自身不写 key；
/// 倍数额 `k * del_ratio` 会在 `k * del_ratio + del_ratio - 1` 被 delete 摘掉——
/// 只有该 delete 位落在本分区序列内（`index + del_ratio - 1 < count`）才真的发生，
/// 尾部的倍数额在写完一遍后仍然存活。
pub(crate) fn is_live(index: usize, count: usize, del_ratio: usize) -> bool {
  del_ratio < 2
    || (!is_deleted(index, del_ratio)
      && !(index.is_multiple_of(del_ratio) && index + del_ratio - 1 < count))
}

/// 一个分区写完一遍后的存活 key 数（闭式，避免对 `rows` 全量遍历）。
///
/// 存活 = 总数 - delete 位数 - 被摘掉的倍数额；delete 位有 `count / del_ratio` 个，
/// 倍数额 `k * del_ratio` 被摘掉当且仅当 `k * del_ratio + del_ratio - 1 < count`。
pub(crate) fn live_key_num(count: usize, del_ratio: usize) -> usize {
  if del_ratio < 2 {
    return count;
  }
  let deleted = count / del_ratio;
  let tomb = if count >= del_ratio {
    (count - del_ratio) / del_ratio + 1
  } else {
    0
  };
  count - deleted - tomb
}

/// 分区 `part` 的序列位再写 `written` 条后，第 `pos` 位的 key 是否存活。
///
/// 覆盖写时每条写都在分区自有序列上推进、越界回绕（对齐 C++ `writing_key_`
/// 到 `max_key` 归零）。最后一遍可能只写了前缀 `rem = written % count`：
///
/// - `rem == 0`：最后一遍完整，按整遍语义判定；
/// - `pos < rem`：最后一遍重写了该位，按前缀 `rem` 判定；
/// - 其余位：最后一遍没碰，仍是一整遍写完的状态。
pub(crate) fn live_after(written: usize, count: usize, pos: usize, del_ratio: usize) -> bool {
  if written == 0 || count == 0 {
    return false;
  }
  let rem = written % count;
  if rem > 0 && pos < rem {
    is_live(pos, rem, del_ratio)
  } else if written >= count {
    is_live(pos, count, del_ratio)
  } else {
    false
  }
}

/// 一个分区写完 `written` 条后，TTL 树里应存的 user key 条数（去重后）。
///
/// TTL key = `expire_ts(8B) ++ user_key`；本 lane 所有 TTL 条目共用同一
/// `expire_ts`，因此按分区内序列位去重：只有「最后一次写仍是带 TTL 的 upsert」
/// 的位置才会留下一条 TTL 索引。`live_after` 已排除 delete 位与被 delete
/// 摘掉的位，再按 `ttl_ratio` 筛出带 TTL 的位置即可，无需遍历写入历史。
pub(crate) fn ttl_live_key_num(written: usize, count: usize, mix: WriteMix) -> usize {
  if mix.ttl_ratio == 0 || mix.ttl_expire_ts == 0 || count == 0 {
    return 0;
  }
  (0..count)
    .filter(|&pos| {
      pos % mix.ttl_ratio == mix.ttl_ratio - 1 && live_after(written, count, pos, mix.del_ratio)
    })
    .count()
}

/// 读侧分区分读参数，对齐 C++ `simple_bench` 的 `partitions` / `read_per_part`。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ReadPlan {
  /// 分区数，每分区一棵独立表。
  pub(crate) partitions: usize,
  /// 每分区点读遍数。
  pub(crate) read_per_part: usize,
}

/// `bench_workload` 的输入参数：CLI 解析后的端到端计量口径。
#[derive(Debug, Clone, Copy)]
pub(crate) struct BenchArgs {
  pub(crate) rows: usize,
  pub(crate) value_size: usize,
  pub(crate) batch_size: usize,
  pub(crate) workload: Workload,
  pub(crate) mix: WriteMix,
  pub(crate) plan: ReadPlan,
  pub(crate) test_secs: f64,
  /// 写阶段全系统总批数；0＝按 `rows` 写一遍。
  pub(crate) write_batchs: usize,
  /// scan 请求的字节窗口；0＝扫到表尾。
  pub(crate) scan_bytes: u64,
  /// `ttl-write` lane 配置的 TTL 时长（毫秒），仅用于输出到指标。
  pub(crate) ttl_ms: u64,
}
/// 实际 key 数受 `max_key` 约束。
pub(crate) fn effective_rows(rows: usize, max_key: usize) -> usize {
  rows.min(max_key).max(1)
}
