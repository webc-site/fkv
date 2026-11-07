//! 归档 crond、全局 fanout 与全局归档 / 分支 / 删表 API。

use std::{
  collections::HashSet,
  fs,
  io::{self, ErrorKind},
  path::{Path, PathBuf},
  sync::{
    Arc, PoisonError,
    atomic::{AtomicBool, Ordering},
  },
  thread,
  thread::JoinHandle,
  time::{Duration, Instant},
};

use crossfire::{
  MTx, RecvTimeoutError, Rx,
  mpsc::{Array, bounded_blocking},
};
use fkv_api::{Request, Response};
use fkv_cloud::CloudStore;
use fkv_error::{KvError, Result};
use fkv_io_compio::CompioStore;
use fkv_object_s3::S3ObjectStore;
use fkv_shard::CloudBackend;
use fkv_types::{TableIdent, archive_file_name, normalize_branch_name, now_us};
use futures::{executor::block_on, future::join_all};
use log::{info, warn};

use crate::{
  ArchiveEntry, GlobalReply, Runtime,
  cloud_env::{CloudStoreConfig, open_runtime_cloud_store},
  prewarm::remote_partition_li,
  route::ShardRoute,
};

/// 单轮归档 fanout 的整体等待上限。
///
/// 归档请求不套数据面默认 deadline（大表 / 远端限流下正常归档可能远慢于普通
/// 请求），但 crond 线程不能无限等一批回执：超过本上限后本端按 `Timeout`
/// 收尾（**不中断**已经开始执行的归档，它在 shard 侧自行跑完），下一轮 tick
/// 继续。正常运行下归档远快于该值，不改变归档语义。
const ARCHIVE_DISPATCH_TIMEOUT: Duration = Duration::from_secs(30);

/// `stop()` 等待归档线程退出的上限。
///
/// `JoinHandle` 没有带时限的 join，用轮询 `is_finished` 实现上界；超时后丢弃
/// 句柄（线程分离）并告警，不让停机等一整轮在飞归档。线程自身受
/// [`ARCHIVE_DISPATCH_TIMEOUT`] 约束，最终会退出。
const ARCHIVE_STOP_TIMEOUT: Duration = Duration::from_secs(2);

/// 目标集合来源：Local/Mem 以盘面分区为准，Cloud 以远端分页列举为准。
///
/// Cloud 下本地 cache 只看得见已 cache 的分区，归档对象必须按远端为准
/// （对齐 C++ `ListPartitions` 直接列对象存储）。
pub(crate) enum ArchiveTarget {
  Local(Vec<PathBuf>),
  Cloud(Box<CloudStoreConfig>, Box<CloudBackend>),
}

/// 归档参数与 attached_li 状态：crond 线程与请求路径共享。
pub(crate) struct ArchiveState {
  pub(crate) attached_li: Vec<AtomicBool>,
  target: ArchiveTarget,
  interval: Duration,
  num_retained: u32,
  /// 单批 fanout 的分区数（`max_global_request_batch`）。
  batch_li: usize,
}

impl ArchiveState {
  pub(crate) fn new(
    shard_num: u16,
    target: ArchiveTarget,
    interval: Duration,
    num_retained: u32,
    batch_li: u32,
  ) -> Self {
    Self {
      attached_li: (0..shard_num).map(|_| AtomicBool::new(false)).collect(),
      target,
      interval,
      num_retained,
      batch_li: batch_size(batch_li),
    }
  }
}

/// `max_global_request_batch` 归一化：0 / 超界一律按 1，避免空批或一次性全投。
fn batch_size(batch_li: u32) -> usize {
  usize::try_from(batch_li).unwrap_or(1).max(1)
}

/// `read_dir` / 目录项错误的 errno → `KvError`（对齐 C++ `ToKvError`）：
/// `ENOENT → NotFound`、`EPERM/EACCES → NoPermission`、其余 → `IoFail`。
fn scan_io_err(err: io::Error) -> KvError {
  match err.kind() {
    ErrorKind::NotFound => KvError::NotFound,
    ErrorKind::PermissionDenied => KvError::NoPermission,
    _ => KvError::IoFail,
  }
}

