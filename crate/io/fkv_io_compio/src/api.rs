//! `CompioStore` 构造、配置投影与诊断计数。

#[cfg(target_os = "linux")]
use std::sync::{Arc, Mutex};
use std::{
  cell::{Cell, RefCell},
  path::{Path, PathBuf},
  sync::atomic::{AtomicU64, Ordering},
};

use fkv_config::{DEFAULT_MAX_INFLIGHT_WRITE, DEFAULT_MAX_WRITE_BATCH_PAGES};
use fkv_types::{
  DEFAULT_SEGMENT_SIZE, DEFAULT_SEGMENTS_PER_FILE_SHIFT, MAIN_BRANCH_NAME, PAGE_ALIGN, TableIdent,
};
use gxhash::{HashMap, HashSet};

#[cfg(target_os = "linux")]
use crate::FixedWritePool;
use crate::{
  BufPool, CompioStore, FdCache, FileIdentity, FileKind, SyncInflight, WriteGate,
  managed_segment_read_supported,
};

impl CompioStore {
  pub fn new(root: impl Into<PathBuf>, page_size: usize, pages_per_file_shift: u8) -> Self {
    Self {
      root: root.into(),
      page_size,
      max_write_batch_pages: DEFAULT_MAX_WRITE_BATCH_PAGES as usize,
      max_inflight_write: DEFAULT_MAX_INFLIGHT_WRITE as usize,
      pages_per_file_shift,
      segment_size: DEFAULT_SEGMENT_SIZE,
      segments_per_file_shift: DEFAULT_SEGMENTS_PER_FILE_SHIFT,
      skip_verify_checksum: Cell::new(false),
      direct_io: Cell::new(false),
      direct_seg: Cell::new(false),
      direct_opened: Cell::new(0),
      direct_fallback_warned: Cell::new(0),
      direct_fallback_num: AtomicU64::new(0),
      direct_discard_num: AtomicU64::new(0),
      direct_open_err: Cell::new(0),
      direct_open_err_path: RefCell::new(None),
      managed_seg: Cell::new(false),
      direct_pool: RefCell::new(HashMap::default()),
      identity: RefCell::new(FileIdentity {
        branch: MAIN_BRANCH_NAME.to_owned(),
        term: 0,
      }),
      dir_index_map: RefCell::new(HashMap::default()),
      dir_ready: RefCell::new(HashSet::default()),
      fd_cache: RefCell::new(FdCache::default()),
      read_buf: RefCell::new(BufPool::default()),
      write_buf: RefCell::new(BufPool::default()),
      write_gate: WriteGate::new(DEFAULT_MAX_INFLIGHT_WRITE as usize),
      sync_inflight: SyncInflight::default(),
      #[cfg(target_os = "linux")]
      fixed_write: RefCell::new(FixedWritePool::default()),
      dirty: RefCell::new(HashMap::default()),
      dirty_dir: RefCell::new(HashSet::default()),
      manifest_dir_synced: RefCell::new(HashSet::default()),
      manifest_offset: RefCell::new(HashMap::default()),
      short_write_bytes: Cell::new(None),
      short_read_bytes: Cell::new(None),
      read_try_again_times: Cell::new(0),
      segment_preallocated: RefCell::new(HashSet::default()),
      data_scan_num: AtomicU64::new(0),
      segment_scan_num: AtomicU64::new(0),
      fd_open_num: AtomicU64::new(0),
      write_at_num: AtomicU64::new(0),
      read_at_num: AtomicU64::new(0),
      read_whole_chunk_num: AtomicU64::new(0),
      stale_read_num: AtomicU64::new(0),
      data_sync_num: AtomicU64::new(0),
      dir_fsync_num: AtomicU64::new(0),
      data_fsync_num: AtomicU64::new(0),
      manifest_append_num: AtomicU64::new(0),
      manifest_sync_num: AtomicU64::new(0),
      #[cfg(target_os = "linux")]
      fixed_io: Arc::new(Mutex::new(None)),
    }
  }

  pub fn with_segment_config(mut self, segment_size: usize, segments_per_file_shift: u8) -> Self {
    self.segment_size = segment_size;
    self.segments_per_file_shift = segments_per_file_shift;
    self
  }

