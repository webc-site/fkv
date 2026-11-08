//! io-06：`read_page_li` 直读调用方 Vec（零整页拷贝、零读池租借），
//! 且候选回退 / 短读重试 / EOF 语义与 `read_page` 完全一致。

use std::{
  fs::{create_dir_all, write},
  slice,
};

use fkv_error::KvError;
use fkv_io::PageIo;
use fkv_io_compio::CompioStore;
use fkv_page::{PageType, set_checksum, set_page_type};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
const SHIFT: u8 = 2;

/// checksum 合法的 data page（读路径默认校验 checksum）。
fn valid_page(fill: u8) -> Vec<u8> {
  let mut page = vec![0u8; PAGE_SIZE];
  set_page_type(&mut page, PageType::Data).unwrap();
  page[100] = fill;
  set_checksum(&mut page).unwrap();
  page
}

/// 单页批读便捷入口：语义等同于 `read_page`，但走 owned 直读路径。
async fn read_one(
  store: &CompioStore,
  tbl: &TableIdent,
  file_page_id: u64,
) -> Result<Vec<u8>, KvError> {
  let mut buf = vec![0u8; PAGE_SIZE];
  store
    .read_page_li(tbl, &[file_page_id], slice::from_mut(&mut buf))
    .await?;
  Ok(buf)
}

/// 单页 owned 读便捷入口：语义等同于 `read_page`，但把调用方 `Vec` 交给后端。
async fn read_one_owned(
  store: &CompioStore,
  tbl: &TableIdent,
  file_page_id: u64,
) -> Result<Vec<u8>, KvError> {
  let mut buf = vec![0u8; PAGE_SIZE];
  store.read_page_owned(tbl, file_page_id, &mut buf).await?;
  Ok(buf)
}

/// 直读路径读回整页：内容逐字节一致，且每页只提交一次 pread。
#[compio::test]
async fn read_page_li_reads_into_caller_vec() {
  let root = TempRoot::new("read_owned_content");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);
  let page_li: Vec<(u64, Vec<u8>)> = (0..4)
    .map(|index| (index * 2, valid_page(0x10 + index as u8)))
    .collect();
  store.write_page_li(&tbl, &page_li).await.unwrap();

  let id_li: Vec<u64> = page_li.iter().map(|(id, _)| *id).collect();
  let mut buf_li = vec![vec![0u8; PAGE_SIZE]; id_li.len()];
  let before = store.read_at_num();
  store.read_page_li(&tbl, &id_li, &mut buf_li).await.unwrap();
  assert_eq!(store.read_at_num() - before, id_li.len() as u64);
  for ((_, expected), buf) in page_li.iter().zip(&buf_li) {
    assert_eq!(buf.len(), PAGE_SIZE);
    assert_eq!(buf, expected);
  }

  // Vec 长度不足一页：与 `read_page` 同样报 InvalidArgs，不静默部分写。
  let mut short = vec![0u8; PAGE_SIZE - 1];
  assert_eq!(
    store
      .read_page_li(&tbl, &[0], slice::from_mut(&mut short))
      .await
      .unwrap_err(),
    KvError::InvalidArgs
  );
}

/// 目标 Vec 长于一页：只覆写前 `page_size` 字节，尾部一个字节都不动。
#[compio::test]
async fn read_page_li_keeps_vec_tail_beyond_page() {
  let root = TempRoot::new("read_owned_tail");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);
  let page = valid_page(0x21);
  store.write_page(&tbl, 0, &page).await.unwrap();

  let mut buf = vec![0xABu8; PAGE_SIZE * 2];
  store
    .read_page_li(&tbl, &[0], slice::from_mut(&mut buf))
    .await
    .unwrap();
  assert_eq!(&buf[..PAGE_SIZE], &page[..]);
  assert!(
    buf[PAGE_SIZE..].iter().all(|byte| *byte == 0xAB),
    "page 之后的区域被改写"
  );
}

