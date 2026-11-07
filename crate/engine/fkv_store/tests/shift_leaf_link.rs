use core::iter::once;

use fkv_btree::BatchEntry;
use fkv_codec::get_varint64;
use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_page::{DataPage, DataPageIter, IndexPageIter, PageType, page_type};
use fkv_store::Store;
use fkv_types::{INVALID_VALUE, TableIdent, encode_file_page_id};

mod common;

use common::{log_mapping_entry_li, page_bytes, read_overflow_chain, snapshot_mapping};

/// sem-49：既有 data 页被写批撑裂时，data mapping 的写出顺序与物理页归属必须
/// 与 C++ 一致 —— 原页先 `remap`，新分裂页后 `allocate`。
///
/// C++ `BatchWriteTask::FinishDataPage`（`batch_write_task.cpp:1064-1116`）把既有
/// 页放入 `leaf_triple_`，由 `ShiftLeafLink`（`:122-131`）按页从左到右
/// `WritePage`；`WritePage` 内的 `AllocatePage` 才分配物理页并
/// `wal_builder_.UpdateMapping`（`write_task.cpp:238-267`、`:551-621`）。因此
/// log 里原页的 mapping 先于新页，原页也先拿到较小的 `FilePageId`。
///
/// 探针（512 B 页、3 + 7 条 64 B value）：首批 data 页 0、index root 1；第二批
/// 把 data 页 0 撑裂成页 0 + 新页 2。旧实现先 `allocate_page()` 分裂页、再在
/// 构造 `child_li` 时 `remap_page()` 原页，delta 为 `[(2,17),(0,25),(1,33)]`；
/// C++ 的从左到右顺序为 `[(0,17),(2,25),(1,33)]`。
#[test]
fn split_existing_data_page_writes_original_before_split_pages() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem49_split_order", 1).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    let first: Vec<BatchEntry> = (0..3)
      .map(|i| BatchEntry::upsert(format!("k{i}").into_bytes(), vec![b'a'; 64], 1))
      .collect();
    store.put(&tbl, &first).await.unwrap();
    // 首批只产生 data 页 0 与 index root 1。
    assert_eq!(store.mapper(&tbl).unwrap().tbl().len(), 2);
    assert_eq!(store.root_id(&tbl), Some(1));

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    // 7 条新键全部排在前 3 条之后：合并后 data 页 0 分裂，新页拿到逻辑 id 2。
    let second: Vec<BatchEntry> = (0..7)
      .map(|i| BatchEntry::upsert(format!("k{i}x").into_bytes(), vec![b'b'; 64], 2))
      .collect();
    store.put(&tbl, &second).await.unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after.get(before.len()..).expect("增量 record");
    let entry_li = log_mapping_entry_li(record);

    assert_eq!(store.root_id(&tbl), Some(1), "index root 逻辑页不变");
    assert_eq!(
      store.mapper(&tbl).unwrap().tbl().len(),
      3,
      "原 data 页 + 新分裂页 + index root"
    );
    // C++：原页 remap → 新分裂页 allocate → index root remap。
    assert_eq!(
      entry_li,
      vec![(0, 17), (2, 25), (1, 33)],
      "data 页 mapping 必须按从左到右写出：原页先于分裂页"
    );
    // 物理页归属：原页拿到本批第一个 FilePageId(2)，新页其次(3)，root 最后(4)。
    let mapper = store.mapper(&tbl).unwrap();
    assert_eq!(mapper.file_page_id(0).unwrap(), 2);
    assert_eq!(mapper.file_page_id(2).unwrap(), 3);
    assert_eq!(mapper.file_page_id(1).unwrap(), 4);
  });
}

