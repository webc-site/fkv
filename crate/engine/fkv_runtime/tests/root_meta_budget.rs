//! rev-42a：`root_meta_cache_size` 经 `KvOption → Runtime → Shard → Store` 接线。
//!
//! 预算压到 1 字节时，写第二张表会驱逐第一张表的整表状态（含 index 缓存），
//! 之后读取第一张表走 reopen 重载；若配置没有接通 store，缓存不会下降。

use std::path::PathBuf;

use fkv_btree::BatchEntry;
use fkv_config::KvOption;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn config(root: PathBuf) -> RuntimeConfig {
  RuntimeConfig {
    num_threads: 1,
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

#[test]
fn root_meta_cache_size_reaches_store() {
  let root = TempRoot::new("runtime_root_meta_reaches");
  let tbl_a = TableIdent::new("demo", 1).unwrap();
  let tbl_b = TableIdent::new("demo", 2).unwrap();
  let option = KvOption {
    root_meta_cache_size: 1,
    ..KvOption::default()
  };
  let batch_a: Vec<BatchEntry> = (0..2000u32)
    .map(|idx| BatchEntry::upsert(format!("k{idx:04}").into_bytes(), vec![b'v'; 8], 1))
    .collect();
  pollster::block_on(async {
    let runtime = Runtime::start_with_option(config(root.path().to_path_buf()), &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    runtime.put(&tbl_a, batch_a).await.unwrap();
    for idx in (0..2000).step_by(3) {
      assert_eq!(
        runtime
          .get(&tbl_a, format!("k{idx:04}").as_bytes())
          .await
          .unwrap(),
        Some(vec![b'v'; 8])
      );
    }
    let warmed = runtime.metrics().shard_li[0].index_cache_bytes;
    assert!(warmed > 0, "读路径必须填充 index 缓存: {warmed}");

    // 预算 1 字节：写 B 时 A 超限且 A 未被打快照，必须被整表驱逐。
    runtime
      .put(
        &tbl_b,
        vec![BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1)],
      )
      .await
      .unwrap();
    let after = runtime.metrics().shard_li[0].index_cache_bytes;
    assert!(
      after < warmed,
      "root_meta_cache_size 未接通 store（A 的缓存未释放）: {warmed} -> {after}"
    );

    // 被驱逐的表按需 reopen，数据完整。
    assert_eq!(
      runtime.get(&tbl_a, b"k0000").await.unwrap(),
      Some(vec![b'v'; 8])
    );
    runtime.shutdown().await.unwrap();
  });
}
