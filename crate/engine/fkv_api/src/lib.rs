//! Shard 请求 / 响应。只放纯数据，不依赖 Store / IO。

use fkv_btree::BatchEntry;
use fkv_types::{FileId, TableIdent};

#[derive(Debug)]
pub enum Request {
  Put {
    tbl: TableIdent,
    batch: Vec<BatchEntry>,
    term: Option<u64>,
  },
  Get {
    tbl: TableIdent,
    key: Vec<u8>,
  },
  PutLarge {
    tbl: TableIdent,
    batch: Vec<BatchEntry>,
    term: Option<u64>,
  },
  GetLarge {
    tbl: TableIdent,
    key: Vec<u8>,
  },
  Scan {
    tbl: TableIdent,
    begin: Vec<u8>,
    end: Vec<u8>,
    max_entries: usize,
    begin_inclusive: bool,
    end_inclusive: bool,
    page_entries: usize,
    page_size: usize,
  },
  Reopen {
    tbl: TableIdent,
  },
  Gc {
    tbl: TableIdent,
    pages_per_file_shift: u8,
  },
  Compact {
    tbl: TableIdent,
    file_id: FileId,
    pages_per_file_shift: u8,
  },
  Truncate {
    tbl: TableIdent,
    position: Vec<u8>,
  },
  Floor {
    tbl: TableIdent,
    key: Vec<u8>,
  },
  Drop {
    tbl: TableIdent,
  },
  AttachShard {
    branch: String,
    shard_id: u32,
    term: u64,
  },
  DetachShard {
    shard_id: u32,
  },
  CreateArchive {
    tbl: TableIdent,
    tag: String,
    num_retained: u32,
  },
  CreateBranch {
    tbl: TableIdent,
    branch: String,
  },
  DeleteBranch {
    tbl: TableIdent,
    branch: String,
  },
  Shutdown,
}

impl Request {
  pub fn table(&self) -> Option<&TableIdent> {
    match self {
      Self::Put { tbl, .. }
      | Self::Get { tbl, .. }
      | Self::PutLarge { tbl, .. }
      | Self::GetLarge { tbl, .. }
      | Self::Scan { tbl, .. }
      | Self::Reopen { tbl }
      | Self::Gc { tbl, .. }
      | Self::Compact { tbl, .. }
      | Self::Truncate { tbl, .. }
      | Self::Floor { tbl, .. }
      | Self::Drop { tbl }
      | Self::CreateArchive { tbl, .. }
      | Self::CreateBranch { tbl, .. }
      | Self::DeleteBranch { tbl, .. } => Some(tbl),
      Self::AttachShard { .. } | Self::DetachShard { .. } => None,
      Self::Shutdown => None,
    }
  }

  pub fn is_shutdown(&self) -> bool {
    matches!(self, Self::Shutdown)
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveReply {
  pub name: String,
  pub bytes: usize,
  pub removed_num: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanPageOptions {
  pub begin_inclusive: bool,
  pub end_inclusive: bool,
  pub page_entries: usize,
  pub page_size: usize,
}

impl Default for ScanPageOptions {
  fn default() -> Self {
    Self {
      begin_inclusive: true,
      end_inclusive: false,
      page_entries: 0,
      page_size: 0,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanReply {
  pub entry_li: Vec<(Vec<u8>, Vec<u8>)>,
  pub has_remaining: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloorReply {
  pub key: Vec<u8>,
  pub value: Vec<u8>,
  pub metadata: Vec<u8>,
  pub timestamp: u64,
  pub expire_ts: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchReply {
  pub name: String,
  pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeValueReply {
  pub value: Vec<u8>,
  pub metadata: Vec<u8>,
}

#[derive(Debug)]
pub enum Response {
  Put,
  Get(Option<Vec<u8>>),
  GetLarge(Option<LargeValueReply>),
  Scan(ScanReply),
  Reopen,
  Gc(usize),
  Compact(usize),
  Truncate,
  Floor(FloorReply),
  Drop,
  CreateArchive(ArchiveReply),
  CreateBranch(BranchReply),
  DeleteBranch(usize),
  AttachShard,
  DetachShard,
  Shutdown,
}
