use std::slice::from_ref;

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io::{FileIo, FixedBuffer, FixedSegmentIo, ManifestIo};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_io_sim::{Op, SimStore};
use fkv_large::SegmentLayout;
use fkv_manifest::{ManifestBuilder, SnapshotArg};
use fkv_recovery::ReplayState;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

/// 当前 live 的 segment 逻辑页 id 与其物理 `FilePageId`，按逻辑页 id 升序。
fn segment_live_li(
  store: &Store<MemStore>,
  tbl: &TableIdent,
) -> (Vec<fkv_types::PageId>, Vec<fkv_types::FilePageId>) {
  let mapper = store.segment_mapper(tbl).expect("segment mapper 必须存在");
  let mut id_li = Vec::new();
  let mut fp_li = Vec::new();
  for id in 0..mapper.tbl().len() as fkv_types::PageId {
    if let Ok(fp) = mapper.tbl().file_page_id(id) {
      id_li.push(id);
      fp_li.push(fp);
    }
  }
  (id_li, fp_li)
}

#[test]
fn memstore_large_value_put_get_cow_and_reopen() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let big = pattern(9000);
    store
      .put_large(
        &tbl,
        &[
          BatchEntry::upsert(b"big".to_vec(), big.clone(), 1).with_metadata(b"block-descriptor"),
          BatchEntry::upsert(b"small".to_vec(), b"S".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    let read = store.get_large(&tbl, b"big").await.unwrap().unwrap();
    assert_eq!(read.value, big);
    assert_eq!(read.metadata, b"block-descriptor");
    assert_eq!(
      store
        .get_large(&tbl, b"small")
        .await
        .unwrap()
        .unwrap()
        .value,
      b"S"
    );
    assert_eq!(store.segment_mapper(&tbl).unwrap().tbl().len(), 3);

    // 小 key 写入触发 data page COW，大 value 的 0b11 flag 必须保留。
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"other".to_vec(), b"O".to_vec(), 2)],
      )
      .await
      .unwrap();
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      big
    );

    // 覆盖为另一个大 value。
    let big2 = pattern(5000);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), big2.clone(), 3)],
      )
      .await
      .unwrap();
    let read = store.get_large(&tbl, b"big").await.unwrap().unwrap();
    assert_eq!(read.value, big2);
    assert!(read.metadata.is_empty());
    assert_eq!(
      store
        .get_large(&tbl, b"small")
        .await
        .unwrap()
        .unwrap()
        .value,
      b"S"
    );

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
      big2
    );
    // core-02：新 value 的 segment 先分配、旧 value 的 3 个 segment 在同批随后释放，
    // 所以活跃 segment 为 2、其余 3 个槽位全部回空闲链表（可被后续批复用）。
    let segment_mapper = reopened.segment_mapper(&tbl).unwrap();
    let live_num = segment_mapper
      .tbl()
      .iter_value_li()
      .filter(|value| fkv_types::is_file_page_id(*value))
      .count();
    assert_eq!(live_num, 2, "old large value segments must be released");
    assert_eq!(segment_mapper.tbl().len(), 5);
    assert_eq!(
      segment_mapper.free_page_cnt() as usize,
      segment_mapper.tbl().len() - live_num,
      "every non-live segment slot must be reusable"
    );
  });
}

#[test]
fn segment_mapping_replays_from_manifest_log() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    // 第一次写建 snapshot。
    store
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    // 第二次写走 manifest log，segment mapping 以 delta 形式追加。
    let big = pattern(4200);
    store
      .put_large(
        &tbl,
        &[
          BatchEntry::upsert(b"big".to_vec(), big.clone(), 2),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2),
        ],
      )
      .await
      .unwrap();
    let mapped = store.segment_mapper(&tbl).unwrap().tbl().len();
    assert_eq!(mapped, 2);

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
      big
    );
    assert_eq!(reopened.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(reopened.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
    assert_eq!(reopened.segment_mapper(&tbl).unwrap().tbl().len(), mapped);
  });
}

/// core-02：反复覆盖大 value 时，segment 逻辑页表必须有界（释放的逻辑页被复用）。
#[test]
fn repeated_put_large_keeps_segment_tbl_bounded() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let mut len_li = Vec::new();
    for ts in 1..=12 {
      store
        .put_large(
          &tbl,
          &[BatchEntry::upsert(b"big".to_vec(), pattern(9000), ts)],
        )
        .await
        .unwrap();
      assert_eq!(
        store
          .get_large(&tbl, b"big")
          .await
          .unwrap()
          .unwrap()
          .value
          .len(),
        9000
      );
      len_li.push(store.segment_mapper(&tbl).unwrap().tbl().len());
    }
    // 每轮 3 个 segment：同批先分配后释放，长度稳定在 6 以内；不复用则单调到 36。
    assert!(
      len_li.iter().all(|len| *len <= 6),
      "segment tbl must stay bounded, got {len_li:?}"
    );
    assert!(store.segment_mapper(&tbl).unwrap().free_page_cnt() > 0);
  });
}

