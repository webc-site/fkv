//! Index 路径 COW：只重写 root → affected data page 路径上的 index page。
//!
//! `ttl` 子模块提供 TTL 索引 key 编码与写入 batch 生成。

use std::collections::BTreeMap;

use fkv_btree::{BatchEntry, merge_data_pages_with_compression};
use fkv_error::{KvError, Result};
use fkv_io::PageIo;
use fkv_mapping::PageMapper;
use fkv_page::{
  DataPage, DataPageIter, IndexPage, IndexPageBuilder, IndexPageIter, PageType, content_len,
  page_type, set_checksum, set_next_page_id, set_prev_page_id,
};
use fkv_types::{PageId, TableIdent};
use gxhash::HashMap;

pub mod ttl;

pub use ttl::{
  PreparedWriteBatch, is_expired, parse_ttl_key, prepare_write_batch, ttl_key, ttl_upsert_batch,
};

pub const MAX_TREE_DEPTH: usize = 64;

pub struct WriteArgs<'a> {
  pub tbl: &'a TableIdent,
  pub root_id: PageId,
  pub mapper: &'a mut PageMapper,
  pub batch: &'a [BatchEntry],
  pub page_size: usize,
  pub restart_interval: u16,
  pub dict: Option<&'a DictCompression>,
}

#[derive(Clone, Copy)]
struct PageLayout<'a> {
  page_size: usize,
  restart_interval: u16,
  dict: Option<&'a DictCompression>,
}

#[derive(Debug, Clone, Copy)]
struct IndexLayout<'a> {
  page: PageLayout<'a>,
  leaf: bool,
}

#[derive(Debug, Clone)]
struct Child {
  first_key: Vec<u8>,
  page_id: PageId,
  fill_ratio: f32,
}

impl Child {
  fn new(first_key: Vec<u8>, page_id: PageId) -> Self {
    Self {
      first_key,
      page_id,
      fill_ratio: 0.0,
    }
  }
}

struct PathNode {
  page_id: PageId,
  first_key: Vec<u8>,
  children: Vec<Child>,
  child_idx: usize,
}

pub async fn write_batch<I: PageIo>(io: &I, args: WriteArgs<'_>) -> Result<PageId> {
  let WriteArgs {
    tbl,
    mut root_id,
    mapper,
    batch,
    page_size,
    restart_interval,
    dict,
  } = args;
  let page_layout = PageLayout {
    page_size,
    restart_interval,
    dict,
  };
  let mut entry_map = BTreeMap::new();
  for entry in batch {
    entry_map.insert(entry.key.clone(), entry.clone());
  }
  let entry_li: Vec<BatchEntry> = entry_map.into_values().collect();
  if entry_li.is_empty() {
    return Ok(root_id);
  }
  if root_id == PageId::MAX {
    let data_child_li = write_new_data_page(io, tbl, mapper, &entry_li, page_layout).await?;
    let index_layout = IndexLayout {
      page: page_layout,
      leaf: true,
    };
    return build_new_index(io, tbl, mapper, &data_child_li, index_layout).await;
  }

  let mut group_map: HashMap<PageId, Vec<BatchEntry>> = HashMap::default();
  for entry in &entry_li {
    let page_id = find_data_page(io, tbl, root_id, mapper, &entry.key, page_size).await?;
    group_map.entry(page_id).or_default().push(entry.clone());
  }
  let mut group_li: Vec<(Vec<u8>, PageId, Vec<BatchEntry>)> = group_map
    .into_iter()
    .map(|(page_id, entries)| {
      let first_key = entries
        .first()
        .map(|entry| entry.key.clone())
        .unwrap_or_default();
      (first_key, page_id, entries)
    })
    .collect();
  group_li.sort_by(|left, right| left.0.cmp(&right.0));
  for (_, page_id, entries) in group_li {
    root_id = upsert_group(io, tbl, root_id, mapper, &entries, page_id, page_layout).await?;
  }
  Ok(root_id)
}

async fn find_data_page<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  root_id: PageId,
  mapper: &PageMapper,
  key: &[u8],
  page_size: usize,
) -> Result<PageId> {
  let mut page_id = root_id;
  let mut page = vec![0u8; page_size];
  for _ in 0..MAX_TREE_DEPTH {
    let file_page_id = mapper.file_page_id(page_id)?;
    page.fill(0);
    io.read_page(tbl, file_page_id, &mut page).await?;
    match page_type(&page)? {
      PageType::Data => return Ok(page_id),
      PageType::LeafIndex | PageType::NonLeafIndex => {
        let (children, _leaf) = read_index_children(&page)?;
        let child_idx = find_child_index(&children, key);
        page_id = children[child_idx].page_id;
      }
      PageType::Overflow | PageType::Deleted => return Err(KvError::Corrupted),
    }
  }
  Err(KvError::Corrupted)
}

