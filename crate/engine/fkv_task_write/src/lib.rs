//! 写路径 COW：整批只路由一次，data 页写完后自底向上重建 index 页；子项集合
//! 未变的 index 页（原位覆盖、未分裂/未删空）整条祖先链都不落盘，同一 index 页
//! 每批只重建一次（issue 49）。
//!
//! `ttl` 子模块提供 TTL 索引 key 编码与写入批次生成。

use core::{cmp::Reverse, mem::take};
use std::collections::BTreeMap;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use fkv_btree::{
  BatchEntry, MergeOption, MergeOutput, OverflowSlot, merge_data_pages_with_options,
};
use fkv_compress::DictCompression;
use fkv_error::{KvError, Result};
use fkv_io::PageIo;
use fkv_mapping::PageMapper;
use fkv_page::{
  CompressionType, DataPage, DataPageBuilder, DataPageIter, EntrySpec, FastIndexPageBuilder,
  INDEX_HEADER_SIZE, IndexPage, IndexPageBuilder, IndexPageIter, PageRegionIter, PageType,
  content_len, entry_size as page_entry_size,
  overflow::{
    OverflowPage, build as build_overflow_page, capacity as overflow_capacity, decode_pointers,
    encode_pointers,
  },
  page_content_len, page_type, region_entry_cnt, restart_num, set_checksum, set_next_page_id,
  set_prev_page_id,
};
use fkv_types::{FilePageId, MAX_TREE_DEPTH, NULL_PAGE, PageId, TableIdent};
use gxhash::{HashMap, HashSet};

pub mod ttl;

pub use ttl::{PreparedWriteBatch, is_expired, parse_ttl_key, prepare_write_batch, ttl_key};

pub struct WriteArg<'a> {
  pub tbl: &'a TableIdent,
  pub root_id: PageId,
  pub mapper: &'a mut PageMapper,
  pub batch_li: &'a [BatchEntry],
  /// 页配置：data 侧（`MergeOption`）+ index restart 间隔。
  pub layout: PageLayout<'a>,
  /// 大 value 的 segment 映射：底页上被覆盖/删除的大 value 需要在此释放段映射。
  pub segment_mapper: Option<&'a mut PageMapper>,
  /// segment 定长（释放大 value 时解析编码头用）。
  pub segment_size: u32,
  /// 旧 TTL 索引删除项出口：被覆盖/删除的旧条目若带 `expire_ts`，其
  /// `ttl_key(old_expire, key)` 必须从 TTL 树删除（eng-50）。
  pub ttl_delete_sink: Option<&'a mut Vec<BatchEntry>>,
}

/// 页配置：data 页参数打包为 [`MergeOption`]，index restart 间隔独立。
#[derive(Clone, Copy)]
pub struct PageLayout<'a> {
  /// data 页写入参数（页大小 / restart 间隔 / overflow 指针 / 字典）。
  pub data: MergeOption<'a>,
  /// index 页 restart 间隔（对齐 C++ `KvOption::index_page_restart_interval`）。
  pub index_restart_interval: u16,
}

impl<'a> PageLayout<'a> {
  pub fn new(
    page_size: usize,
    data_restart_interval: u16,
    index_restart_interval: u16,
    dict: Option<&'a DictCompression>,
    overflow_pointers: u8,
  ) -> Self {
    Self {
      data: MergeOption {
        page_size,
        restart_interval: data_restart_interval,
        overflow_pointers,
        dict,
      },
      index_restart_interval,
    }
  }
}

/// 批量写路径的延迟写预算：对齐 C++ `KvOption::write_buffer_size` 默认 1 MB。
///
/// 达到预算即整批落盘，内存上界 = 预算 + 单个 data 页组的页数。
const PENDING_WRITE_BUDGET: usize = 1 << 20;

/// 批内延迟写缓冲（perf-82）：跨 data 页组累积页写，一次 `write_page_li`，
/// 对齐 C++ `WriteBufferAggregator` 的「同一 write task 内累积、`ShouldFlush` 触发」。
///
/// 同一 `FilePageId` 后写覆盖前写：批内对同一页的顺序写以最后一版为准。
/// 批内 read-after-write（分组读旧页、`update_next`/`update_prev`）必须查本缓冲。
#[derive(Default)]
struct PendingWrite {
  page_li: Vec<(FilePageId, Vec<u8>)>,
  slot_map: HashMap<FilePageId, usize>,
  bytes: usize,
}

impl PendingWrite {
  fn push(&mut self, file_page_id: FilePageId, page: Vec<u8>) {
    if let Some(&idx) = self.slot_map.get(&file_page_id) {
      let slot = &mut self.page_li[idx].1;
      self.bytes = self.bytes + page.len() - slot.len();
      *slot = page;
      return;
    }
    self.slot_map.insert(file_page_id, self.page_li.len());
    self.bytes += page.len();
    self.page_li.push((file_page_id, page));
  }

  /// 批内最新版本的页字节；未写过返回 `None`。
  fn get(&self, file_page_id: FilePageId) -> Option<&[u8]> {
    let idx = *self.slot_map.get(&file_page_id)?;
    self.page_li.get(idx).map(|(_, page)| page.as_slice())
  }

  /// 取出并移除；`remap_page` 后内容随逻辑页搬到新 `FilePageId`。
  fn take(&mut self, file_page_id: FilePageId) -> Option<Vec<u8>> {
    let idx = self.slot_map.remove(&file_page_id)?;
    let (_, page) = self.page_li.swap_remove(idx);
    self.bytes -= page.len();
    // `swap_remove` 把尾项搬到 idx：同步它在索引表里的下标。
    if let Some((moved, _)) = self.page_li.get(idx) {
      self.slot_map.insert(*moved, idx);
    }
    Some(page)
  }

  fn over_budget(&self) -> bool {
    self.bytes >= PENDING_WRITE_BUDGET
  }

  /// 整批落盘；一次 `write_page_li` 由 IO 层按 `max_write_batch_pages` 切分。
  async fn flush<I: PageIo>(&mut self, io: &I, tbl: &TableIdent) -> Result<()> {
    if self.page_li.is_empty() {
      return Ok(());
    }
    let page_li = take(&mut self.page_li);
    self.slot_map.clear();
    self.bytes = 0;
    io.write_page_li(tbl, &page_li).await
  }
}

/// 底页重写所需的共享上下文：IO、表、布局，以及大 value 的 segment 映射。
struct ValueCtx<'a, 'io, I: PageIo> {
  io: &'io I,
  tbl: &'io TableIdent,
  layout: PageLayout<'a>,
  segment_mapper: Option<&'io mut PageMapper>,
  segment_size: u32,
  ttl_delete_sink: Option<&'io mut Vec<BatchEntry>>,
  /// 批内延迟写缓冲（perf-82）。
  pending: PendingWrite,
}

#[derive(Clone, Copy)]
struct IndexLayout<'a> {
  page: PageLayout<'a>,
  leaf: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Child {
  first_key: Vec<u8>,
  page_id: PageId,
}

impl Child {
  fn new(first_key: Vec<u8>, page_id: PageId) -> Self {
    Self { first_key, page_id }
  }
}

/// 一个 data 页的整组写入：批量按 key 分桶的结果（issue 29）。
struct WriteGroup<'a> {
  /// 组内最小 key；用于按 key 升序处理各组。
  first_key: &'a [u8],
  page_id: PageId,
  /// 本批开始时 root → 最底层 index 的路径快照 `(index 页, 子项下标)`。
  path_li: Vec<(PageId, usize)>,
  entry_li: Vec<&'a BatchEntry>,
}

/// 单个 index 页在本批开始时的快照。
struct Frame {
  /// 子项快照（`first_key` 升序，首项为空）。
  child_li: Vec<Child>,
  /// 本批开始时该页的原始字节（整页缓冲）；供重建时做 region 直拷（fmt-17b）。
  page: Vec<u8>,
  /// 该页在父节点中的分隔键；root 为空。
  first_key: Vec<u8>,
  /// `(父 index 页, 本页在父节点 child_li 中的下标)`；root 为 `None`。
  parent: Option<(PageId, usize)>,
  /// 到 root 的层数，root 为 0。
  level: usize,
  leaf: bool,
}

pub async fn write_batch<I: PageIo>(io: &I, args: WriteArg<'_>) -> Result<PageId> {
  let WriteArg {
    tbl,
    root_id,
    mapper,
    batch_li,
    layout,
    segment_mapper,
    segment_size,
    ttl_delete_sink,
  } = args;
  if batch_li.is_empty() {
    return Ok(root_id);
  }
  // 同批重复 key：按 timestamp 取较新者（相等时后者胜，对齐原 BTreeMap 语义），
  // 同时保证 key 升序供路由游标单调推进（eng-50）。只排引用下标、原地折叠，
  // 不 clone key / value（perf-79）。
  let mut entry_li: Vec<&BatchEntry> = batch_li.iter().collect();
  // 稳定排序：同 key 的批内先后关系保留，供下面的 timestamp 折叠取“后者”。
  entry_li.sort_by(|left, right| left.key.cmp(&right.key));
  let mut kept_idx = 0usize;
  for read_idx in 1..entry_li.len() {
    let cur = entry_li[read_idx];
    if cur.key == entry_li[kept_idx].key {
      if cur.timestamp >= entry_li[kept_idx].timestamp {
        entry_li[kept_idx] = cur;
      }
      continue;
    }
    kept_idx += 1;
    entry_li[kept_idx] = cur;
  }
  entry_li.truncate(kept_idx + 1);
  let mut ctx = ValueCtx {
    io,
    tbl,
    layout,
    segment_mapper,
    segment_size,
    ttl_delete_sink,
    pending: PendingWrite::default(),
  };
  // data 页之上第一层 index 必为 leaf index。
  let data_layout = IndexLayout {
    page: layout,
    leaf: true,
  };
  if root_id == NULL_PAGE {
    let data_child_li =
      write_new_data_page(io, tbl, mapper, &entry_li, layout, &mut ctx.pending).await?;
    return build_new_index(io, tbl, mapper, &data_child_li, data_layout).await;
  }

  // 一次有序下降到最底层 index，得到 data page 的 key 区间路由表；
  // 之后按 key 直接分桶，避免每个 entry 各自从 root 下降一次（issue 29）。
  let mut route = RouteCursor::open(io, tbl, root_id, mapper, layout.data.page_size).await?;
  if route.data_page != NULL_PAGE {
    // 整棵树只有一个 data 页（root 即 data 页）：所有 key 都落在这一页。
    let page_id = route.data_page;
    let old_page = read_page_by_id(
      io,
      tbl,
      mapper,
      page_id,
      layout.data.page_size,
      &ctx.pending,
    )
    .await?;
    let child_li = rewrite_data_page(&mut ctx, mapper, page_id, &old_page, &entry_li, b"").await?;
    ctx.pending.flush(io, tbl).await?;
    return build_new_index(io, tbl, mapper, &child_li, data_layout).await;
  }
  let mut group_map: HashMap<PageId, usize> = HashMap::default();
  let mut group_li: Vec<WriteGroup<'_>> = Vec::new();
  for entry in entry_li {
    let page_id = route
      .page_for(io, tbl, mapper, layout.data.page_size, &entry.key)
      .await?;
    let index = *group_map.entry(page_id).or_insert_with(|| {
      let index = group_li.len();
      group_li.push(WriteGroup {
        first_key: entry.key.as_slice(),
        page_id,
        path_li: route.path_snapshot(),
        entry_li: Vec::new(),
      });
      index
    });
    group_li[index].entry_li.push(entry);
  }
  // 路由结束：只留本批会改动的 index 页及其祖先（各组路径并集）。
  // 其余帧（已离开的子树）在回溯时已释放，大跨批不再驻留整棵索引（R2-2）。
  route.retain_frames();
  group_li.sort_by(|left, right| left.first_key.cmp(right.first_key));

  // 逐组重写 data 页：路由快照在本阶段保持不变，因为 index 页一次都不写盘。
  // 同父 index 页下的多个 data 页变更在内存中合并，最后自底向上重建一次（issue 49）。
  let mut change_map: HashMap<PageId, Vec<(usize, Vec<Child>)>> = HashMap::default();
  for group in &group_li {
    let (leaf_page_id, child_idx) = *group.path_li.last().ok_or(KvError::Corrupted)?;
    let frame = route
      .frame_map
      .get(&leaf_page_id)
      .ok_or(KvError::Corrupted)?;
    // 每组的父 index 原分隔键：data 页首段沿用，不自作主张抬高（issue 49）。
    let old_sep = frame
      .child_li
      .get(child_idx)
      .ok_or(KvError::Corrupted)?
      .first_key
      .clone();
    // 读覆盖层：前一组可能已改写本页的 prev/next 指针。
    let old_page = read_page_by_id(
      io,
      tbl,
      mapper,
      group.page_id,
      layout.data.page_size,
      &ctx.pending,
    )
    .await?;
    let child_li = rewrite_data_page(
      &mut ctx,
      mapper,
      group.page_id,
      &old_page,
      &group.entry_li,
      &old_sep,
    )
    .await?;
    // 跨组累积到预算即落盘，缓冲内存有界（perf-82）。
    if ctx.pending.over_budget() {
      ctx.pending.flush(io, tbl).await?;
    }
    change_map
      .entry(leaf_page_id)
      .or_default()
      .push((child_idx, child_li));
  }
  // data / overflow 页先落盘，再重建 index 页。
  ctx.pending.flush(io, tbl).await?;
  apply_index_changes(
    io,
    tbl,
    mapper,
    &route.frame_map,
    root_id,
    layout,
    change_map,
  )
  .await
}

