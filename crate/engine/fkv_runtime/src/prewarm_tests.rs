use std::{
  collections::VecDeque,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
  },
  thread,
  time::{Duration, Instant},
};

use bytes::Bytes;
use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_error::{KvError, Result};
use fkv_io_mem::MemStore;
use fkv_object::{MemObjectStore, ObjectStore};
use fkv_store::Store;
use fkv_types::{MAIN_BRANCH_NAME, TableIdent};

use crate::prewarm::{
  MAX_PENDING_PREWARM_TABLES, PREWARM_STOP_TIMEOUT, PrewarmCrond, PrewarmState, lock_prewarm_state,
  plan_targets,
};

fn cloud(local: MemStore, remote: MemObjectStore) -> CloudStore<MemStore, MemObjectStore> {
  CloudStore::new(
    local,
    remote,
    "fkv",
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 1)
}

/// 轮询共享状态直到条件成立；超时 panic（避免测试永久挂住）。
fn wait_until(state: &Arc<Mutex<PrewarmState>>, cond: impl Fn(&PrewarmState) -> bool) {
  for _ in 0..500 {
    if cond(&lock_prewarm_state(state)) {
      return;
    }
    thread::sleep(Duration::from_millis(10));
  }
  panic!("prewarm condition not reached in time");
}

/// 预热线程卡在整轮 tick（列举 + 全表下载）时，`stop()` 必须有界返回。
///
/// 用注入的线程体桩（不响应停止、不返回）模拟「一轮预热在飞」：真实 S3 慢速
/// 路径无法在单测里注入，本用例锁定的是 `stop()` 的等待上界；在飞 tick 不被
/// 中断，但停机不再等整轮结束（超时告警并分离线程）。
#[test]
fn prewarm_crond_stop_bounded_while_tick_in_flight() {
  let state = Arc::new(Mutex::new(PrewarmState::new(1)));
  let mut crond = PrewarmCrond::start_inner(
    cloud(MemStore::new(512), MemObjectStore::new()),
    state,
    1,
    |_cloud, _state, _table_concurrency, _signal_rx| {
      thread::sleep(Duration::from_secs(60));
    },
  )
  .unwrap();
  let started = Instant::now();
  crond.stop();
  let elapsed = started.elapsed();
  assert!(
    elapsed >= PREWARM_STOP_TIMEOUT,
    "stop 应等到上界才放弃: {elapsed:?}"
  );
  assert!(
    elapsed < PREWARM_STOP_TIMEOUT + Duration::from_secs(1),
    "stop 必须有界，不能等线程退出: {elapsed:?}"
  );
}

/// reopen 投递的 per-table 目标：不受 `done_li` 限制、重复去重、按 shard 合并进
/// attach 轮；未 attach 的 shard 目标直接丢弃（等 attach 后的整轮覆盖）。
#[test]
fn pending_table_targets_ignore_done_and_dedupe() {
  let mut state = PrewarmState::new(1);
  state.attach(0, MAIN_BRANCH_NAME, 3);
  state.done_li[0] = true;
  let tbl = TableIdent::new("late", 1).unwrap();
  state.enqueue_table(tbl.clone());
  state.enqueue_table(tbl.clone());
  assert_eq!(state.pending_tbl_li.len(), 1, "同表应去重");

  let target_li = plan_targets(state.take_plan_input(), Vec::new());
  assert_eq!(target_li.len(), 1, "{target_li:?}");
  assert_eq!(target_li[0].shard_id, 0);
  assert_eq!(target_li[0].term, 3);
  assert_eq!(target_li[0].table_li, vec![tbl]);
  assert!(state.pending_tbl_li.is_empty(), "pending 应被消费");

  // 未 attach：目标丢弃，不留在队列里等下一轮（attach 轮会覆盖全部分区）。
  state.detach(0);
  state.enqueue_table(TableIdent::new("lost", 1).unwrap());
  assert!(plan_targets(state.take_plan_input(), Vec::new()).is_empty());
  assert!(state.pending_tbl_li.is_empty());
}

