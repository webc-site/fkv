//! io-33 行为锁定：standby 追赶任务化后，慢分区同步期间**已打开分区**的读
//! 不被全 shard 停摆拖住。
//!
//! 场景对齐 issue 探针：master 两张表 `slowtbl.0` / `fasttbl.0`；假 `rsync`
//! （PATH 注入）对参数含 `slowtbl` 的调用 `sleep 1`，其余透传真 rsync。
//! replica shard 先懒开 `fasttbl`，再从旁路触发 `slowtbl` 首访问；断言同步
//! 期间对 `fasttbl`（已打开）的 `Get` 在 200ms 内回执——内联实现（第 40 轮
//! 基线）实测 6950ms：同步全程把事件循环挂起，所有分区的回执都被压到同步
//! 结束。C++ 语义（`RsyncPartition` + `WaitIo`：只挂起发起任务，shard 继续
//! 服务）下该读只付出微秒级。
//!
//! 同用例顺带锁定：同步期间对同一慢表的第二条读合并等待（单飞 waiter），
//! 同步完成后两条读都正确回执。

use std::{
  fs,
  path::Path,
  process::Command,
  sync::Arc,
  time::{Duration, Instant},
};

use compio::{
  runtime::spawn,
  time::{sleep as async_sleep, timeout},
};
use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_error::{KvError, Result};
use fkv_shard::{Shard, ShardConfig};
use fkv_standby::{RsyncSource, RsyncStandbyService, StandbyConfig, StandbyRole, StdCommandRunner};
use fkv_testkit::TempRoot;
use fkv_types::{StoreMode, TableIdent};

/// 已打开分区读的回执上界：任务化后是 fast path（微秒级），内联实现是被
/// 同步全程挡住（秒级），200ms 有充分判别力。
const OPEN_TABLE_READ_LIMIT: Duration = Duration::from_millis(200);

/// 触发慢表同步后、开始计时前留给事件循环出队并派发追赶的缓冲。
const SYNC_START_DELAY: Duration = Duration::from_millis(300);

fn attach(term: u64) -> Request {
  Request::AttachShard {
    branch: "main".to_owned(),
    shard_id: 0,
    term,
  }
}

/// `Response` 未实现 `PartialEq`：用模式匹配断言 Get 结果。
fn expect_get(response: Result<Response>, want: &[u8]) {
  match response {
    Ok(Response::Get(Some(got))) => assert_eq!(got, want),
    other => panic!("unexpected response: {other:?}"),
  }
}

fn put(tbl: &TableIdent, key: &str, val: &str) -> Request {
  Request::Put {
    tbl: tbl.clone(),
    batch_li: vec![BatchEntry::upsert(
      key.as_bytes().to_vec(),
      val.as_bytes().to_vec(),
      1,
    )],
    term: Some(1),
  }
}

fn get(tbl: &TableIdent, key: &str) -> Request {
  Request::Get {
    tbl: tbl.clone(),
    key: key.as_bytes().to_vec(),
  }
}

/// 环境没有 rsync 就跳过（与 C++ 的 standby 测试同一前提）。
fn rsync_available() -> bool {
  Command::new("rsync")
    .arg("--version")
    .output()
    .is_ok_and(|output| output.status.success())
}

/// PATH 注入工具（unix）：假 rsync 放慢 `slowtbl` 的子进程调用。
#[cfg(unix)]
mod fake_rsync {
  use std::{
    env,
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
  };

  use fkv_testkit::TempRoot;

  /// 在 PATH 中解析真实 rsync 的绝对路径（须在注入假脚本**之前**调用）。
  fn real_rsync() -> PathBuf {
    env::var_os("PATH")
      .and_then(|path| {
        env::split_paths(&path).find_map(|dir| {
          let candidate = dir.join("rsync");
          candidate.is_file().then_some(candidate)
        })
      })
      .expect("rsync on PATH（rsync_available 已前置检查）")
  }

  /// 安装假 rsync：参数含 `slowtbl` 时 sleep 1s（放慢该表每一次子进程调用，
  /// 一次同步 ≈ 多个子进程 × 1s），随后透传真 rsync。返回 bin 目录。
  pub(super) fn install(tag: &str) -> TempRoot {
    let bin = TempRoot::new(tag);
    let script = format!(
      "#!/bin/sh\n\
       for arg in \"$@\"; do\n\
         case \"$arg\" in\n\
           *slowtbl*) sleep 1; break ;;\n\
         esac\n\
       done\n\
       exec \"{}\" \"$@\"\n",
      real_rsync().display()
    );
    let path = bin.path().join("rsync");
    fs::write(&path, script).unwrap();
    let mut perm = fs::metadata(&path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&path, perm).unwrap();
    bin
  }

