use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io::{ManifestIo, PageIo};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_store::{CleanExpiredStats, ScanOptions, Store};
use fkv_task_write::{is_expired, parse_ttl_key, ttl_key, ttl_upsert_batch};
use fkv_types::{PageId, TableIdent};

fn upsert_with_expire(key: &[u8], value: &[u8], ts: u64, expire_ts: u64) -> BatchEntry {
  let mut entry = BatchEntry::upsert(key.to_vec(), value.to_vec(), ts);
  entry.expire_ts = expire_ts;
  entry
}

/// 在 now=0 写入，避免测试里的历史时间戳被写时过滤掉。
async fn seed<I: PageIo + ManifestIo>(
  store: &mut Store<I>,
  tbl: &TableIdent,
  batch: &[BatchEntry],
) {
  store.put_at(tbl, batch, 0).await.unwrap();
}

fn temp_root(tag: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_ttl_{tag}_{}_{}_{}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos(),
    seq,
  ));
  create_dir_all(&root).unwrap();
  root
}

#[test]
fn ttl_key_layout_and_batch() {
  let key = ttl_key(0x0102_0304_0506_0708, b"user");
  assert_eq!(&key[..8], &0x0102_0304_0506_0708u64.to_be_bytes());
  assert_eq!(&key[8..], b"user");
  assert_eq!(
    parse_ttl_key(&key).unwrap(),
    (0x0102_0304_0506_0708, &b"user"[..])
  );
  assert_eq!(parse_ttl_key(b"short").unwrap_err(), KvError::Corrupted);
  assert!(ttl_key(1, b"z") < ttl_key(2, b"a"));
  assert!(is_expired(100, 100));
  assert!(!is_expired(101, 100));
  assert!(!is_expired(0, u64::MAX));

  let mut deleted = BatchEntry::delete(b"gone".to_vec(), 1);
  deleted.expire_ts = 50;
  let batch = [
    upsert_with_expire(b"a", b"A", 1, 100),
    upsert_with_expire(b"b", b"B", 1, 50),
    upsert_with_expire(b"a", b"A2", 2, 100),
    BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 1),
    deleted,
  ];
  let ttl_batch = ttl_upsert_batch(&batch);
  assert_eq!(ttl_batch.len(), 2);
  assert_eq!(ttl_batch[0].key, ttl_key(50, b"b"));
  assert_eq!(ttl_batch[1].key, ttl_key(100, b"a"));
  assert!(ttl_batch.iter().all(|entry| entry.value.is_empty()));
}

#[test]
fn get_scan_filter_expired_and_clean_expired() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    seed(
      &mut store,
      &tbl,
      &[
        upsert_with_expire(b"a", b"A", 1, 1000),
        BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        upsert_with_expire(b"c", b"C", 1, 500),
      ],
    )
    .await;
    assert_ne!(store.ttl_root_id(&tbl).unwrap(), PageId::MAX);

    assert_eq!(
      store.get_at(&tbl, b"a", 999).await.unwrap(),
      Some(b"A".to_vec())
    );
    assert_eq!(store.get_at(&tbl, b"a", 1000).await.unwrap(), None);
    assert_eq!(store.get_at(&tbl, b"c", 500).await.unwrap(), None);
    assert_eq!(
      store.scan_at(&tbl, b"", b"", 0, 499).await.unwrap(),
      vec![
        (b"a".to_vec(), b"A".to_vec()),
        (b"b".to_vec(), b"B".to_vec()),
        (b"c".to_vec(), b"C".to_vec())
      ]
    );
    assert_eq!(
      store.scan_at(&tbl, b"", b"", 0, 500).await.unwrap(),
      vec![
        (b"a".to_vec(), b"A".to_vec()),
        (b"b".to_vec(), b"B".to_vec())
      ]
    );
    assert_eq!(
      store.scan_at(&tbl, b"", b"", 0, 1000).await.unwrap(),
      vec![(b"b".to_vec(), b"B".to_vec())]
    );

    let stats = store.clean_expired(&tbl, 400).await.unwrap();
    assert_eq!(stats.deleted, 0);
    assert_eq!(stats.ttl_deleted, 0);
    assert_eq!(stats.next_expire_ts, 500);

    let stats = store.clean_expired(&tbl, 500).await.unwrap();
    assert_eq!(stats.deleted, 1);
    assert_eq!(stats.ttl_deleted, 1);
    assert_eq!(stats.next_expire_ts, 1000);
    assert_eq!(
      store.scan_at(&tbl, b"", b"", 0, 500).await.unwrap(),
      vec![
        (b"a".to_vec(), b"A".to_vec()),
        (b"b".to_vec(), b"B".to_vec())
      ]
    );

    let stats = store.clean_expired(&tbl, 1000).await.unwrap();
    assert_eq!(stats.deleted, 1);
    assert_eq!(stats.ttl_deleted, 1);
    assert_eq!(stats.next_expire_ts, 0);
    assert_eq!(store.get_at(&tbl, b"a", 0).await.unwrap(), None);
    assert_eq!(
      store.scan_at(&tbl, b"", b"", 0, 0).await.unwrap(),
      vec![(b"b".to_vec(), b"B".to_vec())]
    );

    let stats = store.clean_expired(&tbl, 2000).await.unwrap();
    assert_eq!(stats, CleanExpiredStats::default());
  });
}

