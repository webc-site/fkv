//! 写批组装：公共入参 [`WriteArg`] / [`PageLayout`]、批内延迟写缓冲、路由游标，
//! 以及一次写批的入口 [`write_batch`]。
//!
//! 页内容一律先进 [`PendingWrite`]，由 [`write_batch`] 在 manifest 之前统一落盘。

use core::{
  mem::take,
  ops::{Deref, Index, IndexMut, Range},
};

use fkv_btree::{BatchEntry, MergeOption};
use fkv_compress::DictCompression;
use fkv_error::{KvError, Result};
use fkv_io::PageIo;
use fkv_mapping::PageMapper;
use fkv_page::{PageType, page_type};
use fkv_task_read::page_buf::PageBuf;
use fkv_types::{FilePageId, MAX_TREE_DEPTH, NULL_PAGE, PageId, TableIdent};
use gxhash::HashMap;

use crate::{
  data::{flush_deferred_all, rewrite_data_page, write_new_data_page},
  index::{apply_index_changes, build_new_index, read_index_children},
};

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
        now_ms: 0,
        ttl_upsert_sink: None,
      },
      index_restart_interval,
    }
  }

  /// 设置写批墙钟（毫秒）：重写 data 页时丢弃 `expire_ts <= now_ms` 的旧条目
  /// （对齐 C++ `BatchWriteTask` 的 `now_ms`）。`0` 表示不过滤。
  pub fn with_now_ms(mut self, now_ms: u64) -> Self {
    self.data.now_ms = now_ms;
    self
  }
}

/// 批量写路径的延迟写预算：对齐 C++ `KvOption::write_buffer_size` 默认 1 MB。
///
/// 达到预算即整批落盘，内存上界 = 预算 + 单个 data 页组的页数。
pub(crate) const PENDING_WRITE_BUDGET: usize = 1 << 20;

/// 批内延迟写缓冲（perf-82）：跨 data 页组累积页写，一次 `write_page_li`，
/// 对齐 C++ `WriteBufferAggregator` 的「同一 write task 内累积、`ShouldFlush` 触发」。
///
/// 同一 `FilePageId` 后写覆盖前写：批内对同一页的顺序写以最后一版为准。
/// 批内 read-after-write（分组读旧页、`update_next`/`update_prev`）必须查本缓冲。
#[derive(Default)]
pub(crate) struct PendingWrite {
  pub(crate) page_li: Vec<(FilePageId, Vec<u8>)>,
  pub(crate) slot_map: HashMap<FilePageId, usize>,
  pub(crate) bytes: usize,
}

impl PendingWrite {
  pub(crate) fn push(&mut self, file_page_id: FilePageId, page: Vec<u8>) {
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
  #[cfg(test)]
  pub(crate) fn get(&self, file_page_id: FilePageId) -> Option<&[u8]> {
    let idx = *self.slot_map.get(&file_page_id)?;
    self.page_li.get(idx).map(|(_, page)| page.as_slice())
  }

  /// 取出并移除；`remap_page` 后内容随逻辑页搬到新 `FilePageId`。
  pub(crate) fn take(&mut self, file_page_id: FilePageId) -> Option<Vec<u8>> {
    let idx = self.slot_map.remove(&file_page_id)?;
    let (_, page) = self.page_li.swap_remove(idx);
    self.bytes -= page.len();
    // `swap_remove` 把尾项搬到 idx：同步它在索引表里的下标。
    if let Some((moved, _)) = self.page_li.get(idx) {
      self.slot_map.insert(*moved, idx);
    }
    Some(page)
  }

  pub(crate) fn over_budget(&self) -> bool {
    self.bytes >= PENDING_WRITE_BUDGET
  }

  /// 整批落盘；一次 `write_page_li` 由 IO 层按 `max_write_batch_pages` 切分。
  pub(crate) async fn flush<I: PageIo>(&mut self, io: &I, tbl: &TableIdent) -> Result<()> {
    if self.page_li.is_empty() {
      return Ok(());
    }
    let page_li = take(&mut self.page_li);
    self.slot_map.clear();
    self.bytes = 0;
    io.write_page_li(tbl, &page_li).await
  }
}

/// 延迟写队列中的一页（C++ `leaf_triple_[0] / [1]` 的元素）。
///
/// 元素按写出顺序排列，队首最老；写出相位统一 `remap`（既有页）或提交预留槽位
/// （新建页）并放进批内缓冲。
pub(crate) struct DeferredPage {
  pub(crate) page_id: PageId,
  pub(crate) page: Vec<u8>,
  /// `true` 表示本批新建页：逻辑槽位由 `PageMapper::reserve_page` 预留，写出时
  /// 才分配物理页并追加 `UpdateMapping`（对齐 C++ `FinishDataPage` 的 `GetPage`
  /// 与 `WritePage` 内的 `AllocatePage`）。
  pub(crate) reserved: bool,
}

/// 写批共享上下文：IO、表、布局、批内延迟写缓冲，以及大 value 的 segment 映射。
pub(crate) struct ValueCtx<'a, 'io, I: PageIo> {
  pub(crate) io: &'io I,
  pub(crate) tbl: &'io TableIdent,
  pub(crate) layout: PageLayout<'a>,
  pub(crate) segment_mapper: Option<&'io mut PageMapper>,
  pub(crate) segment_size: u32,
  pub(crate) ttl_delete_sink: Option<&'io mut Vec<BatchEntry>>,
  /// 批内延迟写缓冲（perf-82）。
  pub(crate) pending: PendingWrite,
  /// 本批下一张会被重写的 data 页（`group_li` 下一组的 page id；`NULL_PAGE`
  /// 表示没有下一组）：后继页的 `prev` 修复延后到它自身的重写（sem-51）。
  pub(crate) next_planned_page: PageId,
  /// 前驱页分裂 / 删除登记、等下一组自身重写时消费的 `prev` 覆盖。
  pub(crate) deferred_prev: Option<(PageId, PageId)>,
  /// 本批已构建、尚未写出的 data 页（C++ `leaf_triple_[0] / [1]`，按叶链顺序
  /// 最多两页）。改写页与其 prev/next 修复都先留在这里，由下一个写出相位或
  /// 批末统一写出（sem-52）；因此前驱页的 `UpdateMapping` 不会抢在删除页的
  /// `DeleteMapping` 之前。分裂组的尾页同样延迟到此：它的 `UpdateMapping`
  /// 因此排在同批删除的后继页 `DeleteMapping` 之后（sem-53）。
  pub(crate) deferred_li: Vec<DeferredPage>,
}

#[derive(Clone, Copy)]
pub(crate) struct IndexLayout<'a> {
  pub(crate) page: PageLayout<'a>,
  pub(crate) leaf: bool,
}