  /// 安装「闸门」假 rsync 到 `bin_dir`：参数含 `marker` 时记录一次调用到
  /// `log`，并等 `gate` 文件消失再透传真 rsync；其余调用直接透传。用于把一次
  /// 追赶精确挡在 rsync 边界，让测试控制放行时机（io-34 首写 deadline 与追赶
  /// 完成的定序）。
  pub(super) fn install_gated(bin_dir: &Path, marker: &str, gate: &Path, log: &Path) {
    let script = format!(
      "#!/bin/sh\n\
       for arg in \"$@\"; do\n\
         case \"$arg\" in\n\
           *{marker}*)\n\
             echo \"invoked\" >> \"{}\"\n\
             while [ -f \"{}\" ]; do sleep 0.05; done\n\
             break ;;\n\
         esac\n\
       done\n\
       exec \"{}\" \"$@\"\n",
      log.display(),
      gate.display(),
      real_rsync().display()
    );
    let path = bin_dir.join("rsync");
    fs::write(&path, script).unwrap();
    let mut perm = fs::metadata(&path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&path, perm).unwrap();
  }

  /// 把目录放到 PATH 最前，Drop 时恢复（同 `fkv_runtime` standby 测试的做法；
  /// nextest 每用例独立进程）。
  pub(super) struct PathGuard {
    old: Option<OsString>,
  }

  impl PathGuard {
    pub(super) fn prepend(dir: &Path) -> Self {
      let old = env::var_os("PATH");
      let mut path_li = vec![dir.to_path_buf()];
      if let Some(old) = &old {
        path_li.extend(env::split_paths(old));
      }
      let joined = env::join_paths(path_li).unwrap();
      // SAFETY: 仅测试进程内改环境变量。
      unsafe { env::set_var("PATH", joined) };
      Self { old }
    }
  }

  impl Drop for PathGuard {
    fn drop(&mut self) {
      // SAFETY: 同上。
      unsafe {
        match &self.old {
          Some(old) => env::set_var("PATH", old),
          None => env::remove_var("PATH"),
        }
      }
    }
  }
}

/// replica 配置：Local 盘面 + StandbyReplica 模式 + 指向 master 根的追赶服务
/// （`master_addr = "local"` → 本机 rsync）。
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
  cfg.mode = StoreMode::StandbyReplica;
  cfg.standby = Some(Arc::new(
    RsyncStandbyService::new(standby_config, source).unwrap(),
  ));
  cfg
}

/// master 用普通 Local shard 写两张表后关闭（replica 只依赖盘面，不需要
/// master 进程存活）。
async fn write_master_two_tables(master_root: &Path, fast: &TableIdent, slow: &TableIdent) {
  let master = Shard::start(ShardConfig::for_tests(master_root.to_path_buf())).unwrap();
  master.request(attach(1)).await.unwrap();
  master.request(put(fast, "a", "A")).await.unwrap();
  master.request(put(slow, "s", "S")).await.unwrap();
  master.shutdown().await.unwrap();
}

