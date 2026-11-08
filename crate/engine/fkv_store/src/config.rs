//! Store 构造与配置投影：builder 选项、页缓存预算、路径索引。

use std::collections::VecDeque;

use fkv_io::PageIo;
use fkv_large::SegmentLayout;
use fkv_mapping::{AllocMode, GenerationSource, SnapshotRegistry};
use fkv_types::{
  DEFAULT_MANIFEST_LIMIT, DEFAULT_PAGES_PER_FILE_SHIFT, MAIN_BRANCH_NAME, TableIdent,
  is_valid_branch_name, manifest_file_name, normalize_branch_name,
};
use gxhash::HashMap;

use crate::Store;

impl<I: PageIo> Store<I> {
  /// `restart_interval` 同时作为 data / index 页的初始间隔；需分别配置时用
  /// [`Store::with_index_restart_interval`] 覆盖 index 侧。
  pub fn new(io: I, page_size: usize, restart_interval: u16) -> Self {
    Self {
      io,
      page_size,
      data_restart_interval: restart_interval,
      index_restart_interval: restart_interval,
      table_map: HashMap::default(),
      large_value_layout: None,
      large_value_threshold: 0,
      branch_name: MAIN_BRANCH_NAME.to_owned(),
      term: 0,
      manifest_file: None,
      branch_file_limit: None,
      branch_segment_limit: None,
      pages_per_file_shift: DEFAULT_PAGES_PER_FILE_SHIFT,
      manifest_limit: DEFAULT_MANIFEST_LIMIT as usize,
      ttl_read_filter: true,
      compression_level: None,
      overflow_pointers: fkv_page::DEFAULT_OVERFLOW_POINTERS,
      file_amplify_factor: 0,
      segment_file_amplify_factor: 0,
      max_open_tables: 0,
      root_meta_cache_bytes: 0,
      root_meta_used: 0,
      table_order_li: VecDeque::new(),
      touch_seq: HashMap::default(),
      touch_clock: 0,
      defer_ttl: false,
      page_cache_bytes: fkv_config::DEFAULT_PAGE_CACHE_BYTES as usize,
      snapshots: SnapshotRegistry::new(),
      generation_source: GenerationSource::new(),
      yield_budget_us: 0,
      alloc_mode: AllocMode::Append,
    }
  }

  /// 物理页分配策略：默认 `Append`；`Pooled` 对齐 C++ `data_append_mode=false`。
  pub fn with_alloc_mode(mut self, alloc_mode: AllocMode) -> Self {
    self.alloc_mode = alloc_mode;
    self
  }

  /// 长循环的协作让出段预算（微秒；`0` = 关闭）。
  ///
  /// 与 shard 的 `max_processing_time_us`（请求边界轮预算）分列：本值管单个
  /// 请求内部的长段（scan 页链 / overflow 链 / GC 文件遍历），默认 20us。
  pub fn with_yield_budget_us(mut self, yield_budget_us: u64) -> Self {
    self.yield_budget_us = yield_budget_us;
    self
  }

  pub fn yield_budget_us(&self) -> u64 {
    self.yield_budget_us
  }

  /// 每表页缓存（读写共用）字节预算（`0` = 关闭页缓存）。
  ///
  /// 写路径的 COW 每批都要读回上一批刚写下的同一路径（root → 内部 index →
  /// leaf index → data 页），读路径的每一页页读也要落盘。本缓存把这些页留
  /// 在内存里，读写两侧共用同一实例：写出的页紧接着就能被读命中，读过的页
  /// 也会被后续读请求命中（对齐 C++ 单池 buffer pool / swizzling 句柄）。
  ///
  /// 预算即 `buffer_pool_size`（C++ 单池口径），超出即按 Clock 逐出；
  /// `0` 显式关闭，读写路径都退化为直连 IO，盘面逐字节不变。
  pub fn with_page_cache_bytes(mut self, bytes: usize) -> Self {
    self.page_cache_bytes = bytes;
    self
  }

  /// 当前页缓存预算（`0` = 关闭）。
  pub fn page_cache_bytes(&self) -> usize {
    self.page_cache_bytes
  }

  /// index 页 restart 间隔（对齐 C++ `KvOption::index_page_restart_interval`）。
  ///
  /// `Store::new` 的第三参同时作为 data / index 初始值；需要与 data 页不同时
  /// 由调用方（SDK / shard）用本方法覆盖 index 侧。
  pub fn with_index_restart_interval(mut self, index_restart_interval: u16) -> Self {
    self.index_restart_interval = index_restart_interval;
    self
  }

  pub fn with_manifest_limit(mut self, manifest_limit: usize) -> Self {
    self.manifest_limit = manifest_limit;
    self
  }

  /// 读路径是否过滤 `expire_ts <= now` 的 entry（默认 true）。
  ///
  /// 关闭后过期项只在 CleanExpired 物理删除后消失。
  pub fn with_ttl_read_filter(mut self, enabled: bool) -> Self {
    self.ttl_read_filter = enabled;
    self
  }

  /// 启用写路径值压缩（zstd level）：`>= SKIP_COMPRESSION_THRESHOLD` 的值
  /// 压后更小才落盘，读路径按 entry 的 compression flag 自动解压。
  ///
  /// 未启用（默认 `compression_level == None`）时新写入一律 raw，即使 manifest
  /// 恢复了历史字典也不复用（对齐 C++ `enable_compression` 门控）；历史字典
  /// 压缩页仍按页级 flag + 恢复的字典可读。重新启用后新写入继续使用该字典。
  pub fn with_compression_level(mut self, level: i32) -> Self {
    self.compression_level = Some(level);
    self
  }

