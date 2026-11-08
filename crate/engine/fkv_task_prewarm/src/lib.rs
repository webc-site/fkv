//! 云缓存预热：把某个 table 的 manifest / data / segment 文件拉到本地 cache。

use core::cell::Cell;
use std::{
  collections::VecDeque,
  pin::pin,
  time::{Duration, Instant},
};

use fkv_cloud::{CLOUD_LIST_PAGE_KEYS, CloudStore, RemoteFileEntry};
use fkv_error::Result;
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_object::ObjectStore;
use fkv_types::{FileId, TableIdent};
use futures::{Stream, StreamExt, stream};
use futures_timer::Delay;

/// 单表并发下载文件数；避免同时打开过多本地文件。
const PREWARM_FILE_CONCURRENCY: usize = 8;
/// 同时预热的表数默认值。运行时可经 `KvOption::prewarm_task_count` 覆盖。
pub const PREWARM_TABLE_CONCURRENCY: usize = 4;
/// 单文件下载遇到可重试错误的尝试次数（含首次）。
///
/// 同表 `PREWARM_FILE_CONCURRENCY` 路并发安装会争同一张表的 scheme 回填权：
/// 只有一路拿到，其余返回 `TryAgain`。不重试的话整表预热会被判成
/// `DownloadError` 中止（冷本地 + 旧目录必现）。预热是后台任务，允许有界
/// 退避等待持权者把 scheme 落盘，因此预算按「回填可能扫整表段文件」放宽。
const PULL_RETRY_TIMES: u32 = 64;
const PULL_RETRY_INIT_DELAY_MS: u64 = 1;
const PULL_RETRY_MAX_DELAY_MS: u64 = 100;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PrewarmStat {
  pub manifest_present: bool,
  pub data_file_num: usize,
  pub segment_file_num: usize,
  pub missing_num: usize,
  pub total_files_listed: usize,
  pub total_files_pulled: usize,
  pub files_skipped_missing: usize,
  pub files_failed: usize,
  pub completion_reason: CompletionReason,
  pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CompletionReason {
  #[default]
  Success,
  DiskFull,
  ListingError,
  DownloadError,
  Shutdown,
}

impl PrewarmStat {
  pub fn file_num(&self) -> usize {
    self.data_file_num + self.segment_file_num + usize::from(self.manifest_present)
  }
}

/// 预热失败的来源：列举远端目录失败，还是单文件下载失败。
enum WarmErr {
  List,
  Pull,
  /// 本地 cache 装不下下一个文件：中止本轮（对齐 C++ `DiskFull`）。
  DiskFull,
}

/// 流式列举某 table 的远端文件条目（id + 对象大小）。
///
/// 每页 `CLOUD_LIST_PAGE_KEYS` 条，页内条目暂存在 `VecDeque`，下游逐条取用；
/// 列举与下载并行推进，内存 O(页) 而非 O(bucket)。对齐 C++ prewarm 的
/// `kMaxPrewarmPendingFiles = 1000` 有界队列。
fn file_entry_stream<'a, I, O>(
  cloud: &'a CloudStore<I, O>,
  tbl: &'a TableIdent,
  segment: bool,
) -> impl Stream<Item = Result<RemoteFileEntry>> + 'a
where
  I: 'a,
  O: ObjectStore + 'a,
{
  stream::unfold(
    (None::<String>, VecDeque::<RemoteFileEntry>::new(), false),
    move |(mut token, mut buf, mut done)| async move {
      loop {
        if let Some(entry) = buf.pop_front() {
          return Some((Ok(entry), (token, buf, done)));
        }
        if done {
          return None;
        }
        let page = if segment {
          cloud
            .remote_segment_file_page(tbl, token.as_deref(), CLOUD_LIST_PAGE_KEYS)
            .await
        } else {
          cloud
            .remote_data_file_page(tbl, token.as_deref(), CLOUD_LIST_PAGE_KEYS)
            .await
        };
        match page {
          Ok(page) => {
            // 整页条目都被名字过滤掉时直接续拉下一页，不向下游产出空转。
            done = page.next_token.is_none();
            token = page.next_token;
            buf.extend(page.entry_li);
          }
          Err(err) => return Some((Err(err), (None, VecDeque::new(), true))),
        }
      }
    },
  )
}