/// 子项条目：键在页级扁平键区中的 `(offset, len)` + 子页 id。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChildEntry {
  pub(crate) offset: u32,
  pub(crate) len: u32,
  pub(crate) page_id: PageId,
}

/// 一个 index 页的子项列表：所有键连续存放在 `keys`，`entries` 只记偏移。
///
/// 读页 / 复制整表时整页最多两次分配（键区 + 项数组），不再按子项逐个分配
/// `Vec<u8>`（perf-07）。键按 `first_key` 升序，首项键恒为空（leftmost 不存键）。
#[derive(Debug, Default, Clone)]
pub(crate) struct Children {
  pub(crate) keys: Vec<u8>,
  pub(crate) entries: Vec<ChildEntry>,
}

impl PartialEq for Children {
  /// 逻辑相等：逐项比较键与 page_id（键区可能留有截断后的冗余字节，不参与比较）。
  fn eq(&self, other: &Self) -> bool {
    self.len() == other.len() && (0..self.len()).all(|idx| self.child(idx) == other.child(idx))
  }
}

impl Eq for Children {}

impl Children {
  pub(crate) fn with_capacity(child_cnt: usize, key_bytes: usize) -> Self {
    Self {
      keys: Vec::with_capacity(key_bytes),
      entries: Vec::with_capacity(child_cnt),
    }
  }

  pub(crate) fn len(&self) -> usize {
    self.entries.len()
  }

  pub(crate) fn is_empty(&self) -> bool {
    self.entries.is_empty()
  }

  pub(crate) fn push(&mut self, key: &[u8], page_id: PageId) -> Result<()> {
    let offset = u32::try_from(self.keys.len()).map_err(|_| KvError::InvalidArgs)?;
    let len = u32::try_from(key.len()).map_err(|_| KvError::InvalidArgs)?;
    self.keys.extend_from_slice(key);
    self.entries.push(ChildEntry {
      offset,
      len,
      page_id,
    });
    Ok(())
  }

  pub(crate) fn key(&self, idx: usize) -> Option<&[u8]> {
    let entry = self.entries.get(idx)?;
    let start = entry.offset as usize;
    self.keys.get(start..start + entry.len as usize)
  }

  pub(crate) fn page_id(&self, idx: usize) -> Option<PageId> {
    self.entries.get(idx).map(|entry| entry.page_id)
  }

  pub(crate) fn child(&self, idx: usize) -> Option<(&[u8], PageId)> {
    let entry = self.entries.get(idx)?;
    let key = self.keys.get(entry.offset as usize..)?;
    let key = key.get(..entry.len as usize)?;
    Some((key, entry.page_id))
  }
}

/// 按下标访问子项列表：扁平快照或其变更合并视图。
pub(crate) trait ChildSource {
  fn len(&self) -> usize;

  fn is_empty(&self) -> bool {
    self.len() == 0
  }