/// owned 直读不租借读池、零额外分配：重复批读读池分配计数恒为 0
/// （旧的「池缓冲 + 拷贝」实现首次批读就要分配池缓冲）。
#[compio::test]
async fn read_page_li_takes_no_read_pool_buffer() {
  let root = TempRoot::new("read_owned_pool");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);
  let page = valid_page(0x31);
  store.write_page(&tbl, 0, &page).await.unwrap();

  let id_li = [0u64, 1, 2, 3];
  for index in 1..4 {
    store
      .write_page(&tbl, index, &valid_page(0x31 + index as u8))
      .await
      .unwrap();
  }
  let mut buf_li = vec![vec![0u8; PAGE_SIZE]; id_li.len()];
  // 预热 fd cache / 目录索引，之后的批读必须完全不再分配。
  store.read_page_li(&tbl, &id_li, &mut buf_li).await.unwrap();

  let alloc_num = store.buf_alloc_num();
  let before = store.read_at_num();
  for _ in 0..8 {
    store.read_page_li(&tbl, &id_li, &mut buf_li).await.unwrap();
  }
  assert_eq!(
    store.buf_alloc_num(),
    alloc_num,
    "批读不允许租借读池 / 分配缓冲"
  );
  assert_eq!(store.read_at_num() - before, 8 * id_li.len() as u64);

  // 计数器本身可动：单页 `read_page`（借用切片）仍走读池，首读分配一次。
  let mut dst = vec![0u8; PAGE_SIZE];
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert!(store.buf_alloc_num() > alloc_num);
}

/// io-31 语义（owned 路径）：新 term torn write（短读）只在同一路径重试，
/// 耗尽后 `Corrupted` 终局，绝不落回旧 term 读回陈旧页。
#[compio::test]
async fn read_page_li_short_read_exhaustion_is_terminal() {
  let root = TempRoot::new("read_owned_short");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);
  let old_page = valid_page(0xA1);
  store.write_page(&tbl, 0, &old_page).await.unwrap();

  store.set_file_identity("main", 1);
  write(root.join("demo.1").join("data_0_main_1"), [0xB2u8; 256]).unwrap();

  let before = store.read_at_num();
  assert_eq!(
    read_one(&store, &tbl, 0).await.unwrap_err(),
    KvError::Corrupted
  );
  assert_eq!(
    store.read_at_num() - before,
    3,
    "3 次尝试全部落在新 term 文件"
  );
  assert_eq!(store.stale_read_num(), 0);
}

/// io-31 语义（owned 路径）：稀疏洞（全零页 checksum 失败）回退旧 term。
#[compio::test]
async fn read_page_li_checksum_hole_falls_back_to_old_term() {
  let root = TempRoot::new("read_owned_hole");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);
  let old_page = valid_page(0xA3);
  store.write_page(&tbl, 0, &old_page).await.unwrap();

  store.set_file_identity("main", 1);
  write(root.join("demo.1").join("data_0_main_1"), [0u8; PAGE_SIZE]).unwrap();

  let before = store.read_at_num();
  assert_eq!(read_one(&store, &tbl, 0).await.unwrap(), old_page);
  assert_eq!(store.read_at_num() - before, 2, "每个候选各读一次");
  assert_eq!(store.stale_read_num(), 1);
}

/// io-31 语义（owned 路径）：新 term 文件不及该页 offset（EOF）回退旧 term。
#[compio::test]
async fn read_page_li_eof_falls_back_to_old_term() {
  let root = TempRoot::new("read_owned_eof_fallback");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);
  let old_page = valid_page(0xA4);
  store.write_page(&tbl, 0, &old_page).await.unwrap();

  store.set_file_identity("main", 1);
  write(root.join("demo.1").join("data_0_main_1"), Vec::new()).unwrap();

  assert_eq!(read_one(&store, &tbl, 0).await.unwrap(), old_page);
  assert_eq!(store.stale_read_num(), 1);
}

/// 0 字节数据文件且无旧 term 候选：确定性 `EndOfFile`（对齐 C++ `res == 0`）。
#[compio::test]
async fn read_page_li_empty_file_returns_end_of_file() {
  let root = TempRoot::new("read_owned_empty");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);
  create_dir_all(root.join("demo.1")).unwrap();
  write(root.join("demo.1").join("data_0_main_0"), Vec::new()).unwrap();

  assert_eq!(
    read_one(&store, &tbl, 0).await.unwrap_err(),
    KvError::EndOfFile
  );
}

/// io-31 语义（owned 路径）：瞬时 `TryAgain` 只重试同一路径，耗尽上抛。
#[compio::test]
async fn read_page_li_transient_error_retries_same_path_only() {
  let root = TempRoot::new("read_owned_retry");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);
  let new_page = valid_page(0xB5);
  store.write_page(&tbl, 0, &new_page).await.unwrap();

  // 前 2 次注入 TryAgain，第 3 次成功。
  store.inject_read_try_again(2);
  let before = store.read_at_num();
  assert_eq!(read_one(&store, &tbl, 0).await.unwrap(), new_page);
  assert_eq!(store.read_at_num() - before, 3);
  assert_eq!(store.stale_read_num(), 0);

  // 3 次全部注入：耗尽后上抛 TryAgain。
  store.inject_read_try_again(3);
  assert_eq!(
    read_one(&store, &tbl, 0).await.unwrap_err(),
    KvError::TryAgain
  );
  assert_eq!(store.stale_read_num(), 0);
}

