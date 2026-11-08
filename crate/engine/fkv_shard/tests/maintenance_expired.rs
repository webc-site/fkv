//! sem-22：请求成功后的隐式写后维护里，`ExpiredTerm` 也必须把整个 shard 置
//! `Expired`（对齐 C++ `Shard::OnTaskFinished` 对任一任务 `ExpiredTerm` 调
//! `MarkShardExpired`），且不改变已成功的写回执。
//!
//! 构造：先写满一整批互不相同的 key（全部 live，放大率约 1，不触发维护），
//! 再由更高 term 的 Store 在同一盘面写出一份 term 7 的归档 manifest；随后覆盖
//! 前一半 key，写任务提交成功时 `needs_maintain` 必为真，任务内的
//! `compact` + `file_gc` 会扫描到更高 term 并返回 `ExpiredTerm`。
//!
//! 对照：`SyncData@2` 注入让同一个写后维护的 compact 同步失败（非 term 错误，
//! 回执与生命周期都不变），锁定「只有 `ExpiredTerm` 置过期」。
use std::{
  ops::Range,
  path::{Path, PathBuf},
  time::{Duration, Instant},
};

use compio::time::sleep;
use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_failpoint::{fired, points, reset, set_spec};
use fkv_io_compio::CompioStore;
use fkv_shard::{Shard, ShardConfig};
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
/// 32 页 / 文件：几十次写就能跨多个文件，放大率门槛低。
const PAGES_PER_FILE_SHIFT: u8 = 5;
/// 覆盖前一半 key 后，非尾文件里既有死页也有活页，compaction 必有活页可搬。
const FULL_KEYS: usize = 200;
const OVERWRITE_KEYS: usize = 100;
const VALUE_LEN: usize = 300;

fn config(root: PathBuf) -> ShardConfig {
  let mut cfg = ShardConfig::for_tests(root);
  cfg.page_size = PAGE_SIZE;
  cfg.pages_per_file_shift = PAGES_PER_FILE_SHIFT;
  cfg.queue_size = 64;
  cfg.file_amplify_factor = 1;
  cfg
}

fn attach(branch: &str, shard_id: u32, term: u64) -> Request {
  Request::AttachShard {
    branch: branch.to_owned(),
    shard_id,
    term,
  }
}

fn key(index: usize) -> Vec<u8> {
  format!("key-{index:04}").into_bytes()
}

fn entries(range: Range<usize>, ts: u64) -> Vec<BatchEntry> {
  range
    .map(|index| BatchEntry::upsert(key(index), vec![b'v'; VALUE_LEN], ts))
    .collect()
}

async fn put_batch(
  shard: &Shard,
  tbl: &TableIdent,
  batch_li: Vec<BatchEntry>,
  term: u64,
) -> fkv_error::Result<Response> {
  shard
    .request(Request::Put {
      tbl: tbl.clone(),
      batch_li,
      term: Some(term),
    })
    .await
}

async fn get(shard: &Shard, tbl: &TableIdent, key: Vec<u8>) -> fkv_error::Result<Response> {
  shard
    .request(Request::Get {
      tbl: tbl.clone(),
      key,
    })
    .await
}

/// 回执先于 `WriteDone` 归还 Store 发出：等写门清零，确保事件循环已收尾上一条
/// 任务（Store 已放回槽位、`WriteDone` 结果已观察完）。
async fn wait_write_running_zero(shard: &Shard) {
  for _ in 0..10_000 {
    if shard.metrics_snapshot().write_running == 0 {
      return;
    }
    sleep(Duration::from_micros(200)).await;
  }
  panic!("写门在跑数始终未归零");
}

/// 写成功 + 写后维护看到更高 term：回执成功后事件循环收尾，生命周期翻转。
async fn wait_until_expired(shard: &Shard) {
  let deadline = Instant::now() + Duration::from_secs(5);
  while shard.lifecycle().is_attached() {
    assert!(
      Instant::now() < deadline,
      "写后维护的 ExpiredTerm 未把 shard 置 Expired"
    );
    sleep(Duration::from_millis(5)).await;
  }
}

/// 铺满一整批互不相同的 key（全新页，放大率约 1，不会触发维护）。
async fn fill(shard: &Shard, tbl: &TableIdent, term: u64) {
  put_batch(shard, tbl, entries(0..FULL_KEYS, 1), term)
    .await
    .unwrap();
  wait_write_running_zero(shard).await;
  assert!(
    shard.lifecycle().is_attached(),
    "铺数据本身不得改变生命周期"
  );
}

