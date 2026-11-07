//! 单 shard 发送路由与收尾守卫（数据面 / 控制面共用）。

use core::future::Future;
use std::{
  sync::Arc,
  time::{Duration, Instant},
};

use fkv_api::{Request, Response};
use fkv_error::{KvError, Result};
use fkv_shard::{Shard, ShardLifecycle, ShardSender};
use fkv_types::TableIdent;

use crate::{
  Runtime,
  attach::{AttachEpoch, ShardViews},
};

/// 单 shard 的发送端 + 收尾句柄：**所有** shard 发送都必须经此。
///
/// 收尾规则：请求出错后读 shard 的真实 lifecycle（本地 `Timeout` 时请求可能已执行
/// 并把 shard 置为 `Expired`）；只要 shard 已不处于 `Attached`，且代际快照仍是
/// 当前代际，就清掉本层视图。
#[derive(Clone)]
pub(crate) struct ShardRoute {
  pub(crate) shard_id: u32,
  sender: ShardSender,
  lifecycle: ShardLifecycle,
  pub(crate) epoch: Arc<AttachEpoch>,
  views: ShardViews,
}

impl ShardRoute {
  /// 装配 shard 发送端 + 收尾句柄。
  ///
  /// `Runtime::start` 建 shard 列表时还没有 `Runtime`，故用自由函数形态复用。
  pub(crate) fn new(
    shard: &Shard,
    sender: ShardSender,
    epoch: &Arc<AttachEpoch>,
    views: &ShardViews,
  ) -> Self {
    Self {
      shard_id: shard.shard_id(),
      sender,
      lifecycle: shard.lifecycle().clone(),
      epoch: Arc::clone(epoch),
      views: views.clone(),
    }
  }

  /// 发送前快照：收尾时据此判断是否有更晚的 attach / detach。
  pub(crate) fn epoch_snapshot(&self) -> u64 {
    self.epoch.snapshot(self.shard_id)
  }

  /// 推进代际并返回新代际：本次 attach / detach 之后，早先请求的迟到错误
  /// 不再清视图。
  pub(crate) fn bump_epoch(&self) -> u64 {
    self.epoch.bump(self.shard_id)
  }

  /// 视图落地：仅当 `epoch` 仍是最新代际时推进代际并执行 `record`。
  ///
  /// 返回 `false` = 期间有更晚的 attach / detach，本次视图不得覆盖它。
  pub(crate) fn record_if_current(&self, epoch: u64, record: impl FnOnce()) -> bool {
    self
      .epoch
      .with_gate(self.shard_id, || {
        if self.epoch.bump_if_current(self.shard_id, epoch) {
          record();
          true
        } else {
          false
        }
      })
      .unwrap_or(false)
  }

  /// 视图失效：仅当 `epoch` 仍是最新代际时清掉。
  ///
  /// 迟到的 detach / 错误收尾不得清掉更晚 attach 刚记录的新视图。
  pub(crate) fn clear_if_current(&self, epoch: u64) -> bool {
    self.views.clear_if_epoch(&self.epoch, self.shard_id, epoch)
  }

  /// 发出请求并按真实 lifecycle 收尾（正常返回与 future 被取消两条路径都覆盖）。
  async fn track(&self, send: impl Future<Output = Result<Response>>) -> Result<Response> {
    let epoch = self.epoch_snapshot();
    let mut guard = FinishGuard {
      route: self,
      epoch,
      armed: true,
    };
    let result = send.await;
    guard.armed = false;
    self.finish(epoch, result)
  }

  pub(crate) async fn request(&self, req: Request) -> Result<Response> {
    // 调用方 drop / cancel 时 `finish` 不会执行：由 `track` 内的守卫兜底收尾。
    self.track(self.sender.request(req)).await
  }

  pub(crate) async fn request_deadline(&self, req: Request, timeout: Duration) -> Result<Response> {
    self.track(self.sender.request_deadline(req, timeout)).await
  }

