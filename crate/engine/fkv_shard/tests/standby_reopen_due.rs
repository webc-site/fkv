//! io-35 行为锁定：standby 追赶任务化（io-33）后，**追赶完成**必须提前同表
//! auto-reopen driver 的到期时间，而不是让 driver 白等一整轮 `retry_delay()`
//! 才去消费已就绪的结果。
//!
//! 场景：standby replica、表在 master 上**不存在**（新建中 / 已删），一条
//! 走 auto-reopen 的挂起读。链路（`reopen.rs::run_due_reopen` 的 standby 分支）：
//!
//! 1. 首访问派发追赶（`admit_standby_sync` → `Parked`），读挂进 waiter；
//! 2. 追赶完成（master 无该分区 → `ResourceMissing`），读被重放缓开；
//! 3. 重放的读经 `reopen_li.park` 建 driver，`due = now + pending_time`；
//! 4. driver 到期 → 表未就绪 → 再派发一次追赶 → **顺延 `retry_delay()`**；
//! 5. 追赶毫秒级完成 → `finish_sync_done` 登记结果。
//!
//! 修复前：`finish_sync_done` 只重放 envelope waiter、不触碰 `reopen_li`，driver
//! 仍按第 4 步的 `now + retry_delay()` 躺到下一轮才消费——而 `retry_delay()` =
//! `max(pending_time, 1ms)`，本用例 `pending_time = 1s` 时总耗时 ≈ 2 × pending
//! （驱动延迟 + 一整轮空等）。C++ `CompleteJob → FinishIo` 与内联版都是完成
//! 即唤醒，故修复后总耗时 ≈ pending_time + 追赶时长（本场景追赶不 exec rsync：
//! 本机 master 目录缺失时 `RsyncSource::list` 直接回 `ResourceMissing`）。
//!
//! 判别力：修复后 ≈ 1.0s，修复前 ≈ 2.0s，上界取 1.5 × pending_time。变异验证
//! （去掉 `bump_due`）实测 ~2.0s，如期越过上界变红。

use std::{
  path::Path,
  sync::Arc,
  time::{Duration, Instant},
};

use compio::{
  runtime::spawn,
  time::{sleep as async_sleep, timeout},
};
use fkv_api::Request;
use fkv_error::KvError;
use fkv_shard::{Shard, ShardConfig};
use fkv_standby::{RsyncSource, RsyncStandbyService, StandbyConfig, StandbyRole, StdCommandRunner};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

/// 单轮 auto-reopen 延迟（driver 建条目时的 `now + pending_time`，也是顺延下限）。
const PENDING_TIME: Duration = Duration::from_millis(1000);

fn replica_config(replica_root: &Path, master_root: &Path) -> ShardConfig {
  let standby_config = StandbyConfig {
    role: StandbyRole::Replica,
    local_root: replica_root.to_path_buf(),
    master_addr: "local".to_owned(),
    master_path_li: vec![master_root.to_string_lossy().into_owned()],
    max_concurrency: 1,
    target_term: 0,
  };
  let source = RsyncSource::new_rooted(
    StdCommandRunner,
    vec![master_root.to_string_lossy().into_owned()],
    vec![0],
    "",
  )
  .unwrap();
  let mut cfg = ShardConfig::for_tests(replica_root.to_path_buf());
  cfg.mode = fkv_types::StoreMode::StandbyReplica;
  // auto-reopen 预算 ≥ 1：重放的读要能挂进 driver。
  cfg.auto_reopen_retry_times = 4;
  cfg.auto_reopen_pending_time_us =
    u64::try_from(PENDING_TIME.as_micros()).expect("pending_time fits u64");
  cfg.standby = Some(Arc::new(
    RsyncStandbyService::new(standby_config, source).unwrap(),
  ));
  cfg
}

/// 追赶完成后 driver 必须立即重试消费：读回执时刻 < `pending_time` 的 1.5 倍
/// （修复前 ≈ 2 × pending_time，因为白等一整轮顺延）。
#[compio::test]
async fn sync_done_advances_reopen_driver_due() {
  let master_root = TempRoot::new("standby_due_master");
  let replica_root = TempRoot::new("standby_due_replica");
  // master 根存在但表目录不存在：追赶走 `RsyncSource::list` 的
  // `ResourceMissing` 早退路径（毫秒级，不 exec rsync），把用例的时间花在
  // driver 的等待 / 唤醒语义上。
  let tbl = TableIdent::new("absent", 1).unwrap();

  let replica = Shard::start(replica_config(replica_root.path(), master_root.path())).unwrap();
  replica
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();

  let started = Instant::now();
  let err = timeout(
    Duration::from_secs(30),
    replica.request(Request::Get {
      tbl,
      key: b"k".to_vec(),
    }),
  )
  .await
  .expect("挂起读不应超时")
  .unwrap_err();
  let elapsed = started.elapsed();
  assert_eq!(err, KvError::ResourceMissing);

  // 下界：读确实走了 auto-reopen driver（首轮追赶 → park → 等一轮 pending_time）。
  // 失败即说明用例没有覆盖到目标路径（例如配置错误导致快速失败），不能当作通过。
  assert!(
    elapsed >= Duration::from_millis(900),
    "读没有等待 auto-reopen driver：{elapsed:?}"
  );
  // 上界（io-35）：driver 在追赶完成后立即消费，而不是再等一整轮
  // `retry_delay() = pending_time`。修复前 ≈ 2 × pending_time。
  let limit = PENDING_TIME + PENDING_TIME / 2;
  assert!(
    elapsed < limit,
    "追赶完成后 driver 未提前到期，读白等了一轮顺延：{elapsed:?}（上界 {limit:?}）"
  );

  replica.shutdown().await.unwrap();
}

/// 提前到期不得把并发 waiter 变成硬错误：driver 仍在等待时到达的第二条读会
/// 自己派发一次追赶，其完成会 bump driver 的到期时间；driver 于是先消费结果
/// （连坐第一条读），第二条读的重放发现结果已被取走，必须重新派发追赶再挂
/// ——绝不能拿 `ensure_table_ready` 的防御性 `Busy` 回执调用方。
#[compio::test]
async fn concurrent_read_during_driver_wait_still_parks_not_busy() {
  let master_root = TempRoot::new("standby_due_race_master");
  let replica_root = TempRoot::new("standby_due_race_replica");
  let tbl = TableIdent::new("absent", 1).unwrap();

  let replica = Shard::start(replica_config(replica_root.path(), master_root.path())).unwrap();
  replica
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();

  let first = {
    let sender = replica.sender();
    let tbl = tbl.clone();
    spawn(async move {
      timeout(
        Duration::from_secs(30),
        sender.request(Request::Get {
          tbl,
          key: b"k".to_vec(),
        }),
      )
      .await
      .expect("第一条读不应超时")
    })
  };
  // 等第一条读走完首轮追赶并挂进 driver（driver 尚未到期）。
  async_sleep(Duration::from_millis(200)).await;
  let second = timeout(
    Duration::from_secs(30),
    replica.request(Request::Get {
      tbl,
      key: b"k".to_vec(),
    }),
  )
  .await
  .expect("第二条读不应超时")
  .unwrap_err();

  // 两条都必须拿到 `ResourceMissing`（表在 master 上不存在），而不是 `Busy`。
  assert_eq!(second, KvError::ResourceMissing);
  assert_eq!(
    first.await.expect("第一条读任务未 panic").unwrap_err(),
    KvError::ResourceMissing
  );

  replica.shutdown().await.unwrap();
}
