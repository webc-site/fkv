use std::slice::from_ref;

use fkv_btree::BatchEntry;
use fkv_codec::{append_varint32, append_varint64};
use fkv_error::KvError;
use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_manifest::parse_header;
use fkv_store::Store;
use fkv_types::{INVALID_VALUE, NULL_PAGE, TableIdent, encode_page_id, is_file_page_id};

mod common;

use common::{
  log_mapping, log_mapping_entry_li, log_segment, log_segment_entry_li, snapshot_mapping,
};

#[test]
fn mem_reopen_after_snapshot_and_log() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    let io = store.into_io();
    let mut reopened = Store::new(io, 512, 16);
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(reopened.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

    reopened
      .put(&tbl, &[BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 2)])
      .await
      .unwrap();
    let io = reopened.into_io();
    let mut second = Store::new(io, 512, 16);
    second.reopen(&tbl).await.unwrap();
    assert_eq!(second.get(&tbl, b"c").await.unwrap(), Some(b"C".to_vec()));
    assert_eq!(second.scan(&tbl, b"", b"", 0).await.unwrap().len(), 3);
  });
}

#[test]
fn manifest_rotation() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 2).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16).with_manifest_limit(1);
    for (i, key) in [b"a", b"b", b"c"].into_iter().enumerate() {
      store
        .put(
          &tbl,
          &[BatchEntry::upsert(
            key.to_vec(),
            vec![b'v'; 8],
            i as u64 + 1,
          )],
        )
        .await
        .unwrap();
    }
    let size = store
      .page_io()
      .read_manifest(&tbl)
      .await
      .unwrap()
      .unwrap()
      .len();
    assert_eq!(size, fkv_types::os_page_align());
    let io = store.into_io();
    let mut reopened = Store::new(io, 512, 16);
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(reopened.scan(&tbl, b"", b"", 0).await.unwrap().len(), 3);
  });
}

/// l2-06：`manifest_limit == 0` 必须与 C++ `WriteTask` 同语义 —— 恒写整份 snapshot
/// （C++ `manifest_size > 0` 使 `manifest_size + log <= 0` 恒假，落 snapshot 分支，
/// `src/tasks/write_task.cpp:807`），而不是 Rust 改动前的「恒 append」。
#[test]
fn manifest_limit_zero_always_writes_snapshot() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 4).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16).with_manifest_limit(0);
    for (i, key) in [b"a", b"b", b"c"].into_iter().enumerate() {
      store
        .put(
          &tbl,
          &[BatchEntry::upsert(
            key.to_vec(),
            vec![b'v'; 8],
            i as u64 + 1,
          )],
        )
        .await
        .unwrap();
    }
    let io = store.into_io();
    // 三次写全部走 snapshot：零次 append、三次整份替换。
    assert_eq!(
      io.manifest_append_num(),
      0,
      "manifest_limit=0 不得走 append"
    );
    assert_eq!(io.manifest_switch_num(), 3);
    // 盘面形态：每条 snapshot 都整份替换，文件恒为一页。
    assert_eq!(
      io.read_manifest(&tbl).await.unwrap().unwrap().len(),
      fkv_types::os_page_align()
    );
    let mut reopened = Store::new(io, 512, 16);
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(reopened.scan(&tbl, b"", b"", 0).await.unwrap().len(), 3);
    assert_eq!(reopened.get(&tbl, b"c").await.unwrap(), Some(vec![b'v'; 8]));
  });
}

/// 对照：默认 8 MiB 下的盘面形态不变 —— 首条 snapshot，之后逐条 append，文件按
/// 对齐 record 累加（3 页）。这条锁住 l2-06 改动「默认路径逐字节不变」。
#[test]
fn default_manifest_limit_still_appends() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 5).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    for (i, key) in [b"a", b"b", b"c"].into_iter().enumerate() {
      store
        .put(
          &tbl,
          &[BatchEntry::upsert(
            key.to_vec(),
            vec![b'v'; 8],
            i as u64 + 1,
          )],
        )
        .await
        .unwrap();
    }
    let io = store.into_io();
    assert_eq!(io.manifest_switch_num(), 1, "首条 record 必须是 snapshot");
    assert_eq!(io.manifest_append_num(), 2, "后续两条走 append");
    assert_eq!(
      io.read_manifest(&tbl).await.unwrap().unwrap().len(),
      3 * fkv_types::os_page_align()
    );
  });
}

