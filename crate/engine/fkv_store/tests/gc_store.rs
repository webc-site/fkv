use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io::FileIo;
use fkv_io_mem::MemStore;
use fkv_store::Store;
use fkv_types::TableIdent;

#[test]
fn compact_gc_and_reopen() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::with_pages_per_file(128, 1), 128, 8);
    let mut batch = Vec::new();
    for i in 0..100u32 {
      batch.push(BatchEntry::upsert(
        format!("k{i:03}").into_bytes(),
        vec![b'v'; 30],
        1,
      ));
    }
    store.put(&tbl, &batch).await.unwrap();
    let mut file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
    file_li.sort_unstable();
    assert!(file_li.len() > 2);
    let mut target = None;
    let mut moved = 0;
    for file_id in &file_li {
      let count = store.compact_file(&tbl, *file_id, 1).await.unwrap();
      if count > 0 {
        target = Some(*file_id);
        moved = count;
        break;
      }
    }
    let target = target.expect("no file with live pages");
    assert!(moved > 0);
    let removed = store.file_gc(&tbl, 1).await.unwrap();
    assert!(removed >= 1);
    assert!(
      !store
        .page_io()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .contains(&target)
    );
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 100);

    let io = store.into_io();
    let mut reopened = Store::new(io, 128, 8);
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(reopened.scan(&tbl, b"", b"", 0).await.unwrap().len(), 100);
  });
}

/// issue 34：放大率超过 `file_amplify_factor` 时 `compact_if_needed` 触发重写，
/// `maintain`/`gc` 后文件数回落且数据完整。
#[test]
fn amplify_factor_triggers_compaction_and_gc() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 2).unwrap();
    let mut store = Store::new(MemStore::with_pages_per_file(128, 1), 128, 8)
      .with_pages_per_file_shift(1)
      .with_file_amplify_factor(1);
    for round in 1..=4u32 {
      let batch: Vec<BatchEntry> = (0..60u32)
        .map(|index| {
          BatchEntry::upsert(
            format!("k{index:03}").into_bytes(),
            vec![b'v'; 30],
            u64::from(round),
          )
        })
        .collect();
      store.put(&tbl, &batch).await.unwrap();
    }
    let file_before = store.page_io().list_file_id_li(&tbl).await.unwrap().len();
    let mut round = 0;
    let mut compacted = 0;
    loop {
      let plan_li = store.compact_if_needed(&tbl).await.unwrap();
      if plan_li.is_empty() {
        break;
      }
      compacted += plan_li.len();
      round += 1;
      assert!(round < 1000, "compaction must make progress");
    }
    assert!(compacted > 0, "amplified files must be compacted");
    store.file_gc(&tbl, 1).await.unwrap();
    let file_after = store.page_io().list_file_id_li(&tbl).await.unwrap().len();
    assert!(
      file_after < file_before,
      "gc must drop garbage files: before={file_before} after={file_after}"
    );
    for index in 0..60u32 {
      let key = format!("k{index:03}");
      assert_eq!(
        store.get(&tbl, key.as_bytes()).await.unwrap(),
        Some(vec![b'v'; 30])
      );
    }
  });
}

/// 启用 `file_amplify_factor` 后每批写后调用 `maintain`，文件数应显著低于裸写。
#[test]
fn maintain_after_put_keeps_file_count_bounded() {
  async fn run(maintained: bool) -> usize {
    let tbl = TableIdent::new("demo", 3).unwrap();
    let mut store =
      Store::new(MemStore::with_pages_per_file(128, 2), 128, 8).with_pages_per_file_shift(2);
    if maintained {
      store = store.with_file_amplify_factor(2);
    }
    for round in 1..=6u32 {
      let batch: Vec<BatchEntry> = (0..60u32)
        .map(|index| {
          BatchEntry::upsert(
            format!("k{index:03}").into_bytes(),
            vec![b'v'; 30],
            u64::from(round),
          )
        })
        .collect();
      if maintained {
        store.put(&tbl, &batch).await.unwrap();
        store.maintain(&tbl).await.unwrap();
      } else {
        store.put(&tbl, &batch).await.unwrap();
      }
    }
    assert_eq!(
      store.get(&tbl, b"k000").await.unwrap(),
      Some(vec![b'v'; 30])
    );
    store.page_io().list_file_id_li(&tbl).await.unwrap().len()
  }
  pollster::block_on(async {
    let plain = run(false).await;
    let maintained = run(true).await;
    assert!(
      maintained < plain,
      "maintained={maintained} plain={plain}; amplification must be reclaimed"
    );
  });
}

