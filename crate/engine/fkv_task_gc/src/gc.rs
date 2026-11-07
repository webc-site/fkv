//! GC 执行：按 retained set 与 in-flight guard 删除 data / segment 文件。

use fkv_error::Result;
use fkv_io::{FileIo, FileVariant};
use fkv_task_read::YieldBudget;
use fkv_types::{FileId, FilePageId, TableIdent};
use gxhash::HashSet;

use crate::inflight::InflightGuard;

/// GC 的删除范围（对齐 C++ 两类请求的差别）。
///
/// - `File`：`FileGc` 语义，按后端选目标 —— 本地 / 内存后端删本地文件，
///   云端后端删远端对象（并清对应本地 cache）；
/// - `Local`：`LocalGc` 语义，只删本地文件 / 本地 cache，绝不触达远端
///   （对齐 C++ `ExecuteLocalGC`）。
///
/// 两者共享同一次 retained set / in-flight guard 判定，差别只在列举与删除
/// 使用的 `FileIo` 方法，保证「删谁」的判定不会分叉。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcScope {
  File,
  Local,
}

impl GcScope {
  async fn list_data_variants<I: FileIo>(
    self,
    io: &I,
    tbl: &TableIdent,
  ) -> Result<Vec<FileVariant>> {
    match self {
      Self::File => io.list_file_variants(tbl).await,
      Self::Local => io.list_local_file_variants(tbl).await,
    }
  }

  /// 删除一个候选变体：有身份按 `(file_id, branch, term)` 精确删（rev-39），
  /// 无身份退回整 id 删除（与 rev-39 之前一致）。
  async fn remove_data_variant<I: FileIo>(
    self,
    io: &I,
    tbl: &TableIdent,
    variant: &FileVariant,
  ) -> Result<()> {
    match (self, &variant.identity) {
      (Self::File, Some((branch, term))) => {
        io.remove_file_variant(tbl, variant.file_id, branch, *term)
          .await
      }
      (Self::File, None) => io.remove_file(tbl, variant.file_id).await,
      (Self::Local, Some((branch, term))) => {
        io.remove_local_file_variant(tbl, variant.file_id, branch, *term)
          .await
      }
      (Self::Local, None) => io.remove_local_file(tbl, variant.file_id).await,
    }
  }

  async fn list_segment_variants<I: FileIo>(
    self,
    io: &I,
    tbl: &TableIdent,
  ) -> Result<Vec<FileVariant>> {
    match self {
      Self::File => io.list_segment_file_variants(tbl).await,
      Self::Local => io.list_local_segment_file_variants(tbl).await,
    }
  }

  async fn remove_segment_variant<I: FileIo>(
    self,
    io: &I,
    tbl: &TableIdent,
    variant: &FileVariant,
  ) -> Result<()> {
    match (self, &variant.identity) {
      (Self::File, Some((branch, term))) => {
        io.remove_segment_file_variant(tbl, variant.file_id, branch, *term)
          .await
      }
      (Self::File, None) => io.remove_segment_file(tbl, variant.file_id).await,
      (Self::Local, Some((branch, term))) => {
        io.remove_local_segment_file_variant(tbl, variant.file_id, branch, *term)
          .await
      }
      (Self::Local, None) => io.remove_local_segment_file(tbl, variant.file_id).await,
    }
  }
}

async fn file_id_set(
  file_page_set: &HashSet<FilePageId>,
  shift: u8,
  yield_budget: &mut YieldBudget,
) -> HashSet<FileId> {
  let mut file_set = HashSet::default();
  for (seq, file_page_id) in file_page_set.iter().enumerate() {
    // retained 集合可能非常大（全表 mapping），按步长检查段预算。
    yield_budget.maybe_yield_every(seq).await;
    file_set.insert(file_page_id >> shift);
  }
  file_set
}

/// 回收象限：data / segment 文件 × [`GcScope`]（File / Local）。
///
/// 两象限的回收循环逐行一致，仅 4 个点位（列举 / 变体级保留 / id 级保留 /
/// 删除）随象限分派。
#[derive(Debug, Clone, Copy)]
enum GcSide {
  Data(GcScope),
  Segment(GcScope),
}