/// 列举盘上分区目录名（`{tbl_name}.{partition_id}`）。
///
/// 对齐 C++ `HandleGlobalArchiveRequest` / `CollectLocalPartitions`：每个
/// interval 重新列举 `store_path` 下全部 partition，含本进程从未写过的只读表；
/// 目录名不合规的条目直接跳过。不可读根 / 目录项错误向上返回，由调用方决定
/// 是否整轮失败（C++ 除 tag 列举外一律整轮回错）。
///
/// 唯一的放宽是**根不存在**按空根处理：C++ 在 `Start` 的 `InitStoreSpace`
/// 里对每个 `store_path` `create_directories`（`eloq_store.cpp:1178-1218`），
/// 所以它运行期看到 ENOENT 只可能是根启动后被摘走；本仓启动不建根（分区目录
/// 在首次写入时 `ensure_dir` 懒创建），首次部署 / 新增挂载点在首次写入前没有
/// 目录，把 ENOENT 当失败会打断正常启动路径。该差分记入 COMPAT_BASELINE。
fn scan_table_li(root: &Path) -> Result<Vec<TableIdent>> {
  let entry_li = match fs::read_dir(root) {
    Ok(entry_li) => entry_li,
    Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
    Err(err) => return Err(scan_io_err(err)),
  };
  let mut table_li = Vec::new();
  for entry in entry_li {
    let entry = entry.map_err(scan_io_err)?;
    // 与 C++ `entry.is_directory(ec)` 一致：类型读不出来按错误上报，不静默跳过。
    if !entry.file_type().map_err(scan_io_err)?.is_dir() {
      continue;
    }
    let name = entry.file_name();
    if let Some(tbl) = name.to_str().and_then(TableIdent::parse_dir_name) {
      table_li.push(tbl);
    }
  }
  Ok(table_li)
}

/// 多根列举：按 `store_path` 顺序拼接每个根的盘上分区，任一可读性错误整轮失败。
///
/// 分区归属由 LUT 固定，正常配置下不会跨根重复；不额外去重（对齐 C++
/// `HandleGlobalArchiveRequest` 直接遍历 `store_path`）。
fn scan_table_li_li(root_li: &[PathBuf]) -> Result<Vec<TableIdent>> {
  let mut table_li = Vec::new();
  for root in root_li {
    table_li.extend(scan_table_li(root)?);
  }
  Ok(table_li)
}

/// 扫描 `root` 下全部分区目录，取带归档 tag 的 manifest 条目。
///
/// `prefix` 非空时按 tag 前缀过滤，结果按 `(tag, term)` 升序。
///
/// 与 C++ 一致的三点：① 直扫盘面、不经 shard、不检查 attach 状态（本进程从未
/// 写过的只读表也要列）；② 排序后按 `(tag, term)` 全等去重（C++ `std::sort` +
/// `std::unique`：多分区 fanout 归档同 tag 同 term 只回一条，同 tag 不同 term
/// 各占一条）；③ 单个目录读失败按跳过处理（控制面列举容错，不报部分错误）。
///
/// 已知偏离：C++ 遍历 `options_.store_path` 的全部根；本仓由 `root_li` 覆盖
/// 全部本地根（单根时就是一个），Cloud 模式下列的是本地 cache 目录，
/// 与 C++ 同（C++ 的该 handler 也只扫本地 store_path，不列远端）。
fn list_archive_tag_li(root_li: &[PathBuf], prefix: &str) -> Vec<ArchiveEntry> {
  let mut entry_li: Vec<ArchiveEntry> = Vec::new();
  for root in root_li {
    list_archive_tags_of(root, prefix, &mut entry_li);
  }
  entry_li.sort();
  entry_li.dedup();
  entry_li
}

fn list_archive_tags_of(root: &Path, prefix: &str, entry_li: &mut Vec<ArchiveEntry>) {
  // 对齐 C++ `HandleGlobalListArchiveTagsRequest`（`eloq_store.cpp:2137-2145`）：
  // 根 / 分区目录列举失败一律 `ec.clear(); continue;`，控制面 tag 列举不因坏根
  // 整轮失败 —— 这是全局操作里唯一保持静默跳过的入口。
  let Ok(tbl_li) = scan_table_li(root) else {
    return;
  };
  for tbl in tbl_li {
    let dir = root.join(tbl.dir_name());
    let Ok(file_li) = fs::read_dir(dir) else {
      continue;
    };
    for entry in file_li.flatten() {
      if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
        continue;
      }
      let name = entry.file_name();
      let Some(name) = name.to_str() else {
        continue;
      };
      // 无 tag 的 manifest（`manifest_<branch>_<term>`）不是归档条目。
      let Some((_, term, Some(tag))) = fkv_types::parse_manifest_file_name(name) else {
        continue;
      };
      if !prefix.is_empty() && !tag.starts_with(prefix) {
        continue;
      }
      entry_li.push(ArchiveEntry {
        tag: tag.to_owned(),
        term,
      });
    }
  }
}

