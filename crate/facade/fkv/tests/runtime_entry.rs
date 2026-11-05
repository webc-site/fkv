//! facade 入口接线：`fkv::{Runtime, RuntimeConfig, Request, Response, StoreMode}`。
//!
//! 覆盖 Local / Standby / Cloud（装配）/ Mem 四条对外可达路径，并验证
//! `Runtime::request` 能用 facade 重导出的 `Request` / `Response` 收发。

use std::{
  thread::scope,
  time::{Duration, Instant},
};

use fkv::{
  BatchEntry, KvError, KvOption, Request, Response, ScanPageOption, StoreMode, TableIdent,
  start_mem_runtime, start_runtime,
};
use fkv_testkit::TempRoot;

const BRANCH: &str = "main";

fn tbl() -> TableIdent {
  TableIdent::new("demo", 1).unwrap()
}

async fn attach_all(runtime: &fkv::Runtime, term: u64) {
  for shard_id in 0..u32::from(runtime.num_threads()) {
    runtime.attach_shard(shard_id, BRANCH, term).await.unwrap();
  }
}

#[test]
fn local_runtime_request_response_roundtrip() {
  let root = TempRoot::new("fkv_facade_local");
  let option = KvOption::default();
  assert_eq!(option.derive_mode(), StoreMode::Local);
  let tbl = tbl();
  pollster::block_on(async {
    let runtime = start_runtime(root.path(), &option).unwrap();
    attach_all(&runtime, 1).await;
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"k").await.unwrap(), Some(b"v".to_vec()));
    // 直接走 facade 暴露的 Request / Response，确认原始请求面也可见。
    let response = runtime
      .request(
        &tbl,
        Request::Get {
          tbl: tbl.clone(),
          key: b"k".to_vec(),
        },
      )
      .await
      .unwrap();
    match response {
      Response::Get(Some(value)) => assert_eq!(value, b"v"),
      other => panic!("unexpected response: {other:?}"),
    }
    runtime.shutdown().await.unwrap();
  });
}

#[test]
fn standby_master_runtime_put_get_across_restart() {
  let root = TempRoot::new("fkv_facade_standby");
  let option = KvOption {
    enable_local_standby: true,
    store_path: vec![root.path().to_string_lossy().into_owned()],
    ..KvOption::default()
  };
  assert_eq!(option.derive_mode(), StoreMode::StandbyMaster);
  let tbl = tbl();
  pollster::block_on(async {
    let runtime = start_runtime(root.path(), &option).unwrap();
    attach_all(&runtime, 1).await;
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"k").await.unwrap(), Some(b"v".to_vec()));
    runtime.shutdown().await.unwrap();

    // 重启后 reopen 必须从本地 manifest 读回（standby 数据面仍是本地存储）。
    let runtime = start_runtime(root.path(), &option).unwrap();
    attach_all(&runtime, 1).await;
    runtime.reopen(&tbl).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"k").await.unwrap(), Some(b"v".to_vec()));
    runtime.shutdown().await.unwrap();
  });
}

#[test]
fn mem_runtime_put_get_without_disk() {
  let root = TempRoot::new("fkv_facade_mem");
  let tbl = tbl();
  pollster::block_on(async {
    let runtime = start_mem_runtime(root.path()).unwrap();
    attach_all(&runtime, 1).await;
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"k").await.unwrap(), Some(b"v".to_vec()));
    runtime.shutdown().await.unwrap();
  });
  assert!(!root.path().join("demo.1").exists());
}

#[test]
fn cloud_runtime_starts_from_kv_option() {
  let root = TempRoot::new("fkv_facade_cloud");
  let option = KvOption {
    cloud_store_path: "fkv-test/prefix".to_owned(),
    cloud_endpoint: "http://127.0.0.1:1".to_owned(),
    ..KvOption::default()
  };
  assert_eq!(option.derive_mode(), StoreMode::Cloud);
  pollster::block_on(async {
    // 只验证 facade 能按 Cloud 装配 S3 worker 与本地 cache；真实读写需外部 S3。
    let runtime = start_runtime(root.path(), &option).unwrap();
    runtime.shutdown().await.unwrap();
  });
}

/// 并发扫描的种子数据量：足够让单条 scan 占住 shard，从而把 64 深度的
/// 入队通道压满。
const SEED_ENTRY_NUM: u32 = 10_000;

fn seed_batch() -> Vec<BatchEntry> {
  (0..SEED_ENTRY_NUM)
    .map(|i| {
      let key = i.to_be_bytes().to_vec();
      BatchEntry::upsert(key.clone(), key, 1)
    })
    .collect()
}