/// 更高 term 的 owner 在同一盘面写出 term 7 的归档 manifest（GC 会扫描到）。
async fn higher_term_archive(root: &Path, tbl: &TableIdent) {
  let mut higher = Store::new(
    CompioStore::new(root.to_path_buf(), PAGE_SIZE, PAGES_PER_FILE_SHIFT)
      .with_segment_config(4096, 2),
    PAGE_SIZE,
    16,
  )
  .with_branch("main", 7);
  higher.reopen(tbl).await.unwrap();
  higher.create_archive(tbl, "tag1", 0).await.unwrap();
}

/// sem-26：同 branch 的分支名（normalize 后与 attach 一致）。
const BRANCH: &str = "f1";

/// 以 main 建出分支 `f1`，再以 term 3 attach 并铺满一整批 key：
/// main 先写少量数据（产出 `manifest_main_0`），`CreateBranch` 克隆出
/// `manifest_f1_0`，分支写入的 manifest 记录 `(f1, 3)` 区间。
async fn fill_branch(shard: &Shard, tbl: &TableIdent) {
  shard.request(attach("main", 0, 5)).await.unwrap();
  put_batch(shard, tbl, entries(0..2, 1), 5).await.unwrap();
  wait_write_running_zero(shard).await;
  let response = shard
    .request(Request::CreateBranch {
      tbl: tbl.clone(),
      branch: BRANCH.to_owned(),
    })
    .await
    .unwrap();
  assert!(
    matches!(response, Response::CreateBranch(_)),
    "建分支失败: {response:?}"
  );
  wait_write_running_zero(shard).await;
  // 换 branch 必须先从 main detach（attach 只接受同 branch 幂等 / 提 term）。
  shard
    .request(Request::DetachShard { shard_id: 0 })
    .await
    .unwrap();
  shard.request(attach(BRANCH, 0, 3)).await.unwrap();
  fill(shard, tbl, 3).await;
}

/// 同一 branch 的更高 term owner（term 7）：重开 `manifest_f1_0` 后写出
/// `manifest_f1_7_tag1`，旧 owner 的 GC 会在同一分区扫描到 `(f1, 7)`。
async fn higher_term_branch_archive(root: &Path, tbl: &TableIdent) {
  let mut higher = Store::new(
    CompioStore::new(root.to_path_buf(), PAGE_SIZE, PAGES_PER_FILE_SHIFT)
      .with_segment_config(4096, 2),
    PAGE_SIZE,
    16,
  )
  .with_pages_per_file_shift(PAGES_PER_FILE_SHIFT);
  higher.open_branch(tbl, BRANCH, 7).await.unwrap();
  higher.create_archive(tbl, "tag1", 0).await.unwrap();
}

#[compio::test]
async fn put_maintenance_expired_term_marks_shard_expired() {
  let root = TempRoot::new("sem22-maintain-expired");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  let tbl = TableIdent::new("sem22", 1).unwrap();

  shard.request(attach("main", 0, 5)).await.unwrap();
  fill(&shard, &tbl, 5).await;
  higher_term_archive(root.path(), &tbl).await;

  // 覆盖前一半 key：分配翻倍、大量死页 → 写任务末尾 `needs_maintain` 为真，
  // 任务内 compact + file_gc 扫到 term 7 的 manifest。
  let response = put_batch(&shard, &tbl, entries(0..OVERWRITE_KEYS, 2), 5).await;
  assert!(
    matches!(response, Ok(Response::Put)),
    "写后维护的 ExpiredTerm 不得改写已成功的写回执: {response:?}"
  );

  // 回执先于 WriteDone：等事件循环回收后生命周期镜像翻转为非托管。
  wait_until_expired(&shard).await;

  // 后续普通读写经 `admit` 快速失败，而不是继续以旧 term 放行。
  assert_eq!(
    get(&shard, &tbl, key(0)).await.unwrap_err(),
    KvError::ExpiredTerm
  );
  assert_eq!(
    put_batch(&shard, &tbl, entries(0..1, 3), 5)
      .await
      .unwrap_err(),
    KvError::ExpiredTerm
  );

  shard.shutdown().await.unwrap();
}