/// sem-51：既有 data 页分裂、且其紧跟的后继 data 页也在同一批被改写时，
/// 后继页的 prev 指针修复必须并入它自己的重写，一批只 remap 一次。
///
/// 探针（512 B 页、3 + 7 + 5 条 64 B value）：
/// 1. 首批 `k0`..`k2`：data 页 0 + index root 1；
/// 2. 第二批 `k0x`..`k6x`：data 页 0 分裂出页 2，delta `[(0,17),(2,25),(1,33)]`；
/// 3. 第三批插入 `k0a`..`k0d` 并覆盖 `k5x`：页 0 再分裂出页 3（后继页 = 2），
///    页 2 自身也被改写。
///
/// C++ `LeafLinkInsert`（`batch_write_task.cpp:140-167`）把后继页放进
/// `leaf_triple_[2]`，`LoadApplyingPage`（`:360-397`）快路径直接复用内存页，
/// `ShiftLeafLink`（`:122-131`）按页从左到右写出，故页 2 只 `WritePage` 一次。
/// 本批分配顺序为页 0 → 页 3 → 页 2 → index root：
/// `[(0,41),(3,49),(2,57),(1,65)]`。
///
/// 旧实现 `update_prev` 先为页 2 单独 remap 一次（57），页 2 自身重写再 remap
/// 一次（65），delta 为 `[(0,41),(3,49),(2,57),(2,65),(1,73)]`（issue sem-51）。
#[test]
fn split_then_successor_rewrite_remaps_each_page_once() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem51_successor", 1).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    let first: Vec<BatchEntry> = (0..3)
      .map(|i| BatchEntry::upsert(format!("k{i}").into_bytes(), vec![b'a'; 64], 1))
      .collect();
    store.put(&tbl, &first).await.unwrap();
    assert_eq!(store.mapper(&tbl).unwrap().tbl().len(), 2);

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let second: Vec<BatchEntry> = (0..7)
      .map(|i| BatchEntry::upsert(format!("k{i}x").into_bytes(), vec![b'b'; 64], 2))
      .collect();
    store.put(&tbl, &second).await.unwrap();
    let after_second = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after_second.get(before.len()..).expect("第二批增量 record");
    assert_eq!(
      log_mapping_entry_li(record),
      vec![(0, 17), (2, 25), (1, 33)],
      "第二批：页 0 分裂出页 2"
    );

    // 第三批：k0a..k0d 落在页 0，k5x 覆盖落在页 2。
    let mut third: Vec<BatchEntry> = (0..4)
      .map(|i| {
        BatchEntry::upsert(
          format!("k0{}", (b'a' + i) as char).into_bytes(),
          vec![b'c'; 64],
          3,
        )
      })
      .collect();
    third.push(BatchEntry::upsert(b"k5x".to_vec(), vec![b'd'; 64], 3));
    store.put(&tbl, &third).await.unwrap();
    let after_third = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after_third
      .get(after_second.len()..)
      .expect("第三批增量 record");
    let entry_li = log_mapping_entry_li(record);

    assert_eq!(
      store.mapper(&tbl).unwrap().tbl().len(),
      4,
      "原页 0 + 分裂页 3 + 页 2 + index root 1：不得多分配物理页"
    );
    assert_eq!(
      entry_li,
      vec![(0, 41), (3, 49), (2, 57), (1, 65)],
      "页 2 只能 remap 一次：prev 修复并入自身重写，不得单独写一条"
    );
    let mapper = store.mapper(&tbl).unwrap();
    assert_eq!(mapper.file_page_id(0).unwrap(), 5);
    assert_eq!(mapper.file_page_id(3).unwrap(), 6);
    assert_eq!(mapper.file_page_id(2).unwrap(), 7);
    assert_eq!(mapper.file_page_id(1).unwrap(), 8);

    // 可见数据与页链一致：页链 0 -> 3 -> 2，页 2 的 prev 指向页 3。
    let page0 = page_bytes(&store, &tbl, 0).await;
    let page3 = page_bytes(&store, &tbl, 3).await;
    let page2 = page_bytes(&store, &tbl, 2).await;
    assert_eq!(DataPage::new(&page0).unwrap().next_page_id().unwrap(), 3);
    assert_eq!(DataPage::new(&page3).unwrap().prev_page_id().unwrap(), 0);
    assert_eq!(DataPage::new(&page3).unwrap().next_page_id().unwrap(), 2);
    assert_eq!(DataPage::new(&page2).unwrap().prev_page_id().unwrap(), 3);
    assert_eq!(store.get(&tbl, b"k5x").await.unwrap(), Some(vec![b'd'; 64]));
    assert_eq!(store.get(&tbl, b"k0c").await.unwrap(), Some(vec![b'c'; 64]));
  });
}

/// sem-51：一批内多张 data 页连续分裂（分裂页的后继页也在本批被改写 / 分裂），
/// 每个逻辑页仍只写一次，data mapping 段按最终叶链从左到右排列。
///
/// 探针（512 B 页）：
/// 1. 首批 `a`..`o`（64 B value）→ data 页 0/1/2 + index root 3；
/// 2. 第二批在页 0 插入 `a0..a2`、页 1 插入 `g0..g2`、覆盖页 2 的 `m`：
///    页 0、页 1 各分裂出一页，页 2 只改写。
///
/// C++ `LeafLinkInsert`（`batch_write_task.cpp:140-167`）把每个后继页留在
/// `leaf_triple_`，`ShiftLeafLink`（`:122-131`）按最终叶链从左到右写出，故
/// 分配顺序 = 叶链顺序：页 0 → 新页 4 → 页 1 → 新页 5 → 页 2 → index root。
/// 旧实现会在页 0 分裂时先单独 remap 页 1、页 1 分裂时再单独 remap 页 2，
/// 页 1 / 页 2 各出现两条 mapping 记录。
#[test]
fn multiple_splits_in_one_batch_remap_each_page_once() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem51_multi_split", 1).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    let first: Vec<BatchEntry> = (0..15u8)
      .map(|i| BatchEntry::upsert(vec![b'a' + i], vec![b'a'; 64], 1))
      .collect();
    store.put(&tbl, &first).await.unwrap();
    assert_eq!(store.mapper(&tbl).unwrap().tbl().len(), 4);
    assert_eq!(store.root_id(&tbl), Some(3));

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let mut second: Vec<BatchEntry> = Vec::new();
    for i in 0..3u8 {
      second.push(BatchEntry::upsert(vec![b'a', b'0' + i], vec![b'c'; 64], 2));
    }
    for i in 0..3u8 {
      second.push(BatchEntry::upsert(vec![b'g', b'0' + i], vec![b'c'; 64], 2));
    }
    second.push(BatchEntry::upsert(b"m".to_vec(), vec![b'd'; 64], 2));
    store.put(&tbl, &second).await.unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after.get(before.len()..).expect("增量 record");
    let entry_li = log_mapping_entry_li(record);

    assert_eq!(
      entry_li,
      vec![(0, 33), (4, 41), (1, 49), (5, 57), (2, 65), (3, 73)],
      "每个逻辑页只出现一次，顺序 = 最终叶链从左到右"
    );
    // 最终叶链 0 -> 4 -> 1 -> 5 -> 2，与 data mapping 段的 page_id 顺序一致。
    let mut leaf_li = Vec::new();
    let mut page_id = 0u32;
    loop {
      leaf_li.push(page_id);
      let page = page_bytes(&store, &tbl, page_id).await;
      let next = DataPage::new(&page).unwrap().next_page_id().unwrap();
      if next == fkv_types::NULL_PAGE {
        break;
      }
      page_id = next;
    }
    assert_eq!(leaf_li, vec![0, 4, 1, 5, 2]);
    assert_eq!(
      entry_li
        .iter()
        .map(|&(page_id, _)| page_id)
        .collect::<Vec<u32>>(),
      leaf_li.iter().copied().chain(once(3)).collect::<Vec<u32>>(),
      "data mapping 段 = 叶链顺序 + 末尾 index root"
    );
    // 物理页按同一顺序递增归属。
    let mapper = store.mapper(&tbl).unwrap();
    for (idx, page_id) in leaf_li.iter().enumerate() {
      assert_eq!(mapper.file_page_id(*page_id).unwrap(), 4 + idx as u64);
    }
    assert_eq!(mapper.file_page_id(3).unwrap(), 9);
    assert_eq!(store.get(&tbl, b"m").await.unwrap(), Some(vec![b'd'; 64]));
  });
}

