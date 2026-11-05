//! 写后维护：`file_amplify_factor` 经 Runtime / Shard 生效，写批次后自动
//! compact + GC 回收数据文件（对齐 C++ `WriteTask::UpdateMeta` 的后台任务）。
use std::{
  env::temp_dir,
  fs::{create_dir_all, read_dir, remove_dir_all},
  path::{Path, PathBuf},
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
    "fkv_runtime_maintain_{tag}_{}_{}_{}",
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
    // 每文件 4 页：6 轮覆盖写足以把旧文件变成全垃圾。
    pages_per_file_shift: 2,
    segment_size: 4096,
    segments_per_file_shift: 2,
    large_value_threshold: 1024,
    fixed_buffer_size: 0,
    queue_size: 64,
    auto_reopen_retry_times: 0,
    auto_reopen_pending_time_us: 0,
    archive_interval_secs: 0,
    archive_num_retained: 100,
  }
}

/// `<root>/<tbl>.<partition>/` 下 `data_*` 文件数。
fn data_file_num(root: &Path, tbl: &TableIdent) -> usize {
  read_dir(root.join(format!("{}.{}", tbl.tbl_name, tbl.partition_id)))
    .map(|entries| {
      entries
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("data_"))
        .count()
    })
    .unwrap_or(0)
}

/// `<root>/<tbl>.<partition>/` 下 `segment_*` 文件数。
fn segment_file_num(root: &Path, tbl: &TableIdent) -> usize {
  read_dir(root.join(format!("{}.{}", tbl.tbl_name, tbl.partition_id)))
    .map(|entries| {
      entries
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("segment_"))
        .count()
    })
    .unwrap_or(0)
}

/// 反复覆盖写同一批 key，返回最终 `data_*` 文件数。
async fn write_rounds(root: PathBuf, option: Option<KvOption>) -> usize {
  let runtime = match &option {
    Some(option) => Runtime::start_with_option(config(root.clone()), option),
    None => Runtime::start(config(root.clone())),
  }
  .unwrap();
  runtime.attach_shard(0, "main", 1).await.unwrap();
  let tbl = TableIdent::new("demo", 1).unwrap();
  for round in 1..=6u64 {
    let batch: Vec<BatchEntry> = (0..60u32)
      .map(|index| BatchEntry::upsert(format!("k{index:03}").into_bytes(), vec![b'v'; 30], round))
      .collect();
    runtime.put(&tbl, batch).await.unwrap();
  }
  // 维护在写请求回复之后执行：再发一个读请求作为同步屏障。
  assert!(runtime.get(&tbl, b"k000").await.unwrap().is_some());
  let file_num = data_file_num(&root, &tbl);
  runtime.shutdown().await.unwrap();
  let _ = remove_dir_all(root);
  file_num
}

#[test]
fn file_amplify_factor_reclaims_data_files() {
  let plain = pollster::block_on(write_rounds(
    temp_root("plain"),
    Some(KvOption {
      file_amplify_factor: 0,
      ..KvOption::default()
    }),
  ));
  let maintained = pollster::block_on(write_rounds(
    temp_root("maintained"),
    Some(KvOption {
      file_amplify_factor: 2,
      ..KvOption::default()
    }),
  ));
  assert!(
    maintained < plain,
    "maintained={maintained} plain={plain}: 写后 maintain 必须回收放大文件"
  );
}

/// 不传 `KvOption`（`Runtime::start`）必须与 `KvOption::default()` 同源：
/// 默认放大因子 2 生效，而不是默认关闭压缩。
#[test]
fn default_start_uses_default_amplify_factor() {
  let plain = pollster::block_on(write_rounds(
    temp_root("default_plain"),
    Some(KvOption {
      file_amplify_factor: 0,
      ..KvOption::default()
    }),
  ));
  let defaulted = pollster::block_on(write_rounds(temp_root("default"), None));
  assert!(
    defaulted < plain,
    "defaulted={defaulted} plain={plain}: Runtime::start 必须套用 KvOption 默认放大因子"
  );
}

/// `segment_file_amplify_factor` 必须经 KvOption -> ShardConfig -> Store 投影：
/// 大 value 反复覆盖后 segment 文件数应低于不压缩的对照组。
async fn write_segment_rounds(root: PathBuf, amplify_factor: u8) -> usize {
  let option = KvOption {
    // 只测 segment 侧，关掉 data 侧自动 compact，避免数据文件数干扰判断。
    file_amplify_factor: 0,
    segment_file_amplify_factor: amplify_factor,
    ..KvOption::default()
  };
  let runtime = Runtime::start_with_option(config(root.clone()), &option).unwrap();
  runtime.attach_shard(0, "main", 1).await.unwrap();
  let tbl = TableIdent::new("demo", 1).unwrap();
  // 常驻大 value 保证 segment 压缩时有 live 段可搬。
  runtime
    .put_large(
      &tbl,
      vec![BatchEntry::upsert(b"keep".to_vec(), vec![b'k'; 9000], 1)],
    )
    .await
    .unwrap();
  for round in 1..=12u64 {
    let value = vec![round as u8; 9000 + round as usize];
    runtime
      .put_large(
        &tbl,
        vec![BatchEntry::upsert(b"big".to_vec(), value, round + 1)],
      )
      .await
      .unwrap();
  }
  // 写后维护在回执之后执行：再发一个读请求作为同步屏障。
  assert!(runtime.get_large(&tbl, b"big").await.unwrap().is_some());
  let file_num = segment_file_num(&root, &tbl);
  runtime.shutdown().await.unwrap();
  let _ = remove_dir_all(root);
  file_num
}

#[test]
fn segment_amplify_factor_reclaims_segment_files() {
  let plain = pollster::block_on(write_segment_rounds(temp_root("seg_plain"), 0));
  let maintained = pollster::block_on(write_segment_rounds(temp_root("seg_maintained"), 2));
  assert!(
    maintained < plain,
    "maintained={maintained} plain={plain}: segment 放大因子必须经 Store 投影生效"
  );
}