async fn upsert_group<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  root_id: PageId,
  mapper: &mut PageMapper,
  entries: &[BatchEntry],
  expected_page_id: PageId,
  layout: PageLayout<'a>,
) -> Result<PageId> {
  let first_key = entries.first().ok_or(KvError::InvalidArgs)?.key.clone();
  let (page_id, path, page) =
    descend_to_data(io, tbl, root_id, mapper, &first_key, layout.page_size).await?;
  if page_id != expected_page_id {
    return Err(KvError::Corrupted);
  }
  let mut index_layout = IndexLayout {
    page: layout,
    leaf: true,
  };
  let new_data_children =
    rewrite_data_page(io, tbl, mapper, page_id, &page, entries, layout).await?;

  if path.is_empty() {
    return build_new_index(io, tbl, mapper, &new_data_children, index_layout).await;
  }

  let mut child_li = new_data_children;
  let mut underfull = false;
  for node in path.iter().rev() {
    let mut children = node.children.clone();
    replace_child(&mut children, node.child_idx, child_li);
    child_li = build_index_pages(
      io,
      tbl,
      mapper,
      node.page_id,
      &node.first_key,
      &children,
      index_layout,
    )
    .await?;
    if node.page_id != root_id && child_li.iter().any(|child| child.fill_ratio < 0.10) {
      underfull = true;
    }
    index_layout.leaf = false;
  }

  let new_root = match child_li.len() {
    0 => PageId::MAX,
    1 => child_li[0].page_id,
    _ => build_new_index(io, tbl, mapper, &child_li, index_layout).await?,
  };
  if underfull && new_root != PageId::MAX {
    return rebuild_index_all(io, tbl, mapper, new_root, layout).await;
  }
  Ok(new_root)
}

async fn descend_to_data<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  root_id: PageId,
  mapper: &mut PageMapper,
  key: &[u8],
  page_size: usize,
) -> Result<(PageId, Vec<PathNode>, Vec<u8>)> {
  let mut path = Vec::new();
  let mut page_id = root_id;
  let mut node_first_key = Vec::new();
  let mut page = vec![0u8; page_size];
  loop {
    let file_page_id = mapper.file_page_id(page_id)?;
    page.fill(0);
    io.read_page(tbl, file_page_id, &mut page).await?;
    match page_type(&page)? {
      PageType::Data => return Ok((page_id, path, page)),
      PageType::LeafIndex | PageType::NonLeafIndex => {
        let (children, _leaf) = read_index_children(&page)?;
        let child_idx = find_child_index(&children, key);
        let child_page_id = children[child_idx].page_id;
        let child_first_key = children[child_idx].first_key.clone();
        path.push(PathNode {
          page_id,
          first_key: node_first_key,
          children,
          child_idx,
        });
        node_first_key = child_first_key;
        page_id = child_page_id;
      }
      PageType::Overflow | PageType::Deleted => return Err(KvError::Corrupted),
    }
    if path.len() >= MAX_TREE_DEPTH {
      return Err(KvError::Corrupted);
    }
  }
}

async fn rewrite_data_page<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  page_id: PageId,
  old_page: &[u8],
  entries: &[BatchEntry],
  layout: PageLayout<'a>,
) -> Result<Vec<Child>> {
  let old_data = DataPage::new(old_page)?;
  let prev = old_data.prev_page_id()?;
  let next = old_data.next_page_id()?;
  let merged = merge_data_pages_with_compression(
    &[old_page],
    entries,
    layout.page_size,
    layout.restart_interval,
    layout.dict,
  )?;
  if merged.is_empty() {
    mapper.free_page(page_id)?;
    if prev != PageId::MAX {
      update_next(io, tbl, mapper, prev, next, layout.page_size).await?;
    }
    if next != PageId::MAX {
      update_prev(io, tbl, mapper, next, prev, layout.page_size).await?;
    }
    return Ok(Vec::new());
  }

  let mut new_page_id_li = Vec::with_capacity(merged.len());
  new_page_id_li.push(page_id);
  for _ in 1..merged.len() {
    new_page_id_li.push(mapper.allocate_page());
  }

  let mut child_li = Vec::with_capacity(merged.len());
  for (idx, mut new_page) in merged.into_iter().enumerate() {
    let page_prev = if idx == 0 {
      prev
    } else {
      new_page_id_li[idx - 1]
    };
    let page_next = new_page_id_li.get(idx + 1).copied().unwrap_or(next);
    set_prev_page_id(&mut new_page, page_prev)?;
    set_next_page_id(&mut new_page, page_next)?;
    set_checksum(&mut new_page)?;
    let mut iter = DataPageIter::new(&new_page)?;
    if !iter.advance()? {
      return Err(KvError::Corrupted);
    }
    child_li.push(Child::new(iter.key().to_vec(), new_page_id_li[idx]));
    let file_page_id = if idx == 0 {
      mapper.remap_page(page_id)?
    } else {
      mapper.file_page_id(new_page_id_li[idx])?
    };
    io.write_page(tbl, file_page_id, &new_page).await?;
  }

  if prev != PageId::MAX {
    update_next(io, tbl, mapper, prev, new_page_id_li[0], layout.page_size).await?;
  }
  if next != PageId::MAX {
    update_prev(
      io,
      tbl,
      mapper,
      next,
      *new_page_id_li.last().ok_or(KvError::Corrupted)?,
      layout.page_size,
    )
    .await?;
  }
  Ok(child_li)
}

