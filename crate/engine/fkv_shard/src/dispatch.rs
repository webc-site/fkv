//! 请求分发：准入、表打开、`Envelope` 执行与各 `Request` 变体路由。

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use fkv_api::{ArchiveReply, BranchReply, CleanExpiredReply, Request, RequestKind, Response};
use fkv_error::{KvError, Result};
use fkv_io::{PageIo, TermIo};
use fkv_metrics::{Op, ShardMetric};
use fkv_types::{InstallOutcome, MAIN_BRANCH_NAME, TableIdent, normalize_branch_name};
use log::{info, warn};

use crate::{
  lifecycle::{AttachKind, DsShardState, ShardState},
  maintenance::{maintain_after_write, maintain_once},
  metrics::{op_of, record_request, sync_cache_metrics_for, sync_cache_metrics_or_full},
  partition::ShardStore,
  read::dispatch_read,
  reopen::ReopenQueue,
  sender::Envelope,
  standby::{
    SyncAdmit, admit_standby_sync, discard_synced, dispatch_standby, standby_sync_due, take_synced,
  },
  write::put_large_body,
};

/// 打开目标表：读路径唯一需要 `&mut Store` 的动作，只允许在串行准入路径调用。
///
/// 幂等 single-flight：`Store::is_open` 为权威，已打开时只补登记不重复 replay；
/// 同一批读请求里对同一张表只会在第一次准入时真正打开。
pub(crate) async fn ensure_table_ready(state: &mut ShardState, tbl: &TableIdent) -> Result<()> {
  if state.stores.is_open(tbl) {
    state.table_open.mark(tbl, state.term, &state.branch);
    state.stores.touch(tbl);
    return Ok(());
  }
  // 写任务正持有该分区 Store：不能新建（会与在飞写分叉），交回调用方等待。
  if state.stores.is_writing(tbl) {
    return Err(KvError::Busy);
  }
  let branch = state.branch.clone();
  let term = state.term.unwrap_or(0);
  let reopen_on_missing = state.reopen_on_missing;
  // `StandbyReplica`：懒开前先追一次 master。本地后端（`CompioStore`）的
  // `read_manifest` 只读本地文件，没有 Cloud 的回源护栏；不先追 master 会
  // replay 上次 rsync 留下的旧 manifest（副本重启 / 换主后静默读旧 root）。
  // 这是**有意超越 C++ 基线**：C++ 懒开路径 `FindRoot` 也是纯本地 replay，
  // 新鲜度依赖控制面发 Reopen；fkv 在副本重启后无进程内收敛机制，故在懒开
  // 入口补一次追赶。失败 `ResourceMissing` 交给上层挂等待重排，不能装空表。
  // 追赶已任务化（io-33）：准入路径派发追赶并挂起触发请求，完成后经 wake_li
  // 重放到这里消费结果——事件循环绝不在本函数内联 await rsync。
  if branch == MAIN_BRANCH_NAME && state.standby.is_some() {
    match take_synced(state, tbl) {
      Some(result) => result?,
      // 没有可消费的结果 = 追赶在飞：触发请求本应在准入处挂起（`standby_sync_due`
      // 与本条件镜像），这里是防御路径——回 Busy 交调用方重试。
      None => {
        warn!(
          target: "fkv_shard",
          "standby sync in flight, defer open tbl={tbl}"
        );
        return Err(KvError::Busy);
      }
    }
  }
  // 新分区 Store 采纳当前 attach 身份后加载该表。
  let result = {
    let store = state.stores.ensure_store(tbl, &branch, term)?;
    if branch == MAIN_BRANCH_NAME {
      match store.reopen(tbl).await {
        Ok(()) => Ok(()),
        // 无 manifest 的首次访问：Local / Mem 下装一个空表状态（首次写时惰性创建，
        // 之后 is_open 为 true），对齐 C++ 懒创建语义。Cloud / StandbyReplica 下
        // 不能懒创建：manifest 只是暂不可见（远端已有数据），装成空表会静默丢读，
        // 且 manifest 之后可见也不会再读该表；把 `ResourceMissing` 交给上层挂等待重排
        // （对齐 C++ `auto_reopen`）。
        Err(KvError::ResourceMissing) if !reopen_on_missing => {
          store.open_empty(tbl);
          Ok(())
        }
        Err(err) => Err(err),
      }
    } else {
      // attach term 直接交给 store 安装 replay：branch 区间必须记当前 identity，
      // 否则 attach 后写出的 `_T` 对象在远端按 `_0` 找不到。
      store.open_branch(tbl, &branch, term).await
    }
  };
  result?;
  // term 与 Store 不一致时才重设：`set_term` 会清共享 IO 缓存，不能每表打开都做。
  let store = state.stores.ensure_store(tbl, &branch, term)?;
  if store.term() != term {
    store.set_term(term);
  }
  state.table_open.mark(tbl, state.term, &branch);
  state.stores.touch(tbl);
  Ok(())
}
/// 毫秒墙钟，用于 TTL 判定。
fn wall_clock_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}
/// 只读请求：只借 `&Store`，可提交为独立 compio task 并发推进（eng-32b）。
///
/// 变体清单见 `fkv_api` 的 `RequestKind::Read` 与 `request_meta!` 变体表；
/// 这里只查询生成的分类。`GetLarge` 也是 `Read`（只借 `&Store`，fixed buffer
/// 由读侧单飞借出，见 `submit_read_li` / `window_admit`）。
pub(crate) fn is_read_op(req: &Request) -> bool {
  req.kind() == RequestKind::Read
}

