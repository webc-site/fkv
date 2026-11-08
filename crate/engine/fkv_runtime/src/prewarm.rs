//! Cloud 预热任务、状态与后台 crond。

use std::{
  collections::VecDeque,
  io,
  marker::PhantomData,
  sync::{Arc, Mutex, MutexGuard, PoisonError},
  thread,
  thread::JoinHandle,
  time::{Duration, Instant},
};

use compio::{driver::ProactorBuilder, runtime};
use crossfire::{
  MTx, RecvTimeoutError, Rx,
  mpsc::{Array, bounded_blocking},
};
use fkv_api::{Request, Response};
use fkv_cloud::{CLOUD_LIST_PAGE_KEYS, CloudStore};
use fkv_error::{KvError, Result};
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_object::ObjectStore;
use fkv_task_prewarm::{CompletionReason, PrewarmStat, prewarm_tables_with_concurrency};
use fkv_types::{MAIN_BRANCH_NAME, TableIdent};
use gxhash::HashSet;
use log::{error, info, warn};

use crate::{PrewarmReport, Runtime};

/// 单个 shard 的预热目标：attach 时的 `(branch, term)` 与其名下的分区。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PrewarmTarget {
  pub(crate) shard_id: usize,
  pub(crate) branch: String,
  pub(crate) term: u64,
  pub(crate) table_li: Vec<TableIdent>,
}

/// reopen 投递的 per-table 预热队列上限（对齐 `fkv_task_prewarm` 的
/// `kMaxPrewarmPendingFiles = 1000` 有界思路，重复表去重）。
pub(crate) const MAX_PENDING_PREWARM_TABLES: usize = 1000;

/// 预热失败回队 / 列举失败后的自唤醒退避：首轮 [`PREWARM_RETRY_MIN`]，
/// 连续失败翻倍，封顶 [`PREWARM_RETRY_MAX`]。
///
/// 没有这一步时失败分区只在下次 attach / reopen 的 `signal()` 才重试，远端
/// 短时抖动会让分区永久停在未预热状态。
const PREWARM_RETRY_MIN: Duration = Duration::from_millis(50);
const PREWARM_RETRY_MAX: Duration = Duration::from_secs(1);

/// `stop()` 等待预热线程退出的上限。
///
/// 预热 tick 是整轮语义（列举全部远端分区 + 逐表下载），大 bucket / 慢 S3 下单轮
/// 可达分钟～小时级。`JoinHandle` 没有带时限的 join，这里与 `ArchiveCrond` 的
/// `ARCHIVE_STOP_TIMEOUT` 同口径轮询 `is_finished`：超时后丢弃句柄（线程分离）并
/// 告警，保证 `Runtime::shutdown` / `Drop` 的等待有上界。丢弃信号发送端后线程
/// 不再开始下一轮，在飞 tick 跑完后在循环顶部看到通道断开即退出。
pub(crate) const PREWARM_STOP_TIMEOUT: Duration = Duration::from_secs(2);

/// `stop()` 轮询 `JoinHandle::is_finished` 的间隔（与 archive crond 同粒度）。
const PREWARM_STOP_POLL: Duration = Duration::from_millis(10);

/// 预热路由状态：每个 shard 的 attach 视图与「已预热」标记。
///
/// crond 线程与 attach / detach / reopen 路径共享（`Mutex` 保护）。
/// - attach 轮只处理「已 attach 且尚未预热」的 shard，避免每个 shard attach
///   都重跑全部 shard 的列举与下载；
/// - `pending_tbl_li` 是 reopen 成功后的 per-table 目标（对齐 C++
///   `ReopenTask::Reopen` → `PrewarmService::Prewarm`）：不受 `done_li` 限制，
///   否则 attach 之后才出现的新分区永远不预热。
#[derive(Debug)]
pub(crate) struct PrewarmState {
  pub(crate) attached_li: Vec<bool>,
  pub(crate) ident_li: Vec<(String, u64)>,
  pub(crate) done_li: Vec<bool>,
  /// reopen 投递、待 crond 消费的单表目标（FIFO，重复表去重）。
  pub(crate) pending_tbl_li: VecDeque<TableIdent>,
  /// `pending_tbl_li` 的 O(1) 去重索引（与队列同步增删）。
  ///
  /// 旧实现用 `VecDeque::contains` 线性查重，`plan` 又对每条 pending 做
  /// `Vec::contains`：pending 上限 1000、单 shard 分区上万时是约 1e7 次
  /// `String` 比较，且全程持 `lock_prewarm_state`。
  pub(crate) pending_set: HashSet<TableIdent>,
  pub(crate) last_report: Option<PrewarmReport>,
}