#[test]
fn ttl_read_filter_can_match_cpp_lazy_cleanup() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 10).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16).with_ttl_read_filter(false);
    seed(
      &mut store,
      &tbl,
      &[
        upsert_with_expire(b"a", b"A", 1, 100),
        BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
      ],
    )
    .await;

    // C++ Read/Scan 不判断 expire_ts：清理前仍返回过期值。
    assert_eq!(
      store.get_at(&tbl, b"a", 100).await.unwrap(),
      Some(b"A".to_vec())
    );
    assert_eq!(
      store.scan_at(&tbl, b"", b"", 0, 100).await.unwrap(),
      vec![
        (b"a".to_vec(), b"A".to_vec()),
        (b"b".to_vec(), b"B".to_vec())
      ]
    );

    let stats = store.clean_expired(&tbl, 100).await.unwrap();
    assert_eq!(stats.deleted, 1);
    assert_eq!(store.get_at(&tbl, b"a", 100).await.unwrap(), None);
  });
}

#[test]
fn scan_pagination_does_not_count_expired_entries() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 9).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let mut batch = Vec::new();
    for index in 0..10u32 {
      let key = format!("k{index:02}").into_bytes();
      if index % 2 == 0 {
        batch.push(upsert_with_expire(&key, b"v", 1, 100));
      } else {
        batch.push(BatchEntry::upsert(key, b"v".to_vec(), 1));
      }
    }
    seed(&mut store, &tbl, &batch).await;

    let mut begin = b"".to_vec();
    let mut begin_inclusive = true;
    let mut page_li = Vec::new();
    loop {
      let page = store
        .scan_page_at(
          &tbl,
          &begin,
          b"",
          100,
          fkv_store::ScanOptions {
            begin_inclusive,
            ..ScanOptions::default()
          },
          fkv_task_scan::PageOptions {
            page_entries: 3,
            page_size: 0,
          },
        )
        .await
        .unwrap();
      // 过期项不占 entry 配额，因此每页尽可能给满 3 条 live entry。
      assert!(page.entry_li.len() <= 3);
      if let Some((key, _)) = page.entry_li.last() {
        begin = key.clone();
        begin_inclusive = false;
      }
      let has_remaining = page.has_remaining;
      page_li.push(page.entry_li);
      if !has_remaining {
        break;
      }
    }
    assert_eq!(page_li.len(), 2);
    assert_eq!(page_li[0].len(), 3);
    assert_eq!(page_li[1].len(), 2);
    let key_li: Vec<Vec<u8>> = page_li.into_iter().flatten().map(|(key, _)| key).collect();
    assert_eq!(
      key_li,
      vec![
        b"k01".to_vec(),
        b"k03".to_vec(),
        b"k05".to_vec(),
        b"k07".to_vec(),
        b"k09".to_vec()
      ]
    );
  });
}

#[test]
fn stale_ttl_entry_does_not_delete_new_value() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    seed(&mut store, &tbl, &[upsert_with_expire(b"k", b"v1", 1, 100)]).await;
    // 覆盖为无 TTL 的新值；旧 TTL 索引条目成为 stale。
    seed(
      &mut store,
      &tbl,
      &[BatchEntry::upsert(b"k".to_vec(), b"v2".to_vec(), 2)],
    )
    .await;
    let stats = store.clean_expired(&tbl, 200).await.unwrap();
    assert_eq!(stats.deleted, 0);
    assert_eq!(stats.ttl_deleted, 1);
    assert_eq!(
      store.get_at(&tbl, b"k", 200).await.unwrap(),
      Some(b"v2".to_vec())
    );

    // 两次带 TTL 覆盖：旧 ts 的索引项不应删除新值。
    seed(&mut store, &tbl, &[upsert_with_expire(b"k", b"v3", 3, 300)]).await;
    seed(&mut store, &tbl, &[upsert_with_expire(b"k", b"v4", 4, 400)]).await;
    let stats = store.clean_expired(&tbl, 300).await.unwrap();
    assert_eq!(stats.deleted, 0);
    assert_eq!(stats.ttl_deleted, 1);
    assert_eq!(
      store.get_at(&tbl, b"k", 300).await.unwrap(),
      Some(b"v4".to_vec())
    );

    let stats = store.clean_expired(&tbl, 400).await.unwrap();
    assert_eq!(stats.deleted, 1);
    assert_eq!(stats.ttl_deleted, 1);
    assert_eq!(store.get_at(&tbl, b"k", 400).await.unwrap(), None);

    // 删除后的 stale TTL 条目只清理索引，不会复活数据。
    seed(&mut store, &tbl, &[upsert_with_expire(b"d", b"D", 5, 500)]).await;
    store
      .put_at(&tbl, &[BatchEntry::delete(b"d".to_vec(), 6)], 6)
      .await
      .unwrap();
    let stats = store.clean_expired(&tbl, 500).await.unwrap();
    assert_eq!(stats.deleted, 0);
    assert_eq!(stats.ttl_deleted, 1);
    assert_eq!(store.get_at(&tbl, b"d", 500).await.unwrap(), None);
  });
}

