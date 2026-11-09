//! 写门排空与写路径派发：写门准入、Put 组准备、任务化写派发与队列清理。

use std::{sync::Arc, time::Instant};

use fkv_api::{ArchiveReply, BranchReply, Request, Response};
use fkv_btree::BatchEntry;
use fkv_error::{KvError, Result};
use fkv_metrics::{Op, ShardMetric};
use fkv_types::TableIdent;
use log::warn;

use super::{
  flight::WriteFlight,
  merge::merge_put_batch_li,
  spawn::{spawn_maintain_task, spawn_prewarm_task, spawn_put_group, spawn_put_large_task},
};
use crate::{
  dispatch::{EnvelopeOutcome, admit, prepare_table, process_envelope},
  lifecycle::{DsShardState, ShardState},
  metrics::{op_of, record_request},
  read::{ReadFlight, finish_read_li, finish_read_partition},
  reopen::ReopenQueue,
  sender::{Envelope, Pri, pri_of},
  standby::{SyncAdmit, admit_standby_sync, discard_synced, standby_sync_due},
  write_gate::WriteGate,
};

/// 写队列被 `Drop` / `DetachShard` / `Shutdown` 清除时，排队请求统一回
/// `NotRunning`（对齐 C++ 清理 `pending_queues_`）。
pub(crate) fn fail_pending(pending: impl IntoIterator<Item = Envelope>) {
  for envelope in pending {
    envelope.send(Err(KvError::NotRunning));
  }
}

/// 控制请求完成后，排队写是否已因身份切换而失效。
///
/// 旧身份下排队的 `FileGc` / `CleanExpired` 若在新 attach（Fresh）后执行，
/// 会作用到新 branch / term 的盘面；detach 成功后同理。
/// 被取消 / 超时提前回 `NotRunning` 的 detach 没有驱逐任何分区状态，
/// 不清队以免误伤其它分区已排队的写；旧 term attach 导致的
/// `Attached -> Expired` 也不清队（排队写会在 drain 时按 `ExpiredTerm` 拒绝）。
pub(crate) fn write_queue_stale(
  was_attach: bool,
  was_detach: bool,
  before: DsShardState,
  after: DsShardState,
) -> bool {
  (was_detach && after != before)
    || (was_attach && before != DsShardState::Attached && after == DsShardState::Attached)
}

/// 写队列推进上下文：事件循环交给写路径的状态切片。drain / 任务派发 / 内联
/// 执行共用，避免逐函数复制七个参数。
pub(crate) struct DrainCx<'a> {
  pub(crate) state: &'a mut ShardState,
  pub(crate) metrics: &'a Arc<ShardMetric>,
  pub(crate) reopen_li: &'a mut ReopenQueue,
  pub(crate) read_flight: &'a mut ReadFlight,
  pub(crate) write_flight: &'a mut WriteFlight,
  pub(crate) gate: &'a mut WriteGate<(Envelope, Pri)>,
  pub(crate) oom_retry_times: u32,
}

/// 执行一条已从写队列取出的非 Put 写请求；`Drop` 结束后丢弃该分区槽位与排队请求。
async fn run_gated_write(cx: &mut DrainCx<'_>, tbl: TableIdent, envelope: Envelope) -> bool {
  // 这类写（Drop / DeleteBranch / ReopenArchive）要独占分区 Store。主循环
  // 可能带着读窗口（预算退出）到达这里，先 drain 在飞读，保持「写先等读」
  // 不变量；否则 `store_mut` 会拿到 `Busy` 并被当成请求错误回执（io-08）。
  finish_read_li(cx.state, cx.metrics, cx.reopen_li, cx.read_flight).await;
  let DrainCx {
    state,
    metrics,
    reopen_li,
    gate,
    oom_retry_times,
    ..
  } = cx;
  let drop_table = matches!(envelope.req, Request::Drop { .. });
  let stop = match process_envelope(state, metrics, reopen_li, envelope, *oom_retry_times).await {
    // standby 追赶在飞（io-33/sem-79）：把请求放回该分区队首并保留写门占用
    // （`suspend`），追赶完成后 `gate.resume` 唤醒重放——重放先于后到的
    // 同分区写，保持 FIFO。不做 finish / 槽位驱逐。
    EnvelopeOutcome::ParkSync(sync_tbl, envelope) => {
      let pri = pri_of(&envelope.req);
      gate.suspend(sync_tbl, vec![(envelope, pri)]);
      return false;
    }
    EnvelopeOutcome::Stop => true,
    EnvelopeOutcome::Done => false,
  };
  gate.finish(&tbl);
  // 只有真正删空（表已不在 Store 里）才丢槽位：失败 / 被取消的 Drop 不影响分区。
  if drop_table && !state.stores.is_open(&tbl) {
    state.stores.evict_table(&tbl);
    fail_pending(
      gate
        .remove_pending(&tbl)
        .into_iter()
        .map(|(envelope, _)| envelope),
    );
  }
  stop
}

