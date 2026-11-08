//! pooled-03：data 侧自动放大率 compaction 只在 `Append` 分配模式生效。
//!
//! Pooled 的 `space_size()` 退化为 `max_fp_id`，且 `update_stat` /
//! `skip_to_next_file` 对它都是 no-op：若不 gate，清空 / 大量删除后
//! `needs_maintain` 恒为真，每次写都会跑全量 maintain（data compact +
//! file_gc + 空分区清理）。对齐 C++：data 侧放大率 compaction 只在
//! `data_append_mode` 下评估（`WriteTask::UpdateMeta` /
//! `BackgroundWrite::Compact` 跳过 `DoCompactDataFile`）。
//!
//! 段侧不走该 gate（段 mapper 恒为 `Append`，见 pooled-02）；本文件只覆盖
//! data 侧判定与写后维护调度，避免把两种分配模式的口径混在一起。

use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_store::{AllocMode, Store};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
/// 每文件 16 页：2000 次写 / 删除足以跨多个文件并触发放大率阈值。
const SHIFT: u8 = 4;

fn batch(lo: u32, hi: u32, ts: u64) -> Vec<BatchEntry> {
  (lo..hi)
    .map(|i| BatchEntry::upsert(format!("k{i:05}").into_bytes(), vec![b'v'; 30], ts))
    .collect()
}

fn new_mem_store(mode: AllocMode) -> Store<MemStore> {
  Store::new(
    MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
    PAGE_SIZE,
    16,
  )
  .with_pages_per_file_shift(SHIFT)
  .with_alloc_mode(mode)
  .with_file_amplify_factor(2)
}

/// 2000 次写后删除 95%，制造 `space > mapped * factor` 的放大 mapping。
async fn fill_then_delete(store: &mut Store<MemStore>, tbl: &TableIdent) {
  for chunk in 0..8u32 {
    store
      .put(tbl, &batch(chunk * 250, (chunk + 1) * 250, 1))
      .await
      .unwrap();
  }
  for i in 0..1900u32 {
    store
      .delete(tbl, format!("k{i:05}").as_bytes(), 2)
      .await
      .unwrap();
  }
}

/// 2000 次写后按「隔 200 留 1」删除：live 页稀疏散布在多个文件，
/// Pooled 下旧口径同样能选出放大文件（用于证明 gate 生效）。
async fn fill_then_delete_scattered(store: &mut Store<MemStore>, tbl: &TableIdent) {
  for chunk in 0..8u32 {
    store
      .put(tbl, &batch(chunk * 250, (chunk + 1) * 250, 1))
      .await
      .unwrap();
  }
  for i in 0..2000u32 {
    if i % 200 != 0 {
      store
        .delete(tbl, format!("k{i:05}").as_bytes(), 2)
        .await
        .unwrap();
    }
  }
}

/// Pooled + 大量删除后连续小写：写后不得再按放大率触发维护。
///
/// 前置断言 `space > mapped * factor` 用来确认旧口径确实会命中——修复前
/// 该条件使 `needs_maintain` 恒真，每次 put 后都会跑全量 maintain。
#[test]
fn pooled_heavy_delete_does_not_trigger_amplification_maintain() {
  pollster::block_on(async {
    let tbl = TableIdent::new("pooled_gate", 1).unwrap();
    let mut store = new_mem_store(AllocMode::Pooled);
    fill_then_delete(&mut store, &tbl).await;
    let mapper = store.mapper(&tbl).expect("table mapper");
    let mapped = mapper.mapping_count() as u64;
    let space = mapper.space_size(SHIFT);
    assert!(
      space > mapped * 2,
      "前置：Pooled 的 space_size 已超放大阈值 space={space} mapped={mapped}"
    );

    for round in 0..5u64 {
      store
        .put(&tbl, &batch(1900, 1910, 3 + round))
        .await
        .unwrap();
      assert!(
        !store.needs_maintain(&tbl),
        "Pooled 不得按 data 放大率触发维护 round={round}"
      );
      assert!(
        store.compact_if_needed(&tbl).await.unwrap().is_empty(),
        "Pooled 不得选出放大文件 round={round}"
      );
    }
  });
}

/// 防回归：Append 下放大率仍会触发维护，且一轮 `maintain` 后回落为 false。
#[test]
fn append_amplification_maintain_unchanged() {
  pollster::block_on(async {
    let tbl = TableIdent::new("pooled_gate", 3).unwrap();
    let mut store = new_mem_store(AllocMode::Append);
    fill_then_delete(&mut store, &tbl).await;
    assert!(
      store.needs_maintain(&tbl),
      "Append 大量删除后必须按放大率触发维护"
    );
    assert!(
      !store.compact_if_needed(&tbl).await.unwrap().is_empty(),
      "Append 必须选出放大文件"
    );

    store.maintain(&tbl).await.unwrap();
    assert!(!store.needs_maintain(&tbl), "一轮维护后必须回落");
    assert!(
      store.compact_if_needed(&tbl).await.unwrap().is_empty(),
      "没有新写时不得空转 compact"
    );

    // 回落后的第一批小写仍不触发（阈值按新的稠密 mapping 重新累计）。
    store.put(&tbl, &batch(1900, 1910, 3)).await.unwrap();
    assert!(!store.needs_maintain(&tbl), "回落后的首批小写不应立即触发");
  });
}

