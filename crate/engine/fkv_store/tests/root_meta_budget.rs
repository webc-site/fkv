//! rev-42a：表级 root 状态字节预算 + LRU 驱逐（对齐 C++ `RootMetaMgr`）。
//!
//! 预算计入 data / segment mapping、index 缓存、字典与 branch 元数据；
//! 超限时从 LRU 尾驱逐整表状态，但不驱逐正在被读快照引用的表。

use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_mapping::CHUNK_SIZE;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn batch(num: u32, tag: u8) -> Vec<BatchEntry> {
  (0..num)
    .map(|index| BatchEntry::upsert(format!("k{index:05}").into_bytes(), vec![tag; 32], 1))
    .collect()
}

fn mapping_bytes(store: &Store<MemStore>, tbl: &TableIdent) -> usize {
  let len = store.mapper(tbl).unwrap().tbl().len();
  len.div_ceil(CHUNK_SIZE) * CHUNK_SIZE * size_of::<u64>()
}

/// 超预算时按 LRU 驱逐最久未写的表；被驱逐表 reopen 后数据完整。
#[compio::test]
async fn root_meta_budget_evicts_lru_table_and_reloads() {
  let root = TempRoot::new("store_root_meta_lru");
  let io = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  // 1 字节预算：任何表状态都超限。当前正在写的表不能驱逐自己，
  // 于是下一次触摸其它表时，最久未写的表成为 LRU 尾候选。
  let mut store = Store::new(io, 512, 16).with_root_meta_cache_bytes(1);
  let tbl_a = TableIdent::new("demo", 1).unwrap();
  let tbl_b = TableIdent::new("demo", 2).unwrap();

  store.put(&tbl_a, &batch(64, b'a')).await.unwrap();
  assert!(store.root_meta_bytes() > 1);
  assert!(store.mapper(&tbl_a).is_some(), "当前表不得驱逐自己");

  store.put(&tbl_b, &batch(64, b'b')).await.unwrap();
  assert!(store.mapper(&tbl_a).is_none(), "最久未写的表必须被驱逐");
  assert!(store.mapper(&tbl_b).is_some());

  // 整表状态已卸载，reopen 从 manifest 重载数据。
  assert!(!store.is_open(&tbl_a));
  store.reopen(&tbl_a).await.unwrap();
  assert_eq!(
    store.get(&tbl_a, b"k00000").await.unwrap(),
    Some(vec![b'a'; 32])
  );
}

/// 活跃读快照引用的表不得被预算驱逐；快照释放后下一次触摸才回收。
#[compio::test]
async fn root_meta_budget_keeps_tables_with_live_snapshot() {
  let mut store = Store::new(MemStore::new(256), 256, 4).with_root_meta_cache_bytes(1);
  let tbl_a = TableIdent::new("demo", 1).unwrap();
  let tbl_b = TableIdent::new("demo", 2).unwrap();

  store.put(&tbl_a, &batch(64, b'a')).await.unwrap();
  let held = store.snapshot(&tbl_a).unwrap();
  assert_eq!(store.snapshot_num(&tbl_a), 1);

  store.put(&tbl_b, &batch(64, b'b')).await.unwrap();
  assert!(store.mapper(&tbl_a).is_some(), "被读快照引用的表不得驱逐");
  assert!(store.mapper(&tbl_b).is_some());

  drop(held);
  assert_eq!(store.snapshot_num(&tbl_a), 0);
  // 下一次写触摸 B 触发预算复查：A 已无快照，成为 LRU 尾候选。
  store.put(&tbl_b, &batch(1, b'c')).await.unwrap();
  assert!(
    store.mapper(&tbl_a).is_none(),
    "快照释放后应由下一轮触摸回收"
  );
}

/// index 缓存计入 root meta 预算；驱逐整表状态时一并释放。
#[compio::test]
async fn root_meta_bytes_include_index_cache_and_release_on_evict() {
  let mut store = Store::new(MemStore::new(256), 256, 4).with_root_meta_cache_bytes(1);
  let tbl_a = TableIdent::new("demo", 1).unwrap();
  let tbl_b = TableIdent::new("demo", 2).unwrap();

  store.put(&tbl_a, &batch(2000, b'a')).await.unwrap();
  // 读全树路径，填充该表 index 缓存。
  for index in (0..2000u32).step_by(3) {
    store
      .get(&tbl_a, format!("k{index:05}").as_bytes())
      .await
      .unwrap()
      .unwrap();
  }
  let cache_bytes = store.index_cache_stat().bytes;
  assert!(cache_bytes > 0, "读路径必须填充 index 缓存");
  let mapping = mapping_bytes(&store, &tbl_a);
  assert!(
    store.root_meta_bytes() >= mapping + cache_bytes,
    "root meta 记账必须包含 mapping 与 index 缓存: used={} mapping={mapping} cache={cache_bytes}",
    store.root_meta_bytes()
  );

  store.put(&tbl_b, &batch(8, b'b')).await.unwrap();
  assert!(store.mapper(&tbl_a).is_none());
  assert_eq!(
    store.index_cache_len(&tbl_a),
    0,
    "驱逐必须同时释放该表 index 缓存"
  );
  assert!(
    store.index_cache_stat().bytes < cache_bytes,
    "聚合缓存占用应随整表驱逐下降"
  );
}

/// 两项上限都为 0 时保持默认零开销语义：不记账、不驱逐。
#[compio::test]
async fn root_meta_budget_default_is_unlimited() {
  let mut store = Store::new(MemStore::new(256), 256, 4);
  assert_eq!(store.root_meta_cache_bytes(), 0);
  let tbl_li: Vec<TableIdent> = (1..=4)
    .map(|shard_id| TableIdent::new("demo", shard_id).unwrap())
    .collect();
  for tbl in &tbl_li {
    store.put(tbl, &batch(8, b'v')).await.unwrap();
  }
  assert_eq!(store.open_table_li().len(), tbl_li.len());
  assert!(store.root_meta_bytes() > 0);
}
