use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_metrics::Op;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

#[test]
fn runtime_branch_and_archive_requests() {
  let root = TempRoot::new("runtime_branch");
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
      auto_reopen_retry_times: 0,
      auto_reopen_pending_time_us: 0,
      archive_interval_secs: 0,
      archive_num_retained: 100,
      queue_size: 64,
    })
    .unwrap();
    runtime.attach_shard(0, "main", 3).await.unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      )
      .await
      .unwrap();

    let first = runtime.create_archive(&tbl, "001", 2).await.unwrap();
    assert_eq!(first.name, "manifest_main_3_001");
    assert!(first.bytes > 0);
    assert_eq!(first.removed_num, 0);
    runtime.create_archive(&tbl, "002", 2).await.unwrap();
    let third = runtime.create_archive(&tbl, "003", 2).await.unwrap();
    assert_eq!(third.removed_num, 1);
    assert!(!root.join("demo.1").join("manifest_main_3_001").exists());
    assert!(root.join("demo.1").join("manifest_main_3_002").exists());
    assert!(root.join("demo.1").join("manifest_main_3_003").exists());

    let branch = runtime.create_branch(&tbl, "Feature").await.unwrap();
    assert_eq!(branch.name, "manifest_feature_0");
    assert!(root.join("demo.1").join(&branch.name).exists());
    assert_eq!(runtime.delete_branch(&tbl, "feature").await.unwrap(), 1);
    assert!(!root.join("demo.1").join(&branch.name).exists());

    assert_eq!(
      runtime
        .create_archive(&tbl, "bad/tag", 1)
        .await
        .unwrap_err(),
      KvError::InvalidArgs
    );
    assert_eq!(
      runtime.delete_branch(&tbl, "main").await.unwrap_err(),
      KvError::InvalidArgs
    );
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

    let metrics = runtime.metrics();
    assert_eq!(metrics.total_requests(Op::CreateArchive), 4);
    assert_eq!(metrics.total_errors(Op::CreateArchive), 1);
    assert_eq!(metrics.total_requests(Op::CreateBranch), 1);
    assert_eq!(metrics.total_requests(Op::DeleteBranch), 2);
    assert_eq!(metrics.total_errors(Op::DeleteBranch), 1);
    runtime.shutdown().await.unwrap();
  });
}