/// sto-56：整表 truncate 后 mapping 为空，维护一轮后 `needs_maintain` 不再为真
/// （对齐 C++ 空 fp_ids 时把分配器 high-water 推到下一文件边界）。
#[test]
fn empty_mapping_stops_repeated_maintenance() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 4).unwrap();
    let mut store = Store::new(MemStore::with_pages_per_file(128, 1), 128, 8)
      .with_pages_per_file_shift(1)
      .with_file_amplify_factor(1);
    let batch: Vec<BatchEntry> = (0..20u32)
      .map(|index| {
        BatchEntry::upsert(
          format!("k{index:02}").into_bytes(),
          vec![b'v'; 30],
          u64::from(index),
        )
      })
      .collect();
    store.put(&tbl, &batch).await.unwrap();
    store.truncate(&tbl).await.unwrap();
    assert!(
      store.needs_maintain(&tbl),
      "空 mapping 仍占空间，必须触发一轮维护"
    );

    store.maintain(&tbl).await.unwrap();
    assert!(!store.needs_maintain(&tbl), "跳界后不应再触发");
    assert!(
      store
        .page_io()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty(),
      "维护后死文件必须被 GC 回收"
    );
  });
}

/// core-28：空表 reopen 后 `space_size` 从尾文件起算，对齐边界时
/// `needs_maintain` 直接为假（对齐 C++ 空 reopen 的 `Replayer::GetMapper`）。
#[test]
fn empty_reopen_space_stat_starts_from_tail_file() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 6).unwrap();
    let mut store = Store::new(MemStore::with_pages_per_file(128, 1), 128, 8)
      .with_pages_per_file_shift(1)
      .with_file_amplify_factor(1);
    // 写到 max_fp_id 正好落在文件边界，制造「对齐情形」。
    let mut index = 0u32;
    while store
      .mapper(&tbl)
      .is_none_or(|mapper| mapper.max_fp_id() % 2 != 0)
    {
      let entries = [BatchEntry::upsert(
        format!("k{index:04}").into_bytes(),
        vec![b'v'; 30],
        1,
      )];
      store.put(&tbl, &entries).await.unwrap();
      index += 1;
      assert!(index < 64, "未能把 max_fp_id 对到文件边界");
    }
    // truncate 只释放逻辑页，不推进 max_fp_id；快照按当前 max 落盘。
    store.truncate(&tbl).await.unwrap();
    let before_max = store.mapper(&tbl).unwrap().max_fp_id();
    assert_eq!(before_max % 2, 0);
    assert!(store.needs_maintain(&tbl), "空 mapping 仍占历史空间");

    let io = store.into_io();
    let mut reopened = Store::new(io, 128, 8)
      .with_pages_per_file_shift(1)
      .with_file_amplify_factor(1);
    reopened.reopen(&tbl).await.unwrap();
    let mapper = reopened.mapper(&tbl).unwrap();
    assert_eq!(mapper.max_fp_id(), before_max);
    assert_eq!(mapper.min_file_id(), before_max >> 1);
    assert_eq!(mapper.space_size(1), 0, "对齐时只余尾文件残量 0");
    assert!(
      !reopened.needs_maintain(&tbl),
      "空表对齐 reopen 后不得再触发全量维护"
    );
  });
}

/// sto-56c：放大触发点是 O(1) 派生预判，一轮维护后必须清零，
/// 后续不产生新的写就不该再触发 compact（等价 C++ `UpdateMeta(trigger_compact=false)`）。
#[test]
fn maintain_clears_amplification_trigger() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 5).unwrap();
    let mut store = Store::new(MemStore::with_pages_per_file(128, 1), 128, 8)
      .with_pages_per_file_shift(1)
      .with_file_amplify_factor(1);
    let batch: Vec<BatchEntry> = (0..60u32)
      .map(|index| BatchEntry::upsert(format!("k{index:03}").into_bytes(), vec![b'v'; 30], 1))
      .collect();
    for _ in 0..3 {
      store.put(&tbl, &batch).await.unwrap();
    }
    assert!(store.needs_maintain(&tbl), "覆盖写后放大率必须触发维护");

    store.maintain(&tbl).await.unwrap();
    assert!(!store.needs_maintain(&tbl), "一轮维护后触发点必须清零");
    assert!(
      store.compact_if_needed(&tbl).await.unwrap().is_empty(),
      "没有新写时不得再空转 compact"
    );
    assert_eq!(
      store.get(&tbl, b"k000").await.unwrap(),
      Some(vec![b'v'; 30])
    );
  });
}

