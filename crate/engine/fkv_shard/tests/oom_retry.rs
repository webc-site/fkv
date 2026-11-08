//! OutOfMem 请求重试 + PageMapper 回滚。
use std::{
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  time::{Duration, Instant},
};

use compio::{
  runtime::spawn,
  time::{sleep, timeout},
};
use fkv_api::{Request, Response, ScanPageOption};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_failpoint::{
  fired,
  points::{READ_PAGE_HOLD, READ_SEGMENTS, READ_SEGMENTS_HOLD},
  release, reset, set_spec,
};
use fkv_shard::{Shard, ShardConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn config(root: PathBuf, oom_retry_times: u32) -> ShardConfig {
  let mut cfg = ShardConfig::for_tests(root);
  cfg.queue_size = 16;
  cfg.oom_retry_times = oom_retry_times;
  cfg.page_cache_bytes = 0;
  cfg
}

#[compio::test]
async fn out_of_mem_retries_request_after_rollback() {
  let root = TempRoot::new("oom");
  let shard = Shard::start(config(root.path().to_path_buf(), 1)).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
  let tbl = TableIdent::new("demo", 1).unwrap();

  // 第一次写页注入 OutOfMem；put_at 回滚 mapping，shard 重试后成功。
  set_spec("WritePage@1!err=OutOfMem");
  shard
    .request(Request::Put {
      tbl: tbl.clone(),
      batch_li: vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      term: Some(1),
    })
    .await
    .unwrap();
  reset();

  let response = shard
    .request(Request::Get {
      tbl: tbl.clone(),
      key: b"a".to_vec(),
    })
    .await
    .unwrap();
  match response {
    Response::Get(Some(value)) => assert_eq!(value, b"A"),
    other => panic!("unexpected get response: {other:?}"),
  }
  shard.shutdown().await.unwrap();
}

#[compio::test]
async fn out_of_mem_retries_large_put_after_rollback() {
  let root = TempRoot::new("oom");
  let shard = Shard::start(config(root.path().to_path_buf(), 1)).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
  let tbl = TableIdent::new("demo", 1).unwrap();
  // 超过 large_value_threshold：走 segment + data page 双写路径。
  let value = vec![b'L'; 2048];

  set_spec("WritePage@1!err=OutOfMem");
  shard
    .request(Request::PutLarge {
      tbl: tbl.clone(),
      batch_li: vec![BatchEntry::upsert(b"big".to_vec(), value.clone(), 1)],
      term: Some(1),
    })
    .await
    .unwrap();
  reset();

  let response = shard
    .request(Request::GetLarge {
      tbl: tbl.clone(),
      key: b"big".to_vec(),
    })
    .await
    .unwrap();
  match response {
    Response::GetLarge(Some(large)) => assert_eq!(large.value, value),
    other => panic!("unexpected get_large response: {other:?}"),
  }
  shard.shutdown().await.unwrap();
}

#[compio::test]
async fn out_of_mem_without_retry_fails() {
  let root = TempRoot::new("oom");
  let shard = Shard::start(config(root.path().to_path_buf(), 0)).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
  let tbl = TableIdent::new("demo", 1).unwrap();
  set_spec("WritePage@1!err=OutOfMem");
  let err = shard
    .request(Request::Put {
      tbl: tbl.clone(),
      batch_li: vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      term: Some(1),
    })
    .await
    .unwrap_err();
  reset();
  assert_eq!(err, KvError::OutOfMem);
  shard.shutdown().await.unwrap();
}

/// 自旋等读侧 gauge 达到 `expect`：确认慢扫描已作为读任务在飞。
async fn wait_read_in_flight(shard: &Shard, expect: u64) {
  assert!(
    wait_until(Duration::from_secs(10), || {
      shard.metrics_snapshot().read_in_flight >= expect
    })
    .await,
    "读在飞数始终未达到 {expect}"
  );
}

/// 等注入点真正命中：此时被测读任务已停在注入点（`!err` 已返回 / `!hold`
/// 已闭合门闩），后续断言不再依赖墙钟时序。
async fn wait_failpoint_fired(name: &str) {
  assert!(
    wait_until(Duration::from_secs(10), || fired(name)).await,
    "failpoint {name} 始终未触发"
  );
}

/// 读回执的超时上限：旧实现把重试内联在事件循环上时，探针请求会一直
/// 等不到回执，这里让用例快速失败而不是挂到 nextest 的 120s 超时。
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// io-11：仍有其它读在飞时，GetLarge 的 OutOfMem 重试必须重新入队，不能
/// 在 `finish_read` 内联重跑。
///
/// 时序全部由 `!hold` 门闩驱动，不再假设「睡 500µs 后重试还没跑完」：
/// 1. 慢扫描的第一笔页读停在 `ReadPageHold` 上，确定性地占住读窗口；
/// 2. 再把 `ReadSegments` 配成「首读注入 OutOfMem + 重排读停在
///    `ReadSegmentsHold`」，OOM 只会落到 GetLarge，重排读必然在门闩上；
/// 3. 门闩闭合期间新到的 Get / Put 必须完成，而 GetLarge 不可能完成
///    （旧内联实现会把整个重试跑在事件循环栈上，探针因此超时）；
/// 4. 打开门闩后扫描与重排读一起继续，GetLarge 恰好回执一次正确结果。
#[compio::test]
async fn get_large_oom_retry_requeues_while_other_reads_in_flight() {
  /// 3MB / 4KB 段 = 768 段；encoded 段 id 列表仍能放进一个 4KB 叶子页。
  const LARGE_VALUE_LEN: usize = 3 * 1024 * 1024;
  /// 扫描表：每个 entry 独占一页，保证 GetLarge 首次 OOM 时扫描仍在飞。
  const SCAN_ENTRIES: usize = 1024;
  const SCAN_VALUE_LEN: usize = 3000;

  let root = TempRoot::new("oom");
  let mut shard_config = config(root.path().to_path_buf(), 1);
  // 4KB 页容纳 768 个段 id；一段一文件放大重排读的打开量。
  shard_config.page_size = 4096;
  shard_config.segments_per_file_shift = 0;
  let shard = Shard::start(shard_config).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();

  let large_tbl = TableIdent::new("large", 1).unwrap();
  let scan_tbl = TableIdent::new("scan", 1).unwrap();
  let get_tbl = TableIdent::new("probe_get", 1).unwrap();
  let put_tbl = TableIdent::new("probe_put", 1).unwrap();

  let large_value = vec![b'L'; LARGE_VALUE_LEN];
  shard
    .request(Request::PutLarge {
      tbl: large_tbl.clone(),
      batch_li: vec![BatchEntry::upsert(b"big".to_vec(), large_value.clone(), 1)],
      term: Some(1),
    })
    .await
    .unwrap();
  for chunk in (0..SCAN_ENTRIES).collect::<Vec<_>>().chunks(64) {
    let batch_li = chunk
      .iter()
      .map(|index| {
        BatchEntry::upsert(
          format!("k{index:04}").into_bytes(),
          vec![b'S'; SCAN_VALUE_LEN],
          1,
        )
      })
      .collect();
    shard
      .request(Request::Put {
        tbl: scan_tbl.clone(),
        batch_li,
        term: Some(1),
      })
      .await
      .unwrap();
  }
  for tbl in [&get_tbl, &put_tbl] {
    shard
      .request(Request::Put {
        tbl: tbl.clone(),
        batch_li: vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
        term: Some(1),
      })
      .await
      .unwrap();
  }

  // 阶段一：慢扫描的第一笔页读停在门闩上，确定性地占住读窗口。
  set_spec(&format!("{READ_PAGE_HOLD}@1!hold"));
  let scan = {
    let sender = shard.sender();
    let tbl = scan_tbl.clone();
    spawn(async move {
      sender
        .request(Request::Scan {
          tbl,
          begin: b"k0000".to_vec(),
          end: b"l".to_vec(),
          max_entries: 0,
          option: ScanPageOption::default(),
        })
        .await
    })
  };
  wait_failpoint_fired(READ_PAGE_HOLD).await;
  wait_read_in_flight(&shard, 1).await;

  // 阶段二：只让 GetLarge 的第一次段读失败；重排读停在段读门闩上。
  set_spec(&format!(
    "{READ_SEGMENTS}@1!err=OutOfMem,{READ_SEGMENTS_HOLD}@1!hold"
  ));
  let large_done = Arc::new(AtomicBool::new(false));
  let large_req = {
    let sender = shard.sender();
    let tbl = large_tbl.clone();
    let done = large_done.clone();
    spawn(async move {
      let result = sender
        .request(Request::GetLarge {
          tbl,
          key: b"big".to_vec(),
        })
        .await;
      done.store(true, Ordering::SeqCst);
      result
    })
  };
  wait_failpoint_fired(READ_SEGMENTS).await;
  // 重排任务已停在门闩上：release 之前 GetLarge 不可能完成。
  wait_failpoint_fired(READ_SEGMENTS_HOLD).await;
  assert!(
    !large_done.load(Ordering::SeqCst),
    "GetLarge 不应在重排读挂起期间完成"
  );

  // 重排被门闩钉住：新到的 Get / Put 必须被出队执行并先于 GetLarge 完成。
  let get_req = {
    let sender = shard.sender();
    let tbl = get_tbl.clone();
    spawn(async move {
      sender
        .request(Request::Get {
          tbl,
          key: b"k".to_vec(),
        })
        .await
    })
  };
  match timeout(PROBE_TIMEOUT, get_req)
    .await
    .expect("新 Get 应在 GetLarge 重排期间完成（旧内联实现会让事件循环停摆）")
    .unwrap()
    .unwrap()
  {
    Response::Get(Some(value)) => assert_eq!(value, b"v"),
    other => panic!("unexpected get response: {other:?}"),
  }
  assert!(
    !large_done.load(Ordering::SeqCst),
    "GetLarge 重排期间新 Get 仍应完成（不得在事件循环内联重试）"
  );
  timeout(
    PROBE_TIMEOUT,
    shard.request(Request::Put {
      tbl: put_tbl.clone(),
      batch_li: vec![BatchEntry::upsert(b"p".to_vec(), b"v2".to_vec(), 2)],
      term: Some(1),
    }),
  )
  .await
  .expect("新 Put 应在 GetLarge 重排期间完成（旧内联实现会让事件循环停摆）")
  .unwrap();
  assert!(
    !large_done.load(Ordering::SeqCst),
    "GetLarge 重排期间新 Put 仍应完成（不得在事件循环内联重试）"
  );

  // 放行扫描与重排读：重排任务必须带着原请求成功，恰好一次正确回执。
  release(READ_SEGMENTS_HOLD);
  release(READ_PAGE_HOLD);
  match large_req.await.unwrap().unwrap() {
    Response::GetLarge(Some(value)) => assert_eq!(value.value, large_value),
    other => panic!("unexpected get_large response: {other:?}"),
  }
  assert!(large_done.load(Ordering::SeqCst));
  reset();

  match scan.await.unwrap().unwrap() {
    Response::Scan(reply) => assert!(!reply.has_remaining, "扫描结果不完整"),
    other => panic!("unexpected scan response: {other:?}"),
  }
  shard.shutdown().await.unwrap();
}

/// 等写门真正取到一条写在跑（`finish_read_li` 已在 drain 在飞读）。
async fn wait_write_running(shard: &Shard) {
  assert!(
    wait_until(Duration::from_secs(10), || {
      shard.metrics_snapshot().write_running >= 1
    })
    .await,
    "写门始终未进入在跑状态"
  );
}

/// io-27 用例的控制写：两者都在 `run_gated_write` 里先走全 shard 屏障，
/// 随后 `store_mut` 独占目标分区。
#[derive(Clone, Copy)]
enum BarrierWrite {
  Drop,
  DeleteBranch,
}

impl BarrierWrite {
  fn request(self, tbl: &TableIdent) -> Request {
    match self {
      Self::Drop => Request::Drop { tbl: tbl.clone() },
      // 非活跃且非 main 的 branch：`delete_branch` 只列一遍 manifest，
      // 但调用前仍会 `store_mut` 独占分区——伪 Busy 的触发点相同。
      Self::DeleteBranch => Request::DeleteBranch {
        tbl: tbl.clone(),
        branch: "stale".to_owned(),
      },
    }
  }

  fn matches(self, result: &Result<Response, KvError>) -> bool {
    matches!(
      (self, result),
      (Self::Drop, Ok(Response::Drop)) | (Self::DeleteBranch, Ok(Response::DeleteBranch(_)))
    )
  }
}

/// io-27：全屏障 `finish_read_li` 只 drain 一次时，屏障内的 GetLarge OOM 重排
/// 会逃逸出屏障，随后 `run_gated_write` 的 `store_mut` 因重排任务仍持有分区
/// `Rc<ShardStore>` 拿到可重试的伪 `Busy`。
///
/// 时序全部由 `!hold` 门闩驱动，不依赖墙钟：
/// 1. `ReadPageHold@1!hold` 把 GetLarge 的首笔页读钉住 → 确定在飞；
/// 2. 同分区控制写到达并被写门取到（`write_running == 1`），
///    `run_gated_write` 进入 `finish_read_li` 的 drain；
/// 3. 放行页读：首个段读命中 `ReadSegments@1!err=OutOfMem`，`finish_read`
///    对 GetLarge 一律 `push_retry`（io-19），重排读停在 `ReadSegmentsHold`
///    上并持有该分区 `Rc`；
/// 4. 修复后的屏障继续等这条重排读；旧实现只 drain 一次便返回，写请求拿到
///    伪 `Busy`；
/// 5. 放行重排读后写请求成功，GetLarge 恰好一次拿到完整值。
async fn barrier_write_after_get_large_oom(write: BarrierWrite) {
  /// 3MB / 4KB 段 = 768 段；`segments_per_file_shift=0` 一段一文件。
  const LARGE_VALUE_LEN: usize = 3 * 1024 * 1024;

  let root = TempRoot::new("oom");
  let mut shard_config = config(root.path().to_path_buf(), 1);
  shard_config.page_size = 4096;
  shard_config.segments_per_file_shift = 0;
  // 预算到点即让读窗口退休：`DeleteBranch` 是低优先请求，读窗口只等高优先
  // 通道，必须靠主循环的低优先通道才能出队（io-19 用例同一做法）。
  shard_config.max_processing_time_us = 1;
  let shard = Shard::start(shard_config).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();

  let tbl = TableIdent::new("barrier", 1).unwrap();
  let large_value = vec![b'L'; LARGE_VALUE_LEN];
  shard
    .request(Request::PutLarge {
      tbl: tbl.clone(),
      batch_li: vec![BatchEntry::upsert(b"big".to_vec(), large_value.clone(), 1)],
      term: Some(1),
    })
    .await
    .unwrap();

  // 三个注入点一次武装：首读停在页读门闩；它的段读注入 OutOfMem；重排读
  // 停在段读门闩（错误注入先于门闩判定，见 `read_segment`）。
  set_spec(&format!(
    "{READ_PAGE_HOLD}@1!hold,{READ_SEGMENTS}@1!err=OutOfMem,{READ_SEGMENTS_HOLD}@1!hold"
  ));
  let large_req = {
    let sender = shard.sender();
    let tbl = tbl.clone();
    spawn(async move {
      sender
        .request(Request::GetLarge {
          tbl,
          key: b"big".to_vec(),
        })
        .await
    })
  };
  // 首读停在页读门闩上：GetLarge 确定在飞，屏障 drain 必然等它。
  wait_failpoint_fired(READ_PAGE_HOLD).await;

  let write_req = {
    let sender = shard.sender();
    let req = write.request(&tbl);
    spawn(async move { sender.request(req).await })
  };
  // 写门已取到控制写：`run_gated_write` 已进入 `finish_read_li` 的 drain。
  wait_write_running(&shard).await;

  // 放行首读：段读 OutOfMem → `finish_read` 重排；重排读停在段读门闩上，
  // 持有分区 `Rc`。屏障必须继续等它，而不是立即返回。
  release(READ_PAGE_HOLD);
  wait_failpoint_fired(READ_SEGMENTS_HOLD).await;

  // 放行重排读后写请求才能继续（旧实现此时已按 Busy 回执）。
  release(READ_SEGMENTS_HOLD);
  let write_result = timeout(PROBE_TIMEOUT, write_req)
    .await
    .expect("屏障写请求应在重排读完成后回执")
    .unwrap();
  assert!(
    write.matches(&write_result),
    "GetLarge OOM 重排逃逸出屏障，写请求拿到伪 Busy：{write_result:?}"
  );

  match timeout(PROBE_TIMEOUT, large_req)
    .await
    .expect("GetLarge 应在重排读完成后回执")
    .unwrap()
    .unwrap()
  {
    Response::GetLarge(Some(value)) => {
      assert_eq!(value.value.len(), LARGE_VALUE_LEN);
      assert!(value.value.iter().all(|byte| *byte == b'L'));
    }
    other => panic!("unexpected get_large response: {other:?}"),
  }
  reset();
  shard.shutdown().await.unwrap();
}

/// io-27：`Drop` 的屏障必须等完 GetLarge 的 OOM 重排读。
#[compio::test]
async fn get_large_oom_requeue_keeps_barrier_until_drop() {
  barrier_write_after_get_large_oom(BarrierWrite::Drop).await;
}

/// io-27：`DeleteBranch` 与 `Drop` 同属 `run_gated_write`，同样不得拿到伪 Busy。
#[compio::test]
async fn get_large_oom_requeue_keeps_barrier_until_delete_branch() {
  barrier_write_after_get_large_oom(BarrierWrite::DeleteBranch).await;
}

/// 等条件成立；超时返回 `false`（由调用方给出场景化断言）。
async fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
  let started = Instant::now();
  while !cond() {
    if started.elapsed() >= timeout {
      return false;
    }
    sleep(Duration::from_micros(200)).await;
  }
  true
}

