//! index 页重建：脏页合并视图编码、region 直拷快路径、索引页四分页重分配。

use std::collections::BTreeMap;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use fkv_error::{KvError, Result};
use fkv_io::PageIo;
use fkv_mapping::PageMapper;
use fkv_page::{
  FastIndexPageBuilder, INDEX_HEADER_SIZE, IndexPage, IndexPageBuilder, IndexPageIter,
  PageRegionIter, content_len, region_entry_cnt,
};
use fkv_types::{NULL_PAGE, PageId, TableIdent};
use gxhash::HashMap;

use crate::{
  batch::{ChildSource, Children, Frame, IndexLayout, MergedChildren, PendingWrite, ValueCtx},
  rebalance::needs_quarter_rebalance,
};

/// `changes` 替换后子项集合是否与原快照完全一致：逐项比较变更值与旧值，
/// 不复制、不拼接整表（perf-07）。
pub(crate) fn children_unchanged(old: &Children, changes: &[(usize, Children)]) -> bool {
  changes.iter().all(|(idx, new)| {
    new.len() == 1
      && old.page_id(*idx) == new.page_id(0)
      // 首项键在重建语义里恒被清空（leftmost 不存键），不参与比较。
      && (*idx == 0 || old.key(*idx) == new.key(0))
  })
}

/// 合并重建本批所有脏 index 页：每个脏页只编码/入缓冲一次；子项集合未变的页
/// 连同整条祖先链都不落盘（issue 49 写放大）。
///
/// 页内容只进 [`ValueCtx::pending`]，由 `write_batch` 在批末统一落盘（perf-02）。
/// `changes` 的下标相对**本批开始时**的子项快照；页按 `level` 自深向浅处理，
/// 父页总是在其全部子页重建完成后才处理一次。
pub(crate) async fn apply_index_changes<I: PageIo>(
  ctx: &mut ValueCtx<'_, '_, I>,
  mapper: &mut PageMapper,
  frame_map: &HashMap<PageId, Frame>,
  root_id: PageId,
  mut changes: HashMap<PageId, Vec<(usize, Children)>>,
) -> Result<PageId> {
  if changes.is_empty() {
    return Ok(root_id);
  }
  let (io, tbl, layout) = (ctx.io, ctx.tbl, ctx.layout);
  let mut by_level: BTreeMap<usize, Vec<PageId>> = BTreeMap::new();
  for page_id in changes.keys() {
    let level = frame_map.get(page_id).ok_or(KvError::Corrupted)?.level;
    by_level.entry(level).or_default().push(*page_id);
  }
  // `changes` 里未处理的键即「已入队待重建」的页：父页在更浅层、必在本页之后
  // 处理，故用 `contains_key` 判首次入队即可，无需额外集合。
  let mut new_root: Option<Children> = None;
  while let Some((_, page_li)) = by_level.pop_last() {
    for page_id in page_li {
      let frame = frame_map.get(&page_id).ok_or(KvError::Corrupted)?;
      let mut page_changes = changes.remove(&page_id).ok_or(KvError::Corrupted)?;
      // 变更按旧下标升序：合并视图与 region 判定都沿新列表下标推进。
      page_changes.sort_unstable_by_key(|change| change.0);
      if children_unchanged(&frame.child_li, &page_changes) {
        // 子项集合未变：本页与整条祖先链都无需重写。
        continue;
      }
      let merged = MergedChildren::new(&frame.child_li, &page_changes)?;
      if frame.parent.is_none() && merged.len() == 1 && !frame.leaf {
        // 树高收缩：根只剩一个**index** 子页时这一层冗余。直接释放旧根页并把
        // 唯一子页上提为新根，避免先建一个只有单子项的根页写盘再丢弃（每次
        // 收缩多占一页、树高永不下降）。
        //
        // leaf 根（唯一子是 data 页）不折叠：data 根与「单 data 子的 leaf
        // index 根」语义等价，折叠只是把根换成 data 页，下一次写又会由
        // `build_new_index` 重新包一层 leaf index 根，每批白烧一页（r7a-core-03）。
        mapper.free_page(page_id)?;
        new_root = Some(merged.to_children()?);
        continue;
      }
      let index_layout = IndexLayout {
        page: layout,
        leaf: frame.leaf,
      };
      let out_li = build_index_pages(
        mapper,
        &mut ctx.pending,
        IndexRebuild {
          page_id,
          first_key: &frame.first_key,
          old_child_li: &frame.child_li,
          old_page: &frame.page,
          changes: &page_changes,
        },
        index_layout,
      )?;
      // 索引页入批内缓冲；超预算立即落盘，内存有界（perf-02）。
      if ctx.pending.over_budget() {
        ctx.pending.flush(io, tbl).await?;
      }
      let Some((parent_id, child_idx)) = frame.parent else {
        new_root = Some(out_li);
        continue;
      };
      let parent = frame_map.get(&parent_id).ok_or(KvError::Corrupted)?;
      if out_li.len() == 1 && parent.child_li.child(child_idx) == out_li.child(0) {
        // 父节点里这一项没变：不再向上传播。
        continue;
      }
      let parent_is_new = !changes.contains_key(&parent_id);
      changes
        .entry(parent_id)
        .or_default()
        .push((child_idx, out_li));
      if parent_is_new {
        by_level.entry(parent.level).or_default().push(parent_id);
      }
    }
  }
  match new_root {
    None => Ok(root_id),
    Some(child_li) if child_li.is_empty() => Ok(NULL_PAGE),
    Some(child_li) if child_li.len() == 1 => child_li.page_id(0).ok_or(KvError::Corrupted),
    Some(child_li) => {
      let index_layout = IndexLayout {
        page: layout,
        leaf: false,
      };
      build_new_index(io, tbl, mapper, child_li, index_layout, &mut ctx.pending).await
    }
  }
}

