//! 确定性模拟 / 故障 / 掉电后端。
//!
//! `SimStore` 自持存储（不包装 [`fkv_io_mem::MemStore`]），把写分成两层：
//!
//! - `volatile`：尚未 `sync_data` 的**数据类**写入（page / segment，含删除墓碑）。
//! - `durable`：已 `sync_data` 的数据写入，以及全部 manifest / term 写入。
//!
//! 真实后端（compio）的 manifest 写路径自带 fdatasync / 目录 fsync，写成功即 durable；
//! [`SimStore::sync_data`] 只刷 page / segment，绝不顺带提升 manifest，否则会掩盖
//! 「manifest 未按后端契约落盘」的缺陷。读走「volatile 覆盖 durable」的合并视图；
//! [`SimStore::sync_data`] 把 volatile 合并进 durable 并清空；[`SimStore::crash`]
//! 直接丢弃 volatile，用来模拟掉电丢失未 fsync 数据。
//! 逻辑时钟按 op 自增，全程不读墙钟、不起线程、不 sleep，因此同一配置跑两次逐字节可复现。
//!
//! 仅用于测试 / V5 崩溃恢复 / 崩溃 fuzz 场景，不进生产依赖图。

use core::{cmp::Ordering, hash::Hash};
use std::sync::{Mutex, MutexGuard};

use fastrand::Rng;
use fkv_error::{KvError, Result};
use fkv_failpoint::points::{
  APPEND_SYNC, APPEND_WRITE, ATOMIC_RENAME, SYNC_DATA, WRITE_FILE_RENAME, WRITE_PAGE,
  WRITE_SEGMENTS,
};
use fkv_io::{FileIo, FixedBuffer, FixedSegmentIo, ManifestIo, PageIo, SegmentIo, TermIo};
use fkv_types::{
  DEFAULT_PAGES_PER_FILE_SHIFT, DEFAULT_SEGMENT_SIZE, DEFAULT_SEGMENTS_PER_FILE_SHIFT, FileId,
  FilePageId, TableIdent,
};
use gxhash::{HashMap as GxHashMap, HashSet as GxHashSet};

pub mod oplog;

type PageKey = (TableIdent, FilePageId);
type ManifestFileKey = (TableIdent, String);
type TermKey = (String, u32);

type PageMap = GxHashMap<PageKey, Vec<u8>>;
type PageOverlay = GxHashMap<PageKey, Option<Vec<u8>>>;
type ManifestMap = GxHashMap<TableIdent, Vec<u8>>;
type ManifestFileMap = GxHashMap<ManifestFileKey, Vec<u8>>;
type TermMap = GxHashMap<TermKey, u64>;

/// 逻辑操作种类（供 `delay_li` / `fault_li` 按「第 N 次该操作」匹配）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpKind {
  ReadPage,
  WritePage,
  SyncData,
  FlushPage,
  ResetManifest,
  ResetManifestFile,
  AppendManifest,
  AppendManifestFile,
  SwitchManifest,
  ReadManifest,
  DropManifest,
  WriteManifestFile,
  ListManifestFileNameLi,
  ListSidecarFileNameLi,
  ReadManifestFile,
  RemoveManifestFile,
  RemoveSidecarFileName,
  ReadTerm,
  UpsertTerm,
  ReadSegment,
  WriteSegment,
  FlushSegment,
  ReadSegmentFixed,
  WriteSegmentFixed,
  RegisterFixedBuffer,
  ListFileIdLi,
  ReadFile,
  WriteFile,
  FileSize,
  RemoveFile,
  ListSegmentFileIdLi,
  RemoveSegmentFile,
  Crash,
}

