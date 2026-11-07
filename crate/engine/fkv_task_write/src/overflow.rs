//! overflow 页链：大 value 落盘、批内指针回填，以及被替换/删除 value 的页链释放。

use fkv_btree::{BatchEntry, OverflowSlot};
use fkv_error::{KvError, Result};
use fkv_io::PageIo;
use fkv_mapping::PageMapper;
use fkv_page::{
  DataPageIter,
  overflow::{
    OverflowPage, build as build_overflow_page, capacity as overflow_capacity, decode_pointers,
    encode_pointers,
  },
  set_checksum,
};
use fkv_task_read::page_buf::PageBuf;
use fkv_types::{PageId, TableIdent, is_expired};
use gxhash::HashSet;

use crate::{
  batch::{DeferredPage, PageLayout, PendingWrite, ValueCtx},
  ttl,
};

/// 一组输出 data 页的写出计划：overflow 槽位按 key 顺序落盘时，每完成一张
/// 输出页就按 C++ `FinishDataPage` 的页边界取页 id / 写出前一页（sem-54）。
///
/// C++ `BatchWriteTask::ApplyOnePage` 的 merge 循环里，overflow value 由
/// `add_to_page` 的 `WriteOverflowValue` **立即**分配并写盘；当前输出页写满时
/// `FinishDataPage` 先给新页 `GetPage()` 取逻辑 id（`batch_write_task.cpp:1070`），
/// 再由 `ShiftLeafLink`（`:122-131`）把前一张页 `WritePage`（`AllocatePage` +
/// `UpdateMapping`，`write_task.cpp:238-267`、`:551-621`）。因此 overflow 记录与
/// data 页 id / mapping 按输出页边界交错，而不是整组 overflow 先于全部 data 页。
///
/// 页边界的触发键是下一张输出页的首键：C++ 在处理该键的 `add_to_page` 里
/// `Add` 失败才进 `FinishDataPage`，而该键的旧值释放（`DelOverflowValue`）与
/// 新 overflow 分配（`WriteOverflowValue`）都已先发生。实现因此按「事件之前
/// 关闭 trigger < key 的页、事件之后关闭 trigger == key 的页」推进。
pub(crate) trait OutputPageWrite {
  /// 关闭 trigger 键已到达的页边界。`inclusive == false` 只关闭 trigger 严格
  /// 小于 `key` 的页（在本键的 free / overflow 分配之前），`true` 关闭 trigger
  /// 不大于 `key` 的页（在本键事件之后）。
  fn close_reached_pages(
    &mut self,
    mapper: &mut PageMapper,
    deferred_li: &mut Vec<DeferredPage>,
    pending: &mut PendingWrite,
    page_li: &mut [Vec<u8>],
    key: &[u8],
    inclusive: bool,
  ) -> Result<()>;

  /// 关闭组内剩余全部页边界（含最后一张页的 `FinishDataPage`，C++
  /// `ApplyOnePage` 末尾的 `FinishDataPage`）。
  fn close_all_pages(
    &mut self,
    mapper: &mut PageMapper,
    deferred_li: &mut Vec<DeferredPage>,
    pending: &mut PendingWrite,
    page_li: &mut [Vec<u8>],
  ) -> Result<()>;
}

/// 没有输出页的场景（整页删除：只释放旧值；或新页路径的空 merge 结果）。
///
/// 没有页边界可关闭：`close_*` 对所有事件都是空操作，只保留 free / alloc 的
/// 逐 key 相位。
impl OutputPageWrite for () {
  fn close_reached_pages(
    &mut self,
    _mapper: &mut PageMapper,
    _deferred_li: &mut Vec<DeferredPage>,
    _pending: &mut PendingWrite,
    _page_li: &mut [Vec<u8>],
    _key: &[u8],
    _inclusive: bool,
  ) -> Result<()> {
    Ok(())
  }

  fn close_all_pages(
    &mut self,
    _mapper: &mut PageMapper,
    _deferred_li: &mut Vec<DeferredPage>,
    _pending: &mut PendingWrite,
    _page_li: &mut [Vec<u8>],
  ) -> Result<()> {
    Ok(())
  }
}