/// 跨 shard fanout：按 partition hash 路由，并按 `max_global_request_batch` 分批
/// 投递 —— 每批顶层并发（`join_all`）、批间串行，限制在途请求与回执内存。
///
/// 调用方要么在 async 上下文 `.await`，要么在 shard poll 栈之外的顶层线程
/// `block_on`（crond）；绝不在 shard 任务栈内驱动调度器。
struct GlobalFanout {
  route_li: Vec<ShardRoute>,
  num_threads: u16,
  batch_li: usize,
}

impl GlobalFanout {
  fn new(route_li: Vec<ShardRoute>, batch_li: usize) -> Self {
    Self {
      num_threads: route_li.len() as u16,
      route_li,
      batch_li,
    }
  }

  fn num_threads(&self) -> u16 {
    self.num_threads
  }

  /// 对每个分区生成 `make_req` 请求并投递到其归属 shard，聚合回执。
  async fn dispatch(
    &self,
    table_li: &[TableIdent],
    make_req: impl Fn(&TableIdent) -> Request,
  ) -> GlobalReply {
    self.dispatch_collect(table_li, make_req).await.0
  }

  /// [`Self::dispatch`] 的带 deadline 版本：整轮 fanout 最多等待到 `deadline`
  /// （每批按剩余时间设置请求 deadline），供后台 crond 这类不得无限等待的
  /// 调用方使用。超时的请求本端回 `Timeout`，已开始执行的请求仍会跑完。
  async fn dispatch_deadline(
    &self,
    table_li: &[TableIdent],
    make_req: impl Fn(&TableIdent) -> Request,
    deadline: Instant,
  ) -> GlobalReply {
    self
      .dispatch_collect_deadline(table_li, make_req, Some(deadline))
      .await
      .0
  }

  /// [`Self::dispatch`] 的带载荷版本：额外返回第一条成功回执（用于提取
  /// `ArchiveReply::name` / `BranchReply::name` 这类全局结果）。
  async fn dispatch_collect(
    &self,
    table_li: &[TableIdent],
    make_req: impl Fn(&TableIdent) -> Request,
  ) -> (GlobalReply, Option<Response>) {
    self
      .dispatch_collect_deadline(table_li, make_req, None)
      .await
  }

  /// [`Self::dispatch_collect`] 的实现：`deadline` 为整轮 fanout 的绝对上限。
  async fn dispatch_collect_deadline(
    &self,
    table_li: &[TableIdent],
    make_req: impl Fn(&TableIdent) -> Request,
    deadline: Option<Instant>,
  ) -> (GlobalReply, Option<Response>) {
    let mut reply = GlobalReply::default();
    let mut first_response = None;
    for chunk in table_li.chunks(self.batch_li) {
      let pending = chunk.iter().map(|tbl| {
        let shard_id = usize::from(tbl.shard_index(self.num_threads));
        // SAFETY: shard_index 对 shard_num 取模，必 < route_li.len()
        let route = unsafe { self.route_li.get_unchecked(shard_id) };
        let req = make_req(tbl);
        // 同一批共享整轮的绝对 deadline；批间串行，剩余时间自然递减。
        let timeout = deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
        async move {
          let result = match timeout {
            Some(timeout) => route.request_deadline(req, timeout).await,
            None => route.request(req).await,
          };
          (tbl, result)
        }
      });
      for (tbl, result) in join_all(pending).await {
        match result {
          Ok(response) => {
            reply.ok_num += 1;
            if first_response.is_none() {
              first_response = Some(response);
            }
          }
          Err(err) => reply.failed_li.push((tbl.clone(), err)),
        }
      }
    }
    (reply, first_response)
  }
}

/// 归档后台线程：crossfire 通道兼作定时器与停止信号，drop 发送端即停止。
pub(crate) struct ArchiveCrond {
  stop_tx: Option<MTx<Array<()>>>,
  join: Option<JoinHandle<()>>,
}

impl ArchiveCrond {
  pub(crate) fn start(state: Arc<ArchiveState>, route_li: Vec<ShardRoute>) -> Result<Self> {
    Self::start_inner(state, route_li, Self::run)
  }