/// sem-46：同批覆盖大 value 时 fkv 先分配新 segment 逻辑页、批末才释放旧页。
///
/// C++ `BatchWriteTask::ApplyBatch` 先 `DelLargeValue` 再 `WriteLargeValue`，
/// 新值复用刚释放的逻辑页（表长保持 live 段数）；fkv 顺序相反，覆盖后表长为
/// 2 × live 段数，下一批才复用它。本用例锁定该布局：若哪天改成对齐 C++，
/// 这里必须显式更新，而不是让 manifest segment 段悄悄换形态。
#[test]
fn put_large_overwrite_allocs_before_frees_segment_slots() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let value_a = pattern(9000);
    store
      .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), value_a, 1)])
      .await
      .unwrap();
    let (id_a, fp_a) = segment_live_li(&store, &tbl);
    assert_eq!(id_a, vec![0, 1, 2], "首次写入追加槽 0..3");
    assert_eq!(store.segment_mapper(&tbl).unwrap().tbl().len(), 3);

    // 同尺寸覆盖：新段先占用槽 3..6，旧槽 0..3 在 data page merge 时才释放。
    let value_b = pattern(9000);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), value_b.clone(), 2)],
      )
      .await
      .unwrap();
    let (id_b, fp_b) = segment_live_li(&store, &tbl);
    assert_eq!(id_b, vec![3, 4, 5], "旧槽 0..3 必须先空闲、新段追加");
    let mapper = store.segment_mapper(&tbl).unwrap();
    assert_eq!(mapper.tbl().len(), 6, "覆盖后表长为 2 × live 段数");
    assert_eq!(mapper.mapping_count(), 3);
    assert_eq!(mapper.free_page_cnt(), 3);
    assert!(
      fp_b.iter().all(|fp| !fp_a.contains(fp)),
      "段 mapper 恒 Append，新段不得复用旧物理页"
    );
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      value_b
    );

    // 第三次覆盖复用上一批释放的槽（空闲链 LIFO）：表长保持 6，不再增长。
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), pattern(9000), 3)],
      )
      .await
      .unwrap();
    let (id_c, _) = segment_live_li(&store, &tbl);
    assert_eq!(id_c, vec![0, 1, 2], "新段必须取回上一批释放的逻辑槽");
    let mapper = store.segment_mapper(&tbl).unwrap();
    assert_eq!(mapper.tbl().len(), 6, "有界复用，表长不得随覆盖轮次增长");
    assert_eq!(mapper.mapping_count(), 3);
  });
}

#[compio::test]
async fn compio_large_value_segment_file_reopen_and_truncate() {
  let root = TempRoot::new("store_large_value_compio");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  let big = pattern(9000);
  store
    .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 1)])
    .await
    .unwrap();
  assert!(root.join("demo.1").join("segment_0_main_0").exists());
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big
  );
  drop(store);

  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  store.reopen(&tbl).await.unwrap();
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big
  );

  store.truncate(&tbl).await.unwrap();
  assert_eq!(store.get_large(&tbl, b"big").await.unwrap(), None);
  assert!(
    store
      .segment_mapper(&tbl)
      .unwrap()
      .tbl()
      .iter_value_li()
      .all(|value| !fkv_types::is_file_page_id(value))
  );

  let big2 = pattern(3000);
  store
    .put_large(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), big2.clone(), 2)],
    )
    .await
    .unwrap();
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big2
  );
}

