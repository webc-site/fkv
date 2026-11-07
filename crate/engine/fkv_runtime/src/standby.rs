//! standby 观测入口。

use fkv_api::{Request, Response};
use fkv_error::{KvError, Result};
use fkv_types::TableIdent;

use crate::{DEFAULT_OBSERVE_TIMEOUT, Runtime};

impl Runtime {
  /// 列举 standby master 上已有的分区（对齐 C++ `ListStandbyPartitionRequest`）。
  ///
  /// 非 standby（没有 master 来源）返回 `InvalidArgs`。当前实现固定发往 shard 0：
  /// 各 shard 的 standby service 共享同一份 master 来源，逐 shard 列举只会得到
  /// 相同结果；多 DS 托管与 `partition_filter` 已按范围外关闭（sto-61b），
  /// 不在此实现 per-ds_shard 过滤。
  ///
  /// 走观测路由（[`Self::route_of_shard`]）：shard 侧同步执行 `ssh` / `ls`，
  /// master 卡死时必须有 deadline 兜底，超时回 [`KvError::Timeout`]。deadline 取
  /// [`Self::with_request_timeout`] 的配置，未配置时用
  /// [`DEFAULT_OBSERVE_TIMEOUT`]。该请求不改变 attach 状态，也不走
  /// [`Self::reject_control_plane`]，因此「控制面观察请求不拒绝」的语义不变。
  pub async fn list_standby_partition(&self) -> Result<Vec<TableIdent>> {
    let route = self.route_of_shard(self.shard_by_id(0)?);
    let timeout = self.request_timeout.unwrap_or(DEFAULT_OBSERVE_TIMEOUT);
    match route
      .request_deadline(Request::ListStandbyPartition, timeout)
      .await?
    {
      Response::ListStandbyPartition(tbl_li) => Ok(tbl_li),
      _ => Err(KvError::Corrupted),
    }
  }
}
