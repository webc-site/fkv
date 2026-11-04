//! Compaction 执行：把指定文件里的 live page 搬到新物理页。
//!
//! 逻辑 PageId 不变，只换 `FilePageId`；搬空后由 `fkv_task_gc` 删除文件。

use fkv_error::Result;
use fkv_io::PageIo;
use fkv_mapping::PageMapper;
use fkv_task_gc::live_page_li;
use fkv_types::{FileId, TableIdent};

pub async fn compact_file<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  file_id: FileId,
  page_size: usize,
  pages_per_file_shift: u8,
) -> Result<usize> {
  let page_li = live_page_li(mapper, file_id, pages_per_file_shift);
  let mut moved = 0;
  for page_id in page_li {
    let old_fp = mapper.file_page_id(page_id)?;
    let mut page = vec![0u8; page_size];
    io.read_page(tbl, old_fp, &mut page).await?;
    let new_fp = mapper.remap_page(page_id)?;
    io.write_page(tbl, new_fp, &page).await?;
    moved += 1;
  }
  Ok(moved)
}
