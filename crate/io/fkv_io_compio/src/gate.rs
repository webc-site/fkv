//! IO 层写闸门与 fdatasync 在飞计数。

use std::{
  cell::{Cell, RefCell},
  future::poll_fn,
  task::Poll,
};

use crate::{SyncInflight, SyncInflightGuard, WriteGate, WritePermit};

impl Drop for WritePermit<'_> {
  fn drop(&mut self) {
    self.gate.release();
  }
}

impl WriteGate {
  pub(crate) const fn new(limit: usize) -> Self {
    Self {
      limit,
      active: Cell::new(0),
      waiter_li: RefCell::new(Vec::new()),
    }
  }

  /// 装配期覆盖容量。只能在闸门空闲（尚未取过名额）时调用。
  pub(crate) fn set_limit(&mut self, limit: usize) {
    debug_assert_eq!(self.active.get(), 0, "WriteGate 必须在空闲时改容量");
    self.limit = limit;
  }

  /// 取一个名额；已满时注册 waker 挂起，由归还方唤醒。
  pub(crate) fn acquire(&self) -> impl Future<Output = WritePermit<'_>> + '_ {
    poll_fn(move |cx| {
      if self.active.get() < self.limit {
        self.active.set(self.active.get() + 1);
        return Poll::Ready(WritePermit { gate: self });
      }
      let mut waiter_li = self.waiter_li.borrow_mut();
      if !waiter_li.iter().any(|waker| waker.will_wake(cx.waker())) {
        waiter_li.push(cx.waker().clone());
      }
      Poll::Pending
    })
  }

  fn release(&self) {
    self.active.set(self.active.get().saturating_sub(1));
    for waker in self.waiter_li.borrow_mut().drain(..) {
      waker.wake();
    }
  }
}

impl SyncInflight {
  pub(crate) fn enter(&self) -> SyncInflightGuard<'_> {
    let now = self.now.get() + 1;
    self.now.set(now);
    if now > self.peak.get() {
      self.peak.set(now);
    }
    SyncInflightGuard { now: &self.now }
  }

  pub(crate) fn peak(&self) -> usize {
    self.peak.get()
  }
}

impl Drop for SyncInflightGuard<'_> {
  fn drop(&mut self) {
    self.now.set(self.now.get().saturating_sub(1));
  }
}
