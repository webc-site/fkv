//! io-07：写组 future 在 `WRITE_FIXED` await 点被取消（drop）时，注册写槽位
//! 配额必须归还；否则累计 16 次（`FIXED_WRITE_SLOT_LIMIT`）后 WRITE_FIXED
//! 静默永久退化为普通写。
#![cfg(target_os = "linux")]

use std::pin::pin;

use fkv_io::PageIo;
use fkv_io_compio::CompioStore;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;
use futures::poll;

/// 取消 20 次（超过 `FIXED_WRITE_SLOT_LIMIT`）：每次 poll 到 SQE 已提交即
/// drop 写组 future，模拟任务 panic 被捕获 / 外部取消后的栈展开丢弃。
/// 之后写入仍必须走 fixed 路径（普通写池零分配）且槽位照常复用。
#[compio::test]
async fn cancelled_write_groups_do_not_leak_fixed_slot_quota() {
  let tbl = TableIdent::new("demo", 21).unwrap();
  let page_size = 4096;
  // 32 页 = 1 个满组（`DEFAULT_MAX_WRITE_BATCH_PAGES`），每次取消恰好丢弃一个槽位。
  let page_li: Vec<(u64, Vec<u8>)> = (0..32)
    .map(|index| (index, vec![index as u8 + 1; page_size]))
    .collect();
  let root = TempRoot::new("fixed_write_cancel");
  let store = CompioStore::new(
    root.path().to_path_buf(),
    page_size,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_checksum_verify(false);

  // 预热：建目录、缓存写句柄；后续 poll 一次即挂起在 WRITE_FIXED 提交点。
  store.write_page_li(&tbl, &page_li).await.unwrap();

  for _ in 0..20 {
    // `pin!` 把 future 固定在栈上，`write` 只是指向它的 `Pin<&mut _>`；取消
    // 发生在循环体作用域结束时——未完成的 future 在那里被真正 drop。
    let mut write = pin!(store.write_page_li(&tbl, &page_li));
    assert!(
      poll!(write.as_mut()).is_pending(),
      "写组 future 应在 WRITE_FIXED 提交后挂起"
    );
  }
  // 每次取消丢弃一个槽位：计数可观测，且配额已同步归还（total 归零）。
  assert_eq!(store.fixed_write_discard_num(), 20);

  // 取消风暴后：写入仍走 fixed 路径（普通写池零分配）、槽位复用不再注册。
  let plain_alloc = store.buf_alloc_num();
  store.write_page_li(&tbl, &page_li).await.unwrap();
  let fixed_alloc = store.fixed_write_alloc_num();
  store.write_page_li(&tbl, &page_li).await.unwrap();
  assert_eq!(
    store.buf_alloc_num(),
    plain_alloc,
    "取消 20 次后 WRITE_FIXED 退化为普通写（配额泄漏）"
  );
  assert_eq!(store.fixed_write_alloc_num(), fixed_alloc);

  for (file_page_id, expected) in &page_li {
    let mut page = vec![0u8; page_size];
    store
      .read_page(&tbl, *file_page_id, &mut page)
      .await
      .unwrap();
    assert_eq!(&page, expected);
  }
}