/// sem-43：fkv 整表 truncate 同时释放 segment 逻辑页（C++ 只释放 data 页）。
///
/// 锁定 fkv 侧语义：manifest segment 段写回**全空闲槽**、`max_segment_fp_id`
/// 保留；旧 segment 文件不再被任何映射引用，下一轮 GC 可整文件回收；后续
/// `put_large` 复用空闲逻辑槽且段水位不回退。两条悬空风险各由一条断言兜住：
///
/// - 「释放映射但段文件泄漏」：truncate 后 `file_gc` 必须删光旧 segment 文件；
/// - 「保留映射但文件被 GC」：重开后 segment 映射仍是空闲槽，不存在指向已删
///   文件的 live FilePageId。
#[compio::test]
async fn truncate_frees_segment_mapping_and_gc_reclaims_files() {
  let root = TempRoot::new("store_large_value_truncate_segment_gc");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 1).unwrap();
  let store_io = || {
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
  };
  let mut store = Store::new(store_io(), 512, 16).with_large_value(layout, 1024);
  let big1 = pattern(9000);
  let big2 = pattern(6000);
  store
    .put_large(
      &tbl,
      &[
        BatchEntry::upsert(b"big1".to_vec(), big1.clone(), 1),
        BatchEntry::upsert(b"big2".to_vec(), big2.clone(), 1),
      ],
    )
    .await
    .unwrap();

  let seg_tbl_len = store.segment_mapper(&tbl).unwrap().tbl().len();
  let seg_max = store.segment_mapper(&tbl).unwrap().max_fp_id();
  assert!(
    store
      .segment_mapper(&tbl)
      .unwrap()
      .tbl()
      .iter_value_li()
      .all(fkv_types::is_file_page_id),
    "truncate 前 segment 槽位应全部指向物理段"
  );
  let seg_files_before = store.page_io().list_segment_file_id_li(&tbl).await.unwrap();
  assert!(
    seg_files_before.len() >= 2,
    "用例需要跨多个 segment 文件，实际 {seg_files_before:?}"
  );

  store.truncate(&tbl).await.unwrap();

  // 内存侧：全部槽位回空闲链，段水位保留（后续分配不会回落到旧段）。
  let segment_mapper = store.segment_mapper(&tbl).unwrap();
  assert_eq!(segment_mapper.tbl().len(), seg_tbl_len);
  assert_eq!(segment_mapper.max_fp_id(), seg_max);
  assert_eq!(segment_mapper.mapping_count(), 0);
  assert_eq!(segment_mapper.free_page_cnt() as usize, seg_tbl_len);
  assert!(
    segment_mapper
      .tbl()
      .iter_value_li()
      .all(|value| !fkv_types::is_file_page_id(value))
  );

  // 盘面：manifest segment 段与内存一致（全空闲槽），max_segment_fp_id 保留。
  let bytes = ManifestIo::read_manifest(store.page_io(), &tbl)
    .await
    .unwrap()
    .unwrap();
  let state = ReplayState::from_snapshot(&bytes).unwrap();
  assert_eq!(state.root_id, fkv_types::NULL_PAGE);
  assert_eq!(state.segment_mapping.len(), seg_tbl_len);
  assert_eq!(state.segment_mapping.mapping_count(), 0);
  assert_eq!(state.max_segment_fp_id, seg_max);

  // truncate 只写 manifest，物理文件留给 GC。
  assert_eq!(
    store.page_io().list_segment_file_id_li(&tbl).await.unwrap(),
    seg_files_before,
    "truncate 本身不删 segment 文件"
  );

  // 冷重开：空表 + 全空闲 segment 映射，旧大 value 不可读。
  let mut reopened = Store::new(store.into_io(), 512, 16).with_large_value(layout, 1024);
  reopened.reopen(&tbl).await.unwrap();
  assert!(reopened.scan(&tbl, b"", b"", 0).await.unwrap().is_empty());
  assert_eq!(reopened.get_large(&tbl, b"big1").await.unwrap(), None);
  let segment_mapper = reopened.segment_mapper(&tbl).unwrap();
  assert_eq!(segment_mapper.tbl().len(), seg_tbl_len);
  assert_eq!(segment_mapper.max_fp_id(), seg_max);
  assert_eq!(segment_mapper.mapping_count(), 0);

  // 释放映射后旧 segment 文件必须可被回收（不泄漏）。分配器水位所在的文件
  // 由 in-flight guard 保守保留——它是下一段写入的目标文件，不属于 truncate
  // 遗留；水位以下的旧文件必须全部删除。
  let removed = reopened
    .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .await
    .unwrap();
  assert!(removed >= seg_files_before.len() - 1);
  let watermark_file = seg_max >> layout.segments_per_file_shift();
  let remaining = reopened
    .page_io()
    .list_segment_file_id_li(&tbl)
    .await
    .unwrap();
  assert!(
    remaining.iter().all(|file_id| *file_id == watermark_file),
    "truncate 后除水位文件外旧 segment 文件应被 GC 回收，实际 {remaining:?}"
  );
  assert!(
    remaining.len() < seg_files_before.len(),
    "水位以下旧 segment 文件必须被回收，实际 {remaining:?}"
  );

  // 后续 put_large：复用 truncate 释放的逻辑槽，段水位不回退。
  let big3 = pattern(5000);
  reopened
    .put_large(
      &tbl,
      &[BatchEntry::upsert(b"big3".to_vec(), big3.clone(), 2)],
    )
    .await
    .unwrap();
  assert_eq!(
    reopened
      .get_large(&tbl, b"big3")
      .await
      .unwrap()
      .unwrap()
      .value,
    big3
  );
  assert_eq!(reopened.get_large(&tbl, b"big1").await.unwrap(), None);
  let segment_mapper = reopened.segment_mapper(&tbl).unwrap();
  assert_eq!(
    segment_mapper.tbl().len(),
    seg_tbl_len,
    "truncate 释放的逻辑槽必须被后续 put_large 复用"
  );
  assert!(
    segment_mapper.max_fp_id() > seg_max,
    "段水位必须只在 truncate 前水位之上继续分配"
  );
}

