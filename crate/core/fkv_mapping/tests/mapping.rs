use fkv_mapping::{AllocMode, AppendAllocator, FilePageAllocator, MappingTbl, PageMapper};
use fkv_types::{
  FREE_PAGE_NONE, INVALID_VALUE, ValType, decode_id, encode_file_page_id, encode_page_id,
  is_file_page_id, val_type,
};

#[test]
fn tags() {
  assert_eq!(val_type(encode_file_page_id(1)), ValType::FilePageId);
  assert_eq!(val_type(encode_page_id(1)), ValType::PageId);
  assert_eq!(val_type(7), ValType::Invalid);
  assert_eq!(decode_id(encode_file_page_id(42)), 42);
  assert!(is_file_page_id(encode_file_page_id(42)));
}

#[test]
fn tbl_roundtrip() {
  let mut tbl = MappingTbl::new();
  tbl.push(encode_file_page_id(1));
  tbl.push(encode_page_id(2));
  tbl.push(7);
  let bytes = tbl.serialize();
  assert_eq!(MappingTbl::deserialize(&bytes).unwrap(), tbl);
}

#[test]
fn append_allocator() {
  let mut allocator = FilePageAllocator::new(AllocMode::Append, 10);
  assert_eq!(allocator.allocate(), 10);
  assert_eq!(allocator.allocate(), 11);
  allocator.free(10);
  assert_eq!(allocator.allocate(), 12);
}

/// sto-56：活跃映射计数只计物理页槽位，free / remap / resize 都要精确跟随。
#[test]
fn mapping_count_tracks_live_slots() {
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  assert_eq!(mapper.mapping_count(), 0);
  let page0 = mapper.allocate_page();
  let page1 = mapper.allocate_page();
  assert_eq!(mapper.mapping_count(), 2);
  mapper.free_page(page0).unwrap();
  assert_eq!(mapper.mapping_count(), 1);
  // remap 是 live -> live，计数不变。
  mapper.remap_page(page1).unwrap();
  assert_eq!(mapper.mapping_count(), 1);

  // 直接写 tbl：live -> free 与 free -> live 双向调整，且序列化后重建一致。
  let mut tbl = MappingTbl::new();
  tbl.push(encode_file_page_id(1));
  tbl.push(INVALID_VALUE);
  tbl.set(1, encode_file_page_id(2));
  assert_eq!(tbl.mapping_count(), 2);
  tbl.set(0, encode_page_id(1));
  assert_eq!(tbl.mapping_count(), 1);
  // 截断掉仅剩的 live 槽位后计数归零。
  tbl.resize(1, INVALID_VALUE);
  assert_eq!(tbl.mapping_count(), 0);
  assert_eq!(
    MappingTbl::deserialize(&tbl.serialize())
      .unwrap()
      .mapping_count(),
    0
  );
}

/// sto-56 / core-26：全空映射跳到文件边界后 `space_size` 归零。
/// 未对齐按 ceil 推进；已对齐保持 `max_fp_id`，只同步 `min_file_id`（对齐 C++）。
#[test]
fn skip_to_next_file_zeroes_space() {
  let mut allocator = AppendAllocator::new(0);
  allocator.skip_to_next_file(2);
  assert_eq!(allocator.max_fp_id(), 0, "空分配器不跳界");
  assert_eq!(allocator.min_file_id(), 0);

  for _ in 0..6 {
    allocator.allocate();
  }
  // shift=2 → 4 页/文件；max=6 在文件 1，上取整到文件 2 起点。
  allocator.skip_to_next_file(2);
  assert_eq!(allocator.max_fp_id(), 8);
  assert_eq!(allocator.min_file_id(), 2);
  assert_eq!(allocator.space_size(2), 0);
  // 幂等：重复调用不再推进。
  allocator.skip_to_next_file(2);
  assert_eq!(allocator.max_fp_id(), 8);
  assert_eq!(allocator.min_file_id(), 2);
  assert_eq!(allocator.space_size(2), 0);
}

