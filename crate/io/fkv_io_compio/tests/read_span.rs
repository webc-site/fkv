//! perf-11：`read_page_span` 连续页合并读。
//!
//! 语义契约（与逐页 `read_page` / `read_page_owned` 完全一致）：整段合并读只在
//! 「同一物理文件内、候选唯一、整段读满、每页 checksum 通过」时生效；任一条件
//! 不满足都退回逐页，因此内容、错误码与读放大边界都必须和逐页调用逐条对齐。

use fkv_error::KvError;
use fkv_io::PageIo;
use fkv_io_compio::CompioStore;
use fkv_page::{PageType, set_checksum, set_page_type};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 4096;
/// 每个物理文件 2^8 = 256 页：跨文件切段的用例只要 6 页就够。
const SHIFT: u8 = 8;

/// checksum 合法的 data page（读路径默认校验 checksum）。
fn valid_page(fill: u8) -> Vec<u8> {
  let mut page = vec![0u8; PAGE_SIZE];
  set_page_type(&mut page, PageType::Data).unwrap();
  page[100] = fill;
  set_checksum(&mut page).unwrap();
  page
}

/// checksum 非法的 data page（模拟损坏 / 稀疏洞）。
fn broken_page(fill: u8) -> Vec<u8> {
  let mut page = valid_page(fill);
  page[100] = page[100].wrapping_add(1);
  page
}

async fn store_with(page_li: &[(u64, Vec<u8>)], shift: u8) -> (TempRoot, TableIdent, CompioStore) {
  let root = TempRoot::new("read_span");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, shift);
  store.write_page_li(&tbl, page_li).await.unwrap();
  (root, tbl, store)
}

/// 连续 8 页：一次下盘读完，内容逐字节等于写入页，页之后的余量不动。
#[compio::test]
async fn read_page_span_merges_consecutive_pages() {
  let page_li: Vec<(u64, Vec<u8>)> = (0..8)
    .map(|index| (index, valid_page(0x30 + index as u8)))
    .collect();
  let (_root, tbl, store) = store_with(&page_li, SHIFT).await;

  // 目的缓冲尾部带哨兵：合并读只应写前 page_size 字节。
  let mut buf_li: Vec<Vec<u8>> = (0..8)
    .map(|_| {
      let mut buf = vec![0u8; PAGE_SIZE];
      buf.extend_from_slice(&[0xAB; 64]);
      buf
    })
    .collect();
  let before = store.read_at_num();
  store.read_page_span(&tbl, 0, &mut buf_li).await.unwrap();
  assert_eq!(
    store.read_at_num() - before,
    1,
    "同一物理文件内连续 8 页必须合并成一次读"
  );
  for ((_, page), buf) in page_li.iter().zip(&buf_li) {
    assert_eq!(&buf[..PAGE_SIZE], page.as_slice());
    assert!(
      buf[PAGE_SIZE..].iter().all(|byte| *byte == 0xAB),
      "页尾被改写"
    );
  }
}

/// 单页段（窗口退化到 1 页时走这条）：行为与 `read_page_owned` 完全一致。
#[compio::test]
async fn read_page_span_single_page_matches_owned_read() {
  let page_li: Vec<(u64, Vec<u8>)> = (0..2)
    .map(|index| (index, valid_page(0x11 + index as u8)))
    .collect();
  let (_root, tbl, store) = store_with(&page_li, SHIFT).await;

  let mut span_li = vec![vec![0u8; PAGE_SIZE]];
  let before = store.read_at_num();
  store.read_page_span(&tbl, 1, &mut span_li).await.unwrap();
  assert_eq!(store.read_at_num() - before, 1);

  let mut owned_buf = vec![0u8; PAGE_SIZE];
  store
    .read_page_owned(&tbl, 1, &mut owned_buf)
    .await
    .unwrap();
  assert_eq!(span_li[0], owned_buf);
  assert_eq!(span_li[0], page_li[1].1);
}

