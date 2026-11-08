//! 1 shard = 1 OS thread + 1 compio Runtime + 1 async request queue。

mod backend;
mod dispatch;
mod event_loop;
mod lifecycle;
mod maintenance;
pub mod metered;
mod metrics;
mod partition;
mod read;
mod reopen;
mod roots;
mod sender;
mod standby;
mod write;
mod write_gate;

pub use backend::{Backend, CloudBackend, StoreBackend, split_cloud_store_path};
pub use fkv_store::AllocMode;
pub use lifecycle::{Shard, ShardConfig, ShardLifecycle};
pub use roots::{RootedConfig, RootedStore};
pub use sender::ShardSender;
pub use write::{merge_put_batches_for_test, poll_ready_once};

/// 单轮从高优先通道收集的读请求上限：只影响收集节奏（避免一条写 / 控制
/// 请求被一整批读挡在通道里），**不是**读侧 in-flight 上限。
///
/// 读窗口满后的准入由 [`MAX_READ_IN_FLIGHT`] 控制：满时只等任意一条读完成，
/// 不再以本值做全量 drain 屏障（io-04）。
pub const MAX_READ_BATCH: usize = 32;

/// 读侧 in-flight 上限：约束同时在跑的读任务数（fd / 读页缓冲上界）。
///
/// 与 [`MAX_READ_BATCH`] 分离：突发读可跨多轮收集持续并发，窗口满时用滑动
/// 窗口等一个空位，而不是把已完成的读也算作名额、再整批 drain。取收集上限的
/// 4 倍：保留上界，又不至于像旧实现那样把高并发读切成 32 条一波的 lockstep
/// （C++ 参考实现读侧无上限）。
///
/// 两条准入路径（读窗口 `read_window` 与主循环批提交 `submit_read_li`）都
/// 遵守本上限：批提交按剩余额度截断，溢出的读进入主循环的 deferred 队列，
/// 由读窗口在下次进入时按到达顺序等空位准入（io-23）。
pub const MAX_READ_IN_FLIGHT: usize = 4 * MAX_READ_BATCH;

/// 同一分区一次 envelope_li commit 合并的最大 Put 请求数。
const MAX_PUT_GROUP_REQUESTS: usize = 64;

/// 一轮最多从高优先通道收集的写请求数，用于形成 envelope_li commit 批次。
const MAX_WRITE_COLLECT: usize = 64;

#[cfg(test)]
mod tests {
  use std::{
    collections::{BTreeMap, VecDeque},
    mem::forget,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
  };