fn scan_req(tbl: &TableIdent) -> Request {
  Request::Scan {
    tbl: tbl.clone(),
    begin: Vec::new(),
    end: Vec::new(),
    max_entries: SEED_ENTRY_NUM as usize,
    option: ScanPageOption::default(),
  }
}

/// 128 个并发 scan（远超 64 深度入队通道），返回每条请求的耗时与结果。
fn concurrent_scan(
  runtime: &fkv::Runtime,
  tbl: &TableIdent,
) -> Vec<(Duration, Result<Response, KvError>)> {
  scope(|scope| {
    let handle_li: Vec<_> = (0..128)
      .map(|_| {
        scope.spawn(move || {
          let started = Instant::now();
          let result = pollster::block_on(runtime.request(tbl, scan_req(tbl)));
          (started.elapsed(), result)
        })
      })
      .collect();
    handle_li
      .into_iter()
      .map(|handle| handle.join().unwrap())
      .collect()
  })
}

/// gap-06：`request_timeout_us` 由 `start_runtime` 接到数据面 sender。
/// 1µs deadline 必然在 shard 开始执行前过期 → `Timeout`，控制面不受影响。
#[test]
fn request_timeout_from_kv_option_applies_to_data_plane() {
  let root = TempRoot::new("fkv_facade_request_timeout");
  let option = KvOption {
    request_timeout_us: 1,
    ..KvOption::default()
  };
  assert_eq!(
    option.request_timeout(),
    Some(Duration::from_micros(1)),
    "Local 显式值必须生效"
  );
  let tbl = tbl();
  let runtime = start_runtime(root.path(), &option).unwrap();
  pollster::block_on(async {
    // attach 是控制面：不套用数据面 deadline，必须拿到回执。
    attach_all(&runtime, 1).await;
    let err = runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      )
      .await
      .unwrap_err();
    assert_eq!(err, KvError::Timeout);
    runtime.shutdown().await.unwrap();
  });
}

/// gap-06：`request_queue_timeout_us` 由 `start_runtime` 接到数据面 sender。
/// 队列饱和时回 `Busy`，等待不早于配置值，也不挂死。
#[test]
fn queue_timeout_from_kv_option_reports_busy() {
  const QUEUE_TIMEOUT_US: u64 = 20_000;
  let root = TempRoot::new("fkv_facade_queue_timeout");
  let option = KvOption {
    request_timeout_us: 5_000_000,
    request_queue_timeout_us: QUEUE_TIMEOUT_US,
    ..KvOption::default()
  };
  let tbl = tbl();
  let runtime = start_runtime(root.path(), &option).unwrap();
  pollster::block_on(async {
    attach_all(&runtime, 1).await;
    runtime.put(&tbl, seed_batch()).await.unwrap();
  });
  let outcome_li = concurrent_scan(&runtime, &tbl);
  pollster::block_on(runtime.shutdown()).unwrap();

  let busy_li: Vec<_> = outcome_li
    .iter()
    .filter(|(_, result)| matches!(result, Err(KvError::Busy)))
    .collect();
  assert!(!busy_li.is_empty(), "队列饱和未回 Busy：{outcome_li:?}");
  for (elapsed, _) in busy_li {
    assert!(
      *elapsed >= Duration::from_micros(QUEUE_TIMEOUT_US / 2),
      "Busy 早于队列上限：{elapsed:?}"
    );
    assert!(*elapsed < Duration::from_secs(2), "Busy 过慢：{elapsed:?}");
  }
  assert!(
    outcome_li.iter().any(|(_, result)| result.is_ok()),
    "有界等待不应让所有请求都失败：{outcome_li:?}"
  );
}

/// gap-06：未配置两个键时保持旧行为（不设 deadline、入队无限等待）：
/// 同样的饱和负载全部成功，不出现 `Busy` / `Timeout`。
#[test]
fn default_kv_option_keeps_unbounded_queue_wait() {
  let option = KvOption::default();
  assert_eq!(option.request_timeout(), None);
  assert_eq!(option.queue_timeout(), None);
  let root = TempRoot::new("fkv_facade_default_timeout");
  let tbl = tbl();
  let runtime = start_runtime(root.path(), &option).unwrap();
  pollster::block_on(async {
    attach_all(&runtime, 1).await;
    runtime.put(&tbl, seed_batch()).await.unwrap();
  });
  let outcome_li = concurrent_scan(&runtime, &tbl);
  pollster::block_on(runtime.shutdown()).unwrap();
  assert!(
    outcome_li.iter().all(|(_, result)| result.is_ok()),
    "默认配置不应引入 Busy / Timeout：{outcome_li:?}"
  );
}
