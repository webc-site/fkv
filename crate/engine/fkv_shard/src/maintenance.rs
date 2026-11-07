//! 写后维护：compact 放大文件、零引用文件回收与延迟 TTL 清理。

use fkv_error::KvError;
use fkv_types::TableIdent;
use log::warn;

use crate::{lifecycle::ShardState, partition::ShardStore};

/// 写后维护：compact 放大文件 + 回收零引用文件 + 清理延迟 TTL。
/// 对齐 C++ `WriteTask::UpdateMeta` 提交后调度的 `Compact` / `FileGc` /
/// `CleanExpired`；调用点在写请求回复之后，失败不改写结果。
///
/// 返回 `true` 表示维护观察到 `ExpiredTerm`：**当前 branch** 已被更高 term 的
/// owner 接管，调用方需把整个 shard 置 `Expired`（对齐 C++
/// `Shard::OnTaskFinished` 对任一任务 `ExpiredTerm` 调 `MarkShardExpired`）。
/// 其它 branch 的更高 term 由 `Store::file_gc` 按 active branch 过滤掉，只跳过
/// 本轮 GC、不改变生命周期（sem-26）。其它维护错误只告警。
pub(crate) async fn maintain_after_write(state: &mut ShardState, tbl: &TableIdent) -> bool {
  let needs_maintain = state.stores.needs_maintain(tbl);
  let pending_ttl = state.stores.pending_ttl_len() > 0;
  match state.store_mut(tbl) {
    Ok(store) => maintain_store(store, tbl, needs_maintain, pending_ttl).await,
    Err(err) => {
      warn!(
        target: "fkv_shard",
        "post-write maintain skipped tbl={} error={}",
        tbl,
        err
      );
      false
    }
  }
}

/// 单轮 Store 级维护：`ExpiredTerm` 转为返回值交给调用方收口，其它错误只告警。
///
/// 写后维护（[`maintain_store`]）与截断 / TTL 清理的内联维护共用，保证两处
/// 对 term 熔断的口径一致（sem-22）。
pub(crate) async fn maintain_once(store: &mut ShardStore, tbl: &TableIdent) -> bool {
  match store.maintain(tbl).await {
    Ok(_) => false,
    Err(KvError::ExpiredTerm) => true,
    Err(err) => {
      warn!(
        target: "fkv_shard",
        "post-write maintain failed tbl={} error={}",
        tbl,
        err
      );
      false
    }
  }
}

/// 写后维护的 Store 级实现；写任务在任务内直接复用（Store 由任务独占）。
///
/// 返回 `true` = 维护观察到**本 branch** 更高 term 的 manifest（`file_gc` 内部
/// 按 active branch 过滤）；`drain_pending_ttl` 目前不产生 `ExpiredTerm`，保持
/// 只告警（若未来 `clean_expired` 增加 term 栅栏，同步纳入）。
pub(crate) async fn maintain_store(
  store: &mut ShardStore,
  tbl: &TableIdent,
  needs_maintain: bool,
  pending_ttl: bool,
) -> bool {
  let expired = needs_maintain && maintain_once(store, tbl).await;
  if pending_ttl && let Err(err) = store.drain_pending_ttl().await {
    warn!(
      target: "fkv_shard",
      "post-write ttl drain failed error={}",
      err
    );
  }
  expired
}
