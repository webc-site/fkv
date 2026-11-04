//! File GC 执行：删除没有任何 retained page 的 data 文件。
//!
//! 文件内仍有 live page 时保留；等 compaction 把 live page 移走后再删。

use fkv_error::Result;
use fkv_gc::retained_file_page_set;
use fkv_io::{FileIo, ManifestIo};
use fkv_mapping::{MappingTbl, PageMapper, decode_id, is_file_page_id};
use fkv_types::{FileId, FilePageId, PageId, TableIdent};
use gxhash::HashSet;

/// 释放指定逻辑页，返回实际释放数量。
pub fn free_page_li(mapper: &mut PageMapper, page_li: &[PageId]) -> Result<usize> {
  let mut freed = 0;
  for page_id in page_li {
    if mapper.free_page(*page_id)?.is_some() {
      freed += 1;
    }
  }
  Ok(freed)
}

/// 删除不包含任何 retained page 的 data 文件。
pub async fn gc_files<I: FileIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &PageMapper,
  pages_per_file_shift: u8,
) -> Result<usize> {
  let retained = retained_file_page_set(mapper.tbl());
  let mut retained_file_set = HashSet::default();
  for file_page_id in retained {
    retained_file_set.insert(file_page_id >> pages_per_file_shift);
  }
  let candidate_li = io.list_file_id_li(tbl).await?;
  let mut removed = 0;
  for file_id in candidate_li {
    if retained_file_set.contains(&file_id) {
      continue;
    }
    io.remove_file(tbl, file_id).await?;
    removed += 1;
  }
  Ok(removed)
}

/// 删除不包含任何 retained segment 的 segment 文件。
pub async fn gc_segment_files<I: FileIo>(
  io: &I,
  tbl: &TableIdent,
  segment_mapper: &PageMapper,
  segments_per_file_shift: u8,
) -> Result<usize> {
  let retained = retained_file_page_set(segment_mapper.tbl());
  let mut retained_file_set = HashSet::default();
  for file_page_id in retained {
    retained_file_set.insert(file_page_id >> segments_per_file_shift);
  }
  let candidate_li = io.list_segment_file_id_li(tbl).await?;
  let mut removed = 0;
  for file_id in candidate_li {
    if retained_file_set.contains(&file_id) {
      continue;
    }
    io.remove_segment_file(tbl, file_id).await?;
    removed += 1;
  }
  Ok(removed)
}

/// 列出某个文件里仍然 live 的逻辑页。
pub fn live_page_li(mapper: &PageMapper, file_id: FileId, pages_per_file_shift: u8) -> Vec<PageId> {
  let min_fp = file_id << pages_per_file_shift;
  let max_fp = (file_id + 1) << pages_per_file_shift;
  let mut page_li = Vec::new();
  for (page_id, value) in mapper.tbl().value_li().iter().enumerate() {
    if fkv_mapping::is_file_page_id(*value) {
      let file_page_id = fkv_mapping::decode_id(*value);
      if file_page_id >= min_fp && file_page_id < max_fp {
        page_li.push(page_id as PageId);
      }
    }
  }
  page_li
}

pub fn add_mapping_retained(mapping: &MappingTbl, file_page_set: &mut HashSet<FilePageId>) {
  for value in mapping.value_li() {
    if is_file_page_id(*value) {
      file_page_set.insert(decode_id(*value));
    }
  }
}

/// 汇总 active manifest + 所有 branch / archive manifest 的 retained page。
pub async fn manifest_retained_file_page_set<I: ManifestIo>(
  io: &I,
  tbl: &TableIdent,
) -> Result<(HashSet<FilePageId>, HashSet<FilePageId>)> {
  let mut data_set = HashSet::default();
  let mut segment_set = HashSet::default();
  let mut apply = |bytes: &[u8]| -> Result<()> {
    let state = fkv_task_reopen::replay_manifest_bytes(bytes)?;
    add_mapping_retained(&state.mapping_tbl, &mut data_set);
    add_mapping_retained(&state.segment_mapping, &mut segment_set);
    Ok(())
  };
  if let Some(bytes) = io.read_manifest(tbl).await? {
    apply(&bytes)?;
  }
  for name in io.list_manifest_file_name_li(tbl).await? {
    if let Some(bytes) = io.read_manifest_file(tbl, &name).await? {
      apply(&bytes)?;
    }
  }
  Ok((data_set, segment_set))
}

fn file_id_set(file_page_set: &HashSet<FilePageId>, shift: u8) -> HashSet<FileId> {
  let mut file_set = HashSet::default();
  for file_page_id in file_page_set {
    file_set.insert(file_page_id >> shift);
  }
  file_set
}

pub async fn gc_files_with_retained<I: FileIo>(
  io: &I,
  tbl: &TableIdent,
  retained_file_page_set: &HashSet<FilePageId>,
  pages_per_file_shift: u8,
) -> Result<usize> {
  let retained_file_set = file_id_set(retained_file_page_set, pages_per_file_shift);
  let candidate_li = io.list_file_id_li(tbl).await?;
  let mut removed = 0;
  for file_id in candidate_li {
    if retained_file_set.contains(&file_id) {
      continue;
    }
    io.remove_file(tbl, file_id).await?;
    removed += 1;
  }
  Ok(removed)
}

pub async fn gc_segment_files_with_retained<I: FileIo>(
  io: &I,
  tbl: &TableIdent,
  retained_file_page_set: &HashSet<FilePageId>,
  segments_per_file_shift: u8,
) -> Result<usize> {
  let retained_file_set = file_id_set(retained_file_page_set, segments_per_file_shift);
  let candidate_li = io.list_segment_file_id_li(tbl).await?;
  let mut removed = 0;
  for file_id in candidate_li {
    if retained_file_set.contains(&file_id) {
      continue;
    }
    io.remove_segment_file(tbl, file_id).await?;
    removed += 1;
  }
  Ok(removed)
}
