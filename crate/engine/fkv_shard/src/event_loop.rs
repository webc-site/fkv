//! shard 事件循环：高 / 低优先调度、等待事件、写队列推进与读窗口调度。

use std::{collections::VecDeque, mem::take, sync::Arc, time::Instant};

use compio::time::timeout;
use crossfire::{MAsyncRx, mpmc::Array};
use fkv_api::Request;
use fkv_error::KvError;
use fkv_metrics::ShardMetric;
use futures::future::{Either, select};

use crate::{
  MAX_READ_BATCH, MAX_READ_IN_FLIGHT, MAX_WRITE_COLLECT,
  dispatch::{is_read_op, is_write_op, process_envelope},
  lifecycle::{Shard, ShardConfig, ShardState},
  metrics::{flush_cache_bytes, sync_cache_metrics},
  read::{ReadEvent, ReadFlight, WindowDeferred, finish_read_li, read_window, submit_read_li},
  reopen::{ReopenQueue, run_due_reopen},
  sender::{Envelope, Pri, pri_of},
  write::{
    DrainCx, WriteDone, WriteFlight, drain_write_gate, fail_pending, finish_write,
    write_queue_stale,
  },
  write_gate::WriteGate,
};

/// 事件循环全量对账的间隔轮数。
///
/// 读完成 / 写完成 / 串行请求回执前都已按目标分区增量同步（O(1)），事件循环
/// 这次全量扫描只兜底请求之外的槽位变化（延迟 reopen 懒开、维护、失败请求的
/// 缓存访问）。指标是单调累计值，错过几轮不会丢数：每 64 轮对账一次即可把
/// 单轮摊薄到可忽略，同时保证空闲前的读数最终一致。
const CACHE_METRICS_RECONCILE_ROUNDS: u32 = 64;