/// 写类请求：进 per-partition FIFO 写队列，同一分区严格串行（对齐 C++
/// `PendingWriteQueue`）。
///
/// 变体清单（含 `Prewarm` 为文件名串行化进队列的特例）见 `fkv_api` 的
/// `RequestKind::Write` 与 `request_meta!` 变体表；这里只查询生成的分类。
pub(crate) fn is_write_op(req: &Request) -> bool {
  req.kind() == RequestKind::Write
}

/// `ResourceMissing` 时挂等待 manifest 的读请求（对齐 C++ `AutoReopenRetry`）。
/// 变体清单见 `Request::auto_reopen_read` 的生成属性。
pub(crate) fn is_auto_reopen_read(req: &Request) -> bool {
  req.auto_reopen_read()
}
/// 准入检查：attach 状态与 term 失效。对齐 C++ `AdmitRequest`：
/// 整个 data store shard 一旦 ExpiredTerm，除 attach/detach/shutdown 外全部快速失败。
pub(crate) fn admit(state: &ShardState, req: &Request) -> Result<()> {
  if req.admit_bypass() {
    return Ok(());
  }
  match state.lifecycle {
    DsShardState::Attached => Ok(()),
    DsShardState::Detached => Err(KvError::NotRunning),
    DsShardState::Expired => Err(KvError::ExpiredTerm),
  }
}

/// 不依赖「表已打开」的请求（清单与语义见 `Request::skips_table_open`）。
///
/// 读窗口的准入（`window_admit`）与读任务提交（`submit_read_li`）也复用它，
/// 否则 `ListArchiveTags` / `ListObject` 会先 `ensure_table_ready` 再执行，
/// 在 Cloud 上无 manifest 的分区会被误判为 `ResourceMissing`。
pub(crate) fn skips_table_open(req: &Request) -> bool {
  req.skips_table_open()
}

pub(crate) async fn prepare_table(state: &mut ShardState, req: &Request) -> Result<()> {
  // `Reopen` / `ReopenExternal` 自己执行 auto_reopen：main branch 下无需先
  // ensure 再 reopen（否则首次访问同一 manifest 读两次）。
  let skip_ensure =
    skips_table_open(req) || (req.opens_manifest() && state.branch == MAIN_BRANCH_NAME);
  if !skip_ensure && let Some(tbl) = req.table() {
    // CreateArchive / CreateBranch / DeleteBranch 对“尚无 manifest 的空分区”
    // 是对齐 C++ 的成功无操作，不是错误：尝试打开（远端已有 manifest 时仍能
    // 正常操作），仅 `ResourceMissing` 被忽略。
    let allow_missing = req.allow_missing();
    match ensure_table_ready(state, tbl).await {
      Ok(()) => {}
      Err(KvError::ResourceMissing) if allow_missing => {}
      // Cloud main 的首写：远端没有 manifest 等价于空表（对齐 C++
      // `MakeCowRoot` 的 NotFound → stub）。读请求仍交给 auto-reopen park，
      // StandbyReplica / branch 不做隐式创建。
      Err(KvError::ResourceMissing)
        if state.reopen_on_missing
          && state.standby.is_none()
          && state.branch == MAIN_BRANCH_NAME
          && !is_read_op(req)
          && is_write_op(req) =>
      {
        let branch = state.branch.clone();
        let term = state.term.unwrap_or(0);
        state
          .stores
          .ensure_store(tbl, &branch, term)?
          .open_empty(tbl);
        state.table_open.mark(tbl, state.term, &branch);
        state.stores.touch(tbl);
      }
      Err(err) => return Err(err),
    }
  }
  Ok(())
}

/// 串行请求入口。返回值统一经过 [`ShardState::observe_task_err`]：任一请求
/// 收到 `ExpiredTerm`（写请求 term 校验、Drop 的 term 文件复查、云 manifest
/// 采纳栅栏等）都把整 shard 置 `Expired`，对齐 C++ `FinishTask` 的
/// `MarkShardExpired`。
pub(crate) async fn handle(state: &mut ShardState, req: &Request) -> Result<Response> {
  let result = handle_inner(state, req).await;
  // attach / detach 有自己的状态转换规则（对齐 C++ 对 attach 任务不调
  // MarkShardExpired）：`admit_attach` 在合法旧 term 场景已置 Expired；
  // Detached 下 term 文件 CAS 拒绝只表示这次 attach 非法，不能把状态钉成
  // Expired（否则后续更高 term 的 attach 会被 `same_owner` 判定误拒）。
  if !matches!(
    req,
    Request::AttachShard { .. } | Request::DetachShard { .. }
  ) {
    state.observe_task_err(&result);
  }
  result
}

