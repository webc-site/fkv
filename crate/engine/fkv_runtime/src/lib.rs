//! Thread-per-core runtime：N 个 Shard，按 TableIdent 路由请求。

use core::{fmt, future::Future};
use std::{
  collections::VecDeque,
  fs, io,
  marker::PhantomData,
  mem::take,
  path::{Path, PathBuf},
  sync::{
    Arc, Mutex, MutexGuard, PoisonError,
    atomic::{AtomicBool, AtomicU64, Ordering},
  },
  thread::{self, JoinHandle},
  time::{Duration, Instant},
};

use compio::{driver::ProactorBuilder, runtime};
use crossfire::{
  MTx, RecvTimeoutError, Rx,
  mpsc::{Array, bounded_blocking},
};
use fkv_api::{
  ArchiveReply, ArchiveTagInfo, BranchReply, CleanExpiredReply, FloorReply, LargeValueReply,
  ListPage, Request, Response, ScanPageOption, ScanReply,
};
use fkv_cloud::{CLOUD_LIST_PAGE_KEYS, CacheGroup, CloudStore};
use fkv_config::KvOption;
use fkv_error::{KvError, Result};
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_io_compio::CompioStore;
use fkv_metrics::MetricsSnapshot;
use fkv_object::ObjectStore;
use fkv_object_s3::S3ObjectStore;
use fkv_shard::{
  Backend, CloudBackend, RootedConfig, Shard, ShardConfig, ShardLifecycle, ShardSender,
  split_cloud_store_path,
};
use fkv_standby::{RsyncSource, RsyncStandbyService, StandbyConfig, StdCommandRunner};
use fkv_task_prewarm::{CompletionReason, PrewarmStat, prewarm_tables_with_concurrency};
use fkv_types::{
  DEFAULT_MANIFEST_LIMIT, DEFAULT_MAX_PROCESSING_TIME_US, FileId, InstallOutcome, MAIN_BRANCH_NAME,
  StoreMode, TableIdent, archive_file_name, normalize_branch_name, now_us,
};
use futures::{executor::block_on, future::join_all, lock::Mutex as AsyncMutex};
use gxhash::HashSet;
use log::{error, info, warn};

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
  pub num_threads: u16,
  pub root: PathBuf,
  /// 未给 `KvOption` 时使用；给了且 `data_page_size` 非默认则以 option 为准。
  pub page_size: usize,
  /// 未给 `KvOption` 时 data / index 页共用的 restart 间隔回退值。
  pub restart_interval: u16,
  /// 未给 `KvOption` 时使用；给了且 `pages_per_file_shift` 非默认则以 option 为准。
  pub pages_per_file_shift: u8,
  /// 未给 `KvOption` 时使用；给了且 `segment_size` 非默认则以 option 为准。
  pub segment_size: u32,
  /// 未给 `KvOption` 时使用；给了且 `segments_per_file_shift` 非默认则以 option 为准。
  pub segments_per_file_shift: u8,
  pub large_value_threshold: usize,
  pub fixed_buffer_size: usize,
  pub queue_size: usize,
  /// 未给 `KvOption` 时使用；给了且 `auto_reopen_*` 非默认则以 option 为准。
  pub auto_reopen_retry_times: u8,
  /// 未给 `KvOption` 时使用；给了且 `auto_reopen_*` 非默认则以 option 为准。
  pub auto_reopen_pending_time_us: u64,
  /// 0 表示关闭定时归档。
  pub archive_interval_secs: u64,
  pub archive_num_retained: u32,
}

impl Default for RuntimeConfig {
  /// 与 `KvOption` 默认值对齐：字段值等于 `KvOption::default()` 时视为
  /// 「未显式指定」，`start_with_option` 会以 `KvOption` 为准，不会触发
  /// 两侧冲突。`large_value_threshold = 0` 表示按 `page_size / 2` 自动取界。
  fn default() -> Self {
    let option = KvOption::default();
    Self {
      num_threads: option.num_threads,
      root: PathBuf::new(),
      page_size: option.data_page_size as usize,
      restart_interval: option.data_page_restart_interval,
      pages_per_file_shift: option.pages_per_file_shift,
      segment_size: option.segment_size,
      segments_per_file_shift: option.segments_per_file_shift,
      large_value_threshold: 0,
      fixed_buffer_size: 0,
      queue_size: 64,
      auto_reopen_retry_times: option.auto_reopen_retry_times,
      auto_reopen_pending_time_us: option.auto_reopen_pending_time_us,
      archive_interval_secs: u64::from(option.archive_interval_secs),
      archive_num_retained: option.num_retained_archives,
    }
  }
}

impl RuntimeConfig {
  /// 以 `root` 为数据根、其余字段取 [`Self::default`] 的配置。
  ///
  /// 配合 [`Runtime::start_with_option`] 使用时，盘面 / 重试 / 归档字段由
  /// `KvOption` 决定，调用方不必重复填十几个字段。
  pub fn new(root: impl Into<PathBuf>) -> Self {
    Self {
      root: root.into(),
      ..Self::default()
    }
  }
}

/// 目标集合来源：Local/Mem 以盘面分区为准，Cloud 以远端分页列举为准。
///
/// Cloud 下本地 cache 只看得见已 cache 的分区，归档对象必须按远端为准
/// （对齐 C++ `ListPartitions` 直接列对象存储）。
enum ArchiveTarget {
  Local(Vec<PathBuf>),
  Cloud(Box<CloudStoreConfig>, Box<CloudBackend>),
}

/// 一个 shard 的 attach 身份（归一化 branch + term）。
///
/// 全局结果名需要它：C++ `HandleGlobalArchiveRequest` 在列举分区**之前**就写
/// `result_archive_ = BranchArchiveName(branch_, ShardTerm(ds_shard), tag)`，
/// 所以 0 个目标或全部失败时也返回非空名字。本仓没有 store 级 `branch_`，
/// 用 attach 身份补齐（记录点见 [`Runtime::attach_shard`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct AttachIdent {
  branch: String,
  term: u64,
}

/// 取 attach 身份表锁；poison 只说明上一次持锁线程 panic，状态本身仍可用。
fn lock_attach_ident_li(
  li: &Mutex<Vec<Option<AttachIdent>>>,
) -> MutexGuard<'_, Vec<Option<AttachIdent>>> {
  li.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 归档参数与 attached_li 状态：crond 线程与请求路径共享。
struct ArchiveState {
  attached_li: Vec<AtomicBool>,
  target: ArchiveTarget,
  interval: Duration,
  num_retained: u32,
  /// 单批 fanout 的分区数（`max_global_request_batch`）。
  batch_li: usize,
}

impl ArchiveState {
  fn new(
    shard_num: u16,
    target: ArchiveTarget,
    interval: Duration,
    num_retained: u32,
    batch_li: u32,
  ) -> Self {
    Self {
      attached_li: (0..shard_num).map(|_| AtomicBool::new(false)).collect(),
      target,
      interval,
      num_retained,
      batch_li: batch_size(batch_li),
    }
  }
}

/// `max_global_request_batch` 归一化：0 / 超界一律按 1，避免空批或一次性全投。
fn batch_size(batch_li: u32) -> usize {
  usize::try_from(batch_li).unwrap_or(1).max(1)
}

/// 写 IO 的两个上限：`(max_inflight_write, max_write_batch_pages)`。
///
/// 未给 `KvOption` 时回落到 `fkv_config` 常量；默认值与 C++ `kv_options.h`
/// 字段初始值一致（`32 << 10` / `32`）。
fn write_limits(option: Option<&KvOption>) -> (usize, usize) {
  option.map_or(
    (
      fkv_config::DEFAULT_MAX_INFLIGHT_WRITE as usize,
      fkv_config::DEFAULT_MAX_WRITE_BATCH_PAGES as usize,
    ),
    |option| {
      (
        option.max_inflight_write as usize,
        option.max_write_batch_pages as usize,
      )
    },
  )
}

/// 列举盘上分区目录名（`{tbl_name}.{partition_id}`），失败（目录尚未创建）返回空。
///
/// 对齐 C++ `HandleGlobalArchiveRequest`：每个 interval 重新列举 `store_path` 下
/// 全部 partition，含本进程从未写过的只读表；目录名不合规的条目直接跳过。
fn scan_table_li(root: &Path) -> Vec<TableIdent> {
  let Ok(entry_li) = fs::read_dir(root) else {
    return Vec::new();
  };
  entry_li
    .flatten()
    .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
    .filter_map(|entry| {
      let name = entry.file_name();
      let (tbl_name, partition_id) = name.to_str()?.rsplit_once('.')?;
      TableIdent::new(tbl_name, partition_id.parse().ok()?).ok()
    })
    .collect()
}

/// 多根列举：按 `store_path` 顺序拼接每个根的盘上分区。
///
/// 分区归属由 LUT 固定，正常配置下不会跨根重复；不额外去重（对齐 C++
/// `HandleGlobalArchiveRequest` 直接遍历 `store_path`）。
fn scan_table_li_li(root_li: &[PathBuf]) -> Vec<TableIdent> {
  root_li
    .iter()
    .flat_map(|root| scan_table_li(root))
    .collect()
}

/// 归档标签条目（`manifest_<branch>_<term>_<tag>` 里的 `(tag, term)`）。
///
/// 对齐 C++ `GlobalListArchiveTagsRequest::ArchiveEntry`。字段顺序 = `Ord` 顺序
/// （先 `tag` 后 `term`），直接 `sort()` 即 C++ 比较器语义。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArchiveEntry {
  pub tag: String,
  pub term: u64,
}

/// 扫描 `root` 下全部分区目录，取带归档 tag 的 manifest 条目。
///
/// `prefix` 非空时按 tag 前缀过滤，结果按 `(tag, term)` 升序。
///
/// 与 C++ 一致的三点：① 直扫盘面、不经 shard、不检查 attach 状态（本进程从未
/// 写过的只读表也要列）；② 排序后按 `(tag, term)` 全等去重（C++ `std::sort` +
/// `std::unique`：多分区 fanout 归档同 tag 同 term 只回一条，同 tag 不同 term
/// 各占一条）；③ 单个目录读失败按跳过处理（控制面列举容错，不报部分错误）。
///
/// 已知偏离：C++ 遍历 `options_.store_path` 的全部根；本仓由 `root_li` 覆盖
/// 全部本地根（单根时就是一个），Cloud 模式下列的是本地 cache 目录，
/// 与 C++ 同（C++ 的该 handler 也只扫本地 store_path，不列远端）。
fn list_archive_tag_li(root_li: &[PathBuf], prefix: &str) -> Vec<ArchiveEntry> {
  let mut entry_li: Vec<ArchiveEntry> = Vec::new();
  for root in root_li {
    list_archive_tags_of(root, prefix, &mut entry_li);
  }
  entry_li.sort();
  entry_li.dedup();
  entry_li
}

fn list_archive_tags_of(root: &Path, prefix: &str, entry_li: &mut Vec<ArchiveEntry>) {
  for tbl in scan_table_li(root) {
    let dir = root.join(format!("{}.{}", tbl.tbl_name, tbl.partition_id));
    let Ok(file_li) = fs::read_dir(dir) else {
      continue;
    };
    for entry in file_li.flatten() {
      if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
        continue;
      }
      let name = entry.file_name();
      let Some(name) = name.to_str() else {
        continue;
      };
      // 无 tag 的 manifest（`manifest_<branch>_<term>`）不是归档条目。
      let Some((_, term, Some(tag))) = fkv_types::parse_manifest_file_name(name) else {
        continue;
      };
      if !prefix.is_empty() && !tag.starts_with(prefix) {
        continue;
      }
      entry_li.push(ArchiveEntry {
        tag: tag.to_owned(),
        term,
      });
    }
  }
}

/// 全局请求（archive / reopen / create_branch / drop_table）回执：
/// 成功条数 + 逐表失败（`TableIdent`, `KvError`），以及需要回传给调用方的
/// 全局命名结果。
///
/// 单表失败必须可见：不能把「N 个分区里挂了 1 个」回成整体成功。
///
/// `tag` / `name` 对齐 C++ 的结果字段：
/// - `global_archive`：`tag` = 实际使用的归档 tag（空 tag 由本层生成一次并
///   广播给全部分区），`name` = 归档 manifest 名（C++ `ResultArchive`）。
///   0 目标 / 全部失败时 `name` 仍按 attach 身份构造，不为空。
/// - `global_delete_archive`：同走 C++ `HandleGlobalArchiveRequest`（Delete），
///   `tag` + `name` 与 create 同语义返回。
/// - `global_create_branch`：`name` = 规范化并加盐后的内部 branch 名
///   （C++ `ResultBranch`），调用方必须用它继续操作该分支。
///
/// `global_reopen` / `global_drop_table` 不适用（有意留空）：
/// - C++ `GlobalReopenRequest` / `DropTableRequest` 的回执只有每分区成功与否，
///   不带 `ResultArchive` / `ResultBranch` 结果字段；fkv 对应的 per-shard
///   `Reopen` / `ReopenArchive` / `Drop` 响应同样不携带名字，本层无从构造。
/// - `tag` 也不回填请求参数：reopen 的 tag 只是输入（空 = 重开活跃快照），
///   回填会被读成「本层生成的归档 tag」，与归档回执语义混淆。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlobalReply {
  pub ok_num: usize,
  pub failed_li: Vec<(TableIdent, KvError)>,
  pub tag: String,
  pub name: String,
}

/// 控制面请求不得走按分区路由的数据面入口（见 [`Request::is_control_plane`]）。
fn reject_control_plane(req: &Request) -> Result<()> {
  if req.is_control_plane() {
    return Err(KvError::InvalidArgs);
  }
  Ok(())
}

impl GlobalReply {
  /// 成功 + 失败总数（= 本轮投递的分区数）。
  pub fn total(&self) -> usize {
    self.ok_num + self.failed_li.len()
  }
}

/// 每 shard 的 attach 代际。
///
/// attach / detach 推进代际；请求在发出前取快照，收尾时只在快照未变时才清本层
/// 视图。否则一个迟到的错误（本地 `Timeout` 后任务被延迟轮询）会清掉更晚成功的
/// attach 刚记录的新视图。
///
/// 代际读写用 `AcqRel`：视图落地 / 失效先以 compare-exchange 抢占「仍是当前代际」，
/// 再在 [`Self::with_gate`] 的 per-shard 临界区里写视图，避免校验与写入之间插进
/// 另一次 detach 的清除（或 attach 的记录）。
#[derive(Debug)]
struct AttachEpoch {
  li: Vec<AtomicU64>,
  /// per-shard 视图临界区：代际校验与视图读写必须是同一个原子步骤。
  gate_li: Vec<Mutex<()>>,
  /// 本地 `Timeout` 时 lifecycle 仍是 `Attached` 的登记（值 = 代际 + 1，0 = 未登记）。
  ///
  /// shard 可能稍后才把 lifecycle 置 `Expired`：此刻既不能清视图（可能只是本地
  /// 排队慢），也不能假设视图永久有效，登记后由下一次观测复核
  /// （[`ShardRoute::settle_timeout`]）。
  timeout_li: Vec<AtomicU64>,
}

impl AttachEpoch {
  fn new(shard_num: u16) -> Self {
    let shard_num = usize::from(shard_num);
    Self {
      li: (0..shard_num).map(|_| AtomicU64::new(0)).collect(),
      gate_li: (0..shard_num).map(|_| Mutex::new(())).collect(),
      timeout_li: (0..shard_num).map(|_| AtomicU64::new(0)).collect(),
    }
  }

  fn index(shard_id: u32) -> Option<usize> {
    usize::try_from(shard_id).ok()
  }

  fn slot(&self, shard_id: u32) -> Option<&AtomicU64> {
    Self::index(shard_id).and_then(|index| self.li.get(index))
  }

  fn snapshot(&self, shard_id: u32) -> u64 {
    self
      .slot(shard_id)
      .map_or(0, |epoch| epoch.load(Ordering::Acquire))
  }

  /// 推进代际并返回本轮新代际。
  fn bump(&self, shard_id: u32) -> u64 {
    self
      .slot(shard_id)
      .map_or(0, |epoch| epoch.fetch_add(1, Ordering::AcqRel) + 1)
  }

  /// 快照仍是当前代际（没有更晚的 attach / detach 覆盖）。
  fn is_current(&self, shard_id: u32, epoch: u64) -> bool {
    self.snapshot(shard_id) == epoch
  }

  /// 原子推进：仅当 `epoch` 仍是当前代际时置为 `epoch + 1`。
  fn bump_if_current(&self, shard_id: u32, epoch: u64) -> bool {
    self.slot(shard_id).is_some_and(|slot| {
      slot
        .compare_exchange(epoch, epoch + 1, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    })
  }

  /// 在 per-shard 视图临界区内执行 `f`。
  fn with_gate<R>(&self, shard_id: u32, f: impl FnOnce() -> R) -> Option<R> {
    let gate = Self::index(shard_id).and_then(|index| self.gate_li.get(index))?;
    let _guard = gate.lock().unwrap_or_else(PoisonError::into_inner);
    Some(f())
  }

  /// 登记「本地超时时 lifecycle 仍 `Attached`」：等下一次观测复核。
  ///
  /// 用 `fetch_max` 而非 `store`：迟到的旧代际 `Timeout` 不得把更晚的登记改小。
  /// 登记值随后被 `settle_timeout` 取出并用作代际校验，改小会让本该清的视图
  /// 因代际不符而永久残留。
  fn mark_timeout(&self, shard_id: u32, epoch: u64) {
    if let Some(slot) = Self::index(shard_id).and_then(|index| self.timeout_li.get(index)) {
      slot.fetch_max(epoch + 1, Ordering::Release);
    }
  }

  /// 复核超时登记：shard 已不 `Attached` 时返回登记时的代际并消除登记。
  fn settle_timeout(&self, shard_id: u32, attached: bool) -> Option<u64> {
    let slot = Self::index(shard_id).and_then(|index| self.timeout_li.get(index))?;
    if attached {
      return None;
    }
    match slot.swap(0, Ordering::AcqRel) {
      0 => None,
      marked => Some(marked - 1),
    }
  }
}

/// 本层为每个 shard 记录的 attach 视图：attach 身份 / 归档目标集合 / 预热路由。
///
/// 三者在 detach、attach 失败与「shard 已不托管」的错误之后必须一起失效，故收在
/// 一处；`ArchiveCrond` 与数据面持同一份 `Arc`。
#[derive(Clone, Default)]
struct ShardViews {
  archive: Option<Arc<ArchiveState>>,
  prewarm: Option<Arc<Mutex<PrewarmState>>>,
  ident: Option<Arc<Mutex<Vec<Option<AttachIdent>>>>>,
}

impl ShardViews {
  /// 代际仍是最新时清视图；校验与清除在同一个 per-shard 临界区内。
  ///
  /// 返回 `false` = 已有更晚的 attach / detach，本次清除弃权。
  fn clear_if_epoch(&self, epoch_li: &AttachEpoch, shard_id: u32, epoch: u64) -> bool {
    epoch_li
      .with_gate(shard_id, || {
        if epoch_li.is_current(shard_id, epoch) {
          self.clear(shard_id);
          true
        } else {
          false
        }
      })
      .unwrap_or(false)
  }

  fn clear(&self, shard_id: u32) {
    if let Some(ident_li) = &self.ident
      && let Some(slot) = lock_attach_ident_li(ident_li).get_mut(shard_id as usize)
    {
      *slot = None;
    }
    if let Some(state) = &self.archive
      && let Some(attached) = state.attached_li.get(shard_id as usize)
    {
      attached.store(false, Ordering::Relaxed);
    }
    if let Some(state) = &self.prewarm {
      lock_prewarm_state(state).detach(shard_id);
    }
  }
}

/// 单 shard 的发送端 + 收尾句柄：**所有** shard 发送都必须经此。
///
/// 收尾规则：请求出错后读 shard 的真实 lifecycle（本地 `Timeout` 时请求可能已执行
/// 并把 shard 置为 `Expired`）；只要 shard 已不处于 `Attached`，且代际快照仍是
/// 当前代际，就清掉本层视图。
#[derive(Clone)]
struct ShardRoute {
  shard_id: u32,
  sender: ShardSender,
  lifecycle: ShardLifecycle,
  epoch: Arc<AttachEpoch>,
  views: ShardViews,
}

impl ShardRoute {
  /// 装配 shard 发送端 + 收尾句柄。
  ///
  /// `Runtime::start` 建 shard 列表时还没有 `Runtime`，故用自由函数形态复用。
  fn new(shard: &Shard, sender: ShardSender, epoch: &Arc<AttachEpoch>, views: &ShardViews) -> Self {
    Self {
      shard_id: shard.shard_id(),
      sender,
      lifecycle: shard.lifecycle().clone(),
      epoch: Arc::clone(epoch),
      views: views.clone(),
    }
  }

  /// 发送前快照：收尾时据此判断是否有更晚的 attach / detach。
  fn epoch_snapshot(&self) -> u64 {
    self.epoch.snapshot(self.shard_id)
  }

  /// 推进代际并返回新代际：本次 attach / detach 之后，早先请求的迟到错误
  /// 不再清视图。
  fn bump_epoch(&self) -> u64 {
    self.epoch.bump(self.shard_id)
  }

  /// 视图落地：仅当 `epoch` 仍是最新代际时推进代际并执行 `record`。
  ///
  /// 返回 `false` = 期间有更晚的 attach / detach，本次视图不得覆盖它。
  fn record_if_current(&self, epoch: u64, record: impl FnOnce()) -> bool {
    self
      .epoch
      .with_gate(self.shard_id, || {
        if self.epoch.bump_if_current(self.shard_id, epoch) {
          record();
          true
        } else {
          false
        }
      })
      .unwrap_or(false)
  }