/// sem-42：delete 负载的 manifest log 增量必须与 C++ `ManifestBuilder::DeleteMapping`
/// 逐字节一致 —— 释放页只写 `InvalidValue(7)`，不落空闲链值。
///
/// 大 value 删除会在同一写批释放 data 页与多条 overflow 页，覆盖「释放多个逻辑页」
/// 的真实场景（C++ `WriteTask::FreePage` 对每页都只记 `page_id | 7`）。
#[test]
fn delete_workload_log_delta_matches_cpp_delete_mapping() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem42_delete", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    // 12 KiB：data 页 + 多条 overflow 页。
    let value: Vec<u8> = (0..12_000u32).map(|i| (i % 251) as u8).collect();
    store
      .put(&tbl, &[BatchEntry::upsert(b"big".to_vec(), value, 1)])
      .await
      .unwrap();
    let page_num = store.mapper(&tbl).unwrap().tbl().len();
    assert!(
      page_num >= 3,
      "应有 data + 多条 overflow 页，实际 {page_num}"
    );

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    store.delete(&tbl, b"big", 2).await.unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    assert!(after.len() > before.len(), "delete 必须走 append 增量");
    let record = after.get(before.len()..).expect("增量 record");

    let entry_li = log_mapping_entry_li(record);
    assert!(!entry_li.is_empty(), "delete 必须记录被释放页");
    // C++ `DeleteMapping` 的字节形状：`varint32(page_id) | varint64(7)`。
    // 纯 delete 负载中释放页保持空闲到 flush，操作时值与最终槽位一致
    // （活跃页写 FilePageId，其余一律 7），因此仍可按最终槽位推导期望值，
    // 不读空闲链的中间态编码，避免用被测实现自证。同批「释放后复用」的
    // 取值时机由 `overwrite_large_value_log_delta_matches_cpp_free_then_update`
    // 覆盖（sem-45）。
    let mapper = store.mapper(&tbl).unwrap();
    let mut expected = Vec::new();
    let mut freed_num = 0usize;
    for &(page_id, _) in &entry_li {
      let final_value = mapper.tbl().get(page_id).unwrap();
      let expected_value = if is_file_page_id(final_value) {
        final_value
      } else {
        freed_num += 1;
        INVALID_VALUE
      };
      append_varint32(&mut expected, page_id);
      append_varint64(&mut expected, expected_value);
    }
    assert!(freed_num >= 2, "delete 应释放多个逻辑页，实际 {freed_num}");
    assert_eq!(log_mapping(record), expected.as_slice());
  });
}

