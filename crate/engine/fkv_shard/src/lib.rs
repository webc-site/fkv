//! 1 shard = 1 OS thread + 1 compio Runtime + 1 async request queue。

use std::{
  collections::HashSet,
  path::PathBuf,
  sync::Arc,
  thread::{self, JoinHandle},
  time::{Duration, Instant},
};

use compio::{runtime::Runtime, time::sleep};
use crossfire::{
  MAsyncTx,
  mpmc::{Array, bounded_async},
};
use fkv_api::{
  ArchiveReply, BranchReply, FloorReply, LargeValueReply, Request, Response, ScanReply,
};
use fkv_error::{KvError, Result};
use fkv_io::{FixedBuffer, FixedSegmentIo};
use fkv_io_compio::CompioStore;
use fkv_large::SegmentLayout;
use fkv_metrics::{MeteredIo, Op, ShardMetrics, ShardSnapshot};
use fkv_store::{ScanOptions, Store};
use fkv_task_reopen::{AutoReopenOptions, Sleeper};
use fkv_types::{TableIdent, WriteOp};
use futures_channel::oneshot;
use log::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct ShardConfig {
  pub shard_id: u32,
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
}

struct Envelope {
  req: Request,
  reply: oneshot::Sender<Result<Response>>,
}

#[derive(Clone)]
pub struct ShardSender {
  tx: MAsyncTx<Array<Envelope>>,
}

impl ShardSender {
  pub async fn request(&self, req: Request) -> Result<Response> {
    let (reply, rx) = oneshot::channel();
    self
      .tx
      .send(Envelope { req, reply })
      .await
      .map_err(|_| KvError::NotRunning)?;
    rx.await.map_err(|_| KvError::NotRunning)?
  }
}

pub struct Shard {
  tx: MAsyncTx<Array<Envelope>>,
  join: Option<JoinHandle<()>>,
  metrics: Arc<ShardMetrics>,
}

impl Shard {
  pub fn start(config: ShardConfig) -> Result<Self> {
    if config.queue_size == 0 {
      return Err(KvError::InvalidArgs);
    }
    let layout = SegmentLayout::new(config.segment_size, config.segments_per_file_shift)?;
    let (tx, rx) = bounded_async::<Envelope>(config.queue_size);
    let page_size = config.page_size;
    let restart_interval = config.restart_interval;
    let large_value_threshold = if config.large_value_threshold == 0 {
      page_size / 2
    } else {
      config.large_value_threshold
    };
    let metrics = Arc::new(ShardMetrics::new(config.shard_id));
    let thread_metrics = metrics.clone();
    let join = thread::Builder::new()
      .name("fkv-shard".to_owned())
      .spawn(move || {
        let Ok(runtime) = Runtime::new() else {
          error!(
            target: "fkv_shard",
            "shard runtime init failed shard_id={}",
            config.shard_id
          );
          return;
        };
        info!(
          target: "fkv_shard",
          "shard start shard_id={} root={} page_size={} pages_per_file_shift={}",
          config.shard_id,
          config.root.display(),
          config.page_size,
          config.pages_per_file_shift
        );
        runtime.block_on(async move {
          let io = MeteredIo::new(
            CompioStore::new(config.root, config.page_size, config.pages_per_file_shift)
              .with_segment_config(config.segment_size as usize, config.segments_per_file_shift),
            thread_metrics.clone(),
          );
          let fixed_buffer = if config.fixed_buffer_size > 0 {
            match io.register_fixed_buffer(config.fixed_buffer_size) {
              Ok(buf) => {
                info!(
                  target: "fkv_shard",
                  "shard fixed buffer shard_id={} len={} registered={}",
                  config.shard_id,
                  buf.len(),
                  buf.is_registered()
                );
                Some(buf)
              }
              Err(err) => {
                warn!(
                  target: "fkv_shard",
                  "shard fixed buffer register failed shard_id={} error={}",
                  config.shard_id,
                  err
                );
                None
              }
            }
          } else {
            None
          };
          let mut state = ShardState {
            store: Store::new(io, page_size, restart_interval)
              .with_large_value(layout, large_value_threshold),
            fixed_buffer,
            opened_table_set: HashSet::new(),
            auto_reopen_options: AutoReopenOptions::new(
              config.auto_reopen_retry_times,
              Duration::from_micros(config.auto_reopen_pending_time_us),
            ),
            attached: false,
            term: None,
            branch: String::new(),
            shard_id: 0,
          };
          while let Ok(envelope) = rx.recv().await {
            let shutdown = envelope.req.is_shutdown();
            let op = op_of(&envelope.req);
            let started = Instant::now();
            let result = handle(&mut state, envelope.req).await;
            if let Some(op) = op {
              let elapsed = started.elapsed();
              thread_metrics.observe(op, elapsed, result.is_err());
              let latency_us = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
              match &result {
                Ok(_) => debug!(
                  target: "fkv_shard",
                  "request shard_id={} op={} status=ok latency_us={}",
                  thread_metrics.shard_id(),
                  op.as_str(),
                  latency_us
                ),
                Err(err) => warn!(
                  target: "fkv_shard",
                  "request shard_id={} op={} status=error error={} latency_us={}",
                  thread_metrics.shard_id(),
                  op.as_str(),
                  err,
                  latency_us
                ),
              }
            }
            let _ = envelope.reply.send(result);
            if shutdown {
              break;
            }
          }
          info!(
            target: "fkv_shard",
            "shard stop shard_id={}",
            thread_metrics.shard_id()
          );
        });
      })
      .map_err(|_| KvError::IoFail)?;
    Ok(Self {
      tx,
      join: Some(join),
      metrics,
    })
  }

