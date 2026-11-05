//! `KvOption` 定义。
//!
//! 字段、默认值、模式派生、`validate` 与 INI 解析均已落地。
//!
//! # 未迁移字段（决策登记）
//!
//! - `comparator_`：不迁移。fkv 盘面只支持默认字节序比较器（见 `fkv_page`），
//!   自定义比较器的数据本身就不是本引擎的可读格式。
//! - `global_registered_memories` / `pinned_memory_chunks`：不迁移。两者是
//!   io_uring 固定 buffer 注册入口，fkv 的 IO 走 compio，无对应物。
//! - `partition_filter`：属「一个 Store 托管多个 data store shard」的宿主集成，
//!   **不做 1:1**；请求路由按
//!   `partition_id % shard_num`，与 C++ 未设 filter 时的默认一致。
//! - `data_append_mode`：语义登记。Rust 只实现 append 分配模式，`false` 不会
//!   切到 C++ 的 `PooledFilePages`（pooled 不在 L1 兼容范围）；字段保留用于
//!   配置兼容与 cloud/standby 的追加写校验，`validate` 会显式提示。
//! - `enable_data_page_cache` / `init_page_count`：C++ data page cache 的开关与
//!   预分配页数；Rust 读路径直接走 index cache + page buffer，没有对应缓存层，
//!   字段仅保留配置兼容。
//! - `direct_io_buffer_pool_size`：C++ 预注册 O_DIRECT buffer 池；fkv 的 IO 走
//!   compio + `fixed_buffer_size` 固定 buffer，无独立池。
//! - `chunk_size`：C++ 大 value chunk 分配单元；Rust 由
//!   `large_value_threshold` / `SegmentLayout` 决定，不单独成块。
//! - `coroutine_stack_size`：协程栈由 compio executor 统一管理，无 per-coroutine
//!   栈配置。
//! - `segment_compact_yield_every`：C++ 按「每 N 段」让出；Rust 的段压实统一用
//!   时间预算 `yield_budget_us`（`YieldBudget`），不按段数计数。
//! - `max_archive_tasks`：归档并发由 runtime 全局 fanout 批
//!   （`max_global_request_batch`）限制，无独立任务池。
//! - `cloud_request_threads`：S3 客户端使用单后台线程 + 异步并发，无独立
//!   HTTP 线程池；飞行中请求上限由 `max_cloud_concurrency` 控制。
//! - `write_buffer_size` / `write_buffer_ratio` / `non_page_io_batch_size`：
//!   C++ 的预注册写缓冲池与批量 IO 参数；fkv 的写路径用 compio fixed buffer
//!   与 group commit，不按这三个值切分。
//! - `reserve_space_ratio`：C++ 空间预留水位；Rust 只有 `local_space_limit`
//!   硬限，未实现预留比例。
//! - `gc_global_mem_size_per_shard`：GC 内存预算未在 Rust 侧单独计量。
//! - `pinned_tail_scratch_slots`：C++ pinned tail scratch 结构，Rust 读路径无
//!   对应物。
//!
//! 以上零消费字段在 `validate` 中统一告警（默认值不告警），避免「配置了却
//! 不生效」被当成静默陷阱。

use core::iter::repeat_n;
use std::{fs::read_to_string, path::Path, str::FromStr, time::Duration};

use fkv_error::{KvError, Result};
use fkv_types::{
  DEFAULT_MANIFEST_LIMIT, DEFAULT_MAX_PROCESSING_TIME_US, DEFAULT_PAGES_PER_FILE_SHIFT,
  DEFAULT_SEGMENT_SIZE, DEFAULT_SEGMENTS_PER_FILE_SHIFT, MAX_OVERFLOW_POINTERS,
  PAGE_ALIGN_U64 as PAGE_ALIGN, StoreMode,
};
use log::warn;