struct RouteNode {
  page_id: PageId,
  idx: usize,
}

/// 写路径的 B+tree 游标：从 root 下降到最底层 index，批次 key 升序时
/// 只在必要时向右推进/回溯，整体 O(树高 + 跨页次数)，对齐 C++ `stack_`。
///
/// 游标同时缓存沿路 index 页的子项快照（`frame_map`），供延迟重建阶段使用：
/// 路由期间一个 index 页都不写盘，因此快照在整批内始终有效（issue 49）。
struct RouteCursor {
  /// index 路径（root → leaf index），最后一层是 `LeafIndex`；子项只在 `frame_map` 里存一份。
  path_li: Vec<RouteNode>,
  /// index 页 → 本批开始时的子项快照。
  frame_map: HashMap<PageId, Frame>,
  /// 已被某个写组路径引用的 index 页：回溯离开子树时仍要交给重建阶段。
  keep_set: HashSet<PageId>,
  /// 根即 data 页时直接返回该页；否则为 `NULL_PAGE`。
  data_page: PageId,
}

impl RouteCursor {
  async fn open<I: PageIo>(
    io: &I,
    tbl: &TableIdent,
    root_id: PageId,
    mapper: &PageMapper,
    page_size: usize,
  ) -> Result<Self> {
    let mut cursor = Self {
      path_li: Vec::new(),
      frame_map: HashMap::default(),
      keep_set: HashSet::default(),
      data_page: NULL_PAGE,
    };
    let mut page_id = root_id;
    let mut page = vec![0u8; page_size];
    for _ in 0..MAX_TREE_DEPTH {
      let file_page_id = mapper.file_page_id(page_id)?;
      io.read_page(tbl, file_page_id, &mut page).await?;
      match page_type(&page)? {
        PageType::Data => {
          cursor.data_page = page_id;
          return Ok(cursor);
        }
        PageType::LeafIndex | PageType::NonLeafIndex => {
          let (child_li, leaf) = read_index_children(&page)?;
          let child_page_id = child_li.first().ok_or(KvError::Corrupted)?.page_id;
          cursor.push_frame(page_id, child_li, leaf, page.clone());
          if leaf {
            return Ok(cursor);
          }
          page_id = child_page_id;
        }
        PageType::Overflow | PageType::Deleted => return Err(KvError::Corrupted),
      }
    }
    Err(KvError::Corrupted)
  }

  /// 批次 key 必须升序；返回 key 所属 data page。
  async fn page_for<I: PageIo>(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    mapper: &PageMapper,
    page_size: usize,
    key: &[u8],
  ) -> Result<PageId> {
    if self.path_li.is_empty() {
      return Ok(self.data_page);
    }
    loop {
      let lvl = self.path_li.len() - 1;
      // 当前子树的上界：父节点下一个兄弟的 first_key。
      let in_range = if lvl == 0 {
        true
      } else {
        let (parent_page_id, parent_idx) =
          (self.path_li[lvl - 1].page_id, self.path_li[lvl - 1].idx);
        self
          .frame_map
          .get(&parent_page_id)
          .ok_or(KvError::Corrupted)?
          .child_li
          .get(parent_idx + 1)
          .is_none_or(|next| key < next.first_key.as_slice())
      };
      if in_range {
        let page_id = self.path_li[lvl].page_id;
        let frame = self.frame_map.get(&page_id).ok_or(KvError::Corrupted)?;
        let mut idx = self.path_li[lvl].idx;
        while let Some(next) = frame.child_li.get(idx + 1) {
          if next.first_key.is_empty() || next.first_key.as_slice() > key {
            break;
          }
          idx += 1;
        }
        let child_page_id = frame.child_li.get(idx).ok_or(KvError::Corrupted)?.page_id;
        self.path_li[lvl].idx = idx;
        return Ok(child_page_id);
      }
      // key 超出当前子树：回溯到能覆盖它的祖先，再向左下降。
      self.pop_frame();
      let mut advanced = false;
      while let Some(top) = self.path_li.last_mut() {
        let child_num = self
          .frame_map
          .get(&top.page_id)
          .ok_or(KvError::Corrupted)?
          .child_li
          .len();
        if top.idx + 1 < child_num {
          top.idx += 1;
          advanced = true;
          break;
        }
        self.pop_frame();
      }
      if !advanced {
        return Err(KvError::Corrupted);
      }
      self.descend_leftmost(io, tbl, mapper, page_size).await?;
    }
  }

  /// 丢弃不在各写组路径上的 index 帧。
  ///
  /// 重建阶段只需要脏页（即各组路径上的 leaf index）与其祖先：`parent` 链
  /// 保证祖先必在各组路径里。回溯时已按 `keep_set` 即时释放，这里只做终检。
  fn retain_frames(&mut self) {
    let keep_set = take(&mut self.keep_set);
    self
      .frame_map
      .retain(|page_id, _| keep_set.contains(page_id));
    self.keep_set = keep_set;
  }

  /// 回溯一层：没有任何写组引用该帧时立即释放，路由期内存降为 O(树高)。
  fn pop_frame(&mut self) {
    if let Some(node) = self.path_li.pop()
      && !self.keep_set.contains(&node.page_id)
    {
      self.frame_map.remove(&node.page_id);
    }
  }

  /// 当前路径快照：`(index 页, 子项下标)`，自 root 到 leaf index。
  /// 同时把这些帧登记为不可释放（重建阶段要用）。
  fn path_snapshot(&mut self) -> Vec<(PageId, usize)> {
    self
      .keep_set
      .extend(self.path_li.iter().map(|node| node.page_id));
    self
      .path_li
      .iter()
      .map(|node| (node.page_id, node.idx))
      .collect()
  }

  /// 压入一层 index 帧；父帧一定已建立（下降方向自 root 向下）。
  fn push_frame(&mut self, page_id: PageId, child_li: Vec<Child>, leaf: bool, page: Vec<u8>) {
    let parent = self.path_li.last().map(|node| (node.page_id, node.idx));
    let parent_frame = parent.and_then(|(parent_id, _)| self.frame_map.get(&parent_id));
    let first_key = match (parent, parent_frame) {
      (Some((_, child_idx)), Some(frame)) => frame
        .child_li
        .get(child_idx)
        .map_or_else(Vec::new, |child| child.first_key.clone()),
      _ => Vec::new(),
    };
    let level = parent_frame.map_or(0, |frame| frame.level + 1);
    self.frame_map.insert(
      page_id,
      Frame {
        child_li,
        page,
        first_key,
        parent,
        level,
        leaf,
      },
    );
    self.path_li.push(RouteNode { page_id, idx: 0 });
  }

  /// 从当前节点最左子页下降到 `LeafIndex` 层并压栈。
  async fn descend_leftmost<I: PageIo>(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    mapper: &PageMapper,
    page_size: usize,
  ) -> Result<()> {
    let mut page = vec![0u8; page_size];
    loop {
      if self.path_li.len() >= MAX_TREE_DEPTH {
        return Err(KvError::Corrupted);
      }
      let (top_page_id, top_idx) = {
        let top = self.path_li.last().ok_or(KvError::Corrupted)?;
        (top.page_id, top.idx)
      };
      let child_page_id = self
        .frame_map
        .get(&top_page_id)
        .ok_or(KvError::Corrupted)?
        .child_li
        .get(top_idx)
        .ok_or(KvError::Corrupted)?
        .page_id;
      // 快照里已有该页（回溯重访）：直接压栈，避免重复读盘。
      if let Some(frame) = self.frame_map.get(&child_page_id) {
        let leaf = frame.leaf;
        self.path_li.push(RouteNode {
          page_id: child_page_id,
          idx: 0,
        });
        if leaf {
          return Ok(());
        }
        continue;
      }
      let file_page_id = mapper.file_page_id(child_page_id)?;
      io.read_page(tbl, file_page_id, &mut page).await?;
      match page_type(&page)? {
        PageType::LeafIndex => {
          let (child_li, _) = read_index_children(&page)?;
          self.push_frame(child_page_id, child_li, true, page.clone());
          return Ok(());
        }
        PageType::NonLeafIndex => {
          let (child_li, _) = read_index_children(&page)?;
          self.push_frame(child_page_id, child_li, false, page.clone());
        }
        // index 页只能指向 index 页：data 页只允许出现在 root。
        PageType::Data | PageType::Overflow | PageType::Deleted => {
          return Err(KvError::Corrupted);
        }
      }
    }
  }
}

/// 合并重建本批所有脏 index 页：每个脏页只编码/写盘一次；子项集合未变的页
/// 连同整条祖先链都不落盘（issue 49 写放大）。
///
/// `changes` 的下标相对**本批开始时**的子项快照；页按 `level` 自深向浅处理，
/// 父页总是在其全部子页重建完成后才处理一次。
async fn apply_index_changes<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  frame_map: &HashMap<PageId, Frame>,
  root_id: PageId,
  layout: PageLayout<'_>,
  mut changes: HashMap<PageId, Vec<(usize, Vec<Child>)>>,
) -> Result<PageId> {
  if changes.is_empty() {
    return Ok(root_id);
  }
  let mut by_level: BTreeMap<usize, Vec<PageId>> = BTreeMap::new();
  for page_id in changes.keys() {
    let level = frame_map.get(page_id).ok_or(KvError::Corrupted)?.level;
    by_level.entry(level).or_default().push(*page_id);
  }
  let mut queued: HashSet<PageId> = changes.keys().copied().collect();
  let mut new_root: Option<Vec<Child>> = None;
  while let Some((_, page_li)) = by_level.pop_last() {
    for page_id in page_li {
      let frame = frame_map.get(&page_id).ok_or(KvError::Corrupted)?;
      let mut child_li = frame.child_li.clone();
      let mut page_changes = changes.remove(&page_id).ok_or(KvError::Corrupted)?;
      // 深位子项先替换，浅位下标才不会漂移。
      page_changes.sort_unstable_by_key(|left| Reverse(left.0));
      for (child_idx, new_child_li) in page_changes {
        replace_child(&mut child_li, child_idx, new_child_li);
      }
      if child_li == frame.child_li {
        // 子项集合未变：本页与整条祖先链都无需重写。
        continue;
      }
      if frame.parent.is_none() && child_li.len() == 1 && !frame.leaf {
        // 树高收缩：根只剩一个**index** 子页时这一层冗余。直接释放旧根页并把
        // 唯一子页上提为新根，避免先建一个只有单子项的根页写盘再丢弃（每次
        // 收缩多占一页、树高永不下降）。
        //
        // leaf 根（唯一子是 data 页）不折叠：data 根与「单 data 子的 leaf
        // index 根」语义等价，折叠只是把根换成 data 页，下一次写又会由
        // `build_new_index` 重新包一层 leaf index 根，每批白烧一页（r7a-core-03）。
        mapper.free_page(page_id)?;
        new_root = Some(child_li);
        continue;
      }
      let index_layout = IndexLayout {
        page: layout,
        leaf: frame.leaf,
      };
      let out_li = build_index_pages(
        io,
        tbl,
        mapper,
        IndexRebuild {
          page_id,
          first_key: &frame.first_key,
          child_li: &child_li,
          old_child_li: &frame.child_li,
          old_page: &frame.page,
        },
        index_layout,
      )
      .await?;
      let Some((parent_id, child_idx)) = frame.parent else {
        new_root = Some(out_li);
        continue;
      };
      let parent = frame_map.get(&parent_id).ok_or(KvError::Corrupted)?;
      if out_li.len() == 1 && parent.child_li.get(child_idx) == out_li.first() {
        // 父节点里这一项没变：不再向上传播。
        continue;
      }
      changes
        .entry(parent_id)
        .or_default()
        .push((child_idx, out_li));
      if queued.insert(parent_id) {
        by_level.entry(parent.level).or_default().push(parent_id);
      }
    }
  }
  match new_root {
    None => Ok(root_id),
    Some(child_li) if child_li.is_empty() => Ok(NULL_PAGE),
    Some(child_li) if child_li.len() == 1 => Ok(child_li[0].page_id),
    Some(child_li) => {
      let index_layout = IndexLayout {
        page: layout,
        leaf: false,
      };
      build_new_index(io, tbl, mapper, &child_li, index_layout).await
    }
  }
}

