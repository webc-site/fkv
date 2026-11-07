//! sem-61 Form A：compact 的搬迁文件顺序按文件号升序。
//!
//! C++ `DoCompactDataFile` / `DoCompactSegmentFile` 都是
//! `for (file_id = begin_file_id; file_id < end_file_id; ++file_id)` 升序循环
//! （background_write.cpp:232 / :409），append 分配器按搬迁顺序发新
//! `FilePageId`，manifest record 内 delta 按操作序记录（sem-45）——搬迁顺序
//! 不同则新页号与 record 形态都不同。fkv 旧口径按放大率降序搬迁；本文件
//! 构造「放大率与文件号逆序」的双超阈值状态，锁定升序口径：计划升序、
//! record 内 delta 按旧文件号升序、第 k 条搬迁拿到第 k 个顺序新号
//! （「C++ 升序模型」的逐位手推期望值）。

use fkv_btree::BatchEntry;
use fkv_gc::{compact_plan, file_live_count};
use fkv_io::{FileIo, ManifestIo};
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_mapping::MappingTbl;
use fkv_store::{AllocMode, Store};
use fkv_task_reopen::split_manifest;
use fkv_types::{FileId, PageId, TableIdent, ValType, decode_id, is_file_page_id, val_type};

mod common;

use common::{log_mapping_entry_li, log_segment_entry_li};

const PAGE_SIZE: usize = 512;
const SHIFT: u8 = 4;
const FACTOR: u8 = 2;

fn key(index: u32) -> Vec<u8> {
  format!("k{index:05}").into_bytes()
}

fn upsert_batch(lo: u32, hi: u32, ts: u64) -> Vec<BatchEntry> {
  (lo..hi)
    .map(|i| BatchEntry::upsert(key(i), vec![b'v'; 30], ts))
    .collect()
}

fn delete_batch(lo: u32, hi: u32, ts: u64) -> Vec<BatchEntry> {
  (lo..hi).map(|i| BatchEntry::delete(key(i), ts)).collect()
}

fn pattern(len: usize, seed: u8) -> Vec<u8> {
  (0..len)
    .map(|index| (index as u8).wrapping_add(seed))
    .collect()
}

/// 快照 mapper 的「逻辑页 → 物理 fp」（只取活跃 `FilePageId` 槽位）。
fn live_fp_snapshot(table: &MappingTbl) -> Vec<(PageId, u64)> {
  (0..table.len() as PageId)
    .filter_map(|page_id| {
      let value = table.get(page_id).ok()?;
      is_file_page_id(value).then(|| (page_id, decode_id(value)))
    })
    .collect()
}

/// 最后一条 record 的 data mapping delta。
fn last_record_data_delta(bytes: &[u8]) -> Vec<(PageId, u64)> {
  log_mapping_entry_li(split_manifest(bytes).unwrap().pop().unwrap())
}

/// 最后一条 record 的 segment delta。
fn last_record_segment_delta(bytes: &[u8]) -> Vec<(PageId, u64)> {
  log_segment_entry_li(split_manifest(bytes).unwrap().pop().unwrap())
}

fn old_file_of(snapshot: &[(PageId, u64)], page_id: PageId, shift: u8) -> u64 {
  snapshot
    .iter()
    .find(|entry| entry.0 == page_id)
    .map_or(u64::MAX, |entry| entry.1 >> shift)
}