/// core-26：`max_fp_id` 已对齐文件边界时不再多烧一整个文件段，
/// 下一次 `allocate` 仍落在当前文件（C++ `UpdateStat` 的 `>` 判定）。
#[test]
fn skip_to_next_file_aligned_keeps_max() {
  let mut allocator = AppendAllocator::new(0);
  for _ in 0..4 {
    allocator.allocate();
  }
  assert_eq!(allocator.max_fp_id(), 4);
  allocator.skip_to_next_file(2);
  assert_eq!(allocator.max_fp_id(), 4, "对齐时不推进");
  assert_eq!(allocator.min_file_id(), 1);
  assert_eq!(allocator.space_size(2), 0);
  // 后续分配复用当前文件的下一页，而不是跳到文件 2。
  assert_eq!(allocator.allocate(), 4);
}

#[test]
fn pooled_allocator_reuses() {
  let mut allocator = FilePageAllocator::new(AllocMode::Pooled, 10);
  assert_eq!(allocator.allocate(), 10);
  assert_eq!(allocator.allocate(), 11);
  allocator.free(10);
  assert_eq!(allocator.allocate(), 10);
}

#[test]
fn mapper_alloc_and_free() {
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  let page = mapper.allocate_page();
  assert_eq!(mapper.file_page_id(page).unwrap(), 0);
  assert_eq!(mapper.free_page(page).unwrap(), Some(0));
  assert_eq!(mapper.free_page_cnt(), 1);
  // 二次释放幂等：不重复入链。
  assert!(mapper.free_page(page).unwrap().is_none());
  assert_eq!(mapper.free_page_cnt(), 1);
  // core-02：空闲逻辑 PageId 优先复用，之后才 append。
  assert_eq!(mapper.allocate_page(), page);
  assert_eq!(mapper.free_page_cnt(), 0);
  assert_eq!(mapper.allocate_page(), 1);
}

/// core-02：循环 alloc/free 不再让 MappingTbl 单调增长（snapshot 体积有界）。
#[test]
fn logical_page_reuse_bounds_tbl_len() {
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  for _ in 0..100 {
    let page = mapper.allocate_page();
    assert_eq!(page, 0);
    assert!(mapper.free_page(page).unwrap().is_some());
  }
  assert_eq!(mapper.tbl().len(), 1);
  assert_eq!(mapper.free_page_cnt(), 1);
  // 链表尾哨兵：槽位存下一个空闲页，尾节点指向 `FREE_PAGE_NONE`。
  assert_eq!(
    decode_id(mapper.tbl().get(0).unwrap()),
    u64::from(FREE_PAGE_NONE)
  );
}

/// core-11：Pooled 模式下 remap 必须先 allocate 再 free，
/// 否则 LIFO 空闲表会把刚释放的旧物理页立刻取回，COW 退化成原地覆盖。
#[test]
fn remap_does_not_reuse_released_page() {
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  let page0 = mapper.allocate_page();
  let page1 = mapper.allocate_page();
  assert_eq!(mapper.file_page_id(page1).unwrap(), 1);
  let released = mapper.free_page(page0).unwrap().unwrap();
  assert_eq!(released, 0);
  let remapped = mapper.remap_page(page1).unwrap();
  assert_eq!(remapped, released);
  assert_ne!(remapped, 1);
}

/// core-12：空文件统计按文件边界扣除，GC 回写后 space_size 单调反映有效空间。
#[test]
fn append_space_size_excludes_empty_files() {
  let mut allocator = AppendAllocator::new(0);
  for _ in 0..10 {
    allocator.allocate();
  }
  assert_eq!(allocator.space_size(2), 10);
  // 最老非空文件前移 1 个文件（4 页）。
  allocator.update_stat(1, 0);
  assert_eq!(allocator.min_file_id(), 1);
  assert_eq!(allocator.space_size(2), 6);
  // 再扣除紧随其后的 1 个空文件。
  allocator.update_stat(1, 1);
  assert_eq!(allocator.empty_file_cnt(), 1);
  assert_eq!(allocator.space_size(2), 2);
  // 边界饱和：统计超出实际分配时返回 0，而不是 panic。
  allocator.update_stat(5, 3);
  assert_eq!(allocator.space_size(2), 0);
}

