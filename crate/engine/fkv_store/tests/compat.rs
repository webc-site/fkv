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
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_store::{ScanOptions, Store};
use fkv_types::TableIdent;

fn temp_root(tag: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_compat_{tag}_{}_{}_{}",
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
fn floor_semantics_and_not_found() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
          BatchEntry::upsert(b"d".to_vec(), b"D".to_vec(), 1),
        ],
      )
      .await
      .unwrap();

    assert_eq!(store.floor(&tbl, b"a").await.unwrap().key, b"a");
    assert_eq!(store.floor(&tbl, b"b").await.unwrap().key, b"b");
    assert_eq!(store.floor(&tbl, b"c").await.unwrap().key, b"b");
    assert_eq!(store.floor(&tbl, b"d").await.unwrap().key, b"d");
    assert_eq!(store.floor(&tbl, b"z").await.unwrap().key, b"d");
    assert_eq!(
      store.floor(&tbl, b"0").await.unwrap_err(),
      KvError::NotFound
    );
    assert_eq!(store.floor(&tbl, b"").await.unwrap_err(), KvError::NotFound);

    assert_eq!(store.read(&tbl, b"a").await.unwrap(), b"A");
    assert_eq!(
      store.read(&tbl, b"missing").await.unwrap_err(),
      KvError::NotFound
    );

    store.delete(&tbl, b"b", 2).await.unwrap();
    assert_eq!(store.floor(&tbl, b"c").await.unwrap().key, b"a");
  });
}

#[test]
fn truncate_from_position_deletes_tail() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    store
      .put(
        &tbl,
        &["a", "b", "c", "d", "e"]
          .iter()
          .map(|key| BatchEntry::upsert(key.as_bytes().to_vec(), key.as_bytes().to_vec(), 1))
          .collect::<Vec<_>>(),
      )
      .await
      .unwrap();

    assert_eq!(store.truncate_from(&tbl, b"c").await.unwrap(), 3);
    let entry_li = store.scan(&tbl, b"", b"", 0).await.unwrap();
    assert_eq!(
      entry_li,
      vec![
        (b"a".to_vec(), b"a".to_vec()),
        (b"b".to_vec(), b"b".to_vec())
      ]
    );
    assert_eq!(store.floor(&tbl, b"z").await.unwrap().key, b"b");

    // position 在最大 key 之后：不删除。
    assert_eq!(store.truncate_from(&tbl, b"zz").await.unwrap(), 0);
    // position 为空：整表 truncate。
    assert_eq!(store.truncate_from(&tbl, b"").await.unwrap(), 0);
    assert!(store.scan(&tbl, b"", b"", 0).await.unwrap().is_empty());
    assert_eq!(
      store.floor(&tbl, b"z").await.unwrap_err(),
      KvError::NotFound
    );
  });
}

#[test]
fn floor_reads_large_value_with_metadata() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let big: Vec<u8> = (0..9000).map(|index| (index % 251) as u8).collect();
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 1).with_metadata(b"meta")],
      )
      .await
      .unwrap();
    let floor = store.floor(&tbl, b"bigz").await.unwrap();
    assert_eq!(floor.key, b"big");
    assert_eq!(floor.value, big);
    assert_eq!(floor.metadata, b"meta");
  });
}

#[compio::test]
async fn partial_truncate_survives_reopen() {
  let root = temp_root("truncate");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16);
  store
    .put(
      &tbl,
      &["a", "b", "c", "d"]
        .iter()
        .map(|key| BatchEntry::upsert(key.as_bytes().to_vec(), key.as_bytes().to_vec(), 1))
        .collect::<Vec<_>>(),
    )
    .await
    .unwrap();
  assert_eq!(store.truncate_from(&tbl, b"c").await.unwrap(), 2);
  drop(store);

  let mut store = Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16);
  store.reopen(&tbl).await.unwrap();
  assert_eq!(
    store.scan(&tbl, b"", b"", 0).await.unwrap(),
    vec![
      (b"a".to_vec(), b"a".to_vec()),
      (b"b".to_vec(), b"b".to_vec())
    ]
  );
  assert_eq!(store.floor(&tbl, b"z").await.unwrap().key, b"b");
  assert_eq!(store.read(&tbl, b"c").await.unwrap_err(), KvError::NotFound);
  remove_dir_all(root).unwrap();
}