/// sem-51：链首 / 链尾分裂同批发生。链首页分裂时后继页不在本批，
/// `update_prev` 仍按原有路径单独 remap 一次（C++ `LeafLinkInsert` 把后继页
/// 放进 `leaf_triple_[2]`，在下一次 `ShiftLeafLink` 写出）；链尾页分裂没有后继，
/// 不得多写任何页。
///
/// 探针（512 B 页）：首批 `a`..`o` → 页 0/1/2 + root 3；第二批在页 0 插入
/// `a0..a2`、在尾页 2 插入 `o0..o4`。预期分配顺序：
/// 页 0 → 新页 4 → 页 1（prev 修复）→ 页 2 → 新页 5 → root。
#[test]
fn head_and_tail_splits_in_one_batch_write_each_page_once() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem51_head_tail", 1).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    let first: Vec<BatchEntry> = (0..15u8)
      .map(|i| BatchEntry::upsert(vec![b'a' + i], vec![b'a'; 64], 1))
      .collect();
    store.put(&tbl, &first).await.unwrap();

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let mut second: Vec<BatchEntry> = Vec::new();
    for i in 0..3u8 {
      second.push(BatchEntry::upsert(vec![b'a', b'0' + i], vec![b'c'; 64], 2));
    }
    for i in 0..5u8 {
      second.push(BatchEntry::upsert(vec![b'o', b'0' + i], vec![b'e'; 64], 2));
    }
    store.put(&tbl, &second).await.unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after.get(before.len()..).expect("增量 record");
    let entry_li = log_mapping_entry_li(record);

    assert_eq!(
      entry_li,
      vec![(0, 33), (4, 41), (1, 49), (2, 57), (5, 65), (3, 73)],
      "链首分裂：原页 → 新页 → 后继页；链尾分裂：原页 → 新页；每个页只写一次"
    );
    // 页链 0 -> 4 -> 1 -> 2 -> 5，页 1 的 prev 由链首分裂的 update_prev 更新。
    let page1 = page_bytes(&store, &tbl, 1).await;
    assert_eq!(DataPage::new(&page1).unwrap().prev_page_id().unwrap(), 4);
    let page5 = page_bytes(&store, &tbl, 5).await;
    assert_eq!(DataPage::new(&page5).unwrap().prev_page_id().unwrap(), 2);
    assert_eq!(
      DataPage::new(&page5).unwrap().next_page_id().unwrap(),
      fkv_types::NULL_PAGE
    );
    assert_eq!(store.get(&tbl, b"o4").await.unwrap(), Some(vec![b'e'; 64]));
  });
}

/// sem-51：邻页在本批被整体删除时，其前驱页的 `next` 指针修复必须原地并入
/// 前驱页自身的写出，不能再分配一次物理页。
///
/// 探针（512 B 页）：首批 `a`..`o` → 页 0/1/2 + root 3；第二批覆盖页 0 的 `a`
/// 并删除页 1 的全部键 `g..l`。旧实现 `update_next` 会为页 0 再 remap 一次，
/// data mapping 段出现两条页 0 记录。sem-52 之后前驱页的写出也推迟到
/// `DeleteMapping` 之后，整段序列与 C++ `leaf_triple_` 一致。
#[test]
fn rewritten_predecessor_of_deleted_page_is_not_remapped_twice() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem51_delete_neighbor", 1).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    let first: Vec<BatchEntry> = (0..15u8)
      .map(|i| BatchEntry::upsert(vec![b'a' + i], vec![b'a'; 64], 1))
      .collect();
    store.put(&tbl, &first).await.unwrap();
    assert_eq!(store.root_id(&tbl), Some(3));

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let mut second: Vec<BatchEntry> = vec![BatchEntry::upsert(b"a".to_vec(), vec![b'z'; 64], 2)];
    for key in [b"g", b"h", b"i", b"j", b"k", b"l"] {
      second.push(BatchEntry::delete(key.to_vec(), 2));
    }
    store.put(&tbl, &second).await.unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after.get(before.len()..).expect("增量 record");
    let entry_li = log_mapping_entry_li(record);

    // 每个逻辑页最多一条 mapping 记录；顺序对齐 C++：删除页 → 前驱页 → 后继页。
    assert_eq!(
      entry_li,
      vec![(1, INVALID_VALUE), (0, 33), (2, 41), (3, 49)],
      "同一逻辑页不得 remap 两次，且前驱页写出在 DeleteMapping 之后"
    );

    // 页链 0 -> 2，页 0 的 next 已指向页 2。
    let page0 = page_bytes(&store, &tbl, 0).await;
    assert_eq!(DataPage::new(&page0).unwrap().next_page_id().unwrap(), 2);
    let page2 = page_bytes(&store, &tbl, 2).await;
    assert_eq!(DataPage::new(&page2).unwrap().prev_page_id().unwrap(), 0);
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 9);
  });
}