/// 页内 entry 条数（不含 header 里的 leftmost）：只走 region 头，不解码键。
pub(crate) fn index_entry_cnt(page: &[u8]) -> Result<usize> {
  PageRegionIter::new(page)?.try_fold(0usize, |sum, region| {
    Ok::<usize, KvError>(sum + usize::from(region_entry_cnt(region)?))
  })
}

pub(crate) fn read_index_children(page: &[u8]) -> Result<(Children, bool)> {
  let index = IndexPage::new(page)?;
  let leaf = index.is_leaf()?;
  // 页级两次分配：键区按整页内容预留，项数组按 entry 条数预留（不再逐键分配）。
  let mut children =
    Children::with_capacity(index_entry_cnt(page)? + 1, usize::from(content_len(page)?));
  children.push(b"", index.leftmost_ptr()?)?;
  let mut iter = IndexPageIter::new(page)?;
  while iter.advance()? {
    children.push(iter.key(), iter.page_id())?;
  }
  Ok((children, leaf))
}

/// index 页重建入参：变更区间 + 旧页快照（供 region 直拷，fmt-17b）。
pub(crate) struct IndexRebuild<'a> {
  /// 待重建的旧页逻辑 id（重建后原位复用，其余分裂页新分配）。
  pub(crate) page_id: PageId,
  /// 该页在父节点中的分隔键；root 为空。
  pub(crate) first_key: &'a [u8],
  /// 本批开始时旧页的子项快照。
  pub(crate) old_child_li: &'a Children,
  /// 本批开始时旧页的原始字节。
  pub(crate) old_page: &'a [u8],
  /// 按旧下标升序的变更；重建时与旧快照合并成虚拟列表，不复制整表。
  pub(crate) changes: &'a [(usize, Children)],
}