/// io-33 核心：慢表追赶期间已打开分区的读 < 200ms 回执；同步期间对同一慢表
/// 的并发读合并等待，完成后全部正确回执。
#[compio::test]
#[cfg(unix)]
async fn slow_partition_sync_does_not_stall_open_table_reads() {
  if !rsync_available() {
    eprintln!("skip: rsync not available");
    return;
  }
  let master_root = TempRoot::new("shard_standby_master");
  let replica_root = TempRoot::new("shard_standby_replica");
  let fast = TableIdent::new("fasttbl", 0).unwrap();
  let slow = TableIdent::new("slowtbl", 0).unwrap();
  write_master_two_tables(master_root.path(), &fast, &slow).await;

  let fake_bin = fake_rsync::install("shard_standby_fake_rsync");
  let _path_guard = fake_rsync::PathGuard::prepend(fake_bin.path());

  let replica = Shard::start(replica_config(replica_root.path(), master_root.path())).unwrap();
  replica.request(attach(1)).await.unwrap();

  // 先懒开 fasttbl（其自身追赶也走任务化路径，普通本机 rsync 一次 ~百毫秒）。
  let first = timeout(Duration::from_secs(30), replica.request(get(&fast, "a")))
    .await
    .expect("fasttbl 首访问不应超时");
  expect_get(first, b"A");

  // 旁路触发 slowtbl 首访问（两条：第二条验证同步期间的读合并为 waiter）。
  let sender_a = replica.sender();
  let sender_b = replica.sender();
  let slow_a = slow.clone();
  let slow_b = slow.clone();
  let handle_a = spawn(async move {
    timeout(Duration::from_secs(60), sender_a.request(get(&slow_a, "s")))
      .await
      .expect("slowtbl 首读不应超时")
  });
  let handle_b = spawn(async move {
    timeout(Duration::from_secs(60), sender_b.request(get(&slow_b, "s")))
      .await
      .expect("slowtbl 并发读不应超时")
  });
  // 留给事件循环出队 slowtbl 首访问并派发追赶（首个 rsync sleep 1s）。
  async_sleep(SYNC_START_DELAY).await;

  // 同步期间对已打开 fasttbl 的读必须立即回执：内联实现这里被同步全程挡住
  // （探针实测 6950ms），任务化后是已打开分区的 fast path。
  let started = Instant::now();
  let during = timeout(Duration::from_secs(30), replica.request(get(&fast, "a")))
    .await
    .expect("同步期间已打开分区的读不得被追赶拖住");
  let elapsed = started.elapsed();
  expect_get(during, b"A");
  assert!(
    elapsed < OPEN_TABLE_READ_LIMIT,
    "慢表同步期间已打开分区的读耗时 {elapsed:?}（上界 {OPEN_TABLE_READ_LIMIT:?}）"
  );

  // 慢表两条读都在同步完成后正确回执。
  for handle in [handle_a, handle_b] {
    let got = handle.await.expect("slowtbl 读任务未 panic");
    expect_get(got, b"S");
  }

  replica.shutdown().await.unwrap();
}

/// io-33 写路径锁定：standby 未打开表的首写同样挂起等追赶，完成后重放
/// 整组 Put 并成功落盘（`prepare_put_group` / `run_gated_spawn` 的准入）。
#[compio::test]
async fn first_write_on_standby_waits_for_sync_then_lands() {
  if !rsync_available() {
    eprintln!("skip: rsync not available");
    return;
  }
  let master_root = TempRoot::new("shard_standby_write_master");
  let replica_root = TempRoot::new("shard_standby_write_replica");
  let tbl = TableIdent::new("put_lazy", 0).unwrap();
  // master 写一条基线数据：replica 首写前必须先追到它。
  write_master_two_tables(master_root.path(), &tbl, &tbl).await;

  let replica = Shard::start(replica_config(replica_root.path(), master_root.path())).unwrap();
  replica.request(attach(1)).await.unwrap();

  let put_reply = timeout(
    Duration::from_secs(30),
    replica.request(put(&tbl, "fresh", "V")),
  )
  .await
  .expect("standby 首写不应超时（追赶挂起后必须重放完成）");
  assert!(put_reply.is_ok(), "{put_reply:?}");
  // 追赶带回的 master 数据与本地新写并存。
  expect_get(replica.request(get(&tbl, "a")).await, b"A");
  expect_get(replica.request(get(&tbl, "fresh")).await, b"V");

  replica.shutdown().await.unwrap();
}

/// sem-79 复现辅助：轮询直到条件成立（有上限，绝不让用例挂死）。
async fn wait_until(label: &str, within: Duration, mut cond: impl FnMut() -> bool) -> bool {
  let deadline = Instant::now() + within;
  while !cond() {
    if Instant::now() >= deadline {
      eprintln!("等待超时: {label}");
      return false;
    }
    async_sleep(Duration::from_millis(5)).await;
  }
  true
}