/// 排空写队列里所有可启动条目：同分区按 FIFO 续跑，跨分区按 ready 顺序推进。
///
/// 高优先 `Put` 组取到就派发成独立 compio task：任务独占该分区 Store，跨分区
/// 写因此真并发；同分区仍由 `WriteGate::running` 串行。长耗时的维护 / 归档 /
/// 分支 / 大值写同样任务化（io-05）：准入留在串行路径，执行体 spawn 后主循环
/// 立即回到顶部，出队 / 读回执 / 写回收不停摆。`Drop` / `DeleteBranch` /
/// `ReopenArchive` 需要在完成时改事件循环状态或组合后端操作，仍在原地执行。
/// 低优先条目一次只跑一条，回到主循环让已就绪的高优先请求插队（eng-34）；
/// 剩余的写条目由循环顶部的 `gate.has_ready()` 继续驱动，不会滞留。
/// 返回 `true` 表示遇到 `Shutdown`。
pub(crate) async fn drain_write_gate(cx: &mut DrainCx<'_>) -> bool {
  // 一轮最多尝试当前全部就绪分区；目标分区仍有在飞读的写会放回队尾让路，
  // 连续让路达到就绪分区数即说明剩下的都只能等读，交给下一轮。
  let ready_num = cx.gate.ready_len();
  let mut deferred = 0usize;
  while let Some((tbl, (envelope, pri))) = cx.gate.take_next() {
    // 只有真正需要目标分区 Store 的写才等该分区读：Prewarm 只依赖共享后端。
    let needs_store = (pri == Pri::High && put_term(&envelope.req).is_some())
      || (is_spawnable_write(&envelope.req) && !matches!(envelope.req, Request::Prewarm { .. }));
    if needs_store && cx.read_flight.partition_len(&tbl) > 0 {
      // 目标分区还有在飞读：先让其它就绪分区推进，避免一条慢 scan 把排在
      // 它后面的跨分区写一起拖住（全量屏障不能只换成队头阻塞）。
      cx.gate.unstart(tbl, (envelope, pri));
      deferred += 1;
      if deferred >= ready_num {
        break;
      }
      continue;
    }
    if pri == Pri::High
      && let Some(_term) = put_term(&envelope.req)
    {
      // 一次只执行一个写请求：对齐 C++ `BatchWriteTask`（每个 BatchWriteRequest
      // 独立 Apply，各自 SyncData + AppendManifest）。Rust 侧此前把同分区连续
      // Put 合并成一次 Store::put 共享一次 fsync，那会让多个请求的 ack 互相
      // 耦合（延迟与组大小相关），属于 C++ 没有的机制，这里去掉。
      let envelope_li = vec![envelope];
      // 写任务要独占目标分区 Store（`Rc` 唯一）：只排空该分区的在飞读，
      // 其它分区的读继续滑动（此前是全 shard `finish_read_li`）。
      finish_read_partition(cx.state, cx.metrics, cx.reopen_li, cx.read_flight, &tbl).await;
      match prepare_put_group(cx.state, cx.metrics, envelope_li).await {
        PutPrep::Spawn(live_li, merged) => match cx.state.stores.take(&tbl) {
          Ok(store) => cx.write_flight.push(spawn_put_group(
            store,
            tbl,
            live_li,
            merged,
            cx.metrics.clone(),
            cx.oom_retry_times,
          )),
          Err(err) => {
            for envelope in live_li {
              record_request(cx.metrics, Some(Op::Put), Instant::now(), &Err(err));
              envelope.send(Err(err));
            }
            cx.gate.finish(&tbl);
          }
        },
        // 已回执（取消 / 超时 / 准入失败）或没有可执行请求。
        PutPrep::Done => cx.gate.finish(&tbl),
        // standby 追赶在飞（io-33/sem-79）：整组放回该分区队首并保留写门占用，
        // 追赶完成后 `gate.resume` 唤醒重放（先于后到的同分区写）。Put 组只在
        // 全组高优先时合并（见上方 `take_pending_while` 的谓词），故统一 High。
        PutPrep::Suspended(live_li) => cx
          .gate
          .suspend(tbl, live_li.into_iter().map(|e| (e, Pri::High)).collect()),
      }
      continue;
    }
    // 可任务化写请求：准入串行、执行体派发为独立任务，主循环不等它完成
    // （io-05）；准入失败也统一回执并释放写门额度。
    if is_spawnable_write(&envelope.req) {
      run_gated_spawn(cx, &tbl, envelope).await;
      if pri == Pri::Low {
        break;
      }
      continue;
    }
    if run_gated_write(cx, tbl, envelope).await {
      return true;
    }
    if pri == Pri::Low {
      break;
    }
  }
  false
}

