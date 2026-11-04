use std::{
  collections::HashMap,
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_store::Store;
use fkv_types::TableIdent;

fn temp_root(tag: &str) -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_compress_{tag}_{}_{}_{}",
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

/// 高可压缩 value：500B 重复字节，远超 SKIP_COMPRESSION_THRESHOLD。
fn compressible(seed: u8) -> Vec<u8> {
  vec![seed; 500]
}

#[compio::test]
async fn mem_store_compression_roundtrip_read_and_scan() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(MemStore::new(4096), 4096, 16).with_compression_level(3);
  let a = compressible(b'a');
  let b = compressible(b'b');
  store
    .put(
      &tbl,
      &[
        BatchEntry::upsert(b"k1".to_vec(), a.clone(), 1),
        BatchEntry::upsert(b"k2".to_vec(), b.clone(), 1),
        // 短值不压缩，走 None 分支。
        BatchEntry::upsert(b"k3".to_vec(), b"tiny".to_vec(), 1),
      ],
    )
    .await
    .unwrap();

  assert_eq!(store.read(&tbl, b"k1").await.unwrap(), a);
  assert_eq!(store.read(&tbl, b"k2").await.unwrap(), b);
  assert_eq!(store.read(&tbl, b"k3").await.unwrap(), b"tiny");

  let entry_li = store.scan(&tbl, b"", b"z", 0).await.unwrap();
  let map: HashMap<_, _> = entry_li.into_iter().collect();
  assert_eq!(map.get(b"k1".as_slice()), Some(&a));
  assert_eq!(map.get(b"k2".as_slice()), Some(&b));
  assert_eq!(map.get(b"k3".as_slice()), Some(&b"tiny".to_vec()));

  // floor 也走解压路径。
  assert_eq!(store.floor(&tbl, b"k2").await.unwrap().value, b);
}

#[compio::test]
async fn compio_store_compression_survives_reopen() {
  let root = temp_root("reopen");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let value = compressible(b'z');
  {
    let io = CompioStore::new(root.clone(), 4096, 18);
    let mut store = Store::new(io, 4096, 16).with_compression_level(3);
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"key".to_vec(), value.clone(), 7)],
      )
      .await
      .unwrap();
    assert_eq!(store.read(&tbl, b"key").await.unwrap(), value);
  }
  {
    let io = CompioStore::new(root.clone(), 4096, 18);
    let mut store = Store::new(io, 4096, 16).with_compression_level(3);
    store.reopen(&tbl).await.unwrap();
    assert_eq!(store.read(&tbl, b"key").await.unwrap(), value);
  }
  let _ = remove_dir_all(&root);
}

#[compio::test]
async fn compression_off_stores_raw_and_still_reads() {
  let root = temp_root("off");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let io = CompioStore::new(root.clone(), 4096, 18);
  let mut store = Store::new(io, 4096, 16);
  let value = compressible(b'q');
  store
    .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), value.clone(), 1)])
    .await
    .unwrap();
  assert_eq!(store.read(&tbl, b"k").await.unwrap(), value);
  let _ = remove_dir_all(&root);
}

/// 字典压缩端到端：写入按字典压缩，manifest 持久化字典，reopen 自动恢复可读。
#[compio::test]
async fn dictionary_compression_end_to_end_reopen() {
  let root = temp_root("dict");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let dict = vec![b'd'; 1024];
  let value = vec![b'd'; 600];
  {
    let io = CompioStore::new(root.clone(), 4096, 18);
    let mut store = Store::new(io, 4096, 16).with_compression_level(3);
    store.set_dictionary(&tbl, dict.clone()).unwrap();
    store
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), value.clone(), 1)])
      .await
      .unwrap();
    assert_eq!(store.read(&tbl, b"k").await.unwrap(), value);
  }
  {
    // 重开时不显式设置字典，靠 manifest 恢复。
    let io = CompioStore::new(root.clone(), 4096, 18);
    let mut store = Store::new(io, 4096, 16);
    store.reopen(&tbl).await.unwrap();
    assert!(store.has_dictionary(&tbl));
    assert_eq!(store.read(&tbl, b"k").await.unwrap(), value);
  }
  let _ = remove_dir_all(&root);
}