async fn handle_inner(state: &mut ShardState, req: &Request) -> Result<Response> {
  admit(state, req)?;
  prepare_table(state, req).await?;
  dispatch(state, req).await
}
/// 一条非只读请求的处理结果（见 [`process_envelope`]）。
pub(crate) enum EnvelopeOutcome {
  /// 已处理完毕（含已回执）；事件循环继续。
  Done,
  /// 收到 `Shutdown`，事件循环应退出。
  Stop,
  /// standby 追赶在飞：请求需挂起等追赶完成。调用方决定挂进追赶 waiter
  /// （非写队列路径，经 `park_sync_waiter`）还是保留写门占用
  /// （写队列路径，经 `WriteGate::suspend`）——sem-79 后者保证重放先于后到写。
  ParkSync(TableIdent, Envelope),
}

/// 处理一条非只读请求：执行、OOM 重试、写后维护与回执。
/// 返回 [`EnvelopeOutcome`] 标明收尾类别（`Stop` = 事件循环应退出）。
pub(crate) async fn process_envelope(
  state: &mut ShardState,
  metrics: &ShardMetric,
  reopen_li: &mut ReopenQueue,
  envelope: Envelope,
  oom_retry_times: u32,
) -> EnvelopeOutcome {
  let shutdown = envelope.req.is_shutdown();
  let op = op_of(&envelope.req);
  let started = Instant::now();
  // 调用方在排队期间放弃等待：请求尚未开始，直接回执退出，避免白跑一次写。
  // `Shutdown` 例外（`shutdown()` 必须拿到回执并真正停机）。
  // 若它是追赶挂起重放的请求（io-33），同时丢弃该表的已完成追赶结果，
  // 不留给更晚的首访问当陈旧结果。
  if !shutdown && envelope.canceled() {
    if let Some(tbl) = envelope.req.table() {
      discard_synced(state, tbl);
    }
    envelope.send(Err(KvError::NotRunning));
    return EnvelopeOutcome::Done;
  }
  // 排队期间已过 deadline：未开始就快速失败，不进入 admit / prepare_table。
  if !shutdown && envelope.expired() {
    warn!(
      target: "fkv_shard",
      "request deadline expired shard_id={} op={}",
      metrics.shard_id(),
      op.map_or("unknown", Op::as_str)
    );
    if let Some(tbl) = envelope.req.table() {
      discard_synced(state, tbl);
    }
    record_request(metrics, op, started, &Err(KvError::Timeout));
    envelope.send(Err(KvError::Timeout));
    return EnvelopeOutcome::Done;
  }
  // StandbyReplica 追赶准入（io-33）：打开路径需要追 master 时，追赶任务交
  // 独立 compio task，本请求挂起为 waiter——完成后经 wake_li 重放完整分发
  // 路径（对齐 C++ `RsyncPartition` + `WaitIo`：只挂起本任务，shard 继续
  // 服务其它请求）。生命周期不合法的请求不白跑一次 rsync，先过 admit 再判追赶。
  // 挂起的请求不回执不计 metrics（重放时会完整重跑，先记会双计）。
  // 这里只返 `ParkSync` 交调用方处置：写队列路径要保留写门占用（sem-79），
  // 不能就地挂进 waiter（否则重放经 wake_li 尾部重排会破坏分区 FIFO）。
  if admit(state, &envelope.req).is_ok()
    && let Some(tbl) = standby_sync_due(state, &envelope.req)
    && matches!(admit_standby_sync(state, &tbl), SyncAdmit::Parked)
  {
    return EnvelopeOutcome::ParkSync(tbl, envelope);
  }
  // 借用而非搬走请求：遇 `ResourceMissing` 要挂起整条 envelope（含原请求与回执）。
  let req = &envelope.req;
  // shutdown 后挂起的 reopen 立即失败（对齐 C++ Stop 路径先清
  // `pending_reopens_` 再停 shard），避免用已驱逐的状态继续重试。
  // `DetachShard` 的合法性由 `dispatch` 的 `admit_detach` 判定：只有真正
  // detach 成功才清挂起条目，非法 / 幂等 detach 不能误清。
  if shutdown {
    reopen_li.fail_all(KvError::NotRunning);
    // 追赶挂起的请求同样立即失败（否则要等线程退出的排空信号）。
    state.standby_flight.fail_stale(KvError::NotRunning);
  }
  // OutOfMem 重试：请求已在 put_at 内回滚，重试从干净 mapping 开始。
  // 由事件循环持有原请求、`handle` 只借用，重试零深拷贝（对齐 C++
  // 把原请求对象挂在任务上重跑）。
  let mut remaining = oom_retry_times;
  let result = loop {
    match handle(state, req).await {
      Ok(response) => break Ok(response),
      Err(KvError::OutOfMem) if remaining > 0 => remaining -= 1,
      Err(err) => break Err(err),
    }
  };
  // 写后维护登记：`needs_maintain` 是 O(1) 放大率预判，只有需要
  // compact/GC 或有待清理 TTL 时才在本请求回复后跑维护（对齐 C++
  // `WriteTask::UpdateMeta` 提交后抛给后台的 Compact / FileGc / CleanExpired）。
  let mut maintain_tbl = None;
  if result.is_ok()
    && req.writes_data()
    && let Some(tbl) = req.table()
    && (state.stores.needs_maintain(tbl) || state.stores.pending_ttl_len() > 0)
  {
    maintain_tbl = Some(tbl.clone());
  }
  // detach 成功：旧身份已驱逐，挂起的 reopen 立即失败（对齐 C++ Stop 路径先清
  // `pending_reopens_`）。非法 shard_id / 已 Detached 的幂等路径不走到这里。
  // 追赶挂起的请求同属旧身份（io-25）：一并失败，在飞任务的结果按「条目
  // 已清」在完成时丢弃。
  if matches!(result, Ok(Response::DetachShard)) {
    reopen_li.fail_all(KvError::NotRunning);
    state.standby_flight.fail_stale(KvError::NotRunning);
  }
  // 回执发出前先同步缓存计数：调用方拿到响应后立刻查指标即可见。
  // 串行请求只可能改动目标分区，O(1) 增量同步即可（无分区信息的控制请求
  // 退回全量对账，频率低、不影响数据面吞吐）。
  sync_cache_metrics_or_full(state, metrics, req.table());
  // 等 manifest 可见：Cloud/Standby 下的读请求挂起重排；`Reopen` / 空 tag 的
  // `ReopenArchive` 只有在同表已有 driver 时才搭 waiter，否则 `park` 立即把
  // `ResourceMissing` 交回调用方（对齐 C++ 外部 `ReopenRequest` 一次即回）。
  //
  // `ReopenArchive` 只有空 tag（= 重开活跃快照）走同一语义；非空 tag 指向确定的
  // 归档快照，缺失即「调用方要的状态不存在」，直接回 `ResourceMissing`，不能拿
  // 活跃 manifest 的可见性当作它的前置条件。
  let park = matches!(result, Err(KvError::ResourceMissing))
    && (req.active_reopen()
      || matches!(req, Request::ReopenArchive { tag, .. } if tag.is_empty())
      || (state.reopen_on_missing && is_auto_reopen_read(req)));
  let envelope = if park {
    reopen_li.park(envelope)
  } else {
    Some(envelope)
  };
  // 只有真正回执的请求才计数：被 park 的请求稍后唤醒会重跑，这里先记会双计。
  if envelope.is_some() {
    record_request(metrics, op, started, &result);
  }
  if let Some(envelope) = envelope {
    envelope.send(result);
  }
  if shutdown {
    return EnvelopeOutcome::Stop;
  }
  // 回复已发出，compaction/GC/TTL 清理不占写延迟窗口。维护看到**本 branch**
  // 更高 term 的 manifest 时回执不变，但 shard 必须置 Expired（对齐 C++
  // FinishTask）；其它 branch 的更高 term 已在 `Store::file_gc` 按 active
  // branch 过滤，不改变生命周期（sem-26）。
  if let Some(tbl) = maintain_tbl {
    let expired = maintain_after_write(state, &tbl).await;
    // 维护读写索引页也会命中 / 填充页缓存：补一次目标分区同步（O(1)），
    // 不把维护产生的增量留给下一轮全量对账。
    sync_cache_metrics_for(state, metrics, &tbl);
    if expired {
      state.observe_task_err(&Err(KvError::ExpiredTerm));
    }
  }
  EnvelopeOutcome::Done
}
/// 请求分发入口：只读请求走 `dispatch_read`；其余按 `Request` 变体路由到各
/// 子分发函数，本函数只保留准入分流与 match 跳转。
async fn dispatch(state: &mut ShardState, req: &Request) -> Result<Response> {
  if is_read_op(req) {
    let tbl = req.table().ok_or(KvError::InvalidArgs)?;
    // 归档 / 远端列举不依赖表打开，但需要一个分区 Store 承载 branch 身份。
    if skips_table_open(req) && !state.stores.has_slot(tbl) {
      state.ensure_slot(tbl)?;
    }
    return dispatch_read(state.stores.store(tbl)?, req).await;
  }
  match req {
    Request::Put { .. } => dispatch_put(state, req).await,
    Request::PutLarge { .. } => dispatch_large_put(state, req).await,
    Request::Reopen { .. } | Request::ReopenArchive { .. } | Request::ReopenExternal { .. } => {
      dispatch_reopen(state, req).await
    }
    Request::AttachShard { .. } | Request::DetachShard { .. } => dispatch_attach(state, req).await,
    Request::FileGc { .. }
    | Request::LocalGc { .. }
    | Request::Compact { .. }
    | Request::Truncate { .. }
    | Request::CleanExpired { .. } => dispatch_maintain(state, req).await,
    Request::Drop { .. } => dispatch_drop(state, req).await,
    Request::CreateArchive { .. } | Request::DeleteArchive { .. } => {
      dispatch_archive(state, req).await
    }
    Request::CreateBranch { .. } | Request::DeleteBranch { .. } => {
      dispatch_branch(state, req).await
    }
    Request::Prewarm { .. } | Request::ListStandbyPartition => dispatch_standby(state, req).await,
    Request::Shutdown => Ok(Response::Shutdown),
    // 只读请求（含 `GetLarge`）已在函数入口走 `dispatch_read`；此臂只为穷尽。
    Request::Get { .. }
    | Request::GetLarge { .. }
    | Request::Scan { .. }
    | Request::Floor { .. }
    | Request::ListArchiveTags { .. }
    | Request::ListObject { .. } => {
      let tbl = req.table().ok_or(KvError::InvalidArgs)?;
      dispatch_read(state.stores.store(tbl)?, req).await
    }
  }
}

