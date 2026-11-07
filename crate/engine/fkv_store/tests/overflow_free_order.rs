use fkv_btree::BatchEntry;
use fkv_codec::append_varint64;
use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_page::{DataPageIter, OverflowPage, PageType, decode_pointers, page_type};
use fkv_store::Store;
use fkv_types::{INVALID_VALUE, TableIdent, encode_page_id, is_file_page_id};

mod common;

use common::{
  find_data_page, log_mapping_entry_li, page_bytes, read_overflow_chain, snapshot_mapping,
};

/// sem-47：overflow 链释放顺序必须与 C++ `BatchWriteTask::DelOverflowValue` 一致。
///
/// C++ 按每批指针数组下标**前向**遍历（batch_write_task.cpp:1451-1466），遇到
/// 一批的最后一槽（`overflow_pointers - 1`）先 `LoadOverflowPage` 取下一批指针、
/// 再 `FreePage`。fkv 旧的 `pending.pop()` 从链尾 LIFO 弹出，12 KiB 值的释放序列
/// 是 `[15,24..16,14..0]`，与本用例的 `[0..24]` 相反。
///
/// 期望序列不取自被测实现的 `dirty_page_li`，而是从盘面显式推导：data 页 leaf
/// entry 的首批指针 + 尾页指针数组给出链序，再按 C++ 的「链前向 → data 页 →
/// index root」顺序拼出。
#[test]
fn overflow_chain_free_order_matches_cpp_del_overflow_value() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem47_free_order", 1).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    // 12 000 B：16 指针首组 + 9 指针尾组，共 25 条 overflow 页（超出单批）。
    let value: Vec<u8> = (0..12_000u32).map(|i| (i % 251) as u8).collect();
    store
      .put(&tbl, &[BatchEntry::upsert(b"big".to_vec(), value, 1)])
      .await
      .unwrap();

    let root = store.root_id(&tbl).expect("root");
    // Append 分配：逻辑页按写入顺序 0 起连续分配，全部活跃。
    let mapper = store.mapper(&tbl).unwrap();
    let live_li: Vec<u32> = mapper
      .tbl()
      .iter()
      .filter(|(_, value)| is_file_page_id(*value))
      .map(|(page_id, _)| page_id)
      .collect();
    let page_num = mapper.tbl().len() as u32;
    assert_eq!(live_li, (0..page_num).collect::<Vec<u32>>());
    assert_eq!(page_num, 27, "25 overflow + data + index root");

    // 唯一 data 页承载 leaf entry；其 value 是首批 overflow 指针。
    let mut data_page = None;
    for page_id in 0..page_num {
      let page = page_bytes(&store, &tbl, page_id).await;
      if page_type(&page).unwrap() == PageType::Data {
        data_page = Some(page_id);
      }
    }
    let data_page = data_page.expect("data page");
    let data = page_bytes(&store, &tbl, data_page).await;
    let mut iter = DataPageIter::new(&data).unwrap();
    let mut head_li = Vec::new();
    while iter.advance().unwrap() {
      if iter.key() == b"big" {
        assert!(iter.is_overflow(), "12 KiB 值必须是 overflow entry");
        head_li = decode_pointers(iter.value()).unwrap();
        break;
      }
    }
    // 首批 16 个指针 = 链序 0..16；尾页 15 的指针数组给出 16..25。
    assert_eq!(head_li, (0..16).collect::<Vec<u32>>());
    let end_page = page_bytes(&store, &tbl, head_li[15]).await;
    let tail_li = OverflowPage::new(&end_page).unwrap().pointer_li().unwrap();
    assert_eq!(tail_li, (16..25).collect::<Vec<u32>>());
    let mut cpp_order: Vec<u32> = head_li;
    cpp_order.extend(tail_li);
    cpp_order.extend([data_page, root]);
    assert_eq!(cpp_order, (0..page_num).collect::<Vec<u32>>());

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    store.delete(&tbl, b"big", 2).await.unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after.get(before.len()..).expect("增量 record");
    let entry_li = log_mapping_entry_li(record);
    // 每条释放记录的值仍是 sem-42 的 InvalidValue(7)，只有 page_id 顺序有判别力。
    assert!(entry_li.iter().all(|(_, value)| *value == INVALID_VALUE));
    let delta_ids: Vec<u32> = entry_li.iter().map(|(page_id, _)| *page_id).collect();
    assert_eq!(
      delta_ids, cpp_order,
      "overflow 链必须按 C++ DelOverflowValue 的前向顺序释放"
    );
  });
}