/// sem-43 双向互读：C++ 整表 truncate 只释放 data 页、保留 segment 映射与文件
/// （`BatchWriteTask::Truncate` 不碰 `segment_mapper_`，`UpdateMeta` 仍把原
/// `seg_mapping` 写进 manifest）。fkv 必须能重开这种 C++ 形态的 manifest：
/// 表为空、旧大 value 不可读、segment 映射仍为 FilePageId（GC 继续保留旧文件，
/// 不会出现指向已删文件的 live 映射），后续 `put_large` 安全续写。
#[compio::test]
async fn reopen_cpp_style_truncate_manifest_keeps_segment_mapping() {
  let root = TempRoot::new("store_large_value_cpp_truncate_reopen");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 1).unwrap();
  let store_io = || {
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
  };
  let mut store = Store::new(store_io(), 512, 16).with_large_value(layout, 1024);
  let big1 = pattern(9000);
  let big2 = pattern(6000);
  store
    .put_large(
      &tbl,
      &[
        BatchEntry::upsert(b"big1".to_vec(), big1, 1),
        BatchEntry::upsert(b"big2".to_vec(), big2, 1),
      ],
    )
    .await
    .unwrap();
  let seg_values_before: Vec<u64> = store
    .segment_mapper(&tbl)
    .unwrap()
    .tbl()
    .iter_value_li()
    .collect();
  let seg_max = store.segment_mapper(&tbl).unwrap().max_fp_id();
  let seg_files_before = store.page_io().list_segment_file_id_li(&tbl).await.unwrap();

  store.truncate(&tbl).await.unwrap();

  // fkv truncate 后的 manifest：data 段为空闲链，segment 段被释放。
  let fkv_bytes = ManifestIo::read_manifest(store.page_io(), &tbl)
    .await
    .unwrap()
    .unwrap();
  let fkv_state = ReplayState::from_snapshot(&fkv_bytes).unwrap();
  let data_values: Vec<u64> = fkv_state.mapping_tbl.iter_value_li().collect();

  // 构造 C++ 形态：data 段沿用 truncate 后的空闲链，segment 段保留 truncate
  // 前的 FilePageId 集合（C++ Truncate 只释放 data mapper）。
  let mut builder = ManifestBuilder::new();
  let cpp_bytes = builder
    .snapshot(
      SnapshotArg {
        root_id: fkv_types::NULL_PAGE,
        ttl_root: fkv_types::NULL_PAGE,
        max_fp_id: fkv_state.max_fp_id,
        dict_bytes: &fkv_state.dict_bytes,
        branch_metadata: &fkv_state.branch_metadata,
        max_segment_fp_id: seg_max,
      },
      &data_values,
      Some(&seg_values_before),
    )
    .unwrap();
  let io = store.into_io();
  ManifestIo::switch_manifest(&io, &tbl, &cpp_bytes)
    .await
    .unwrap();

  let mut reopened = Store::new(io, 512, 16).with_large_value(layout, 1024);
  reopened.reopen(&tbl).await.unwrap();
  assert!(reopened.scan(&tbl, b"", b"", 0).await.unwrap().is_empty());
  assert_eq!(reopened.get_large(&tbl, b"big1").await.unwrap(), None);
  let segment_mapper = reopened.segment_mapper(&tbl).unwrap();
  assert_eq!(segment_mapper.tbl().len(), seg_values_before.len());
  assert_eq!(segment_mapper.mapping_count(), seg_values_before.len());
  assert_eq!(segment_mapper.max_fp_id(), seg_max);
  assert!(
    segment_mapper
      .tbl()
      .iter_value_li()
      .all(fkv_types::is_file_page_id),
    "C++ 保留的 segment 映射必须原样恢复"
  );

  // GC 必须保留 C++ 侧仍映射的旧 segment 文件：映射活着，文件不能删。
  reopened
    .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .await
    .unwrap();
  assert_eq!(
    reopened
      .page_io()
      .list_segment_file_id_li(&tbl)
      .await
      .unwrap(),
    seg_files_before,
    "C++ 保留的 segment 映射必须继续被 GC 保留"
  );
  assert!(
    reopened
      .page_io()
      .list_file_id_li(&tbl)
      .await
      .unwrap()
      .is_empty(),
    "data 映射已全部释放，data 文件应被回收"
  );

  // 续写：新段从保留的水位之后分配，读回正确；旧 key 仍不可见。
  let big3 = pattern(5000);
  reopened
    .put_large(
      &tbl,
      &[BatchEntry::upsert(b"big3".to_vec(), big3.clone(), 2)],
    )
    .await
    .unwrap();
  assert_eq!(
    reopened
      .get_large(&tbl, b"big3")
      .await
      .unwrap()
      .unwrap()
      .value,
    big3
  );
  assert_eq!(reopened.get_large(&tbl, b"big1").await.unwrap(), None);
  assert!(reopened.segment_mapper(&tbl).unwrap().max_fp_id() > seg_max);
}

