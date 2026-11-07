//! data 页写入：合并结果落盘、新建 data 页，以及页间 prev/next 链接维护。

use core::mem::take;

use fkv_btree::{
  BatchEntry, MergeOutput, comparator::find_shortest_separator, merge_data_pages_with_options,
};
use fkv_error::{KvError, Result};
use fkv_io::PageIo;
use fkv_mapping::PageMapper;
use fkv_page::{DataPage, DataPageIter, set_checksum, set_next_page_id, set_prev_page_id};
use fkv_task_read::page_buf::PageBuf;
use fkv_types::{FilePageId, NULL_PAGE, PageId, TableIdent};

use crate::{
  batch::{Children, DeferredPage, PageLayout, PendingWrite, ValueCtx},
  overflow::{OutputPageWrite, free_replaced_values, rewrite_page_values},
  rebalance::rebalance_data_page_li,
};

/// 写出延迟队列队首的一页（对齐 C++ `ShiftLeafLink` 的
/// `WritePage(leaf_triple_[0])`）：既有页此刻才 `remap_page` 分配新物理页，本批
/// 新建页（分裂尾页 / 分裂中间页）才 `commit_reserved_page` 分配物理页并追加
/// `UpdateMapping`，随后把页字节放进批内缓冲。
fn flush_deferred_front(
  deferred_li: &mut Vec<DeferredPage>,
  pending: &mut PendingWrite,
  mapper: &mut PageMapper,
) -> Result<()> {
  if deferred_li.is_empty() {
    return Ok(());
  }
  // 队列最多两页（见 `ValueCtx::deferred_li`），`remove(0)` 的搬移是常数级。
  let DeferredPage {
    page_id,
    page,
    reserved,
  } = deferred_li.remove(0);
  let file_page_id = if reserved {
    mapper.commit_reserved_page(page_id)?
  } else {
    mapper.remap_page(page_id)?
  };
  pending.push(file_page_id, page);
  Ok(())
}

/// 按叶链顺序写出全部延迟 data 页：批末两次 `ShiftLeafLink` 的等价物。
pub(crate) fn flush_deferred_all(
  deferred_li: &mut Vec<DeferredPage>,
  pending: &mut PendingWrite,
  mapper: &mut PageMapper,
) -> Result<()> {
  while !deferred_li.is_empty() {
    flush_deferred_front(deferred_li, pending, mapper)?;
  }
  Ok(())
}

/// 处理上一组留下的延迟页（对齐 C++ `LoadApplyingPage`）：只保留队首且正好是
/// 本组前驱的一页——它的 next 修复可能在本组删除路径里继续原地改；队尾的后继页
/// （删除页登记）与更早的页按 `ShiftLeafLink` 顺序先写出。
fn retain_deferred_predecessor<I: PageIo>(
  ctx: &mut ValueCtx<'_, '_, I>,
  mapper: &mut PageMapper,
  prev: PageId,
) -> Result<()> {
  while ctx.deferred_li.len() > 1 {
    flush_deferred_front(&mut ctx.deferred_li, &mut ctx.pending, mapper)?;
  }
  if ctx
    .deferred_li
    .first()
    .is_some_and(|deferred| deferred.page_id != prev)
  {
    flush_deferred_front(&mut ctx.deferred_li, &mut ctx.pending, mapper)?;
  }
  Ok(())
}