/// 按页「列举 → 下载」一个类别（data 或 segment）。
///
/// 在途下载数受 `PREWARM_FILE_CONCURRENCY` 限制，pending 上限约一页 + 在途。
/// 单个文件失败（列举 / 持久下载错误 / 重试预算耗尽）立即中止整轮（对齐 C++
/// 的持久错误中止）；瞬时错误（`TryAgain` 等）先按 [`pull_with_retry`] 有界
/// 重试，不会因为同组回填权竞争把整表预热打成失败。
/// 每个文件下载前按对象大小与 `cache_remained()` 预判磁盘余量
/// （对齐 C++ `prewarm_task.cpp:135`），不足即中止本轮（丢弃流 → 在途下载
/// 取消、列举停止）并置 `CompletionReason::DiskFull`。单文件失败计入 stats；
/// 列举失败置 `ListingError`，已完成的下载计数保留。
/// 结果逐条消费，不物化整表结果集。
async fn prewarm_paged<I, O>(
  cloud: &CloudStore<I, O>,
  tbl: &TableIdent,
  segment: bool,
  stats: &mut PrewarmStat,
) where
  I: FileIo + ManifestIo + PageIo + SegmentIo,
  O: ObjectStore,
{
  let listed = Cell::new(0usize);
  let stream = file_entry_stream(cloud, tbl, segment)
    .map(|item| {
      let listed = &listed;
      async move {
        let entry = item.map_err(|_| WarmErr::List)?;
        listed.set(listed.get() + 1);
        // 物理已存在的文件先登记记账（重启后内存态为空）：否则既不计预算也不参与
        // 淘汰，还会在余量门前被当成 miss 而误报 DiskFull。已热文件不新增占用，
        // 预算满也不该中止整轮。
        let present = cloud
          .adopt_local_file(tbl, entry.file_id, segment, entry.size)
          .await
          .map_err(|_| WarmErr::Pull)?;
        if !present && cloud.cache_remained() < entry.size {
          return Err(WarmErr::DiskFull);
        }
        let pulled = pull_with_retry(cloud, tbl, entry.file_id, segment).await;
        pulled.map_err(|_| WarmErr::Pull)
      }
    })
    .buffer_unordered(PREWARM_FILE_CONCURRENCY);
  let mut stream = pin!(stream);
  while let Some(result) = stream.next().await {
    match result {
      Ok(true) if segment => stats.segment_file_num += 1,
      Ok(true) => stats.data_file_num += 1,
      Ok(false) => stats.missing_num += 1,
      Err(WarmErr::DiskFull) => {
        stats.completion_reason = CompletionReason::DiskFull;
        break;
      }
      // 持久错误立即中止整轮（对齐 C++ `StopAllPrewarmTasks` + `ClearPrewarmFiles`）：
      // 继续拉只会对已经坏掉的 master 继续加压、并挤占前台请求。
      Err(WarmErr::List) => {
        stats.files_failed += 1;
        stats.completion_reason = CompletionReason::ListingError;
        break;
      }
      Err(WarmErr::Pull) => {
        stats.files_failed += 1;
        stats.completion_reason = CompletionReason::DownloadError;
        break;
      }
    }
  }
  stats.total_files_listed += listed.get();
}

