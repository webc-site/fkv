//! 批量提交：串行准入 / 打开后把读请求 spawn 成独立任务。

use std::time::Instant;

use fkv_api::Request;
use fkv_error::KvError;
use fkv_metrics::{Op, ShardMetric};
use log::warn;

use super::{finish::finish_read_partition, flight::ReadFlight};
use crate::{
  dispatch::{admit, ensure_table_ready, is_auto_reopen_read, skips_table_open},
  lifecycle::ShardState,
  metrics::{op_of, record_request},
  reopen::ReopenQueue,
  sender::Envelope,
  standby::{SyncAdmit, admit_standby_sync, discard_synced, park_sync_waiter, standby_sync_due},
};

/// 提交一批只读请求：串行完成准入 + 打开（唯一需要 `&mut Store` 的部分），
/// 之后每条请求 spawn 成独立 compio task，彼此并发、与主循环解耦。
///
/// 两阶段：所有打开完成后再统一 spawn。否则前一条读任务的 `Rc` 克隆会让
/// 后一条的 `ensure_table_ready` 拿不到 `&mut Store`。
///
/// 返回本轮未能提交的大值读（已有大值读在飞，io-05）：调用方按原顺序放进
/// 大值等待队列（`large_park_li`），等单飞名额空出后重排。
///
/// `large_deferred`：主循环已有更早的大值读在等单飞名额。本轮即使名额暂时
/// 空出也不能让新到的大值读插队，否则持续大值读流量会让更早的等待项饿死
/// （io-14）。
pub(crate) async fn submit_read_li(
  state: &mut ShardState,
  metrics: &ShardMetric,
  reopen_li: &mut ReopenQueue,
  flight: &mut ReadFlight,
  batch_li: Vec<Envelope>,
  large_deferred: bool,
) -> Vec<Envelope> {
  // 读批大小分布：本次高优先通道合并提交的读请求条数（eng-32c）。
  metrics.record_read_batch(u64::try_from(batch_li.len()).unwrap_or(u64::MAX));
  let mut ready_li = Vec::with_capacity(batch_li.len());
  for envelope in batch_li {
    let started = Instant::now();
    let op = op_of(&envelope.req);
    // 排队期间调用方已放弃：不执行，按 `NotRunning` 回执（oneshot 完成契约）。
    // 若它是追赶挂起重放的请求（io-33），同时丢弃该表的已完成追赶结果。
    if envelope.canceled() {
      if let Some(tbl) = envelope.req.table() {
        discard_synced(state, tbl);
      }
      envelope.send(Err(KvError::NotRunning));
      continue;
    }
    // 排队期间已过 deadline：跳过，不进入 admit / prepare_table。
    if envelope.expired() {
      warn!(
        target: "fkv_shard",
        "request deadline expired shard_id={} op={}",
        metrics.shard_id(),
        op.map_or("unknown", Op::as_str)
      );
      if let Some(tbl) = envelope.req.table() {
        discard_synced(state, tbl);
      }
      let err = Err(KvError::Timeout);
      record_request(metrics, op, started, &err);
      envelope.send(err);
      continue;
    }
    if let Err(err) = admit(state, &envelope.req) {
      record_request(metrics, op, started, &Err(err));
      envelope.send(Err(err));
      continue;
    }
    let Some(tbl) = envelope.req.table().cloned() else {
      let err = Err(KvError::InvalidArgs);
      record_request(metrics, op, started, &err);
      envelope.send(err);
      continue;
    };
    // 归档列举 / 远端列举不依赖表打开（不懒建空表、不等待 manifest 可见），
    // 但仍需要一个分区槽位承载 branch / manifest 身份。
    if skips_table_open(&envelope.req)
      && !state.stores.has_slot(&tbl)
      && let Err(err) = state.ensure_slot(&tbl)
    {
      record_request(metrics, op, started, &Err(err));
      envelope.send(Err(err));
      continue;
    }
    // 串行打开：每表只在这里 replay 一次 manifest（幂等），读任务本身不再打开。
    if !skips_table_open(&envelope.req)
      && !state
        .table_open
        .ready(&state.stores, &tbl, state.term, &state.branch)
    {
      // standby 首访问追赶任务化（io-33）：追赶交独立 compio task，本读请求
      // 挂起为 waiter，完成后经 wake_li 重放到这里消费结果（`ensure_table_ready`
      // 内 `take_synced`）；期间事件循环照常服务其它分区。
      if let Some(sync_tbl) = standby_sync_due(state, &envelope.req)
        && matches!(admit_standby_sync(state, &sync_tbl), SyncAdmit::Parked)
      {
        park_sync_waiter(state, &sync_tbl, envelope);
        continue;
      }
      // 打开表要目标分区的 `&mut Store`：只排空该分区的在飞读。
      // 这里可能带着读窗口预算退出后的在飞读（io-08），不能假定窗口已排空。
      finish_read_partition(state, metrics, reopen_li, flight, &tbl).await;
      if let Err(err) = ensure_table_ready(state, &tbl).await {
        // Cloud / StandbyReplica：manifest 暂不可见 → 挂等待重排（对齐 C++
        // `AutoReopenRetry`）；其余错误直接回执。
        let park = state.reopen_on_missing
          && matches!(err, KvError::ResourceMissing)
          && is_auto_reopen_read(&envelope.req);
        // 云 manifest 采纳栅栏 / store 层 term 错误：整 shard 置 Expired，
        // 后续请求快速失败（对齐 C++ FinishTask 的 MarkShardExpired）。
        state.observe_task_err(&Err(err));
        let envelope = if park {
          reopen_li.park(envelope)
        } else {
          Some(envelope)
        };
        if envelope.is_some() {
          record_request(metrics, op, started, &Err(err));
        }
        if let Some(envelope) = envelope {
          envelope.send(Err(err));
        }
        continue;
      }
    }
    ready_li.push((tbl, envelope, started));
  }
  // 全部打开完成后统一提交：此后不再需要 `&mut Store`。
  let epoch = state.table_open.epoch;
  let mut leftover_li = Vec::new();
  for (tbl, envelope, started) in ready_li {
    // 大值读单飞：已有大值读在飞（独占 fixed buffer / 内存峰值约束）或更早
    // 的等待项尚未重排时，本轮不提交，按原顺序交回调用方（io-05 / io-14）。
    if matches!(envelope.req, Request::GetLarge { .. }) && (state.large_read_out || large_deferred)
    {
      leftover_li.push(envelope);
      continue;
    }
    match state.stores.store_rc(&tbl) {
      Ok(store) => {
        if matches!(envelope.req, Request::GetLarge { .. }) {
          state.large_read_out = true;
          let buf = state.fixed_buffer.take();
          flight.push_large(store, epoch, envelope, started, buf);
        } else {
          flight.push(store, epoch, envelope, started);
        }
      }
      Err(err) => {
        record_request(metrics, op_of(&envelope.req), started, &Err(err));
        envelope.send(Err(err));
      }
    }
  }
  leftover_li
}
