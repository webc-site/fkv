//! term 变化时 allocator 必须跳到下一文件边界（对齐 C++ `Replayer::GetMapper`）。

use fkv_btree::BatchEntry;
use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_store::Store;
use fkv_types::TableIdent;

const SHIFT: u8 = 2;

fn entry(key: &[u8], value: &[u8]) -> BatchEntry {
  BatchEntry::upsert(key.to_vec(), value.to_vec(), 1)
}

#[test]
fn reopen_with_new_term_bumps_allocator_to_next_file_boundary() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::with_pages_per_file(512, SHIFT), 512, 16)
      .with_branch("main", 4)
      .with_pages_per_file_shift(SHIFT);
    store
      .put(&tbl, &[entry(b"a", b"A"), entry(b"b", b"B")])
      .await
      .unwrap();
    let old_max = store.mapper(&tbl).unwrap().max_fp_id();
    assert!(old_max > 0);

    // 同 term reopen：allocator 原地续写。
    let io = store.into_io();
    let mut same = Store::new(io, 512, 16)
      .with_branch("main", 4)
      .with_pages_per_file_shift(SHIFT);
    same.reopen(&tbl).await.unwrap();
    assert_eq!(same.mapper(&tbl).unwrap().max_fp_id(), old_max);

    // 新 term reopen：旧 max 跨到下一文件边界，旧数据仍可读。
    let io = same.into_io();
    let mut next = Store::new(io, 512, 16)
      .with_branch("main", 5)
      .with_pages_per_file_shift(SHIFT);
    next.reopen(&tbl).await.unwrap();
    let bumped = ((old_max >> SHIFT) + 1) << SHIFT;
    assert_eq!(next.mapper(&tbl).unwrap().max_fp_id(), bumped);
    assert_eq!(next.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

    // 新 term 的首笔写从 bumped 文件开始，不复用旧 term 的 file id。
    next.put(&tbl, &[entry(b"c", b"C")]).await.unwrap();
    assert!(next.mapper(&tbl).unwrap().max_fp_id() > bumped);
  });
}

/// attach term 必须在安装 replay 之前采纳：branch 区间记当前 identity，
/// 且 allocator 抬到下一文件边界，attach 后写出的对象才能按 `_T` 找到。
#[test]
fn open_branch_adopts_attach_term_and_bumps_file_boundary() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut main = Store::new(MemStore::with_pages_per_file(512, SHIFT), 512, 16)
      .with_pages_per_file_shift(SHIFT);
    main.put(&tbl, &[entry(b"a", b"A")]).await.unwrap();
    main.create_branch(&tbl, "feature").await.unwrap();
    let io = main.into_io();

    // 以 term 0 打开，取 branch 基线高水位。
    let mut zero = Store::new(io, 512, 16).with_pages_per_file_shift(SHIFT);
    zero.open_branch(&tbl, "feature", 0).await.unwrap();
    let old_max = zero.mapper(&tbl).unwrap().max_fp_id();
    let io = zero.into_io();

    // attach 到新 term：先采纳 term 再 install，allocator 必须跨文件边界。
    let mut attached = Store::new(io, 512, 16).with_pages_per_file_shift(SHIFT);
    attached.open_branch(&tbl, "feature", 7).await.unwrap();
    let bumped = ((old_max >> SHIFT) + 1) << SHIFT;
    assert_eq!(attached.mapper(&tbl).unwrap().max_fp_id(), bumped);
    assert_eq!(attached.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

    attached.put(&tbl, &[entry(b"b", b"B")]).await.unwrap();
    let written_max = attached.mapper(&tbl).unwrap().max_fp_id();

    // 新写入区间必须记 attach term，而不是 0。
    let io = attached.into_io();
    let bytes = io
      .read_manifest_file(&tbl, "manifest_feature_0")
      .await
      .unwrap()
      .unwrap();
    let state = fkv_task_reopen::replay_manifest_bytes(&bytes).unwrap();
    let last = state.branch_metadata.file_range_li.last().unwrap();
    assert_eq!(last.branch_name.as_ref(), "feature");
    assert_eq!(last.term, 7, "branch 区间必须记当前 attach term");
    assert_eq!(last.max_file_id, bumped >> SHIFT);
    assert_eq!(
      state.branch_metadata.term, 7,
      "manifest 的活跃 term 必须同步"
    );

    // 同 term 再打开：term 未变，不得再次抬文件边界。
    let mut again = Store::new(io, 512, 16).with_pages_per_file_shift(SHIFT);
    again.open_branch(&tbl, "feature", 7).await.unwrap();
    assert_eq!(again.mapper(&tbl).unwrap().max_fp_id(), written_max);
  });
}
