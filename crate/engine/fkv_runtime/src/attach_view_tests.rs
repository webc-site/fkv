use std::{
  sync::{Arc, mpsc},
  thread,
  time::{Duration, Instant},
};

use fkv_api::Request;
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;
use futures::task::noop_waker;

use crate::{Runtime, RuntimeConfig, attach::AttachEpoch};

fn config(root: &TempRoot) -> RuntimeConfig {
  RuntimeConfig {
    num_threads: 1,
    root: root.path().to_path_buf(),
    page_size: 512,
    restart_interval: 16,
    pages_per_file_shift: 18,
    segment_size: 4096,
    segments_per_file_shift: 2,
    large_value_threshold: 1024,
    fixed_buffer_size: 0,
    auto_reopen_retry_times: 0,
    auto_reopen_pending_time_us: 0,
    archive_interval_secs: 0,
    archive_num_retained: 100,
    queue_size: 64,
  }
}

/// 直接向 shard 发旧 term 写：把 lifecycle 置 `Expired`，但**绕过**本层收尾，
/// 模拟「错误迟到 / 视图尚未失效」的中间态。
async fn expire_shard(runtime: &Runtime, tbl: &TableIdent, term: u64) {
  assert_eq!(
    runtime.shard_li[0]
      .request(Request::Put {
        tbl: tbl.clone(),
        batch_li: vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
        term: Some(term),
      })
      .await
      .unwrap_err(),
    KvError::ExpiredTerm
  );
}

/// 自旋等待 `pred` 成立：只等「另一个线程走到已知状态」，不是用 sleep 掩盖竞态。
///
/// 超时说明真实进度缺失（例如 attach future 根本没被 poll），带场景名报错。
fn wait_until(mut pred: impl FnMut() -> bool, what: &str) {
  let deadline = Instant::now() + Duration::from_secs(30);
  while !pred() {
    assert!(Instant::now() < deadline, "等待超时：{what}");
    thread::yield_now();
  }
}

/// 让 attach 的视图落地确定性地排在新代际之后。
///
/// 独立线程先占住 per-shard 视图门并通知 `held`；调用方随后启动 attach，
/// 再发 `release`。持门线程**在仍持门时**推进代际才释放，于是 attach 的
/// `record_if_current` 只能看到过期代际——与 shard 线程何时回执无关。
///
/// 返回 `(已持门信号, 放行信号, 句柄)`；放行一次后线程退出，句柄必须 join。
fn hold_view_gate(
  epoch: Arc<AttachEpoch>,
  shard_id: u32,
) -> (mpsc::Receiver<()>, mpsc::Sender<()>, thread::JoinHandle<()>) {
  let (held_tx, held_rx) = mpsc::channel();
  let (release_tx, release_rx) = mpsc::channel();
  let gate = thread::spawn(move || {
    epoch.with_gate(shard_id, || {
      held_tx.send(()).unwrap();
      release_rx.recv().unwrap();
      epoch.bump(shard_id);
    });
  });
  (held_rx, release_tx, gate)
}

/// 收尾按 shard 的**真实** lifecycle 清视图，而不是只看错误码。
/// 旧实现只在 `ExpiredTerm` 时清：shard 被 detach 后普通请求回 `NotRunning`，
/// 视图（身份 / 归档目标 / 预热路由）会一直残留。回退成「仅 `ExpiredTerm` 清」
/// 时第一条断言失败。
#[test]
fn non_expired_error_clears_view_by_real_state() {
  pollster::block_on(async {
    let root = TempRoot::new("fkv_attach_view");
    let runtime = Runtime::start_mem(config(&root)).unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime.attach_shard(0, "main", 5).await.unwrap();
    // 直接 detach shard，绕过本层收尾：视图仍是 (main, 5)，shard 已经 Detached。
    runtime.shard_li[0]
      .request(Request::DetachShard { shard_id: 0 })
      .await
      .unwrap();
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap_err(),
      KvError::NotRunning
    );
    assert_eq!(
      runtime.global_archive_name("x"),
      "manifest_main_0_x",
      "shard 已不托管，视图必须清掉"
    );

    // 反向：shard 仍在托管时，本地超时不得清视图（避免过度清理）。
    runtime.attach_shard(0, "main", 6).await.unwrap();
    assert_eq!(
      runtime
        .request_deadline(
          &tbl,
          Request::Get {
            tbl: tbl.clone(),
            key: b"a".to_vec()
          },
          Duration::ZERO
        )
        .await
        .unwrap_err(),
      KvError::Timeout
    );
    assert_eq!(
      runtime.global_archive_name("x"),
      "manifest_main_6_x",
      "shard 仍 Attached，超时不应清视图"
    );
    runtime.shutdown().await.unwrap();
  });
}