/// 计数断言：Pooled 下写后维护不再被调度，目录扫描计数不随写轮数增长。
///
/// 判定用 `CompioStore::path_scan_num`（强制 / 增量 `getdents` 的累计次数）：
/// 修复前 `needs_maintain` 恒真，每次写后 `maintain` 都会强制列举 data /
/// segment 目录；修复后写轮次不再产生任何目录扫描。
#[compio::test]
async fn pooled_puts_do_not_scan_directories() {
  let root = TempRoot::new("pooled_maintain_gate");
  let tbl = TableIdent::new("pooled_gate", 4).unwrap();
  let mut store = Store::new(
    CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT),
    PAGE_SIZE,
    16,
  )
  .with_pages_per_file_shift(SHIFT)
  .with_alloc_mode(AllocMode::Pooled)
  .with_file_amplify_factor(2);

  for chunk in 0..8u32 {
    store
      .put(&tbl, &batch(chunk * 250, (chunk + 1) * 250, 1))
      .await
      .unwrap();
  }
  for i in 0..1900u32 {
    store
      .delete(&tbl, format!("k{i:05}").as_bytes(), 2)
      .await
      .unwrap();
  }
  assert!(!store.needs_maintain(&tbl));

  let before = store.page_io().path_scan_num();
  let mut maintain_num = 0usize;
  for round in 0..5u64 {
    store
      .put(&tbl, &batch(1900, 1910, 3 + round))
      .await
      .unwrap();
    // 与 shard 写后钩子同构：只有 needs_maintain 时才跑 maintain。
    if store.needs_maintain(&tbl) || store.needs_segment_maintain(&tbl) {
      maintain_num += 1;
      store.maintain(&tbl).await.unwrap();
    }
  }
  assert_eq!(maintain_num, 0, "Pooled 写后不得再调度全量 maintain");
  assert_eq!(
    store.page_io().path_scan_num(),
    before,
    "Pooled 写后不得强制列举 data / segment 目录"
  );
}

/// Pooled 下 `create_branch` 的 pre-compact 不得搬 data 页。
///
/// 对齐 C++ `BackgroundWrite::Compact`：data 侧 `DoCompactDataFile` 只在
/// `data_append_mode` 下执行。用 `MemStore::read_num` 探测搬迁读页：branch
/// 继承 mapping 与末尾 `file_gc` 都不读 data page。
#[test]
fn pooled_create_branch_skips_data_compaction() {
  pollster::block_on(async {
    let tbl = TableIdent::new("pooled_gate", 5).unwrap();
    let mut store = new_mem_store(AllocMode::Pooled);
    fill_then_delete_scattered(&mut store, &tbl).await;
    // 前置：旧口径下 data compact 计划非空（对照用例已覆盖同构 mapping）。
    assert!(!store.needs_maintain(&tbl));

    let before = store.page_io().read_num();
    store.create_branch(&tbl, "feature").await.unwrap();
    assert_eq!(
      store.page_io().read_num(),
      before,
      "Pooled 下 create_branch 不得搬迁 data 页"
    );
  });
}

/// 防回归：Append 下 `create_branch` 仍按放大率搬 data 页。
#[test]
fn append_create_branch_still_compacts_data() {
  pollster::block_on(async {
    let tbl = TableIdent::new("pooled_gate", 6).unwrap();
    let mut store = new_mem_store(AllocMode::Append);
    fill_then_delete_scattered(&mut store, &tbl).await;

    let before = store.page_io().read_num();
    store.create_branch(&tbl, "feature").await.unwrap();
    assert!(
      store.page_io().read_num() > before,
      "Append 下 create_branch 必须按放大率搬迁 data 页"
    );
  });
}

fn pattern(len: usize, seed: u8) -> Vec<u8> {
  (0..len)
    .map(|index| (index as u8).wrapping_add(seed))
    .collect()
}

/// Pooled data + Append 段：segment 放大率照常生效并可回落，不因 data 侧
/// gate 而失效（pooled-02：段 mapper 恒为 `Append`，与 `alloc_mode` 无关）。
#[test]
fn pooled_segment_maintain_falls_back_after_compaction() {
  pollster::block_on(async {
    let tbl = TableIdent::new("pooled_gate", 7).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(
      MemStore::with_pages_per_file(PAGE_SIZE, SHIFT)
        .with_segment_config(layout.segment_size() as usize, 2),
      PAGE_SIZE,
      16,
    )
    .with_pages_per_file_shift(SHIFT)
    .with_alloc_mode(AllocMode::Pooled)
    .with_large_value(layout, 1024)
    .with_segment_amplify_factor(2);

    // 常驻 key 的段留在 file 0，压缩才有可搬的 live 段。
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"keep".to_vec(), pattern(9000, 9), 1)],
      )
      .await
      .unwrap();
    for round in 0..12u32 {
      store
        .put_large(
          &tbl,
          &[BatchEntry::upsert(
            b"big".to_vec(),
            pattern(9000 + round as usize, round as u8),
            u64::from(round) + 1,
          )],
        )
        .await
        .unwrap();
    }
    assert!(!store.needs_maintain(&tbl), "Pooled data 侧不按放大率触发");
    assert!(
      store.needs_segment_maintain(&tbl),
      "段侧必须按 segment 放大率触发（不受 data alloc_mode 门控）"
    );

    let mut rounds = 0;
    while store.needs_segment_maintain(&tbl) {
      store.maintain(&tbl).await.unwrap();
      rounds += 1;
      assert!(rounds < 10, "段侧维护必须收敛，不得恒真");
    }
    assert!(
      !store.needs_segment_maintain(&tbl),
      "段侧维护后触发点必须回落"
    );
    // 数据在维护后仍完整。
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      pattern(9000 + 11, 11)
    );
  });
}
