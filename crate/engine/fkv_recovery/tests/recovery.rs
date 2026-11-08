use fkv_codec::{append_varint32, append_varint64};
use fkv_manifest::{
  BranchManifestMetadata, HEADER_BYTES, ManifestBuilder, OFFSET_PAYLOAD_LEN, OFFSET_ROOT,
  OFFSET_TTL_ROOT, SnapshotArg, manifest_checksum,
};
use fkv_recovery::ReplayState;
use fkv_types::{BranchFileRange, encode_file_page_id, encode_page_id};

fn branch() -> BranchManifestMetadata {
  BranchManifestMetadata {
    branch_name: "main".into(),
    term: 3,
    file_range_li: vec![BranchFileRange {
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
    .snapshot(
      SnapshotArg {
        root_id: 1,
        ttl_root: 2,
        max_fp_id: 8,
        dict_bytes: b"dict",
        branch_metadata: &branch(),
        max_segment_fp_id: 12,
      },
      &mapping_li,
      Some(&segment_li),
    )
    .unwrap();
  let state = ReplayState::from_snapshot(&record).unwrap();
  assert_eq!(state.root_id, 1);
  assert_eq!(state.ttl_root_id, 2);
  assert_eq!(state.max_fp_id, 8);
  assert_eq!(state.dict_bytes, b"dict");
  assert_eq!(
    state.mapping_tbl.iter_value_li().collect::<Vec<_>>(),
    &mapping_li
  );
  assert_eq!(
    state.segment_mapping.iter_value_li().collect::<Vec<_>>(),
    &segment_li
  );
  assert_eq!(state.branch_metadata, branch());
}

#[test]
fn log_replay() {
  let mut builder = ManifestBuilder::new();
  builder.update_mapping(1, encode_file_page_id(9));
  builder.delete_mapping(2);
  builder.append_branch_metadata(&branch()).unwrap();
  let empty_seg = 0u32.to_le_bytes();
  builder.append_segment_mapping(&empty_seg);
  let record = builder.finalize(1, 2).unwrap();

  let mut state = ReplayState::default();
  state.apply_log(&record).unwrap();
  assert_eq!(state.root_id, 1);
  assert_eq!(state.ttl_root_id, 2);
  assert_eq!(state.mapping_tbl.len(), 3);
  assert_eq!(state.mapping_tbl.get(1).unwrap(), encode_file_page_id(9));
  assert_eq!(state.mapping_tbl.get(2).unwrap(), fkv_types::INVALID_VALUE);
}

/// fmt-09：log record 缺 branch/segment 可选段时应能 replay（对齐 C++ ReplayLog）。
#[test]
fn log_without_optional_sections_replays() {
  let snapshot = ManifestBuilder::new()
    .snapshot(
      SnapshotArg {
        root_id: 1,
        ttl_root: 2,
        max_fp_id: 3,
        dict_bytes: b"",
        branch_metadata: &branch(),
        max_segment_fp_id: 0,
      },
      &[encode_file_page_id(5)],
      None,
    )
    .unwrap();
  let mut state = ReplayState::from_snapshot(&snapshot).unwrap();

  // 只写 mapping 段，不写 branch / segment 段。
  let mut builder = ManifestBuilder::new();
  builder.update_mapping(1, encode_file_page_id(9));
  let log = builder.finalize(3, 4).unwrap();
  state.apply_log(&log).unwrap();
  assert_eq!(state.mapping_tbl.get(1).unwrap(), encode_file_page_id(9));
  assert_eq!(state.root_id, 3);
  // 缺失的可选段不改变已有 branch metadata。
  assert_eq!(state.branch_metadata.term, 3);
}

/// sto-53：`apply_log` 按 delta 增量维护 `max_fp_id`，不再每条 record 全表重扫。
#[test]
fn log_replay_increments_max_fp_id() {
  let mut builder = ManifestBuilder::new();
  builder.update_mapping(1, encode_file_page_id(20));
  // 非 FilePageId 不参与高水位。
  builder.update_mapping(2, encode_page_id(99));
  builder.delete_mapping(3);
  let record = builder.finalize(1, 2).unwrap();

  let mut state = ReplayState::default();
  state.apply_log(&record).unwrap();
  assert_eq!(state.max_fp_id, 21);

  // 后续 record 只升不降。
  let mut builder = ManifestBuilder::new();
  builder.update_mapping(1, encode_file_page_id(30));
  let record = builder.finalize(1, 2).unwrap();
  state.apply_log(&record).unwrap();
  assert_eq!(state.max_fp_id, 31);

  let mut builder = ManifestBuilder::new();
  builder.update_mapping(1, encode_file_page_id(10));
  let record = builder.finalize(1, 2).unwrap();
  state.apply_log(&record).unwrap();
  assert_eq!(state.max_fp_id, 31);
}

/// sto-53：snapshot 的持久化高水位高于表内最大值时，log 增量从该值继续。
#[test]
fn log_replay_keeps_snapshot_max_fp_id() {
  let snapshot = ManifestBuilder::new()
    .snapshot(
      SnapshotArg {
        root_id: 1,
        ttl_root: 2,
        max_fp_id: 50,
        dict_bytes: b"",
        branch_metadata: &branch(),
        max_segment_fp_id: 0,
      },
      &[encode_file_page_id(5)],
      None,
    )
    .unwrap();
  let mut state = ReplayState::from_snapshot(&snapshot).unwrap();
  assert_eq!(state.max_fp_id, 50);

  let mut builder = ManifestBuilder::new();
  builder.update_mapping(1, encode_file_page_id(9));
  let log = builder.finalize(1, 2).unwrap();
  state.apply_log(&log).unwrap();
  assert_eq!(state.max_fp_id, 50);
}

/// sto-53：segment delta 同样增量维护 `max_segment_fp_id`。
#[test]
fn log_replay_increments_max_segment_fp_id() {
  let seg_li = [(1u32, encode_file_page_id(11)), (2, encode_file_page_id(7))];
  let mut seg_bytes = Vec::new();
  let mut payload = Vec::new();
  for (page_id, value) in seg_li {
    fkv_codec::append_varint32(&mut payload, page_id);
    fkv_codec::append_varint64(&mut payload, value);
  }
  seg_bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
  seg_bytes.extend_from_slice(&payload);

  let mut builder = ManifestBuilder::new();
  builder.update_mapping(1, encode_file_page_id(3));
  builder.append_branch_metadata(&branch()).unwrap();
  builder.append_segment_mapping(&seg_bytes);
  let record = builder.finalize(1, 2).unwrap();

  let mut state = ReplayState::default();
  state.apply_log(&record).unwrap();
  assert_eq!(state.max_segment_fp_id, 12);
}

/// 组装完整 record：补 header / payload_len / checksum，对齐 `ManifestBuilder::finalize`。
fn wrap_record(payload: &[u8], root_id: u32, ttl_root: u32) -> Vec<u8> {
  let mut record = vec![0u8; HEADER_BYTES];
  record[OFFSET_ROOT..OFFSET_ROOT + 4].copy_from_slice(&root_id.to_le_bytes());
  record[OFFSET_TTL_ROOT..OFFSET_TTL_ROOT + 4].copy_from_slice(&ttl_root.to_le_bytes());
  record[OFFSET_PAYLOAD_LEN..OFFSET_PAYLOAD_LEN + 4]
    .copy_from_slice(&(payload.len() as u32).to_le_bytes());
  record.extend_from_slice(payload);
  let sum = manifest_checksum(&record[8..]);
  record[..8].copy_from_slice(&sum.to_le_bytes());
  record
}

/// snapshot payload：`max_fp_id | dict_len | dict | mapping_len | mapping`，
/// 可在 mapping 之后追加任意 `tail`（用于构造缺段 / 尾残留）。
fn snapshot_payload(max_fp_id: u64, mapping_li: &[u64], tail: &[u8]) -> Vec<u8> {
  let mut mapping = Vec::new();
  for value in mapping_li {
    append_varint64(&mut mapping, *value);
  }
  let mut payload = Vec::new();
  append_varint64(&mut payload, max_fp_id);
  append_varint32(&mut payload, 0);
  payload.extend_from_slice(&(mapping.len() as u32).to_le_bytes());
  payload.extend_from_slice(&mapping);
  payload.extend_from_slice(tail);
  payload
}

/// log payload：`mapping_len | mapping | branch_tail | seg_tail`。
fn log_payload(entry_li: &[(u32, u64)], branch_tail: &[u8], seg_tail: &[u8]) -> Vec<u8> {
  let mut mapping = Vec::new();
  for (page_id, value) in entry_li {
    append_varint32(&mut mapping, *page_id);
    append_varint64(&mut mapping, *value);
  }
  let mut payload = (mapping.len() as u32).to_le_bytes().to_vec();
  payload.extend_from_slice(&mapping);
  payload.extend_from_slice(branch_tail);
  payload.extend_from_slice(seg_tail);
  payload
}

/// sto-68：旧格式 snapshot 无 branch 段，按 main/term=0 默认值继续 replay。
#[test]
fn snapshot_without_branch_section_replays() {
  let payload = snapshot_payload(9, &[encode_file_page_id(5), encode_page_id(7)], &[]);
  let snapshot = wrap_record(&payload, 1, 2);
  let state = ReplayState::from_snapshot(&snapshot).unwrap();
  assert_eq!(state.root_id, 1);
  assert_eq!(state.ttl_root_id, 2);
  assert_eq!(state.max_fp_id, 9);
  assert_eq!(
    state.branch_metadata,
    BranchManifestMetadata::new("main", 0)
  );
}

/// sto-68：snapshot 尾部 1-3 字节残留按缺段处理（C++ `remaining.size() >= 4`）。
#[test]
fn snapshot_tolerates_short_tail() {
  for tail_len in 1..4 {
    let payload = snapshot_payload(9, &[encode_file_page_id(5)], &vec![0xAA; tail_len]);
    let snapshot = wrap_record(&payload, 1, 2);
    let state = ReplayState::from_snapshot(&snapshot).unwrap();
    assert_eq!(state.mapping_tbl.len(), 1);
    assert_eq!(
      state.branch_metadata,
      BranchManifestMetadata::new("main", 0)
    );
  }
}

/// sto-68：log 的 branch 段长度为 0 时保留旧 metadata 并继续 replay。
#[test]
fn log_with_empty_branch_section_keeps_old_metadata() {
  let snapshot = ManifestBuilder::new()
    .snapshot(
      SnapshotArg {
        root_id: 1,
        ttl_root: 2,
        max_fp_id: 3,
        dict_bytes: b"",
        branch_metadata: &branch(),
        max_segment_fp_id: 0,
      },
      &[encode_file_page_id(5)],
      None,
    )
    .unwrap();
  let mut state = ReplayState::from_snapshot(&snapshot).unwrap();

  let payload = log_payload(
    &[(1, encode_file_page_id(9))],
    &0u32.to_le_bytes(),
    &0u32.to_le_bytes(),
  );
  let record = wrap_record(&payload, 3, 4);
  state.apply_log(&record).unwrap();
  assert_eq!(state.mapping_tbl.get(1).unwrap(), encode_file_page_id(9));
  assert_eq!(state.branch_metadata, branch());
}

/// sto-68：log 声明长度超过剩余字节时跳过该段（C++ 同样不消费负载）。
#[test]
fn log_with_oversized_branch_section_keeps_old_metadata() {
  let mut state = ReplayState {
    branch_metadata: branch(),
    ..Default::default()
  };

  let payload = log_payload(&[(1, encode_file_page_id(9))], &999u32.to_le_bytes(), &[]);
  let record = wrap_record(&payload, 3, 4);
  state.apply_log(&record).unwrap();
  assert_eq!(state.mapping_tbl.get(1).unwrap(), encode_file_page_id(9));
  assert_eq!(state.branch_metadata, branch());
}