/// 全局 fanout 也必须走统一收尾：`GlobalFanout::dispatch*` 直发 `sender_of`
/// 时，shard 已 `Expired` 的错误不会清本层视图，结果名继续用失效身份。
#[test]
fn global_fanout_clears_view_when_shard_expired() {
  let root = TempRoot::new("fkv_fanout_view");
  pollster::block_on(async {
    let runtime = Runtime::start(config(&root)).unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime.attach_shard(0, "main", 5).await.unwrap();
    // 落盘分区：归档 fanout 才有目标可派发。
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"seed".to_vec(), b"S".to_vec(), 1)],
      )
      .await
      .unwrap();
    // 直接对 shard 发旧 term 写：lifecycle 置 Expired，本层视图保持 (main, 5)。
    expire_shard(&runtime, &tbl, 4).await;
    let reply = runtime.global_archive("late", 1).await.unwrap();
    assert_eq!(reply.ok_num, 0, "{reply:?}");
    assert_eq!(
      reply.name, "manifest_main_0_late",
      "fanout 错误必须清掉已失效的 attach 视图"
    );
    runtime.shutdown().await.unwrap();
  });
}

/// 迟到的 attach 错误不得清掉更晚成功的 attach 视图：收尾前取的代际快照过期
/// 即弃权。回退 epoch 校验（只看 lifecycle）时新视图会被清掉。
#[test]
fn stale_attach_error_keeps_newer_view() {
  pollster::block_on(async {
    let root = TempRoot::new("fkv_attach_view");
    let runtime = Runtime::start_mem(config(&root)).unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime.attach_shard(0, "main", 5).await.unwrap();
    // 旧 attach 发送前的代际快照（模拟一个还在途中的请求）。
    let stale = runtime.route_of(0).unwrap();
    let stale_epoch = stale.epoch_snapshot();
    expire_shard(&runtime, &tbl, 4).await;
    // 更晚的 attach 成功：视图换成 (main, 7)，代际推进。
    runtime.attach_shard(0, "main", 7).await.unwrap();
    // shard 再次被旧 term 请求打过标，但本层视图是更晚 attach 记的。
    expire_shard(&runtime, &tbl, 3).await;
    // 迟到错误：lifecycle 已是 `Expired`，但代际变了，不能清掉 (main, 7)。
    stale
      .finish(stale_epoch, Err(KvError::ExpiredTerm))
      .unwrap_err();
    assert_eq!(
      runtime.global_archive_name("x"),
      "manifest_main_7_x",
      "更晚成功的 attach 视图被迟到错误清掉了"
    );
    // 反证：同代际的错误收尾确实会清视图（否则上面的断言没有区分度）。
    let current = runtime.route_of(0).unwrap();
    let epoch = current.epoch_snapshot();
    current
      .finish(epoch, Err(KvError::ExpiredTerm))
      .unwrap_err();
    assert_eq!(runtime.global_archive_name("x"), "manifest_main_0_x");
    runtime.shutdown().await.unwrap();
  });
}

/// 迟到的 detach 清除不得抹掉更晚 attach 的视图。
///
/// 回退 `clear_if_current`（detach 成功无条件 `clear_view`）时视图被清成
/// `(main, 0)`。
#[test]
fn stale_detach_clear_keeps_newer_view() {
  pollster::block_on(async {
    let root = TempRoot::new("fkv_attach_view");
    let runtime = Runtime::start_mem(config(&root)).unwrap();
    runtime.attach_shard(0, "main", 5).await.unwrap();
    let route = runtime.route_of(0).unwrap();
    // 模拟已发出、回执未到的 detach：发送前推进代际并取快照。
    let stale = route.bump_epoch();
    // 更晚的 detach + attach 成功落地 (main, 9)。
    runtime.detach_shard(0).await.unwrap();
    runtime.attach_shard(0, "main", 9).await.unwrap();
    assert_eq!(runtime.global_archive_name("x"), "manifest_main_9_x");

    assert!(
      !route.clear_if_current(stale),
      "过期代际的 detach 不得清视图"
    );
    assert_eq!(
      runtime.global_archive_name("x"),
      "manifest_main_9_x",
      "迟到 detach 清掉了更晚 attach 的视图"
    );
    runtime.shutdown().await.unwrap();
  });
}

