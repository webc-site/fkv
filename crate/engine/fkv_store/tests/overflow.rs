use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn big_value(len: usize, seed: u8) -> Vec<u8> {
  (0..len)
    .map(|i| seed.wrapping_add((i % 251) as u8))
    .collect()
}

/// 超过单页容量、未启用 large value 的值走 overflow 页链，读路径拼回原值。
#[compio::test]
async fn overflow_single_group_roundtrip() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(MemStore::new(512), 512, 16);
  let value = big_value(2000, 7);
  store
    .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), value.clone(), 1)])
    .await
    .unwrap();

  assert_eq!(store.get(&tbl, b"k").await.unwrap(), Some(value.clone()));
  let entry_li = store.scan(&tbl, b"", b"z", 0).await.unwrap();
  assert_eq!(entry_li, vec![(b"k".to_vec(), value.clone())]);
  assert_eq!(store.floor(&tbl, b"k").await.unwrap().value, value);
}

/// 值超过一批 overflow 页（`overflow_pointers * page_cap`）时走多级链。
#[compio::test]
async fn overflow_multi_group_roundtrip() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(MemStore::new(512), 512, 16);
  // 每页可用 ~500B，16 指针一批 → 约 8000B/批；12000B 需要两批。
  let value = big_value(12000, 11);
  store
    .put(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), value.clone(), 1)],
    )
    .await
    .unwrap();
  assert_eq!(store.get(&tbl, b"big").await.unwrap(), Some(value.clone()));
  let entry_li = store.scan(&tbl, b"", b"z", 0).await.unwrap();
  assert_eq!(entry_li, vec![(b"big".to_vec(), value)]);
}

/// overflow 值经 manifest 落盘后 reopen 仍可读，且与普通值混排不乱序。
#[compio::test]
async fn overflow_survives_reopen_and_mixes_with_small_values() {
  let root = TempRoot::new("store_overflow_reopen");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let big = big_value(2048, 3);
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
          BatchEntry::upsert(b"big".to_vec(), big.clone(), 1),
          BatchEntry::upsert(b"z".to_vec(), b"Z".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, b"big").await.unwrap(), Some(big.clone()));
  }
  {
    let io = CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 512, 16);
    store.reopen(&tbl).await.unwrap();
    assert_eq!(store.get(&tbl, b"big").await.unwrap(), Some(big.clone()));
    let entry_li = store.scan(&tbl, b"", b"zzz", 0).await.unwrap();
    assert_eq!(
      entry_li,
      vec![
        (b"a".to_vec(), b"A".to_vec()),
        (b"big".to_vec(), big),
        (b"z".to_vec(), b"Z".to_vec()),
      ]
    );
  }
}

/// issue 09a：删除 overflow 值必须释放其页链（data page + 多级 overflow 页）。
#[compio::test]
async fn overflow_pages_are_freed_on_delete() {
  use fkv_types::is_file_page_id;

  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(MemStore::new(512), 512, 16);
  let value = big_value(12_000, 13);
  store
    .put(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), value.clone(), 1)],
    )
    .await
    .unwrap();
  let page_num = store.mapper(&tbl).unwrap().tbl().len();
  assert!(
    page_num >= 3,
    "data + multiple overflow pages, got {page_num}"
  );

  store.delete(&tbl, b"big", 2).await.unwrap();
  assert_eq!(store.get(&tbl, b"big").await.unwrap(), None);
  let live = store
    .mapper(&tbl)
    .unwrap()
    .tbl()
    .iter_value_li()
    .filter(|value| is_file_page_id(*value))
    .count();
  assert_eq!(
    live, 0,
    "all overflow + data pages must be freed (page_num={page_num}, live={live})"
  );

  // 覆盖为小值时同样释放旧链。
  store
    .put(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), value.clone(), 3)],
    )
    .await
    .unwrap();
  let page_num = store.mapper(&tbl).unwrap().tbl().len();
  let live_before = store
    .mapper(&tbl)
    .unwrap()
    .tbl()
    .iter_value_li()
    .filter(|value| is_file_page_id(*value))
    .count();
  assert!(page_num >= 3, "overflow chain must be live");
  assert!(live_before > 1, "overflow chain must be live");
  store
    .put(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), b"tiny".to_vec(), 4)],
    )
    .await
    .unwrap();
  // 剩下的 live 页只能是 data / index 页，不能再有 overflow 页。
  use fkv_io::PageIo;
  use fkv_page::{PageType, page_type};
  let mapper = store.mapper(&tbl).unwrap();
  let live_id_li: Vec<u32> = mapper
    .tbl()
    .iter_value_li()
    .enumerate()
    .filter(|(_, value)| is_file_page_id(*value))
    .map(|(page_id, _)| page_id as u32)
    .collect();
  for page_id in live_id_li {
    let fp = mapper.file_page_id(page_id).unwrap();
    let mut page = vec![0u8; 512];
    store
      .page_io()
      .read_page(&tbl, fp, &mut page)
      .await
      .unwrap();
    assert_ne!(
      page_type(&page).unwrap(),
      PageType::Overflow,
      "overflow page {page_id} leaked"
    );
  }
  assert_eq!(
    store.get(&tbl, b"big").await.unwrap(),
    Some(b"tiny".to_vec())
  );
}

/// perf-85：`truncate_from` 只需 key，不读 overflow 链来物化 value。
///
/// 读页数只包含叶子页定位 + 写路径释放链（每页一次）；若恢复为全量扫描，
/// 每条 4 个 overflow 页会被多读一遍，总数接近翻倍。
#[compio::test]
async fn truncate_from_skips_overflow_value_reads() {
  const OVERFLOW_PAGES: u64 = 3 * 4;
  let tbl = TableIdent::new("demo", 2).unwrap();
  let mut store = Store::new(MemStore::new(512), 512, 16);
  let batch: Vec<BatchEntry> = [b"a", b"b", b"c", b"d", b"e"]
    .into_iter()
    .enumerate()
    .map(|(index, key)| {
      let value = if index < 2 {
        vec![b's'; 8]
      } else {
        big_value(2000, index as u8)
      };
      BatchEntry::upsert(key.to_vec(), value, 1)
    })
    .collect();
  store.put(&tbl, &batch).await.unwrap();

  let before = store.page_io().read_num();
  assert_eq!(store.truncate_from(&tbl, b"c").await.unwrap(), 3);
  let reads = store.page_io().read_num() - before;
  assert!(
    reads <= OVERFLOW_PAGES + 8,
    "truncate materialized overflow values: {reads} reads"
  );

  // 删除语义不变：留下的 key 仍可读，被截断的 key 不可见。
  let entry_li = store.scan(&tbl, b"", b"", 0).await.unwrap();
  assert_eq!(entry_li.len(), 2);
  assert_eq!(entry_li[0].0, b"a");
  assert_eq!(store.get(&tbl, b"c").await.unwrap(), None);
  assert_eq!(store.floor(&tbl, b"z").await.unwrap().key, b"b");
}
