//! 读窗口：准入判定、等待事件与常驻预算。

use std::{
  collections::VecDeque,
  mem::take,
  time::{Duration, Instant},
};

use compio::time::timeout;
use crossfire::{MAsyncRx, mpmc::Array};
use fkv_api::Request;
use fkv_error::KvError;
use fkv_metrics::{Op, ShardMetric};
use futures::future::{Either, select};
use log::warn;

use super::{
  finish::{finish_read, finish_read_li, finish_read_partition},
  flight::{ReadDone, ReadFlight},
};
use crate::{
  MAX_READ_BATCH,
  dispatch::{admit, is_read_op, skips_table_open},
  lifecycle::ShardState,
  metrics::{op_of, record_request},
  reopen::ReopenQueue,
  sender::Envelope,
};

/// 大值读期间窗口空闲等待的最小粒度（微秒）。
///
/// 大值读在飞时窗口等待集（读完成 / 高优先请求）没有事件要靠超时回到主
/// 循环。预算被配到 1µs 级时，逐 µs 超时会让主循环空转（io-08 风险记录）；
/// 给等待加一个下限：驻留上界变成「预算 + 该粒度」，仍是有界的；默认 400µs
/// 预算下不改变行为。
const WINDOW_IDLE_WAIT_FLOOR_US: u64 = 50;

/// 读窗口的一次事件。
pub(crate) enum ReadEvent {
  /// 窗口返回主循环：在飞读可能已全部完成并回执（自然排空），也可能是窗口
  /// 常驻预算到点主动退出（io-08）。后者**不** drain 在飞读，调用方不能假定
  /// `ReadFlight` 已空。
  Drained,
  /// 遇到需要主循环串行处理的请求（非读 / 读表未打开 / Store 不可借）：
  /// 请求按到达顺序交回主循环，**不保证在飞读已排空**。
  ///
  /// 写派发只排空目标分区（`drain_write_gate`），控制请求在执行前自己做
  /// 全量排空；Store 不可借的读由主循环挂进 `blocked_read_li`，等写完成
  /// 事件唤醒。这里不再为一条请求牺牲其它分区的在飞读。
  ///
  /// 用队列而非单条：读窗口满时可能已暂存了更早的读请求，它们必须排在
  /// 这条请求之前，不能被丢掉或乱序。
  ///
  /// 窗口退休时暂存里等单飞名额的大值读也走这里（io-14）：主循环凭
  /// `large_read_waiting` 把它们挂进专门等待队列，而不是 `pending`。
  Exclusive(VecDeque<Envelope>),
}

/// 进入读窗口时携带的窗外延迟读（io-14 / io-23）。
pub(crate) struct WindowDeferred {
  /// 窗口外已有更早的大值读在等单飞名额：窗口内新到的大值读不能插队
  /// （io-14），否则持续大值读流量会让更早的等待项饿死。
  pub large: bool,
  /// 主循环批提交按在飞额度截断后的溢出读（io-23）：按到达顺序先于通道
  /// 新读准入，避免持续流量下被新读长期插队。
  pub seed: VecDeque<Envelope>,
}

/// 读窗口准入结果。
enum Admit {
  /// 已提交为读任务，或已就地回执（取消 / 超时 / 准入失败）。
  Done,
  /// 在飞读已达上限：请求未准入，调用方按到达顺序暂存，等空位再试。
  ///
  /// 与 [`Admit::Defer`] 的区别：这里是「读并发窗口」满，等的是任意一条读
  /// 完成腾出的空位，通常在一批读的时长内可推进；后者等的是大值读单飞名额，
  /// 可能被一条长读（几百 MB 的 GetLarge 可达分钟级）独占，不能在窗口内
  /// 一直等到它结束（io-14）。
  Full(Envelope),
  /// 大值读单飞名额被占：请求未准入，调用方按到达顺序暂存，等名额空出。
  ///
  /// 窗口退休（预算到点）时这类请求不再留在窗口等待集里，而是交回主循环
  /// 挂进专门的大值等待队列；主循环只在大值读完成事件后重排，期间照常
  /// 派发写 / 出低优先 / 跑 reopen（io-14）。
  Defer(Envelope),
  /// 需要主循环串行处理（非读 / 读表未打开 / Store 不可借）：请求按到达顺序
  /// 交回主循环，**不做全 shard 读屏障**。
  ///
  /// - 读表未打开：只排空目标分区（打开表要该分区的 `&mut Store`）；
  /// - 同分区写任务在飞（Store 不可借）：不排空任何读，主循环按
  ///   `is_blocked_read` 挂进 blocked 队列，等写完成事件唤醒；
  /// - 写 / 控制请求：写派发在 `drain_write_gate` 里按目标分区排空，控制
  ///   请求在执行前自己做全量排空（见 `event_loop`）。
  Exit(Envelope),
}