fn put_term(req: &Request) -> Option<Option<u64>> {
  match req {
    Request::Put { term, .. } => Some(*term),
    _ => None,
  }
}

/// Put 组准入结果。
enum PutPrep {
  /// 可以派发写任务：活请求清单 + 合并后的批次。
  Spawn(Vec<Envelope>, Vec<BatchEntry>),
  /// 请求已回执（取消 / 超时 / 准入失败）或没有可执行请求：调用方只需
  /// `gate.finish`。
  Done,
  /// standby 追赶在飞：调用方把活请求放回分区队首并保留写门占用
  /// （`gate.suspend`），追赶完成后 `gate.resume` 唤醒重放。
  Suspended(Vec<Envelope>),
}

/// 准入一条 Put 组：过滤取消 / 超时，串行打开目标表并校验 term。
///
/// 返回 [`PutPrep`] 标明后续动作；这里不触碰 Store，只做 `&mut ShardState`
/// 的串行准备。
async fn prepare_put_group(
  state: &mut ShardState,
  metrics: &ShardMetric,
  envelope_li: Vec<Envelope>,
) -> PutPrep {
  let started = Instant::now();
  let mut live_li = Vec::with_capacity(envelope_li.len());
  for envelope in envelope_li {
    if envelope.canceled() {
      // 排队 / 挂起重放期间调用方已放弃：不执行；若它是追赶挂起重放的请求
      // （io-34），同时丢弃该表的已完成追赶结果，不留陈旧快照给更晚的首访问。
      if let Some(tbl) = envelope.req.table() {
        discard_synced(state, tbl);
      }
      envelope.send(Err(KvError::NotRunning));
    } else if envelope.expired() {
      // 排队 / 等待 envelope_li commit 期间过 deadline：不参与合并，直接回 Timeout。
      warn!(
        target: "fkv_shard",
        "request deadline expired shard_id={} op=put",
        metrics.shard_id()
      );
      // 同 canceled 分支：追赶结果已过期，丢弃而非留给更晚的首访问消费。
      if let Some(tbl) = envelope.req.table() {
        discard_synced(state, tbl);
      }
      record_request(metrics, Some(Op::Put), started, &Err(KvError::Timeout));
      envelope.send(Err(KvError::Timeout));
    } else {
      live_li.push(envelope);
    }
  }
  if live_li.is_empty() {
    return PutPrep::Done;
  }

  let term = put_term(&live_li[0].req).unwrap_or(None);
  let mut result = admit(state, &live_li[0].req);
  // standby 首写追赶任务化（io-33）：整组同表，一次追赶覆盖整组（与内联时
  // 一次 `ensure_table_ready` 覆盖整组一致）。sem-79：不在追赶期间释放该分区
  // 写门占用——整组原地保留在队首（`gate.suspend`），追赶完成后 `gate.resume`
  // 让重放的先到写先执行；否则后到同分区写会先被取出并越过它（FIFO 反转）。
  if result.is_ok()
    && let Some(tbl) = standby_sync_due(state, &live_li[0].req)
    && matches!(admit_standby_sync(state, &tbl), SyncAdmit::Parked)
  {
    return PutPrep::Suspended(live_li);
  }
  if result.is_ok() {
    result = prepare_table(state, &live_li[0].req).await;
  }
  if result.is_ok()
    && let Some(term) = term
    && state.term != Some(term)
  {
    // 与单条 Put 路径对齐：stale term 之后整个 shard 进入 Expired 快速失败。
    state.set_lifecycle(DsShardState::Expired);
    result = Err(KvError::ExpiredTerm);
  }
  if let Err(err) = result {
    for envelope in live_li {
      record_request(metrics, Some(Op::Put), started, &Err(err));
      envelope.send(Err(err));
    }
    return PutPrep::Done;
  }
  let merged = merge_put_batch_li(&mut live_li);
  PutPrep::Spawn(live_li, merged)
}

