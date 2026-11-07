//! CLI 参数定义与解析。

use std::{env, error::Error, path::PathBuf};

use clap_args::{ArgAction, arg, clap::Command};
use fkv_types::now_ms;

use crate::{
  lane::{KEY_BYTES, MAX_KEY_BYTES, set_key_width},
  report::git_head,
  ttl::DEFAULT_TTL_MS,
  workload::{RngMode, Workload, effective_rows},
};

/// 各计量阶段的默认时间窗（秒）：对齐 C++ `simple_bench.cpp:31`
/// `DEFINE_uint32(test_secs, 60, "read/scan test time")`；`0`＝固定次数模式
/// （历史行为），供既有调用回退。
pub(crate) const DEFAULT_TEST_SECS: f64 = 60.0;
/// `--kv-size` 缺省：整对 KV 宽度（含 8B key），对齐 C++ `simple_bench.cpp:24`
/// `DEFINE_uint32(kv_size, 128, "size of a pair of KV")` 的默认值
/// （rev-01 落地前的默认 64 是 value 字节口径，已断代）。
pub(crate) const DEFAULT_KV_SIZE: usize = 128;
/// `--key-size` 缺省：8B 大端 uint64（历史 / C++ simple_bench 口径）。
pub(crate) const DEFAULT_KEY_SIZE: usize = KEY_BYTES;
/// `--buffer-pool-size` 缺省 0：沿用 `KvOption::default()` 的 128 MiB，
/// 保持历史行为。>0 时作为**总缓存预算**（index + data page 各一半）。
pub(crate) const DEFAULT_BUFFER_POOL_SIZE: u64 = 0;

/// `--scan-bytes` 缺省：0＝扫到表尾，对齐 C++ `DEFINE_uint64(scan_bytes, 0, ...)`。
pub(crate) const DEFAULT_SCAN_BYTES: u64 = 0;
/// `--write-batchs` 缺省：0＝把 `--rows` 写一遍（历史行为），对齐 C++ 前不改写入量。
pub(crate) const DEFAULT_WRITE_BATCHS: usize = 0;
/// `--warmup-secs` 缺省：0＝不预热。
pub(crate) const DEFAULT_WARMUP_SECS: f64 = 0.0;
/// C++ `simple_bench.cpp` 随机游走生成器硬编码的 key 间隔与 delete 比例。
/// 这两个值不来自 C++ CLI，但决定了实际 workload；历史 / 基线必须按生效值
/// 记录，否则 schema=2 会把 4/4 的负载标成 1/0。
pub(crate) const CPP_KEY_INTERVAL: usize = 4;
pub(crate) const CPP_DEL_RATIO: usize = 4;

/// 解析后的 CLI 参数（含由参数派生的口径）。
pub(crate) struct Cli {
  pub(crate) commit: String,
  pub(crate) baseline: String,
  pub(crate) history: Option<String>,
  pub(crate) impl_name: String,
  pub(crate) max_key: usize,
  pub(crate) rows: usize,
  pub(crate) batch_size: usize,
  pub(crate) kv_size: usize,
  pub(crate) value_size: usize,
  /// 总缓存预算（字节）；0＝沿用 `KvOption::default()`（128 MiB index + page）。
  pub(crate) buffer_pool_size: u64,
  /// 固定点读总次数；0＝按 `--test-secs` / 每链读一遍（历史行为）。
  pub(crate) read_count: u64,
  /// 固定 scan 页请求总次数；0＝按 `--test-secs` / 每链一页（历史行为）。
  pub(crate) scan_count: u64,
  pub(crate) read_per_part: usize,
  pub(crate) partitions: usize,
  pub(crate) key_interval: usize,
  pub(crate) del_ratio: usize,
  pub(crate) write_batchs: usize,
  pub(crate) scan_bytes: u64,
  pub(crate) test_secs: f64,
  pub(crate) workload: Workload,
  pub(crate) ttl_ratio: usize,
  pub(crate) ttl_ms: u64,
  pub(crate) ttl_expire_ts: u64,
  pub(crate) bench_root: Option<PathBuf>,
  /// key / delete 生成口径；CLI 缺省 `cpp`（对齐 C++ simple_bench）。
  pub(crate) rng: RngMode,
  /// 预热窗口（秒）；0＝不预热。
  pub(crate) warmup_secs: f64,
  /// `--load-only`：只写数据集并保留 store 目录。
  pub(crate) load_only: bool,
  /// `--read-only-existing`：打开已存在 store 只读。
  pub(crate) read_only_existing: bool,
  /// load / read 两段共用的固定 store 目录（`--store-dir`）。
  pub(crate) store_dir: Option<PathBuf>,
  /// 重复次数；>1 时输出每指标的中位数 + min/max。
  pub(crate) repeat: usize,
  /// 读 / scan 阶段独立重复次数；>1 时取阶段指标中位数（同一 store，不重写）。
  pub(crate) trials: usize,
}