#[test]
fn already_expired_upsert_is_skipped_at_write() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 11).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    // now=0 时写入，100 到期。
    seed(&mut store, &tbl, &[upsert_with_expire(b"k", b"v1", 1, 100)]).await;

    // now=100 时写入一个已过期的新值：不落主树，只删除对应 TTL 索引。
    store
      .put_at(&tbl, &[upsert_with_expire(b"k", b"v2", 2, 100)], 100)
      .await
      .unwrap();
    assert_eq!(
      store.get_at(&tbl, b"k", 0).await.unwrap(),
      Some(b"v1".to_vec())
    );
    let stats = store.clean_expired(&tbl, 100).await.unwrap();
    // TTL 索引已在写路径删除，clean 看不到待清理项，也不会误删旧值。
    assert_eq!(stats.deleted, 0);
    assert_eq!(stats.ttl_deleted, 0);

    // 全新的已过期 key 完全不产生主树 entry。
    store
      .put_at(&tbl, &[upsert_with_expire(b"gone", b"G", 3, 50)], 100)
      .await
      .unwrap();
    assert_eq!(store.get_at(&tbl, b"gone", 0).await.unwrap(), None);
    assert_eq!(
      store.scan_at(&tbl, b"", b"", 0, 0).await.unwrap(),
      vec![(b"k".to_vec(), b"v1".to_vec())]
    );
  });
}

#[test]
fn write_after_expiry_triggers_cleanup() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 12).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    seed(&mut store, &tbl, &[upsert_with_expire(b"a", b"A", 1, 100)]).await;

    // 未到期：普通写入不触发清理。
    store
      .put_at(
        &tbl,
        &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)],
        99,
      )
      .await
      .unwrap();
    assert_eq!(
      store.get_at(&tbl, b"a", 99).await.unwrap(),
      Some(b"A".to_vec())
    );

    // now=100 的写入触发 TriggerTTL，过期项在写入后被物理删除。
    store
      .put_at(
        &tbl,
        &[BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 3)],
        100,
      )
      .await
      .unwrap();
    assert_eq!(store.get_at(&tbl, b"a", 100).await.unwrap(), None);
    assert_eq!(
      store.scan_at(&tbl, b"", b"", 0, 100).await.unwrap(),
      vec![
        (b"b".to_vec(), b"B".to_vec()),
        (b"c".to_vec(), b"C".to_vec())
      ]
    );
  });
}

#[compio::test]
async fn expired_only_write_reopens_without_data() {
  let root = temp_root("expired_only");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16);
  store
    .put_at(&tbl, &[upsert_with_expire(b"gone", b"G", 1, 10)], 100)
    .await
    .unwrap();
  assert_eq!(store.get_at(&tbl, b"gone", 0).await.unwrap(), None);
  drop(store);

  let mut store = Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16);
  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.get_at(&tbl, b"gone", 0).await.unwrap(), None);
  assert!(
    store
      .scan_at(&tbl, b"", b"", 0, 0)
      .await
      .unwrap()
      .is_empty()
  );
  remove_dir_all(root).unwrap();
}

#[compio::test]
async fn ttl_tree_survives_reopen_and_log_replay() {
  let root = temp_root("reopen");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16);
  seed(&mut store, &tbl, &[upsert_with_expire(b"a", b"A", 1, 1000)]).await;
  // 第二次 put 走 manifest log，TTL 映射也必须被 replay。
  seed(&mut store, &tbl, &[upsert_with_expire(b"b", b"B", 2, 2000)]).await;
  let ttl_root = store.ttl_root_id(&tbl).unwrap();
  assert_ne!(ttl_root, PageId::MAX);
  drop(store);

  let mut store = Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16);
  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.ttl_root_id(&tbl), Some(ttl_root));
  assert_eq!(
    store.get_at(&tbl, b"a", 999).await.unwrap(),
    Some(b"A".to_vec())
  );
  assert_eq!(store.get_at(&tbl, b"a", 1000).await.unwrap(), None);
  assert_eq!(store.get_at(&tbl, b"b", 2000).await.unwrap(), None);

  let stats = store.clean_expired(&tbl, 1000).await.unwrap();
  assert_eq!(stats.deleted, 1);
  assert_eq!(stats.next_expire_ts, 2000);
  let stats = store.clean_expired(&tbl, 2000).await.unwrap();
  assert_eq!(stats.deleted, 1);
  assert_eq!(stats.next_expire_ts, 0);
  drop(store);

  let mut store = Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16);
  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.get_at(&tbl, b"a", 0).await.unwrap(), None);
  assert_eq!(store.get_at(&tbl, b"b", 0).await.unwrap(), None);
  assert_eq!(store.scan_at(&tbl, b"", b"", 0, 0).await.unwrap(), vec![]);

  remove_dir_all(root).unwrap();
}