#[compio::test]
async fn non_term_maintenance_error_keeps_shard_attached() {
  reset();
  let root = TempRoot::new("sem22-maintain-iofail");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  let tbl = TableIdent::new("sem22", 1).unwrap();

  shard.request(attach("main", 0, 5)).await.unwrap();
  fill(&shard, &tbl, 5).await;

  // 同一次覆盖写的写后维护：Put 本体的 `SyncData` 是第 1 次，compact 的
  // `sync_data` 是第 2 次 → 只有维护拿到 IoFail，写回执保持成功。
  set_spec(&format!("{}@2!err=IoFail", points::SYNC_DATA));
  let response = put_batch(&shard, &tbl, entries(0..OVERWRITE_KEYS, 2), 5).await;
  assert!(
    matches!(response, Ok(Response::Put)),
    "非 term 维护错误不得改写写回执: {response:?}"
  );
  wait_write_running_zero(&shard).await;
  assert!(
    fired(points::SYNC_DATA),
    "维护的同步注入必须命中，否则非 term 对照无效"
  );
  reset();

  // 非 term 维护错误只告警：生命周期不变，shard 仍可正常读写。
  assert!(
    shard.lifecycle().is_attached(),
    "非 term 维护错误不得把 shard 置 Expired"
  );
  assert!(matches!(
    get(&shard, &tbl, key(FULL_KEYS - 1)).await.unwrap(),
    Response::Get(Some(value)) if value.len() == VALUE_LEN
  ));

  shard.shutdown().await.unwrap();
}

/// sem-26：同一 branch 被更高 term 接管后，旧 owner 的写后维护必须把 shard
/// 置 `Expired`。此前 `maintenance_expiry` 只采信主 branch，分支上真实的
/// 「本 branch 更高 term」信号被丢弃，旧 owner 继续以旧 term 写并静默丢写。
#[compio::test]
async fn branch_same_term_takeover_marks_shard_expired() {
  let root = TempRoot::new("sem26-branch-takeover");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  let tbl = TableIdent::new("sem26", 1).unwrap();

  fill_branch(&shard, &tbl).await;
  higher_term_branch_archive(root.path(), &tbl).await;

  // 覆盖写：写后维护的 file_gc 扫到同 branch 的 term 7 manifest。
  let response = put_batch(&shard, &tbl, entries(0..OVERWRITE_KEYS, 2), 3).await;
  assert!(
    matches!(response, Ok(Response::Put)),
    "写后维护的 ExpiredTerm 不得改写已成功的写回执: {response:?}"
  );

  wait_until_expired(&shard).await;

  assert_eq!(
    get(&shard, &tbl, key(0)).await.unwrap_err(),
    KvError::ExpiredTerm
  );
  assert_eq!(
    put_batch(&shard, &tbl, entries(0..1, 3), 3)
      .await
      .unwrap_err(),
    KvError::ExpiredTerm
  );

  shard.shutdown().await.unwrap();
}

/// sem-26 对照：只有祖先 main 的更高 term（同一分区里的另一 branch）时，
/// 分支的维护 term 检查不得把本 shard 误置 `Expired`。全局闸门仍然生效：
/// 本轮 GC 不删除任何文件，只返回 0；分支保持可读写。
#[compio::test]
async fn branch_with_only_main_higher_term_stays_attached() {
  let root = TempRoot::new("sem26-branch-main-higher");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  let tbl = TableIdent::new("sem26", 1).unwrap();

  fill_branch(&shard, &tbl).await;
  // 主 branch 的更高 term（term 7）写出 `manifest_main_7_tag1`：分支 f1 的
  // GC 会看到全局更高 term，但 branch 过滤后没有 (f1, >3) 的 manifest。
  higher_term_archive(root.path(), &tbl).await;

  // 写后维护路径：回执不变、生命周期不变。
  let response = put_batch(&shard, &tbl, entries(0..OVERWRITE_KEYS, 2), 3).await;
  assert!(
    matches!(response, Ok(Response::Put)),
    "分支写回执不得被其它 branch 的更高 term 改写: {response:?}"
  );
  wait_write_running_zero(&shard).await;
  assert!(
    shard.lifecycle().is_attached(),
    "只有祖先 main 更高 term 不得把分支 shard 置 Expired"
  );

  // 显式 FileGc：确定性走同一 term 熔断，回 0（本轮不删）而不是 ExpiredTerm。
  let response = shard
    .request(Request::FileGc { tbl: tbl.clone() })
    .await
    .unwrap();
  assert!(
    matches!(response, Response::FileGc(0)),
    "分支只看到 main 更高 term 时 GC 应跳过删除而不是报过期: {response:?}"
  );
  wait_write_running_zero(&shard).await;
  assert!(shard.lifecycle().is_attached());

  // 分支继续可读写。
  assert!(matches!(
    get(&shard, &tbl, key(FULL_KEYS - 1)).await.unwrap(),
    Response::Get(Some(value)) if value.len() == VALUE_LEN
  ));
  assert!(matches!(
    put_batch(&shard, &tbl, entries(300..301, 4), 3).await,
    Ok(Response::Put)
  ));

  shard.shutdown().await.unwrap();
}
