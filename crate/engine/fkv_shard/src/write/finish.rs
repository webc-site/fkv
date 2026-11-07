//! 写任务完成回收：Store / fixed buffer 归还、写门额度释放与指标同步。

use fkv_error::KvError;
use fkv_io::FixedBuffer;
use fkv_metrics::ShardMetric;
use fkv_types::TableIdent;

use crate::{
  lifecycle::ShardState,
  metrics::sync_cache_metrics_for,
  partition::ShardStore,
  sender::{Envelope, Pri},
  write_gate::WriteGate,
};

/// 写任务完成结果。
///
/// `Store::store` 装箱：`ShardStore` 远大于 `Backend` 变体，平铺会把整个枚举
/// 撑大（所有完成路径都要搬运它）。
pub(crate) enum WriteDone {
  /// 任务独占过分区 Store：`Some` = 交回槽位；`None` = 任务 panic / 取消，
  /// Store 已随任务丢失，槽位按需 reopen。`buf` = 任务借出的 fixed buffer
  /// （大值写任务），完成后原样交回。`expired` = 任务结果是否 `ExpiredTerm`
  /// （panic / 取消无结果，恒为 `false`）：事件循环据此把整个 shard 置
  /// `Expired`，与串行 `handle` 共用同一收尾规则。`maintain_expired` = 写后 /
  /// 内联维护是否观察到**本 branch** 更高 term（`file_gc` 已按 active branch
  /// 过滤，sem-26）。
  Store {
    tbl: TableIdent,
    store: Option<Box<ShardStore>>,
    buf: Option<FixedBuffer>,
    expired: bool,
    maintain_expired: bool,
  },
  /// 任务只依赖共享后端（Prewarm 预热本地 cache），无 Store 需要归还（io-05）。
  Backend {
    tbl: TableIdent,
    /// 同 [`WriteDone::Store`]：任务结果是否 `ExpiredTerm`。
    expired: bool,
  },
}

impl WriteDone {
  /// `(任务结果是否 ExpiredTerm, 写后维护是否观察到本 branch 更高 term)`。
  ///
  /// 任务 panic / 取消没有结果可观察，按 `false` 处理（保持既有丢槽位语义，
  /// 不额外钉死生命周期）。
  fn expired_flags(&self) -> (bool, bool) {
    match self {
      Self::Store {
        expired,
        maintain_expired,
        ..
      } => (*expired, *maintain_expired),
      Self::Backend { expired, .. } => (*expired, false),
    }
  }
}

/// 写任务完成：Store 放回槽位（panic 则丢槽位）、释放写门额度、同步指标，
/// 并把任务的 `ExpiredTerm` 结果交给 [`ShardState::observe_task_err`]。
pub(crate) fn finish_write(
  state: &mut ShardState,
  gate: &mut WriteGate<(Envelope, Pri)>,
  metrics: &ShardMetric,
  done: WriteDone,
) {
  let (expired, maintain_expired) = done.expired_flags();
  let tbl = match done {
    WriteDone::Store {
      tbl,
      store: Some(store),
      buf,
      ..
    } => {
      state.stores.put_back(&tbl, *store);
      // 大值写任务借出的 fixed buffer 到期归还：后续大值读 / 写恢复直读路径。
      if let Some(buf) = buf {
        state.fixed_buffer = Some(buf);
      }
      gate.finish(&tbl);
      tbl
    }
    WriteDone::Store {
      tbl, store: None, ..
    } => {
      state.stores.lose(&tbl);
      state.table_open.forget(&tbl);
      gate.finish(&tbl);
      tbl
    }
    // Prewarm 等纯后端任务无 Store 可归还，只释放写门额度。
    WriteDone::Backend { tbl, .. } => {
      gate.finish(&tbl);
      tbl
    }
  };
  // 任务化写 / 维护 / 大值写 / Prewarm 与串行 `handle` 共用同一收尾规则：
  // 任一任务返回 `ExpiredTerm` 即整 shard 置 `Expired`（对齐 C++ `FinishTask`
  // 的 `MarkShardExpired`），后续普通请求经 `admit` 快速失败。隐式维护的
  // `ExpiredTerm` 已由 `Store::file_gc` 按 active branch 过滤：只有本 branch
  // 被更高 term 接管才走到这里，其它 branch 的更高 term 只跳过 GC（sem-26）。
  // panic / 取消的任务没有结果可观察，保持 `expired = false`，不额外钉死生命周期。
  if expired || maintain_expired {
    state.observe_task_err(&Err(KvError::ExpiredTerm));
  }
  // 只同步本分区：写任务独占该分区 Store，其它分区的缓存状态没有变化。
  sync_cache_metrics_for(state, metrics, &tbl);
}