/// pending 上限：超限丢最早，去重索引与队列同步；plan 与 attach 轮合并时
/// 同表只出现一次（去重从线性 `contains` 换成桶内 `sort` + `dedup` 后行为不变）。
#[test]
fn pending_table_targets_evict_oldest_and_keep_index() {
  let mut state = PrewarmState::new(1);
  state.attach(0, MAIN_BRANCH_NAME, 1);
  let first = TableIdent::new("evict", 0).unwrap();
  state.enqueue_table(first.clone());
  for part in 0..=MAX_PENDING_PREWARM_TABLES as u32 {
    state.enqueue_table(TableIdent::new("evict", part).unwrap());
  }
  assert_eq!(state.pending_tbl_li.len(), MAX_PENDING_PREWARM_TABLES);
  assert_eq!(state.pending_set.len(), state.pending_tbl_li.len());
  assert!(!state.pending_set.contains(&first), "超限必须丢最早的目标");

  // plan 的 attach 列表包含全部 pending 表：合并后不得重复。
  let table_li: Vec<TableIdent> = state.pending_tbl_li.iter().cloned().collect();
  let target_li = plan_targets(state.take_plan_input(), table_li);
  let total: usize = target_li.iter().map(|target| target.table_li.len()).sum();
  assert_eq!(total, MAX_PENDING_PREWARM_TABLES);
  assert!(state.pending_set.is_empty());
  assert!(state.pending_tbl_li.is_empty());
}

/// detach 清掉该 shard 的 pending：重 attach 换了身份后，旧目标不会再按新
/// `(branch, term)` 预热；其他 shard 的 pending 不受影响。
#[test]
fn detach_drops_pending_of_shard() {
  let mut state = PrewarmState::new(2);
  state.attach(0, MAIN_BRANCH_NAME, 1);
  state.attach(1, MAIN_BRANCH_NAME, 1);
  let tbl0 = TableIdent::new("drop", 2).unwrap();
  let tbl1 = TableIdent::new("keep", 1).unwrap();
  assert_eq!(tbl0.shard_index(2), 0);
  assert_eq!(tbl1.shard_index(2), 1);
  state.enqueue_table(tbl0);
  state.enqueue_table(tbl1.clone());
  state.detach(0);
  assert_eq!(state.pending_tbl_li, VecDeque::from([tbl1.clone()]));
  assert_eq!(state.pending_set.len(), 1, "去重索引必须与队列同步");
  assert!(state.pending_set.contains(&tbl1));

  state.attach(0, "next", 2);
  let target_li = plan_targets(state.take_plan_input(), Vec::new());
  assert_eq!(target_li.len(), 1, "{target_li:?}");
  assert_eq!(target_li[0].shard_id, 1);
}

/// 可切换失败的远端：`fail` 置位时所有 `get` / `head` 返回 `IoFail`，
/// 列举不受影响，用来制造单 shard 预热失败。
#[derive(Debug, Clone)]
struct FlakyObjectStore {
  inner: MemObjectStore,
  fail: Arc<AtomicBool>,
}

impl ObjectStore for FlakyObjectStore {
  async fn get(&self, key: &str) -> Result<Option<Bytes>> {
    if self.fail.load(Ordering::Relaxed) {
      return Err(KvError::IoFail);
    }
    self.inner.get(key).await
  }

  async fn head(&self, key: &str) -> Result<Option<fkv_object::ObjectMeta>> {
    if self.fail.load(Ordering::Relaxed) {
      return Err(KvError::IoFail);
    }
    self.inner.head(key).await
  }

  async fn put(
    &self,
    key: &str,
    value: Bytes,
    if_match: Option<&str>,
    if_none_match: bool,
  ) -> Result<String> {
    self.inner.put(key, value, if_match, if_none_match).await
  }

  async fn delete(&self, key: &str) -> Result<()> {
    self.inner.delete(key).await
  }

  async fn list(&self, prefix: &str) -> Result<Vec<fkv_object::ObjectMeta>> {
    self.inner.list(prefix).await
  }

  async fn list_page(
    &self,
    prefix: &str,
    token: Option<&str>,
    max_keys: usize,
  ) -> Result<fkv_object::ListPage> {
    self.inner.list_page(prefix, token, max_keys).await
  }
}

