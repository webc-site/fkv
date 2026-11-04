use fkv_io::PageIo;
use fkv_io_mem::MemStore;
use fkv_mapping::{AllocMode, PageMapper};
use fkv_page::{
  CompressionType, DataPageBuilder, IndexPageBuilder, set_next_page_id, set_prev_page_id,
};
use fkv_task_scan::scan_range;
use fkv_types::{PageId, TableIdent};

fn data_page(entry_li: &[(&str, &str)]) -> Vec<u8> {
  let mut builder = DataPageBuilder::new(512, 16).unwrap();
  for (key, value) in entry_li {
    builder
      .add(
        key.as_bytes(),
        value.as_bytes(),
        false,
        0,
        0,
        CompressionType::None,
        false,
      )
      .unwrap();
  }
  builder.finish().unwrap()
}

fn index_page(leaf: bool, leftmost: PageId) -> Vec<u8> {
  let mut builder = IndexPageBuilder::new(512, 16, leaf).unwrap();
  builder.add(b"", leftmost).unwrap();
  builder.finish().unwrap()
}

#[test]
fn scan_chain() {
  pollster::block_on(async {
    let store = MemStore::new(512);
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let p0 = mapper.allocate_page();
    let p1 = mapper.allocate_page();
    let root = mapper.allocate_page();
    mapper.set_file_page(p0, 0).unwrap();
    mapper.set_file_page(p1, 1).unwrap();
    mapper.set_file_page(root, 2).unwrap();

    let mut page0 = data_page(&[("a", "A"), ("b", "B")]);
    set_next_page_id(&mut page0, p1).unwrap();
    let mut page1 = data_page(&[("c", "C"), ("d", "D")]);
    set_prev_page_id(&mut page1, p0).unwrap();
    set_next_page_id(&mut page1, PageId::MAX).unwrap();
    store.write_page(&tbl, 0, &page0).await.unwrap();
    store.write_page(&tbl, 1, &page1).await.unwrap();
    store
      .write_page(&tbl, 2, &index_page(true, p0))
      .await
      .unwrap();

    let all = scan_range(
      &store,
      fkv_task_scan::ScanArgs {
        tbl: &tbl,
        root_id: root,
        mapper: &mapper,
        begin: b"",
        end: b"",
        page_size: 512,
        max_entries: 0,
        dict: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(
      all,
      vec![
        (b"a".to_vec(), b"A".to_vec()),
        (b"b".to_vec(), b"B".to_vec()),
        (b"c".to_vec(), b"C".to_vec()),
        (b"d".to_vec(), b"D".to_vec()),
      ]
    );
    let part = scan_range(
      &store,
      fkv_task_scan::ScanArgs {
        tbl: &tbl,
        root_id: root,
        mapper: &mapper,
        begin: b"b",
        end: b"d",
        page_size: 512,
        max_entries: 0,
        dict: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(
      part,
      vec![
        (b"b".to_vec(), b"B".to_vec()),
        (b"c".to_vec(), b"C".to_vec()),
      ]
    );
  });
}
