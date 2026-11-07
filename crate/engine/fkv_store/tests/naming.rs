use std::{fs::read_dir, panic::catch_unwind, path::Path};

use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

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
  let root = TempRoot::new("store_naming");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let mut store = Store::new(
    CompioStore::new(root.path().to_path_buf(), 512, 2).with_segment_config(4096, 2),
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
  assert!(has_file(root.path(), "data_", "_feature-1_7"));

  // term 变化后，新文件使用新 term；旧 term 文件继续按 file id 读取。
  store.set_term(8);
  assert!(!has_file(root.path(), "data_", "_feature-1_8"));
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
    if has_file(root.path(), "data_", "_feature-1_8") {
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
  assert!(has_file(root.path(), "segment_", "_feature-1_8"));
  drop(store);

  let mut store = Store::new(
    CompioStore::new(root.path().to_path_buf(), 512, 2).with_segment_config(4096, 2),
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
}

/// r9b-03：`with_branch` 归一化失败不得静默沿用非法原名（对齐同族 attach
/// 入口的 `InvalidArgs` 与 C++ `NormalizeBranchName` fail-fast 口径）。
/// debug 构建（含 nextest 默认跑法）下 `debug_assert` 直接炸出；release
/// 构建断言被编译掉，回退 `MAIN_BRANCH_NAME` 而非 panic。覆盖空名、
/// 下划线、斜杠、点四种 `normalize_branch_name` 的 `None` 触发形态。
#[test]
fn with_branch_rejects_unnormalized_names() {
  for name in ["", "Feature_1", "feature/1", "feature.1"] {
    let panicked = catch_unwind(|| {
      let _ = Store::new(MemStore::new(512), 512, 16).with_branch(name, 1);
    })
    .is_err();
    if cfg!(debug_assertions) {
      assert!(
        panicked,
        "debug 构建下非法 branch 名 {name:?} 应被 with_branch 拒绝"
      );
    } else {
      assert!(
        !panicked,
        "release 构建下非法 branch 名 {name:?} 应回退 main 而非 panic"
      );
    }
  }
}
