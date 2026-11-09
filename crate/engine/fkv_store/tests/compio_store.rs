use std::fs::{read, write};

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io_compio::CompioStore;
use fkv_page::{DataPageIter, PageType, page_type};
use fkv_store::{ScanOption, Store};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;

#[compio::test]
async fn compio_end_to_end() {
  let root = TempRoot::new("store_compio_e2e");
  let io = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  let mut store = Store::new(io, 512, 16);
  let tbl = TableIdent::new("demo", 1).unwrap();
  let batch = [
    BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
    BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
  ];
  store.put(&tbl, &batch).await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 2);
}

/// issue 48：`Store::with_verify_checksum(false)` 可关闭读 page 的 checksum 校验。
#[compio::test]
async fn store_can_toggle_page_checksum_verify() {
  let root = TempRoot::new("store_compio_checksum");
  let tbl = TableIdent::new("demo", 1).unwrap();
  {
    let io = CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 512, 16);
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
  }
  // 破坏 data page 前缀里的 checksum。
  let path = root.join("demo.1").join("data_0_main_0");
  let mut bytes = read(&path).unwrap();
  bytes[0] ^= 0xff;
  write(&path, &bytes).unwrap();

  {
    let io = CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 512, 16);
    store.reopen(&tbl).await.unwrap();
    assert!(
      store.get(&tbl, b"a").await.is_err(),
      "checksum must be verified"
    );
  }
  {
    let io = CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 512, 16).with_verify_checksum(false);
    store.reopen(&tbl).await.unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  }
}

/// test-02：盘上 data 页损坏必须同时被 scan 与 get 检测到。
///
/// 对齐 C++ `eloqstore/tests/persist.cpp` 的 `detect corrupted page`：破坏页内
/// 非 checksum 区后，先断言 Scan 报 `Corrupted`，再断言同一页上的 Read 也报
/// `Corrupted`。Rust data page 头部布局与 C++ 不同（offset 10 落在 content_len），
/// 因此改坏首条 value 的字节：同属非 checksum 区，且页结构仍然合法，便于用
/// 「关闭校验后 scan 仍能读完」做对照。
#[compio::test]
async fn scan_detects_corrupted_data_page() {
  let root = TempRoot::new("store_compio_scan_corrupted");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let (first_key, entry_num) = {
    let io = CompioStore::new(
      root.path().to_path_buf(),
      PAGE_SIZE,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, PAGE_SIZE, 16);
    let batch_li: Vec<BatchEntry> = (0..40)
      .map(|idx| BatchEntry::upsert(format!("k{idx:04}").into_bytes(), vec![b'v'; 64], 1))
      .collect();
    store.put(&tbl, &batch_li).await.unwrap();
    // 破坏前先走一遍 scan，确认数据确实跨页可读。
    assert_eq!(
      store.scan(&tbl, b"", b"", 0).await.unwrap().len(),
      batch_li.len()
    );
    (batch_li[0].key.clone(), batch_li.len())
  };

  let path = root.join("demo.1").join("data_0_main_0");
  let mut bytes = read(&path).unwrap();
  // 首次写入先分配 data 页、后分配 index 页，故文件前两个页槽必为 data 页。
  for idx in 0..2 {
    assert_eq!(
      page_type(&bytes[idx * PAGE_SIZE..(idx + 1) * PAGE_SIZE]).unwrap(),
      PageType::Data,
      "写入后前两个页槽都应是 data 页（保证 >= 2 个 data 页）"
    );
  }
  // 改坏首页第一条 value 的一个字节：不碰 8 字节 checksum 前缀。
  let value_off = {
    let mut iter = DataPageIter::new(&bytes[..PAGE_SIZE]).unwrap();
    assert!(iter.advance().unwrap());
    assert_eq!(iter.key(), first_key.as_slice());
    iter.value_range().0
  };
  assert!(value_off >= fkv_page::CHECKSUM_BYTES);
  bytes[value_off] ^= 0xff;
  write(&path, &bytes).unwrap();

  {
    // 默认 verify_checksum = true：scan 下降到叶子后读首页即报 Corrupted。
    let io = CompioStore::new(
      root.path().to_path_buf(),
      PAGE_SIZE,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, PAGE_SIZE, 16);
    store.reopen(&tbl).await.unwrap();
    assert_eq!(
      store.scan(&tbl, b"", b"", 0).await.unwrap_err(),
      KvError::Corrupted,
      "scan must verify data page checksum"
    );
    // keys_only 只是不物化 value，仍走 read_page，因此同样校验 checksum。
    assert_eq!(
      store
        .scan_with_option(
          &tbl,
          b"",
          b"",
          0,
          ScanOption {
            keys_only: true,
            ..ScanOption::default()
          },
        )
        .await
        .unwrap_err(),
      KvError::Corrupted
    );
    // 同一 data 页上的点读也必须报 Corrupted（保留原有 get 覆盖）。
    assert_eq!(
      store.get(&tbl, &first_key).await.unwrap_err(),
      KvError::Corrupted,
      "get must verify data page checksum"
    );
  }
  {
    // 对照：scan 自身没有跳过 checksum 的分支，唯一的旁路是 IO 层的
    // skip_verify_checksum（`Store::with_verify_checksum(false)`）。
    let io = CompioStore::new(
      root.path().to_path_buf(),
      PAGE_SIZE,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, PAGE_SIZE, 16).with_verify_checksum(false);
    store.reopen(&tbl).await.unwrap();
    assert_eq!(
      store.scan(&tbl, b"", b"", 0).await.unwrap().len(),
      entry_num,
      "with_verify_checksum(false) is the only scan bypass"
    );
  }
}

/// eng-37：`evict_table` 清空内存状态，之后 reopen 能重新加载数据。
#[compio::test]
async fn evict_table_clears_state_and_reopen_reloads() {
  let root = TempRoot::new("store_compio_io");
  let tbl = TableIdent::new("demo", 2).unwrap();
  let io = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  let mut store = Store::new(io, 512, 16);
  store
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

  store.evict_table(&tbl);
  assert!(store.mapper(&tbl).is_none());
  assert!(store.root_id(&tbl).is_none());
  // 状态已卸载：读不到数据（不会误当成空表返回旧值）。
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), None);

  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
}
