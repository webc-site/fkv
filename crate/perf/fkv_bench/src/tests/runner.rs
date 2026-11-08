//! runner 时间窗、warmup 与 load-only 测试。

use super::*;

/// `--test-secs` 折算：0 / 负值 / NaN 一律固定次数模式，正值换算成
/// Duration，超出 `Duration` 域的正值饱和到 `MAX` 而非退化。
#[test]
fn window_duration_zero_or_invalid_is_fixed_mode() {
  assert_eq!(window_duration(0.0), Duration::ZERO);
  assert_eq!(window_duration(-1.5), Duration::ZERO);
  assert_eq!(window_duration(f64::NAN), Duration::ZERO);
  assert_eq!(window_duration(0.2), Duration::from_millis(200));
  assert_eq!(window_duration(60.0), Duration::from_secs(60));
  assert_eq!(window_duration(f64::INFINITY), Duration::MAX);
}

/// `--warmup-secs`：先跑一段不计量负载，正式计量仍有完整指标。
#[test]
fn warmup_runs_before_measured_phase() {
  let started = Instant::now();
  let metric_li = bench_workload_ext(
    BenchArgs {
      rows: 32,
      value_size: 16,
      batch_size: 8,
      workload: Workload::WriteOnly,
      mix: WriteMix::default(),
      plan: ReadPlan {
        partitions: 1,
        read_per_part: 1,
      },
      test_secs: 0.0,
      write_batchs: 0,
      scan_bytes: 0,
      ttl_ms: 0,
    },
    None,
    RunControl {
      warmup_secs: 0.2,
      ..RunControl::default()
    },
  );
  assert!(started.elapsed() >= Duration::from_millis(200));
  assert_latency_metric_li(&metric_li, "put");
  // 预热指标不混入正式输出：窗口计数只覆盖正式阶段（远小于预热时长）。
  let window_num = metric_li
    .iter()
    .find(|(name, ..)| name == "put_window_num")
    .map(|item| item.1)
    .unwrap();
  assert!(window_num >= 1.0);
}

/// load-only 保留 store；read-only-existing 复用同一目录只读，指标齐全。
#[test]
fn load_only_then_read_only_existing() {
  let store_dir = env::temp_dir().join(format!("fkv_bench_load_test_{}_{}", id(), now_secs()));
  let args = BenchArgs {
    rows: 32,
    value_size: 16,
    batch_size: 8,
    workload: Workload::WriteRead,
    mix: WriteMix::default(),
    plan: ReadPlan {
      partitions: 1,
      read_per_part: 2,
    },
    test_secs: 0.0,
    write_batchs: 0,
    scan_bytes: 0,
    ttl_ms: 0,
  };
  let load_li = bench_workload_ext(
    args,
    None,
    RunControl {
      mode: RunMode::LoadOnly,
      store_dir: Some(store_dir.clone()),
      resource: true,
      ..RunControl::default()
    },
  );
  assert!(store_dir.is_dir(), "load-only 必须保留 store 目录");
  assert_latency_metric_li(&load_li, "put");
  assert!(
    load_li
      .iter()
      .any(|(name, value, ..)| name == "store_bytes" && *value > 0.0),
    "load-only 应输出 store_bytes"
  );

  let read_li = bench_workload_ext(
    args,
    None,
    RunControl {
      mode: RunMode::ReadOnlyExisting,
      store_dir: Some(store_dir.clone()),
      resource: true,
      ..RunControl::default()
    },
  );
  assert_latency_metric_li(&read_li, "get");
  assert_latency_metric_li(&read_li, "scan");
  // read 阶段不写数据、不清目录。
  assert!(store_dir.is_dir(), "read-only-existing 不允许删 store 目录");
  assert!(!read_li.iter().any(|(name, ..)| name == "put_kv_s"));

  let _ = fs::remove_dir_all(&store_dir);
  let state_path = store_dir.with_file_name(format!(
    "{}.state.json",
    store_dir.file_name().unwrap().to_string_lossy()
  ));
  let _ = fs::remove_file(state_path);
}

/// `--trials > 1` 只重复读 / scan 阶段：写指标仍只有一份，读侧每指标也只有一份中位数。
#[test]
fn phase_trials_keep_one_metric_per_name() {
  let metric_li = bench_workload_ext(
    BenchArgs {
      rows: 32,
      value_size: 16,
      batch_size: 8,
      workload: Workload::WriteRead,
      mix: WriteMix::default(),
      plan: ReadPlan {
        partitions: 1,
        read_per_part: 1,
      },
      test_secs: 0.0,
      write_batchs: 0,
      scan_bytes: 0,
      ttl_ms: 0,
    },
    None,
    RunControl {
      trials: 3,
      ..RunControl::default()
    },
  );
  for prefix in ["put", "get", "scan"] {
    let name = format!("{prefix}_kv_s");
    let count = metric_li
      .iter()
      .filter(|(metric, ..)| metric == &name)
      .count();
    assert_eq!(count, 1, "{name} 应只有一个中位数值");
  }
  assert_latency_metric_li(&metric_li, "get");
  assert_latency_metric_li(&metric_li, "scan");
}

