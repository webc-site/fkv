//! 任务化执行体：Put 组、大值写、维护 / 归档 / 分支与 Prewarm 四个任务。

use std::{sync::Arc, time::Instant};

use compio::runtime::spawn;
use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_error::{KvError, Result};
use fkv_io::FixedBuffer;
use fkv_metrics::{Op, ShardMetric};
use fkv_task_prewarm::CompletionReason;
use fkv_types::{TableIdent, WriteOp};

use super::{finish::WriteDone, flight::WriteTask};
use crate::{
  backend::StoreBackend,
  dispatch::gated_body,
  maintenance::maintain_store,
  metrics::{op_of, record_request},
  partition::ShardStore,
  sender::Envelope,
};

/// 把一条已准入的 Put 组派发为独立 compio task。
///
/// 任务持有分区 Store 的独占所有权：同分区后续写由 `WriteGate::running`
/// 挡住，不同分区各自的 Store 互不共享 `&mut`，IO 可以重叠。回执、指标、
/// 写后维护都在任务内完成；Store 通过 [`WriteDone`] 交回主循环。
pub(super) fn spawn_put_group(
  store: ShardStore,
  tbl: TableIdent,
  envelope_li: Vec<Envelope>,
  merged: Vec<BatchEntry>,
  metrics: Arc<ShardMetric>,
  oom_retry_times: u32,
) -> WriteTask {
  let task_tbl = tbl.clone();
  WriteTask {
    tbl,
    handle: spawn(async move {
      let mut store = store;
      let started = Instant::now();
      let mut result = Ok(());
      let mut remaining = oom_retry_times;
      loop {
        match store.put(&task_tbl, &merged).await {
          Ok(()) => break,
          Err(KvError::OutOfMem) if remaining > 0 => remaining -= 1,
          Err(err) => {
            result = Err(err);
            break;
          }
        }
      }
      // 整组共享一次 Store::put 的结果：记录 term 失效标记，组内每个请求回执
      // 后由事件循环统一把 shard 置 Expired（对齐 C++ FinishTask）。写后维护
      // 在 GC 时看到更高 term 也记为维护标记（sem-22）。
      let expired = matches!(result, Err(KvError::ExpiredTerm));
      let mut maintain_expired = false;
      for envelope in envelope_li {
        // 合并阶段已 `mem::take` 走批内容；任务路径只消费 reply 通道。
        debug_assert!(
          matches!(&envelope.req, Request::Put { batch_li, .. } if batch_li.is_empty()),
          "put group task must not read merged batch content"
        );
        let response = result.map(|()| Response::Put);
        record_request(&metrics, Some(Op::Put), started, &response);
        envelope.send(response);
      }
      if result.is_ok() {
        let needs_maintain =
          store.needs_maintain(&task_tbl) || store.needs_segment_maintain(&task_tbl);
        let pending_ttl = store.pending_ttl_len() > 0;
        maintain_expired = maintain_store(&mut store, &task_tbl, needs_maintain, pending_ttl).await;
      }
      WriteDone::Store {
        tbl: task_tbl,
        store: Some(Box::new(store)),
        buf: None,
        expired,
        maintain_expired,
      }
    }),
  }
}

/// 大值写执行体：fixed buffer 可用且装得下时走固定缓冲路径，否则普通大值写。
/// 串行分发（防御路径）与任务化路径共用（io-05）。
pub(crate) async fn put_large_body(
  store: &mut ShardStore,
  tbl: &TableIdent,
  batch_li: &[BatchEntry],
  buf: Option<&mut FixedBuffer>,
) -> Result<()> {
  let max_len = batch_li
    .iter()
    .filter(|entry| entry.op == WriteOp::Upsert)
    .map(|entry| entry.value.len())
    .max()
    .unwrap_or(0);
  if let Some(buf) = buf
    && max_len <= buf.len()
  {
    store.put_large_fixed(tbl, batch_li, buf).await
  } else {
    store.put_large(tbl, batch_li).await
  }
}

