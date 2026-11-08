//! StandbyReplica 支持：追赶任务化（io-33）、远端分区 / 预热请求。
//!
//! 追赶与打开解耦（io-33）：`sync_table_with_term`（`Arc` 服务，Send+Sync）交
//! 独立 compio task 执行，触发请求（懒开首访问 / Reopen 族 / auto-reopen
//! driver）挂起等待，完成后经 `finish_standby_sync` 重放进 `wake_li` 走完整
//! 打开路径——对齐 C++ `ReopenTask::Reopen` 的 `RsyncPartition`（提交后台
//! Job，立即返回）+ `WaitIo`（挂起单任务、交还线程）语义；事件循环在同步
//! 期间照常出队 / 回执在飞读 / 推进写门与 reopen 到期，不再被 rsync 全程挂起。

use std::{
  collections::VecDeque,
  future::Future,
  mem::take,
  pin::Pin,
  task::{Context, Poll},
  time::Instant,
};

use compio::runtime::{self, spawn};
use fkv_api::{Request, Response};
use fkv_error::{KvError, Result};
use fkv_io::{ManifestIo, PageIo};
use fkv_task_prewarm::CompletionReason;
use fkv_types::{MAIN_BRANCH_NAME, TableIdent};
use futures::{StreamExt, future::poll_fn, stream::FuturesUnordered};
use gxhash::{HashMap, HashSet};
use log::{error, warn};

use crate::{
  lifecycle::ShardState,
  reopen::ReopenQueue,
  sender::{Envelope, Pri},
  write::poll_ready_once,
  write_gate::WriteGate,
};

/// 追赶准入结果。
pub(crate) enum SyncAdmit {
  /// 条目已有完成结果（由打开路径经 [`take_synced`] 消费）：请求继续正常分发。
  Ready,
  /// 追赶在飞 / 刚派发：envelope 调用方随即 [`park_sync_waiter`] 挂起；
  /// auto-reopen driver 无 envelope，靠 reopen 到期轮询在完成后重试。
  Parked,
}

/// 每表追赶条目：`result == None` 表示任务在飞；`Some` 表示已完成、等待首个
/// 重放的打开路径消费（消费即整条移除，不留给更晚的首访问当陈旧结果）。
struct SyncEntry {
  waiter_li: Vec<Envelope>,
  result: Option<Result<()>>,
}

/// 一条追赶任务：`JoinHandle` 失败（任务 panic）时保留 `tbl` 并按 `IoFail`
/// 回执，条目照常走完成收尾（对齐 `WriteTask` 的处理）。
struct SyncTask {
  tbl: TableIdent,
  handle: runtime::JoinHandle<Result<()>>,
}

impl Future for SyncTask {
  type Output = (TableIdent, Result<()>);

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    let this = self.get_mut();
    let tbl = this.tbl.clone();
    match Pin::new(&mut this.handle).poll(cx) {
      Poll::Ready(Ok(result)) => Poll::Ready((tbl, result)),
      Poll::Ready(Err(err)) => {
        error!(
          target: "fkv_shard",
          "standby sync task failed tbl={tbl} error={err}"
        );
        Poll::Ready((tbl, Err(KvError::IoFail)))
      }
      Poll::Pending => Poll::Pending,
    }
  }
}

/// standby 追赶单飞集合：每表至多一个在飞任务，完成经事件循环回收。
pub(crate) struct StandbyFlight {
  task_set: FuturesUnordered<SyncTask>,
  /// 有任务在飞的表（含身份切换前派发的旧任务）：`StandbyService` 不做按表
  /// 互斥（`sync_all` 要求表互不相同，两轮 rsync 会争用同一 staging 目录），
  /// 新派发必须等旧任务退出，见 [`respawn_deferred`]。
  inflight: HashSet<TableIdent>,
  /// 当前等待 / 待消费条目；身份切换时整体清除（见 [`StandbyFlight::fail_stale`]）。
  entries: HashMap<TableIdent, SyncEntry>,
  /// 旧身份任务仍在飞期间到达的新身份等待请求：旧任务退出后以当前身份重派。
  deferred: HashMap<TableIdent, Vec<Envelope>>,
}

impl StandbyFlight {
  pub(crate) fn new() -> Self {
    Self {
      task_set: FuturesUnordered::new(),
      inflight: HashSet::default(),
      entries: HashMap::default(),
      deferred: HashMap::default(),
    }
  }

