use fkv_mapping::{
  AllocMode, AppendAllocator, FilePageAllocator, GenerationSource, MappingTbl, PageMapper,
};
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
  // sem-42：链尾哨兵对齐 C++ `PageMapper::FreePage` —— 写 `InvalidValue(7)`，
  // 而不是 `encode_page_id(FREE_PAGE_NONE)`。
  assert_eq!(mapper.tbl().get(0).unwrap(), INVALID_VALUE);
}

/// sem-42：空闲链逐槽编码对齐 C++ `PageMapper::FreePage`（page_mapper.cpp:377-387）。
#[test]
fn free_chain_tail_uses_cpp_invalid_value() {
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let page_li: Vec<_> = (0..3).map(|_| mapper.allocate_page()).collect();
  assert_eq!(page_li, [0, 1, 2]);
  for &page in &page_li {
    assert!(mapper.free_page(page).unwrap().is_some());
  }
  // 释放顺序 0 → 1 → 2：链头 = 2；尾节点 0 写 `7`，其余写 `encode_page_id(next)`。
  assert_eq!(mapper.tbl().get(0).unwrap(), INVALID_VALUE);
  assert_eq!(mapper.tbl().get(1).unwrap(), encode_page_id(0));
  assert_eq!(mapper.tbl().get(2).unwrap(), encode_page_id(1));
  // 依次弹出：尾哨兵必须被解释成链尾，而不是逻辑页 0。
  assert_eq!(mapper.allocate_page(), 2);
  assert_eq!(mapper.allocate_page(), 1);
  assert_eq!(mapper.allocate_page(), 0);
  assert_eq!(mapper.free_page_cnt(), 0);
  // 链空后走 append，不会因 `7` 被误解为逻辑页 0 而错分页。
  assert_eq!(mapper.allocate_page(), 3);
}

/// sem-45：脏页日志记录**操作时**的值，而不是 flush 时的最终槽值。
///
/// 同一页「释放后复用」时 C++ WAL 在操作处依次追加 `DeleteMapping(7)` 与
/// `UpdateMapping(FilePageId)`；`dirty_li` 必须按同样顺序携带这两个值，不能
/// 等到 flush 再读最终槽值（那样两次都会是 `FilePageId`）。
#[test]
fn dirty_log_records_operation_time_values() {
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let page0 = mapper.allocate_page();
  let page1 = mapper.allocate_page();
  assert_eq!((page0, page1), (0, 1));
  // free → realloc → free → realloc：每次操作各记一条，不去重。
  assert_eq!(mapper.free_page(page0).unwrap(), Some(0));
  assert_eq!(mapper.allocate_page(), page0);
  assert_eq!(mapper.free_page(page0).unwrap(), Some(2));
  assert_eq!(mapper.allocate_page(), page0);
  assert_eq!(
    mapper.dirty_page_li(),
    [
      (page0, encode_file_page_id(0)),
      (page1, encode_file_page_id(1)),
      (page0, INVALID_VALUE),
      (page0, encode_file_page_id(2)),
      (page0, INVALID_VALUE),
      (page0, encode_file_page_id(3)),
    ]
  );
}

/// sem-45：`abort_batch` 回滚 `dirty_li` 时按长度截断，操作时的值一并撤销。
#[test]
fn abort_batch_rolls_back_dirty_log_values() {
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let page0 = mapper.allocate_page();
  assert!(mapper.begin_batch());
  mapper.free_page(page0).unwrap();
  mapper.allocate_page();
  assert_eq!(
    mapper.dirty_page_li(),
    [
      (page0, encode_file_page_id(0)),
      (page0, INVALID_VALUE),
      (page0, encode_file_page_id(1)),
    ]
  );
  assert!(mapper.abort_batch());
  assert_eq!(
    mapper.dirty_page_li(),
    [(page0, encode_file_page_id(0))],
    "回滚只保留批次前的日志条目"
  );
  assert_eq!(mapper.file_page_id(page0).unwrap(), 0);
}