/// 一次逻辑操作的参数。用于 `op_log_li` 的确定性序列断言。
///
/// 与 [`OpKind`] 分开：`OpKind` 是可按「第 N 次」匹配的类别，`Op` 记录该次操作的具体参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
  ReadPage {
    tbl: TableIdent,
    file_page_id: FilePageId,
  },
  WritePage {
    tbl: TableIdent,
    file_page_id: FilePageId,
    len: usize,
  },
  SyncData {
    tbl: TableIdent,
  },
  /// `sync_data` 内部按刷写顺序逐页落盘（`reorder_seed` 打乱后仍确定性记录）。
  FlushPage {
    tbl: TableIdent,
    file_page_id: FilePageId,
  },
  ResetManifest {
    tbl: TableIdent,
    size: u64,
  },
  ResetManifestFile {
    tbl: TableIdent,
    name: String,
    size: u64,
  },
  AppendManifest {
    tbl: TableIdent,
    len: usize,
  },
  AppendManifestFile {
    tbl: TableIdent,
    name: String,
    len: usize,
  },
  SwitchManifest {
    tbl: TableIdent,
    len: usize,
  },
  ReadManifest {
    tbl: TableIdent,
  },
  DropManifest {
    tbl: TableIdent,
  },
  WriteManifestFile {
    tbl: TableIdent,
    name: String,
    len: usize,
  },
  ListManifestFileNameLi {
    tbl: TableIdent,
  },
  ListSidecarFileNameLi {
    tbl: TableIdent,
  },
  ReadManifestFile {
    tbl: TableIdent,
    name: String,
  },
  RemoveManifestFile {
    tbl: TableIdent,
    name: String,
  },
  RemoveSidecarFileName {
    tbl: TableIdent,
    name: String,
  },
  ReadTerm {
    branch: String,
    shard_id: u32,
  },
  UpsertTerm {
    branch: String,
    shard_id: u32,
    term: u64,
  },
  ReadSegment {
    tbl: TableIdent,
    segment_fp_id: FilePageId,
    len: usize,
  },
  WriteSegment {
    tbl: TableIdent,
    segment_fp_id: FilePageId,
    len: usize,
  },
  FlushSegment {
    tbl: TableIdent,
    segment_fp_id: FilePageId,
  },
  ReadSegmentFixed {
    tbl: TableIdent,
    segment_fp_id: FilePageId,
    offset: usize,
    len: usize,
  },
  WriteSegmentFixed {
    tbl: TableIdent,
    segment_fp_id: FilePageId,
    offset: usize,
    len: usize,
  },
  RegisterFixedBuffer {
    len: usize,
  },
  ListFileIdLi {
    tbl: TableIdent,
  },
  ReadFile {
    tbl: TableIdent,
    file_id: FileId,
  },
  WriteFile {
    tbl: TableIdent,
    file_id: FileId,
    len: usize,
  },
  FileSize {
    tbl: TableIdent,
    file_id: FileId,
  },
  RemoveFile {
    tbl: TableIdent,
    file_id: FileId,
  },
  ListSegmentFileIdLi {
    tbl: TableIdent,
  },
  RemoveSegmentFile {
    tbl: TableIdent,
    file_id: FileId,
  },
  Crash,
}

impl Op {
  /// 该操作对应的类别。
  pub fn kind(&self) -> OpKind {
    match self {
      Self::ReadPage { .. } => OpKind::ReadPage,
      Self::WritePage { .. } => OpKind::WritePage,
      Self::SyncData { .. } => OpKind::SyncData,
      Self::FlushPage { .. } => OpKind::FlushPage,
      Self::ResetManifest { .. } => OpKind::ResetManifest,
      Self::ResetManifestFile { .. } => OpKind::ResetManifestFile,
      Self::AppendManifest { .. } => OpKind::AppendManifest,
      Self::AppendManifestFile { .. } => OpKind::AppendManifestFile,
      Self::SwitchManifest { .. } => OpKind::SwitchManifest,
      Self::ReadManifest { .. } => OpKind::ReadManifest,
      Self::DropManifest { .. } => OpKind::DropManifest,
      Self::WriteManifestFile { .. } => OpKind::WriteManifestFile,
      Self::ListManifestFileNameLi { .. } => OpKind::ListManifestFileNameLi,
      Self::ListSidecarFileNameLi { .. } => OpKind::ListSidecarFileNameLi,
      Self::ReadManifestFile { .. } => OpKind::ReadManifestFile,
      Self::RemoveManifestFile { .. } => OpKind::RemoveManifestFile,
      Self::RemoveSidecarFileName { .. } => OpKind::RemoveSidecarFileName,
      Self::ReadTerm { .. } => OpKind::ReadTerm,
      Self::UpsertTerm { .. } => OpKind::UpsertTerm,
      Self::ReadSegment { .. } => OpKind::ReadSegment,
      Self::WriteSegment { .. } => OpKind::WriteSegment,
      Self::FlushSegment { .. } => OpKind::FlushSegment,
      Self::ReadSegmentFixed { .. } => OpKind::ReadSegmentFixed,
      Self::WriteSegmentFixed { .. } => OpKind::WriteSegmentFixed,
      Self::RegisterFixedBuffer { .. } => OpKind::RegisterFixedBuffer,
      Self::ListFileIdLi { .. } => OpKind::ListFileIdLi,
      Self::ReadFile { .. } => OpKind::ReadFile,
      Self::WriteFile { .. } => OpKind::WriteFile,
      Self::FileSize { .. } => OpKind::FileSize,
      Self::RemoveFile { .. } => OpKind::RemoveFile,
      Self::ListSegmentFileIdLi { .. } => OpKind::ListSegmentFileIdLi,
      Self::RemoveSegmentFile { .. } => OpKind::RemoveSegmentFile,
      Self::Crash => OpKind::Crash,
    }
  }
}

/// `op_log_li` 的一条记录：操作、发生时逻辑时钟、配置的逻辑延迟。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpRecord {
  pub op: Op,
  pub clock: u64,
  pub delay: u64,
  /// 写类操作的入参字节；仅在 `with_payload_capture` 打开时记录。
  pub payload: Option<Vec<u8>>,
}

