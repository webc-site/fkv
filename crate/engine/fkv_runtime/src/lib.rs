//! Thread-per-core runtime：N 个 Shard，按 TableIdent 路由请求。

use std::{
  collections::HashSet,
  mem::take,
  path::PathBuf,
  sync::{
    Arc, Condvar, Mutex, MutexGuard,
    atomic::{AtomicBool, Ordering},
  },
  thread::{self, JoinHandle},
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use fkv_api::{
  ArchiveReply, BranchReply, FloorReply, LargeValueReply, Request, Response, ScanPageOptions,
  ScanReply,
};
use fkv_error::{KvError, Result};
use fkv_metrics::MetricsSnapshot;
use fkv_shard::{Shard, ShardConfig, ShardSender};
use fkv_types::{FileId, TableIdent};
use log::{error, info, warn};

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
  pub num_threads: u16,
  pub root: PathBuf,
  pub page_size: usize,
  pub restart_interval: u16,
  pub pages_per_file_shift: u8,
  pub segment_size: u32,
  pub segments_per_file_shift: u8,
  pub large_value_threshold: usize,
  pub fixed_buffer_size: usize,
  pub queue_size: usize,
  pub auto_reopen_retry_times: u8,
  pub auto_reopen_pending_time_us: u64,
  /// 0 表示关闭定时归档。
  pub archive_interval_secs: u64,
  pub archive_num_retained: u32,
}

struct ArchiveState {
  stop: Mutex<bool>,
  wake: Condvar,
  attached: Vec<AtomicBool>,
  tables: Mutex<HashSet<TableIdent>>,
  interval: Duration,
  num_retained: u32,
}

impl ArchiveState {
  fn new(shard_num: u16, interval: Duration, num_retained: u32) -> Self {
    Self {
      stop: Mutex::new(false),
      wake: Condvar::new(),
      attached: (0..shard_num).map(|_| AtomicBool::new(false)).collect(),
      tables: Mutex::new(HashSet::new()),
      interval,
      num_retained,
    }
  }
}

struct ArchiveCrond {
  state: Arc<ArchiveState>,
  join: Option<JoinHandle<()>>,
}

impl ArchiveCrond {
  fn start(state: Arc<ArchiveState>, sender_li: Vec<ShardSender>) -> Result<Self> {
    let thread_state = state.clone();
    let join = thread::Builder::new()
      .name("fkv-archive".to_owned())
      .spawn(move || Self::run(thread_state, sender_li))
      .map_err(|_| KvError::IoFail)?;
    Ok(Self {
      state,
      join: Some(join),
    })
  }

  fn run(state: Arc<ArchiveState>, sender_li: Vec<ShardSender>) {
    loop {
      let should_stop = {
        let stop = lock_mutex(&state.stop);
        match state.wake.wait_timeout(stop, state.interval) {
          Ok((guard, _)) => *guard,
          Err(poisoned) => *poisoned.into_inner().0,
        }
      };
      if should_stop {
        break;
      }
      let table_li: Vec<TableIdent> = lock_mutex(&state.tables).iter().cloned().collect();
      if table_li.is_empty() {
        continue;
      }
      let tag = archive_tag();
      let shard_num = sender_li.len() as u16;
      let mut archived = 0usize;
      for (shard_id, sender) in sender_li.iter().enumerate() {
        if !state.attached[shard_id].load(Ordering::Relaxed) {
          continue;
        }
        for tbl in &table_li {
          if usize::from(tbl.shard_index(shard_num)) != shard_id {
            continue;
          }
          let request = Request::CreateArchive {
            tbl: tbl.clone(),
            tag: tag.clone(),
            num_retained: state.num_retained,
          };
          match pollster::block_on(sender.request(request)) {
            Ok(_) => archived += 1,
            Err(KvError::NotRunning) => {}
            Err(err) => warn!(
              target: "fkv_runtime",
              "archive crond failed table={} tag={} error={}",
              tbl,
              tag,
              err
            ),
          }
        }
      }
      if archived > 0 {
        info!(
          target: "fkv_runtime",
          "archive crond run tables={} archived={} tag={}",
          table_li.len(),
          archived,
          tag
        );
      }
    }
  }