/// core-02：`with_tbl` 从落盘槽位重建空闲链表（`Invalid` 与 `PageId` 槽位都算空闲）。
#[test]
fn with_tbl_rebuilds_free_list() {
  let mut tbl = MappingTbl::new();
  tbl.push(encode_file_page_id(5));
  tbl.push(INVALID_VALUE);
  tbl.push(encode_file_page_id(7));
  tbl.push(INVALID_VALUE);
  let mut mapper = PageMapper::with_tbl(tbl, AllocMode::Append, 8);
  assert_eq!(mapper.free_page_cnt(), 2);
  // 重建按 PageId 升序串链，链表头是最大空闲页。
  assert_eq!(mapper.allocate_page(), 3);
  assert_eq!(mapper.allocate_page(), 1);
  assert_eq!(mapper.free_page_cnt(), 0);
  // 链表已空：再分配走追加，不复用旧槽位。
  assert_eq!(mapper.allocate_page(), 4);
  // 复用后仍能读到原本活跃的页。
  assert_eq!(mapper.file_page_id(0).unwrap(), 5);
  assert_eq!(mapper.file_page_id(2).unwrap(), 7);
}

/// core-02：`abort_batch` 必须把空闲链表头/计数一并回滚。
#[test]
fn abort_batch_restores_free_list() {
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let p0 = mapper.allocate_page();
  let p1 = mapper.allocate_page();
  let p1_value = mapper.tbl().get(p1).unwrap();

  mapper.begin_batch();
  assert_eq!(mapper.free_page(p1).unwrap(), Some(1));
  assert_eq!(mapper.free_page_cnt(), 1);
  assert_eq!(mapper.allocate_page(), p1);
  mapper.abort_batch();

  assert_eq!(mapper.free_page_cnt(), 0);
  assert_eq!(mapper.tbl().len(), 2);
  assert_eq!(mapper.tbl().get(p1).unwrap(), p1_value);
  assert_eq!(mapper.file_page_id(p1).unwrap(), 1);
  assert_eq!(mapper.file_page_id(p0).unwrap(), 0);
}

/// core-02：Pooled reopen 需把未被引用的物理页填回空闲池。
#[test]
fn pooled_with_tbl_restores_free_file_pages() {
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  mapper.allocate_page();
  let p1 = mapper.allocate_page();
  let p2 = mapper.allocate_page();
  assert_eq!(mapper.free_page(p1).unwrap(), Some(1));
  assert_eq!(mapper.free_page(p2).unwrap(), Some(2));

  let tbl = MappingTbl::deserialize(&mapper.serialize()).unwrap();
  let mut restored = PageMapper::with_tbl(tbl, AllocMode::Pooled, mapper.max_fp_id());
  // 逻辑页与物理页都恢复：p2 是链表头、物理 2 也已回池。
  assert_eq!(restored.free_page_cnt(), 2);
  assert_eq!(restored.allocate_page(), p2);
  assert_eq!(restored.file_page_id(p2).unwrap(), 2);
  assert_eq!(restored.allocate_page(), p1);
  assert_eq!(restored.file_page_id(p1).unwrap(), 1);
}