/// 故障注入：第 `at` 次 `op` 返回 `err`，且该次不改变任何状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fault {
  pub op: OpKind,
  pub at: u32,
  pub err: KvError,
}

#[derive(Debug, Default)]
struct Config {
  delay_li: Vec<(OpKind, u64)>,
  fault_li: Vec<Fault>,
  reorder_seed: Option<u64>,
  capture_payload: bool,
}

/// 已 fsync 的一层状态。
#[derive(Debug, Default)]
struct Layer {
  page_map: PageMap,
  segment_map: PageMap,
  manifest_map: ManifestMap,
  manifest_file_map: ManifestFileMap,
  term_map: TermMap,
}

/// 未 fsync 的覆盖层：`None` 是删除墓碑。
#[derive(Debug, Default)]
struct Overlay {
  page_map: PageOverlay,
  segment_map: PageOverlay,
}

#[derive(Debug, Default)]
struct SimState {
  durable: Layer,
  volatile: Overlay,
  op_log_li: Vec<OpRecord>,
  op_count: GxHashMap<OpKind, u32>,
  clock: u64,
}

/// 盘面快照（排序去重后的逻辑视图），供「两次跑逐字节相等」断言。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
  pub page_li: Vec<(TableIdent, FilePageId, Vec<u8>)>,
  pub segment_li: Vec<(TableIdent, FilePageId, Vec<u8>)>,
  pub manifest_li: Vec<(TableIdent, Vec<u8>)>,
  pub manifest_file_li: Vec<(TableIdent, String, Vec<u8>)>,
  pub term_li: Vec<(String, u32, u64)>,
}

/// 确定性模拟 / 故障 / 掉电后端。
#[derive(Debug)]
pub struct SimStore {
  page_size: usize,
  pages_per_file_shift: u8,
  segment_size: usize,
  segments_per_file_shift: u8,
  config: Config,
  state: Mutex<SimState>,
}

impl SimStore {
  pub fn new(page_size: usize) -> Self {
    Self {
      page_size,
      pages_per_file_shift: DEFAULT_PAGES_PER_FILE_SHIFT,
      segment_size: DEFAULT_SEGMENT_SIZE,
      segments_per_file_shift: DEFAULT_SEGMENTS_PER_FILE_SHIFT,
      config: Config::default(),
      state: Mutex::new(SimState::default()),
    }
  }

  pub fn with_pages_per_file(mut self, page_size: usize, pages_per_file_shift: u8) -> Self {
    self.page_size = page_size;
    self.pages_per_file_shift = pages_per_file_shift;
    self
  }

  pub fn with_segment_config(mut self, segment_size: usize, segments_per_file_shift: u8) -> Self {
    self.segment_size = segment_size;
    self.segments_per_file_shift = segments_per_file_shift;
    self
  }

  /// 给某类操作记录逻辑延迟（只进 `op_log_li`，不 sleep）。
  pub fn with_delay(mut self, op: OpKind, delay: u64) -> Self {
    self.config.delay_li.push((op, delay));
    self
  }

  /// 注入故障：第 `fault.at` 次 `fault.op` 返回 `fault.err` 且不写状态。
  pub fn with_fault(mut self, fault: Fault) -> Self {
    self.config.fault_li.push(fault);
    self
  }

  /// 非空时 `sync_data` 按 `fastrand::Rng::with_seed` 打乱待刷写页/段顺序（仍确定性）。
  pub fn with_reorder_seed(mut self, seed: u64) -> Self {
    self.config.reorder_seed = Some(seed);
    self
  }

  /// 在 `op_log_li` 中记录写类操作的 payload（供文本 op log 回放 / 诊断）。
  pub fn with_payload_capture(mut self) -> Self {
    self.config.capture_payload = true;
    self
  }

  pub fn page_size(&self) -> usize {
    self.page_size
  }

  pub fn pages_per_file_shift(&self) -> u8 {
    self.pages_per_file_shift
  }

  pub fn segment_size(&self) -> usize {
    self.segment_size
  }

  pub fn segments_per_file_shift(&self) -> u8 {
    self.segments_per_file_shift
  }

  /// 丢弃未 `sync_data` 的写入，模拟掉电（保留已 fsync 状态）。
  pub fn crash(&self) {
    let mut state = self.lock();
    let _ = log_op(&self.config, &mut state, Op::Crash);
    state.volatile = Overlay::default();
  }

  /// `crash` 的别名：重启后只剩 durable 视图。
  pub fn restart(&self) {
    self.crash();
  }

  /// 逻辑时钟（已记录 op 数）。
  pub fn clock(&self) -> u64 {
    self.lock().clock
  }

  /// 某类操作截至当前的命中次数（fault / 断言用）。
  pub fn op_count(&self, kind: OpKind) -> u32 {
    self.lock().op_count.get(&kind).copied().unwrap_or(0)
  }