/// 可任务化的写类请求：执行体只依赖独占 Store（Prewarm 只依赖共享后端），
/// 不触碰事件循环状态，准入后可 spawn 成独立 compio task（io-05）。
///
/// 不在列：`Put`（走组派发）、`Drop`（完成后要按结果丢槽位 / 清排队，见
/// `run_gated_write`）、`DeleteBranch`（Store 操作后还要清 term 文件，组合
/// 了后端操作）、`ReopenArchive`（会改表状态且 StandbyReplica 下先追赶）。
fn is_spawnable_write(req: &Request) -> bool {
  matches!(
    req,
    Request::PutLarge { .. }
      | Request::FileGc { .. }
      | Request::LocalGc { .. }
      | Request::Compact { .. }
      | Request::Truncate { .. }
      | Request::CleanExpired { .. }
      | Request::Prewarm { .. }
      | Request::CreateArchive { .. }
      | Request::DeleteArchive { .. }
      | Request::CreateBranch { .. }
  )
}

/// 准入失败（取消 / 超时 / 生命周期 / 打开表 / 类型校验）统一收口：回执、
/// 计数并释放写门额度，请求不执行。
async fn reply_gated_reject(
  metrics: &ShardMetric,
  gate: &mut WriteGate<(Envelope, Pri)>,
  tbl: &TableIdent,
  envelope: Envelope,
  started: Instant,
  result: Result<Response>,
) {
  record_request(metrics, op_of(&envelope.req), started, &result);
  envelope.send(result);
  gate.finish(tbl);
}

