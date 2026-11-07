//! io-05 验收：长耗时维护请求与大值读写任务化后，事件循环不再被内联执行
//! 卡死——维护 / 大值操作执行期间，其它分区的读写请求仍能出队、执行、回执。
//!
//! 判定方式不用墙钟阈值，而用「完成顺序」：慢操作还在飞（完成标志未置位 /
//! 写门在跑数仍为 1）时，另一分区的读写必须已完成。内联执行（旧路径）时
//! 事件循环卡在慢操作的 await 栈里，读写只能等它结束后才被出队，这些断言
//! 必然失败。
use std::{
  hint::spin_loop,
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  thread,
  time::Duration,
};

use compio::{runtime::spawn, time::sleep};
use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_store::AllocMode;
use fkv_testkit::TempRoot;
use fkv_types::{StoreMode, TableIdent};

/// 慢维护操作的数据规模：350 entry / 批（低于写路径批量字节上限）× 20 轮，
/// 512B 页下约 7000 页全部落在 file 0；`Compact{file_id: 0}` 要整文件重写，
/// 耗时数毫秒且只随数据量增长，是稳定的「慢维护」载体（`FileGc` 在没有
/// 垃圾数据时几乎空转，窗口不稳定）。注意本配置下单表写入上限约 26 轮。
const PAGES_PER_FILE_SHIFT: u8 = 18;
const GC_ROUNDS: usize = 20;
const GC_BATCH: usize = 350;

/// 大值测试的 value：256KB（64 段 × 4KB），超过 `large_value_threshold`，
/// 且编码后的段 id 列表能放进一页（更大的值会超页容量被拒）。
const LARGE_VALUE_LEN: usize = 256 * 1024;
/// fixed buffer 容量：大于单个大值，保证大值读写走 fixed 直读 / 直写路径。
const FIXED_BUFFER_SIZE: usize = 1024 * 1024;

fn config(root: PathBuf) -> ShardConfig {
  ShardConfig {
    alloc_mode: AllocMode::Append,
    shard_id: 0,
    root,
    page_size: 512,
    restart_interval: 16,
    index_restart_interval: 16,
    pages_per_file_shift: PAGES_PER_FILE_SHIFT,
    segment_size: 4096,
    segments_per_file_shift: 2,
    large_value_threshold: 1024,
    fixed_buffer_size: FIXED_BUFFER_SIZE,
    queue_size: 256,
    auto_reopen_retry_times: 0,
    auto_reopen_pending_time_us: 0,
    oom_retry_times: 0,
    compression_level: None,
    verify_checksum: true,
    overflow_pointers: fkv_page::DEFAULT_OVERFLOW_POINTERS,
    file_amplify_factor: 0,
    segment_file_amplify_factor: 0,
    manifest_limit: 8 * 1024 * 1024,
    max_write_batch_pages: 32,
    max_inflight_write: 32 * 1024,
    fd_limit: 10_000,
    index_cache_bytes: fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize,
    index_cache_total_bytes: fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize,
    root_meta_cache_bytes: 0,
    io_queue_size: 4096,
    backend: Backend::default(),
    mode: StoreMode::Local,
    standby: None,
    max_write_concurrency: 0,
    // 关掉时间片 / 让出预算，避免调度时序干扰并发窗口判定。
    max_processing_time_us: 0,
    yield_budget_us: 0,
  }
}

async fn attach(shard: &Shard) {
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
}

/// 给表铺 `rounds × batch` 条 entry（512B 页下约一 entry 一页）。
async fn fill_table(shard: &Shard, tbl: &TableIdent, rounds: usize, batch: usize) {
  for round in 0..rounds {
    let batch_li = (0..batch)
      .map(|index| {
        let key = format!("key-{:08}", round * batch + index);
        BatchEntry::upsert(key.into_bytes(), vec![b'v'; 300], 1)
      })
      .collect();
    shard
      .request(Request::Put {
        tbl: tbl.clone(),
        batch_li,
        term: Some(1),
      })
      .await
      .unwrap_or_else(|err| panic!("fill_table round {round} failed: {err:?}"));
  }
}

/// 自旋等写门出现 `expect_running` 条在跑写（维护 / 大值写任务已派发）。
/// 内联执行（旧路径）时写门同样计数，但循环会卡到操作完成才回来；这里只
/// 用它确认「慢操作确实在飞」，真正的判定在完成顺序断言上。
async fn wait_write_running(shard: &Shard, expect_running: u64) {
  for _ in 0..10_000 {
    if shard.metrics_snapshot().write_running >= expect_running {
      return;
    }
    sleep(Duration::from_micros(200)).await;
  }
  panic!("写门在跑数始终未达到 {expect_running}");
}

