//! 点读：root index → data page，最多 64 层。

use fkv_btree::{decompress_value, seek_index};
use fkv_compress::DictCompression;
use fkv_error::{KvError, Result};
use fkv_io::PageIo;
use fkv_mapping::PageMapper;
use fkv_page::{DataPage, DataPageIter, PageType, page_type};
use fkv_types::{PageId, TableIdent};

pub const MAX_TREE_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadEntry {
  pub value: Vec<u8>,
  pub timestamp: u64,
  pub expire_ts: u64,
  pub is_large_value: bool,
}

pub async fn read_value<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  root_id: PageId,
  mapper: &PageMapper,
  key: &[u8],
  page_size: usize,
  dict: Option<&DictCompression>,
) -> Result<Option<Vec<u8>>> {
  Ok(
    read_entry(io, tbl, root_id, mapper, key, page_size, dict)
      .await?
      .map(|entry| entry.value),
  )
}

pub async fn read_entry<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  root_id: PageId,
  mapper: &PageMapper,
  key: &[u8],
  page_size: usize,
  dict: Option<&DictCompression>,
) -> Result<Option<ReadEntry>> {
  if root_id == PageId::MAX {
    return Ok(None);
  }
  let mut page_id = root_id;
  let mut page = vec![0u8; page_size];
  for _ in 0..MAX_TREE_DEPTH {
    let file_page_id = mapper.file_page_id(page_id)?;
    io.read_page(tbl, file_page_id, &mut page).await?;
    match page_type(&page)? {
      PageType::Data => {
        let mut iter = DataPageIter::new(&page)?;
        if iter.seek(key)? && iter.key() == key {
          return Ok(Some(ReadEntry {
            value: decompress_value(iter.value(), iter.compression(), dict)?,
            timestamp: iter.timestamp(),
            expire_ts: iter.expire_ts(),
            is_large_value: iter.is_large_value(),
          }));
        }
        return Ok(None);
      }
      PageType::LeafIndex | PageType::NonLeafIndex => {
        let child = seek_index(&page, key)?;
        if child == page_id {
          return Err(KvError::Corrupted);
        }
        page_id = child;
      }
      PageType::Overflow | PageType::Deleted => return Err(KvError::Corrupted),
    }
  }
  Err(KvError::Corrupted)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadFloorEntry {
  pub key: Vec<u8>,
  pub value: Vec<u8>,
  pub timestamp: u64,
  pub expire_ts: u64,
  pub is_large_value: bool,
}

/// 最大且 <= search_key 的 entry；不存在返回 `None`。
pub async fn read_floor_entry<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  root_id: PageId,
  mapper: &PageMapper,
  key: &[u8],
  page_size: usize,
  dict: Option<&DictCompression>,
) -> Result<Option<ReadFloorEntry>> {
  if root_id == PageId::MAX {
    return Ok(None);
  }
  let mut page_id = root_id;
  let mut page = vec![0u8; page_size];
  for _ in 0..MAX_TREE_DEPTH {
    let file_page_id = mapper.file_page_id(page_id)?;
    io.read_page(tbl, file_page_id, &mut page).await?;
    match page_type(&page)? {
      PageType::Data => break,
      PageType::LeafIndex | PageType::NonLeafIndex => {
        let child = seek_index(&page, key)?;
        if child == page_id {
          return Err(KvError::Corrupted);
        }
        page_id = child;
      }
      PageType::Overflow | PageType::Deleted => return Err(KvError::Corrupted),
    }
  }
  let mut iter = DataPageIter::new(&page)?;
  if iter.seek_floor(key)? {
    return Ok(Some(floor_entry(&mut iter, dict)?));
  }
  let data_page = DataPage::new(&page)?;
  let prev_page_id = data_page.prev_page_id()?;
  if prev_page_id == PageId::MAX {
    return Ok(None);
  }
  let file_page_id = mapper.file_page_id(prev_page_id)?;
  page.fill(0);
  io.read_page(tbl, file_page_id, &mut page).await?;
  let mut iter = DataPageIter::new(&page)?;
  if iter.seek_floor(key)? {
    return Ok(Some(floor_entry(&mut iter, dict)?));
  }
  Ok(None)
}

fn floor_entry(
  iter: &mut DataPageIter<'_>,
  dict: Option<&DictCompression>,
) -> Result<ReadFloorEntry> {
  Ok(ReadFloorEntry {
    key: iter.key().to_vec(),
    value: decompress_value(iter.value(), iter.compression(), dict)?,
    timestamp: iter.timestamp(),
    expire_ts: iter.expire_ts(),
    is_large_value: iter.is_large_value(),
  })
}
