//! 读完成回执：代际校验、OOM 重试 / 重排、park、指标与全量 / 分分区屏障。

use fkv_api::Request;
use fkv_error::KvError;
use fkv_metrics::ShardMetric;
use fkv_types::TableIdent;
use log::{error, warn};

use super::flight::{ReadDone, ReadFlight, ReadSlot};
use crate::{
  dispatch::handle,
  lifecycle::ShardState,
  metrics::{op_of, record_request, sync_cache_metrics, sync_cache_metrics_or_full},
  reopen::ReopenQueue,
  sender::Envelope,
};

/// 单轮全屏障回执告警阈值（io-10）。
///
/// compio 0.19.2 的跨线程 waker 队列容量为 64；一次 drain 连续唤醒超过这个
/// 数，生产者线程可能在 `Remote::schedule` 满队自旋（上游 `735e473c` 改为
/// 无界队列，未进 0.19.2）。超过阈值只告警，不改变回执行为。
const CROSS_THREAD_REPLY_BURST_WARN: usize = 64;

/// 一条读完成后的收尾：代际校验、OOM 重试 / 重排、`ResourceMissing` 挂起、
/// 指标与回执。保证该请求恰好「回执一次」或「重新入队一次」。
///
/// 返回是否真正向调用方发了回执（重排 / park 不计），供 [`finish_read_li`]
/// 统计单轮回执批大小。
///
/// `sync_cache`：回执前是否同步 index 缓存计数。窗口内逐条同步（调用方拿到
/// 读回执后立刻查缓存指标即可见，不依赖窗口何时排空）；屏障路径由
/// [`finish_read_li`] 统一同步一次。
pub(crate) async fn finish_read(
  state: &mut ShardState,
  metrics: &ShardMetric,
  reopen_li: &mut ReopenQueue,
  flight: &mut ReadFlight,
  done: ReadDone,
  sync_cache: bool,
) -> bool {
  let ReadDone {
    slot,
    req,
    result,
    buf,
  } = done;
  let ReadSlot {
    reply,
    cancel,
    deadline,
    reopen_retry,
    started,
    epoch: slot_epoch,
    oom_remaining,
    large,
    // 归属分区：完成时只同步该分区的 index 缓存指标（旧实现每次全量扫描
    // 全部槽位，多分区 shard 的读路径因此随分区数线性变慢）。
    tbl,
  } = slot;
  // 大值读完成：归还 fixed buffer（任务 panic 时随任务丢失，此后大值读退回
  // 普通路径），并解除「同一时刻一条」的单飞限制（io-05）。
  if large {
    state.large_read_out = false;
    if let Some(buf) = buf {
      state.fixed_buffer = Some(buf);
    }
  }
  // 任务 panic / 取消：请求体已随任务丢失，只能按内部错误回执。
  let Some(req) = req else {
    if sync_cache {
      sync_cache_metrics_or_full(state, metrics, tbl.as_ref());
    }
    let _ = reply.send(Err(KvError::IoFail));
    return true;
  };
  let mut result = result;
  // 跨 attach 代际的读结果作废（控制请求都会先 drain，正常不会发生）。
  if slot_epoch != state.table_open.epoch {
    result = Err(KvError::NotRunning);
  }
  if matches!(result, Err(KvError::OutOfMem)) && oom_remaining > 0 {
    // 无在飞读（无 `Rc` 竞争、拿得到独占状态）时保持 inline 重试。
    //
    // `GetLarge` 例外：大值读单飞时它常常就是唯一在飞读，此时 inline 重试
    // 仍会把一次完整的大值读跑在事件循环栈上——期间请求不出队、读窗口预算
    // 不可达、写完成回收 / reopen / 低优先全部停摆（io-11 / io-19）。大值
    // 读无论 `flight` 是否为空都走下面的重排。
    if flight.is_empty() && !matches!(req, Request::GetLarge { .. }) {
      let mut remaining = oom_remaining;
      while matches!(result, Err(KvError::OutOfMem)) && remaining > 0 {
        remaining -= 1;
        result = handle(state, &req).await;
      }
    } else if let Ok(store) = req
      .table()
      .ok_or(KvError::InvalidArgs)
      .and_then(|tbl| state.stores.store_rc(tbl))
    {
      // 仍有其它读在飞（拿不到 `&mut Store`）或 `GetLarge`（大值读不内联，
      // 即使它是唯一在飞读）：按剩余次数重新入队，由窗口在下一个空位继续
      // 执行；不在这里串行 `handle`、也不触发屏障。
      //
      // 大值读同样走重排（`large=false`）：fixed buffer 已在函数顶部归还，
      // plain 大值读只借 `&Store`，重排任务不重新占用单飞名额；完成时按普通
      // 读回执（`large` 标志留在原任务上）。
      let envelope = Envelope {
        req,
        reply,
        cancel,
        deadline,
        reopen_retry,
      };
      flight.push_retry(store, slot_epoch, envelope, started, oom_remaining - 1);
      return false;
    }
  }
  if sync_cache {
    sync_cache_metrics_or_full(state, metrics, tbl.as_ref());
  }
  // 云 manifest 采纳栅栏 / store 层 term 错误：读任务完成时同样把整 shard
  // 置 Expired（对齐 C++ `FinishTask`：读任务也走 MarkShardExpired），
  // 后续请求经 `admit` 快速失败。
  state.observe_task_err(&result);
  let op = op_of(&req);
  let envelope = Envelope {
    req,
    reply,
    cancel,
    deadline,
    reopen_retry,
  };
  // Cloud / StandbyReplica：manifest 暂不可见 → 挂等待 manifest 可见后重排。
  let envelope = if state.reopen_on_missing && matches!(result, Err(KvError::ResourceMissing)) {
    reopen_li.park(envelope)
  } else {
    Some(envelope)
  };
  // 被 park 的读请求唤醒后会重跑，先记会双计；只记真正回执的这一条。
  if envelope.is_some() {
    record_request(metrics, op, started, &result);
  }
  if let Some(envelope) = envelope {
    envelope.send(result);
    true
  } else {
    false
  }
}