/// sem-42 旧盘面兼容：旧 fkv snapshot 的链尾是 `encode_page_id(FREE_PAGE_NONE)`；
/// reopen 重建必须归一为 `InvalidValue`，且分配顺序不变。
#[test]
fn reopen_normalizes_legacy_and_cpp_free_chain_tails() {
  for (name, tail) in [
    ("legacy_fkv", encode_page_id(FREE_PAGE_NONE)),
    ("cpp", INVALID_VALUE),
  ] {
    let mut tbl = MappingTbl::new();
    tbl.push(tail);
    tbl.push(encode_page_id(0));
    tbl.push(encode_file_page_id(9));
    let mut mapper = PageMapper::with_tbl(tbl, AllocMode::Append, 10);
    assert_eq!(mapper.free_page_cnt(), 2, "case {name}");
    // 两种尾哨兵 reopen 后盘面统一为 C++ 形态。
    assert_eq!(mapper.tbl().get(0).unwrap(), INVALID_VALUE, "case {name}");
    assert_eq!(
      mapper.tbl().get(1).unwrap(),
      encode_page_id(0),
      "case {name}"
    );
    // 分配顺序：1 → 0 → append（槽位 2 是活跃页，不能复用）。
    assert_eq!(mapper.allocate_page(), 1, "case {name}");
    assert_eq!(mapper.allocate_page(), 0, "case {name}");
    assert_eq!(mapper.allocate_page(), 3, "case {name}");
  }
}