/// data / segment 两侧的回收共体（见 [`GcSide`]：仅 4 个点位随象限分派）。
async fn gc_files<I: FileIo>(
  io: &I,
  tbl: &TableIdent,
  retained_file_page_set: &HashSet<FilePageId>,
  guard: &InflightGuard,
  per_file_shift: u8,
  yield_budget_us: u64,
  side: GcSide,
) -> Result<usize> {
  let mut yield_budget = YieldBudget::new(yield_budget_us);
  let retained_file_set =
    file_id_set(retained_file_page_set, per_file_shift, &mut yield_budget).await;
  let candidate_li = match side {
    GcSide::Data(scope) => scope.list_data_variants(io, tbl).await?,
    GcSide::Segment(scope) => scope.list_segment_variants(io, tbl).await?,
  };
  let mut removed = 0;
  for variant in &candidate_li {
    yield_budget.maybe_yield().await;
    let file_id = variant.file_id;
    // id 级并集仍是安全下限（sto-54a）：任一变体被引用，整 id 不会因
    // 「无引用」被删；变体级死活由 guard 的归属知识继续判定。
    let referenced = retained_file_set.contains(&file_id);
    let keep = match (&variant.identity, side) {
      // 文件名自带身份：按变体精确判定（引用归属 + 自身 (branch, term) in-flight）。
      (Some((branch, term)), GcSide::Data(_)) => {
        guard.retain_data_variant(file_id, branch, *term, referenced)
      }
      (Some((branch, term)), GcSide::Segment(_)) => {
        guard.retain_segment_variant(file_id, branch, *term, referenced)
      }
      // 无身份后端（默认实现）：保持 rev-39 之前的 id 级判定。
      (None, GcSide::Data(_)) => referenced || guard.retain_data(file_id),
      (None, GcSide::Segment(_)) => referenced || guard.retain_segment(file_id),
    };
    if keep {
      continue;
    }
    match side {
      GcSide::Data(scope) => scope.remove_data_variant(io, tbl, variant).await?,
      GcSide::Segment(scope) => scope.remove_segment_variant(io, tbl, variant).await?,
    };
    removed += 1;
  }
  Ok(removed)
}

pub async fn gc_files_with_retained<I: FileIo>(
  io: &I,
  tbl: &TableIdent,
  retained_file_page_set: &HashSet<FilePageId>,
  guard: &InflightGuard,
  pages_per_file_shift: u8,
) -> Result<usize> {
  gc_files_with_retained_budget(
    io,
    tbl,
    retained_file_page_set,
    guard,
    pages_per_file_shift,
    0,
  )
  .await
}

/// 同 [`gc_files_with_retained`]，但每个候选文件检查一次段预算。
pub async fn gc_files_with_retained_budget<I: FileIo>(
  io: &I,
  tbl: &TableIdent,
  retained_file_page_set: &HashSet<FilePageId>,
  guard: &InflightGuard,
  pages_per_file_shift: u8,
  yield_budget_us: u64,
) -> Result<usize> {
  gc_files(
    io,
    tbl,
    retained_file_page_set,
    guard,
    pages_per_file_shift,
    yield_budget_us,
    GcSide::Data(GcScope::File),
  )
  .await
}

/// `LocalGc` 的 data 文件回收：列举 / 删除都只走本地路径，绝不删远端对象。
pub async fn gc_local_files_with_retained_budget<I: FileIo>(
  io: &I,
  tbl: &TableIdent,
  retained_file_page_set: &HashSet<FilePageId>,
  guard: &InflightGuard,
  pages_per_file_shift: u8,
  yield_budget_us: u64,
) -> Result<usize> {
  gc_files(
    io,
    tbl,
    retained_file_page_set,
    guard,
    pages_per_file_shift,
    yield_budget_us,
    GcSide::Data(GcScope::Local),
  )
  .await
}

pub async fn gc_segment_files_with_retained<I: FileIo>(
  io: &I,
  tbl: &TableIdent,
  retained_file_page_set: &HashSet<FilePageId>,
  guard: &InflightGuard,
  segments_per_file_shift: u8,
) -> Result<usize> {
  gc_segment_files_with_retained_budget(
    io,
    tbl,
    retained_file_page_set,
    guard,
    segments_per_file_shift,
    0,
  )
  .await
}

/// 同 [`gc_segment_files_with_retained`]，但每个候选文件检查一次段预算。
pub async fn gc_segment_files_with_retained_budget<I: FileIo>(
  io: &I,
  tbl: &TableIdent,
  retained_file_page_set: &HashSet<FilePageId>,
  guard: &InflightGuard,
  segments_per_file_shift: u8,
  yield_budget_us: u64,
) -> Result<usize> {
  gc_files(
    io,
    tbl,
    retained_file_page_set,
    guard,
    segments_per_file_shift,
    yield_budget_us,
    GcSide::Segment(GcScope::File),
  )
  .await
}

/// `LocalGc` 的 segment 文件回收：只删本地 cache / 本地文件。
pub async fn gc_local_segment_files_with_retained_budget<I: FileIo>(
  io: &I,
  tbl: &TableIdent,
  retained_file_page_set: &HashSet<FilePageId>,
  guard: &InflightGuard,
  segments_per_file_shift: u8,
  yield_budget_us: u64,
) -> Result<usize> {
  gc_files(
    io,
    tbl,
    retained_file_page_set,
    guard,
    segments_per_file_shift,
    yield_budget_us,
    GcSide::Segment(GcScope::Local),
  )
  .await
}
