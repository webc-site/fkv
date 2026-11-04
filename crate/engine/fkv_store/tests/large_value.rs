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
use fkv_io::{FileIo, FixedSegmentIo};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_mapping::{ValType, val_type};
use fkv_store::Store;
use fkv_types::TableIdent;

fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

fn temp_root(tag: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_large_store_{tag}_{}_{}_{}",
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
    // 旧大 value 的 3 个 segment 已释放，新 value 追加 2 个 id。
    assert_eq!(reopened.segment_mapper(&tbl).unwrap().tbl().len(), 5);
    let value_li = reopened.segment_mapper(&tbl).unwrap().tbl().value_li();
    assert!(
      value_li[..3]
        .iter()
        .all(|value| val_type(*value) == ValType::Invalid)
    );
    assert!(
      value_li[3..]
        .iter()
        .all(|value| val_type(*value) == ValType::FilePageId)
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

#[compio::test]
async fn compio_large_value_segment_file_reopen_and_truncate() {
  let root = temp_root("compio");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let mut store =
    Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16).with_large_value(layout, 1024);
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

  let mut store =
    Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16).with_large_value(layout, 1024);
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
      .value_li()
      .iter()
      .all(|value| fkv_mapping::val_type(*value) == ValType::Invalid)
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

  remove_dir_all(root).unwrap();
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
  let root = temp_root("segment_gc");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 1).unwrap();
  let mut store = Store::new(
    CompioStore::new(root.clone(), 512, 18).with_segment_config(4096, 1),
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
  let removed = store.gc(&tbl, 18).await.unwrap();
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
    CompioStore::new(root.clone(), 512, 18).with_segment_config(4096, 1),
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
  store.gc(&tbl, 18).await.unwrap();
  assert!(
    store
      .page_io()
      .list_segment_file_id_li(&tbl)
      .await
      .unwrap()
      .is_empty()
  );
  assert!(!root.join("demo.1").join("segment_1_main_0").exists());

  remove_dir_all(root).unwrap();
}

#[compio::test]
async fn store_fixed_buffer_put_and_get() {
  let root = temp_root("fixed");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let io = CompioStore::new(root.clone(), 512, 18).with_segment_config(4096, 2);
  let mut buf = io.register_fixed_buffer(64 * 1024).unwrap();
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
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    value
  );

  remove_dir_all(root).unwrap();
}