impl Shard {
  /// shard 事件循环：接收高 / 低优先请求，推进读写任务与 reopen 重试，直到关停。
  ///
  /// `state` / 重试队列由调用方装配；循环内只用 `config` 的队列与时间片参数。
  pub(crate) async fn run_event_loop(
    mut state: ShardState,
    thread_metrics: Arc<ShardMetric>,
    reopen_li: &mut ReopenQueue,
    rx_hi: &MAsyncRx<Array<Envelope>>,
    rx_lo: &MAsyncRx<Array<Envelope>>,
    config: &ShardConfig,
  ) {
    // 读批收集 / 读窗口退出时可能多取到非只读请求：按 FIFO 暂存，
    // 下一轮优先处理，保证与后续写 / 控制请求的先后顺序不变（eng-32a）。
    let mut pending: VecDeque<Envelope> = VecDeque::new();
    // 全量对账倒计时：读 / 写 / 串行请求完成路径都已按目标分区增量同步
    // （O(1)），事件循环的全量扫描只兜底请求外的槽位变化（延迟 reopen 的
    // 懒开、维护、失败请求的缓存访问）。计数器单调，错过一轮不会丢数。
    let mut cache_reconcile_countdown = 0u32;
    // manifest 出现后重排的请求（原挂起的读请求 / 后续 `Reopen`）：优先处理，
    // 但不与新到达的请求混批，保持它们在挂起前的相对顺序。
    let mut wake_li: VecDeque<Envelope> = VecDeque::new();
    // per-partition FIFO 写队列 + 全局写并发门（eng-33）；队列深度 / 在跑写
    // 通过 gauge 上报（eng-33b），读侧从 `Shard::metrics_snapshot` 取。
    let mut gate = WriteGate::new(config.max_write_concurrency, thread_metrics.clone());
    // 在飞读任务：读请求 spawn 成独立 compio task，主循环只做准入 / 打开 /
    // 回执（eng-32b）。任何需要独占 Store 的路径先 `finish_read_li`。
    // in-flight / 读批大小指标随 `ReadFlight` 上报（eng-32c）。
    let mut read_flight = ReadFlight::new(thread_metrics.clone(), config.oom_retry_times);
    // 在飞写任务：高优先 Put 组按分区派发成独立 compio task，跨分区真并发；
    // 完成时把 Store 交回槽位并释放写门额度（eng-33a）。
    let mut write_flight = WriteFlight::new();
    // 目标分区正在写（Store 被写任务持有）的读请求：等写完成后再准入，
    // 不阻塞其它分区 / 其它请求。
    let mut blocked_read_li: VecDeque<Envelope> = VecDeque::new();
    // 等大值读单飞名额的请求（io-14）：与 `blocked_read_li` 同属「资源阻塞的
    // 读」，单独排队而**不**进 `pending`——`pending` 非空会抑制写派发
    // （eng-34 / io-04），一条 GetLarge 就能把写、低优先与写回收压到首条
    // 大值读结束（大值读可达分钟级）。只在 `large_read_out` 清空的循环顶部
    // 按到达顺序重排回 `pending`。
    let mut large_park_li: VecDeque<Envelope> = VecDeque::new();
    // 因读侧在飞已满而未能提交的普通读（io-23）：批提交路径与读窗口共用
    // `MAX_READ_IN_FLIGHT` 上限，溢出的读**不能**回 `pending`——`pending`
    // 非空会跳过读窗口，下一轮又从 `pending` 弹出同一条读，在飞仍满时再次
    // 溢出，形成自旋。这里单独暂存，下一轮交给读窗口作为初始暂存集，由窗口
    // 按到达顺序等空位准入（窗口有 `is_full` 检查与滑动等待）。
    let mut deferred_read_li: VecDeque<Envelope> = VecDeque::new();
    // 高优先连续服务的起点：超过 `max_processing_time_us` 后先让一条
    // 低优先，防止后台被持续的前台流量饿死。
    let mut round_start = Instant::now();
    'shard_loop: loop {
      // 回收已完成的写任务：Store 放回分区槽位、释放写门额度、同步指标；
      // 再唤醒目标分区已空闲的阻塞读。`try_next` 用本任务的 waker 非阻塞 poll
      // 一次，完成通知才会回到事件循环（不能用 noop waker 轮询，io-13）。
      while let Some(done) = write_flight.try_next().await {
        finish_write(&mut state, &mut gate, &thread_metrics, done);
      }
      promote_blocked_reads(&state, &mut blocked_read_li, &mut pending);
      // 防御：单飞标记与在飞读不一致（正常不可达）时清掉，否则大值等待队列
      // 会永远等不到重排机会。
      if state.large_read_out && read_flight.is_empty() {
        state.large_read_out = false;
      }
      promote_large_deferred(&state, &mut large_park_li, &mut pending);
      // 读并发窗口：在飞读由 compio 推进，主循环同时接收新请求；窗口满时
      // 只等任意一条读完成（滑动窗口）；遇到非读请求 / 未打开表 / Store
      // 不可借才 drain 并交回独占路径。
      // 暂存读（`deferred_read_li`）也在这里交给窗口：即使当前没有在飞读，
      // 窗口也能先把它们准入，再继续从通道收新请求（io-23）。
      if !read_flight.is_empty() || !deferred_read_li.is_empty() {
        if pending.is_empty() {
          match read_window(
            &mut state,
            &thread_metrics,
            reopen_li,
            &mut read_flight,
            rx_hi,
            config.max_processing_time_us,
            WindowDeferred {
              large: !large_park_li.is_empty(),
              seed: take(&mut deferred_read_li),
            },
          )
          .await
          {
            ReadEvent::Drained => {}
            ReadEvent::Exclusive(envelope_li) => {
              for envelope in envelope_li {
                if large_read_waiting(&state, &large_park_li, &envelope) {
                  // 更早的大值读还在等单飞名额：按到达顺序排在它之后，
                  // 等名额空出后统一重排。
                  large_park_li.push_back(envelope);
                } else if is_blocked_read(&state, &envelope) {
                  blocked_read_li.push_back(envelope);
                } else {
                  // 保持先后顺序：该请求排在已暂存的非只读请求之后。
                  pending.push_back(envelope);
                }
              }
              continue;
            }
          }
        } else {
          // 还有更早的暂存请求未处理：不再做全 shard 读屏障。暂存里的写由
          // `drain_write_gate` 按目标分区排空（同分区读不越过后到的写），
          // 控制请求在执行前自己全量排空；暂存里的读按 FIFO 交回读批路径。
          // 旧实现在这里 `finish_read_li`，任何一条暂存写都会等全部在飞读。
        }
      }
      // 把上一轮请求产生的 index 缓存计数推入 shard 指标。热路径（读完成 /
      // 写完成 / 串行请求回执）已各自增量同步目标分区；这里每轮只 push 一次
      // 待发布的占用 gauge（驱逐 / 失败回收会改总量，必须在回执前可见），
      // 全量对账降频为每 [`CACHE_METRICS_RECONCILE_ROUNDS`] 轮一次：旧实现
      // 每轮遍历全部槽位，多分区 shard 的事件循环被指标扫描占满。
      flush_cache_bytes(&mut state, &thread_metrics);
      if cache_reconcile_countdown == 0 {
        sync_cache_metrics(&mut state, &thread_metrics);
        cache_reconcile_countdown = CACHE_METRICS_RECONCILE_ROUNDS;
      } else {
        cache_reconcile_countdown -= 1;
      }
      // 先跑到期的重试再等请求：挂起条目非空时按最早到期时间等待，睡够即回
      // 循环顶部重试，绝不在请求任务内 `sleep` 阻塞整个 shard。
      run_due_reopen(&mut state, reopen_li, &mut wake_li).await;
      // 上一轮为给低优先让路而暂存的写请求继续推进；高优先写不在此被饿。
      // 有更早的暂存请求（含刚解除阻塞的读）时先处理它们、本轮不再派发新写：
      // 否则维护任务化（io-05）后，写队列会在每次任务完成时立刻补位，同分区
      // 的阻塞读永远抢不到窗口（eng-34 的「用户读不被队头阻塞」）。
      // 暂存里的写仍经 gate.push 入队，per-partition FIFO 不受派发顺序影响。
      if gate.has_ready() && pending.is_empty() {
        let mut cx = DrainCx {
          state: &mut state,
          metrics: &thread_metrics,
          reopen_li: &mut *reopen_li,
          read_flight: &mut read_flight,
          write_flight: &mut write_flight,
          gate: &mut gate,
          oom_retry_times: config.oom_retry_times,
        };
        if drain_write_gate(&mut cx).await {
          break;
        }
      }
      // 选下一条请求：wake / 暂存队列与高优先通道先跑；低优先只在没有
      // 前台请求，或高优先已连续占用超过时间片时各跑一条（eng-34）。
      let mut next = wake_li.pop_front().or_else(|| pending.pop_front());
      if next.is_none() {
        next = rx_hi.try_recv().ok();
      }
      // 时间片到点且有后台请求在等：先跑一条低优先，高优先暂存到
      // `pending` 下一轮先跑，保证后台不被持续前台流量饿死。
      if round_budget_due(config.max_processing_time_us, round_start) {
        // 预算到点就计数（eng-34c）：低优先通道为空时也算「前台霸占过时间片」。
        thread_metrics.inc_round_over_budget();
        // 到点即重置：通道为空时不清零会让后续每轮都重复计数。
        round_start = Instant::now();
        if let Ok(low) = rx_lo.try_recv() {
          thread_metrics.inc_yield();
          thread_metrics.dec_low_pq_depth();
          if let Some(envelope) = next.take() {
            pending.push_front(envelope);
          }
          next = Some(low);
        }
      }
      if next.is_none()
        && let Ok(low) = rx_lo.try_recv()
      {
        thread_metrics.dec_low_pq_depth();
        next = Some(low);
        round_start = Instant::now();
      }
      let envelope = match next {
        Some(envelope) => envelope,
        None => {
          // 读窗口因预算退出但仍有在飞读：窗口外的 `wait_event` 不 select 读
          // 完成，落进去会让已完成读的回执与写任务回收一直等新请求（io-08）。
          // 大值等待队列非空时同理：它不在 `pending` / 通道里，落进
          // `wait_event` 会一直等到下一条新请求才被重排（io-14）。
          // 顶部写回收 / reopen 到期 / 时间片 / 低优先本轮已各跑一次，直接回
          // 循环顶部（新窗口重新计预算，不会忙转）。
          if !read_flight.is_empty() || !large_park_li.is_empty() {
            continue;
          }
          match wait_event(rx_hi, rx_lo, &mut write_flight, reopen_li.next_due()).await {
            WaitEvent::Request(envelope) => {
              // 空闲等待不计入高优先连续占用：否则等一小时后的第一条请求
              // 会被立刻判定「超时片」而误让路。
              round_start = Instant::now();
              dec_low_pq_if_low(&thread_metrics, envelope)
            }
            WaitEvent::WriteDone(done) => {
              // 没有就绪请求时也要推进写任务：完成后回顶部重放 pending / gate。
              finish_write(&mut state, &mut gate, &thread_metrics, *done);
              round_start = Instant::now();
              continue;
            }
            WaitEvent::ReopenDue => {
              // 等待到期不算前台占用；同时避免下一轮立即再次判定超时片。
              round_start = Instant::now();
              continue;
            }
            WaitEvent::Closed => {
              // 两条通道都关闭：等在飞写落地后再退出。
              write_flight
                .drain(&mut state, &mut gate, &thread_metrics)
                .await;
              break;
            }
          }
        }
      };
      // 连续只读请求成批同轮推进：不阻塞地多取，遇到非只读立即停止。
      // `pending` / `deferred_read_li` 非空说明还有更早的读未处理，不能再插批
      // 让新读插队（io-23）。
      let mut batch = vec![envelope];
      // 批提交路径与读窗口共用同一在飞上限：收集时只取剩余额度，避免把
      // 「提交不了」的读从通道里搬出来又放回去（io-23）。
      let quota = MAX_READ_IN_FLIGHT.saturating_sub(read_flight.len());
      if wake_li.is_empty()
        && pending.is_empty()
        && deferred_read_li.is_empty()
        && batch
          .first()
          .is_some_and(|envelope| is_read_op(&envelope.req))
      {
        while batch.len() < MAX_READ_BATCH && batch.len() < quota {
          match rx_hi.try_recv() {
            Ok(next) if is_read_op(&next.req) => batch.push(next),
            Ok(next) => {
              // 非只读的高优先请求：留待下一轮先跑。
              pending.push_back(next);
              break;
            }
            Err(_) => break,
          }
        }
      }
      // 只读请求（含单条）：串行准入 + 打开后 spawn 成独立任务，进入读并发
      // 窗口；不再 `join_all` 串行等到批结束。
      if batch
        .first()
        .is_some_and(|envelope| is_read_op(&envelope.req))
      {
        // 批提交也要遵守 `MAX_READ_IN_FLIGHT`（io-23）：按剩余额度切出本轮
        // 可提交的读，超出的放进 `deferred_read_li`，下一轮交读窗口等空位。
        // 阻塞读（目标分区正在写）不占在飞额度，仍走 blocked 队列。
        let mut runnable = Vec::with_capacity(batch.len());
        let mut free = quota;
        for envelope in batch {
          if is_blocked_read(&state, &envelope) {
            // 目标分区写进行中：等写完成再准入，不阻塞其它读。
            blocked_read_li.push_back(envelope);
          } else if free == 0 {
            deferred_read_li.push_back(envelope);
          } else {
            free -= 1;
            runnable.push(envelope);
          }
        }
        if !runnable.is_empty() {
          // 已有大值读在飞时，本轮提交不了的大值读进专门等待队列，等在飞
          // 那条完成后由循环顶部统一重排（io-05 / io-14）。
          let leftover_li = submit_read_li(
            &mut state,
            &thread_metrics,
            reopen_li,
            &mut read_flight,
            runnable,
            !large_park_li.is_empty(),
          )
          .await;
          if !leftover_li.is_empty() {
            // 大值读单飞：不进 `pending`。主循环不能在这里等「任意一条读
            // 完成」再重排——在飞的可能正是那条长 GetLarge，等待时长等于它
            // 的全程，且 `pending` 非空会抑制写派发（eng-34 / io-04）。挂进
            // `large_park_li` 后主循环照常推进写 / 低优先 / reopen，只在
            // 大值读完成（`large_read_out` 清空）后的循环顶重重排。
            if read_flight.is_empty() {
              // 防御：单飞标记与实际在飞不一致（正常不可达），清掉避免
              // leftover 永远重排不进去。
              state.large_read_out = false;
            }
            large_park_li.extend(leftover_li);
          }
        }
        continue;
      }
      let Some(envelope) = batch.pop() else {
        continue;
      };
      // 写类请求进 per-partition FIFO 队列（eng-33）：同一分区严格串行，
      // 跨分区按 ready 顺序推进，全局并发由 `max_write_concurrency` 限流。
      // `Reopen` / AttachShard / DetachShard / Shutdown 是控制请求，不进队列：
      // 前者有独立的延迟重试队列，后者必须旁路，否则会与排空逻辑互等。
      if is_write_op(&envelope.req) {
        // 先连续收一批高优先写请求，再统一进 gate；这样 drain 时
        // 同分区的 Put 能被 envelope_li commit 合并（一次 manifest fsync）。
        let mut write_li = vec![envelope];
        if pending.is_empty() {
          while write_li.len() < MAX_WRITE_COLLECT {
            match rx_hi.try_recv() {
              Ok(next) if is_write_op(&next.req) => write_li.push(next),
              Ok(next) => {
                pending.push_back(next);
                break;
              }
              Err(_) => break,
            }
          }
        }
        for envelope in write_li {
          let Some(tbl) = envelope.req.table().cloned() else {
            // 写类请求均带分区，此臂只为防御性兜底。
            if process_envelope(
              &mut state,
              &thread_metrics,
              reopen_li,
              envelope,
              config.oom_retry_times,
            )
            .await
            {
              break 'shard_loop;
            }
            continue;
          };
          let pri = pri_of(&envelope.req);
          gate.push(tbl, (envelope, pri));
        }
        let mut cx = DrainCx {
          state: &mut state,
          metrics: &thread_metrics,
          reopen_li: &mut *reopen_li,
          read_flight: &mut read_flight,
          write_flight: &mut write_flight,
          gate: &mut gate,
          oom_retry_times: config.oom_retry_times,
        };
        if drain_write_gate(&mut cx).await {
          break;
        }
        continue;
      }
      let was_detach = matches!(envelope.req, Request::DetachShard { .. });
      let was_attach = matches!(envelope.req, Request::AttachShard { .. });
      let lifecycle_before = state.lifecycle;
      // 控制请求（Reopen / ReopenExternal / Attach / Detach / Shutdown）可能改
      // 全局身份或取全部分区 Store 的独占：先等在飞读 / 写全部落地。读窗口
      // 因预算退出时主循环可能仍带着在飞读到达这里（io-08），不能假定窗口
      // 已替我们 drain。
      finish_read_li(&mut state, &thread_metrics, reopen_li, &mut read_flight).await;
      write_flight
        .drain(&mut state, &mut gate, &thread_metrics)
        .await;
      let stop = process_envelope(
        &mut state,
        &thread_metrics,
        reopen_li,
        envelope,
        config.oom_retry_times,
      )
      .await;
      if stop || write_queue_stale(was_attach, was_detach, lifecycle_before, state.lifecycle) {
        // detach / Fresh attach / shutdown：排队中的写与全部暂存读都属于旧身份，
        // 不能在新 branch / term 下执行，统一回 NotRunning。
        fail_stale_pending(
          &mut gate,
          &mut blocked_read_li,
          &mut large_park_li,
          &mut deferred_read_li,
          reopen_li,
        );
      }
      if stop {
        break;
      }
    }
  }
}
/// 高优先是否已连续服务超过时间片（`budget_us == 0` 表示关闭）。
///
/// 到点且有后台积压时，主循环先跑一条低优先再回到前台，对齐 C++
/// `ExecuteReadyTasks` 的「至少跑一个低优先防饿死」。
fn round_budget_due(budget_us: u64, round_start: Instant) -> bool {
  budget_us != 0
    && u64::try_from(round_start.elapsed().as_micros()).unwrap_or(u64::MAX) >= budget_us
}