  fn child(&self, idx: usize) -> Option<(&[u8], PageId)>;
}

impl ChildSource for Children {
  fn len(&self) -> usize {
    self.entries.len()
  }

  fn is_empty(&self) -> bool {
    self.entries.is_empty()
  }

  fn child(&self, idx: usize) -> Option<(&[u8], PageId)> {
    Children::child(self, idx)
  }
}

/// 旧快照 + 变更区间的虚拟合并列表：重建时按需取子项，不复制整表（perf-07）。
///
/// `changes` 按旧下标升序且互不重叠；第 k 项用 `changes[k].1` 替换旧列表的
/// `changes[k].0`。合并后首项键恒为空，保持原 `replace_child` 清理后的语义。
pub(crate) struct MergedChildren<'a> {
  pub(crate) old: &'a Children,
  pub(crate) changes: &'a [(usize, Children)],
  /// 第 k 个变更在新列表中的起始下标。
  pub(crate) starts: Vec<usize>,
  pub(crate) len: usize,
}

impl<'a> MergedChildren<'a> {
  pub(crate) fn new(old: &'a Children, changes: &'a [(usize, Children)]) -> Result<Self> {
    let mut starts = Vec::with_capacity(changes.len());
    let mut shift = 0isize;
    for (k, (idx, new)) in changes.iter().enumerate() {
      if *idx >= old.len() || (k > 0 && *idx <= changes[k - 1].0) {
        return Err(KvError::Corrupted);
      }
      let start = isize::try_from(*idx)
        .ok()
        .and_then(|idx| idx.checked_add(shift))
        .and_then(|start| usize::try_from(start).ok())
        .ok_or(KvError::Corrupted)?;
      starts.push(start);
      let delta = isize::try_from(new.len()).map_err(|_| KvError::Corrupted)?;
      shift = shift.checked_add(delta - 1).ok_or(KvError::Corrupted)?;
    }
    Ok(Self {
      old,
      changes,
      starts,
      len: old
        .len()
        .checked_add_signed(shift)
        .ok_or(KvError::Corrupted)?,
    })
  }

  /// 合并列表 `idx` 处的原始子项（不做 leftmost 键清空）。
  pub(crate) fn child_raw(&self, idx: usize) -> Option<(&[u8], PageId)> {
    if idx >= self.len {
      return None;
    }
    let k = self.starts.partition_point(|&start| start <= idx);
    let Some(k) = k.checked_sub(1) else {
      return self.old.child(idx);
    };
    let (old_idx, new) = self.changes.get(k)?;
    let off = idx - self.starts[k];
    if off < new.len() {
      new.child(off)
    } else {
      self.old.child(old_idx + 1 + off - new.len())
    }
  }

  /// 物化整个合并列表（仅根收缩等常数级场景使用）。
  pub(crate) fn to_children(&self) -> Result<Children> {
    let mut out = Children::with_capacity(self.len, self.old.keys.capacity());
    for idx in 0..self.len {
      // 经 `ChildSource` 取子项：首项键按 leftmost 语义清空。
      let (key, page_id) = ChildSource::child(self, idx).ok_or(KvError::Corrupted)?;
      out.push(key, page_id)?;
    }
    Ok(out)
  }
}

impl ChildSource for MergedChildren<'_> {
  fn len(&self) -> usize {
    self.len
  }

  fn child(&self, idx: usize) -> Option<(&[u8], PageId)> {
    if idx == 0 {
      // 首项是页 leftmost，键不参与编码；左most 被删时合并视图首项来自
      // 旧列表的下一项，必须清空其键（对齐原 `replace_child`）。
      return self.child_raw(0).map(|(_, page_id)| (&[][..], page_id));
    }
    self.child_raw(idx)
  }
}

/// 一个 data 页的整组写入：批量按 key 分桶的结果（issue 29）。
pub(crate) struct WriteGroup<'a> {
  /// 组内最小 key；用于按 key 升序处理各组。
  pub(crate) first_key: &'a [u8],
  pub(crate) page_id: PageId,
  /// 本批开始时最底层 index 页及其子项下标。
  pub(crate) leaf_index: (PageId, usize),
  /// 本组在 `entry_li` 中的连续区间（key 全局升序 ⇒ 同页组必然连续）。
  pub(crate) entry_range: Range<usize>,
}