/// O_DIRECT 下调用方 Vec 不保证页对齐：owned 目标退回「对齐池缓冲直读 +
/// 一次拷贝」，行为与 `read_page` 一致（io-21 / io-06）。
#[cfg(target_os = "linux")]
#[compio::test]
async fn read_page_li_under_direct_io_roundtrip() {
  let root = TempRoot::new("read_owned_direct");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 4096, SHIFT).with_direct_io(true);
  let mut page = vec![0u8; 4096];
  set_page_type(&mut page, PageType::Data).unwrap();
  page[100] = 0x7;
  set_checksum(&mut page).unwrap();
  store.write_page(&tbl, 0, &page).await.unwrap();
  // 批读两页：第二页必须真实存在，否则批读返回 EndOfFile（不是对齐问题）。
  let mut page_1 = vec![0u8; 4096];
  set_page_type(&mut page_1, PageType::Data).unwrap();
  page_1[100] = 0x9;
  set_checksum(&mut page_1).unwrap();
  store.write_page(&tbl, 1, &page_1).await.unwrap();

  let mut buf_li = vec![vec![0u8; 4096]; 2];
  store
    .read_page_li(&tbl, &[0, 1], &mut buf_li)
    .await
    .unwrap();
  assert_eq!(buf_li[0], page);
  assert_eq!(buf_li[1], page_1);
}

// perf-10：单页 `read_page_owned` 复用同一套 owned 直读语义。以下逐条复刻
// `read_page_li` 的行为测试（内容 / 尾部 / 读池 / 短读 / 回退 / EOF / 重试）。

/// 单页 owned 读回整页：内容逐字节一致、只提交一次 pread、页后区域不动。
#[compio::test]
async fn read_page_owned_reads_into_caller_vec() {
  let root = TempRoot::new("read_owned_single_content");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);
  let page = valid_page(0x41);
  store.write_page(&tbl, 0, &page).await.unwrap();

  let before = store.read_at_num();
  let mut buf = vec![0xCDu8; PAGE_SIZE * 2];
  store.read_page_owned(&tbl, 0, &mut buf).await.unwrap();
  assert_eq!(store.read_at_num() - before, 1);
  assert_eq!(&buf[..PAGE_SIZE], &page[..]);
  assert!(
    buf[PAGE_SIZE..].iter().all(|byte| *byte == 0xCD),
    "page 之后的区域被改写"
  );

  // Vec 长度不足一页：与 `read_page` 同样报 InvalidArgs，不静默部分写。
  let mut short = vec![0u8; PAGE_SIZE - 1];
  assert_eq!(
    store
      .read_page_owned(&tbl, 0, &mut short)
      .await
      .unwrap_err(),
    KvError::InvalidArgs
  );
}

/// 单页 owned 直读不租借读池：预热 fd cache 后重复读，读池租借 / 分配计数
/// 都不增长；同口径的 `read_page` 每次都要租借一次池缓冲。
#[compio::test]
async fn read_page_owned_takes_no_read_pool_buffer() {
  let root = TempRoot::new("read_owned_single_pool");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);
  let page = valid_page(0x42);
  store.write_page(&tbl, 0, &page).await.unwrap();

  // 预热 fd cache。
  let mut buf = vec![0u8; PAGE_SIZE];
  store.read_page_owned(&tbl, 0, &mut buf).await.unwrap();

  let take_num = store.read_buf_take_num();
  let alloc_num = store.buf_alloc_num();
  for _ in 0..8 {
    store.read_page_owned(&tbl, 0, &mut buf).await.unwrap();
  }
  assert_eq!(
    store.read_buf_take_num(),
    take_num,
    "单页 owned 读不允许租借读池"
  );
  assert_eq!(
    store.buf_alloc_num(),
    alloc_num,
    "单页 owned 读不允许分配缓冲"
  );

  // 计数器本身可动：借用切片路径仍走读池。
  let mut dst = vec![0u8; PAGE_SIZE];
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert!(store.read_buf_take_num() > take_num);
}