/// 取到的是低优先请求时同步深度 gauge（eng-34c）。
///
/// 只在 `recv_prioritized` 的返回处调用：`try_recv` 弹出的两处（预算让路 /
/// 空闲）已经各自记账，在公共出口再减一次会双减。
fn dec_low_pq_if_low(metrics: &ShardMetric, envelope: Envelope) -> Envelope {
  if pri_of(&envelope.req) == Pri::Low {
    metrics.dec_low_pq_depth();
  }
  envelope
}

/// 等待下一条请求：高优先通道优先；一侧关闭后继续抽另一侧，两侧都关闭才
/// 返回 `None`。
///
/// 用 `futures::select` 而非跨线程阻塞 select：本函数跑在 shard 的 poll 栈上，
/// 禁止阻塞线程（compio thread-per-core 纪律）。
async fn recv_prioritized(
  rx_hi: &MAsyncRx<Array<Envelope>>,
  rx_lo: &MAsyncRx<Array<Envelope>>,
) -> Option<Envelope> {
  match select(Box::pin(rx_hi.recv()), Box::pin(rx_lo.recv())).await {
    Either::Left((Ok(envelope), _)) | Either::Right((Ok(envelope), _)) => Some(envelope),
    // 高优先通道关闭：低优先通道仍可能有货。
    Either::Left((Err(_), _)) => rx_lo.recv().await.ok(),
    Either::Right((Err(_), _)) => rx_hi.recv().await.ok(),
  }
}