  pub fn metrics(&self) -> &Arc<ShardMetrics> {
    &self.metrics
  }

  pub fn metrics_snapshot(&self) -> ShardSnapshot {
    self.metrics.snapshot()
  }

  pub async fn request(&self, req: Request) -> Result<Response> {
    self.sender().request(req).await
  }

  pub fn sender(&self) -> ShardSender {
    ShardSender {
      tx: self.tx.clone(),
    }
  }

  pub async fn shutdown(mut self) -> Result<()> {
    let _ = self.request(Request::Shutdown).await;
    if let Some(join) = self.join.take() {
      join.join().map_err(|_| KvError::IoFail)?;
    }
    Ok(())
  }
}

async fn ensure_table_ready(state: &mut ShardState, tbl: &TableIdent) -> Result<()> {
  if state.opened_table_set.contains(tbl) {
    return Ok(());
  }
  let branch = state.branch.clone();
  let term = state.term.unwrap_or(0);
  if branch == "main" {
    match state.store.reopen(tbl).await {
      Ok(()) | Err(KvError::ResourceMissing) => {}
      Err(err) => return Err(err),
    }
  } else {
    state.store.open_branch(tbl, &branch, 0).await?;
  }
  state.store.set_term(term);
  state.opened_table_set.insert(tbl.clone());
  Ok(())
}

struct CompioSleeper;

impl Sleeper for CompioSleeper {
  async fn sleep(&self, duration: Duration) {
    sleep(duration).await;
  }
}

fn op_of(req: &Request) -> Option<Op> {
  match req {
    Request::Put { .. } | Request::PutLarge { .. } => Some(Op::Put),
    Request::Get { .. } | Request::GetLarge { .. } => Some(Op::Get),
    Request::Scan { .. } => Some(Op::Scan),
    Request::Reopen { .. } => Some(Op::Reopen),
    Request::Gc { .. } => Some(Op::Gc),
    Request::Compact { .. } => Some(Op::Compact),
    Request::Truncate { .. } => Some(Op::Truncate),
    Request::Floor { .. } => Some(Op::Floor),
    Request::Drop { .. } => Some(Op::Drop),
    Request::CreateArchive { .. } => Some(Op::CreateArchive),
    Request::CreateBranch { .. } => Some(Op::CreateBranch),
    Request::DeleteBranch { .. } => Some(Op::DeleteBranch),
    Request::AttachShard { .. } => Some(Op::AttachShard),
    Request::DetachShard { .. } => Some(Op::DetachShard),
    Request::Shutdown => Some(Op::Shutdown),
  }
}

struct ShardState {
  store: Store<MeteredIo<CompioStore>>,
  fixed_buffer: Option<FixedBuffer>,
  opened_table_set: HashSet<TableIdent>,
  auto_reopen_options: AutoReopenOptions,
  attached: bool,
  term: Option<u64>,
  branch: String,
  shard_id: u32,
}

