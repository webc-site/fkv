use std::{
  env::temp_dir,
  fs::{create_dir_all, read_dir, remove_dir_all},
  path::{Path, PathBuf},
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_large::SegmentLayout;
use fkv_store::Store;
use fkv_types::TableIdent;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_store_naming_{}_{}_{}",
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

fn has_file(root: &Path, prefix: &str, suffix: &str) -> bool {
  let Ok(entry_li) = read_dir(root.join("demo.1")) else {
    return false;
  };
  entry_li.flatten().any(|entry| {
    entry
      .file_name()
      .to_str()
      .is_some_and(|name| name.starts_with(prefix) && name.ends_with(suffix))
  })
}

#[compio::test]
async fn store_writes_branch_term_files_and_reads_old_terms() {
  let root = temp_root();
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let mut store = Store::new(
    CompioStore::new(root.clone(), 512, 2).with_segment_config(4096, 2),
    512,
    16,
  )
  .with_pages_per_file_shift(2)
  .with_branch("Feature-1", 7)
  .with_large_value(layout, 1024);

  let mut key_index = 0usize;
  for _ in 0..4 {
    let batch: Vec<BatchEntry> = (0..50)
      .map(|_| {
        let key = format!("k:{key_index:06}").into_bytes();
        let value = format!("v:{key_index}").into_bytes();
        key_index += 1;
        BatchEntry::upsert(key, value, 1)
      })
      .collect();
    store.put(&tbl, &batch).await.unwrap();
  }
  assert!(has_file(&root, "data_", "_feature-1_7"));

  // term 变化后，新文件使用新 term；旧 term 文件继续按 file id 读取。
  store.set_term(8);
  assert!(!has_file(&root, "data_", "_feature-1_8"));
  let mut term8 = false;
  for _ in 0..40 {
    let batch: Vec<BatchEntry> = (0..50)
      .map(|_| {
        let key = format!("k:{key_index:06}").into_bytes();
        let value = format!("v:{key_index}").into_bytes();
        key_index += 1;
        BatchEntry::upsert(key, value, 2)
      })
      .collect();
    store.put(&tbl, &batch).await.unwrap();
    if has_file(&root, "data_", "_feature-1_8") {
      term8 = true;
      break;
    }
  }
  assert!(term8, "expected a new data file with term 8");

  let big: Vec<u8> = (0..20_000).map(|index| (index % 251) as u8).collect();
  store
    .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 3)])
    .await
    .unwrap();
  assert!(has_file(&root, "segment_", "_feature-1_8"));
  drop(store);

  let mut store = Store::new(
    CompioStore::new(root.clone(), 512, 2).with_segment_config(4096, 2),
    512,
    16,
  )
  .with_pages_per_file_shift(2)
  .with_branch("feature-1", 8)
  .with_large_value(layout, 1024);
  store.reopen(&tbl).await.unwrap();
  assert_eq!(
    store.get(&tbl, b"k:000000").await.unwrap(),
    Some(b"v:0".to_vec())
  );
  assert_eq!(
    store.get(&tbl, b"k:000100").await.unwrap(),
    Some(b"v:100".to_vec())
  );
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big
  );

  remove_dir_all(root).unwrap();
}