  /// 是否还有在飞追赶任务（主循环据此把它们并进等待集，io-32 同类自查）。
  pub(crate) fn is_empty(&self) -> bool {
    self.task_set.is_empty()
  }

  /// 非阻塞回收一个已完成的追赶任务。
  ///
  /// 用 `poll_fn` 取**事件循环当前的 waker** 再 poll 一次（io-13 纪律，同
  /// `WriteFlight::try_next`）：完成通知必须回到事件循环 waker。
  pub(crate) async fn try_next(&mut self) -> Option<(TableIdent, Result<()>)> {
    let set = &mut self.task_set;
    poll_fn(|cx| Poll::Ready(poll_ready_once(set, cx))).await
  }

  /// 阻塞等一个追赶任务完成（主循环空闲等待集的事件源）。
  pub(crate) async fn next(&mut self) -> Option<(TableIdent, Result<()>)> {
    self.task_set.next().await
  }

  /// 身份切换（detach / Fresh attach）清理（io-25）：挂起 waiter 统一回错，
  /// 条目与 deferred 清空。在飞任务保留（rsync 不能中途取消），其结果按
  /// 「条目已清」在完成时丢弃；期间新身份的请求进新的 deferred 等重派。
  pub(crate) fn fail_stale(&mut self, err: KvError) {
    for (_, mut entry) in self.entries.drain() {
      for envelope in entry.waiter_li.drain(..) {
        envelope.send(Err(err));
      }
    }
    for (_, waiter_li) in self.deferred.drain() {
      for envelope in waiter_li {
        envelope.send(Err(err));
      }
    }
  }
}

/// 该请求的打开路径是否需要先追 master（io-33 准入判定）。
///
/// 镜像 `ensure_table_ready` / `dispatch_reopen` 的内联同步条件，两者必须
/// 保持一致：本函数判真时请求会被挂起等追赶，重放的打开路径经
/// [`take_synced`] 消费结果；判假时打开路径不得再依赖同步结果。
pub(crate) fn standby_sync_due(state: &ShardState, req: &Request) -> Option<TableIdent> {
  state.standby.as_ref()?;
  let tbl = req.table()?;
  // Reopen 族：`dispatch_reopen` 的追赶不以「已打开 / main 分支」为前提，
  // 已打开表的显式重开同样要追一次（对齐 C++ `ReopenTask::Reopen`）。
  if matches!(
    req,
    Request::Reopen { .. } | Request::ReopenArchive { .. } | Request::ReopenExternal { .. }
  ) {
    return Some(tbl.clone());
  }
  // 其余带表请求走 `ensure_table_ready` 的同步点：main 分支、未打开且无在飞
  // 写（已打开走 fast path，写占用先回 Busy，与内联条件一致）。
  if !req.skips_table_open()
    && state.branch == MAIN_BRANCH_NAME
    && !state.stores.is_open(tbl)
    && !state.stores.is_writing(tbl)
  {
    return Some(tbl.clone());
  }
  None
}

/// 追赶单飞准入。
///
/// - 条目已有完成结果：`Ready`，请求继续分发（结果由打开路径消费）；
/// - 无条目且无在飞任务：派发追赶任务（对齐 C++ `RsyncPartition` 提交后台
///   Job 即返回），返回 `Parked`；
/// - 追赶在飞（含旧身份遗留任务）：返回 `Parked`，调用方挂请求等完成。
pub(crate) fn admit_standby_sync(state: &mut ShardState, tbl: &TableIdent) -> SyncAdmit {
  if state
    .standby_flight
    .entries
    .get(tbl)
    .is_some_and(|entry| entry.result.is_some())
  {
    return SyncAdmit::Ready;
  }
  if !state.standby_flight.entries.contains_key(tbl) && !state.standby_flight.inflight.contains(tbl)
  {
    spawn_sync(state, tbl);
  }
  SyncAdmit::Parked
}

/// 把触发请求挂进追赶等待（[`admit_standby_sync`] 返回 `Parked` 后调用）：
/// 条目在则挂 waiter；旧身份任务在飞（无条目）则进 deferred 等重派。
pub(crate) fn park_sync_waiter(state: &mut ShardState, tbl: &TableIdent, envelope: Envelope) {
  let flight = &mut state.standby_flight;
  match flight.entries.get_mut(tbl) {
    Some(entry) => entry.waiter_li.push(envelope),
    None => flight
      .deferred
      .entry(tbl.clone())
      .or_default()
      .push(envelope),
  }
}

