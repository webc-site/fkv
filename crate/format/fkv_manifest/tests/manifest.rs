use fkv_manifest::{
  BranchManifestMetadata, ManifestBuilder, deserialize_branch_metadata, encode_file_page_id,
  encode_page_id, invalid_mapping_value, manifest_checksum, parse_header, payload,
  serialize_branch_metadata, validate_record,
};
use fkv_types::BranchFileRange;

fn sample_branch() -> BranchManifestMetadata {
  BranchManifestMetadata {
    branch_name: "main".into(),
    term: 7,
    file_ranges: vec![BranchFileRange {
      branch_name: "main".into(),
      term: 7,
      max_file_id: 42,
      max_segment_file_id: 9,
    }],
  }
}

#[test]
fn branch_metadata_roundtrip() {
  let metadata = sample_branch();
  let bytes = serialize_branch_metadata(&metadata);
  assert_eq!(deserialize_branch_metadata(&bytes).unwrap(), metadata);
}

#[test]
fn snapshot_record() {
  let mapping_value_li = [
    encode_file_page_id(5),
    encode_page_id(7),
    invalid_mapping_value(),
  ];
  let mut builder = ManifestBuilder::new();
  let record = builder
    .snapshot(fkv_manifest::SnapshotArgs {
      root_id: 3,
      ttl_root: 4,
      max_fp_id: 8,
      dict_bytes: b"dict",
      mapping_value_li: &mapping_value_li,
      branch_metadata: &sample_branch(),
      segment_value_li: Some(&[encode_file_page_id(11)]),
      max_segment_fp_id: 12,
    })
    .unwrap();
  assert_eq!(record.len() % 4096, 0);
  assert!(validate_record(&record).unwrap());
  let header = parse_header(&record).unwrap();
  assert_eq!(header.root, 3);
  assert_eq!(header.ttl_root, 4);
  assert!(!payload(&record).unwrap().is_empty());
}

#[test]
fn log_record() {
  let mut builder = ManifestBuilder::new();
  builder.update_mapping(1, encode_file_page_id(9));
  builder.delete_mapping(2);
  builder.append_branch_metadata(&sample_branch());
  let record = builder.finalize(1, 2).unwrap();
  assert!(validate_record(&record).unwrap());
  let header = parse_header(&record).unwrap();
  assert_eq!(header.root, 1);
  assert_eq!(header.ttl_root, 2);
  assert!(header.payload_len > 0);
}

#[test]
fn checksum_empty_is_zero() {
  assert_eq!(manifest_checksum(b""), 0);
  assert_ne!(manifest_checksum(b"x"), 0);
}