/// sem-51：链首 data 页在本批被整体删除、其后继页也在本批被改写时，后继页的
/// prev 修复同样并入自身重写，不得先单独 remap 一次。
///
/// 探针（512 B 页）：首批 `a`..`o` → 页 0/1/2 + root 3；第二批删除页 0 的全部键
/// `a..f` 并覆盖页 1 的 `g`。C++ `LeafLinkDelete`（`batch_write_task.cpp:168-185`）
/// 把页 1 留在 `leaf_triple_[2]`，`LoadApplyingPage` 快路径复用它，页 1 只写一次：
/// `[(0,7),(1,33),(3,41)]`；旧实现 `update_prev` 先 remap 页 1，页 1 自身重写再
/// remap 一次，delta 为 `[(0,7),(1,33),(1,41),(3,49)]`。
#[test]
fn deleted_head_page_successor_rewrite_remaps_once() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem51_delete_head", 1).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    let first: Vec<BatchEntry> = (0..15u8)
      .map(|i| BatchEntry::upsert(vec![b'a' + i], vec![b'a'; 64], 1))
      .collect();
    store.put(&tbl, &first).await.unwrap();
    assert_eq!(store.root_id(&tbl), Some(3));

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let mut second: Vec<BatchEntry> = Vec::new();
    for key in [b"a", b"b", b"c", b"d", b"e", b"f"] {
      second.push(BatchEntry::delete(key.to_vec(), 2));
    }
    second.push(BatchEntry::upsert(b"g".to_vec(), vec![b'z'; 64], 2));
    store.put(&tbl, &second).await.unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after.get(before.len()..).expect("增量 record");
    let entry_li = log_mapping_entry_li(record);

    assert_eq!(
      entry_li,
      vec![(0, INVALID_VALUE), (1, 33), (3, 41)],
      "页 1 只 remap 一次：链首页删除后的 prev=NULL 修复并入自身重写"
    );
    let page1 = page_bytes(&store, &tbl, 1).await;
    assert_eq!(
      DataPage::new(&page1).unwrap().prev_page_id().unwrap(),
      fkv_types::NULL_PAGE
    );
    assert_eq!(store.get(&tbl, b"g").await.unwrap(), Some(vec![b'z'; 64]));
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 9);
  });
}

/// sem-52：删除链中间页时，其被本批改写的前驱页的 `UpdateMapping` 必须排在
/// 删除页的 `DeleteMapping` 之后。
///
/// C++ `BatchWriteTask::FinishDataPage`（`batch_write_task.cpp:1064-1116`）把改写
/// 后的页留在 `leaf_triple_[0]`；`LeafLinkDelete`（`:168-185`）先 `FreePage`
/// 删除页，前驱页留到后续 / 批末 `ShiftLeafLink`（`:122-131`）才 `WritePage`。
/// 探针（512 B 页）：首批 `a`..`o` → 页 0/1/2 + root 3；第二批覆盖页 0 的 `a`
/// 并删除页 1 的全部键 `g..l`。C++ 记录序列为
/// `[(1,Invalid),(0,33),(2,41),(3,49)]`；旧实现在页 0 自己的组里就 remap，
/// 序列为 `[(0,33),(1,Invalid),(2,41),(3,49)]`。
#[test]
fn delete_middle_page_predecessor_record_after_delete_mapping() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem52_delete_middle", 1).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    let first: Vec<BatchEntry> = (0..15u8)
      .map(|i| BatchEntry::upsert(vec![b'a' + i], vec![b'a'; 64], 1))
      .collect();
    store.put(&tbl, &first).await.unwrap();
    assert_eq!(store.root_id(&tbl), Some(3));

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let mut second: Vec<BatchEntry> = vec![BatchEntry::upsert(b"a".to_vec(), vec![b'z'; 64], 2)];
    for key in [b"g", b"h", b"i", b"j", b"k", b"l"] {
      second.push(BatchEntry::delete(key.to_vec(), 2));
    }
    store.put(&tbl, &second).await.unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after.get(before.len()..).expect("增量 record");

    assert_eq!(
      log_mapping_entry_li(record),
      vec![(1, INVALID_VALUE), (0, 33), (2, 41), (3, 49)],
      "前驱页 UpdateMapping 必须排在删除页 DeleteMapping 之后"
    );
    // 页链 0 -> 2，页 0 的 next 指向页 2、页 2 的 prev 指向页 0。
    let page0 = page_bytes(&store, &tbl, 0).await;
    assert_eq!(DataPage::new(&page0).unwrap().next_page_id().unwrap(), 2);
    let page2 = page_bytes(&store, &tbl, 2).await;
    assert_eq!(DataPage::new(&page2).unwrap().prev_page_id().unwrap(), 0);
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 9);
  });
}

/// sem-52：删除中间页且后继页也在本批被改写时，前驱页的 `UpdateMapping` 仍在
/// `DeleteMapping` 之后、后继页之前（C++ `LoadApplyingPage` 快路径复用
/// `leaf_triple_[1]`，`FinishDataPage` 写出 `leaf_triple_[0]`）。
///
/// 探针：首批 `a`..`o` → 页 0/1/2 + root 3；第二批覆盖页 0 的 `a`、删除页 1 的
/// `g..l`、覆盖页 2 的 `m`。期望 `[(1,Invalid),(0,33),(2,41),(3,49)]`。
#[test]
fn delete_middle_page_then_rewrite_successor_record_order() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem52_delete_middle_succ", 1).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    let first: Vec<BatchEntry> = (0..15u8)
      .map(|i| BatchEntry::upsert(vec![b'a' + i], vec![b'a'; 64], 1))
      .collect();
    store.put(&tbl, &first).await.unwrap();
    assert_eq!(store.root_id(&tbl), Some(3));

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let mut second: Vec<BatchEntry> = vec![BatchEntry::upsert(b"a".to_vec(), vec![b'z'; 64], 2)];
    for key in [b"g", b"h", b"i", b"j", b"k", b"l"] {
      second.push(BatchEntry::delete(key.to_vec(), 2));
    }
    second.push(BatchEntry::upsert(b"m".to_vec(), vec![b'y'; 64], 2));
    store.put(&tbl, &second).await.unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after.get(before.len()..).expect("增量 record");

    assert_eq!(
      log_mapping_entry_li(record),
      vec![(1, INVALID_VALUE), (0, 33), (2, 41), (3, 49)],
      "删除页 → 前驱页 → 后继页的写出相位"
    );
    assert_eq!(store.get(&tbl, b"m").await.unwrap(), Some(vec![b'y'; 64]));
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 9);
  });
}