/// 迟到的 attach 记录不得覆盖更晚 detach 的清除。
///
/// 回退 `record_if_current`（attach 成功无条件记录）时视图被重新写成
/// `(main, 9)`。
#[test]
fn stale_attach_record_skipped_after_newer_detach() {
  pollster::block_on(async {
    let root = TempRoot::new("fkv_attach_view");
    let runtime = Runtime::start_mem(config(&root)).unwrap();
    runtime.attach_shard(0, "main", 5).await.unwrap();
    let route = runtime.route_of(0).unwrap();
    // 模拟在途 attach：发送前推进代际。
    let stale = route.bump_epoch();
    // 更晚的 detach 成功：视图清空。
    runtime.detach_shard(0).await.unwrap();
    assert_eq!(runtime.global_archive_name("x"), "manifest_main_0_x");

    let recorded = route.record_if_current(stale, || runtime.record_attach(0, "main", 9));
    assert!(!recorded, "过期代际的 attach 记录不得落地");
    assert_eq!(
      runtime.global_archive_name("x"),
      "manifest_main_0_x",
      "迟到 attach 记录覆盖了更晚 detach 的清除"
    );
    runtime.shutdown().await.unwrap();
  });
}

/// 代际 CAS：过期快照不能抢占，只有当前代际能推进。
///
/// 覆盖 [`AttachEpoch::bump_if_current`] 的 `AcqRel` compare-exchange 语义。
#[test]
fn epoch_cas_rejects_stale_generation() {
  let epoch = AttachEpoch::new(1);
  let first = epoch.bump(0);
  assert_eq!(first, 1);
  assert!(epoch.bump_if_current(0, first));
  assert!(!epoch.bump_if_current(0, first), "过期代际不能被抢占");
  assert!(epoch.bump_if_current(0, first + 1));
  assert_eq!(epoch.snapshot(0), first + 2);
}

/// 在途 detach 回执迟到（代际已被更晚操作推进）：`detach_shard` 不得清视图。
///
/// 覆盖调用点而非助手函数：回退成 `route.clear_view()`（无条件清）时视图被
/// 清成 `(main, 0)`。
#[test]
fn in_flight_detach_does_not_clear_newer_view() {
  use std::{future::Future, task::Context};

  pollster::block_on(async {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    // mem shard 可能在第一次 poll 前就完成 detach；重试直到真的拿到在途状态。
    for _ in 0..32 {
      let root = TempRoot::new("fkv_attach_view");
      let runtime = Runtime::start_mem(config(&root)).unwrap();
      runtime.attach_shard(0, "main", 5).await.unwrap();
      let mut fut = Box::pin(runtime.detach_shard(0));
      if fut.as_mut().poll(&mut cx).is_pending() {
        // 更晚的控制面操作已推进代际（真实场景里是重 attach 开始）。
        runtime.route_of(0).unwrap().bump_epoch();
        assert!(fut.await.is_ok());
        assert_eq!(
          runtime.global_archive_name("x"),
          "manifest_main_5_x",
          "迟到 detach 回执清掉了更晚操作拥有的视图"
        );
        runtime.shutdown().await.unwrap();
        return;
      }
      drop(fut);
      runtime.shutdown().await.unwrap();
    }
    panic!("detach 在 32 次尝试内都同步完成，无法构造在途回执");
  });
}