/// 只让 data 文件下载失败的远端：manifest 列举与下载正常，`prewarm_table` 以
/// `CompletionReason::DownloadError` 收尾但仍返回 `Ok`。
#[derive(Debug)]
struct DataFailStore {
  inner: MemObjectStore,
  fail: Arc<AtomicBool>,
}

fn is_data_key(key: &str) -> bool {
  let name = key.rsplit('/').next().unwrap_or(key);
  name
    .strip_prefix(fkv_types::FILE_NAME_DATA)
    .is_some_and(|rest| rest.starts_with('_'))
}

impl ObjectStore for DataFailStore {
  async fn get(&self, key: &str) -> Result<Option<Bytes>> {
    if self.fail.load(Ordering::Relaxed) && is_data_key(key) {
      return Err(KvError::IoFail);
    }
    self.inner.get(key).await
  }

  async fn head(&self, key: &str) -> Result<Option<fkv_object::ObjectMeta>> {
    self.inner.head(key).await
  }

  async fn put(
    &self,
    key: &str,
    value: Bytes,
    if_match: Option<&str>,
    if_none_match: bool,
  ) -> Result<String> {
    self.inner.put(key, value, if_match, if_none_match).await
  }

  async fn delete(&self, key: &str) -> Result<()> {
    self.inner.delete(key).await
  }

  async fn list(&self, prefix: &str) -> Result<Vec<fkv_object::ObjectMeta>> {
    self.inner.list(prefix).await
  }

  async fn list_page(
    &self,
    prefix: &str,
    token: Option<&str>,
    max_keys: usize,
  ) -> Result<fkv_object::ListPage> {
    self.inner.list_page(prefix, token, max_keys).await
  }
}

/// 单表下载失败（`Ok` + `DownloadError`）不得让整个 shard 置 `done_li`：失败表
/// 必须回队，否则永久不再重试。回退「只要 `Ok` 就置 `done_li`」时本用例失败。
#[test]
fn partial_download_failure_requeues_table() {
  pollster::block_on(async {
    let remote = MemObjectStore::new();
    let tbl = TableIdent::new("partial", 1).unwrap();
    {
      let mut store = Store::new(cloud(MemStore::new(512), remote.clone()), 512, 16);
      store
        .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
        .await
        .unwrap();
    }
    let fail = Arc::new(AtomicBool::new(true));
    let state = Arc::new(Mutex::new(PrewarmState::new(1)));
    let crond = PrewarmCrond::start(
      CloudStore::new(
        MemStore::new(512),
        DataFailStore {
          inner: remote.clone(),
          fail: fail.clone(),
        },
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1),
      state.clone(),
      1,
    )
    .unwrap();
    lock_prewarm_state(&state).attach(0, MAIN_BRANCH_NAME, 0);
    crond.signal();
    wait_until(&state, |state| {
      state.last_report.is_some_and(|report| report.table_num > 0)
    });
    {
      let state0 = lock_prewarm_state(&state);
      let report = state0.last_report.unwrap();
      assert!(
        report.listing_failed,
        "部分失败必须体现在回执里: {report:?}"
      );
      assert!(report.files_failed > 0, "{report:?}");
      assert!(!state0.done_li[0], "部分失败不能标记已预热");
      assert!(
        state0.pending_tbl_li.contains(&tbl),
        "失败表必须回队: {:?}",
        state0.pending_tbl_li
      );
    }

    // 远端恢复：下一轮重试成功，`done_li` 置位、pending 清空。
    fail.store(false, Ordering::Relaxed);
    lock_prewarm_state(&state).last_report = None;
    crond.signal();
    wait_until(&state, |state| {
      state
        .last_report
        .is_some_and(|report| report.table_num > 0 && !report.listing_failed)
    });
    let state0 = lock_prewarm_state(&state);
    assert!(state0.done_li[0], "全部成功后必须置 done_li");
    assert!(
      state0.pending_tbl_li.is_empty(),
      "{:?}",
      state0.pending_tbl_li
    );
  });
}