/// sem-52：链尾页被整体删除时，被改写的前驱页（现在的尾页）同样在
/// `DeleteMapping` 之后才写出。
///
/// 探针：首批 `a`..`o` → 页 0/1/2 + root 3；第二批覆盖页 1 的 `g` 并删除页 2 的
/// `m..o`。期望 `[(2,Invalid),(1,33),(3,41)]`。
#[test]
fn delete_tail_page_predecessor_record_after_delete_mapping() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem52_delete_tail", 1).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    let first: Vec<BatchEntry> = (0..15u8)
      .map(|i| BatchEntry::upsert(vec![b'a' + i], vec![b'a'; 64], 1))
      .collect();
    store.put(&tbl, &first).await.unwrap();
    assert_eq!(store.root_id(&tbl), Some(3));

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let mut second: Vec<BatchEntry> = vec![BatchEntry::upsert(b"g".to_vec(), vec![b'z'; 64], 2)];
    for key in [b"m", b"n", b"o"] {
      second.push(BatchEntry::delete(key.to_vec(), 2));
    }
    store.put(&tbl, &second).await.unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after.get(before.len()..).expect("增量 record");

    assert_eq!(
      log_mapping_entry_li(record),
      vec![(2, INVALID_VALUE), (1, 33), (3, 41)],
      "链尾删除：删除页 DeleteMapping 先于前驱页 UpdateMapping"
    );
    let page1 = page_bytes(&store, &tbl, 1).await;
    assert_eq!(
      DataPage::new(&page1).unwrap().next_page_id().unwrap(),
      fkv_types::NULL_PAGE
    );
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 12);
  });
}

/// sem-52：同批删除链上连续两页时，两条 `DeleteMapping` 都排在前驱页写出之前
/// （C++ 删除页先 `FreePage`，前驱页一直留在 `leaf_triple_[0]` 到批末）。
///
/// 探针：首批 `a`..`o` → 页 0/1/2 + root 3；第二批覆盖页 0 的 `a` 并删除页 1、2
/// 的全部键。期望 `[(1,Invalid),(2,Invalid),(0,33),(3,41)]`。
#[test]
fn delete_multiple_middle_pages_predecessor_record_after_delete_mappings() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem52_delete_multi", 1).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    let first: Vec<BatchEntry> = (0..15u8)
      .map(|i| BatchEntry::upsert(vec![b'a' + i], vec![b'a'; 64], 1))
      .collect();
    store.put(&tbl, &first).await.unwrap();
    assert_eq!(store.root_id(&tbl), Some(3));

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let mut second: Vec<BatchEntry> = vec![BatchEntry::upsert(b"a".to_vec(), vec![b'z'; 64], 2)];
    for key in [b"g", b"h", b"i", b"j", b"k", b"l", b"m", b"n", b"o"] {
      second.push(BatchEntry::delete(key.to_vec(), 2));
    }
    store.put(&tbl, &second).await.unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after.get(before.len()..).expect("增量 record");

    assert_eq!(
      log_mapping_entry_li(record),
      vec![(1, INVALID_VALUE), (2, INVALID_VALUE), (0, 33), (3, 41)],
      "多页删除：两条 DeleteMapping 均先于前驱页 UpdateMapping"
    );
    let page0 = page_bytes(&store, &tbl, 0).await;
    assert_eq!(
      DataPage::new(&page0).unwrap().next_page_id().unwrap(),
      fkv_types::NULL_PAGE
    );
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 6);
  });
}

/// sem-53：分裂组最后一张分裂页的 `UpdateMapping` 必须排在同批删除的直接后继页
/// `DeleteMapping` 之后。
///
/// C++ `FinishDataPage`（`batch_write_task.cpp:1064-1116`）对新页只 `GetPage()` 取
/// 逻辑 id（`page_mapper.cpp:358-374`），尾页留在 `leaf_triple_[0]`；后继页
/// `LeafLinkDelete`（`:168-185`）之后才 `FreePage`（`DeleteMapping`），尾页直到
/// 批末 `ShiftLeafLink`（`:122-131`）才 `WritePage` → `AllocatePage` +
/// `UpdateMapping`（`write_task.cpp:238-267`）。
///
/// 探针（512 B 页）：首批 `a`..`o` → 页 0/1/2 + root 3；第二批插入 `a0`..`a5`
/// 使页 0 分裂为页 0 + 页 4，并删除页 1 的全部键 `g..l`。C++ 记录序列为
/// `[(0,33),(1,Invalid),(4,41),(2,49),(3,57)]`；旧实现尾页在分裂时立即
/// `allocate_page()`，序列为 `[(0,33),(4,41),(1,Invalid),(2,49),(3,57)]`。
#[test]
fn split_tail_update_mapping_after_deleted_successor() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem53_split_tail_delete", 1).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    let first: Vec<BatchEntry> = (0..15u8)
      .map(|i| BatchEntry::upsert(vec![b'a' + i], vec![b'a'; 64], 1))
      .collect();
    store.put(&tbl, &first).await.unwrap();
    assert_eq!(store.root_id(&tbl), Some(3));

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let mut second: Vec<BatchEntry> = Vec::new();
    for i in 0..6u8 {
      second.push(BatchEntry::upsert(vec![b'a', b'0' + i], vec![b'z'; 64], 2));
    }
    for key in [b"g", b"h", b"i", b"j", b"k", b"l"] {
      second.push(BatchEntry::delete(key.to_vec(), 2));
    }
    store.put(&tbl, &second).await.unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after.get(before.len()..).expect("增量 record");

    assert_eq!(
      log_mapping_entry_li(record),
      vec![(0, 33), (1, INVALID_VALUE), (4, 41), (2, 49), (3, 57)],
      "分裂尾页 UpdateMapping 必须排在删除后继页 DeleteMapping 之后"
    );
    // 页链 0 -> 4 -> 2：尾页 4 的 next 已并入删除页的 next 修复。
    let page0 = page_bytes(&store, &tbl, 0).await;
    assert_eq!(DataPage::new(&page0).unwrap().next_page_id().unwrap(), 4);
    let page4 = page_bytes(&store, &tbl, 4).await;
    let data4 = DataPage::new(&page4).unwrap();
    assert_eq!(data4.prev_page_id().unwrap(), 0);
    assert_eq!(data4.next_page_id().unwrap(), 2);
    let page2 = page_bytes(&store, &tbl, 2).await;
    assert_eq!(DataPage::new(&page2).unwrap().prev_page_id().unwrap(), 4);
    // 页 1 已回收，尾页 4 的数据完整（a5 与 b..f）。
    assert!(store.mapper(&tbl).unwrap().file_page_id(1).is_err());
    assert_eq!(store.get(&tbl, b"a5").await.unwrap(), Some(vec![b'z'; 64]));
    assert_eq!(store.get(&tbl, b"g").await.unwrap(), None);
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 15);
  });
}