async fn read_page_by_id<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &PageMapper,
  page_id: PageId,
  page_size: usize,
  pending: &PendingWrite,
) -> Result<Vec<u8>> {
  let file_page_id = mapper.file_page_id(page_id)?;
  // 批内已改写的页以缓冲里的最新版本为准。
  if let Some(page) = pending.get(file_page_id) {
    return Ok(page.to_vec());
  }
  let mut page = vec![0u8; page_size];
  io.read_page(tbl, file_page_id, &mut page).await?;
  Ok(page)
}

/// 释放被本批 delete / 覆盖的 overflow 页链与大 value segment（对齐 C++
/// `ApplyOnePage` 的 `DelOverflowValue` / `DelLargeValue`）。
async fn free_replaced_values<I: PageIo>(
  ctx: &mut ValueCtx<'_, '_, I>,
  mapper: &mut PageMapper,
  old_page: &[u8],
  batch_li: &[&BatchEntry],
) -> Result<()> {
  let io = ctx.io;
  let tbl = ctx.tbl;
  let page_size = ctx.layout.data.page_size;
  let batch_map: HashMap<&[u8], &BatchEntry> = batch_li
    .iter()
    .map(|entry| (entry.key.as_slice(), *entry))
    .collect();
  if batch_map.is_empty() {
    return Ok(());
  }
  let mut iter = DataPageIter::new(old_page)?;
  while iter.advance()? {
    let Some(batch_li) = batch_map.get(iter.key()) else {
      continue;
    };
    // 只有 batch_li 真的覆盖了旧条目（同序或更新）才能释放旧值与旧 TTL 索引项：
    // 更旧时间戳的 delete 会被 merge 丢弃（旧条目存活，见 `fkv_btree` 的
    // `kept.timestamp < iter.timestamp()` 分支），此时释放会让存活条目悬垂、
    // 旧 TTL 索引被清掉导致过期永不触发（sto-65）。
    if batch_li.timestamp < iter.timestamp() {
      continue;
    }
    if iter.is_overflow() {
      free_overflow_chain(io, tbl, mapper, iter.value(), page_size).await?;
    } else if iter.is_large_value()
      && let Some(segment_mapper) = ctx.segment_mapper.as_deref_mut()
    {
      fkv_large::free_value(segment_mapper, iter.value(), ctx.segment_size)?;
    }
    // 旧 TTL 索引项：只有 expire_ts 真的变了才删旧项（对齐 C++ `base_expire != expire_ts`）。
    // 相同 expire 的覆盖若不跳过，新批的 `upsert ttl_key(E,k)`（时间戳恒为 0）
    // 会被这里推的 `delete ttl_key(E,k)`（时间戳 > 0）在同 key 折叠里压掉 →
    // TTL 条目消失、到期后 `clean_expired` 扫不到、空间无界泄漏（sto-66）。
    let new_expire = if batch_li.op.is_delete() {
      0
    } else {
      batch_li.expire_ts
    };
    if iter.expire_ts() != 0
      && iter.expire_ts() != new_expire
      && let Some(sink) = ctx.ttl_delete_sink.as_deref_mut()
    {
      let key = ttl::ttl_key(iter.expire_ts(), iter.key());
      sink.push(BatchEntry::delete(key, batch_li.timestamp));
    }
  }
  Ok(())
}

/// 沿 overflow 页链逐页 free；`seen` 防止损坏盘面成环导致死循环。
async fn free_overflow_chain<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  encoded_pointer_li: &[u8],
  page_size: usize,
) -> Result<()> {
  let mut pending = decode_pointers(encoded_pointer_li)?;
  let mut seen = HashSet::default();
  let mut page = vec![0u8; page_size];
  while let Some(page_id) = pending.pop() {
    if !seen.insert(page_id) {
      continue;
    }
    let file_page_id = mapper.file_page_id(page_id)?;
    io.read_page(tbl, file_page_id, &mut page).await?;
    let overflow = OverflowPage::new(&page)?;
    pending.extend(overflow.pointer_li()?);
    mapper.free_page(page_id)?;
  }
  Ok(())
}

/// 重写一个 data 页；返回该页及其分裂页在父 index 中的子项。
///
/// `sep` 为该 data 页在父 index 中原有的分隔键：首段沿用它（对齐 C++
/// `BatchWriteTask::FinishDataPage` 里 `idx_page_iter_.Key()`），既不抬高分隔键
/// 影响路由，也让未分裂的原位更新完全不必改父 index（issue 49）。
async fn rewrite_data_page<I: PageIo>(
  ctx: &mut ValueCtx<'_, '_, I>,
  mapper: &mut PageMapper,
  page_id: PageId,
  old_page: &[u8],
  batch_li: &[&BatchEntry],
  sep: &[u8],
) -> Result<Vec<Child>> {
  let io = ctx.io;
  let tbl = ctx.tbl;
  let layout = ctx.layout;
  let old_data = DataPage::new(old_page)?;
  let prev = old_data.prev_page_id()?;
  let next = old_data.next_page_id()?;
  // 旧版本已由调用方读入 `old_page`：丢弃缓冲里的同名项，避免向已释放的
  // `FilePageId` 重复写（perf-82 延迟写）。
  drop(ctx.pending.take(mapper.file_page_id(page_id)?));
  let MergeOutput {
    mut page_li,
    mut overflow_li,
  } = merge_data_pages_with_options(&[old_page], batch_li, layout.data)?;
  // 对齐 C++ `FinishDataPage` 的四分页重分配：新页 < 1/4 且前页 > 3/4 时，
  // 在两页间重新分配 entry，避免产生极小尾页（不再全树重建）。
  let redistributed_li = rebalance_data_page_li(
    &mut page_li,
    layout.data.page_size,
    layout.data.restart_interval,
    &mut overflow_li,
  )?;
  // 被本批 delete / 覆盖的 overflow 页链与大 value 段：一并释放（09a / eng-51）。
  free_replaced_values(ctx, mapper, old_page, batch_li).await?;
  if page_li.is_empty() {
    mapper.free_page(page_id)?;
    if prev != NULL_PAGE {
      update_next(
        io,
        tbl,
        mapper,
        prev,
        next,
        layout.data.page_size,
        &mut ctx.pending,
      )
      .await?;
    }
    if next != NULL_PAGE {
      update_prev(
        io,
        tbl,
        mapper,
        next,
        prev,
        layout.data.page_size,
        &mut ctx.pending,
      )
      .await?;
    }
    return Ok(Vec::new());
  }
  // 先落 overflow 页，再把首批指针回填到 data page 占位区。
  write_overflow_value_li(
    io,
    tbl,
    mapper,
    &mut page_li,
    &overflow_li,
    layout,
    &mut ctx.pending,
  )
  .await?;

  let mut new_page_id_li = Vec::with_capacity(page_li.len());
  new_page_id_li.push(page_id);
  for _ in 1..page_li.len() {
    new_page_id_li.push(mapper.allocate_page());
  }

  // 分裂后各页的分隔键：首段沿用父 index 原分隔键，其余段用最短分隔键；
  // 被四分页重分配改写过首键的新页改用真实首键。
  let sep_li = page_separator_li(&page_li, sep, &redistributed_li)?;
  let mut child_li = Vec::with_capacity(page_li.len());
  for ((idx, mut new_page), child_sep) in page_li.into_iter().enumerate().zip(sep_li) {
    let page_prev = if idx == 0 {
      prev
    } else {
      new_page_id_li[idx - 1]
    };
    let page_next = new_page_id_li.get(idx + 1).copied().unwrap_or(next);
    set_prev_page_id(&mut new_page, page_prev)?;
    set_next_page_id(&mut new_page, page_next)?;
    set_checksum(&mut new_page)?;
    child_li.push(Child::new(child_sep, new_page_id_li[idx]));
    let file_page_id = if idx == 0 {
      mapper.remap_page(page_id)?
    } else {
      mapper.file_page_id(new_page_id_li[idx])?
    };
    // 跨组累积到批内缓冲，由批量 flush 合并成更少的 `write_page_li`（perf-82）。
    ctx.pending.push(file_page_id, new_page);
  }

  // idx 0 复用原 `page_id`，`prev.next` 不变；未分裂时 `next.prev` 也不变。
  // 仅分裂（页数 > 1）时才需要改 `next.prev` 指向新的尾页（issue 29 写放大）。
  if new_page_id_li.len() > 1 && next != NULL_PAGE {
    update_prev(
      io,
      tbl,
      mapper,
      next,
      *new_page_id_li.last().ok_or(KvError::Corrupted)?,
      layout.data.page_size,
      &mut ctx.pending,
    )
    .await?;
  }
  Ok(child_li)
}

async fn write_new_data_page<'a, I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  batch_li: &[&BatchEntry],
  layout: PageLayout<'a>,
  pending: &mut PendingWrite,
) -> Result<Vec<Child>> {
  let MergeOutput {
    mut page_li,
    mut overflow_li,
  } = merge_data_pages_with_options(&[], batch_li, layout.data)?;
  // 空表首写同样做四分页重分配（对齐 C++ `FinishDataPage` 对新页的处理），
  // 否则尾页可稀疏到 1 条。分隔键由调用者按真实首键生成，无需此处的重分配标记。
  rebalance_data_page_li(
    &mut page_li,
    layout.data.page_size,
    layout.data.restart_interval,
    &mut overflow_li,
  )?;
  write_overflow_value_li(io, tbl, mapper, &mut page_li, &overflow_li, layout, pending).await?;
  let mut page_id_li = Vec::with_capacity(page_li.len());
  for _ in 0..page_li.len() {
    page_id_li.push(mapper.allocate_page());
  }
  let mut child_li = Vec::with_capacity(page_li.len());
  for (idx, mut page) in page_li.into_iter().enumerate() {
    let prev = if idx == 0 {
      NULL_PAGE
    } else {
      page_id_li[idx - 1]
    };
    let next = page_id_li.get(idx + 1).copied().unwrap_or(NULL_PAGE);
    set_prev_page_id(&mut page, prev)?;
    set_next_page_id(&mut page, next)?;
    set_checksum(&mut page)?;
    let mut iter = DataPageIter::new(&page)?;
    if !iter.advance()? {
      return Err(KvError::Corrupted);
    }
    let first_key = iter.key().to_vec();
    let file_page_id = mapper.file_page_id(page_id_li[idx])?;
    pending.push(file_page_id, page);
    // 大批量首写可能产生大量新页：按预算分批落盘，内存有界。
    if pending.over_budget() {
      pending.flush(io, tbl).await?;
    }
    child_li.push(Child::new(first_key, page_id_li[idx]));
  }
  // 余下的页一次落盘：连续页由 IO 层按 `max_write_batch_pages` 合并。
  pending.flush(io, tbl).await?;
  Ok(child_li)
}