  /// 视图失效：仅当 `epoch` 仍是最新代际时清掉。
  ///
  /// 迟到的 detach / 错误收尾不得清掉更晚 attach 刚记录的新视图。
  fn clear_if_current(&self, epoch: u64) -> bool {
    self.views.clear_if_epoch(&self.epoch, self.shard_id, epoch)
  }

  /// 发出请求并按真实 lifecycle 收尾（正常返回与 future 被取消两条路径都覆盖）。
  async fn track(&self, send: impl Future<Output = Result<Response>>) -> Result<Response> {
    let epoch = self.epoch_snapshot();
    let mut guard = FinishGuard {
      route: self,
      epoch,
      armed: true,
    };
    let result = send.await;
    guard.armed = false;
    self.finish(epoch, result)
  }

  async fn request(&self, req: Request) -> Result<Response> {
    // 调用方 drop / cancel 时 `finish` 不会执行：由 `track` 内的守卫兜底收尾。
    self.track(self.sender.request(req)).await
  }

  async fn request_deadline(&self, req: Request, timeout: Duration) -> Result<Response> {
    self.track(self.sender.request_deadline(req, timeout)).await
  }

  async fn request_at(&self, req: Request, deadline: Instant) -> Result<Response> {
    self.track(self.sender.request_at(req, deadline)).await
  }

  fn finish(&self, epoch: u64, result: Result<Response>) -> Result<Response> {
    match &result {
      // 本地 `Timeout`：请求可能已入队 / 已执行，shard 稍后才置 `Expired`。
      // 此刻读到 `Attached` 不代表视图失效，登记待复核；已接受的近似是复核
      // 之前视图可能短暂残留（`fkv_shard` 不提供 Expired 写点回调）。
      Err(KvError::Timeout) if self.lifecycle.is_attached() => {
        self.epoch.mark_timeout(self.shard_id, epoch);
      }
      Err(_) => {
        if !self.lifecycle.is_attached() {
          self.clear_if_current(epoch);
        }
      }
      Ok(_) => self.settle_timeout(),
    }
    result
  }

  /// 复核此前的超时登记：shard 现在已不 `Attached` 且代际未变才清视图。
  fn settle_timeout(&self) {
    let Some(epoch) = self
      .epoch
      .settle_timeout(self.shard_id, self.lifecycle.is_attached())
    else {
      return;
    };
    self.clear_if_current(epoch);
  }
}

/// `request*` 的收尾守卫：future 在 `.await` 未完成时被 drop / cancel，
/// 也必须按 shard 的真实 lifecycle 收尾，否则视图会残留到下一次错误。
struct FinishGuard<'a> {
  route: &'a ShardRoute,
  epoch: u64,
  /// 正常 `.await` 返回后置 `false`，收尾交给 [`ShardRoute::finish`]。
  armed: bool,
}

impl Drop for FinishGuard<'_> {
  fn drop(&mut self) {
    if self.armed && !self.route.lifecycle.is_attached() {
      self.route.clear_if_current(self.epoch);
    }
  }
}

/// 跨 shard fanout：按 partition hash 路由，并按 `max_global_request_batch` 分批
/// 投递 —— 每批顶层并发（`join_all`）、批间串行，限制在途请求与回执内存。
///
/// 调用方要么在 async 上下文 `.await`，要么在 shard poll 栈之外的顶层线程
/// `block_on`（crond）；绝不在 shard 任务栈内驱动调度器。
struct GlobalFanout {
  route_li: Vec<ShardRoute>,
  num_threads: u16,
  batch_li: usize,
}

impl GlobalFanout {
  fn new(route_li: Vec<ShardRoute>, batch_li: usize) -> Self {
    Self {
      num_threads: route_li.len() as u16,
      route_li,
      batch_li,
    }
  }

  fn num_threads(&self) -> u16 {
    self.num_threads
  }

  /// 对每个分区生成 `make_req` 请求并投递到其归属 shard，聚合回执。
  async fn dispatch(
    &self,
    table_li: &[TableIdent],
    make_req: impl Fn(&TableIdent) -> Request,
  ) -> GlobalReply {
    self.dispatch_collect(table_li, make_req).await.0
  }

  /// [`Self::dispatch`] 的带载荷版本：额外返回第一条成功回执（用于提取
  /// `ArchiveReply::name` / `BranchReply::name` 这类全局结果）。
  async fn dispatch_collect(
    &self,
    table_li: &[TableIdent],
    make_req: impl Fn(&TableIdent) -> Request,
  ) -> (GlobalReply, Option<Response>) {
    let mut reply = GlobalReply::default();
    let mut first_response = None;
    for chunk in table_li.chunks(self.batch_li) {
      let pending = chunk.iter().map(|tbl| {
        let shard_id = usize::from(tbl.shard_index(self.num_threads));
        // SAFETY: shard_index 对 shard_num 取模，必 < route_li.len()
        let route = unsafe { self.route_li.get_unchecked(shard_id) };
        let req = make_req(tbl);
        async move { (tbl, route.request(req).await) }
      });
      for (tbl, result) in join_all(pending).await {
        match result {
          Ok(response) => {
            reply.ok_num += 1;
            if first_response.is_none() {
              first_response = Some(response);
            }
          }
          Err(err) => reply.failed_li.push((tbl.clone(), err)),
        }
      }
    }
    (reply, first_response)
  }
}

/// 归档后台线程：crossfire 通道兼作定时器与停止信号，drop 发送端即停止。
struct ArchiveCrond {
  stop_tx: Option<MTx<Array<()>>>,
  join: Option<JoinHandle<()>>,
}

impl ArchiveCrond {
  fn start(state: Arc<ArchiveState>, route_li: Vec<ShardRoute>) -> Result<Self> {
    let (stop_tx, stop_rx) = bounded_blocking::<()>(1);
    let join = thread::Builder::new()
      .name("fkv-archive".to_owned())
      .spawn(move || Self::run(&state, &route_li, &stop_rx))
      .map_err(|_| KvError::IoFail)?;
    Ok(Self {
      stop_tx: Some(stop_tx),
      join: Some(join),
    })
  }

  fn run(state: &ArchiveState, route_li: &[ShardRoute], stop_rx: &Rx<Array<()>>) {
    // Cloud 模式列远端分区需要一个运行时侧 `CloudStore`；它只做列举、不碰本地
    // cache，所以不需要 compio runtime。惰性构造一次后跨 tick 复用。
    let mut cloud_store: Option<CloudStore<CompioStore, S3ObjectStore>> = None;
    while let Err(RecvTimeoutError::Timeout) = stop_rx.recv_timeout(state.interval) {
      Self::tick(state, route_li, &mut cloud_store);
    }
  }

  /// 一轮归档：先确定目标分区（Local 列盘 / Cloud 列远端），再按
  /// `max_global_request_batch` 分批 fanout；本线程只在顶层（非任务栈）等待全部
  /// 回执。只归档 attached_li 的 shard。
  fn tick(
    state: &ArchiveState,
    route_li: &[ShardRoute],
    cloud_store: &mut Option<CloudStore<CompioStore, S3ObjectStore>>,
  ) {
    let table_li = match Self::enum_target(state, cloud_store) {
      Ok(table_li) => table_li,
      Err(err) => {
        warn!(target: "fkv_runtime", "archive crond listing failed error={}", err);
        return;
      }
    };
    let fanout = GlobalFanout::new(route_li.to_vec(), state.batch_li);
    let shard_num = fanout.num_threads();
    let pending: Vec<TableIdent> = table_li
      .into_iter()
      .filter(|tbl| {
        state.attached_li[usize::from(tbl.shard_index(shard_num))].load(Ordering::Relaxed)
      })
      .collect();
    if pending.is_empty() {
      return;
    }
    let tag = fkv_api::archive_tag();
    let num_retained = state.num_retained;
    let reply = block_on(fanout.dispatch(&pending, |tbl| Request::CreateArchive {
      tbl: tbl.clone(),
      tag: tag.clone(),
      num_retained,
    }));
    let total = reply.total();
    let archived = reply.ok_num;
    // shard 未 attach 的 `NotRunning` 是预期结果，不算失败；其余错误必须告警。
    let failed = reply
      .failed_li
      .iter()
      .filter(|(tbl, err)| {
        if matches!(err, KvError::NotRunning) {
          return false;
        }
        warn!(
          target: "fkv_runtime",
          "archive crond failed table={} tag={} error={}",
          tbl,
          tag,
          err
        );
        true
      })
      .count();
    if archived > 0 {
      info!(
        target: "fkv_runtime",
        "archive crond run tables={} archived={} failed={} tag={}",
        total,
        archived,
        failed,
        tag
      );
    }
  }

  /// 目标集合：Local 列盘上分区目录；Cloud 列远端分区（惰性建 store 后复用）。
  fn enum_target(
    state: &ArchiveState,
    cloud_store: &mut Option<CloudStore<CompioStore, S3ObjectStore>>,
  ) -> Result<Vec<TableIdent>> {
    match &state.target {
      ArchiveTarget::Local(root_li) => Ok(scan_table_li_li(root_li)),
      ArchiveTarget::Cloud(config, cloud) => {
        if cloud_store.is_none() {
          *cloud_store = Some(open_runtime_cloud_store(config, cloud)?);
        }
        match cloud_store.as_ref() {
          Some(store) => block_on(remote_partition_li(store)),
          None => Err(KvError::CloudErr),
        }
      }
    }
  }

  fn stop(&mut self) {
    self.stop_tx.take();
    if let Some(join) = self.join.take() {
      let _ = join.join();
    }
  }
}

impl Drop for ArchiveCrond {
  fn drop(&mut self) {
    self.stop();
  }
}

/// 一轮预热的汇总回执（`Runtime::prewarm_report` 观测）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PrewarmReport {
  /// 本轮实际列举的分区数。
  pub table_num: usize,
  /// 拉取成功的文件数（manifest + data + segment）。
  pub pulled_file_num: usize,
  /// 单文件失败数。
  pub files_failed: usize,
  /// 因本地 cache 余量不足中止的表数。
  pub disk_full_num: usize,
  /// 远端不存在的文件数。
  pub missing_num: usize,
  /// 本轮不完整：整轮列举失败（网络 / 权限等）、单 shard 列举失败，或某张表以
  /// `DownloadError` / `DiskFull` 结束。失败分区会重新入队，下一轮重试；回执不会
  /// 被后续轮次抹掉。
  pub listing_failed: bool,
}

/// 单个 shard 的预热目标：attach 时的 `(branch, term)` 与其名下的分区。
#[derive(Debug, PartialEq, Eq)]
struct PrewarmTarget {
  shard_id: usize,
  branch: String,
  term: u64,
  table_li: Vec<TableIdent>,
}

/// reopen 投递的 per-table 预热队列上限（对齐 `fkv_task_prewarm` 的
/// `kMaxPrewarmPendingFiles = 1000` 有界思路，重复表去重）。
const MAX_PENDING_PREWARM_TABLES: usize = 1000;

/// 预热失败回队 / 列举失败后的自唤醒退避：首轮 [`PREWARM_RETRY_MIN`]，
/// 连续失败翻倍，封顶 [`PREWARM_RETRY_MAX`]。
///
/// 没有这一步时失败分区只在下次 attach / reopen 的 `signal()` 才重试，远端
/// 短时抖动会让分区永久停在未预热状态。
const PREWARM_RETRY_MIN: Duration = Duration::from_millis(50);
const PREWARM_RETRY_MAX: Duration = Duration::from_secs(1);

/// 预热路由状态：每个 shard 的 attach 视图与「已预热」标记。
///
/// crond 线程与 attach / detach / reopen 路径共享（`Mutex` 保护）。
/// - attach 轮只处理「已 attach 且尚未预热」的 shard，避免每个 shard attach
///   都重跑全部 shard 的列举与下载；
/// - `pending_tbl_li` 是 reopen 成功后的 per-table 目标（对齐 C++
///   `ReopenTask::Reopen` → `PrewarmService::Prewarm`）：不受 `done_li` 限制，
///   否则 attach 之后才出现的新分区永远不预热。
#[derive(Debug)]
struct PrewarmState {
  attached_li: Vec<bool>,
  ident_li: Vec<(String, u64)>,
  done_li: Vec<bool>,
  /// reopen 投递、待 crond 消费的单表目标（FIFO，重复表去重）。
  pending_tbl_li: VecDeque<TableIdent>,
  /// `pending_tbl_li` 的 O(1) 去重索引（与队列同步增删）。
  ///
  /// 旧实现用 `VecDeque::contains` 线性查重，`plan` 又对每条 pending 做
  /// `Vec::contains`：pending 上限 1000、单 shard 分区上万时是约 1e7 次
  /// `String` 比较，且全程持 `lock_prewarm_state`。
  pending_set: HashSet<TableIdent>,
  last_report: Option<PrewarmReport>,
}

impl PrewarmState {
  fn new(shard_num: u16) -> Self {
    let num = usize::from(shard_num);
    Self {
      attached_li: vec![false; num],
      ident_li: vec![(MAIN_BRANCH_NAME.to_owned(), 0); num],
      done_li: vec![false; num],
      pending_tbl_li: VecDeque::new(),
      pending_set: HashSet::default(),
      last_report: None,
    }
  }

  /// 记录 attach：更新 `(branch, term)` 并允许再次预热（term 可能变化）。
  fn attach(&mut self, shard_id: u32, branch: &str, term: u64) {
    let Some(index) = usize::try_from(shard_id)
      .ok()
      .filter(|index| *index < self.attached_li.len())
    else {
      return;
    };
    self.attached_li[index] = true;
    self.ident_li[index] = (branch.to_owned(), term);
    self.done_li[index] = false;
  }

  fn detach(&mut self, shard_id: u32) {
    let Some(index) = usize::try_from(shard_id)
      .ok()
      .filter(|index| *index < self.attached_li.len())
    else {
      return;
    };
    self.attached_li[index] = false;
    self.done_li[index] = false;
    // pending 是按旧 `(branch, term)` 投递的：留到重 attach 之后会按新身份
    // 预热同一批分区，故按 shard 一并清掉（去重索引与队列同步重建）。
    let shard_num = self.attached_li.len() as u16;
    self
      .pending_tbl_li
      .retain(|tbl| usize::from(tbl.shard_index(shard_num)) != index);
    self.pending_set = self.pending_tbl_li.iter().cloned().collect();
  }

  /// reopen 成功后登记单表预热：不阻塞 reopen 回执，由 crond 下一轮消费。
  ///
  /// 重复表去重；超过上限时丢最早的一条（保留最近一次 reopen 反映的当前状态）。
  fn enqueue_table(&mut self, tbl: TableIdent) {
    if !self.pending_set.insert(tbl.clone()) {
      return;
    }
    if self.pending_tbl_li.len() >= MAX_PENDING_PREWARM_TABLES
      && let Some(evicted) = self.pending_tbl_li.pop_front()
    {
      self.pending_set.remove(&evicted);
    }
    self.pending_tbl_li.push_back(tbl);
  }

  /// 锁内只搬状态：复制 attach 视图并取走 pending 队列。
  ///
  /// 与分区数线性相关的分桶 / 排序 / 去重放到锁外（[`plan_targets`]），
  /// 避免列举上万分区时长时间独占 `lock_prewarm_state`。
  fn take_plan_input(&mut self) -> PlanInput {
    self.pending_set.clear();
    PlanInput {
      attached_li: self.attached_li.clone(),
      done_li: self.done_li.clone(),
      ident_li: self.ident_li.clone(),
      pending_li: self.pending_tbl_li.drain(..).collect(),
    }
  }
}

/// [`PrewarmState::take_plan_input`] 搬出的状态快照（锁外使用）。
struct PlanInput {
  attached_li: Vec<bool>,
  done_li: Vec<bool>,
  ident_li: Vec<(String, u64)>,
  pending_li: Vec<TableIdent>,
}

/// 按 `TableIdent::shard_index` 把分区分发到各 shard（锁外执行）。
///
/// attach 轮只保留「已 attach 且未预热」的 shard；reopen 投递的单表目标不受
/// `done_li` 限制，但只接受已 attach 的 shard（其余等 attach 后的整轮覆盖）。
/// 同一 shard 的两类目标合并成一个 [`PrewarmTarget`]，重复表只保留一份。
///
/// 去重用桶内 `sort_unstable` + `dedup`（单次比较相邻元素，无哈希分配、不逐条
/// `clone`），比每 shard 一个 `HashSet` 少一轮堆分配；顺带让同一轮的预热顺序
/// 稳定（远端列举顺序在分页间可变）。
fn plan_targets(input: PlanInput, table_li: Vec<TableIdent>) -> Vec<PrewarmTarget> {
  let PlanInput {
    attached_li,
    done_li,
    ident_li,
    pending_li,
  } = input;
  let shard_num = attached_li.len() as u16;
  if shard_num == 0 {
    return Vec::new();
  }
  let mut group_li: Vec<Vec<TableIdent>> = vec![Vec::new(); attached_li.len()];
  for tbl in table_li {
    let shard_id = usize::from(tbl.shard_index(shard_num));
    if attached_li[shard_id] && !done_li[shard_id] {
      group_li[shard_id].push(tbl);
    }
  }
  // 再消费 pending：这些表可能不在 attach 时刻的远端列举里（后来才创建）。
  for tbl in pending_li {
    let shard_id = usize::from(tbl.shard_index(shard_num));
    if attached_li[shard_id] {
      group_li[shard_id].push(tbl);
    }
  }
  group_li
    .into_iter()
    .enumerate()
    .filter_map(|(shard_id, mut table_li)| {
      table_li.sort_unstable_by(|left, right| {
        left
          .tbl_name
          .cmp(&right.tbl_name)
          .then(left.partition_id.cmp(&right.partition_id))
      });
      table_li.dedup();
      if table_li.is_empty() {
        return None;
      }
      Some(PrewarmTarget {
        shard_id,
        branch: ident_li[shard_id].0.clone(),
        term: ident_li[shard_id].1,
        table_li,
      })
    })
    .collect()
}

/// 分页列举远端已存在的分区；页间用 `PartitionPage::last` 去重，内存 O(页)。
async fn remote_partition_li<I, O: ObjectStore>(
  cloud: &CloudStore<I, O>,
) -> Result<Vec<TableIdent>> {
  let mut table_li = Vec::new();
  let mut after = None;
  let mut token = None;
  loop {
    let page = cloud
      .remote_partition_page(after.as_ref(), token.as_deref(), CLOUD_LIST_PAGE_KEYS)
      .await?;
    table_li.extend(page.table_li);
    after = page.last;
    token = page.next_token;
    if token.is_none() {
      break;
    }
  }
  Ok(table_li)
}

/// 汇总单表统计到整轮回执。
fn merge_stats(report: &mut PrewarmReport, stats: &PrewarmStat) {
  report.pulled_file_num += stats.total_files_pulled;
  report.files_failed += stats.files_failed;
  report.missing_num += stats.missing_num;
  report.disk_full_num += usize::from(stats.completion_reason == CompletionReason::DiskFull);
}

/// 逐个 shard 目标预热：先按该 shard 的 `(branch, term)` 对齐远端列举身份，再逐表
/// 预热（表级并发由 `table_concurrency` 限流）。
///
/// 返回汇总、**全部表成功**的 shard 列表（附完成时的 `(branch, term)`，供调用方
/// 比对期间是否被重新 attach），以及需回队的 `(target 下标, 分区)`。单表以
/// `DownloadError` / `DiskFull` / `ListingError` 结束时不置 `done_li`：该 shard 的
/// 失败分区必须回队重试，否则 `done_li` 会让它永不再预热。
async fn run_prewarm_targets<I, O>(
  cloud: &CloudStore<I, O>,
  target_li: &[PrewarmTarget],
  table_concurrency: usize,
) -> (
  PrewarmReport,
  Vec<(usize, String, u64)>,
  Vec<(usize, TableIdent)>,
)
where
  I: FileIo + ManifestIo + PageIo + SegmentIo,
  O: ObjectStore,
{
  let mut report = PrewarmReport::default();
  let mut done_li = Vec::with_capacity(target_li.len());
  let mut requeue_li = Vec::new();
  for (idx, target) in target_li.iter().enumerate() {
    cloud.set_identity(&target.branch, target.term);
    match prewarm_tables_with_concurrency(cloud, &target.table_li, table_concurrency).await {
      Ok(stats_li) => {
        report.table_num += stats_li.len();
        let mut failed_num = 0usize;
        for (tbl, stats) in &stats_li {
          merge_stats(&mut report, stats);
          if stats.completion_reason == CompletionReason::Success {
            continue;
          }
          failed_num += 1;
          requeue_li.push((idx, tbl.clone()));
        }
        if failed_num == 0 {
          done_li.push((target.shard_id, target.branch.clone(), target.term));
        } else {
          // 部分失败：整轮不完整，失败的 shard 不能置 `done_li`。
          report.listing_failed = true;
          warn!(
            target: "fkv_runtime",
            "prewarm shard partial failure shard_id={} failed_tables={}",
            target.shard_id,
            failed_num
          );
        }
      }
      Err(err) => {
        report.listing_failed = true;
        // 整 shard 列举失败：回队全部分区（去重与上限由 `enqueue_table` 兜住）。
        requeue_li.extend(target.table_li.iter().cloned().map(|tbl| (idx, tbl)));
        warn!(
          target: "fkv_runtime",
          "prewarm shard failed shard_id={} error={}",
          target.shard_id,
          err
        );
      }
    }
  }
  (report, done_li, requeue_li)
}

/// 取锁；poison 只说明上一次持锁线程 panic，状态本身仍可用。
fn lock_prewarm_state(state: &Mutex<PrewarmState>) -> MutexGuard<'_, PrewarmState> {
  state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 分支内部名的 8 位十六进制 salt：把完整 64 位微秒时间戳折叠成非零 `u32`。