/// issue 30：MappingTbl 分块（512/slot），clone 共享 chunk、修改 COW。
#[test]
fn chunked_mapping_clone_is_cow_and_resize_works() {
  use fkv_types::INVALID_VALUE;

  let mut tbl = MappingTbl::new();
  for value in 0..2000u64 {
    let page_id = tbl.push(value + 1);
    assert_eq!(page_id, value as u32);
  }
  assert_eq!(tbl.len(), 2000);
  assert_eq!(tbl.get(1999).unwrap(), 2000);
  assert_eq!(tbl.iter_value_li().count(), 2000);

  let snapshot = tbl.clone();
  tbl.set(0, 42);
  assert_eq!(
    snapshot.get(0).unwrap(),
    1,
    "clone must share pre-mutation chunk"
  );
  assert_eq!(tbl.get(0).unwrap(), 42);

  tbl.resize(2100, INVALID_VALUE);
  assert_eq!(tbl.len(), 2100);
  assert_eq!(tbl.get(2099).unwrap(), INVALID_VALUE);

  tbl.resize(10, INVALID_VALUE);
  assert_eq!(tbl.len(), 10);
  assert_eq!(tbl.iter_value_li().count(), 10);
  assert_eq!(tbl.get(0).unwrap(), 42);

  // serialize/deserialize round-trip 保留分块内容。
  let bytes = tbl.serialize();
  let restored = MappingTbl::deserialize(&bytes).unwrap();
  assert_eq!(restored, tbl);
}

/// `deserialize` 的 chunk 边界：空表、满 chunk、跨 chunk、末尾残块都要精确重建。
#[test]
fn deserialize_handles_chunk_boundaries() {
  use fkv_mapping::CHUNK_SIZE;

  assert_eq!(MappingTbl::deserialize(&[]).unwrap(), MappingTbl::new());

  for count in [
    1usize,
    CHUNK_SIZE - 1,
    CHUNK_SIZE,
    CHUNK_SIZE + 1,
    CHUNK_SIZE * 2,
  ] {
    let mut tbl = MappingTbl::new();
    for value in 0..count {
      tbl.push(encode_file_page_id(value as u64 + 1));
    }
    let restored = MappingTbl::deserialize(&tbl.serialize()).unwrap();
    assert_eq!(restored, tbl, "count {count}");
    assert_eq!(restored.len(), count);
  }
}

/// core-12：由活跃槽位反推最老非空文件、区间空文件数与最大 live fp id。
#[test]
fn file_stat_derives_min_holes_and_max_fp() {
  // shift=1 → 2 页/文件。
  let mut tbl = MappingTbl::new();
  tbl.set(0, encode_file_page_id(2)); // 文件 1
  tbl.set(1, encode_file_page_id(3)); // 文件 1
  // 文件 2（fp 4,5）没有活跃页。
  tbl.set(4, encode_file_page_id(6)); // 文件 3
  tbl.set(5, encode_file_page_id(7)); // 文件 3
  assert_eq!(tbl.file_stat(1), (Some(1), 1, Some(7)));

  assert_eq!(MappingTbl::new().file_stat(1), (None, 0, None));
}

/// core-12：扫捕回写后 `space_size` 等于「含活跃页的文件数 × 每文件页数」。
#[test]
fn update_stat_by_scan_sets_space_size() {
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  for _ in 0..6 {
    mapper.allocate_page();
  }
  // 释放中间文件（fp 2,3）的全部页，制造一段空文件。
  assert_eq!(mapper.free_page(2).unwrap(), Some(2));
  assert_eq!(mapper.free_page(3).unwrap(), Some(3));

  mapper.update_stat_by_scan(1);
  assert_eq!(mapper.min_file_id(), 0);
  // live 文件为 0 与 2，共 2 个文件 × 2 页 = 4。
  assert_eq!(mapper.space_size(1), 4);

  // 空表不产生统计，保持 0。
  let mut empty = PageMapper::new(AllocMode::Append, 0);
  empty.update_stat_by_scan(1);
  assert_eq!(empty.min_file_id(), 0);
  assert_eq!(empty.space_size(1), 0);
}

