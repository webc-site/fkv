//! 请求与 shutdown 竞态：提交进行中调用 shutdown，被接受的请求必须全部 resolve
//! （对齐 C++ `eloqstore/tests/persist.cpp` 的 `requests racing Stop all complete`）。
//!
//! 复现 test-05 死锁：`Shard::shutdown` 内联 `join.join()` 阻塞调用方 runtime
//! 线程后，shard 线程 `fail_pending` 的回执唤醒打满调用方 compio runtime 的
//! 跨线程同步队列（容量 64），`Remote::schedule` 满队自旋，双方互等。场景必须
//! 跑在独立线程上：死锁时 poll 线程被 join 卡死，async 超时无法抢占，只能由
//! 主测试线程限时观察，表现为测试失败而非挂死 CI。

use std::{
  any::Any,
  panic::{AssertUnwindSafe, catch_unwind},
  path::PathBuf,
  thread,
  time::Duration,
};

use compio::{
  runtime::{Runtime, spawn},
  time::timeout,
};
use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_store::AllocMode;
use fkv_testkit::TempRoot;
use fkv_types::{StoreMode, TableIdent};
use futures_channel::oneshot;

/// 在飞写条数：远超 compio 跨线程同步队列容量（64），保证 shutdown 时回执
/// 积压足以打满队列（test-05 复现参数 512 条）。
const WRITE_NUM: usize = 512;
/// value 尺寸（test-05 复现参数 400B）。
const VALUE_LEN: usize = 400;
/// 场景限时：修复前死锁在此表现为失败；修复后 512 条本地写远低于该时限。
const SCENE_DEADLINE: Duration = Duration::from_secs(15);

/// `queue_size` 压到 16 放大积压，其余与 write_gate 测试一致（本地盘后端）。
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
    queue_size: 16,
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
    fd_limit: 10_000,
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

fn put(tbl: &TableIdent, index: usize) -> Request {
  Request::Put {
    tbl: tbl.clone(),
    batch_li: vec![BatchEntry::upsert(
      format!("key-{index:04}").into_bytes(),
      vec![b'v'; VALUE_LEN],
      index as u64,
    )],
    term: Some(1),
  }
}

/// 竞态场景：attach 后连发 512 条写，提交进行中（不 await 任何回执）直接
/// shutdown，再等全部回执；每条请求要么写成功、要么排空为 NotRunning。
async fn race_shutdown(root: PathBuf) {
  let shard = Shard::start(config(root.clone())).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
  let tbl = TableIdent::new("race", 1).unwrap();
  let mut handle_li = Vec::with_capacity(WRITE_NUM);
  for index in 0..WRITE_NUM {
    let sender = shard.sender();
    let tbl = tbl.clone();
    handle_li.push(spawn(async move { sender.request(put(&tbl, index)).await }));
  }
  shard.shutdown().await.unwrap();
  for (index, handle) in handle_li.into_iter().enumerate() {
    match handle.await.unwrap() {
      Ok(Response::Put) => {}
      Err(KvError::NotRunning) => {}
      other => panic!("第 {index} 条请求未按预期完成: {other:?}"),
    }
  }
}

/// panic payload 转可读消息（场景线程的断言原因带回主线程）。
fn panic_message(payload: Box<dyn Any + Send>) -> String {
  if let Some(msg) = payload.downcast_ref::<&str>() {
    (*msg).to_owned()
  } else if let Some(msg) = payload.downcast_ref::<String>() {
    msg.clone()
  } else {
    "未知 panic".to_owned()
  }
}

/// 提交进行中 shutdown：全部请求 resolve、shutdown 返回 Ok、场景限时完成。
#[compio::test]
async fn requests_racing_shutdown_all_resolve() {
  let root = TempRoot::new("shutdown_race");
  let (done_tx, done_rx) = oneshot::channel();
  let scene = thread::Builder::new()
    .name("fkv-shutdown-race".to_owned())
    .spawn(move || {
      let outcome = catch_unwind(AssertUnwindSafe(|| {
        let runtime = Runtime::new().unwrap();
        runtime.block_on(race_shutdown(root.path().to_path_buf()));
      }));
      done_tx.send(outcome.map_err(panic_message)).ok();
    })
    .unwrap();
  match timeout(SCENE_DEADLINE, done_rx).await {
    Ok(Ok(Ok(()))) => {}
    Ok(Ok(Err(msg))) => panic!("shutdown 竞态场景失败: {msg}"),
    // 场景线程在回传前异常退出：按失败上报，不静默通过。
    Ok(Err(_)) => panic!("场景线程提前退出"),
    Err(_) => panic!(
      "shutdown 与在飞回执互等超时（{SCENE_DEADLINE:?}）：Shard::shutdown 阻塞 \
       调用方 runtime，或线程退出后通道缓冲内的请求未被排空（test-05）"
    ),
  }
  // done 先于线程退出送达：此处 join 只等线程收尾，不会长时间阻塞。
  scene.join().unwrap();
}
