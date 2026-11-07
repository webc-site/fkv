use fkv_error::KvError;
use fkv_manifest::{
  BranchManifestMetadata, HEADER_BYTES, ManifestBuilder, deserialize_branch_metadata,
  manifest_checksum, parse_header, payload, serialize_branch_metadata, validate_record,
};
use fkv_types::{BranchFileRange, INVALID_VALUE, encode_file_page_id, encode_page_id};

fn sample_branch() -> BranchManifestMetadata {
  BranchManifestMetadata {
    branch_name: "main".into(),
    term: 7,
    file_range_li: vec![BranchFileRange {
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
  let bytes = serialize_branch_metadata(&metadata).unwrap();
  assert_eq!(deserialize_branch_metadata(&bytes).unwrap(), metadata);
}

#[test]
fn snapshot_record() {
  let mapping_value_li = [encode_file_page_id(5), encode_page_id(7), INVALID_VALUE];
  let mut builder = ManifestBuilder::new();
  let record = builder
    .snapshot(
      fkv_manifest::SnapshotArg {
        root_id: 3,
        ttl_root: 4,
        max_fp_id: 8,
        dict_bytes: b"dict",
        branch_metadata: &sample_branch(),
        max_segment_fp_id: 12,
      },
      &mapping_value_li,
      Some(&[encode_file_page_id(11)]),
    )
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
  builder.append_branch_metadata(&sample_branch()).unwrap();
  let record = builder.finalize(1, 2).unwrap();
  assert!(validate_record(&record).unwrap());
  let header = parse_header(&record).unwrap();
  assert_eq!(header.root, 1);
  assert_eq!(header.ttl_root, 2);
  assert!(header.payload_len > 0);
}

/// eng-76：跨多个 chunk 的 mapping，迭代器入参与切片入参必须产出完全相同的盘面字节。
#[test]
fn snapshot_with_iterator_matches_slice_across_chunks() {
  use fkv_mapping::{CHUNK_SIZE, MappingTbl};

  let mut tbl = MappingTbl::new();
  for index in 0..(CHUNK_SIZE * 2 + 7) {
    tbl.push(encode_file_page_id(index as u64 + 1));
  }
  let args = fkv_manifest::SnapshotArg {
    root_id: 3,
    ttl_root: 4,
    max_fp_id: 5,
    dict_bytes: b"dict",
    branch_metadata: &sample_branch(),
    max_segment_fp_id: 12,
  };
  let mapping_li = tbl.iter_value_li().collect::<Vec<_>>();
  let segment_li = [encode_file_page_id(11)];

  let slice = ManifestBuilder::new()
    .snapshot(args, &mapping_li, Some(&segment_li))
    .unwrap();
  let iter = ManifestBuilder::new()
    .snapshot_with(args, tbl.iter_value_li(), Some(segment_li.iter().copied()))
    .unwrap();

  assert_eq!(slice, iter);
  assert!(validate_record(&iter).unwrap());
}

#[test]
fn checksum_empty_is_zero() {
  assert_eq!(manifest_checksum(b""), 0);
  assert_ne!(manifest_checksum(b"x"), 0);
}

/// fmt-14：branch metadata 的 name 非 UTF-8 时直接 Corrupted，不做 lossy 替换。
#[test]
fn branch_metadata_rejects_invalid_utf8() {
  let metadata = sample_branch();
  let mut bytes = serialize_branch_metadata(&metadata).unwrap();
  // 第一个字段是 u32 name_len，紧跟 name 字节。
  let name_len = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
  assert!(name_len > 0);
  bytes[4] = 0xff;
  assert_eq!(
    deserialize_branch_metadata(&bytes).unwrap_err(),
    KvError::Corrupted
  );
}

/// `finalize` 取走 `buf` 后 builder 必须回到初始态：复用同一 builder 再
/// `append_branch_metadata` 时要重新写 mapping 段长度占位，否则长度下溢 / 记录缺 header。
#[test]
fn builder_reuse_after_finalize_restarts_cleanly() {
  let mut builder = ManifestBuilder::new();
  builder.update_mapping(1, encode_file_page_id(9));
  let first = builder.finalize(1, 2).unwrap();
  assert!(validate_record(&first).unwrap());

  builder.append_branch_metadata(&sample_branch()).unwrap();
  let second = builder.finalize(3, 4).unwrap();
  assert!(validate_record(&second).unwrap());
  let header = parse_header(&second).unwrap();
  assert_eq!((header.root, header.ttl_root), (3, 4));
  // 新记录没有 mapping 条目，mapping 段长度必须是 0。
  assert_eq!(
    u32::from_le_bytes(second[HEADER_BYTES..HEADER_BYTES + 4].try_into().unwrap()),
    0
  );
}

/// 仅写 segment 段（无 mapping / branch）时，mapping_len 必须按段边界写 0，
/// 且 segment 段前缀与内容不能被子段长度回填覆盖。
#[test]
fn segment_only_record_keeps_mapping_boundary() {
  let mut segment = Vec::new();
  fkv_codec::append_varint32(&mut segment, 1);
  fkv_codec::append_varint64(&mut segment, encode_file_page_id(7));

  let mut builder = ManifestBuilder::new();
  builder.append_segment_mapping(&segment);
  let record = builder.finalize(1, 2).unwrap();
  assert!(validate_record(&record).unwrap());

  let payload = payload(&record).unwrap();
  assert_eq!(&payload[..4], &0u32.to_le_bytes(), "mapping_len 必须为 0");
  assert_eq!(&payload[4..], segment.as_slice(), "segment 段必须原样保留");
}