/// data 侧：两个超阈值文件、放大率与文件号逆序（低文件 live 多、高文件
/// live 少），搬迁必须按文件号升序，record 内 delta 与新 fp 发号随之对齐。
#[test]
fn data_compact_moves_files_in_ascending_order() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem61_data", 1).unwrap();
    let mut store = Store::new(
      MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
      PAGE_SIZE,
      16,
    )
    .with_pages_per_file_shift(SHIFT)
    .with_alloc_mode(AllocMode::Append)
    .with_file_amplify_factor(FACTOR);

    // 600 个键顺序写入：叶页按写序分配，fp 随之递增。
    for chunk in 0..6u32 {
      store
        .put(&tbl, &upsert_batch(chunk * 100, (chunk + 1) * 100, 1))
        .await
        .unwrap();
    }
    // 保留 k00000..k00059（多页，落在低文件、放大率低）与 k00300..k00347
    // （少页，落在高文件、放大率高），其余批删：放大率与文件号逆序。
    // 删除批的边界叶与根路径会重写至分配器尾（尾文件不入计划），两组的
    // 内侧叶保持原 fp 不动。
    store.put(&tbl, &delete_batch(60, 300, 2)).await.unwrap();
    store.put(&tbl, &delete_batch(348, 600, 2)).await.unwrap();

    // 前置：按同一口径重算计划，必须含 ≥2 个文件，且存在「文件号更小、
    // live 更多」的逆序对——否则旧口径（放大率降序）与升序不可区分。
    let live_map = {
      let mapper = store.mapper(&tbl).unwrap();
      file_live_count(mapper.tbl(), SHIFT)
    };
    let candidate_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
    let base = store.mapper(&tbl).unwrap().max_fp_id();
    let plan_expected: Vec<FileId> = compact_plan(
      &live_map,
      &candidate_li,
      1u64 << SHIFT,
      FACTOR,
      base >> SHIFT,
    );
    let live_of = |file_id: FileId| live_map.get(&file_id).copied().unwrap_or(0);
    assert!(
      plan_expected.len() >= 2,
      "前置：计划须含多个超阈值文件 live={live_map:?} candidate={candidate_li:?}"
    );
    assert!(
      plan_expected.iter().any(|lo| {
        plan_expected
          .iter()
          .any(|hi| lo < hi && live_of(*lo) > live_of(*hi))
      }),
      "前置：须存在放大率与文件号逆序的文件对 live={live_map:?} plan={plan_expected:?}"
    );

    let old_fp_li = live_fp_snapshot(store.mapper(&tbl).unwrap().tbl());
    let moved_total: u64 = plan_expected.iter().map(|f| live_of(*f)).sum();

    let appends = store.page_io().manifest_append_num();
    let plan = store.compact_if_needed(&tbl).await.unwrap();
    assert_eq!(
      plan, plan_expected,
      "搬迁计划必须按文件号升序（C++ DoCompactDataFile 升序循环）"
    );
    assert!(
      plan.windows(2).all(|pair| pair[0] < pair[1]),
      "计划必须严格按文件号升序: {plan:?}"
    );
    assert_eq!(
      store.page_io().manifest_append_num(),
      appends + 1,
      "一轮 compact 只追加一条 record"
    );

    let bytes = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let delta = last_record_data_delta(&bytes);
    assert_eq!(
      delta.len(),
      moved_total as usize,
      "delta 条数必须等于搬迁页数"
    );
    for (index, &(page_id, value)) in delta.iter().enumerate() {
      assert_eq!(val_type(value), ValType::FilePageId);
      assert_eq!(
        decode_id(value),
        base + index as u64,
        "第 {index} 条搬迁（page={page_id}）的新 fp 必须为 base+index"
      );
    }
    let old_file_li: Vec<u64> = delta
      .iter()
      .map(|&(page_id, _)| old_file_of(&old_fp_li, page_id, SHIFT))
      .collect();
    assert!(
      old_file_li.windows(2).all(|pair| pair[0] <= pair[1]),
      "record 内 delta 必须按旧文件号升序: {old_file_li:?}"
    );

    // 搬迁后数据仍完整。
    for i in (0..60u32).step_by(7) {
      assert_eq!(
        store.get(&tbl, key(i).as_slice()).await.unwrap(),
        Some(vec![b'v'; 30])
      );
    }
    for i in (300..348u32).step_by(11) {
      assert_eq!(
        store.get(&tbl, key(i).as_slice()).await.unwrap(),
        Some(vec![b'v'; 30])
      );
    }
  });
}

/// segment 侧：段 fp 可逐段手推（每段 4096B、每文件 4 段）。
/// keep 9000B → 段 0..2（file 0，3 段 live，放大率 4/3x）；big 覆盖 v1/v2/v3
/// 制造死段；v4 12289B → 段 11..14（live），分配器下一号 15 → 尾文件 3。
/// file 0（live 3，放大率低）与 file 2（live 1 = 段 11，放大率 4x）逆序。
#[test]
fn segment_compact_moves_files_in_ascending_order() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem61_seg", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(
      MemStore::new(PAGE_SIZE).with_segment_config(layout.segment_size() as usize, 2),
      PAGE_SIZE,
      16,
    )
    .with_large_value(layout, 1024)
    .with_segment_amplify_factor(1);

    let keep = pattern(9000, 9);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"keep".to_vec(), keep.clone(), 1)],
      )
      .await
      .unwrap();
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), pattern(9000, 0), 1)],
      )
      .await
      .unwrap();
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), pattern(9000, 1), 2)],
      )
      .await
      .unwrap();
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), pattern(4100, 2), 3)],
      )
      .await
      .unwrap();
    let big_last = pattern(12289, 3);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), big_last.clone(), 4)],
      )
      .await
      .unwrap();

    let base = store.segment_mapper(&tbl).unwrap().max_fp_id();
    assert_eq!(base, 15, "段分配器推进必须可手推：下一号为 15");
    let old_seg_fp_li = live_fp_snapshot(store.segment_mapper(&tbl).unwrap().tbl());

    let plan = store.compact_segment_if_needed(&tbl).await.unwrap();
    assert_eq!(
      plan,
      vec![0, 2],
      "segment 计划必须按文件号升序（C++ DoCompactSegmentFile 升序循环）"
    );

    let bytes = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let delta = last_record_segment_delta(&bytes);
    assert_eq!(delta.len(), 4, "搬迁段数必须为 3（file 0）+ 1（file 2）");
    let old_file_li: Vec<u64> = delta
      .iter()
      .map(|&(page_id, _)| old_file_of(&old_seg_fp_li, page_id, 2))
      .collect();
    assert_eq!(
      old_file_li,
      vec![0, 0, 0, 2],
      "delta 必须先 file 0 后 file 2"
    );
    for (index, &(_, value)) in delta.iter().enumerate() {
      assert_eq!(val_type(value), ValType::FilePageId);
      assert_eq!(
        decode_id(value),
        base + index as u64,
        "第 {index} 条搬迁的新段 fp 必须为 base+index"
      );
    }

    // 搬迁后大 value 仍完整。
    assert_eq!(
      store.get_large(&tbl, b"keep").await.unwrap().unwrap().value,
      keep
    );
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      big_last
    );
  });
}