///
/// `now_us() as u32` 每 2³² µs（约 71.6 分钟）回绕，同一逻辑名在两个回绕周期后
/// 会得到完全相同的内部名，第二次创建会落到已存在的 branch 上。这里先用
/// splitmix32 混合时间戳高位，保持 8 位十六进制形状的同时消除回绕重名。
/// 显式 `salt_ts` 不走本函数（对齐 C++ `SetSaltTimestamp` 的确定性命名）。
fn branch_salt(ts_us: u64) -> u32 {
  let mut x = (ts_us ^ (ts_us >> 32)) as u32;
  x ^= x >> 16;
  x = x.wrapping_mul(0x7feb_352d);
  x ^= x >> 15;
  x = x.wrapping_mul(0x846c_a68b);
  x ^= x >> 16;
  x.max(1)
}

/// Cloud 模式 S3 实例数：每个 shard 一个，加运行时侧全局列举（长驻 1 个）、
/// 预热 crond 与归档 crond（按配置启用，未启用不占额度）。
///
/// [`CloudBackend::s3_config`] 按该数均分 `max_cloud_concurrency`：漏算运行时侧
/// 实例会把整机飞行请求放大到 `实际实例数 × 每实例额度`。
fn cloud_s3_instance_num(num_threads: u16, archive_on: bool, prewarm_on: bool) -> usize {
  usize::from(num_threads) + 1 + usize::from(archive_on) + usize::from(prewarm_on)
}

/// 构造预热线程的 compio runtime。
///
/// 对齐 `fkv_shard::build_runtime`：优先 `SINGLE_ISSUER + DEFER_TASKRUN + TASKRUN_FLAG`
/// 的调优 ring，老内核 / 非 io_uring 后端不支持时回落默认配置。
fn build_prewarm_runtime() -> io::Result<runtime::Runtime> {
  let mut proactor = ProactorBuilder::new();
  proactor
    .single_issuer(true)
    .defer_taskrun(true)
    .taskrun_flag(true);
  match runtime::Runtime::builder().with_proactor(proactor).build() {
    Ok(runtime) => Ok(runtime),
    Err(err) => {
      warn!(
        target: "fkv_runtime",
        "prewarm tuned ring unavailable error={} fallback=default",
        err
      );
      runtime::Runtime::builder().build()
    }
  }
}

/// 预热后台线程：attach 唤醒一轮「列举远端分区 → 按 shard 分发 → 逐表预热」。
///
/// 与 [`ArchiveCrond`] 同构：crossfire 通道兼作唤醒与停止信号（容量 1 合并重复
/// attach），drop 发送端即停止。对齐 C++ `PrewarmService::PrewarmLoop`——预热不在
/// 请求路径上同步等待，attach 只投递信号就返回。
struct PrewarmCrond<I, O> {
  signal_tx: Option<MTx<Array<()>>>,
  join: Option<JoinHandle<()>>,
  _marker: PhantomData<fn() -> (I, O)>,
}

impl<I, O> PrewarmCrond<I, O>
where
  I: FileIo + ManifestIo + PageIo + SegmentIo + Send + 'static,
  O: ObjectStore + Send + 'static,
{
  fn start(
    cloud: CloudStore<I, O>,
    state: Arc<Mutex<PrewarmState>>,
    table_concurrency: usize,
  ) -> Result<Self> {
    let (signal_tx, signal_rx) = bounded_blocking::<()>(1);
    let join = thread::Builder::new()
      .name("fkv-prewarm".to_owned())
      .spawn(move || Self::run(cloud, state, table_concurrency, &signal_rx))
      .map_err(|_| KvError::IoFail)?;
    Ok(Self {
      signal_tx: Some(signal_tx),
      join: Some(join),
      _marker: PhantomData,
    })
  }

  fn run(
    cloud: CloudStore<I, O>,
    state: Arc<Mutex<PrewarmState>>,
    table_concurrency: usize,
    signal_rx: &Rx<Array<()>>,
  ) {
    // 本地 cache 是 `CompioStore`：页 / 文件 IO 走 io_uring（Linux）/ polling（macOS），
    // 必须在 compio runtime 内驱动。信号等待放在 runtime 之外阻塞 recv，否则占住
    // executor 线程会让在途 IO future 不被 poll。
    let runtime = match build_prewarm_runtime() {
      Ok(runtime) => runtime,
      Err(err) => {
        error!(target: "fkv_runtime", "prewarm runtime init failed error={}", err);
        return;
      }
    };
    // 上一轮是否仍有待重试目标（回队分区 / 列举失败）。有则带退避自唤醒，
    // 无则阻塞等外部 signal；发送端被 drop（shutdown / Drop）时返回 Err，线程退出。
    let mut retry_wait: Option<Duration> = None;
    loop {
      match retry_wait {
        Some(wait) => match signal_rx.recv_timeout(wait) {
          Ok(()) | Err(RecvTimeoutError::Timeout) => {}
          Err(RecvTimeoutError::Disconnected) => break,
        },
        None => {
          if signal_rx.recv().is_err() {
            break;
          }
        }
      }
      let retry = runtime.block_on(Self::tick(&cloud, &state, table_concurrency));
      retry_wait = retry.then(|| {
        retry_wait.map_or(PREWARM_RETRY_MIN, |wait| {
          wait.saturating_mul(2).min(PREWARM_RETRY_MAX)
        })
      });
    }
  }

  /// 跑一轮预热，返回是否还需要重试（有回队分区或本轮列举失败）。
  async fn tick(
    cloud: &CloudStore<I, O>,
    state: &Mutex<PrewarmState>,
    table_concurrency: usize,
  ) -> bool {
    // reopen 投递的单表目标不依赖整轮远端列举：即使列举失败也要先消费，
    // 否则 attach 之后新出现的分区在列举抖动时被永久丢弃。
    // 锁内只搬状态，分桶 / 去重在锁外做。
    let (target_li, listing_failed) = match remote_partition_li(cloud).await {
      Ok(table_li) => {
        let input = lock_prewarm_state(state).take_plan_input();
        (plan_targets(input, table_li), false)
      }
      Err(err) => {
        warn!(target: "fkv_runtime", "prewarm listing failed error={}", err);
        let input = lock_prewarm_state(state).take_plan_input();
        (plan_targets(input, Vec::new()), true)
      }
    };
    if target_li.is_empty() && !listing_failed {
      return false;
    }
    let mut report = PrewarmReport::default();
    if !target_li.is_empty() {
      let (round, done_li, requeue_li) =
        run_prewarm_targets(cloud, &target_li, table_concurrency).await;
      report = round;
      let mut state = lock_prewarm_state(state);
      // 列举失败时整轮不完整，不能把 shard 标成已预热。
      if !listing_failed {
        for (shard_id, branch, term) in done_li {
          // 下载期间可能被重新 attach 到新 `(branch, term)`：只有身份仍一致才置
          // done_li，否则会把新 term 误标为已预热。
          if state.attached_li[shard_id]
            && state.ident_li[shard_id].0 == branch
            && state.ident_li[shard_id].1 == term
          {
            state.done_li[shard_id] = true;
          }
        }
      }
      // 失败目标的分区已在 `take_plan_input` 里被取走，不回队就永不重试
      // （`done_li` 已置位的 shard 尤其如此）。身份仍一致才回队；重 attach
      // 换了身份则由 attach 轮的全量列举覆盖。按表回队：同 shard 里已成功的
      // 表不重复下载。
      for (idx, tbl) in requeue_li {
        let target = &target_li[idx];
        let shard_id = target.shard_id;
        if state.attached_li[shard_id]
          && state.ident_li[shard_id].0 == target.branch
          && state.ident_li[shard_id].1 == target.term
        {
          state.done_li[shard_id] = false;
          state.enqueue_table(tbl);
        }
      }
    }
    // 单 shard 失败也在 `round` 里置位，不能被本轮的远端列举结果覆盖。
    report.listing_failed |= listing_failed;
    let retry = {
      let mut state = lock_prewarm_state(state);
      state.last_report = Some(report);
      // 回队分区（`pending_tbl_li`）与列举失败都必须自唤醒重试，不能等外部 signal。
      report.listing_failed || !state.pending_tbl_li.is_empty()
    };
    info!(
      target: "fkv_runtime",
      "prewarm run tables={} pulled={} failed={} disk_full={} missing={} listing_failed={}",
      report.table_num,
      report.pulled_file_num,
      report.files_failed,
      report.disk_full_num,
      report.missing_num,
      report.listing_failed
    );
    retry
  }

  /// 唤醒一轮预热；容量 1 已被占满时合并（不阻塞 attach 路径）。
  fn signal(&self) {
    if let Some(tx) = &self.signal_tx {
      let _ = tx.try_send(());
    }
  }
}

/// `stop` 只操作通道与 join 句柄，不依赖 IO 泛型约束；放在无约束 impl 里
/// 才能被同样无约束的 `Drop` 调用。
impl<I, O> PrewarmCrond<I, O> {
  fn stop(&mut self) {
    self.signal_tx.take();
    if let Some(join) = self.join.take() {
      let _ = join.join();
    }
  }
}

impl<I, O> Drop for PrewarmCrond<I, O> {
  fn drop(&mut self) {
    self.stop();
  }
}

/// 构造运行时侧 `CloudStore` 所需的装配参数（从 `ShardConfig` 投影）。
///
/// 运行时侧的预热线程 / 归档 crond / 全局列举都需要一个 `CloudStore`，但它们
/// 不该长期持有一整份 `ShardConfig`。
#[derive(Debug, Clone)]
struct CloudStoreConfig {
  root: PathBuf,
  page_size: usize,
  pages_per_file_shift: u8,
  segment_size: u32,
  segments_per_file_shift: u8,
  max_write_batch_pages: usize,
  max_inflight_write: usize,
  fd_limit: usize,
  allow_reuse_local_caches: bool,
}

impl CloudStoreConfig {
  fn from_shard(config: &ShardConfig, cloud: &CloudBackend) -> Self {
    Self {
      root: config.root.clone(),
      page_size: config.page_size,
      pages_per_file_shift: config.pages_per_file_shift,
      segment_size: config.segment_size,
      segments_per_file_shift: config.segments_per_file_shift,
      max_write_batch_pages: config.max_write_batch_pages,
      max_inflight_write: config.max_inflight_write,
      fd_limit: config.fd_limit,
      allow_reuse_local_caches: cloud.allow_reuse_local_caches,
    }
  }
}

/// 构造 Cloud 模式的运行时侧 `CloudStore`（本地 cache + S3）。
///
/// `CloudStore` 内含 `RefCell` 非 `Sync`，shard 与运行时侧的预热线程 / 归档 crond
/// 必须各持实例，因此这里按同一装配参数（同一 root / bucket / prefix / segment
/// 配置）复刻 `StoreBackend::cloud`。
///
/// **有意不接 `ShardMetric`（cld-55）**：这里的实例服务 prewarm crond /
/// 全局列举等运行时侧工作，不归属任何 shard，把某个 shard 的计数器挂上去会让
/// 「这个 cache 的命中率」指向错误的 shard。shard 自己的实例在
/// `fkv_shard::StoreBackend::cloud` 里接线。
///
/// **预算边界（12d）**：实例不再各记一份 `local_space_limit`，而是与全部 shard
/// 共用 `CloudBackend::cache_group`：预热下载、shard 读写都计入同一份全局驻留
/// 记账，淘汰也按全局用量触发，因此整机本地 cache 上界是 `local_space_limit`
/// 而不是 `(shard_num + 1) × local_space_limit`。归档 crond / 全局列举实例只做
/// 远端列举、不写本地 cache，共享预算组不产生额外占用。
///
/// **只读边界**：本函数产出的实例（预热 / 全局列举 / 归档 crond）都不是分区
/// 写实例：`read_only(true)` 只关闭表级 segment scheme 的落盘与回填，安装 /
/// 读 cache 照常进行。跨进程共用同一本地 root 时只有该分区的写实例（shard
/// 侧 `StoreBackend::cloud`）能启用 scheme，否则预热会把对方进程正在写的
/// 半装文件按旧尾槽规则盖成「完整」。
fn open_runtime_cloud_store(
  config: &CloudStoreConfig,
  cloud: &CloudBackend,
) -> Result<CloudStore<CompioStore, S3ObjectStore>> {
  let local = CompioStore::new(
    config.root.clone(),
    config.page_size,
    config.pages_per_file_shift,
  )
  .with_segment_config(config.segment_size as usize, config.segments_per_file_shift)
  .with_managed_segment_read()
  .with_max_write_batch_pages(config.max_write_batch_pages)
  .with_max_inflight_write(config.max_inflight_write)
  .with_fd_cache_limit(config.fd_limit);
  // 与 shard 侧共用同一份 `s3_config()`：并发上限按实例数拆分，禁止两侧分叉。
  let remote = S3ObjectStore::start(cloud.s3_config())?;
  let store = CloudStore::new(
    local,
    remote,
    cloud.prefix.clone(),
    config.pages_per_file_shift,
  )
  .with_segment_config(config.segment_size as usize, config.segments_per_file_shift)
  .with_cache_group(cloud.cache_group.clone())
  .with_reuse_local_caches(config.allow_reuse_local_caches)
  // 运行时侧实例一律只读：scheme 回填权归该分区的 shard 写实例。
  .with_read_only(true);
  // 装配点自检：与 shard 侧共用同一份 KvOption 投影，禁止默认值分叉。
  debug_assert_eq!(
    store.reuse_local_caches(),
    config.allow_reuse_local_caches,
    "运行时侧 CloudStore 未接线 allow_reuse_local_caches"
  );
  debug_assert!(
    store.read_only(),
    "运行时侧 CloudStore 必须置只读，不得参与 segment scheme 回填"
  );
  debug_assert_eq!(
    config.allow_reuse_local_caches, cloud.allow_reuse_local_caches,
    "运行时侧与 shard 侧的 allow_reuse_local_caches 不一致"
  );
  Ok(store)
}

pub struct Runtime {
  shard_li: Vec<Shard>,
  archive_crond: Option<ArchiveCrond>,
  archive_state: Option<Arc<ArchiveState>>,
  prewarm_crond: Option<PrewarmCrond<CompioStore, S3ObjectStore>>,
  prewarm_state: Option<Arc<Mutex<PrewarmState>>>,
  /// 全局请求的目标集合来源：无 = Local/Mem 列盘，有 = Cloud 列远端。
  cloud_enum: Option<(CloudStoreConfig, CloudBackend)>,
  /// 列举用长生命周期 `CloudStore`：首次全局操作时装配，之后复用。
  ///
  /// 每次 `enumerate_target_li` 新建实例都会各起一个 S3 后台线程，全局归档 /
  /// 重开 / 删除频繁调用时线程数随之增长。实例只做远端列举、不写本地 cache，
  /// 串行调用复用同一实例；并发调用各自临时建、用完丢弃。
  cloud_enum_store: Mutex<Option<Box<CloudStore<CompioStore, S3ObjectStore>>>>,
  /// 本地数据根（Local/Mem/Standby 的全局列举与归档 crond 目标）。
  root_li: Vec<PathBuf>,
  /// 各 shard 的 attach 身份；`None` = 未 attach。
  ///
  /// 只用于全局结果名：`global_archive` / `global_delete_archive` 在 0 目标或
  /// 全部失败时按最早 shard 的身份构造 `manifest_<branch>_<term>_<tag>`。
  attach_ident_li: Arc<Mutex<Vec<Option<AttachIdent>>>>,
  /// 每 shard 的 attach 代际（`ShardRoute` 共享）。
  attach_epoch: Arc<AttachEpoch>,
  /// 本层 attach 视图集合（归档目标 / 预热路由 / attach 身份），`ShardRoute`
  /// 在错误收尾时据此清理；`ArchiveCrond` 持同一份。
  views: ShardViews,
  /// 全局 fanout 单批上限（`max_global_request_batch`）。
  max_global_request_batch: usize,
  /// 数据面默认请求 deadline；`None` = 不设（保持旧行为）。
  request_timeout: Option<Duration>,
  /// 数据面默认入队等待上限；`None` = 无限等待（保持旧行为）。
  queue_timeout: Option<Duration>,
  /// per-shard 控制面串行锁：bump 代际、发送请求与落地视图必须在同一临界区
  /// 内，否则并发 attach / detach 会让已在 shard 生效的 attach 因代际过期而
  /// 丢视图（shard `Attached`、本层视图空）。
  control_li: Vec<AsyncMutex<()>>,
}

/// 未给 `KvOption` 时的全局 fanout 单批上限（对齐 `fkv_config` 默认值）。
const DEFAULT_MAX_GLOBAL_REQUEST_BATCH: u32 = 1000;

/// 观测类请求（`ListStandbyPartition`）的默认 deadline。
///
/// 该类请求在 shard 线程内同步跑 `ssh` / `ls`（见 `fkv_standby::rsync`），
/// master 卡死时没有 deadline 会让调用方永久挂起；`with_request_timeout`
/// 显式配置时以配置为准。
const DEFAULT_OBSERVE_TIMEOUT: Duration = Duration::from_secs(30);

/// 本地数据根列表：`store_path` 非空时以其为准，否则保持 `RuntimeConfig::root`。
///
/// `store_path_lut` 由 `KvOption::validate` 构建；这里只检查它已就绪，
/// 不重新计算权重分布（索引越界由 `RootedConfig::new` 校验）。Cloud 模式
/// 不使用该列表（本地 cache 仍是 `RuntimeConfig::root`）。
fn local_root_li(option: Option<&KvOption>, root: &Path) -> Result<Vec<PathBuf>> {
  let Some(option) = option else {
    return Ok(vec![root.to_path_buf()]);
  };
  if option.store_path.is_empty() {
    return Ok(vec![root.to_path_buf()]);
  }
  if option.store_path_lut_li.is_empty() {
    error!(
      target: "fkv_runtime",
      "store_path configured but store_path_lut is empty (validate not called)"
    );
    return Err(KvError::InvalidArgs);
  }
  Ok(
    option
      .store_path
      .iter()
      .map(|path| PathBuf::from(path.as_str()))
      .collect(),
  )
}

/// 按 `KvOption` 的运行模式选择 shard IO 后端。
///
/// - `Local`：本地 `CompioStore`；`store_path` 多根时按 LUT 选根。
/// - `Cloud`：本地 cache + S3（`cloud_store_path` = `bucket/prefix`）。
/// - `StandbyMaster` / `StandbyReplica`：数据面仍是本地 `CompioStore`（standby 是
///   「本地存储之上的复制」）；replica 的分区追赶挂在 reopen 路径上，
///   见 `standby_service()` 与 `fkv_shard::sync_standby_partition`。
///
/// 多根 standby 暂不支持：rsync 追赶服务按单 `local_root` 工作，允许多根会让
/// 副本只追第一个根、其余根静默落后；显式拒绝而不是假装成功。
fn backend_of(option: Option<&KvOption>, root: &Path) -> Result<Backend> {
  let Some(option) = option else {
    return Ok(Backend::Local);
  };
  match option.derive_mode() {
    StoreMode::Local => local_backend(Some(option), root),
    StoreMode::Cloud => {
      // 只支持 S3 兼容后端（`aws` / `s3`）。gcs 等既无实现也无对照，显式拒绝而不是
      // 静默按 S3 起（否则凭据 / endpoint 语义错配会以超时表现，难定位）。
      if !option.cloud_provider.eq_ignore_ascii_case("aws")
        && !option.cloud_provider.eq_ignore_ascii_case("s3")
      {
        error!(
          target: "fkv_runtime",
          "cloud provider unsupported provider={}",
          option.cloud_provider
        );
        return Err(KvError::InvalidArgs);
      }
      let (bucket, prefix) = split_cloud_store_path(&option.cloud_store_path)?;
      Ok(Backend::Cloud(CloudBackend {
        bucket,
        prefix,
        endpoint: option.cloud_endpoint.clone(),
        region: option.cloud_region.clone(),
        access_key: option.cloud_access_key.clone(),
        secret_key: option.cloud_secret_key.clone(),
        allow_http: option.cloud_endpoint.starts_with("http://"),
        // 本地 cache 预算取 `local_space_limit`（0 = 不限），对齐 C++ 的本地空间
        // 上限；整组由全部 shard 与运行时侧预热共享（12d）。
        cache_group: Arc::new(CacheGroup::new(option.local_space_limit)),
        // 复用旧 cache 默认关闭（对齐 C++）；必须显式投影，否则该旋钮无效。
        allow_reuse_local_caches: option.allow_reuse_local_caches,
        max_cloud_concurrency: option.max_cloud_concurrency as usize,
        // 运行时启动后按 `num_threads` 覆写为真实实例数，这里先按单实例。
        s3_instance_num: 1,
        verify_ssl: option.cloud_verify_ssl,
      }))
    }
    StoreMode::StandbyMaster | StoreMode::StandbyReplica => {
      // 数据面仍旧是本地存储（standby 是「本地存储之上的复制」，对齐 C++
      // `StoreMode` 语义）；replica 的分区追赶挂在 reopen 路径上，
      // 见 `standby_service()` 与 `fkv_shard::sync_standby_partition`。
      let root_li = local_root_li(Some(option), root)?;
      if root_li.len() > 1 {
        error!(
          target: "fkv_runtime",
          "standby multi-root store_path unsupported roots={}",
          root_li.len()
        );
        return Err(KvError::InvalidArgs);
      }
      local_backend(Some(option), root)
    }
  }
}