  /// 完整操作日志（含读 / 写 / flush / crash）。
  pub fn op_log_li(&self) -> Vec<OpRecord> {
    self.lock().op_log_li.clone()
  }

  /// 当前可见盘面（durable + volatile，排序去重）。
  pub fn snapshot(&self) -> Snapshot {
    let state = self.lock();
    snapshot_of(&state)
  }

  fn lock(&self) -> MutexGuard<'_, SimState> {
    self.state.lock().unwrap_or_else(|err| err.into_inner())
  }
}

/// 记录一次逻辑操作；命中故障时返回错误且调用方不得改状态。
fn log_op(config: &Config, state: &mut SimState, op: Op) -> Result<()> {
  log_op_payload(config, state, op, None)
}

/// 带 payload 的记录入口；`capture_payload` 关闭时 payload 恒为 `None`。
fn log_op_payload(
  config: &Config,
  state: &mut SimState,
  op: Op,
  payload: Option<&[u8]>,
) -> Result<()> {
  state.clock += 1;
  let kind = op.kind();
  let count = state.op_count.entry(kind).or_insert(0);
  *count += 1;
  let at = *count;
  let delay = config
    .delay_li
    .iter()
    .find(|(k, _)| *k == kind)
    .map_or(0, |(_, delay)| *delay);
  state.op_log_li.push(OpRecord {
    op,
    clock: state.clock,
    delay,
    payload: payload
      .filter(|_| config.capture_payload)
      .map(<[u8]>::to_vec),
  });
  if let Some(fault) = config
    .fault_li
    .iter()
    .find(|fault| fault.op == kind && fault.at == at)
  {
    return Err(fault.err);
  }
  Ok(())
}

/// volatile 覆盖 durable 的读取。
fn overlay_get<K, V>(
  durable: &GxHashMap<K, V>,
  volatile: &GxHashMap<K, Option<V>>,
  key: &K,
) -> Option<V>
where
  K: Eq + Hash,
  V: Clone,
{
  match volatile.get(key) {
    Some(entry) => entry.clone(),
    None => durable.get(key).cloned(),
  }
}

fn tbl_cmp(a: &TableIdent, b: &TableIdent) -> Ordering {
  (a.tbl_name.as_ref(), a.partition_id).cmp(&(b.tbl_name.as_ref(), b.partition_id))
}

fn page_key_cmp(a: &PageKey, b: &PageKey) -> Ordering {
  tbl_cmp(&a.0, &b.0).then(a.1.cmp(&b.1))
}

fn sort_page_key_li(key_li: &mut [PageKey]) {
  key_li.sort_by(page_key_cmp);
}

/// 某 table 当前可见（未删除）的页键，排序去重。
fn live_page_key_li(durable: &Layer, volatile: &Overlay, tbl: &TableIdent) -> Vec<PageKey> {
  let mut keys: Vec<PageKey> = Vec::new();
  for key in durable.page_map.keys() {
    if &key.0 == tbl && overlay_get(&durable.page_map, &volatile.page_map, key).is_some() {
      keys.push(key.clone());
    }
  }
  for key in volatile.page_map.keys() {
    if &key.0 == tbl && overlay_get(&durable.page_map, &volatile.page_map, key).is_some() {
      keys.push(key.clone());
    }
  }
  sort_page_key_li(&mut keys);
  keys.dedup();
  keys
}

/// 某 table 在两层里出现过的页键（含墓碑），用于删除 / 覆盖时打墓碑。
fn existing_page_key_li(durable: &Layer, volatile: &Overlay, tbl: &TableIdent) -> Vec<PageKey> {
  let mut keys: Vec<PageKey> = durable
    .page_map
    .keys()
    .chain(volatile.page_map.keys())
    .filter(|key| &key.0 == tbl)
    .map(PageKey::clone)
    .collect();
  sort_page_key_li(&mut keys);
  keys.dedup();
  keys
}

fn live_segment_key_li(durable: &Layer, volatile: &Overlay, tbl: &TableIdent) -> Vec<PageKey> {
  let mut keys: Vec<PageKey> = Vec::new();
  for key in durable.segment_map.keys() {
    if &key.0 == tbl && overlay_get(&durable.segment_map, &volatile.segment_map, key).is_some() {
      keys.push(key.clone());
    }
  }
  for key in volatile.segment_map.keys() {
    if &key.0 == tbl && overlay_get(&durable.segment_map, &volatile.segment_map, key).is_some() {
      keys.push(key.clone());
    }
  }
  sort_page_key_li(&mut keys);
  keys.dedup();
  keys
}