/// 释放被本批 delete / 覆盖的 overflow 页链与大 value segment（对齐 C++
/// `ApplyOnePage` 的 `DelOverflowValue` / `DelLargeValue`）。
///
/// 只用于合并结果为空（整页条目全被删除 / 过期）的 data 页：没有新 overflow
/// 链要写，只需按旧页 key 升序释放。仍有条目的页面走 [`rewrite_page_values`]，
/// 在那里释放与分配按 key 交错。
pub(crate) async fn free_replaced_values<I: PageIo>(
  ctx: &mut ValueCtx<'_, '_, I>,
  mapper: &mut PageMapper,
  old_page: &[u8],
  batch_li: &[&BatchEntry],
) -> Result<()> {
  let mut empty_page_li = [];
  rewrite_page_values(
    ctx,
    mapper,
    old_page,
    batch_li,
    &mut empty_page_li,
    &[],
    &mut (),
  )
  .await
}

/// 重写既有 data 页的 value 落盘：按 key 升序对每个 key 先释放被替换的旧值，
/// 再写该 key 的新 overflow 链并回填页内指针。
///
/// 对齐 C++ `BatchWriteTask::ApplyOnePage` 的 merge 循环（batch_write_task.cpp:
/// 527-693）：匹配到被覆盖的旧条目时立即 `DelOverflowValue` / `DelLargeValue`，
/// 随后 `add_to_page` 内的 `WriteOverflowValue` 立即分配新链。若整页先全量
/// free 再全量 alloc（sem-48 前的实现），空闲链头是「后释放的 key」，第一个新
/// value 会复用后一个 key 的旧逻辑页——log delta 的 page_id 序列、最终 mapping
/// 归属与 data page 的 overflow 指针列表都与 C++ 不同。
///
/// 旧条目即使没有被本批覆盖，只要 `expire_ts <= now_ms`，merge 阶段就已把它
/// 从新页丢弃（sem-50，对齐 C++ `add_to_page` 的过期分支）；这里在同一个 key
/// 位置释放其 overflow / 段并删除 TTL 索引项。释放与分配因此仍按合并后的 key
/// 顺序交错，与 C++ 的 free→alloc 相位一致。
///
/// 输出页边界（sem-54）由 `output` 的 [`OutputPageWrite`] 按 C++ `FinishDataPage`
/// 的相位推进：下一张输出页的首键即当前页的触发键，该键的 free / overflow 分配
/// 先于页边界，边界在该键事件之后关闭；trigger 键没有事件时推迟到下一个事件
/// 之前。组内最后一页的边界由 [`OutputPageWrite::close_all_pages`] 在末尾关闭。
///
/// 旧页 key 与 `overflow_li` 均严格升序，双游标一遍完成，不建中间事件表。
pub(crate) async fn rewrite_page_values<I: PageIo, B: OutputPageWrite>(
  ctx: &mut ValueCtx<'_, '_, I>,
  mapper: &mut PageMapper,
  old_page: &[u8],
  batch_li: &[&BatchEntry],
  page_li: &mut [Vec<u8>],
  overflow_li: &[OverflowSlot],
  output: &mut B,
) -> Result<()> {
  debug_assert!(
    batch_li.windows(2).all(|pair| pair[0].key < pair[1].key),
    "批内 key 必须严格升序且已去重"
  );
  debug_assert!(
    overflow_li.windows(2).all(|pair| pair[0].key < pair[1].key),
    "overflow_li 必须按 key 严格升序"
  );
  // 页内仍是等长占位指针：先把每个 slot 的 value 区间一次定位好。
  let range_li = overflow_slot_range_li(page_li, overflow_li)?;
  let mut slot_idx = 0usize;
  let mut batch_idx = 0usize;
  // 新页路径没有旧页（`old_page` 为空）：只走尾部 slot 循环。
  if !old_page.is_empty() {
    let mut iter = DataPageIter::new(old_page)?;
    while iter.advance()? {
      let key = iter.key();
      // 排在本旧 key 之前的新键（纯插入）：C++ 在同一 merge 循环里先为它们分配。
      while slot_idx < overflow_li.len() && overflow_li[slot_idx].key.as_slice() < key {
        let slot_key = overflow_li[slot_idx].key.as_slice();
        output.close_reached_pages(
          mapper,
          &mut ctx.deferred_li,
          &mut ctx.pending,
          page_li,
          slot_key,
          false,
        )?;
        write_page_slot(ctx, mapper, page_li, &range_li[slot_idx]).await?;
        output.close_reached_pages(
          mapper,
          &mut ctx.deferred_li,
          &mut ctx.pending,
          page_li,
          slot_key,
          true,
        )?;
        slot_idx += 1;
      }
      // 本旧 key 的 free / overflow 分配之前：先关闭 trigger 键更小的页。
      output.close_reached_pages(
        mapper,
        &mut ctx.deferred_li,
        &mut ctx.pending,
        page_li,
        key,
        false,
      )?;
      // 批内 key 与旧页 key 均升序：单指针跳过落在当前旧 key 之前的批项。
      while batch_idx < batch_li.len() && batch_li[batch_idx].key.as_slice() < key {
        batch_idx += 1;
      }
      let batch_entry = batch_li
        .get(batch_idx)
        .filter(|entry| entry.key.as_slice() == key);
      // 只有 batch_entry 真的覆盖了旧条目（同序或更新）才能释放旧值与旧 TTL 索引项：
      // 更旧时间戳的 delete 会被 merge 丢弃（旧条目存活，见 `fkv_btree` 的
      // `kept.timestamp < iter.timestamp()` 分支），此时释放会让存活条目悬垂、
      // 旧 TTL 索引被清掉导致过期永不触发（sto-65）。
      let released = if let Some(batch_entry) =
        batch_entry.filter(|entry| entry.timestamp >= iter.timestamp())
      {
        release_replaced_value(ctx, mapper, &iter, batch_entry).await?;
        true
      } else if is_expired(iter.expire_ts(), ctx.layout.data.now_ms) {
        // 本批没有覆盖它，但旧条目已过期：merge 已把它从新页丢弃，这里释放
        // 旧值与 TTL 索引项（对齐 C++ `add_to_page` 的过期分支）。
        release_expired_value(ctx, mapper, &iter).await?;
        true
      } else {
        false
      };
      if released {
        // 本 key 的新 overflow 链：释放后立即分配（对齐 C++ `add_to_page` 内的
        // `WriteOverflowValue`，同 key 复用自己刚释放的逻辑页）。
        if slot_idx < overflow_li.len() && overflow_li[slot_idx].key.as_slice() == key {
          write_page_slot(ctx, mapper, page_li, &range_li[slot_idx]).await?;
          slot_idx += 1;
        }
      }
      // 本 key 的 free / overflow 分配都已完成：trigger 键等于本 key 的页边界。
      output.close_reached_pages(
        mapper,
        &mut ctx.deferred_li,
        &mut ctx.pending,
        page_li,
        key,
        true,
      )?;
    }
  }
  // 剩余 slot 的 key 大于旧页所有 key（尾部插入）。
  for slot in &range_li[slot_idx..] {
    let slot_key = slot.slot.key.as_slice();
    output.close_reached_pages(
      mapper,
      &mut ctx.deferred_li,
      &mut ctx.pending,
      page_li,
      slot_key,
      false,
    )?;
    write_page_slot(ctx, mapper, page_li, slot).await?;
    output.close_reached_pages(
      mapper,
      &mut ctx.deferred_li,
      &mut ctx.pending,
      page_li,
      slot_key,
      true,
    )?;
  }
  // 组内最后一页的 `FinishDataPage`（C++ `ApplyOnePage` 末尾）与 trigger 键没有
  // 事件的页边界：按输出页顺序补关。
  output.close_all_pages(mapper, &mut ctx.deferred_li, &mut ctx.pending, page_li)?;
  Ok(())
}