/// 单个 index 页在本批开始时的快照。
pub(crate) struct Frame {
  /// 子项快照（`first_key` 升序，首项为空）。
  pub(crate) child_li: Children,
  /// 本批开始时该页的原始字节（整页缓冲）；供重建时做 region 直拷（fmt-17b）。
  /// 缓冲来自线程本地池，帧释放时自动归还，读入后不再克隆（perf-08）。
  pub(crate) page: PageBuf,
  /// 该页在父节点中的分隔键；root 为空。
  pub(crate) first_key: Vec<u8>,
  /// `(父 index 页, 本页在父节点 child_li 中的下标)`；root 为 `None`。
  pub(crate) parent: Option<(PageId, usize)>,
  /// 到 root 的层数，root 为 0。
  pub(crate) level: usize,
  pub(crate) leaf: bool,
  /// 已被某个写组路径引用：回溯离开子树时仍要交给重建阶段。
  pub(crate) referenced: bool,
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
  // 升序批（批量加载 / 范围批量写）占绝大多数：先线性探测一次，已是严格
  // 升序时排序与折叠都可整段跳过（perf-63）。非严格升序（含同 key）只需
  // 折叠、不必排序；只有真正乱序才走稳定排序。
  let mut sorted = true;
  let mut strict = true;
  for pair in entry_li.windows(2) {
    match pair[0].key.cmp(&pair[1].key) {
      core::cmp::Ordering::Less => {}
      core::cmp::Ordering::Equal => strict = false,
      core::cmp::Ordering::Greater => {
        sorted = false;
        strict = false;
        break;
      }
    }
  }
  if !sorted {
    // 稳定排序：同 key 的批内先后关系保留，供下面的 timestamp 折叠取"后者"。
    entry_li.sort_by(|left, right| left.key.cmp(&right.key));
  }
  if !strict {
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
  }
  let mut ctx = ValueCtx {
    io,
    tbl,
    layout,
    segment_mapper,
    segment_size,
    ttl_delete_sink,
    pending: PendingWrite::default(),
    next_planned_page: NULL_PAGE,
    deferred_prev: None,
    deferred_li: Vec::new(),
  };
  // data 页之上第一层 index 必为 leaf index。
  let data_layout = IndexLayout {
    page: layout,
    leaf: true,
  };
  if root_id == NULL_PAGE {
    let data_child_li =
      write_new_data_page(io, tbl, mapper, &entry_li, layout, &mut ctx.pending).await?;
    let root = build_new_index(
      io,
      tbl,
      mapper,
      data_child_li,
      data_layout,
      &mut ctx.pending,
    )
    .await?;
    // data + index 页同批落盘：manifest 之前只此一次 `write_page_li`（perf-02）。
    ctx.pending.flush(io, tbl).await?;
    return Ok(root);
  }