/// io-31 语义（单页 owned）：新 term torn write（短读）只在同一路径重试，
/// 耗尽后 `Corrupted` 终局，绝不落回旧 term 读回陈旧页。
#[compio::test]
async fn read_page_owned_short_read_exhaustion_is_terminal() {
  let root = TempRoot::new("read_owned_single_short");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);
  store.write_page(&tbl, 0, &valid_page(0xA1)).await.unwrap();

  store.set_file_identity("main", 1);
  write(root.join("demo.1").join("data_0_main_1"), [0xB2u8; 256]).unwrap();

  let before = store.read_at_num();
  assert_eq!(
    read_one_owned(&store, &tbl, 0).await.unwrap_err(),
    KvError::Corrupted
  );
  assert_eq!(
    store.read_at_num() - before,
    3,
    "3 次尝试全部落在新 term 文件"
  );
  assert_eq!(store.stale_read_num(), 0);
}

/// io-31 语义（单页 owned）：稀疏洞（全零页 checksum 失败）回退旧 term。
#[compio::test]
async fn read_page_owned_checksum_hole_falls_back_to_old_term() {
  let root = TempRoot::new("read_owned_single_hole");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);
  let old_page = valid_page(0xA3);
  store.write_page(&tbl, 0, &old_page).await.unwrap();

  store.set_file_identity("main", 1);
  write(root.join("demo.1").join("data_0_main_1"), [0u8; PAGE_SIZE]).unwrap();

  let before = store.read_at_num();
  assert_eq!(read_one_owned(&store, &tbl, 0).await.unwrap(), old_page);
  assert_eq!(store.read_at_num() - before, 2, "每个候选各读一次");
  assert_eq!(store.stale_read_num(), 1);
}

/// io-31 语义（单页 owned）：新 term 文件不及该页 offset（EOF）回退旧 term。
#[compio::test]
async fn read_page_owned_eof_falls_back_to_old_term() {
  let root = TempRoot::new("read_owned_single_eof");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);
  let old_page = valid_page(0xA4);
  store.write_page(&tbl, 0, &old_page).await.unwrap();

  store.set_file_identity("main", 1);
  write(root.join("demo.1").join("data_0_main_1"), Vec::new()).unwrap();

  assert_eq!(read_one_owned(&store, &tbl, 0).await.unwrap(), old_page);
  assert_eq!(store.stale_read_num(), 1);
}

/// 0 字节数据文件且无旧 term 候选：确定性 `EndOfFile`（对齐 C++ `res == 0`）。
#[compio::test]
async fn read_page_owned_empty_file_returns_end_of_file() {
  let root = TempRoot::new("read_owned_single_empty");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);
  create_dir_all(root.join("demo.1")).unwrap();
  write(root.join("demo.1").join("data_0_main_0"), Vec::new()).unwrap();

  assert_eq!(
    read_one_owned(&store, &tbl, 0).await.unwrap_err(),
    KvError::EndOfFile
  );
}

/// io-31 语义（单页 owned）：瞬时 `TryAgain` 只重试同一路径，耗尽上抛。
#[compio::test]
async fn read_page_owned_transient_error_retries_same_path_only() {
  let root = TempRoot::new("read_owned_single_retry");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT);
  let new_page = valid_page(0xB5);
  store.write_page(&tbl, 0, &new_page).await.unwrap();

  store.inject_read_try_again(2);
  let before = store.read_at_num();
  assert_eq!(read_one_owned(&store, &tbl, 0).await.unwrap(), new_page);
  assert_eq!(store.read_at_num() - before, 3);
  assert_eq!(store.stale_read_num(), 0);

  store.inject_read_try_again(3);
  assert_eq!(
    read_one_owned(&store, &tbl, 0).await.unwrap_err(),
    KvError::TryAgain
  );
  assert_eq!(store.stale_read_num(), 0);
}

/// O_DIRECT 下调用方 Vec 不保证页对齐：单页 owned 同样退回「对齐池缓冲直读
/// + 一次拷贝」，行为与 `read_page` 一致（io-21 / io-06）。
#[cfg(target_os = "linux")]
#[compio::test]
async fn read_page_owned_under_direct_io_roundtrip() {
  let root = TempRoot::new("read_owned_single_direct");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 4096, SHIFT).with_direct_io(true);
  let mut page = vec![0u8; 4096];
  set_page_type(&mut page, PageType::Data).unwrap();
  page[100] = 0x8;
  set_checksum(&mut page).unwrap();
  store.write_page(&tbl, 0, &page).await.unwrap();

  let mut buf = vec![0u8; 4096];
  store.read_page_owned(&tbl, 0, &mut buf).await.unwrap();
  assert_eq!(buf, page);
}