/// sem-45：同批「释放后复用」的页，释放项必须落 `InvalidValue(7)`，不能写
/// flush 时的最终 `FilePageId`。
///
/// 场景（issue 探针）：同一 key 用另一份 12 KiB value 覆盖。写路径先释放旧
/// overflow 链（`free_replaced_values`），再从 LIFO 空闲链分配新 overflow 页，
/// 同一逻辑页因此在本批「先释放、后分配」。C++ `WriteTask::FreePage` 在操作处
/// 追加 `DeleteMapping(page_id | 7)`，后续 `AllocatePage` 追加
/// `UpdateMapping(page_id | FilePageId)`；fkv 的 log delta 必须逐条一致。
#[test]
fn overwrite_large_value_log_delta_matches_cpp_free_then_update() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem45_overwrite", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let value_a: Vec<u8> = (0..12_000u32).map(|i| (i % 251) as u8).collect();
    store
      .put(&tbl, &[BatchEntry::upsert(b"big".to_vec(), value_a, 1)])
      .await
      .unwrap();
    // 覆盖前的活跃页集合：释放项必须是这些页之一。
    let live_before: Vec<(u32, u64)> = store
      .mapper(&tbl)
      .unwrap()
      .tbl()
      .iter()
      .filter(|(_, value)| is_file_page_id(*value))
      .collect();
    assert!(
      live_before.len() >= 3,
      "应有 data + 多条 overflow 页，实际 {}",
      live_before.len()
    );
    let before_tbl = store.mapper(&tbl).unwrap().tbl().clone();

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let value_b: Vec<u8> = (0..12_000u32).map(|i| (i % 241) as u8 ^ 0x5a).collect();
    store
      .put(&tbl, &[BatchEntry::upsert(b"big".to_vec(), value_b, 2)])
      .await
      .unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    assert!(after.len() > before.len(), "覆盖写必须走 append 增量");
    let record = after.get(before.len()..).expect("增量 record");
    let entry_li = log_mapping_entry_li(record);
    assert!(!entry_li.is_empty(), "覆盖写必须记录映射变化");

    let mapper = store.mapper(&tbl).unwrap();
    let mut released_li: Vec<u32> = Vec::new();
    let mut reused_num = 0usize;
    for &(page_id, value) in &entry_li {
      // 取值只能是两种操作语义：DeleteMapping(7) 或 UpdateMapping(FilePageId)。
      assert!(
        value == INVALID_VALUE || is_file_page_id(value),
        "page {page_id} 的 log 值既不是 7 也不是 FilePageId: {value}"
      );
      if value == INVALID_VALUE {
        // 释放项只能来自覆盖前的活跃页，且同一页在本批只释放一次。
        assert!(
          live_before.iter().any(|(page, _)| *page == page_id),
          "page {page_id} 被释放，但覆盖前不是活跃页"
        );
        assert!(
          !released_li.contains(&page_id),
          "page {page_id} 重复释放，不应出现在同一批"
        );
        released_li.push(page_id);
      } else {
        // 复用 / remap 项：写操作时的 FilePageId，且本负载中未被后续释放。
        assert_eq!(
          mapper.tbl().get(page_id).unwrap(),
          value,
          "page {page_id} 的 FilePageId 应与最终槽位一致"
        );
        reused_num += 1;
      }
    }
    assert!(
      released_li.len() >= 3,
      "覆盖 12 KiB 应释放多条 overflow 页，实际 {}",
      released_li.len()
    );
    assert!(reused_num >= 1, "释放逻辑页后必须在本批重新分配");

    // 同一 page id 先出现 7、后出现 FilePageId：逐页核对「释放 → 复用」序列。
    // 旧实现在 flush 时读最终槽值，首次出现也是 FilePageId —— 本断言即回归点。
    let mut released_then_reused = 0usize;
    for &page_id in &released_li {
      let idx_li: Vec<usize> = entry_li
        .iter()
        .enumerate()
        .filter(|(_, (entry_page, _))| *entry_page == page_id)
        .map(|(idx, _)| idx)
        .collect();
      assert_eq!(
        entry_li[idx_li[0]].1, INVALID_VALUE,
        "page {page_id} 的释放项必须是首次记录"
      );
      if idx_li.len() > 1 {
        released_then_reused += 1;
        assert!(is_file_page_id(entry_li[idx_li[1]].1));
      }
    }
    assert!(
      released_then_reused >= 3,
      "12 KiB 值覆盖后释放页应被本批复用，实际 {released_then_reused}"
    );

    // 按操作序重放 log 段：起始于覆盖前 mapping，结果必须等于覆盖后 mapping
    //（不丢映射变化，也不多写）。
    let mut replayed = before_tbl;
    let final_len = mapper.tbl().len();
    if replayed.len() < final_len {
      replayed.resize(final_len, INVALID_VALUE);
    }
    for &(page_id, value) in &entry_li {
      replayed.set(page_id, value);
    }
    assert_eq!(
      replayed.serialize(),
      mapper.tbl().serialize(),
      "log 段重放后必须与最终 mapping 逐字节一致"
    );
  });
}

/// sem-45：混合写批（delete + 大 value 覆盖）的 log 段同样按操作时取值，
/// 且对 batch 前后的 mapping 逐字节可重放。
#[test]
fn mixed_batch_log_delta_replays_to_final_mapping() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem45_mixed", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"a".to_vec(), vec![b'x'; 32], 1)],
      )
      .await
      .unwrap();
    let value_a: Vec<u8> = (0..12_000u32).map(|i| (i % 251) as u8).collect();
    store
      .put(&tbl, &[BatchEntry::upsert(b"big".to_vec(), value_a, 2)])
      .await
      .unwrap();

    let before_tbl = store.mapper(&tbl).unwrap().tbl().clone();
    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let value_b: Vec<u8> = (0..12_000u32).map(|i| (i % 239) as u8 ^ 0xa5).collect();
    store
      .put(
        &tbl,
        &[
          BatchEntry::delete(b"a".to_vec(), 3),
          BatchEntry::upsert(b"big".to_vec(), value_b, 3),
        ],
      )
      .await
      .unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    assert!(after.len() > before.len(), "混合批必须走 append 增量");
    let record = after.get(before.len()..).expect("增量 record");
    let entry_li = log_mapping_entry_li(record);
    assert!(!entry_li.is_empty(), "混合批必须记录映射变化");

    let mapper = store.mapper(&tbl).unwrap();
    for &(page_id, value) in &entry_li {
      assert!(
        value == INVALID_VALUE || is_file_page_id(value),
        "page {page_id} 的 log 值既不是 7 也不是 FilePageId: {value}"
      );
    }
    // 最终仍空闲的页，其最后一条日志必须是 DeleteMapping(7)。
    for &(page_id, _) in &entry_li {
      let final_value = mapper.tbl().get(page_id).unwrap();
      if !is_file_page_id(final_value) {
        let last = entry_li
          .iter()
          .rfind(|(entry_page, _)| *entry_page == page_id)
          .map(|(_, value)| *value)
          .unwrap();
        assert_eq!(
          last, INVALID_VALUE,
          "page {page_id} 最终空闲，最后一条日志必须是 7"
        );
      }
    }
    // 本批同时存在释放与写入：至少一条 7、至少一条 FilePageId。
    assert!(entry_li.iter().any(|(_, value)| *value == INVALID_VALUE));
    assert!(entry_li.iter().any(|(_, value)| is_file_page_id(*value)));

    // 重放：把 log 段按序应用到批前 mapping，必须得到批后 mapping。
    let mut replayed = before_tbl;
    let final_len = mapper.tbl().len();
    if replayed.len() < final_len {
      replayed.resize(final_len, INVALID_VALUE);
    }
    for &(page_id, value) in &entry_li {
      replayed.set(page_id, value);
    }
    assert_eq!(
      replayed.serialize(),
      mapper.tbl().serialize(),
      "log 段重放后必须与最终 mapping 逐字节一致"
    );
  });
}