#[test]
fn put_large_requires_layout() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    assert_eq!(
      store
        .put_large(
          &tbl,
          &[BatchEntry::upsert(b"a".to_vec(), vec![1u8; 4096], 1)]
        )
        .await
        .unwrap_err(),
      KvError::InvalidArgs
    );
  });
}

#[compio::test]
async fn segment_gc_reclaims_overwritten_and_deleted_values() {
  let root = TempRoot::new("store_large_value_segment_gc");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 1).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  let big = pattern(9000);
  store
    .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 1)])
    .await
    .unwrap();
  assert_eq!(
    store.page_io().list_segment_file_id_li(&tbl).await.unwrap(),
    vec![0, 1]
  );

  // 9000B / 4096 = 3 segments；覆盖为 3000B 只占 1 个新 segment（file 1）。
  let big2 = pattern(3000);
  store
    .put_large(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), big2.clone(), 2)],
    )
    .await
    .unwrap();
  let removed = store
    .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .await
    .unwrap();
  assert!(removed >= 1);
  assert_eq!(
    store.page_io().list_segment_file_id_li(&tbl).await.unwrap(),
    vec![1]
  );
  assert!(!root.join("demo.1").join("segment_0_main_0").exists());
  assert!(root.join("demo.1").join("segment_1_main_0").exists());
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big2
  );

  drop(store);
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  store.reopen(&tbl).await.unwrap();
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big2
  );
  assert_eq!(
    store.page_io().list_segment_file_id_li(&tbl).await.unwrap(),
    vec![1]
  );

  // 删除旧大 value 后，最后一个 segment 文件也应被 GC 回收。
  store
    .put_large(&tbl, &[BatchEntry::delete(b"big".to_vec(), 3)])
    .await
    .unwrap();
  assert_eq!(store.get_large(&tbl, b"big").await.unwrap(), None);
  store
    .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .await
    .unwrap();
  assert!(
    store
      .page_io()
      .list_segment_file_id_li(&tbl)
      .await
      .unwrap()
      .is_empty()
  );
  assert!(!root.join("demo.1").join("segment_1_main_0").exists());
}

#[compio::test]
async fn store_fixed_buffer_put_and_get() {
  let root = TempRoot::new("store_large_value_fixed");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let io = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 2);
  // 注册缓冲依赖内核 / memlock 限额；runner 不提供时跳过这条能力用例。
  let Ok(mut buf) = io.register_fixed_buffer(64 * 1024) else {
    eprintln!("skip: register_fixed_buffer unavailable");
    return;
  };
  assert_eq!(buf.is_registered(), cfg!(target_os = "linux"));
  let mut store = Store::new(io, 512, 16).with_large_value(layout, 1024);
  let value = pattern(9000);
  let mut entry = BatchEntry::upsert(b"big".to_vec(), value.clone(), 1);
  entry.metadata = b"meta".to_vec();
  store
    .put_large_fixed(&tbl, &[entry], &mut buf)
    .await
    .unwrap();

  let into = store
    .get_large_into(&tbl, b"big", &mut buf)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(into.value_len, value.len());
  assert_eq!(&buf.as_slice()[..into.value_len], value);
  assert_eq!(into.metadata, b"meta");
  assert_eq!(into.timestamp, 1);
  assert_eq!(into.expire_ts, 0);
  let large = store.get_large(&tbl, b"big").await.unwrap().unwrap();
  assert_eq!(large.value, value);
  assert_eq!(large.metadata, b"meta");
  assert_eq!(large.timestamp, 1);
  assert_eq!(large.expire_ts, 0);
}