pub(crate) fn build_index_pages(
  mapper: &mut PageMapper,
  pending: &mut PendingWrite,
  rebuild: IndexRebuild<'_>,
  layout: IndexLayout<'_>,
) -> Result<Children> {
  let IndexRebuild {
    page_id: old_page_id,
    first_key,
    old_child_li,
    old_page,
    changes,
  } = rebuild;
  let merged = MergedChildren::new(old_child_li, changes)?;
  if merged.is_empty() {
    mapper.free_page(old_page_id)?;
    return Ok(Children::default());
  }
  // 旧页可用时优先 region 直拷（fmt-17b）；不适用则回退逐条重编码。
  let page_li = match build_index_page_region(&merged, old_child_li, old_page, layout)? {
    Some(page) => vec![(first_key.to_vec(), page)],
    None => build_index_page_li(first_key, &merged, layout)?,
  };
  let mut out_li = Children::with_capacity(page_li.len(), page_li.len() * 16);
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
    // 页内容延迟到批末统一落盘（perf-02）；子项只需要逻辑 page_id。
    pending.push(file_page_id, page);
    out_li.push(&first_key, page_id)?;
  }
  Ok(out_li)
}

pub(crate) async fn build_new_index<'a, I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  mut child_li: Children,
  mut layout: IndexLayout<'a>,
  pending: &mut PendingWrite,
) -> Result<PageId> {
  if child_li.is_empty() {
    return Ok(NULL_PAGE);
  }
  loop {
    let page_li = build_index_page_li(&[], &child_li, layout)?;
    let mut out_li = Children::with_capacity(page_li.len(), page_li.len() * 16);
    for (first_key, page) in page_li {
      let page_id = mapper.allocate_page();
      let file_page_id = mapper.file_page_id(page_id)?;
      // 新建索引页与 data 页共用批内缓冲（perf-02）。
      pending.push(file_page_id, page);
      // 大批量首写 / 树高增长可能产生大量页：按预算分批落盘，内存有界。
      if pending.over_budget() {
        pending.flush(io, tbl).await?;
      }
      out_li.push(&first_key, page_id)?;
    }
    if out_li.len() == 1 {
      return out_li.page_id(0).ok_or(KvError::Corrupted);
    }
    child_li = out_li;
    layout.leaf = false;
  }
}

/// 索引页的普通 entry 列表（不含 header leftmost）：键与 page_id 按页内顺序。
pub(crate) fn collect_index_entry_li(page: &[u8]) -> Result<Vec<(Vec<u8>, u32)>> {
  let mut iter = IndexPageIter::new(page)?;
  let mut entry_li = Vec::new();
  while iter.advance()? {
    entry_li.push((iter.key().to_vec(), iter.page_id()));
  }
  Ok(entry_li)
}

