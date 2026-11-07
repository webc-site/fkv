//! 低优先通道深度 gauge 的回滚验收。
//!
//! `low_pq_depth` 在入队侧先 +1，投递成功交给消费侧 -1；投递失败或 future 被
//! drop 必须由 `LowPqGuard` 回滚。否则关停 / 取消后 gauge 会残留非零，调度
//! 指标永久失真。

use std::{
  future::Future,
  path::PathBuf,
  pin::Pin,
  task::{Context, Poll},
  time::Duration,
};

use compio::{runtime::spawn, time::sleep};
use fkv_api::{Request, ScanPageOption};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_store::AllocMode;
use fkv_testkit::TempRoot;
use fkv_types::{StoreMode, TableIdent};
use futures::task::noop_waker;

/// 积压规模：远大于 `queue_size = 1`。用 `CreateArchive`（写唯一 tag 的归档文件
/// + prune）而不是 `Gc`（空表近似 no-op），保证低优先通道在观测窗口内保持满。
const BACKLOG_NUM: usize = 512;

/// 全表扫描的数据规模：`SCAN_ROUNDS × SCAN_BATCH` 条 entry（512B 页下约一
/// entry 一页），让一条 Scan 要读数千页、耗时数十毫秒，把事件循环稳定留在
/// 读窗口里。单批 entry 数要低于本配置写路径的批量字节上限（400 条 300B
/// value 的批会被拒）。
const SCAN_ROUNDS: usize = 20;
const SCAN_BATCH: usize = 350;

fn config(root: PathBuf) -> ShardConfig {
  ShardConfig {
    alloc_mode: AllocMode::Append,
    shard_id: 0,
    root,
    page_size: 512,
    restart_interval: 16,
    index_restart_interval: 16,
    pages_per_file_shift: 18,
    segment_size: 4096,
    segments_per_file_shift: 2,
    large_value_threshold: 1024,
    fixed_buffer_size: 0,
    // 故意极小：通道必然被打满。
    queue_size: 1,
    auto_reopen_retry_times: 0,
    auto_reopen_pending_time_us: 0,
    oom_retry_times: 0,
    compression_level: None,
    verify_checksum: true,
    overflow_pointers: fkv_page::DEFAULT_OVERFLOW_POINTERS,
    file_amplify_factor: 0,
    segment_file_amplify_factor: 0,
    manifest_limit: 8 * 1024 * 1024,
    max_write_batch_pages: 32,
    max_inflight_write: 32 * 1024,
    fd_limit: 0,
    index_cache_bytes: fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize,
    index_cache_total_bytes: fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize,
    root_meta_cache_bytes: 0,
    io_queue_size: 4096,
    backend: Backend::default(),
    mode: StoreMode::Local,
    standby: None,
    max_write_concurrency: 0,
    max_processing_time_us: 0,
    yield_budget_us: 0,
  }
}

fn low_req(tbl: &TableIdent, tag: String) -> Request {
  Request::CreateArchive {
    tbl: tbl.clone(),
    tag,
    num_retained: 1,
  }
}

/// 只 poll 一次：让请求完成到「登记 send」这一步，但不推进到完成。
fn poll_once<F: Future>(fut: Pin<&mut F>) -> Poll<F::Output> {
  let waker = noop_waker();
  let mut cx = Context::from_waker(&waker);
  fut.poll(&mut cx)
}

async fn attach_and_put(shard: &Shard, tbl: &TableIdent) {
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
  shard
    .request(Request::Put {
      tbl: tbl.clone(),
      batch_li: vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      term: Some(1),
    })
    .await
    .unwrap();
}

/// 入队失败（shard 已停，通道断开）：深度必须回滚到 0。
#[compio::test]
async fn enqueue_failure_rolls_back_low_depth() {
  let root = TempRoot::new("shard-backpressure");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  let metrics = shard.metrics().clone();
  let sender = shard.sender();
  let tbl = TableIdent::new("enqueue_fail", 1).unwrap();
  shard.shutdown().await.unwrap();

  assert_eq!(
    sender
      .request(low_req(&tbl, "closed".to_owned()))
      .await
      .unwrap_err(),
    KvError::NotRunning,
    "shard 停止后入队必须失败"
  );
  assert_eq!(
    metrics.snapshot().low_pq_depth,
    0,
    "入队失败后低优先深度必须回滚"
  );
}

/// 请求已记账但还没入队时被 drop（调用方取消）：深度必须回滚。
///
/// 维护请求任务化（io-05）后事件循环消费低优先通道不再被内联 IO 挡住，
/// 通道不会因慢消费自然保持满；这里用一条全表扫描把循环留在读窗口
/// （窗口只消费高优先通道），低优先通道才能稳定塞满。
#[compio::test]
async fn dropped_pending_low_request_rolls_back_depth() {
  let root = TempRoot::new("shard-backpressure");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  let tbl = TableIdent::new("drop_depth", 1).unwrap();
  attach_and_put(&shard, &tbl).await;
  // 建一张有足够多页的表（512B 页 × 300B value ≈ 一 entry 一页）：
  // 一条大范围 Scan 要读数万页，读窗口足够长时间保持活跃。
  for round in 0..SCAN_ROUNDS {
    let batch_li = (0..SCAN_BATCH)
      .map(|index| {
        let key = format!("key-{:08}", round * SCAN_BATCH + index);
        BatchEntry::upsert(key.into_bytes(), vec![b'v'; 300], 1)
      })
      .collect();
    shard
      .request(Request::Put {
        tbl: tbl.clone(),
        batch_li,
        term: Some(1),
      })
      .await
      .unwrap();
  }
  let scan_sender = shard.sender();
  let scan_tbl = tbl.clone();
  let scan = spawn(async move {
    scan_sender
      .request(Request::Scan {
        tbl: scan_tbl,
        begin: b"key-".to_vec(),
        end: b"key~".to_vec(),
        max_entries: usize::MAX,
        option: ScanPageOption::default(),
      })
      .await
  });
  sleep(Duration::from_millis(5)).await;

  // 先塞满低优先通道，保证新请求的 send 一定处于 Pending。
  let mut backlog_li = Vec::with_capacity(BACKLOG_NUM);
  for index in 0..BACKLOG_NUM {
    let sender = shard.sender();
    let tbl = tbl.clone();
    let tag = format!("{index:020}");
    backlog_li.push(spawn(
      async move { sender.request(low_req(&tbl, tag)).await },
    ));
  }
  sleep(Duration::from_millis(10)).await;
  assert!(
    shard.metrics_snapshot().low_pq_depth > 0,
    "积压没有进入低优先通道，测试前提不成立"
  );

  let sender = shard.sender();
  let mut pending = Box::pin(sender.request(low_req(&tbl, "pending".to_owned())));
  assert!(
    poll_once(pending.as_mut()).is_pending(),
    "通道满时第一次 poll 必须是 Pending"
  );
  // 只 poll 过一次就 drop：`LowPqGuard` 必须把入队侧的 +1 回滚。
  // 必须用 `Box::pin`：`pin_mut!` 把 future 藏在作用域内的临时变量里，
  // drop 掉 `Pin<&mut _>` 并不会 drop 真正的 future。
  drop(pending);

  scan.await.unwrap().unwrap();
  for handle in backlog_li {
    handle.await.unwrap().unwrap();
  }
  assert_eq!(
    shard.metrics_snapshot().low_pq_depth,
    0,
    "被 drop 的低优先请求必须回滚深度"
  );

  shard.shutdown().await.unwrap();
}
