//! 请求 → 指标 Op 映射、单请求指标记录与页缓存计数同步。

use std::time::Instant;

use fkv_api::{Request, Response};
use fkv_error::Result;
use fkv_metrics::{Op, ShardMetric};
use fkv_types::TableIdent;
use log::{debug, warn};

use crate::lifecycle::ShardState;

pub(crate) fn op_of(req: &Request) -> Option<Op> {
  match req {
    Request::Put { .. } | Request::PutLarge { .. } => Some(Op::Put),
    Request::Get { .. } | Request::GetLarge { .. } => Some(Op::Get),
    Request::Scan { .. } => Some(Op::Scan),
    Request::Reopen { .. } => Some(Op::Reopen),
    Request::ReopenExternal { .. } => Some(Op::Reopen),
    // C++ `ReopenRequest`（含 tag）就是 `RequestType::Reopen`。
    Request::ReopenArchive { .. } => Some(Op::Reopen),
    // C++ `FileGcRequest` / `LocalGcRequest` 各自对应一个 `RequestType`。
    Request::FileGc { .. } => Some(Op::FileGc),
    Request::LocalGc { .. } => Some(Op::LocalGc),
    Request::Compact { .. } => Some(Op::Compact),
    Request::Truncate { .. } => Some(Op::Truncate),
    Request::Floor { .. } => Some(Op::Floor),
    Request::Drop { .. } => Some(Op::Drop),
    Request::CleanExpired { .. } => Some(Op::CleanExpired),
    Request::CreateArchive { .. } => Some(Op::CreateArchive),
    // C++ `ArchiveRequest::Action::Delete` 的 `Type()` 同为 `Archive`（指标
    // 标签 `archive`），沿用 `Op::CreateArchive` 保证跨语言对账唯一。
    Request::DeleteArchive { .. } => Some(Op::CreateArchive),
    Request::CreateBranch { .. } => Some(Op::CreateBranch),
    Request::DeleteBranch { .. } => Some(Op::DeleteBranch),
    // fkv 扩展的 Cloud 预热请求；C++ 预热由 `PrewarmService` 后台触发，没有
    // 同名 `RequestType`，因此 `Op::Prewarm` 的 C++ 标签集为空。
    Request::Prewarm { .. } => Some(Op::Prewarm),
    // per-table 归档列举：C++ 只有未注册指标的 `global_list_archive_tags`，
    // 这里给 fkv 自有的控制面 Op，不参与跨语言对账。
    Request::ListArchiveTags { .. } => Some(Op::ListArchiveTags),
    Request::ListObject { .. } => Some(Op::ListObject),
    Request::ListStandbyPartition => Some(Op::ListStandbyPartition),
    Request::AttachShard { .. } => Some(Op::AttachShard),
    Request::DetachShard { .. } => Some(Op::DetachShard),
    Request::Shutdown => Some(Op::Shutdown),
  }
}
/// 把所有分区 Store 的页缓存累计计数与占用推入 shard 指标（全量对账）。
///
/// 只用于事件循环降频兜底：热路径用 [`sync_cache_metrics_for`] 只更新目标分区。
pub(crate) fn sync_cache_metrics(state: &mut ShardState, metrics: &ShardMetric) {
  state.stores.sync_cache_metrics(metrics);
}
/// 只把目标分区的页缓存指标推入 shard 指标（读 / 写完成热路径 O(1)）。
pub(crate) fn sync_cache_metrics_for(
  state: &mut ShardState,
  metrics: &ShardMetric,
  tbl: &TableIdent,
) {
  state.stores.sync_cache_metrics_for(metrics, tbl);
}
/// 完成路径的目标分区同步：没有分区信息（防御性兜底）时退回全量对账。
pub(crate) fn sync_cache_metrics_or_full(
  state: &mut ShardState,
  metrics: &ShardMetric,
  tbl: Option<&TableIdent>,
) {
  match tbl {
    Some(tbl) => sync_cache_metrics_for(state, metrics, tbl),
    None => sync_cache_metrics(state, metrics),
  }
}
/// 只推送待发布的页缓存占用 gauge（O(1)，事件循环每轮调用）。
pub(crate) fn flush_cache_bytes(state: &mut ShardState, metrics: &ShardMetric) {
  state.stores.flush_cache_bytes(metrics);
}
/// 记录单请求指标并打印状态行；读批与串行路径共用。
pub(crate) fn record_request(
  metrics: &ShardMetric,
  op: Option<Op>,
  started: Instant,
  result: &Result<Response>,
) {
  let Some(op) = op else {
    return;
  };
  let elapsed = started.elapsed();
  metrics.observe(op, elapsed, result.is_err());
  let latency_us = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
  match result {
    Ok(_) => debug!(
      target: "fkv_shard",
      "request shard_id={} op={} status=ok latency_us={}",
      metrics.shard_id(),
      op.as_str(),
      latency_us
    ),
    Err(err) => warn!(
      target: "fkv_shard",
      "request shard_id={} op={} status=error error={} latency_us={}",
      metrics.shard_id(),
      op.as_str(),
      err,
      latency_us
    ),
  }
}
