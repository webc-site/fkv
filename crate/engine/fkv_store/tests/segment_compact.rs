//! sto-56b：segment 文件压缩与回收。
//!
//! 覆盖两条闭环：
//! 1. 大 value 反复覆盖后，按 `segment_file_amplify_factor` 搬走 live 段，
//!    GC 删掉空文件，文件数回落到 live 段数；
//! 2. 旧 segment 文件仍被其它 branch manifest 引用时，压缩 / GC 都不得删除。

use fkv_btree::BatchEntry;
use fkv_io::FileIo;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_store::Store;
use fkv_types::TableIdent;

fn pattern(len: usize, seed: u8) -> Vec<u8> {
  (0..len)
    .map(|index| (index as u8).wrapping_add(seed))
    .collect()
}

fn mem_store(layout: SegmentLayout) -> Store<MemStore> {
  Store::new(
    MemStore::new(512).with_segment_config(layout.segment_size() as usize, 2),
    512,
    16,
  )
  .with_large_value(layout, 1024)
}

fn mem_store_from(io: MemStore, layout: SegmentLayout) -> Store<MemStore> {
  Store::new(io, 512, 16).with_large_value(layout, 1024)
}

/// 反复覆盖同一个大 value：维护一轮后 segment 文件数回落到 live 段数。
#[test]
fn segment_compaction_reclaims_dead_files() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = mem_store(layout).with_segment_amplify_factor(1);
    // 常驻 key 的段留在 file 0，压缩才有可搬的 live 段。
    let keep = pattern(9000, 9);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"keep".to_vec(), keep.clone(), 1)],
      )
      .await
      .unwrap();
    let mut last = Vec::new();
    for round in 0..12u32 {
      last = pattern(9000 + round as usize, round as u8);
      store
        .put_large(
          &tbl,
          &[BatchEntry::upsert(
            b"big".to_vec(),
            last.clone(),
            u64::from(round) + 1,
          )],
        )
        .await
        .unwrap();
    }
    let before = store
      .page_io()
      .list_segment_file_id_li(&tbl)
      .await
      .unwrap()
      .len();
    assert!(before > 1, "多轮覆盖应留下多个 segment 文件: {before}");

    let mut round = 0;
    loop {
      let plan_li = store.compact_segment_if_needed(&tbl).await.unwrap();
      if plan_li.is_empty() {
        break;
      }
      round += 1;
      assert!(round < 100, "segment compaction 必须收敛");
      store
        .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
        .await
        .unwrap();
    }
    store.maintain(&tbl).await.unwrap();
    let after = store
      .page_io()
      .list_segment_file_id_li(&tbl)
      .await
      .unwrap()
      .len();
    assert!(
      after < before,
      "GC 必须回收空 segment 文件: {before} -> {after}"
    );
    // live 段 6 个、每文件 4 槽：压缩后最多 2 个文件。
    assert!(after <= 2, "segment 文件数未回落到 live 段数: {after}");
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      last
    );
    assert_eq!(
      store.get_large(&tbl, b"keep").await.unwrap().unwrap().value,
      keep
    );

    // reopen 从 manifest 恢复 segment 映射后数据一致。
    let io = store.into_io();
    let mut reopened = mem_store_from(io, layout);
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(
      reopened
        .get_large(&tbl, b"big")
        .await
        .unwrap()
        .unwrap()
        .value,
      last
    );
  });
}

/// 旧 segment 文件仍被 branch manifest 引用时，压缩 / GC 必须保留该文件。
#[test]
fn segment_compaction_keeps_branch_referenced_files() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 2).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let big = pattern(9000, 1);
    let keep = pattern(9000, 2);

    let mut main = mem_store(layout);
    main
      .put_large(
        &tbl,
        &[
          BatchEntry::upsert(b"big".to_vec(), big.clone(), 1),
          BatchEntry::upsert(b"keep".to_vec(), keep.clone(), 1),
        ],
      )
      .await
      .unwrap();
    // branch 继承主分支的 segment 映射（引用 fp 0..6）。
    main.create_branch(&tbl, "feature").await.unwrap();
    let io = main.into_io();

    // 主分支覆盖 big：旧段在 file 0 变死，但 branch manifest 仍引用。
    let mut main = mem_store_from(io, layout).with_segment_amplify_factor(1);
    main.reopen(&tbl).await.unwrap();
    let big2 = pattern(9000, 3);
    main
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), big2.clone(), 2)],
      )
      .await
      .unwrap();

    let before = main.page_io().list_segment_file_id_li(&tbl).await.unwrap();
    assert!(
      before.contains(&0),
      "file 0 应含 keep 的 live 段: {before:?}"
    );
    let mut round = 0;
    loop {
      let plan_li = main.compact_segment_if_needed(&tbl).await.unwrap();
      if plan_li.is_empty() {
        break;
      }
      assert!(plan_li.contains(&0), "file 0 应进入压缩计划: {plan_li:?}");
      round += 1;
      assert!(round < 100, "segment compaction 必须收敛");
      main
        .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
        .await
        .unwrap();
    }
    let after = main.page_io().list_segment_file_id_li(&tbl).await.unwrap();
    assert!(
      after.contains(&0),
      "branch 引用的 segment 文件被误删: {after:?}"
    );
    assert_eq!(
      main.get_large(&tbl, b"keep").await.unwrap().unwrap().value,
      keep
    );

    // branch 视角的数据仍然完整。
    main.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(
      main.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      big
    );
    assert_eq!(
      main.get_large(&tbl, b"keep").await.unwrap().unwrap().value,
      keep
    );
  });
}