  /// 线程体可注入（单测用桩 tick 模拟在飞归档不回执），生产入口固定
  /// [`Self::run`]。
  fn start_inner(
    state: Arc<ArchiveState>,
    route_li: Vec<ShardRoute>,
    run: impl FnOnce(&ArchiveState, &[ShardRoute], &Rx<Array<()>>) + Send + 'static,
  ) -> Result<Self> {
    let (stop_tx, stop_rx) = bounded_blocking::<()>(1);
    let join = thread::Builder::new()
      .name("fkv-archive".to_owned())
      .spawn(move || run(&state, &route_li, &stop_rx))
      .map_err(|_| KvError::IoFail)?;
    Ok(Self {
      stop_tx: Some(stop_tx),
      join: Some(join),
    })
  }

  fn run(state: &ArchiveState, route_li: &[ShardRoute], stop_rx: &Rx<Array<()>>) {
    // Cloud 模式列远端分区需要一个运行时侧 `CloudStore`；它只做列举、不碰本地
    // cache，所以不需要 compio runtime。惰性构造一次后跨 tick 复用。
    let mut cloud_store: Option<CloudStore<CompioStore, S3ObjectStore>> = None;
    while let Err(RecvTimeoutError::Timeout) = stop_rx.recv_timeout(state.interval) {
      Self::tick(state, route_li, &mut cloud_store);
    }
  }

  /// 一轮归档：先确定目标分区（Local 列盘 / Cloud 列远端），再按
  /// `max_global_request_batch` 分批 fanout；本线程只在顶层（非任务栈）等待全部
  /// 回执。只归档 attached_li 的 shard。
  fn tick(
    state: &ArchiveState,
    route_li: &[ShardRoute],
    cloud_store: &mut Option<CloudStore<CompioStore, S3ObjectStore>>,
  ) {
    let table_li = match Self::enum_target(state, cloud_store) {
      Ok(table_li) => table_li,
      Err(err) => {
        warn!(target: "fkv_runtime", "archive crond listing failed error={}", err);
        return;
      }
    };
    let fanout = GlobalFanout::new(route_li.to_vec(), state.batch_li);
    let shard_num = fanout.num_threads();
    let pending: Vec<TableIdent> = table_li
      .into_iter()
      .filter(|tbl| {
        state.attached_li[usize::from(tbl.shard_index(shard_num))].load(Ordering::Relaxed)
      })
      .collect();
    if pending.is_empty() {
      return;
    }
    let tag = fkv_api::archive_tag();
    let num_retained = state.num_retained;
    // 单轮 fanout 有整体上限：归档再慢也不能让 crond 线程（以及 `stop()` 的
    // join）无限等待。超时只影响本端收尾，归档请求在 shard 侧仍会跑完。
    let reply = block_on(fanout.dispatch_deadline(
      &pending,
      |tbl| Request::CreateArchive {
        tbl: tbl.clone(),
        tag: tag.clone(),
        num_retained,
      },
      Instant::now() + ARCHIVE_DISPATCH_TIMEOUT,
    ));
    let total = reply.total();
    let archived = reply.ok_num;
    // shard 未 attach 的 `NotRunning` 是预期结果，不算失败；其余错误必须告警。
    let failed = reply
      .failed_li
      .iter()
      .filter(|(tbl, err)| {
        if matches!(err, KvError::NotRunning) {
          return false;
        }
        warn!(
          target: "fkv_runtime",
          "archive crond failed table={} tag={} error={}",
          tbl,
          tag,
          err
        );
        true
      })
      .count();
    if archived > 0 {
      info!(
        target: "fkv_runtime",
        "archive crond run tables={} archived={} failed={} tag={}",
        total,
        archived,
        failed,
        tag
      );
    }
  }

  /// 目标集合：Local 列盘上分区目录；Cloud 列远端分区（惰性建 store 后复用）。
  fn enum_target(
    state: &ArchiveState,
    cloud_store: &mut Option<CloudStore<CompioStore, S3ObjectStore>>,
  ) -> Result<Vec<TableIdent>> {
    match &state.target {
      ArchiveTarget::Local(root_li) => scan_table_li_li(root_li),
      ArchiveTarget::Cloud(config, cloud) => {
        if cloud_store.is_none() {
          *cloud_store = Some(open_runtime_cloud_store(config, cloud)?);
        }
        match cloud_store.as_ref() {
          Some(store) => block_on(remote_partition_li(store)),
          None => Err(KvError::CloudErr),
        }
      }
    }
  }