/// test-19 / sem-42：segment mapping 的 manifest log 增量必须与 C++ 逐字节一致。
///
/// C++ `WriteTask::RecordSegmentMappingDelete`（write_task.cpp:716-720）对释放
/// 段固定写 `InvalidValue(7)`，分配段写 `UpdateMapping(page_id | FilePageId)`。
/// sem-46 的覆盖顺序是「同批先分配新段、再释放旧段」：旧段在 flush 时仍停在
/// segment mapper 的空闲链里，若实现重读 `tbl()` 的最终槽值，释放项就会落成
/// `encode_page_id(next)`。replay 的 `rebuild_free_page_li` 会把两种编码都归一
/// 为空闲槽，读写结果不变 —— 只有盘面字节断言有判别力。
#[test]
fn overwrite_large_value_segment_log_delta_matches_cpp_free_then_update() {
  pollster::block_on(async {
    let tbl = TableIdent::new("test19_segment_delta", 1).unwrap();
    let layout = SegmentLayout::new(4096, 1).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 1), 512, 16)
      .with_large_value(layout, 1024);
    let value_a: Vec<u8> = (0..20_000u32).map(|i| (i % 251) as u8).collect();
    store
      .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), value_a, 1)])
      .await
      .unwrap();
    let before_tbl = store.segment_mapper(&tbl).unwrap().tbl().clone();
    let live_before = before_tbl
      .iter_value_li()
      .filter(|value| is_file_page_id(*value))
      .count();
    assert_eq!(live_before, 5, "20 000 B / 4 KiB = 5 个 segment");
    assert_eq!(before_tbl.len(), live_before, "首写后不应有空闲 segment 槽");

    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let value_b: Vec<u8> = (0..20_000u32).map(|i| (i % 241) as u8 ^ 0x5a).collect();
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), value_b.clone(), 2)],
      )
      .await
      .unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    assert!(after.len() > before.len(), "覆盖写必须走 append 增量");
    let record = after.get(before.len()..).expect("增量 record");
    let entry_li = log_segment_entry_li(record);
    assert!(!entry_li.is_empty(), "覆盖写必须记录 segment 映射变化");

    let mapper = store.segment_mapper(&tbl).unwrap();
    // sem-46 顺序下本批「新段分配」与「旧段释放」的 PageId 不相交，每个逻辑页
    // 至多一条记录，因此可直接用最终槽位推导期望值，不读实现内部的
    // dirty_page_li 自证。
    let mut unique_li: Vec<u32> = entry_li.iter().map(|(page_id, _)| *page_id).collect();
    unique_li.sort_unstable();
    unique_li.dedup();
    assert_eq!(
      unique_li.len(),
      entry_li.len(),
      "分配先于释放，本场景同批 segment 逻辑页不应重复"
    );

    let mut expected = Vec::new();
    let mut freed_num = 0usize;
    let mut allocated_num = 0usize;
    for &(page_id, value) in &entry_li {
      let final_value = mapper.tbl().get(page_id).unwrap();
      let expected_value = if is_file_page_id(final_value) {
        allocated_num += 1;
        final_value
      } else {
        freed_num += 1;
        INVALID_VALUE
      };
      assert_eq!(
        value, expected_value,
        "page {page_id} 的 segment log 值应为 {expected_value}，实际 {value}"
      );
      append_varint32(&mut expected, page_id);
      append_varint64(&mut expected, expected_value);
    }
    assert!(
      freed_num >= 4,
      "覆盖 20 000 B 应释放多条 segment，实际 {freed_num}"
    );
    assert!(
      allocated_num >= 5,
      "同批应分配新 segment，实际 {allocated_num}"
    );
    assert_eq!(
      log_segment(record),
      expected.as_slice(),
      "segment log 段必须逐字节等于 C++ RecordSegmentMappingDelete/UpdateMapping"
    );

    // 重放：把 log 段按操作序应用到批前映射，再按 replay 的空闲链重建规则
    // 归一，结果必须与最终 mapping 逐字节一致（不丢、不多、不写错值）。
    let mut replayed = before_tbl;
    let mut free_head = None;
    for &(page_id, value) in &entry_li {
      if value == INVALID_VALUE {
        replayed.set(page_id, free_head.map_or(INVALID_VALUE, encode_page_id));
        free_head = Some(page_id);
      } else {
        replayed.set(page_id, value);
      }
    }
    assert_eq!(
      replayed.serialize(),
      mapper.tbl().serialize(),
      "segment log 段重放后必须与最终 segment 映射逐字节一致"
    );
    let final_len = mapper.tbl().len();
    let final_live_li: Vec<(u32, u64)> = (0..final_len as u32)
      .filter_map(|page_id| {
        let value = mapper.tbl().get(page_id).unwrap();
        is_file_page_id(value).then_some((page_id, value))
      })
      .collect();
    assert_eq!(final_live_li.len(), allocated_num);

    // 冷重开走真实 replay：活跃 segment 映射与覆盖后的值必须原样恢复。
    let io = store.into_io();
    let mut reopened = Store::new(io, 512, 16).with_large_value(layout, 1024);
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(
      reopened
        .get_large(&tbl, b"big")
        .await
        .unwrap()
        .unwrap()
        .value,
      value_b
    );
    let reopened_mapper = reopened.segment_mapper(&tbl).unwrap();
    assert_eq!(reopened_mapper.tbl().len(), final_len);
    assert_eq!(reopened_mapper.mapping_count(), allocated_num);
    let reopened_live_li: Vec<(u32, u64)> = (0..final_len as u32)
      .filter_map(|page_id| {
        let value = reopened_mapper.tbl().get(page_id).unwrap();
        is_file_page_id(value).then_some((page_id, value))
      })
      .collect();
    assert_eq!(reopened_live_li, final_live_li);
  });
}