pub const KB: u64 = 1 << 10;
pub const MB: u64 = 1 << 20;
pub const GB: u64 = 1 << 30;
pub const TB: u64 = 1 << 40;
/// IO 层在飞写组默认上限（对齐 C++ `include/kv_options.h` 的
/// `KvOption::max_inflight_write = 32 << 10`）。
///
/// 注意区分两处非默认值：`src/kv_options.cpp` 的 `4096` 是「INI 键存在但
/// 取不到值」时的解析兜底，`db_stress` 的 `4096` 是压测入口 gflag；二者都不是
/// 库默认值。Rust 默认值以头文件字段初始值为准。
pub const DEFAULT_MAX_INFLIGHT_WRITE: u32 = 32 << 10;
/// 单次批量写合并的页数默认上限（对齐 C++ `include/kv_options.h` 的
/// `KvOption::max_write_batch_pages = 32`）。
///
/// `src/kv_options.cpp` 的 `64` 是「INI 键存在但取不到值」时的解析兜底，
/// `db_stress` 的 `64` 是压测 gflag；库默认值仍是头文件的 32。
pub const DEFAULT_MAX_WRITE_BATCH_PAGES: u32 = 32;
/// FD 缓存默认上限（对齐 C++ `KvOption::fd_limit`）。
pub const DEFAULT_FD_LIMIT: u32 = 10_000;
/// io_uring SQ 深度默认值（对齐 C++ `KvOption::io_queue_size`）。
pub const DEFAULT_IO_QUEUE_SIZE: u32 = 4096;
/// 每表 index page 缓存默认预算（fkv 扩展键 `index_cache_bytes`）。
pub const DEFAULT_INDEX_CACHE_BYTES: u64 = 64 * MB;
/// 每 shard root meta 缓存默认预算（对齐 C++ `KvOption::root_meta_cache_size`
/// 的 1 GiB）：mapper / segment mapping / index cache / 字典 / branch 元数据
/// 合计的字节上限。
///
/// runtime 按线程数（shard 数）均分后下发给各 shard，语义对齐 C++
/// `root_meta.cpp` 的 `capacity_bytes_ = root_meta_cache_size / num_threads`。
pub const DEFAULT_ROOT_META_CACHE_BYTES: u64 = GB;
/// `OutOfMem` 整请求重试次数默认值。
///
/// runtime 未给 `KvOption` 时也回落到本常量，避免以前 `KvOption=5` /
/// runtime 写死 `3` 的默认值分叉。
pub const DEFAULT_AUTO_OOM_RETRY_TIMES: u8 = 5;
/// 单任务内长循环的协作让出段预算默认值（微秒）；`0` = 关闭。
pub const DEFAULT_YIELD_BUDGET_US: u64 = 20;
/// Cloud 模式数据面请求默认 deadline（微秒）。
///
/// 依据 `fkv_object_s3`：`DEFAULT_RETRY_TIMEOUT = 30s`，即一次 S3 操作从首次
/// 尝试起最多重试 30s（`max_retries = 3` + 100ms 起始、上限 15s 的指数退避）。
/// 单请求可能串多个 S3 IO，但仍按一次重试窗口兜底：超过该窗口的请求大概率
/// 已在重试里空转，继续挂住只会放大调用方积压。
pub const DEFAULT_CLOUD_REQUEST_TIMEOUT_US: u64 = 30_000_000;
/// Cloud 模式数据面入队等待默认上限（微秒）。
///
/// 依据同一套 S3 参数：`max_retries = 3` 的退避总量约 100 + 200 + 400ms，
/// 取 5s（约 7 倍）容忍瞬时抖动；同时明显早于 30s 请求窗口，队列饱和时回
/// `Busy`（可重试）而不是被请求 deadline 误报成 `Timeout`。
pub const DEFAULT_CLOUD_QUEUE_TIMEOUT_US: u64 = 5_000_000;
/// GCS/Google 系 provider 的默认 endpoint（对齐 C++ `ValidateOption`）。
const GCS_ENDPOINT: &str = "https://storage.googleapis.com";

/// `store_path_lut` 最大条目数（对齐 C++ `kDefaultStorePathLutEntries = 1 << 20`）。
///
/// 权重按 gcd 归一后再按本上限等比缩放并裁剪，避免 `store_path =
/// /a,/b:1000000,1` 这类配置把 LUT 撑到百万级以上。
pub const MAX_STORE_PATH_LUT_ENTRIES: usize = 1 << 20;

/// 按权重生成「分区 -> store_path 序号」LUT（对齐 C++ `ComputeStorePathLut`）。
///
/// 表项按路径顺序连续填充：`weights = [3, 7]` → 前 3 槽为 0、后 7 槽为 1；
/// 分区选根用 `lut[partition_id % lut.len()]`。权重 `0` 按 `1` 处理（C++ 同），
/// 但 `KvOption::validate` 会在调用前拒绝 0，保持显式配置错误可见。
pub fn compute_store_path_lut(weight_li: &[u64]) -> Vec<u32> {
  compute_store_path_lut_capped(weight_li, MAX_STORE_PATH_LUT_ENTRIES)
}

/// [`compute_store_path_lut`] 的可指定上限版本（测试缩放分支用）。
///
/// 返回长度恒 `<= max_entries`（除空输入 / `max_entries == 0` 返回空）：
/// 等比缩放下取整后仍超出时，从当前权重最大者逐个扣槽（同权重取下标小者）；
/// `max_entries` 小于根目录数时每根最多保底 1 槽，只保留前 `max_entries` 个根。
pub fn compute_store_path_lut_capped(weight_li: &[u64], max_entries: usize) -> Vec<u32> {
  if weight_li.is_empty() || max_entries == 0 {
    return Vec::new();
  }
  let mut weight_li: Vec<u64> = weight_li.iter().map(|weight| (*weight).max(1)).collect();
  let gcd = weight_li.iter().copied().fold(0, gcd_u64);
  if gcd > 1 {
    for weight in &mut weight_li {
      *weight /= gcd;
    }
  }
  let mut total = weight_li.iter().copied().fold(0u64, u64::saturating_add);
  if total > max_entries as u64 {
    let scale = total.div_ceil(max_entries as u64);
    total = 0;
    for weight in &mut weight_li {
      *weight = (*weight / scale).max(1);
      total = total.saturating_add(*weight);
    }
    // 缩放后每根仍保底 1 槽，溢出量最多为根数；从当前权重最大者逐个扣减
    // （同权重取下标小者），保证 total <= max_entries 且结果确定。
    while total > max_entries as u64 {
      let Some(idx) = weight_li
        .iter()
        .enumerate()
        .filter(|(_, weight)| **weight > 1)
        .max_by(|left, right| left.1.cmp(right.1).then(right.0.cmp(&left.0)))
        .map(|(idx, _)| idx)
      else {
        // 上限比根目录数还小：每根已只剩 1 槽，只能保留前 `max_entries` 个根。
        weight_li.truncate(max_entries);
        total = max_entries as u64;
        break;
      };
      weight_li[idx] -= 1;
      total -= 1;
    }
  }
  let mut lut = Vec::with_capacity(usize::try_from(total).unwrap_or(max_entries));
  for (idx, weight) in weight_li.iter().enumerate() {
    lut.extend(repeat_n(idx as u32, *weight as usize));
  }
  lut
}