/// 等在飞读**全部**完成并逐条回执（全 shard 屏障）。
///
/// 返回后不再有任何读任务持有分区 `Rc<ShardStore>`：OOM 重排可能在某一轮
/// 回执后把读重新压回 `flight`（`push_retry`），循环会继续等到它结束。
///
/// 只用于需要全量线性化的控制路径（attach / detach / shutdown / drop /
/// reopen / 写队列清扫）。写派发只需独占目标分区，用
/// [`finish_read_partition`]：读任务释放 `Rc` 克隆后 `Rc::get_mut` 才能成功，
/// 而读任务只持自己分区的克隆，排空一个分区不必等其它分区。
pub(crate) async fn finish_read_li(
  state: &mut ShardState,
  metrics: &ShardMetric,
  reopen_li: &mut ReopenQueue,
  flight: &mut ReadFlight,
) {
  let mut replied = 0usize;
  // 一次 drain 不是终点：`finish_read` 的 OOM 重排在 flight 已排空后仍会
  // `push_retry`（`GetLarge` 无论是否有其它读任务都不内联，io-19），新任务
  // 会重新持有目标分区的 `Rc<ShardStore>`。屏障的契约是「返回时没有读任务
  // 持有 Store」，因此每轮回执后重新检查集合并继续等重排读。
  //
  // 循环必然收敛：每次重排 `oom_remaining` 减一，预算耗尽后 `finish_read`
  // 直接回执 `OutOfMem`，不会再把请求压回 `flight`。
  while !flight.is_empty() {
    let done_li = flight.drain().await;
    if done_li.is_empty() {
      // 循环条件保证集合非空，却取不到任何完成项：任务号与 slot 表不一致的
      // 内部错误（`next` 已逐条记录）。再等也不会有回执，退出避免死等。
      error!(
        target: "fkv_shard",
        "read barrier without completion count={}",
        flight.len()
      );
      break;
    }
    // drain 已把每条完成 publish 到 gauge；这里显式归零，保证空窗口读数为 0。
    flight.publish();
    // 屏障路径没有在飞读：缓存计数统一同步一次，再逐条回执。
    sync_cache_metrics(state, metrics);
    for done in done_li {
      if finish_read(state, metrics, reopen_li, flight, done, false).await {
        replied += 1;
      }
    }
  }
  // 一次屏障连续回执超过 compio 0.19.2 的跨线程唤醒队列容量：调用方线程
  // 若正卡在长任务 poll 上，回执唤醒会打满队列并让 shard 线程在
  // `Remote::schedule` 自旋（io-10）。只告警，不改变回执节奏。
  if replied > CROSS_THREAD_REPLY_BURST_WARN {
    warn!(
      target: "fkv_shard",
      "read barrier reply burst shard_id={} count={} threshold={}",
      metrics.shard_id(), replied, CROSS_THREAD_REPLY_BURST_WARN
    );
  }
}

/// 等目标分区的在飞读全部完成并逐条回执；其它分区照常滑动。
///
/// 语义与 [`finish_read_li`] 相同（每条读都走 `finish_read`，回执 / 取消 /
/// 超时 / reopen park / OOM 重排不延迟），只是等待范围收窄到 `tbl`：
///
/// - 目标分区计数归零 ⇔ 该分区没有读任务持有 `Rc<ShardStore>` 克隆，
///   调用方随后 `store_mut` / `take(tbl)` 才能拿到独占；
/// - 「先到的读先完成、后到的写才开始」在目标分区上保持；
/// - 等待期间完成的分区外读会被正常处理，不会被压到写之后回执。
///
/// OOM 重排可能在同一次 `finish_read` 里把读重新压回 `tbl`（`push_retry`），
/// 循环会重新看到的计数，继续等它结束。
pub(crate) async fn finish_read_partition(
  state: &mut ShardState,
  metrics: &ShardMetric,
  reopen_li: &mut ReopenQueue,
  flight: &mut ReadFlight,
  tbl: &TableIdent,
) {
  while flight.partition_len(tbl) > 0 {
    let Some(done) = flight.next().await else {
      // 计数与任务集合不一致（内部错误）：计数已无法再降，记录后返回，
      // 避免 shard 线程在这里死等。
      error!(
        target: "fkv_shard",
        "read partition count without in-flight task tbl={} count={}",
        tbl,
        flight.partition_len(tbl)
      );
      break;
    };
    finish_read(state, metrics, reopen_li, flight, done, true).await;
  }
}