/// [`write_overflow_slot`] 的写批上下文版本。
async fn write_page_slot<I: PageIo>(
  ctx: &mut ValueCtx<'_, '_, I>,
  mapper: &mut PageMapper,
  page_li: &mut [Vec<u8>],
  slot: &SlotRange<'_>,
) -> Result<()> {
  write_overflow_slot(
    ctx.io,
    ctx.tbl,
    mapper,
    page_li,
    ctx.layout,
    &mut ctx.pending,
    slot,
  )
  .await
}

/// 释放一条旧值占用的物理资源：overflow 页链或 large value segment。
async fn release_old_value<I: PageIo>(
  ctx: &mut ValueCtx<'_, '_, I>,
  mapper: &mut PageMapper,
  iter: &DataPageIter<'_>,
) -> Result<()> {
  if iter.is_overflow() {
    free_overflow_chain(
      ctx.io,
      ctx.tbl,
      mapper,
      iter.value(),
      ctx.layout.data.page_size,
      ctx.layout.data.overflow_pointers,
    )
    .await?;
  } else if iter.is_large_value()
    && let Some(segment_mapper) = ctx.segment_mapper.as_deref_mut()
  {
    fkv_large::free_value(segment_mapper, iter.value(), ctx.segment_size)?;
  }
  Ok(())
}

