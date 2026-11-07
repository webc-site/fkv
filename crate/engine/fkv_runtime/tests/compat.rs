use fkv_api::ScanPageOption;
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_metrics::Op;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

#[test]
fn floor_read_error_codes_and_truncate_position() {
  let root = TempRoot::new("runtime_compat");
  pollster::block_on(async {
    let runtime = Runtime::start(RuntimeConfig {
      num_threads: 1,
      root: root.path().to_path_buf(),
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
        .scan_page(
          &tbl,
          b"a",
          b"b",
          ScanPageOption {
            begin_inclusive: true,
            end_inclusive: true,
            ..ScanPageOption::default()
          },
        )
        .await
        .unwrap()
        .entry_li
        .len(),
      2
    );
    assert_eq!(
      runtime
        .scan_page(
          &tbl,
          b"a",
          b"b",
          ScanPageOption {
            begin_inclusive: false,
            end_inclusive: false,
            ..ScanPageOption::default()
          },
        )
        .await
        .unwrap()
        .entry_li,
      Vec::<(Vec<u8>, Vec<u8>)>::new()
    );
    assert_eq!(
      runtime
        .scan_page(
          &tbl,
          b"a",
          b"",
          ScanPageOption {
            begin_inclusive: true,
            ..ScanPageOption::default()
          },
        )
        .await
        .unwrap()
        .entry_li
        .len(),
      2
    );

    let first = runtime
      .scan_page(
        &tbl,
        b"",
        b"",
        ScanPageOption {
          page_entries: 1,
          ..ScanPageOption::default()
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
        ScanPageOption {
          begin_inclusive: false,
          page_entries: 1,
          ..ScanPageOption::default()
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
}

#[test]
fn runtime_scan_returns_large_value_unsupported() {
  let root = TempRoot::new("runtime_compat");
  pollster::block_on(async {
    let runtime = Runtime::start(RuntimeConfig {
      num_threads: 1,
      root: root.path().to_path_buf(),
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
        .scan_page(
          &tbl,
          b"a",
          b"",
          ScanPageOption {
            begin_inclusive: false,
            ..ScanPageOption::default()
          },
        )
        .await
        .unwrap_err(),
      KvError::LargeValueUnsupported
    );
    let metrics = runtime.metrics();
    assert!(metrics.total_errors(Op::Scan) >= 2);
    runtime.shutdown().await.unwrap();
  });
}

/// sem-34：运行时请求层的有意差异锁定。
///
/// C++ `ReadRequest` 默认对 large entry 返回 `NoError + metadata` 且总是输出
/// `ts_` / `expire_ts_`；fkv 的 `Request::Get` 保持 value-only 分层（large →
/// `LargeValueUnsupported`），完整大 value 走 `Request::GetLarge`，entry 级
/// `ts` / `expire_ts` / metadata-only 读法在进程内用
/// `fkv_store::Store::get_entry` / `fkv_sdk::KvStore::get_entry`。本测试锁定
/// 请求层不被静默改成另一种形态。
#[test]
fn runtime_point_read_keeps_large_value_layer_boundary() {
  let root = TempRoot::new("runtime_compat");
  pollster::block_on(async {
    let runtime = Runtime::start(RuntimeConfig {
      num_threads: 1,
      root: root.path().to_path_buf(),
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
    let value: Vec<u8> = (0..9000).map(|index| (index % 251) as u8).collect();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      )
      .await
      .unwrap();
    runtime
      .put_large(
        &tbl,
        vec![BatchEntry::upsert(b"b".to_vec(), value.clone(), 7).with_metadata(b"meta")],
      )
      .await
      .unwrap();

    // 普通 value：`Request::Get` 仍返回完整 value。
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    // large value：请求层保持 `LargeValueUnsupported`（issue 15 的分层）。
    assert_eq!(
      runtime.get(&tbl, b"b").await.unwrap_err(),
      KvError::LargeValueUnsupported
    );
    // 完整大 value（含 metadata）走 `Request::GetLarge`。
    let large = runtime.get_large(&tbl, b"b").await.unwrap().unwrap();
    assert_eq!(large.value, value);
    assert_eq!(large.metadata, b"meta");
    // `Floor` 是请求层现有的 `ts` / `expire_ts` 载体，可作迁移绕行。
    let floor = runtime.floor(&tbl, b"c").await.unwrap();
    assert_eq!(floor.key, b"b");
    assert_eq!(floor.timestamp, 7);
    assert_eq!(floor.expire_ts, 0);
    runtime.shutdown().await.unwrap();
  });
}