  /// 停止归档线程，等待有上界。
  ///
  /// 置停止信号后最多等 [`ARCHIVE_STOP_TIMEOUT`]：归档在飞（`block_on` 等一批
  /// 回执）时不再无限 join，超时丢弃句柄让线程分离并告警；线程自身受
  /// [`ARCHIVE_DISPATCH_TIMEOUT`] 约束，最终退出。
  pub(crate) fn stop(&mut self) {
    self.stop_tx.take();
    let Some(join) = self.join.take() else {
      return;
    };
    // `JoinHandle` 没有带时限的 join：轮询 `is_finished` 做上界（10ms 粒度）。
    let deadline = Instant::now() + ARCHIVE_STOP_TIMEOUT;
    while !join.is_finished() {
      if Instant::now() >= deadline {
        warn!(
          target: "fkv_runtime",
          "archive crond stop timed out timeout_ms={} thread=detached",
          ARCHIVE_STOP_TIMEOUT.as_millis()
        );
        // `join` 在此 drop：线程分离，不阻塞停机。
        return;
      }
      thread::sleep(Duration::from_millis(10));
    }
    let _ = join.join();
  }
}

impl Drop for ArchiveCrond {
  fn drop(&mut self) {
    self.stop();
  }
}

/// 分支内部名的 8 位十六进制 salt：把完整 64 位微秒时间戳折叠成非零 `u32`。
///
/// `now_us() as u32` 每 2³² µs（约 71.6 分钟）回绕，同一逻辑名在两个回绕周期后
/// 会得到完全相同的内部名，第二次创建会落到已存在的 branch 上。这里先用
/// splitmix32 混合时间戳高位，保持 8 位十六进制形状的同时消除回绕重名。
/// 显式 `salt_ts` 不走本函数（对齐 C++ `SetSaltTimestamp` 的确定性命名）。
pub(crate) fn branch_salt(ts_us: u64) -> u32 {
  let mut x = (ts_us ^ (ts_us >> 32)) as u32;
  x ^= x >> 16;
  x = x.wrapping_mul(0x7feb_352d);
  x ^= x >> 15;
  x = x.wrapping_mul(0x846c_a68b);
  x ^= x >> 16;
  x.max(1)
}

impl Runtime {
  /// 全局请求的目标集合：Local/Mem 列盘上分区目录；Cloud 列远端分区
  /// （`ObjectStore::list_page(prefix)` 分页，内存 O(页)）。
  ///
  /// Cloud 下复用运行时侧列举 store（控制面操作、不频繁；列举不碰本地
  /// cache，所以不需要 compio runtime）。实例长驻，避免每次全局操作都新起
  /// 一个 S3 线程。
  ///
  /// Local 下任一 `store_path` 根不可读即整轮 `Err`（`NoPermission` / `IoFail`，
  /// 对齐 C++ `directory_iterator(root, ec)` 失败整轮回错），不再把该根当空；
  /// 根不存在仍按空根处理（C++ 启动期会建根，本仓懒建，见 `scan_table_li`）。
  pub(crate) async fn enumerate_target_li(&self) -> Result<Vec<TableIdent>> {
    match &self.cloud_enum {
      Some((cfg, cloud)) => {
        // `CloudStore` 非 `Sync`，而 `Runtime` 要被多线程 `Arc` 共享：锁内只做
        // 取出 / 放回，不跨 await 持锁。并发调用各自临时建实例，串行调用复用。
        let store = {
          let mut slot = self
            .cloud_enum_store
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
          match slot.take() {
            Some(store) => store,
            None => Box::new(open_runtime_cloud_store(cfg, cloud)?),
          }
        };
        let result = remote_partition_li(&store).await;
        let mut slot = self
          .cloud_enum_store
          .lock()
          .unwrap_or_else(PoisonError::into_inner);
        if slot.is_none() {
          *slot = Some(store);
        }
        result
      }
      None => scan_table_li_li(&self.root_li),
    }
  }

  /// 跨 shard fanout（按 `max_global_request_batch` 分批）。
  fn fanout(&self) -> GlobalFanout {
    self.settle_timeout_views();
    let route_li: Vec<ShardRoute> = self
      .shard_li
      .iter()
      .map(|shard| self.route_of_shard(shard))
      .collect();
    GlobalFanout::new(route_li, self.max_global_request_batch)
  }

