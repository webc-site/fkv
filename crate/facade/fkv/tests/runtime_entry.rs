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

/// 饱和负载的并发 scan 数：必须同时超过「64 深度入队通道 + shard 读窗口
/// 可吸收的请求数」（io-04 后读窗口最多 128 条在飞 + 32 条暂存），否则所有
/// 请求都能被 shard 收下，`queue_timeout` 不会出现 `Busy`。
///
/// 实测（空载单跑）：256 条里约 20 条回 `Busy`，余量偏薄；整机有负载时
/// `Busy` 明显增多（全量并行跑约 180 条）。若要给 `Busy` 留更大余量可上调本值
/// （实测 384 条约 142 条 `Busy`），代价是下面两个饱和用例各慢几秒。
const CONCURRENT_SCAN_NUM: usize = 256;

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

/// `CONCURRENT_SCAN_NUM` 个并发 scan（远超 64 深度入队通道 + 读窗口吸收量），
/// 返回每条请求的耗时与结果。
fn concurrent_scan(
  runtime: &fkv::Runtime,
  tbl: &TableIdent,
) -> Vec<(Duration, Result<Response, KvError>)> {
  scope(|scope| {
    let handle_li: Vec<_> = (0..CONCURRENT_SCAN_NUM)
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

/// Busy 判定上限：队列上限（`QUEUE_TIMEOUT_US`）只有 20ms 量级，远早于本
/// 上限就说明失败确实来自队列上限，而不是别的墙钟等待。
const BUSY_LATENCY_LIMIT: Duration = Duration::from_secs(2);

/// 执行侧请求 deadline：1 小时，实际上不可达（挂死由 nextest 120s 的
/// `terminate-after = 1` 兜底）。
///
/// 判定只看「完成计数 + 失败形态」时不能再用 5s 这种墙钟上限：那种写法要求
/// 机器在 5s 内把饱和队列跑完，高负载下必然 flaky（test-12）。把 deadline 取到
/// 不可达，就等于声明「本用例只断言**活性**（请求最终会被服务），不断言延迟」。
const LIVENESS_DEADLINE_US: u64 = 60 * 60 * 1_000_000;

/// gap-06：`request_queue_timeout_us` 由 `start_runtime` 接到数据面 sender。
///
/// 队列饱和（`CONCURRENT_SCAN_NUM` 条并发 scan 超过入队通道 64 + 读窗口 128 +
/// 暂存 32 的吸收量）时，被挡在队外的请求必须在 `QUEUE_TIMEOUT_US` 后拿到
/// `Busy`；断言全部基于**完成计数与失败形态**，与机器负载解耦（test-12）：
///
/// - 入队成功 = 不被队列上限拒绝，shard 最终会把它跑成 `Ok`（执行侧 deadline
///   不可达）→ 至少一条 `Ok`（有界等待不应让所有请求都失败）；
/// - 入队被挡超 `QUEUE_TIMEOUT_US` → `Busy`，且不早于配置值、不晚于
///   [`BUSY_LATENCY_LIMIT`]（失败形态是「有界背压」，不是墙钟超时）；
/// - 两者之外不应出现任何结果（尤其不应出现 `Timeout`）：两键同时配置且队列
///   上限更早时，`queue_wait` 必须取较早的队列上限。
#[test]
fn queue_timeout_from_kv_option_reports_busy() {
  const QUEUE_TIMEOUT_US: u64 = 20_000;
  let root = TempRoot::new("fkv_facade_queue_timeout");
  let option = KvOption {
    request_timeout_us: LIVENESS_DEADLINE_US,
    request_queue_timeout_us: QUEUE_TIMEOUT_US,
    ..KvOption::default()
  };
  assert_eq!(
    option.queue_timeout(),
    Some(Duration::from_micros(QUEUE_TIMEOUT_US))
  );
  let tbl = tbl();
  let runtime = start_runtime(root.path(), &option).unwrap();
  pollster::block_on(async {
    attach_all(&runtime, 1).await;
    runtime.put(&tbl, seed_batch()).await.unwrap();
  });
  let outcome_li = concurrent_scan(&runtime, &tbl);
  pollster::block_on(runtime.shutdown()).unwrap();

  let mut ok_num = 0usize;
  let mut busy_num = 0usize;
  for (elapsed, result) in &outcome_li {
    match result {
      Ok(_) => ok_num += 1,
      Err(KvError::Busy) => {
        busy_num += 1;
        assert!(
          *elapsed >= Duration::from_micros(QUEUE_TIMEOUT_US / 2),
          "Busy 早于队列上限：{elapsed:?}"
        );
        assert!(*elapsed < BUSY_LATENCY_LIMIT, "Busy 过慢：{elapsed:?}");
      }
      other => panic!("队列饱和只应产出 Ok / Busy：{other:?}"),
    }
  }
  assert!(
    busy_num > 0,
    "队列饱和未回 Busy（ok={ok_num}）：{outcome_li:?}"
  );
  assert!(
    ok_num > 0,
    "有界等待不应让所有请求都失败（busy={busy_num}）：{outcome_li:?}"
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