/// sem-42：整表 truncate 的 snapshot mapping 段必须与 C++ `PageMapper::FreePage`
/// + `MappingSnapshot::Serialize` 的输出逐字节一致。
///
/// C++ 全量 truncate 按 PageId 升序 `FreePage`，链尾（页 0）写 `InvalidValue`，
/// 页 i > 0 写 `EncodePageId(i - 1)`，随后整份 snapshot 覆盖 manifest。
#[test]
fn truncate_snapshot_free_chain_matches_cpp_free_page() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem42_truncate", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    for i in 0..32u64 {
      store
        .put(
          &tbl,
          &[BatchEntry::upsert(
            format!("k{i:03}").into_bytes(),
            vec![b'v'; 64],
            i + 1,
          )],
        )
        .await
        .unwrap();
    }
    let page_num = store.mapper(&tbl).unwrap().tbl().len() as u32;
    assert!(page_num >= 3, "应分配多页，实际 {page_num}");
    // 纯 put 负载不应有已释放槽位：升序 free 的链形状因此是确定的。
    let live_num = store
      .mapper(&tbl)
      .unwrap()
      .tbl()
      .iter_value_li()
      .filter(|value| is_file_page_id(*value))
      .count();
    assert_eq!(live_num, page_num as usize, "truncate 前应全部为活跃页");

    store.truncate(&tbl).await.unwrap();
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
    assert_eq!(snapshot_mapping(&record), expected.as_slice());
  });
}