/// CLI 定义：参数与帮助文本。
fn command(cmd: Command) -> Command {
  cmd
      .arg(arg!(--history <FILE> "perf.json 历史文件").action(ArgAction::Set))
      .arg(arg!(--commit <COMMIT> "提交号").action(ArgAction::Set))
      .arg(arg!(--baseline <BASELINE> "兼容基线").action(ArgAction::Set))
      .arg(arg!(--"impl" <IMPL> "实现名：rust / cpp").action(ArgAction::Set))
      // 兼容现有参数
      .arg(
        arg!(--trials <N> "读 / scan 阶段独立跑 N 次并取中位数，默认 1；写阶段只跑一次")
          .action(ArgAction::Set),
      )
      .arg(arg!(--rows <ROWS> "端到端 benchmark key 数，默认等于 --max-key").action(ArgAction::Set))
      .arg(
        arg!(--"batch-size" <N> "单分区单请求条数（对齐 C++ simple_bench 的 batch_size），默认 100")
          .action(ArgAction::Set),
      )
      .arg(arg!(--root <DIR> "benchmark 数据目录（例如 NVMe 挂载点）").action(ArgAction::Set))
      // 新增参数，保持默认行为与原实现一致
      .arg(
        arg!(--workload <MODE> "write-read|write-only|read-only|write-scan|scan|ttl-write|compact|delete-only|compact-only，默认 write-read；ttl-write / compact / delete-only / compact-only 是 Rust-only lane，不参与 C++ 比值")
          .action(ArgAction::Set),
      )
      .arg(
        // 帮助文本的默认值与校验下界由 `DEFAULT_KV_SIZE` / `KEY_BYTES` 生成，
        // 单一事实源，防止文案与代码再漂移（rev-01 的教训）。
        arg!(--"kv-size" <BYTES>)
          .help(format!(
            "整对 KV 宽度（含 {KEY_BYTES}B key），value 字节 = kv_size - {KEY_BYTES}；默认 {DEFAULT_KV_SIZE}，对齐 C++ simple_bench"
          ))
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"key-size" <BYTES>)
          .help(format!(
            "key 宽度字节，默认 {DEFAULT_KEY_SIZE}（8B 大端 uint64）；24 为 redb 口径，末尾 8B 仍是下标大端编码、前面补零"
          ))
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"buffer-pool-size" <BYTES> "总缓存预算（字节）；0＝沿用 KvOption 默认（128 MiB index + 128 MiB data page），>0 时 index / data page 各占一半")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"read-count" <N> "固定点读总次数（跨全部读链均分）；0＝按 --test-secs 或每链读一遍（历史行为）")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"scan-count" <N> "固定 scan 页请求总次数（跨全部 scan 链均分）；0＝按 --test-secs 或每链一页（历史行为）")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"max-key" <N> "最大 key 数，默认 10000000").action(ArgAction::Set),
      )
      .arg(arg!(--"read-per-part" <N> "每分区读取次数，默认 4").action(ArgAction::Set))
      .arg(
        arg!(--partitions <N> "分区数，默认 1；分区是全局键空间的分片（C++ simple_bench 为每分区一份全量副本，严格对等需 1）")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"key-interval" <N> "key 稀疏间隔；C++ simple_bench 用 4，默认 1")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"del-ratio" <N> "每 N 个写把最后一个改成 delete；C++ simple_bench 用 4，0 表示不删，默认 0")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"write-batchs" <N> "写阶段全系统总批数；每批 = 单分区 batch_size 条，超过分区键数回绕覆盖（对齐 C++ write_batchs）；0＝把 --rows 写一遍，默认 0")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"scan-bytes" <BYTES> "每个 scan 请求的字节窗口；0＝扫到表尾（对齐 C++ scan_bytes），默认 0")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"test-secs" <SECS> "读/scan 阶段时间窗（秒），窗口内循环执行、分位按全部样本算；写阶段按 --write-batchs/--rows 次数驱动；默认 60 对齐 C++ simple_bench，0＝固定次数模式")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"ttl-ratio" <N> "ttl-write lane 每 N 个 upsert 附带 expire_ts（0＝不写 TTL）；仅在 --workload ttl-write 下生效，默认 0")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"ttl-ms" <MS> "ttl-write lane 的 TTL 时长（毫秒），默认 3600000；回收阶段用同一到期时刻触发 clean_expired，不真实等待")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--rng <MODE> "key / delete 生成口径：cpp＝复刻 simple_bench 的 mt19937(0) 随机游走（默认），grid＝历史均匀网格")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"warmup-secs" <SECS> "预热窗口（秒）：先按同形状负载跑该时长，指标整份丢弃，再开始计量；默认 0")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--"load-only" "只写数据集并保留 store（不清目录），供 --read-only-existing 分阶段跑；目录由 --store-dir 指定")
          .action(ArgAction::SetTrue),
      )
      .arg(
        arg!(--"read-only-existing" "打开已存在的 store（reopen 丢弃 index cache）只读，不写数据、不清目录")
          .action(ArgAction::SetTrue),
      )
      .arg(
        arg!(--"store-dir" <DIR> "load-only / read-only-existing 共用的固定 store 目录；缺省 <tmp>/fkv_bench_store")
          .action(ArgAction::Set),
      )
      .arg(
        arg!(--repeat <N> "重复跑 N 次（默认 1）；>1 时每个指标输出中位数，并附 <metric>_min / <metric>_max")
          .action(ArgAction::Set),
      )
}