impl PrewarmState {
  pub(crate) fn new(shard_num: u16) -> Self {
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
  pub(crate) fn attach(&mut self, shard_id: u32, branch: &str, term: u64) {
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

  pub(crate) fn detach(&mut self, shard_id: u32) {
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
  pub(crate) fn enqueue_table(&mut self, tbl: TableIdent) {
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
  pub(crate) fn take_plan_input(&mut self) -> PlanInput {
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
pub(crate) struct PlanInput {
  pub(crate) attached_li: Vec<bool>,
  pub(crate) done_li: Vec<bool>,
  pub(crate) ident_li: Vec<(String, u64)>,
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
pub(crate) fn plan_targets(input: PlanInput, table_li: Vec<TableIdent>) -> Vec<PrewarmTarget> {
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
pub(crate) async fn remote_partition_li<I, O: ObjectStore>(
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
pub(crate) fn lock_prewarm_state(state: &Mutex<PrewarmState>) -> MutexGuard<'_, PrewarmState> {
  state.lock().unwrap_or_else(PoisonError::into_inner)
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
pub(crate) struct PrewarmCrond<I, O> {
  signal_tx: Option<MTx<Array<()>>>,
  join: Option<JoinHandle<()>>,
  _marker: PhantomData<fn() -> (I, O)>,
}

impl<I, O> PrewarmCrond<I, O>
where
  I: FileIo + ManifestIo + PageIo + SegmentIo + Send + 'static,
  O: ObjectStore + Send + 'static,
{
  pub(crate) fn start(
    cloud: CloudStore<I, O>,
    state: Arc<Mutex<PrewarmState>>,
    table_concurrency: usize,
  ) -> Result<Self> {
    Self::start_inner(cloud, state, table_concurrency, Self::run)
  }

  /// 线程体可注入（单测用桩模拟「一轮预热在飞、不返回」），生产入口固定
  /// [`Self::run`]。
  pub(crate) fn start_inner(
    cloud: CloudStore<I, O>,
    state: Arc<Mutex<PrewarmState>>,
    table_concurrency: usize,
    run: impl FnOnce(CloudStore<I, O>, Arc<Mutex<PrewarmState>>, usize, &Rx<Array<()>>) + Send + 'static,
  ) -> Result<Self> {
    let (signal_tx, signal_rx) = bounded_blocking::<()>(1);
    let join = thread::Builder::new()
      .name("fkv-prewarm".to_owned())
      .spawn(move || run(cloud, state, table_concurrency, &signal_rx))
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
  pub(crate) fn signal(&self) {
    if let Some(tx) = &self.signal_tx {
      let _ = tx.try_send(());
    }
  }
}

/// `stop` 只操作通道与 join 句柄，不依赖 IO 泛型约束；放在无约束 impl 里
/// 才能被同样无约束的 `Drop` 调用。
impl<I, O> PrewarmCrond<I, O> {
  /// 停止预热线程，等待有上界。
  ///
  /// 丢弃信号发送端即停止后续调度：线程在两轮 tick 之间看到通道断开后退出，
  /// 在飞的一轮 tick 不中断（整轮下载完成后自然结束）。最多等
  /// [`PREWARM_STOP_TIMEOUT`]，超时告警并丢弃 `JoinHandle` 让线程分离，避免
  /// `Runtime::shutdown` / `Drop` 被整轮预热拖住。
  pub(crate) fn stop(&mut self) {
    self.signal_tx.take();
    let Some(join) = self.join.take() else {
      return;
    };
    // `JoinHandle` 没有带时限的 join：轮询 `is_finished` 做上界。
    let deadline = Instant::now() + PREWARM_STOP_TIMEOUT;
    while !join.is_finished() {
      if Instant::now() >= deadline {
        warn!(
          target: "fkv_runtime",
          "prewarm crond stop timed out timeout_ms={} thread=detached",
          PREWARM_STOP_TIMEOUT.as_millis()
        );
        // `join` 在此 drop：线程分离，不阻塞停机。
        return;
      }
      thread::sleep(PREWARM_STOP_POLL);
    }
    let _ = join.join();
  }
}

impl<I, O> Drop for PrewarmCrond<I, O> {
  fn drop(&mut self) {
    self.stop();
  }
}

impl Runtime {
  /// reopen 成功后按表投递预热（仅 Cloud + `prewarm_cloud_cache` 生效）。
  ///
  /// 对齐 C++ `ReopenTask::Reopen` 末尾的 `PrewarmService::Prewarm(tbl)`：
  /// 只入队并唤醒后台 crond，不阻塞 reopen 回执。attach 轮的 `done_li` 只覆盖
  /// attach 瞬间的远端分区集合，reopen 是「新分区后来才出现」的补充触发源。
  pub(crate) fn signal_table_prewarm(&self, tbl: &TableIdent) {
    let (Some(state), Some(crond)) = (&self.prewarm_state, &self.prewarm_crond) else {
      return;
    };
    lock_prewarm_state(state).enqueue_table(tbl.clone());
    crond.signal();
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
}