  /// 全局归档：目标集合 = 全部分区；按 `max_global_request_batch` 分批 fanout。
  /// 单表失败（含未 attach shard 的 `NotRunning`）计入回执，不静默。
  ///
  /// 空 tag 时**只生成一次**并从本层广播给全部分区（对齐 C++
  /// `HandleGlobalArchiveRequest` 的 `tag = UnixTs` + `archive_req->SetTag(tag)`），
  /// 保证一次全局归档是一份同名快照；生成的 tag 与归档 manifest 名通过
  /// [`GlobalReply::tag`] / [`GlobalReply::name`] 返回给调用方。
  ///
  /// 0 个目标（空盘 / 全部 detach）或全部失败时 `name` 仍按 attach 身份构造
  /// （见 [`Self::global_archive_name`]），与 C++ `result_archive_` 的赋值时机一致。
  pub async fn global_archive(&self, tag: &str, num_retained: u32) -> Result<GlobalReply> {
    let tag = if tag.is_empty() {
      fkv_api::archive_tag()
    } else {
      tag.to_owned()
    };
    // 0 个成功回执时按 attach 身份构造名字：C++ `HandleGlobalArchiveRequest`
    // 在列举分区前就写 `result_archive_`（`BranchArchiveName`），空集合也回传。
    let table_li = self.enumerate_target_li().await?;
    let (mut reply, first_response) = self
      .fanout()
      .dispatch_collect(&table_li, |tbl| Request::CreateArchive {
        tbl: tbl.clone(),
        tag: tag.clone(),
        num_retained,
      })
      .await;
    reply.name = match first_response {
      // 首个成功分区可能是空表：C++ 在列举前就写 result_archive_，空表不返回
      // 空名。只有非空归档回执才覆盖身份名。
      Some(Response::CreateArchive(archive)) if !archive.name.is_empty() => archive.name,
      _ => self.global_archive_name(&tag),
    };
    reply.tag = tag;
    Ok(reply)
  }

  /// 全局重开：`tag` 为空按活跃快照重开（等价普通 `Reopen`）；非空按归档 tag
  /// 重开（对齐 C++ `GlobalReopenRequest`：同一个 tag 广播到全部分区）。
  ///
  /// `StandbyReplica` 的目标集合是 **master 远端 ∪ 本地扫描**（对齐 C++
  /// `HandleGlobalReopenRequest` 的 standby 分支）：远端独有分区走 `Reopen`
  /// 的 rsync 安装路径拉到本地；仅本地存在（master 已删）的分区置
  /// `clean = true`，由 shard 按 C++ `ReopenTask::Reopen` 的 `clear_local_state`
  /// 语义清理本地状态并回成功，不再进 `failed_li`。非 standby 模式枚举不变。
  ///
  /// 非空 tag 沿用 `ReopenArchive`，clean 语义与空 tag 完全一致（C++ 的
  /// `SetClean(true)` 同样对带 tag 的 `ReopenRequest` 生效）：目标集合仍与
  /// C++ 一样包含远端独有分区，「仅本地存在」的分区清理成功、不进 `failed_li`。
  /// master 仍有该分区但该 tag 的归档缺失时，`reopen_archive` 按显式重开的
  /// 差异回 `ResourceMissing`（C++ 会清本地状态并成功，见 COMPAT_BASELINE
  /// 「缺失快照的重开语义」）。
  pub async fn global_reopen(&self, tag: &str) -> Result<GlobalReply> {
    let (table_li, remote_set) = if self.standby_replica {
      let (table_li, remote_set) = self.standby_reopen_target_li().await?;
      (table_li, Some(remote_set))
    } else {
      (self.enumerate_target_li().await?, None)
    };
    // 仅本地存在（远端没有）→ 对齐 C++ `SetClean(true)`；空 tag 与非空 tag 同口径。
    let clean_of = |tbl: &TableIdent| remote_set.as_ref().is_some_and(|set| !set.contains(tbl));
    Ok(
      self
        .fanout()
        .dispatch(&table_li, |tbl| {
          if tag.is_empty() {
            Request::Reopen {
              tbl: tbl.clone(),
              clean: clean_of(tbl),
            }
          } else {
            Request::ReopenArchive {
              tbl: tbl.clone(),
              tag: tag.to_owned(),
              clean: clean_of(tbl),
            }
          }
        })
        .await,
    )
  }