/// core-28：空映射按尾文件起算，`space_size` 只保留尾文件残量。
#[test]
fn update_stat_by_scan_empty_uses_tail_file() {
  // shift=2 → 4 页/文件；max=6 落在文件 1，残量 2。
  let mut mapper = PageMapper::new(AllocMode::Append, 6);
  mapper.update_stat_by_scan(2);
  assert_eq!(mapper.min_file_id(), 1);
  assert_eq!(mapper.space_size(2), 2);

  // 已对齐文件边界时 space_size 为 0（C++ 空 reopen 完全 no-op）。
  let mut aligned = PageMapper::new(AllocMode::Append, 8);
  aligned.update_stat_by_scan(2);
  assert_eq!(aligned.min_file_id(), 2);
  assert_eq!(aligned.space_size(2), 0);

  // 空分配器不回退已有进度。
  let mut fresh = PageMapper::new(AllocMode::Append, 0);
  fresh.update_stat_by_scan(2);
  assert_eq!(fresh.min_file_id(), 0);
  assert_eq!(fresh.space_size(2), 0);
}

/// core-18a：快照释放的 chunk 归还线程池，下一次同尺寸分配全部命中（零真实分配）。
#[test]
fn chunk_buffers_are_recycled_across_tables() {
  use fkv_mapping::{CHUNK_CACHE_MAX, CHUNK_SIZE, chunk_alloc_num};

  // 块数超过池容量，保证池被填满、剩余部分真实释放。
  let total = CHUNK_CACHE_MAX + 16;
  {
    let mut tbl = MappingTbl::new();
    for value in 0..(total * CHUNK_SIZE) as u64 {
      tbl.push(value + 1);
    }
  }
  let after_drop = chunk_alloc_num();
  let mut again = MappingTbl::new();
  for value in 0..(total * CHUNK_SIZE) as u64 {
    again.push(value + 1);
  }
  assert_eq!(
    chunk_alloc_num() - after_drop,
    (total - CHUNK_CACHE_MAX) as u64,
    "回收池内 {CHUNK_CACHE_MAX} 块必须全部命中",
  );
}

/// eng-68：批次回滚必须截断 changed 记录，失败批次不得把变更泄漏到下一批。
#[test]
fn abort_batch_truncates_changed_page_li() {
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let page = mapper.allocate_page();
  assert!(mapper.take_changed_page_li().is_empty());

  mapper.begin_batch();
  mapper.remap_page(page).unwrap();
  assert_eq!(mapper.changed_page_li(), [page]);
  assert!(mapper.abort_batch());
  assert!(
    mapper.changed_page_li().is_empty(),
    "aborted batch must not leak changed pages"
  );

  mapper.remap_page(page).unwrap();
  mapper.clear_changed_page_li();
  assert!(mapper.changed_page_li().is_empty());
}

/// r7a-core-01/02：映射代际进程内单调，abort 与重建都不复用旧值。
///
/// 代际是索引缓存的键，「同一个数值 ⇒ 同一份映射」必须恒成立：abort 回退到
/// 批次前的取值、或重建后从 0 起算，都会让回滚批次用过的数值被下一批重新
/// 取到，在途旧快照回填的缓存节点就能被当成新代条目命中。
#[test]
fn generation_is_monotonic_across_abort_and_rebuild() {
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let g0 = mapper.generation();
  mapper.begin_batch();
  mapper.allocate_page();
  let g1 = mapper.generation();
  assert!(g1 > g0, "映射变更必须取新的代际");
  mapper.abort_batch();
  assert!(
    mapper.generation() > g1,
    "abort 后必须继续前进，不得回退到批次前的取值"
  );

  // 重建（reopen / 新化身）同样取全新代际。
  let rebuilt = PageMapper::with_tbl(mapper.tbl().clone(), AllocMode::Append, 0);
  assert!(
    rebuilt.generation() > mapper.generation(),
    "新化身的代际不得与旧化身重叠"
  );
  // 连续两次变更也必须是两个不同取值（不回绕到旧值）。
  let mut other = PageMapper::new(AllocMode::Append, 0);
  let first = {
    other.allocate_page();
    other.generation()
  };
  other.allocate_page();
  assert!(other.generation() > first);
}