  use compio::time::timeout;
  use crossfire::{
    MAsyncRx,
    mpmc::{Array, bounded_async},
  };
  use fkv_api::{Request, Response};
  use fkv_btree::BatchEntry;
  use fkv_error::{KvError, Result};
  use fkv_metrics::ShardMetric;
  use fkv_task_reopen::AutoReopenOption;
  use fkv_testkit::TempRoot;
  use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, StoreMode, TableIdent};
  use futures::future::{Either, select};
  use futures_channel::oneshot;
  use futures_timer::Delay;

  use crate::{
    MAX_READ_BATCH, MAX_READ_IN_FLIGHT,
    backend::StoreBackend,
    event_loop::fail_stale_pending,
    lifecycle::{DsShardState, TableOpen},
    metered::MeteredIo,
    read::ReadFlight,
    reopen::ReopenQueue,
    sender::{Envelope, Pri, ShardExit, ShardSender},
    standby::StandbyFlight,
    write::{merge_put_batch_li, write_queue_stale},
    write_gate::WriteGate,
  };

  fn envelope(batch_li: Vec<BatchEntry>, term: Option<u64>) -> Envelope {
    let (reply, _rx) = oneshot::channel();
    Envelope {
      req: Request::Put {
        tbl: TableIdent::new("demo", 1).unwrap(),
        batch_li,
        term,
      },
      reply,
      cancel: Arc::new(AtomicBool::new(false)),
      deadline: None,
      reopen_retry: None,
    }
  }

  fn entry(key: &[u8], value: &[u8], timestamp: u64) -> BatchEntry {
    BatchEntry::upsert(key.to_vec(), value.to_vec(), timestamp)
  }

  #[test]
  fn merge_put_batches_keeps_cross_request_timestamp_semantics() {
    let mut envelope_li = vec![
      envelope(
        vec![
          entry(b"a", b"old", 1),
          entry(b"b", b"keep", 2),
          // 单请求内部同 key 也按 timestamp 折叠：ts=5 胜 ts=4。
          entry(b"c", b"first", 5),
          entry(b"c", b"last", 4),
        ],
        None,
      ),
      envelope(
        vec![
          entry(b"a", b"new", 2),
          // 跨请求旧 timestamp 不覆盖新值。
          entry(b"b", b"stale", 1),
          // c 的 timestamp 3 小于请求 1 合并后的 5，跳过。
          entry(b"c", b"stale", 3),
        ],
        None,
      ),
    ];
    let merged: BTreeMap<Vec<u8>, BatchEntry> = merge_put_batch_li(&mut envelope_li)
      .into_iter()
      .map(|entry| (entry.key.clone(), entry))
      .collect();
    assert_eq!(merged[b"a".as_slice()].value, b"new");
    assert_eq!(merged[b"b".as_slice()].value, b"keep");
    assert_eq!(merged[b"c".as_slice()].value, b"first");
  }

  #[test]
  fn merge_put_batches_equal_timestamp_later_request_wins() {
    let mut envelope_li = vec![
      envelope(vec![entry(b"k", b"first", 7)], Some(3)),
      envelope(vec![entry(b"k", b"second", 7)], Some(3)),
    ];
    let merged = merge_put_batch_li(&mut envelope_li);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].value, b"second");
  }

  /// 同请求同 key：timestamp 大者胜，相等时批内后写胜（与 Store::put 折叠一致）。
  #[test]
  fn merge_put_batches_equal_timestamp_later_entry_wins() {
    let mut envelope_li = vec![envelope(
      vec![
        entry(b"k", b"first", 7),
        entry(b"k", b"second", 7),
        entry(b"k", b"stale", 6),
      ],
      None,
    )];
    let merged = merge_put_batch_li(&mut envelope_li);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].timestamp, 7);
    assert_eq!(merged[0].value, b"second");
  }

  /// 请求内代表条必须先结算，再参与跨请求 timestamp 比较。
  ///
  /// 反例：若单趟里边折请求内边比跨请求 ts，后请求的 ts5 会先覆盖老请求的
  /// ts7，再被请求内后写的 ts5 替换，错误得出 ts5。
  #[test]
  fn merge_put_batches_request_inner_rep_before_cross_request_compare() {
    let mut envelope_li = vec![
      envelope(vec![entry(b"k", b"old-req", 7)], None),
      envelope(
        vec![entry(b"k", b"fresh", 9), entry(b"k", b"fresh-last", 5)],
        None,
      ),
    ];
    let merged = merge_put_batch_li(&mut envelope_li);
    assert_eq!(merged.len(), 1);
    // 请求 2 内 ts=9 胜 ts=5；再与请求 1 的 ts=7 比较，ts=9 胜。
    assert_eq!(merged[0].timestamp, 9);
    assert_eq!(merged[0].value, b"fresh");
  }

  /// 组上限场景（64 请求 × 单 key）：输出只剩最后请求的幸存者。
  #[test]
  fn merge_put_batches_large_group_keeps_last_request() {
    let mut envelope_li: Vec<Envelope> = (0..64u64)
      .map(|index| envelope(vec![entry(b"k", &index.to_be_bytes(), index)], None))
      .collect();
    let merged = merge_put_batch_li(&mut envelope_li);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].timestamp, 63);
    assert_eq!(merged[0].value, 63u64.to_be_bytes());
  }

  /// 输出保持 key 升序（旧 BTreeMap 语义），非 Put 请求跳过。
  #[test]
  fn merge_put_batches_outputs_key_order_and_skips_non_put() {
    let mut envelope_li = vec![
      envelope(vec![entry(b"z", b"1", 1), entry(b"a", b"1", 1)], None),
      raw_envelope(),
      envelope(vec![entry(b"m", b"2", 2)], None),
    ];
    let merged = merge_put_batch_li(&mut envelope_li);
    let key_li: Vec<&[u8]> = merged.iter().map(|entry| entry.key.as_slice()).collect();
    assert_eq!(
      key_li,
      vec![b"a".as_slice(), b"m".as_slice(), b"z".as_slice()]
    );
  }

  fn get_request() -> Request {
    Request::Get {
      tbl: TableIdent::new("demo", 1).unwrap(),
      key: b"k".to_vec(),
    }
  }

  fn raw_envelope() -> Envelope {
    let (reply, _rx) = oneshot::channel();
    Envelope {
      req: Request::Shutdown,
      reply,
      cancel: Arc::new(AtomicBool::new(false)),
      deadline: None,
      reopen_retry: None,
    }
  }

  /// 占满容量的高优先通道 + 指定入队等待策略的 sender。
  fn full_sender(queue_timeout: Option<Duration>) -> (ShardSender, MAsyncRx<Array<Envelope>>) {
    let (tx_hi, rx_hi) = bounded_async::<Envelope>(1);
    let (tx_lo, _rx_lo) = bounded_async::<Envelope>(1);
    tx_hi.try_send(raw_envelope()).unwrap();
    let (exit, exit_guard) = ShardExit::new();
    // 握住 guard 不触发退出信号：测试只验证入队等待语义。
    forget(exit_guard);
    let sender = ShardSender {
      tx_hi,
      tx_lo,
      metrics: Arc::new(ShardMetric::new(0)),
      exit,
      request_timeout: None,
      queue_timeout,
    };
    (sender, rx_hi)
  }

  /// 队列满 + 配置 queue_timeout：按时返回 Busy，不静默挂起。
  #[compio::test]
  async fn enqueue_full_returns_busy_after_queue_timeout() {
    let (sender, rx_hi) = full_sender(Some(Duration::from_millis(20)));
    let started = Instant::now();
    let Err(err) = sender.enqueue(get_request(), None).await else {
      panic!("队列满时必须回 Busy");
    };
    assert_eq!(err, KvError::Busy);
    assert!(started.elapsed() >= Duration::from_millis(20));
    drop(rx_hi);
  }

  /// 未配置 queue_timeout：保持旧行为，槽位释放后投递成功。
  #[compio::test]
  async fn enqueue_default_waits_for_free_slot() {
    let (sender, rx_hi) = full_sender(None);
    let pending = Box::pin(sender.enqueue(get_request(), None));
    let Either::Left(((), pending)) = select(Delay::new(Duration::from_millis(30)), pending).await
    else {
      panic!("队列满且未配置 queue_timeout 时不应立即完成");
    };
    // 消费一条，释放槽位。
    rx_hi.try_recv().unwrap();
    let handle = pending.await.unwrap();
    drop(handle);
  }

  /// 队列满 + 已过期 deadline：优先回 Timeout，不被队列阻塞。
  #[compio::test]
  async fn enqueue_full_with_expired_deadline_returns_timeout() {
    let (sender, rx_hi) = full_sender(None);
    let expired = Instant::now() - Duration::from_millis(1);
    let Err(err) = sender.enqueue(get_request(), Some(expired)).await else {
      panic!("已过 deadline 且队列满时必须回 Timeout");
    };
    assert_eq!(err, KvError::Timeout);
    drop(rx_hi);
  }

  /// 每表打开状态：代际 / term / branch 任一变化都必须判为「未就绪」，
  /// 迫使读请求走串行打开路径，而不是拿旧身份直接 spawn。
  #[test]
  fn table_slot_requires_same_epoch_and_identity() {
    let mut open = TableOpen::new();
    let tbl = TableIdent::new("slot", 1).unwrap();
    assert!(!open.slot_ready(&tbl, Some(7), "main"));
    open.mark(&tbl, Some(7), "main");
    assert!(open.slot_ready(&tbl, Some(7), "main"));
    // 换 term / branch（重新 attach）后旧登记失效。
    assert!(!open.slot_ready(&tbl, Some(8), "main"));
    assert!(!open.slot_ready(&tbl, Some(7), "other"));
    // attach / detach 提升代际：全部登记失效，重新打开后恢复。
    open.bump();
    assert!(!open.slot_ready(&tbl, Some(7), "main"));
    open.mark(&tbl, Some(8), "other");
    assert!(open.slot_ready(&tbl, Some(8), "other"));
  }

  /// spawn 的读任务各自持一份 `Rc<Store>` 克隆：在飞期间写路径拿不到独占
  /// （`Rc::get_mut` 失败），`drain` 后引用归一并恢复独占。这是「读并发 +
  /// 写前 drain 屏障」的核心不变量。
  #[compio::test]
  async fn read_flight_holds_store_until_drained() {
    use fkv_io_mem::MemStore;
    use fkv_store::Store;

    let metrics = Arc::new(ShardMetric::new(0));
    let io = MeteredIo::new(
      StoreBackend::Mem(Rc::new(
        MemStore::with_pages_per_file(512, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
          .with_segment_config(4096, 2),
      )),
      metrics.clone(),
    );
    let mut store = Rc::new(Store::new(io, 512, 16));
    assert!(Rc::get_mut(&mut store).is_some());

    let mut flight = ReadFlight::new(metrics.clone(), 0);
    let tbl = TableIdent::new("flight", 1).unwrap();
    for key in [b"a".as_slice(), b"b".as_slice()] {
      let (reply, _rx) = oneshot::channel();
      let envelope = Envelope {
        req: Request::Get {
          tbl: tbl.clone(),
          key: key.to_vec(),
        },
        reply,
        cancel: Arc::new(AtomicBool::new(false)),
        deadline: None,
        reopen_retry: None,
      };
      flight.push(Rc::clone(&store), 0, envelope, Instant::now());
    }
    // 两条读任务 + 主线程各持一份：写路径此时不能独占。
    assert_eq!(Rc::strong_count(&store), 3);
    assert!(Rc::get_mut(&mut store).is_none());
    // 读侧 in-flight gauge 在提交后同步可见（eng-32c）。
    assert_eq!(metrics.snapshot().read_in_flight, 2);
    assert_eq!(metrics.snapshot().read_in_flight_max, 2);

    let done_li = flight.drain().await;
    // 未打开的表读作空：两条任务都完成并带回结果，不丢请求。
    assert_eq!(done_li.len(), 2);
    assert!(
      done_li
        .iter()
        .all(|done| { matches!(&done.result, Ok(Response::Get(None))) })
    );
    assert_eq!(flight.len(), 0);
    assert_eq!(flight.slot_count(), 0, "完成读的 slot 必须立即回收");
    assert_eq!(metrics.snapshot().read_in_flight, 0);
    assert_eq!(Rc::strong_count(&store), 1);
    assert!(Rc::get_mut(&mut store).is_some());
  }

  /// 读侧 in-flight 上限与单轮收集上限分离：`MAX_READ_BATCH` 条只占窗口
  /// 一部分，达到 `MAX_READ_IN_FLIGHT` 才 `is_full`；drain 后 slot 全部回收、
  /// gauge 归零（io-04 / eng-32c）。
  #[compio::test]
  async fn read_flight_full_at_in_flight_limit() {
    use fkv_io_mem::MemStore;
    use fkv_store::Store;

    let metrics = Arc::new(ShardMetric::new(0));
    let io = MeteredIo::new(
      StoreBackend::Mem(Rc::new(
        MemStore::with_pages_per_file(512, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
          .with_segment_config(4096, 2),
      )),
      metrics.clone(),
    );
    let store = Rc::new(Store::new(io, 512, 16));
    let tbl = TableIdent::new("full", 1).unwrap();
    let mut flight = ReadFlight::new(metrics.clone(), 0);
    for index in 0..MAX_READ_BATCH {
      assert!(!flight.is_full(), "第 {index} 条提交前不应达到上限");
      flight.push(
        Rc::clone(&store),
        0,
        req_envelope(read_req(&tbl)),
        Instant::now(),
      );
    }
    // 批收集上限不再兼作 in-flight 上限：窗口仍可继续准入。
    assert!(!flight.is_full(), "MAX_READ_BATCH 不应再截断读并发窗口");
    for index in MAX_READ_BATCH..MAX_READ_IN_FLIGHT {
      assert!(!flight.is_full(), "第 {index} 条提交前不应达到上限");
      flight.push(
        Rc::clone(&store),
        0,
        req_envelope(read_req(&tbl)),
        Instant::now(),
      );
    }
    assert!(flight.is_full());
    assert_eq!(flight.len(), MAX_READ_IN_FLIGHT);
    assert_eq!(flight.slot_count(), MAX_READ_IN_FLIGHT);
    assert_eq!(metrics.snapshot().read_in_flight, MAX_READ_IN_FLIGHT as u64);
    assert_eq!(
      metrics.snapshot().read_in_flight_max,
      MAX_READ_IN_FLIGHT as u64
    );

    let done_li = flight.drain().await;
    assert_eq!(done_li.len(), MAX_READ_IN_FLIGHT);
    assert_eq!(flight.len(), 0);
    assert_eq!(flight.slot_count(), 0);
    assert_eq!(metrics.snapshot().read_in_flight, 0);
  }

  /// 完成即回收 slot：取走一条完成读后其槽位立刻释放，在飞数 / gauge 同步
  /// 下降，不再把已完成的读算作窗口名额（io-04 的滑动窗口前提）。
  #[compio::test]
  async fn read_flight_reclaims_completed_slot_immediately() {
    use fkv_io_mem::MemStore;
    use fkv_store::Store;

    let metrics = Arc::new(ShardMetric::new(0));
    let io = MeteredIo::new(
      StoreBackend::Mem(Rc::new(
        MemStore::with_pages_per_file(512, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
          .with_segment_config(4096, 2),
      )),
      metrics.clone(),
    );
    let store = Rc::new(Store::new(io, 512, 16));
    let tbl = TableIdent::new("reclaim", 1).unwrap();
    let mut flight = ReadFlight::new(metrics.clone(), 0);
    for _ in 0..2 {
      flight.push(
        Rc::clone(&store),
        0,
        req_envelope(read_req(&tbl)),
        Instant::now(),
      );
    }
    assert_eq!(flight.slot_count(), 2);
    let first = flight.next().await.expect("应有一条读完成");
    assert!(matches!(first.result, Ok(Response::Get(None))));
    assert_eq!(flight.len(), 1);
    assert_eq!(flight.slot_count(), 1, "完成读的槽位没有立即回收");
    assert_eq!(metrics.snapshot().read_in_flight, 1);

    let second = flight.next().await.expect("第二条读也应完成");
    assert!(matches!(second.result, Ok(Response::Get(None))));
    assert_eq!(flight.len(), 0);
    assert_eq!(flight.slot_count(), 0);
    assert_eq!(metrics.snapshot().read_in_flight, 0);
  }

  /// io-09：在飞计数按分区维护——`push` +1、完成回收 -1，归零删键；
  /// 排空后两个分区都回到 0。这是写派发只等目标分区的前提。
  #[compio::test]
  async fn read_flight_tracks_in_flight_per_partition() {
    use fkv_io_mem::MemStore;
    use fkv_store::Store;

    let metrics = Arc::new(ShardMetric::new(0));
    let io = MeteredIo::new(
      StoreBackend::Mem(Rc::new(
        MemStore::with_pages_per_file(512, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
          .with_segment_config(4096, 2),
      )),
      metrics.clone(),
    );
    let store = Rc::new(Store::new(io, 512, 16));
    let a = TableIdent::new("part-a", 1).unwrap();
    let b = TableIdent::new("part-b", 2).unwrap();
    let mut flight = ReadFlight::new(metrics, 0);
    for _ in 0..2 {
      flight.push(
        Rc::clone(&store),
        0,
        req_envelope(read_req(&a)),
        Instant::now(),
      );
    }
    flight.push(
      Rc::clone(&store),
      0,
      req_envelope(read_req(&b)),
      Instant::now(),
    );
    assert_eq!(flight.partition_len(&a), 2);
    assert_eq!(flight.partition_len(&b), 1);
    // 任一条完成只回收到它自己的分区；总数从 3 降到 2。
    assert!(flight.next().await.is_some());
    assert_eq!(flight.partition_len(&a) + flight.partition_len(&b), 2);
    // 再补一条 B 后全部排空：两个分区的计数都归零（键被删除，不是残留 0）。
    flight.push(
      Rc::clone(&store),
      0,
      req_envelope(read_req(&b)),
      Instant::now(),
    );
    let done_li = flight.drain().await;
    assert_eq!(done_li.len(), 3);
    assert_eq!(flight.partition_len(&a), 0);
    assert_eq!(flight.partition_len(&b), 0);
  }

  fn req_envelope(req: Request) -> Envelope {
    let (reply, _rx) = oneshot::channel();
    Envelope {
      req,
      reply,
      cancel: Arc::new(AtomicBool::new(false)),
      deadline: None,
      reopen_retry: None,
    }
  }

  /// 保留回执端的 `Envelope`：身份切换清理要断言每条排队请求都被回执。
  fn tracked_envelope(req: Request) -> (Envelope, oneshot::Receiver<Result<Response>>) {
    let (reply, rx) = oneshot::channel();
    (
      Envelope {
        req,
        reply,
        cancel: Arc::new(AtomicBool::new(false)),
        deadline: None,
        reopen_retry: None,
      },
      rx,
    )
  }

  fn assert_not_running(mut rx: oneshot::Receiver<Result<Response>>, path: &str, what: &str) {
    let result = rx
      .try_recv()
      .unwrap_or_else(|_| panic!("{path}: {what} 的回执通道已关闭"));
    assert!(
      matches!(result, Some(Err(KvError::NotRunning))),
      "{path}: {what} 应以 NotRunning 回执"
    );
  }

  fn read_req(tbl: &TableIdent) -> Request {
    Request::Get {
      tbl: tbl.clone(),
      key: b"k".to_vec(),
    }
  }

  /// 外部 `Reopen` 无同表 driver：不建延迟条目，立即交回回执（对齐 C++）。
  #[test]
  fn reopen_queue_external_reopen_does_not_wait_without_driver() {
    let mut queue = ReopenQueue::new(AutoReopenOption::new(2, Duration::from_millis(10)));
    let tbl = TableIdent::new("ext", 1).unwrap();
    let envelope = req_envelope(Request::Reopen {
      tbl: tbl.clone(),
      clean: false,
    });
    assert!(queue.park(envelope).is_some(), "外部 Reopen 不应挂起");
    assert!(queue.next_due().is_none());
    // 空 tag 的 `ReopenArchive` 同理。
    let envelope = req_envelope(Request::ReopenArchive {
      tbl,
      tag: String::new(),
      clean: false,
    });
    assert!(
      queue.park(envelope).is_some(),
      "外部 ReopenArchive 不应挂起"
    );
    assert!(queue.next_due().is_none());
  }

  /// 外部 `Reopen` 可搭已有读 driver 的 waiter，且不消耗读请求的预算。
  #[test]
  fn reopen_queue_external_reopen_joins_existing_driver() {
    let mut queue = ReopenQueue::new(AutoReopenOption::new(1, Duration::from_millis(10)));
    let tbl = TableIdent::new("join", 1).unwrap();
    assert!(queue.park(req_envelope(read_req(&tbl))).is_none());
    assert!(
      queue
        .park(req_envelope(Request::Reopen {
          tbl: tbl.clone(),
          clean: false,
        }))
        .is_none()
    );
    let delayed = queue.heap.pop().unwrap();
    let mut wake_li = VecDeque::new();
    delayed.entry.wake(&mut wake_li);
    assert_eq!(wake_li.len(), 2);
    assert!(matches!(wake_li[0].req, Request::Get { .. }));
    assert!(matches!(wake_li[1].req, Request::Reopen { .. }));
  }

  /// 预算挂在读请求上：driver 成功重排后再次 `ResourceMissing` 才消耗下一轮，
  /// 耗尽后不再挂起；同表请求合并进同一个延迟条目。
  #[test]
  fn reopen_queue_read_retry_budget_is_per_request_and_merged() {
    let mut queue = ReopenQueue::new(AutoReopenOption::new(2, Duration::from_millis(10)));
    let tbl = TableIdent::new("budget", 1).unwrap();
    // 第一轮：两个读各自播种 2 → 消耗 1，合并成一条延迟条目。
    assert!(queue.park(req_envelope(read_req(&tbl))).is_none());
    assert!(queue.park(req_envelope(read_req(&tbl))).is_none());
    assert_eq!(queue.heap.len(), 1);

    // driver 成功：waiter 重排（预算剩 1），再次失败消耗最后一轮并合并。
    let delayed = queue.heap.pop().unwrap();
    let mut wake_li = VecDeque::new();
    delayed.entry.wake(&mut wake_li);
    assert_eq!(wake_li.len(), 2);
    for envelope in wake_li {
      assert!(queue.park(envelope).is_none());
    }
    assert_eq!(queue.heap.len(), 1);

    // 第三轮：预算已耗尽，两个读都不再挂起。
    let delayed = queue.heap.pop().unwrap();
    let mut wake_li = VecDeque::new();
    delayed.entry.wake(&mut wake_li);
    assert_eq!(wake_li.len(), 2);
    for envelope in wake_li {
      assert!(queue.park(envelope).is_some(), "预算耗尽后不应再挂起");
    }
    assert!(queue.next_due().is_none());
  }

  /// `retry_times == 0`：读请求立即失败，不建延迟条目。
  #[test]
  fn reopen_queue_zero_budget_does_not_park() {
    let mut queue = ReopenQueue::new(AutoReopenOption::new(0, Duration::from_millis(10)));
    let tbl = TableIdent::new("zero", 1).unwrap();
    assert!(queue.park(req_envelope(read_req(&tbl))).is_some());
    assert!(queue.next_due().is_none());
  }

  /// io-35：`bump_due` 只把同表条目的到期时间提前到给定时刻，不改别的表、
  /// 不新增条目；提前后 `next_due` 指向新到期时刻，driver 下一轮即可重试。
  #[test]
  fn reopen_queue_bump_due_advances_same_table_only() {
    const DELAY: Duration = Duration::from_secs(10);
    let mut queue = ReopenQueue::new(AutoReopenOption::new(2, DELAY));
    let hit = TableIdent::new("hit", 1).unwrap();
    let miss = TableIdent::new("miss", 1).unwrap();
    assert!(queue.park(req_envelope(read_req(&hit))).is_none());
    assert!(queue.park(req_envelope(read_req(&miss))).is_none());
    assert_eq!(queue.heap.len(), 2);
    // 两张表各自的到期时刻都远晚于 now（`park` 按 `pending_time` 延迟）。
    let parked = queue.next_due().expect("有挂起条目");
    assert!(parked.saturating_duration_since(Instant::now()) > DELAY / 2);

    let now = Instant::now();
    // 对不存在的表 no-op：不新增条目、最早到期不变。
    queue.bump_due(&TableIdent::new("absent", 1).unwrap(), now);
    assert_eq!(queue.heap.len(), 2, "bump_due 不新增条目");
    assert_eq!(queue.next_due(), Some(parked));

    queue.bump_due(&hit, now);
    assert_eq!(queue.next_due(), Some(now), "同表条目应提前到期");
    // 提前只是改到期键：弹掉提前的条目后，另一张表仍在原到期时刻（未被连带
    // 提前），堆规模也不因 bump 增长。
    let _ = queue.heap.pop();
    let other = queue.next_due().expect("另一张表条目仍在堆内");
    assert!(
      other.saturating_duration_since(now) > DELAY / 2,
      "其它表条目不应被提前：{other:?}"
    );
  }

  /// Fresh attach（Detached / Expired -> Attached）后旧身份排队写必须作废；
  /// 幂等 attach 与旧 term attach 导致的 Expired 不清队。
  #[test]
  fn fresh_attach_invalidates_queued_writes_only_on_reentry() {
    use DsShardState::{Attached, Detached, Expired};
    assert!(write_queue_stale(true, false, Detached, Attached));
    assert!(write_queue_stale(true, false, Expired, Attached));
    // 同 shard / branch / term 重复 attach：不驱逐 table 状态，也不清队。
    assert!(!write_queue_stale(true, false, Attached, Attached));
    // 旧 term attach 只让 shard 过期，排队写在 drain 时自会回 ExpiredTerm。
    assert!(!write_queue_stale(true, false, Attached, Expired));
    // 成功 detach；取消 / 超时的 detach 生命周期不变。
    assert!(write_queue_stale(false, true, Attached, Detached));
    assert!(!write_queue_stale(false, true, Detached, Detached));
    assert!(!write_queue_stale(false, false, Attached, Attached));
  }

  /// io-25：身份切换 / 关停清理必须覆盖 io-23 新增的 `deferred_read_li`。
  ///
  /// 触发形态：写 / 维护任务返回 `ExpiredTerm` 把 shard 置 `Expired`，此时读
  /// 在飞仍满额，一条读被读窗口以 `Full` 暂存、随后因 `quota == 0` 进入
  /// `deferred_read_li`；同一个 Fresh attach 到达后事件循环清空旧身份排队
  /// 请求。修复前 deferred 队列不在清单里，这条读会在 attach 后经 `admit`
  /// （此时已 `Attached`）按新 term 打开表并返回数据。这里按事件循环的三条
  /// 路径（Fresh attach / detach / shutdown）构造四个排队面与挂起的 reopen，
  /// 断言全部以 `NotRunning` 回执。
  #[test]
  fn identity_switch_fails_deferred_reads() {
    use DsShardState::{Attached, Detached, Expired};
    let tbl = TableIdent::new("stale", 1).unwrap();
    let cases = [
      (
        "fresh-attach",
        write_queue_stale(true, false, Expired, Attached),
      ),
      ("detach", write_queue_stale(false, true, Attached, Detached)),
      // shutdown：`stop` 与身份切换共用同一清理块。
      ("shutdown", true),
    ];
    for (path, stale) in cases {
      assert!(stale, "{path}: 应触发旧身份清理");
      let mut gate = WriteGate::new(0, Arc::new(ShardMetric::new(0)));
      let (write, write_rx) = tracked_envelope(Request::Put {
        tbl: tbl.clone(),
        batch_li: vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
        term: Some(5),
      });
      gate.push(tbl.clone(), (write, Pri::High));
      let (blocked, blocked_rx) = tracked_envelope(read_req(&tbl));
      let (large_park, large_park_rx) = tracked_envelope(Request::GetLarge {
        tbl: tbl.clone(),
        key: b"k".to_vec(),
      });
      // 满额批提交溢出到 deferred 的普通读（io-23）。
      let (deferred, deferred_rx) = tracked_envelope(read_req(&tbl));
      let (parked, parked_rx) = tracked_envelope(read_req(&tbl));
      let mut blocked_li = VecDeque::from([blocked]);
      let mut large_park_li = VecDeque::from([large_park]);
      let mut deferred_read_li = VecDeque::from([deferred]);
      let mut reopen_li = ReopenQueue::new(AutoReopenOption::new(1, Duration::from_millis(10)));
      assert!(
        reopen_li.park(parked).is_none(),
        "{path}: 读请求应挂进 reopen 等待队列"
      );

      fail_stale_pending(
        &mut gate,
        &mut blocked_li,
        &mut large_park_li,
        &mut deferred_read_li,
        &mut reopen_li,
        &mut StandbyFlight::new(),
      );

      assert!(
        blocked_li.is_empty() && large_park_li.is_empty() && deferred_read_li.is_empty(),
        "{path}: 暂存读队列未被清空"
      );
      assert!(!gate.has_ready(), "{path}: 写队列未被清空");
      assert!(
        reopen_li.next_due().is_none(),
        "{path}: 挂起的 reopen 未被清理"
      );
      assert_not_running(write_rx, path, "写队列");
      assert_not_running(blocked_rx, path, "blocked 读");
      assert_not_running(large_park_rx, path, "大值等待读");
      assert_not_running(deferred_rx, path, "deferred 读");
      assert_not_running(parked_rx, path, "挂起的 reopen 读");
    }
  }

  /// test-15：io-25 的身份切换清理必须由**事件循环调用点**保护，而不只是
  /// `fail_stale_pending` 单测里的队列清单。
  ///
  /// 用 [`ReadFlight::force_full_for_test`] 把在读槽位置为满额（等价于 128 条
  /// 读同时在飞，单测无法稳定构造），随后一条普通读 C 因批提交额度为 0 真实
  /// 进入 `deferred_read_li`。`AttachShard(term = 0)` 先把 shard 置 `Expired`，
  /// `AttachShard(term = 2)` 走 Fresh attach 触发清理块：
  ///
  /// - 修复后：C 在清理块内以 `NotRunning` 回执（本用例断言）；
  /// - 调用点回退成 io-25 前的内联清理（漏 deferred）：C 留在队列里，Fresh
  ///   attach 后按新 term 打开 hot 表返回数据——断言失败。
  ///
  /// 用 `ShardSender::enqueue` 而不是 `spawn(request)` 入队 C：`enqueue` 返回
  /// 时请求已确定进入高优先通道，排在随后发送的 Fresh attach 之前，请求顺序
  /// 不依赖任务调度。
  #[compio::test]
  async fn fresh_attach_clears_deferred_read_through_event_loop() {
    const WATCHDOG: Duration = Duration::from_secs(10);

    let root = TempRoot::new("deferred_stale_event_loop");
    let shard = crate::Shard::start(event_loop_config(root.path().to_path_buf())).unwrap();
    let hot = TableIdent::new("hot", 1).unwrap();
    let attach = |term: u64| Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term,
    };
    shard.request(attach(1)).await.unwrap();
    shard
      .request(Request::Put {
        tbl: hot.clone(),
        batch_li: vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
        term: Some(1),
      })
      .await
      .unwrap();

    // 在读满额：此后批提交额度为 0，新读进入 `deferred_read_li`。
    let force_full = ForceFullGuard::new();
    // Expired：旧 term attach 只把 shard 标记过期，不触发本 issue 的清理块。
    assert_eq!(
      shard.request(attach(0)).await.unwrap_err(),
      KvError::ExpiredTerm
    );

    // C：旧身份下的普通读，真实走「批提交发现额度为 0 -> deferred」。
    let sender = shard.sender();
    let deferred = sender
      .enqueue(
        Request::Get {
          tbl: hot.clone(),
          key: b"k".to_vec(),
        },
        None,
      )
      .await
      .unwrap();
    // Fresh attach（Expired -> Attached）：事件循环清理块必须失败 C。
    shard.request(attach(2)).await.unwrap();

    // 松开满额探针：若调用点漏了 deferred，C 会在此后按新 term 执行并返回
    // Ok；修复路径下 C 已在上一步被回执 NotRunning。
    drop(force_full);
    let result = timeout(WATCHDOG, deferred)
      .await
      .expect("deferred 读未在超时前回执");
    assert_eq!(
      result.unwrap_err(),
      KvError::NotRunning,
      "Fresh attach 后旧身份的 deferred 读必须以 NotRunning 失败"
    );
    shard.shutdown().await.unwrap();
  }

  /// [`ReadFlight::force_full_for_test`] 的 RAII 守卫：用例 panic 时也复位
  /// 进程级开关，避免影响同进程内其它 shard 用例。
  struct ForceFullGuard;

  impl ForceFullGuard {
    fn new() -> Self {
      ReadFlight::force_full_for_test(true);
      Self
    }
  }

  impl Drop for ForceFullGuard {
    fn drop(&mut self) {
      ReadFlight::force_full_for_test(false);
    }
  }

  /// 事件循环级用例的配置：Local 后端 + 小页/段，hot 表数据落在 data 页上。
  fn event_loop_config(root: PathBuf) -> crate::ShardConfig {
    crate::ShardConfig {
      alloc_mode: crate::AllocMode::Append,
      shard_id: 0,
      root,
      page_size: 4096,
      restart_interval: 16,
      index_restart_interval: 16,
      pages_per_file_shift: DEFAULT_PAGES_PER_FILE_SHIFT,
      segment_size: 4096,
      segments_per_file_shift: 0,
      large_value_threshold: 1024,
      fixed_buffer_size: 0,
      queue_size: 4096,
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
      page_cache_bytes: fkv_config::DEFAULT_PAGE_CACHE_BYTES as usize,
      root_meta_cache_bytes: 0,
      io_queue_size: 4096,
      backend: crate::Backend::default(),
      mode: StoreMode::Local,
      standby: None,
      max_write_concurrency: 0,
      max_processing_time_us: 0,
      yield_budget_us: 0,
    }
  }
}
