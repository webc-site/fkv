use fkv_gc::{
  file_amplification, retained_file_page_set, should_compact, unreferenced_file_page_li,
};
use fkv_mapping::{MappingTbl, encode_file_page_id, encode_page_id};

#[test]
fn retained_and_candidates() {
  let mut tbl = MappingTbl::new();
  tbl.push(encode_file_page_id(1));
  tbl.push(encode_page_id(2));
  tbl.push(7);
  let retained = retained_file_page_set(&tbl);
  assert!(retained.contains(&1));
  assert_eq!(unreferenced_file_page_li([1, 2, 3], &retained), vec![2, 3]);
}

#[test]
fn amplification() {
  assert_eq!(file_amplification(10, 20), 2.0);
  assert_eq!(file_amplification(0, 20), 0.0);
  assert!(should_compact(10, 21, 2));
  assert!(!should_compact(10, 20, 2));
}
