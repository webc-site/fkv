//! 单飞整文件下载：在飞登记、等待者队列与 owner 取消守卫。

use core::future::Future;

use fkv_error::{KvError, Result};
use fkv_types::{FileId, TableIdent};
use futures::channel::oneshot;

use crate::CloudStore;

/// 单飞下载键：同一文件的并发冷读只允许一个 owner 回源。
pub(crate) type DownloadKey = (TableIdent, bool, FileId);

/// 单飞下载结果：owner 已把文件装入本地，或远端确认不存在。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DownloadOutcome {
  Cached,
  Missing,
}

/// owner 下载期间挂起的等待者；owner 完成后统一下发结果。
pub(crate) type DownloadWaiter = oneshot::Sender<Result<DownloadOutcome>>;

/// 在飞下载登记：`generation` 标识当前 owner。
///
/// `evict_table` 摘掉登记后，同一 key 会被新一代 owner 重新登记；旧 owner
/// 完成 / 取消时只能摘**自己那代**的登记，否则会删掉新 owner 的入口并把旧
/// 结果发给新 owner 的等待者（同一 key 双 owner 重复下载、`<file>.tmp`
/// 竞争、等待者拿到错误结果）。
#[derive(Debug)]
pub(crate) struct InflightDownload {
  pub(crate) generation: u64,
  pub(crate) waiter_li: Vec<DownloadWaiter>,
}

/// owner 取消（future 被 drop）时移除在飞登记并唤醒等待者，避免永久挂起。
pub(crate) struct DownloadGuard<'a, I, O> {
  pub(crate) store: &'a CloudStore<I, O>,
  pub(crate) key: DownloadKey,
  /// 本 owner 的世代号：完成时只摘自己这代登记。
  pub(crate) generation: u64,
  pub(crate) done: bool,
}

impl<I, O> Drop for DownloadGuard<'_, I, O> {
  fn drop(&mut self) {
    if !self.done {
      self
        .store
        .finish_download(&self.key, self.generation, Err(KvError::TryAgain));
    }
  }
}

impl<I, O> DownloadGuard<'_, I, O> {
  pub(crate) fn finish(mut self, outcome: Result<DownloadOutcome>) -> Result<DownloadOutcome> {
    self
      .store
      .finish_download(&self.key, self.generation, outcome);
    self.done = true;
    outcome
  }
}

impl<I, O> CloudStore<I, O> {
  /// 分配一个新的在飞下载世代号。
  pub(crate) fn next_download_generation(&self) -> u64 {
    let generation = self.download_generation.get().wrapping_add(1);
    self.download_generation.set(generation);
    generation
  }
}

impl<I, O> CloudStore<I, O> {
  /// 结束一次在飞下载：移除**本代**登记并唤醒本代等待者（取消路径传 `Err`）。
  ///
  /// 只按 `generation` 命中自己那代：`evict_table` 摘掉登记后同一 key 可能已被
  /// 新一代 owner 接手，旧 owner 完成时删掉它会把旧结果发给新登记的等待者。
  pub(crate) fn finish_download(
    &self,
    key: &DownloadKey,
    generation: u64,
    outcome: Result<DownloadOutcome>,
  ) {
    let waiter_li = {
      let mut map = self.inflight_download.borrow_mut();
      match map.get(key) {
        Some(entry) if entry.generation == generation => {
          // 组内在飞登记由 `begin_segment_install` / `SegmentInflightGuard`
          // 的 RAII 守卫负责（预热直连安装也走那条路），单飞只负责等待者队列。
          map.remove(key).map(|entry| entry.waiter_li)
        }
        _ => None,
      }
    };
    for waiter in waiter_li.into_iter().flatten() {
      // 等待者可能已被取消；发送失败不影响 owner。
      let _ = waiter.send(outcome);
    }
  }
}

impl<I, O> CloudStore<I, O> {
  /// 同一个 `(table, segment, file_id)` 的并发冷读只回源一次。
  ///
  /// 后到者挂起等待 owner 的 `Cached` / `Missing`；owner 被取消时由
  /// [`DownloadGuard`] 唤醒等待者并返回 `TryAgain`，不会永久挂起。
  ///
  /// 返回值第二项表示**本任务是否是 owner**：等待者拿到的 `Cached` 只代表
  /// owner 执行过 fetch，不保证覆盖等待者自己的起点（见
  /// [`Self::ensure_resident`]）。
  pub(crate) async fn single_flight<F, Fut>(
    &self,
    key: DownloadKey,
    fetch: F,
  ) -> Result<(DownloadOutcome, bool)>
  where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<DownloadOutcome>>,
  {
    let (waiter, generation) = {
      let mut map = self.inflight_download.borrow_mut();
      match map.get_mut(&key) {
        Some(entry) => {
          let (tx, rx) = oneshot::channel();
          entry.waiter_li.push(tx);
          (Some(rx), entry.generation)
        }
        None => {
          let generation = self.next_download_generation();
          map.insert(
            key.clone(),
            InflightDownload {
              generation,
              waiter_li: Vec::new(),
            },
          );
          (None, generation)
        }
      }
    };
    if let Some(rx) = waiter {
      // owner 发送的是 `Result<DownloadOutcome>`；先解 oneshot 取消，再传播
      // owner 的回源错误。
      let outcome = rx.await.map_err(|_| KvError::TryAgain)??;
      return Ok((outcome, false));
    }
    let guard = DownloadGuard {
      store: self,
      key,
      generation,
      done: false,
    };
    guard.finish(fetch().await).map(|outcome| (outcome, true))
  }
}