/// 释放被本批覆盖 / 删除的旧值：overflow 链、大 value 段与旧 TTL 索引项。
async fn release_replaced_value<I: PageIo>(
  ctx: &mut ValueCtx<'_, '_, I>,
  mapper: &mut PageMapper,
  iter: &DataPageIter<'_>,
  batch_entry: &BatchEntry,
) -> Result<()> {
  release_old_value(ctx, mapper, iter).await?;
  // 旧 TTL 索引项：只有 expire_ts 真的变了才删旧项（对齐 C++ `base_expire != expire_ts`）。
  // 相同 expire 的覆盖若不跳过，新批的 `upsert ttl_key(E,k)`（时间戳恒为 0）
  // 会被这里推的 `delete ttl_key(E,k)`（时间戳 > 0）在同 key 折叠里压掉 →
  // TTL 条目消失、到期后 `clean_expired` 扫不到、空间无界泄漏（sto-66）。
  let new_expire = if batch_entry.op.is_delete() {
    0
  } else {
    batch_entry.expire_ts
  };
  if iter.expire_ts() != 0
    && iter.expire_ts() != new_expire
    && let Some(sink) = ctx.ttl_delete_sink.as_deref_mut()
  {
    let key = ttl::ttl_key(iter.expire_ts(), iter.key());
    sink.push(BatchEntry::delete(key, batch_entry.timestamp));
  }
  Ok(())
}

/// 释放一条已过期且未被本批覆盖的旧条目：旧值与 TTL 索引项。
///
/// 对齐 C++ `add_to_page` 的过期分支（`DelOverflowValue` / `DelLargeValue` /
/// `UpdateTTL(Delete)`）：merge 阶段该条目已不进入新页，这里回收它占用的物理
/// 资源并删除 `ttl_key(old_expire, key)`。TTL 索引项版本号与 C++ `UpdateTTL`
/// 一致写 0（引擎自写条目版本恒为 0，同 ts 时删除项在批内折叠中后到胜出）。
async fn release_expired_value<I: PageIo>(
  ctx: &mut ValueCtx<'_, '_, I>,
  mapper: &mut PageMapper,
  iter: &DataPageIter<'_>,
) -> Result<()> {
  release_old_value(ctx, mapper, iter).await?;
  if iter.expire_ts() != 0
    && let Some(sink) = ctx.ttl_delete_sink.as_deref_mut()
  {
    let key = ttl::ttl_key(iter.expire_ts(), iter.key());
    sink.push(BatchEntry::delete(key, 0));
  }
  Ok(())
}