/// 解析命令行；`--help` / `--version` 已在内部处理，返回 `None` 表示正常退出。
pub(crate) fn parse() -> Result<Option<Cli>, Box<dyn Error>> {
  let Some(arg_match) = clap_args::parse!(command) else {
    return Ok(None);
  };

  let commit = arg_match
    .get_one::<String>("commit")
    .cloned()
    .or_else(|| env::var("GITHUB_SHA").ok())
    .unwrap_or_else(git_head);
  let baseline = arg_match
    .get_one::<String>("baseline")
    .cloned()
    .unwrap_or_default();
  let history = arg_match.get_one::<String>("history").cloned();
  let impl_name = arg_match
    .get_one::<String>("impl")
    .cloned()
    .unwrap_or_else(|| "rust".to_owned());
  let max_key = arg_match
    .get_one::<String>("max-key")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(10_000_000)
    .max(1);
  let rows = arg_match
    .get_one::<String>("rows")
    .and_then(|value| value.parse::<usize>().ok())
    // 默认与 C++ `max_key` 同量级，否则数据规模与冷读比例不可比。
    .unwrap_or(max_key)
    .max(1);
  let batch_size = arg_match
    .get_one::<String>("batch-size")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(100)
    .max(1);
  // key 宽度先于 kv-size 解析：`--kv-size` 是整对宽度，value 长度依赖 key 宽度。
  let key_size = arg_match
    .get_one::<String>("key-size")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(DEFAULT_KEY_SIZE);
  if !(KEY_BYTES..=MAX_KEY_BYTES).contains(&key_size) {
    return Err(
      format!("--key-size {key_size} 非法：必须在 [{KEY_BYTES}, {MAX_KEY_BYTES}] 字节内").into(),
    );
  }
  // 进程级 key 宽度：读 / scan / 写路径统一从这里编码。
  set_key_width(key_size);
  // `--kv-size` 是整对 KV 宽度（含 key），`<= key_size` 报参数错误；value 取差值。
  let kv_size = resolve_kv_size_with_key(
    arg_match.get_one::<String>("kv-size").map(String::as_str),
    key_size,
  )?;
  let value_size = value_len_with_key(kv_size, key_size);
  let buffer_pool_size = arg_match
    .get_one::<String>("buffer-pool-size")
    .and_then(|value| value.parse::<u64>().ok())
    .unwrap_or(DEFAULT_BUFFER_POOL_SIZE);
  let read_count = arg_match
    .get_one::<String>("read-count")
    .and_then(|value| value.parse::<u64>().ok())
    .unwrap_or(0);
  let scan_count = arg_match
    .get_one::<String>("scan-count")
    .and_then(|value| value.parse::<u64>().ok())
    .unwrap_or(0);
  let read_per_part = arg_match
    .get_one::<String>("read-per-part")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(4)
    .max(1);
  let partitions = arg_match
    .get_one::<String>("partitions")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(1)
    .max(1);
  // 写路径口径：默认「间隔 1、不删」＝历史行为；本仓使用固定步长。
  let raw_key_interval = arg_match
    .get_one::<String>("key-interval")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(1)
    .max(1);
  let raw_del_ratio = arg_match
    .get_one::<String>("del-ratio")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(0);
  // 写阶段总批数：0＝按 `--rows` 写一遍（历史行为），>0 按 C++ `write_batchs`
  // 的全系统批数驱动（每批 `batch_size` 条，越界回绕）。
  let write_batchs = arg_match
    .get_one::<String>("write-batchs")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(DEFAULT_WRITE_BATCHS);
  // scan 请求的字节窗口：0＝扫到表尾（C++ 同口径）。
  let scan_bytes = arg_match
    .get_one::<String>("scan-bytes")
    .and_then(|value| value.parse::<u64>().ok())
    .unwrap_or(DEFAULT_SCAN_BYTES);
  // 时间窗（秒）：默认对齐 C++ `simple_bench` 的 `test_secs`（60）；
  // 0＝固定次数模式（历史行为），负值按 0 处理。
  let test_secs = arg_match
    .get_one::<String>("test-secs")
    .and_then(|value| value.parse::<f64>().ok())
    .unwrap_or(DEFAULT_TEST_SECS)
    .max(0.0);
  let workload = arg_match
    .get_one::<String>("workload")
    .map(|value| Workload::parse(value))
    .unwrap_or(Workload::WriteRead);
  // TTL 参数：只在 `ttl-write` lane 生效。其它 lane 一律保持 `expire_ts == 0`
  // 的整批借用快路径，默认输出与既有数据逐字节可比。
  let ttl_ratio = arg_match
    .get_one::<String>("ttl-ratio")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(0);
  let ttl_ms = arg_match
    .get_one::<String>("ttl-ms")
    .and_then(|value| value.parse::<u64>().ok())
    .unwrap_or(DEFAULT_TTL_MS)
    .max(1);
  let ttl_ratio = if workload == Workload::TtlWrite {
    ttl_ratio
  } else {
    if ttl_ratio > 0 {
      eprintln!(
        "fkv_bench: --ttl-ratio {ttl_ratio} 已忽略：该旋钮只在 --workload ttl-write 下生效"
      );
    }
    0
  };
  // 所有 TTL 条目共用一个到期时刻：写入阶段绝不触发 `is_expired`，回收阶段
  // 直接把这个时刻注入 `clean_expired`，无需真实等待。
  let ttl_expire_ts = if ttl_ratio > 0 {
    now_ms().saturating_add(ttl_ms)
  } else {
    0
  };
  // key / delete 生成口径：默认对齐 C++ simple_bench 的 mt19937(0) 随机游走。
  let mut rng = arg_match
    .get_one::<String>("rng")
    .map(|value| RngMode::parse(value))
    .unwrap_or(RngMode::Cpp);
  // ttl-write 是 Rust-only 诊断 lane：TTL 位置按网格序列位选取，随机游走
  // 没有对应位置模型，回收量闭式断言会失效。强制回网格并提示。
  if workload == Workload::TtlWrite && !rng.is_grid() {
    eprintln!("fkv_bench: ttl-write lane 固定使用 --rng grid（TTL 位置按序列位选取）");
    rng = RngMode::Grid;
  }
  // delete-only 的删除 key 是网格顺序位：随机游走生成器无法给出「删全部
  // 已写 key」的序列，强制回网格并提示。
  if workload == Workload::DeleteOnly && !rng.is_grid() {
    eprintln!("fkv_bench: delete-only lane 固定使用 --rng grid（按分区序列位删除全部 key）");
    rng = RngMode::Grid;
  }
  // C++ 随机游走自带 delete 判定与单位步进，`--key-interval` / `--del-ratio`
  // 在 cpp 口径下不参与生成；显式传了就提示，避免误以为生效。
  let (key_interval, del_ratio) = if rng.is_grid() {
    (raw_key_interval, raw_del_ratio)
  } else {
    if arg_match.get_one::<String>("key-interval").is_some()
      || arg_match.get_one::<String>("del-ratio").is_some()
    {
      eprintln!(
        "fkv_bench: --key-interval / --del-ratio 在 --rng cpp 下被 C++ 兼容生成器取代，已忽略"
      );
    }
    (CPP_KEY_INTERVAL, CPP_DEL_RATIO)
  };
  let warmup_secs = arg_match
    .get_one::<String>("warmup-secs")
    .and_then(|value| value.parse::<f64>().ok())
    .unwrap_or(DEFAULT_WARMUP_SECS)
    .max(0.0);
  let load_only = arg_match.get_flag("load-only");
  let read_only_existing = arg_match.get_flag("read-only-existing");
  if load_only && read_only_existing {
    return Err("--load-only 与 --read-only-existing 互斥".into());
  }
  if read_only_existing
    && !(workload.measures_read()
      || workload.measures_scan()
      || workload.measures_delete()
      || workload.is_compact_only())
  {
    return Err(
      "--read-only-existing 需要读 / scan / delete-only / compact-only workload（write-only、ttl-write、compact 不适用）".into(),
    );
  }
  let store_dir = arg_match.get_one::<String>("store-dir").map(PathBuf::from);
  if (load_only || read_only_existing)
    && store_dir.is_none()
    && arg_match.get_one::<String>("root").is_none()
  {
    eprintln!("fkv_bench: 未指定 --store-dir，使用缺省临时目录 <tmp>/fkv_bench_store");
  }
  let repeat = arg_match
    .get_one::<String>("repeat")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(1)
    .max(1);
  if repeat > 1 && (load_only || read_only_existing) {
    eprintln!("fkv_bench: --repeat 只对 Normal 模式生效，load-only / read-only-existing 已忽略");
  }
  let trials = arg_match
    .get_one::<String>("trials")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(1)
    .max(1);
  // 实际 key 数受 max_key 约束。
  let rows = effective_rows(rows, max_key);
  let bench_root = arg_match.get_one::<String>("root").map(PathBuf::from);
  Ok(Some(Cli {
    commit,
    baseline,
    history,
    impl_name,
    max_key,
    rows,
    batch_size,
    kv_size,
    value_size,
    buffer_pool_size,
    read_count,
    scan_count,
    read_per_part,
    partitions,
    key_interval,
    del_ratio,
    write_batchs,
    scan_bytes,
    test_secs,
    workload,
    ttl_ratio,
    ttl_ms,
    ttl_expire_ts,
    bench_root,
    rng,
    warmup_secs,
    load_only,
    read_only_existing,
    store_dir,
    repeat: if load_only || read_only_existing {
      1
    } else {
      repeat
    },
    trials,
  }))
}