/// 读窗口内处理一条新到的高优先请求。
///
/// `large_deferred`：窗口外已有更早的大值读在等单飞名额。即使当前没有大值
/// 读在飞（上一轮刚完成），也不能让窗口里的新大值读插到它前面——否则持续
/// 的大值读流量会让更早的等待项永远拿不到名额（io-14）。
async fn window_admit(
  state: &mut ShardState,
  metrics: &ShardMetric,
  reopen_li: &mut ReopenQueue,
  flight: &mut ReadFlight,
  envelope: Envelope,
  large_deferred: bool,
) -> Admit {
  let read = is_read_op(&envelope.req);
  let ready = read
    && (skips_table_open(&envelope.req)
      || envelope.req.table().is_some_and(|tbl| {
        state
          .table_open
          .ready(&state.stores, tbl, state.term, &state.branch)
      }));
  // 只为「读并发窗口满」等一个空位：不 drain，也不把已完成的读算作名额。
  // 表未 ready 的读也必须先等空位（io-24）：这里若直接 Exit，主循环会因
  // `quota == 0` 把它放进 `deferred_read_li`，下一轮又作为窗口 seed 重入并
  // 再次 Exit——整条环路没有一次 await（该分区在飞计数为 0 时
  // `finish_read_partition` 直接返回），shard 线程忙转，128 条在飞读永远
  // 得不到 poll / 完成。等任意一条读完成腾出空位后，主循环 `quota > 0`，
  // 该读会走 `submit_read_li` 的串行补开（唯一补开点）并重新准入。
  // 非读请求不受读额度约束，仍立即 Exit，由主循环派发写 / 控制请求。
  if read && flight.is_full() {
    return Admit::Full(envelope);
  }
  // 非读请求、读表未打开：交回主循环，不做全 shard 屏障。
  // - 读表未打开：主循环的串行准入会补开（唯一需要该分区 `&mut Store` 的
  //   动作），这里只排空目标分区；其它分区的在飞读继续滑动。
  // - 写 / 控制请求：写派发按目标分区排空，控制请求在执行前自己全量排空。
  if !ready {
    if read && let Some(tbl) = envelope.req.table() {
      finish_read_partition(state, metrics, reopen_li, flight, tbl).await;
    }
    return Admit::Exit(envelope);
  }
  if envelope.canceled() {
    envelope.send(Err(KvError::NotRunning));
    return Admit::Done;
  }
  let started = Instant::now();
  let op = op_of(&envelope.req);
  if envelope.expired() {
    warn!(
      target: "fkv_shard",
      "request deadline expired shard_id={} op={}",
      metrics.shard_id(),
      op.map_or("unknown", Op::as_str)
    );
    let err = Err(KvError::Timeout);
    record_request(metrics, op, started, &err);
    envelope.send(err);
    return Admit::Done;
  }
  if let Err(err) = admit(state, &envelope.req) {
    record_request(metrics, op, started, &Err(err));
    envelope.send(Err(err));
    return Admit::Done;
  }
  // 大值读单飞：已有大值读在飞（或窗口外已有更早的等待项）时不能准入
  // （io-05：fixed buffer 独占 + 大值内存峰值约束）。用 `Defer` 而不是
  // `Full` 标记原因：窗口满等一个空位是短等待，单飞名额是长等待，退休阶段
  // 必须把后者交回主循环（io-14）。
  if matches!(envelope.req, Request::GetLarge { .. }) && (state.large_read_out || large_deferred) {
    return Admit::Defer(envelope);
  }
  let epoch = state.table_open.epoch;
  let tbl = envelope.req.table().cloned();
  match tbl.as_ref().map(|tbl| state.stores.store_rc(tbl)) {
    Some(Ok(store)) => {
      if matches!(envelope.req, Request::GetLarge { .. }) {
        state.large_read_out = true;
        let buf = state.fixed_buffer.take();
        flight.push_large(store, epoch, envelope, started, buf);
      } else {
        flight.push(store, epoch, envelope, started);
      }
    }
    _ => {
      // Store 不可借（刚被写任务取走 / 槽位消失）：不 drain 任何在飞读，
      // 交回主循环按 FIFO 重排。主循环会把它归入 `blocked_read_li`（写完成
      // 事件唤醒），其它分区的在飞读不受这条请求影响。
      return Admit::Exit(envelope);
    }
  }
  Admit::Done
}

