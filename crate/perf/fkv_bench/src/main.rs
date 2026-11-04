use std::{
  env,
  error::Error,
  fs,
  hint::black_box,
  path::{Path, PathBuf},
  process::{Command as ProcessCommand, id},
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use clap_args::{ArgAction, arg, clap::Command};
use compio::runtime::Runtime;
use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_store::{ScanOptions, Store};
use fkv_types::TableIdent;
use rusty_alloc_default as _;
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize, Serialize)]
struct PerfData {
  baseline: String,
  generated_at: i64,
  runs: Vec<Run>,
}

#[derive(Deserialize, Serialize)]
struct Run {
  commit: String,
  date: i64,
  platform: String,
  #[serde(rename = "impl")]
  impl_name: String,
  metric: String,
  value: f64,
  unit: String,
}

fn main() -> Result<(), Box<dyn Error>> {
  let Some(arg_match) = clap_args::parse!(|cmd: Command| {
    cmd
      .arg(arg!(--history <FILE> "perf.json 历史文件").action(ArgAction::Set))
      .arg(arg!(--commit <COMMIT> "提交号").action(ArgAction::Set))
      .arg(arg!(--baseline <BASELINE> "兼容基线").action(ArgAction::Set))
      .arg(arg!(--"impl" <IMPL> "实现名：rust / cpp").action(ArgAction::Set))
      .arg(arg!(--rows <ROWS> "端到端 benchmark key 数").action(ArgAction::Set))
      .arg(arg!(--"value-size" <BYTES> "value 字节数").action(ArgAction::Set))
      .arg(arg!(--"batch-size" <N> "批量写大小").action(ArgAction::Set))
      .arg(arg!(--root <DIR> "benchmark 数据目录（例如 NVMe 挂载点）").action(ArgAction::Set))
  }) else {
    return Ok(());
  };

  let commit = arg_match
    .get_one::<String>("commit")
    .cloned()
    .or_else(|| env::var("GITHUB_SHA").ok())
    .unwrap_or_else(git_head);
  let baseline = arg_match
    .get_one::<String>("baseline")
    .cloned()
    .unwrap_or_else(read_baseline);
  let history = arg_match.get_one::<String>("history").cloned();
  let impl_name = arg_match
    .get_one::<String>("impl")
    .cloned()
    .unwrap_or_else(|| "rust".to_owned());
  let rows = arg_match
    .get_one::<String>("rows")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(20_000)
    .max(1);
  let value_size = arg_match
    .get_one::<String>("value-size")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(64)
    .max(1);
  let batch_size = arg_match
    .get_one::<String>("batch-size")
    .and_then(|value| value.parse::<usize>().ok())
    .unwrap_or(100)
    .max(1);
  let bench_root = arg_match.get_one::<String>("root").map(PathBuf::from);
  let now = now_secs();
  let mut data = history
    .as_deref()
    .and_then(|path| fs::read(path).ok())
    .and_then(|bytes| sonic_rs::from_slice::<PerfData>(&bytes).ok())
    .unwrap_or_default();

  data.baseline = baseline;
  data.generated_at = now;

  let platform = format!("{}-{}", env::consts::OS, env::consts::ARCH);
  data.runs.push(Run {
    commit: commit.clone(),
    date: now,
    platform: platform.clone(),
    impl_name: impl_name.clone(),
    metric: "gxhash_4096B_gib_s".to_owned(),
    value: bench_gxhash(4096),
    unit: "GiB/s".to_owned(),
  });

  let workload_li = bench_workload(rows, value_size, batch_size, bench_root.as_deref());
  for (metric, value, unit) in workload_li {
    println!("{metric}: {value:.2} {unit}");
    data.runs.push(Run {
      commit: commit.clone(),
      date: now,
      platform: platform.clone(),
      impl_name: impl_name.clone(),
      metric: metric.to_owned(),
      value,
      unit: unit.to_owned(),
    });
  }

  let json = sonic_rs::to_string_pretty(&data)?;
  if let Some(path) = history {
    if let Some(parent) = Path::new(&path).parent() {
      fs::create_dir_all(parent)?;
    }
    fs::write(path, json + "\n")?;
  } else {
    println!("{json}");
  }
  Ok(())
}

fn bench_gxhash(size: usize) -> f64 {
  let data = vec![0u8; size];
  let start = Instant::now();
  let mut count = 0u64;
  let mut acc = 0u64;
  while start.elapsed() < Duration::from_millis(300) {
    acc ^= gxhash::gxhash64(black_box(&data), count as i64);
    count += 1;
  }
  black_box(acc);
  let secs = start.elapsed().as_secs_f64();
  (count as f64 * size as f64) / secs / (1u64 << 30) as f64
}