/// 一组输出 data 页的写出计划（C++ `FinishDataPage` / `ShiftLeafLink` 相位）。
///
/// 原实现把整组的 overflow 槽位一次性写完，才分配 / remap 全部 data 页，
/// manifest 的 data mapping 段因此是「全部 overflow → 原页 → 分裂页」。C++
/// 在每个输出页写满时 `FinishDataPage`：新页只 `GetPage()` 取逻辑 id，随后
/// `ShiftLeafLink` 把上一页 `WritePage`（物理页 + `UpdateMapping`）。本结构在
/// overflow 槽位按 key 落盘的过程中按输出页边界推进这一相位（sem-54）。
struct OutputPagePlan<'k> {
  /// 第 0 张输出页的既有逻辑页 id；`NULL_PAGE` 表示整组都是新页（新树首写）。
  first_page_id: PageId,
  /// 组首页的原前驱与组尾页的原后继。
  prev: PageId,
  next: PageId,
  /// 每张输出页的边界触发键：第 i 项是第 i+1 张输出页的首键。长度恒为
  /// `page_cnt - 1`；最后一页的 `FinishDataPage` 在组末关闭。
  boundary_key_li: &'k [Vec<u8>],
  /// 每页在父 index 中的子项键。
  child_key: ChildKeySource<'k>,
  /// 已关闭的页边界数 = 已取得逻辑 id 的页数。
  finished: usize,
  /// 最后一张已关闭页的逻辑 id（写下一页的 prev / 组的 `next.prev` 修复用）。
  last_page_id: PageId,
  /// 边关闭边累积的父 index 子项（避免为页 id 单独分配数组：该路径在覆盖写
  /// 热路径上，`tests/alloc.rs` 对小分配计数有回归约束）。
  child_li: Children,
}

/// 输出页在父 index 中的子项键来源。
enum ChildKeySource<'k> {
  /// 未分裂：沿用父 index 原分隔键（只有一张页）。
  Uniform(&'k [u8]),
  /// 分裂 / 新页：每页一个键（分裂页用最短分隔键，新页用真实首键）。
  PerPage(&'k [Vec<u8>]),
}

impl<'k> OutputPagePlan<'k> {
  fn new(
    first_page_id: PageId,
    prev: PageId,
    next: PageId,
    boundary_key_li: &'k [Vec<u8>],
    child_key: ChildKeySource<'k>,
    child_li: Children,
  ) -> Self {
    Self {
      first_page_id,
      prev,
      next,
      boundary_key_li,
      child_key,
      finished: 0,
      last_page_id: NULL_PAGE,
      child_li,
    }
  }

  /// 取走累积的父 index 子项与最后一张页的逻辑 id。
  fn into_children(self) -> (Children, PageId) {
    (self.child_li, self.last_page_id)
  }

  /// 关闭第 `idx` 张输出页（C++ `FinishDataPage` + `ShiftLeafLink`）：
  /// 新页 `reserve_page` 取逻辑 id；上一页在此时才 `remap_page` / 提交物理页并
  /// 追加 `UpdateMapping`；本页排进延迟队列，等下一段 overflow 分配之后的写出
  /// 相位（`leaf_triple_[0]`）。
  fn finish_output_page(
    &mut self,
    mapper: &mut PageMapper,
    deferred_li: &mut Vec<DeferredPage>,
    pending: &mut PendingWrite,
    page_li: &mut [Vec<u8>],
    idx: usize,
  ) -> Result<()> {
    debug_assert_eq!(self.finished, idx, "输出页必须按序关闭");
    let is_new_page = idx > 0 || self.first_page_id == NULL_PAGE;
    let page_id = if is_new_page {
      // C++ `FinishDataPage` 对新页只 `GetPage()` 取逻辑 id，物理页推迟到写出。
      mapper.reserve_page()
    } else {
      self.first_page_id
    };
    // 父 index 子项：`FinishDataPage` 先把 `page_key -> page_id` 追加到栈顶
    // index 页的 changes（`batch_write_task.cpp:1108-1115`）。
    let child_key: &[u8] = match &self.child_key {
      ChildKeySource::Uniform(sep) => sep,
      ChildKeySource::PerPage(li) => li.get(idx).ok_or(KvError::Corrupted)?,
    };
    self.child_li.push(child_key, page_id)?;
    self.finished += 1;
    // C++ `LeafLinkInsert`：`prev_page->SetNextPageId(new_page_id)`；上一页在
    // `ShiftLeafLink` 里写出，此时它的 next 已指向本页。
    if idx > 0 {
      let prev_deferred = deferred_li.first_mut().ok_or(KvError::Corrupted)?;
      debug_assert_eq!(prev_deferred.page_id, self.last_page_id);
      set_next_page_id(&mut prev_deferred.page, page_id)?;
      set_checksum(&mut prev_deferred.page)?;
    }
    flush_deferred_front(deferred_li, pending, mapper)?;
    // 本页进延迟队列：物理页与 `UpdateMapping` 推迟到下一张页的 `FinishDataPage`
    // 或批末 `ShiftLeafLink`（C++ `leaf_triple_[0]`）。
    let mut page = take(page_li.get_mut(idx).ok_or(KvError::Corrupted)?);
    let page_prev = if idx == 0 {
      self.prev
    } else {
      self.last_page_id
    };
    set_prev_page_id(&mut page, page_prev)?;
    set_next_page_id(&mut page, self.next)?;
    set_checksum(&mut page)?;
    deferred_li.push(DeferredPage {
      page_id,
      page,
      reserved: is_new_page,
    });
    self.last_page_id = page_id;
    Ok(())
  }
}

impl OutputPageWrite for OutputPagePlan<'_> {
  fn close_reached_pages(
    &mut self,
    mapper: &mut PageMapper,
    deferred_li: &mut Vec<DeferredPage>,
    pending: &mut PendingWrite,
    page_li: &mut [Vec<u8>],
    key: &[u8],
    inclusive: bool,
  ) -> Result<()> {
    loop {
      let reached = {
        let Some(trigger) = self.boundary_key_li.get(self.finished) else {
          break;
        };
        let trigger = trigger.as_slice();
        if inclusive {
          trigger <= key
        } else {
          trigger < key
        }
      };
      if !reached {
        break;
      }
      let idx = self.finished;
      self.finish_output_page(mapper, deferred_li, pending, page_li, idx)?;
    }
    Ok(())
  }

  fn close_all_pages(
    &mut self,
    mapper: &mut PageMapper,
    deferred_li: &mut Vec<DeferredPage>,
    pending: &mut PendingWrite,
    page_li: &mut [Vec<u8>],
  ) -> Result<()> {
    // 边界触发键表只覆盖前 `page_cnt - 1` 张页；最后一张页（`finished` 自增到
    // `page_li.len()`）也在组末关闭（C++ `ApplyOnePage` 末尾的 `FinishDataPage`）。
    while self.finished < page_li.len() {
      let idx = self.finished;
      self.finish_output_page(mapper, deferred_li, pending, page_li, idx)?;
    }
    Ok(())
  }
}

