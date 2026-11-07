//! 页首访在飞表：对齐 C++ `WaitingZone` 的首访 single-flight。
//!
//! 同一 `(tbl, page_id, generation)` 的并发读只允许首个读者（owner）读盘，
//! 其余挂在本表等待；owner 完成后把整页字节一次性广播给全部等待者，失败
//! （读盘 / 页校验 / 节点构造）按同一错误语义广播。owner 的 future 被 drop
//! （任务取消）时由 RAII 守卫唤醒等待者返回 `TryAgain`，不会永久挂起。
//!
//! 代际必须入键：只有快照代际相同的读者才共享同一次读盘，等待者拿到的页
//! 必然来自自己那代映射解析出的物理页。改动活跃映射的写路径
//! （`PageMapper::remap_page` / `set_file_page` / `free_page`）都会推进映射
//! 代际，因此并发 COW / 释放之后旧代在飞登记与新代读者落在不同的键上：
//! 新代读者各自读新代盘面，绝不会接到旧代的在飞结果，旧代读者也不会被
//! 新代条目顶掉。
//!
//! 单线程 shard 模型下本表用 `RefCell` 内部可变，等待者用 `oneshot` 挂起，
//! 无忙等、无锁。

use std::{cell::RefCell, rc::Rc};

use fkv_error::{KvError, Result};
use fkv_types::{PageId, TableIdent};
use futures_channel::oneshot;
use gxhash::HashMap;

/// owner 交付给等待者的整页字节：`Rc` 共享，等待者各自拷入自有页缓冲。
pub(crate) type PagePayload = Rc<Vec<u8>>;

/// 一次在飞登记：owner 的票号 + 本代等待者发送端。
#[derive(Debug)]
struct Inflight {
  ticket: u64,
  waiters: Vec<oneshot::Sender<Result<PagePayload>>>,
}

/// `enter` 的结果：owner 负责读盘，或等待 owner 广播结果。
pub(crate) enum LoadStep {
  Owner {
    ticket: u64,
  },
  Waiter {
    rx: oneshot::Receiver<Result<PagePayload>>,
  },
}

/// 页首访在飞表：shard / Store 单线程独占访问。
///
/// 外层按表分组，内层按 `(page_id, generation)`：表键在常规命中 / 结束时以
/// `&TableIdent` 借用查找，不为每次读克隆表名；表内无在飞项时整组摘除，
/// 表名不随历史读无限累积。
#[derive(Debug, Default)]
pub struct PageFlight {
  entries: HashMap<TableIdent, HashMap<(PageId, u64), Inflight>>,
  ticket_seq: u64,
}

impl PageFlight {
  /// 登记一次首访：已有同键在飞则作为等待者挂起，否则成为 owner。
  pub(crate) fn enter(&mut self, tbl: &TableIdent, page_id: PageId, generation: u64) -> LoadStep {
    self.ticket_seq = self.ticket_seq.wrapping_add(1);
    let ticket = self.ticket_seq;
    if let Some(per_table) = self.entries.get_mut(tbl) {
      if let Some(entry) = per_table.get_mut(&(page_id, generation)) {
        let (tx, rx) = oneshot::channel();
        entry.waiters.push(tx);
        return LoadStep::Waiter { rx };
      }
      per_table.insert(
        (page_id, generation),
        Inflight {
          ticket,
          waiters: Vec::new(),
        },
      );
      return LoadStep::Owner { ticket };
    }
    let mut per_table = HashMap::default();
    per_table.insert(
      (page_id, generation),
      Inflight {
        ticket,
        waiters: Vec::new(),
      },
    );
    self.entries.insert(tbl.clone(), per_table);
    LoadStep::Owner { ticket }
  }

  /// owner 结束：只摘**自己那代**（ticket 匹配）的登记并广播结果。
  ///
  /// ticket 匹配是必需的：登记被别的路径摘掉后同一键可能已被新 owner 接手，
  /// 旧 owner 完成时删掉它会删掉新 owner 的入口。仅在有等待者时才构造
  /// payload（`FnOnce` 延迟到确有等待者），常规无竞争首访零额外拷贝。
  pub(crate) fn finish(
    &mut self,
    tbl: &TableIdent,
    page_id: PageId,
    generation: u64,
    ticket: u64,
    outcome: Result<()>,
    payload: impl FnOnce() -> PagePayload,
  ) {
    let (entry, empty) = {
      let Some(per_table) = self.entries.get_mut(tbl) else {
        return;
      };
      if !per_table
        .get(&(page_id, generation))
        .is_some_and(|entry| entry.ticket == ticket)
      {
        return;
      }
      let entry = per_table.remove(&(page_id, generation));
      (entry, per_table.is_empty())
    };
    if empty {
      self.entries.remove(tbl);
    }
    let Some(entry) = entry else {
      return;
    };
    if entry.waiters.is_empty() {
      return;
    }
    match outcome {
      Ok(()) => {
        let payload = payload();
        for waiter in entry.waiters {
          // 等待者可能已被取消；发送失败不影响 owner。
          let _ = waiter.send(Ok(Rc::clone(&payload)));
        }
      }
      Err(err) => {
        for waiter in entry.waiters {
          let _ = waiter.send(Err(err));
        }
      }
    }
  }
}

/// owner 取消守卫：future 被 drop 时按 `TryAgain` 广播，避免等待者永久挂起。
pub(crate) struct LoadGuard<'a> {
  flight: &'a RefCell<PageFlight>,
  tbl: &'a TableIdent,
  page_id: PageId,
  generation: u64,
  ticket: u64,
  done: bool,
}

impl<'a> LoadGuard<'a> {
  pub(crate) fn new(
    flight: &'a RefCell<PageFlight>,
    tbl: &'a TableIdent,
    page_id: PageId,
    generation: u64,
    ticket: u64,
  ) -> Self {
    Self {
      flight,
      tbl,
      page_id,
      generation,
      ticket,
      done: false,
    }
  }

  /// owner 成功：把整页字节广播给等待者（仅当确有等待者时才拷贝）。
  pub(crate) fn finish_ok(mut self, page: &[u8]) {
    self.flight.borrow_mut().finish(
      self.tbl,
      self.page_id,
      self.generation,
      self.ticket,
      Ok(()),
      || Rc::new(page.to_vec()),
    );
    self.done = true;
  }

  /// owner 失败：按同一错误语义广播给等待者。
  pub(crate) fn finish_err(mut self, err: KvError) {
    self.flight.borrow_mut().finish(
      self.tbl,
      self.page_id,
      self.generation,
      self.ticket,
      Err(err),
      || Rc::new(Vec::new()),
    );
    self.done = true;
  }
}

impl Drop for LoadGuard<'_> {
  fn drop(&mut self) {
    if !self.done {
      // 任务取消 / panic：按 `TryAgain` 唤醒等待者，语义同云端 DownloadGuard。
      self.flight.borrow_mut().finish(
        self.tbl,
        self.page_id,
        self.generation,
        self.ticket,
        Err(KvError::TryAgain),
        || Rc::new(Vec::new()),
      );
    }
  }
}