  /// 单组合并写页数上限（对齐 C++ `max_write_batch_pages`）；0 按 1 处理。
  pub fn with_max_write_batch_pages(mut self, pages: usize) -> Self {
    self.max_write_batch_pages = pages.max(1);
    self
  }

  /// IO 层在飞 IO 上限（对齐 C++ `max_inflight_write`）；0 按 1 处理。
  ///
  /// 同一个值既作跨调用的写闸门容量，也作单次 `write_page_li` 的提交窗口；
  /// `sync_data` 的 fdatasync 并发度同样以它为上限（脏文件更少时取脏文件数），
  /// 因此全局只有这一个在飞 IO 上限；`max_write_batch_pages` 只决定每组页数，
  /// 不改变总额。
  pub fn with_max_inflight_write(mut self, limit: usize) -> Self {
    self.max_inflight_write = limit.max(1);
    self.write_gate.set_limit(self.max_inflight_write);
    self
  }

  /// 当前生效的在飞 IO 上限（诊断 / 测试观察用）。
  pub fn max_inflight_write(&self) -> usize {
    self.max_inflight_write
  }

  /// data / segment 使用 O_DIRECT（仅 Linux 生效；data 要求 page_size、segment
  /// 还要求 segment_size 是 4 KiB 整数倍）。manifest 仍走 buffered：其 record
  /// 长度由上层 framing 决定，IO 层不做补零。
  ///
  /// **不支持 O_DIRECT 的文件系统（tmpfs / 部分 overlay / 网络文件系统）**：
  /// open 返回 `EINVAL` / `EOPNOTSUPP` 时按路径类别一次性回退 buffered 并打
  /// warn 日志（io-18），不会让该分区后续所有页 / 段 IO 持续 `IoFail`。这是
  /// 对 C++（硬编码 O_DIRECT、无回退）的**有意偏离**：C++ 在同一路径上直接
  /// 失败，fkv 选择降级运行并在日志中保留诊断。
  ///
  /// segment 也页对齐（`direct_segment()`）时，segment 读走 `read_segment` 的
  /// O_DIRECT 分支并直接 return，provided-buffer（managed）读不再可达；因此会
  /// 显式关闭 [`Self::with_managed_segment_read`] 请求并打 warn 日志（io-16）。
  /// 仅 data 页 direct、segment_size 未页对齐时 segment 文件仍是 buffered，
  /// managed 开关不受影响。
  pub fn with_direct_io(self, enabled: bool) -> Self {
    let direct = enabled && self.page_size.is_multiple_of(PAGE_ALIGN);
    self.direct_io.set(direct);
    self.direct_seg.set(direct);
    // 冲突条件按 segment 的实际路径判定：只有 segment 文件也 O_DIRECT
    // （`segment_size` 页对齐）时才关闭 managed；data 页 direct 不构成冲突。
    if self.managed_seg.get() && self.direct_segment() {
      warn_managed_segment_read_disabled(self.segment_size);
      self.managed_seg.set(false);
    }
    self
  }

  /// 当前 data 页 O_DIRECT 是否生效（不支持时回退后为 false；诊断 / 测试用）。
  ///
  /// 回退是**按路径类别**的：data 回退不影响仍可用的 segment O_DIRECT。
  pub fn direct_io_enabled(&self) -> bool {
    self.direct_io.get()
  }

  /// 当前 segment 文件 O_DIRECT 是否生效（`segment_size` 页对齐且未回退）。
  pub fn direct_segment_enabled(&self) -> bool {
    self.direct_segment()
  }

  /// O_DIRECT open 因文件系统不支持（`EINVAL` / `EOPNOTSUPP`）回退 buffered
  /// 的累计次数（诊断 / 测试用）。
  pub fn direct_fallback_num(&self) -> u64 {
    self.direct_fallback_num.load(Ordering::Relaxed)
  }

  /// O_DIRECT open 成功返回时类别开关已被并发回退关闭、句柄被丢弃并按
  /// buffered 重开的累计次数（io-21 竞态诊断 / 测试用）。
  ///
  /// 非零说明发生过「失败 open 先完成回退、成功 open 后完成」的交错；这些
  /// direct 句柄没有进入 fd 缓存，类别开关保持与缓存内容一致。
  pub fn direct_discard_num(&self) -> u64 {
    self.direct_discard_num.load(Ordering::Relaxed)
  }