/// 见到更高 term 的 manifest 时整轮 GC 返回 ExpiredTerm，且不删任何文件
/// （对齐 C++ `ExecuteCloudGC` 的 term 熔断）。
#[test]
fn gc_aborts_on_manifest_term_above_process_term() {
  use fkv_io::ManifestIo;
  use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArg};
  use fkv_types::BranchFileRange;

  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store =
      Store::new(MemStore::with_pages_per_file(128, 1), 128, 8).with_branch("main", 5);
    store
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    let before = store.page_io().list_file_id_li(&tbl).await.unwrap();
    assert!(!before.is_empty());

    let metadata = BranchManifestMetadata {
      branch_name: "main".into(),
      term: 7,
      file_range_li: vec![BranchFileRange {
        branch_name: "main".into(),
        term: 7,
        max_file_id: 10,
        max_segment_file_id: 0,
      }],
    };
    let bytes = ManifestBuilder::new()
      .snapshot(
        SnapshotArg {
          root_id: 0,
          ttl_root: 0,
          max_fp_id: 10 << 1,
          dict_bytes: &[],
          branch_metadata: &metadata,
          max_segment_fp_id: 0,
        },
        &[],
        None,
      )
      .unwrap();
    store
      .page_io()
      .write_manifest_file(&tbl, "manifest_main_7", &bytes)
      .await
      .unwrap();

    assert_eq!(
      store.file_gc(&tbl, 1).await.unwrap_err(),
      KvError::ExpiredTerm
    );
    assert_eq!(store.page_io().list_file_id_li(&tbl).await.unwrap(), before);
  });
}

/// sem-26：term 熔断按 active branch 过滤（attach term 是 per-branch）。
///
/// - 只有其它 branch（祖先 main）的更高 term：全局删除闸门仍整轮跳过，但返回
///   `Ok(0)` 而不是 `ExpiredTerm` —— 本 branch 没有被同 branch 更高 term 接管。
/// - 同 branch 出现更高 term：返回 `ExpiredTerm`，同样整轮不删。
#[test]
fn gc_term_fence_is_filtered_by_active_branch() {
  use fkv_io::{FileIo, ManifestIo};
  use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArg};
  use fkv_types::BranchFileRange;

  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut main = Store::new(MemStore::with_pages_per_file(128, 1), 128, 8)
      .with_branch("main", 5)
      .with_pages_per_file_shift(1);
    main
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    main.create_branch(&tbl, "feature").await.unwrap();

    let mut branch = Store::new(main.into_io(), 128, 8).with_pages_per_file_shift(1);
    branch.open_branch(&tbl, "feature", 3).await.unwrap();
    branch
      .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1)])
      .await
      .unwrap();

    let manifest = |branch_name: &str, term: u64| {
      let metadata = BranchManifestMetadata {
        branch_name: branch_name.into(),
        term,
        file_range_li: vec![BranchFileRange {
          branch_name: branch_name.into(),
          term,
          max_file_id: 10,
          max_segment_file_id: 0,
        }],
      };
      ManifestBuilder::new()
        .snapshot(
          SnapshotArg {
            root_id: 0,
            ttl_root: 0,
            max_fp_id: 10 << 1,
            dict_bytes: &[],
            branch_metadata: &metadata,
            max_segment_fp_id: 0,
          },
          &[],
          None,
        )
        .unwrap()
    };

    // 祖先 main 的更高 term：全局闸门跳过删除，但不构成本 branch 过期。
    let main_above = manifest("main", 7);
    branch
      .page_io()
      .write_manifest_file(&tbl, "manifest_main_7", &main_above)
      .await
      .unwrap();
    let before = branch.page_io().list_file_id_li(&tbl).await.unwrap();
    assert_eq!(
      branch.file_gc(&tbl, 1).await.unwrap(),
      0,
      "只看到其它 branch 更高 term 时本轮 GC 必须跳过删除"
    );
    assert_eq!(
      branch.page_io().list_file_id_li(&tbl).await.unwrap(),
      before
    );

    // 同 branch 更高 term：本 owner 过期，整轮不删。
    let feature_above = manifest("feature", 7);
    branch
      .page_io()
      .write_manifest_file(&tbl, "manifest_feature_7", &feature_above)
      .await
      .unwrap();
    assert_eq!(
      branch.file_gc(&tbl, 1).await.unwrap_err(),
      KvError::ExpiredTerm
    );
    assert_eq!(
      branch.page_io().list_file_id_li(&tbl).await.unwrap(),
      before
    );
  });
}