/// 自旋等写门清零：回执先于 Store 归还发出，采样瞬间可能仍为 1。
async fn wait_write_running_zero(shard: &Shard) {
  for _ in 0..10_000 {
    if shard.metrics_snapshot().write_running == 0 {
      return;
    }
    sleep(Duration::from_micros(200)).await;
  }
  panic!("写门在跑数始终未归零");
}

/// Compact 执行期间：另一分区的读与写都完成，且完成时 Compact 仍在飞。
#[compio::test]
async fn compact_running_foreground_read_write_proceed() {
  let root = TempRoot::new("io05-compact");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  attach(&shard).await;
  let gc_tbl = TableIdent::new("compact_tbl", 1).unwrap();
  fill_table(&shard, &gc_tbl, GC_ROUNDS, GC_BATCH).await;
  // 读 / 写用另外两个分区：跨分区不受 Compact 独占 Store 影响。
  let get_tbl = TableIdent::new("get_tbl", 1).unwrap();
  let put_tbl = TableIdent::new("put_tbl", 1).unwrap();
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

  let compact_done = Arc::new(AtomicBool::new(false));
  let compact_req = {
    let shard_sender = shard.sender();
    let tbl = gc_tbl.clone();
    let done = compact_done.clone();
    spawn(async move {
      let result = shard_sender
        .request(Request::Compact {
          tbl,
          file_id: 0,
          pages_per_file_shift: PAGES_PER_FILE_SHIFT,
        })
        .await;
      done.store(true, Ordering::SeqCst);
      result
    })
  };
  wait_write_running(&shard, 1).await;

  // Compact 仍在飞时，另一分区的 Get 必须完成（旧内联路径会先跑完 Compact）。
  let got = shard
    .request(Request::Get {
      tbl: get_tbl.clone(),
      key: b"k".to_vec(),
    })
    .await
    .unwrap();
  assert!(
    matches!(&got, Response::Get(Some(value)) if value.as_slice() == b"v"),
    "unexpected get response: {got:?}"
  );
  assert!(
    !compact_done.load(Ordering::SeqCst),
    "Get 完成时 Compact 仍在执行"
  );
  // 新 Put 同样能继续出队执行（维护任务不占住事件循环）。
  shard
    .request(Request::Put {
      tbl: put_tbl.clone(),
      batch_li: vec![BatchEntry::upsert(b"k2".to_vec(), b"v2".to_vec(), 2)],
      term: Some(1),
    })
    .await
    .unwrap();
  assert!(
    !compact_done.load(Ordering::SeqCst),
    "Put 完成时 Compact 仍在执行"
  );

  compact_req.await.unwrap().unwrap();
  assert!(compact_done.load(Ordering::SeqCst));
  wait_write_running_zero(&shard).await;
  shard.shutdown().await.unwrap();
}