/// 消费已完成的追赶结果：重放的打开路径（`ensure_table_ready` /
/// `dispatch_reopen`）调用；消费即移除条目，之后同表首访问重新派发追赶
/// （与内联「每次打开尝试前追一次」的新鲜度语义一致）。
pub(crate) fn take_synced(state: &mut ShardState, tbl: &TableIdent) -> Option<Result<()>> {
  let flight = &mut state.standby_flight;
  let result = flight
    .entries
    .get_mut(tbl)
    .filter(|entry| entry.result.is_some())
    .and_then(|entry| entry.result.take());
  if result.is_some() {
    flight.entries.remove(tbl);
  }
  result
}

/// 丢弃已完成的追赶结果：挂起重放的请求取消 / 过期时调用——不留可能过期的
/// 结果给下一次首访问消费（在飞条目不受影响）。
pub(crate) fn discard_synced(state: &mut ShardState, tbl: &TableIdent) {
  let flight = &mut state.standby_flight;
  if flight
    .entries
    .get(tbl)
    .is_some_and(|entry| entry.result.is_some())
  {
    flight.entries.remove(tbl);
  }
}

/// 派发一条追赶任务：`Arc` 服务交独立 compio task，完成经 `StandbyFlight`
/// 的任务集合回事件循环（对齐 C++ `StartRsyncJob` 的 inflight job 异步收割）。
fn spawn_sync(state: &mut ShardState, tbl: &TableIdent) {
  let Some(service) = state.standby.clone() else {
    return;
  };
  // 同步用当前 attach 的 term 做 fencing（对齐 C++ `store_->Term()`）。
  let term = state.term.unwrap_or(0);
  let task_tbl = tbl.clone();
  let handle = spawn(async move {
    service
      .sync_table_with_term(&task_tbl, term)
      .await
      .map(|_| ())
  });
  let flight = &mut state.standby_flight;
  flight.inflight.insert(tbl.clone());
  flight.entries.insert(
    tbl.clone(),
    SyncEntry {
      waiter_li: Vec::new(),
      result: None,
    },
  );
  flight.task_set.push(SyncTask {
    tbl: tbl.clone(),
    handle,
  });
}

/// 旧身份任务退出后重派：有新身份等待请求（deferred）时以当前身份重新派发
/// 追赶，等待请求转为该任务的 waiter。
fn respawn_deferred(state: &mut ShardState, tbl: &TableIdent) {
  if !state
    .standby_flight
    .deferred
    .get(tbl)
    .is_some_and(|waiter_li| !waiter_li.is_empty())
  {
    return;
  }
  spawn_sync(state, tbl);
  let flight = &mut state.standby_flight;
  if let Some(waiter_li) = flight.deferred.remove(tbl)
    && let Some(entry) = flight.entries.get_mut(tbl)
  {
    entry.waiter_li = waiter_li;
  }
}

/// 回收已完成的追赶任务（事件循环顶部）：逐条收尾并重放挂起请求。
///
/// sem-79：追赶期间保留写门占用的分区写（[`crate::write_gate::WriteGate::suspend`]）
/// 在此恢复——`resume` 让随后的 `drain_write_gate` 从该分区队首取回挂起请求
/// （先于追赶期间到达的后到写），恢复分区写 FIFO。未挂起（或非写触发）时为
/// no-op。恢复放在状态侧收尾之后：结果已登记，重放即可消费（`take_synced`）。
pub(crate) async fn finish_standby_sync(
  state: &mut ShardState,
  reopen_li: &mut ReopenQueue,
  wake_li: &mut VecDeque<Envelope>,
  gate: &mut WriteGate<(Envelope, Pri)>,
) {
  while let Some((tbl, result)) = state.standby_flight.try_next().await {
    finish_sync_done(state, reopen_li, wake_li, tbl.clone(), result).await;
    gate.resume(&tbl);
  }
}