  /// 测试注入：让指定类别的 O_DIRECT open 返回 `EINVAL`，验证回退与告警。
  ///
  /// `segment == false` 注入 data 文件，`true` 注入 segment 文件；可分别注入，
  /// 用于验证两类路径独立回退。只影响后续 open，不影响已缓存的句柄。
  #[doc(hidden)]
  pub fn inject_direct_open_einval(&self, segment: bool) {
    let kind = if segment {
      FileKind::Segment
    } else {
      FileKind::Data
    };
    self
      .direct_open_err
      .set(self.direct_open_err.get() | kind.bit());
  }

  /// 测试注入：只让指定路径的 O_DIRECT open 返回 `EINVAL`。
  ///
  /// 与 [`Self::inject_direct_open_einval`] 的类别级注入不同，并发竞态用例
  /// 需要同一类别的另一条 open 真实成功：失败 open 先返回并关闭类别开关，
  /// 成功 open 后完成，验证其 direct 句柄不会落缓存（io-21）。
  #[doc(hidden)]
  pub fn inject_direct_open_einval_path(&self, path: impl Into<PathBuf>) {
    *self.direct_open_err_path.borrow_mut() = Some(path.into());
  }

  /// segment 读请求走内核 provided-buffer（compio `read_managed_at`）。
  ///
  /// **与 O_DIRECT 互斥（io-16）**：O_DIRECT 段读要求缓冲地址 / 长度页对齐，
  /// 而 compio runtime 缓冲池的默认 `BoxAllocator` 不保证地址页对齐，无法作为
  /// O_DIRECT 读目标。因此 `direct_segment()` 为真时本开关不生效：构造时打
  /// warn 日志并保持关闭，`read_segment` 走对齐缓冲分支。生产装配（shard /
  /// cloud local cache）默认开启 O_DIRECT，故本开关只在 buffered segment
  /// 配置下生效。
  ///
  /// 须在 `with_segment_config` 之后调用，开关按最终 `segment_size` 判定；
  /// 若之后才开启 O_DIRECT，[`Self::with_direct_io`] 同样会关闭并告警。
  ///
  /// 池未配置、耗尽或短读时自动回落自管读池，开关只影响性能不影响正确性。
  pub fn with_managed_segment_read(self) -> Self {
    if self.direct_segment() {
      warn_managed_segment_read_disabled(self.segment_size);
      self.managed_seg.set(false);
    } else {
      self
        .managed_seg
        .set(managed_segment_read_supported(self.segment_size));
    }
    self
  }

  /// managed segment 读当前是否真正生效（诊断 / 装配自检用）。
  ///
  /// `with_managed_segment_read` 的请求在 O_DIRECT 下会被显式关闭，这里同时
  /// 兜底 `direct_segment()` 判定，避免两条读路径同时启用。
  pub fn managed_segment_read_enabled(&self) -> bool {
    self.managed_seg.get() && !self.direct_segment()
  }

  /// 读 page 后是否校验 checksum（默认开启）。
  /// 只写原始字节的测试 / 迁移工具可关闭。
  pub fn with_checksum_verify(self, enabled: bool) -> Self {
    self.skip_verify_checksum.set(!enabled);
    self
  }

  /// 运行期切换 checksum 校验（对齐 `KvOption::skip_verify_checksum`）。
  pub fn set_skip_verify_checksum(&self, skip: bool) {
    self.skip_verify_checksum.set(skip);
  }

  /// 限制 FD 缓存条数（对齐 C++ `fd_limit`）；0 表示不限制。
  pub fn with_fd_cache_limit(self, limit: usize) -> Self {
    self.fd_cache.borrow_mut().limit = if limit == 0 { usize::MAX } else { limit };
    self
  }

  pub fn root(&self) -> &Path {
    &self.root
  }

  pub fn segment_size(&self) -> usize {
    self.segment_size
  }

  pub fn segments_per_file_shift(&self) -> u8 {
    self.segments_per_file_shift
  }

  /// 累计 open 次数：FD 缓存命中不计数，用于验证 FD 复用。
  pub fn fd_open_num(&self) -> u64 {
    self.fd_open_num.load(Ordering::Relaxed)
  }