/// sem-79 行为锁定：standby 写路径「挂起→追赶完成→重放」必须保持分区写 FIFO。
///
/// 场景（对应 issue 触发链）：
/// 1. `A = Put(T, k=1)` 首到，T 未打开 → 需追赶（io-33）；假 rsync 闸门把追赶挡
///    在 rsync 边界，A 在此挂起；
/// 2. 同分区后到写 `B = Put(T, k=2)` 到达；
/// 3. 放行追赶，等 A / B 回执。
///
/// 断言：
/// - **结构性（确定性判红点）**：B 到达后 `write_queue_depth >= 2`——A 挂起期间
///   **保留**它在 T 写队列的槽位（先到写仍在队首）。旧行为（挂起即 `gate.finish`
///   释放占用、改挂进追赶 waiter）下 A 不在写队列里，B 入队后被取出并同样挂起，
///   深度回到 0，本断言必然失败（不等它的队列深度）。
/// - **语义（契约本体）**：最终 `Get(T, k) == 2`。两个 Put 固定同一 `timestamp`
///   （`put` 辅助 ts=1），merge 的 `change_ts >= base_ts` 门不掩蔽先后顺序：谁后
///   落盘谁生效；A 先重放执行（k=1）、B 后执行（k=2）。旧行为的 FIFO 反转
///   （B 先执行、A 回填 k=1）是「占用释放」与「追赶完成」的竞态，见 issue 说明。
#[compio::test]
#[cfg(unix)]
async fn parked_first_write_keeps_partition_fifo() {
  if !rsync_available() {
    eprintln!("skip: rsync not available");
    return;
  }
  const MARKER: &str = "fifotbl";
  let master_root = TempRoot::new("shard_standby_fifo_master");
  let replica_root = TempRoot::new("shard_standby_fifo_replica");
  let tbl = TableIdent::new(MARKER, 0).unwrap();

  // master 保持存活：T 有可追的基线数据。
  let master = Shard::start(ShardConfig::for_tests(master_root.path().to_path_buf())).unwrap();
  master.request(attach(1)).await.unwrap();
  master.request(put(&tbl, "a", "A")).await.unwrap();

  let fake_bin = TempRoot::new("shard_standby_fifo_bin");
  let gate = fake_bin.path().join("gate");
  let log = fake_bin.path().join("invoked.log");
  fake_rsync::install_gated(fake_bin.path(), MARKER, &gate, &log);
  let _path_guard = fake_rsync::PathGuard::prepend(fake_bin.path());
  // 立闸门：T 的第一轮追赶会停在 rsync 边界等放行。
  fs::write(&gate, b"").unwrap();

  let replica = Shard::start(replica_config(replica_root.path(), master_root.path())).unwrap();
  replica.request(attach(1)).await.unwrap();

  // 步骤 1：A 首写挂起于 T 的追赶。
  let sender_a = replica.sender();
  let a_tbl = tbl.clone();
  let a_task = spawn(async move { sender_a.request(put(&a_tbl, "k", "1")).await });
  let log_path = log.clone();
  assert!(
    wait_until("A 挂起追赶", Duration::from_secs(30), move || {
      fs::read_to_string(&log_path).is_ok_and(|text| text.contains("invoked"))
    })
    .await,
    "A 必须实际挂起追赶（假 rsync 未被调用）"
  );

  // 步骤 2：同分区后到写 B。
  let sender_b = replica.sender();
  let b_tbl = tbl.clone();
  let b_task = spawn(async move { sender_b.request(put(&b_tbl, "k", "2")).await });

  // 结构性断言（sem-79 核心）：A 挂起期间保留其在 T 写队列的槽位，深度含 A + B。
  assert!(
    wait_until("A 保留写队列槽位", Duration::from_secs(5), || {
      replica.metrics_snapshot().write_queue_depth >= 2
    })
    .await,
    "先到的挂起写必须保留其分区写队列槽位（旧行为挂起即释放占用，深度回到 0）"
  );

  // 步骤 3：放行追赶，等两条写回执。
  fs::remove_file(&gate).unwrap();
  for (label, task) in [("A", a_task), ("B", b_task)] {
    let reply = timeout(Duration::from_secs(60), task)
      .await
      .unwrap_or_else(|_| panic!("{label} 回执超时"))
      .unwrap_or_else(|_| panic!("{label} 任务 panic"));
    assert!(reply.is_ok(), "{label} 写失败: {reply:?}");
  }

  // 语义：A 重放先执行（k=1）→ 后到的 B 生效（k=2）。
  expect_get(replica.request(get(&tbl, "k")).await, b"2");

  replica.shutdown().await.unwrap();
  master.shutdown().await.unwrap();
}

/// PutLarge 首写（走 `run_gated_spawn` 的可任务化写路径）。
fn put_large(tbl: &TableIdent, key: &[u8], value: &[u8]) -> Request {
  Request::PutLarge {
    tbl: tbl.clone(),
    batch_li: vec![BatchEntry::upsert(key.to_vec(), value.to_vec(), 1)],
    term: Some(1),
  }
}