/// sem-47：前向释放顺序决定 snapshot mapping 的空闲链布局（= 后续复用顺序）。
///
/// `manifest_limit == 1` 使 delete 直接落整份 snapshot（不走 log 增量），
/// `snapshot_mapping` 即 `PageMapper::FreePage` 串出的空闲链。C++ 前向释放
/// 0..24（overflow）、data 页、index root 后，链头是最后一页，链节依次指向前
/// 一页：页 0 为链尾 `InvalidValue`，页 i>0 写 `EncodePageId(i - 1)`。
/// 旧的 LIFO 顺序会把 16..24 插在 0 与 15 之间，链形状完全不同。
#[test]
fn delete_overflow_snapshot_free_chain_matches_cpp_forward_free() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem47_snapshot", 2).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16).with_manifest_limit(1);
    let value: Vec<u8> = (0..12_000u32).map(|i| (i % 251) as u8).collect();
    store
      .put(&tbl, &[BatchEntry::upsert(b"big".to_vec(), value, 1)])
      .await
      .unwrap();
    let page_num = store.mapper(&tbl).unwrap().tbl().len() as u32;
    assert_eq!(page_num, 27, "25 overflow + data + index root");

    store.delete(&tbl, b"big", 2).await.unwrap();
    let record = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let mut expected = Vec::with_capacity(page_num as usize * 5);
    for page_id in 0..page_num {
      let value = if page_id == 0 {
        INVALID_VALUE
      } else {
        encode_page_id(page_id - 1)
      };
      append_varint64(&mut expected, value);
    }
    assert_eq!(
      snapshot_mapping(&record),
      expected.as_slice(),
      "snapshot 空闲链必须逐字节等于 C++ 前向释放的 FreePage 链"
    );
  });
}

/// sem-48：同页多个 overflow key 被同一批覆盖时，free / alloc 必须按 key 交错，
/// 对齐 C++ `BatchWriteTask::ApplyOnePage` 的逐 key 相位。
///
/// C++ 在 merge 循环里处理 a：先 `DelOverflowValue`（前向释放 0..24），随后
/// `add_to_page` 内的 `WriteOverflowValue` 立即从 LIFO 空闲链取回 a 自己的页
/// 集合；b 同理（25..49）。旧实现整页先 free a、b 再统一 alloc，新 a 链会拿到
/// b 的页集合、新 b 链拿到 a 的页集合 —— log delta 的相位与 data page 的
/// overflow 指针列表都不同（issue sem-48）。
#[test]
fn same_page_multikey_rewrite_interleaves_free_alloc_per_key() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem48_same_page", 1).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    let value_a: Vec<u8> = (0..12_000u32).map(|i| (i % 251) as u8).collect();
    let value_b: Vec<u8> = (0..12_000u32).map(|i| (i % 241) as u8).collect();
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), value_a, 1),
          BatchEntry::upsert(b"b".to_vec(), value_b, 1),
        ],
      )
      .await
      .unwrap();

    // Append 分配：a 链 0..24，b 链 25..49，data 页 50，index root 51。
    let page_num = store.mapper(&tbl).unwrap().tbl().len() as u32;
    assert_eq!(page_num, 52, "25+25 overflow + data + index root");
    assert_eq!(store.root_id(&tbl), Some(51), "index root 为最后一页");
    let data_page_id = find_data_page(&store, &tbl, page_num).await;
    let old_data = page_bytes(&store, &tbl, data_page_id).await;
    let old_a = read_overflow_chain(&store, &tbl, &old_data, b"a").await;
    let old_b = read_overflow_chain(&store, &tbl, &old_data, b"b").await;
    assert_eq!(old_a, (0..25).collect::<Vec<u32>>());
    assert_eq!(old_b, (25..50).collect::<Vec<u32>>());

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let new_a: Vec<u8> = (0..12_000u32).map(|i| (i % 239) as u8 ^ 0x5a).collect();
    let new_b: Vec<u8> = (0..12_000u32).map(|i| (i % 233) as u8 ^ 0x3c).collect();
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), new_a, 2),
          BatchEntry::upsert(b"b".to_vec(), new_b, 2),
        ],
      )
      .await
      .unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after.get(before.len()..).expect("增量 record");
    let entry_li = log_mapping_entry_li(record);

    // C++ 逐 key 相位：free(a 前向) → alloc(a，LIFO 逆序复用 a 的页) →
    // free(b 前向) → alloc(b，复用 b 的页) → data 页 remap。
    let mut expected: Vec<(u32, bool)> = Vec::with_capacity(101);
    for &page_id in &old_a {
      expected.push((page_id, true));
    }
    expected.extend(old_a.iter().rev().map(|&page_id| (page_id, false)));
    for &page_id in &old_b {
      expected.push((page_id, true));
    }
    expected.extend(old_b.iter().rev().map(|&page_id| (page_id, false)));
    expected.push((data_page_id, false));

    let got: Vec<(u32, bool)> = entry_li
      .iter()
      .map(|&(page_id, value)| (page_id, value == INVALID_VALUE))
      .collect();
    assert_eq!(
      got, expected,
      "log delta 必须是 free(a)/alloc(a)/free(b)/alloc(b) 的逐 key 相位"
    );
    // 释放项固定 7；分配项是操作时的 FilePageId，本批后未被再次改写。
    let mapper = store.mapper(&tbl).unwrap();
    for &(page_id, value) in &entry_li {
      if value == INVALID_VALUE {
        continue;
      }
      assert!(is_file_page_id(value));
      assert_eq!(mapper.tbl().get(page_id).unwrap(), value);
    }

    // data page 指针列表：a 链 = a 旧集合的 LIFO 逆序，b 链 = b 旧集合的逆序。
    let new_data = page_bytes(&store, &tbl, data_page_id).await;
    let new_a = read_overflow_chain(&store, &tbl, &new_data, b"a").await;
    let new_b = read_overflow_chain(&store, &tbl, &new_data, b"b").await;
    assert_eq!(new_a, old_a.iter().rev().copied().collect::<Vec<u32>>());
    assert_eq!(new_b, old_b.iter().rev().copied().collect::<Vec<u32>>());
    assert_ne!(
      new_data, old_data,
      "data page 的 overflow 指针列表必须已更新"
    );
  });
}