/// 每张输出页的首键（C++ `FinishDataPage` 的触发键：下一张页的首键即当前页的
/// `FinishDataPage` 触发点）。页非空由 merge 保证。
fn output_page_first_key_li(page_li: &[Vec<u8>]) -> Result<Vec<Vec<u8>>> {
  let mut out = Vec::with_capacity(page_li.len());
  for page in page_li {
    let mut iter = DataPageIter::new(page)?;
    if !iter.advance()? {
      return Err(KvError::Corrupted);
    }
    out.push(iter.key().to_vec());
  }
  Ok(out)
}

/// 重写一个 data 页；返回该页及其分裂页在父 index 中的子项。
///
/// `sep` 为该 data 页在父 index 中原有的分隔键：首段沿用它（对齐 C++
/// `BatchWriteTask::FinishDataPage` 里 `idx_page_iter_.Key()`），既不抬高分隔键
/// 影响路由，也让未分裂的原位更新完全不必改父 index（issue 49）。
pub(crate) async fn rewrite_data_page<I: PageIo>(
  ctx: &mut ValueCtx<'_, '_, I>,
  mapper: &mut PageMapper,
  page_id: PageId,
  old_page: &[u8],
  batch_li: &[&BatchEntry],
  sep: &[u8],
) -> Result<Children> {
  let layout = ctx.layout;
  let old_data = DataPage::new(old_page)?;
  // 前驱页本批分裂 / 被删除时已登记新的 prev（`update_prev` 只记 pending）：
  // 本页稍后自身重写，指针在这里一并写出，整批只 remap 一次（sem-51）。
  // `deferred_prev` 只可能属于 `next_planned_page`，即本组。
  let prev = match ctx.deferred_prev {
    Some((target, prev_page)) if target == page_id => {
      ctx.deferred_prev = None;
      prev_page
    }
    _ => old_data.prev_page_id()?,
  };
  let next = old_data.next_page_id()?;
  // 上一组留下的延迟页：只让正好是本组前驱的一页继续留在队列里（本组若删除
  // 自己，前驱的 next 修复要原地并入）；其余按 C++ `LoadApplyingPage` 相位先写出。
  retain_deferred_predecessor(ctx, mapper, prev)?;
  // 旧版本已由调用方读入 `old_page`：丢弃缓冲里的同名项，避免向已释放的
  // `FilePageId` 重复写（perf-82 延迟写）。
  drop(ctx.pending.take(mapper.file_page_id(page_id)?));
  let MergeOutput {
    mut page_li,
    mut overflow_li,
  } = merge_data_pages_with_options(&[old_page], batch_li, layout.data)?;
  // 对齐 C++ `FinishDataPage` 的四分页重分配：新页 < 1/4 且前页 > 3/4 时，
  // 在两页间重新分配 entry，避免产生极小尾页（不再全树重建）。
  // 未分裂时不分配标记数组（绝大多数覆盖写）。
  let mut redistributed_li = Vec::new();
  rebalance_data_page_li(
    &mut page_li,
    layout.data.page_size,
    layout.data.restart_interval,
    &mut overflow_li,
    &mut redistributed_li,
  )?;
  // 被本批 delete / 覆盖的 overflow 页链与大 value 段：按 key 升序与新区块的
  // 分配交错执行（对齐 C++ `ApplyOnePage` 的逐 key free→alloc，sem-48）。
  // 整页条目全部删除时没有新 overflow 链，只释放旧值。
  if page_li.is_empty() {
    free_replaced_values(ctx, mapper, old_page, batch_li).await?;
    mapper.free_page(page_id)?;
    if prev != NULL_PAGE {
      update_next(ctx, mapper, prev, next).await?;
    }
    if next != NULL_PAGE {
      update_prev(ctx, mapper, next, prev).await?;
    }
    return Ok(Children::default());
  }
  // 逐 key 先释放旧值、再落新 overflow 页并回填首批指针到 data page 占位区；
  // 同时按输出页边界推进 C++ `FinishDataPage` 的页级相位（sem-54）：每张页
  // 写满时才 `GetPage` 取逻辑 id / 写出上一页，其间的 overflow 分配即时发生。
  let split = page_li.len() > 1;
  let (sep_li, first_key_li) = if split {
    (
      page_separator_li(&page_li, sep, &redistributed_li)?,
      output_page_first_key_li(&page_li)?,
    )
  } else {
    (Vec::new(), Vec::new())
  };
  // 第 i+1 张输出页的首键即第 i 张页的 `FinishDataPage` 触发键。
  let (boundary_key_li, child_key, child_li) = if split {
    (
      first_key_li.get(1..).unwrap_or_default(),
      ChildKeySource::PerPage(&sep_li),
      Children::with_capacity(sep_li.len(), sep_li.iter().map(Vec::len).sum()),
    )
  } else {
    (
      &[] as &[Vec<u8>],
      ChildKeySource::Uniform(sep),
      Children::with_capacity(1, sep.len()),
    )
  };
  let mut plan = OutputPagePlan::new(page_id, prev, next, boundary_key_li, child_key, child_li);
  rewrite_page_values(
    ctx,
    mapper,
    old_page,
    batch_li,
    &mut page_li,
    &overflow_li,
    &mut plan,
  )
  .await?;
  // 组内最后一页仍在延迟队列（C++ `leaf_triple_[0]`）：由下一组 / 批末写出。
  let (child_li, last_page_id) = plan.into_children();
  debug_assert_eq!(child_li.len(), page_li.len(), "每张输出页都须取得 id");
  // idx 0 复用原 `page_id`，`prev.next` 不变；未分裂时 `next.prev` 也不变。
  // 仅分裂（页数 > 1）时才需要改 `next.prev` 指向新的尾页（issue 29 写放大）。
  if split && next != NULL_PAGE {
    update_prev(ctx, mapper, next, last_page_id).await?;
  }
  Ok(child_li)
}