/// sem-56：已清空分区（两棵树均空）上的 no-op 批必须与 C++
/// `WriteTask::FlushManifest` 的空分区标记分支同形（write_task.cpp:726-733、
/// 769-787）—— wal 为空的批每批整写一份空快照（C++ `SwitchManifest` 等价），
/// manifest 恒为快照大小、内容逐字节不变、不随批数增长。既不得早退零写入
/// （0 条目批），也不得追加「仅含 branch 元数据」的空 delta record（对不存在
/// key 的纯 delete 批：折叠后 `live_li` 非空，但空树 merge 产出零页）。
#[test]
fn noop_batch_on_cleared_partition_rewrites_empty_snapshot() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem56_noop", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    for i in 0..3u64 {
      store
        .put(
          &tbl,
          &[BatchEntry::upsert(
            format!("k{i}").into_bytes(),
            vec![b'v'; 8],
            i + 1,
          )],
        )
        .await
        .unwrap();
    }
    for i in 0..3u64 {
      store
        .delete(&tbl, format!("k{i}").as_bytes(), 100 + i)
        .await
        .unwrap();
    }
    // 清空批 wal 非空（释放页的 mapping delta）：与 C++ 一致走 append。
    assert_eq!(store.page_io().manifest_switch_num(), 1, "首写 snapshot");
    let appends_after_clear = store.page_io().manifest_append_num();
    assert!(appends_after_clear >= 1, "清空批必须走 append 记录释放页");

    // 0 条目批：每批整写空快照（switch +1、append 不变、字节不变）。
    store.put(&tbl, &[]).await.unwrap();
    let snapshot_bytes = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    assert_eq!(
      store.page_io().manifest_switch_num(),
      2,
      "空批必须整写空快照（SwitchManifest 等价）"
    );
    assert_eq!(
      store.page_io().manifest_append_num(),
      appends_after_clear,
      "空批不得追加 delta record"
    );
    let header = parse_header(&snapshot_bytes).expect("snapshot header");
    assert_eq!(header.root, NULL_PAGE, "空快照 root 必须为空");
    assert_eq!(header.ttl_root, NULL_PAGE, "空快照 ttl root 必须为空");
    for round in 1..3u64 {
      store.put(&tbl, &[]).await.unwrap();
      let bytes = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
      assert_eq!(
        store.page_io().manifest_switch_num(),
        round + 2,
        "空批必须整写空快照"
      );
      assert_eq!(
        store.page_io().manifest_append_num(),
        appends_after_clear,
        "空批不得追加 delta record"
      );
      assert_eq!(bytes, snapshot_bytes, "每批快照逐字节一致，不随批数增长");
    }

    // 对不存在 key 的纯 delete 批：同样整写空快照，不追加空 delta record。
    for round in 0..3u64 {
      store.delete(&tbl, b"ghost", 200 + round).await.unwrap();
      let bytes = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
      assert_eq!(
        store.page_io().manifest_switch_num(),
        round + 5,
        "无效果 delete 批必须整写空快照"
      );
      assert_eq!(
        store.page_io().manifest_append_num(),
        appends_after_clear,
        "无效果 delete 批不得追加空 delta record"
      );
      assert_eq!(bytes, snapshot_bytes, "无效果批的空快照逐字节一致");
    }

    // reopen：空快照回放得空树，旧 key 与 ghost 均不可读。
    let io = store.into_io();
    let mut reopened = Store::new(io, 512, 16);
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(reopened.scan(&tbl, b"", b"", 0).await.unwrap().len(), 0);
    assert_eq!(reopened.get(&tbl, b"k0").await.unwrap(), None);
    assert_eq!(reopened.get(&tbl, b"ghost").await.unwrap(), None);
  });
}

/// sem-56 反向对照：非空分区上的同批不触发重写 —— 0 条目批零写入（C++ wal 空
/// 且 roots 非空同样直接返回，write_task.cpp:730-733）；对不存在 key 的纯
/// delete 批在非空分区仍无条件重写覆盖页并 append（sem-47/48 既有行为）。
#[test]
fn noop_batch_on_nonempty_partition_keeps_manifest() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem56_nonempty", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    store
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), vec![b'v'; 8], 1)])
      .await
      .unwrap();
    let switches = store.page_io().manifest_switch_num();
    let appends = store.page_io().manifest_append_num();
    let bytes = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();

    // 0 条目批：分区非空（root 非空），与 C++ 一致不做任何 manifest 写入。
    store.put(&tbl, &[]).await.unwrap();
    assert_eq!(store.page_io().manifest_switch_num(), switches);
    assert_eq!(store.page_io().manifest_append_num(), appends);
    assert_eq!(
      store.page_io().read_manifest(&tbl).await.unwrap().unwrap(),
      bytes,
      "非空分区上的空批不得触碰 manifest"
    );

    // 对不存在 key 的 delete：覆盖页被无条件重写（C++ `ApplyOnePage`），走 append。
    store.delete(&tbl, b"ghost", 2).await.unwrap();
    assert_eq!(store.page_io().manifest_switch_num(), switches);
    assert_eq!(
      store.page_io().manifest_append_num(),
      appends + 1,
      "非空分区的纯 delete 批仍重写覆盖页并 append"
    );
    assert!(
      store
        .page_io()
        .read_manifest(&tbl)
        .await
        .unwrap()
        .unwrap()
        .len()
        > bytes.len(),
      "覆盖页重写必须落 mapping delta"
    );
  });
}