/// 普通写：term fencing 校验通过后按分区串行写入。
async fn dispatch_put(state: &mut ShardState, req: &Request) -> Result<Response> {
  match req {
    Request::Put {
      tbl,
      batch_li,
      term,
    } => {
      if let Some(term) = *term
        && state.term != Some(term)
      {
        state.set_lifecycle(DsShardState::Expired);
        return Err(KvError::ExpiredTerm);
      }
      state.stores.store_mut(tbl)?.put(tbl, batch_li).await?;
      Ok(Response::Put)
    }
    _ => Err(KvError::InvalidArgs),
  }
}

/// 大值写（串行防御路径；正常路径任务化见 `run_gated_spawn`）：与普通写同样
/// 做 term 校验，`fixed_buffer` 可用且装得下时走固定缓冲路径。
async fn dispatch_large_put(state: &mut ShardState, req: &Request) -> Result<Response> {
  match req {
    Request::PutLarge {
      tbl,
      batch_li,
      term,
    } => {
      if let Some(term) = *term
        && state.term != Some(term)
      {
        state.set_lifecycle(DsShardState::Expired);
        return Err(KvError::ExpiredTerm);
      }
      let buf = state.fixed_buffer.as_mut();
      let store = state.stores.store_mut(tbl)?;
      put_large_body(store, tbl, batch_li, buf).await?;
      Ok(Response::Put)
    }
    _ => Err(KvError::InvalidArgs),
  }
}