/// 新树首写：merge 出一组全新 data 页并按 C++ `FinishDataPage` 的输出页边界
/// 交错 overflow 分配与 data 页 `GetPage` / `WritePage`（sem-54）。
///
/// 保留独立入参（不接收 [`ValueCtx`]）供 `fkv_task_write` 的单元测试与
/// `write_batch` 共用：内部借用一份临时 `ValueCtx`，返回前把批内缓冲写回
/// `pending`（segment / TTL 出口在首写路径为空）。
pub(crate) async fn write_new_data_page<'a, I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  batch_li: &[&BatchEntry],
  layout: PageLayout<'a>,
  pending: &mut PendingWrite,
) -> Result<Children> {
  let mut ctx = ValueCtx {
    io,
    tbl,
    layout,
    segment_mapper: None,
    segment_size: 0,
    ttl_delete_sink: None,
    pending: take(pending),
    next_planned_page: NULL_PAGE,
    deferred_prev: None,
    deferred_li: Vec::new(),
  };
  let result = write_new_data_page_inner(&mut ctx, mapper, batch_li).await;
  *pending = take(&mut ctx.pending);
  result
}

async fn write_new_data_page_inner<I: PageIo>(
  ctx: &mut ValueCtx<'_, '_, I>,
  mapper: &mut PageMapper,
  batch_li: &[&BatchEntry],
) -> Result<Children> {
  let layout = ctx.layout;
  let MergeOutput {
    mut page_li,
    mut overflow_li,
  } = merge_data_pages_with_options(&[], batch_li, layout.data)?;
  // 空表首写同样做四分页重分配（对齐 C++ `FinishDataPage` 对新页的处理），
  // 否则尾页可稀疏到 1 条。分隔键由调用者按真实首键生成，无需此处的重分配标记。
  let mut redistributed_li = Vec::new();
  rebalance_data_page_li(
    &mut page_li,
    layout.data.page_size,
    layout.data.restart_interval,
    &mut overflow_li,
    &mut redistributed_li,
  )?;
  let first_key_li = output_page_first_key_li(&page_li)?;
  // C++ 新页路径的页边界同样由下一张页的首键触发：`FinishDataPage(P_i)` 先为
  // P_i `GetPage()` 取逻辑 id（此时 P_{i+1} 首个 overflow 已分配），再写出
  // P_{i-1}；组内最后一页在批末 `ShiftLeafLink` 写出（先于 index 页）。
  let mut plan = OutputPagePlan::new(
    NULL_PAGE,
    NULL_PAGE,
    NULL_PAGE,
    first_key_li.get(1..).unwrap_or_default(),
    ChildKeySource::PerPage(&first_key_li),
    Children::with_capacity(first_key_li.len(), first_key_li.len() * 16),
  );
  rewrite_page_values(
    ctx,
    mapper,
    &[],
    batch_li,
    &mut page_li,
    &overflow_li,
    &mut plan,
  )
  .await?;
  // 批末两次 `ShiftLeafLink`：最后一张 data 页在这里写出，早于 index 页。
  flush_deferred_all(&mut ctx.deferred_li, &mut ctx.pending, mapper)?;
  let (child_li, _) = plan.into_children();
  debug_assert_eq!(child_li.len(), first_key_li.len());
  // 大批量首写可能产生大量新页：按预算分批落盘，内存有界（perf-82）。
  if ctx.pending.over_budget() {
    let io = ctx.io;
    let tbl = ctx.tbl;
    ctx.pending.flush(io, tbl).await?;
  }
  Ok(child_li)
}

