//! io-27：合并页写复用注册写槽位，重复批次不再增长注册次数。
use fkv_io::PageIo;
use fkv_io_compio::CompioStore;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

#[compio::test]
async fn repeated_batches_reuse_write_bufs() {
  let tbl = TableIdent::new("demo", 9).unwrap();
  let page_size = 4096;
  // 96 页 / 每组 18 页 = 5 个满组 + 1 个 6 页尾组，覆盖两种组长度。
  let page_li: Vec<(u64, Vec<u8>)> = (0..96)
    .map(|index| (index, vec![index as u8 + 1; page_size]))
    .collect();
  let root = TempRoot::new("write_pool_buf_reuse");
  let store = CompioStore::new(
    root.path().to_path_buf(),
    page_size,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_checksum_verify(false);

  store.write_page_li(&tbl, &page_li).await.unwrap();
  // 首批可能因调度只完成部分组的懒注册；先跑两批再记录基准。
  store.write_page_li(&tbl, &page_li).await.unwrap();
  // Linux 走注册写槽位，普通写池不分配；其它平台首批按组长度各分配一次。
  let alloc_num = store.buf_alloc_num();
  for _ in 0..4 {
    store.write_page_li(&tbl, &page_li).await.unwrap();
    assert_eq!(store.buf_alloc_num(), alloc_num, "重复批次不应再分配写缓冲");
  }
}

#[compio::test]
async fn repeated_batches_reuse_registered_write_slots() {
  let tbl = TableIdent::new("demo", 7).unwrap();
  let page_size = 4096;
  let page_li: Vec<(u64, Vec<u8>)> = (0..96)
    .map(|index| (index, vec![index as u8 + 1; page_size]))
    .collect();
  // `TempRoot` 析构时递归清理，无需手工 `remove_dir_all`。
  let root = TempRoot::new("write_pool_reuse");
  let store = CompioStore::new(
    root.path().to_path_buf(),
    page_size,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_checksum_verify(false);

  // 96 页 / 每组 32 页 = 3 组，首批注册 3 个槽位（非 Linux 恒为 0）。
  store.write_page_li(&tbl, &page_li).await.unwrap();
  // 首批的组注册可能尚未全部完成；再跑一批作为暖机。
  store.write_page_li(&tbl, &page_li).await.unwrap();
  let alloc_num = store.fixed_write_alloc_num();
  assert!(alloc_num <= 3);

  // 后续批次必须命中已有槽位，注册次数不再增长。
  for _ in 0..4 {
    store.write_page_li(&tbl, &page_li).await.unwrap();
    assert_eq!(store.fixed_write_alloc_num(), alloc_num);
  }

  for (file_page_id, expected) in &page_li {
    let mut page = vec![0u8; page_size];
    store
      .read_page(&tbl, *file_page_id, &mut page)
      .await
      .unwrap();
    assert_eq!(&page, expected);
  }
}

/// io-40：在飞写组上限由 `max_inflight_write` 单独决定；默认对齐 C++
/// `KvOption` 头文件（32 << 10），调小到 1 后大批量写仍全部落盘。
#[compio::test]
async fn inflight_write_limit_is_configurable() {
  let tbl = TableIdent::new("demo", 11).unwrap();
  let page_size = 4096;
  let page_li: Vec<(u64, Vec<u8>)> = (0..96)
    .map(|index| (index, vec![index as u8 + 1; page_size]))
    .collect();

  let default_root = TempRoot::new("write_pool_inflight_default");
  let default = CompioStore::new(
    default_root.path().to_path_buf(),
    page_size,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_checksum_verify(false);
  assert_eq!(default.max_inflight_write(), 32 << 10);
  default.write_page_li(&tbl, &page_li).await.unwrap();

  let small_root = TempRoot::new("write_pool_inflight_small");
  let small = CompioStore::new(
    small_root.path().to_path_buf(),
    page_size,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_checksum_verify(false)
  .with_max_inflight_write(1);
  assert_eq!(small.max_inflight_write(), 1);
  small.write_page_li(&tbl, &page_li).await.unwrap();
  // 槽位缓存上限跟随在飞上限收紧：串行提交只需复用 1 个注册槽位。
  assert!(small.fixed_write_alloc_num() <= 1);

  for (file_page_id, expected) in &page_li {
    let mut page = vec![0u8; page_size];
    small
      .read_page(&tbl, *file_page_id, &mut page)
      .await
      .unwrap();
    assert_eq!(&page, expected);
  }
}
