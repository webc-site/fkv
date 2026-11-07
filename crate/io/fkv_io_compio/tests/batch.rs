//! eng-35：批量页写把连续页合并为一次 write_at。
use fkv_error::KvError;
use fkv_failpoint::set_spec;
use fkv_io::PageIo;
use fkv_io_compio::CompioStore;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

#[compio::test]
async fn contiguous_pages_share_one_write_at() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let page_size = 512;
  let page_li: Vec<(u64, Vec<u8>)> = (0..4)
    .map(|index| (index, vec![index as u8 + 1; page_size]))
    .collect();

  // 逐页写：4 次 write_at。
  // `TempRoot` 析构时递归清理，无需手工 `remove_dir_all`。
  let single_root = TempRoot::new("batch_single");
  let single = CompioStore::new(
    single_root.path().to_path_buf(),
    page_size,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_checksum_verify(false);
  let before = single.write_at_num();
  for (file_page_id, page) in &page_li {
    single.write_page(&tbl, *file_page_id, page).await.unwrap();
  }
  assert_eq!(single.write_at_num() - before, 4);

  // 批量写：连续页合并为 1 次 write_at。
  let batch_root = TempRoot::new("batch_batch");
  let batch = CompioStore::new(
    batch_root.path().to_path_buf(),
    page_size,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_checksum_verify(false);
  let before = batch.write_at_num();
  batch.write_page_li(&tbl, &page_li).await.unwrap();
  assert_eq!(batch.write_at_num() - before, 1);

  for (file_page_id, expected) in &page_li {
    let mut page = vec![0u8; page_size];
    batch
      .read_page(&tbl, *file_page_id, &mut page)
      .await
      .unwrap();
    assert_eq!(&page, expected);
  }

  // 非连续页不合并。
  let sparse = [(0u64, vec![1u8; page_size]), (2u64, vec![2u8; page_size])];
  let before = batch.write_at_num();
  batch.write_page_li(&tbl, &sparse).await.unwrap();
  assert_eq!(batch.write_at_num() - before, 2);

  set_spec("WritePage@1!err=OutOfMem");
  assert_eq!(
    batch
      .write_page_li(&tbl, &[(4, vec![0u8; page_size])])
      .await
      .unwrap_err(),
    KvError::OutOfMem
  );
  set_spec("");
}

#[compio::test]
async fn read_page_li_batch_matches_pages() {
  let tbl = TableIdent::new("demo", 2).unwrap();
  let page_size = 512;
  let page_li: Vec<(u64, Vec<u8>)> = (0..4)
    .map(|index| (index * 2, vec![index as u8 + 1; page_size]))
    .collect();
  let root = TempRoot::new("batch_read");
  let store = CompioStore::new(
    root.path().to_path_buf(),
    page_size,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_checksum_verify(false);
  store.write_page_li(&tbl, &page_li).await.unwrap();

  let file_page_id_li: Vec<u64> = page_li
    .iter()
    .map(|(file_page_id, _)| *file_page_id)
    .collect();
  let mut buf_li = vec![vec![0u8; page_size]; file_page_id_li.len()];
  store
    .read_page_li(&tbl, &file_page_id_li, &mut buf_li)
    .await
    .unwrap();
  for ((_, expected), buf) in page_li.iter().zip(&buf_li) {
    assert_eq!(buf, expected);
  }

  // 页号与缓冲数量不一致必须报错，不能静默少读。
  assert_eq!(
    store
      .read_page_li(&tbl, &file_page_id_li[..1], &mut buf_li)
      .await
      .unwrap_err(),
    KvError::InvalidArgs
  );
}

#[compio::test]
async fn contiguous_pages_split_by_max_write_batch_pages() {
  let tbl = TableIdent::new("demo", 3).unwrap();
  let page_size = 512;
  let page_li: Vec<(u64, Vec<u8>)> = (0..100)
    .map(|index| (index, vec![index as u8 + 1; page_size]))
    .collect();

  // 默认上限 32：100 页 -> ceil(100 / 32) = 4 组。
  let default_root = TempRoot::new("batch_split_default");
  let default = CompioStore::new(
    default_root.path().to_path_buf(),
    page_size,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_checksum_verify(false);
  let before = default.write_at_num();
  default.write_page_li(&tbl, &page_li).await.unwrap();
  assert_eq!(default.write_at_num() - before, 4);

  // 改为 4 页/组：恰好 25 组；末组仍按余数合并。
  let small_root = TempRoot::new("batch_split_small");
  let small = CompioStore::new(
    small_root.path().to_path_buf(),
    page_size,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_checksum_verify(false)
  .with_max_write_batch_pages(4);
  let before = small.write_at_num();
  small.write_page_li(&tbl, &page_li).await.unwrap();
  assert_eq!(small.write_at_num() - before, 25);

  // 末组余数：100 页、每组 7 页 -> ceil(100 / 7) = 15 组。
  let tail_root = TempRoot::new("batch_split_tail");
  let tail = CompioStore::new(
    tail_root.path().to_path_buf(),
    page_size,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_checksum_verify(false)
  .with_max_write_batch_pages(7);
  let before = tail.write_at_num();
  tail.write_page_li(&tbl, &page_li).await.unwrap();
  assert_eq!(tail.write_at_num() - before, 15);

  // 切分不能丢页或串页：逐页读回校验。
  for (file_page_id, expected) in &page_li {
    let mut page = vec![0u8; page_size];
    tail
      .read_page(&tbl, *file_page_id, &mut page)
      .await
      .unwrap();
    assert_eq!(&page, expected);
  }
}