/// 取该页在当前物理页上的最新版本：批内缓冲命中时直接取走，否则读盘
/// （本页随后要改写，池缓冲转为 owned 输出页）。
async fn take_or_read_page<I: PageIo>(
  io: &I,
  tbl: &TableIdent,
  pending: &mut PendingWrite,
  file_page_id: FilePageId,
  page_size: usize,
) -> Result<Vec<u8>> {
  if let Some(page) = pending.take(file_page_id) {
    return Ok(page);
  }
  let mut page = PageBuf::new(page_size);
  io.read_page_owned(tbl, file_page_id, page.as_mut_vec())
    .await?;
  Ok(page.into_vec())
}

/// 改 `page_id` 的 next 指针（某页被删除时的前驱修复）。
pub(crate) async fn update_next<I: PageIo>(
  ctx: &mut ValueCtx<'_, '_, I>,
  mapper: &mut PageMapper,
  page_id: PageId,
  next: PageId,
) -> Result<()> {
  // 该页还在延迟队列里（上一组刚改写 / 上一删除页的前驱 / 分裂尾页）：原地改
  // 指针，让它与删除页的 `DeleteMapping` 保持 C++ 的先后（sem-52、sem-53）。
  if let Some(deferred) = ctx
    .deferred_li
    .iter_mut()
    .find(|deferred| deferred.page_id == page_id)
  {
    set_next_page_id(&mut deferred.page, next)?;
    set_checksum(&mut deferred.page)?;
    return Ok(());
  }
  let io = ctx.io;
  let tbl = ctx.tbl;
  let page_size = ctx.layout.data.page_size;
  let file_page_id = mapper.file_page_id(page_id)?;
  let mut page = take_or_read_page(io, tbl, &mut ctx.pending, file_page_id, page_size).await?;
  set_next_page_id(&mut page, next)?;
  set_checksum(&mut page)?;
  // 本批尚未写过的前驱页：只改内存页，留到写出相位统一 remap
  // （C++ `LeafLinkDelete` 把它载入 `leaf_triple_[0]`）。
  ctx.deferred_li.push(DeferredPage {
    page_id,
    page,
    reserved: false,
  });
  Ok(())
}