  fn stop(&mut self) {
    *lock_mutex(&self.state.stop) = true;
    self.state.wake.notify_all();
    if let Some(join) = self.join.take() {
      let _ = join.join();
    }
  }
}

impl Drop for ArchiveCrond {
  fn drop(&mut self) {
    self.stop();
  }
}

fn lock_mutex<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
  match mutex.lock() {
    Ok(guard) => guard,
    Err(poisoned) => poisoned.into_inner(),
  }
}

fn archive_tag() -> String {
  let now_ms = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
    .unwrap_or(0);
  format!("{now_ms:020}")
}

pub struct Runtime {
  shard_li: Vec<Shard>,
  archive_crond: Option<ArchiveCrond>,
  archive_state: Option<Arc<ArchiveState>>,
}

impl Runtime {
  pub fn start(config: RuntimeConfig) -> Result<Self> {
    if config.num_threads == 0 {
      error!(target: "fkv_runtime", "runtime start rejected: num_threads=0");
      return Err(KvError::InvalidArgs);
    }
    info!(
      target: "fkv_runtime",
      "runtime start threads={} root={} page_size={} pages_per_file_shift={}",
      config.num_threads,
      config.root.display(),
      config.page_size,
      config.pages_per_file_shift
    );
    let mut shard_li = Vec::with_capacity(usize::from(config.num_threads));
    for shard_id in 0..config.num_threads {
      let shard = Shard::start(ShardConfig {
        shard_id: u32::from(shard_id),
        root: config.root.clone(),
        page_size: config.page_size,
        restart_interval: config.restart_interval,
        pages_per_file_shift: config.pages_per_file_shift,
        segment_size: config.segment_size,
        segments_per_file_shift: config.segments_per_file_shift,
        large_value_threshold: config.large_value_threshold,
        fixed_buffer_size: config.fixed_buffer_size,
        queue_size: config.queue_size,
        auto_reopen_retry_times: config.auto_reopen_retry_times,
        auto_reopen_pending_time_us: config.auto_reopen_pending_time_us,
      })
      .inspect_err(|err| {
        error!(
          target: "fkv_runtime",
          "runtime shard start failed shard_id={} error={}",
          shard_id,
          err
        );
      })?;
      shard_li.push(shard);
    }
    let archive_state = (config.archive_interval_secs > 0).then(|| {
      Arc::new(ArchiveState::new(
        config.num_threads,
        Duration::from_secs(config.archive_interval_secs),
        config.archive_num_retained,
      ))
    });
    let archive_crond = if let Some(state) = &archive_state {
      let sender_li: Vec<ShardSender> = shard_li.iter().map(Shard::sender).collect();
      match ArchiveCrond::start(state.clone(), sender_li) {
        Ok(crond) => {
          info!(
            target: "fkv_runtime",
            "archive crond start interval_secs={} retain={}",
            config.archive_interval_secs,
            config.archive_num_retained
          );
          Some(crond)
        }
        Err(err) => {
          error!(target: "fkv_runtime", "archive crond start failed error={}", err);
          None
        }
      }
    } else {
      None
    };
    Ok(Self {
      shard_li,
      archive_crond,
      archive_state,
    })
  }

  pub fn num_threads(&self) -> u16 {
    self.shard_li.len() as u16
  }

  pub fn metrics(&self) -> MetricsSnapshot {
    MetricsSnapshot {
      shard_li: self.shard_li.iter().map(Shard::metrics_snapshot).collect(),
    }
  }

  pub fn shard_metrics(&self, shard_id: u32) -> Option<fkv_metrics::ShardSnapshot> {
    self
      .shard_li
      .get(shard_id as usize)
      .map(Shard::metrics_snapshot)
  }

  fn shard(&self, tbl: &TableIdent) -> &Shard {
    let index = usize::from(tbl.shard_index(self.num_threads()));
    &self.shard_li[index]
  }