/// 在途 attach 回执迟到（代际被更晚操作推进）：`attach_shard` 不落地视图，
/// 且因 shard 仍托管必须回 `Busy` 让调用方重试。
///
/// 仅防御：生产路径由 per-shard `control_li` 串行后不可达（代际推进与视图
/// 落地在同一临界区），这里手工 `bump_epoch` 模拟「在控制锁之外推进代际」的
/// 控制操作，覆盖 superseded 分支。回退成无条件 `record_attach` 时结果名变成
/// `manifest_main_9_x`。
///
/// 在途状态由 [`hold_view_gate`] 构造：attach 取完代际快照后，持门线程**仍在
/// 门内**推进代际才放行，所以 `record_if_current` 必定看到过期代际。不再假设
/// 首 poll 必为 `Pending`（shard 线程负载高时可能已完成）。
#[test]
fn in_flight_attach_does_not_override_newer_view() {
  pollster::block_on(async {
    let root = TempRoot::new("fkv_attach_view");
    let runtime = Arc::new(Runtime::start_mem(config(&root)).unwrap());
    let base = runtime.attach_epoch.snapshot(0);
    let (held_rx, release_tx, gate) = hold_view_gate(Arc::clone(&runtime.attach_epoch), 0);
    held_rx.recv().unwrap();
    let worker_runtime = Arc::clone(&runtime);
    let worker =
      thread::spawn(move || pollster::block_on(worker_runtime.attach_shard(0, "main", 9)));
    // attach 的首 poll 已先推进代际，此后才算「更晚的控制面操作」。
    wait_until(
      || runtime.attach_epoch.snapshot(0) > base,
      "attach 未取代际快照",
    );
    release_tx.send(()).unwrap();
    let result = worker.join().unwrap();
    gate.join().unwrap();
    assert!(
      matches!(result, Err(KvError::Busy)),
      "被更晚操作接管的 attach 必须返 Busy"
    );
    assert_eq!(
      runtime.global_archive_name("x"),
      "manifest_main_0_x",
      "迟到的 attach 视图覆盖了更晚操作的清除"
    );
    let runtime = Arc::try_unwrap(runtime).ok().expect("runtime still shared");
    runtime.shutdown().await.unwrap();
  });
}

/// superseded 且 shard 已 `Detached`：必须回 `Busy`（可重试），不得 `Ok(())`。
///
/// 仅防御（同上条，生产路径不可达）：手工推进代际模拟「在控制锁之外推进代际」
/// 的控制操作。回退「`Detached` 时返 `Ok(())`」时这里拿到 `Ok`，调用方以为
/// attach 成功，但 shard 终态是 `Detached`，后续请求全 `NotRunning`。
///
/// 在途状态由 [`hold_view_gate`] 构造：先等 shard 真正执行过 attach（回执已
/// 产生），再绕过本层收尾直接 detach，最后在门内推进代际。
#[test]
fn superseded_attach_on_detached_shard_reports_busy() {
  pollster::block_on(async {
    let root = TempRoot::new("fkv_attach_view");
    let runtime = Arc::new(Runtime::start_mem(config(&root)).unwrap());
    let tbl = TableIdent::new("demo", 1).unwrap();
    let (held_rx, release_tx, gate) = hold_view_gate(Arc::clone(&runtime.attach_epoch), 0);
    held_rx.recv().unwrap();
    let worker_runtime = Arc::clone(&runtime);
    let worker =
      thread::spawn(move || pollster::block_on(worker_runtime.attach_shard(0, "main", 9)));
    // shard 已执行过 attach：detach 必定排在它之后。
    wait_until(
      || runtime.shard_li[0].lifecycle().is_attached(),
      "shard 未执行 attach",
    );
    // 绕过本层收尾直接 detach：attach 的回执仍是 `AttachShard`，
    // 但 shard 的终态已是 `Detached`。
    runtime.shard_li[0]
      .request(Request::DetachShard { shard_id: 0 })
      .await
      .unwrap();
    // 更晚的控制面操作推进代际：本次 attach 视图不得落地。
    release_tx.send(()).unwrap();
    let result = worker.join().unwrap();
    gate.join().unwrap();
    assert!(
      matches!(result, Err(KvError::Busy)),
      "shard 已 Detached 的 superseded attach 必须返 Busy，不得谎报成功"
    );
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap_err(),
      KvError::NotRunning,
      "attach 未生效，后续请求必须显式 NotRunning"
    );

    // `Busy` 可重试：调用方重放 attach 即恢复。
    runtime.attach_shard(0, "main", 9).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), None);
    let runtime = Arc::try_unwrap(runtime).ok().expect("runtime still shared");
    runtime.shutdown().await.unwrap();
  });
}