pub(crate) async fn update_prev<I: PageIo>(
  ctx: &mut ValueCtx<'_, '_, I>,
  mapper: &mut PageMapper,
  page_id: PageId,
  prev: PageId,
) -> Result<()> {
  // 后继页本批自身也会被重写（它就是下一组）：只登记 prev 覆盖，由那次写出
  // 统一落盘，保证同一逻辑页一批内最多 remap 一次（sem-51）。
  if page_id == ctx.next_planned_page {
    ctx.deferred_prev = Some((page_id, prev));
    return Ok(());
  }
  // 已在延迟队列：原地改指针（与改写页保持同一写出相位）。
  if let Some(deferred) = ctx
    .deferred_li
    .iter_mut()
    .find(|deferred| deferred.page_id == page_id)
  {
    set_prev_page_id(&mut deferred.page, prev)?;
    set_checksum(&mut deferred.page)?;
    return Ok(());
  }
  let io = ctx.io;
  let tbl = ctx.tbl;
  let page_size = ctx.layout.data.page_size;
  let file_page_id = mapper.file_page_id(page_id)?;
  let mut page = take_or_read_page(io, tbl, &mut ctx.pending, file_page_id, page_size).await?;
  set_prev_page_id(&mut page, prev)?;
  set_checksum(&mut page)?;
  // 与改写页同相位写出（C++ `LeafLinkDelete` 把后继页放进 `leaf_triple_[2]`，
  // 批末 `ShiftLeafLink` 才写出），不得抢在删除页 `DeleteMapping` 之前（sem-52）。
  ctx.deferred_li.push(DeferredPage {
    page_id,
    page,
    reserved: false,
  });
  Ok(())
}

/// 分裂后每页在父 index 中的分隔键：首段沿用父 index 原分隔键，其余段取
/// `find_shortest_separator(前页末键, 本页首键)`（对齐 C++ `FindShortestSeparator`）。
/// `redistributed_li[idx]` 为真表示该页由四分页重分配新生成，其首键已改变，
/// 分隔键须改用真实首键——对齐 C++ `FinishDataPage` 的 `page_key.assign(iter.Key())`。
pub(crate) fn page_separator_li(
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
      out.push(find_shortest_separator(&prev_last, iter.key()).to_vec());
    }
  }
  Ok(out)
}

/// data 页的末条 key；空页返回 `Corrupted`。
pub(crate) fn last_data_key(page: &[u8]) -> Result<Vec<u8>> {
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
