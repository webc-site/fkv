//! 在飞写任务：任务句柄、完成结果与非阻塞回收集合。

use std::{
  future::Future,
  pin::Pin,
  task::{Context, Poll},
};

use compio::runtime;
use fkv_metrics::ShardMetric;
use fkv_types::TableIdent;
use futures::{Stream, StreamExt, future::poll_fn, stream::FuturesUnordered};
use log::error;

use super::finish::{WriteDone, finish_write};
use crate::{
  lifecycle::ShardState,
  sender::{Envelope, Pri},
  write_gate::WriteGate,
};

/// 一条已派发的写任务：独占目标分区 Store，完成时把 Store 与缓存计数交回。
pub(crate) struct WriteTask {
  pub(super) tbl: TableIdent,
  pub(super) handle: runtime::JoinHandle<WriteDone>,
}

impl Future for WriteTask {
  type Output = WriteDone;

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    let this = self.get_mut();
    match Pin::new(&mut this.handle).poll(cx) {
      Poll::Ready(Ok(done)) => Poll::Ready(done),
      Poll::Ready(Err(err)) => {
        error!(
          target: "fkv_shard",
          "write task failed tbl={} error={}",
          this.tbl,
          err
        );
        // Store / buffer 已随任务丢失：按丢槽位处理（Backend 任务无槽位，
        // 仅触发一次后端缓存驱逐，无副作用）。任务没有可观察的结果，不置
        // shard 过期。
        Poll::Ready(WriteDone::Store {
          tbl: this.tbl.clone(),
          store: None,
          buf: None,
          expired: false,
          maintain_expired: false,
        })
      }
      Poll::Pending => Poll::Pending,
    }
  }
}

/// 非阻塞轮询一次 `FuturesUnordered`：父 waker 完全由调用方的 `Context` 决定。
///
/// `FuturesUnordered::poll_next` 第一件事就是把父 waker 换成传入的 waker
/// （futures-util 0.3.34 `stream/futures_unordered/mod.rs:420`），子任务完成时
/// 正是靠它反向唤醒父任务（同 crate `futures_unordered/task.rs:125` 的
/// `inner.waker.wake()`）。因此**不能**用 `now_or_never()` 轮询：它以 noop waker
/// 构造 `Context`（`future/future/mod.rs:590-602`），会把事件循环注册的 waker
/// 覆盖成 noop，此后写任务完成只唤醒 noop，回收要等下一次同步 poll（io-13）。
///
/// `#[doc(hidden)]`：它同时是集成测试入口——测试用计数 waker 直接验证
/// 「非阻塞 poll 之后父 waker 仍是调用方的 waker」，不必为测试暴露
/// `WriteFlight` 的内部任务类型。
#[doc(hidden)]
pub fn poll_ready_once<F: Future>(
  set: &mut FuturesUnordered<F>,
  cx: &mut Context<'_>,
) -> Option<F::Output> {
  match Pin::new(set).poll_next(cx) {
    Poll::Ready(item) => item,
    Poll::Pending => None,
  }
}

/// 在飞写任务集合；任务由 compio 独立推进，主循环只负责完成回调。
pub(crate) struct WriteFlight {
  task_set: FuturesUnordered<WriteTask>,
}

impl WriteFlight {
  pub(crate) fn new() -> Self {
    Self {
      task_set: FuturesUnordered::new(),
    }
  }

  pub(crate) fn is_empty(&self) -> bool {
    self.task_set.is_empty()
  }

  pub(crate) fn push(&mut self, task: WriteTask) {
    self.task_set.push(task);
  }

  /// 非阻塞回收已完成任务。
  ///
  /// 用 `poll_fn` 取到**事件循环当前的 waker** 再 poll 一次（闭包立即返回
  /// `Ready`，不会挂起）：`FuturesUnordered` 需要靠这条 waker 在写任务完成时
  /// 唤醒事件循环。旧实现 `now_or_never()` 用 noop waker 轮询，等于把事件循环
  /// 的 waker 注销掉，完成回收退化成被动轮询（io-13）。
  pub(crate) async fn try_next(&mut self) -> Option<WriteDone> {
    let set = &mut self.task_set;
    poll_fn(|cx| Poll::Ready(poll_ready_once(set, cx))).await
  }

  pub(crate) async fn next(&mut self) -> Option<WriteDone> {
    self.task_set.next().await
  }

  /// 等在飞写全部完成并交回 Store / 释放写门额度。
  pub(crate) async fn drain(
    &mut self,
    state: &mut ShardState,
    gate: &mut WriteGate<(Envelope, Pri)>,
    metrics: &ShardMetric,
  ) {
    while let Some(done) = self.task_set.next().await {
      finish_write(state, gate, metrics, done);
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{
    future::Future,
    pin::pin,
    sync::{
      Arc,
      atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
    time::Duration,
  };

  use compio::{runtime, runtime::spawn, time::sleep};
  use fkv_types::TableIdent;

  use super::{WriteDone, WriteFlight, WriteTask};

  /// 计数 waker：模拟 shard 事件循环的 waker。
  struct CountWake(AtomicUsize);

  impl Wake for CountWake {
    fn wake(self: Arc<Self>) {
      self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
      self.0.fetch_add(1, Ordering::SeqCst);
    }
  }

  /// `try_next` 必须用调用方的 waker 非阻塞轮询：在飞写任务完成的通知要回到
  /// 事件循环 waker，而不是被 `now_or_never()` 注销成 noop（io-13）。
  #[test]
  fn try_next_polls_with_caller_waker() {
    let runtime = runtime::Runtime::new().expect("compio runtime");
    runtime.block_on(async {
      let tbl = TableIdent::new("db.users", 0).expect("table ident");
      let mut flight = WriteFlight::new();
      let task_tbl = tbl.clone();
      // 只依赖共享后端的写任务：这里只关心完成通知，不关心执行体。
      flight.push(WriteTask {
        tbl,
        handle: spawn(async move {
          sleep(Duration::from_millis(1)).await;
          WriteDone::Backend {
            tbl: task_tbl,
            expired: false,
          }
        }),
      });
      // 循环顶部的非阻塞回收：任务还没在 runtime 上跑过，必然 Pending。
      let wake = Arc::new(CountWake(AtomicUsize::new(0)));
      let waker = Waker::from(wake.clone());
      let mut cx = Context::from_waker(&waker);
      {
        let mut next = pin!(flight.try_next());
        // 非阻塞回收不挂起：任务未完成时直接返回 `Ready(None)`。
        assert!(matches!(
          Future::poll(next.as_mut(), &mut cx),
          Poll::Ready(None)
        ));
      }
      // 让任务在 runtime 上完成：完成通知必须回到上面那条 waker。
      // 基准取在首次 poll 之后：`FuturesUnordered` 在本轮 poll 完所有子任务时
      // 会先显式 yield 唤醒一次父 waker，那一次不算完成通知。
      // 有界等待（不依赖固定时长）：通知没到就一直等到上限。
      let before = wake.0.load(Ordering::SeqCst);
      for _ in 0..100 {
        if wake.0.load(Ordering::SeqCst) > before {
          break;
        }
        sleep(Duration::from_millis(10)).await;
      }
      assert!(
        wake.0.load(Ordering::SeqCst) > before,
        "写完成通知必须回到事件循环 waker"
      );
      assert!(matches!(
        flight.try_next().await,
        Some(WriteDone::Backend { .. })
      ));
    });
  }
}