/// 远端分区缺失时的内存侧清理（追赶完成路径已删本地 manifest）。
///
/// 对齐 C++ `ReopenTask` 的 `clear_local_state`：该清理对
/// `sync_err == NotFound / ResourceMissing` 不区分 `Clean()`。漏清时表虽已
/// 不在盘上，内存 `Store` 与 `table_open` 登记仍在，后续读会命中
/// `table_open.ready` 直接返回删除前的旧值。
fn clear_missing_table(state: &mut ShardState, tbl: &TableIdent) {
  state.stores.evict_table(tbl);
  state.table_open.forget(tbl);
}

/// 消费准入路径派发的 standby 追赶结果（io-33）：重放的 Reopen 族请求在
/// 这里拿到同步结果（状态侧收尾已在任务完成时做过）。
///
/// 非 StandbyReplica（无追赶服务）没有追赶步骤，直接成功（对齐内联版
/// `sync_standby_partition` 的 `standby == None` 短路）。`None` = 追赶在飞：
/// 触发请求本应在准入处挂起，这里是防御路径——回 `Busy` 交上层延迟队列 /
/// 调用方重试，不在事件循环内联等 rsync。
fn synced_result(state: &mut ShardState, tbl: &TableIdent) -> Result<()> {
  if state.standby.is_none() {
    return Ok(());
  }
  match take_synced(state, tbl) {
    Some(result) => result,
    None => {
      warn!(
        target: "fkv_shard",
        "standby sync in flight, defer reopen tbl={tbl}"
      );
      Err(KvError::Busy)
    }
  }
}