/// 有界退避重试单文件下载：只重试瞬时错误，持久错误立刻上抛。
async fn pull_with_retry<I, O>(
  cloud: &CloudStore<I, O>,
  tbl: &TableIdent,
  file_id: FileId,
  segment: bool,
) -> Result<bool>
where
  I: FileIo + ManifestIo + PageIo + SegmentIo,
  O: ObjectStore,
{
  let mut attempt = 1;
  let mut delay_ms = PULL_RETRY_INIT_DELAY_MS;
  loop {
    let pulled = if segment {
      cloud.prewarm_segment_file(tbl, file_id).await
    } else {
      cloud.prewarm_data_file(tbl, file_id).await
    };
    let err = match pulled {
      Ok(found) => return Ok(found),
      Err(err) => err,
    };
    if !err.is_retryable() || attempt >= PULL_RETRY_TIMES {
      return Err(err);
    }
    Delay::new(Duration::from_millis(delay_ms)).await;
    delay_ms = (delay_ms * 2).min(PULL_RETRY_MAX_DELAY_MS);
    attempt += 1;
  }
}

/// 预热单个 table。先 manifest，再 data，最后 segment；磁盘不足即中止。
pub async fn prewarm_table<I, O>(cloud: &CloudStore<I, O>, tbl: &TableIdent) -> Result<PrewarmStat>
where
  I: FileIo + ManifestIo + PageIo + SegmentIo,
  O: ObjectStore,
{
  let started = Instant::now();
  let mut stats = PrewarmStat {
    manifest_present: cloud.prewarm_manifest(tbl).await?,
    ..PrewarmStat::default()
  };
  if !stats.manifest_present {
    // 远端没有 manifest：该表已被 Drop，data / segment 对象只是残留。继续按
    // 目录列举整表拉回等于复活已删除的表（`prewarm_manifest` 已清本地 manifest
    // 与 per-file 归属链）。直接返回，不进入 data / segment 阶段。
    stats.missing_num += 1;
    stats.files_skipped_missing = stats.missing_num;
    stats.duration_ms = started.elapsed().as_millis() as u64;
    return Ok(stats);
  }
  stats.total_files_listed = usize::from(stats.manifest_present);
  prewarm_paged(cloud, tbl, false, &mut stats).await;
  // data 阶段未走完（失败或磁盘不足）时不再继续 segment，与既有语义一致。
  if stats.completion_reason == CompletionReason::Success {
    prewarm_paged(cloud, tbl, true, &mut stats).await;
  }
  stats.files_skipped_missing = stats.missing_num;
  stats.total_files_pulled =
    stats.data_file_num + stats.segment_file_num + usize::from(stats.manifest_present);
  stats.duration_ms = started.elapsed().as_millis() as u64;
  Ok(stats)
}

pub async fn prewarm_tables<I, O>(
  cloud: &CloudStore<I, O>,
  table_li: &[TableIdent],
) -> Result<Vec<(TableIdent, PrewarmStat)>>
where
  I: FileIo + ManifestIo + PageIo + SegmentIo,
  O: ObjectStore,
{
  prewarm_tables_with_concurrency(cloud, table_li, PREWARM_TABLE_CONCURRENCY).await
}

/// 同 [`prewarm_tables`]，但表级并发可配（对齐 `KvOption::prewarm_task_count`）。
/// `table_concurrency == 0` 时按 1 串行，避免 `buffer_unordered(0)` 永不推进。
pub async fn prewarm_tables_with_concurrency<I, O>(
  cloud: &CloudStore<I, O>,
  table_li: &[TableIdent],
  table_concurrency: usize,
) -> Result<Vec<(TableIdent, PrewarmStat)>>
where
  I: FileIo + ManifestIo + PageIo + SegmentIo,
  O: ObjectStore,
{
  let concurrency = table_concurrency.max(1);
  let result_li: Vec<Result<(TableIdent, PrewarmStat)>> = stream::iter(table_li)
    .map(|tbl| async move {
      let stats = prewarm_table(cloud, tbl).await?;
      Ok((tbl.clone(), stats))
    })
    .buffer_unordered(concurrency)
    .collect()
    .await;
  result_li.into_iter().collect()
}