/// 大值读提交为独立读任务：执行期间读窗口照常准入其它分区读；两条并发
/// GetLarge 按单飞排队，结果互不串值。
#[compio::test]
async fn get_large_runs_as_read_task_foreground_proceeds() {
  let root = TempRoot::new("io05-getlarge");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  attach(&shard).await;
  let large_tbl = TableIdent::new("large_tbl", 1).unwrap();
  let get_tbl = TableIdent::new("get_tbl", 1).unwrap();
  shard
    .request(Request::Put {
      tbl: get_tbl.clone(),
      batch_li: vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      term: Some(1),
    })
    .await
    .unwrap();
  shard
    .request(Request::PutLarge {
      tbl: large_tbl.clone(),
      batch_li: vec![BatchEntry::upsert(
        b"big".to_vec(),
        vec![b'L'; LARGE_VALUE_LEN],
        1,
      )],
      term: Some(1),
    })
    .await
    .unwrap();

  // 独立线程高频采样在读 gauge：GetLarge 必须以读任务形态在飞。内联执行
  // （旧路径）时 `read_in_flight` 恒为 0，采样必然落空。
  let metrics = shard.metrics().clone();
  let saw_read_task = Arc::new(AtomicBool::new(false));
  let sampler_started = Arc::new(AtomicBool::new(false));
  let stop_sampler = Arc::new(AtomicBool::new(false));
  let sampler = {
    let saw = saw_read_task.clone();
    let started = sampler_started.clone();
    let stop = stop_sampler.clone();
    thread::spawn(move || {
      // 先宣告已进入采样循环再开始观测：线程启动本身要上百微秒，可能慢于
      // 大值读任务的整个生命周期。
      started.store(true, Ordering::SeqCst);
      while !stop.load(Ordering::Relaxed) {
        if metrics.snapshot().read_in_flight > 0 {
          saw.store(true, Ordering::SeqCst);
        }
        spin_loop();
      }
    })
  };
  while !sampler_started.load(Ordering::SeqCst) {
    spin_loop();
  }

  let first = {
    let sender = shard.sender();
    let tbl = large_tbl.clone();
    spawn(async move {
      sender
        .request(Request::GetLarge {
          tbl,
          key: b"big".to_vec(),
        })
        .await
    })
  };
  let second = {
    let sender = shard.sender();
    let tbl = large_tbl.clone();
    spawn(async move {
      sender
        .request(Request::GetLarge {
          tbl,
          key: b"big".to_vec(),
        })
        .await
    })
  };
  // 判定窗口只覆盖两条 GetLarge 的生命周期：期间唯一可能出现在飞的读就是
  // GetLarge 自己（采样在线程里自旋，µs 级窗口也必然采到）。内联路径下
  // `read_in_flight` 恒为 0，此断言必然失败。
  for handle in [first, second] {
    match handle.await.unwrap().unwrap() {
      Response::GetLarge(Some(large)) => {
        assert_eq!(large.value.len(), LARGE_VALUE_LEN);
        assert!(large.value.iter().all(|byte| *byte == b'L'));
      }
      other => panic!("unexpected get_large response: {other:?}"),
    }
  }
  stop_sampler.store(true, Ordering::Relaxed);
  sampler.join().unwrap();
  assert!(
    saw_read_task.load(Ordering::SeqCst),
    "GetLarge 必须以独立读任务在飞（read_in_flight 从未非零）"
  );
  // 大值读之后另一分区的 Get 仍完成且拿到正确值。
  let got = shard
    .request(Request::Get {
      tbl: get_tbl.clone(),
      key: b"k".to_vec(),
    })
    .await
    .unwrap();
  assert!(
    matches!(&got, Response::Get(Some(value)) if value.as_slice() == b"v"),
    "unexpected get response: {got:?}"
  );
  shard.shutdown().await.unwrap();
}

/// 大值写提交为独立写任务：执行期间另一分区读完成；任务归还 fixed buffer
/// 后，后续大值读写仍走通（值可回读）。
#[compio::test]
async fn put_large_runs_as_write_task_foreground_proceeds() {
  let root = TempRoot::new("io05-putlarge");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  attach(&shard).await;
  let large_tbl = TableIdent::new("large_tbl", 1).unwrap();
  let get_tbl = TableIdent::new("get_tbl", 1).unwrap();
  shard
    .request(Request::Put {
      tbl: get_tbl.clone(),
      batch_li: vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      term: Some(1),
    })
    .await
    .unwrap();

  let put_done = Arc::new(AtomicBool::new(false));
  let put_req = {
    let sender = shard.sender();
    let tbl = large_tbl.clone();
    let done = put_done.clone();
    // 4 个不同 key 的大值（共 1MB 段写）：足够跨多个 IO，保证读写断言窗口。
    let batch_li = (0..4u32)
      .map(|index| BatchEntry::upsert(index.to_be_bytes().to_vec(), vec![b'P'; LARGE_VALUE_LEN], 1))
      .collect();
    spawn(async move {
      let result = sender
        .request(Request::PutLarge {
          tbl,
          batch_li,
          term: Some(1),
        })
        .await;
      done.store(true, Ordering::SeqCst);
      result
    })
  };
  wait_write_running(&shard, 1).await;

  let got = shard
    .request(Request::Get {
      tbl: get_tbl.clone(),
      key: b"k".to_vec(),
    })
    .await
    .unwrap();
  assert!(
    matches!(&got, Response::Get(Some(value)) if value.as_slice() == b"v"),
    "unexpected get response: {got:?}"
  );
  assert!(
    !put_done.load(Ordering::SeqCst),
    "Get 完成时 PutLarge 仍在执行"
  );

  put_req.await.unwrap().unwrap();
  // 任务归还 fixed buffer 后大值读仍可回读（buffer 往返无损）。
  match shard
    .request(Request::GetLarge {
      tbl: large_tbl.clone(),
      key: 1u32.to_be_bytes().to_vec(),
    })
    .await
    .unwrap()
  {
    Response::GetLarge(Some(large)) => {
      assert_eq!(large.value.len(), LARGE_VALUE_LEN);
      assert!(large.value.iter().all(|byte| *byte == b'P'));
    }
    other => panic!("unexpected get_large response: {other:?}"),
  }
  // 再写一条大值：fixed buffer 归还后写路径可继续借用。
  shard
    .request(Request::PutLarge {
      tbl: large_tbl.clone(),
      batch_li: vec![BatchEntry::upsert(
        b"again".to_vec(),
        vec![b'Q'; LARGE_VALUE_LEN],
        2,
      )],
      term: Some(1),
    })
    .await
    .unwrap();
  shard.shutdown().await.unwrap();
}