  fn shard_by_id(&self, shard_id: u32) -> Result<&Shard> {
    self
      .shard_li
      .get(shard_id as usize)
      .ok_or(KvError::InvalidArgs)
  }

  pub async fn attach_shard(&self, shard_id: u32, branch: &str, term: u64) -> Result<()> {
    info!(
      target: "fkv_runtime",
      "attach shard shard_id={} branch={} term={}",
      shard_id,
      branch,
      term
    );
    match self
      .shard_by_id(shard_id)?
      .request(Request::AttachShard {
        branch: branch.to_owned(),
        shard_id,
        term,
      })
      .await?
    {
      Response::AttachShard => {
        if let Some(state) = &self.archive_state {
          state.attached[shard_id as usize].store(true, Ordering::Relaxed);
        }
        Ok(())
      }
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn detach_shard(&self, shard_id: u32) -> Result<()> {
    info!(
      target: "fkv_runtime",
      "detach shard shard_id={}",
      shard_id
    );
    match self
      .shard_by_id(shard_id)?
      .request(Request::DetachShard { shard_id })
      .await?
    {
      Response::DetachShard => {
        if let Some(state) = &self.archive_state {
          state.attached[shard_id as usize].store(false, Ordering::Relaxed);
        }
        Ok(())
      }
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn request(&self, tbl: &TableIdent, req: Request) -> Result<Response> {
    if let Some(state) = &self.archive_state
      && let Ok(mut table_set) = state.tables.lock()
    {
      table_set.insert(tbl.clone());
    }
    self.shard(tbl).request(req).await
  }

  pub async fn put(&self, tbl: &TableIdent, batch: Vec<fkv_btree::BatchEntry>) -> Result<()> {
    match self
      .request(
        tbl,
        Request::Put {
          tbl: tbl.clone(),
          batch,
          term: None,
        },
      )
      .await?
    {
      Response::Put => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn put_with_term(
    &self,
    tbl: &TableIdent,
    batch: Vec<fkv_btree::BatchEntry>,
    term: u64,
  ) -> Result<()> {
    match self
      .request(
        tbl,
        Request::Put {
          tbl: tbl.clone(),
          batch,
          term: Some(term),
        },
      )
      .await?
    {
      Response::Put => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn put_large(&self, tbl: &TableIdent, batch: Vec<fkv_btree::BatchEntry>) -> Result<()> {
    match self
      .request(
        tbl,
        Request::PutLarge {
          tbl: tbl.clone(),
          batch,
          term: None,
        },
      )
      .await?
    {
      Response::Put => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn get_large(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<LargeValueReply>> {
    match self
      .request(
        tbl,
        Request::GetLarge {
          tbl: tbl.clone(),
          key: key.to_vec(),
        },
      )
      .await?
    {
      Response::GetLarge(value) => Ok(value),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn read(&self, tbl: &TableIdent, key: &[u8]) -> Result<Vec<u8>> {
    self.get(tbl, key).await?.ok_or(KvError::NotFound)
  }

  pub async fn floor(&self, tbl: &TableIdent, key: &[u8]) -> Result<FloorReply> {
    match self
      .request(
        tbl,
        Request::Floor {
          tbl: tbl.clone(),
          key: key.to_vec(),
        },
      )
      .await?
    {
      Response::Floor(reply) => Ok(reply),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn get(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<Vec<u8>>> {
    match self
      .request(
        tbl,
        Request::Get {
          tbl: tbl.clone(),
          key: key.to_vec(),
        },
      )
      .await?
    {
      Response::Get(value) => Ok(value),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn scan(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    match self
      .request(
        tbl,
        Request::Scan {
          tbl: tbl.clone(),
          begin: begin.to_vec(),
          end: end.to_vec(),
          max_entries,
          begin_inclusive: true,
          end_inclusive: false,
          page_entries: 0,
          page_size: 0,
        },
      )
      .await?
    {
      Response::Scan(reply) => Ok(reply.entry_li),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn scan_page(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    options: ScanPageOptions,
  ) -> Result<ScanReply> {
    match self
      .request(
        tbl,
        Request::Scan {
          tbl: tbl.clone(),
          begin: begin.to_vec(),
          end: end.to_vec(),
          max_entries: 0,
          begin_inclusive: options.begin_inclusive,
          end_inclusive: options.end_inclusive,
          page_entries: options.page_entries,
          page_size: options.page_size,
        },
      )
      .await?
    {
      Response::Scan(reply) => Ok(reply),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn scan_opts(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    begin_inclusive: bool,
    end_inclusive: bool,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    match self
      .request(
        tbl,
        Request::Scan {
          tbl: tbl.clone(),
          begin: begin.to_vec(),
          end: end.to_vec(),
          max_entries,
          begin_inclusive,
          end_inclusive,
          page_entries: 0,
          page_size: 0,
        },
      )
      .await?
    {
      Response::Scan(reply) => Ok(reply.entry_li),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn reopen(&self, tbl: &TableIdent) -> Result<()> {
    match self
      .request(tbl, Request::Reopen { tbl: tbl.clone() })
      .await?
    {
      Response::Reopen => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn gc(&self, tbl: &TableIdent, pages_per_file_shift: u8) -> Result<usize> {
    match self
      .request(
        tbl,
        Request::Gc {
          tbl: tbl.clone(),
          pages_per_file_shift,
        },
      )
      .await?
    {
      Response::Gc(count) => Ok(count),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn compact(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    pages_per_file_shift: u8,
  ) -> Result<usize> {
    match self
      .request(
        tbl,
        Request::Compact {
          tbl: tbl.clone(),
          file_id,
          pages_per_file_shift,
        },
      )
      .await?
    {
      Response::Compact(count) => Ok(count),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn truncate(&self, tbl: &TableIdent) -> Result<()> {
    self.truncate_from(tbl, b"").await
  }

  pub async fn truncate_from(&self, tbl: &TableIdent, position: &[u8]) -> Result<()> {
    match self
      .request(
        tbl,
        Request::Truncate {
          tbl: tbl.clone(),
          position: position.to_vec(),
        },
      )
      .await?
    {
      Response::Truncate => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn drop_table(&self, tbl: &TableIdent) -> Result<()> {
    match self
      .request(tbl, Request::Drop { tbl: tbl.clone() })
      .await?
    {
      Response::Drop => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn create_archive(
    &self,
    tbl: &TableIdent,
    tag: &str,
    num_retained: u32,
  ) -> Result<ArchiveReply> {
    match self
      .request(
        tbl,
        Request::CreateArchive {
          tbl: tbl.clone(),
          tag: tag.to_owned(),
          num_retained,
        },
      )
      .await?
    {
      Response::CreateArchive(reply) => Ok(reply),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn create_branch(&self, tbl: &TableIdent, branch: &str) -> Result<BranchReply> {
    match self
      .request(
        tbl,
        Request::CreateBranch {
          tbl: tbl.clone(),
          branch: branch.to_owned(),
        },
      )
      .await?
    {
      Response::CreateBranch(reply) => Ok(reply),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn delete_branch(&self, tbl: &TableIdent, branch: &str) -> Result<usize> {
    match self
      .request(
        tbl,
        Request::DeleteBranch {
          tbl: tbl.clone(),
          branch: branch.to_owned(),
        },
      )
      .await?
    {
      Response::DeleteBranch(removed) => Ok(removed),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn shutdown(mut self) -> Result<()> {
    info!(
      target: "fkv_runtime",
      "runtime shutdown begin threads={}",
      self.shard_li.len()
    );
    if let Some(mut crond) = self.archive_crond.take() {
      crond.stop();
      info!(target: "fkv_runtime", "archive crond stopped");
    }
    for shard in take(&mut self.shard_li) {
      shard.shutdown().await?;
    }
    info!(target: "fkv_runtime", "runtime shutdown complete");
    Ok(())
  }
}

impl Drop for Runtime {
  fn drop(&mut self) {
    if let Some(mut crond) = self.archive_crond.take() {
      crond.stop();
    }
  }
}