/// sem-54：既有 data 页分裂成 ≥3 页、且后续输出页仍含 overflow 分配时，
/// overflow 记录与 data 页逻辑 id / mapping 必须按 C++ 的输出页边界交错。
///
/// C++ `BatchWriteTask::ApplyOnePage` 在 merge 循环里对每个 overflow value 立即
/// `WriteOverflowValue`（`batch_write_task.cpp:493-503` 的失败分支 →
/// `:1377-1445` 的 `GetPage` + `WritePage`）；当前输出页写满时 `FinishDataPage`
/// （`:1064-1116`）先给新页 `GetPage()` 取逻辑 id，再由 `ShiftLeafLink`
/// （`:122-131`）写出上一页（`AllocatePage` + `UpdateMapping`，
/// `write_task.cpp:551-621`）。因此 overflow 记录与 data 页 mapping 按输出页
/// 边界交错，而不是整组 overflow 先于全部 data 页。
///
/// 探针（512 B 页、restart 16）：首批 `k0`..`k2`（64 B）→ data 页 0 + root 1；
/// 第二批 150 条 `n000`..`n149`（500 B = 恰好一条 overflow 页）把页 0 撑成 4 页：
/// 页 0（`k0`..`k2` + `n000`..`n028`）、`n029` 起、`n080` 起、`n131` 起。
/// C++ 的分配相位（页 id = 该时刻的逻辑槽位，值 = `FilePageId` 编码）：
/// - `n000`..`n080` 的 overflow（2..82）；
/// - 页 0 的 `UpdateMapping`（由 `n080` 触发 `FinishDataPage(P0)`）；
/// - `n081`..`n130` 的 overflow（84..133）+ `n131` 的 overflow（134）；
/// - 页 1 的 `UpdateMapping`（由 `n131` 触发 `FinishDataPage(P1)`）；
/// - `n132`..`n149` 的 overflow（136..153）；
/// - 页 2（组末 `FinishDataPage`）、页 3（批末 `ShiftLeafLink`）、index root。
///
/// 旧实现把 150 条 overflow 全部写完才分配 data 页，delta 为
/// `[2..151, 0, 152, 153, 154, 1]`（issue sem-54 的探针）。
#[test]
fn split_overflow_allocation_interleaves_data_mapping_at_output_page_boundaries() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem54_split_overflow", 1).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    let first: Vec<BatchEntry> = (0..3)
      .map(|i| BatchEntry::upsert(format!("k{i}").into_bytes(), vec![b'a'; 64], 1))
      .collect();
    store.put(&tbl, &first).await.unwrap();
    assert_eq!(store.root_id(&tbl), Some(1));
    assert_eq!(store.mapper(&tbl).unwrap().tbl().len(), 2);

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let second: Vec<BatchEntry> = (0..150)
      .map(|i| BatchEntry::upsert(format!("n{i:03}").into_bytes(), vec![b'b'; 500], 2))
      .collect();
    store.put(&tbl, &second).await.unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after.get(before.len()..).expect("增量 record");
    let entry_li = log_mapping_entry_li(record);

    // 盘面页布局：4 张 data 页（页 0 + 3 张分裂页）+ index root。
    assert_eq!(store.root_id(&tbl), Some(1), "index root 逻辑页不变");
    assert_eq!(store.mapper(&tbl).unwrap().tbl().len(), 155);
    let root_page = page_bytes(&store, &tbl, 1).await;
    let mut child_li = Vec::new();
    let mut iter = IndexPageIter::new(&root_page).unwrap();
    while iter.advance().unwrap() {
      child_li.push(iter.page_id());
    }
    assert_eq!(
      child_li,
      vec![83, 135, 154],
      "分裂页 id 必须在各自 FinishDataPage 取到：与 overflow 分配交错"
    );
    // 页 0 承载 k0..k2 + n000..n028，分裂页首键 n029 / n080 / n131。
    let page0 = page_bytes(&store, &tbl, 0).await;
    let mut page0_iter = DataPageIter::new(&page0).unwrap();
    let mut page0_cnt = 0usize;
    let mut page0_last = Vec::new();
    while page0_iter.advance().unwrap() {
      page0_cnt += 1;
      page0_last = page0_iter.key().to_vec();
    }
    assert_eq!(page0_cnt, 32);
    assert_eq!(page0_last, b"n028");
    let mut first_key_li = Vec::new();
    for page_id in [83u32, 135, 154] {
      let page = page_bytes(&store, &tbl, page_id).await;
      let mut iter = DataPageIter::new(&page).unwrap();
      assert!(iter.advance().unwrap());
      first_key_li.push(iter.key().to_vec());
    }
    assert_eq!(
      first_key_li,
      vec![b"n029".to_vec(), b"n080".to_vec(), b"n131".to_vec()],
      "触发键 = 下一张输出页的首键"
    );

    // 由盘面布局推导的 C++ 相位序列（值 = encode_file_page_id，Append 顺序分配）。
    let mut expected = Vec::new();
    expected.extend((2..=82u32).map(|page_id| (page_id, u64::from(page_id) * 8 + 1)));
    // FinishDataPage(P0)：n080 的 overflow 已分配，ShiftLeafLink 写出页 0。
    expected.push((0, 83 * 8 + 1));
    expected.extend((84..=133u32).map(|page_id| (page_id, u64::from(page_id) * 8 + 1)));
    // n131 的 overflow 先于 FinishDataPage(P1)，后者写出页 1。
    expected.push((134, 134 * 8 + 1));
    expected.push((83, 135 * 8 + 1));
    expected.extend((136..=153u32).map(|page_id| (page_id, u64::from(page_id) * 8 + 1)));
    // 组末 FinishDataPage(P2) → 批末 ShiftLeafLink(P3) → index root remap。
    expected.push((135, 154 * 8 + 1));
    expected.push((154, 155 * 8 + 1));
    expected.push((1, 156 * 8 + 1));
    assert_eq!(
      entry_li, expected,
      "overflow 与 data 页 mapping 必须按输出页边界交错"
    );

    // 终态映射：逻辑页 id 与物理页归属都按 C++ 相位。
    let mapper = store.mapper(&tbl).unwrap();
    assert_eq!(mapper.file_page_id(0).unwrap(), 83);
    assert_eq!(mapper.file_page_id(83).unwrap(), 135);
    assert_eq!(mapper.file_page_id(135).unwrap(), 154);
    assert_eq!(mapper.file_page_id(154).unwrap(), 155);
    assert_eq!(mapper.file_page_id(1).unwrap(), 156);

    // data 页里的 overflow 指针引用交错后的逻辑 id。
    let page_83 = page_bytes(&store, &tbl, 83).await;
    let page_135 = page_bytes(&store, &tbl, 135).await;
    let page_154 = page_bytes(&store, &tbl, 154).await;
    assert_eq!(
      read_overflow_chain(&store, &tbl, &page0, b"n028").await,
      vec![30]
    );
    assert_eq!(
      read_overflow_chain(&store, &tbl, &page_83, b"n029").await,
      vec![31]
    );
    assert_eq!(
      read_overflow_chain(&store, &tbl, &page_135, b"n080").await,
      vec![82]
    );
    assert_eq!(
      read_overflow_chain(&store, &tbl, &page_154, b"n131").await,
      vec![134]
    );
    // 页链 0 -> 83 -> 135 -> 154。
    let data0 = DataPage::new(&page0).unwrap();
    let data83 = DataPage::new(&page_83).unwrap();
    let data135 = DataPage::new(&page_135).unwrap();
    let data154 = DataPage::new(&page_154).unwrap();
    assert_eq!(data0.next_page_id().unwrap(), 83);
    assert_eq!(data83.prev_page_id().unwrap(), 0);
    assert_eq!(data83.next_page_id().unwrap(), 135);
    assert_eq!(data135.prev_page_id().unwrap(), 83);
    assert_eq!(data135.next_page_id().unwrap(), 154);
    assert_eq!(data154.prev_page_id().unwrap(), 135);
    assert_eq!(data154.next_page_id().unwrap(), fkv_types::NULL_PAGE);
    // 可见数据完整。
    assert_eq!(
      store.get(&tbl, b"n000").await.unwrap(),
      Some(vec![b'b'; 500])
    );
    assert_eq!(
      store.get(&tbl, b"n149").await.unwrap(),
      Some(vec![b'b'; 500])
    );
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 153);
  });
}