/// 主循环的等待事件：新请求 / 写任务完成 / reopen 到期 / 通道关闭。
enum WaitEvent {
  Request(Envelope),
  /// Boxed：`WriteDone` 内含整个分区 `Store`，避免枚举被单一大变体撑大。
  WriteDone(Box<WriteDone>),
  ReopenDue,
  Closed,
}

/// 等待下一个可推进事件：请求、写任务完成、或到期重试。
///
/// 有在飞写时必须同时 poll 写完成：否则「pending 里的读在等某个分区写结束」
/// 时主循环会一直卡在 recv 上。`reopen_due` 已到期且仍有在飞写时不立即返回，
/// 先等一次写完成（写任务总会结束，避免零时长超时导致忙转）。
async fn wait_event(
  rx_hi: &MAsyncRx<Array<Envelope>>,
  rx_lo: &MAsyncRx<Array<Envelope>>,
  write_flight: &mut WriteFlight,
  reopen_due: Option<Instant>,
) -> WaitEvent {
  let recv = recv_prioritized(rx_hi, rx_lo);
  if write_flight.is_empty() {
    return match reopen_due {
      Some(due) => {
        // 用剩余时长而非绝对时刻：`due` 可能刚过去几微秒，`timeout_at` 会在
        // `deadline - now` 上 panic；0 时长即刻到期，回到顶部重试。
        let wait = due.saturating_duration_since(Instant::now());
        match timeout(wait, recv).await {
          Ok(Some(envelope)) => WaitEvent::Request(envelope),
          Ok(None) => WaitEvent::Closed,
          Err(_) => WaitEvent::ReopenDue,
        }
      }
      None => match recv.await {
        Some(envelope) => WaitEvent::Request(envelope),
        None => WaitEvent::Closed,
      },
    };
  }
  let select_fut = select(Box::pin(write_flight.next()), Box::pin(recv));
  // 到期但仍在写：等「写完成 / 新请求」，不设零时长超时（否则空转）。
  let wait = reopen_due
    .map(|due| due.saturating_duration_since(Instant::now()))
    .filter(|wait| !wait.is_zero());
  let outcome = match wait {
    Some(wait) => timeout(wait, select_fut).await.ok(),
    None => Some(select_fut.await),
  };
  match outcome {
    None => WaitEvent::ReopenDue,
    Some(Either::Left((Some(done), _))) => WaitEvent::WriteDone(Box::new(done)),
    Some(Either::Left((None, _))) => WaitEvent::Closed,
    Some(Either::Right((Some(envelope), _))) => WaitEvent::Request(envelope),
    Some(Either::Right((None, _))) => WaitEvent::Closed,
  }
}