/// sem-57：从未写入（尚无 manifest）分区上的无效果批不得凭空创建 manifest ——
/// C++ `FlushManifest` 对 wal 空且 `manifest_size_ == 0` 的批直接返回
/// （write_task.cpp:726-733），manifest 永不创建；首份快照只由真实写批创建
/// （wal 非空时走 `SwitchManifest` 全量快照，write_task.cpp:827-844）。三种
/// 无效果形态同锁：对不存在 key 的纯 delete、0 条目批、全过期折叠 delete。
#[test]
fn noop_batch_on_never_written_partition_creates_no_manifest() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem57_noop", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let mut expired = BatchEntry::upsert(b"exp".to_vec(), vec![b'v'; 8], 1);
    expired.expire_ts = 1;

    // 对不存在 key 的纯 delete：折叠后 live_li 非空，但空树 merge 产出零页。
    store.delete(&tbl, b"ghost", 1).await.unwrap();
    // 0 条目批与全过期折叠 delete 批同形。
    store.put(&tbl, &[]).await.unwrap();
    store.put(&tbl, from_ref(&expired)).await.unwrap();

    assert_eq!(
      store.page_io().manifest_switch_num(),
      0,
      "无效果批不得创建 manifest（C++ manifest_size_ == 0）"
    );
    assert_eq!(store.page_io().manifest_append_num(), 0);
    assert_eq!(
      store.page_io().read_manifest(&tbl).await.unwrap(),
      None,
      "manifest 文件不得存在"
    );
    assert!(
      store
        .page_io()
        .list_manifest_file_name_li(&tbl)
        .await
        .unwrap()
        .is_empty(),
      "manifest 对象集必须为空"
    );
    assert_eq!(
      store.scan(&tbl, b"", b"", 0).await.unwrap().len(),
      0,
      "无效果批不得产生可见数据"
    );

    // 无 manifest 分区的 reopen 语义不变：仍按 ResourceMissing 拒绝
    // （与从未触碰的分区一致）。
    assert!(
      matches!(store.reopen(&tbl).await, Err(KvError::ResourceMissing)),
      "无 manifest 分区的 reopen 语义不得改变"
    );
  });
}

/// sem-57 后半：无效果批只是不创建 manifest，不吞真实写批 —— 首个有效写批
/// 照常走 `SwitchManifest` 全量快照建 manifest，数据可读、reopen 完整恢复。
#[test]
fn first_effective_write_after_noop_batches_creates_manifest() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem57_first_write", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let mut expired = BatchEntry::upsert(b"exp".to_vec(), vec![b'v'; 8], 1);
    expired.expire_ts = 1;
    store.delete(&tbl, b"ghost", 1).await.unwrap();
    store.put(&tbl, from_ref(&expired)).await.unwrap();
    assert_eq!(store.page_io().manifest_switch_num(), 0);

    // 首个有效写批：manifest 正常创建，且快照只含该批数据。
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"real".to_vec(), vec![b'v'; 8], 2)],
      )
      .await
      .unwrap();
    assert_eq!(
      store.page_io().manifest_switch_num(),
      1,
      "首份 manifest 由真实写批创建"
    );
    let bytes = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let header = parse_header(&bytes).expect("snapshot header");
    assert_ne!(header.root, NULL_PAGE, "首份快照必须携带真实数据树");
    assert_eq!(store.get(&tbl, b"real").await.unwrap(), Some(vec![b'v'; 8]));
    assert_eq!(store.get(&tbl, b"ghost").await.unwrap(), None);
    assert_eq!(store.get(&tbl, b"exp").await.unwrap(), None);

    // reopen：manifest 存在后语义不变，真实数据完整恢复。
    let io = store.into_io();
    let mut reopened = Store::new(io, 512, 16);
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(
      reopened.get(&tbl, b"real").await.unwrap(),
      Some(vec![b'v'; 8])
    );
    assert_eq!(reopened.scan(&tbl, b"", b"", 0).await.unwrap().len(), 1);
  });
}