  /// 是否校验读到的 page checksum（默认开启；对齐 `KvOption::skip_verify_checksum`）。
  pub fn with_verify_checksum(self, enabled: bool) -> Self {
    self.io.set_verify_checksum(enabled);
    self
  }

  /// 写后是否延迟 TTL 清理（默认 false，保持内联清理）。
  /// 开启后用 `drain_pending_ttl*` 执行清理。
  pub fn with_deferred_ttl(mut self, defer_ttl: bool) -> Self {
    self.defer_ttl = defer_ttl;
    self
  }

  /// 限制同时驻留内存的 table 数；超限驱逐最久未写的表（0 = 不限制）。
  pub fn with_max_open_tables(mut self, max_open_tables: usize) -> Self {
    self.max_open_tables = max_open_tables;
    self
  }

  /// 表级 root 状态字节预算；`0` = 不限制（对齐 C++ `root_meta_cache_size`）。
  ///
  /// 计入 data / segment mapping、页缓存、压缩字典与 branch 元数据。
  /// 超限时从 LRU 尾驱逐未被读快照引用、字典已持久化的整表状态，之后按需
  /// reopen 重载（不实现 C++ `pending_erase`：Rust 单 actor + Arc 快照生命周期
  /// 天然避免 entry re-enter）。
  ///
  /// `Store::new` 默认 `0`（不限制）；产品路径由 shard 从 `KvOption` 注入。
  pub fn with_root_meta_cache_bytes(mut self, bytes: usize) -> Self {
    self.root_meta_cache_bytes = bytes;
    self
  }

  /// 当前配置的表级 root 状态字节预算（`0` = 不限制）。
  pub fn root_meta_cache_bytes(&self) -> usize {
    self.root_meta_cache_bytes
  }

  /// 设置文件空间放大触发阈值（对齐 C++ `file_amplify_factor`）；0 关闭。
  pub fn with_file_amplify_factor(mut self, factor: u8) -> Self {
    self.file_amplify_factor = factor;
    self
  }

  /// 设置 segment 文件空间放大触发阈值（对齐 C++ `segment_file_amplify_factor`）；0 关闭。
  pub fn with_segment_amplify_factor(mut self, factor: u8) -> Self {
    self.segment_file_amplify_factor = factor;
    self
  }

  /// 设置一批 overflow 页的最大指针数（对齐 C++ `overflow_pointers`）。
  pub fn with_overflow_pointers(mut self, overflow_pointers: u8) -> Self {
    self.overflow_pointers = overflow_pointers;
    self
  }

  /// 让后端预加载 table 目录的 file id -> path 索引，返回索引文件数。
  pub fn preload_path_li(&self, tbl: &TableIdent) -> usize {
    self.io.preload_path_li(tbl)
  }

  /// 外部同步工具改动目录后主动失效后端路径索引。
  pub fn invalidate_path_cache(&self) {
    self.io.invalidate_path_cache();
  }

  pub fn with_branch(mut self, branch_name: impl Into<String>, term: u64) -> Self {
    let branch_name = branch_name.into();
    let normalized = match normalize_branch_name(&branch_name) {
      Some(name) => name.into_string(),
      None => {
        // r9b-03：与同族入口（prepare_attach / adopt_attach / open_branch 均以
        // InvalidArgs 拒绝）及 C++ NormalizeBranchName 的 fail-fast 口径对齐，
        // 归一化失败不再静默沿用非法原名——否则文件身份与 manifest 元数据会
        // 携带非法名落盘，直到某个控制面入口才炸出 InvalidArgs。builder 无
        // Result 通道：debug 构建（含测试）直接断言炸出；release 回退
        // MAIN_BRANCH_NAME 并告警，非法身份运行时可见、不再静默生效。
        debug_assert!(
          is_valid_branch_name(&branch_name),
          "with_branch 收到非法 branch 名 {branch_name:?}：合法字符集 [a-zA-Z0-9-]",
        );
        log::warn!("with_branch 收到非法 branch 名 {branch_name:?}，回退 {MAIN_BRANCH_NAME}");
        MAIN_BRANCH_NAME.to_owned()
      }
    };
    self.io.set_file_identity(&normalized, term);
    self.branch_name = normalized;
    self.term = term;
    self
  }

  pub fn with_pages_per_file_shift(mut self, pages_per_file_shift: u8) -> Self {
    self.pages_per_file_shift = pages_per_file_shift;
    self
  }

  /// attach 时更新 active term；后续 manifest / archive 记录使用该 term。
  ///
  /// 已在 branch manifest 模式（`manifest_file` 由 `adopt_attach` /
  /// `open_branch` 建立）时同步重算具名文件名（sem-60：名字带 attach term），
  /// 维持 `manifest_file == manifest_file_name(branch, term)` 不变式，否则
  /// reopen 读新名、首写却发布旧名；main / `with_branch` 的活跃 manifest 模式
  /// （无具名文件）不因换 term 而切换模式。
  pub fn set_term(&mut self, term: u64) {
    self.term = term;
    if self.manifest_file.is_some() {
      self.manifest_file = Some(manifest_file_name(&self.branch_name, term));
    }
    self.io.set_file_identity(&self.branch_name, term);
  }

  /// 当前 Store 采纳的 term（分支 manifest / 文件身份用）。
  pub fn term(&self) -> u64 {
    self.term
  }

  /// 启用 Store 级大 value：`value.len() >= threshold` 的 upsert 写入 segment 文件。
  pub fn with_large_value(mut self, layout: SegmentLayout, threshold: usize) -> Self {
    self.large_value_layout = Some(layout);
    self.large_value_threshold = threshold;
    self
  }

  pub fn page_size(&self) -> usize {
    self.page_size
  }
}
