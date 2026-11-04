use fkv_io::PageIo;
use fkv_io_mem::MemStore;
use fkv_mapping::{AllocMode, PageMapper};
use fkv_page::{CompressionType, DataPageBuilder, IndexPageBuilder};
use fkv_task_read::read_value;
use fkv_types::{PageId, TableIdent};

fn data_page() -> Vec<u8> {
  let mut builder = DataPageBuilder::new(512, 16).unwrap();
  builder
    .add(b"a", b"A", false, 1, 0, CompressionType::None, false)
    .unwrap();
  builder
    .add(b"b", b"B", false, 2, 0, CompressionType::None, false)
    .unwrap();
  builder.finish().unwrap()
}

fn index_page(leaf: bool, leftmost: PageId) -> Vec<u8> {
  let mut builder = IndexPageBuilder::new(512, 16, leaf).unwrap();
  builder.add(b"", leftmost).unwrap();
  builder.finish().unwrap()
}

#[test]
fn single_level_read() {
  pollster::block_on(async {
    let store = MemStore::new(512);
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let data_id = mapper.allocate_page();
    let root_id = mapper.allocate_page();
    mapper.set_file_page(data_id, 0).unwrap();
    mapper.set_file_page(root_id, 1).unwrap();
    store.write_page(&tbl, 0, &data_page()).await.unwrap();
    store
      .write_page(&tbl, 1, &index_page(true, data_id))
      .await
      .unwrap();
    assert_eq!(
      read_value(&store, &tbl, root_id, &mapper, b"b", 512, None)
        .await
        .unwrap(),
      Some(b"B".to_vec())
    );
    assert_eq!(
      read_value(&store, &tbl, root_id, &mapper, b"z", 512, None)
        .await
        .unwrap(),
      None
    );
  });
}

#[test]
fn two_level_read() {
  pollster::block_on(async {
    let store = MemStore::new(512);
    let tbl = TableIdent::new("demo", 2).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let data_id = mapper.allocate_page();
    let leaf_index_id = mapper.allocate_page();
    let root_id = mapper.allocate_page();
    mapper.set_file_page(data_id, 0).unwrap();
    mapper.set_file_page(leaf_index_id, 1).unwrap();
    mapper.set_file_page(root_id, 2).unwrap();
    store.write_page(&tbl, 0, &data_page()).await.unwrap();
    store
      .write_page(&tbl, 1, &index_page(true, data_id))
      .await
      .unwrap();
    store
      .write_page(&tbl, 2, &index_page(false, leaf_index_id))
      .await
      .unwrap();
    assert_eq!(
      read_value(&store, &tbl, root_id, &mapper, b"a", 512, None)
        .await
        .unwrap(),
      Some(b"A".to_vec())
    );
  });
}