  pub(crate) async fn request_at(&self, req: Request, deadline: Instant) -> Result<Response> {
    self.track(self.sender.request_at(req, deadline)).await
  }

  pub(crate) fn finish(&self, epoch: u64, result: Result<Response>) -> Result<Response> {
    match &result {
      // 本地 `Timeout`：请求可能已入队 / 已执行，shard 稍后才置 `Expired`。
      // 此刻读到 `Attached` 不代表视图失效，登记待复核；已接受的近似是复核
      // 之前视图可能短暂残留（`fkv_shard` 不提供 Expired 写点回调）。
      Err(KvError::Timeout) if self.lifecycle.is_attached() => {
        self.epoch.mark_timeout(self.shard_id, epoch);
      }
      Err(_) => {
        if !self.lifecycle.is_attached() {
          self.clear_if_current(epoch);
        }
      }
      Ok(_) => self.settle_timeout(),
    }
    result
  }

  /// 复核此前的超时登记：shard 现在已不 `Attached` 且代际未变才清视图。
  pub(crate) fn settle_timeout(&self) {
    let Some(epoch) = self
      .epoch
      .settle_timeout(self.shard_id, self.lifecycle.is_attached())
    else {
      return;
    };
    self.clear_if_current(epoch);
  }
}

/// `request*` 的收尾守卫：future 在 `.await` 未完成时被 drop / cancel，
/// 也必须按 shard 的真实 lifecycle 收尾，否则视图会残留到下一次错误。
struct FinishGuard<'a> {
  route: &'a ShardRoute,
  epoch: u64,
  /// 正常 `.await` 返回后置 `false`，收尾交给 [`ShardRoute::finish`]。
  armed: bool,
}

impl Drop for FinishGuard<'_> {
  fn drop(&mut self) {
    if self.armed && !self.route.lifecycle.is_attached() {
      self.route.clear_if_current(self.epoch);
    }
  }
}

impl Runtime {
  /// 按本运行时配置装配 sender：默认 deadline / 队列背压上限。
  fn sender_of(&self, shard: &Shard) -> ShardSender {
    shard
      .sender()
      .with_request_timeout(self.request_timeout)
      .with_queue_timeout(self.queue_timeout)
  }

  /// shard 发送端 + 收尾句柄（数据面默认 deadline）。
  pub(crate) fn route_of_shard(&self, shard: &Shard) -> ShardRoute {
    ShardRoute::new(
      shard,
      self.sender_of(shard),
      &self.attach_epoch,
      &self.views,
    )
  }

  /// 按 shard id 取数据面路由；生产路径都按 `TableIdent` 路由，仅测试需要。
  #[cfg(test)]
  pub(crate) fn route_of(&self, shard_id: u32) -> Result<ShardRoute> {
    Ok(self.route_of_shard(self.shard_by_id(shard_id)?))
  }

  /// 控制面路由：不套数据面 deadline / 队列超时。
  ///
  /// attach / detach 必须拿到确定性回执：本地超时会把 shard 状态留在半途
  /// （attach 已生效但调用方以为失败）。
  pub(crate) fn control_route(&self, shard_id: u32) -> Result<ShardRoute> {
    let shard = self.shard_by_id(shard_id)?;
    Ok(ShardRoute::new(
      shard,
      shard.sender(),
      &self.attach_epoch,
      &self.views,
    ))
  }

  pub(crate) fn route(&self, tbl: &TableIdent) -> ShardRoute {
    self.route_of_shard(self.shard(tbl))
  }

  fn shard(&self, tbl: &TableIdent) -> &Shard {
    let index = usize::from(tbl.shard_index(self.num_threads()));
    &self.shard_li[index]
  }

  pub(crate) fn shard_by_id(&self, shard_id: u32) -> Result<&Shard> {
    self
      .shard_li
      .get(shard_id as usize)
      .ok_or(KvError::InvalidArgs)
  }
}