/// 准入并派发一条可任务化写请求（io-05）。调用方已用 [`is_spawnable_write`]
/// 过滤，本函数必然消费该请求。
///
/// 准入（取消 / 超时 / 生命周期 / 打开表 / 类型特定检查）留在事件循环串行
/// 执行；执行体 spawn 成独立 compio task 后本函数立即返回，主循环回到顶部
/// 继续出队 / 读回执 / 写任务回收。同分区 FIFO 由 `WriteGate::running` 保持
/// （额度在 `finish_write` 释放）。
async fn run_gated_spawn(cx: &mut DrainCx<'_>, tbl: &TableIdent, envelope: Envelope) {
  let DrainCx {
    state,
    metrics,
    reopen_li,
    read_flight,
    write_flight,
    gate,
    oom_retry_times,
  } = cx;
  let (metrics, oom_retry_times) = (*metrics, *oom_retry_times);
  let started = Instant::now();
  // 与 `process_envelope` 相同的开始前检查：取消 / 超时的请求不白跑。若是
  // 追赶挂起重放的请求（io-34），同时丢弃该表的已完成追赶结果，与读批
  // （`submit_read_li`）和串行路径（`process_envelope`）逐行同型。
  let early: Option<Result<Response>> = if envelope.canceled() {
    if let Some(tbl) = envelope.req.table() {
      discard_synced(state, tbl);
    }
    Some(Err(KvError::NotRunning))
  } else if envelope.expired() {
    warn!(
      target: "fkv_shard",
      "request deadline expired shard_id={} op={}",
      metrics.shard_id(),
      op_of(&envelope.req).map_or("unknown", Op::as_str)
    );
    if let Some(tbl) = envelope.req.table() {
      discard_synced(state, tbl);
    }
    Some(Err(KvError::Timeout))
  } else if let Err(err) = admit(state, &envelope.req) {
    Some(Err(err))
  } else {
    None
  };
  if let Some(result) = early {
    reply_gated_reject(metrics, gate, tbl, envelope, started, result).await;
    return;
  }
  // standby 首访问追赶任务化（io-33）：请求挂起等追赶完成。sem-79：挂起时
  // 保留该分区写门占用——请求放回队首（`gate.suspend`），追赶完成后
  // `gate.resume` 唤醒重放，先于后到的同分区写（不 `finish`，否则重放会
  // 撞上自己的占用且让后到写插队）。
  if let Some(sync_tbl) = standby_sync_due(state, &envelope.req)
    && matches!(admit_standby_sync(state, &sync_tbl), SyncAdmit::Parked)
  {
    let pri = pri_of(&envelope.req);
    gate.suspend(sync_tbl, vec![(envelope, pri)]);
    return;
  }
  // 打开表 / 类型检查（`ensure_slot`）与执行体都要目标分区 `&mut Store`：
  // 先只排空该分区的在飞读（Prewarm 只依赖共享后端，不在此列）。其它分区
  // 的读由 `drain_write_gate` 的让路逻辑保护，不会被这里的等待拖住。
  if !matches!(envelope.req, Request::Prewarm { .. }) {
    finish_read_partition(state, metrics, reopen_li, read_flight, tbl).await;
  }
  // 串行准入：打开目标表（manifest replay 只发生在串行路径）。
  if let Err(err) = prepare_table(state, &envelope.req).await {
    reply_gated_reject(metrics, gate, tbl, envelope, started, Err(err)).await;
    return;
  }
  // 类型特定准入检查（与各内联分发保持同一判定顺序：admit → prepare → 校验）。
  let ready: Option<Result<Response>> = match &envelope.req {
    // Compact 位宽只取 shard 配置：请求携带不符位宽直接拒绝（防删活文件）。
    Request::Compact {
      pages_per_file_shift,
      ..
    } if *pages_per_file_shift != state.pages_per_file_shift => Some(Err(KvError::InvalidArgs)),
    // PutLarge term fencing：stale term 之后整个 shard 进入 Expired 快速失败。
    Request::PutLarge { term, .. } => {
      if let Some(term) = *term
        && state.term != Some(term)
      {
        state.set_lifecycle(DsShardState::Expired);
        Some(Err(KvError::ExpiredTerm))
      } else {
        None
      }
    }
    // 空分区归档 / 建分支：对齐 C++ 成功无操作，直接回执，无需派发任务。
    Request::CreateArchive { .. } if state.stores.store(tbl).is_err() => {
      Some(Ok(Response::CreateArchive(ArchiveReply {
        name: String::new(),
        bytes: 0,
        removed_num: 0,
      })))
    }
    Request::CreateBranch { .. } if state.stores.store(tbl).is_err() => {
      Some(Ok(Response::CreateBranch(BranchReply {
        name: String::new(),
        bytes: 0,
      })))
    }
    // DeleteArchive 只删归档文件：确保分区槽位承载 branch / manifest 身份。
    Request::DeleteArchive { .. } => match state.ensure_slot(tbl) {
      Ok(()) => None,
      Err(err) => Some(Err(err)),
    },
    _ => None,
  };
  if let Some(result) = ready {
    reply_gated_reject(metrics, gate, tbl, envelope, started, result).await;
    return;
  }
  // Prewarm 只依赖共享后端：不占分区 Store，也不必 drain 在飞读。
  if matches!(envelope.req, Request::Prewarm { .. }) {
    let backend = state.stores.backend().inner().clone();
    write_flight.push(spawn_prewarm_task(
      backend,
      tbl.clone(),
      envelope,
      metrics.clone(),
    ));
    return;
  }
  // 目标分区已在上面的 `finish_read_partition` 排空：`Rc` 唯一可直接取走。
  let taken = state.stores.take(tbl);
  match taken {
    Ok(store) => {
      let task = if matches!(envelope.req, Request::PutLarge { .. }) {
        // fixed buffer 空闲则借给任务（独占）；被大值读 / 其它大值写借走时
        // 本次走普通大值写路径，语义一致。
        let buf = state.fixed_buffer.take();
        spawn_put_large_task(
          store,
          tbl.clone(),
          envelope,
          buf,
          metrics.clone(),
          oom_retry_times,
        )
      } else {
        spawn_maintain_task(
          store,
          tbl.clone(),
          envelope,
          state.pages_per_file_shift,
          metrics.clone(),
          oom_retry_times,
        )
      };
      write_flight.push(task);
    }
    Err(err) => {
      reply_gated_reject(metrics, gate, tbl, envelope, started, Err(err)).await;
    }
  }
}