async fn write_new_data_page<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  entries: &[BatchEntry],
  layout: PageLayout<'a>,
) -> Result<Vec<Child>> {
  let merged = merge_data_pages_with_compression(
    &[],
    entries,
    layout.page_size,
    layout.restart_interval,
    layout.dict,
  )?;
  let mut page_id_li = Vec::with_capacity(merged.len());
  for _ in 0..merged.len() {
    page_id_li.push(mapper.allocate_page());
  }
  let mut child_li = Vec::with_capacity(merged.len());
  for (idx, mut page) in merged.into_iter().enumerate() {
    let prev = if idx == 0 {
      PageId::MAX
    } else {
      page_id_li[idx - 1]
    };
    let next = page_id_li.get(idx + 1).copied().unwrap_or(PageId::MAX);
    set_prev_page_id(&mut page, prev)?;
    set_next_page_id(&mut page, next)?;
    set_checksum(&mut page)?;
    let mut iter = DataPageIter::new(&page)?;
    if !iter.advance()? {
      return Err(KvError::Corrupted);
    }
    let first_key = iter.key().to_vec();
    let file_page_id = mapper.file_page_id(page_id_li[idx])?;
    io.write_page(tbl, file_page_id, &page).await?;
    child_li.push(Child::new(first_key, page_id_li[idx]));
  }
  Ok(child_li)
}

struct TreePages {
  data_li: Vec<Child>,
  index_page_li: Vec<PageId>,
}

async fn collect_tree_pages<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  root_id: PageId,
  mapper: &PageMapper,
  page_size: usize,
) -> Result<TreePages> {
  if root_id == PageId::MAX {
    return Ok(TreePages {
      data_li: Vec::new(),
      index_page_li: Vec::new(),
    });
  }
  let mut node_stack = vec![root_id];
  let mut data_li = Vec::new();
  let mut index_page_li = Vec::new();
  let mut page = vec![0u8; page_size];
  while let Some(page_id) = node_stack.pop() {
    let file_page_id = mapper.file_page_id(page_id)?;
    page.fill(0);
    io.read_page(tbl, file_page_id, &mut page).await?;
    match page_type(&page)? {
      PageType::Data => data_li.push(Child::new(Vec::new(), page_id)),
      PageType::LeafIndex | PageType::NonLeafIndex => {
        index_page_li.push(page_id);
        let (children, leaf) = read_index_children(&page)?;
        if leaf {
          data_li.extend(children);
        } else {
          node_stack.extend(children.into_iter().map(|child| child.page_id).rev());
        }
      }
      PageType::Overflow | PageType::Deleted => return Err(KvError::Corrupted),
    }
  }
  Ok(TreePages {
    data_li,
    index_page_li,
  })
}

async fn rebuild_index_all<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  root_id: PageId,
  layout: PageLayout<'a>,
) -> Result<PageId> {
  let tree = collect_tree_pages(io, tbl, root_id, mapper, layout.page_size).await?;
  let index_layout = IndexLayout {
    page: layout,
    leaf: true,
  };
  let new_root = build_new_index(io, tbl, mapper, &tree.data_li, index_layout).await?;
  for page_id in tree.index_page_li {
    mapper.free_page(page_id)?;
  }
  Ok(new_root)
}

async fn update_next<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  page_id: PageId,
  next: PageId,
  page_size: usize,
) -> Result<()> {
  let file_page_id = mapper.file_page_id(page_id)?;
  let mut page = vec![0u8; page_size];
  io.read_page(tbl, file_page_id, &mut page).await?;
  set_next_page_id(&mut page, next)?;
  set_checksum(&mut page)?;
  let new_file_page_id = mapper.remap_page(page_id)?;
  io.write_page(tbl, new_file_page_id, &page).await
}