/// io-19：`GetLarge` 是唯一在飞读时，OutOfMem 重试同样不能内联跑完整大值读。
///
/// 首个段读注入 OutOfMem 后，重排任务按 `large=false` 重排并停在
/// `ReadSegmentsHold` 门闩上；门闩闭合期间：
/// - 新到的 Put 必须被出队执行，且先于 GetLarge 完成；
/// - 低优先 FileGc 必须被主循环出队执行，且先于 GetLarge 完成；
/// - GetLarge 不可能完成（release 之前重排读一直停在门闩上）。
///
/// 打开门闩后 GetLarge 恰好一次正确回执。旧实现在 `flight.is_empty()` 分支
/// 内联 `handle`，整个重试期间事件循环停摆，探针请求会超时。
#[compio::test]
async fn get_large_oom_retry_requeues_when_sole_read_in_flight() {
  /// 3MB / 4KB 段 = 768 段；`segments_per_file_shift=0` 一段一文件。
  const LARGE_VALUE_LEN: usize = 3 * 1024 * 1024;

  let root = TempRoot::new("oom");
  let mut shard_config = config(root.path().to_path_buf(), 1);
  shard_config.page_size = 4096;
  shard_config.segments_per_file_shift = 0;
  // 预算到点即让出一条低优先：重试期间 FileGc 能被主循环出队。
  shard_config.max_processing_time_us = 1;
  let shard = Shard::start(shard_config).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();

  let large_tbl = TableIdent::new("large", 1).unwrap();
  let put_tbl = TableIdent::new("probe_put", 2).unwrap();
  let get_tbl = TableIdent::new("probe_get", 3).unwrap();
  let gc_tbl = TableIdent::new("probe_gc", 4).unwrap();

  let large_value = vec![b'L'; LARGE_VALUE_LEN];
  shard
    .request(Request::PutLarge {
      tbl: large_tbl.clone(),
      batch_li: vec![BatchEntry::upsert(b"big".to_vec(), large_value.clone(), 1)],
      term: Some(1),
    })
    .await
    .unwrap();
  for tbl in [&put_tbl, &get_tbl, &gc_tbl] {
    shard
      .request(Request::Put {
        tbl: tbl.clone(),
        batch_li: vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
        term: Some(1),
      })
      .await
      .unwrap();
  }

  // 唯一在飞读：GetLarge。首个段读注入 OutOfMem；重排读停在段读门闩上，
  // release 之前不可能完成——不再用 sleep 推断「重试还没跑完」。
  set_spec(&format!(
    "{READ_SEGMENTS}@1!err=OutOfMem,{READ_SEGMENTS_HOLD}@1!hold"
  ));
  let large_done = Arc::new(AtomicBool::new(false));
  let large_req = {
    let sender = shard.sender();
    let tbl = large_tbl.clone();
    let done = large_done.clone();
    spawn(async move {
      let result = sender
        .request(Request::GetLarge {
          tbl,
          key: b"big".to_vec(),
        })
        .await;
      done.store(true, Ordering::SeqCst);
      result
    })
  };
  wait_failpoint_fired(READ_SEGMENTS).await;
  // 重排任务已停在门闩上：GetLarge 在 release 前不可能完成。
  wait_failpoint_fired(READ_SEGMENTS_HOLD).await;
  assert!(
    !large_done.load(Ordering::SeqCst),
    "GetLarge 不应在重排读挂起期间完成"
  );

  // 重试期间新 Put：必须被出队执行并先于 GetLarge 完成。
  let put_req = {
    let sender = shard.sender();
    let tbl = put_tbl.clone();
    spawn(async move {
      sender
        .request(Request::Put {
          tbl,
          batch_li: vec![BatchEntry::upsert(b"p".to_vec(), b"v2".to_vec(), 2)],
          term: Some(1),
        })
        .await
    })
  };
  match timeout(PROBE_TIMEOUT, put_req)
    .await
    .expect("新 Put 应在 GetLarge 重排期间完成（旧内联实现会让事件循环停摆）")
    .unwrap()
    .unwrap()
  {
    Response::Put => {}
    other => panic!("unexpected put response: {other:?}"),
  }
  assert!(
    !large_done.load(Ordering::SeqCst),
    "GetLarge OOM 重试期间新 Put 仍应完成（不得在事件循环内联重试）"
  );

  // 低优先 FileGc：入队后由一条高优先读唤醒读窗口，主循环按预算先出一条
  // 低优先；FileGc 走独立分区 Store，不必等 GetLarge 重排任务结束。
  let gc_done = Arc::new(AtomicBool::new(false));
  let gc_req = {
    let sender = shard.sender();
    let tbl = gc_tbl.clone();
    let done = gc_done.clone();
    spawn(async move {
      let result = sender.request(Request::FileGc { tbl }).await;
      done.store(true, Ordering::SeqCst);
      result
    })
  };
  assert!(
    wait_until(Duration::from_secs(5), || {
      gc_done.load(Ordering::SeqCst) || shard.metrics_snapshot().low_pq_depth > 0
    })
    .await,
    "FileGc 既未完成也未入队（场景无效）：{:?}",
    shard.metrics_snapshot()
  );
  let probe = {
    let sender = shard.sender();
    let tbl = get_tbl.clone();
    spawn(async move {
      sender
        .request(Request::Get {
          tbl,
          key: b"k".to_vec(),
        })
        .await
    })
  };
  assert!(
    wait_until(Duration::from_secs(5), || gc_done.load(Ordering::SeqCst)).await,
    "GetLarge 重试期间低优先 FileGc 没有被出队执行"
  );
  assert!(
    !large_done.load(Ordering::SeqCst),
    "FileGc 未能在 GetLarge 重试完成前回执（读窗口仍被重试占住）"
  );
  match timeout(PROBE_TIMEOUT, probe)
    .await
    .expect("探针 Get 应在 GetLarge 重排期间完成（旧内联实现会让事件循环停摆）")
    .unwrap()
    .unwrap()
  {
    Response::Get(Some(value)) => assert_eq!(value, b"v"),
    other => panic!("unexpected get response: {other:?}"),
  }
  match gc_req.await.unwrap().unwrap() {
    Response::FileGc(_) => {}
    other => panic!("unexpected file_gc response: {other:?}"),
  }

  // 放行重排读：重排任务必须带着原请求成功，恰好一次正确回执。
  release(READ_SEGMENTS_HOLD);
  match timeout(PROBE_TIMEOUT, large_req)
    .await
    .expect("打开门闩后 GetLarge 重排应完成")
    .unwrap()
    .unwrap()
  {
    Response::GetLarge(Some(value)) => {
      assert_eq!(value.value.len(), LARGE_VALUE_LEN);
      assert!(value.value.iter().all(|byte| *byte == b'L'));
    }
    other => panic!("unexpected get_large response: {other:?}"),
  }
  assert!(large_done.load(Ordering::SeqCst));
  reset();
  shard.shutdown().await.unwrap();
}
