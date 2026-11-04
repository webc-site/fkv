//! 范围扫描：从 begin 下降到 data page，再沿 next 链遍历。

use fkv_btree::{decompress_value, seek_index};
use fkv_compress::DictCompression;
use fkv_error::{KvError, Result};
use fkv_io::PageIo;
use fkv_mapping::PageMapper;
use fkv_page::{DataPage, DataPageIter, PageType, page_type};
use fkv_types::{PageId, TableIdent};

pub const MAX_TREE_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanOptions {
  pub begin_inclusive: bool,
  pub end_inclusive: bool,
  /// 对齐 ScanTask：命中超大 value 返回 `LargeValueUnsupported`。
  pub reject_large_value: bool,
}

impl Default for ScanOptions {
  fn default() -> Self {
    Self {
      begin_inclusive: true,
      end_inclusive: false,
      reject_large_value: true,
    }
  }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageOptions {
  /// 0 表示不限制 entry 数。
  pub page_entries: usize,
  /// 0 表示不限制字节数。字节口径：key + value + 16（两个 u64）。
  pub page_size: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanPage {
  pub entry_li: Vec<ScannedEntry>,
  pub has_remaining: bool,
}

pub struct ScanArgs<'a> {
  pub tbl: &'a TableIdent,
  pub root_id: PageId,
  pub mapper: &'a PageMapper,
  pub begin: &'a [u8],
  pub end: &'a [u8],
  pub page_size: usize,
  pub max_entries: usize,
  pub dict: Option<&'a DictCompression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedEntry {
  pub key: Vec<u8>,
  pub value: Vec<u8>,
  pub expire_ts: u64,
}

pub async fn scan_range<I: PageIo>(io: &I, args: ScanArgs<'_>) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
  Ok(
    scan_entries(io, args)
      .await?
      .into_iter()
      .map(|entry| (entry.key, entry.value))
      .collect(),
  )
}

pub async fn scan_entries<I: PageIo>(io: &I, args: ScanArgs<'_>) -> Result<Vec<ScannedEntry>> {
  scan_entries_with(io, args, ScanOptions::default()).await
}

pub async fn scan_entries_with<I: PageIo>(
  io: &I,
  args: ScanArgs<'_>,
  options: ScanOptions,
) -> Result<Vec<ScannedEntry>> {
  Ok(
    scan_page_with(io, args, options, PageOptions::default())
      .await?
      .entry_li,
  )
}

pub async fn scan_page_with<I: PageIo>(
  io: &I,
  args: ScanArgs<'_>,
  options: ScanOptions,
  page_opts: PageOptions,
) -> Result<ScanPage> {
  let ScanArgs {
    tbl,
    root_id,
    mapper,
    begin,
    end,
    page_size,
    max_entries,
    dict,
  } = args;
  if root_id == PageId::MAX {
    return Ok(ScanPage::default());
  }
  let mut page_id = root_id;
  let mut page = vec![0u8; page_size];
  for _ in 0..MAX_TREE_DEPTH {
    let file_page_id = mapper.file_page_id(page_id)?;
    io.read_page(tbl, file_page_id, &mut page).await?;
    match page_type(&page)? {
      PageType::Data => break,
      PageType::LeafIndex | PageType::NonLeafIndex => {
        let child = seek_index(&page, begin)?;
        if child == page_id {
          return Err(KvError::Corrupted);
        }
        page_id = child;
      }
      PageType::Overflow | PageType::Deleted => return Err(KvError::Corrupted),
    }
  }

  let mut result_li = Vec::new();
  let mut result_size = 0usize;
  let mut has_remaining = false;
  let mut iter = DataPageIter::new(&page)?;
  let mut has = iter.seek(begin)?;
  if !options.begin_inclusive && has && iter.key() == begin {
    has = iter.advance()?;
  }
  loop {
    if has {
      let key = iter.key();
      let end_reached = !end.is_empty() && (key > end || (!options.end_inclusive && key == end));
      if end_reached {
        break;
      }
      if page_opts.page_entries != 0 && result_li.len() == page_opts.page_entries {
        has_remaining = true;
        break;
      }
      if options.reject_large_value && iter.is_large_value() {
        return Err(KvError::LargeValueUnsupported);
      }
      let entry_size = key.len() + iter.value().len() + 16;
      if page_opts.page_size != 0
        && result_size > 0
        && result_size.saturating_add(entry_size) > page_opts.page_size
      {
        has_remaining = true;
        break;
      }
      result_li.push(ScannedEntry {
        key: key.to_vec(),
        value: decompress_value(iter.value(), iter.compression(), dict)?,
        expire_ts: iter.expire_ts(),
      });
      result_size += entry_size;
      if max_entries != 0 && result_li.len() >= max_entries {
        break;
      }
      has = iter.advance()?;
      continue;
    }
    let data_page = DataPage::new(&page)?;
    let next = data_page.next_page_id()?;
    if next == PageId::MAX {
      break;
    }
    page_id = next;
    let file_page_id = mapper.file_page_id(page_id)?;
    page.fill(0);
    io.read_page(tbl, file_page_id, &mut page).await?;
    iter = DataPageIter::new(&page)?;
    has = iter.advance()?;
  }
  Ok(ScanPage {
    entry_li: result_li,
    has_remaining,
  })
}
