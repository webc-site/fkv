use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, TableIdent};

/// eng-68：写一个 key 不应清空整表 index 缓存，未变更子树继续命中。
#[compio::test]
async fn write_keeps_unmodified_index_pages_cached() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(MemStore::new(256), 256, 4);
  let batch: Vec<BatchEntry> = (0..2000u32)
    .map(|i| BatchEntry::upsert(format!("k{i:06}").into_bytes(), vec![b'v'; 8], 1))
    .collect();
  store.put(&tbl, &batch).await.unwrap();
  assert!(store.get(&tbl, b"k001000").await.unwrap().is_some());
  let warm = store.index_cache_len(&tbl);
  assert!(warm > 1, "warm cache should hold multiple index pages");

  store
    .put(
      &tbl,
      &[BatchEntry::upsert(b"new".to_vec(), b"v".to_vec(), 2)],
    )
    .await
    .unwrap();
  let after = store.index_cache_len(&tbl);
  assert!(
    after + 4 >= warm,
    "write must not clear the whole cache (warm={warm}, after={after})"
  );
}

/// eng-68：compact 只换物理 FilePageId，逻辑页内容不变。
/// 搬迁记录必须在 compact 出口丢弃，否则下一次 put 会把整批热点索引页逐出。
#[compio::test]
async fn compact_keeps_index_cache_for_next_write() {
  use fkv_io::FileIo;

  let tbl = TableIdent::new("demo", 3).unwrap();
  let mut store = Store::new(MemStore::with_pages_per_file(256, 3), 256, 4);
  let batch: Vec<BatchEntry> = (0..2000u32)
    .map(|i| BatchEntry::upsert(format!("k{i:06}").into_bytes(), vec![b'v'; 8], 1))
    .collect();
  store.put(&tbl, &batch).await.unwrap();

  // 读全树路径，尽量填满 index 缓存。
  for i in (0..2000u32).step_by(3) {
    let key = format!("k{i:06}");
    assert!(store.get(&tbl, key.as_bytes()).await.unwrap().is_some());
  }
  let warm = store.index_cache_len(&tbl);
  assert!(warm >= 3, "warm cache too small: {warm}");

  let mut file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
  file_li.sort_unstable();
  let mut moved = 0;
  for file_id in file_li {
    moved += store.compact_file(&tbl, file_id, 3).await.unwrap();
  }
  assert!(moved > 0, "compaction must move live pages");
  assert!(
    store.mapper(&tbl).unwrap().changed_page_li().is_empty(),
    "compact must discard physical remap records"
  );
  assert_eq!(
    store.index_cache_len(&tbl),
    warm,
    "compaction alone must not evict index pages"
  );

  store
    .put(
      &tbl,
      &[BatchEntry::upsert(b"zzz".to_vec(), b"v".to_vec(), 2)],
    )
    .await
    .unwrap();
  let after = store.index_cache_len(&tbl);
  // 未修复时 compact 记录会把全部已缓存索引页在这一次 put 中逐出（after == 0）。
  assert!(
    after >= 2,
    "post-compact write must not mass-evict (warm={warm}, after={after})"
  );
}

/// eng-68：truncate 已整表清空缓存，逐页释放记录无意义，必须在出口丢弃。
#[compio::test]
async fn truncate_discards_changed_page_li() {
  let tbl = TableIdent::new("demo", 4).unwrap();
  let mut store = Store::new(MemStore::new(256), 256, 4);
  let batch: Vec<BatchEntry> = (0..500u32)
    .map(|i| BatchEntry::upsert(format!("k{i:05}").into_bytes(), vec![b'v'; 8], 1))
    .collect();
  store.put(&tbl, &batch).await.unwrap();
  assert!(store.get(&tbl, b"k00000").await.unwrap().is_some());
  assert!(store.index_cache_len(&tbl) > 0);

  store.truncate(&tbl).await.unwrap();
  assert_eq!(store.index_cache_len(&tbl), 0);
  assert!(
    store.mapper(&tbl).unwrap().changed_page_li().is_empty(),
    "truncate must discard physical free records"
  );
}