/// sem-34：C++ `ReadRequest` 默认（`std::monostate`）形态对 large entry
/// 返回 `NoError + metadata`、且始终填 `ts_` / `expire_ts_`（`task.cpp:394-420`）。
/// `get_entry` 与之对齐：large 走 metadata-only（不读 segment），普通 value
/// 返回完整 value；`get` / `read` 的 `LargeValueUnsupported` 契约不变。
#[test]
fn get_entry_matches_cpp_read_request_metadata_only_contract() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let big = pattern(9000);
    let mut large = BatchEntry::upsert(b"big".to_vec(), big.clone(), 7).with_metadata(b"meta");
    large.expire_ts = u64::MAX / 2;
    let plain = BatchEntry::upsert(b"small".to_vec(), b"S".to_vec(), 9);
    store.put_large(&tbl, &[large, plain]).await.unwrap();

    // large：metadata-only；`value` 不泄露 segment 指针编码。
    let entry = store.get_entry(&tbl, b"big").await.unwrap().unwrap();
    assert!(entry.is_large_value);
    assert!(entry.value.is_empty());
    assert_eq!(entry.metadata, b"meta");
    assert_eq!(entry.timestamp, 7);
    assert_eq!(entry.expire_ts, u64::MAX / 2);

    // 普通 value：完整 value + 空 metadata。
    let entry = store.get_entry(&tbl, b"small").await.unwrap().unwrap();
    assert!(!entry.is_large_value);
    assert_eq!(entry.value, b"S");
    assert!(entry.metadata.is_empty());
    assert_eq!(entry.timestamp, 9);
    assert_eq!(entry.expire_ts, 0);

    assert_eq!(store.get_entry(&tbl, b"missing").await.unwrap(), None);
    // 既有分层不变：直接 `get` / `read` 仍拒绝 large value（issue 15）。
    assert_eq!(
      store.get(&tbl, b"big").await.unwrap_err(),
      KvError::LargeValueUnsupported
    );
  });
}

/// issue 15：`get` / `read` 不能把 large value 的内部编码（segment 指针+metadata）
/// 当作数据返回；必须让调用方走 `get_large*`。
#[test]
fn get_and_read_reject_large_value() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let big = pattern(9000);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 1).with_metadata(b"meta")],
      )
      .await
      .unwrap();
    assert_eq!(
      store.get(&tbl, b"big").await.unwrap_err(),
      KvError::LargeValueUnsupported
    );
    assert_eq!(
      store.read(&tbl, b"big").await.unwrap_err(),
      KvError::LargeValueUnsupported
    );
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      big
    );

    // 同表的普通 value 不受影响。
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"small".to_vec(), b"S".to_vec(), 2)],
      )
      .await
      .unwrap();
    assert_eq!(
      store.get(&tbl, b"small").await.unwrap(),
      Some(b"S".to_vec())
    );
  });
}

/// sem-34：`get_entry` 的 large 分支必须真的跳过 segment IO（对齐 C++
/// `ReadRequest` monostate 的「No segment reads」），而 `get_large` 仍然读。
/// 用 `SimStore` 的确定性 op log 锁定：前者只在 data page 上取 header/trailer。
#[test]
fn get_entry_metadata_only_skips_segment_reads() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(SimStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), pattern(9000), 3).with_metadata(b"meta")],
      )
      .await
      .unwrap();

    let before = store.page_io().op_log_li().len();
    let entry = store.get_entry(&tbl, b"big").await.unwrap().unwrap();
    let read_ops = store.page_io().op_log_li()[before..]
      .iter()
      .filter(|record| {
        matches!(
          record.op,
          Op::ReadSegment { .. } | Op::ReadSegmentFixed { .. }
        )
      })
      .count();
    assert_eq!(
      read_ops, 0,
      "metadata-only get_entry must not read segments"
    );
    assert!(entry.value.is_empty());
    assert_eq!(entry.metadata, b"meta");
    assert_eq!(entry.timestamp, 3);

    let before = store.page_io().op_log_li().len();
    let large = store.get_large(&tbl, b"big").await.unwrap().unwrap();
    let read_ops = store.page_io().op_log_li()[before..]
      .iter()
      .filter(|record| {
        matches!(
          record.op,
          Op::ReadSegment { .. } | Op::ReadSegmentFixed { .. }
        )
      })
      .count();
    assert_eq!(read_ops, 3, "9000B / 4096B = 3 segments");
    assert_eq!(large.value, pattern(9000));
  });
}

fn live_segment_num(store: &Store<MemStore>) -> usize {
  store
    .segment_mapper(&TableIdent::new("demo", 1).unwrap())
    .map_or(0, |mapper| {
      mapper
        .tbl()
        .iter_value_li()
        .filter(|value| fkv_types::is_file_page_id(*value))
        .count()
    })
}