/// 把 merge 出来的 overflow entry 落盘：先写 overflow 页链，再把首批指针
/// 回填到 data page 里等长的占位区，最后重算 checksum。
async fn write_overflow_value_li<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  page_li: &mut [Vec<u8>],
  overflow_li: &[OverflowSlot],
  layout: PageLayout<'_>,
  pending: &mut PendingWrite,
) -> Result<()> {
  if overflow_li.is_empty() {
    return Ok(());
  }
  // 按 (page_idx, key) 排序 slot 下标：同一页只遍历一次，一次定位全部 value 区间。
  let mut order_li: Vec<usize> = (0..overflow_li.len()).collect();
  order_li.sort_unstable_by(|&a, &b| {
    overflow_li[a]
      .page_idx
      .cmp(&overflow_li[b].page_idx)
      .then_with(|| overflow_li[a].key.cmp(&overflow_li[b].key))
  });
  let mut range_li = vec![None; overflow_li.len()];
  let mut start = 0;
  while start < order_li.len() {
    let page_idx = overflow_li[order_li[start]].page_idx;
    let mut end = start + 1;
    while end < order_li.len() && overflow_li[order_li[end]].page_idx == page_idx {
      end += 1;
    }
    let key_li: Vec<&[u8]> = order_li[start..end]
      .iter()
      .map(|&slot_idx| overflow_li[slot_idx].key.as_slice())
      .collect();
    let page = page_li.get(page_idx).ok_or(KvError::Corrupted)?;
    for (offset, range) in locate_value_range_li(page, &key_li)?
      .into_iter()
      .enumerate()
    {
      range_li[order_li[start + offset]] = range;
    }
    start = end;
  }
  for (slot_idx, slot) in overflow_li.iter().enumerate() {
    let pointer_bytes = build_overflow_chain(io, tbl, mapper, &slot.value, layout, pending).await?;
    let (begin, end) = range_li[slot_idx].ok_or(KvError::Corrupted)?;
    let page = page_li.get_mut(slot.page_idx).ok_or(KvError::Corrupted)?;
    let dst = page.get_mut(begin..end).ok_or(KvError::Corrupted)?;
    if dst.len() != pointer_bytes.len() {
      return Err(KvError::Corrupted);
    }
    dst.copy_from_slice(&pointer_bytes);
    set_checksum(page)?;
  }
  Ok(())
}

/// 单遍定位一页内多个 key 的 value 区间；`key_li` 须按字节升序。
/// 页内不存在的 key 对应位置返回 `None`，由调用方按损坏处理。
fn locate_value_range_li(page: &[u8], key_li: &[&[u8]]) -> Result<Vec<Option<(usize, usize)>>> {
  let mut range_li = vec![None; key_li.len()];
  let mut iter = DataPageIter::new(page)?;
  let mut next = 0;
  while next < key_li.len() && iter.advance()? {
    let key = iter.key();
    // 页内 key 严格升序：跳过页里没有的目标键。
    while next < key_li.len() && key_li[next] < key {
      next += 1;
    }
    if next == key_li.len() {
      break;
    }
    if key_li[next] == key {
      range_li[next] = Some(iter.value_range());
      next += 1;
    }
  }
  Ok(range_li)
}

/// 构造 overflow 多级链：每批最多 `overflow_pointers` 页，该批末页尾部存
/// 指向下一批的指针（容量变小）；返回首批页指针的编码（放入 leaf entry）。
///
/// 每构造完一页立即进批内缓冲，超预算即落盘：整链攒在临时 Vec 里再搬会让
/// 峰值内存达到 2×value，大 value 可击穿 [`PENDING_WRITE_BUDGET`]。
///
/// 对齐 C++ `BatchWriteTask::WriteOverflowValue`。
async fn build_overflow_chain<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  value: &[u8],
  layout: PageLayout<'_>,
  pending: &mut PendingWrite,
) -> Result<Vec<u8>> {
  let page_cap = overflow_capacity(layout.data.page_size, layout.data.overflow_pointers, false)?;
  let end_cap = overflow_capacity(layout.data.page_size, layout.data.overflow_pointers, true)?;
  let group_max = usize::from(layout.data.overflow_pointers);
  // 任一批量容量为 0 都会让 `len = 0`：写不出 value 却持续分配页，
  // 退化成死循环 + 页分配无界。极端 page_size 在入口直接拒绝。
  if page_cap == 0 || end_cap == 0 || group_max == 0 {
    return Err(KvError::InvalidArgs);
  }
  let mut head_li: Option<Vec<PageId>> = None;
  // 本批末页：等下一批指针分配完后才能构造。
  let mut end_page: Option<(PageId, Vec<u8>)> = None;
  let mut pos = 0usize;
  while pos < value.len() {
    let group_size = (value.len() - pos).div_ceil(page_cap).min(group_max);
    let id_li: Vec<PageId> = (0..group_size).map(|_| mapper.allocate_page()).collect();
    if head_li.is_none() {
      head_li = Some(id_li.clone());
    }
    if let Some((end_page_id, end_value)) = end_page.take() {
      let page = build_overflow_page(
        layout.data.page_size,
        &end_value,
        &id_li,
        layout.data.overflow_pointers,
      )?;
      push_overflow_page(io, tbl, mapper, pending, end_page_id, page).await?;
    }
    for (idx, page_id) in id_li.iter().enumerate() {
      let remaining = value.len() - pos;
      if idx + 1 == group_max && remaining > page_cap {
        // 本批末页要存下一批指针，可装 value 变少。
        let len = end_cap.min(remaining);
        if len == 0 {
          return Err(KvError::InvalidArgs);
        }
        end_page = Some((*page_id, value[pos..pos + len].to_vec()));
        pos += len;
        break;
      }
      let len = page_cap.min(remaining);
      if len == 0 {
        return Err(KvError::InvalidArgs);
      }
      let page = build_overflow_page(
        layout.data.page_size,
        &value[pos..pos + len],
        &[],
        layout.data.overflow_pointers,
      )?;
      push_overflow_page(io, tbl, mapper, pending, *page_id, page).await?;
      pos += len;
    }
  }
  if let Some((end_page_id, end_value)) = end_page {
    // 正常不会走到：end_page 只在还有后续 value 时设置。
    let page = build_overflow_page(
      layout.data.page_size,
      &end_value,
      &[],
      layout.data.overflow_pointers,
    )?;
    push_overflow_page(io, tbl, mapper, pending, end_page_id, page).await?;
  }
  let head_li = head_li.ok_or(KvError::InvalidArgs)?;
  Ok(encode_pointers(&head_li))
}

/// 单个 overflow 页入批内缓冲；达到预算立即落盘，链式构造内存有界。
async fn push_overflow_page<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &PageMapper,
  pending: &mut PendingWrite,
  page_id: PageId,
  page: Vec<u8>,
) -> Result<()> {
  pending.push(mapper.file_page_id(page_id)?, page);
  if pending.over_budget() {
    pending.flush(io, tbl).await?;
  }
  Ok(())
}

/// 反序列化出一个 data page 的全部 entry（value 保持页内压缩字节）。
struct RawEntry {
  key: Vec<u8>,
  value: Vec<u8>,
  ptr: bool,
  ts: u64,
  expire: u64,
  comp: CompressionType,
  large: bool,
}

fn collect_entry_li(page: &[u8]) -> Result<Vec<RawEntry>> {
  let mut out = Vec::new();
  let mut iter = DataPageIter::new(page)?;
  while iter.advance()? {
    out.push(RawEntry {
      key: iter.key().to_vec(),
      value: iter.value().to_vec(),
      ptr: iter.is_overflow(),
      ts: iter.timestamp(),
      expire: iter.expire_ts(),
      comp: iter.compression(),
      large: iter.is_large_value(),
    });
  }
  Ok(out)
}

/// 用给定 entry 构建单页；放不下返回 `None`。
fn build_raw_page(
  entry_li: &[RawEntry],
  page_size: usize,
  restart_interval: u16,
) -> Result<Option<Vec<u8>>> {
  let mut builder = DataPageBuilder::new(page_size, restart_interval)?;
  for entry in entry_li {
    if !builder.add(EntrySpec {
      key: &entry.key,
      value: &entry.value,
      ts: entry.ts,
      expire_ts: entry.expire,
      overflow: entry.ptr,
      compression: entry.comp,
      large_value: entry.large,
    })? {
      return Ok(None);
    }
  }
  if builder.is_empty() {
    return Ok(None);
  }
  Ok(Some(builder.finish()?))
}

/// 索引页重分配阈值（页大小四分位）。
const fn rebalance_bounds(page_size: usize) -> (usize, usize) {
  let quarter = page_size / 4;
  (quarter, page_size - quarter)
}

/// 四分页重分配判定：后页不足 1/4、前页超过 3/4 且前页 restart 数 > 1。
///
/// 数据页与索引页共用同一口径，避免两套阈值各自漂移（C++ `FinishDataPage` /
/// `FinishIndexPage` 同为 quarter / three_quarter 判定）。
fn needs_quarter_rebalance(prev: &[u8], cur: &[u8], page_size: usize) -> Result<bool> {
  let (quarter, three_quarter) = rebalance_bounds(page_size);
  Ok(
    usize::from(content_len(cur)?) < quarter
      && usize::from(content_len(prev)?) > three_quarter
      && restart_num(prev)? > 1,
  )
}

/// 对齐 C++ `BatchWriteTask::Redistribute`：当后页不足 1/4、前页超过 3/4
/// 且前页 restart 数 > 1 时，把两页的 entry 重新按字节均分，消除极小尾页。
/// 切分点以 `(prev_len + cur_len) / 2` 字节中点为起点（对齐 C++ `average_len`），
/// 而非条目数中点，避免变长 entry 下切出不均页。
///
/// `overflow_li` 中落在被重分配两页的 slot 按新切分键重定位 `page_idx`；必须早于
/// `write_overflow_value_li` 调用，此时页内仍是等长占位指针，原地迁移不改变编码。
/// 返回与 `page_li` 等长的标记：`true` 表示该页作为重分配新页被替换、首键已变，
/// 其父 index 分隔键需改用真实首键（见 `page_separator_li`）。
fn rebalance_data_page_li(
  page_li: &mut [Vec<u8>],
  page_size: usize,
  restart_interval: u16,
  overflow_li: &mut [OverflowSlot],
) -> Result<Vec<bool>> {
  let mut redistributed_li = vec![false; page_li.len()];
  if page_li.len() < 2 {
    return Ok(redistributed_li);
  }
  for idx in 1..page_li.len() {
    if !needs_quarter_rebalance(&page_li[idx - 1], &page_li[idx], page_size)? {
      continue;
    }
    let prev_len = usize::from(content_len(&page_li[idx - 1])?);
    let cur_len = usize::from(content_len(&page_li[idx])?);
    let mut entry_li = collect_entry_li(&page_li[idx - 1])?;
    entry_li.extend(collect_entry_li(&page_li[idx])?);
    let n = entry_li.len();
    if n < 2 {
      continue;
    }
    let target = (prev_len + cur_len) >> 1;
    let Some(k) = pick_split(&entry_li, target, page_size, restart_interval)? else {
      continue;
    };
    // 估算已保证两侧各装得下：这里每页只实际编码一次。
    let Some(first) = build_raw_page(&entry_li[..k], page_size, restart_interval)? else {
      continue;
    };
    let Some(second) = build_raw_page(&entry_li[k..], page_size, restart_interval)? else {
      continue;
    };
    // 落在本对的 overflow slot：按新切分键重定位到所属页（keys 严格升序）。
    let split_key = entry_li[k].key.as_slice();
    for slot in overflow_li.iter_mut() {
      if slot.page_idx == idx - 1 || slot.page_idx == idx {
        slot.page_idx = if slot.key.as_slice() < split_key {
          idx - 1
        } else {
          idx
        };
      }
    }
    page_li[idx - 1] = first;
    page_li[idx] = second;
    // 后页是重分配新生成的新页，首键已变：其分隔键须改用真实首键。
    redistributed_li[idx] = true;
  }
  Ok(redistributed_li)
}

