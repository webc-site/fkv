use fkv_mapping::{
  AllocMode, FilePageAllocator, MappingTbl, PageMapper, ValType, decode_id, encode_file_page_id,
  encode_page_id, is_file_page_id, val_type,
};

#[test]
fn tags() {
  assert_eq!(val_type(encode_file_page_id(1)), ValType::FilePageId);
  assert_eq!(val_type(encode_page_id(1)), ValType::PageId);
  assert_eq!(val_type(7), ValType::Invalid);
  assert_eq!(decode_id(encode_file_page_id(42)), 42);
  assert!(is_file_page_id(encode_file_page_id(42)));
}

#[test]
fn tbl_roundtrip() {
  let mut tbl = MappingTbl::new();
  tbl.push(encode_file_page_id(1));
  tbl.push(encode_page_id(2));
  tbl.push(7);
  let bytes = tbl.serialize();
  assert_eq!(MappingTbl::deserialize(&bytes).unwrap(), tbl);
}

#[test]
fn append_allocator() {
  let mut allocator = FilePageAllocator::new(AllocMode::Append, 10);
  assert_eq!(allocator.allocate(), 10);
  assert_eq!(allocator.allocate(), 11);
  allocator.free(10);
  assert_eq!(allocator.allocate(), 12);
}

#[test]
fn pooled_allocator_reuses() {
  let mut allocator = FilePageAllocator::new(AllocMode::Pooled, 10);
  assert_eq!(allocator.allocate(), 10);
  assert_eq!(allocator.allocate(), 11);
  allocator.free(10);
  assert_eq!(allocator.allocate(), 10);
}

#[test]
fn mapper_alloc_and_free() {
  let mut mapper = PageMapper::new(AllocMode::Pooled, 0);
  let page = mapper.allocate_page();
  assert_eq!(mapper.file_page_id(page).unwrap(), 0);
  assert_eq!(mapper.free_page(page).unwrap(), Some(0));
  assert!(mapper.free_page(page).unwrap().is_none());
  assert_eq!(mapper.allocate_page(), 1);
}