/// eng-51：delete / 小值覆盖大 value 时必须释放 segment 映射，不能永久泄漏。
#[test]
fn delete_and_overwrite_free_large_value_segments() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), pattern(9000), 1)],
      )
      .await
      .unwrap();
    assert_eq!(live_segment_num(&store), 3);

    // 普通小值覆盖大 value。
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), b"tiny".to_vec(), 2)],
      )
      .await
      .unwrap();
    assert_eq!(live_segment_num(&store), 0, "覆盖后旧段必须释放");
    assert_eq!(
      store.get(&tbl, b"big").await.unwrap(),
      Some(b"tiny".to_vec())
    );

    // 重新写大 value 后 delete。
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big2".to_vec(), pattern(9000), 3)],
      )
      .await
      .unwrap();
    assert_eq!(live_segment_num(&store), 3);
    store.delete(&tbl, b"big2", 4).await.unwrap();
    assert_eq!(live_segment_num(&store), 0, "delete 后段必须释放");

    // 覆盖成新的大 value：旧段释放、新段保留。
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big2".to_vec(), pattern(5000), 5)],
      )
      .await
      .unwrap();
    assert_eq!(live_segment_num(&store), 2);
  });
}

/// eng-75：plain / fixed 两条写路径与 owned / fixed 两条读路径逐字节等价。
#[test]
fn large_value_paths_are_byte_equivalent() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut plain = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let mut fixed = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let mut buf = FixedBuffer::new(64 * 1024);

    // 阈值以下、恰在阈值、段边界两侧、多段。
    for (seq, len) in [1usize, 1024, 4096, 4097, 9000, 16384]
      .into_iter()
      .enumerate()
    {
      let key = format!("k{len}").into_bytes();
      let value = pattern(len);
      let mut entry = BatchEntry::upsert(key.clone(), value.clone(), seq as u64);
      if len >= 1024 {
        entry.metadata = format!("m{len}").into_bytes();
      }
      plain.put_large(&tbl, from_ref(&entry)).await.unwrap();
      fixed
        .put_large_fixed(&tbl, from_ref(&entry), &mut buf)
        .await
        .unwrap();

      let owned = plain.get_large(&tbl, &key).await.unwrap().unwrap();
      assert_eq!(owned, fixed.get_large(&tbl, &key).await.unwrap().unwrap());
      assert_eq!(owned.value, value, "len={len}");
      assert_eq!(owned.metadata, entry.metadata, "len={len}");

      for store in [&plain, &fixed] {
        let into = store
          .get_large_into(&tbl, &key, &mut buf)
          .await
          .unwrap()
          .unwrap();
        assert_eq!(into.value_len, value.len(), "len={len}");
        assert_eq!(
          &buf.as_slice()[..into.value_len],
          value.as_slice(),
          "len={len}"
        );
        assert_eq!(into.metadata, owned.metadata, "len={len}");
      }

      let floor = plain.floor(&tbl, &key).await.unwrap();
      assert_eq!(floor.key, key);
      assert_eq!(floor.value, value);
      assert_eq!(floor.metadata, owned.metadata);
    }
  });
}

/// eng-75：fixed buffer 装不下 value 时与 plain 路径同样报 `InvalidArgs`，
/// 且整批回滚（不落 segment、不写同批其他 entry）。
#[test]
fn put_large_fixed_rejects_value_larger_than_buffer() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let mut buf = FixedBuffer::new(1024);
    let batch = [
      BatchEntry::upsert(b"small".to_vec(), b"S".to_vec(), 1),
      BatchEntry::upsert(b"big".to_vec(), pattern(4096), 2),
    ];
    assert_eq!(
      store
        .put_large_fixed(&tbl, &batch, &mut buf)
        .await
        .unwrap_err(),
      KvError::InvalidArgs
    );
    assert_eq!(store.get_large(&tbl, b"big").await.unwrap(), None);
    assert_eq!(store.get(&tbl, b"small").await.unwrap(), None);
    assert_eq!(live_segment_num(&store), 0);
  });
}

/// perf-85：`truncate_from` 不物化 value，但删除仍须释放大 value segment。
#[test]
fn truncate_from_frees_large_value_segments() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    store
      .put_large(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"tiny".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), pattern(9000), 1),
          BatchEntry::upsert(b"c".to_vec(), pattern(9000), 1),
        ],
      )
      .await
      .unwrap();
    assert_eq!(live_segment_num(&store), 6);

    assert_eq!(store.truncate_from(&tbl, b"b").await.unwrap(), 2);
    assert_eq!(live_segment_num(&store), 0, "截断必须释放旧 segment");
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"tiny".to_vec()));
    assert_eq!(store.get(&tbl, b"b").await.unwrap(), None);
    assert_eq!(store.floor(&tbl, b"z").await.unwrap().key, b"a");
  });
}

