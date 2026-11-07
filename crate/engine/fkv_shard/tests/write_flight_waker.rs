//! io-13 回归：非阻塞回收（`WriteFlight::try_next`）必须用事件循环的 waker
//! 轮询 `FuturesUnordered`，不能退化成 `now_or_never()` 的 noop waker。
//!
//! `FuturesUnordered::poll_next` 会把父 waker 换成传入的 waker；用 noop 轮询
//! 一次之后，子任务完成只唤醒 noop，写完成回收要等下一次同步 poll 才可见。
//! 这里用计数 waker 观察父 waker：非阻塞 poll 之后触发子任务完成，计数必须
//! 增加；旧实现（noop waker）计数不变。
use std::{
  future::Future,
  pin::Pin,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
  task::{Context, Poll, Wake, Waker},
};

use fkv_shard::poll_ready_once;
use futures::stream::FuturesUnordered;

/// 事件循环 waker 的替身：记录被唤醒次数。
struct CountWake(AtomicUsize);

impl CountWake {
  fn count(&self) -> usize {
    self.0.load(Ordering::SeqCst)
  }
}

impl Wake for CountWake {
  fn wake(self: Arc<Self>) {
    self.0.fetch_add(1, Ordering::SeqCst);
  }
  fn wake_by_ref(self: &Arc<Self>) {
    self.0.fetch_add(1, Ordering::SeqCst);
  }
}

/// 由测试控制完成时机的任务：poll 时把 `FuturesUnordered` 派生的子任务 waker
/// 交出来，`done` 置位后返回 Ready。
struct Gate {
  done: Arc<AtomicBool>,
  child: Arc<Mutex<Option<Waker>>>,
  out: u32,
}

impl Future for Gate {
  type Output = u32;

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
    if self.done.load(Ordering::SeqCst) {
      Poll::Ready(self.out)
    } else {
      *self.child.lock().unwrap_or_else(|err| err.into_inner()) = Some(cx.waker().clone());
      Poll::Pending
    }
  }
}

/// 子任务完成：用 `FuturesUnordered` 派生的子任务 waker 触发，通知父 waker。
fn complete_child(child: &Mutex<Option<Waker>>) {
  child
    .lock()
    .unwrap_or_else(|err| err.into_inner())
    .take()
    .expect("child waker must be registered by the first poll")
    .wake();
}

/// 一条 pending 任务 + 它的完成开关与子任务 waker 槽。
fn pending_task() -> (Gate, Arc<AtomicBool>, Arc<Mutex<Option<Waker>>>) {
  let done = Arc::new(AtomicBool::new(false));
  let child = Arc::new(Mutex::new(None));
  let gate = Gate {
    done: done.clone(),
    child: child.clone(),
    out: 0x13,
  };
  (gate, done, child)
}

/// 非阻塞回收之后父 waker 仍是调用方（事件循环）的 waker：写任务完成必须
/// 唤醒它，而不是唤醒已注销的 noop waker。
#[test]
fn nonblocking_poll_keeps_caller_waker() {
  let wake = Arc::new(CountWake(AtomicUsize::new(0)));
  let waker = Waker::from(wake.clone());
  let mut set = FuturesUnordered::new();
  let (gate, done, child) = pending_task();
  set.push(gate);

  // 循环顶部的非阻塞回收：poll 一次即返回，不等待。
  let mut cx = Context::from_waker(&waker);
  assert_eq!(poll_ready_once(&mut set, &mut cx), None);
  // 子任务完成 → 通知父 waker。旧实现（noop waker 轮询）这里计数不变。
  let before = wake.count();
  done.store(true, Ordering::SeqCst);
  complete_child(&child);
  assert_eq!(wake.count(), before + 1, "写完成通知必须回到事件循环 waker");

  // 下一次非阻塞回收直接取到结果。
  assert_eq!(poll_ready_once(&mut set, &mut cx), Some(0x13));
}

/// 每次非阻塞 poll 都把父 waker 更新成当次的调用方 waker：循环顶部反复
/// `try_next` 不会残留上一轮的 waker，完成通知只发给最近一次 poll 的 waker。
#[test]
fn nonblocking_poll_refreshes_parent_waker() {
  let first = Arc::new(CountWake(AtomicUsize::new(0)));
  let second = Arc::new(CountWake(AtomicUsize::new(0)));
  let mut set = FuturesUnordered::new();
  let (gate, done, child) = pending_task();
  set.push(gate);

  let first_waker = Waker::from(first.clone());
  let mut first_cx = Context::from_waker(&first_waker);
  assert_eq!(poll_ready_once(&mut set, &mut first_cx), None);

  let second_waker = Waker::from(second.clone());
  let mut second_cx = Context::from_waker(&second_waker);
  assert_eq!(poll_ready_once(&mut set, &mut second_cx), None);

  let (first_before, second_before) = (first.count(), second.count());
  done.store(true, Ordering::SeqCst);
  complete_child(&child);
  assert_eq!(first.count(), first_before, "旧的父 waker 不应再收到通知");
  assert_eq!(
    second.count(),
    second_before + 1,
    "通知必须发给最近一次 poll 的 waker"
  );
}