/// 索引页版四分之一重分配（对齐 C++ `BatchWriteTask::Redistribute` 索引分支）：
/// 尾页不足 1/4、前页超过 3/4 且前页 restart 数 > 1 时，把前页尾部按 restart
/// region 迁到尾页头部，消除稀疏尾页（页数 / 树高 / 盘面布局对齐 C++）。
///
/// region 边界即 restart 点，搬迁后仍合法，故新尾页自首个迁移 entry 起重新按
/// `restart_interval` 分块，与逐条重建的字节一致。保留区段按 `(prev_len + cur_len)/2`
/// 字节中点选边界（对齐 C++ `average_len`），尾页首键随之改为首个迁移 entry 的键；
/// 返回 `true` 表示该键已变，父 index 分隔键须改用真实首键。
pub(crate) fn rebalance_index_page_li(
  page_li: &mut [(Vec<u8>, Vec<u8>)],
  page_size: usize,
  restart_interval: u16,
  leaf: bool,
) -> Result<bool> {
  let Some(idx) = page_li.len().checked_sub(1).filter(|&idx| idx > 0) else {
    return Ok(false);
  };
  let prev_page = &page_li[idx - 1].1;
  let (cur_key, cur_page) = (&page_li[idx].0, &page_li[idx].1);
  if !needs_quarter_rebalance(prev_page, cur_page, page_size)? {
    return Ok(false);
  }
  let prev_len = usize::from(content_len(prev_page)?);
  let cur_len = usize::from(content_len(cur_page)?);
  let average_len = (prev_len + cur_len) >> 1;
  let region_li: Vec<&[u8]> = PageRegionIter::new(prev_page)?.collect();
  // 保留区段：header + 首个 restart offset 起算，逐 region 累加到平均长度。
  let mut preserve_len = INDEX_HEADER_SIZE + 2;
  let mut preserve_cnt = 0usize;
  while preserve_len < average_len {
    let Some(region) = region_li.get(preserve_cnt) else {
      break;
    };
    preserve_len += region.len() + 2;
    preserve_cnt += 1;
  }
  // 全保留（无法迁移）或没有可迁移 region：保持原状。
  if preserve_len == prev_len || preserve_cnt == 0 || preserve_cnt >= region_li.len() {
    return Ok(false);
  }
  let prev_entry_li = collect_index_entry_li(prev_page)?;
  let cur_entry_li = collect_index_entry_li(cur_page)?;
  let moved_start = region_li[..preserve_cnt]
    .iter()
    .try_fold(0usize, |sum, region| {
      Ok::<usize, KvError>(sum + usize::from(region_entry_cnt(region)?))
    })?;
  let Some(new_leftmost) = prev_entry_li.get(moved_start) else {
    return Ok(false);
  };
  // 新尾页：迁移区段首条 entry 作 leftmost（键丢弃，改作页分隔键）。
  let mut builder = IndexPageBuilder::new(page_size, restart_interval, leaf)?;
  if !builder.add(b"", new_leftmost.1)? {
    return Ok(false);
  }
  for (key, page_id) in &prev_entry_li[moved_start + 1..] {
    if !builder.add(key, *page_id)? {
      return Ok(false);
    }
  }
  // 原尾页 leftmost 指针落成新页首条普通 entry，键用旧分隔键。
  let cur_leftmost = IndexPage::new(cur_page)?.leftmost_ptr()?;
  if !builder.add(cur_key, cur_leftmost)? {
    return Ok(false);
  }
  for (key, page_id) in &cur_entry_li {
    if !builder.add(key, *page_id)? {
      return Ok(false);
    }
  }
  let new_cur = builder.finish()?;
  // 前页只保留已迁出区段之前的 region（region 直拷，字节不变）。
  let mut prev_builder = FastIndexPageBuilder::from_page(prev_page)?;
  for region in &region_li[..preserve_cnt] {
    if !prev_builder.add_region(region)? {
      return Ok(false);
    }
  }
  let new_prev = prev_builder.finish()?;
  page_li[idx - 1].1 = new_prev;
  page_li[idx].0.clone_from(&new_leftmost.0);
  page_li[idx].1 = new_cur;
  Ok(true)
}

pub(crate) fn build_index_page_li<S: ChildSource>(
  first_key: &[u8],
  child_li: &S,
  layout: IndexLayout<'_>,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
  let mut out_li = Vec::new();
  let mut builder = IndexPageBuilder::new(
    layout.page.data.page_size,
    layout.page.index_restart_interval,
    layout.leaf,
  )?;
  let mut page_first_key = first_key.to_vec();
  let (_, first_page_id) = child_li.child(0).ok_or(KvError::Corrupted)?;
  if !builder.add(b"", first_page_id)? {
    return Err(KvError::OutOfSpace);
  }
  for idx in 1..child_li.len() {
    let (key, page_id) = child_li.child(idx).ok_or(KvError::Corrupted)?;
    if builder.add(key, page_id)? {
      continue;
    }
    out_li.push((page_first_key, builder.finish()?));
    builder = IndexPageBuilder::new(
      layout.page.data.page_size,
      layout.page.index_restart_interval,
      layout.leaf,
    )?;
    page_first_key = key.to_vec();
    if !builder.add(b"", page_id)? {
      return Err(KvError::OutOfSpace);
    }
  }
  out_li.push((page_first_key, builder.finish()?));
  // 尾页稀疏时按 restart region 重分配，对齐 C++ `FinishIndexPage`。
  rebalance_index_page_li(
    &mut out_li,
    layout.page.data.page_size,
    layout.page.index_restart_interval,
    layout.leaf,
  )?;
  Ok(out_li)
}