/// 维护 / 归档 / 分支类任务：独占 Store 执行 [`gated_body`]，回执后把 Store
/// 交回主循环。对齐 C++ 这些操作在 IO 边界 `Yield` / `YieldToLowPQ` 的交错
/// 语义：前台请求的等待以单个 IO 为界，而不是以整个维护操作为界。
pub(super) fn spawn_maintain_task(
  mut store: ShardStore,
  tbl: TableIdent,
  envelope: Envelope,
  shift: u8,
  metrics: Arc<ShardMetric>,
  oom_retry_times: u32,
) -> WriteTask {
  let task_tbl = tbl.clone();
  WriteTask {
    tbl,
    handle: spawn(async move {
      let started = Instant::now();
      let op = op_of(&envelope.req);
      // OutOfMem 重试与串行路径一致：请求已回滚，从干净状态重跑。
      let mut remaining = oom_retry_times;
      let (result, maintain_expired) = loop {
        match gated_body(&mut store, &envelope.req, shift).await {
          (Ok(response), expired) => break (Ok(response), expired),
          (Err(KvError::OutOfMem), _) if remaining > 0 => remaining -= 1,
          (Err(err), expired) => break (Err(err), expired),
        }
      };
      // 截断 / TTL 清理成功后的内联维护若看到更高 term，记为维护标记（回执
      // 保持成功；对齐 C++ FinishTask，sem-22）。
      let expired = matches!(&result, Err(KvError::ExpiredTerm));
      record_request(&metrics, op, started, &result);
      envelope.send(result);
      WriteDone::Store {
        tbl: task_tbl,
        store: Some(Box::new(store)),
        buf: None,
        expired,
        maintain_expired,
      }
    }),
  }
}

/// 大值写任务：独占 Store 与（若空闲）fixed buffer，回执后连同写后维护
/// 一起完成，Store 与 buffer 经 [`WriteDone`] 交回（io-05）。
pub(super) fn spawn_put_large_task(
  mut store: ShardStore,
  tbl: TableIdent,
  envelope: Envelope,
  buf: Option<FixedBuffer>,
  metrics: Arc<ShardMetric>,
  oom_retry_times: u32,
) -> WriteTask {
  let task_tbl = tbl.clone();
  WriteTask {
    tbl,
    handle: spawn(async move {
      let started = Instant::now();
      let mut buf = buf;
      let mut remaining = oom_retry_times;
      let result = loop {
        let body = match &envelope.req {
          Request::PutLarge { tbl, batch_li, .. } => {
            put_large_body(&mut store, tbl, batch_li, buf.as_mut()).await
          }
          _ => Err(KvError::InvalidArgs),
        };
        match body {
          Ok(()) => break Ok(Response::Put),
          Err(KvError::OutOfMem) if remaining > 0 => remaining -= 1,
          Err(err) => break Err(err),
        }
      };
      record_request(&metrics, Some(Op::Put), started, &result);
      let expired = matches!(&result, Err(KvError::ExpiredTerm));
      let write_ok = result.is_ok();
      envelope.send(result);
      let mut maintain_expired = false;
      if write_ok {
        let needs_maintain =
          store.needs_maintain(&task_tbl) || store.needs_segment_maintain(&task_tbl);
        let pending_ttl = store.pending_ttl_len() > 0;
        maintain_expired = maintain_store(&mut store, &task_tbl, needs_maintain, pending_ttl).await;
      }
      WriteDone::Store {
        tbl: task_tbl,
        store: Some(Box::new(store)),
        buf,
        expired,
        maintain_expired,
      }
    }),
  }
}

/// Prewarm 任务：只拉远端文件到本地 cache，不占分区 Store；同分区写 FIFO
/// 仍由写门 `running` 保持（io-05）。
pub(super) fn spawn_prewarm_task(
  backend: StoreBackend,
  tbl: TableIdent,
  envelope: Envelope,
  metrics: Arc<ShardMetric>,
) -> WriteTask {
  let task_tbl = tbl.clone();
  WriteTask {
    tbl,
    handle: spawn(async move {
      let started = Instant::now();
      let result = match backend.prewarm_table(&task_tbl).await {
        Ok(stats) => match stats.completion_reason {
          CompletionReason::Success => Ok(Response::Prewarm),
          CompletionReason::DiskFull => Err(KvError::OutOfSpace),
          CompletionReason::Shutdown => Err(KvError::NotRunning),
          CompletionReason::ListingError | CompletionReason::DownloadError => {
            Err(KvError::CloudErr)
          }
        },
        Err(err) => Err(err),
      };
      let expired = matches!(&result, Err(KvError::ExpiredTerm));
      record_request(&metrics, Some(Op::Prewarm), started, &result);
      envelope.send(result);
      WriteDone::Backend {
        tbl: task_tbl,
        expired,
      }
    }),
  }
}