fn snapshot_of(state: &SimState) -> Snapshot {
  let mut snapshot = Snapshot::default();

  let mut page_keys: Vec<PageKey> = Vec::new();
  page_keys.extend(state.durable.page_map.keys().cloned());
  page_keys.extend(state.volatile.page_map.keys().cloned());
  sort_page_key_li(&mut page_keys);
  page_keys.dedup();
  for key in page_keys {
    if let Some(bytes) = overlay_get(&state.durable.page_map, &state.volatile.page_map, &key) {
      snapshot.page_li.push((key.0, key.1, bytes));
    }
  }

  let mut segment_keys: Vec<PageKey> = Vec::new();
  segment_keys.extend(state.durable.segment_map.keys().cloned());
  segment_keys.extend(state.volatile.segment_map.keys().cloned());
  sort_page_key_li(&mut segment_keys);
  segment_keys.dedup();
  for key in segment_keys {
    if let Some(bytes) = overlay_get(
      &state.durable.segment_map,
      &state.volatile.segment_map,
      &key,
    ) {
      snapshot.segment_li.push((key.0, key.1, bytes));
    }
  }

  let mut manifest_keys: Vec<TableIdent> = state.durable.manifest_map.keys().cloned().collect();
  manifest_keys.sort_by(tbl_cmp);
  manifest_keys.dedup();
  for tbl in manifest_keys {
    if let Some(bytes) = state.durable.manifest_map.get(&tbl) {
      let bytes = bytes.clone();
      snapshot.manifest_li.push((tbl, bytes));
    }
  }

  let mut manifest_file_keys: Vec<ManifestFileKey> =
    state.durable.manifest_file_map.keys().cloned().collect();
  manifest_file_keys.sort_by(|a, b| tbl_cmp(&a.0, &b.0).then(a.1.cmp(&b.1)));
  manifest_file_keys.dedup();
  for key in manifest_file_keys {
    if let Some(bytes) = state.durable.manifest_file_map.get(&key) {
      let bytes = bytes.clone();
      snapshot.manifest_file_li.push((key.0, key.1, bytes));
    }
  }

  let mut term_keys: Vec<TermKey> = state.durable.term_map.keys().cloned().collect();
  term_keys.sort();
  for key in term_keys {
    let term = state.durable.term_map[&key];
    snapshot.term_li.push((key.0, key.1, term));
  }

  snapshot
}

impl PageIo for SimStore {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ReadPage {
        tbl: tbl.clone(),
        file_page_id,
      },
    )?;
    let key = (tbl.clone(), file_page_id);
    let src = overlay_get(&state.durable.page_map, &state.volatile.page_map, &key)
      .ok_or(KvError::ResourceMissing)?;
    if dst.len() < src.len() {
      return Err(KvError::InvalidArgs);
    }
    dst[..src.len()].copy_from_slice(&src);
    Ok(())
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    if src.len() != self.page_size {
      return Err(KvError::InvalidArgs);
    }
    fkv_failpoint::fail_point!(WRITE_PAGE);
    fkv_failpoint::kill_point(WRITE_PAGE);
    let mut state = self.lock();
    log_op_payload(
      &self.config,
      &mut state,
      Op::WritePage {
        tbl: tbl.clone(),
        file_page_id,
        len: src.len(),
      },
      Some(src),
    )?;
    state
      .volatile
      .page_map
      .insert((tbl.clone(), file_page_id), Some(src.to_vec()));
    Ok(())
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    fkv_failpoint::fail_point!(SYNC_DATA);
    fkv_failpoint::kill_point(SYNC_DATA);
    let mut state = self.lock();
    log_op(&self.config, &mut state, Op::SyncData { tbl: tbl.clone() })?;

    let mut page_keys: Vec<PageKey> = state
      .volatile
      .page_map
      .keys()
      .filter(|key| &key.0 == tbl)
      .map(PageKey::clone)
      .collect();
    sort_page_key_li(&mut page_keys);
    if let Some(seed) = self.config.reorder_seed {
      Rng::with_seed(seed).shuffle(&mut page_keys);
    }
    for key in page_keys {
      // 故障判定前置到状态变更之前：命中时该次 FlushPage 不得改动任何状态。
      log_op(
        &self.config,
        &mut state,
        Op::FlushPage {
          tbl: key.0.clone(),
          file_page_id: key.1,
        },
      )?;
      if let Some(entry) = state.volatile.page_map.remove(&key) {
        match entry {
          Some(bytes) => {
            state.durable.page_map.insert(key.clone(), bytes);
          }
          None => {
            state.durable.page_map.remove(&key);
          }
        }
      }
    }

    let mut segment_keys: Vec<PageKey> = state
      .volatile
      .segment_map
      .keys()
      .filter(|key| &key.0 == tbl)
      .map(PageKey::clone)
      .collect();
    sort_page_key_li(&mut segment_keys);
    for key in segment_keys {
      // 同页刷写：先判故障，再迁移 volatile → durable。
      log_op(
        &self.config,
        &mut state,
        Op::FlushSegment {
          tbl: key.0.clone(),
          segment_fp_id: key.1,
        },
      )?;
      if let Some(entry) = state.volatile.segment_map.remove(&key) {
        match entry {
          Some(bytes) => {
            state.durable.segment_map.insert(key.clone(), bytes);
          }
          None => {
            state.durable.segment_map.remove(&key);
          }
        }
      }
    }

    // manifest / 旁路文件不在这里刷：真实后端的 manifest 写路径自己 fsync，
    // 写成功即 durable；由 `sync_data` 顺带提升会掩盖「写路径漏 fsync」的缺陷。
    Ok(())
  }
}

