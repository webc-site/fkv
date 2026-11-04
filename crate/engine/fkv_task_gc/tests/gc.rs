use fkv_io::{FileIo, PageIo};
use fkv_io_mem::MemStore;
use fkv_mapping::{AllocMode, PageMapper};
use fkv_task_gc::{free_page_li, gc_files, live_page_li};
use fkv_types::TableIdent;

#[test]
fn gc_removes_empty_files() {
  pollster::block_on(async {
    let store = MemStore::with_pages_per_file(8, 1);
    let tbl = TableIdent::new("demo", 1).unwrap();
    store.write_page(&tbl, 0, b"12345678").await.unwrap();
    store.write_page(&tbl, 2, b"12345678").await.unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let p0 = mapper.allocate_page();
    let p1 = mapper.allocate_page();
    mapper.set_file_page(p0, 0).unwrap();
    mapper.set_file_page(p1, 2).unwrap();
    assert_eq!(live_page_li(&mapper, 0, 1), vec![p0]);
    assert_eq!(free_page_li(&mut mapper, &[p0]).unwrap(), 1);
    let removed = gc_files(&store, &tbl, &mapper, 1).await.unwrap();
    assert_eq!(removed, 1);
    let file_li = store.list_file_id_li(&tbl).await.unwrap();
    assert_eq!(file_li, vec![1]);
  });
}