async fn handle(state: &mut ShardState, req: Request) -> Result<Response> {
  if !matches!(
    req,
    Request::AttachShard { .. } | Request::DetachShard { .. } | Request::Shutdown
  ) && !state.attached
  {
    return Err(KvError::NotRunning);
  }
  if let Some(tbl) = req.table().cloned() {
    ensure_table_ready(state, &tbl).await?;
  }
  match req {
    Request::Put { tbl, batch, term } => {
      if let Some(term) = term
        && state.term != Some(term)
      {
        return Err(KvError::ExpiredTerm);
      }
      state.store.put(&tbl, &batch).await?;
      Ok(Response::Put)
    }
    Request::Get { tbl, key } => Ok(Response::Get(state.store.get(&tbl, &key).await?)),
    Request::PutLarge { tbl, batch, term } => {
      if let Some(term) = term
        && state.term != Some(term)
      {
        return Err(KvError::ExpiredTerm);
      }
      let ShardState {
        store,
        fixed_buffer,
        ..
      } = state;
      let max_len = batch
        .iter()
        .filter(|entry| entry.op == WriteOp::Upsert)
        .map(|entry| entry.value.len())
        .max()
        .unwrap_or(0);
      if let Some(buf) = fixed_buffer.as_mut()
        && max_len <= buf.len()
      {
        store.put_large_fixed(&tbl, &batch, buf).await?;
      } else {
        store.put_large(&tbl, &batch).await?;
      }
      Ok(Response::Put)
    }
    Request::GetLarge { tbl, key } => {
      let ShardState {
        store,
        fixed_buffer,
        ..
      } = state;
      let value = if let Some(buf) = fixed_buffer.as_mut() {
        match store.get_large_into(&tbl, &key, buf).await {
          Ok(value) => value.map(|into| LargeValueReply {
            value: buf.as_slice()[..into.value_len].to_vec(),
            metadata: into.metadata,
          }),
          Err(KvError::InvalidArgs) => {
            store
              .get_large(&tbl, &key)
              .await?
              .map(|large| LargeValueReply {
                value: large.value,
                metadata: large.metadata,
              })
          }
          Err(err) => return Err(err),
        }
      } else {
        store
          .get_large(&tbl, &key)
          .await?
          .map(|large| LargeValueReply {
            value: large.value,
            metadata: large.metadata,
          })
      };
      Ok(Response::GetLarge(value))
    }
    Request::Scan {
      tbl,
      begin,
      end,
      max_entries,
      begin_inclusive,
      end_inclusive,
      page_entries,
      page_size,
    } => {
      let page = state
        .store
        .scan_page(
          &tbl,
          &begin,
          &end,
          fkv_store::ScanOptions {
            begin_inclusive,
            end_inclusive,
            ..ScanOptions::default()
          },
          page_entries.max(max_entries),
          page_size,
        )
        .await?;
      Ok(Response::Scan(ScanReply {
        entry_li: page.entry_li,
        has_remaining: page.has_remaining,
      }))
    }
    Request::Reopen { tbl } => {
      let ShardState {
        store,
        auto_reopen_options,
        ..
      } = state;
      store
        .auto_reopen_with_sleeper(&tbl, *auto_reopen_options, &CompioSleeper)
        .await?;
      Ok(Response::Reopen)
    }
    Request::Gc {
      tbl,
      pages_per_file_shift,
    } => Ok(Response::Gc(
      state.store.gc(&tbl, pages_per_file_shift).await?,
    )),
    Request::Compact {
      tbl,
      file_id,
      pages_per_file_shift,
    } => Ok(Response::Compact(
      state
        .store
        .compact_file(&tbl, file_id, pages_per_file_shift)
        .await?,
    )),
    Request::Truncate { tbl, position } => {
      if position.is_empty() {
        state.store.truncate(&tbl).await?;
      } else {
        state.store.truncate_from(&tbl, &position).await?;
      }
      Ok(Response::Truncate)
    }
    Request::Floor { tbl, key } => {
      let floor = state.store.floor(&tbl, &key).await?;
      Ok(Response::Floor(FloorReply {
        key: floor.key,
        value: floor.value,
        metadata: floor.metadata,
        timestamp: floor.timestamp,
        expire_ts: floor.expire_ts,
      }))
    }
    Request::Drop { tbl } => {
      state.store.drop_table(&tbl).await?;
      Ok(Response::Drop)
    }
    Request::CreateArchive {
      tbl,
      tag,
      num_retained,
    } => {
      let info = state.store.create_archive(&tbl, &tag, num_retained).await?;
      Ok(Response::CreateArchive(ArchiveReply {
        name: info.name,
        bytes: info.bytes,
        removed_num: info.removed_num,
      }))
    }
    Request::CreateBranch { tbl, branch } => {
      let info = state.store.create_branch(&tbl, &branch).await?;
      Ok(Response::CreateBranch(BranchReply {
        name: info.name,
        bytes: info.bytes,
      }))
    }
    Request::DeleteBranch { tbl, branch } => {
      let removed = state.store.delete_branch(&tbl, &branch).await?;
      Ok(Response::DeleteBranch(removed))
    }
    Request::AttachShard {
      branch,
      shard_id,
      term,
    } => {
      if state.branch == branch && state.term.is_some_and(|current| current > term) {
        return Err(KvError::ExpiredTerm);
      }
      info!(
        target: "fkv_shard",
        "shard attach shard_id={} branch={} term={}",
        shard_id,
        branch,
        term
      );
      state.store.prepare_attach(&branch, term)?;
      state.opened_table_set.clear();
      state.attached = true;
      state.term = Some(term);
      state.branch = branch;
      state.shard_id = shard_id;
      Ok(Response::AttachShard)
    }
    Request::DetachShard { shard_id } => {
      if state.shard_id == shard_id {
        // 保留 branch / term：同一 branch 以旧 term 重新 attach 必须被拒绝。
        state.attached = false;
        info!(
          target: "fkv_shard",
          "shard detach shard_id={}",
          shard_id
        );
      }
      Ok(Response::DetachShard)
    }
    Request::Shutdown => Ok(Response::Shutdown),
  }
}