/// 打开 / 重开：普通、归档、外部快照三种入口，StandbyReplica 下先追赶 master
/// （追赶任务化见 `standby_sync_due`，重放请求经 `synced_result` 消费结果）。
async fn dispatch_reopen(state: &mut ShardState, req: &Request) -> Result<Response> {
  match req {
    Request::Reopen { tbl, clean } => {
      // StandbyReplica：先把该分区从 master 追一次再打开本地 manifest，
      // 对齐 C++ `ReopenTask::Reopen`（`RsyncPartition` + `WaitIo` 后才继续）。
      match synced_result(state, tbl) {
        Ok(()) => {}
        // C++ `ReopenTask::Reopen` 的 `clear_local_state`：master 已删除该分区
        // 时清本地状态（不以 `Clean()` 为前提）。`clean` 只决定回执：standby
        // 全局重开对「仅本地存在」的分区置位并回成功、不写 `failed_li`；
        // 非 clean 的显式 reopen / auto-reopen 清完后仍回 `ResourceMissing`，
        // 交给上层延迟队列或调用方。
        Err(err) => {
          if matches!(err, KvError::NotFound | KvError::ResourceMissing) {
            clear_missing_table(state, tbl);
            if *clean {
              return Ok(Response::Reopen);
            }
          }
          return Err(err);
        }
      }
      // main branch 下 `prepare_table` 不再预先 ensure：先建分区槽位再 reopen。
      state.ensure_slot(tbl)?;
      // 单次尝试：`ResourceMissing` 由事件循环的延迟队列重试，不在本任务内
      // `sleep`（否则整个 shard 线程被阻塞最长 `retry_times * pending_time`）。
      state.stores.store_mut(tbl)?.reopen(tbl).await?;
      state.table_open.mark(tbl, state.term, &state.branch);
      state.stores.touch(tbl);
      Ok(Response::Reopen)
    }
    Request::ReopenArchive { tbl, tag, clean } => {
      // StandbyReplica 与普通 Reopen 一样先追一次 master（同步会带上归档
      // manifest）；Cloud 下 `read_manifest_file` 会按需回源远端对象。
      match synced_result(state, tbl) {
        Ok(()) => {}
        // 与 `Request::Reopen` 同口径：master 已删该分区时同步路径已 drop
        // 本地 manifest，这里无论 clean 都清内存状态（C++ `clear_local_state`
        // 不以 `Clean()` 为前提）；clean 只决定是否收敛为成功回执
        // （standby 全局重开对带 tag 的请求同样置 clean）。
        Err(err) => {
          if matches!(err, KvError::NotFound | KvError::ResourceMissing) {
            clear_missing_table(state, tbl);
            if *clean {
              return Ok(Response::Reopen);
            }
          }
          return Err(err);
        }
      }
      // 归档重开不经过 `ensure_table_ready`（避免懒建空表），只需分区槽位。
      state.ensure_slot(tbl)?;
      // 非空 tag 缺档即便 clean 也照旧上抛：master 仍有该分区时，C++ 的
      // 「清空即成功」过于激进，fkv 保持显式失败可见（COMPAT_BASELINE
      // 「缺失快照的重开语义」）。
      state
        .stores
        .store_mut(tbl)?
        .reopen_archive(tbl, tag)
        .await?;
      state.table_open.mark(tbl, state.term, &state.branch);
      Ok(Response::Reopen)
    }
    Request::ReopenExternal { tbl } => {
      // 仅 Cloud / StandbyReplica：Local / Mem 没有远端状态可采纳。
      if !state.reopen_on_missing {
        return Err(KvError::InvalidArgs);
      }
      // StandbyReplica 先追 master；master 也没有该分区时，本地 manifest 已被
      // 同步路径删掉，这里只清内存状态并回「等待远端」，不以空表起跑。
      if state.standby.is_some() {
        match synced_result(state, tbl) {
          Ok(()) => {}
          Err(KvError::NotFound | KvError::ResourceMissing) => {
            clear_missing_table(state, tbl);
            return Ok(Response::ReopenExternal(InstallOutcome::WaitingRemote));
          }
          Err(err) => return Err(err),
        }
      }
      state.ensure_slot(tbl)?;
      let outcome = state
        .stores
        .store_mut(tbl)?
        .install_external_snapshot(tbl)
        .await?;
      if matches!(outcome, InstallOutcome::Installed) {
        state.table_open.mark(tbl, state.term, &state.branch);
      } else {
        state.table_open.forget(tbl);
      }
      Ok(Response::ReopenExternal(outcome))
    }
    _ => Err(KvError::InvalidArgs),
  }
}

/// 身份切换：attach 做 term fencing 与 Store 作废，detach 清运行期身份。
async fn dispatch_attach(state: &mut ShardState, req: &Request) -> Result<Response> {
  match req {
    Request::AttachShard {
      branch,
      shard_id,
      term,
    } => {
      // 分支名先归一到盘面 / term 文件用的规范形式（小写、仅 `[a-z0-9-]`）：
      // 不归一会让 `"MAIN"` 写出 `CURRENT_TERM_MAIN_0`、并与 `MAIN_BRANCH_NAME`
      // 的判定全面失配（sto-69）。
      let branch = normalize_branch_name(branch).ok_or(KvError::InvalidArgs)?;
      // shard 身份固定在启动配置：别的 shard id 走同一线程会共用 term 文件，
      // fencing 失效（旧主仍能写）。
      if *shard_id != state.shard_id {
        return Err(KvError::InvalidArgs);
      }
      if state.admit_attach(&branch, *shard_id, *term)? == AttachKind::Idempotent {
        // 同 shard / branch / term 重复 attach：不驱逐 table 状态，不清缓存。
        return Ok(Response::AttachShard);
      }
      info!(
        target: "fkv_shard",
        "shard attach shard_id={} branch={} term={}",
        shard_id,
        branch,
        term
      );
      // fencing token 持久化：同 term 幂等，旧 term 必须被磁盘 / 云端拒绝。
      // Local 模式同样落 `CURRENT_TERM_<branch>_<shard_id>`（与 Cloud / Mem 一致）：
      // `Request::Put { term }` 的 fencing 契约不因后端而分叉，交叉进程重新 attach
      // 时由该文件拒绝旧 term；C++ Local 不写该文件属于已知差异（sto-61c）。
      state
        .stores
        .backend()
        .upsert_term(&branch, *shard_id, *term)
        .await?;
      // 身份切换：旧分区 Store 全部作废（各自持有旧 branch/term 的 manifest 与
      // 文件身份）；共享后端只统一失效一次路径 / fd 缓存。
      state.stores.clear();
      state.stores.backend().set_file_identity(&branch, *term);
      state.set_lifecycle(DsShardState::Attached);
      state.term = Some(*term);
      state.branch = branch.into();
      // 身份变化：在飞读已在读窗口 drain，代际 +1 防止旧代结果误回执。
      state.table_open.bump();
      Ok(Response::AttachShard)
    }
    Request::DetachShard { shard_id } => {
      if state.admit_detach(*shard_id)? {
        // 驱逐全部分区 Store 与 fd/路径缓存（对齐 C++ EvictDsShard）。
        state.stores.clear();
        // 运行期身份归零（对齐 C++ `slot.term_.store(0)`）：detach 后本 shard 不再
        // 持有 term / branch，任何残留请求都先被 `NotRunning` 拒绝。旧 term 的重新
        // attach 仍由持久化 term 文件 CAS 拒绝（`claim_term`），不依赖内存状态。
        state.set_lifecycle(DsShardState::Detached);
        state.term = None;
        state.branch.clear();
        state.table_open.bump();
        info!(
          target: "fkv_shard",
          "shard detach shard_id={}",
          shard_id
        );
      }
      Ok(Response::DetachShard)
    }
    _ => Err(KvError::InvalidArgs),
  }
}