/// 单条 entry 的两种编码长度：页首 / restart 共用「无前驱」口径（共享前缀与
/// 时间戳基点都归零），restart 只多出 restart 数组的 2 字节偏移。
#[derive(Clone, Copy)]
struct EntryLenPair {
  /// 页首 / restart：无共享前缀、时间戳基点为 0。
  first: usize,
  /// 链式：与页内前一条共享前缀。
  chain: usize,
}

/// 按给定前驱角色估算单条 entry 的编码长度。
fn raw_entry_len(entry: &RawEntry, prev: Option<&RawEntry>, restart: bool) -> Result<usize> {
  Ok(
    page_entry_size(
      EntrySpec {
        key: &entry.key,
        value: &entry.value,
        ts: entry.ts,
        expire_ts: entry.expire,
        overflow: entry.ptr,
        compression: entry.comp,
        large_value: entry.large,
      },
      prev.map(|prev| (prev.key.as_slice(), prev.ts)),
      restart,
    )?
    .len,
  )
}

/// 以 `(prev_len + cur_len) / 2` 为字节目标，单次线性扫描选出重分配切分点。
///
/// 原实现逐个候选切分点整页重编码前后两页，最坏 O(n²)；这里改用同口径的
/// `entry_size` 做可行性判断：
/// - 前缀页只依赖前 k 条、长度随 k 单调递增，反向不存在，正向一遍即得；
/// - 后缀页 `entry_li[k..]` 的条目长度只与其 restart 对齐有关：把「restart 相对
///   链式的长度差」按 `i % restart_interval` 分组，反向扫描累加即可在 O(n) 内
///   得到所有 k 的后缀页长度。
///
/// 选定后两侧各只编码一次，切分点与逐点试编码的结果完全一致。
fn pick_split(
  entry_li: &[RawEntry],
  target: usize,
  page_size: usize,
  restart_interval: u16,
) -> Result<Option<usize>> {
  let n = entry_li.len();
  if n < 2 {
    return Ok(None);
  }
  let (prefix_li, suffix_li) = split_len_li(entry_li, restart_interval)?;
  // 与原切分点试探的停止条件一致：首页累加到 `target` 即停，至少留一条给后页。
  let mut k0 = 0;
  for (k, &len) in prefix_li.iter().enumerate().take(n).skip(1) {
    if len > page_size {
      break;
    }
    k0 = k;
    if len >= target {
      break;
    }
  }
  let k0 = k0.max(1);
  // 与原探测顺序一致：k0, k0+1, k0-1, k0+2, k0-2 ...
  let mut picked = None;
  for delta in 0..n {
    for k in [k0 + delta, k0.saturating_sub(delta)] {
      if k == 0 || k >= n {
        continue;
      }
      if prefix_li[k] <= page_size && suffix_li[k] <= page_size {
        picked = Some(k);
        break;
      }
    }
    if picked.is_some() || (k0 + delta >= n && k0 <= delta) {
      break;
    }
  }
  Ok(picked)
}

/// 一遍正向 + 一遍反向求出所有切分点的前后页内容长度：
/// `prefix[k] = len(entry_li[..k])`、`suffix[k] = len(entry_li[k..])`。
///
/// 后缀页的条目长度只与 restart 对齐有关：`index % restart_interval` 相同的
/// 条目在给定后缀里同为 restart，故把「restart 相对链式的长度差」按余数分组
/// 反向累加，即可一次得到全部 k。
fn split_len_li(entry_li: &[RawEntry], restart_interval: u16) -> Result<(Vec<usize>, Vec<usize>)> {
  let n = entry_li.len();
  let ri = usize::from(restart_interval).max(1);
  let mut len_li = Vec::with_capacity(n);
  let mut prefix_li = Vec::with_capacity(n + 1);
  prefix_li.push(page_content_len(0, 1));
  for (idx, entry) in entry_li.iter().enumerate() {
    let first = raw_entry_len(entry, None, false)?;
    let lens = EntryLenPair {
      first,
      chain: match idx.checked_sub(1) {
        Some(prev) => raw_entry_len(entry, Some(&entry_li[prev]), false)?,
        None => first,
      },
    };
    let add = if idx == 0 {
      lens.first
    } else if idx % ri == 0 {
      lens.first + 2
    } else {
      lens.chain
    };
    prefix_li.push(prefix_li[idx] + add);
    len_li.push(lens);
  }
  let mut suffix_li = vec![0usize; n];
  let mut residue_li = vec![0i64; ri];
  let mut chain_sum = 0i64;
  for k in (1..n).rev() {
    // 后缀 `entry_li[k..]` 的 restart 数（含页首隐含项）。
    let restart_cnt = 1 + (n - k - 1) / ri;
    let total = page_content_len(0, restart_cnt) as i64
      + len_li[k].first as i64
      + chain_sum
      + residue_li[k % ri];
    suffix_li[k] = usize::try_from(total).unwrap_or(usize::MAX);
    chain_sum += len_li[k].chain as i64;
    residue_li[k % ri] += len_li[k].first as i64 - len_li[k].chain as i64;
  }
  Ok((prefix_li, suffix_li))
}

async fn update_next<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  page_id: PageId,
  next: PageId,
  page_size: usize,
  pending: &mut PendingWrite,
) -> Result<()> {
  let file_page_id = mapper.file_page_id(page_id)?;
  let mut page = match pending.take(file_page_id) {
    Some(page) => page,
    None => {
      let mut page = vec![0u8; page_size];
      io.read_page(tbl, file_page_id, &mut page).await?;
      page
    }
  };
  set_next_page_id(&mut page, next)?;
  set_checksum(&mut page)?;
  let new_file_page_id = mapper.remap_page(page_id)?;
  pending.push(new_file_page_id, page);
  Ok(())
}

async fn update_prev<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  page_id: PageId,
  prev: PageId,
  page_size: usize,
  pending: &mut PendingWrite,
) -> Result<()> {
  let file_page_id = mapper.file_page_id(page_id)?;
  let mut page = match pending.take(file_page_id) {
    Some(page) => page,
    None => {
      let mut page = vec![0u8; page_size];
      io.read_page(tbl, file_page_id, &mut page).await?;
      page
    }
  };
  set_prev_page_id(&mut page, prev)?;
  set_checksum(&mut page)?;
  let new_file_page_id = mapper.remap_page(page_id)?;
  pending.push(new_file_page_id, page);
  Ok(())
}

fn read_index_children(page: &[u8]) -> Result<(Vec<Child>, bool)> {
  let index = IndexPage::new(page)?;
  let leaf = index.is_leaf()?;
  let mut child_li = vec![Child::new(Vec::new(), index.leftmost_ptr()?)];
  let mut iter = IndexPageIter::new(page)?;
  while iter.advance()? {
    child_li.push(Child::new(iter.key().to_vec(), iter.page_id()));
  }
  Ok((child_li, leaf))
}

/// 分裂后每页在父 index 中的分隔键：首段沿用父 index 原分隔键，其余段取
/// `find_shortest_separator(前页末键, 本页首键)`（对齐 C++ `FindShortestSeparator`）。
/// `redistributed_li[idx]` 为真表示该页由四分页重分配新生成，其首键已改变，
/// 分隔键须改用真实首键——对齐 C++ `FinishDataPage` 的 `page_key.assign(iter.Key())`。
fn page_separator_li(
  page_li: &[Vec<u8>],
  first_sep: &[u8],
  redistributed_li: &[bool],
) -> Result<Vec<Vec<u8>>> {
  let mut out = Vec::with_capacity(page_li.len());
  out.push(first_sep.to_vec());
  for (idx, page) in page_li.iter().enumerate().skip(1) {
    let mut iter = DataPageIter::new(page)?;
    if !iter.advance()? {
      return Err(KvError::Corrupted);
    }
    if redistributed_li[idx] {
      out.push(iter.key().to_vec());
    } else {
      let prev_last = last_data_key(&page_li[idx - 1])?;
      out.push(find_shortest_separator(&prev_last, iter.key()));
    }
  }
  Ok(out)
}

/// data 页的末条 key；空页返回 `Corrupted`。
fn last_data_key(page: &[u8]) -> Result<Vec<u8>> {
  let mut iter = DataPageIter::new(page)?;
  let mut key = Vec::new();
  while iter.advance()? {
    key.clear();
    key.extend_from_slice(iter.key());
  }
  if key.is_empty() {
    return Err(KvError::Corrupted);
  }
  Ok(key)
}

/// 对齐 C++ `FindShortestSeparator`：取 `right` 中第一个与 `left` 不同的字节
/// 作结尾，得到满足 `left < sep <= right` 的最短前缀（`left < right` 时必有解）。
fn find_shortest_separator(left: &[u8], right: &[u8]) -> Vec<u8> {
  let limit = left.len().min(right.len());
  let mut diff = 0;
  while diff < limit && left[diff] == right[diff] {
    diff += 1;
  }
  if diff < right.len() {
    right[..=diff].to_vec()
  } else {
    right.to_vec()
  }
}

fn replace_child(child_li: &mut Vec<Child>, idx: usize, new_child_li: Vec<Child>) {
  child_li.splice(idx..=idx, new_child_li);
  if let Some(first) = child_li.first_mut() {
    first.first_key.clear();
  }
}

/// index 页重建入参：新子项 + 旧页快照（供 region 直拷，fmt-17b）。
struct IndexRebuild<'a> {
  /// 待重建的旧页逻辑 id（重建后原位复用，其余分裂页新分配）。
  page_id: PageId,
  /// 该页在父节点中的分隔键；root 为空。
  first_key: &'a [u8],
  /// 重建后的子项。
  child_li: &'a [Child],
  /// 本批开始时旧页的子项快照。
  old_child_li: &'a [Child],
  /// 本批开始时旧页的原始字节。
  old_page: &'a [u8],
}

async fn build_index_pages<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  rebuild: IndexRebuild<'_>,
  layout: IndexLayout<'_>,
) -> Result<Vec<Child>> {
  let IndexRebuild {
    page_id: old_page_id,
    first_key,
    child_li,
    old_child_li,
    old_page,
  } = rebuild;
  if child_li.is_empty() {
    mapper.free_page(old_page_id)?;
    return Ok(Vec::new());
  }
  // 旧页可用时优先 region 直拷（fmt-17b）；不适用则回退逐条重编码。
  let page_li = match build_index_page_region(child_li, old_child_li, old_page, layout)? {
    Some(page) => vec![(first_key.to_vec(), page)],
    None => build_index_page_li(first_key, child_li, layout)?,
  };
  let mut out_li = Vec::with_capacity(page_li.len());
  let mut write_li = Vec::with_capacity(page_li.len());
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
    write_li.push((file_page_id, page));
    out_li.push(Child::new(first_key, page_id));
  }
  io.write_page_li(tbl, &write_li).await?;
  Ok(out_li)
}

async fn build_new_index<'a, I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  child_li: &[Child],
  mut layout: IndexLayout<'a>,
) -> Result<PageId> {
  if child_li.is_empty() {
    return Ok(NULL_PAGE);
  }
  let mut child_li = child_li.to_vec();
  loop {
    let page_li = build_index_page_li(&[], &child_li, layout)?;
    let mut out_li = Vec::with_capacity(page_li.len());
    let mut write_li = Vec::with_capacity(page_li.len());
    for (first_key, page) in page_li {
      let page_id = mapper.allocate_page();
      let file_page_id = mapper.file_page_id(page_id)?;
      write_li.push((file_page_id, page));
      out_li.push(Child::new(first_key, page_id));
    }
    io.write_page_li(tbl, &write_li).await?;
    if out_li.len() == 1 {
      return Ok(out_li[0].page_id);
    }
    child_li = out_li;
    layout.leaf = false;
  }
}