/// 跨物理文件的连续页号：按文件切段，每段一次读，内容与逐页读一致。
#[compio::test]
async fn read_page_span_splits_at_file_boundary() {
  // SHIFT=2 → 每文件 4 页；页号 2..8 横跨三个文件。
  let page_li: Vec<(u64, Vec<u8>)> = (0..8)
    .map(|index| (index, valid_page(0x40 + index as u8)))
    .collect();
  let (_root, tbl, store) = store_with(&page_li, 2).await;

  let mut buf_li = vec![vec![0u8; PAGE_SIZE]; 6];
  let before = store.read_at_num();
  store.read_page_span(&tbl, 2, &mut buf_li).await.unwrap();
  // 2..4 | 4..8：两个物理文件各一次读。
  assert_eq!(store.read_at_num() - before, 2);
  for (index, buf) in buf_li.iter().enumerate() {
    assert_eq!(buf, &page_li[2 + index].1);
  }
}

/// 段尾：整段读不满（末页之后没有页）时退回逐页，错误码与逐页读逐条相同，
/// 且段内真实存在的页照常读回。
#[compio::test]
async fn read_page_span_eof_matches_per_page_reads() {
  let page_li: Vec<(u64, Vec<u8>)> = (0..3)
    .map(|index| (index, valid_page(0x50 + index as u8)))
    .collect();
  let (_root, tbl, store) = store_with(&page_li, SHIFT).await;

  let mut buf_li = vec![vec![0u8; PAGE_SIZE]; 5];
  let span_err = store
    .read_page_span(&tbl, 0, &mut buf_li)
    .await
    .unwrap_err();

  let mut single = vec![0u8; PAGE_SIZE];
  let owned_err = store
    .read_page_owned(&tbl, 3, &mut single)
    .await
    .unwrap_err();
  assert_eq!(span_err, owned_err, "合并读不得改变段尾错误码");
  // 段内存在的页按逐页读语义写好了缓冲（第 3 页不存在，故只到第 2 页）。
  assert_eq!(buf_li[0], page_li[0].1);
  assert_eq!(buf_li[2], page_li[2].1);
}

/// checksum 坏的页夹在中间：合并读整体退回逐页，错误与逐页读同源（不静默
/// 把损坏页当命中）。
#[compio::test]
async fn read_page_span_checksum_hole_matches_per_page() {
  let page_li: Vec<(u64, Vec<u8>)> = vec![
    (0, valid_page(0x60)),
    (1, valid_page(0x61)),
    (2, broken_page(0x62)),
    (3, valid_page(0x63)),
  ];
  let (_root, tbl, store) = store_with(&page_li, SHIFT).await;

  let mut buf_li = vec![vec![0u8; PAGE_SIZE]; 4];
  let span_err = store
    .read_page_span(&tbl, 0, &mut buf_li)
    .await
    .unwrap_err();
  let mut single = vec![0u8; PAGE_SIZE];
  let owned_err = store
    .read_page_owned(&tbl, 2, &mut single)
    .await
    .unwrap_err();
  assert_eq!(span_err, owned_err);
  assert_eq!(span_err, KvError::Corrupted);
}

/// 目的缓冲不足整页：与逐页契约同码报错（不静默读短路）。
#[compio::test]
async fn read_page_span_short_dst_is_invalid_args() {
  let page_li: Vec<(u64, Vec<u8>)> = (0..4)
    .map(|index| (index, valid_page(0x70 + index as u8)))
    .collect();
  let (_root, tbl, store) = store_with(&page_li, SHIFT).await;

  let mut buf_li = vec![vec![0u8; PAGE_SIZE], vec![0u8; PAGE_SIZE - 1]];
  let err = store
    .read_page_span(&tbl, 0, &mut buf_li)
    .await
    .unwrap_err();
  assert_eq!(err, KvError::InvalidArgs);

  let mut short = vec![0u8; PAGE_SIZE - 1];
  let err = store
    .read_page_owned(&tbl, 0, &mut short)
    .await
    .unwrap_err();
  assert_eq!(err, KvError::InvalidArgs);
}
