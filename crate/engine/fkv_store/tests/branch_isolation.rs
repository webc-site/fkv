use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  thread::spawn,
  time::{SystemTime, UNIX_EPOCH},
};

use compio::runtime::Runtime;
use fkv_btree::BatchEntry;
use fkv_io::ManifestIo;
use fkv_io_compio::CompioStore;
use fkv_large::SegmentLayout;
use fkv_recovery::ReplayState;
use fkv_store::{BRANCH_FILE_RESERVE_FILE_NUM, Store};
use fkv_types::TableIdent;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_branch_iso_{}_{}_{}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos(),
    seq,
  ));
  create_dir_all(&root).unwrap();
  root
}

fn segment_file_id_set(store: &Store<CompioStore>, shift: u8) -> Vec<u64> {
  store
    .segment_mapper(&TableIdent::new("demo", 1).unwrap())
    .unwrap()
    .tbl()
    .value_li()
    .iter()
    .filter(|value| fkv_mapping::is_file_page_id(**value))
    .map(|value| fkv_mapping::decode_id(*value) >> shift)
    .collect()
}

fn file_id_set(store: &Store<CompioStore>) -> Vec<u64> {
  store
    .mapper(&TableIdent::new("demo", 1).unwrap())
    .unwrap()
    .tbl()
    .value_li()
    .iter()
    .filter(|value| fkv_mapping::is_file_page_id(**value))
    .map(|value| fkv_mapping::decode_id(*value) >> 18)
    .collect()
}