/// `delete-only`：准备阶段的 upsert 不计入指标，计量阶段只输出 delete 指标；
/// 删除预算等于各分区一遍的批数之和。
#[test]
fn delete_only_reports_delete_metrics_without_put() {
  let metric_li = bench_workload(
    BenchArgs {
      rows: 64,
      value_size: 16,
      batch_size: 8,
      workload: Workload::DeleteOnly,
      mix: WriteMix::default(),
      plan: ReadPlan {
        partitions: 2,
        read_per_part: 1,
      },
      test_secs: 0.0,
      write_batchs: 0,
      scan_bytes: 0,
      ttl_ms: 0,
    },
    None,
  );
  let names = metric_name_li(&metric_li);
  assert!(
    names.contains(&"delete_kv_s"),
    "缺少 delete_kv_s: {names:?}"
  );
  assert!(
    names.contains(&"delete_batch_s"),
    "缺少 delete_batch_s: {names:?}"
  );
  assert!(!names.iter().any(|name| name.starts_with("put_")));
  assert!(!names.iter().any(|name| name.starts_with("get_")));
  assert!(!names.iter().any(|name| name.starts_with("scan_")));
  assert_latency_metric_li(&metric_li, "delete");
  let metric = |name: &str| {
    metric_li
      .iter()
      .find(|item| item.0 == name)
      .map(|item| item.1)
      .unwrap_or_else(|| panic!("缺少指标 {name}"))
  };
  // 64 keys / batch 8 = 8 个请求：kv 吞吐 / batch 吞吐 == 8。
  let kv_per_batch = metric("delete_kv_s") / metric("delete_batch_s");
  assert!(
    (kv_per_batch - 8.0).abs() < 1e-6,
    "delete kv/batch = {kv_per_batch}"
  );
}

/// `compact-only`：load-only 装载后，read-only-existing 对同一 store 逐分区
/// 调用 `compact_if_needed`，输出前后字节与回收率；不经过并发 compact lane。
#[test]
fn compact_only_reports_before_after_bytes() {
  let store_dir = env::temp_dir().join(format!("fkv_bench_compact_only_{}_{}", id(), now_secs()));
  let args = BenchArgs {
    rows: 64,
    value_size: 16,
    batch_size: 8,
    workload: Workload::CompactOnly,
    mix: WriteMix::default(),
    plan: ReadPlan {
      partitions: 1,
      read_per_part: 1,
    },
    test_secs: 0.0,
    write_batchs: 0,
    scan_bytes: 0,
    ttl_ms: 0,
  };
  let load_li = bench_workload_ext(
    args,
    None,
    RunControl {
      mode: RunMode::LoadOnly,
      store_dir: Some(store_dir.clone()),
      resource: true,
      ..RunControl::default()
    },
  );
  assert!(store_dir.is_dir(), "load-only 必须保留 store 目录");
  assert!(
    load_li
      .iter()
      .any(|(name, value, ..)| name == "store_bytes" && *value > 0.0),
    "load-only 应写出非空 store"
  );
  let metric_li = bench_workload_ext(
    args,
    None,
    RunControl {
      mode: RunMode::ReadOnlyExisting,
      store_dir: Some(store_dir.clone()),
      resource: true,
      ..RunControl::default()
    },
  );
  let names = metric_name_li(&metric_li);
  for name in [
    "compact_total_us",
    "compact_file_num",
    "store_bytes_before_compact",
    "store_bytes_after_compact",
    "compact_reclaim_ratio",
  ] {
    assert!(names.contains(&name), "缺少指标 {name}: {names:?}");
  }
  let metric = |name: &str| {
    metric_li
      .iter()
      .find(|item| item.0 == name)
      .map(|item| item.1)
      .unwrap_or_else(|| panic!("缺少指标 {name}"))
  };
  assert!(metric("store_bytes_before_compact") > 0.0);
  assert!(metric("store_bytes_after_compact") > 0.0);
  assert!(metric("compact_total_us") >= 0.0);
  assert!(metric("compact_reclaim_ratio").is_finite());
  assert!(store_dir.is_dir(), "read-only-existing 不允许删 store 目录");

  let _ = fs::remove_dir_all(&store_dir);
  let state_path = store_dir.with_file_name(format!(
    "{}.state.json",
    store_dir.file_name().unwrap().to_string_lossy()
  ));
  let _ = fs::remove_file(state_path);
}
