//! perf-10：写路径的单页读改走 owned 直读后，整批不再租借读池。
//!
//! 旧实现每次单页 `read_page` 取一块读池缓冲、读完整页拷回、再归还；本
//! 测试用 `read_buf_take_num` 锁定「一次都不租借」（输出页走写池，不在此
//! 口径内）。

use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_mapping::{AllocMode, PageMapper};
use fkv_task_write::{PageLayout, WriteArg, write_batch};
use fkv_testkit::TempRoot;
use fkv_types::{NULL_PAGE, PageId, TableIdent};

const PAGE_SIZE: usize = 512;

fn layout() -> PageLayout<'static> {
  PageLayout::new(PAGE_SIZE, 16, 16, None, 16)
}

/// 写一批 entry，返回新的 root id。
async fn write_batch_li(
  store: &CompioStore,
  tbl: &TableIdent,
  mapper: &mut PageMapper,
  root_id: PageId,
  batch_li: &[BatchEntry],
) -> PageId {
  write_batch(
    store,
    WriteArg {
      tbl,
      root_id,
      mapper,
      batch_li,
      layout: layout(),
      segment_mapper: None,
      segment_size: 0,
      ttl_delete_sink: None,
    },
  )
  .await
  .unwrap()
}

/// 有序小批（同页覆盖、不分裂）：RouteCursor 读 root + leaf index，随后
/// `read_page_by_id` 读 data 页，共 3 次单页读；全部走 owned 直读，读池
/// 租借次数必须为 0。
#[compio::test]
async fn rewrite_batch_takes_no_read_pool_buffer() {
  let root = TempRoot::new("perf10_write_read_pool");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, 2);
  let mut mapper = PageMapper::new(AllocMode::Append, 0);

  // 建出高度 2 的树：root index → leaf index → data 页（2000 条足够多 data 页，
  // 单张 leaf index 装不下）。
  let build_li: Vec<BatchEntry> = (0..2000u32)
    .map(|idx| {
      BatchEntry::upsert(
        format!("k{idx:04}").into_bytes(),
        vec![b'v'; 40],
        u64::from(idx) + 1,
      )
    })
    .collect();
  let mut root_id = write_batch_li(&store, &tbl, &mut mapper, NULL_PAGE, &build_li).await;

  // 预热：同一页内覆盖一条 key，让 fd cache / 目录索引进入稳态。
  let warm_li = [BatchEntry::upsert(b"k0100".to_vec(), b"w".to_vec(), 201)];
  root_id = write_batch_li(&store, &tbl, &mut mapper, root_id, &warm_li).await;

  let before_take = store.read_buf_take_num();
  let before_read = store.read_at_num();
  let rewrite_li = [BatchEntry::upsert(b"k0100".to_vec(), b"x".to_vec(), 202)];
  let _root = write_batch_li(&store, &tbl, &mut mapper, root_id, &rewrite_li).await;

  assert_eq!(
    store.read_at_num() - before_read,
    3,
    "预期 root index + leaf index + data 页各读一次"
  );
  assert_eq!(
    store.read_buf_take_num(),
    before_take,
    "写路径单页读不允许租借读池缓冲"
  );
}
