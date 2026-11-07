//! 逻辑操作模型：类别 `OpKind`、带参记录 `Op`、op log 条目与故障注入描述。

use fkv_error::KvError;
use fkv_types::{FileId, FilePageId, TableIdent};

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