/// eng-66：命中 / 未命中 / 占用可观测，容量旋钮真正生效。
#[compio::test]
async fn cache_stat_and_budget_knob() {
  let tbl = TableIdent::new("demo", 2).unwrap();
  // 预算约一页：第二条插入即超预算，Clock 逐出保留最新一页。
  let mut tiny = Store::new(MemStore::new(256), 256, 4).with_index_cache_bytes(512);
  let batch: Vec<BatchEntry> = (0..2000u32)
    .map(|i| BatchEntry::upsert(format!("k{i:06}").into_bytes(), vec![b'v'; 8], 1))
    .collect();
  tiny.put(&tbl, &batch).await.unwrap();
  for i in (0..2000u32).step_by(7) {
    assert!(
      tiny
        .get(&tbl, format!("k{i:06}").as_bytes())
        .await
        .unwrap()
        .is_some()
    );
  }
  let stat = tiny.index_cache_stat();
  assert_eq!(stat.capacity, 512);
  assert!(stat.miss > 0, "cold reads must count misses");
  assert!(stat.evict > 0, "tiny budget must evict index pages");
  assert!(
    stat.len <= 1,
    "clock eviction keeps at most one page: {stat:?}"
  );
  assert!(stat.bytes > 0);

  // 默认预算：同一 key 连读两次，第二次全部命中。
  let mut store = Store::new(MemStore::new(256), 256, 4);
  store.put(&tbl, &batch).await.unwrap();
  assert!(store.get(&tbl, b"k001000").await.unwrap().is_some());
  let cold = store.index_cache_stat();
  assert_eq!(cold.hit, 0);
  assert!(cold.miss > 0);
  assert!(cold.evict == 0);
  assert!(store.get(&tbl, b"k001000").await.unwrap().is_some());
  let warm = store.index_cache_stat();
  assert!(warm.hit > 0, "second read must hit: {warm:?}");
  assert_eq!(warm.miss, cold.miss);
  assert!(warm.bytes > 0);
  assert_eq!(warm.capacity, fkv_btree::DEFAULT_INDEX_CACHE_BYTES);
}

/// eng-69：多表共享同一份 index 缓存聚合预算，总量不超上限且各表都有缓存。
#[compio::test]
async fn aggregate_budget_shares_across_tables() {
  const TOTAL: usize = 4096;
  let (a, b) = (
    TableIdent::new("alpha", 11).unwrap(),
    TableIdent::new("beta", 12).unwrap(),
  );
  let mut store = Store::new(MemStore::new(256), 256, 4).with_index_cache_total_bytes(TOTAL);
  let batch: Vec<BatchEntry> = (0..2000u32)
    .map(|i| BatchEntry::upsert(format!("k{i:06}").into_bytes(), vec![b'v'; 8], 1))
    .collect();

  // A 先建缓存：此时只有一份缓存，可独占整池。
  store.put(&a, &batch).await.unwrap();
  for i in (0..2000u32).step_by(5) {
    store.get(&a, format!("k{i:06}").as_bytes()).await.unwrap();
  }
  assert!(store.index_cache_len(&a) > 0);

  // B 建缓存后均分额度减半，A 必须被同步裁剪。
  store.put(&b, &batch).await.unwrap();
  for i in (0..2000u32).step_by(5) {
    store.get(&b, format!("k{i:06}").as_bytes()).await.unwrap();
  }
  let stat = store.index_cache_stat();
  assert_eq!(stat.capacity, fkv_btree::DEFAULT_INDEX_CACHE_BYTES);
  assert!(stat.bytes <= TOTAL, "aggregate budget must hold: {stat:?}");
  assert!(store.index_cache_len(&a) > 0, "table a keeps its share");
  assert!(store.index_cache_len(&b) > 0, "table b keeps its share");
}