  /// 实际 `write_at` 次数（测试用；批量写合并后应小于页数）。
  pub fn write_at_num(&self) -> u64 {
    self.write_at_num.load(Ordering::Relaxed)
  }

  /// 页读系统调用尝试次数（含同路径重试与测试注入）。
  pub fn read_at_num(&self) -> u64 {
    self.read_at_num.load(Ordering::Relaxed)
  }

  /// 整文件读（O_DIRECT 分块路径）累计提交的块读次数。
  ///
  /// 分块实现下读 `len` 字节文件提交 `ceil(len / WHOLE_READ_CHUNK)` 次块读；
  /// 旧的一次性整文件读恒为 1 次但要额外驻留一整份对齐缓冲。
  pub fn read_whole_chunk_num(&self) -> u64 {
    self.read_whole_chunk_num.load(Ordering::Relaxed)
  }

  /// 旧 term 候选回退命中次数：更新的候选 EOF / checksum 失败后，从更旧 term
  /// 读到完整页的次数。非零只说明发生过回退读，不代表读到的一定陈旧。
  pub fn stale_read_num(&self) -> u64 {
    self.stale_read_num.load(Ordering::Relaxed)
  }

  /// 测试用：让下一次 manifest 追加只写前 `len` 字节，验证短写回滚。
  ///
  /// 一次性生效；`len == 0` 表示下一次写入直接返回 0 字节。
  pub fn inject_short_manifest_write(&self, len: usize) {
    self.short_write_bytes.set(Some(len));
  }

  /// 测试用：让下一次 `read_whole` 的普通读只返回前 `len` 字节。
  ///
  /// 模拟并发截断 / 撕裂尾导致的短读；一次性生效，O_DIRECT 分支不消费。
  pub fn inject_short_read(&self, len: usize) {
    self.short_read_bytes.set(Some(len));
  }

  /// 测试用：接下来 `times` 次页读返回瞬时 `TryAgain`（模拟 EAGAIN）。
  ///
  /// 用于验证瞬时错误只在同一路径重试、耗尽后上抛，绝不切换旧 term 候选。
  pub fn inject_read_try_again(&self, times: usize) {
    self.read_try_again_times.set(times);
  }

  /// 当前 FD 缓存中的句柄数。
  pub fn fd_cache_len(&self) -> usize {
    self.fd_cache.borrow().len()
  }

  /// FD 缓存条数上限；`usize::MAX` 表示不限制（`with_fd_cache_limit(0)`）。
  ///
  /// 对应 C++ `IouringMgr::GetOpenFileLimit`；`fd_cache_len` 对应
  /// `GetOpenFileCount`。两者是 `open_file_count/limit` gauge 的数据源，
  /// 上报通道尚未接到 `fkv_metrics`，见 `doc/metrics.md`。
  pub fn fd_cache_limit(&self) -> usize {
    self.fd_cache.borrow().limit
  }

  /// 读 / 写缓冲池累计分配次数。
  pub fn buf_alloc_num(&self) -> u64 {
    self.read_buf.borrow().alloc_num + self.write_buf.borrow().alloc_num
  }

  /// 读缓冲池当前常驻字节数（测试 / 诊断用）。
  pub fn read_buf_pooled_bytes(&self) -> usize {
    self.read_buf.borrow().pooled_bytes
  }

  /// 写缓冲池当前常驻字节数（测试 / 诊断用）。
  pub fn write_buf_pooled_bytes(&self) -> usize {
    self.write_buf.borrow().pooled_bytes
  }

  /// 读缓冲池累计租借次数（含池命中，测试用）。
  ///
  /// 与 [`Self::buf_alloc_num`] 互补：池预热后分配次数不再增长，owned 直读
  /// 路径的收益要用「一次都不租借」验证。
  pub fn read_buf_take_num(&self) -> u64 {
    self.read_buf.borrow().take_num
  }

