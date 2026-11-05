//! 27a：GC retained set 并集活跃 mapper 快照。
//!
//! 快照独占的旧文件在快照存活期间不得删除；快照 drop 后弱引用失效，
//! 下一轮 GC 即可回收（注册表不主动触发 GC）。

use std::collections::HashSet;

use fkv_btree::BatchEntry;
use fkv_io::{FileIo, PageIo};
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_mapping::PageMapper;
use fkv_store::Store;
use fkv_types::{FileId, TableIdent, decode_id, is_file_page_id};

fn pattern(len: usize, seed: u8) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8 ^ seed).collect()
}

/// mapper 当前引用到的文件集合。
fn file_set(mapper: &PageMapper, shift: u8) -> HashSet<FileId> {
  mapper
    .tbl()
    .iter_value_li()
    .filter(|value| is_file_page_id(*value))
    .map(|value| decode_id(value) >> shift)
    .collect()
}

/// 覆盖写之前已封存的文件：append 分配从 `max_fp_id` 之后继续，这些文件
/// 不会再接收新页，覆盖后只可能被旧快照引用。
async fn frozen_file_li<I: PageIo + FileIo>(
  store: &Store<I>,
  tbl: &TableIdent,
  shift: u8,
) -> Vec<FileId> {
  let tail_file = store.mapper(tbl).unwrap().max_fp_id() >> shift;
  store
    .page_io()
    .list_file_id_li(tbl)
    .await
    .unwrap()
    .into_iter()
    .filter(|file_id| *file_id < tail_file)
    .collect()
}

#[test]
fn gc_retains_active_snapshot_data_files_then_reclaims_after_drop() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 27).unwrap();
    let mut store =
      Store::new(MemStore::with_pages_per_file(512, 1), 512, 8).with_pages_per_file_shift(1);
    let batch: Vec<BatchEntry> = (0..40u32)
      .map(|index| BatchEntry::upsert(format!("k{index:03}").into_bytes(), vec![b'a'; 60], 1))
      .collect();
    store.put(&tbl, &batch).await.unwrap();

    let frozen_li = frozen_file_li(&store, &tbl, 1).await;
    assert!(
      frozen_li.len() >= 2,
      "需要多个已封存文件才能覆盖快照独占场景: {frozen_li:?}"
    );

    let snapshot = store.snapshot(&tbl).unwrap();
    assert!(snapshot.generation() > 0);
    assert!(snapshot.data_tbl().mapping_count() > 0);
    assert_eq!(snapshot.root_id(), store.root_id(&tbl).unwrap());
    assert_eq!(
      store.ttl_root_id(&tbl),
      Some(snapshot.ttl_root_id()),
      "快照固定取出时刻的 ttl root"
    );
    assert_eq!(
      snapshot.max_fp_id(),
      store.mapper(&tbl).unwrap().max_fp_id()
    );
    assert_eq!(store.snapshot_num(&tbl), 1);
    // clone 共享同一份快照：drop 一份不得摘链。
    let held = snapshot.clone();
    drop(snapshot);
    assert_eq!(store.snapshot_num(&tbl), 1, "clone 存活时不得摘链");

    // 全量覆盖：新页追加到更高 file id，旧文件只剩快照引用。
    let overwrite: Vec<BatchEntry> = (0..40u32)
      .map(|index| BatchEntry::upsert(format!("k{index:03}").into_bytes(), vec![b'b'; 60], 2))
      .collect();
    store.put(&tbl, &overwrite).await.unwrap();
    let current = file_set(store.mapper(&tbl).unwrap(), 1);
    assert!(
      frozen_li.iter().all(|file_id| !current.contains(file_id)),
      "覆盖后当前 mapping 不应再引用旧文件: frozen={frozen_li:?} current={current:?}"
    );

    let removed = store.file_gc(&tbl, 1).await.unwrap();
    assert_eq!(removed, 0, "快照独占的旧文件不得被回收");
    let file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
    for file_id in &frozen_li {
      assert!(
        file_li.contains(file_id),
        "file {file_id} 仍被活跃快照引用: {file_li:?}"
      );
    }

    drop(held);
    assert_eq!(store.snapshot_num(&tbl), 0, "最后一份快照 drop 后必须摘链");
    let removed = store.file_gc(&tbl, 1).await.unwrap();
    assert_eq!(
      removed,
      frozen_li.len(),
      "快照释放后旧文件应在下一轮 GC 回收"
    );
    let file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
    for file_id in &frozen_li {
      assert!(!file_li.contains(file_id), "file {file_id} 应已回收");
    }
    for index in 0..40u32 {
      let key = format!("k{index:03}");
      assert_eq!(
        store.get(&tbl, key.as_bytes()).await.unwrap(),
        Some(vec![b'b'; 60]),
        "GC 不得误删当前数据: {key}"
      );
    }
  });
}

