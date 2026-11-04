use fkv_manifest::{
  BranchManifestMetadata, ManifestBuilder, SnapshotArgs, encode_file_page_id, encode_page_id,
};
use fkv_recovery::ReplayState;
use fkv_types::BranchFileRange;

fn branch() -> BranchManifestMetadata {
  BranchManifestMetadata {
    branch_name: "main".into(),
    term: 3,
    file_ranges: vec![BranchFileRange {
      branch_name: "main".into(),
      term: 3,
      max_file_id: 7,
      max_segment_file_id: 2,
    }],
  }
}

#[test]
fn snapshot_replay() {
  let mapping_li = [encode_file_page_id(5), encode_page_id(7)];
  let segment_li = [encode_file_page_id(11)];
  let mut builder = ManifestBuilder::new();
  let record = builder
    .snapshot(SnapshotArgs {
      root_id: 1,
      ttl_root: 2,
      max_fp_id: 8,
      dict_bytes: b"dict",
      mapping_value_li: &mapping_li,
      branch_metadata: &branch(),
      segment_value_li: Some(&segment_li),
      max_segment_fp_id: 12,
    })
    .unwrap();
  let state = ReplayState::from_snapshot(&record).unwrap();
  assert_eq!(state.root_id, 1);
  assert_eq!(state.ttl_root_id, 2);
  assert_eq!(state.max_fp_id, 8);
  assert_eq!(state.dict_bytes, b"dict");
  assert_eq!(state.mapping_tbl.value_li(), &mapping_li);
  assert_eq!(state.segment_mapping.value_li(), &segment_li);
  assert_eq!(state.branch_metadata, branch());
}

#[test]
fn log_replay() {
  let mut builder = ManifestBuilder::new();
  builder.update_mapping(1, encode_file_page_id(9));
  builder.delete_mapping(2);
  builder.append_branch_metadata(&branch());
  let empty_seg = 0u32.to_le_bytes();
  builder.append_segment_mapping(&empty_seg);
  let record = builder.finalize(1, 2).unwrap();

  let mut state = ReplayState::default();
  state.apply_log(&record).unwrap();
  assert_eq!(state.root_id, 1);
  assert_eq!(state.ttl_root_id, 2);
  assert_eq!(state.mapping_tbl.len(), 3);
  assert_eq!(state.mapping_tbl.get(1).unwrap(), encode_file_page_id(9));
  assert_eq!(
    state.mapping_tbl.get(2).unwrap(),
    fkv_manifest::invalid_mapping_value()
  );
}