  /// O_DIRECT 对齐池累计新分配块数（诊断 / 测试用）。
  ///
  /// 整文件读按 [`crate::WHOLE_READ_CHUNK`] 分块：读任意大小文件都只应新增
  /// 一块 ≤ 块长的对齐缓冲，之后从池复用。计数增长即说明出现了按文件长度
  /// 分配的整文件缓冲。
  pub fn direct_buf_alloc_num(&self) -> u64 {
    self
      .direct_pool
      .borrow()
      .values()
      .map(|pool| pool.alloc_num())
      .sum()
  }

  /// O_DIRECT 对齐池中最大的块长（诊断 / 测试用）。
  ///
  /// 整文件读不应出现按文件长度建桶的大块：该值大于
  /// [`crate::WHOLE_READ_CHUNK`] 就说明有整文件对齐缓冲被分配过。
  pub fn direct_pool_max_entry(&self) -> usize {
    self.direct_pool.borrow().keys().copied().max().unwrap_or(0)
  }

  /// 注册写槽位累计注册次数（非 Linux 恒为 0）。
  pub fn fixed_write_alloc_num(&self) -> u64 {
    #[cfg(target_os = "linux")]
    {
      self.fixed_write.borrow().alloc_num
    }
    #[cfg(not(target_os = "linux"))]
    {
      0
    }
  }

  /// 注册写槽位被丢弃（写组 future 在 `WRITE_FIXED` await 点取消、在飞 op
  /// 未确认）的累计次数（非 Linux 恒为 0）。
  ///
  /// 与 [`Self::fixed_write_alloc_num`] 对照：discard 增长说明发生取消
  /// （写任务 panic / future 被丢弃），配额已由守卫归还、不会累积退化。
  pub fn fixed_write_discard_num(&self) -> u64 {
    #[cfg(target_os = "linux")]
    {
      self.fixed_write.borrow().discard_num
    }
    #[cfg(not(target_os = "linux"))]
    {
      0
    }
  }

  /// 本表尚未 fdatasync 的 data / segment 文件数。
  pub fn dirty_file_num(&self, tbl: &TableIdent) -> usize {
    self.dirty.borrow().get(tbl).map_or(0, HashSet::len)
  }

  /// `sync_data` 累计调用次数（测试用；只计入口命中，不计脏文件数）。
  pub fn data_sync_num(&self) -> u64 {
    self.data_sync_num.load(Ordering::Relaxed)
  }

  /// 成功的目录项 fsync 次数（测试用，对应 C++ `mkdirat` 后的目录 fdatasync）。
  pub fn dir_fsync_num(&self) -> u64 {
    self.dir_fsync_num.load(Ordering::Relaxed)
  }

  /// 实际提交的 data / segment 文件 fdatasync 次数（测试用）。
  ///
  /// 与 [`Self::data_sync_num`] 配合可确定每个 barrier 的脏文件数：一次
  /// `sync_data` 只应 fsync 真正写过的文件，脏集合为空时不得发出 syscall。
  pub fn data_fsync_num(&self) -> u64 {
    self.data_fsync_num.load(Ordering::Relaxed)
  }

  /// manifest 追加写次数（测试用；snapshot 原子替换不计入）。
  pub fn manifest_append_num(&self) -> u64 {
    self.manifest_append_num.load(Ordering::Relaxed)
  }

  /// manifest 追加后的 fdatasync 次数（测试用；应与 append 次数相等）。
  pub fn manifest_sync_num(&self) -> u64 {
    self.manifest_sync_num.load(Ordering::Relaxed)
  }

  /// `sync_data` 同时在飞的 fdatasync 峰值（测试用；验证并发度来自
  /// 脏文件数 / `max_inflight_write`，而不是硬编码上限）。
  pub fn sync_inflight_peak(&self) -> usize {
    self.sync_inflight.peak()
  }
}

/// O_DIRECT 与 managed segment 读互斥时的显式告警（io-16）。
///
/// 不能静默忽略开关：生产装配先调 `with_managed_segment_read` 再开
/// `with_direct_io`，若无日志，维护者会误以为 provided-buffer 仍生效。
fn warn_managed_segment_read_disabled(segment_size: usize) {
  log::warn!(
    target: "fkv_io_compio",
    "managed segment read 关闭：O_DIRECT segment 读优先 segment_size={}；\
     compio provided-buffer 由 BoxAllocator 分配、地址不保证页对齐，\
     不能作为 O_DIRECT 读目标",
    segment_size
  );
}