/// 收尾**一条**已完成的追赶任务（事件循环顶部批量回收与空闲唤醒
/// `WaitEvent::SyncDone` 共用：完成通知只能被消费一次，等待集取到的结果
/// 必须就地处理）。这是内联 `sync_standby_partition` 收尾的任务化搬移：
/// - 同步用 rename 替换本地 data / segment / manifest：旧 inode 的 FD 缓存与
///   路径索引必须掉，否则随后的 reopen 会继续读被替换掉的旧文件（表现为
///   `Corrupted`）。对齐 C++ reopen 前重新打开分区文件。
/// - master 已无该分区（`NotFound` / `ResourceMissing`）时按 C++
///   `clear_local_state` 语义只清本地 manifest（绝不删远端对象），错误收敛为
///   `ResourceMissing` 交给上层的 reopen 延迟队列 / 调用方。
/// - 条目已随身份切换清掉：旧身份的追赶结果对新身份无意义，丢弃；期间新
///   身份的等待请求（deferred）以当前身份重派。
pub(crate) async fn finish_sync_done(
  state: &mut ShardState,
  reopen_li: &mut ReopenQueue,
  wake_li: &mut VecDeque<Envelope>,
  tbl: TableIdent,
  result: Result<()>,
) {
  state.standby_flight.inflight.remove(&tbl);
  if !state.standby_flight.entries.contains_key(&tbl) {
    respawn_deferred(state, &tbl);
    return;
  }
  // 状态侧效果（条目仍在 = 当前身份的追赶）。
  state.stores.backend().evict_table(&tbl);
  let mapped = match result {
    Ok(()) => Ok(()),
    Err(KvError::NotFound | KvError::ResourceMissing) => {
      warn!(
        target: "fkv_shard",
        "standby partition absent on master, drop local manifest tbl={tbl}"
      );
      match state.stores.backend().drop_manifest(&tbl).await {
        Ok(()) => Err(KvError::ResourceMissing),
        Err(err) => Err(err),
      }
    }
    Err(err) => Err(err),
  };
  // 登记结果并重放 waiter：打开路径在重放时消费（`take_synced`）。
  if let Some(entry) = state.standby_flight.entries.get_mut(&tbl) {
    entry.result = Some(mapped);
    wake_li.extend(take(&mut entry.waiter_li));
  }
  // 追赶结果就绪，同表 auto-reopen driver 的到期时间提前到「现在」（io-35）：
  // driver 无 envelope，靠 reopen 到期轮询来消费结果；不提前就要白等一整轮
  // `retry_delay()`（生产默认 `pending_time` = 10s），而内联版与 C++ 的
  // `CompleteJob → FinishIo` 都是完成即唤醒。无同表条目时为 no-op。
  reopen_li.bump_due(&tbl, Instant::now());
}

/// 远端 / 预备：Cloud cache 预热与 standby 远端分区列举。
pub(crate) async fn dispatch_standby(state: &mut ShardState, req: &Request) -> Result<Response> {
  match req {
    Request::Prewarm { tbl } => {
      // 仅 Cloud 后端能预热；Local / Mem / Standby 在 `StoreBackend::prewarm_table`
      // 内显式返回 `InvalidArgs`（不静默成功）。预热只写本地 cache，不碰
      // Store 的表状态，因此借 `&Store` 即可。
      let stats = state.stores.backend().inner().prewarm_table(tbl).await?;
      // 显式 Prewarm 请求必须把本轮失败回传给调用方（API 注释承诺
      // `Err(KvError)`）；后台 crond 走 `prewarm_tables`，仍以统计报告为准。
      match stats.completion_reason {
        CompletionReason::Success => Ok(Response::Prewarm),
        CompletionReason::DiskFull => Err(KvError::OutOfSpace),
        CompletionReason::Shutdown => Err(KvError::NotRunning),
        CompletionReason::ListingError | CompletionReason::DownloadError => Err(KvError::CloudErr),
      }
    }
    Request::ListStandbyPartition => {
      // 非 standby 模式没有 master 来源，显式拒绝；不走「返回空列表」的静默路径。
      // 观察类控制请求，仍同步执行（调用方有 deadline 兜底）。
      let Some(service) = state.standby.clone() else {
        return Err(KvError::InvalidArgs);
      };
      Ok(Response::ListStandbyPartition(
        service.list_remote_partition_li().await?,
      ))
    }
    _ => Err(KvError::InvalidArgs),
  }
}