/// 解析 `--kv-size`：整对 KV 宽度（含 8B key），缺省取 [`DEFAULT_KV_SIZE`]。
/// `kv_size <= 8` 拒绝并报参数错误，对齐 C++ `simple_bench.cpp:550`
/// `CHECK(FLAGS_kv_size > sizeof(uint64_t))`（pair 口径下 value = kv_size - 8，
/// 再小就下溢）。非法数值沿用本文件其余参数的口径：按缺省值处理。
#[cfg(test)]
pub(crate) fn resolve_kv_size(arg: Option<&str>) -> Result<usize, String> {
  resolve_kv_size_with_key(arg, KEY_BYTES)
}

/// 按 key 宽度解析 `--kv-size`：整对宽度必须大于 key 宽度。
pub(crate) fn resolve_kv_size_with_key(
  arg: Option<&str>,
  key_size: usize,
) -> Result<usize, String> {
  let kv_size = arg
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(DEFAULT_KV_SIZE);
  if kv_size <= key_size {
    return Err(format!(
      "--kv-size {kv_size} 非法：kv_size 是整对 KV 宽度（含 {key_size}B key），必须 > {key_size}"
    ));
  }
  Ok(kv_size)
}

/// value 字节数 = 整对 KV 宽度 - key 宽度；caller 先经 [`resolve_kv_size_with_key`]
/// 保证 `kv_size > key_size`，不会下溢。
pub(crate) fn value_len_with_key(kv_size: usize, key_size: usize) -> usize {
  kv_size - key_size
}

/// 8B key 的历史口径（仅测试使用）。
#[cfg(test)]
pub(crate) fn value_len(kv_size: usize) -> usize {
  value_len_with_key(kv_size, KEY_BYTES)
}