#[test]
fn gc_retains_active_snapshot_segment_files_then_reclaims_after_drop() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 28).unwrap();
    let layout = SegmentLayout::new(4096, 1).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 1), 512, 16)
      .with_pages_per_file_shift(1)
      .with_large_value(layout, 1024);

    let first = pattern(20_000, 1);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), first.clone(), 1)],
      )
      .await
      .unwrap();
    let tail_segment_file = store.segment_mapper(&tbl).unwrap().max_fp_id() >> 1;
    let frozen_segment_li: Vec<FileId> = store
      .page_io()
      .list_segment_file_id_li(&tbl)
      .await
      .unwrap()
      .into_iter()
      .filter(|file_id| *file_id < tail_segment_file)
      .collect();
    assert!(
      !frozen_segment_li.is_empty(),
      "需要至少一个已封存 segment 文件"
    );

    let snapshot = store.snapshot(&tbl).unwrap();
    assert!(snapshot.segment_tbl().mapping_count() > 0);
    assert_eq!(
      snapshot.max_segment_fp_id(),
      store.segment_mapper(&tbl).unwrap().max_fp_id()
    );
    let second = pattern(20_000, 2);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), second.clone(), 2)],
      )
      .await
      .unwrap();

    let removed = store.file_gc(&tbl, 1).await.unwrap();
    assert_eq!(removed, 0, "快照独占的 segment 文件不得被回收");
    let segment_li = store.page_io().list_segment_file_id_li(&tbl).await.unwrap();
    for file_id in &frozen_segment_li {
      assert!(
        segment_li.contains(file_id),
        "segment file {file_id} 仍被活跃快照引用: {segment_li:?}"
      );
    }

    drop(snapshot);
    assert_eq!(store.snapshot_num(&tbl), 0);
    let removed = store.file_gc(&tbl, 1).await.unwrap();
    assert!(
      removed >= frozen_segment_li.len(),
      "快照释放后至少回收全部旧 segment 文件: removed={removed}"
    );
    let segment_li = store.page_io().list_segment_file_id_li(&tbl).await.unwrap();
    for file_id in &frozen_segment_li {
      assert!(
        !segment_li.contains(file_id),
        "segment file {file_id} 应已回收"
      );
    }
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      second,
      "GC 不得误删当前大 value"
    );
  });
}

/// 27c 用例 5：无活跃快照时零回归。
///
/// 覆盖写后的旧文件必须在**当轮** GC 全部回收（快照并集分支此时只是空表
/// 遍历，不把任何旧页并进 retained set），读路径看到的是新值 / 新扫描结果。
#[test]
fn gc_reclaims_old_files_immediately_without_active_snapshot() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 29).unwrap();
    let mut store =
      Store::new(MemStore::with_pages_per_file(512, 1), 512, 8).with_pages_per_file_shift(1);
    let first: Vec<BatchEntry> = (0..40u32)
      .map(|index| BatchEntry::upsert(format!("k{index:03}").into_bytes(), vec![b'a'; 60], 1))
      .collect();
    store.put(&tbl, &first).await.unwrap();
    let frozen_li = frozen_file_li(&store, &tbl, 1).await;
    assert!(
      frozen_li.len() >= 2,
      "需要多个已封存文件才能验证整轮回收: {frozen_li:?}"
    );
    assert_eq!(store.snapshot_num(&tbl), 0, "用例前提：无活跃快照");

    let second: Vec<BatchEntry> = (0..40u32)
      .map(|index| BatchEntry::upsert(format!("k{index:03}").into_bytes(), vec![b'b'; 60], 2))
      .collect();
    store.put(&tbl, &second).await.unwrap();
    let current = file_set(store.mapper(&tbl).unwrap(), 1);
    assert!(
      frozen_li.iter().all(|file_id| !current.contains(file_id)),
      "覆盖后当前 mapping 不应再引用旧文件: frozen={frozen_li:?} current={current:?}"
    );

    // 没有任何快照引用旧页：当轮 GC 必须全部回收，不能等下一轮。
    let removed = store.file_gc(&tbl, 1).await.unwrap();
    assert_eq!(
      removed,
      frozen_li.len(),
      "无快照时旧文件必须在当轮 GC 全部回收: frozen={frozen_li:?}"
    );
    let file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
    for file_id in &frozen_li {
      assert!(!file_li.contains(file_id), "file {file_id} 应已回收");
    }

    // 读路径零回归：点读 / 扫描都只看当前版本，且不遗留快照登记。
    for index in 0..40u32 {
      let key = format!("k{index:03}");
      assert_eq!(
        store.get(&tbl, key.as_bytes()).await.unwrap(),
        Some(vec![b'b'; 60]),
        "GC 后读当前值: {key}"
      );
    }
    let entry_li = store.scan(&tbl, b"", b"", 0).await.unwrap();
    assert_eq!(entry_li.len(), 40, "GC 后 scan 必须覆盖当前全部 key");
    assert_eq!(store.snapshot_num(&tbl), 0, "普通读写不得遗留快照登记");
  });
}
