use fkv_manifest::BranchManifestMetadata;
use fkv_mapping::{AllocMode, MappingTbl};
use fkv_root::{RootMeta, RootMetaMgr};
use fkv_types::TableIdent;

#[test]
fn create_pin_release() {
  let mut mgr = RootMetaMgr::new(0, 1);
  let tbl = TableIdent::new("demo", 1).unwrap();
  let meta = mgr.get_or_create(&tbl, AllocMode::Append).unwrap();
  assert_eq!(meta.max_fp_id(), 0);
  assert!(mgr.pin(&tbl).is_some());
  assert!(mgr.release(&tbl));
  assert!(mgr.get(&tbl).is_some());
  assert_eq!(mgr.len(), 1);
}

#[test]
fn evict_unpinned() {
  let mut mgr = RootMetaMgr::new(8, 1);
  let tbl = TableIdent::new("demo", 1).unwrap();
  let meta = mgr.get_or_create(&tbl, AllocMode::Append).unwrap();
  for _ in 0..4 {
    meta.mapper.allocate_page();
  }
  assert_eq!(mgr.evict_if_needed(0), 1);
  assert!(mgr.get(&tbl).is_none());
}

#[test]
fn replay_meta() {
  let meta = RootMeta::from_replay(fkv_root::ReplayArgs {
    mode: AllocMode::Append,
    compression_level: 3,
    mapping_tbl: MappingTbl::new(),
    max_fp_id: 5,
    segment_mapping: MappingTbl::new(),
    max_segment_fp_id: 0,
    root_id: 1,
    ttl_root_id: 2,
    dict_bytes: Vec::new(),
    branch_metadata: BranchManifestMetadata::new("main", 0),
  })
  .unwrap();
  assert_eq!(meta.root_id, 1);
  assert_eq!(meta.ttl_root_id, 2);
  assert_eq!(meta.max_fp_id(), 5);
}