/// R4-05：同批同 key 多版本必须先按 timestamp 折叠，只为胜者写 segment。
///
/// 先逐条落段再交给 `put` 折叠时，败者的段链既无树引用也不会释放，
/// 映射与空间会一直泄漏到下一次段压缩。
#[test]
fn put_large_folds_duplicate_keys_before_writing_segments() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let new = pattern(9000);
    store
      .put_large(
        &tbl,
        &[
          BatchEntry::upsert(b"big".to_vec(), pattern(20_000), 1),
          BatchEntry::upsert(b"big".to_vec(), new.clone(), 2),
        ],
      )
      .await
      .unwrap();
    // 胜者 9000B 占 3 个 segment；败者 20000B（5 个）不得落盘。
    assert_eq!(live_segment_num(&store), 3, "败者段链泄漏");
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      new
    );
  });
}

/// sem-62：`put_large` 批内低于 `large_value_threshold` 的 value 落 inline
/// 数据页，达到阈值（`>=`，含恰好等于）才落 segment——与 C++ 的输入契约
/// 差异，登记为有意差异（issue/done/sem-62-put-large-subthreshold-inline.md）。
///
/// C++ 引擎对 large entry 没有大小门槛：落段与否由宿主构造 `WriteDataEntry`
/// 决定（`include/types.h` 的 `large_val_` 非 `monostate` 即 `HasLargeValue()`，
/// `BatchWriteTask::ApplyBatch` 对其无条件 `WriteLargeValue` 走 segment），
/// inline 构造（`BatchWriteRequest::AddWrite`）提交的任意大小 value 都走数据页
/// （inline / overflow 链），全仓不存在 large value 阈值配置。fkv 的
/// `put_large_impl`（`fkv_store/src/write.rs`）则有阈值门：
/// `value.len() >= large_value_threshold` 才落 segment（`fkv_shard/src/lifecycle.rs`
/// 默认 `page_size / 2`、config 为 0 时自动取界；本用例显式取 1024）。
///
/// 判别点：小 value 若被改为落段（对齐 C++ 宿主全 large 构造），`Store::get`
/// 会得到 `LargeValueUnsupported`（只有数据页路径可经 `get` 读回）、manifest
/// segment 段槽数会多出小 value 的一槽；`edge`（恰等于阈值）锁定 `>=` 语义，
/// 改成严格大于时它落数据页、`get` 不再拒绝即红。
#[test]
fn put_large_routes_subthreshold_values_to_data_pages() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let big = pattern(9000); // >= 1024 → segment
    let edge = pattern(1024); // 恰等于阈值 → segment（>= 语义）
    let small = pattern(600); // < 1024 → inline 数据页（跨 512B 页走 overflow 链）
    store
      .put_large(
        &tbl,
        &[
          BatchEntry::upsert(b"big".to_vec(), big.clone(), 1),
          BatchEntry::upsert(b"edge".to_vec(), edge.clone(), 1),
          BatchEntry::upsert(b"small".to_vec(), small.clone(), 1),
        ],
      )
      .await
      .unwrap();

    // 小 value：inline 数据页路径——普通 `get` 直接读回。
    assert_eq!(store.get(&tbl, b"small").await.unwrap(), Some(small));
    // 大 value（含恰好等于阈值的 edge）：segment 路径——`get` 拒绝、`get_large` 读回。
    assert_eq!(
      store.get(&tbl, b"big").await.unwrap_err(),
      KvError::LargeValueUnsupported
    );
    assert_eq!(
      store.get(&tbl, b"edge").await.unwrap_err(),
      KvError::LargeValueUnsupported
    );
    assert_eq!(
      store.get_large(&tbl, b"edge").await.unwrap().unwrap().value,
      edge
    );

    // manifest segment 段只含大 value 槽：9000B / 4096B = 3 + 1024B = 1，
    // 共 4 槽全 live；小 value 若落段会多出第 5 槽。
    let bytes = ManifestIo::read_manifest(store.page_io(), &tbl)
      .await
      .unwrap()
      .unwrap();
    let state = ReplayState::from_snapshot(&bytes).unwrap();
    assert_eq!(state.segment_mapping.len(), 4);
    assert_eq!(state.segment_mapping.mapping_count(), 4);
    assert_eq!(live_segment_num(&store), 4);
  });
}

/// 同批被过期 upsert 覆盖时不落任何 segment：胜者最终折叠为 delete。
#[test]
fn put_large_skips_segments_for_expired_upsert() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let mut expired = BatchEntry::upsert(b"big".to_vec(), pattern(20_000), 2);
    expired.expire_ts = 1;
    store
      .put_large(
        &tbl,
        &[
          BatchEntry::upsert(b"big".to_vec(), pattern(9000), 1),
          expired,
        ],
      )
      .await
      .unwrap();
    assert_eq!(live_segment_num(&store), 0, "过期 upsert 不得落段");
    assert_eq!(store.get(&tbl, b"big").await.unwrap(), None);
  });
}