/// 身份切换 / 关停时清理旧身份下排队的请求。
///
/// `DetachShard` / Fresh attach / shutdown 之后，写队列、各类暂存读与挂起的
/// reopen 都带旧 (branch, term)：在新身份下执行会作用到新 owner 的盘面，
/// 统一回 `NotRunning`（对齐 C++ 清理 `pending_queues_`）。
///
/// `deferred_read_li`（io-23 的批提交溢出读）与 `blocked_read_li` /
/// `large_park_li` 同属「因资源暂缓的读」，必须在同一清单里（io-25）：漏掉它
/// 会让 `Expired` 下暂存、等读窗口空位的读，在 Fresh attach 后经 `admit`
/// （此时已 `Attached`）按新 term 打开表并返回数据，term fencing 在只读路径
/// 上出现缺口。
pub(crate) fn fail_stale_pending(
  gate: &mut WriteGate<(Envelope, Pri)>,
  blocked_read: &mut VecDeque<Envelope>,
  large_park: &mut VecDeque<Envelope>,
  deferred_read: &mut VecDeque<Envelope>,
  reopen_li: &mut ReopenQueue,
) {
  fail_pending(gate.clear().into_iter().map(|(envelope, _)| envelope));
  fail_pending(blocked_read.drain(..));
  fail_pending(large_park.drain(..));
  fail_pending(deferred_read.drain(..));
  // 挂起的 reopen 同样带旧 (branch, term)：Fresh attach 后不能拿新身份
  // 重跑旧请求（会读到另一 branch 的数据），统一失败。
  reopen_li.fail_all(KvError::NotRunning);
}

