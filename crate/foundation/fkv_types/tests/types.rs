use fkv_types::{TableIdent, is_valid_branch_name, normalize_branch_name};

#[test]
fn table_ident_roundtrip() {
  let table = TableIdent::new("demo", 7).unwrap();
  assert_eq!(table.to_string(), "demo.7");
  assert_eq!(table.to_string().parse::<TableIdent>().unwrap(), table);
}

#[test]
fn table_ident_rejects_empty() {
  assert!(TableIdent::new("", 0).is_err());
}

#[test]
fn shard_index_in_range() {
  let table = TableIdent::new("demo", 7).unwrap();
  for n in 1..=64u16 {
    assert!(table.shard_index(n) < n);
  }
  assert_eq!(table.shard_index(0), 0);
}

#[test]
fn branch_name_rules() {
  assert!(is_valid_branch_name("main"));
  assert!(is_valid_branch_name("feature-a1"));
  assert!(!is_valid_branch_name("feature_a"));
  assert_eq!(
    normalize_branch_name("Feature-A1").as_deref(),
    Some("feature-a1")
  );
  assert_eq!(normalize_branch_name("feature_a"), None);
}

#[test]
fn file_names() {
  use fkv_types::{
    archive_file_name, data_file_name, manifest_file_name, parse_data_file_name,
    parse_manifest_file_name, segment_file_name,
  };
  assert_eq!(data_file_name(3, "main", 7), "data_3_main_7");
  assert_eq!(segment_file_name(3, "main", 7), "segment_3_main_7");
  assert_eq!(manifest_file_name("main", 7), "manifest_main_7");
  assert_eq!(archive_file_name("main", 7, "tag"), "manifest_main_7_tag");
  assert_eq!(parse_data_file_name("data_3_main_7"), Some((3, "main", 7)));
  assert_eq!(
    parse_data_file_name("segment_3_main_7"),
    Some((3, "main", 7))
  );
  assert_eq!(
    parse_manifest_file_name("manifest_main_7"),
    Some(("main", 7, None))
  );
  assert_eq!(
    parse_manifest_file_name("manifest_main_7_tag"),
    Some(("main", 7, Some("tag")))
  );
  assert_eq!(parse_data_file_name("data_3_main"), None);
}