/// 沿 overflow 页链逐页 free，顺序对齐 C++ `BatchWriteTask::DelOverflowValue`：
/// 按每批指针数组下标**前向**遍历，即沿 value 写入 / 分配顺序。
///
/// 一批的最后一槽（下标 `overflow_pointers - 1`）是该批尾页，页尾存下一批
/// 指针：先读尾页取下一批指针，再释放尾页（与 C++ 同序）。`seen` 防止损坏
/// 盘面成环导致死循环。
pub(crate) async fn free_overflow_chain<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  encoded_pointer_li: &[u8],
  page_size: usize,
  overflow_pointers: u8,
) -> Result<()> {
  let mut pointer_li = decode_pointers(encoded_pointer_li)?;
  let group_end = usize::from(overflow_pointers);
  let mut idx = 0usize;
  let mut seen = HashSet::default();
  let mut page = PageBuf::new(page_size);
  while idx < pointer_li.len() {
    let page_id = pointer_li[idx];
    let is_end = idx + 1 == group_end;
    if !seen.insert(page_id) {
      // 重复页（损坏盘面）：普通页跳过、继续，尾页的下一批指针已不可读，终止。
      if is_end {
        break;
      }
      idx += 1;
      continue;
    }
    if is_end {
      let file_page_id = mapper.file_page_id(page_id)?;
      io.read_page_owned(tbl, file_page_id, page.as_mut_vec())
        .await?;
      let next_li = OverflowPage::new(&page)?.pointer_li()?;
      // 先取下一批指针再释放本页（C++ `DelOverflowValue` 同序）。
      mapper.free_page(page_id)?;
      // 下一批首页已访问过：链成环，继续会死循环。
      if next_li.first().is_some_and(|next| seen.contains(next)) {
        break;
      }
      pointer_li = next_li;
      idx = 0;
    } else {
      mapper.free_page(page_id)?;
      idx += 1;
    }
  }
  Ok(())
}

/// 一条待落盘的 overflow slot：目标 slot + 它在 data page 里的 value 区间。
struct SlotRange<'a> {
  slot: &'a OverflowSlot,
  begin: usize,
  end: usize,
}

/// 定位每个 overflow slot 在目标 data page 里的 value 区间（占位指针区）。
///
/// 按 `(page_idx, key)` 排序 slot 下标：同一页只遍历一次，一次定位全部区间。
fn overflow_slot_range_li<'a>(
  page_li: &[Vec<u8>],
  overflow_li: &'a [OverflowSlot],
) -> Result<Vec<SlotRange<'a>>> {
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
  overflow_li
    .iter()
    .zip(range_li)
    .map(|(slot, range)| {
      let (begin, end) = range.ok_or(KvError::Corrupted)?;
      Ok(SlotRange { slot, begin, end })
    })
    .collect()
}

/// 写一条 overflow value 的页链，并把首批指针回填到 data page 的占位区。
async fn write_overflow_slot<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  page_li: &mut [Vec<u8>],
  layout: PageLayout<'_>,
  pending: &mut PendingWrite,
  slot: &SlotRange<'_>,
) -> Result<()> {
  let pointer_bytes =
    build_overflow_chain(io, tbl, mapper, &slot.slot.value, layout, pending).await?;
  let page = page_li
    .get_mut(slot.slot.page_idx)
    .ok_or(KvError::Corrupted)?;
  let dst = page
    .get_mut(slot.begin..slot.end)
    .ok_or(KvError::Corrupted)?;
  if dst.len() != pointer_bytes.len() {
    return Err(KvError::Corrupted);
  }
  dst.copy_from_slice(&pointer_bytes);
  set_checksum(page)?;
  Ok(())
}

/// 单遍定位一页内多个 key 的 value 区间；`key_li` 须按字节升序。
/// 页内不存在的 key 对应位置返回 `None`，由调用方按损坏处理。
pub(crate) fn locate_value_range_li(
  page: &[u8],
  key_li: &[&[u8]],
) -> Result<Vec<Option<(usize, usize)>>> {
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
pub(crate) async fn build_overflow_chain<I: PageIo>(
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
pub(crate) async fn push_overflow_page<I: PageIo>(
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