impl ManifestIo for SimStore {
  async fn reset_manifest(&self, tbl: &TableIdent, size: u64) -> Result<()> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ResetManifest {
        tbl: tbl.clone(),
        size,
      },
    )?;
    if let Some(bytes) = state.durable.manifest_map.get_mut(tbl) {
      bytes.truncate(size as usize);
    }
    Ok(())
  }

  async fn reset_manifest_file(&self, tbl: &TableIdent, name: &str, size: u64) -> Result<()> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ResetManifestFile {
        tbl: tbl.clone(),
        name: name.to_owned(),
        size,
      },
    )?;
    let key = (tbl.clone(), name.to_owned());
    if let Some(bytes) = state.durable.manifest_file_map.get_mut(&key) {
      bytes.truncate(size as usize);
    }
    Ok(())
  }

  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    fkv_failpoint::fail_point!(APPEND_WRITE);
    fkv_failpoint::kill_point(APPEND_WRITE);
    fkv_failpoint::kill_point(APPEND_SYNC);
    let mut state = self.lock();
    log_op_payload(
      &self.config,
      &mut state,
      Op::AppendManifest {
        tbl: tbl.clone(),
        len: record.len(),
      },
      Some(record),
    )?;
    let mut bytes = state.durable.manifest_map.remove(tbl).unwrap_or_default();
    bytes.extend_from_slice(record);
    state.durable.manifest_map.insert(tbl.clone(), bytes);
    Ok(())
  }

  async fn append_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    fkv_failpoint::fail_point!(APPEND_WRITE);
    fkv_failpoint::kill_point(APPEND_WRITE);
    fkv_failpoint::kill_point(APPEND_SYNC);
    let mut state = self.lock();
    log_op_payload(
      &self.config,
      &mut state,
      Op::AppendManifestFile {
        tbl: tbl.clone(),
        name: name.to_owned(),
        len: record.len(),
      },
      Some(record),
    )?;
    let key = (tbl.clone(), name.to_owned());
    let mut bytes = state
      .durable
      .manifest_file_map
      .remove(&key)
      .unwrap_or_default();
    bytes.extend_from_slice(record);
    state.durable.manifest_file_map.insert(key, bytes);
    Ok(())
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    fkv_failpoint::kill_point(ATOMIC_RENAME);
    fkv_failpoint::fail_point!(ATOMIC_SYNC);
    let mut state = self.lock();
    log_op_payload(
      &self.config,
      &mut state,
      Op::SwitchManifest {
        tbl: tbl.clone(),
        len: record.len(),
      },
      Some(record),
    )?;
    // 对齐 `switch_manifest`（tmp → fdatasync → rename）：写成功即 durable。
    state
      .durable
      .manifest_map
      .insert(tbl.clone(), record.to_vec());
    Ok(())
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ReadManifest { tbl: tbl.clone() },
    )?;
    Ok(state.durable.manifest_map.get(tbl).cloned())
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::DropManifest { tbl: tbl.clone() },
    )?;
    state.durable.manifest_map.remove(tbl);
    Ok(())
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    fkv_failpoint::kill_point(ATOMIC_RENAME);
    fkv_failpoint::fail_point!(ATOMIC_SYNC);
    let mut state = self.lock();
    log_op_payload(
      &self.config,
      &mut state,
      Op::WriteManifestFile {
        tbl: tbl.clone(),
        name: name.to_owned(),
        len: record.len(),
      },
      Some(record),
    )?;
    state
      .durable
      .manifest_file_map
      .insert((tbl.clone(), name.to_owned()), record.to_vec());
    Ok(())
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ListManifestFileNameLi { tbl: tbl.clone() },
    )?;
    let mut names: Vec<String> = Vec::new();
    for key in state.durable.manifest_file_map.keys() {
      // 只返回真 manifest：`segcomplete*` 等旁路名不得混入 GC 的 manifest 名单。
      if &key.0 == tbl && fkv_types::parse_manifest_file_name(&key.1).is_some() {
        names.push(key.1.clone());
      }
    }
    names.sort();
    names.dedup();
    Ok(names)
  }

  /// 非 manifest 命名规则的旁路文件（如 segment 完成标记）。
  async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ListSidecarFileNameLi { tbl: tbl.clone() },
    )?;
    let mut names: Vec<String> = state
      .durable
      .manifest_file_map
      .keys()
      .filter(|key| &key.0 == tbl && fkv_types::parse_manifest_file_name(&key.1).is_none())
      .map(|key| key.1.clone())
      .collect();
    names.sort_unstable();
    names.dedup();
    Ok(names)
  }

  async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::RemoveSidecarFileName {
        tbl: tbl.clone(),
        name: name.to_owned(),
      },
    )?;
    state
      .durable
      .manifest_file_map
      .remove(&(tbl.clone(), name.to_owned()));
    Ok(())
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ReadManifestFile {
        tbl: tbl.clone(),
        name: name.to_owned(),
      },
    )?;
    let key = (tbl.clone(), name.to_owned());
    Ok(state.durable.manifest_file_map.get(&key).cloned())
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::RemoveManifestFile {
        tbl: tbl.clone(),
        name: name.to_owned(),
      },
    )?;
    state
      .durable
      .manifest_file_map
      .remove(&(tbl.clone(), name.to_owned()));
    Ok(())
  }
}