/// 迟到旧代际的超时登记不得把更晚的登记改小。
///
/// 回退 `mark_timeout` 为 `store`（后到者覆盖）时 settle 取到旧代际，
/// `clear_if_current` 弃权，视图永久残留。
#[test]
fn late_timeout_registration_keeps_newer_epoch() {
  let epoch = AttachEpoch::new(1);
  epoch.mark_timeout(0, 9);
  epoch.mark_timeout(0, 3);
  assert_eq!(epoch.settle_timeout(0, false), Some(9));
  assert_eq!(epoch.settle_timeout(0, false), None);
}

/// 数据面 / 控制面 / 归档 crond 的 `ShardRoute` 共享同一代际与视图：
/// 三者都由 [`ShardRoute::new`] 构造。
#[test]
fn route_constructors_share_epoch_and_views() {
  pollster::block_on(async {
    let root = TempRoot::new("fkv_attach_view");
    let runtime = Runtime::start_mem(config(&root)).unwrap();
    let data = runtime.route_of(0).unwrap();
    let control = runtime.control_route(0).unwrap();
    assert_eq!(data.shard_id, control.shard_id);
    assert!(Arc::ptr_eq(&data.epoch, &control.epoch));
    assert!(Arc::ptr_eq(&data.epoch, &runtime.attach_epoch));

    // 共享代际：control 推进后 data 的旧快照立即过期。
    let stale = data.epoch_snapshot();
    control.bump_epoch();
    assert!(!data.epoch.is_current(0, stale));
    runtime.shutdown().await.unwrap();
  });
}

/// 调用方在 `.await` 中途 drop 请求 future：收尾仍要按 shard 的真实 lifecycle
/// 清视图。
///
/// 回退 `Drop` 守卫（只在 `.await` 正常返回后 `finish`）时视图残留
/// `(main, 5)`。
#[test]
fn dropped_request_still_clears_view_by_lifecycle() {
  use std::{future::Future, task::Context};

  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    for _ in 0..32 {
      let root = TempRoot::new("fkv_attach_view");
      let runtime = Runtime::start_mem(config(&root)).unwrap();
      runtime.attach_shard(0, "main", 5).await.unwrap();
      let route = runtime.route_of(0).unwrap();
      let mut fut = Box::pin(route.request(Request::Get {
        tbl: tbl.clone(),
        key: b"a".to_vec(),
      }));
      if fut.as_mut().poll(&mut cx).is_pending() {
        // 绕过本层收尾直接 detach shard：lifecycle 变 `Detached`，视图仍是 (main, 5)。
        runtime.shard_li[0]
          .request(Request::DetachShard { shard_id: 0 })
          .await
          .unwrap();
        // 取消在途请求：守卫必须收尾。
        drop(fut);
        assert_eq!(
          runtime.global_archive_name("x"),
          "manifest_main_0_x",
          "被取消的请求也必须清视图"
        );
        runtime.shutdown().await.unwrap();
        return;
      }
      drop(fut);
      runtime.shutdown().await.unwrap();
    }
    panic!("请求在 32 次尝试内都同步完成，无法构造在途回执");
  });
}

/// 本地 `Timeout` 后 shard 才置 `Expired`：下一次观测点必须清掉残留视图。
///
/// 回退「超时登记 + 观测复核」时视图一直停在 `(main, 5)`（只在遇到下一次
/// 错误时才清）。这也是 `fkv_shard` 不提供 Expired 写点回调时的已接受近似。
#[test]
fn timeout_then_late_expire_clears_view_on_next_observation() {
  pollster::block_on(async {
    let root = TempRoot::new("fkv_attach_view");
    let runtime = Runtime::start_mem(config(&root)).unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime.attach_shard(0, "main", 5).await.unwrap();

    // 本地超时：请求已入队，shard 仍是 `Attached`，此刻不能清视图。
    assert_eq!(
      runtime
        .request_deadline(
          &tbl,
          Request::Get {
            tbl: tbl.clone(),
            key: b"a".to_vec()
          },
          Duration::ZERO
        )
        .await
        .unwrap_err(),
      KvError::Timeout
    );
    assert_eq!(
      runtime.global_archive_name("x"),
      "manifest_main_5_x",
      "shard 仍托管，超时不该清视图"
    );

    // shard 稍后才因请求真正执行而置 `Expired`（绕过本层收尾）。
    expire_shard(&runtime, &tbl, 4).await;
    assert_eq!(
      runtime.global_archive_name("x"),
      "manifest_main_0_x",
      "超时登记未在下一次观测点复核"
    );
    runtime.shutdown().await.unwrap();
  });
}