/// 回队后的自唤醒：失败表在没有外部 `signal()` 的情况下也必须被重试。
///
/// 回退「`tick` 返回是否需重试 + `run` 带退避 `recv_timeout`」时，crond 只在
/// 下一次 attach / reopen 才复活，本用例等不到 `done_li`。
#[test]
fn failed_table_retries_without_external_signal() {
  pollster::block_on(async {
    let remote = MemObjectStore::new();
    let tbl = TableIdent::new("auto_retry", 1).unwrap();
    {
      let mut store = Store::new(cloud(MemStore::new(512), remote.clone()), 512, 16);
      store
        .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
        .await
        .unwrap();
    }
    let fail = Arc::new(AtomicBool::new(true));
    let state = Arc::new(Mutex::new(PrewarmState::new(1)));
    let crond = PrewarmCrond::start(
      CloudStore::new(
        MemStore::new(512),
        DataFailStore {
          inner: remote.clone(),
          fail: fail.clone(),
        },
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1),
      state.clone(),
      1,
    )
    .unwrap();
    lock_prewarm_state(&state).attach(0, MAIN_BRANCH_NAME, 0);
    // 唯一一次外部唤醒：之后全靠 crond 自唤醒。
    crond.signal();
    wait_until(&state, |state| {
      state
        .last_report
        .is_some_and(|report| report.files_failed > 0)
    });

    // 远端恢复后不再 signal：必须由回队触发的重试把表拉完。
    fail.store(false, Ordering::Relaxed);
    wait_until(&state, |state| {
      state.done_li[0]
        && state
          .last_report
          .is_some_and(|report| report.table_num > 0 && !report.listing_failed)
    });
    assert!(
      lock_prewarm_state(&state).pending_tbl_li.is_empty(),
      "重试成功后队列必须清空"
    );
  });
}

/// 单 shard 预热失败：回执必须可见（不能被本轮列举成功覆盖），且失败分区
/// 回队；远端恢复后下一轮信号即重试成功。
#[test]
fn shard_failure_stays_visible_and_retries() {
  pollster::block_on(async {
    let remote = MemObjectStore::new();
    let tbl = TableIdent::new("retry", 1).unwrap();
    {
      let mut store = Store::new(cloud(MemStore::new(512), remote.clone()), 512, 16);
      store
        .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
        .await
        .unwrap();
    }
    let fail = Arc::new(AtomicBool::new(true));
    let state = Arc::new(Mutex::new(PrewarmState::new(1)));
    let crond = PrewarmCrond::start(
      CloudStore::new(
        MemStore::new(512),
        FlakyObjectStore {
          inner: remote.clone(),
          fail: fail.clone(),
        },
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      )
      .with_segment_config(4096, 1),
      state.clone(),
      1,
    )
    .unwrap();
    lock_prewarm_state(&state).attach(0, MAIN_BRANCH_NAME, 0);
    crond.signal();
    wait_until(&state, |state| state.last_report.is_some());
    {
      let state0 = lock_prewarm_state(&state);
      let report = state0.last_report.unwrap();
      assert!(
        report.listing_failed,
        "单 shard 失败必须体现在回执里: {report:?}"
      );
      assert_eq!(report.table_num, 0, "{report:?}");
      assert!(!state0.done_li[0], "失败的 shard 不能标记已预热");
      assert_eq!(state0.pending_tbl_li.len(), 1, "失败分区必须回队重试");
    }

    // 远端恢复：同一轮信号重试成功，pending 清空、done_li 置位。
    fail.store(false, Ordering::Relaxed);
    lock_prewarm_state(&state).last_report = None;
    crond.signal();
    wait_until(&state, |state| state.done_li[0]);
    let report = lock_prewarm_state(&state).last_report.unwrap();
    assert!(!report.listing_failed, "{report:?}");
    assert!(report.pulled_file_num >= 2, "{report:?}");
    assert!(lock_prewarm_state(&state).pending_tbl_li.is_empty());
  });
}