/// 读目标分区是否正被写任务占用（Store 不在槽位里）。
fn is_blocked_read(state: &ShardState, envelope: &Envelope) -> bool {
  is_read_op(&envelope.req)
    && envelope
      .req
      .table()
      .is_some_and(|tbl| state.stores.is_writing(tbl))
}

/// 把已解除阻塞的读放回 `pending`；仍在写的留在原队列。
fn promote_blocked_reads(
  state: &ShardState,
  blocked: &mut VecDeque<Envelope>,
  pending: &mut VecDeque<Envelope>,
) {
  if blocked.is_empty() {
    return;
  }
  let mut remain = VecDeque::with_capacity(blocked.len());
  while let Some(envelope) = blocked.pop_front() {
    if envelope
      .req
      .table()
      .is_some_and(|tbl| state.stores.is_writing(tbl))
    {
      remain.push_back(envelope);
    } else {
      pending.push_back(envelope);
    }
  }
  *blocked = remain;
}

/// 该请求是否在等大值读单飞名额（io-14）。
///
/// 条件成立时不能放进 `pending`：`pending` 非空会抑制写派发（eng-34），而
/// 单飞名额由另一条大值读决定，等待时长可达该读全程。`deferred` 非空时即使
/// 当前没有大值读在飞（上一轮刚完成）也要继续排队，保证更早的等待项不被
/// 新到的大值读插队。
fn large_read_waiting(
  state: &ShardState,
  deferred: &VecDeque<Envelope>,
  envelope: &Envelope,
) -> bool {
  matches!(envelope.req, Request::GetLarge { .. }) && (state.large_read_out || !deferred.is_empty())
}

/// 大值读单飞名额空出后，把等待队列按到达顺序放回 `pending` 队首。
///
/// 逆序 `push_front` 一遍完成：整批保持原顺序，且排在其它暂存请求之前——
/// 它们是更早到达、只因资源（单飞名额 / fixed buffer）被阻塞的读。
fn promote_large_deferred(
  state: &ShardState,
  deferred: &mut VecDeque<Envelope>,
  pending: &mut VecDeque<Envelope>,
) {
  if deferred.is_empty() || state.large_read_out {
    return;
  }
  while let Some(envelope) = deferred.pop_back() {
    pending.push_front(envelope);
  }
}