/// perf-06：代际来源按 Store 归属。同一 Store 内重建 mapper（reopen）继续消费
/// 同一序列，旧 / 新化身不重叠；两个 Store 的序列互不影响（旧实现共用进程级
/// 全局原子，跨 shard 争同一 cacheline）。
#[compio::test]
async fn generation_source_is_per_store_and_survives_reopen() {
  let tbl = TableIdent::new("demo", 12).unwrap();
  let batch = [BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)];

  let mut a = Store::new(MemStore::new(256), 256, 4);
  a.put(&tbl, &batch).await.unwrap();
  let a_before = a.mapper(&tbl).unwrap().generation();

  let mut b = Store::new(MemStore::new(256), 256, 4);
  b.put(&tbl, &batch).await.unwrap();
  assert_eq!(
    b.mapper(&tbl).unwrap().generation(),
    a_before,
    "两个 Store 各持独立来源，同样的写操作应得到同一代际"
  );

  a.reopen(&tbl).await.unwrap();
  let a_after = a.mapper(&tbl).unwrap().generation();
  assert!(
    a_after > a_before,
    "同一 Store 的 mapper 化身必须共享并推进代际序列"
  );
  assert_eq!(
    b.mapper(&tbl).unwrap().generation(),
    a_before,
    "A 的 reopen 不得推进 B 的代际"
  );
}

/// eng-67：term 变化后 file id 空间不可信，缓存必须整表丢弃。
#[compio::test]
async fn reopen_drops_index_cache_when_term_changes() {
  let tbl = TableIdent::new("demo", 6).unwrap();
  let mut store = Store::new(MemStore::new(256), 256, 4);
  let batch: Vec<BatchEntry> = (0..1000u32)
    .map(|i| BatchEntry::upsert(format!("k{i:06}").into_bytes(), vec![b'v'; 8], 1))
    .collect();
  store.put(&tbl, &batch).await.unwrap();
  assert!(store.get(&tbl, b"k000500").await.unwrap().is_some());
  assert!(store.index_cache_len(&tbl) > 0);

  store.set_term(3);
  store.reopen(&tbl).await.unwrap();
  assert_eq!(
    store.index_cache_len(&tbl),
    0,
    "term change must drop the whole index cache"
  );
  // 缓存丢弃不丢数据：reopen 后仍可读。
  assert!(store.get(&tbl, b"k000500").await.unwrap().is_some());
}

/// core-27：reopen 挂新缓存后必须同步裁剪既有缓存。
///
/// A 独占整池，B 重新 open 使均分额度减半；修复前 A 要等下一次写才收缩，
/// B 再涨满自己的份额就长期突破聚合上限。
#[compio::test]
async fn reopen_rebalances_cache_budget() {
  const TOTAL: usize = 4096;
  let (a, b) = (
    TableIdent::new("alpha", 21).unwrap(),
    TableIdent::new("beta", 22).unwrap(),
  );
  let mut store = Store::new(MemStore::new(256), 256, 4).with_index_cache_total_bytes(TOTAL);
  let batch: Vec<BatchEntry> = (0..2000u32)
    .map(|i| BatchEntry::upsert(format!("k{i:06}").into_bytes(), vec![b'v'; 8], 1))
    .collect();

  // B 先落盘再驱逐：reopen 时按新实例挂缓存（不是复用旧缓存）。
  store.put(&b, &batch).await.unwrap();
  store.evict_table(&b);

  // A 独占整池：warm 后占用接近 TOTAL。
  store.put(&a, &batch).await.unwrap();
  for i in 0..2000u32 {
    assert!(
      store
        .get(&a, format!("k{i:06}").as_bytes())
        .await
        .unwrap()
        .is_some()
    );
  }
  let alone = store.index_cache_stat();
  assert!(
    alone.bytes > TOTAL / 2,
    "warm cache should use most of the pool: {alone:?}"
  );
  let warm_len = store.index_cache_len(&a);

  // reopen B：只读不写也必须让 A 按新份额收缩。
  store.reopen(&b).await.unwrap();
  let reopened = store.index_cache_stat();
  assert!(
    store.index_cache_len(&a) < warm_len,
    "reopen must trim the existing cache (warm={warm_len})"
  );
  assert!(
    reopened.bytes <= TOTAL,
    "aggregate budget must hold right after reopen: {reopened:?}"
  );

  // B 涨满自己的份额后总量仍不得越界。
  for i in 0..2000u32 {
    assert!(
      store
        .get(&b, format!("k{i:06}").as_bytes())
        .await
        .unwrap()
        .is_some()
    );
  }
  let both = store.index_cache_stat();
  assert!(
    both.bytes <= TOTAL,
    "aggregate budget must hold after both warm: {both:?}"
  );
  assert!(store.index_cache_len(&b) > 0);
}

