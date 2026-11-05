//! rev-42a：`root_meta_cache_size` 经 `KvOption → Runtime → Shard → Store` 接线。
//!
//! 预算压到 1 字节时，写第二张表会驱逐第一张表的整表状态（含 index 缓存），
//! 之后读取第一张表走 reopen 重载；若配置没有接通 store，缓存不会下降。

use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_config::KvOption;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_types::TableIdent;

fn temp_root(tag: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_runtime_root_meta_{tag}_{}_{}_{}",
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
  let root = temp_root("reaches");
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
    let runtime = Runtime::start_with_option(config(root.clone()), &option).unwrap();
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
  remove_dir_all(root).unwrap();
}