impl TermIo for SimStore {
  /// term 立即落 durable（对齐 C++ `CURRENT_TERM` 写后 fsync），不参与 volatile 覆盖。
  async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ReadTerm {
        branch: branch.to_owned(),
        shard_id,
      },
    )?;
    Ok(
      state
        .durable
        .term_map
        .get(&(branch.to_owned(), shard_id))
        .copied(),
    )
  }

  async fn upsert_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<u64> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::UpsertTerm {
        branch: branch.to_owned(),
        shard_id,
        term,
      },
    )?;
    let key = (branch.to_owned(), shard_id);
    if let Some(current) = state.durable.term_map.get(&key).copied()
      && current > term
    {
      return Err(KvError::ExpiredTerm);
    }
    state.durable.term_map.insert(key, term);
    Ok(term)
  }

  async fn remove_term(&self, branch: &str, shard_id: u32) -> Result<()> {
    // Sim 的 term 本来就立即 durable；直接删等价于 C++ unlink + 目录 fsync。
    self
      .lock()
      .durable
      .term_map
      .remove(&(branch.to_owned(), shard_id));
    Ok(())
  }
}

impl SegmentIo for SimStore {
  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    if dst.len() > self.segment_size {
      return Err(KvError::InvalidArgs);
    }
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ReadSegment {
        tbl: tbl.clone(),
        segment_fp_id,
        len: dst.len(),
      },
    )?;
    // 空读是 no-op，不要求段存在（与 compio / mem 一致）。
    if dst.is_empty() {
      return Ok(());
    }
    let key = (tbl.clone(), segment_fp_id);
    let src = overlay_get(
      &state.durable.segment_map,
      &state.volatile.segment_map,
      &key,
    )
    .ok_or(KvError::ResourceMissing)?;
    let copy_len = dst.len().min(src.len());
    dst[..copy_len].copy_from_slice(&src[..copy_len]);
    dst[copy_len..].fill(0);
    Ok(())
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> Result<()> {
    if src.len() > self.segment_size {
      return Err(KvError::InvalidArgs);
    }
    fkv_failpoint::fail_point!(WRITE_SEGMENTS);
    fkv_failpoint::kill_point(WRITE_SEGMENTS);
    let mut state = self.lock();
    log_op_payload(
      &self.config,
      &mut state,
      Op::WriteSegment {
        tbl: tbl.clone(),
        segment_fp_id,
        len: src.len(),
      },
      Some(src),
    )?;
    state
      .volatile
      .segment_map
      .insert((tbl.clone(), segment_fp_id), Some(src.to_vec()));
    Ok(())
  }
}

impl FixedSegmentIo for SimStore {
  fn register_fixed_buffer(&self, len: usize) -> Result<FixedBuffer> {
    let mut state = self.lock();
    log_op(&self.config, &mut state, Op::RegisterFixedBuffer { len })?;
    Ok(FixedBuffer::new(len))
  }

  async fn read_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &mut FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    let end = offset.checked_add(len).ok_or(KvError::InvalidArgs)?;
    if end > buf.len() {
      return Err(KvError::InvalidArgs);
    }
    {
      let mut state = self.lock();
      log_op(
        &self.config,
        &mut state,
        Op::ReadSegmentFixed {
          tbl: tbl.clone(),
          segment_fp_id,
          offset,
          len,
        },
      )?;
    }
    let dst = buf.as_mut_slice().ok_or(KvError::Busy)?;
    self
      .read_segment(tbl, segment_fp_id, &mut dst[offset..end])
      .await
  }

  async fn write_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    let end = offset.checked_add(len).ok_or(KvError::InvalidArgs)?;
    if end > buf.len() {
      return Err(KvError::InvalidArgs);
    }
    {
      let mut state = self.lock();
      log_op(
        &self.config,
        &mut state,
        Op::WriteSegmentFixed {
          tbl: tbl.clone(),
          segment_fp_id,
          offset,
          len,
        },
      )?;
    }
    self
      .write_segment(tbl, segment_fp_id, &buf.as_slice()[offset..end])
      .await
  }
}