/// core-11 / pooled-retire：Pooled 模式下 remap 先分配新页、旧页只 retire，
/// 不原地覆盖；reclaim 之前释放的物理页不会被 `allocate` 取回。
#[test]
fn remap_retires_released_page_without_reuse() {
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  let page0 = mapper.allocate_page();
  let page1 = mapper.allocate_page();
  assert_eq!(mapper.file_page_id(page1).unwrap(), 1);
  // 释放 p0 只 retire，不直接回池。
  let released = mapper.free_page(page0).unwrap().unwrap();
  assert_eq!(released, 0);
  assert_eq!(mapper.retired_len(), 1);
  // COW：新页取新 id，不是刚释放的 0。
  let remapped = mapper.remap_page(page1).unwrap();
  assert_eq!(remapped, 2);
  assert_ne!(remapped, released);
  assert_eq!(mapper.retired_len(), 2);
  // reclaim 之前 allocate 仍不复用 0 / 1。
  let page2 = mapper.allocate_page();
  assert_eq!(mapper.file_page_id(page2).unwrap(), 3);
  // 无快照读者：水位 MAX 回收全部，LIFO 先取回最后 retire 的 1。
  assert_eq!(mapper.reclaim_retired(u64::MAX), 2);
  assert_eq!(mapper.retired_len(), 0);
  let page3 = mapper.allocate_page();
  assert_eq!(mapper.file_page_id(page3).unwrap(), 1);
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

/// r7a-core-01：同一 mapper 内代际单调，abort 继续前进、不复用批次前的取值。
///
/// 代际是索引缓存的键，「同一个数值 ⇒ 同一份映射」必须恒成立：abort 回退到
/// 批次前的取值会让回滚批次用过的数值被下一批重新取到，在途旧快照回填的缓存
/// 节点就能被当成新代条目命中。
#[test]
fn generation_is_monotonic_across_abort() {
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
  // 连续两次变更也必须是两个不同取值（不回绕到旧值）。
  let g2 = mapper.generation();
  mapper.allocate_page();
  assert!(mapper.generation() > g2);
}

/// perf-06：独立 mapper 各持一份代际来源，序列互不影响。
///
/// 两个来源从同一初值开始，各自按自己的变更次数推进；一方分配页不得让另一方
/// 的代际前进（旧实现共享进程级计数器，两个 mapper 的序列会互相穿插）。
#[test]
fn independent_mappers_do_not_share_generation_sequence() {
  let mut a = PageMapper::new(AllocMode::Append, 0);
  let mut b = PageMapper::new(AllocMode::Append, 0);
  assert_eq!(a.generation(), b.generation(), "独立来源从同一初值开始");

  let b_before = b.generation();
  a.allocate_page();
  let a_after = a.generation();
  assert!(a_after > b_before, "同一 mapper 内仍须单调前进");
  assert_eq!(b.generation(), b_before, "A 的变更不得推进 B 的代际");

  b.allocate_page();
  assert_eq!(b.generation(), a_after, "独立序列按各自步数推进");
  a.allocate_page();
  assert_eq!(b.generation(), a_after, "A 再次前进仍不影响 B");
}

/// perf-06：同一 Store 内的 mapper 化身共享来源，重建不得复用旧代际。
///
/// `Store::install_replay_state` 会复用 index cache 并 `retag` 到新 mapper 的
/// 代际；若重建从 0 / 1 重新起算，在途旧快照（旧化身）的代际可能与新化身
/// 相等，旧节点就能命中新读者（r7a-core-02）。
#[test]
fn mapper_incarnations_sharing_source_do_not_reuse_generations() {
  let source = GenerationSource::new();
  let mut old = PageMapper::new_with_source(source.clone(), AllocMode::Append, 0);
  old.allocate_page();
  let old_gen = old.generation();

  let rebuilt =
    PageMapper::with_tbl_and_source(old.tbl().clone(), source.clone(), AllocMode::Append, 0);
  assert!(
    rebuilt.generation() > old_gen,
    "同一 Store 重建的化身不得与旧化身重叠"
  );

  // 共享来源的后续 mapper 继续前进；旧化身不再影响新序列的唯一性。
  let sibling = PageMapper::new_with_source(source, AllocMode::Append, 0);
  assert!(sibling.generation() > rebuilt.generation());
}

/// sem-53：`reserve_page` 只占逻辑槽位，不分配物理页、不追加 mapping 记录；
/// `commit_reserved_page` 到写出相位才分配物理页并追加一条 `UpdateMapping`。
///
/// 对齐 C++ `PageMapper::GetPage`（只取逻辑 id）与 `WriteTask::WritePage` →
/// `AllocatePage` + `UpdateMapping` 的两段式（`page_mapper.cpp:358-374`、
/// `write_task.cpp:238-267`）。
#[test]
fn reserved_page_defers_physical_alloc_and_mapping_record() {
  let mut mapper = PageMapper::new(AllocMode::Append, 100);
  mapper.begin_batch();
  let page_id = mapper.reserve_page();
  assert_eq!(page_id, 0);
  assert_eq!(mapper.page_cnt(), 1);
  assert_eq!(mapper.mapping_count(), 0, "预留槽位不是活跃映射");
  assert!(mapper.dirty_page_li().is_empty(), "预留不追加 mapping 记录");
  assert_eq!(mapper.max_fp_id(), 100, "预留不分配物理页");
  assert!(mapper.file_page_id(page_id).is_err());

  let file_page_id = mapper.commit_reserved_page(page_id).unwrap();
  assert_eq!(file_page_id, 100);
  assert_eq!(mapper.max_fp_id(), 101);
  assert_eq!(mapper.mapping_count(), 1);
  assert_eq!(
    mapper.dirty_page_li(),
    &[(page_id, encode_file_page_id(file_page_id))]
  );
  assert_eq!(mapper.file_page_id(page_id).unwrap(), file_page_id);
  assert!(
    mapper.commit_reserved_page(page_id).is_err(),
    "已提交的槽位不得再次提交"
  );
  mapper.commit_batch();
}

/// sem-53：预留复用空闲链表头时 abort 恢复原链表；批次内追加的预留槽位与
/// 已提交的物理页一起回滚。
#[test]
fn abort_batch_rolls_back_reserved_and_committed_pages() {
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let reused = mapper.allocate_page();
  mapper.free_page(reused).unwrap();
  mapper.clear_dirty_page_li();

  mapper.begin_batch();
  let reserved = mapper.reserve_page();
  assert_eq!(reserved, reused, "预留同样优先复用空闲链表头");
  assert_eq!(mapper.free_page_cnt(), 0);
  let committed = mapper.commit_reserved_page(reserved).unwrap();
  assert_eq!(committed, 1);
  mapper.abort_batch();

  assert_eq!(mapper.page_cnt(), 1, "批次内追加的预留槽位被截断");
  assert_eq!(mapper.mapping_count(), 0);
  assert!(mapper.dirty_page_li().is_empty());
  assert_eq!(mapper.max_fp_id(), 1, "物理高水位回滚");
  assert_eq!(mapper.free_page_cnt(), 1, "复用的空闲槽位回到链表");
  assert_eq!(mapper.allocate_page(), reused, "回滚后仍复用同一空闲页");
}

/// sem-53：Pooled 模式下预留不占池内物理页，提交时才从空闲池弹出。
#[test]
fn reserved_page_does_not_consume_pooled_slot() {
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  let page = mapper.allocate_page();
  mapper.free_page(page).unwrap();
  // 未接线快照代际：watermark = u64::MAX 立即回收。
  assert_eq!(mapper.reclaim_retired(u64::MAX), 1);
  let free_len = mapper.pooled_free_len();
  assert!(free_len > 0);

  mapper.begin_batch();
  let reserved = mapper.reserve_page();
  assert_eq!(mapper.pooled_free_len(), free_len, "预留不占物理页");
  let file_page_id = mapper.commit_reserved_page(reserved).unwrap();
  assert_eq!(mapper.pooled_free_len(), free_len - 1);
  assert_eq!(file_page_id, u64::from(page), "Pooled 取回刚回收的物理页");
  mapper.commit_batch();
}