  /// StandbyReplica 全局重开的目标集合：master 远端 ∪ 本地扫描（去重）。
  ///
  /// 对齐 C++ `HandleGlobalReopenRequest` 的 standby 分支：
  /// `ListStandbyPartitionRequest` 拿 master 分区清单，再列本地盘面补上仅本地
  /// 存在的分区；返回远端集合，供调用方给「本地独有」分区置 clean。
  /// 远端列举失败整轮失败（C++ 同样直接 `SetDone(list_error)`），不静默退化
  /// 成只扫本地 —— 否则远端新增分区会被漏掉且调用方无从察觉。
  async fn standby_reopen_target_li(&self) -> Result<(Vec<TableIdent>, HashSet<TableIdent>)> {
    let remote_li = self.list_standby_partition().await?;
    let mut seen = HashSet::with_capacity(remote_li.len());
    let mut remote_set = HashSet::with_capacity(remote_li.len());
    let mut target_li = Vec::with_capacity(remote_li.len());
    for tbl in remote_li {
      if seen.insert(tbl.clone()) {
        remote_set.insert(tbl.clone());
        target_li.push(tbl);
      }
    }
    for tbl in scan_table_li_li(&self.root_li)? {
      if seen.insert(tbl.clone()) {
        target_li.push(tbl);
      }
    }
    Ok((target_li, remote_set))
  }

  /// 全局删除归档 tag：同一 tag 广播到全部分区（对齐 C++
  /// `GlobalArchiveRequest::Action::Delete`，空 tag 非法）。
  ///
  /// `name` 与 create 同路径返回（C++ 删归档也走 `HandleGlobalArchiveRequest`
  /// 的 `result_archive_`），0 目标 / 全部失败时按 attach 身份构造。
  pub async fn global_delete_archive(&self, tag: &str) -> Result<GlobalReply> {
    if tag.is_empty() {
      return Err(KvError::InvalidArgs);
    }
    let table_li = self.enumerate_target_li().await?;
    let mut reply = self
      .fanout()
      .dispatch(&table_li, |tbl| Request::DeleteArchive {
        tbl: tbl.clone(),
        tag: tag.to_owned(),
      })
      .await;
    reply.tag = tag.to_owned();
    reply.name = self.global_archive_name(tag);
    Ok(reply)
  }

  /// 全局建分支（branch = 快照）：目标集合 = 全部分区。
  ///
  /// 对齐 C++ `HandleGlobalCreateBranchRequest`：调用方给的是**逻辑名**，本层
  /// 归一化后追加 `-<8 位十六进制 salt>` 作为内部 branch 名，并通过
  /// [`GlobalReply::name`] 返回；调用方必须用该名字继续操作分支。
  /// salt 由完整 64 位微秒时间戳经 [`branch_salt`] 混合折叠成非零 32 位：
  /// 低 32 位约每 71.6 分钟回绕一次，先与时戳高位异或再 splitmix 混合，
  /// 使跨回绕周期的同一逻辑名得到不同内部名；定宽 `%08x` 输出。
  pub async fn global_create_branch(&self, branch: &str) -> Result<GlobalReply> {
    self.global_create_branch_with_salt(branch, 0).await
  }

  /// 同 [`Self::global_create_branch`]，但 salt 可由调用方指定：
  /// `salt_ts == 0` 用实时时钟，非 0 用给定时间戳低 32 位 —— 对齐 C++
  /// `GlobalCreateBranchRequest::SetSaltTimestamp` 的确定性命名语义。
  pub async fn global_create_branch_with_salt(
    &self,
    branch: &str,
    salt_ts: u64,
  ) -> Result<GlobalReply> {
    let normalized = normalize_branch_name(branch).ok_or(KvError::InvalidArgs)?;
    let salt_val = if salt_ts == 0 {
      branch_salt(now_us())
    } else {
      salt_ts as u32
    };
    let internal_name = format!("{normalized}-{salt_val:08x}");
    let table_li = self.enumerate_target_li().await?;
    let (mut reply, _) = self
      .fanout()
      .dispatch_collect(&table_li, |tbl| Request::CreateBranch {
        tbl: tbl.clone(),
        branch: internal_name.clone(),
      })
      .await;
    reply.name = internal_name;
    Ok(reply)
  }

