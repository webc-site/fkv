//! attach / detach 身份与代际：视图、超时登记与收尾。

use std::sync::{
  Arc, Mutex, MutexGuard, PoisonError,
  atomic::{AtomicU64, Ordering},
};

use fkv_api::{Request, Response};
use fkv_error::{KvError, Result};
use fkv_types::{MAIN_BRANCH_NAME, normalize_branch_name};
use log::{info, warn};

use crate::{
  Runtime,
  archive::ArchiveState,
  prewarm::{PrewarmState, lock_prewarm_state},
};

/// 一个 shard 的 attach 身份（归一化 branch + term）。
///
/// 全局结果名需要它：C++ `HandleGlobalArchiveRequest` 在列举分区**之前**就写
/// `result_archive_ = BranchArchiveName(branch_, ShardTerm(ds_shard), tag)`，
/// 所以 0 个目标或全部失败时也返回非空名字。本仓没有 store 级 `branch_`，
/// 用 attach 身份补齐（记录点见 [`Runtime::attach_shard`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttachIdent {
  pub(crate) branch: String,
  pub(crate) term: u64,
}

/// 取 attach 身份表锁；poison 只说明上一次持锁线程 panic，状态本身仍可用。
fn lock_attach_ident_li(
  li: &Mutex<Vec<Option<AttachIdent>>>,
) -> MutexGuard<'_, Vec<Option<AttachIdent>>> {
  li.lock().unwrap_or_else(PoisonError::into_inner)
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
pub(crate) struct AttachEpoch {
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
  pub(crate) fn new(shard_num: u16) -> Self {
    let shard_num = usize::from(shard_num);
    Self {
      li: (0..shard_num).map(|_| AtomicU64::new(0)).collect(),
      gate_li: (0..shard_num).map(|_| Mutex::new(())).collect(),
      timeout_li: (0..shard_num).map(|_| AtomicU64::new(0)).collect(),
    }
  }

  pub(crate) fn index(shard_id: u32) -> Option<usize> {
    usize::try_from(shard_id).ok()
  }

  pub(crate) fn slot(&self, shard_id: u32) -> Option<&AtomicU64> {
    Self::index(shard_id).and_then(|index| self.li.get(index))
  }

  pub(crate) fn snapshot(&self, shard_id: u32) -> u64 {
    self
      .slot(shard_id)
      .map_or(0, |epoch| epoch.load(Ordering::Acquire))
  }

  /// 推进代际并返回本轮新代际。
  pub(crate) fn bump(&self, shard_id: u32) -> u64 {
    self
      .slot(shard_id)
      .map_or(0, |epoch| epoch.fetch_add(1, Ordering::AcqRel) + 1)
  }

  /// 快照仍是当前代际（没有更晚的 attach / detach 覆盖）。
  pub(crate) fn is_current(&self, shard_id: u32, epoch: u64) -> bool {
    self.snapshot(shard_id) == epoch
  }

  /// 原子推进：仅当 `epoch` 仍是当前代际时置为 `epoch + 1`。
  pub(crate) fn bump_if_current(&self, shard_id: u32, epoch: u64) -> bool {
    self.slot(shard_id).is_some_and(|slot| {
      slot
        .compare_exchange(epoch, epoch + 1, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    })
  }

  /// 在 per-shard 视图临界区内执行 `f`。
  pub(crate) fn with_gate<R>(&self, shard_id: u32, f: impl FnOnce() -> R) -> Option<R> {
    let gate = Self::index(shard_id).and_then(|index| self.gate_li.get(index))?;
    let _guard = gate.lock().unwrap_or_else(PoisonError::into_inner);
    Some(f())
  }

  /// 登记「本地超时时 lifecycle 仍 `Attached`」：等下一次观测复核。
  ///
  /// 用 `fetch_max` 而非 `store`：迟到的旧代际 `Timeout` 不得把更晚的登记改小。
  /// 登记值随后被 `settle_timeout` 取出并用作代际校验，改小会让本该清的视图
  /// 因代际不符而永久残留。
  pub(crate) fn mark_timeout(&self, shard_id: u32, epoch: u64) {
    if let Some(slot) = Self::index(shard_id).and_then(|index| self.timeout_li.get(index)) {
      slot.fetch_max(epoch + 1, Ordering::Release);
    }
  }

  /// 复核超时登记：shard 已不 `Attached` 时返回登记时的代际并消除登记。
  pub(crate) fn settle_timeout(&self, shard_id: u32, attached: bool) -> Option<u64> {
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
pub(crate) struct ShardViews {
  pub(crate) archive: Option<Arc<ArchiveState>>,
  pub(crate) prewarm: Option<Arc<Mutex<PrewarmState>>>,
  pub(crate) ident: Option<Arc<Mutex<Vec<Option<AttachIdent>>>>>,
}

impl ShardViews {
  /// 代际仍是最新时清视图；校验与清除在同一个 per-shard 临界区内。
  ///
  /// 返回 `false` = 已有更晚的 attach / detach，本次清除弃权。
  pub(crate) fn clear_if_epoch(&self, epoch_li: &AttachEpoch, shard_id: u32, epoch: u64) -> bool {
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

impl Runtime {
  /// 复核本地超时登记：某次请求本地 `Timeout` 时 shard 仍是 `Attached`，之后才
  /// 可能因请求真正执行而置 `Expired`。这里在下一次观测点（取结果名 / fanout 前）
  /// 补做一次判定，把那时读到的 `Attached` 变成「稍后再看」而不是「永远有效」。
  ///
  /// 已接受的近似：两次观测之间视图仍可能残留，`fkv_shard` 不提供 Expired 写点
  /// 回调；清视图前仍校验代际，避免抹掉更晚 attach 的新视图。
  pub(crate) fn settle_timeout_views(&self) {
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

  /// 结果名使用的 attach 身份；未 attach 回落 `(main, 0)`。
  pub(crate) fn attach_ident(&self) -> AttachIdent {
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
  pub(crate) fn record_attach(&self, shard_id: u32, branch: &str, term: u64) {
    let mut ident_li = lock_attach_ident_li(&self.attach_ident_li);
    if let Some(slot) = ident_li.get_mut(shard_id as usize) {
      *slot = Some(AttachIdent {
        branch: branch.to_owned(),
        term,
      });
    }
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
}