/// 索引页的普通 entry 列表（不含 header leftmost）：键与 page_id 按页内顺序。
fn collect_index_entry_li(page: &[u8]) -> Result<Vec<(Vec<u8>, u32)>> {
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
fn rebalance_index_page_li(
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

fn build_index_page_li<'a>(
  first_key: &[u8],
  child_li: &[Child],
  layout: IndexLayout<'a>,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
  let mut out_li = Vec::new();
  let mut builder = IndexPageBuilder::new(
    layout.page.data.page_size,
    layout.page.index_restart_interval,
    layout.leaf,
  )?;
  let mut page_first_key = first_key.to_vec();
  if !builder.add(b"", child_li[0].page_id)? {
    return Err(KvError::OutOfSpace);
  }
  for child in &child_li[1..] {
    if builder.add(&child.first_key, child.page_id)? {
      continue;
    }
    out_li.push((page_first_key, builder.finish()?));
    builder = IndexPageBuilder::new(
      layout.page.data.page_size,
      layout.page.index_restart_interval,
      layout.leaf,
    )?;
    page_first_key = child.first_key.clone();
    if !builder.add(b"", child.page_id)? {
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
static REGION_REUSE_HITS: AtomicUsize = AtomicUsize::new(0);

/// 用旧页 region 直拷 + 变更区段重编码重建**单页** index 页（fmt-17b）：未变更的整段
/// region 直接 `memcpy`，只有首键 / page_id 变化的区段重新 varint 编码。
///
/// 返回 `None` 表示不适用（页型/尺寸不符、旧页 restart 间距与当前配置不一致、单页放不下），
/// 调用方回退 [`build_index_page_li`] 逐条重编码。命中时产物与逐条重编码**逐字节一致**：
/// region 边界即 restart 点（`shared == 0`、page_id 记绝对值），region 字节自包含。
fn build_index_page_region(
  child_li: &[Child],
  old_child_li: &[Child],
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
  builder.set_leftmost(child_li[0].page_id)?;
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
        if old_child_li.get(a..b) == child_li.get(a..b)
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
        entry_li.extend(
          child_li[a..b]
            .iter()
            .map(|child| (child.first_key.as_slice(), child.page_id)),
        );
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

#[cfg(test)]
mod unit_tests {
  use std::sync::atomic::Ordering;

  use fkv_io_mem::MemStore;
  use fkv_page::{DataPageBuilder, DataPageIter, content_len, header::OVERFLOW_HEADER_SIZE};

  use super::*;

  /// `end_cap == 0` 的极端页配置：末页写不出任何字节，旧循环会原地打转
  /// 并无界分配页，必须在入口拒绝且不分配任何页。
  #[test]
  fn overflow_chain_rejects_zero_end_capacity() {
    use fkv_mapping::{AllocMode, PageMapper};

    let pointers = 16u8;
    let page_size = OVERFLOW_HEADER_SIZE + 1 + 4 * usize::from(pointers);
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let mut pending = PendingWrite {
      page_li: Vec::new(),
      slot_map: HashMap::default(),
      bytes: 0,
    };
    // 超过一批容量（page_cap * group_max），才会走到末页指针分支。
    let value = vec![b'x'; 2048];
    let err = pollster::block_on(build_overflow_chain(
      &MemStore::new(page_size),
      &TableIdent::new("demo", 1).unwrap(),
      &mut mapper,
      &value,
      PageLayout::new(page_size, 4, 4, None, pointers),
      &mut pending,
    ))
    .unwrap_err();
    assert!(matches!(err, KvError::InvalidArgs));
    assert_eq!(mapper.tbl().len(), 0, "校验必须先于任何页分配");
    assert!(pending.page_li.is_empty());
  }

  /// 构造 index 页的 child_li：`child_li[0]` 是 leftmost（key 空），其余为 `keys`。
  fn child_li(keys: &[&str], page_id_base: u32) -> Vec<Child> {
    let mut out = Vec::with_capacity(keys.len() + 1);
    out.push(Child::new(Vec::new(), page_id_base));
    for (idx, key) in keys.iter().enumerate() {
      out.push(Child::new(
        key.as_bytes().to_vec(),
        page_id_base + 1 + idx as u32,
      ));
    }
    out
  }

  fn index_layout(page_size: usize, restart_interval: u16, leaf: bool) -> IndexLayout<'static> {
    IndexLayout {
      page: PageLayout::new(
        page_size,
        // 测试里两页间隔同值：index 布局只消费 index 侧。
        restart_interval,
        restart_interval,
        None,
        16,
      ),
      leaf,
    }
  }

  /// fmt-18：index 页 restart 密度只由 `index_restart_interval` 决定，
  /// data 侧改动不得影响 index 页字节（对齐 C++ 两独立旋钮）。
  #[test]
  fn index_page_uses_index_restart_interval_only() {
    let page_size = 4096;
    let keys: Vec<String> = (0..12).map(|idx| format!("k{idx:04}")).collect();
    let key_ref_li: Vec<&str> = keys.iter().map(String::as_str).collect();
    let child_li = child_li(&key_ref_li, 100);
    let layout_of = |data_restart_interval: u16, index_restart_interval: u16| IndexLayout {
      page: PageLayout::new(
        page_size,
        data_restart_interval,
        index_restart_interval,
        None,
        16,
      ),
      leaf: true,
    };
    let build = |layout: IndexLayout<'static>| {
      let page_li = build_index_page_li(b"", &child_li, layout).unwrap();
      page_li.into_iter().next().unwrap().1
    };

    let base = build(layout_of(16, 16));
    // data 侧改动：index 页字节必须逐字节不变。
    assert_eq!(base, build(layout_of(2, 16)));
    // index 侧改动：restart 点密度变化，字节随之改变，语义（child_li 序列）不变。
    let dense = build(layout_of(16, 4));
    assert_ne!(base, dense);
    assert_eq!(restart_num(&base).unwrap(), 1);
    assert_eq!(restart_num(&dense).unwrap(), 3);
    assert_eq!(read_index_children(&base).unwrap().0, child_li);
    assert_eq!(read_index_children(&dense).unwrap().0, child_li);
  }

  /// fmt-18：data 页只受 `data_restart_interval` 影响，index 侧改动不得改变 data 页字节。
  #[test]
  fn data_page_ignores_index_restart_interval() {
    use fkv_io_mem::MemStore;
    use fkv_mapping::{AllocMode, PageMapper};
    use fkv_types::TableIdent;

    pollster::block_on(async {
      const PAGE_SIZE: usize = 512;
      let batch_li: Vec<BatchEntry> = (0..40)
        .map(|idx| BatchEntry::upsert(format!("k{idx:04}").into_bytes(), vec![b'v'; 8], 1))
        .collect();
      let entry_li: Vec<&BatchEntry> = batch_li.iter().collect();

      let dump = async |index_restart_interval: u16| -> Vec<Vec<u8>> {
        let store = MemStore::new(PAGE_SIZE);
        let tbl = TableIdent::new("t", 1).unwrap();
        let mut mapper = PageMapper::new(AllocMode::Append, 0);
        let layout = PageLayout::new(PAGE_SIZE, 4, index_restart_interval, None, 16);
        let child_li = write_new_data_page(
          &store,
          &tbl,
          &mut mapper,
          &entry_li,
          layout,
          &mut PendingWrite::default(),
        )
        .await
        .unwrap();
        let mut buf = vec![0u8; PAGE_SIZE];
        let mut out_li = Vec::with_capacity(child_li.len());
        for child in &child_li {
          let file_page_id = mapper.file_page_id(child.page_id).unwrap();
          store.read_page(&tbl, file_page_id, &mut buf).await.unwrap();
          out_li.push(buf.clone());
        }
        out_li
      };

      let wide = dump(16).await;
      let narrow = dump(2).await;
      assert!(wide.len() > 1, "测试数据须产生多个 data 页");
      assert_eq!(wide, narrow, "index 间隔不得改变 data 页字节");
    });
  }

  /// 单页重建：region 直拷产物必须与 `build_index_page_li` 逐条重编码**逐字节一致**。
  /// 覆盖：全不变 / 改 page_id / 改分隔键 / child_li 变短变长。
  #[test]
  fn region_reuse_matches_entry_rebuild() {
    let page_size = 4096;
    let ri = 4u16;
    let layout = index_layout(page_size, ri, true);
    let keys: Vec<String> = (0..40).map(|idx| format!("k{idx:04}")).collect();
    let key_ref_li: Vec<&str> = keys.iter().map(String::as_str).collect();
    let old_child_li = child_li(&key_ref_li, 100);
    let old_pages = build_index_page_li(b"", &old_child_li, layout).unwrap();
    assert_eq!(old_pages.len(), 1, "测试数据须落在单页");
    let old_page = &old_pages[0].1;

    // 逐条重编码基准（同一 child_li → 单页）。
    let expect_single = |child_li: &[Child]| -> Vec<u8> {
      let pages = build_index_page_li(b"", child_li, layout).unwrap();
      assert_eq!(pages.len(), 1, "基准重建须仍在单页");
      pages.into_iter().next().unwrap().1
    };

    // 1) 完全不变：直拷产物 == 旧页字节（且命中快路径）。
    let same = build_index_page_region(&old_child_li, &old_child_li, old_page, layout)
      .unwrap()
      .expect("应命中 region 快路径");
    assert_eq!(&same, old_page);

    // 2) 改一个 child 的 page_id（key 不变）。
    let mut changed = old_child_li.clone();
    changed[10] = Child::new(b"k0009".to_vec(), 9999);
    let fast = build_index_page_region(&changed, &old_child_li, old_page, layout)
      .unwrap()
      .expect("应命中 region 快路径");
    assert_eq!(fast, expect_single(&changed));
    assert_eq!(read_index_children(&fast).unwrap().0, changed);

    // 3) 改一个 child 的分隔键（key 变化）。
    let mut changed = old_child_li.clone();
    changed[5] = Child::new(b"k0004x".to_vec(), 5555);
    let fast = build_index_page_region(&changed, &old_child_li, old_page, layout)
      .unwrap()
      .expect("应命中 region 快路径");
    assert_eq!(fast, expect_single(&changed));

    // 4) child_li 变短（尾区段比旧页对应区段短）与变长。
    let mut shorter = old_child_li.clone();
    shorter.truncate(11);
    let fast = build_index_page_region(&shorter, &old_child_li, old_page, layout)
      .unwrap()
      .expect("应命中 region 快路径");
    assert_eq!(fast, expect_single(&shorter));

    let mut longer = old_child_li.clone();
    longer.push(Child::new(b"k0100".to_vec(), 7777));
    let fast = build_index_page_region(&longer, &old_child_li, old_page, layout)
      .unwrap()
      .expect("应命中 region 快路径");
    assert_eq!(fast, expect_single(&longer));

    // 5) 旧页 leaf 与当前布局不符 → 不适用。
    assert!(
      build_index_page_region(
        &old_child_li,
        &old_child_li,
        old_page,
        index_layout(page_size, ri, false)
      )
      .unwrap()
      .is_none()
    );
  }

  /// 旧页 `restart_interval` 与当前配置不一致时必须放弃直拷（否则 region 错位串页）。
  #[test]
  fn region_reuse_rejects_restart_interval_mismatch() {
    let page_size = 4096;
    let keys: Vec<String> = (0..6).map(|idx| format!("k{idx:04}")).collect();
    let key_ref_li: Vec<&str> = keys.iter().map(String::as_str).collect();
    let child_li = child_li(&key_ref_li, 100);

    // 旧页按 ri=3 切 region：region 数 2，与 ri=4 的 ceil(6/4)=2 相同（只靠计数挡不住）。
    let old_layout = index_layout(page_size, 3, true);
    let old_page = &build_index_page_li(b"", &child_li, old_layout).unwrap()[0].1;
    assert!(
      build_index_page_region(
        &child_li,
        &child_li,
        old_page,
        index_layout(page_size, 4, true)
      )
      .unwrap()
      .is_none()
    );
    assert!(
      build_index_page_region(
        &child_li,
        &child_li,
        old_page,
        index_layout(page_size, 8, true)
      )
      .unwrap()
      .is_none()
    );
    // 同配置则命中，且与逐条重编码一致。
    let fast = build_index_page_region(&child_li, &child_li, old_page, old_layout)
      .unwrap()
      .expect("应命中 region 快路径");
    assert_eq!(&fast, old_page);
  }

  /// `build_index_pages` 在旧页可用时必须走 region 直拷，且产物与逐条重编码一致。
  #[test]
  fn build_index_pages_prefers_region_reuse() {
    use fkv_io_mem::MemStore;
    use fkv_mapping::{AllocMode, PageMapper};
    use fkv_types::TableIdent;

    pollster::block_on(async {
      let page_size = 4096;
      let layout = index_layout(page_size, 4, true);
      let keys: Vec<String> = (0..30).map(|idx| format!("k{idx:04}")).collect();
      let key_ref_li: Vec<&str> = keys.iter().map(String::as_str).collect();
      let old_child_li = child_li(&key_ref_li, 100);
      let old_page = build_index_page_li(b"", &old_child_li, layout)
        .unwrap()
        .remove(0)
        .1;
      let mut new_children = old_child_li.clone();
      new_children[7] = Child::new(b"k0006".to_vec(), 4242);

      let store = MemStore::new(page_size);
      let tbl = TableIdent::new("t", 1).unwrap();
      let mut mapper = PageMapper::new(AllocMode::Append, 0);
      let old_page_id = mapper.allocate_page();
      let file_page_id = mapper.file_page_id(old_page_id).unwrap();
      store
        .write_page(&tbl, file_page_id, &old_page)
        .await
        .unwrap();

      let hits_before = REGION_REUSE_HITS.load(Ordering::Relaxed);
      let out_li = build_index_pages(
        &store,
        &tbl,
        &mut mapper,
        IndexRebuild {
          page_id: old_page_id,
          first_key: b"",
          child_li: &new_children,
          old_child_li: &old_child_li,
          old_page: &old_page,
        },
        layout,
      )
      .await
      .unwrap();
      assert!(
        REGION_REUSE_HITS.load(Ordering::Relaxed) > hits_before,
        "应命中 region 直拷快路径"
      );
      assert_eq!(out_li.len(), 1);
      assert_eq!(out_li[0].page_id, old_page_id);
      let written = mapper.file_page_id(out_li[0].page_id).unwrap();
      let mut buf = vec![0u8; page_size];
      store.read_page(&tbl, written, &mut buf).await.unwrap();
      assert_eq!(
        buf,
        build_index_page_li(b"", &new_children, layout).unwrap()[0].1
      );
    });
  }

  /// 真实写路径：第二批只改少量 data 页时，leaf index 重建必须命中 region 直拷。
  #[test]
  fn write_batch_uses_region_reuse() {
    use fkv_io_mem::MemStore;
    use fkv_mapping::{AllocMode, PageMapper};
    use fkv_types::TableIdent;

    pollster::block_on(async {
      let page_size = 512;
      let store = MemStore::new(page_size);
      let tbl = TableIdent::new("t", 2).unwrap();
      let mut mapper = PageMapper::new(AllocMode::Append, 0);
      let batch_li: Vec<BatchEntry> = (0..200)
        .map(|idx| BatchEntry::upsert(format!("k{idx:04}").into_bytes(), vec![b'v'; 8], 1))
        .collect();
      let root = write_batch(
        &store,
        WriteArg {
          tbl: &tbl,
          root_id: NULL_PAGE,
          mapper: &mut mapper,
          batch_li: &batch_li,
          layout: PageLayout::new(page_size, 4, 4, None, 16),
          segment_mapper: None,
          segment_size: 0,
          ttl_delete_sink: None,
        },
      )
      .await
      .unwrap();

      // 插入一批更大的 key：末位 data 页分裂 → leaf index 子项集合改变，触发重建；
      // 其余子项不变，故首个 region 可直接直拷。
      let batch2: Vec<BatchEntry> = (0..60)
        .map(|idx| BatchEntry::upsert(format!("n{idx:04}").into_bytes(), vec![b'w'; 8], 2))
        .collect();
      let hits_before = REGION_REUSE_HITS.load(Ordering::Relaxed);
      let _root2 = write_batch(
        &store,
        WriteArg {
          tbl: &tbl,
          root_id: root,
          mapper: &mut mapper,
          batch_li: &batch2,
          layout: PageLayout::new(page_size, 4, 4, None, 16),
          segment_mapper: None,
          segment_size: 0,
          ttl_delete_sink: None,
        },
      )
      .await
      .unwrap();
      assert!(
        REGION_REUSE_HITS.load(Ordering::Relaxed) > hits_before,
        "leaf index 重建应命中 region 直拷"
      );
    });
  }

  /// 最短分隔键必须落在 `(前页末键, 本页首键]`，才不破坏 index 路由。
  #[test]
  fn shortest_separator_stays_between_keys() {
    for (left, right) in [
      (&b"abc"[..], &b"abd"[..]),
      (&b"abc"[..], &b"abcd"[..]),
      (&b"a"[..], &b"b"[..]),
      (&b"key0100"[..], &b"key0200"[..]),
    ] {
      let sep = find_shortest_separator(left, right);
      assert!(left < sep.as_slice(), "left={left:?} sep={sep:?}");
      assert!(sep.as_slice() <= right, "sep={sep:?} right={right:?}");
    }
  }

  fn page_of(keys: &[&str], value: &[u8], page_size: usize) -> (Vec<u8>, usize) {
    let mut builder = DataPageBuilder::new(page_size, 8).unwrap();
    let mut added = 0;
    for key in keys {
      if !builder
        .add(EntrySpec {
          key: key.as_bytes(),
          value,
          ts: 1,
          expire_ts: 0,
          overflow: false,
          compression: CompressionType::None,
          large_value: false,
        })
        .unwrap()
      {
        break;
      }
      added += 1;
    }
    (builder.finish().unwrap(), added)
  }

  /// 用给定 child_li 构建单页（全部须放得下），返回页字节。
  fn index_page_of(child_li: &[Child], layout: IndexLayout<'static>) -> Vec<u8> {
    let mut builder = IndexPageBuilder::new(
      layout.page.data.page_size,
      layout.page.index_restart_interval,
      layout.leaf,
    )
    .unwrap();
    assert!(builder.add(b"", child_li[0].page_id).unwrap());
    for child in &child_li[1..] {
      assert!(builder.add(&child.first_key, child.page_id).unwrap());
    }
    builder.finish().unwrap()
  }

  /// 把多页索引的 child_li 顺序拉平：首页 leftmost 键为空，其后各页 leftmost
  /// 键即该页分隔键。
  fn flatten_index_children(page_li: &[(Vec<u8>, Vec<u8>)]) -> Vec<Child> {
    let mut out = Vec::new();
    for (idx, (key, page)) in page_li.iter().enumerate() {
      let mut child_li = read_index_children(page).unwrap().0;
      if idx > 0 {
        child_li[0].first_key.clone_from(key);
      }
      out.extend(child_li);
    }
    out
  }

  /// sto-67：索引页尾页不足 1/4、前页超 3/4 且 restart 数 > 1 时，按 restart
  /// region 把前页尾部迁到尾页，消除稀疏尾页；child_li 顺序不变。
  #[test]
  fn index_page_quarter_redistribution_balances_tail() {
    let page_size = 512;
    let restart_interval = 4;
    let layout = index_layout(page_size, restart_interval, true);
    let key_li: Vec<String> = (0..200).map(|idx| format!("k{idx:04}")).collect();
    let key_ref_li: Vec<&str> = key_li.iter().map(String::as_str).collect();
    let children_all = child_li(&key_ref_li, 100);

    // 复刻无重分配路径：按「放不下就开新页」切分，得到前页 + 稀疏尾页。
    let mut builder = IndexPageBuilder::new(page_size, restart_interval, true).unwrap();
    assert!(builder.add(b"", children_all[0].page_id).unwrap());
    let mut split = 0;
    for (pos, child) in children_all.iter().enumerate().skip(1) {
      if !builder.add(&child.first_key, child.page_id).unwrap() {
        split = pos;
        break;
      }
    }
    assert!(split > 1, "fixture 需先填满前页");
    // 尾页只留 leftmost + 1 条 entry，确保 < 1/4。
    let child_li = children_all[..split + 2].to_vec();
    let prev = index_page_of(&child_li[..split], layout);
    let cur = index_page_of(&child_li[split..], layout);
    let prev_len = usize::from(content_len(&prev).unwrap());
    let cur_len = usize::from(content_len(&cur).unwrap());
    assert!(prev_len > page_size * 3 / 4, "prev={prev_len}");
    assert!(cur_len < page_size / 4, "cur={cur_len}");
    assert!(restart_num(&prev).unwrap() > 1);

    let mut page_li = vec![
      (b"".to_vec(), prev),
      (child_li[split].first_key.clone(), cur),
    ];
    let old_cur_key = page_li[1].0.clone();
    assert!(rebalance_index_page_li(&mut page_li, page_size, restart_interval, true).unwrap());
    // 尾页不再 < 1/4（前页同样落在阈值内）。
    for (_, page) in &page_li {
      let len = usize::from(content_len(page).unwrap());
      assert!(
        len >= page_size / 4 && len <= page_size * 3 / 4,
        "len={len}"
      );
    }
    // child_li 顺序不变：各页 leftmost + entry 依次拼接（页首 leftmost 键即页分隔键）。
    let out = flatten_index_children(&page_li);
    assert_eq!(out, child_li);
    // 尾页首键已改为迁移入口的键（不再是原尾页首键）。
    assert_ne!(page_li[1].0, old_cur_key);
  }

  /// 端到端：`build_index_page_li` 自动重分配，尾页不再稀疏，页数不变。
  #[test]
  fn build_index_page_li_rebalances_sparse_tail() {
    let page_size = 512;
    let layout = index_layout(page_size, 4, true);
    let key_li: Vec<String> = (0..130).map(|idx| format!("k{idx:04}")).collect();
    let key_ref_li: Vec<&str> = key_li.iter().map(String::as_str).collect();
    let child_li = child_li(&key_ref_li, 100);

    let page_li = build_index_page_li(b"", &child_li, layout).unwrap();
    assert!(page_li.len() >= 2);
    let last_len = usize::from(content_len(&page_li.last().unwrap().1).unwrap());
    assert!(last_len >= page_size / 4, "last={last_len}");
    let out = flatten_index_children(&page_li);
    assert_eq!(out, child_li);
  }

  /// 前页 > 3/4、后页 < 1/4 时，重分配后两页都落在 [1/4, 3/4]。
  #[test]
  fn quarter_redistribution_balances_pages() {
    let page_size = 256;
    let value = vec![b'v'; 8];
    let key_li: Vec<String> = (0..24).map(|index| format!("k{index:02}")).collect();
    let key_ref_li: Vec<&str> = key_li.iter().map(String::as_str).collect();
    let (prev, prev_cnt) = page_of(&key_ref_li, &value, page_size);
    assert!(prev_cnt >= 2, "prev page must hold multiple entry_li");
    let last_key = format!("k{prev_cnt:02}");
    let (cur, cur_cnt) = page_of(&[last_key.as_str()], &value, page_size);
    assert_eq!(cur_cnt, 1);

    let prev_len = usize::from(content_len(&prev).unwrap());
    let cur_len = usize::from(content_len(&cur).unwrap());
    assert!(prev_len > page_size * 3 / 4, "prev={prev_len}");
    assert!(cur_len < page_size / 4, "cur={cur_len}");

    let mut page_li = vec![prev, cur];
    let redistributed_li = rebalance_data_page_li(&mut page_li, page_size, 8, &mut []).unwrap();
    // 后页（新页）首键被改写，须标记为改用真实首键；前页未标记。
    assert_eq!(redistributed_li, vec![false, true]);
    assert_eq!(page_li.len(), 2);
    for page in &page_li {
      let len = usize::from(content_len(page).unwrap());
      assert!(
        len >= page_size / 4 && len <= page_size * 3 / 4,
        "len={len}"
      );
    }
    // 全部 entry 且顺序不变。
    let mut keys = Vec::new();
    for page in &page_li {
      let mut iter = DataPageIter::new(page).unwrap();
      while iter.advance().unwrap() {
        keys.push(iter.key().to_vec());
      }
    }
    let expected: Vec<Vec<u8>> = (0..=prev_cnt)
      .map(|index| format!("k{index:02}").into_bytes())
      .collect();
    assert_eq!(keys, expected);
  }

  /// 被四分页重分配改写过首键的新页：分隔键用真实首键（对齐 C++
  /// `page_key.assign(iter.Key())`）；未重分配的页仍用最短分隔键。
  #[test]
  fn redistributed_page_uses_real_first_key() {
    let first_sep = b"parent".to_vec();
    let (page_a, _) = page_of(&["k00", "k01", "k02"], b"v", 256);
    let (page_b, _) = page_of(&["k05", "k06"], b"v", 256);
    let (page_c, _) = page_of(&["k10"], b"v", 256);
    let page_li = vec![page_a, page_b, page_c];
    let redistributed_li = vec![false, true, false];
    let sep_li = page_separator_li(&page_li, &first_sep, &redistributed_li).unwrap();
    assert_eq!(sep_li[0], first_sep);
    assert_eq!(sep_li[1], b"k05".to_vec());
    assert_eq!(sep_li[2], find_shortest_separator(b"k06", b"k10"));
  }

  /// 用异构 `(key, value, overflow)` 列表构建单页，返回 (页字节, 实际条数)。
  fn page_from(items: &[(&str, &[u8], bool)], page_size: usize) -> (Vec<u8>, usize) {
    let mut builder = DataPageBuilder::new(page_size, 8).unwrap();
    let mut added = 0;
    for (key, value, overflow) in items {
      if !builder
        .add(EntrySpec {
          key: key.as_bytes(),
          value,
          ts: 1,
          expire_ts: 0,
          overflow: *overflow,
          compression: CompressionType::None,
          large_value: false,
        })
        .unwrap()
      {
        break;
      }
      added += 1;
    }
    (builder.finish().unwrap(), added)
  }

  /// 重分配按字节中点而非条目数中点切分：变长 entry 下后者会把前页切到 < 1/4。
  #[test]
  fn redistribution_splits_by_byte_midpoint() {
    let page_size = 2048;
    let small = vec![b's'; 10];
    let large = vec![b'l'; 400];
    let mut items: Vec<(String, Vec<u8>)> = (0..40)
      .map(|index| (format!("k{index:03}"), small.clone()))
      .collect();
    for index in 0..3 {
      items.push((format!("z{index:03}"), large.clone()));
    }
    let ref_li: Vec<(&str, &[u8], bool)> = items
      .iter()
      .map(|(key, value)| (key.as_str(), value.as_slice(), false))
      .collect();
    let (prev, prev_cnt) = page_from(&ref_li, page_size);
    assert_eq!(prev_cnt, items.len(), "前页须容纳全部 entry");
    let (cur, cur_cnt) = page_from(&[("z003", small.as_slice(), false)], page_size);
    assert_eq!(cur_cnt, 1);

    let prev_len = usize::from(content_len(&prev).unwrap());
    let cur_len = usize::from(content_len(&cur).unwrap());
    assert!(prev_len > page_size * 3 / 4, "prev={prev_len}");
    assert!(cur_len < page_size / 4, "cur={cur_len}");

    let mut page_li = vec![prev, cur];
    rebalance_data_page_li(&mut page_li, page_size, 8, &mut []).unwrap();
    for page in &page_li {
      let len = usize::from(content_len(page).unwrap());
      assert!(
        len >= page_size / 4 && len <= page_size * 3 / 4,
        "len={len}"
      );
    }
  }

  /// 构造覆盖共享前缀、restart 对齐、负数时间戳增量、TTL、压缩标志与大 value 的条目。
  fn probe_entry_li(scale: usize) -> Vec<RawEntry> {
    (0..24)
      .map(|idx| {
        let value_len = if idx % 7 == 3 {
          40 * scale
        } else {
          8 + idx % 5
        };
        // ts 故意乱序，制造大 / 负的时间戳增量。
        let ts = if idx % 5 == 1 { 1 } else { 7 + idx as u64 };
        RawEntry {
          key: format!("key-{idx:03}").into_bytes(),
          value: vec![b'v'; value_len],
          ptr: false,
          ts,
          expire: if idx % 6 == 2 { 999 } else { 0 },
          comp: if idx % 4 == 0 {
            CompressionType::Dictionary
          } else {
            CompressionType::None
          },
          large: idx % 11 == 5,
        }
      })
      .collect()
  }

  /// `split_len_li` 估出的前后页长度必须与实际编码逐字节一致（含放不下的判定）。
  #[test]
  fn split_lens_matches_encoded_page_lengths() {
    for &page_size in &[256usize, 512, 1024] {
      for &ri in &[1u16, 3, 8, 16] {
        let entry_li = probe_entry_li(if page_size >= 1024 { 4 } else { 1 });
        let (prefix_li, suffix_li) = split_len_li(&entry_li, ri).unwrap();
        let n = entry_li.len();
        for k in 1..n {
          let ctx = format!("page_size={page_size} ri={ri} k={k}");
          match build_raw_page(&entry_li[..k], page_size, ri).unwrap() {
            Some(page) => assert_eq!(
              prefix_li[k],
              usize::from(content_len(&page).unwrap()),
              "前缀页长度不一致 {ctx}"
            ),
            None => assert!(prefix_li[k] > page_size, "前缀页应放不下 {ctx}"),
          }
          match build_raw_page(&entry_li[k..], page_size, ri).unwrap() {
            Some(page) => assert_eq!(
              suffix_li[k],
              usize::from(content_len(&page).unwrap()),
              "后缀页长度不一致 {ctx}"
            ),
            None => assert!(suffix_li[k] > page_size, "后缀页应放不下 {ctx}"),
          }
        }
      }
    }
  }

  /// 旧逐点试编码算法（参考实现）：用于验证 `pick_split` 选点完全一致。
  fn brute_pick(entry_li: &[RawEntry], target: usize, page_size: usize, ri: u16) -> Option<usize> {
    let n = entry_li.len();
    if n < 2 {
      return None;
    }
    let mut builder = DataPageBuilder::new(page_size, ri).unwrap();
    let mut k0 = 0;
    for entry in entry_li.iter().take(n - 1) {
      let added = builder
        .add(EntrySpec {
          key: &entry.key,
          value: &entry.value,
          ts: entry.ts,
          expire_ts: entry.expire,
          overflow: entry.ptr,
          compression: entry.comp,
          large_value: entry.large,
        })
        .unwrap();
      if !added {
        break;
      }
      k0 += 1;
      if builder.current_size_estimate() >= target {
        break;
      }
    }
    let k0 = k0.max(1);
    for delta in 0..n {
      for k in [k0 + delta, k0.saturating_sub(delta)] {
        if k == 0 || k >= n {
          continue;
        }
        if build_raw_page(&entry_li[..k], page_size, ri)
          .unwrap()
          .is_some()
          && build_raw_page(&entry_li[k..], page_size, ri)
            .unwrap()
            .is_some()
        {
          return Some(k);
        }
      }
      if k0 + delta >= n && k0 <= delta {
        break;
      }
    }
    None
  }

  /// 线性估算的选点结果必须与旧算法（逐点试编码）完全一致。
  #[test]
  fn pick_split_matches_bruteforce_probe() {
    for &page_size in &[256usize, 512] {
      for &ri in &[1u16, 4, 8] {
        let entry_li = probe_entry_li(1);
        let (prefix_li, _) = split_len_li(&entry_li, ri).unwrap();
        let target = prefix_li[entry_li.len() / 2];
        assert_eq!(
          pick_split(&entry_li, target, page_size, ri).unwrap(),
          brute_pick(&entry_li, target, page_size, ri),
          "page_size={page_size} ri={ri}"
        );
      }
    }
  }

  /// 含 overflow 占位指针的页照常参与重分配，slot 的 `page_idx` 跟随条目迁移。
  #[test]
  fn redistribution_migrates_overflow_slot() {
    let page_size = 1024;
    let small = vec![b's'; 10];
    let mut items: Vec<(String, Vec<u8>, bool)> = (0..30)
      .map(|index| (format!("k{index:03}"), small.clone(), false))
      .collect();
    items.push(("z000".to_string(), vec![0xAB; 300], true));
    let ref_li: Vec<(&str, &[u8], bool)> = items
      .iter()
      .map(|(key, value, overflow)| (key.as_str(), value.as_slice(), *overflow))
      .collect();
    let (prev, prev_cnt) = page_from(&ref_li, page_size);
    assert_eq!(prev_cnt, items.len(), "前页须容纳全部 entry");
    let (cur, _) = page_from(&[("z001", small.as_slice(), false)], page_size);

    let prev_len = usize::from(content_len(&prev).unwrap());
    assert!(prev_len > page_size * 3 / 4, "prev={prev_len}");
    assert!(usize::from(content_len(&cur).unwrap()) < page_size / 4);

    let mut slot_li = vec![OverflowSlot {
      page_idx: 0,
      key: b"z000".to_vec(),
      value: vec![0u8; 300],
    }];
    let mut page_li = vec![prev, cur];
    rebalance_data_page_li(&mut page_li, page_size, 8, &mut slot_li).unwrap();
    // 占位条目前移到第二页，slot 必须跟随，且指针仍可按 key 定位回填。
    assert_eq!(slot_li[0].page_idx, 1);
    let range_li = locate_value_range_li(&page_li[1], &[b"z000"]).unwrap();
    assert_eq!(range_li[0].map(|(begin, end)| end - begin), Some(300));
  }

  /// 空表首次批量写入（`write_new_data_page`）同样做四分页重分配，消除稀疏尾页。
  #[test]
  fn fresh_write_rebalances_tail_page() {
    use fkv_io_mem::MemStore;
    use fkv_mapping::{AllocMode, PageMapper};
    use fkv_types::TableIdent;

    pollster::block_on(async {
      let page_size = 128;
      let restart_interval = 4;
      let layout = PageLayout::new(page_size, restart_interval, restart_interval, None, 16);
      // 6 条 8B value + 2 条 1B value 把首页填到接近满（仅余极少字节），
      // 第 9 条 1B value 只能单独成 31 字节（< 1/4 的 32）的稀疏尾页。
      let batch_li: Vec<BatchEntry> = (0..9)
        .map(|index| {
          let value = if index >= 6 {
            vec![b'v'; 1]
          } else {
            vec![b'v'; 8]
          };
          BatchEntry::upsert(format!("k{index:02}").into_bytes(), value, 1)
        })
        .collect();

      let MergeOutput { page_li: raw, .. } = merge_data_pages_with_options(
        &[],
        &batch_li,
        MergeOption {
          page_size,
          restart_interval,
          overflow_pointers: 16,
          dict: None,
        },
      )
      .unwrap();
      let raw_len_li: Vec<usize> = raw
        .iter()
        .map(|page| usize::from(content_len(page).unwrap()))
        .collect();
      assert_eq!(raw_len_li.len(), 2, "{raw_len_li:?}");
      assert!(raw_len_li[0] > page_size * 3 / 4, "{raw_len_li:?}");
      assert!(raw_len_li[1] < page_size / 4, "{raw_len_li:?}");

      let store = MemStore::new(page_size);
      let tbl = TableIdent::new("t", 1).unwrap();
      let mut mapper = PageMapper::new(AllocMode::Append, 0);
      let entry_li: Vec<&BatchEntry> = batch_li.iter().collect();
      let child_li = write_new_data_page(
        &store,
        &tbl,
        &mut mapper,
        &entry_li,
        layout,
        &mut PendingWrite::default(),
      )
      .await
      .unwrap();
      assert_eq!(child_li.len(), raw_len_li.len());
      let mut buf = vec![0u8; page_size];
      for child in &child_li {
        let file_page_id = mapper.file_page_id(child.page_id).unwrap();
        store.read_page(&tbl, file_page_id, &mut buf).await.unwrap();
        let len = usize::from(content_len(&buf).unwrap());
        assert!(len >= page_size / 4, "len={len}");
      }
    });
  }
}
