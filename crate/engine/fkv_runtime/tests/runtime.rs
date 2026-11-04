use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_metrics::Op;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_types::TableIdent;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_runtime_{}_{}_{}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos(),
    seq
  ));
  create_dir_all(&root).unwrap();
  root
}

fn config(root: PathBuf, num_threads: u16) -> RuntimeConfig {
  RuntimeConfig {
    num_threads,
    root,
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

async fn attach_all(runtime: &Runtime, branch: &str, term: u64) {
  for shard_id in 0..u32::from(runtime.num_threads()) {
    runtime.attach_shard(shard_id, branch, term).await.unwrap();
  }
}

#[test]
fn thread_per_core_put_get_scan_reopen() {
  let root = temp_root();
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.clone(), 2)).unwrap();
    attach_all(&runtime, "main", 1).await;
    let tbl_a = TableIdent::new("demo", 1).unwrap();
    let tbl_b = TableIdent::new("demo", 2).unwrap();
    runtime
      .put(
        &tbl_a,
        vec![
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    runtime
      .put(
        &tbl_b,
        vec![BatchEntry::upsert(b"x".to_vec(), b"X".to_vec(), 1)],
      )
      .await
      .unwrap();
    assert_eq!(
      runtime.get(&tbl_a, b"a").await.unwrap(),
      Some(b"A".to_vec())
    );
    assert_eq!(runtime.scan(&tbl_a, b"", b"", 0).await.unwrap().len(), 2);
    assert_eq!(
      runtime.get(&tbl_b, b"x").await.unwrap(),
      Some(b"X".to_vec())
    );
    runtime.shutdown().await.unwrap();

    let runtime = Runtime::start(config(root.clone(), 2)).unwrap();
    attach_all(&runtime, "main", 1).await;
    runtime.reopen(&tbl_a).await.unwrap();
    runtime.reopen(&tbl_b).await.unwrap();
    assert_eq!(
      runtime.get(&tbl_a, b"b").await.unwrap(),
      Some(b"B".to_vec())
    );
    assert_eq!(
      runtime.get(&tbl_b, b"x").await.unwrap(),
      Some(b"X".to_vec())
    );
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}

#[test]
fn detach_and_term_validation() {
  let root = temp_root();
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.clone(), 1)).unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap_err(),
      KvError::NotRunning
    );
    runtime.attach_shard(0, "main", 5).await.unwrap();
    runtime
      .put_with_term(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
        5,
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(
      runtime
        .put_with_term(
          &tbl,
          vec![BatchEntry::upsert(b"a".to_vec(), b"A2".to_vec(), 1)],
          4,
        )
        .await
        .unwrap_err(),
      KvError::ExpiredTerm
    );
    assert_eq!(
      runtime.attach_shard(0, "main", 4).await.unwrap_err(),
      KvError::ExpiredTerm
    );

    runtime.detach_shard(0).await.unwrap();
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap_err(),
      KvError::NotRunning
    );
    assert_eq!(
      runtime
        .put_with_term(
          &tbl,
          vec![BatchEntry::upsert(b"a".to_vec(), b"A3".to_vec(), 1)],
          5,
        )
        .await
        .unwrap_err(),
      KvError::NotRunning
    );

    runtime.attach_shard(0, "main", 6).await.unwrap();
    assert_eq!(
      runtime
        .put_with_term(
          &tbl,
          vec![BatchEntry::upsert(b"a".to_vec(), b"A3".to_vec(), 1)],
          5,
        )
        .await
        .unwrap_err(),
      KvError::ExpiredTerm
    );
    runtime
      .put_with_term(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A3".to_vec(), 1)],
        6,
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A3".to_vec()));

    // 换 branch 后 term 重新计数；先为 other table 创建 branch manifest。
    let tbl_other = TableIdent::new("other", 1).unwrap();
    runtime.create_branch(&tbl_other, "other").await.unwrap();
    runtime.attach_shard(0, "other", 1).await.unwrap();
    runtime
      .put_with_term(
        &tbl_other,
        vec![BatchEntry::upsert(b"x".to_vec(), b"X".to_vec(), 1)],
        1,
      )
      .await
      .unwrap();
    assert_eq!(
      runtime.get(&tbl_other, b"x").await.unwrap(),
      Some(b"X".to_vec())
    );
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}

#[test]
fn truncate_and_drop_requests() {
  let root = temp_root();
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.clone(), 1)).unwrap();
    attach_all(&runtime, "main", 1).await;
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime
      .put(
        &tbl,
        vec![
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    runtime.truncate(&tbl).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), None);
    assert!(runtime.scan(&tbl, b"", b"", 0).await.unwrap().is_empty());

    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"x".to_vec(), b"X".to_vec(), 2)],
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"x").await.unwrap(), Some(b"X".to_vec()));
    runtime.drop_table(&tbl).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"x").await.unwrap(), None);

    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"z".to_vec(), b"Z".to_vec(), 3)],
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"z").await.unwrap(), Some(b"Z".to_vec()));
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}

#[test]
fn metrics_track_requests_latency_and_io() {
  let root = temp_root();
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.clone(), 2)).unwrap();
    attach_all(&runtime, "main", 1).await;
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    runtime.scan(&tbl, b"", b"", 0).await.unwrap();

    let metrics = runtime.metrics();
    assert_eq!(metrics.shard_li.len(), 2);
    assert_eq!(metrics.total_requests(Op::Put), 1);
    assert_eq!(metrics.total_requests(Op::Scan), 1);
    assert_eq!(metrics.total_errors(Op::Put), 0);
    let put_latency: u64 = metrics
      .shard_li
      .iter()
      .map(|shard| shard.op(Op::Put).latency.count)
      .sum();
    assert_eq!(put_latency, 1);
    assert!(
      metrics
        .shard_li
        .iter()
        .map(|shard| shard.page_writes)
        .sum::<u64>()
        > 0
    );
    assert!(
      metrics
        .shard_li
        .iter()
        .map(|shard| shard.bytes_written)
        .sum::<u64>()
        > 0
    );

    assert_eq!(
      runtime
        .put_with_term(
          &tbl,
          vec![BatchEntry::upsert(b"a".to_vec(), b"A2".to_vec(), 2)],
          2,
        )
        .await
        .unwrap_err(),
      KvError::ExpiredTerm
    );
    let metrics = runtime.metrics();
    assert_eq!(metrics.total_errors(Op::Put), 1);
    assert_eq!(metrics.total_requests(Op::Put), 2);
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}