fn gcd_u64(mut a: u64, mut b: u64) -> u64 {
  while b != 0 {
    (a, b) = (b, a % b);
  }
  a
}

/// `KvOption` 字段唯一声明点：一行同时给出「字段 + 类型 + 默认值 + INI 键与解析器」，
/// 宏展开出 struct、`Default` 与 `apply_ini`，从结构上消除三处字段表漂移：
/// 以前新增一个 knob 要同步 struct / `Default` / `load_from_ini` 三处，漏一处就是
/// 「字段存在但 INI 永远读不到」或「INI 能配但默认值分叉」。
///
/// 语法：`字段: 类型 = 默认值, [节: 解析器];`
/// - 节 `Run` / `Permanent`：只在该 INI 节生效；
/// - 节 `Ext`：fkv 扩展键，`[run]` / `[permanent]` 均接受；
/// - 节 `Skip`：无 INI 键（如由 `store_path` 派生的权重 / lut 表）。
///
/// 单遍展开（不递归），保证 struct 字段、默认值、INI 派发表出自同一份声明。
macro_rules! kv_options {
  (
    $(
      $(#[$meta:meta])*
      $name:ident : $ty:ty = $default:expr, [$section:ident: $parse:ident]
    );*
    $(;)?
  ) => {
    #[derive(Debug, Clone, PartialEq)]
    pub struct KvOption {
      $( $(#[$meta])* pub $name: $ty, )*
    }

    impl Default for KvOption {
      fn default() -> Self {
        Self { $( $name: $default, )* }
      }
    }

    /// 按 INI 键派发一次赋值；返回 `false` 表示该键不在本表（由调用方继续处理派生键）。
    ///
    /// `KvOption::apply_ini` 只做转发，特判一次写多字段的路径键。
    fn apply_ini_field(
      opts: &mut KvOption,
      section: Section,
      key: &str,
      value: &str,
    ) -> Result<bool> {
      match key {
        $(
          stringify!($name) if section_matches(Section::$section, section) => {
            opts.$name = $parse(value)?;
          }
        )*
        _ => return Ok(false),
      }
      Ok(true)
    }
  };
}

kv_options! {
  num_threads: u16 = 1, [Run: parse_num];
  /// data 页 restart 间隔（对齐 C++ `KvOption::data_page_restart_interval`）。
  data_page_restart_interval: u16 = 16, [Run: parse_num];
  /// index 页 restart 间隔（对齐 C++ `KvOption::index_page_restart_interval`）。
  ///
  /// 两旋钮独立：index 页 restart 密度只由本字段决定，不受 data 侧影响。
  index_page_restart_interval: u16 = 16, [Run: parse_num];
  init_page_count: u32 = 1 << 15, [Run: parse_num];
  skip_verify_checksum: bool = false, [Run: parse_bool];
  buffer_pool_size: u64 = 128 * MB, [Run: parse_size];
  enable_data_page_cache: bool = false, [Run: parse_bool];
  root_meta_cache_size: u64 = DEFAULT_ROOT_META_CACHE_BYTES, [Run: parse_size];
  manifest_limit: u32 = DEFAULT_MANIFEST_LIMIT, [Run: parse_size_u32];
  fd_limit: u32 = DEFAULT_FD_LIMIT, [Run: parse_num];
  io_queue_size: u32 = DEFAULT_IO_QUEUE_SIZE, [Run: parse_num];
  /// 每表 index page 缓存字节预算，`0` 表示不限制（fkv 扩展键）。
  index_cache_bytes: u64 = DEFAULT_INDEX_CACHE_BYTES, [Ext: parse_size];
  /// 全 store index page 缓存聚合字节预算，`0` 表示不限制（fkv 扩展键）。
  ///
  /// 与 `index_cache_bytes` 独立：实际生效上限是两者的较小者，多表时本值
  /// 再按挂载的缓存表数均分。默认与每表预算一致，避免只调每表预算时总量失控。
  index_cache_total_bytes: u64 = DEFAULT_INDEX_CACHE_BYTES, [Ext: parse_size];
  /// IO 层在飞写组上限（对齐 C++ `max_inflight_write`，默认 32 << 10）。
  max_inflight_write: u32 = DEFAULT_MAX_INFLIGHT_WRITE, [Run: parse_num];
  /// 单次批量写合并的页数上限（对齐 C++ `max_write_batch_pages`，默认 32）。
  max_write_batch_pages: u32 = DEFAULT_MAX_WRITE_BATCH_PAGES, [Run: parse_num];
  coroutine_stack_size: u32 = 32 * KB as u32, [Run: parse_size_u32];
  num_retained_archives: u32 = 100, [Run: parse_num];
  archive_interval_secs: u32 = 0, [Run: parse_num];
  max_archive_tasks: u16 = 256, [Run: parse_num];
  max_global_request_batch: u32 = 1000, [Run: parse_num];
  file_amplify_factor: u8 = 2, [Run: parse_num];
  segment_file_amplify_factor: u8 = 2, [Ext: parse_num];
  segment_compact_yield_every: u32 = 8, [Ext: parse_num];
  local_space_limit: u64 = TB, [Run: parse_size];
  reserve_space_ratio: u16 = 100, [Run: parse_num];
  max_cloud_concurrency: u32 = 20, [Run: parse_num];
  max_write_concurrency: u32 = 0, [Run: parse_num];
  cloud_request_threads: u16 = 1, [Run: parse_num];
  standby_max_concurrency: u16 = 1, [Run: parse_num];
  direct_io_buffer_pool_size: u32 = 16, [Run: parse_num];
  write_buffer_size: u64 = MB, [Run: parse_size];
  non_page_io_batch_size: u64 = MB, [Run: parse_size];
  write_buffer_ratio: f64 = 0.05, [Run: parse_f64];
  allow_reuse_local_caches: bool = false, [Run: parse_bool];
  chunk_size: u32 = 256, [Ext: parse_num];
  segment_size: u32 = DEFAULT_SEGMENT_SIZE as u32, [Ext: parse_size_u32];
  gc_global_mem_size_per_shard: u64 = 32 * MB, [Ext: parse_size];
  pinned_tail_scratch_slots: u16 = 8, [Ext: parse_num];
  segments_per_file_shift: u8 = DEFAULT_SEGMENTS_PER_FILE_SHIFT, [Ext: parse_num];

  // `store_path` / `standby_master_store_paths` 一次写路径与权重两字段，
  // 由 `KvOption::apply_ini` 手写处理。
  store_path: Vec<String> = Vec::new(), [Skip: parse_derived_list];
  store_path_weights: Vec<u64> = Vec::new(), [Skip: parse_derived_list];
  store_path_lut_li: Vec<u32> = Vec::new(), [Skip: parse_derived_list];
  cloud_store_path: String = String::new(), [Permanent: parse_str];
  cloud_provider: String = "aws".to_owned(), [Permanent: parse_str];
  cloud_endpoint: String = String::new(), [Permanent: parse_str];
  cloud_region: String = "us-east-1".to_owned(), [Permanent: parse_str];
  cloud_access_key: String = "minioadmin".to_owned(), [Permanent: parse_str];
  cloud_secret_key: String = "minioadmin".to_owned(), [Permanent: parse_str];
  cloud_auto_credentials: bool = false, [Permanent: parse_bool];
  enable_local_standby: bool = false, [Ext: parse_bool];
  standby_master_addr: String = String::new(), [Permanent: parse_str];
  standby_master_store_paths: Vec<String> = Vec::new(), [Skip: parse_derived_list];
  standby_master_store_path_weights: Vec<u64> = Vec::new(), [Skip: parse_derived_list];
  standby_master_store_path_lut_li: Vec<u32> = Vec::new(), [Skip: parse_derived_list];
  cloud_verify_ssl: bool = false, [Permanent: parse_bool];

  data_page_size: u32 = PAGE_ALIGN as u32, [Permanent: parse_size_u32];
  pages_per_file_shift: u8 = DEFAULT_PAGES_PER_FILE_SHIFT, [Ext: parse_num];
  /// 对齐 C++ `KvOption::overflow_pointers`（fkv_page 同名常量同为 16）。
  overflow_pointers: u8 = 16, [Permanent: parse_num];
  /// 追加写开关（对齐 C++ `KvOption::data_append_mode`）。
  ///
  /// Rust 引擎只实现 append 分配模式，本字段属语义登记：`false` 不会切到
  /// `PooledFilePages` 原地复用分配器（不在 L1 兼容范围），
  /// `validate` 会显式提示；cloud / standby 模式
  /// 仍按 C++ 规则强制改写为 `true`。
  data_append_mode: bool = false, [Permanent: parse_bool];
  enable_compression: bool = false, [Ext: parse_bool];
  zstd_compression_level: i32 = 3, [Ext: parse_num];
  prewarm_cloud_cache: bool = false, [Run: parse_bool];
  prewarm_task_count: u16 = 3, [Run: parse_num];
  /// 每读请求的 auto-reopen 尝试轮数（对齐 C++ `auto_reopen_retry_times`）。
  /// 预算挂在请求上；driver 每次只做一次延迟尝试，失败即连坐 waiter。
  auto_reopen_retry_times: u8 = 10, [Ext: parse_num];
  auto_oom_retry_times: u8 = DEFAULT_AUTO_OOM_RETRY_TIMES, [Ext: parse_num];
  /// 单轮 auto-reopen 延迟（微秒，对齐 C++ `auto_reopen_pending_time_us`）。
  auto_reopen_pending_time_us: u64 = 10_000_000, [Ext: parse_num];
  /// 高优先请求连续服务的时间片（微秒；0 = 关闭）。到点且后台队列非空时
  /// 先跑一条后台请求，防 GC / compact / archive 被前台流量饿死。
  max_processing_time_us: u64 = DEFAULT_MAX_PROCESSING_TIME_US, [Ext: parse_num];
  /// 单任务内长循环（scan 页链 / overflow 链 / GC 文件遍历）的协作让出
  /// 段预算（微秒；0 = 关闭）。管单请求 p99，与 `max_processing_time_us`
  /// 的请求边界轮预算分列。
  yield_budget_us: u64 = DEFAULT_YIELD_BUDGET_US, [Ext: parse_num];
  /// 数据面请求默认 deadline（微秒；`0` = 未配置）。
  ///
  /// `0` 时 Local / Standby / Mem 不设 deadline（保持旧行为），Cloud 回退到
  /// [`DEFAULT_CLOUD_REQUEST_TIMEOUT_US`]（一次 S3 重试窗口量级）。
  /// 非 `0` 时两侧都按显式值生效。由 facade `start_runtime` 接到
  /// `Runtime::with_request_timeout`；控制面 attach / detach / shutdown 不套用。
  request_timeout_us: u64 = 0, [Ext: parse_num];
  /// 数据面请求入队等待上限（微秒；`0` = 未配置）。
  ///
  /// `0` 时 Local / Standby / Mem 无限等待（保持旧行为），Cloud 回退到
  /// [`DEFAULT_CLOUD_QUEUE_TIMEOUT_US`]；队列饱和超时回 `Busy`。接线点同
  /// `request_timeout_us`。
  request_queue_timeout_us: u64 = 0, [Ext: parse_num];
}

impl KvOption {
  /// 按 INI 键派发一次赋值；返回 `false` 表示未知键（调用方继续处理派生键）。
  ///
  /// 键表由 `kv_options!` 生成；`store_path` / `standby_master_store_paths`
  /// 一次写路径与权重两字段，在此特判。
  fn apply_ini(&mut self, section: Section, key: &str, value: &str) -> Result<bool> {
    match (section, key) {
      (Section::Permanent, "store_path") => {
        let (path_li, weight_li) = parse_store_path(value)?;
        self.store_path = path_li;
        self.store_path_weights = weight_li;
        Ok(true)
      }
      (Section::Permanent, "standby_master_store_paths") => {
        let (path_li, weight_li) = parse_store_path(value)?;
        self.standby_master_store_paths = path_li;
        self.standby_master_store_path_weights = weight_li;
        Ok(true)
      }
      _ => apply_ini_field(self, section, key, value),
    }
  }

  /// 运行模式派生，优先级对齐 C++ `DeriveStoreMode`：standby 是「本地存储之上的
  /// 复制模式」，先于 cloud 判定。`validate` 在 `cloud_store_path` 非空时会清掉
  /// standby 字段，所以「先 validate 再 derive」仍得 `Cloud`（对齐 C++
  /// `ValidateOption`）；未经 validate 的路径（`StandbyConfig::from_option`）
  /// 保持 standby 优先，避免把 standby 误判成 cloud。
  pub fn derive_mode(&self) -> StoreMode {
    match self.enable_local_standby {
      true => match self.standby_master_addr.is_empty() {
        true => StoreMode::StandbyMaster,
        false => StoreMode::StandbyReplica,
      },
      false if self.cloud_store_path.is_empty() => StoreMode::Local,
      false => StoreMode::Cloud,
    }
  }

  /// 生效的数据面请求 deadline；`None` = 不设（Local / Standby / Mem 的旧行为）。
  ///
  /// `request_timeout_us = 0` 视为未配置：Cloud 回退
  /// [`DEFAULT_CLOUD_REQUEST_TIMEOUT_US`]。Cloud 判定用 `cloud_store_path`
  /// 非空——`validate` 对 cloud 会清 standby 字段，所以这就是 validate 后的
  /// 终态模式，避免拿未收敛的 standby 开关误判。
  ///
  /// 注意：`0` 是「未配置」而非「显式关闭」，Cloud 的默认窗口无法用 `0` 关掉。
  pub fn request_timeout(&self) -> Option<Duration> {
    self.resolve_timeout(self.request_timeout_us, DEFAULT_CLOUD_REQUEST_TIMEOUT_US)
  }

  /// 生效的数据面入队等待上限；`None` = 无限等待（Local / Standby / Mem 的旧行为）。
  ///
  /// `request_queue_timeout_us = 0` 视为未配置：Cloud 回退
  /// [`DEFAULT_CLOUD_QUEUE_TIMEOUT_US`]。Cloud 判定同 [`Self::request_timeout`]。
  pub fn queue_timeout(&self) -> Option<Duration> {
    self.resolve_timeout(
      self.request_queue_timeout_us,
      DEFAULT_CLOUD_QUEUE_TIMEOUT_US,
    )
  }

  /// 超时解析共用：显式值优先；`0` 在 Cloud 下回退 `cloud_default_us`，
  /// 其余模式返回 `None`（旧行为）。
  fn resolve_timeout(&self, timeout_us: u64, cloud_default_us: u64) -> Option<Duration> {
    let timeout_us = match timeout_us {
      0 if self.cloud_store_path.is_empty() => return None,
      0 => cloud_default_us,
      timeout_us => timeout_us,
    };
    Some(Duration::from_micros(timeout_us))
  }

  /// 对齐 `ValidateOption`：校验失败返回 `InvalidArgs`，必要时 rewrite 字段。
  pub fn validate(&mut self) -> Result<()> {
    while self.cloud_store_path.ends_with('/') {
      self.cloud_store_path.pop();
    }
    // GCS/Google 系 provider 未显式给 endpoint 时补官方默认（对齐 C++ `ValidateOption`）。
    if self.cloud_endpoint.is_empty()
      && (self.cloud_provider.eq_ignore_ascii_case("gcs")
        || self.cloud_provider.eq_ignore_ascii_case("google")
        || self.cloud_provider.eq_ignore_ascii_case("google-cloud"))
    {
      self.cloud_endpoint = GCS_ENDPOINT.to_owned();
    }
    if self.num_threads == 0
      || self.data_page_restart_interval == 0
      || self.index_page_restart_interval == 0
      || self.max_inflight_write == 0
      || self.io_queue_size == 0
      || self.max_global_request_batch == 0
      || self.max_write_batch_pages == 0
      || self.data_page_size == 0
      || !u64::from(self.data_page_size).is_multiple_of(PAGE_ALIGN)
      // `data_file_size = data_page_size << shift` 必须落在 u64 内：4KB 页在
      // shift >= 52 时高位被静默移出、release 下回绕成 0，必须在这里拒绝。
      || u64::from(self.data_page_size)
        .checked_shl(u32::from(self.pages_per_file_shift))
        .is_none_or(|size| {
          size >> self.pages_per_file_shift != u64::from(self.data_page_size)
        })
      || self.segment_size == 0
      || !u64::from(self.segment_size).is_multiple_of(PAGE_ALIGN)
      || !u64::from(self.coroutine_stack_size).is_multiple_of(PAGE_ALIGN)
      || self.non_page_io_batch_size == 0
      || !self.non_page_io_batch_size.is_multiple_of(PAGE_ALIGN)
    {
      return Err(KvError::InvalidArgs);
    }
    if self.overflow_pointers == 0 || self.overflow_pointers > MAX_OVERFLOW_POINTERS {
      return Err(KvError::InvalidArgs);
    }
    if (self.write_buffer_size != 0 || self.write_buffer_ratio != 0.0)
      && (self.write_buffer_size == 0
        || !(0.0..1.0).contains(&self.write_buffer_ratio)
        || !self.write_buffer_size.is_multiple_of(PAGE_ALIGN))
    {
      return Err(KvError::InvalidArgs);
    }
    if !self.cloud_store_path.is_empty() {
      self.enable_local_standby = false;
      self.standby_master_addr.clear();
      self.standby_master_store_paths.clear();
      self.standby_master_store_path_weights.clear();
      self.standby_master_store_path_lut_li.clear();
    }
    // 多根目录：路径 / 权重 / LUT 三者同源校验（standby 与非 standby 一致）。
    // 显式权重必须与路径一一对应且非 0；未给权重时按等权处理（对齐 C++
    // `InitStoreSpace` 无 `store_path_weights` 时的默认槽位）。
    if self.store_path.is_empty() {
      if !self.store_path_weights.is_empty() {
        return Err(KvError::InvalidArgs);
      }
      self.store_path_lut_li.clear();
    } else {
      if !self.store_path_weights.is_empty()
        && self.store_path_weights.len() != self.store_path.len()
      {
        return Err(KvError::InvalidArgs);
      }
      if self.store_path_weights.contains(&0) {
        return Err(KvError::InvalidArgs);
      }
      if self.store_path_weights.is_empty() {
        self.store_path_weights = vec![1; self.store_path.len()];
      }
      self.store_path_lut_li = compute_store_path_lut(&self.store_path_weights);
      if self.store_path_lut_li.is_empty() {
        return Err(KvError::InvalidArgs);
      }
    }
    // standby master 路径同样生成 LUT（C++ 在 InitStoreSpace 一并构建）。
    if self.standby_master_store_paths.is_empty() {
      if !self.standby_master_store_path_weights.is_empty() {
        return Err(KvError::InvalidArgs);
      }
      self.standby_master_store_path_lut_li.clear();
    } else {
      if !self.standby_master_store_path_weights.is_empty()
        && self.standby_master_store_path_weights.len() != self.standby_master_store_paths.len()
      {
        return Err(KvError::InvalidArgs);
      }
      if self.standby_master_store_path_weights.contains(&0) {
        return Err(KvError::InvalidArgs);
      }
      if self.standby_master_store_path_weights.is_empty() {
        self.standby_master_store_path_weights = vec![1; self.standby_master_store_paths.len()];
      }
      self.standby_master_store_path_lut_li =
        compute_store_path_lut(&self.standby_master_store_path_weights);
    }
    if !self.enable_local_standby && !self.standby_master_addr.is_empty() {
      return Err(KvError::InvalidArgs);
    }
    if self.enable_local_standby {
      if self.store_path.is_empty() {
        return Err(KvError::InvalidArgs);
      }
      // standby 必须追加写：非追加模式的 fdatasync 语义与 standby 追赶冲突。
      if !self.data_append_mode {
        warn!("standby 模式要求追加写，自动开启 data_append_mode");
        self.data_append_mode = true;
      }
    }
    if !self.cloud_store_path.is_empty() {
      if self.max_cloud_concurrency == 0 || self.cloud_request_threads == 0 {
        return Err(KvError::InvalidArgs);
      }
      if self.max_write_concurrency == 0 {
        self.max_write_concurrency = self.max_cloud_concurrency;
        warn!(
          "cloud 模式未设置 max_write_concurrency，回退为 max_cloud_concurrency {}",
          self.max_write_concurrency
        );
      }
      if self.local_space_limit == 0 {
        self.local_space_limit = TB;
        warn!("cloud 模式 local_space_limit=0，回退默认 {} 字节", TB);
      }
      // `cloud_auto_credentials=false` 时必须显式给 key：否则 S3 客户端会
      // 静默走环境凭据链，与配置语义相反。
      if !self.cloud_auto_credentials
        && (self.cloud_access_key.is_empty() || self.cloud_secret_key.is_empty())
      {
        return Err(KvError::InvalidArgs);
      }
      // 自动凭据且未给完整静态 key 时清空两者，让 S3 builder 走默认凭据链
      // （对齐 C++ `cloud_auto_credentials && secret.empty()`）；只清一个会
      // 让 builder 报 MissingSecretAccessKey。
      if self.cloud_auto_credentials
        && (self.cloud_access_key.is_empty() || self.cloud_secret_key.is_empty())
      {
        self.cloud_access_key.clear();
        self.cloud_secret_key.clear();
        warn!("cloud_auto_credentials=true 且未给完整静态 key，使用默认凭据链");
      }
      if !self.data_append_mode {
        warn!("cloud 模式要求追加写，自动开启 data_append_mode");
        self.data_append_mode = true;
      }
    }
    // pooled 分配器不在 L1 兼容范围：显式提示该字段在 Rust 侧是空操作，
    // 避免「配置了 false 却仍按 append 写」被当成静默陷阱。
    if !self.data_append_mode {
      warn!("Rust 仅实现 append 分配模式，data_append_mode=false 不生效（pooled 不在 L1 范围）");
    }
    // 零消费配置：仅当调用方显式改成非默认值时才告警，避免默认配置刷屏。
    // 决策登记见模块文档「未迁移字段」。
    let default_opts = KvOption::default();
    let mut ignored_li: Vec<&str> = Vec::new();
    macro_rules! track_ignored {
      ($($field:ident),* $(,)?) => {
        $(
          if self.$field != default_opts.$field {
            ignored_li.push(stringify!($field));
          }
        )*
      };
    }
    track_ignored!(
      enable_data_page_cache,
      init_page_count,
      coroutine_stack_size,
      max_archive_tasks,
      reserve_space_ratio,
      direct_io_buffer_pool_size,
      chunk_size,
      gc_global_mem_size_per_shard,
      pinned_tail_scratch_slots,
      cloud_request_threads,
      write_buffer_size,
      write_buffer_ratio,
      non_page_io_batch_size,
      segment_compact_yield_every,
    );
    if !ignored_li.is_empty() {
      warn!(
        "以下配置在 Rust 侧未接线（决策登记见 fkv_config 模块文档），已忽略：{}",
        ignored_li.join(", ")
      );
    }
    Ok(())
  }
}

impl KvOption {
  /// 读取 `.ini`；键按 `[run]` / `[permanent]` 归属生效，缺任一节报错。
  pub fn load_from_ini(&mut self, path: impl AsRef<Path>) -> Result<()> {
    let text = read_to_string(path).map_err(|_| KvError::IoFail)?;
    let mut data_file_size = None;
    // 显式 shift 与派生的 `data_file_size` 同给时不能静默覆盖前者。
    let mut explicit_shift = false;
    let (mut has_run, mut has_permanent) = (false, false);
    let mut section = None;
    for raw in text.lines() {
      let line = raw.trim();
      if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
        continue;
      }
      if let Some(name) = line
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
      {
        section = match name.trim() {
          "run" => {
            has_run = true;
            Some(Section::Run)
          }
          "permanent" => {
            has_permanent = true;
            Some(Section::Permanent)
          }
          _ => None,
        };
        continue;
      }
      let Some((key, value)) = line.split_once('=') else {
        continue;
      };
      let Some(section) = section else {
        continue;
      };
      let key = key.trim();
      let value = value.trim();
      // `data_file_size` 是派生键：换算成 `pages_per_file_shift`，不直接落字段。
      if section == Section::Permanent && key == "data_file_size" {
        data_file_size = Some(parse_size(value)?);
        continue;
      }
      // 键表由 `kv_options!` 生成，未知键静默忽略（对齐 C++ `LoadFromIni`）。
      if !self.apply_ini(section, key, value)? {
        continue;
      }
      // `pages_per_file_shift` 是 Ext 键（两节均可），显式给出时要压过派生值。
      if key == "pages_per_file_shift" {
        explicit_shift = true;
      }
    }
    if !has_run || !has_permanent {
      return Err(KvError::InvalidArgs);
    }
    // 两者都显式给出时保留显式 shift 并告警：无条件用 `data_file_size` 覆盖会让
    // 调用方明确写的布局参数失效（页数换算 / file_id 位宽全变）。
    if let Some(size) = data_file_size {
      if explicit_shift {
        warn!(
          "data_file_size {size} 与显式 pages_per_file_shift={} 冲突，保留显式值",
          self.pages_per_file_shift
        );
      } else {
        let page_size = u64::from(self.data_page_size);
        if page_size == 0 {
          return Err(KvError::InvalidArgs);
        }
        let pages = size / page_size;
        self.pages_per_file_shift = if pages == 0 {
          warn!("data_file_size {size} 小于 data_page_size {page_size}，回退单页");
          0
        } else {
          let shift = pages.ilog2();
          if !pages.is_power_of_two() {
            warn!(
              "data_file_size {size} 非 data_page_size {page_size} 的二次幂倍数，向下取整到 {} 字节",
              page_size << shift
            );
          }
          shift as u8
        };
      }
    }
    Ok(())
  }
}

/// `.ini` 节；键只在所属节内生效（对齐 C++ `LoadFromIni` 的 `HasValue(section, key)`）。
///
/// `Ext` / `Skip` 不是真实节：宏表里分别标记 fkv 扩展键（两节均可）与无 INI 键字段。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
  Run,
  Permanent,
  Ext,
  Skip,
}

/// INI 键所属节是否匹配当前行所在节；`Ext` 通配两节，`Skip` 永不匹配。
fn section_matches(allowed: Section, actual: Section) -> bool {
  match allowed {
    Section::Ext => true,
    Section::Run | Section::Permanent => allowed == actual,
    Section::Skip => false,
  }
}

fn parse_bool(value: &str) -> Result<bool> {
  match value.to_ascii_lowercase().as_str() {
    "1" | "true" | "yes" | "on" => Ok(true),
    "0" | "false" | "no" | "off" => Ok(false),
    _ => Err(KvError::InvalidArgs),
  }
}

fn parse_num<T: FromStr>(value: &str) -> Result<T> {
  value.parse().map_err(|_| KvError::InvalidArgs)
}

fn parse_str(value: &str) -> Result<String> {
  Ok(value.to_owned())
}

/// 派生列表字段（权重 / lut）无 INI 键，`[Skip: ...]` 分支永不命中；
/// 保留类型一致的解析器让单遍展开的表统一，不产生特例分支。
fn parse_derived_list<T>(_value: &str) -> Result<Vec<T>> {
  Ok(Vec::new())
}

fn parse_f64(value: &str) -> Result<f64> {
  value.parse().map_err(|_| KvError::InvalidArgs)
}

fn size_to_u32(value: u64) -> Result<u32> {
  u32::try_from(value).map_err(|_| KvError::InvalidArgs)
}

fn parse_size_u32(value: &str) -> Result<u32> {
  size_to_u32(parse_size(value)?)
}

fn parse_size(value: &str) -> Result<u64> {
  let value = value.trim();
  let split = value
    .find(|ch: char| !ch.is_ascii_digit())
    .unwrap_or(value.len());
  let (num, unit) = value.split_at(split);
  let num: u64 = num.parse().map_err(|_| KvError::InvalidArgs)?;
  // 对齐 C++ `ParseSizeWithUnit`：只认两位单位尾缀 `KB/MB/GB/TB`（大小写不敏感），
  // 无单位按字节。C++ 对解析失败静默返回 0，这里保持 `InvalidArgs`：
  // 宁可启动报错，也不静默降级成 0 让配置悄悄漂移。
  let unit = unit.trim();
  let mult = if unit.is_empty() {
    1
  } else if unit.eq_ignore_ascii_case("kb") {
    KB
  } else if unit.eq_ignore_ascii_case("mb") {
    MB
  } else if unit.eq_ignore_ascii_case("gb") {
    GB
  } else if unit.eq_ignore_ascii_case("tb") {
    TB
  } else {
    return Err(KvError::InvalidArgs);
  };
  num.checked_mul(mult).ok_or(KvError::InvalidArgs)
}

/// 解析 `path1,path2:w1,w2`（对齐 C++ `ParseStorePathListWithWeights`）。
/// 无 `:w` 后缀时权重为空；权重个数与路径不符报错。
fn parse_store_path(value: &str) -> Result<(Vec<String>, Vec<u64>)> {
  let (path_part, weight_part) = value.split_once(':').map_or((value, ""), |(p, w)| (p, w));
  let paths = split_list(path_part);
  if paths.is_empty() {
    return Err(KvError::InvalidArgs);
  }
  if weight_part.is_empty() {
    return Ok((paths, Vec::new()));
  }
  let weights = parse_u64_list(weight_part)?;
  if weights.len() != paths.len() {
    return Err(KvError::InvalidArgs);
  }
  Ok((paths, weights))
}

fn split_list(value: &str) -> Vec<String> {
  value
    .split(',')
    .map(str::trim)
    .filter(|item| !item.is_empty())
    .map(str::to_owned)
    .collect()
}

fn parse_u64_list(value: &str) -> Result<Vec<u64>> {
  split_list(value)
    .iter()
    .map(|item| item.parse().map_err(|_| KvError::InvalidArgs))
    .collect()
}
