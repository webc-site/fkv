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
}

/// 0 shard 是编程错误：`Runtime` 已拒绝 0 线程，这里直接除零而不是静默路由到 0 号 shard。
#[test]
#[should_panic]
fn shard_index_rejects_zero_shards() {
  let table = TableIdent::new("demo", 7).unwrap();
  let _ = table.shard_index(0);
}

/// 对齐 C++ `TableIdent::ShardIndex`：`partition_id % num_shards`，
/// 与表名无关，同一分区在所有表名下路由到同一 shard。
#[test]
fn shard_index_matches_cpp_modulo() {
  let a = TableIdent::new("alpha", 5).unwrap();
  let b = TableIdent::new("beta", 5).unwrap();
  for n in 1..=16u16 {
    assert_eq!(a.shard_index(n), 5 % n);
    assert_eq!(a.shard_index(n), b.shard_index(n));
  }
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

/// issue 47：取模路由下，同一 partition_id 的所有表落同一 shard；
/// 调用方应保证 partition_id 在 shard 间均匀分配。
#[test]
fn shard_index_same_partition_same_shard_and_even_distribution() {
  const SHARD_NUM: u16 = 8;
  for name in ["alpha", "beta", "gamma"] {
    let table = TableIdent::new(name, 5).unwrap();
    assert_eq!(table.shard_index(SHARD_NUM), 5 % SHARD_NUM);
  }
  let mut count = [0usize; SHARD_NUM as usize];
  for partition in 0..SHARD_NUM as u32 * 100 {
    let table = TableIdent::new("demo", partition).unwrap();
    count[usize::from(table.shard_index(SHARD_NUM))] += 1;
  }
  assert!(count.iter().all(|value| *value == 100), "{count:?}");
}

/// fmt-15：拒绝 legacy `manifest_<term>`（branch 段纯数字），对齐 C++。
#[test]
fn manifest_file_name_rejects_legacy_numeric_branch() {
  use fkv_types::parse_manifest_file_name;
  assert_eq!(parse_manifest_file_name("manifest_5"), None);
  assert_eq!(parse_manifest_file_name("manifest_123_0"), None);
  assert_eq!(
    parse_manifest_file_name("manifest_main_2"),
    Some(("main", 2, None))
  );
  assert_eq!(
    parse_manifest_file_name("manifest_feature-1_0_snap"),
    Some(("feature-1", 0, Some("snap")))
  );
}