/// 测试用：region 直拷快路径命中次数（仅 `cfg(test)`，无生产开销）。
#[cfg(test)]
pub(crate) static REGION_REUSE_HITS: AtomicUsize = AtomicUsize::new(0);

/// 用旧页 region 直拷 + 变更区段重编码重建**单页** index 页（fmt-17b）：未变更的整段
/// region 直接 `memcpy`，只有首键 / page_id 变化的区段重新 varint 编码。
///
/// 返回 `None` 表示不适用（页型/尺寸不符、旧页 restart 间距与当前配置不一致、单页放不下），
/// 调用方回退 [`build_index_page_li`] 逐条重编码。命中时产物与逐条重编码**逐字节一致**：
/// region 边界即 restart 点（`shared == 0`、page_id 记绝对值），region 字节自包含。
pub(crate) fn build_index_page_region<S: ChildSource, O: ChildSource>(
  child_li: &S,
  old_child_li: &O,
  old_page: &[u8],
  layout: IndexLayout<'_>,
) -> Result<Option<Vec<u8>>> {
  let page_size = layout.page.data.page_size;
  // child_li[0] 是 header 里的 leftmost，不占 entry，单独设置。
  if child_li.is_empty()
    || old_page.len() != page_size
    || IndexPage::new(old_page)?.is_leaf()? != layout.leaf
  {
    return Ok(None);
  }
  let ri = usize::from(layout.page.index_restart_interval);
  if ri == 0 {
    return Ok(None);
  }
  let Ok(regions) = PageRegionIter::new(old_page) else {
    return Ok(None);
  };
  let region_li: Vec<&[u8]> = regions.collect();
  let old_entries = old_child_li.len().saturating_sub(1);
  // 旧页必须按当前 `restart_interval` 均匀切 region，否则 region 边界与 chunk 错位、
  // 直拷会串页。region 数 + 首 region 的 entry 数即可验证均匀间距。
  if region_li.len() != old_entries.div_ceil(ri) {
    return Ok(None);
  }
  let head_ok = match region_li.first() {
    Some(head) => usize::from(region_entry_cnt(head)?) == ri.min(old_entries),
    None => true,
  };
  if !head_ok {
    return Ok(None);
  }
  let mut builder = FastIndexPageBuilder::from_page(old_page)?;
  let (_, leftmost) = child_li.child(0).ok_or(KvError::Corrupted)?;
  builder.set_leftmost(leftmost)?;
  // 变更区段重编码用的可复用 builder（`encode_region` 内部 `reset` 复用缓冲）。
  let mut encoder =
    IndexPageBuilder::new(page_size, layout.page.index_restart_interval, layout.leaf)?;
  let mut entry_li: Vec<(&[u8], u32)> = Vec::new();
  let total = child_li.len() - 1;
  let mut entry_idx = 0usize;
  while entry_idx < total {
    let end = (entry_idx + ri).min(total);
    // child_li 下标比 entry 下标大 1（child_li[0] 是 leftmost）。
    let (a, b) = (entry_idx + 1, end + 1);
    let copy = match region_li.get(entry_idx / ri) {
      Some(region)
        if (a..b).all(|pos| old_child_li.child(pos) == child_li.child(pos))
          // 末区段可能比旧页对应区段短（新页变短），故核对 entry 数。
          && (end != total || usize::from(region_entry_cnt(region)?) == b - a) =>
      {
        Some(*region)
      }
      _ => None,
    };
    let added = match copy {
      Some(region) => builder.add_region(region)?,
      None => {
        entry_li.clear();
        for pos in a..b {
          entry_li.push(child_li.child(pos).ok_or(KvError::Corrupted)?);
        }
        builder.add_region(encoder.encode_region(&entry_li)?)?
      }
    };
    if !added {
      return Ok(None);
    }
    entry_idx = end;
  }
  #[cfg(test)]
  REGION_REUSE_HITS.fetch_add(1, Ordering::Relaxed);
  Ok(Some(builder.finish()?))
}