fn git_head() -> String {
  ProcessCommand::new("git")
    .args(["rev-parse", "HEAD"])
    .output()
    .ok()
    .filter(|out| out.status.success())
    .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    .unwrap_or_else(|| "unknown".to_owned())
}

fn read_baseline() -> String {
  fs::read_to_string("COMPAT_BASELINE")
    .ok()
    .and_then(|text| {
      text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(2).map(str::to_owned))
    })
    .unwrap_or_default()
}

fn now_secs() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_secs() as i64)
    .unwrap_or(0)
}

fn bench_workload(
  rows: usize,
  value_size: usize,
  batch_size: usize,
  base_dir: Option<&Path>,
) -> Vec<(&'static str, f64, &'static str)> {
  let runtime = Runtime::new().unwrap();
  runtime.block_on(async move {
    let root = bench_root(base_dir);
    let tbl = TableIdent::new("bench", 1).unwrap();
    let mut store = Store::new(CompioStore::new(root.clone(), 4096, 18), 4096, 16);

    let value = vec![0x5au8; value_size];
    let batch_li: Vec<Vec<BatchEntry>> = (0..rows)
      .collect::<Vec<_>>()
      .chunks(batch_size)
      .map(|chunk| {
        chunk
          .iter()
          .map(|index| {
            BatchEntry::upsert(format!("key:{index:010}").into_bytes(), value.clone(), 1)
          })
          .collect()
      })
      .collect();
    let key_li: Vec<Vec<u8>> = (0..rows)
      .map(|index| format!("key:{index:010}").into_bytes())
      .collect();

    let mut result_li = Vec::new();
    let started = Instant::now();
    for batch in &batch_li {
      store.put(&tbl, batch).await.unwrap();
    }
    let elapsed = started.elapsed().as_secs_f64();
    result_li.push(("put_kv_s", rows as f64 / elapsed, "kv/s"));
    result_li.push(("put_batch_s", batch_li.len() as f64 / elapsed, "batch/s"));

    let started = Instant::now();
    for key in &key_li {
      let got = store.get(&tbl, key).await.unwrap().unwrap();
      assert_eq!(got.len(), value_size);
    }
    let elapsed = started.elapsed().as_secs_f64();
    result_li.push(("get_kv_s", rows as f64 / elapsed, "kv/s"));

    let started = Instant::now();
    let entry_li = store.scan(&tbl, b"", b"", 0).await.unwrap();
    let elapsed = started.elapsed().as_secs_f64();
    assert_eq!(entry_li.len(), rows);
    result_li.push(("scan_kv_s", rows as f64 / elapsed, "kv/s"));

    let started = Instant::now();
    let mut begin = Vec::new();
    let mut begin_inclusive = true;
    let mut page_num = 0usize;
    let mut scanned = 0usize;
    loop {
      let page = store
        .scan_page(
          &tbl,
          &begin,
          b"",
          ScanOptions {
            begin_inclusive,
            ..ScanOptions::default()
          },
          256,
          0,
        )
        .await
        .unwrap();
      scanned += page.entry_li.len();
      page_num += 1;
      if !page.has_remaining {
        break;
      }
      begin = page
        .entry_li
        .last()
        .map(|entry| entry.0.clone())
        .unwrap_or_default();
      begin_inclusive = false;
    }
    let elapsed = started.elapsed().as_secs_f64();
    assert_eq!(scanned, rows);
    result_li.push(("scan_page_s", page_num as f64 / elapsed, "page/s"));

    let _ = fs::remove_dir_all(root);
    result_li
  })
}

fn bench_root(base_dir: Option<&Path>) -> PathBuf {
  let base = base_dir.map_or_else(env::temp_dir, Path::to_path_buf);
  let root = base.join(format!("fkv_bench_{}_{}", id(), now_secs()));
  let _ = fs::remove_dir_all(&root);
  root
}

#[cfg(test)]
mod tests {
  use super::bench_workload;

  #[test]
  fn workload_reports_all_metrics() {
    let metric_li = bench_workload(64, 32, 16, None);
    assert_eq!(metric_li.len(), 5);
    let name_li: Vec<&str> = metric_li.iter().map(|(name, ..)| *name).collect();
    assert_eq!(
      name_li,
      vec![
        "put_kv_s",
        "put_batch_s",
        "get_kv_s",
        "scan_kv_s",
        "scan_page_s"
      ]
    );
    for (name, value, unit) in metric_li {
      assert!(value > 0.0, "{name} = {value}");
      assert!(!unit.is_empty());
    }
  }
}