/// 读窗口等待到的下一个事件（把 `select` 的结果与借用解耦）。
enum Wake {
  Done(ReadDone),
  /// 在飞读集合已空。
  Empty,
  Envelope(Envelope),
  /// 高优先通道关闭。
  Closed,
}

/// 读窗口常驻预算是否到点（`budget_us == 0` = 关闭）。
///
/// 与主循环的 `round_budget_due` 同一配置来源（`max_processing_time_us`）：
/// 窗口内没有「请求边界」，持续读流量下必须自带出口，否则低优先通道、
/// reopen 到期重试与写任务回收都只能在读流量停止后才推进（io-08）。
fn window_budget_due(budget_us: u64, window_start: Instant) -> bool {
  budget_us != 0
    && u64::try_from(window_start.elapsed().as_micros()).unwrap_or(u64::MAX) >= budget_us
}

/// 读并发窗口：在飞读由 compio 独立推进，主循环同时接收新请求。
///
/// 窗口满（[`MAX_READ_IN_FLIGHT`]）时只等**任意一条**读完成再准入，不再
/// drain 全部。窗口本身不做全 shard 读屏障：非读 / 读表未打开 / Store 不可借
/// 的请求按到达顺序交回主循环——写派发只排空目标分区（保证同分区
/// 「读不越过后到的写」），控制请求在执行前全量排空，Store 不可借的读挂进
/// `blocked_read_li` 等写完成事件。
///
/// 窗口常驻有界：进入时记 [`Instant`]，超过 `max_processing_time_us` 后
/// 不再拉新请求，把已暂存的读按完成节奏准入完即返回 [`ReadEvent::Drained`]
/// （**不** drain 在飞读），让主循环跑一轮写回收 / reopen 到期 / 时间片 /
/// 低优先。**大值读在飞 / 等待时，等任意读完成也带剩余预算超时**：单飞名额
/// 只有首条大值读完成才释放，只靠事件边界检查预算会让窗口驻留到首条大值读
/// 结束（io-14）；普通读没有额外等待项，仍按事件边界检查预算（io-08）。
///
/// 暂存读不交回主循环，避免 `pending` 非空抑制写派发（io-04 的 lockstep
/// 回归）。唯一例外是**等单飞名额的大值读**：它的等待时长由另一条大值读
/// 决定（可达分钟级），退休时必须交回主循环，由专门的等待队列承载，不能
/// 成为窗口的无界等待项（io-14）。
pub(crate) async fn read_window(
  state: &mut ShardState,
  metrics: &ShardMetric,
  reopen_li: &mut ReopenQueue,
  flight: &mut ReadFlight,
  rx_hi: &MAsyncRx<Array<Envelope>>,
  budget_us: u64,
  deferred: WindowDeferred,
) -> ReadEvent {
  // 窗口满时暂存的读（FIFO）：等任一读完成腾出空位后按到达顺序准入。
  // 上限取一轮收集量，避免把有界通道整体搬进无界内存、压掉背压。
  //
  // 初始项来自主循环批提交路径按在飞额度截断后的溢出读（io-23）：它们比
  // 通道里的新读更早到达，必须先于新读准入。
  let large_deferred = deferred.large;
  let mut wait_li: VecDeque<Envelope> = deferred.seed;
  let window_start = Instant::now();
  // 预算到点后停止拉新（背压留在有界通道），只清空已暂存读再退出。
  let mut retiring = false;
  loop {
    // 0. 每个准入 / 完成 / 等待边界都检查一次预算；暂存非空时先把它们
    //    推进窗口（一次完成腾一个空位），清空后才退出。
    if !retiring && window_budget_due(budget_us, window_start) {
      retiring = true;
    }
    if retiring && wait_li.is_empty() {
      return ReadEvent::Drained;
    }
    // 1. 先消化暂存读；仍满则转到等待。
    while let Some(envelope) = wait_li.pop_front() {
      match window_admit(state, metrics, reopen_li, flight, envelope, large_deferred).await {
        Admit::Done => {}
        Admit::Full(envelope) => {
          wait_li.push_front(envelope);
          break;
        }
        Admit::Defer(envelope) => {
          if retiring {
            // io-14：预算已到点，不能为等大值读单飞名额把窗口留住——那会
            // 让驻留时长退化为「首条大值读全程」（几百 MB 可达分钟级），
            // 期间写派发 / 写回收 / 低优先 / reopen 全部停摆。该请求与更晚
            // 的暂存读一起交回主循环；主循环把大值读挂进专门等待队列，其余
            // 按原顺序处理。
            let mut out = VecDeque::from([envelope]);
            out.append(&mut wait_li);
            return ReadEvent::Exclusive(out);
          }
          // 未退休：单飞名额由在飞的大值读完成事件释放，留在窗口暂存即可。
          wait_li.push_front(envelope);
          break;
        }
        Admit::Exit(envelope) => {
          // 更晚的暂存读排在它之后，一起交回主循环。
          let mut out = VecDeque::from([envelope]);
          out.append(&mut wait_li);
          return ReadEvent::Exclusive(out);
        }
      }
    }
    // 2. 非阻塞取新请求；满时也要收，才能及时看到写 / 控制请求。
    // 暂存已达上限或窗口正在退出就停止拉取，把背压留给有界请求通道。
    if !retiring
      && wait_li.len() < MAX_READ_BATCH
      && let Ok(envelope) = rx_hi.try_recv()
    {
      match window_admit(state, metrics, reopen_li, flight, envelope, large_deferred).await {
        Admit::Done => continue,
        Admit::Full(envelope) | Admit::Defer(envelope) => wait_li.push_back(envelope),
        Admit::Exit(envelope) => {
          // 更早的暂存读必须排在它之前：按到达顺序整体交回主循环，
          // 由主循环逐条准入；这里不做屏障，也不替它们提前抢窗口空位。
          let mut out = take(&mut wait_li);
          out.push_back(envelope);
          return ReadEvent::Exclusive(out);
        }
      }
    }
    // 3. 暂存已满或正在退出：只等完成（一次腾一个空位），不继续抽通道。
    if wait_li.is_empty() && retiring {
      // 暂存已在本轮准入清空：不 drain 在飞读，交回主循环。
      return ReadEvent::Drained;
    }
    if wait_li.len() >= MAX_READ_BATCH || retiring {
      match flight.next().await {
        Some(done) => {
          finish_read(state, metrics, reopen_li, flight, done, true).await;
        }
        None => {
          // 没有在飞读却仍有暂存（理论不可达）：交回主循环按顺序处理。
          return ReadEvent::Exclusive(take(&mut wait_li));
        }
      }
      continue;
    }
    // 4. 等任意一条读完成或新请求；大值读在飞 / 等待时带剩余预算超时。
    //
    // 只有大值读单飞会把窗口的等待时长从「一条读」放大成「另一条大值读
    // 全程」：预算检查在每个事件边界做，而 GL2 暂存后窗口只等 GL1 完成，
    // 期间没有任何事件边界（io-14）。这里在大值读在飞或已有大值读在窗外
    // 等待时给等待加超时，超时只置 `retiring` 回到顶部：暂存非空时仍按空位
    // 推进（不丢请求），暂存为空立即 `Drained`，主循环得以跑写回收 / 低优先
    // / reopen。普通读不装这个超时：它们没有额外的无界等待项，保持 io-08 的
    // 事件边界预算语义与既有调度节奏。
    //
    // 等待时长取下限 [`WINDOW_IDLE_WAIT_FLOOR_US`]，避免 1µs 级预算下的逐
    // 微秒空转；大值读场景的驻留上界相应为「预算 + 该粒度」。
    //
    // 先把 `select` 落到只含所有权数据的 [`Wake`]：`select` 返回的未选中
    // future 仍借用 `flight` / `rx_hi`，不能在同一作用域里再次借用它们。
    // `select` 的 `Either` 里带着未选中的 future（借用 `flight` / `rx_hi`），
    // 不能先绑定到变量再跨语句使用；两个分支各自在 match 表达式内解构，
    // 只把所有权数据带进 [`Wake`]。
    let large_busy = state.large_read_out || large_deferred;
    let wake = if budget_us == 0 || !large_busy {
      match select(Box::pin(flight.next()), Box::pin(rx_hi.recv())).await {
        Either::Left((Some(done), _)) => Wake::Done(done),
        Either::Left((None, _)) => Wake::Empty,
        Either::Right((Ok(envelope), _)) => Wake::Envelope(envelope),
        Either::Right((Err(_), _)) => Wake::Closed,
      }
    } else {
      let spent = u64::try_from(window_start.elapsed().as_micros()).unwrap_or(u64::MAX);
      let wait_us = budget_us
        .saturating_sub(spent)
        .max(WINDOW_IDLE_WAIT_FLOOR_US);
      let remaining = Duration::from_micros(wait_us);
      match timeout(
        remaining,
        select(Box::pin(flight.next()), Box::pin(rx_hi.recv())),
      )
      .await
      {
        Ok(Either::Left((Some(done), _))) => Wake::Done(done),
        Ok(Either::Left((None, _))) => Wake::Empty,
        Ok(Either::Right((Ok(envelope), _))) => Wake::Envelope(envelope),
        Ok(Either::Right((Err(_), _))) => Wake::Closed,
        Err(_) => {
          // 预算超时：回到顶部置 `retiring`，按暂存是否可推进决定退出或等
          // 一次完成腾空位；不在等待集里继续驻留。
          retiring = true;
          continue;
        }
      }
    };
    match wake {
      Wake::Done(done) => {
        let last = flight.is_empty();
        finish_read(state, metrics, reopen_li, flight, done, true).await;
        if last {
          // 窗口已空：暂存读（理论上窗口满时不会只剩最后一条）交回主循环，
          // 绝不能随 Drained 丢掉。
          if wait_li.is_empty() {
            return ReadEvent::Drained;
          }
          return ReadEvent::Exclusive(take(&mut wait_li));
        }
      }
      Wake::Empty => {
        // 在飞读已空；暂存读没有理由还留着（防御性交回主循环）。
        if wait_li.is_empty() {
          return ReadEvent::Drained;
        }
        return ReadEvent::Exclusive(take(&mut wait_li));
      }
      Wake::Envelope(envelope) => {
        match window_admit(state, metrics, reopen_li, flight, envelope, large_deferred).await {
          Admit::Done => {}
          // 退休路径在步骤 3 就转去等完成，不会走到这里；非退休时两种
          // 「未准入」都按到达顺序暂存。
          Admit::Full(envelope) | Admit::Defer(envelope) => wait_li.push_back(envelope),
          Admit::Exit(envelope) => {
            // 更早的暂存读排在它之前，按到达顺序一起交回主循环。
            let mut out = take(&mut wait_li);
            out.push_back(envelope);
            return ReadEvent::Exclusive(out);
          }
        }
      }
      Wake::Closed => {
        // 高优先通道关闭：先让在飞读完，暂存读交回主循环按顺序处理。
        finish_read_li(state, metrics, reopen_li, flight).await;
        if wait_li.is_empty() {
          return ReadEvent::Drained;
        }
        return ReadEvent::Exclusive(take(&mut wait_li));
      }
    }
  }
}
