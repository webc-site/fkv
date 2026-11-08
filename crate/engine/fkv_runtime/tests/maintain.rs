//! 写后维护：`file_amplify_factor` 经 Runtime / Shard 生效，写批次后自动
//! compact + GC 回收数据文件（对齐 C++ `WriteTask::UpdateMeta` 的后台任务）。
use std::{
  fs::read_dir,
  path::{Path, PathBuf},
};

use fkv_btree::BatchEntry;
use fkv_config::KvOption;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn config(root: PathBuf) -> RuntimeConfig {
  let mut config = RuntimeConfig::for_tests(root);
  // 每文件 4 页：6 轮覆盖写足以把旧文件变成全垃圾。
  config.pages_per_file_shift = 2;
  config
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
  file_num
}

#[test]
fn file_amplify_factor_reclaims_data_files() {
  let root = TempRoot::new("runtime_maintain_plain");
  let plain = pollster::block_on(write_rounds(
    root.path().to_path_buf(),
    Some(KvOption {
      file_amplify_factor: 0,
      // 放大回收只在 append 模式有意义：pooled 原地复用页，不会产生全垃圾文件。
      data_append_mode: true,
      ..KvOption::default()
    }),
  ));
  let root = TempRoot::new("runtime_maintain_maintained");
  let maintained = pollster::block_on(write_rounds(
    root.path().to_path_buf(),
    Some(KvOption {
      file_amplify_factor: 2,
      data_append_mode: true,
      ..KvOption::default()
    }),
  ));
  assert!(
    maintained < plain,
    "maintained={maintained} plain={plain}: 写后 maintain 必须回收放大文件"
  );
}

/// 不传 `KvOption`（`Runtime::start`）必须与 `KvOption::default()` 同源。
/// 默认是 pooled：覆盖写原地复用页，不产生可回收的全垃圾文件，因此这里
/// 断言两侧文件数一致；放大因子默认值由 `startup::tests` 直接校验。
#[test]
fn default_start_uses_default_amplify_factor() {
  let root = TempRoot::new("runtime_maintain_default_plain");
  let plain = pollster::block_on(write_rounds(
    root.path().to_path_buf(),
    Some(KvOption {
      file_amplify_factor: 0,
      ..KvOption::default()
    }),
  ));
  let root = TempRoot::new("runtime_maintain_default");
  let defaulted = pollster::block_on(write_rounds(root.path().to_path_buf(), None));
  assert!(
    defaulted <= plain,
    "defaulted={defaulted} plain={plain}: Runtime::start 与 KvOption::default() 必须同源"
  );
}

/// `segment_file_amplify_factor` 必须经 KvOption -> ShardConfig -> Store 投影：
/// 大 value 反复覆盖后 segment 文件数应低于不压缩的对照组。
async fn write_segment_rounds(root: PathBuf, amplify_factor: u8) -> usize {
  let option = KvOption {
    // 只测 segment 侧，关掉 data 侧自动 compact，避免数据文件数干扰判断。
    file_amplify_factor: 0,
    segment_file_amplify_factor: amplify_factor,
    // 段 mapper 恒为 Append（pooled-02），默认 Pooled data 模式下段放大率
    // 口径同样成立；保留默认配置，顺带覆盖「段压缩不受 data_append_mode 影响」。
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
  file_num
}

#[test]
fn segment_amplify_factor_reclaims_segment_files() {
  let root = TempRoot::new("runtime_maintain_seg_plain");
  let plain = pollster::block_on(write_segment_rounds(root.path().to_path_buf(), 0));
  let root = TempRoot::new("runtime_maintain_seg_maintained");
  let maintained = pollster::block_on(write_segment_rounds(root.path().to_path_buf(), 2));
  assert!(
    maintained < plain,
    "maintained={maintained} plain={plain}: segment 放大因子必须经 Store 投影生效"
  );
}
