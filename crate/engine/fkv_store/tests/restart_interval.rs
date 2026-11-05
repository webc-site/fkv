//! fmt-18：index / data 两 restart 旋钮各自生效（对齐 C++ `KvOption` 两独立字段）。
//!
//! 默认两值同为 16 时盘面字节与旧实现一致；显式设置 index 旋钮只改变 index 页
//! restart 布局，data 页字节不受影响。

use fkv_btree::BatchEntry;
use fkv_io::PageIo;
use fkv_io_mem::MemStore;
use fkv_page::{IndexPage, PageType, page_type, restart_num};
use fkv_store::Store;
use fkv_types::{PageId, TableIdent};

const PAGE_SIZE: usize = 512;
const DATA_RI: u16 = 4;

async fn write_batch_of(store: &mut Store<MemStore>, tbl: &TableIdent) {
  let batch: Vec<BatchEntry> = (0..400)
    .map(|idx| BatchEntry::upsert(format!("k{idx:04}").into_bytes(), vec![b'v'; 8], 1))
    .collect();
  store.put(tbl, &batch).await.unwrap();
}

async fn read_page(store: &Store<MemStore>, tbl: &TableIdent, page_id: PageId) -> Vec<u8> {
  let mut buf = vec![0u8; PAGE_SIZE];
  let file_page_id = store.mapper(tbl).unwrap().file_page_id(page_id).unwrap();
  store
    .page_io()
    .read_page(tbl, file_page_id, &mut buf)
    .await
    .unwrap();
  buf
}

/// 沿 leftmost 指针下到 data 页。
async fn leftmost_data_page(store: &Store<MemStore>, tbl: &TableIdent) -> Vec<u8> {
  let mut page_id = store.root_id(tbl).unwrap();
  loop {
    let buf = read_page(store, tbl, page_id).await;
    match page_type(&buf).unwrap() {
      PageType::LeafIndex | PageType::NonLeafIndex => {
        page_id = IndexPage::new(&buf).unwrap().leftmost_ptr().unwrap();
      }
      _ => return buf,
    }
  }
}

#[test]
fn index_restart_interval_is_independent_of_data() {
  pollster::block_on(async {
    let tbl = TableIdent::new("fmt18", 1).unwrap();

    let mut base = Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, DATA_RI);
    write_batch_of(&mut base, &tbl).await;
    let base_root = read_page(&base, &tbl, base.root_id(&tbl).unwrap()).await;
    let base_data = leftmost_data_page(&base, &tbl).await;

    let mut dense =
      Store::new(MemStore::new(PAGE_SIZE), PAGE_SIZE, DATA_RI).with_index_restart_interval(2);
    write_batch_of(&mut dense, &tbl).await;
    let dense_root = read_page(&dense, &tbl, dense.root_id(&tbl).unwrap()).await;
    let dense_data = leftmost_data_page(&dense, &tbl).await;

    // data 页只受 data 侧影响：两 store 的 data 间隔相同 → 字节逐字节一致。
    assert_eq!(base_data, dense_data, "index 间隔不得改变 data 页字节");
    // index 页按各自间隔切 restart：ri=4 比 ri=2 稀疏，字节不同。
    let base_restarts = restart_num(&base_root).unwrap();
    let dense_restarts = restart_num(&dense_root).unwrap();
    assert!(
      base_restarts > 1,
      "root 须有多条 entry，实际 {base_restarts}"
    );
    assert!(
      dense_restarts > base_restarts,
      "{dense_restarts} vs {base_restarts}"
    );
    assert_ne!(base_root, dense_root);
  });
}