/// attach 轮已完成（`done_li`）后，reopen 投递的新分区仍会被 crond 预热。
#[test]
fn prewarm_crond_consumes_reopen_table_after_attach_done() {
  pollster::block_on(async {
    let remote = MemObjectStore::new();
    let first_tbl = TableIdent::new("first", 1).unwrap();
    {
      let mut store = Store::new(cloud(MemStore::new(512), remote.clone()), 512, 16);
      store
        .put(
          &first_tbl,
          &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
        )
        .await
        .unwrap();
    }

    let state = Arc::new(Mutex::new(PrewarmState::new(1)));
    let crond =
      PrewarmCrond::start(cloud(MemStore::new(512), remote.clone()), state.clone(), 1).unwrap();
    lock_prewarm_state(&state).attach(0, MAIN_BRANCH_NAME, 0);
    crond.signal();
    wait_until(&state, |state| state.done_li[0]);

    // attach 之后远端才出现新分区（另一个 writer 推送）。
    let late_tbl = TableIdent::new("late", 1).unwrap();
    {
      let mut store = Store::new(cloud(MemStore::new(512), remote.clone()), 512, 16);
      store
        .put(
          &late_tbl,
          &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
        )
        .await
        .unwrap();
    }
    lock_prewarm_state(&state).last_report = None;
    lock_prewarm_state(&state).enqueue_table(late_tbl);
    crond.signal();
    // attach 轮已 done，本轮只应包含 reopen 投递的这张表。
    wait_until(&state, |state| {
      state
        .last_report
        .is_some_and(|report| report.table_num == 1 && report.pulled_file_num >= 2)
    });
    let report = lock_prewarm_state(&state).last_report.unwrap();
    assert!(!report.listing_failed, "{report:?}");
    assert!(
      lock_prewarm_state(&state).done_li[0],
      "attach 轮完成标记不能被覆盖"
    );
  });
}

/// 纯路由：按 `shard_index` 分组，未 attach 的 shard 本轮跳过，已预热的 shard 不重跑。
#[test]
fn plan_routes_by_shard_index_and_respects_attach() {
  // 2 shard：partition 1 → shard1，partition 2 → shard0。
  let mut state = PrewarmState::new(2);
  let tbl_shard1 = TableIdent::new("t", 1).unwrap();
  let tbl_shard0 = TableIdent::new("t", 2).unwrap();
  assert_eq!(tbl_shard1.shard_index(2), 1);
  assert_eq!(tbl_shard0.shard_index(2), 0);

  // 都没 attach：不产生目标。
  assert!(
    plan_targets(
      state.take_plan_input(),
      vec![tbl_shard1.clone(), tbl_shard0.clone()]
    )
    .is_empty()
  );

  // 只 attach shard0：只有 tbl_shard0 进目标，且带 attach 时的 (branch, term)。
  state.attach(0, MAIN_BRANCH_NAME, 7);
  let target_li = plan_targets(
    state.take_plan_input(),
    vec![tbl_shard1.clone(), tbl_shard0.clone()],
  );
  assert_eq!(target_li.len(), 1, "{target_li:?}");
  assert_eq!(target_li[0].shard_id, 0);
  assert_eq!(target_li[0].branch, MAIN_BRANCH_NAME);
  assert_eq!(target_li[0].term, 7);
  assert_eq!(target_li[0].table_li, vec![tbl_shard0.clone()]);

  // shard0 已预热：不再重复；此时 attach shard1 才轮到 tbl_shard1。
  state.done_li[0] = true;
  assert!(plan_targets(state.take_plan_input(), vec![tbl_shard0]).is_empty());
  state.attach(1, MAIN_BRANCH_NAME, 9);
  let target_li = plan_targets(state.take_plan_input(), vec![tbl_shard1.clone()]);
  assert_eq!(target_li.len(), 1);
  assert_eq!(target_li[0].shard_id, 1);
  assert_eq!(target_li[0].term, 9);

  // detach 后重新纳入下一轮（done_li 一并复位）。
  state.detach(1);
  state.done_li[1] = true;
  assert!(plan_targets(state.take_plan_input(), vec![tbl_shard1.clone()]).is_empty());
  state.attach(1, MAIN_BRANCH_NAME, 11);
  assert_eq!(
    plan_targets(state.take_plan_input(), vec![tbl_shard1]).len(),
    1
  );
}