#[compio::test]
async fn main_and_branch_use_disjoint_file_ranges_under_concurrent_writes() {
  let root = temp_root();
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let mut main =
    Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16).with_large_value(layout, 1024);
  main
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  main.create_branch(&tbl, "feature").await.unwrap();

  let branch_bytes = main
    .page_io()
    .read_manifest_file(&tbl, "manifest_feature_0")
    .await
    .unwrap()
    .unwrap();
  let branch_state = ReplayState::from_snapshot(&branch_bytes).unwrap();
  let shift = 18u32;
  let base_file = branch_state.max_fp_id >> shift;
  let limit_file = base_file + BRANCH_FILE_RESERVE_FILE_NUM;
  let base_fp = base_file << shift;
  let limit_fp = limit_file << shift;
  let segment_shift = 2u8;
  let segment_base_file = branch_state.max_segment_fp_id >> segment_shift;
  let segment_limit_file = segment_base_file + BRANCH_FILE_RESERVE_FILE_NUM;
  assert_eq!(main.mapper(&tbl).unwrap().max_fp_id(), limit_fp);
  assert_eq!(
    main.segment_mapper(&tbl).unwrap().max_fp_id(),
    segment_limit_file << segment_shift
  );

  let mut branch =
    Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16).with_large_value(layout, 1024);
  branch.open_branch(&tbl, "feature", 0).await.unwrap();
  assert_eq!(
    branch.segment_mapper(&tbl).unwrap().max_fp_id(),
    segment_base_file << segment_shift
  );
  assert_eq!(
    branch.branch_segment_limit(),
    Some(segment_limit_file << segment_shift)
  );
  assert_eq!(branch.mapper(&tbl).unwrap().max_fp_id(), base_fp);
  assert_eq!(branch.branch_file_limit(), Some(limit_fp));

  let branch_tbl = tbl.clone();
  let branch_handle = spawn(move || {
    let runtime = Runtime::new().unwrap();
    runtime.block_on(async move {
      for batch_index in 0..5u64 {
        let batch: Vec<BatchEntry> = (0..10)
          .map(|index| {
            let key_index = batch_index * 10 + index;
            BatchEntry::upsert(
              format!("b:{key_index:04}").into_bytes(),
              format!("branch-{key_index}").into_bytes(),
              2,
            )
          })
          .collect();
        branch.put(&branch_tbl, &batch).await.unwrap();
      }
      branch
        .put_large(
          &branch_tbl,
          &[BatchEntry::upsert(
            b"b-big".to_vec(),
            (0..5000)
              .map(|index| (index % 251) as u8)
              .collect::<Vec<u8>>(),
            2,
          )],
        )
        .await
        .unwrap();
      branch
    })
  });

  for batch_index in 0..5u64 {
    let batch: Vec<BatchEntry> = (0..10)
      .map(|index| {
        let key_index = batch_index * 10 + index;
        BatchEntry::upsert(
          format!("m:{key_index:04}").into_bytes(),
          format!("main-{key_index}").into_bytes(),
          2,
        )
      })
      .collect();
    main.put(&tbl, &batch).await.unwrap();
  }
  main
    .put_large(
      &tbl,
      &[BatchEntry::upsert(
        b"m-big".to_vec(),
        (0..9000)
          .map(|index| (index % 251) as u8)
          .collect::<Vec<u8>>(),
        2,
      )],
    )
    .await
    .unwrap();
  let branch = branch_handle.join().unwrap();

  let main_file_li = file_id_set(&main);
  assert!(
    main_file_li
      .iter()
      .all(|file_id| *file_id < base_file || *file_id >= limit_file),
    "main wrote into branch range: {main_file_li:?}"
  );
  assert!(main_file_li.iter().any(|file_id| *file_id >= limit_file));
  let branch_file_li = file_id_set(&branch);
  assert!(
    branch_file_li.iter().all(|file_id| *file_id < limit_file),
    "branch exceeded reserved range: {branch_file_li:?}"
  );
  assert!(branch_file_li.iter().any(|file_id| *file_id >= base_file));
  let main_segment_li = segment_file_id_set(&main, segment_shift);
  assert!(
    main_segment_li
      .iter()
      .all(|file_id| *file_id < segment_base_file || *file_id >= segment_limit_file),
    "main wrote into branch segment range: {main_segment_li:?}"
  );
  assert!(
    main_segment_li
      .iter()
      .any(|file_id| *file_id >= segment_limit_file)
  );
  let branch_segment_li = segment_file_id_set(&branch, segment_shift);
  assert!(
    branch_segment_li
      .iter()
      .all(|file_id| *file_id < segment_limit_file),
    "branch exceeded reserved segment range: {branch_segment_li:?}"
  );
  assert!(
    branch_segment_li
      .iter()
      .any(|file_id| *file_id >= segment_base_file)
  );

  for index in 0..50 {
    assert_eq!(
      main
        .get(&tbl, format!("m:{index:04}").as_bytes())
        .await
        .unwrap(),
      Some(format!("main-{index}").into_bytes())
    );
    assert_eq!(
      main
        .get(&tbl, format!("b:{index:04}").as_bytes())
        .await
        .unwrap(),
      None
    );
    assert_eq!(
      branch
        .get(&tbl, format!("b:{index:04}").as_bytes())
        .await
        .unwrap(),
      Some(format!("branch-{index}").into_bytes())
    );
    assert_eq!(
      branch
        .get(&tbl, format!("m:{index:04}").as_bytes())
        .await
        .unwrap(),
      None
    );
  }
  drop(main);
  drop(branch);

  // 重启后区间上限仍保持：main 从 limit 之后分配，branch 仍限制在预留块内。
  let mut main =
    Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16).with_large_value(layout, 1024);
  main.reopen(&tbl).await.unwrap();
  assert!(main.mapper(&tbl).unwrap().max_fp_id() >= limit_fp);
  assert_eq!(
    main.get(&tbl, b"m:0000").await.unwrap(),
    Some(b"main-0".to_vec())
  );
  assert_eq!(main.get(&tbl, b"b:0000").await.unwrap(), None);
  drop(main);

  let mut branch =
    Store::new(CompioStore::new(root.clone(), 512, 18), 512, 16).with_large_value(layout, 1024);
  branch.open_branch(&tbl, "feature", 0).await.unwrap();
  assert_eq!(branch.branch_file_limit(), Some(limit_fp));
  assert!(branch.mapper(&tbl).unwrap().max_fp_id() < limit_fp);
  assert_eq!(
    branch.get(&tbl, b"b:0000").await.unwrap(),
    Some(b"branch-0".to_vec())
  );
  assert_eq!(branch.get(&tbl, b"m:0000").await.unwrap(), None);

  remove_dir_all(root).unwrap();
}