/// Compact 任务在飞时 shutdown 仍能完成（写侧排空语义按任务类型覆盖）。
#[compio::test]
async fn shutdown_drains_running_compact() {
  let root = TempRoot::new("io05-shutdown");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  attach(&shard).await;
  let gc_tbl = TableIdent::new("gc_tbl", 1).unwrap();
  fill_table(&shard, &gc_tbl, GC_ROUNDS, GC_BATCH).await;

  let gc_req = {
    let sender = shard.sender();
    let tbl = gc_tbl.clone();
    spawn(async move {
      sender
        .request(Request::Compact {
          tbl,
          file_id: 0,
          pages_per_file_shift: PAGES_PER_FILE_SHIFT,
        })
        .await
    })
  };
  wait_write_running(&shard, 1).await;
  // shutdown 等在飞写任务（含维护任务）落地后再退出，必须正常返回。
  shard.shutdown().await.unwrap();
  let _ = gc_req.await;
}

/// 维护请求与同分区写仍按 per-partition FIFO：Compact 在飞时提交的同分区 Put
/// 必须排队等它完成后才执行；写门 `running` 语义不因任务化改变。
///
/// 不能用「`write_queue_depth > 0` ⇒ `write_running == 1`」判定：二者是独立
/// gauge，`ShardSnapshot` 由两次 Relaxed load 拼成；且 Compact 结束、Put 仍在
/// 门里等下一轮 drain 时 `depth>0 && running==0` 是合法状态（`WriteGate::finish`
/// 先发布深度、再发布在跑数）。判据改为：
/// - `write_queue_depth > 0`：Put 确实进了写门队列，不要求在跑数同刻为 1；
/// - 轮询到的任一快照 `write_running <= 1`：本场景只有 Compact / Put 两条同
///   分区写，在跑数不可能超过 1（单原子读，不受两次 load 交错影响）；
/// - Put 回执必须成功：写门若把同分区 Put 与 Compact 并发派发，分区 Store 已
///   被 Compact 任务取走，第二次 `stores.take` 必然回 `Busy`。
#[compio::test]
async fn compact_keeps_partition_fifo_with_put() {
  let root = TempRoot::new("io05-fifo");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  attach(&shard).await;
  let tbl = TableIdent::new("fifo_tbl", 1).unwrap();
  fill_table(&shard, &tbl, GC_ROUNDS, GC_BATCH).await;

  let gc_req = {
    let sender = shard.sender();
    let tbl = tbl.clone();
    spawn(async move {
      sender
        .request(Request::Compact {
          tbl,
          file_id: 0,
          pages_per_file_shift: PAGES_PER_FILE_SHIFT,
        })
        .await
    })
  };
  wait_write_running(&shard, 1).await;

  // 同分区 Put 必须先在写门排队，不能与在飞的 Compact 并发；一直观察到 Put
  // 回执为止，覆盖「Put 排队后 Compact 才结束」的整个窗口。
  let put_req = {
    let sender = shard.sender();
    let tbl = tbl.clone();
    spawn(async move {
      sender
        .request(Request::Put {
          tbl,
          batch_li: vec![BatchEntry::upsert(b"after-gc".to_vec(), b"v".to_vec(), 1)],
          term: Some(1),
        })
        .await
    })
  };
  // `write_queue_depth > 0` 只证明 Put 进过写门队列：Compact 结束、Put 仍在
  // 门里等 drain 时 `running` 已归零是合法状态，所以不能要求同刻为 1；串行
  // 另由「在跑数 <= 1」与 Put 回执（并发派发会 `Busy`）保证。
  let mut queued = false;
  for _ in 0..10_000 {
    let snapshot = shard.metrics_snapshot();
    assert!(
      snapshot.write_running <= 1,
      "同分区 Put 与在飞 Compact 并发（写门串行被破坏）"
    );
    queued |= snapshot.write_queue_depth > 0;
    if put_req.is_finished() {
      break;
    }
    sleep(Duration::from_micros(100)).await;
  }
  assert!(queued, "同分区 Put 未被写门排队（per-partition FIFO 失效）");

  put_req.await.unwrap().unwrap();
  gc_req.await.unwrap().unwrap();
  wait_write_running_zero(&shard).await;
  shard.shutdown().await.unwrap();
}