/// sem-59：清空分区经一次维护（file_gc + `cleanup_empty_partition`）后，活跃
/// manifest 必须保留 —— C++ GC 收尾（`ExecuteLocalGC` 第 5 步 →
/// `TryCleanupLocalPartitionDir`）实质是一次 `rmdir(2)`，manifest 文件挡住
/// rmdir 时按 `-ENOTEMPTY` 成功吞掉；全仓删 manifest 的唯一生产调用点是显式
/// Drop（`batch_write_task.cpp:1896`）。manifest 保留 ⇒ `manifest_size_ > 0`
/// 状态不复位 ⇒ 其后 no-op 批仍走 sem-56 的空快照整写（`need_empty_snapshot`，
/// write_task.cpp:724-733）、首个真实写批走 append（write_task.cpp:806-810），
/// 「清空 + 维护」路径不得旁路 sem-56 的对齐结果。
#[test]
fn maintain_keeps_manifest_on_cleared_partition_noop_still_snapshots() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem59_cleanup", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    for i in 0..3u64 {
      store
        .put(
          &tbl,
          &[BatchEntry::upsert(
            format!("k{i}").into_bytes(),
            vec![b'v'; 8],
            i + 1,
          )],
        )
        .await
        .unwrap();
    }
    for i in 0..3u64 {
      store
        .delete(&tbl, format!("k{i}").as_bytes(), 100 + i)
        .await
        .unwrap();
    }
    assert_eq!(store.page_io().manifest_switch_num(), 1, "首写 snapshot");
    let appends_after_clear = store.page_io().manifest_append_num();
    assert!(appends_after_clear >= 1, "清空批必须走 append 记录释放页");
    let bytes_before = store
      .page_io()
      .read_manifest(&tbl)
      .await
      .unwrap()
      .expect("维护前 manifest 必须存在");

    // 一次写后维护：GC 删光 data 文件，`cleanup_empty_partition` 收尾。
    // manifest 保留（对齐 C++ GC 收尾），计数与文件内容都不被收尾改动。
    store.maintain(&tbl).await.unwrap();
    let bytes_after = store
      .page_io()
      .read_manifest(&tbl)
      .await
      .unwrap()
      .expect("维护收尾后 manifest 必须保留（sem-59）");
    assert_eq!(bytes_after, bytes_before, "维护收尾不得改写 manifest 字节");
    assert_eq!(store.page_io().manifest_switch_num(), 1);
    assert_eq!(store.page_io().manifest_append_num(), appends_after_clear);

    // 维护后的 no-op 批：sem-56 行为不被旁路 —— 整写空快照。
    store.put(&tbl, &[]).await.unwrap();
    let empty_snapshot = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    assert_eq!(
      store.page_io().manifest_switch_num(),
      2,
      "维护后空批必须整写空快照（need_empty_snapshot：manifest_size_ > 0）"
    );
    assert_eq!(
      store.page_io().manifest_append_num(),
      appends_after_clear,
      "空批不得追加 delta record"
    );
    let header = parse_header(&empty_snapshot).expect("snapshot header");
    assert_eq!(header.root, NULL_PAGE, "空快照 root 必须为空");
    assert_eq!(header.ttl_root, NULL_PAGE, "空快照 ttl root 必须为空");

    // 重复 no-op 批：快照逐字节一致，不随批数增长（C++ 每批 PUT 同一内容）。
    store.put(&tbl, &[]).await.unwrap();
    assert_eq!(store.page_io().manifest_switch_num(), 3);
    assert_eq!(
      store.page_io().read_manifest(&tbl).await.unwrap().unwrap(),
      empty_snapshot,
      "空批的空快照逐字节一致"
    );

    // 维护后的首个真实写批：走 append（manifest 未删），不重建对象。
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"new".to_vec(), vec![b'v'; 8], 200)],
      )
      .await
      .unwrap();
    assert_eq!(
      store.page_io().manifest_switch_num(),
      3,
      "真实写批不得重建 manifest（C++ manifest 未删走 append）"
    );
    assert_eq!(
      store.page_io().manifest_append_num(),
      appends_after_clear + 1,
      "真实写批追加 delta record"
    );
    assert_ne!(
      store.page_io().read_manifest(&tbl).await.unwrap().unwrap(),
      empty_snapshot
    );

    // reopen：manifest 是「空快照 + new 的 delta」，旧 key 不可见、new 恢复。
    let io = store.into_io();
    let mut reopened = Store::new(io, 512, 16);
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(reopened.scan(&tbl, b"", b"", 0).await.unwrap().len(), 1);
    assert_eq!(
      reopened.get(&tbl, b"new").await.unwrap(),
      Some(vec![b'v'; 8])
    );
    assert_eq!(reopened.get(&tbl, b"k0").await.unwrap(), None);
  });
}