/// 可任务化写请求的执行体（io-05）：只依赖独占 `&mut ShardStore`，不触碰
/// 事件循环状态。串行分发（防御路径）与任务化路径（`run_gated_spawn` 派发的
/// 任务）共用同一实现，行为完全一致。
///
/// `shift` 是 shard 配置的布局位宽：`Compact` 请求携带值与之不符即拒绝，
/// 防止调用方传入与盘面不符的位宽让 retained file_id 换算错位（删活文件）。
///
/// 返回 `(回执, 维护是否观察到 ExpiredTerm)`：截断 / TTL 清理成功后的内联维护
/// 若发现更高 term 的 manifest，回执保持成功，但调用方须把 shard 置 `Expired`
/// （对齐 C++ `Shard::OnTaskFinished` 对任一任务 `ExpiredTerm` 调
/// `MarkShardExpired`；sem-22）。
pub(crate) async fn gated_body(
  store: &mut ShardStore,
  req: &Request,
  shift: u8,
) -> (Result<Response>, bool) {
  let result = gated_body_inner(store, req, shift).await;
  let expired = match (&result, req) {
    // 对齐 C++ Truncate / CleanExpired 走 `UpdateMeta` 触发 compact / GC：
    // 维护失败不回滚已生效的请求，只有 `ExpiredTerm` 改变生命周期。
    (Ok(_), Request::Truncate { tbl, .. } | Request::CleanExpired { tbl }) => {
      store.needs_maintain(tbl) && maintain_once(store, tbl).await
    }
    _ => false,
  };
  (result, expired)
}

/// 执行体本体：只产生请求回执，不含截断 / TTL 清理的写后维护（见 [`gated_body`]）。
async fn gated_body_inner(store: &mut ShardStore, req: &Request, shift: u8) -> Result<Response> {
  match req {
    Request::FileGc { tbl } => Ok(Response::FileGc(store.file_gc(tbl, shift).await?)),
    Request::LocalGc { tbl } => Ok(Response::LocalGc(store.local_gc(tbl, shift).await?)),
    Request::Compact {
      tbl,
      file_id,
      pages_per_file_shift,
    } if *pages_per_file_shift == shift => Ok(Response::Compact(
      store
        .compact_file(tbl, *file_id, *pages_per_file_shift)
        .await?,
    )),
    Request::Compact { .. } => Err(KvError::InvalidArgs),
    Request::Truncate { tbl, position } => {
      if position.is_empty() {
        store.truncate(tbl).await?;
      } else {
        store.truncate_from(tbl, position).await?;
      }
      Ok(Response::Truncate)
    }
    Request::CleanExpired { tbl } => {
      let stats = store.clean_expired(tbl, wall_clock_ms()).await?;
      Ok(Response::CleanExpired(CleanExpiredReply {
        deleted: stats.deleted,
        ttl_deleted: stats.ttl_deleted,
        next_expire_ts: stats.next_expire_ts,
      }))
    }
    Request::CreateArchive {
      tbl,
      tag,
      num_retained,
    } => {
      // 空 tag：自动生成定宽微秒时间戳（对齐 C++ `background_write.cpp` 空 tag 走
      // 时间戳；定宽见 `fkv_api::archive_tag`，保证 prune 的名字排序=时间排序）。
      let generated;
      let tag = if tag.is_empty() {
        generated = fkv_api::archive_tag();
        generated.as_str()
      } else {
        tag.as_str()
      };
      let info = store.create_archive(tbl, tag, *num_retained).await?;
      Ok(Response::CreateArchive(ArchiveReply {
        name: info.name,
        bytes: info.bytes,
        removed_num: info.removed_num,
      }))
    }
    Request::DeleteArchive { tbl, tag } => Ok(Response::DeleteArchive(
      store.delete_archive(tbl, tag).await?,
    )),
    Request::CreateBranch { tbl, branch } => {
      let info = store.create_branch(tbl, branch).await?;
      Ok(Response::CreateBranch(BranchReply {
        name: info.name,
        bytes: info.bytes,
      }))
    }
    _ => Err(KvError::InvalidArgs),
  }
}

