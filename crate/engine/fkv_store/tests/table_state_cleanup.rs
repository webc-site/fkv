//! eng-73：每表状态收敛到 `TableState` 后，所有清理路径必须整条移除，
//! 不残留字典 / 页缓存 / TTL 待清理标记。

use fkv_btree::BatchEntry;
use fkv_io_mem::MemStore;
use fkv_store::Store;
use fkv_types::TableIdent;

fn large_batch() -> Vec<BatchEntry> {
  (0..500u32)
    .map(|i| BatchEntry::upsert(format!("k{i:05}").into_bytes(), vec![b'v'; 64], 1))
    .collect()
}

/// `evict_table` 一次性清掉整表状态：打开标记、字典、页缓存、TTL 待清理。
#[compio::test]
async fn evict_table_clears_whole_table_state() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(MemStore::new(256), 256, 4)
    .with_compression_level(3)
    .with_deferred_ttl(true);
  store.set_dictionary(&tbl, vec![b'd'; 1024]).unwrap();
  let mut ttl_entry = BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 1);
  ttl_entry.expire_ts = 100;
  // 过期项在 now=0 写入，now=200 的新写触发 pending 标记。
  store.put_at(&tbl, &[ttl_entry], 0).await.unwrap();
  store.put_at(&tbl, &large_batch(), 200).await.unwrap();
  assert!(store.get(&tbl, b"k00000").await.unwrap().is_some());

  assert!(store.is_open(&tbl));
  assert!(store.has_dictionary(&tbl));
  assert!(store.page_cache_stat().len > 0);
  assert_eq!(store.pending_ttl_len(), 1);

  store.evict_table(&tbl);
  assert!(!store.is_open(&tbl));
  assert!(!store.has_dictionary(&tbl));
  assert_eq!(store.page_cache_stat().len, 0);
  assert_eq!(store.pending_ttl_len(), 0);
  assert_eq!(store.root_id(&tbl), None);
  assert_eq!(store.ttl_root_id(&tbl), None);
  assert!(store.open_table_li().is_empty());
}

/// `drop_table` 走同一条清理路径，内存状态与打开列表都清零。
#[compio::test]
async fn drop_table_clears_whole_table_state() {
  let tbl = TableIdent::new("demo", 2).unwrap();
  let mut store = Store::new(MemStore::new(256), 256, 4).with_compression_level(3);
  store.set_dictionary(&tbl, vec![b'd'; 1024]).unwrap();
  store.put(&tbl, &large_batch()).await.unwrap();
  assert!(store.get(&tbl, b"k00000").await.unwrap().is_some());
  assert!(store.has_dictionary(&tbl));
  assert!(store.page_cache_stat().len > 0);

  store.drop_table(&tbl).await.unwrap();
  assert!(!store.is_open(&tbl));
  assert!(!store.has_dictionary(&tbl));
  assert_eq!(store.page_cache_stat().len, 0);
  assert!(store.open_table_li().is_empty());
}

/// 默认 `max_open_tables == 0` 不做任何驱逐，多表写入互不影响。
#[compio::test]
async fn default_config_keeps_all_tables_open() {
  let mut store = Store::new(MemStore::new(256), 256, 4);
  let tbl_li: Vec<TableIdent> = (1..=4)
    .map(|shard_id| TableIdent::new("demo", shard_id).unwrap())
    .collect();
  for tbl in &tbl_li {
    store
      .put(tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
      .await
      .unwrap();
  }
  assert_eq!(store.open_table_li().len(), tbl_li.len());
  for tbl in &tbl_li {
    assert_eq!(store.get(tbl, b"k").await.unwrap(), Some(b"v".to_vec()));
  }
}