/// 本地后端：`store_path` 为空保持单 `RuntimeConfig::root`；非空时单根 / 多根
/// 都走 `RootedStore`（单根 LUT = `[0]`，固定 buffer 与旧行为一致）。
fn local_backend(option: Option<&KvOption>, root: &Path) -> Result<Backend> {
  let Some(option) = option.filter(|option| !option.store_path.is_empty()) else {
    return Ok(Backend::Local);
  };
  let root_li = local_root_li(Some(option), root)?;
  Ok(Backend::LocalRouted(RootedConfig::new(
    root_li,
    option.store_path_lut_li.clone(),
  )?))
}

/// `StandbyReplica` 的追赶服务；其余模式返回 `None`。
///
/// 来源是全部 `standby_master_store_paths`，按
/// `standby_master_store_path_lut_li[partition_id % lut.len()]` 选根（cld-58，
/// 与本地多根 `RootedConfig::root_index` / C++ `TableIdent::StorePathIndex`
/// 同式）；单根时 LUT 恒 `[0]`，行为不变。`StandbyConfig::source_addr()` 为空
/// （`standby_master_addr` 为空或 `"local"`）时走**本机 rsync**，否则走
/// `rsync`/`ssh <addr>`——与 C++ `StandbyService::RemoteSpec` 同一判定。
fn standby_service(
  option: &KvOption,
  local_root: &Path,
) -> Result<Option<Arc<RsyncStandbyService>>> {
  if option.derive_mode() != StoreMode::StandbyReplica {
    return Ok(None);
  }
  if option.standby_master_store_paths.is_empty()
    || option.standby_master_store_path_lut_li.is_empty()
  {
    error!(
      target: "fkv_runtime",
      "standby replica requires standby_master_store_paths and its lut"
    );
    return Err(KvError::InvalidArgs);
  }
  let config = StandbyConfig::from_option(option, local_root)?;
  // 多根 master：路径与 LUT 都由 `KvOption::validate` 从
  // `standby_master_store_path_weights` 生成；这里只消费，不再自行推导。
  let source = RsyncSource::new_rooted(
    StdCommandRunner,
    option.standby_master_store_paths.clone(),
    option.standby_master_store_path_lut_li.clone(),
    config.source_addr().to_owned(),
  )?;
  Ok(Some(Arc::new(RsyncStandbyService::new(config, source)?)))
}

/// `RuntimeConfig` 与 `KvOption` 双处存在的盘面 / 重试字段合并。
///
/// 只有偏离 `KvOption` 默认值的才算「有语义」：两侧都显式且不一致直接拒绝，
/// 避免同名字段静默分叉；否则显式值优先，未显式时沿用 `RuntimeConfig`。
fn merge_field<T>(
  name: &'static str,
  from_config: T,
  from_option: Option<T>,
  default: T,
) -> Result<T>
where
  T: Copy + PartialEq + fmt::Debug,
{
  let Some(value) = from_option else {
    return Ok(from_config);
  };
  if value != default && from_config != default && value != from_config {
    error!(
      target: "fkv_runtime",
      "runtime config conflict field={name} config={from_config:?} option={value:?}"
    );
    return Err(KvError::InvalidArgs);
  }
  Ok(if value == default { from_config } else { value })
}

impl Runtime {
  pub fn start(config: RuntimeConfig) -> Result<Self> {
    Self::start_impl(config, None, None)
  }

  /// 用 `KvOption` 驱动压缩 / checksum / overflow / compaction / OOM 重试等配置。
  pub fn start_with_option(config: RuntimeConfig, option: &KvOption) -> Result<Self> {
    let mut option = option.clone();
    option.validate()?;
    Self::start_impl(config, Some(&option), None)
  }

  /// 纯内存模式：数据只存在 `MemStore`，进程退出即丢，无 durability。
  ///
  /// `KvOption::derive_mode()` 没有 Mem（C++ `StoreMode` 同样只有
  /// Local/Cloud/Standby），因此单独提供入口，而不是让调用方去碰
  /// `fkv_shard` 的内部 `Backend`。`config.root` 仅用于日志与本地 cache
  /// 约定，本模式不落盘；全局列举 / 定时归档的目标集合为空。
  pub fn start_mem(config: RuntimeConfig) -> Result<Self> {
    Self::start_impl(config, None, Some(Backend::Mem))
  }

  fn start_impl(
    config: RuntimeConfig,
    option: Option<&KvOption>,
    backend_override: Option<Backend>,
  ) -> Result<Self> {
    // 盘面 / 重试字段与 `KvOption` 同名：显式值优先，两侧都显式且冲突则拒绝。
    let option_default = KvOption::default();
    let num_threads = merge_field(
      "num_threads",
      config.num_threads,
      option.map(|opts| opts.num_threads),
      option_default.num_threads,
    )?;
    let page_size = merge_field(
      "page_size",
      config.page_size,
      option.map(|opts| opts.data_page_size as usize),
      option_default.data_page_size as usize,
    )?;
    let pages_per_file_shift = merge_field(
      "pages_per_file_shift",
      config.pages_per_file_shift,
      option.map(|opts| opts.pages_per_file_shift),
      option_default.pages_per_file_shift,
    )?;
    let segment_size = merge_field(
      "segment_size",
      config.segment_size,
      option.map(|opts| opts.segment_size),
      option_default.segment_size,
    )?;
    let segments_per_file_shift = merge_field(
      "segments_per_file_shift",
      config.segments_per_file_shift,
      option.map(|opts| opts.segments_per_file_shift),
      option_default.segments_per_file_shift,
    )?;
    let auto_reopen_retry_times = merge_field(
      "auto_reopen_retry_times",
      config.auto_reopen_retry_times,
      option.map(|opts| opts.auto_reopen_retry_times),
      option_default.auto_reopen_retry_times,
    )?;
    let auto_reopen_pending_time_us = merge_field(
      "auto_reopen_pending_time_us",
      config.auto_reopen_pending_time_us,
      option.map(|opts| opts.auto_reopen_pending_time_us),
      option_default.auto_reopen_pending_time_us,
    )?;
    if num_threads == 0 {
      error!(target: "fkv_runtime", "runtime start rejected: num_threads=0");
      return Err(KvError::InvalidArgs);
    }
    info!(
      target: "fkv_runtime",
      "runtime start threads={} root={} page_size={} pages_per_file_shift={}",
      num_threads,
      config.root.display(),
      page_size,
      pages_per_file_shift
    );
    // 归档参数与盘面字段同一套合并规则：`KvOption` 显式（非默认）才覆盖
    // `RuntimeConfig`，两侧都显式且冲突直接拒绝，避免传一份 KvOption 就把
    // 定时归档静默关掉（`archive_interval_secs` 默认 0）。提前到后端装配前：
    // S3 实例数要把归档 crond 计进去。
    let archive_interval_secs = merge_field(
      "archive_interval_secs",
      config.archive_interval_secs,
      option.map(|opts| u64::from(opts.archive_interval_secs)),
      u64::from(option_default.archive_interval_secs),
    )?;
    let mut backend = match backend_override {
      Some(backend) => backend,
      None => backend_of(option, &config.root)?,
    };
    // 每个 shard 各一个 S3ObjectStore 线程；运行时侧另有全局列举（长驻 1 个）、
    // 预热 crond（可选）与归档 crond（可选，惰性创建）。按真实构造点计数，
    // 未启用的 crond 不占额度；并发上限按实例数均分，避免整机实际并发被放大到
    // `实例数 × max_cloud_concurrency`。
    if let Backend::Cloud(cloud) = &mut backend {
      cloud.s3_instance_num = cloud_s3_instance_num(
        num_threads,
        archive_interval_secs > 0,
        option.is_some_and(|opts| opts.prewarm_cloud_cache),
      );
    }
    // 本地全局列举 / 归档 crond 的根集合：Local 用 store_path（单根或多根），
    // Cloud 的本地 cache 仍是 RuntimeConfig::root；Mem 不落盘，目标集合为空。
    let root_li = match &backend {
      Backend::Cloud(_) => vec![config.root.clone()],
      Backend::Mem => Vec::new(),
      _ => local_root_li(option, &config.root)?,
    };
    // 运行模式透传到 shard：决定 `ResourceMissing` 是本地懒创建还是等 manifest 可见。
    let mode = option.map_or(StoreMode::Local, |opts| opts.derive_mode());
    // `StandbyReplica`：reopen 前追赶 master 的服务（`standby_master_addr` 为空或
    // `"local"` 时走本机 rsync）。启动失败必须冒泡——副本没在追赶却报成功是最坏结果。
    let standby = match option {
      // standby 的本地根与数据根一致（`store_path` 单根时用它，否则 RuntimeConfig::root）。
      Some(opts) => standby_service(opts, root_li.first().ok_or(KvError::InvalidArgs)?)?,
      None => None,
    };
    if standby.is_some() {
      info!(
        target: "fkv_runtime",
        "standby replica enabled master_paths={:?} master_path_lut={:?}",
        option.map(|opts| opts.standby_master_store_paths.clone()),
        option.map(|opts| opts.standby_master_store_path_lut_li.clone())
      );
    }
    let mut shard_li = Vec::with_capacity(usize::from(num_threads));
    // FD 上限按 shard 均分（对齐 C++ `store_fd_limit / num_threads`）；0 保留为不限制。
    let store_fd_limit = option.map_or(fkv_config::DEFAULT_FD_LIMIT as usize, |opts| {
      opts.fd_limit as usize
    });
    let shard_fd_limit = if store_fd_limit == 0 {
      0
    } else {
      (store_fd_limit / usize::from(num_threads)).max(1)
    };
    // root meta 字节预算同样按 shard 均分（对齐 C++ `root_meta.cpp` 的
    // `capacity_bytes_ = root_meta_cache_size / num_threads`）；0 保留为不限制。
    let store_root_meta_cache_bytes = option
      .map_or(fkv_config::DEFAULT_ROOT_META_CACHE_BYTES, |opts| {
        opts.root_meta_cache_size
      });
    let shard_root_meta_cache_bytes = if store_root_meta_cache_bytes == 0 {
      0
    } else {
      usize::try_from(store_root_meta_cache_bytes / u64::from(num_threads))
        .unwrap_or(usize::MAX)
        .max(1)
    };
    // SQ 深度对齐 C++ `io_queue_size`；未给 KvOption 时用默认值。
    let io_queue_size = option.map_or(fkv_config::DEFAULT_IO_QUEUE_SIZE, |opts| opts.io_queue_size);
    // 写 IO 两个上限同源投影，避免默认值 / 配置值在 runtime 内再分叉。
    let (max_inflight_write, max_write_batch_pages) = write_limits(option);
    // `KvOption` → `ShardConfig` 的唯一投影点：除 `shard_id` / `fd_limit` 外
    // 所有字段与 shard 无关，先解一次再逐 shard 克隆，回退默认值只在此处出现。
    let shard_cfg = ShardConfig {
      shard_id: 0,
      root: config.root.clone(),
      page_size,
      // 两旋钮各取各的；未给 KvOption 时回落到 RuntimeConfig 的共用值。
      restart_interval: option.map_or(config.restart_interval, |opts| {
        opts.data_page_restart_interval
      }),
      index_restart_interval: option.map_or(config.restart_interval, |opts| {
        opts.index_page_restart_interval
      }),
      pages_per_file_shift,
      segment_size,
      segments_per_file_shift,
      large_value_threshold: config.large_value_threshold,
      fixed_buffer_size: config.fixed_buffer_size,
      queue_size: config.queue_size,
      auto_reopen_retry_times,
      auto_reopen_pending_time_us,
      oom_retry_times: option.map_or(
        u32::from(fkv_config::DEFAULT_AUTO_OOM_RETRY_TIMES),
        |opts| u32::from(opts.auto_oom_retry_times),
      ),
      compression_level: option
        .filter(|opts| opts.enable_compression)
        .map(|opts| opts.zstd_compression_level),
      verify_checksum: option.is_none_or(|opts| !opts.skip_verify_checksum),
      overflow_pointers: option.map_or(fkv_page::DEFAULT_OVERFLOW_POINTERS, |opts| {
        opts.overflow_pointers
      }),
      // 未给 KvOption 时回落到 KvOption 默认值，与 `start_with_option(default)`
      // 同源；否则默认路径关闭压缩、显式路径默认开启，同默认两套行为。
      file_amplify_factor: option.map_or(option_default.file_amplify_factor, |opts| {
        opts.file_amplify_factor
      }),
      segment_file_amplify_factor: option
        .map_or(option_default.segment_file_amplify_factor, |opts| {
          opts.segment_file_amplify_factor
        }),
      manifest_limit: option.map_or(DEFAULT_MANIFEST_LIMIT as usize, |opts| {
        opts.manifest_limit as usize
      }),
      max_write_batch_pages,
      max_inflight_write,
      fd_limit: shard_fd_limit,
      index_cache_bytes: option.map_or(fkv_config::DEFAULT_INDEX_CACHE_BYTES, |opts| {
        opts.index_cache_bytes
      }) as usize,
      index_cache_total_bytes: option.map_or(fkv_config::DEFAULT_INDEX_CACHE_BYTES, |opts| {
        opts.index_cache_total_bytes
      }) as usize,
      root_meta_cache_bytes: shard_root_meta_cache_bytes,
      io_queue_size,
      backend,
      mode,
      standby,
      max_write_concurrency: option.map_or(0, |opts| opts.max_write_concurrency as usize),
      max_processing_time_us: option.map_or(DEFAULT_MAX_PROCESSING_TIME_US, |opts| {
        opts.max_processing_time_us
      }),
      yield_budget_us: option.map_or(fkv_config::DEFAULT_YIELD_BUDGET_US, |opts| {
        opts.yield_budget_us
      }),
    };
    for shard_id in 0..num_threads {
      let per_shard_cfg = ShardConfig {
        shard_id: u32::from(shard_id),
        ..shard_cfg.clone()
      };
      // 线程创建在 runner 资源紧张时可能瞬时返回 EAGAIN（映射为 `IoFail`）。
      // 重试几次，避免把瞬时资源压力报成启动失败；其它错误原样返回。
      let mut attempt = 0;
      let shard = loop {
        match Shard::start(per_shard_cfg.clone()) {
          Ok(shard) => break shard,
          Err(KvError::IoFail) if attempt < 10 => {
            attempt += 1;
            error!(
              target: "fkv_runtime",
              "runtime shard start retry shard_id={} attempt={} error=IoFail",
              shard_id,
              attempt
            );
            thread::sleep(Duration::from_millis(50));
          }
          Err(err) => {
            error!(
              target: "fkv_runtime",
              "runtime shard start failed shard_id={} error={}",
              shard_id,
              err
            );
            return Err(err);
          }
        }
      };
      shard_li.push(shard);
    }
    // 运行时侧 Cloud 装配参数：预热线程 / 归档 crond / 全局列举共用同一份
    // （`CloudStore` 非 `Sync`，但装配参数一致，只有实例不同）。
    let cloud_enum = match &shard_cfg.backend {
      Backend::Cloud(cloud) => Some((
        CloudStoreConfig::from_shard(&shard_cfg, cloud),
        cloud.clone(),
      )),
      _ => None,
    };
    let max_global_request_batch =
      usize::try_from(option.map_or(DEFAULT_MAX_GLOBAL_REQUEST_BATCH, |opts| {
        opts.max_global_request_batch
      }))
      .unwrap_or(1)
      .max(1);
    let archive_num_retained = merge_field(
      "num_retained_archives",
      config.archive_num_retained,
      option.map(|opts| opts.num_retained_archives),
      option_default.num_retained_archives,
    )?;
    let archive_state = (archive_interval_secs > 0).then(|| {
      // Cloud 模式归档目标以**远端**为准：本地 cache 只看得见已 cache 的分区，
      // 否则远端已有、本地未 cache 的分区会永远不被归档。
      let target = match &cloud_enum {
        Some((cfg, cloud)) => ArchiveTarget::Cloud(Box::new(cfg.clone()), Box::new(cloud.clone())),
        None => ArchiveTarget::Local(root_li.clone()),
      };
      Arc::new(ArchiveState::new(
        num_threads,
        target,
        Duration::from_secs(archive_interval_secs),
        archive_num_retained,
        u32::try_from(max_global_request_batch).unwrap_or(u32::MAX),
      ))
    });
    // Cloud 模式预热：`prewarm_cloud_cache` 打开时按 `prewarm_task_count` 起后台线程。
    // attach 只向 crond 投递信号就返回，不在请求路径上同步等下载（对齐 C++
    // `AttachShard` 排队 + `PrewarmService::PrewarmLoop`）。
    //
    // 先于归档 crond 建：归档线程的 `ShardRoute` 也带预热视图，shard 过期时两边
    // 一起失效。
    let (prewarm_crond, prewarm_state) = match (option, cloud_enum.as_ref()) {
      (Some(opts), Some((cfg, cloud))) if opts.prewarm_cloud_cache => {
        let store = open_runtime_cloud_store(cfg, cloud)?;
        let state = Arc::new(Mutex::new(PrewarmState::new(num_threads)));
        let task_count = usize::from(opts.prewarm_task_count).max(1);
        let crond = PrewarmCrond::start(store, state.clone(), task_count)?;
        info!(
          target: "fkv_runtime",
          "prewarm crond start task_count={}",
          task_count
        );
        (Some(crond), Some(state))
      }
      _ => (None, None),
    };
    let attach_ident_li = Arc::new(Mutex::new(vec![None; usize::from(num_threads)]));
    let attach_epoch = Arc::new(AttachEpoch::new(num_threads));
    let views = ShardViews {
      archive: archive_state.clone(),
      prewarm: prewarm_state.clone(),
      ident: Some(Arc::clone(&attach_ident_li)),
    };
    let archive_crond = if let Some(state) = &archive_state {
      // 归档 crond 的 sender 不套数据面 timeout：归档本身可能远慢于默认 deadline。
      let route_li: Vec<ShardRoute> = shard_li
        .iter()
        .map(|shard| ShardRoute::new(shard, shard.sender(), &attach_epoch, &views))
        .collect();
      // 配置了归档就必须有定时任务：启动失败上抛，不能静默降级为「归档关闭」。
      let crond = ArchiveCrond::start(state.clone(), route_li)?;
      info!(
        target: "fkv_runtime",
        "archive crond start interval_secs={} retain={}",
        archive_interval_secs,
        archive_num_retained
      );
      Some(crond)
    } else {
      None
    };
    Ok(Self {
      shard_li,
      archive_crond,
      archive_state,
      prewarm_crond,
      prewarm_state,
      cloud_enum,
      cloud_enum_store: Mutex::new(None),
      root_li,
      attach_ident_li,
      attach_epoch,
      views,
      max_global_request_batch,
      request_timeout: None,
      queue_timeout: None,
      control_li: (0..usize::from(num_threads))
        .map(|_| AsyncMutex::new(()))
        .collect(),
    })
  }

  pub fn num_threads(&self) -> u16 {
    self.shard_li.len() as u16
  }

