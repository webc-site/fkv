//! 报告 schema、RSS 与 repeat 聚合测试。

use super::*;

/// 时间戳必须是 Unix **秒**（不是毫秒），且与墙钟同步；
/// C++ `utils::UnixTs<seconds>` 同一口径，跨实现对比才不会差 1000 倍。
#[test]
fn now_secs_is_unix_seconds() {
  let wall = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;
  assert!((now_secs() - wall).abs() <= 2);
}

/// history 口径：旧 perf.json 缺 `test_secs` / `write_batchs` / `scan_bytes`
/// 字段反序列化不破（默认 0）；新文件序列化带这三个字段且可回读。
#[test]
fn perf_data_new_fields_back_compat() {
  let old = r#"{
      "baseline": "",
      "generated_at": 0,
      "workload": "write-read",
      "kv_size": 1024,
      "batch_size": 100,
      "max_key": 10000000,
      "read_per_part": 4,
      "partitions": 1,
      "run_li": []
    }"#;
  let data: PerfData = sonic_rs::from_slice(old.as_bytes()).unwrap();
  assert_eq!(data.test_secs, 0.0);
  assert_eq!(data.write_batchs, 0);
  assert_eq!(data.scan_bytes, 0);
  let data = PerfData {
    test_secs: 60.0,
    write_batchs: 32768,
    scan_bytes: 1 << 20,
    ..PerfData::default()
  };
  let json = sonic_rs::to_string(&data).unwrap();
  assert!(json.contains("test_secs"));
  assert!(json.contains("write_batchs"));
  assert!(json.contains("scan_bytes"));
  let back: PerfData = sonic_rs::from_slice(json.as_bytes()).unwrap();
  assert_eq!(back.test_secs, 60.0);
  assert_eq!(back.write_batchs, 32768);
  assert_eq!(back.scan_bytes, 1 << 20);
}

#[test]
fn parse_vmhwm_reads_peak_rss_field() {
  let status = "Name:\tfkv_bench\nVmPeak:\t 123456 kB\nVmHWM:\t  7890 kB\nVmRSS:\t  4000 kB\n";
  assert_eq!(parse_vmhwm(status), Some(7890));
}

#[test]
fn parse_vmhwm_absent_or_malformed_is_none() {
  assert_eq!(parse_vmhwm("Name:\tfkv_bench\n"), None);
  assert_eq!(parse_vmhwm("VmHWM:\tnot-a-number kB\n"), None);
}

/// `--repeat` 聚合：中位数 + min/max；指标名不一致时退化为第一次结果。
#[test]
fn repeat_aggregates_median_min_max() {
  let name = "put_kv_s".to_owned();
  let run = |value: f64| vec![(name.clone(), value, "kv/s")];
  let out = aggregate_repeat(vec![run(1.0), run(3.0), run(2.0)]);
  assert_eq!(
    out,
    vec![
      (name.clone(), 2.0, "kv/s"),
      ("put_kv_s_min".to_owned(), 1.0, "kv/s"),
      ("put_kv_s_max".to_owned(), 3.0, "kv/s"),
    ]
  );
  // 结构不一致时保留第一次结果。
  let out = aggregate_repeat(vec![run(5.0), vec![("get_kv_s".to_owned(), 1.0, "kv/s")]]);
  assert_eq!(out, run(5.0));
}