  // 一次有序下降到最底层 index，得到 data page 的 key 区间路由表；
  // 之后按 key 直接分桶，避免每个 entry 各自从 root 下降一次（issue 29）。
  let mut route = RouteCursor::open(
    io,
    tbl,
    root_id,
    mapper,
    layout.data.page_size,
    entry_li[0].key.as_slice(),
  )
  .await?;
  if route.data_page != NULL_PAGE {
    // 整棵树只有一个 data 页（root 即 data 页）：所有 key 都落在这一页。
    let page_id = route.data_page;
    let old_page = read_page_by_id(
      io,
      tbl,
      mapper,
      page_id,
      layout.data.page_size,
      &mut ctx.pending,
    )
    .await?;
    let child_li = rewrite_data_page(&mut ctx, mapper, page_id, &old_page, &entry_li, b"").await?;
    // 根即 data 页：本组重写留在延迟队列里，批末（index 重建之前）写出
    // （C++ 批末 `ShiftLeafLink`，sem-52）。
    flush_deferred_all(&mut ctx.deferred_li, &mut ctx.pending, mapper)?;
    let root = build_new_index(io, tbl, mapper, child_li, data_layout, &mut ctx.pending).await?;
    ctx.pending.flush(io, tbl).await?;
    return Ok(root);
  }
  let mut group_li: Vec<WriteGroup<'_>> = Vec::new();
  for (entry_idx, entry) in entry_li.iter().enumerate() {
    let page_id = route
      .page_for(io, tbl, mapper, layout.data.page_size, &entry.key)
      .await?;
    // 升序批次中 data 页只会单调推进：与当前组同页即续到区间末尾，否则开新组。
    // 因此不需要 `page_id → 组` 的哈希表，也不会为非连续的同页条目回填。
    match group_li.last_mut() {
      Some(group) if group.page_id == page_id => {
        if group.entry_range.end != entry_idx {
          return Err(KvError::Corrupted);
        }
        group.entry_range.end = entry_idx + 1;
      }
      _ => group_li.push(WriteGroup {
        first_key: entry.key.as_slice(),
        page_id,
        leaf_index: route.path_snapshot(),
        entry_range: entry_idx..entry_idx + 1,
      }),
    }
  }
  // 路由结束：只留本批会改动的 index 页及其祖先（各组路径并集）。
  // 其余帧（已离开的子树）在回溯时已释放，大跨批不再驻留整棵索引（R2-2）。
  route.retain_frames();
  group_li.sort_by(|left, right| left.first_key.cmp(right.first_key));
  // 逐组重写 data 页：路由快照在本阶段保持不变，因为 index 页一次都不写盘。
  // 同父 index 页下的多个 data 页变更在内存中合并，最后自底向上重建一次（issue 49）。
  let mut change_map: HashMap<PageId, Vec<(usize, Children)>> = HashMap::default();
  for (group_idx, group) in group_li.iter().enumerate() {
    // 下一组会整组重写；若它是本页的后继页，`update_prev` 只登记 prev 覆盖，
    // 由下一组的写出统一落盘（sem-51）。
    ctx.next_planned_page = group_li
      .get(group_idx + 1)
      .map_or(NULL_PAGE, |next| next.page_id);
    // 上一组登记的后继页 prev 覆盖必须正好属于本组。
    if let Some((target, _)) = ctx.deferred_prev {
      debug_assert_eq!(target, group.page_id, "prev 覆盖必须属于下一组");
    }
    let (leaf_page_id, child_idx) = group.leaf_index;
    let frame = route
      .frame_map
      .get(&leaf_page_id)
      .ok_or(KvError::Corrupted)?;
    // 每组的父 index 原分隔键：data 页首段沿用，不自作主张抬高（issue 49）。
    let old_sep = frame.child_li.key(child_idx).ok_or(KvError::Corrupted)?;
    // 读覆盖层：前一组可能已改写本页的 prev/next 指针。
    let old_page = read_page_by_id(
      io,
      tbl,
      mapper,
      group.page_id,
      layout.data.page_size,
      &mut ctx.pending,
    )
    .await?;
    let child_li = rewrite_data_page(
      &mut ctx,
      mapper,
      group.page_id,
      &old_page,
      &entry_li[group.entry_range.clone()],
      old_sep,
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
  debug_assert!(
    ctx.deferred_prev.is_none(),
    "prev 覆盖必须被下一组的重写消费"
  );
  // 批末写出全部延迟 data 页（C++ `ProcessBatch` 末尾的两次 `ShiftLeafLink`），
  // 之后才重建 index 页（sem-52）。
  flush_deferred_all(&mut ctx.deferred_li, &mut ctx.pending, mapper)?;
  // data / overflow / index 页统一进批内缓冲，manifest 之前一次 `write_page_li`
  // （perf-02：索引页不再逐页独立写）。
  let root = apply_index_changes(&mut ctx, mapper, &route.frame_map, root_id, change_map).await?;
  ctx.pending.flush(io, tbl).await?;
  Ok(root)
}

#[derive(Clone, Copy, Default)]
pub(crate) struct RouteNode {
  pub(crate) page_id: PageId,
  pub(crate) idx: usize,
}

/// 路由路径栈：深度上界是编译期常量 `MAX_TREE_DEPTH`，定长栈上存储，
/// 每批不再为路径本身分配堆内存。
pub(crate) struct RouteStack {
  pub(crate) nodes: [RouteNode; MAX_TREE_DEPTH],
  pub(crate) len: usize,
}

impl RouteStack {
  pub(crate) fn new() -> Self {
    Self {
      nodes: [RouteNode::default(); MAX_TREE_DEPTH],
      len: 0,
    }
  }

  pub(crate) fn len(&self) -> usize {
    self.len
  }

  pub(crate) fn is_empty(&self) -> bool {
    self.len == 0
  }

  pub(crate) fn last(&self) -> Option<&RouteNode> {
    self.nodes.get(self.len.checked_sub(1)?)
  }

  pub(crate) fn last_mut(&mut self) -> Option<&mut RouteNode> {
    self.nodes.get_mut(self.len.checked_sub(1)?)
  }

  pub(crate) fn push(&mut self, node: RouteNode) -> Result<()> {
    let slot = self.nodes.get_mut(self.len).ok_or(KvError::Corrupted)?;
    *slot = node;
    self.len += 1;
    Ok(())
  }

  pub(crate) fn pop(&mut self) -> Option<RouteNode> {
    self.len = self.len.checked_sub(1)?;
    self.nodes.get(self.len).copied()
  }

  pub(crate) fn iter(&self) -> impl Iterator<Item = &RouteNode> {
    self.nodes.get(..self.len).unwrap_or(&[]).iter()
  }
}

impl Index<usize> for RouteStack {
  type Output = RouteNode;

  fn index(&self, idx: usize) -> &Self::Output {
    &self.nodes[idx]
  }
}

impl IndexMut<usize> for RouteStack {
  fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
    &mut self.nodes[idx]
  }
}

/// 写路径的 B+tree 游标：从 root 下降到最底层 index，批次 key 升序时
/// 只在必要时向右推进/回溯，整体 O(树高 + 跨页次数)，对齐 C++ `stack_`。
///
/// 游标同时缓存沿路 index 页的子项快照（`frame_map`），供延迟重建阶段使用：
/// 路由期间一个 index 页都不写盘，因此快照在整批内始终有效（issue 49）。
pub(crate) struct RouteCursor {
  /// index 路径（root → leaf index），最后一层是 `LeafIndex`；子项只在 `frame_map` 里存一份。
  pub(crate) path_li: RouteStack,
  /// index 页 → 本批开始时的子项快照。
  pub(crate) frame_map: HashMap<PageId, Frame>,
  /// 根即 data 页时直接返回该页；否则为 `NULL_PAGE`。
  pub(crate) data_page: PageId,
  /// 相邻 key 快速路由缓存：上一次命中的 data 页 + 该子项的上界分隔键（不含）。
  ///
  /// 批内 key 严格升序（`page_for` 的入参契约），因此绝大多数 key 只落在上一次
  /// 命中的同一页里。命中缓存时只需一次 `key < bound` 比较，省掉每个 key 两次
  /// `frame_map` 查表、一次祖先上界遍历与一次子项二分（批量删除实测该项占
  /// `write_batch` 三成以上 CPU）。`None` 的上界表示「子树无后继兄弟」，即
  /// 上界为正无穷。任何改变路由状态的路径（`open` / 回溯 / 下降）都必须让该
  /// 缓存失效，见 `invalidate_fast` 与 `refresh_fast` 的调用点。
  fast_route: Option<(PageId, Option<Vec<u8>>)>,
}

impl RouteCursor {
  /// 上界比较：`key` 严格小于缓存上界时仍落在缓存页（上界为 `None` 即正无穷）。
  fn fast_hit(&self, key: &[u8]) -> Option<PageId> {
    let (page_id, bound) = self.fast_route.as_ref()?;
    match bound {
      Some(bound) if key >= bound.as_slice() => None,
      _ => Some(*page_id),
    }
  }