impl FileIo for SimStore {
  async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ReadFile {
        tbl: tbl.clone(),
        file_id,
      },
    )?;
    let mask = (1u64 << self.pages_per_file_shift) - 1;
    let keys = live_page_key_li(&state.durable, &state.volatile, tbl);
    let mut max_index: Option<u64> = None;
    for key in &keys {
      if key.1 >> self.pages_per_file_shift != file_id {
        continue;
      }
      let index = key.1 & mask;
      max_index = Some(max_index.map_or(index, |current| current.max(index)));
    }
    let Some(max_index) = max_index else {
      return Ok(None);
    };
    let mut bytes = vec![0u8; (max_index as usize + 1) * self.page_size];
    for key in &keys {
      if key.1 >> self.pages_per_file_shift != file_id {
        continue;
      }
      let Some(src) = overlay_get(&state.durable.page_map, &state.volatile.page_map, key) else {
        continue;
      };
      let offset = (key.1 & mask) as usize * self.page_size;
      bytes[offset..offset + src.len()].copy_from_slice(&src);
    }
    Ok(Some(bytes))
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()> {
    if !bytes.len().is_multiple_of(self.page_size) {
      return Err(KvError::InvalidArgs);
    }
    fkv_failpoint::fail_point!(WRITE_FILE_SYNC_FAIL);
    fkv_failpoint::kill_point(WRITE_FILE_RENAME);
    let mut state = self.lock();
    log_op_payload(
      &self.config,
      &mut state,
      Op::WriteFile {
        tbl: tbl.clone(),
        file_id,
        len: bytes.len(),
      },
      Some(bytes),
    )?;
    let min_fp = file_id << self.pages_per_file_shift;
    let max_fp = (file_id + 1) << self.pages_per_file_shift;
    let targets: Vec<PageKey> = existing_page_key_li(&state.durable, &state.volatile, tbl)
      .into_iter()
      .filter(|key| key.1 >= min_fp && key.1 < max_fp)
      .collect();
    for key in targets {
      state.volatile.page_map.insert(key, None);
    }
    for (index, chunk) in bytes.chunks(self.page_size).enumerate() {
      // 全零块是空洞，不占内存；真实 page 必含非零 checksum / header。
      if chunk.iter().all(|byte| *byte == 0) {
        continue;
      }
      state
        .volatile
        .page_map
        .insert((tbl.clone(), min_fp + index as u64), Some(chunk.to_vec()));
    }
    Ok(())
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::FileSize {
        tbl: tbl.clone(),
        file_id,
      },
    )?;
    let mask = (1u64 << self.pages_per_file_shift) - 1;
    let mut max_index: Option<u64> = None;
    for key in live_page_key_li(&state.durable, &state.volatile, tbl) {
      if key.1 >> self.pages_per_file_shift != file_id {
        continue;
      }
      let index = key.1 & mask;
      max_index = Some(max_index.map_or(index, |current| current.max(index)));
    }
    Ok(max_index.map(|index| (index + 1) * self.page_size as u64))
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::RemoveFile {
        tbl: tbl.clone(),
        file_id,
      },
    )?;
    let min_fp = file_id << self.pages_per_file_shift;
    let max_fp = (file_id + 1) << self.pages_per_file_shift;
    let targets: Vec<PageKey> = existing_page_key_li(&state.durable, &state.volatile, tbl)
      .into_iter()
      .filter(|key| key.1 >= min_fp && key.1 < max_fp)
      .collect();
    for key in targets {
      state.volatile.page_map.insert(key, None);
    }
    Ok(())
  }

  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ListFileIdLi { tbl: tbl.clone() },
    )?;
    let mut set = GxHashSet::default();
    for key in live_page_key_li(&state.durable, &state.volatile, tbl) {
      set.insert(key.1 >> self.pages_per_file_shift);
    }
    let mut file_id_li: Vec<FileId> = set.into_iter().collect();
    file_id_li.sort_unstable();
    Ok(file_id_li)
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ListSegmentFileIdLi { tbl: tbl.clone() },
    )?;
    let mut set = GxHashSet::default();
    for key in live_segment_key_li(&state.durable, &state.volatile, tbl) {
      set.insert(key.1 >> self.segments_per_file_shift);
    }
    let mut file_id_li: Vec<FileId> = set.into_iter().collect();
    file_id_li.sort_unstable();
    Ok(file_id_li)
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::RemoveSegmentFile {
        tbl: tbl.clone(),
        file_id,
      },
    )?;
    let min_fp = file_id << self.segments_per_file_shift;
    let max_fp = (file_id + 1) << self.segments_per_file_shift;
    let targets: Vec<PageKey> = state
      .durable
      .segment_map
      .keys()
      .chain(state.volatile.segment_map.keys())
      .filter(|key| &key.0 == tbl && key.1 >= min_fp && key.1 < max_fp)
      .map(PageKey::clone)
      .collect();
    for key in targets {
      state.volatile.segment_map.insert(key, None);
    }
    Ok(())
  }
}
