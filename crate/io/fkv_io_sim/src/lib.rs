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

use std::sync::{Mutex, MutexGuard};

use fkv_types::{
  DEFAULT_PAGES_PER_FILE_SHIFT, DEFAULT_SEGMENT_SIZE, DEFAULT_SEGMENTS_PER_FILE_SHIFT,
};

mod file;
mod manifest;
mod op;
pub mod oplog;
mod page;
mod segment;
mod state;
mod term;

pub use op::{Fault, Op, OpKind, OpRecord};
pub use state::Snapshot;

use crate::state::{Config, Overlay, SimState, log_op, snapshot_of};

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

  /// 清空已注入的故障（对应 C++ `FailPoint::Disarm`），保留 op log / 已落盘状态。
  pub fn clear_fault_li(&mut self) {
    self.config.fault_li.clear();
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