  /// 写入 / 清除快速路由缓存：本批路由状态变化后必须调用。
  fn invalidate_fast(&mut self) {
    self.fast_route = None;
  }

  /// 用当前游标位置刷新快速路由缓存（只在完整路由成功后调用）。
  ///
  /// `child_page_id` 是本次命中的 data 页（`page_for` 的返回值），不是 `path_li`
  /// 栈顶的 leaf index 页 id。上界取「本帧下一个子项的分隔键」与「最近一个有
  /// 下一个兄弟的祖先的分隔键」的较小者：后者正是原 `in_range` 判定使用的子树
  /// 上界。子项键沿路径单调，前者存在时必然 <= 后者，因此前者存在就取它。
  fn refresh_fast(&mut self, child_page_id: PageId) -> Result<()> {
    let Some(lvl) = self.path_li.len().checked_sub(1) else {
      self.invalidate_fast();
      return Ok(());
    };
    let page_id = self.path_li[lvl].page_id;
    let idx = self.path_li[lvl].idx;
    let own_next = self
      .frame_map
      .get(&page_id)
      .ok_or(KvError::Corrupted)?
      .child_li
      .child(idx + 1)
      .map(|(key, _)| key.to_vec());
    let bound = match own_next {
      Some(key) => Some(key),
      None => {
        let mut found = None;
        for up in (0..lvl).rev() {
          let frame = self
            .frame_map
            .get(&self.path_li[up].page_id)
            .ok_or(KvError::Corrupted)?;
          if let Some((key, _)) = frame.child_li.child(self.path_li[up].idx + 1) {
            found = Some(key.to_vec());
            break;
          }
        }
        found
      }
    };
    self.fast_route = Some((child_page_id, bound));
    Ok(())
  }
}

/// 子项键严格升序（首项为 leftmost、键为空）：取「最后一个键 <= `key` 的子项」，
/// 与读路径 `IndexPageIter::seek_floor` 的定位口径一致。
///
/// 二分而非从游标位置线性右推：游标回溯（`descend_leftmost` 把新压入层的 idx
/// 归零）后线性右推要重扫整张 index 页的子项，单键批下每跨一个子项还要重读并
/// 重解码整页，代价随表增大线性放大（200k 行实测 ~600 次子项比较 / 批）。
pub(crate) fn last_child_le(child_li: &Children, key: &[u8]) -> Result<usize> {
  last_child_le_from(child_li, key, 0)
}

/// 同上，但已知 `from` 之前的子项都已确定不覆盖 `key`（`from == 0` 时 leftmost
/// 恒为起点）：只在下标 `>= from` 的区间内二分，保持「游标只前进」的语义。
pub(crate) fn last_child_le_from(child_li: &Children, key: &[u8], from: usize) -> Result<usize> {
  // `from` 越界时报 `Corrupted` 与线性右推一致（`page_id(from)` 取不到），夹一下
  // 只为避免 `hi - idx` 下溢。
  let mut idx = from.min(child_li.len());
  let mut hi = child_li.len();
  while hi - idx > 1 {
    let mid = idx + (hi - idx) / 2;
    let (mid_key, _) = child_li.child(mid).ok_or(KvError::Corrupted)?;
    // 空键只可能出现在 leftmost（下标 0，恒不参与比较）；出现即视为「已越过 key」。
    if mid_key.is_empty() || mid_key > key {
      hi = mid;
    } else {
      idx = mid;
    }
  }
  Ok(idx)
}

impl RouteCursor {
  /// `first_key` 是本批最小键（批内 key 升序）：游标直接按它下降到所属子树。
  ///
  /// 不能盲取 leftmost 后再靠 `page_for` 逐子项右推——右推每跨一个子项都要重读
  /// 并重解码整张 index 页，单键批下随表增大线性放大（对齐 C++ `BatchWriteTask`
  /// 从 root 按 key 二分下降的寻址；推进逻辑仍在 `page_for` 里兜底）。
  pub(crate) async fn open<I: PageIo>(
    io: &I,
    tbl: &TableIdent,
    root_id: PageId,
    mapper: &PageMapper,
    page_size: usize,
    first_key: &[u8],
  ) -> Result<Self> {
    let mut cursor = Self {
      path_li: RouteStack::new(),
      frame_map: HashMap::default(),
      data_page: NULL_PAGE,
      fast_route: None,
    };
    let mut page_id = root_id;
    for _ in 0..MAX_TREE_DEPTH {
      let file_page_id = mapper.file_page_id(page_id)?;
      // 每层一个池缓冲、owned 直读后直接 move 进帧：读入不再整页拷贝，
      // 也不再「读入 + clone」双份（perf-08 / perf-10）。
      let mut page = PageBuf::new(page_size);
      io.read_page_owned(tbl, file_page_id, page.as_mut_vec())
        .await?;
      match page_type(&page)? {
        PageType::Data => {
          // 根即 data 页：缓冲随局部变量归还池。
          cursor.data_page = page_id;
          return Ok(cursor);
        }
        PageType::LeafIndex | PageType::NonLeafIndex => {
          let (child_li, leaf) = read_index_children(&page)?;
          let child_idx = last_child_le(&child_li, first_key)?;
          let child_page_id = child_li.page_id(child_idx).ok_or(KvError::Corrupted)?;
          cursor.push_frame(page_id, child_li, leaf, page)?;
          // 压栈后再定位本层游标：下一层帧的 `first_key` 取本层 `child_idx` 的分隔键，
          // 与「本帧是父页第 child_idx 个子项」保持一致（issue 49 的原分隔键口径）。
          if let Some(top) = cursor.path_li.last_mut() {
            top.idx = child_idx;
          }
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
  pub(crate) async fn page_for<I: PageIo>(
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
    // 相邻 key 快路径：仍在上次命中页的上界内就直接返回。批内 key 升序是
    // `page_for` 的入参契约，故无需再验证下界（上次命中的子项键 <= key）。
    if let Some(page_id) = self.fast_hit(key) {
      return Ok(page_id);
    }
    self.invalidate_fast();
    loop {
      let lvl = self.path_li.len() - 1;
      // 当前子树的上界：沿路径向上找最近一个还有下一个兄弟的祖先，取其
      // 兄弟的分隔键。不能只看直接父节点——当前页是父页末子、而父页本身
      // 还有兄弟时，祖先上界依然生效（sem-30：root 分裂后左子树 100 的末子
      // 越过 root 分隔键仍被判为「在范围内」，超界 key 被写进该子树最后一页，
      // 新页挂到错误父页，点读按分隔键路由时整段漏读，scan 因走页链仍可见）。
      let mut in_range = true;
      for up in (0..lvl).rev() {
        let frame = self
          .frame_map
          .get(&self.path_li[up].page_id)
          .ok_or(KvError::Corrupted)?;
        if let Some((next_key, _)) = frame.child_li.child(self.path_li[up].idx + 1) {
          in_range = key < next_key;
          break;
        }
      }
      if in_range {
        let page_id = self.path_li[lvl].page_id;
        let frame = self.frame_map.get(&page_id).ok_or(KvError::Corrupted)?;
        // 定位「最后一个键 <= key 的子项」：二分（O(log 子项数)）而非从游标位置
        // 线性右推——游标回溯（`descend_leftmost` 把新压入层的 idx 归零）后线性
        // 右推要重扫整张 index 页的子项。批内 key 升序时二分退化为一两次比较。
        let idx = last_child_le_from(&frame.child_li, key, self.path_li[lvl].idx)?;
        let child_page_id = frame.child_li.page_id(idx).ok_or(KvError::Corrupted)?;
        self.path_li[lvl].idx = idx;
        self.refresh_fast(child_page_id)?;
        return Ok(child_page_id);
      }
      // key 超出当前子树：回溯到能覆盖它的祖先，再向左下降。
      self.invalidate_fast();
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
  /// 保证祖先必在各组路径里。回溯时已按 `referenced` 即时释放，这里只做终检。
  pub(crate) fn retain_frames(&mut self) {
    self.frame_map.retain(|_, frame| frame.referenced);
  }

  /// 回溯一层：没有任何写组引用该帧时立即释放，路由期内存降为 O(树高)。
  pub(crate) fn pop_frame(&mut self) {
    if let Some(node) = self.path_li.pop()
      && self
        .frame_map
        .get(&node.page_id)
        .is_some_and(|frame| !frame.referenced)
    {
      self.frame_map.remove(&node.page_id);
    }
  }

  /// 当前最底层 index 帧 `(index 页, 子项下标)`；同时把整条路径登记为不可释放
  /// （重建阶段要用）。
  pub(crate) fn path_snapshot(&mut self) -> (PageId, usize) {
    for node in self.path_li.iter() {
      if let Some(frame) = self.frame_map.get_mut(&node.page_id) {
        frame.referenced = true;
      }
    }
    self
      .path_li
      .last()
      .map_or((NULL_PAGE, 0), |node| (node.page_id, node.idx))
  }

  /// 压入一层 index 帧；父帧一定已建立（下降方向自 root 向下）。
  pub(crate) fn push_frame(
    &mut self,
    page_id: PageId,
    child_li: Children,
    leaf: bool,
    page: PageBuf,
  ) -> Result<()> {
    let parent = self.path_li.last().map(|node| (node.page_id, node.idx));
    let parent_frame = parent.and_then(|(parent_id, _)| self.frame_map.get(&parent_id));
    let first_key = match (parent, parent_frame) {
      (Some((_, child_idx)), Some(frame)) => frame
        .child_li
        .key(child_idx)
        .map_or_else(Vec::new, <[u8]>::to_vec),
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
        referenced: false,
      },
    );
    self.path_li.push(RouteNode { page_id, idx: 0 })?;
    Ok(())
  }

  /// 从当前节点最左子页下降到 `LeafIndex` 层并压栈。
  pub(crate) async fn descend_leftmost<I: PageIo>(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    mapper: &PageMapper,
    page_size: usize,
  ) -> Result<()> {
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
        .page_id(top_idx)
        .ok_or(KvError::Corrupted)?;
      // 快照里已有该页（回溯重访）：直接压栈，避免重复读盘。
      if let Some(frame) = self.frame_map.get(&child_page_id) {
        let leaf = frame.leaf;
        self.path_li.push(RouteNode {
          page_id: child_page_id,
          idx: 0,
        })?;
        if leaf {
          return Ok(());
        }
        continue;
      }
      let file_page_id = mapper.file_page_id(child_page_id)?;
      // 池缓冲 owned 直读后直接 move 进帧（perf-08 / perf-10）。
      let mut page = PageBuf::new(page_size);
      io.read_page_owned(tbl, file_page_id, page.as_mut_vec())
        .await?;
      match page_type(&page)? {
        PageType::LeafIndex => {
          let (child_li, _) = read_index_children(&page)?;
          self.push_frame(child_page_id, child_li, true, page)?;
          return Ok(());
        }
        PageType::NonLeafIndex => {
          let (child_li, _) = read_index_children(&page)?;
          self.push_frame(child_page_id, child_li, false, page)?;
        }
        // index 页只能指向 index 页：data 页只允许出现在 root。
        PageType::Data | PageType::Overflow | PageType::Deleted => {
          return Err(KvError::Corrupted);
        }
      }
    }
  }
}

/// 读到的整页：批内缓冲命中时取走其 `Vec`（零拷贝，不再整页 `to_vec`），否则
/// 读入线程本地池缓冲。
///
/// 两种来源都只在本组重写期间使用：池缓冲 `Drop` 时归还池；批内缓冲被取走后
/// 由重写产生新版本（`rewrite_data_page` 原本也会丢弃该页的旧版本）。
pub(crate) enum PageRead {
  Pending(Vec<u8>),
  Pooled(PageBuf),
}

impl Deref for PageRead {
  type Target = [u8];

  fn deref(&self) -> &Self::Target {
    match self {
      Self::Pending(page) => page.as_slice(),
      Self::Pooled(page) => page,
    }
  }
}

pub(crate) async fn read_page_by_id<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &PageMapper,
  page_id: PageId,
  page_size: usize,
  pending: &mut PendingWrite,
) -> Result<PageRead> {
  let file_page_id = mapper.file_page_id(page_id)?;
  // 批内已改写的页以缓冲里的最新版本为准。
  if let Some(page) = pending.take(file_page_id) {
    return Ok(PageRead::Pending(page));
  }
  let mut page = PageBuf::new(page_size);
  io.read_page_owned(tbl, file_page_id, page.as_mut_vec())
    .await?;
  Ok(PageRead::Pooled(page))
}