/// 串行执行可任务化请求：内联维护的 `ExpiredTerm` 只改生命周期，不改回执
/// （对齐 C++ `FinishTask`；sem-22）。
async fn run_gated_body(state: &mut ShardState, req: &Request) -> Result<Response> {
  let shift = state.pages_per_file_shift;
  let tbl = req.table().ok_or(KvError::InvalidArgs)?;
  let (result, expired) = gated_body(state.stores.store_mut(tbl)?, req, shift).await;
  if expired {
    state.observe_task_err(&Err(KvError::ExpiredTerm));
  }
  result
}

/// 维护类请求：文件 GC、本地 GC、compact、截断与 TTL 清理（串行防御路径；
/// 正常路径任务化见 `run_gated_spawn`）。
async fn dispatch_maintain(state: &mut ShardState, req: &Request) -> Result<Response> {
  run_gated_body(state, req).await
}

/// 删表：先按 term 文件做 fencing，再删分区并跑一轮 FileGc。
async fn dispatch_drop(state: &mut ShardState, req: &Request) -> Result<Response> {
  match req {
    Request::Drop { tbl } => {
      // 本地活跃 manifest 固定名（`manifest_main_0`）不携带 term，旧 owner
      // 无法仅凭文件名与新 owner 隔离。Drop 前先读 term 文件确认本实例仍是
      // 当前 owner：更高 term 已接管时拒绝并标记 Expired（对齐 C++ 高 term
      // 只删自己 `manifest_<branch>_<term>` 时的 fencing 效果）。
      if let Some(term) = state.term {
        let branch = state.branch.clone();
        let current = state
          .stores
          .backend()
          .read_term(&branch, state.shard_id)
          .await?;
        if current.is_some_and(|current| current > term) {
          state.set_lifecycle(DsShardState::Expired);
          return Err(KvError::ExpiredTerm);
        }
      }
      // Drop 不依赖表已打开：`open_empty` 的失败分区 / 损坏 manifest 也要能删。
      // `store_mut` 在槽位不存在时返回 ResourceMissing，这里按需建 Store。
      let branch = state.branch.clone();
      let term = state.term.unwrap_or(0);
      let shift = state.pages_per_file_shift;
      let store = state.stores.ensure_store(tbl, &branch, term)?;
      store.drop_table(tbl).await?;
      // 对齐 C++ `BatchWriteTask::Drop` 末尾的 `TryAddFileGc`：逻辑删除后立刻
      // 跑一轮 FileGc。这里同步执行（C++ 是低优先排队），语义相同：只回收
      // 不再被任何 active / branch / archive manifest 引用的文件。GC 失败只
      // 告警，不回滚已完成的 Drop。
      if let Err(err) = store.file_gc(tbl, shift).await {
        log::warn!(target: "fkv_shard", "drop 后 FileGc 失败 tbl={tbl} err={err}");
      }
      state.table_open.forget(tbl);
      Ok(Response::Drop)
    }
    _ => Err(KvError::InvalidArgs),
  }
}

/// 归档：创建归档与删除归档（串行防御路径；正常路径任务化见 `run_gated_spawn`）。
async fn dispatch_archive(state: &mut ShardState, req: &Request) -> Result<Response> {
  match req {
    Request::CreateArchive { tbl, .. } => {
      if state.stores.store(tbl).is_err() {
        // 尚无 manifest 的空分区：C++ `CreateArchive` 直接成功且不写文件。
        return Ok(Response::CreateArchive(ArchiveReply {
          name: String::new(),
          bytes: 0,
          removed_num: 0,
        }));
      }
    }
    Request::DeleteArchive { tbl, .. } => {
      // 只删归档文件，不为它懒建空表；分区槽位承载 branch / manifest 身份。
      state.ensure_slot(tbl)?;
    }
    _ => return Err(KvError::InvalidArgs),
  }
  run_gated_body(state, req).await
}

/// 分支：创建 branch 与删除 branch（删 branch 时同时清 term 文件）。
async fn dispatch_branch(state: &mut ShardState, req: &Request) -> Result<Response> {
  match req {
    Request::CreateBranch { tbl, .. } => {
      if state.stores.store(tbl).is_err() {
        // 空分区：C++ `CreateBranch` 成功但无 branch manifest。
        return Ok(Response::CreateBranch(BranchReply {
          name: String::new(),
          bytes: 0,
        }));
      }
      run_gated_body(state, req).await
    }
    Request::DeleteBranch { tbl, branch } => {
      let removed = if state.stores.store(tbl).is_ok() {
        state
          .stores
          .store_mut(tbl)?
          .delete_branch(tbl, branch)
          .await?
      } else {
        // 空分区 / 未打开：仍要清 term 文件，manifest 数为 0。
        0
      };
      // 对齐 C++ `DeleteBranchFiles`：branch 的 manifest 删完后同时删
      // `CURRENT_TERM_<branch>_<shard>`，否则重建同名 branch 时旧高 term
      // 会让 attach 直接 ExpiredTerm。
      let normalized = normalize_branch_name(branch).ok_or(KvError::InvalidArgs)?;
      state
        .stores
        .backend()
        .remove_term(normalized.as_ref(), state.shard_id)
        .await?;
      Ok(Response::DeleteBranch(removed))
    }
    _ => Err(KvError::InvalidArgs),
  }
}