#[test]
fn scan_begin_end_inclusive_options() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    store
      .put(
        &tbl,
        &["a", "b", "c"]
          .iter()
          .map(|key| BatchEntry::upsert(key.as_bytes().to_vec(), key.as_bytes().to_vec(), 1))
          .collect::<Vec<_>>(),
      )
      .await
      .unwrap();
    let scan = |begin: &'static [u8], end: &'static [u8], begin_inclusive, end_inclusive| {
      store.scan_opts(
        &tbl,
        begin,
        end,
        0,
        0,
        ScanOptions {
          begin_inclusive,
          end_inclusive,
          ..ScanOptions::default()
        },
      )
    };
    assert_eq!(scan(b"a", b"c", true, false).await.unwrap().len(), 2);
    assert_eq!(
      scan(b"a", b"c", false, false).await.unwrap(),
      vec![(b"b".to_vec(), b"b".to_vec())]
    );
    assert_eq!(scan(b"a", b"c", true, true).await.unwrap().len(), 3);
    assert_eq!(
      scan(b"b", b"c", false, true).await.unwrap(),
      vec![(b"c".to_vec(), b"c".to_vec())]
    );
    assert!(scan(b"c", b"c", true, false).await.unwrap().is_empty());
    assert_eq!(
      scan(b"c", b"c", true, true).await.unwrap(),
      vec![(b"c".to_vec(), b"c".to_vec())]
    );
    assert_eq!(scan(b"", b"", true, false).await.unwrap().len(), 3);
  });
}

#[test]
fn scan_page_entries_bytes_and_has_remaining() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    store
      .put(
        &tbl,
        &["a", "b", "c", "d", "e"]
          .iter()
          .map(|key| BatchEntry::upsert(key.as_bytes().to_vec(), key.as_bytes().to_vec(), 1))
          .collect::<Vec<_>>(),
      )
      .await
      .unwrap();
    let opts = ScanOptions::default();

    let page1 = store.scan_page(&tbl, b"", b"", opts, 2, 0).await.unwrap();
    assert_eq!(
      page1.entry_li,
      vec![
        (b"a".to_vec(), b"a".to_vec()),
        (b"b".to_vec(), b"b".to_vec())
      ]
    );
    assert!(page1.has_remaining);
    let begin = page1.entry_li.last().unwrap().0.clone();
    let next_opts = ScanOptions {
      begin_inclusive: false,
      end_inclusive: false,
      ..ScanOptions::default()
    };
    let page2 = store
      .scan_page(&tbl, &begin, b"", next_opts, 2, 0)
      .await
      .unwrap();
    assert_eq!(
      page2.entry_li,
      vec![
        (b"c".to_vec(), b"c".to_vec()),
        (b"d".to_vec(), b"d".to_vec())
      ]
    );
    assert!(page2.has_remaining);
    let begin = page2.entry_li.last().unwrap().0.clone();
    let page3 = store
      .scan_page(&tbl, &begin, b"", next_opts, 2, 0)
      .await
      .unwrap();
    assert_eq!(page3.entry_li, vec![(b"e".to_vec(), b"e".to_vec())]);
    assert!(!page3.has_remaining);

    // 正好取完时 has_remaining=false。
    let page = store.scan_page(&tbl, b"", b"c", opts, 2, 0).await.unwrap();
    assert_eq!(page.entry_li.len(), 2);
    assert!(!page.has_remaining);

    // 字节分页：每项 key+value+16 = 18 字节，36 字节容纳 2 项。
    let page = store.scan_page(&tbl, b"", b"", opts, 0, 36).await.unwrap();
    assert_eq!(page.entry_li.len(), 2);
    assert!(page.has_remaining);
    // 单项超过 page_size 时仍返回第一项。
    let page = store.scan_page(&tbl, b"", b"", opts, 0, 1).await.unwrap();
    assert_eq!(page.entry_li.len(), 1);
    assert!(page.has_remaining);
  });
}

#[test]
fn scan_rejects_large_value_but_truncate_still_works() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let big: Vec<u8> = (0..9000).map(|index| (index % 251) as u8).collect();
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    store
      .put_large(&tbl, &[BatchEntry::upsert(b"b".to_vec(), big, 1)])
      .await
      .unwrap();

    assert_eq!(
      store.scan(&tbl, b"", b"", 0).await.unwrap_err(),
      KvError::LargeValueUnsupported
    );
    // end=b 前不含大 value，可以正常 scan。
    assert_eq!(
      store.scan(&tbl, b"", b"b", 0).await.unwrap(),
      vec![(b"a".to_vec(), b"A".to_vec())]
    );
    // 分页第一页不触碰大 value，续页才报错（对齐 ScanTask）。
    let page = store
      .scan_page(&tbl, b"", b"", ScanOptions::default(), 1, 0)
      .await
      .unwrap();
    assert_eq!(page.entry_li, vec![(b"a".to_vec(), b"A".to_vec())]);
    assert!(page.has_remaining);
    let begin = page.entry_li[0].0.clone();
    let next_opts = ScanOptions {
      begin_inclusive: false,
      ..ScanOptions::default()
    };
    assert_eq!(
      store
        .scan_page(&tbl, &begin, b"", next_opts, 1, 0)
        .await
        .unwrap_err(),
      KvError::LargeValueUnsupported
    );

    // truncate 内部扫描允许大 value，删除 >= b 的 key。
    assert_eq!(store.truncate_from(&tbl, b"b").await.unwrap(), 2);
    assert_eq!(
      store.scan(&tbl, b"", b"", 0).await.unwrap(),
      vec![(b"a".to_vec(), b"A".to_vec())]
    );
    assert_eq!(store.get_large(&tbl, b"b").await.unwrap(), None);
  });
}
