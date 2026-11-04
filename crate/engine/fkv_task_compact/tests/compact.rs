use fkv_io::{FileIo, PageIo};
use fkv_io_mem::MemStore;
use fkv_mapping::{AllocMode, PageMapper};
use fkv_task_compact::compact_file;
use fkv_task_gc::gc_files;
use fkv_types::TableIdent;

#[test]
fn compact_then_gc() {
  pollster::block_on(async {
    let store = MemStore::with_pages_per_file(8, 1);
    let tbl = TableIdent::new("demo", 1).unwrap();
    store.write_page(&tbl, 0, b"12345678").await.unwrap();
    store.write_page(&tbl, 2, b"abcdefgh").await.unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 4);
    let p0 = mapper.allocate_page();
    let p1 = mapper.allocate_page();
    mapper.set_file_page(p0, 0).unwrap();
    mapper.set_file_page(p1, 2).unwrap();

    assert_eq!(
      compact_file(&store, &tbl, &mut mapper, 0, 8, 1)
        .await
        .unwrap(),
      1
    );
    assert_eq!(mapper.file_page_id(p0).unwrap(), 6);
    let mut dst = [0u8; 8];
    store.read_page(&tbl, 6, &mut dst).await.unwrap();
    assert_eq!(&dst, b"12345678");

    assert_eq!(gc_files(&store, &tbl, &mapper, 1).await.unwrap(), 1);
    let mut file_li = store.list_file_id_li(&tbl).await.unwrap();
    file_li.sort_unstable();
    assert_eq!(file_li, vec![1, 3]);
  });
}
