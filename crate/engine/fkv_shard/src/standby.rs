//! StandbyReplica 支持：reopen 前追赶 master 与远端分区 / 预热请求。

use fkv_api::{Request, Response};
use fkv_error::{KvError, Result};
use fkv_io::{ManifestIo, PageIo};
use fkv_task_prewarm::CompletionReason;
use fkv_types::TableIdent;
use log::warn;

use crate::lifecycle::ShardState;

/// `StandbyReplica`：reopen 前把该分区从 master 追一次。
///
/// 对齐 C++ `ReopenTask::Reopen`：非 standby（`standby == None`）直接返回；
/// 同步用当前 attach 到的 term 做 fencing（C++ 用 `store_->Term()`）。
/// 远端分区已不存在（`NotFound` / `ResourceMissing`）时按 C++ 的 `clear_local_state`
/// 语义只清本地 manifest（绝不删远端对象），并把 `ResourceMissing` 交给上层的
/// reopen 延迟队列重试；其余错误原样上抛。
pub(crate) async fn sync_standby_partition(state: &mut ShardState, tbl: &TableIdent) -> Result<()> {
  let Some(service) = state.standby.clone() else {
    return Ok(());
  };
  let result = service
    .sync_table_with_term(tbl, state.term.unwrap_or(0))
    .await;
  // 同步用 rename 替换本地 data / segment / manifest：旧 inode 的 FD 缓存与
  // 路径索引必须掉，否则随后的 reopen 会继续读被替换掉的旧文件（表现为
  // `Corrupted`）。对齐 C++ reopen 前重新打开分区文件。
  state.stores.backend().evict_table(tbl);
  match result {
    Ok(_) => Ok(()),
    Err(KvError::NotFound | KvError::ResourceMissing) => {
      warn!(
        target: "fkv_shard",
        "standby partition absent on master, drop local manifest tbl={tbl}"
      );
      state.stores.backend().drop_manifest(tbl).await?;
      Err(KvError::ResourceMissing)
    }
    Err(err) => Err(err),
  }
}
/// 远端 / 预备：Cloud cache 预热与 standby 远端分区列举。
pub(crate) async fn dispatch_standby(state: &mut ShardState, req: &Request) -> Result<Response> {
  match req {
    Request::Prewarm { tbl } => {
      // 仅 Cloud 后端能预热；Local / Mem / Standby 在 `StoreBackend::prewarm_table`
      // 内显式返回 `InvalidArgs`（不静默成功）。预热只写本地 cache，不碰
      // Store 的表状态，因此借 `&Store` 即可。
      let stats = state.stores.backend().inner().prewarm_table(tbl).await?;
      // 显式 Prewarm 请求必须把本轮失败回传给调用方（API 注释承诺
      // `Err(KvError)`）；后台 crond 走 `prewarm_tables`，仍以统计报告为准。
      match stats.completion_reason {
        CompletionReason::Success => Ok(Response::Prewarm),
        CompletionReason::DiskFull => Err(KvError::OutOfSpace),
        CompletionReason::Shutdown => Err(KvError::NotRunning),
        CompletionReason::ListingError | CompletionReason::DownloadError => Err(KvError::CloudErr),
      }
    }
    Request::ListStandbyPartition => {
      // 非 standby 模式没有 master 来源，显式拒绝；不走「返回空列表」的静默路径。
      let Some(service) = state.standby.clone() else {
        return Err(KvError::InvalidArgs);
      };
      Ok(Response::ListStandbyPartition(
        service.list_remote_partition_li().await?,
      ))
    }
    _ => Err(KvError::InvalidArgs),
  }
}