/// io-34 行为锁定：standby 首写挂起等追赶期间过期（`request_at` 的服务端
/// deadline，无本端兜底），追赶完成重放这条已过期的写时不残留追赶结果；此后
/// master 更新，更晚的同表首访问必须重新追赶、读到 master 更新后的值。
///
/// 定序（关键是不用猜时间）：
/// 1. 假 rsync 闸门挡住第一轮追赶的 rsync，master 此刻只有旧数据 `a`；
/// 2. 首写出队 → 挂起为追赶 waiter → deadline 过后放行闸门；
/// 3. 追赶完成（同步的是旧数据）→ 重放已过期的首写 → 回 `Timeout`——这个
///    回执就是「第一轮追赶已完成且挂起请求已重放」的确定性信号；
/// 4. 此时才更新 master（新增 `b`）；
/// 5. 更晚的同表首读：修复前消费残留追赶结果 → reopen 旧 manifest → 读不到
///    `b`；修复后残留被丢弃（`discard_synced`）→ 重新追赶 → 读到 `b`。
async fn expired_write_replay_sees_fresh_master(make_write: fn(&TableIdent) -> Request, tag: &str) {
  if !rsync_available() {
    eprintln!("skip: rsync not available");
    return;
  }
  // 表名带标记：假 rsync 只按这张表的名字挡 / 记账。
  const MARKER: &str = "staletbl";
  let master_root = TempRoot::new(&format!("{tag}_master"));
  let replica_root = TempRoot::new(&format!("{tag}_replica"));
  let tbl = TableIdent::new(MARKER, 0).unwrap();

  // master 保持存活：追赶被挡期间只有旧数据，之后才更新。
  let master = Shard::start(ShardConfig::for_tests(master_root.path().to_path_buf())).unwrap();
  master.request(attach(1)).await.unwrap();
  master.request(put(&tbl, "a", "A")).await.unwrap();

  let fake_bin = TempRoot::new(&format!("{tag}_bin"));
  let gate = fake_bin.path().join("gate");
  let log = fake_bin.path().join("invoked.log");
  fake_rsync::install_gated(fake_bin.path(), MARKER, &gate, &log);
  let _path_guard = fake_rsync::PathGuard::prepend(fake_bin.path());
  // 立闸门：第一轮追赶的 rsync 会停在这里，等我们放行。
  fs::write(&gate, b"").unwrap();

  let replica = Shard::start(replica_config(replica_root.path(), master_root.path())).unwrap();
  replica.request(attach(1)).await.unwrap();

  // 首写只设服务端 deadline（`request_at` 不做本端兜底）：出队时未过期 →
  // 挂起等追赶；追赶被闸门挡住，deadline 在等待期间过期。
  let first_write = make_write(&tbl);
  let sender = replica.sender();
  let task = spawn(async move {
    sender
      .request_at(first_write, Instant::now() + Duration::from_millis(200))
      .await
  });
  // 等首写完成第一次出队并挂起追赶、且 deadline（200ms）已过。
  async_sleep(Duration::from_millis(800)).await;
  // 放行第一轮追赶：它同步到的 master 仍只有 "a"。
  fs::remove_file(&gate).unwrap();
  let reply = timeout(Duration::from_secs(30), task)
    .await
    .expect("挂起重放的首写不应超时")
    .expect("首写任务不应 panic");
  assert_eq!(
    reply.unwrap_err(),
    KvError::Timeout,
    "挂起重放的首写应在 deadline 过后回 Timeout"
  );
  // 守卫：假 rsync 确实被调用过 = 追赶确实派发过 = 首写确实挂起过。
  // 否则说明首写第一次出队时已过期、从未挂起，本用例会退化成假绿。
  assert!(
    fs::read_to_string(&log).is_ok_and(|text| text.contains("invoked")),
    "首写必须实际挂起追赶（假 rsync 未被调用）"
  );

  // 第一轮追赶已完成且过期的首写已重放：此时才更新 master，新 key 只能被
  // 更晚首访问触发的后一轮追赶看到。
  master.request(put(&tbl, "b", "B")).await.unwrap();

  let got = timeout(Duration::from_secs(60), replica.request(get(&tbl, "b")))
    .await
    .expect("更晚的同表首读不应超时");
  expect_get(got, b"B");

  replica.shutdown().await.unwrap();
  master.shutdown().await.unwrap();
}

/// io-34 Put 组路径（`prepare_put_group` 的 canceled / expired 分支）。
#[compio::test]
async fn expired_first_put_replay_does_not_pin_stale_sync_result() {
  expired_write_replay_sees_fresh_master(|tbl| put(tbl, "x", "X"), "shard_standby_stale_put").await;
}

/// io-34 可任务化写路径（`run_gated_spawn` 的 early canceled / expired 分支）。
#[compio::test]
async fn expired_first_put_large_replay_does_not_pin_stale_sync_result() {
  expired_write_replay_sees_fresh_master(
    |tbl| put_large(tbl, b"x", &[7u8; 2048]),
    "shard_standby_stale_put_large",
  )
  .await;
}
