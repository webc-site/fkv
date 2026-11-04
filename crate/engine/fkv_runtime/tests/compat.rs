use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_api::ScanPageOptions;
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_metrics::Op;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_types::TableIdent;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_runtime_compat_{}_{}_{}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos(),
    seq,
  ));
  create_dir_all(&root).unwrap();
  root
}

#[test]
fn floor_read_error_codes_and_truncate_position() {
  let root = temp_root();
  pollster::block_on(async {
    let runtime = Runtime::start(RuntimeConfig {
      num_threads: 1,
      root: root.clone(),
      page_size: 512,
      restart_interval: 16,
      pages_per_file_shift: 18,
      segment_size: 4096,
      segments_per_file_shift: 2,
      large_value_threshold: 1024,
      fixed_buffer_size: 0,
      queue_size: 64,
      auto_reopen_retry_times: 0,
      auto_reopen_pending_time_us: 0,
      archive_interval_secs: 0,
      archive_num_retained: 100,
    })
    .unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime
      .put(
        &tbl,
        vec![
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
          BatchEntry::upsert(b"d".to_vec(), b"D".to_vec(), 1),
        ],
      )
      .await
      .unwrap();

    assert_eq!(runtime.floor(&tbl, b"c").await.unwrap().key, b"b");
    assert_eq!(runtime.floor(&tbl, b"z").await.unwrap().key, b"d");
    assert_eq!(runtime.floor(&tbl, b"a").await.unwrap().value, b"A");
    assert_eq!(
      runtime.floor(&tbl, b"0").await.unwrap_err(),
      KvError::NotFound
    );
    assert_eq!(runtime.read(&tbl, b"a").await.unwrap(), b"A");
    assert_eq!(
      runtime.read(&tbl, b"missing").await.unwrap_err(),
      KvError::NotFound
    );

    runtime.truncate_from(&tbl, b"c").await.unwrap();
    assert_eq!(
      runtime.scan(&tbl, b"", b"", 0).await.unwrap(),
      vec![
        (b"a".to_vec(), b"A".to_vec()),
        (b"b".to_vec(), b"B".to_vec())
      ]
    );
    assert_eq!(runtime.floor(&tbl, b"z").await.unwrap().key, b"b");
    assert_eq!(runtime.get(&tbl, b"d").await.unwrap(), None);

    assert_eq!(
      runtime
        .scan_opts(&tbl, b"a", b"b", 0, true, true)
        .await
        .unwrap()
        .len(),
      2
    );
    assert_eq!(
      runtime
        .scan_opts(&tbl, b"a", b"b", 0, false, false)
        .await
        .unwrap(),
      Vec::<(Vec<u8>, Vec<u8>)>::new()
    );
    assert_eq!(
      runtime
        .scan_opts(&tbl, b"a", b"", 0, true, false)
        .await
        .unwrap()
        .len(),
      2
    );

    let first = runtime
      .scan_page(
        &tbl,
        b"",
        b"",
        ScanPageOptions {
          page_entries: 1,
          ..ScanPageOptions::default()
        },
      )
      .await
      .unwrap();
    assert_eq!(first.entry_li, vec![(b"a".to_vec(), b"A".to_vec())]);
    assert!(first.has_remaining);
    let begin = first.entry_li.last().unwrap().0.clone();
    let second = runtime
      .scan_page(
        &tbl,
        &begin,
        b"",
        ScanPageOptions {
          begin_inclusive: false,
          page_entries: 1,
          ..ScanPageOptions::default()
        },
      )
      .await
      .unwrap();
    assert_eq!(second.entry_li, vec![(b"b".to_vec(), b"B".to_vec())]);
    assert!(!second.has_remaining);

    let metrics = runtime.metrics();
    assert!(metrics.total_requests(Op::Floor) >= 4);
    assert_eq!(metrics.total_errors(Op::Floor), 1);

    runtime.truncate_from(&tbl, b"").await.unwrap();
    assert!(runtime.scan(&tbl, b"", b"", 0).await.unwrap().is_empty());
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}

#[test]
fn runtime_scan_returns_large_value_unsupported() {
  let root = temp_root();
  pollster::block_on(async {
    let runtime = Runtime::start(RuntimeConfig {
      num_threads: 1,
      root: root.clone(),
      page_size: 512,
      restart_interval: 16,
      pages_per_file_shift: 18,
      segment_size: 4096,
      segments_per_file_shift: 2,
      large_value_threshold: 1024,
      fixed_buffer_size: 0,
      queue_size: 64,
      auto_reopen_retry_times: 0,
      auto_reopen_pending_time_us: 0,
      archive_interval_secs: 0,
      archive_num_retained: 100,
    })
    .unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      )
      .await
      .unwrap();
    let big: Vec<u8> = (0..9000).map(|index| (index % 251) as u8).collect();
    runtime
      .put_large(&tbl, vec![BatchEntry::upsert(b"b".to_vec(), big, 1)])
      .await
      .unwrap();

    assert_eq!(
      runtime.scan(&tbl, b"", b"", 0).await.unwrap_err(),
      KvError::LargeValueUnsupported
    );
    assert_eq!(
      runtime.scan(&tbl, b"", b"b", 0).await.unwrap(),
      vec![(b"a".to_vec(), b"A".to_vec())]
    );
    assert_eq!(
      runtime
        .scan_opts(&tbl, b"a", b"", 0, false, false)
        .await
        .unwrap_err(),
      KvError::LargeValueUnsupported
    );
    let metrics = runtime.metrics();
    assert!(metrics.total_errors(Op::Scan) >= 2);
    runtime.shutdown().await.unwrap();
  });
  remove_dir_all(root).unwrap();
}