/// sem-48：混合批（插入 key 排序在删除 key 之前）的分配 / 释放顺序。
///
/// C++ 的 merge 循环按 key 升序处理：先为插入 a 分配新 overflow 链（此时空闲
/// 链为空，只能追加新逻辑页），随后才释放被删除 z 的旧链。旧实现先整页 free z
/// 再 alloc a，a 会直接复用 z 的旧页集合。
#[test]
fn mixed_batch_insert_before_delete_allocates_before_free() {
  const PAGE_SIZE: usize = 512;
  pollster::block_on(async {
    let tbl = TableIdent::new("sem48_mixed", 2).unwrap();
    let mut store = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, 16);
    let value_z: Vec<u8> = (0..12_000u32).map(|i| (i % 251) as u8).collect();
    store
      .put(&tbl, &[BatchEntry::upsert(b"z".to_vec(), value_z, 1)])
      .await
      .unwrap();

    // z 链 0..24，data 页 25，index root 26。
    let old_page_num = store.mapper(&tbl).unwrap().tbl().len() as u32;
    assert_eq!(old_page_num, 27, "25 overflow + data + index root");
    let old_live: Vec<u32> = store
      .mapper(&tbl)
      .unwrap()
      .tbl()
      .iter()
      .filter(|(_, value)| is_file_page_id(*value))
      .map(|(page_id, _)| page_id)
      .collect();
    assert_eq!(old_live, (0..27).collect::<Vec<u32>>());
    let data_page_id = find_data_page(&store, &tbl, old_page_num).await;
    let old_data = page_bytes(&store, &tbl, data_page_id).await;
    let z_chain = read_overflow_chain(&store, &tbl, &old_data, b"z").await;
    assert_eq!(z_chain, (0..25).collect::<Vec<u32>>());

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let value_a: Vec<u8> = (0..12_000u32).map(|i| (i % 239) as u8 ^ 0x5a).collect();
    store
      .put(
        &tbl,
        &[
          BatchEntry::delete(b"z".to_vec(), 2),
          BatchEntry::upsert(b"a".to_vec(), value_a, 2),
        ],
      )
      .await
      .unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after.get(before.len()..).expect("增量 record");
    let entry_li = log_mapping_entry_li(record);

    // 先 25 条 a 的新链分配（追加页，不与 z 旧链相交），再 25 条 z 链前向释放，
    // 最后 remap data 页。
    let alloc_num = z_chain.len();
    assert_eq!(
      entry_li.len(),
      alloc_num * 2 + 1,
      "alloc + free + data remap"
    );
    let (alloc_li, rest) = entry_li.split_at(alloc_num);
    assert!(alloc_li.iter().all(|(_, value)| is_file_page_id(*value)));
    let alloc_ids: Vec<u32> = alloc_li.iter().map(|(page_id, _)| *page_id).collect();
    assert!(
      alloc_ids.iter().all(|page_id| !z_chain.contains(page_id)),
      "插入 a 必须先于删除 z 释放：不得复用 z 的旧页"
    );
    let (free_li, data_li) = rest.split_at(alloc_num);
    assert!(free_li.iter().all(|(_, value)| *value == INVALID_VALUE));
    assert_eq!(
      free_li
        .iter()
        .map(|(page_id, _)| *page_id)
        .collect::<Vec<u32>>(),
      z_chain,
      "删除 z 的链必须按 C++ DelOverflowValue 前向顺序释放"
    );
    assert_eq!(data_li.len(), 1);
    assert_eq!(data_li[0].0, data_page_id, "最后才 remap data 页");
    assert!(is_file_page_id(data_li[0].1));

    // data page 只剩 a，其指针列表 = 上面的分配顺序（全部是新追加的逻辑页）。
    let new_data = page_bytes(&store, &tbl, data_page_id).await;
    let a_chain = read_overflow_chain(&store, &tbl, &new_data, b"a").await;
    assert_eq!(a_chain, alloc_ids);
    assert!(a_chain.iter().all(|page_id| !old_live.contains(page_id)));
  });
}