async fn update_prev<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  page_id: PageId,
  prev: PageId,
  page_size: usize,
) -> Result<()> {
  let file_page_id = mapper.file_page_id(page_id)?;
  let mut page = vec![0u8; page_size];
  io.read_page(tbl, file_page_id, &mut page).await?;
  set_prev_page_id(&mut page, prev)?;
  set_checksum(&mut page)?;
  let new_file_page_id = mapper.remap_page(page_id)?;
  io.write_page(tbl, new_file_page_id, &page).await
}

fn read_index_children(page: &[u8]) -> Result<(Vec<Child>, bool)> {
  let index = IndexPage::new(page)?;
  let leaf = index.is_leaf()?;
  let mut children = vec![Child::new(Vec::new(), index.leftmost_ptr()?)];
  let mut iter = IndexPageIter::new(page)?;
  while iter.advance()? {
    children.push(Child::new(iter.key().to_vec(), iter.page_id()));
  }
  Ok((children, leaf))
}

fn find_child_index(children: &[Child], key: &[u8]) -> usize {
  children
    .partition_point(|child| child.first_key.is_empty() || child.first_key.as_slice() <= key)
    .saturating_sub(1)
}

fn replace_child(children: &mut Vec<Child>, idx: usize, new_child_li: Vec<Child>) {
  children.splice(idx..=idx, new_child_li);
  if let Some(first) = children.first_mut() {
    first.first_key.clear();
  }
}

async fn build_index_pages<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  old_page_id: PageId,
  first_key: &[u8],
  children: &[Child],
  layout: IndexLayout,
) -> Result<Vec<Child>> {
  if children.is_empty() {
    mapper.free_page(old_page_id)?;
    return Ok(Vec::new());
  }
  let page_li = build_index_page_li(first_key, children, layout)?;
  let mut out_li = Vec::with_capacity(page_li.len());
  for (idx, (first_key, page)) in page_li.into_iter().enumerate() {
    let page_id = if idx == 0 {
      old_page_id
    } else {
      mapper.allocate_page()
    };
    let file_page_id = if idx == 0 {
      mapper.remap_page(old_page_id)?
    } else {
      mapper.file_page_id(page_id)?
    };
    let fill_ratio = f32::from(content_len(&page)?) / layout.page.page_size as f32;
    io.write_page(tbl, file_page_id, &page).await?;
    out_li.push(Child {
      first_key,
      page_id,
      fill_ratio,
    });
  }
  Ok(out_li)
}

async fn build_new_index<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  child_li: &[Child],
  mut layout: IndexLayout,
) -> Result<PageId> {
  if child_li.is_empty() {
    return Ok(PageId::MAX);
  }
  let mut children = child_li.to_vec();
  loop {
    let page_li = build_index_page_li(&[], &children, layout)?;
    let mut out_li = Vec::with_capacity(page_li.len());
    for (first_key, page) in page_li {
      let page_id = mapper.allocate_page();
      let file_page_id = mapper.file_page_id(page_id)?;
      let fill_ratio = f32::from(content_len(&page)?) / layout.page.page_size as f32;
      io.write_page(tbl, file_page_id, &page).await?;
      out_li.push(Child {
        first_key,
        page_id,
        fill_ratio,
      });
    }
    if out_li.len() == 1 {
      return Ok(out_li[0].page_id);
    }
    children = out_li;
    layout.leaf = false;
  }
}

fn build_index_page_li(
  first_key: &[u8],
  children: &[Child],
  layout: IndexLayout,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
  let mut out_li = Vec::new();
  let mut builder = IndexPageBuilder::new(
    layout.page.page_size,
    layout.page.restart_interval,
    layout.leaf,
  )?;
  let mut page_first_key = first_key.to_vec();
  if !builder.add(b"", children[0].page_id)? {
    return Err(KvError::OutOfSpace);
  }
  for child in &children[1..] {
    if builder.add(&child.first_key, child.page_id)? {
      continue;
    }
    out_li.push((page_first_key, builder.finish()?));
    builder = IndexPageBuilder::new(
      layout.page.page_size,
      layout.page.restart_interval,
      layout.leaf,
    )?;
    page_first_key = child.first_key.clone();
    if !builder.add(b"", child.page_id)? {
      return Err(KvError::OutOfSpace);
    }
  }
  out_li.push((page_first_key, builder.finish()?));
  Ok(out_li)
}