/// sem-54：新树首写（`root_id == NULL_PAGE`）同样按输出页边界交错 overflow
/// 分配与 data 页 `GetPage`。
///
/// 探针：空表一次写入 150 条 `n000`..`n149`（500 B），3 张 data 页：
/// A = `n000`..`n050`（51 条）、B = `n051`..`n101`、C = `n102`..`n149`。
/// C++ 相位：
/// - `n000`..`n050` 的 overflow（页 0..50）+ 触发页 B 的 `n051`（页 51）；
/// - `FinishDataPage(A)`：`GetPage` → A = 52（`ShiftLeafLink` 此时无页可写）；
/// - `n052`..`n101`（53..102）+ `n102`（103）；
/// - `FinishDataPage(B)`：`GetPage` → B = 104，写出 A（物理页 103）；
/// - `n103`..`n149`（105..151）；
/// - 组末 `FinishDataPage(C)`：C = 152，写出 B（物理页 151）；
/// - 批末 `ShiftLeafLink` 写出 C（物理页 152）；index root = 153（物理页 153）。
///
/// 旧实现是 `[overflow 0..149, data 150/151/152, root 153]`。首写落整份
/// snapshot，因此用 snapshot mapping 段断言逻辑页 id 与物理页归属的交错。
#[test]
fn new_tree_write_interleaves_data_page_ids_with_overflow_allocations() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem54_new_tree", 1).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    let batch: Vec<BatchEntry> = (0..150)
      .map(|i| BatchEntry::upsert(format!("n{i:03}").into_bytes(), vec![b'b'; 500], 1))
      .collect();
    store.put(&tbl, &batch).await.unwrap();
    assert_eq!(store.root_id(&tbl), Some(153));
    assert_eq!(store.mapper(&tbl).unwrap().tbl().len(), 154);

    // 按盘面页类型定位 data / index 页：逻辑 id 必须与 overflow 页交错。
    let mut data_li = Vec::new();
    let mut index_li = Vec::new();
    for page_id in 0..154u32 {
      let page = page_bytes(&store, &tbl, page_id).await;
      match page_type(&page).unwrap() {
        PageType::Data => data_li.push(page_id),
        PageType::LeafIndex => index_li.push(page_id),
        PageType::Overflow => {}
        other => panic!("意外的页类型 {other:?}（page {page_id}）"),
      }
    }
    assert_eq!(
      data_li,
      vec![52, 104, 152],
      "data 页 id 在各自 FinishDataPage 取"
    );
    assert_eq!(index_li, vec![153]);

    // 三张 data 页的键区间（与 C++ 的触发键一致）。
    let mut page_li = Vec::new();
    let mut range_li = Vec::new();
    for page_id in &data_li {
      let page = page_bytes(&store, &tbl, *page_id).await;
      let mut iter = DataPageIter::new(&page).unwrap();
      let mut cnt = 0usize;
      let mut first = Vec::new();
      let mut last = Vec::new();
      while iter.advance().unwrap() {
        if cnt == 0 {
          first = iter.key().to_vec();
        }
        last = iter.key().to_vec();
        cnt += 1;
      }
      range_li.push((cnt, first, last));
      page_li.push(page);
    }
    assert_eq!(
      range_li,
      vec![
        (51, b"n000".to_vec(), b"n050".to_vec()),
        (51, b"n051".to_vec(), b"n101".to_vec()),
        (48, b"n102".to_vec(), b"n149".to_vec()),
      ]
    );

    // snapshot mapping：值 = 每页 FilePageId 编码（Append 分配）。
    let record = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let section = snapshot_mapping(&record);
    let mut value_li = Vec::new();
    let mut pos = 0usize;
    while pos < section.len() {
      let (value, read) = get_varint64(&section[pos..]).expect("mapping value");
      pos += read;
      value_li.push(value);
    }
    assert_eq!(value_li.len(), 154);
    // 逻辑 id 52/104/152 是 data 页，但它们与 overflow 页共享同一分配序列：
    // 物理页在写出相位分配，故 52 -> 103、104 -> 151、152/153 顺次收尾。
    assert_eq!(value_li[52], encode_file_page_id(103));
    assert_eq!(value_li[104], encode_file_page_id(151));
    assert_eq!(value_li[152], encode_file_page_id(152));
    assert_eq!(value_li[153], encode_file_page_id(153));
    // 交错点前后：51 是触发 data 页 A 的 overflow 页，53 是 A 之后的第一个
    // overflow 页（物理页比逻辑 id 小 1：A 的逻辑槽位先占、物理页后分配）。
    assert_eq!(value_li[51], encode_file_page_id(51));
    assert_eq!(value_li[53], encode_file_page_id(52));

    // index 子项与 data 页里的 overflow 指针。
    let index_page = page_bytes(&store, &tbl, 153).await;
    let mut child_li = Vec::new();
    let mut iter = IndexPageIter::new(&index_page).unwrap();
    while iter.advance().unwrap() {
      child_li.push(iter.page_id());
    }
    assert_eq!(
      child_li,
      data_li[1..],
      "leaf index 的 leftmost 子项不参与迭代"
    );
    assert_eq!(
      read_overflow_chain(&store, &tbl, &page_li[0], b"n050").await,
      vec![50]
    );
    assert_eq!(
      read_overflow_chain(&store, &tbl, &page_li[1], b"n051").await,
      vec![51]
    );
    assert_eq!(
      read_overflow_chain(&store, &tbl, &page_li[2], b"n102").await,
      vec![103]
    );
    // 页链 A -> B -> C。
    assert_eq!(
      DataPage::new(&page_li[0]).unwrap().next_page_id().unwrap(),
      104
    );
    assert_eq!(
      DataPage::new(&page_li[1]).unwrap().prev_page_id().unwrap(),
      52
    );
    assert_eq!(
      DataPage::new(&page_li[2]).unwrap().prev_page_id().unwrap(),
      104
    );
    assert_eq!(
      DataPage::new(&page_li[2]).unwrap().next_page_id().unwrap(),
      fkv_types::NULL_PAGE
    );
    // 可见数据。
    assert_eq!(
      store.get(&tbl, b"n000").await.unwrap(),
      Some(vec![b'b'; 500])
    );
    assert_eq!(
      store.get(&tbl, b"n149").await.unwrap(),
      Some(vec![b'b'; 500])
    );
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 150);
  });
}