  /// 全局删表：只删 `tbl_name` 名下全部分区（逐分区 `Drop`），对齐 C++
  /// `DropTableRequest`（按表名收集分区后 fanout，绝不删其它表）。
  pub async fn global_drop_table(&self, tbl_name: &str) -> Result<GlobalReply> {
    if tbl_name.is_empty() {
      return Err(KvError::InvalidArgs);
    }
    let table_li: Vec<TableIdent> = self
      .enumerate_target_li()
      .await?
      .into_iter()
      .filter(|tbl| tbl.tbl_name.as_ref() == tbl_name)
      .collect();
    Ok(
      self
        .fanout()
        .dispatch(&table_li, |tbl| Request::Drop { tbl: tbl.clone() })
        .await,
    )
  }

  /// 全局列举归档标签（控制面读）：扫 `root_li` 下全部分区目录里带 tag 的
  /// manifest，`prefix` 非空时按 tag 前缀过滤，按 `(tag, term)` 升序并去重。
  ///
  /// 对齐 C++ `HandleGlobalListArchiveTagsRequest`：**不经 shard、不 fanout**，
  /// 所以未 attach 的只读分区也能列出；任何根 / 分区目录列举失败都静默跳过
  /// （控制面列举不因单个坏目录整体失败，两侧一致，是全局操作里唯一如此处理的
  /// 入口）。语义细节见 `list_archive_tag_li`。
  pub async fn global_list_archive_tag_li(&self, prefix: &str) -> Result<Vec<ArchiveEntry>> {
    Ok(list_archive_tag_li(&self.root_li, prefix))
  }

  /// [`Self::global_list_archive_tag_li`] 的同义方法（C++ 请求名
  /// `GlobalListArchiveTags` 不带 `_li` 后缀）。
  pub async fn global_list_archive_tags(&self, prefix: &str) -> Result<Vec<ArchiveEntry>> {
    self.global_list_archive_tag_li(prefix).await
  }

  /// 全局归档结果名：`manifest_<branch>_<term>_<tag>`（C++ `BranchArchiveName`）。
  ///
  /// `branch` / `term` 取**最低 shard_id 的已 attach 身份**；没有任何 shard
  /// attach 时回落 `(main, 0)` —— C++ `branch_` 默认 `MainBranchName`，
  /// `ShardTerm` 对未 attach 的 shard 返回 0。多 shard 身份不一致时取最低 id，
  /// 与 C++ `DefaultDsShard`（Start 时 attach 的 shard，否则最先 attach 的）
  /// 的确定性取向一致；正常用法下全部 shard 共用同一 `(branch, term)`。
  ///
  /// 已知近似：C++ `branch_` 在 `Start(branch)` 时固定、attach 不改它；
  /// fkv `Runtime::start` 没有 branch 参数，只能用 attach 身份，全部 detach 后
  /// 回到默认 `main`。控制面结果名的用途（回传 / 记账）不依赖这一差异。
  pub(crate) fn global_archive_name(&self, tag: &str) -> String {
    let ident = self.attach_ident();
    archive_file_name(&ident.branch, ident.term, tag)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// 归档线程卡在 `block_on(fanout.dispatch(...))` 时，`stop()` 必须有界返回。
  ///
  /// 用注入的 tick 桩（不回执、不响应停止信号）模拟「一批归档请求在飞」：
  /// 真实 store 没有可在运行时注入的慢归档入口（未覆盖面见 issue 记录），
  /// 本测试锁定的是 `stop()` 的等待上界；单请求 deadline 本身由
  /// `fkv_shard/tests/deadline.rs` 覆盖。
  #[test]
  fn shutdown_bounded_while_archive_in_flight() {
    let state = Arc::new(ArchiveState::new(
      1,
      ArchiveTarget::Local(Vec::new()),
      Duration::from_secs(3600),
      4,
      1,
    ));
    let mut crond = ArchiveCrond::start_inner(state, Vec::new(), |_, _, _| {
      thread::sleep(Duration::from_secs(60));
    })
    .unwrap();
    let started = Instant::now();
    crond.stop();
    let elapsed = started.elapsed();
    assert!(
      elapsed >= ARCHIVE_STOP_TIMEOUT,
      "stop 应等到上界才放弃: {elapsed:?}"
    );
    assert!(
      elapsed < ARCHIVE_STOP_TIMEOUT + Duration::from_secs(1),
      "stop 必须有界，不能等线程退出: {elapsed:?}"
    );
  }
}