  /// 数据面请求默认 deadline：所有经 [`Runtime::request`] 的便捷方法生效。
  ///
  /// 控制面 `attach_shard` / `detach_shard` 不套用：这两个必须拿到确定性回执，
  /// 超时会把 shard 状态留在半途。
  pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
    self.request_timeout = Some(timeout);
    self
  }

  /// 数据面请求的队列背压上限：队列满时最长等待 `timeout`，超时回 `Busy`。
  ///
  /// 不设时保持旧行为（无限等待）。
  pub fn with_queue_timeout(mut self, timeout: Duration) -> Self {
    self.queue_timeout = Some(timeout);
    self
  }

  pub fn metrics(&self) -> MetricsSnapshot {
    MetricsSnapshot {
      shard_li: self.shard_li.iter().map(Shard::metrics_snapshot).collect(),
    }
  }

  /// 最近一轮 Cloud 预热回执；未开启预热（非 Cloud，或 `prewarm_cloud_cache=false`）
  /// 时为 `None`。预热在后台线程跑，本查询不进请求路径。
  pub fn prewarm_report(&self) -> Option<PrewarmReport> {
    self
      .prewarm_state
      .as_ref()
      .and_then(|state| lock_prewarm_state(state).last_report)
  }

  /// reopen 成功后按表投递预热（仅 Cloud + `prewarm_cloud_cache` 生效）。
  ///
  /// 对齐 C++ `ReopenTask::Reopen` 末尾的 `PrewarmService::Prewarm(tbl)`：
  /// 只入队并唤醒后台 crond，不阻塞 reopen 回执。attach 轮的 `done_li` 只覆盖
  /// attach 瞬间的远端分区集合，reopen 是「新分区后来才出现」的补充触发源。
  fn signal_table_prewarm(&self, tbl: &TableIdent) {
    let (Some(state), Some(crond)) = (&self.prewarm_state, &self.prewarm_crond) else {
      return;
    };
    lock_prewarm_state(state).enqueue_table(tbl.clone());
    crond.signal();
  }

  /// 全局请求的目标集合：Local/Mem 列盘上分区目录；Cloud 列远端分区
  /// （`ObjectStore::list_page(prefix)` 分页，内存 O(页)）。
  ///
  /// Cloud 下复用运行时侧列举 store（控制面操作、不频繁；列举不碰本地
  /// cache，所以不需要 compio runtime）。实例长驻，避免每次全局操作都新起
  /// 一个 S3 线程。
  async fn enumerate_target_li(&self) -> Result<Vec<TableIdent>> {
    match &self.cloud_enum {
      Some((cfg, cloud)) => {
        // `CloudStore` 非 `Sync`，而 `Runtime` 要被多线程 `Arc` 共享：锁内只做
        // 取出 / 放回，不跨 await 持锁。并发调用各自临时建实例，串行调用复用。
        let store = {
          let mut slot = self
            .cloud_enum_store
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
          match slot.take() {
            Some(store) => store,
            None => Box::new(open_runtime_cloud_store(cfg, cloud)?),
          }
        };
        let result = remote_partition_li(&store).await;
        let mut slot = self
          .cloud_enum_store
          .lock()
          .unwrap_or_else(PoisonError::into_inner);
        if slot.is_none() {
          *slot = Some(store);
        }
        result
      }
      None => Ok(scan_table_li_li(&self.root_li)),
    }
  }

  /// 跨 shard fanout（按 `max_global_request_batch` 分批）。
  fn fanout(&self) -> GlobalFanout {
    self.settle_timeout_views();
    let route_li: Vec<ShardRoute> = self
      .shard_li
      .iter()
      .map(|shard| self.route_of_shard(shard))
      .collect();
    GlobalFanout::new(route_li, self.max_global_request_batch)
  }

  /// 复核本地超时登记：某次请求本地 `Timeout` 时 shard 仍是 `Attached`，之后才
  /// 可能因请求真正执行而置 `Expired`。这里在下一次观测点（取结果名 / fanout 前）
  /// 补做一次判定，把那时读到的 `Attached` 变成「稍后再看」而不是「永远有效」。
  ///
  /// 已接受的近似：两次观测之间视图仍可能残留，`fkv_shard` 不提供 Expired 写点
  /// 回调；清视图前仍校验代际，避免抹掉更晚 attach 的新视图。
  fn settle_timeout_views(&self) {
    for shard in &self.shard_li {
      let shard_id = shard.shard_id();
      let Some(epoch) = self
        .attach_epoch
        .settle_timeout(shard_id, shard.lifecycle().is_attached())
      else {
        continue;
      };
      self
        .views
        .clear_if_epoch(&self.attach_epoch, shard_id, epoch);
    }
  }

  /// 按本运行时配置装配 sender：默认 deadline / 队列背压上限。
  fn sender_of(&self, shard: &Shard) -> ShardSender {
    shard
      .sender()
      .with_request_timeout(self.request_timeout)
      .with_queue_timeout(self.queue_timeout)
  }

  /// shard 发送端 + 收尾句柄（数据面默认 deadline）。
  fn route_of_shard(&self, shard: &Shard) -> ShardRoute {
    ShardRoute::new(
      shard,
      self.sender_of(shard),
      &self.attach_epoch,
      &self.views,
    )
  }

  /// 按 shard id 取数据面路由；生产路径都按 `TableIdent` 路由，仅测试需要。
  #[cfg(test)]
  fn route_of(&self, shard_id: u32) -> Result<ShardRoute> {
    Ok(self.route_of_shard(self.shard_by_id(shard_id)?))
  }

  /// 控制面路由：不套数据面 deadline / 队列超时。
  ///
  /// attach / detach 必须拿到确定性回执：本地超时会把 shard 状态留在半途
  /// （attach 已生效但调用方以为失败）。
  fn control_route(&self, shard_id: u32) -> Result<ShardRoute> {
    let shard = self.shard_by_id(shard_id)?;
    Ok(ShardRoute::new(
      shard,
      shard.sender(),
      &self.attach_epoch,
      &self.views,
    ))
  }

  fn route(&self, tbl: &TableIdent) -> ShardRoute {
    self.route_of_shard(self.shard(tbl))
  }

  /// 全局归档：目标集合 = 全部分区；按 `max_global_request_batch` 分批 fanout。
  /// 单表失败（含未 attach shard 的 `NotRunning`）计入回执，不静默。
  ///
  /// 空 tag 时**只生成一次**并从本层广播给全部分区（对齐 C++
  /// `HandleGlobalArchiveRequest` 的 `tag = UnixTs` + `archive_req->SetTag(tag)`），
  /// 保证一次全局归档是一份同名快照；生成的 tag 与归档 manifest 名通过
  /// [`GlobalReply::tag`] / [`GlobalReply::name`] 返回给调用方。
  ///
  /// 0 个目标（空盘 / 全部 detach）或全部失败时 `name` 仍按 attach 身份构造
  /// （见 [`Self::global_archive_name`]），与 C++ `result_archive_` 的赋值时机一致。
  pub async fn global_archive(&self, tag: &str, num_retained: u32) -> Result<GlobalReply> {
    let tag = if tag.is_empty() {
      fkv_api::archive_tag()
    } else {
      tag.to_owned()
    };
    // 0 个成功回执时按 attach 身份构造名字：C++ `HandleGlobalArchiveRequest`
    // 在列举分区前就写 `result_archive_`（`BranchArchiveName`），空集合也回传。
    let table_li = self.enumerate_target_li().await?;
    let (mut reply, first_response) = self
      .fanout()
      .dispatch_collect(&table_li, |tbl| Request::CreateArchive {
        tbl: tbl.clone(),
        tag: tag.clone(),
        num_retained,
      })
      .await;
    reply.name = match first_response {
      // 首个成功分区可能是空表：C++ 在列举前就写 result_archive_，空表不返回
      // 空名。只有非空归档回执才覆盖身份名。
      Some(Response::CreateArchive(archive)) if !archive.name.is_empty() => archive.name,
      _ => self.global_archive_name(&tag),
    };
    reply.tag = tag;
    Ok(reply)
  }

  /// 全局重开：`tag` 为空按活跃快照重开（等价普通 `Reopen`）；非空按归档 tag
  /// 重开（对齐 C++ `GlobalReopenRequest`：同一个 tag 广播到全部分区）。
  pub async fn global_reopen(&self, tag: &str) -> Result<GlobalReply> {
    let table_li = self.enumerate_target_li().await?;
    Ok(
      self
        .fanout()
        .dispatch(&table_li, |tbl| {
          if tag.is_empty() {
            Request::Reopen { tbl: tbl.clone() }
          } else {
            Request::ReopenArchive {
              tbl: tbl.clone(),
              tag: tag.to_owned(),
            }
          }
        })
        .await,
    )
  }

  /// 全局删除归档 tag：同一 tag 广播到全部分区（对齐 C++
  /// `GlobalArchiveRequest::Action::Delete`，空 tag 非法）。
  ///
  /// `name` 与 create 同路径返回（C++ 删归档也走 `HandleGlobalArchiveRequest`
  /// 的 `result_archive_`），0 目标 / 全部失败时按 attach 身份构造。
  pub async fn global_delete_archive(&self, tag: &str) -> Result<GlobalReply> {
    if tag.is_empty() {
      return Err(KvError::InvalidArgs);
    }
    let table_li = self.enumerate_target_li().await?;
    let mut reply = self
      .fanout()
      .dispatch(&table_li, |tbl| Request::DeleteArchive {
        tbl: tbl.clone(),
        tag: tag.to_owned(),
      })
      .await;
    reply.tag = tag.to_owned();
    reply.name = self.global_archive_name(tag);
    Ok(reply)
  }

  /// 全局建分支（branch = 快照）：目标集合 = 全部分区。
  ///
  /// 对齐 C++ `HandleGlobalCreateBranchRequest`：调用方给的是**逻辑名**，本层
  /// 归一化后追加 `-<8 位十六进制 salt>` 作为内部 branch 名，并通过
  /// [`GlobalReply::name`] 返回；调用方必须用该名字继续操作分支。
  /// salt 由完整 64 位微秒时间戳经 [`branch_salt`] 混合折叠成非零 32 位：
  /// 低 32 位约每 71.6 分钟回绕一次，先与时戳高位异或再 splitmix 混合，
  /// 使跨回绕周期的同一逻辑名得到不同内部名；定宽 `%08x` 输出。
  pub async fn global_create_branch(&self, branch: &str) -> Result<GlobalReply> {
    self.global_create_branch_with_salt(branch, 0).await
  }

  /// 同 [`Self::global_create_branch`]，但 salt 可由调用方指定：
  /// `salt_ts == 0` 用实时时钟，非 0 用给定时间戳低 32 位 —— 对齐 C++
  /// `GlobalCreateBranchRequest::SetSaltTimestamp` 的确定性命名语义。
  pub async fn global_create_branch_with_salt(
    &self,
    branch: &str,
    salt_ts: u64,
  ) -> Result<GlobalReply> {
    let normalized = normalize_branch_name(branch).ok_or(KvError::InvalidArgs)?;
    let salt_val = if salt_ts == 0 {
      branch_salt(now_us())
    } else {
      salt_ts as u32
    };
    let internal_name = format!("{normalized}-{salt_val:08x}");
    let table_li = self.enumerate_target_li().await?;
    let (mut reply, _) = self
      .fanout()
      .dispatch_collect(&table_li, |tbl| Request::CreateBranch {
        tbl: tbl.clone(),
        branch: internal_name.clone(),
      })
      .await;
    reply.name = internal_name;
    Ok(reply)
  }

  /// 全局删表：只删 `tbl_name` 名下全部分区（逐分区 `Drop`），对齐 C++
  /// `DropTableRequest`（按表名收集分区后 fanout，绝不删其它表）。
  pub async fn global_drop_table(&self, tbl_name: &str) -> Result<GlobalReply> {
    if tbl_name.is_empty() {
      return Err(KvError::InvalidArgs);
    }
    let table_li: Vec<TableIdent> = self
      .enumerate_target_li()
      .await?
      .into_iter()
      .filter(|tbl| tbl.tbl_name.as_ref() == tbl_name)
      .collect();
    Ok(
      self
        .fanout()
        .dispatch(&table_li, |tbl| Request::Drop { tbl: tbl.clone() })
        .await,
    )
  }

  /// 全局列举归档标签（控制面读）：扫 `root_li` 下全部分区目录里带 tag 的
  /// manifest，`prefix` 非空时按 tag 前缀过滤，按 `(tag, term)` 升序并去重。
  ///
  /// 对齐 C++ `HandleGlobalListArchiveTagsRequest`：**不经 shard、不 fanout**，
  /// 所以未 attach 的只读分区也能列出；错误面只有「读 `root` 失败」以外的场景
  /// 一律跳过（控制面列举不因单个坏目录整体失败）。语义细节见 `list_archive_tag_li`。
  pub async fn global_list_archive_tag_li(&self, prefix: &str) -> Result<Vec<ArchiveEntry>> {
    Ok(list_archive_tag_li(&self.root_li, prefix))
  }

  /// [`Self::global_list_archive_tag_li`] 的同义方法（C++ 请求名
  /// `GlobalListArchiveTags` 不带 `_li` 后缀）。
  pub async fn global_list_archive_tags(&self, prefix: &str) -> Result<Vec<ArchiveEntry>> {
    self.global_list_archive_tag_li(prefix).await
  }

  /// 全局归档结果名：`manifest_<branch>_<term>_<tag>`（C++ `BranchArchiveName`）。
  ///
  /// `branch` / `term` 取**最低 shard_id 的已 attach 身份**；没有任何 shard
  /// attach 时回落 `(main, 0)` —— C++ `branch_` 默认 `MainBranchName`，
  /// `ShardTerm` 对未 attach 的 shard 返回 0。多 shard 身份不一致时取最低 id，
  /// 与 C++ `DefaultDsShard`（Start 时 attach 的 shard，否则最先 attach 的）
  /// 的确定性取向一致；正常用法下全部 shard 共用同一 `(branch, term)`。
  ///
  /// 已知近似：C++ `branch_` 在 `Start(branch)` 时固定、attach 不改它；
  /// fkv `Runtime::start` 没有 branch 参数，只能用 attach 身份，全部 detach 后
  /// 回到默认 `main`。控制面结果名的用途（回传 / 记账）不依赖这一差异。
  fn global_archive_name(&self, tag: &str) -> String {
    let ident = self.attach_ident();
    archive_file_name(&ident.branch, ident.term, tag)
  }

  /// 结果名使用的 attach 身份；未 attach 回落 `(main, 0)`。
  fn attach_ident(&self) -> AttachIdent {
    // 结果名必须反映真实托管状态：先把此前的超时登记复核掉。
    self.settle_timeout_views();
    lock_attach_ident_li(&self.attach_ident_li)
      .iter()
      .flatten()
      .next()
      .cloned()
      .unwrap_or_else(|| AttachIdent {
        branch: MAIN_BRANCH_NAME.to_owned(),
        term: 0,
      })
  }

  /// 记录 shard 的 attach 身份（`branch` 已归一化，term 与 shard 侧一致）。
  fn record_attach(&self, shard_id: u32, branch: &str, term: u64) {
    let mut ident_li = lock_attach_ident_li(&self.attach_ident_li);
    if let Some(slot) = ident_li.get_mut(shard_id as usize) {
      *slot = Some(AttachIdent {
        branch: branch.to_owned(),
        term,
      });
    }
  }

  fn shard(&self, tbl: &TableIdent) -> &Shard {
    let index = usize::from(tbl.shard_index(self.num_threads()));
    &self.shard_li[index]
  }

  fn shard_by_id(&self, shard_id: u32) -> Result<&Shard> {
    self
      .shard_li
      .get(shard_id as usize)
      .ok_or(KvError::InvalidArgs)
  }

  /// 把 shard 绑定到 `branch` / `term`，并落地本层视图（attach 身份 / 归档目标 /
  /// 预热路由）。
  ///
  /// 控制面串行：本方法与 [`Self::detach_shard`] 共用 per-shard 的
  /// `control_li[shard_id]` 锁，锁内按「代际 +1 → 发请求 → 落地 / 清除视图」
  /// 顺序执行，并发的 attach / detach 不会交错。锁序固定为
  /// `control_li` → `attach_epoch` 门 → 视图锁（`attach_ident_li` /
  /// `archive_state` / `prewarm_state`）；反向获取会死锁。数据面请求与
  /// [`Self::list_standby_partition`] 不取 `control_li`，只读代际快照做收尾。
  ///
  /// # Errors
  ///
  /// - [`KvError::InvalidArgs`]：`shard_id` 越界，或 `branch` 无法归一化。
  /// - [`KvError::ExpiredTerm`]：`term` 低于 shard 已见 term，shard 粘滞为
  ///   `Expired`，除 attach / detach / shutdown 外的请求全部快速失败。必须以
  ///   不低于当前 term 重放 attach（或先 detach）才能恢复。
  /// - [`KvError::Busy`]：视图被更晚的控制操作接管（防御分支，生产路径由
  ///   `control_li` 串行后不可达，见下方注释），或入队背压超时。可重试：
  ///   调用方重放本方法即可；重放期间不得假定 shard 已 attach。
  /// - [`KvError::NotRunning`] / [`KvError::Timeout`] / [`KvError::Corrupted`]：
  ///   shard 发送端 / 回执校验错误，按数据面语义处理。
  ///
  /// 成功返回表示 shard 已 `Attached` **且**本层视图已落地；`Busy` /
  /// `ExpiredTerm` 等错误返回后视图可能为空，后续请求按真实 lifecycle 得到
  /// `NotRunning` / `ExpiredTerm`，调用方不得视作 attach 成功。
  pub async fn attach_shard(&self, shard_id: u32, branch: &str, term: u64) -> Result<()> {
    // 归一化在这里做一次：shard 侧同样会归一化，但结果名由本层构造，
    // 必须记录与盘面 / term 文件一致的规范 branch（sto-69）。
    let branch = normalize_branch_name(branch).ok_or(KvError::InvalidArgs)?;
    info!(
      target: "fkv_runtime",
      "attach shard shard_id={} branch={} term={}",
      shard_id,
      branch,
      term
    );
    // 控制面 per-shard 串行：bump 代际、发请求与落地视图必须原子成组。
    // 否则并发的 attach / detach 会在两者之间推进代际，让已在 shard 生效的
    // attach 落到 `!recorded` 分支，留下「shard Attached、本层视图空」。
    let control = self
      .control_li
      .get(shard_id as usize)
      .ok_or(KvError::InvalidArgs)?;
    let _serial = control.lock().await;
    let route = self.control_route(shard_id)?;
    // 代际 +1：本次 attach 之前发出的请求即使迟到失败，也不得清掉本次视图。
    let epoch = route.bump_epoch();
    // 失败收尾由 `ShardRoute::finish` 完成：`ExpiredTerm` 已置 `Expired`、本地
    // `Timeout` 时请求也可能已执行并置 `Expired`，两种都按真实 lifecycle 清视图；
    // `Busy` 等未改变状态时 lifecycle 仍为 `Attached`，视图保留等重试。
    match route
      .request(Request::AttachShard {
        branch: branch.to_string(),
        shard_id,
        term,
      })
      .await?
    {
      Response::AttachShard => {
        // 代际校验 + 推进 + 记录在同一个临界区内：并发的 detach 已经推进代际时，
        // 本次视图不得落地（否则会覆盖更晚 detach 的清除或反之）。
        let recorded = route.record_if_current(epoch, || {
          self.record_attach(shard_id, &branch, term);
          if let Some(state) = &self.archive_state {
            state.attached_li[shard_id as usize].store(true, Ordering::Relaxed);
          }
          if let Some(state) = &self.prewarm_state {
            lock_prewarm_state(state).attach(shard_id, &branch, term);
          }
        });
        if !recorded {
          warn!(
            target: "fkv_runtime",
            "attach superseded by newer control op shard_id={} branch={} term={}",
            shard_id,
            branch,
            term
          );
          // 防御分支：生产路径不可达 —— `control_li[shard_id]` 已把代际推进与
          // 视图落地串行在同一临界区，触发条件只有「在 `control_li` 之外推进
          // 代际」的控制操作（未来新增的快路径，或测试手工构造）。
          //
          // 两种终态都回 `Busy`（可重试）：shard 仍 `Attached` 说明视图归更晚
          // 操作所有；shard 已 `Detached` 说明本次 attach 事实上没生效。返
          // `Ok(())` 会谎报成功，调用方后续请求全部 `NotRunning`。
          return Err(KvError::Busy);
        }
        if let Some(crond) = &self.prewarm_crond {
          crond.signal();
        }
        Ok(())
      }
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn detach_shard(&self, shard_id: u32) -> Result<()> {
    info!(
      target: "fkv_runtime",
      "detach shard shard_id={}",
      shard_id
    );
    // 与 attach 共用同一把 per-shard 控制面锁，保证代际推进与视图落地有序。
    let control = self
      .control_li
      .get(shard_id as usize)
      .ok_or(KvError::InvalidArgs)?;
    let _serial = control.lock().await;
    let route = self.control_route(shard_id)?;
    // 代际 +1：在途 attach 的迟到失败不得清掉 detach 之后的状态。
    let epoch = route.bump_epoch();
    match route.request(Request::DetachShard { shard_id }).await? {
      Response::DetachShard => {
        // 只有本次 detach 之后没有更晚的 attach / detach 才清视图：并发的 attach
        // 可能已经记下新视图，无条件清会把它抹掉。
        route.clear_if_current(epoch);
        Ok(())
      }
      _ => Err(KvError::Corrupted),
    }
  }

  /// 数据面请求：套用 [`Runtime::with_request_timeout`] /
  /// [`Runtime::with_queue_timeout`] 的默认值；未配置时与旧行为一致。
  ///
  /// 控制面请求（`AttachShard` / `DetachShard` / `Shutdown`）拒绝：它们按 shard
  /// 而非按 `TableIdent` 定位，走这里会绕过 attach 身份记录与视图收尾，并按
  /// 调用方的 `tbl` 清错 shard。请用 [`Runtime::attach_shard`] /
  /// [`Runtime::detach_shard`] / [`Runtime::shutdown`]。
  pub async fn request(&self, tbl: &TableIdent, req: Request) -> Result<Response> {
    reject_control_plane(&req)?;
    self.route(tbl).request(req).await
  }

  /// 显式 deadline 的请求，覆盖运行时默认值。
  pub async fn request_deadline(
    &self,
    tbl: &TableIdent,
    req: Request,
    timeout: Duration,
  ) -> Result<Response> {
    reject_control_plane(&req)?;
    self.route(tbl).request_deadline(req, timeout).await
  }

  /// 只设服务端截止时刻的请求（本端不兜底）；过期由 shard 回 `Timeout`。
  pub async fn request_at(
    &self,
    tbl: &TableIdent,
    req: Request,
    deadline: Instant,
  ) -> Result<Response> {
    reject_control_plane(&req)?;
    self.route(tbl).request_at(req, deadline).await
  }

  pub async fn put(&self, tbl: &TableIdent, batch_li: Vec<fkv_btree::BatchEntry>) -> Result<()> {
    match self
      .request(
        tbl,
        Request::Put {
          tbl: tbl.clone(),
          batch_li,
          term: None,
        },
      )
      .await?
    {
      Response::Put => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn put_with_term(
    &self,
    tbl: &TableIdent,
    batch_li: Vec<fkv_btree::BatchEntry>,
    term: u64,
  ) -> Result<()> {
    match self
      .request(
        tbl,
        Request::Put {
          tbl: tbl.clone(),
          batch_li,
          term: Some(term),
        },
      )
      .await?
    {
      Response::Put => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn put_large(
    &self,
    tbl: &TableIdent,
    batch_li: Vec<fkv_btree::BatchEntry>,
  ) -> Result<()> {
    match self
      .request(
        tbl,
        Request::PutLarge {
          tbl: tbl.clone(),
          batch_li,
          term: None,
        },
      )
      .await?
    {
      Response::Put => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn get_large(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<LargeValueReply>> {
    match self
      .request(
        tbl,
        Request::GetLarge {
          tbl: tbl.clone(),
          key: key.to_vec(),
        },
      )
      .await?
    {
      Response::GetLarge(value) => Ok(value),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn read(&self, tbl: &TableIdent, key: &[u8]) -> Result<Vec<u8>> {
    self.get(tbl, key).await?.ok_or(KvError::NotFound)
  }

  pub async fn floor(&self, tbl: &TableIdent, key: &[u8]) -> Result<FloorReply> {
    match self
      .request(
        tbl,
        Request::Floor {
          tbl: tbl.clone(),
          key: key.to_vec(),
        },
      )
      .await?
    {
      Response::Floor(reply) => Ok(reply),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn get(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<Vec<u8>>> {
    match self
      .request(
        tbl,
        Request::Get {
          tbl: tbl.clone(),
          key: key.to_vec(),
        },
      )
      .await?
    {
      Response::Get(value) => Ok(value),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn scan(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    match self
      .request(
        tbl,
        Request::Scan {
          tbl: tbl.clone(),
          begin: begin.to_vec(),
          end: end.to_vec(),
          max_entries,
          option: ScanPageOption::default(),
        },
      )
      .await?
    {
      Response::Scan(reply) => Ok(reply.entry_li),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn scan_page(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    option: ScanPageOption,
  ) -> Result<ScanReply> {
    match self
      .request(
        tbl,
        Request::Scan {
          tbl: tbl.clone(),
          begin: begin.to_vec(),
          end: end.to_vec(),
          max_entries: 0,
          option,
        },
      )
      .await?
    {
      Response::Scan(reply) => Ok(reply),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn scan_opt(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    option: ScanPageOption,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    match self
      .request(
        tbl,
        Request::Scan {
          tbl: tbl.clone(),
          begin: begin.to_vec(),
          end: end.to_vec(),
          max_entries,
          option,
        },
      )
      .await?
    {
      Response::Scan(reply) => Ok(reply.entry_li),
      _ => Err(KvError::Corrupted),
    }
  }

  /// 显式重新打开分区。Cloud + `prewarm_cloud_cache` 下成功后按表投递后台预热，
  /// 不阻塞本回执（对齐 C++ `ReopenTask::Reopen` → `PrewarmService::Prewarm`）。
  pub async fn reopen(&self, tbl: &TableIdent) -> Result<()> {
    match self
      .request(tbl, Request::Reopen { tbl: tbl.clone() })
      .await?
    {
      Response::Reopen => {
        self.signal_table_prewarm(tbl);
        Ok(())
      }
      _ => Err(KvError::Corrupted),
    }
  }

  /// 采纳外部（远端）当前快照：Cloud 强制回源 manifest，StandbyReplica 先追赶
  /// master，再由 shard 的 `install_external_snapshot` 安装。仅 Cloud /
  /// StandbyReplica 可用，其余模式回 `InvalidArgs`。
  ///
  /// 真正安装（`Installed`）后与 [`Self::reopen`] 一样投递后台 per-table 预热；
  /// `WaitingRemote` 没有可预热的远端分区，不投递。
  pub async fn reopen_external(&self, tbl: &TableIdent) -> Result<InstallOutcome> {
    match self
      .request(tbl, Request::ReopenExternal { tbl: tbl.clone() })
      .await?
    {
      Response::ReopenExternal(outcome) => {
        if matches!(outcome, InstallOutcome::Installed) {
          self.signal_table_prewarm(tbl);
        }
        Ok(outcome)
      }
      _ => Err(KvError::Corrupted),
    }
  }

  /// 文件 GC：Cloud 删远端对象（并清本地 cache），Local / Mem 删本地文件。
  /// 对齐 C++ `FileGcRequest`；布局位宽由 shard 配置决定，调用方不需要传。
  pub async fn file_gc(&self, tbl: &TableIdent) -> Result<usize> {
    match self
      .request(tbl, Request::FileGc { tbl: tbl.clone() })
      .await?
    {
      Response::FileGc(count) => Ok(count),
      _ => Err(KvError::Corrupted),
    }
  }

  /// 本地 GC：只删本地文件 / 本地 cache，绝不删远端对象。
  /// 对齐 C++ `LocalGcRequest`。
  pub async fn local_gc(&self, tbl: &TableIdent) -> Result<usize> {
    match self
      .request(tbl, Request::LocalGc { tbl: tbl.clone() })
      .await?
    {
      Response::LocalGc(count) => Ok(count),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn compact(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    pages_per_file_shift: u8,
  ) -> Result<usize> {
    match self
      .request(
        tbl,
        Request::Compact {
          tbl: tbl.clone(),
          file_id,
          pages_per_file_shift,
        },
      )
      .await?
    {
      Response::Compact(count) => Ok(count),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn truncate(&self, tbl: &TableIdent) -> Result<()> {
    self.truncate_from(tbl, b"").await
  }

  pub async fn truncate_from(&self, tbl: &TableIdent, position: &[u8]) -> Result<()> {
    match self
      .request(
        tbl,
        Request::Truncate {
          tbl: tbl.clone(),
          position: position.to_vec(),
        },
      )
      .await?
    {
      Response::Truncate => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  /// 触发该表 TTL 树的物理清理（对齐 C++ `CleanExpired` 内部请求）。
  pub async fn clean_expired(&self, tbl: &TableIdent) -> Result<CleanExpiredReply> {
    match self
      .request(tbl, Request::CleanExpired { tbl: tbl.clone() })
      .await?
    {
      Response::CleanExpired(reply) => Ok(reply),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn drop_table(&self, tbl: &TableIdent) -> Result<()> {
    match self
      .request(tbl, Request::Drop { tbl: tbl.clone() })
      .await?
    {
      Response::Drop => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn create_archive(
    &self,
    tbl: &TableIdent,
    tag: &str,
    num_retained: u32,
  ) -> Result<ArchiveReply> {
    match self
      .request(
        tbl,
        Request::CreateArchive {
          tbl: tbl.clone(),
          tag: tag.to_owned(),
          num_retained,
        },
      )
      .await?
    {
      Response::CreateArchive(reply) => Ok(reply),
      _ => Err(KvError::Corrupted),
    }
  }

  /// 按 tag 重新打开归档快照（`tag` 为空 = 重开活跃快照）。
  pub async fn reopen_archive(&self, tbl: &TableIdent, tag: &str) -> Result<()> {
    match self
      .request(
        tbl,
        Request::ReopenArchive {
          tbl: tbl.clone(),
          tag: tag.to_owned(),
        },
      )
      .await?
    {
      Response::Reopen => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  /// 删除该分区当前分支下指定 tag 的归档 manifest，返回删除条数。
  pub async fn delete_archive(&self, tbl: &TableIdent, tag: &str) -> Result<usize> {
    match self
      .request(
        tbl,
        Request::DeleteArchive {
          tbl: tbl.clone(),
          tag: tag.to_owned(),
        },
      )
      .await?
    {
      Response::DeleteArchive(removed) => Ok(removed),
      _ => Err(KvError::Corrupted),
    }
  }

  /// Cloud 模式：同步预热一个分区（把远端 manifest / data / segment 拉到本地
  /// cache），完成后返回。仅 `Backend::Cloud` 可用；Local / Mem / Standby 返回
  /// `InvalidArgs`，不静默降级为成功。
  ///
  /// 后台周期预热仍由 `PrewarmCrond` 负责；本方法给调用方一个「按分区、可等待」
  /// 的显式入口。统计明细（列出 / 拉取 / 失败数、结束原因）可通过
  /// [`Self::prewarm_report`] 观察最近一轮后台结果。
  pub async fn prewarm(&self, tbl: &TableIdent) -> Result<()> {
    match self
      .request(tbl, Request::Prewarm { tbl: tbl.clone() })
      .await?
    {
      Response::Prewarm => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  /// 列举该表的归档 `(tag, term)` 清单（全部 branch / term，按 `(tag, term)`
  /// 升序去重，对齐 C++ `GlobalListArchiveTagsRequest` 的排序与去重规则）。
  pub async fn list_archive_tags(&self, tbl: &TableIdent) -> Result<Vec<ArchiveTagInfo>> {
    match self
      .request(tbl, Request::ListArchiveTags { tbl: tbl.clone() })
      .await?
    {
      Response::ListArchiveTags(tag_li) => Ok(tag_li),
      _ => Err(KvError::Corrupted),
    }
  }

  /// Cloud 模式：分页列举远端对象。
  ///
  /// `prefix` 是**远端 key 的原始前缀**（对齐 C++ `ListObjectRequest::SetRemotePath`），
  /// 不做表目录自动拼接；例如要列 `demo` 分区 1 的全部对象，传
  /// `<cloud prefix>/demo.1/`。`tbl` 只用于选择服务 shard。
  /// 非 Cloud 模式返回 `InvalidArgs`。
  pub async fn list_object(
    &self,
    tbl: &TableIdent,
    prefix: &str,
    token: Option<&str>,
    max_keys: usize,
  ) -> Result<ListPage> {
    match self
      .request(
        tbl,
        Request::ListObject {
          tbl: tbl.clone(),
          prefix: prefix.to_owned(),
          token: token.map(str::to_owned),
          max_keys,
        },
      )
      .await?
    {
      Response::ListObject(page) => Ok(page),
      _ => Err(KvError::Corrupted),
    }
  }

  /// 列举 standby master 上已有的分区（对齐 C++ `ListStandbyPartitionRequest`）。
  ///
  /// 非 standby（没有 master 来源）返回 `InvalidArgs`。当前实现固定发往 shard 0：
  /// 各 shard 的 standby service 共享同一份 master 来源，逐 shard 列举只会得到
  /// 相同结果；多 DS 托管与 `partition_filter` 已按范围外关闭（sto-61b），
  /// 不在此实现 per-ds_shard 过滤。
  ///
  /// 走观测路由（[`Self::route_of_shard`]）：shard 侧同步执行 `ssh` / `ls`，
  /// master 卡死时必须有 deadline 兜底，超时回 [`KvError::Timeout`]。deadline 取
  /// [`Self::with_request_timeout`] 的配置，未配置时用
  /// [`DEFAULT_OBSERVE_TIMEOUT`]。该请求不改变 attach 状态，也不走
  /// [`Self::reject_control_plane`]，因此「控制面观察请求不拒绝」的语义不变。
  pub async fn list_standby_partition(&self) -> Result<Vec<TableIdent>> {
    let route = self.route_of_shard(self.shard_by_id(0)?);
    let timeout = self.request_timeout.unwrap_or(DEFAULT_OBSERVE_TIMEOUT);
    match route
      .request_deadline(Request::ListStandbyPartition, timeout)
      .await?
    {
      Response::ListStandbyPartition(tbl_li) => Ok(tbl_li),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn create_branch(&self, tbl: &TableIdent, branch: &str) -> Result<BranchReply> {
    match self
      .request(
        tbl,
        Request::CreateBranch {
          tbl: tbl.clone(),
          branch: branch.to_owned(),
        },
      )
      .await?
    {
      Response::CreateBranch(reply) => Ok(reply),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn delete_branch(&self, tbl: &TableIdent, branch: &str) -> Result<usize> {
    match self
      .request(
        tbl,
        Request::DeleteBranch {
          tbl: tbl.clone(),
          branch: branch.to_owned(),
        },
      )
      .await?
    {
      Response::DeleteBranch(removed) => Ok(removed),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn shutdown(mut self) -> Result<()> {
    info!(
      target: "fkv_runtime",
      "runtime shutdown begin threads={}",
      self.shard_li.len()
    );
    if let Some(mut crond) = self.archive_crond.take() {
      crond.stop();
      info!(target: "fkv_runtime", "archive crond stopped");
    }
    if let Some(mut crond) = self.prewarm_crond.take() {
      crond.stop();
      info!(target: "fkv_runtime", "prewarm crond stopped");
    }
    // 逐个 shutdown 全部 shard 后再统一返回首个错误：首错即 `?` 会让余下 shard
    // 落到 `Drop`（只告警不 join），丢失确定性清理时机。
    let mut first_err = None;
    for shard in take(&mut self.shard_li) {
      if let Err(err) = shard.shutdown().await
        && first_err.is_none()
      {
        first_err = Some(err);
      }
    }
    if let Some(err) = first_err {
      error!(target: "fkv_runtime", "runtime shutdown failed error={err}");
      return Err(err);
    }
    info!(target: "fkv_runtime", "runtime shutdown complete");
    Ok(())
  }
}

impl Drop for Runtime {
  fn drop(&mut self) {
    if let Some(mut crond) = self.archive_crond.take() {
      crond.stop();
    }
    if let Some(mut crond) = self.prewarm_crond.take() {
      crond.stop();
    }
  }
}

#[cfg(test)]
mod prewarm_tests {
  use bytes::Bytes;
  use fkv_btree::BatchEntry;
  use fkv_io_mem::MemStore;
  use fkv_object::MemObjectStore;
  use fkv_store::Store;

  use super::*;

  fn cloud(local: MemStore, remote: MemObjectStore) -> CloudStore<MemStore, MemObjectStore> {
    CloudStore::new(
      local,
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
  }

  /// 轮询共享状态直到条件成立；超时 panic（避免测试永久挂住）。
  fn wait_until(state: &Arc<Mutex<PrewarmState>>, cond: impl Fn(&PrewarmState) -> bool) {
    for _ in 0..500 {
      if cond(&lock_prewarm_state(state)) {
        return;
      }
      thread::sleep(Duration::from_millis(10));
    }
    panic!("prewarm condition not reached in time");
  }

  /// reopen 投递的 per-table 目标：不受 `done_li` 限制、重复去重、按 shard 合并进
  /// attach 轮；未 attach 的 shard 目标直接丢弃（等 attach 后的整轮覆盖）。
  #[test]
  fn pending_table_targets_ignore_done_and_dedupe() {
    let mut state = PrewarmState::new(1);
    state.attach(0, MAIN_BRANCH_NAME, 3);
    state.done_li[0] = true;
    let tbl = TableIdent::new("late", 1).unwrap();
    state.enqueue_table(tbl.clone());
    state.enqueue_table(tbl.clone());
    assert_eq!(state.pending_tbl_li.len(), 1, "同表应去重");

    let target_li = plan_targets(state.take_plan_input(), Vec::new());
    assert_eq!(target_li.len(), 1, "{target_li:?}");
    assert_eq!(target_li[0].shard_id, 0);
    assert_eq!(target_li[0].term, 3);
    assert_eq!(target_li[0].table_li, vec![tbl]);
    assert!(state.pending_tbl_li.is_empty(), "pending 应被消费");

    // 未 attach：目标丢弃，不留在队列里等下一轮（attach 轮会覆盖全部分区）。
    state.detach(0);
    state.enqueue_table(TableIdent::new("lost", 1).unwrap());
    assert!(plan_targets(state.take_plan_input(), Vec::new()).is_empty());
    assert!(state.pending_tbl_li.is_empty());
  }

  /// pending 上限：超限丢最早，去重索引与队列同步；plan 与 attach 轮合并时
  /// 同表只出现一次（去重从线性 `contains` 换成桶内 `sort` + `dedup` 后行为不变）。
  #[test]
  fn pending_table_targets_evict_oldest_and_keep_index() {
    let mut state = PrewarmState::new(1);
    state.attach(0, MAIN_BRANCH_NAME, 1);
    let first = TableIdent::new("evict", 0).unwrap();
    state.enqueue_table(first.clone());
    for part in 0..=MAX_PENDING_PREWARM_TABLES as u32 {
      state.enqueue_table(TableIdent::new("evict", part).unwrap());
    }
    assert_eq!(state.pending_tbl_li.len(), MAX_PENDING_PREWARM_TABLES);
    assert_eq!(state.pending_set.len(), state.pending_tbl_li.len());
    assert!(!state.pending_set.contains(&first), "超限必须丢最早的目标");

    // plan 的 attach 列表包含全部 pending 表：合并后不得重复。
    let table_li: Vec<TableIdent> = state.pending_tbl_li.iter().cloned().collect();
    let target_li = plan_targets(state.take_plan_input(), table_li);
    let total: usize = target_li.iter().map(|target| target.table_li.len()).sum();
    assert_eq!(total, MAX_PENDING_PREWARM_TABLES);
    assert!(state.pending_set.is_empty());
    assert!(state.pending_tbl_li.is_empty());
  }

  /// detach 清掉该 shard 的 pending：重 attach 换了身份后，旧目标不会再按新
  /// `(branch, term)` 预热；其他 shard 的 pending 不受影响。
  #[test]
  fn detach_drops_pending_of_shard() {
    let mut state = PrewarmState::new(2);
    state.attach(0, MAIN_BRANCH_NAME, 1);
    state.attach(1, MAIN_BRANCH_NAME, 1);
    let tbl0 = TableIdent::new("drop", 2).unwrap();
    let tbl1 = TableIdent::new("keep", 1).unwrap();
    assert_eq!(tbl0.shard_index(2), 0);
    assert_eq!(tbl1.shard_index(2), 1);
    state.enqueue_table(tbl0);
    state.enqueue_table(tbl1.clone());
    state.detach(0);
    assert_eq!(state.pending_tbl_li, VecDeque::from([tbl1.clone()]));
    assert_eq!(state.pending_set.len(), 1, "去重索引必须与队列同步");
    assert!(state.pending_set.contains(&tbl1));

    state.attach(0, "next", 2);
    let target_li = plan_targets(state.take_plan_input(), Vec::new());
    assert_eq!(target_li.len(), 1, "{target_li:?}");
    assert_eq!(target_li[0].shard_id, 1);
  }

  /// 可切换失败的远端：`fail` 置位时所有 `get` / `head` 返回 `IoFail`，
  /// 列举不受影响，用来制造单 shard 预热失败。
  #[derive(Debug, Clone)]
  struct FlakyObjectStore {
    inner: MemObjectStore,
    fail: Arc<AtomicBool>,
  }

  impl ObjectStore for FlakyObjectStore {
    async fn get(&self, key: &str) -> Result<Option<Bytes>> {
      if self.fail.load(Ordering::Relaxed) {
        return Err(KvError::IoFail);
      }
      self.inner.get(key).await
    }

    async fn head(&self, key: &str) -> Result<Option<fkv_object::ObjectMeta>> {
      if self.fail.load(Ordering::Relaxed) {
        return Err(KvError::IoFail);
      }
      self.inner.head(key).await
    }

    async fn put(
      &self,
      key: &str,
      value: Bytes,
      if_match: Option<&str>,
      if_none_match: bool,
    ) -> Result<String> {
      self.inner.put(key, value, if_match, if_none_match).await
    }

    async fn delete(&self, key: &str) -> Result<()> {
      self.inner.delete(key).await
    }

    async fn list(&self, prefix: &str) -> Result<Vec<fkv_object::ObjectMeta>> {
      self.inner.list(prefix).await
    }

    async fn list_page(
      &self,
      prefix: &str,
      token: Option<&str>,
      max_keys: usize,
    ) -> Result<fkv_object::ListPage> {
      self.inner.list_page(prefix, token, max_keys).await
    }
  }

  /// 只让 data 文件下载失败的远端：manifest 列举与下载正常，`prewarm_table` 以
  /// `CompletionReason::DownloadError` 收尾但仍返回 `Ok`。
  #[derive(Debug)]
  struct DataFailStore {
    inner: MemObjectStore,
    fail: Arc<AtomicBool>,
  }

  fn is_data_key(key: &str) -> bool {
    let name = key.rsplit('/').next().unwrap_or(key);
    name
      .strip_prefix(fkv_types::FILE_NAME_DATA)
      .is_some_and(|rest| rest.starts_with('_'))
  }

  impl ObjectStore for DataFailStore {
    async fn get(&self, key: &str) -> Result<Option<Bytes>> {
      if self.fail.load(Ordering::Relaxed) && is_data_key(key) {
        return Err(KvError::IoFail);
      }
      self.inner.get(key).await
    }

    async fn head(&self, key: &str) -> Result<Option<fkv_object::ObjectMeta>> {
      self.inner.head(key).await
    }

    async fn put(
      &self,
      key: &str,
      value: Bytes,
      if_match: Option<&str>,
      if_none_match: bool,
    ) -> Result<String> {
      self.inner.put(key, value, if_match, if_none_match).await
    }

    async fn delete(&self, key: &str) -> Result<()> {
      self.inner.delete(key).await
    }

    async fn list(&self, prefix: &str) -> Result<Vec<fkv_object::ObjectMeta>> {
      self.inner.list(prefix).await
    }

    async fn list_page(
      &self,
      prefix: &str,
      token: Option<&str>,
      max_keys: usize,
    ) -> Result<fkv_object::ListPage> {
      self.inner.list_page(prefix, token, max_keys).await
    }
  }

  /// 单表下载失败（`Ok` + `DownloadError`）不得让整个 shard 置 `done_li`：失败表
  /// 必须回队，否则永久不再重试。回退「只要 `Ok` 就置 `done_li`」时本用例失败。
  #[test]
  fn partial_download_failure_requeues_table() {
    pollster::block_on(async {
      let remote = MemObjectStore::new();
      let tbl = TableIdent::new("partial", 1).unwrap();
      {
        let mut store = Store::new(cloud(MemStore::new(512), remote.clone()), 512, 16);
        store
          .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
          .await
          .unwrap();
      }
      let fail = Arc::new(AtomicBool::new(true));
      let state = Arc::new(Mutex::new(PrewarmState::new(1)));
      let crond = PrewarmCrond::start(
        CloudStore::new(
          MemStore::new(512),
          DataFailStore {
            inner: remote.clone(),
            fail: fail.clone(),
          },
          "fkv",
          fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
        )
        .with_segment_config(4096, 1),
        state.clone(),
        1,
      )
      .unwrap();
      lock_prewarm_state(&state).attach(0, MAIN_BRANCH_NAME, 0);
      crond.signal();
      wait_until(&state, |state| {
        state.last_report.is_some_and(|report| report.table_num > 0)
      });
      {
        let state0 = lock_prewarm_state(&state);
        let report = state0.last_report.unwrap();
        assert!(
          report.listing_failed,
          "部分失败必须体现在回执里: {report:?}"
        );
        assert!(report.files_failed > 0, "{report:?}");
        assert!(!state0.done_li[0], "部分失败不能标记已预热");
        assert!(
          state0.pending_tbl_li.contains(&tbl),
          "失败表必须回队: {:?}",
          state0.pending_tbl_li
        );
      }

      // 远端恢复：下一轮重试成功，`done_li` 置位、pending 清空。
      fail.store(false, Ordering::Relaxed);
      lock_prewarm_state(&state).last_report = None;
      crond.signal();
      wait_until(&state, |state| {
        state
          .last_report
          .is_some_and(|report| report.table_num > 0 && !report.listing_failed)
      });
      let state0 = lock_prewarm_state(&state);
      assert!(state0.done_li[0], "全部成功后必须置 done_li");
      assert!(
        state0.pending_tbl_li.is_empty(),
        "{:?}",
        state0.pending_tbl_li
      );
    });
  }

  /// 回队后的自唤醒：失败表在没有外部 `signal()` 的情况下也必须被重试。
  ///
  /// 回退「`tick` 返回是否需重试 + `run` 带退避 `recv_timeout`」时，crond 只在
  /// 下一次 attach / reopen 才复活，本用例等不到 `done_li`。
  #[test]
  fn failed_table_retries_without_external_signal() {
    pollster::block_on(async {
      let remote = MemObjectStore::new();
      let tbl = TableIdent::new("auto_retry", 1).unwrap();
      {
        let mut store = Store::new(cloud(MemStore::new(512), remote.clone()), 512, 16);
        store
          .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
          .await
          .unwrap();
      }
      let fail = Arc::new(AtomicBool::new(true));
      let state = Arc::new(Mutex::new(PrewarmState::new(1)));
      let crond = PrewarmCrond::start(
        CloudStore::new(
          MemStore::new(512),
          DataFailStore {
            inner: remote.clone(),
            fail: fail.clone(),
          },
          "fkv",
          fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
        )
        .with_segment_config(4096, 1),
        state.clone(),
        1,
      )
      .unwrap();
      lock_prewarm_state(&state).attach(0, MAIN_BRANCH_NAME, 0);
      // 唯一一次外部唤醒：之后全靠 crond 自唤醒。
      crond.signal();
      wait_until(&state, |state| {
        state
          .last_report
          .is_some_and(|report| report.files_failed > 0)
      });

      // 远端恢复后不再 signal：必须由回队触发的重试把表拉完。
      fail.store(false, Ordering::Relaxed);
      wait_until(&state, |state| {
        state.done_li[0]
          && state
            .last_report
            .is_some_and(|report| report.table_num > 0 && !report.listing_failed)
      });
      assert!(
        lock_prewarm_state(&state).pending_tbl_li.is_empty(),
        "重试成功后队列必须清空"
      );
    });
  }

  /// 单 shard 预热失败：回执必须可见（不能被本轮列举成功覆盖），且失败分区
  /// 回队；远端恢复后下一轮信号即重试成功。
  #[test]
  fn shard_failure_stays_visible_and_retries() {
    pollster::block_on(async {
      let remote = MemObjectStore::new();
      let tbl = TableIdent::new("retry", 1).unwrap();
      {
        let mut store = Store::new(cloud(MemStore::new(512), remote.clone()), 512, 16);
        store
          .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
          .await
          .unwrap();
      }
      let fail = Arc::new(AtomicBool::new(true));
      let state = Arc::new(Mutex::new(PrewarmState::new(1)));
      let crond = PrewarmCrond::start(
        CloudStore::new(
          MemStore::new(512),
          FlakyObjectStore {
            inner: remote.clone(),
            fail: fail.clone(),
          },
          "fkv",
          fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
        )
        .with_segment_config(4096, 1),
        state.clone(),
        1,
      )
      .unwrap();
      lock_prewarm_state(&state).attach(0, MAIN_BRANCH_NAME, 0);
      crond.signal();
      wait_until(&state, |state| state.last_report.is_some());
      {
        let state0 = lock_prewarm_state(&state);
        let report = state0.last_report.unwrap();
        assert!(
          report.listing_failed,
          "单 shard 失败必须体现在回执里: {report:?}"
        );
        assert_eq!(report.table_num, 0, "{report:?}");
        assert!(!state0.done_li[0], "失败的 shard 不能标记已预热");
        assert_eq!(state0.pending_tbl_li.len(), 1, "失败分区必须回队重试");
      }

      // 远端恢复：同一轮信号重试成功，pending 清空、done_li 置位。
      fail.store(false, Ordering::Relaxed);
      lock_prewarm_state(&state).last_report = None;
      crond.signal();
      wait_until(&state, |state| state.done_li[0]);
      let report = lock_prewarm_state(&state).last_report.unwrap();
      assert!(!report.listing_failed, "{report:?}");
      assert!(report.pulled_file_num >= 2, "{report:?}");
      assert!(lock_prewarm_state(&state).pending_tbl_li.is_empty());
    });
  }

  /// attach 轮已完成（`done_li`）后，reopen 投递的新分区仍会被 crond 预热。
  #[test]
  fn prewarm_crond_consumes_reopen_table_after_attach_done() {
    pollster::block_on(async {
      let remote = MemObjectStore::new();
      let first_tbl = TableIdent::new("first", 1).unwrap();
      {
        let mut store = Store::new(cloud(MemStore::new(512), remote.clone()), 512, 16);
        store
          .put(
            &first_tbl,
            &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
          )
          .await
          .unwrap();
      }

      let state = Arc::new(Mutex::new(PrewarmState::new(1)));
      let crond =
        PrewarmCrond::start(cloud(MemStore::new(512), remote.clone()), state.clone(), 1).unwrap();
      lock_prewarm_state(&state).attach(0, MAIN_BRANCH_NAME, 0);
      crond.signal();
      wait_until(&state, |state| state.done_li[0]);

      // attach 之后远端才出现新分区（另一个 writer 推送）。
      let late_tbl = TableIdent::new("late", 1).unwrap();
      {
        let mut store = Store::new(cloud(MemStore::new(512), remote.clone()), 512, 16);
        store
          .put(
            &late_tbl,
            &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
          )
          .await
          .unwrap();
      }
      lock_prewarm_state(&state).last_report = None;
      lock_prewarm_state(&state).enqueue_table(late_tbl);
      crond.signal();
      // attach 轮已 done，本轮只应包含 reopen 投递的这张表。
      wait_until(&state, |state| {
        state
          .last_report
          .is_some_and(|report| report.table_num == 1 && report.pulled_file_num >= 2)
      });
      let report = lock_prewarm_state(&state).last_report.unwrap();
      assert!(!report.listing_failed, "{report:?}");
      assert!(
        lock_prewarm_state(&state).done_li[0],
        "attach 轮完成标记不能被覆盖"
      );
    });
  }

  /// 纯路由：按 `shard_index` 分组，未 attach 的 shard 本轮跳过，已预热的 shard 不重跑。
  #[test]
  fn plan_routes_by_shard_index_and_respects_attach() {
    // 2 shard：partition 1 → shard1，partition 2 → shard0。
    let mut state = PrewarmState::new(2);
    let tbl_shard1 = TableIdent::new("t", 1).unwrap();
    let tbl_shard0 = TableIdent::new("t", 2).unwrap();
    assert_eq!(tbl_shard1.shard_index(2), 1);
    assert_eq!(tbl_shard0.shard_index(2), 0);

    // 都没 attach：不产生目标。
    assert!(
      plan_targets(
        state.take_plan_input(),
        vec![tbl_shard1.clone(), tbl_shard0.clone()]
      )
      .is_empty()
    );

    // 只 attach shard0：只有 tbl_shard0 进目标，且带 attach 时的 (branch, term)。
    state.attach(0, MAIN_BRANCH_NAME, 7);
    let target_li = plan_targets(
      state.take_plan_input(),
      vec![tbl_shard1.clone(), tbl_shard0.clone()],
    );
    assert_eq!(target_li.len(), 1, "{target_li:?}");
    assert_eq!(target_li[0].shard_id, 0);
    assert_eq!(target_li[0].branch, MAIN_BRANCH_NAME);
    assert_eq!(target_li[0].term, 7);
    assert_eq!(target_li[0].table_li, vec![tbl_shard0.clone()]);

    // shard0 已预热：不再重复；此时 attach shard1 才轮到 tbl_shard1。
    state.done_li[0] = true;
    assert!(plan_targets(state.take_plan_input(), vec![tbl_shard0]).is_empty());
    state.attach(1, MAIN_BRANCH_NAME, 9);
    let target_li = plan_targets(state.take_plan_input(), vec![tbl_shard1.clone()]);
    assert_eq!(target_li.len(), 1);
    assert_eq!(target_li[0].shard_id, 1);
    assert_eq!(target_li[0].term, 9);

    // detach 后重新纳入下一轮（done_li 一并复位）。
    state.detach(1);
    state.done_li[1] = true;
    assert!(plan_targets(state.take_plan_input(), vec![tbl_shard1.clone()]).is_empty());
    state.attach(1, MAIN_BRANCH_NAME, 11);
    assert_eq!(
      plan_targets(state.take_plan_input(), vec![tbl_shard1]).len(),
      1
    );
  }

  /// crond 端到端：远端两个分区分属两个 shard，只 attach 的 shard 被预热；
  /// 第二个 shard attach 后补预，已预过的 shard 不重跑。
  #[test]
  fn prewarm_crond_fans_out_by_attached_shard() {
    pollster::block_on(async {
      let remote = MemObjectStore::new();
      let tbl_shard1 = TableIdent::new("a", 1).unwrap();
      let tbl_shard0 = TableIdent::new("b", 2).unwrap();
      {
        let mut store = Store::new(cloud(MemStore::new(512), remote.clone()), 512, 16);
        for tbl in [&tbl_shard1, &tbl_shard0] {
          store
            .put(tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
            .await
            .unwrap();
        }
      }

      let state = Arc::new(Mutex::new(PrewarmState::new(2)));
      let crond =
        PrewarmCrond::start(cloud(MemStore::new(512), remote.clone()), state.clone(), 2).unwrap();

      // 只 attach shard1（tbl_shard1 的归属）：只预一张表。
      lock_prewarm_state(&state).attach(1, MAIN_BRANCH_NAME, 0);
      crond.signal();
      wait_until(&state, |state| state.done_li[1]);
      let first = lock_prewarm_state(&state).last_report.unwrap();
      assert_eq!(first.table_num, 1, "{first:?}");
      assert!(first.pulled_file_num >= 2, "{first:?}");
      assert!(!first.listing_failed, "{first:?}");

      // shard0 attach 后补预 tbl_shard0；shard1 已 done_li 不再重跑。
      lock_prewarm_state(&state).last_report = None;
      lock_prewarm_state(&state).attach(0, MAIN_BRANCH_NAME, 0);
      crond.signal();
      wait_until(&state, |state| state.done_li[0]);
      let second = lock_prewarm_state(&state).last_report.unwrap();
      assert_eq!(second.table_num, 1, "{second:?}");
      assert!(second.pulled_file_num >= 2, "{second:?}");
    });
  }

  /// 远端对象名带写时的 branch / term：prewarm 按列举到的 (branch, term) 打开，
  /// 不因目标 shard attach 的 term 不同而漏掉旧/新 term 对象（对齐 C++ PrewarmFile）。
  #[test]
  fn prewarm_uses_listed_term_across_attach_identity() {
    pollster::block_on(async {
      let remote = MemObjectStore::new();
      let tbl = TableIdent::new("t", 1).unwrap();
      {
        // 以 term=7 写入远端。
        let mut store = Store::new(cloud(MemStore::new(512), remote.clone()), 512, 16);
        store.prepare_attach(MAIN_BRANCH_NAME, 7).unwrap();
        store
          .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
          .await
          .unwrap();
      }

      // attach 身份 term=7：能列举并拉取。
      let state = Arc::new(Mutex::new(PrewarmState::new(1)));
      lock_prewarm_state(&state).attach(0, MAIN_BRANCH_NAME, 7);
      let crond =
        PrewarmCrond::start(cloud(MemStore::new(512), remote.clone()), state.clone(), 1).unwrap();
      crond.signal();
      wait_until(&state, |state| state.last_report.is_some());
      let hit = lock_prewarm_state(&state).last_report.unwrap();
      assert!(hit.pulled_file_num >= 2, "{hit:?}");

      // attach 身份 term=0：仍按列举到的 term=7 对象拉取，不能因 identity 不同而 0 拉取。
      let state0 = Arc::new(Mutex::new(PrewarmState::new(1)));
      lock_prewarm_state(&state0).attach(0, MAIN_BRANCH_NAME, 0);
      let crond0 =
        PrewarmCrond::start(cloud(MemStore::new(512), remote), state0.clone(), 1).unwrap();
      crond0.signal();
      wait_until(&state0, |state| state.last_report.is_some());
      let miss = lock_prewarm_state(&state0).last_report.unwrap();
      assert!(miss.pulled_file_num >= 2, "{miss:?}");
      assert_eq!(miss.missing_num, 0, "{miss:?}");
    });
  }
}

#[cfg(test)]
mod global_tests {
  use std::{env::temp_dir, process::id};

  use super::*;

  fn temp_root(tag: &str) -> PathBuf {
    let root = temp_dir().join(format!("fkv_runtime_global_{tag}_{}", id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
  }

  fn config(root: PathBuf, threads: u16) -> RuntimeConfig {
    RuntimeConfig {
      num_threads: threads,
      root,
      page_size: 512,
      restart_interval: 16,
      pages_per_file_shift: fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      segment_size: 4096,
      segments_per_file_shift: 2,
      large_value_threshold: 1024,
      fixed_buffer_size: 0,
      auto_reopen_retry_times: 0,
      auto_reopen_pending_time_us: 0,
      archive_interval_secs: 0,
      archive_num_retained: 100,
      queue_size: 64,
    }
  }

  /// `write_limits` 是 KvOption → ShardConfig 的写 IO 投影：配置值原样透传。
  /// None 路径落到 `fkv_config` 默认常量（唯一来源），无需跨 crate 守卫。
  #[test]
  fn write_limits_project_config_overrides() {
    let option = KvOption {
      max_inflight_write: 7,
      max_write_batch_pages: 3,
      ..KvOption::default()
    };
    assert_eq!(write_limits(Some(&option)), (7, 3));
  }

  /// `backend_of`：Cloud 的共享预算组上限取 `local_space_limit`；非 S3 兼容
  /// provider 显式拒绝（不静默按 S3 起）。
  #[test]
  fn backend_of_maps_cloud_budget_and_rejects_other_providers() {
    let option = KvOption {
      cloud_store_path: "bucket/prefix".to_owned(),
      local_space_limit: 12345,
      allow_reuse_local_caches: true,
      ..KvOption::default()
    };
    match backend_of(Some(&option), Path::new("fkv-root")).unwrap() {
      Backend::Cloud(cloud) => {
        assert_eq!(cloud.bucket, "bucket");
        assert_eq!(cloud.prefix, "prefix");
        assert_eq!(cloud.cache_group.limit(), 12345);
        assert!(
          cloud.allow_reuse_local_caches,
          "KvOption::allow_reuse_local_caches 必须投影进 CloudBackend"
        );
      }
      other => panic!("expected cloud backend, got {other:?}"),
    }

    let bad = KvOption {
      cloud_store_path: "bucket/prefix".to_owned(),
      cloud_provider: "gcs".to_owned(),
      ..KvOption::default()
    };
    assert_eq!(
      backend_of(Some(&bad), Path::new("fkv-root")).unwrap_err(),
      KvError::InvalidArgs,
      "非 aws/s3 provider 必须拒绝"
    );
  }

  /// 归档 crond 的 Cloud 目标：运行时侧 `CloudStore` 能在**非 compio** 线程构造
  /// （它只做远端列举、不碰本地 cache），且与 shard 共用同一个预算组（12d）。
  #[test]
  fn cloud_store_builds_on_plain_thread_and_shares_budget_group() {
    let cfg = CloudStoreConfig {
      root: temp_root("cloud_store"),
      page_size: 512,
      pages_per_file_shift: fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      segment_size: 4096,
      segments_per_file_shift: 2,
      max_write_batch_pages: 64,
      max_inflight_write: 4096,
      fd_limit: 8,
      allow_reuse_local_caches: true,
    };
    let cloud = CloudBackend {
      bucket: "bucket".to_owned(),
      prefix: "prefix".to_owned(),
      endpoint: "http://127.0.0.1:1".to_owned(),
      region: "us-east-1".to_owned(),
      access_key: "k".to_owned(),
      secret_key: "s".to_owned(),
      allow_http: true,
      cache_group: Arc::new(CacheGroup::new(777)),
      allow_reuse_local_caches: true,
      max_cloud_concurrency: 32,
      s3_instance_num: 1,
      verify_ssl: true,
    };
    let store = open_runtime_cloud_store(&cfg, &cloud).unwrap();
    assert_eq!(store.cache_budget(), 777);
    assert!(
      store.reuse_local_caches(),
      "allow_reuse_local_caches 必须从装配参数透传到 CloudStore"
    );
    // 运行时侧实例必须与 shard 共用同一个预算组，不能各自一份上限。
    assert!(
      Arc::ptr_eq(store.cache_group(), &cloud.cache_group),
      "运行时侧 CloudStore 未共享 shard 的 CacheGroup"
    );
    // 装配自检：运行时侧（预热 / 全局列举 / 归档）不是分区写实例，必须只读，
    // 只有 shard 侧写实例允许启用表级 segment scheme。
    assert!(
      store.read_only(),
      "运行时侧 CloudStore 必须置 read_only(true)"
    );
    let _ = fs::remove_dir_all(cfg.root);
  }

  /// 全局 fanout：按 `shard_index` 覆盖全部已 attach 分区、按
  /// `max_global_request_batch` 分批（批大小 1 也全投），未 attach shard 的
  /// `NotRunning` 在回执里可见而不是静默吞掉。
  #[test]
  fn global_drop_table_fans_out_and_reports_not_running() {
    pollster::block_on(async {
      let root = temp_root("drop");
      let option = KvOption {
        max_global_request_batch: 1,
        ..KvOption::default()
      };
      let runtime = Runtime::start_with_option(config(root.clone(), 2), &option).unwrap();
      // 2 shard：partition 1 / 3 → shard1，partition 2 → shard0。
      for part in [1u32, 2, 3] {
        fs::create_dir_all(root.join(format!("demo.{part}"))).unwrap();
      }
      // 另一张表的分区不能被 `global_drop_table("demo")` 波及。
      fs::create_dir_all(root.join("other.1")).unwrap();
      runtime.attach_shard(0, "main", 1).await.unwrap();

      let reply = runtime.global_drop_table("demo").await.unwrap();
      assert_eq!(reply.total(), 3, "{reply:?}");
      assert_eq!(reply.ok_num, 1, "{reply:?}");
      assert_eq!(reply.failed_li.len(), 2, "{reply:?}");
      for (tbl, err) in &reply.failed_li {
        assert_eq!(tbl.tbl_name.as_ref(), "demo");
        assert_eq!(tbl.shard_index(2), 1, "{tbl}");
        assert_eq!(*err, KvError::NotRunning);
      }

      // detach 后连 shard0 也回 NotRunning：fanout 仍然全投（不提前退出）。
      runtime.detach_shard(0).await.unwrap();
      // 第一次 drop 可能清掉 shard0 上的空分区目录，重新造出来再验回执。
      for part in [1u32, 2, 3] {
        fs::create_dir_all(root.join(format!("demo.{part}"))).unwrap();
      }
      let reply = runtime.global_drop_table("demo").await.unwrap();
      assert_eq!(reply.total(), 3, "{reply:?}");
      assert_eq!(reply.ok_num, 0, "{reply:?}");

      // 按表名过滤：另一张表只剩它自己的 1 个分区。
      let reply = runtime.global_drop_table("other").await.unwrap();
      assert_eq!(reply.total(), 1, "{reply:?}");
      assert_eq!(reply.ok_num, 0, "{reply:?}");
      assert_eq!(reply.failed_li[0].0.tbl_name.as_ref(), "other");
      assert_eq!(
        runtime.global_drop_table("").await.unwrap_err(),
        KvError::InvalidArgs
      );

      runtime.shutdown().await.unwrap();
      let _ = fs::remove_dir_all(&root);
    });
  }

  /// 全局列举归档 tag：直扫盘面（**未 attach 也要列出来**）、跳过无 tag 的 manifest
  /// 与非 manifest 文件、按 `(tag, term)` 升序、全等 `(tag, term)` 跨分区去重
  /// （C++ `sort` + `unique`），不同 term 的同名 tag 各占一条。
  #[test]
  fn global_list_archive_tags_scans_disk_and_sorts() {
    pollster::block_on(async {
      let root = temp_root("list_archive_tag_li");
      let runtime =
        Runtime::start_with_option(config(root.clone(), 2), &KvOption::default()).unwrap();
      for part in [1u32, 2] {
        fs::create_dir_all(root.join(format!("demo.{part}"))).unwrap();
      }
      let write = |part: u32, name: &str| {
        fs::write(root.join(format!("demo.{part}")).join(name), b"x").unwrap();
      };
      write(1, &fkv_types::archive_file_name("main", 2, "001"));
      write(1, &fkv_types::archive_file_name("branchA", 1, "001"));
      write(1, &fkv_types::archive_file_name("main", 2, "002"));
      write(2, &fkv_types::archive_file_name("main", 3, "0012"));
      // 跨分区全等 `(tag, term)`：fanout 归档的正常产物，列举只回一条。
      write(2, &fkv_types::archive_file_name("main", 2, "001"));
      // 负例：无 tag 的 manifest、current_term 文件、子目录、非法分区名。
      write(1, &fkv_types::manifest_file_name("main", 9));
      write(1, &fkv_types::current_term_file_name("main", 0));
      fs::create_dir_all(root.join("demo.1").join("nested")).unwrap();
      fs::create_dir_all(root.join("not_a_partition")).unwrap();

      // 排序键 = (tag, term)："0012" < "002"，同名 tag 按 term 升序。
      let entry_li = runtime.global_list_archive_tag_li("").await.unwrap();
      assert_eq!(
        entry_li,
        vec![
          ArchiveEntry {
            tag: "001".to_owned(),
            term: 1
          },
          ArchiveEntry {
            tag: "001".to_owned(),
            term: 2
          },
          ArchiveEntry {
            tag: "0012".to_owned(),
            term: 3
          },
          ArchiveEntry {
            tag: "002".to_owned(),
            term: 2
          },
        ],
        "{entry_li:?}"
      );

      // 前缀过滤（不要求整段相等，对齐 C++ rfind(prefix, 0)）。
      assert_eq!(
        runtime
          .global_list_archive_tag_li("00")
          .await
          .unwrap()
          .len(),
        4
      );
      assert_eq!(
        runtime.global_list_archive_tag_li("001").await.unwrap(),
        vec![
          ArchiveEntry {
            tag: "001".to_owned(),
            term: 1
          },
          ArchiveEntry {
            tag: "001".to_owned(),
            term: 2
          },
          ArchiveEntry {
            tag: "0012".to_owned(),
            term: 3
          },
        ]
      );
      assert!(
        runtime
          .global_list_archive_tag_li("zz")
          .await
          .unwrap()
          .is_empty()
      );

      runtime.shutdown().await.unwrap();
      let _ = fs::remove_dir_all(&root);
    });
  }
}

#[cfg(test)]
mod branch_salt_tests {
  use super::branch_salt;

  /// 2³² µs 回绕前后的时间戳必须得到不同 salt：旧实现取 `as u32`，两个值
  /// 完全相同，同一逻辑名会落到已存在的 branch 上。
  #[test]
  fn salt_does_not_repeat_after_low_32_bit_wrap() {
    let base = 1_700_000_000_000_000u64;
    for k in 0..8u64 {
      let a = base + k * (1u64 << 32);
      let b = a + (1u64 << 32);
      assert_eq!(a as u32, b as u32, "构造必须落在同一个 u32 回绕点");
      assert_ne!(branch_salt(a), branch_salt(b), "回绕周期后 salt 重名: {a}");
      assert_ne!(branch_salt(a), 0, "salt 不能为 0");
    }
  }
}

#[cfg(test)]
mod s3_instance_tests {
  use super::cloud_s3_instance_num;

  /// 实例数只计真实构造点：shard 数个 + 全局列举 1 个 + 已启用的 crond。
  #[test]
  fn counts_shards_and_enabled_runtime_side_instances() {
    assert_eq!(cloud_s3_instance_num(1, false, false), 2);
    assert_eq!(cloud_s3_instance_num(1, true, false), 3);
    assert_eq!(cloud_s3_instance_num(1, false, true), 3);
    assert_eq!(cloud_s3_instance_num(4, true, true), 7);
  }
}

#[cfg(test)]
mod attach_view_tests {
  use std::{
    env::temp_dir,
    process::id,
    time::{SystemTime, UNIX_EPOCH},
  };

  use fkv_btree::BatchEntry;
  use futures::task::noop_waker;

  use super::*;

  fn config() -> RuntimeConfig {
    RuntimeConfig {
      num_threads: 1,
      root: temp_dir().join(format!(
        "fkv_attach_view_{}_{}",
        id(),
        SystemTime::now()
          .duration_since(UNIX_EPOCH)
          .unwrap()
          .as_nanos()
      )),
      page_size: 512,
      restart_interval: 16,
      pages_per_file_shift: 18,
      segment_size: 4096,
      segments_per_file_shift: 2,
      large_value_threshold: 1024,
      fixed_buffer_size: 0,
      auto_reopen_retry_times: 0,
      auto_reopen_pending_time_us: 0,
      archive_interval_secs: 0,
      archive_num_retained: 100,
      queue_size: 64,
    }
  }

  /// 直接向 shard 发旧 term 写：把 lifecycle 置 `Expired`，但**绕过**本层收尾，
  /// 模拟「错误迟到 / 视图尚未失效」的中间态。
  async fn expire_shard(runtime: &Runtime, tbl: &TableIdent, term: u64) {
    assert_eq!(
      runtime.shard_li[0]
        .request(Request::Put {
          tbl: tbl.clone(),
          batch_li: vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
          term: Some(term),
        })
        .await
        .unwrap_err(),
      KvError::ExpiredTerm
    );
  }

  /// 收尾按 shard 的**真实** lifecycle 清视图，而不是只看错误码。
  ///
  /// 旧实现只在 `ExpiredTerm` 时清：shard 被 detach 后普通请求回 `NotRunning`，
  /// 视图（身份 / 归档目标 / 预热路由）会一直残留。回退成「仅 `ExpiredTerm` 清」
  /// 时第一条断言失败。
  #[test]
  fn non_expired_error_clears_view_by_real_state() {
    pollster::block_on(async {
      let runtime = Runtime::start_mem(config()).unwrap();
      let tbl = TableIdent::new("demo", 1).unwrap();
      runtime.attach_shard(0, "main", 5).await.unwrap();
      // 直接 detach shard，绕过本层收尾：视图仍是 (main, 5)，shard 已经 Detached。
      runtime.shard_li[0]
        .request(Request::DetachShard { shard_id: 0 })
        .await
        .unwrap();
      assert_eq!(
        runtime.get(&tbl, b"a").await.unwrap_err(),
        KvError::NotRunning
      );
      assert_eq!(
        runtime.global_archive_name("x"),
        "manifest_main_0_x",
        "shard 已不托管，视图必须清掉"
      );

      // 反向：shard 仍在托管时，本地超时不得清视图（避免过度清理）。
      runtime.attach_shard(0, "main", 6).await.unwrap();
      assert_eq!(
        runtime
          .request_deadline(
            &tbl,
            Request::Get {
              tbl: tbl.clone(),
              key: b"a".to_vec()
            },
            Duration::ZERO
          )
          .await
          .unwrap_err(),
        KvError::Timeout
      );
      assert_eq!(
        runtime.global_archive_name("x"),
        "manifest_main_6_x",
        "shard 仍 Attached，超时不应清视图"
      );
      runtime.shutdown().await.unwrap();
    });
  }

  /// 全局 fanout 也必须走统一收尾：`GlobalFanout::dispatch*` 直发 `sender_of`
  /// 时，shard 已 `Expired` 的错误不会清本层视图，结果名继续用失效身份。
  #[test]
  fn global_fanout_clears_view_when_shard_expired() {
    let root = temp_dir().join(format!(
      "fkv_fanout_view_{}_{}",
      id(),
      SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    pollster::block_on(async {
      let mut config = config();
      config.root = root.clone();
      let runtime = Runtime::start(config).unwrap();
      let tbl = TableIdent::new("demo", 1).unwrap();
      runtime.attach_shard(0, "main", 5).await.unwrap();
      // 落盘分区：归档 fanout 才有目标可派发。
      runtime
        .put(
          &tbl,
          vec![BatchEntry::upsert(b"seed".to_vec(), b"S".to_vec(), 1)],
        )
        .await
        .unwrap();
      // 直接对 shard 发旧 term 写：lifecycle 置 Expired，本层视图保持 (main, 5)。
      expire_shard(&runtime, &tbl, 4).await;
      let reply = runtime.global_archive("late", 1).await.unwrap();
      assert_eq!(reply.ok_num, 0, "{reply:?}");
      assert_eq!(
        reply.name, "manifest_main_0_late",
        "fanout 错误必须清掉已失效的 attach 视图"
      );
      runtime.shutdown().await.unwrap();
    });
    let _ = fs::remove_dir_all(root);
  }

  /// 迟到的 attach 错误不得清掉更晚成功的 attach 视图：收尾前取的代际快照过期
  /// 即弃权。回退 epoch 校验（只看 lifecycle）时新视图会被清掉。
  #[test]
  fn stale_attach_error_keeps_newer_view() {
    pollster::block_on(async {
      let runtime = Runtime::start_mem(config()).unwrap();
      let tbl = TableIdent::new("demo", 1).unwrap();
      runtime.attach_shard(0, "main", 5).await.unwrap();
      // 旧 attach 发送前的代际快照（模拟一个还在途中的请求）。
      let stale = runtime.route_of(0).unwrap();
      let stale_epoch = stale.epoch_snapshot();
      expire_shard(&runtime, &tbl, 4).await;
      // 更晚的 attach 成功：视图换成 (main, 7)，代际推进。
      runtime.attach_shard(0, "main", 7).await.unwrap();
      // shard 再次被旧 term 请求打过标，但本层视图是更晚 attach 记的。
      expire_shard(&runtime, &tbl, 3).await;
      // 迟到错误：lifecycle 已是 `Expired`，但代际变了，不能清掉 (main, 7)。
      stale
        .finish(stale_epoch, Err(KvError::ExpiredTerm))
        .unwrap_err();
      assert_eq!(
        runtime.global_archive_name("x"),
        "manifest_main_7_x",
        "更晚成功的 attach 视图被迟到错误清掉了"
      );
      // 反证：同代际的错误收尾确实会清视图（否则上面的断言没有区分度）。
      let current = runtime.route_of(0).unwrap();
      let epoch = current.epoch_snapshot();
      current
        .finish(epoch, Err(KvError::ExpiredTerm))
        .unwrap_err();
      assert_eq!(runtime.global_archive_name("x"), "manifest_main_0_x");
      runtime.shutdown().await.unwrap();
    });
  }

  /// 迟到的 detach 清除不得抹掉更晚 attach 的视图。
  ///
  /// 回退 `clear_if_current`（detach 成功无条件 `clear_view`）时视图被清成
  /// `(main, 0)`。
  #[test]
  fn stale_detach_clear_keeps_newer_view() {
    pollster::block_on(async {
      let runtime = Runtime::start_mem(config()).unwrap();
      runtime.attach_shard(0, "main", 5).await.unwrap();
      let route = runtime.route_of(0).unwrap();
      // 模拟已发出、回执未到的 detach：发送前推进代际并取快照。
      let stale = route.bump_epoch();
      // 更晚的 detach + attach 成功落地 (main, 9)。
      runtime.detach_shard(0).await.unwrap();
      runtime.attach_shard(0, "main", 9).await.unwrap();
      assert_eq!(runtime.global_archive_name("x"), "manifest_main_9_x");

      assert!(
        !route.clear_if_current(stale),
        "过期代际的 detach 不得清视图"
      );
      assert_eq!(
        runtime.global_archive_name("x"),
        "manifest_main_9_x",
        "迟到 detach 清掉了更晚 attach 的视图"
      );
      runtime.shutdown().await.unwrap();
    });
  }

  /// 迟到的 attach 记录不得覆盖更晚 detach 的清除。
  ///
  /// 回退 `record_if_current`（attach 成功无条件记录）时视图被重新写成
  /// `(main, 9)`。
  #[test]
  fn stale_attach_record_skipped_after_newer_detach() {
    pollster::block_on(async {
      let runtime = Runtime::start_mem(config()).unwrap();
      runtime.attach_shard(0, "main", 5).await.unwrap();
      let route = runtime.route_of(0).unwrap();
      // 模拟在途 attach：发送前推进代际。
      let stale = route.bump_epoch();
      // 更晚的 detach 成功：视图清空。
      runtime.detach_shard(0).await.unwrap();
      assert_eq!(runtime.global_archive_name("x"), "manifest_main_0_x");

      let recorded = route.record_if_current(stale, || runtime.record_attach(0, "main", 9));
      assert!(!recorded, "过期代际的 attach 记录不得落地");
      assert_eq!(
        runtime.global_archive_name("x"),
        "manifest_main_0_x",
        "迟到 attach 记录覆盖了更晚 detach 的清除"
      );
      runtime.shutdown().await.unwrap();
    });
  }

  /// 代际 CAS：过期快照不能抢占，只有当前代际能推进。
  ///
  /// 覆盖 [`AttachEpoch::bump_if_current`] 的 `AcqRel` compare-exchange 语义。
  #[test]
  fn epoch_cas_rejects_stale_generation() {
    let epoch = AttachEpoch::new(1);
    let first = epoch.bump(0);
    assert_eq!(first, 1);
    assert!(epoch.bump_if_current(0, first));
    assert!(!epoch.bump_if_current(0, first), "过期代际不能被抢占");
    assert!(epoch.bump_if_current(0, first + 1));
    assert_eq!(epoch.snapshot(0), first + 2);
  }

  /// 在途 detach 回执迟到（代际已被更晚操作推进）：`detach_shard` 不得清视图。
  ///
  /// 覆盖调用点而非助手函数：回退成 `route.clear_view()`（无条件清）时视图被
  /// 清成 `(main, 0)`。
  #[test]
  fn in_flight_detach_does_not_clear_newer_view() {
    use std::{future::Future, task::Context};

    pollster::block_on(async {
      let waker = noop_waker();
      let mut cx = Context::from_waker(&waker);
      // mem shard 可能在第一次 poll 前就完成 detach；重试直到真的拿到在途状态。
      for _ in 0..32 {
        let runtime = Runtime::start_mem(config()).unwrap();
        runtime.attach_shard(0, "main", 5).await.unwrap();
        let mut fut = Box::pin(runtime.detach_shard(0));
        if fut.as_mut().poll(&mut cx).is_pending() {
          // 更晚的控制面操作已推进代际（真实场景里是重 attach 开始）。
          runtime.route_of(0).unwrap().bump_epoch();
          assert!(fut.await.is_ok());
          assert_eq!(
            runtime.global_archive_name("x"),
            "manifest_main_5_x",
            "迟到 detach 回执清掉了更晚操作拥有的视图"
          );
          runtime.shutdown().await.unwrap();
          return;
        }
        let _ = fut.await;
        runtime.shutdown().await.unwrap();
      }
      panic!("detach 在 32 次尝试内都同步完成，无法构造在途回执");
    });
  }

  /// 在途 attach 回执迟到（代际被更晚操作推进）：`attach_shard` 不落地视图，
  /// 且因 shard 仍托管必须回 `Busy` 让调用方重试。
  ///
  /// 仅防御：生产路径由 per-shard `control_li` 串行后不可达（代际推进与视图
  /// 落地在同一临界区），这里手工 `bump_epoch` 模拟「在控制锁之外推进代际」的
  /// 控制操作，覆盖 superseded 分支。回退成无条件 `record_attach` 时结果名变成
  /// `manifest_main_9_x`。
  #[test]
  fn in_flight_attach_does_not_override_newer_view() {
    use std::{future::Future, task::Context};

    pollster::block_on(async {
      let runtime = Runtime::start_mem(config()).unwrap();
      let mut fut = Box::pin(runtime.attach_shard(0, "main", 9));
      let waker = noop_waker();
      let mut cx = Context::from_waker(&waker);
      assert!(fut.as_mut().poll(&mut cx).is_pending());
      // 更晚的控制面操作已推进代际。
      runtime.route_of(0).unwrap().bump_epoch();
      assert!(
        matches!(fut.await, Err(KvError::Busy)),
        "被更晚操作接管的 attach 必须返 Busy"
      );
      assert_eq!(
        runtime.global_archive_name("x"),
        "manifest_main_0_x",
        "迟到的 attach 视图覆盖了更晚操作的清除"
      );
      runtime.shutdown().await.unwrap();
    });
  }

  /// superseded 且 shard 已 `Detached`：必须回 `Busy`（可重试），不得 `Ok(())`。
  ///
  /// 仅防御（同上条，生产路径不可达）：手工推进代际模拟「在控制锁之外推进代际」
  /// 的控制操作。回退「`Detached` 时返 `Ok(())`」时这里拿到 `Ok`，调用方以为
  /// attach 成功，但 shard 终态是 `Detached`，后续请求全 `NotRunning`。
  #[test]
  fn superseded_attach_on_detached_shard_reports_busy() {
    use std::{future::Future, task::Context};

    pollster::block_on(async {
      let runtime = Runtime::start_mem(config()).unwrap();
      let tbl = TableIdent::new("demo", 1).unwrap();
      let mut fut = Box::pin(runtime.attach_shard(0, "main", 9));
      let waker = noop_waker();
      let mut cx = Context::from_waker(&waker);
      assert!(fut.as_mut().poll(&mut cx).is_pending());
      // 绕过本层收尾直接 detach：attach 请求先入队，回执仍是 `AttachShard`，
      // 但 shard 的终态已是 `Detached`。
      runtime.shard_li[0]
        .request(Request::DetachShard { shard_id: 0 })
        .await
        .unwrap();
      // 更晚的控制面操作推进代际：本次 attach 视图不得落地。
      runtime.route_of(0).unwrap().bump_epoch();
      assert!(
        matches!(fut.await, Err(KvError::Busy)),
        "shard 已 Detached 的 superseded attach 必须返 Busy，不得谎报成功"
      );
      assert_eq!(
        runtime.get(&tbl, b"a").await.unwrap_err(),
        KvError::NotRunning,
        "attach 未生效，后续请求必须显式 NotRunning"
      );

      // `Busy` 可重试：调用方重放 attach 即恢复。
      runtime.attach_shard(0, "main", 9).await.unwrap();
      assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), None);
      runtime.shutdown().await.unwrap();
    });
  }

  /// 迟到旧代际的超时登记不得把更晚的登记改小。
  ///
  /// 回退 `mark_timeout` 为 `store`（后到者覆盖）时 settle 取到旧代际，
  /// `clear_if_current` 弃权，视图永久残留。
  #[test]
  fn late_timeout_registration_keeps_newer_epoch() {
    let epoch = AttachEpoch::new(1);
    epoch.mark_timeout(0, 9);
    epoch.mark_timeout(0, 3);
    assert_eq!(epoch.settle_timeout(0, false), Some(9));
    assert_eq!(epoch.settle_timeout(0, false), None);
  }

  /// 数据面 / 控制面 / 归档 crond 的 `ShardRoute` 共享同一代际与视图：
  /// 三者都由 [`ShardRoute::new`] 构造。
  #[test]
  fn route_constructors_share_epoch_and_views() {
    pollster::block_on(async {
      let runtime = Runtime::start_mem(config()).unwrap();
      let data = runtime.route_of(0).unwrap();
      let control = runtime.control_route(0).unwrap();
      assert_eq!(data.shard_id, control.shard_id);
      assert!(Arc::ptr_eq(&data.epoch, &control.epoch));
      assert!(Arc::ptr_eq(&data.epoch, &runtime.attach_epoch));

      // 共享代际：control 推进后 data 的旧快照立即过期。
      let stale = data.epoch_snapshot();
      control.bump_epoch();
      assert!(!data.epoch.is_current(0, stale));
      runtime.shutdown().await.unwrap();
    });
  }

  /// 调用方在 `.await` 中途 drop 请求 future：收尾仍要按 shard 的真实 lifecycle
  /// 清视图。
  ///
  /// 回退 `Drop` 守卫（只在 `.await` 正常返回后 `finish`）时视图残留
  /// `(main, 5)`。
  #[test]
  fn dropped_request_still_clears_view_by_lifecycle() {
    use std::{future::Future, task::Context};

    pollster::block_on(async {
      let tbl = TableIdent::new("demo", 1).unwrap();
      let waker = noop_waker();
      let mut cx = Context::from_waker(&waker);
      for _ in 0..32 {
        let runtime = Runtime::start_mem(config()).unwrap();
        runtime.attach_shard(0, "main", 5).await.unwrap();
        let route = runtime.route_of(0).unwrap();
        let mut fut = Box::pin(route.request(Request::Get {
          tbl: tbl.clone(),
          key: b"a".to_vec(),
        }));
        if fut.as_mut().poll(&mut cx).is_pending() {
          // 绕过本层收尾直接 detach shard：lifecycle 变 `Detached`，视图仍是 (main, 5)。
          runtime.shard_li[0]
            .request(Request::DetachShard { shard_id: 0 })
            .await
            .unwrap();
          // 取消在途请求：守卫必须收尾。
          drop(fut);
          assert_eq!(
            runtime.global_archive_name("x"),
            "manifest_main_0_x",
            "被取消的请求也必须清视图"
          );
          runtime.shutdown().await.unwrap();
          return;
        }
        let _ = fut.await;
        runtime.shutdown().await.unwrap();
      }
      panic!("请求在 32 次尝试内都同步完成，无法构造在途回执");
    });
  }

  /// 本地 `Timeout` 后 shard 才置 `Expired`：下一次观测点必须清掉残留视图。
  ///
  /// 回退「超时登记 + 观测复核」时视图一直停在 `(main, 5)`（只在遇到下一次
  /// 错误时才清）。这也是 `fkv_shard` 不提供 Expired 写点回调时的已接受近似。
  #[test]
  fn timeout_then_late_expire_clears_view_on_next_observation() {
    pollster::block_on(async {
      let runtime = Runtime::start_mem(config()).unwrap();
      let tbl = TableIdent::new("demo", 1).unwrap();
      runtime.attach_shard(0, "main", 5).await.unwrap();

      // 本地超时：请求已入队，shard 仍是 `Attached`，此刻不能清视图。
      assert_eq!(
        runtime
          .request_deadline(
            &tbl,
            Request::Get {
              tbl: tbl.clone(),
              key: b"a".to_vec()
            },
            Duration::ZERO
          )
          .await
          .unwrap_err(),
        KvError::Timeout
      );
      assert_eq!(
        runtime.global_archive_name("x"),
        "manifest_main_5_x",
        "shard 仍托管，超时不该清视图"
      );

      // shard 稍后才因请求真正执行而置 `Expired`（绕过本层收尾）。
      expire_shard(&runtime, &tbl, 4).await;
      assert_eq!(
        runtime.global_archive_name("x"),
        "manifest_main_0_x",
        "超时登记未在下一次观测点复核"
      );
      runtime.shutdown().await.unwrap();
    });
  }
}