/// crond 端到端：远端两个分区分属两个 shard，只 attach 的 shard 被预热；
/// 第二个 shard attach 后补预，已预过的 shard 不重跑。
#[test]
fn prewarm_crond_fans_out_by_attached_shard() {
  pollster::block_on(async {
    let remote = MemObjectStore::new();
    let tbl_shard1 = TableIdent::new("a", 1).unwrap();
    let tbl_shard0 = TableIdent::new("b", 2).unwrap();
    {
      let mut store = Store::new(cloud(MemStore::new(512), remote.clone()), 512, 16);
      for tbl in [&tbl_shard1, &tbl_shard0] {
        store
          .put(tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
          .await
          .unwrap();
      }
    }

    let state = Arc::new(Mutex::new(PrewarmState::new(2)));
    let crond =
      PrewarmCrond::start(cloud(MemStore::new(512), remote.clone()), state.clone(), 2).unwrap();

    // 只 attach shard1（tbl_shard1 的归属）：只预一张表。
    lock_prewarm_state(&state).attach(1, MAIN_BRANCH_NAME, 0);
    crond.signal();
    wait_until(&state, |state| state.done_li[1]);
    let first = lock_prewarm_state(&state).last_report.unwrap();
    assert_eq!(first.table_num, 1, "{first:?}");
    assert!(first.pulled_file_num >= 2, "{first:?}");
    assert!(!first.listing_failed, "{first:?}");

    // shard0 attach 后补预 tbl_shard0；shard1 已 done_li 不再重跑。
    lock_prewarm_state(&state).last_report = None;
    lock_prewarm_state(&state).attach(0, MAIN_BRANCH_NAME, 0);
    crond.signal();
    wait_until(&state, |state| state.done_li[0]);
    let second = lock_prewarm_state(&state).last_report.unwrap();
    assert_eq!(second.table_num, 1, "{second:?}");
    assert!(second.pulled_file_num >= 2, "{second:?}");
  });
}

/// 远端对象名带写时的 branch / term：prewarm 按列举到的 (branch, term) 打开，
/// 不因目标 shard attach 的 term 不同而漏掉旧/新 term 对象（对齐 C++ PrewarmFile）。
#[test]
fn prewarm_uses_listed_term_across_attach_identity() {
  pollster::block_on(async {
    let remote = MemObjectStore::new();
    let tbl = TableIdent::new("t", 1).unwrap();
    {
      // 以 term=7 写入远端。
      let mut store = Store::new(cloud(MemStore::new(512), remote.clone()), 512, 16);
      store.prepare_attach(MAIN_BRANCH_NAME, 7).unwrap();
      store
        .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
        .await
        .unwrap();
    }

    // attach 身份 term=7：能列举并拉取。
    let state = Arc::new(Mutex::new(PrewarmState::new(1)));
    lock_prewarm_state(&state).attach(0, MAIN_BRANCH_NAME, 7);
    let crond =
      PrewarmCrond::start(cloud(MemStore::new(512), remote.clone()), state.clone(), 1).unwrap();
    crond.signal();
    wait_until(&state, |state| state.last_report.is_some());
    let hit = lock_prewarm_state(&state).last_report.unwrap();
    assert!(hit.pulled_file_num >= 2, "{hit:?}");

    // attach 身份 term=0：仍按列举到的 term=7 对象拉取，不能因 identity 不同而 0 拉取。
    let state0 = Arc::new(Mutex::new(PrewarmState::new(1)));
    lock_prewarm_state(&state0).attach(0, MAIN_BRANCH_NAME, 0);
    let crond0 = PrewarmCrond::start(cloud(MemStore::new(512), remote), state0.clone(), 1).unwrap();
    crond0.signal();
    wait_until(&state0, |state| state.last_report.is_some());
    let miss = lock_prewarm_state(&state0).last_report.unwrap();
    assert!(miss.pulled_file_num >= 2, "{miss:?}");
    assert_eq!(miss.missing_num, 0, "{miss:?}");
  });
}
