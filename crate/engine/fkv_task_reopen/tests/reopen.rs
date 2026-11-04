use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArgs, encode_file_page_id};
use fkv_task_reopen::{reopen, split_manifest};
use fkv_types::TableIdent;

#[test]
fn reopen_snapshot_and_log() {
  pollster::block_on(async {
    let store = MemStore::new(512);
    let tbl = TableIdent::new("demo", 1).unwrap();
    let branch = BranchManifestMetadata::new("main", 0);
    let mut builder = ManifestBuilder::new();
    let snapshot = builder
      .snapshot(SnapshotArgs {
        root_id: 1,
        ttl_root: 2,
        max_fp_id: 3,
        dict_bytes: b"",
        mapping_value_li: &[encode_file_page_id(5)],
        branch_metadata: &branch,
        segment_value_li: None,
        max_segment_fp_id: 0,
      })
      .unwrap();
    assert_eq!(split_manifest(&snapshot).unwrap().len(), 1);

    let mut builder = ManifestBuilder::new();
    builder.update_mapping(1, encode_file_page_id(9));
    builder.append_branch_metadata(&branch);
    builder.append_segment_mapping(&0u32.to_le_bytes());
    let log = builder.finalize(3, 4).unwrap();

    store.append_manifest(&tbl, &snapshot).await.unwrap();
    store.append_manifest(&tbl, &log).await.unwrap();
    let state = reopen(&store, &tbl).await.unwrap();
    assert_eq!(state.root_id, 3);
    assert_eq!(state.ttl_root_id, 4);
    assert_eq!(state.mapping_tbl.get(0).unwrap(), encode_file_page_id(5));
    assert_eq!(state.mapping_tbl.get(1).unwrap(), encode_file_page_id(9));
  });
}