/// r7a-core-01：写批次回滚后缓存 baseline 抬到回滚后的 live 代际。
///
/// 回滚让映射内容回到批次前，但代际继续前进：清空条目 + 抬 baseline 之后，
/// 新读者仍能正常回填并命中（baseline 没有越过 live 代际），在途旧快照的
/// 回填则因代际更早被拒。用文件后端的 failpoint 制造 manifest 写失败，
/// 与真实回滚路径一致。
#[compio::test]
async fn failed_write_resets_cache_to_live_generation() {
  let root = TempRoot::new("store_index_cache_abort");
  let tbl = TableIdent::new("demo", 11).unwrap();
  let mut store = Store::new(
    CompioStore::new(root.path().to_path_buf(), 512, DEFAULT_PAGES_PER_FILE_SHIFT),
    512,
    16,
  );
  let batch: Vec<BatchEntry> = (0..2000u32)
    .map(|i| BatchEntry::upsert(format!("k{i:06}").into_bytes(), vec![b'v'; 8], 1))
    .collect();
  store.put(&tbl, &batch).await.unwrap();
  // 先 warm：缓存里出现 baseline 代条目。
  for i in (0..2000u32).step_by(97) {
    assert!(
      store
        .get(&tbl, format!("k{i:06}").as_bytes())
        .await
        .unwrap()
        .is_some()
    );
  }
  assert!(store.index_cache_len(&tbl) > 0);

  // 第二次写入走增量 manifest，注入写失败迫使 mapper 批次回滚。
  fkv_failpoint::set_spec("AppendManifest:Write@1!err=IoFail");
  let failed = store
    .put(
      &tbl,
      &[BatchEntry::upsert(b"zzz".to_vec(), b"Z".to_vec(), 2)],
    )
    .await;
  fkv_failpoint::reset();
  assert!(failed.is_err(), "注入失败必须让写返回错误");
  assert_eq!(store.index_cache_len(&tbl), 0, "回滚必须清空该表缓存");

  // 回滚后新读者必须能重新回填：baseline 若越过 live 代际，insert 会被全拒。
  for i in (0..2000u32).step_by(97) {
    assert!(
      store
        .get(&tbl, format!("k{i:06}").as_bytes())
        .await
        .unwrap()
        .is_some(),
      "回滚不得影响已提交数据"
    );
  }
  assert!(
    store.index_cache_len(&tbl) > 0,
    "回滚后缓存必须还能回填（baseline 不得高于 live 代际）"
  );

  // 后续成功写把 baseline 推进到新代际，读仍然命中缓存。
  store
    .put(
      &tbl,
      &[BatchEntry::upsert(b"zzz".to_vec(), b"Z".to_vec(), 3)],
    )
    .await
    .unwrap();
  assert_eq!(store.get(&tbl, b"zzz").await.unwrap(), Some(b"Z".to_vec()));
  let warm = store.index_cache_len(&tbl);
  assert!(warm > 0, "成功写后的 retag 必须让缓存继续可用");
  for i in (0..2000u32).step_by(97) {
    assert!(
      store
        .get(&tbl, format!("k{i:06}").as_bytes())
        .await
        .unwrap()
        .is_some()
    );
  }
}