/// sem-54 对照：输出只有 2 页时相位不变 —— C++ 的最后一个 `FinishDataPage`
/// 在组末才触发，此时整组 overflow 都已分配，`ShiftLeafLink` 才写出原页
/// （这也是 sem-49 无 overflow 探针看不出差异的原因）。
///
/// 探针：首批 `k0`..`k2`（64 B）→ 页 0 + root 1；第二批 30 条 500 B 把页 0
/// 撑成 2 页（页 0 + 新页 32）。记录序列保持
/// `overflow 2..31 → (0,257) → (32,265) → (1,273)`。
#[test]
fn two_page_split_keeps_overflow_before_data_mapping() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem54_two_page", 1).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    let first: Vec<BatchEntry> = (0..3)
      .map(|i| BatchEntry::upsert(format!("k{i}").into_bytes(), vec![b'a'; 64], 1))
      .collect();
    store.put(&tbl, &first).await.unwrap();
    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let second: Vec<BatchEntry> = (0..30)
      .map(|i| BatchEntry::upsert(format!("n{i:03}").into_bytes(), vec![b'b'; 500], 2))
      .collect();
    store.put(&tbl, &second).await.unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after.get(before.len()..).expect("增量 record");
    let entry_li = log_mapping_entry_li(record);

    assert_eq!(store.mapper(&tbl).unwrap().tbl().len(), 33);
    let mut expected: Vec<(u32, u64)> = (2..=31u32)
      .map(|page_id| (page_id, u64::from(page_id) * 8 + 1))
      .collect();
    expected.push((0, 32 * 8 + 1));
    expected.push((32, 33 * 8 + 1));
    expected.push((1, 34 * 8 + 1));
    assert_eq!(
      entry_li, expected,
      "2 页分裂：overflow 记录仍全部先于原页，原页先于新页"
    );
    let mapper = store.mapper(&tbl).unwrap();
    assert_eq!(mapper.file_page_id(0).unwrap(), 32);
    assert_eq!(mapper.file_page_id(32).unwrap(), 33);
    assert_eq!(mapper.file_page_id(1).unwrap(), 34);
    assert_eq!(
      store.get(&tbl, b"n029").await.unwrap(),
      Some(vec![b'b'; 500])
    );
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 33);
  });
}
