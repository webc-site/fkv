use fkv_btree::BatchEntry;
use fkv_codec::{get_fixed32, get_varint32, get_varint64};
use fkv_error::KvError;
use fkv_io::{FileIo, ManifestIo};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_recovery::ReplayState;
use fkv_store::{ArchiveTagInfo, BRANCH_FILE_RESERVE_FILE_NUM, Store};
use fkv_testkit::TempRoot;
use fkv_types::{PageId, TableIdent, decode_id, is_file_page_id};

fn take_varint64(body: &[u8], pos: &mut usize) -> u64 {
  let (value, read) = get_varint64(&body[*pos..]).expect("varint64");
  *pos += read;
  value
}

fn take_varint32(body: &[u8], pos: &mut usize) -> u32 {
  let (value, read) = get_varint32(&body[*pos..]).expect("varint32");
  *pos += read;
  value
}

fn take_fixed32(body: &[u8], pos: &mut usize) -> u32 {
  let value = get_fixed32(&body[*pos..]).expect("fixed32");
  *pos += 4;
  value
}

fn decode_values(section: &[u8]) -> Vec<u64> {
  let mut pos = 0usize;
  let mut value_li = Vec::new();
  while pos < section.len() {
    value_li.push(take_varint64(section, &mut pos));
  }
  value_li
}

/// 解析 snapshot 的可选 segment 段：`None` 表示记录在 branch metadata 后结束，
/// 与 C++ `ManifestBuilder::Snapshot` 的 `segment_mapping == nullptr` 一致。
fn snapshot_segment_section(record: &[u8]) -> Option<(u64, Vec<u64>)> {
  let body = fkv_manifest::payload(record).expect("snapshot payload");
  let mut pos = 0usize;
  let _max_fp_id = take_varint64(body, &mut pos);
  let dict_len = take_varint32(body, &mut pos) as usize;
  pos += dict_len;
  let mapping_len = take_fixed32(body, &mut pos) as usize;
  pos += mapping_len;
  let branch_len = take_fixed32(body, &mut pos) as usize;
  pos += branch_len;
  if pos == body.len() {
    return None;
  }
  let max_segment_fp_id = take_varint64(body, &mut pos);
  let seg_len = take_fixed32(body, &mut pos) as usize;
  let value_li = decode_values(&body[pos..pos + seg_len]);
  pos += seg_len;
  assert_eq!(pos, body.len(), "snapshot payload 不应有尾部多余字节");
  Some((max_segment_fp_id, value_li))
}

fn archive_tag_li(name_li: &[String]) -> Vec<String> {
  let mut tag_li: Vec<String> = name_li
    .iter()
    .filter_map(|name| {
      let (_, _, tag) = fkv_types::parse_manifest_file_name(name)?;
      tag.map(str::to_owned)
    })
    .collect();
  tag_li.sort();
  tag_li
}

#[test]
fn archive_creation_retention_and_branch_metadata() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16)
      .with_branch("main", 7)
      .with_pages_per_file_shift(18);
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    let parent_max_fp = store.mapper(&tbl).unwrap().max_fp_id();
    let parent_root = store.root_id(&tbl).unwrap();

    let first = store.create_archive(&tbl, "001", 2).await.unwrap();
    assert_eq!(first.name, "manifest_main_7_001");
    assert!(first.bytes > 0);
    assert_eq!(first.removed_num, 0);
    store.create_archive(&tbl, "002", 2).await.unwrap();
    let third = store.create_archive(&tbl, "003", 2).await.unwrap();
    assert_eq!(third.removed_num, 1, "oldest archive should be pruned");

    let name_li = store
      .page_io()
      .list_manifest_file_name_li(&tbl)
      .await
      .unwrap();
    assert_eq!(archive_tag_li(&name_li), vec!["002", "003"]);
    assert!(!name_li.contains(&"manifest_main_7_001".to_owned()));

    let bytes = store
      .page_io()
      .read_manifest_file(&tbl, "manifest_main_7_003")
      .await
      .unwrap()
      .unwrap();
    assert!(fkv_manifest::validate_record(&bytes).unwrap());
    let state = ReplayState::from_snapshot(&bytes).unwrap();
    assert_eq!(state.root_id, parent_root);
    assert_eq!(state.branch_metadata.branch_name.as_ref(), "main");
    assert_eq!(state.branch_metadata.term, 7);
    assert_eq!(state.branch_metadata.file_range_li.len(), 1);
    assert_eq!(
      state.branch_metadata.file_range_li[0].max_file_id,
      parent_max_fp >> 18
    );

    // N=0 表示不裁剪。
    let info = store.create_archive(&tbl, "004", 0).await.unwrap();
    assert_eq!(info.removed_num, 0);
    assert_eq!(
      archive_tag_li(
        &store
          .page_io()
          .list_manifest_file_name_li(&tbl)
          .await
          .unwrap()
      )
      .len(),
      3
    );

    let branch = store.create_branch(&tbl, "Feature-1").await.unwrap();
    assert_eq!(branch.name, "manifest_feature-1_0");
    let branch_bytes = store
      .page_io()
      .read_manifest_file(&tbl, &branch.name)
      .await
      .unwrap()
      .unwrap();
    let branch_state = ReplayState::from_snapshot(&branch_bytes).unwrap();
    assert_eq!(branch_state.root_id, parent_root);
    assert_eq!(
      branch_state.branch_metadata.branch_name.as_ref(),
      "feature-1"
    );
    assert_eq!(branch_state.branch_metadata.term, 0);
    let next_file = (parent_max_fp >> 18) + 1;
    let expected_file =
      next_file.div_ceil(BRANCH_FILE_RESERVE_FILE_NUM) * BRANCH_FILE_RESERVE_FILE_NUM;
    assert_eq!(branch_state.max_fp_id, expected_file << 18);
    // eng-54：子分支 manifest 必须继承父分支的 file_range_li 链。
    assert!(
      branch_state.branch_metadata.file_range_li.len() >= 2,
      "branch manifest should inherit parent ranges, got {:?}",
      branch_state.branch_metadata.file_range_li
    );
    assert_eq!(
      branch_state.branch_metadata.file_range_li[0]
        .branch_name
        .as_ref(),
      "main"
    );
    assert_eq!(
      branch_state
        .branch_metadata
        .file_range_li
        .last()
        .unwrap()
        .branch_name
        .as_ref(),
      "feature-1"
    );

    assert_eq!(
      store.create_branch(&tbl, "main").await.unwrap_err(),
      KvError::InvalidArgs
    );
    assert_eq!(
      store.delete_branch(&tbl, "main").await.unwrap_err(),
      KvError::InvalidArgs
    );
    assert_eq!(store.delete_branch(&tbl, "feature-1").await.unwrap(), 1);
    assert!(
      store
        .page_io()
        .read_manifest_file(&tbl, "manifest_feature-1_0")
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(
      store.create_archive(&tbl, "bad/tag", 1).await.unwrap_err(),
      KvError::InvalidArgs
    );
  });
}

/// `list_archive_tags`：表级 inventory，跨 branch / term，按 `(tag, term)` 升序
/// 去重；不读 manifest 内容，也不要求表处于打开状态。
#[test]
fn list_archive_tags_is_table_scoped_sorted_and_deduped() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let other = TableIdent::new("demo", 2).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16)
      .with_branch("main", 7)
      .with_pages_per_file_shift(18);
    store
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    store
      .put(
        &other,
        &[BatchEntry::upsert(b"x".to_vec(), b"X".to_vec(), 1)],
      )
      .await
      .unwrap();

    // 不按创建顺序：结果必须按 (tag, term) 升序。
    for tag in ["010", "002", "zz"] {
      store.create_archive(&tbl, tag, 0).await.unwrap();
    }
    store.create_archive(&other, "other", 0).await.unwrap();

    assert_eq!(
      store.list_archive_tags(&tbl).await.unwrap(),
      vec![
        ArchiveTagInfo {
          tag: "002".to_owned(),
          term: 7,
        },
        ArchiveTagInfo {
          tag: "010".to_owned(),
          term: 7,
        },
        ArchiveTagInfo {
          tag: "zz".to_owned(),
          term: 7,
        },
      ]
    );
    // 表级隔离：另一个分区的归档不出现在本表清单里。
    assert_eq!(
      store.list_archive_tags(&other).await.unwrap(),
      vec![ArchiveTagInfo {
        tag: "other".to_owned(),
        term: 7,
      }]
    );

    // 换 branch / term 后仍能看到旧 branch 的归档（表级 inventory），
    // 且同一 table 内相同 (tag, term) 只回一条。
    store.prepare_attach("dev", 8).unwrap();
    let entry_li = store.list_archive_tag_li(&tbl).await.unwrap();
    assert_eq!(entry_li, store.list_archive_tags(&tbl).await.unwrap());
    assert_eq!(entry_li.len(), 3);
    assert!(entry_li.iter().all(|entry| entry.term == 7));

    // 没有归档的分区返回空清单，不建表、不报错。
    let empty = TableIdent::new("absent", 9).unwrap();
    assert!(store.list_archive_tags(&empty).await.unwrap().is_empty());
  });
}

#[compio::test]
async fn compio_archive_files_and_branch_manifest() {
  let root = TempRoot::new("store_branch_archive_compio");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  )
  .with_branch("main", 3);
  store
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  store.create_archive(&tbl, "001", 2).await.unwrap();
  store.create_archive(&tbl, "002", 2).await.unwrap();
  store.create_archive(&tbl, "003", 2).await.unwrap();
  assert!(!root.join("demo.1").join("manifest_main_3_001").exists());
  assert!(root.join("demo.1").join("manifest_main_3_002").exists());
  assert!(root.join("demo.1").join("manifest_main_3_003").exists());

  let branch = store.create_branch(&tbl, "feature").await.unwrap();
  assert!(root.join("demo.1").join(&branch.name).exists());
  assert!(root.join("demo.1").join("manifest_main_0").exists());
  assert_eq!(store.delete_branch(&tbl, "feature").await.unwrap(), 1);
  assert!(!root.join("demo.1").join(&branch.name).exists());

  drop(store);
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_ne!(store.root_id(&tbl), Some(PageId::MAX));
}

/// `reopen_archive(tag)` 把归档快照提升为活跃状态；`delete_archive(tag)` 删除
/// 该 tag 的归档且幂等。两者都必须经得起进程重启（盘面契约，不只是内存态）。
#[compio::test]
async fn reopen_and_delete_archive() {
  let root = TempRoot::new("store_reopen_archive");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  )
  .with_branch("main", 3);
  store
    .put(
      &tbl,
      &[
        BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
        BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
      ],
    )
    .await
    .unwrap();
  let archived = store.create_archive(&tbl, "v1", 0).await.unwrap();
  assert_eq!(archived.name, "manifest_main_3_v1");
  // Compio（真实文件）后端：归档 manifest 也能被表级列举看到。
  assert_eq!(
    store.list_archive_tags(&tbl).await.unwrap(),
    vec![ArchiveTagInfo {
      tag: "v1".to_owned(),
      term: 3,
    }]
  );

  // 归档后继续写：活跃状态领先于 v1。
  store
    .put(&tbl, &[BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 2)])
    .await
    .unwrap();
  assert_eq!(store.get(&tbl, b"c").await.unwrap(), Some(b"C".to_vec()));

  // 按 tag 重开：内存态回到 v1，并写回活跃 manifest。
  let info = store.reopen_archive(&tbl, "v1").await.unwrap();
  assert_eq!(info.name, "manifest_main_3_v1");
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(store.get(&tbl, b"c").await.unwrap(), None);
  store
    .put(&tbl, &[BatchEntry::upsert(b"d".to_vec(), b"D".to_vec(), 3)])
    .await
    .unwrap();

  // 重启后仍是 v1 快照：证明确实把归档提升为活跃 manifest，而不是只改内存。
  drop(store);
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(store.get(&tbl, b"c").await.unwrap(), None);
  assert_eq!(store.get(&tbl, b"d").await.unwrap(), Some(b"D".to_vec()));

  // 冷启动直接按 tag 恢复（不先 reopen 活跃 manifest）：归档选择不依赖 table
  // 已打开；恢复后活跃状态 = 归档快照。
  drop(store);
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  let cold = store.reopen_archive(&tbl, "v1").await.unwrap();
  assert_eq!(cold.name, "manifest_main_3_v1");
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(store.get(&tbl, b"d").await.unwrap(), None);

  // 空 tag = 重开活跃快照（C++ ReopenRequest 语义）。
  let active = store.reopen_archive(&tbl, "").await.unwrap();
  assert_eq!(active.name, "manifest_main_0");
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

  // 删除 tag：v1 从盘面消失；重复删除幂等；空 tag 非法。
  assert_eq!(store.delete_archive(&tbl, "v1").await.unwrap(), 1);
  assert!(
    store
      .page_io()
      .read_manifest_file(&tbl, "manifest_main_3_v1")
      .await
      .unwrap()
      .is_none()
  );
  assert!(store.list_archive_tags(&tbl).await.unwrap().is_empty());
  assert_eq!(store.delete_archive(&tbl, "v1").await.unwrap(), 0);
  assert_eq!(
    store.delete_archive(&tbl, "").await.unwrap_err(),
    KvError::InvalidArgs
  );
  assert_eq!(
    store.reopen_archive(&tbl, "missing").await.unwrap_err(),
    KvError::ResourceMissing
  );
  assert_eq!(
    store.reopen_archive(&tbl, "bad/tag").await.unwrap_err(),
    KvError::InvalidArgs
  );
}

/// plain store（未启用 large value）的 branch / archive snapshot 必须与 C++
/// `ManifestBuilder::Snapshot` 完全同构：记录在 branch metadata 后结束，
/// 不写可选 segment 段，也不为 branch 凭空创建 segment mapper。
#[test]
fn branch_and_archive_without_segment_mapping_omit_segment_section() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16)
      .with_branch("main", 7)
      .with_pages_per_file_shift(18);
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    assert!(store.segment_mapper(&tbl).is_none());

    let archive = store.create_archive(&tbl, "001", 2).await.unwrap();
    let archive_bytes = store
      .page_io()
      .read_manifest_file(&tbl, &archive.name)
      .await
      .unwrap()
      .unwrap();
    assert!(fkv_manifest::validate_record(&archive_bytes).unwrap());
    assert_eq!(
      snapshot_segment_section(&archive_bytes),
      None,
      "无 segment mapping 的 archive snapshot 不应带 segment 段"
    );
    let archive_state = ReplayState::from_snapshot(&archive_bytes).unwrap();
    assert!(archive_state.segment_mapping.is_empty());
    assert_eq!(archive_state.max_segment_fp_id, 0);

    let branch = store.create_branch(&tbl, "feature").await.unwrap();
    let branch_bytes = store
      .page_io()
      .read_manifest_file(&tbl, &branch.name)
      .await
      .unwrap()
      .unwrap();
    assert!(fkv_manifest::validate_record(&branch_bytes).unwrap());
    assert_eq!(
      snapshot_segment_section(&branch_bytes),
      None,
      "无 segment mapping 的 branch snapshot 不应带 segment 段"
    );
    assert!(
      store.segment_mapper(&tbl).is_none(),
      "plain store 的 create_branch 不应创建 segment mapper"
    );
    let branch_state = ReplayState::from_snapshot(&branch_bytes).unwrap();
    assert!(branch_state.segment_mapping.is_empty());
    assert_eq!(branch_state.max_segment_fp_id, 0);
    assert_eq!(
      store
        .page_io()
        .read_manifest_file(&tbl, &branch.name)
        .await
        .unwrap()
        .unwrap(),
      branch_bytes
    );
  });
}

/// 启用 large value 且已写入 segment 时，branch snapshot 必须保留 segment
/// 映射（C++ `CreateBranch` 不传该段，但 Rust 用它在分支间继承大 value，
/// 这是有意保留的扩展）；重开分支后仍能读回父分支的大 value。
#[test]
fn branch_snapshot_with_large_value_keeps_segment_mapping() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let big: Vec<u8> = (0..4200).map(|index| (index % 251) as u8).collect();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024)
      .with_branch("main", 7);
    store
      .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 1)])
      .await
      .unwrap();
    let live_segment_num = store
      .segment_mapper(&tbl)
      .unwrap()
      .tbl()
      .iter_value_li()
      .filter(|value| fkv_types::is_file_page_id(*value))
      .count();
    assert!(live_segment_num > 0);

    let archive = store.create_archive(&tbl, "001", 2).await.unwrap();
    let archive_bytes = store
      .page_io()
      .read_manifest_file(&tbl, &archive.name)
      .await
      .unwrap()
      .unwrap();
    let (_, archive_segments) =
      snapshot_segment_section(&archive_bytes).expect("archive with large value keeps segment");
    assert_eq!(archive_segments.len(), live_segment_num);

    let branch = store.create_branch(&tbl, "feature").await.unwrap();
    let branch_bytes = store
      .page_io()
      .read_manifest_file(&tbl, &branch.name)
      .await
      .unwrap()
      .unwrap();
    let (_, branch_segments) =
      snapshot_segment_section(&branch_bytes).expect("branch with large value keeps segment");
    assert_eq!(branch_segments.len(), live_segment_num);
    let branch_state = ReplayState::from_snapshot(&branch_bytes).unwrap();
    assert!(!branch_state.segment_mapping.is_empty());

    // 分支继承父分支的 mapping + segment mapping：重开分支可读回大 value。
    let io = store.into_io();
    let mut reopened = Store::new(io, 512, 16).with_large_value(layout, 1024);
    reopened.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(
      reopened
        .get_large(&tbl, b"big")
        .await
        .unwrap()
        .unwrap()
        .value,
      big
    );
  });
}

/// Drop 只删当前 branch 的活跃 manifest，不能破坏 archive / 其它 branch。
#[compio::test]
async fn drop_keeps_archive_and_other_branch_readable() {
  let root = TempRoot::new("store_drop_archive_branch");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  let archived = store.create_archive(&tbl, "v1", 0).await.unwrap();
  assert_eq!(archived.name, "manifest_main_0_v1");
  let branch = store.create_branch(&tbl, "feature").await.unwrap();
  assert_eq!(branch.name, "manifest_feature_0");

  store.drop_table(&tbl).await.unwrap();
  assert!(!root.join("demo.1").join("manifest_main_0").exists());
  assert!(root.join("demo.1").join("manifest_feature_0").exists());
  assert!(root.join("demo.1").join("manifest_main_0_v1").exists());

  // GC 不能删 archive / branch 仍引用的 data 文件。
  store
    .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .await
    .unwrap();
  store.reopen_archive(&tbl, "v1").await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

  // 回到 branch 视图仍可读（archive reopen 只改活跃状态，不删 branch manifest）。
  store.prepare_attach("feature", 0).unwrap();
  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
}

/// 在 branch 身份下 Drop 只删 branch manifest，main 的数据与 manifest 保持可读。
#[compio::test]
async fn drop_branch_identity_does_not_touch_main_manifest() {
  let root = TempRoot::new("store_drop_branch_identity");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  store.create_branch(&tbl, "feature").await.unwrap();

  store.prepare_attach("feature", 1).unwrap();
  store.reopen(&tbl).await.unwrap();
  store.drop_table(&tbl).await.unwrap();
  assert!(!root.join("demo.1").join("manifest_feature_0").exists());
  assert!(root.join("demo.1").join("manifest_main_0").exists());

  store.prepare_attach("main", 0).unwrap();
  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
}

/// 收集 mapping 当前引用的 data 文件集合（`pages_per_file_shift` 编码进 file id）。
fn live_data_file_li(store: &Store<MemStore>, tbl: &TableIdent, shift: u8) -> Vec<u64> {
  let mut file_li: Vec<u64> = store
    .mapper(tbl)
    .unwrap()
    .tbl()
    .iter_value_li()
    .filter(|value| is_file_page_id(*value))
    .map(|value| decode_id(value) >> shift)
    .collect();
  file_li.sort_unstable();
  file_li.dedup();
  file_li
}

/// 收集 segment mapping 当前引用的 segment 文件集合（槽位 id 编码在
/// file page id 内，文件号 = 槽位 id >> `segments_per_file_shift`）。
fn live_segment_file_li(store: &Store<MemStore>, tbl: &TableIdent, shift: u8) -> Vec<u64> {
  let mut file_li: Vec<u64> = store
    .segment_mapper(tbl)
    .unwrap()
    .tbl()
    .iter_value_li()
    .filter(|value| is_file_page_id(*value))
    .map(|value| decode_id(value) >> shift)
    .collect();
  file_li.sort_unstable();
  file_li.dedup();
  file_li
}

/// segment mapping 的活跃段物理 id（排序后逐项比较，不依赖迭代顺序）。
fn live_segment_value_li(store: &Store<MemStore>, tbl: &TableIdent) -> Vec<u64> {
  let mut value_li: Vec<u64> = store
    .segment_mapper(tbl)
    .unwrap()
    .tbl()
    .iter_value_li()
    .filter(|value| is_file_page_id(*value))
    .collect();
  value_li.sort_unstable();
  value_li
}

fn snapshot_segment_value_li(state: &ReplayState) -> Vec<u64> {
  let mut value_li: Vec<u64> = state
    .segment_mapping
    .iter_value_li()
    .filter(|value| is_file_page_id(*value))
    .collect();
  value_li.sort_unstable();
  value_li
}

fn large_value_pattern(len: usize, seed: u8) -> Vec<u8> {
  (0..len)
    .map(|index| (index as u8).wrapping_add(seed))
    .collect()
}

/// 多轮覆盖写制造碎片：每轮 60 个 key 覆盖上一轮，旧页全死且无写路径
/// maintain（store 直连，不走 shard 的写后维护），磁盘留下大量死文件。
async fn fragment_store(store: &mut Store<MemStore>, tbl: &TableIdent) {
  for round in 1..=4u32 {
    let batch: Vec<BatchEntry> = (0..60u32)
      .map(|index| {
        BatchEntry::upsert(
          format!("k{index:03}").into_bytes(),
          vec![b'v'; 30],
          u64::from(round),
        )
      })
      .collect();
    store.put(tbl, &batch).await.unwrap();
  }
}

/// sem-03：`create_archive` 落 snapshot 前按放大率先 compact + GC（对齐 C++
/// `CreateArchive` 的 needs_compact 分支）。碎片化后不再写入（写路径 maintain
/// 不会先行的场景），归档自身必须把 live page 搬到文件尾、回收搬空的死文件：
/// 归档继承稠密 mapping，且归档 manifest 不引用死文件里的页。
#[test]
fn archive_compacts_before_snapshot() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 13).unwrap();
    let mut store = Store::new(MemStore::with_pages_per_file(128, 1), 128, 8)
      .with_pages_per_file_shift(1)
      .with_file_amplify_factor(1);
    fragment_store(&mut store, &tbl).await;
    let file_before = store.page_io().list_file_id_li(&tbl).await.unwrap().len();
    assert!(
      file_before > live_data_file_li(&store, &tbl, 1).len(),
      "前置：碎片化 workload 应留下死文件 files={file_before}"
    );

    let info = store.create_archive(&tbl, "001", 0).await.unwrap();
    assert!(info.bytes > 0);

    let live_file_li = live_data_file_li(&store, &tbl, 1);
    let file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
    assert!(
      file_li.len() < file_before,
      "pre-compact must drop garbage files: before={file_before} after={}",
      file_li.len()
    );
    assert!(
      file_li.iter().all(|file_id| live_file_li.contains(file_id)),
      "no dead file may survive pre-archive compact + gc: files={file_li:?} live={live_file_li:?}"
    );

    // 归档 manifest 自身的 mapping 也不引用死文件：reopen_archive 把归档
    // 快照装回内存，用它的 mapping 再校验一次磁盘文件全被引用。
    store.reopen_archive(&tbl, "001").await.unwrap();
    let archived_live_file_li = live_data_file_li(&store, &tbl, 1);
    assert!(
      file_li
        .iter()
        .all(|file_id| archived_live_file_li.contains(file_id)),
      "archive manifest must not reference dead files: files={file_li:?} live={archived_live_file_li:?}"
    );
    for index in 0..60u32 {
      let key = format!("k{index:03}");
      assert_eq!(
        store.get(&tbl, key.as_bytes()).await.unwrap(),
        Some(vec![b'v'; 30])
      );
    }
  });
}

/// sem-03 对照组：`file_amplify_factor = 0`（谓词恒 false，对齐 C++
/// `MapperExceedsAmplification` 对 amp_factor=0 返回 false）时归档前不
/// compact、不 GC——与上一条 factor>0 的断言互为镜像，锁死「归档前维护由
/// 放大率谓词触发」这一因果（区别于 branch 的无条件 pre-compact）。
#[test]
fn archive_without_amplify_factor_keeps_fragmented_files() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 14).unwrap();
    let mut store = Store::new(MemStore::with_pages_per_file(128, 1), 128, 8)
      .with_pages_per_file_shift(1)
      .with_file_amplify_factor(0);
    fragment_store(&mut store, &tbl).await;
    let file_before = store.page_io().list_file_id_li(&tbl).await.unwrap();
    assert!(
      file_before.len() > live_data_file_li(&store, &tbl, 1).len(),
      "前置：碎片化 workload 应留下死文件 files={}",
      file_before.len()
    );

    store.create_archive(&tbl, "001", 0).await.unwrap();

    // factor=0：磁盘文件集合原样保留（不 compact、不回收死文件）。
    let file_after = store.page_io().list_file_id_li(&tbl).await.unwrap();
    assert_eq!(
      file_after, file_before,
      "factor=0 时 create_archive 不得 compact / 回收文件"
    );
    // 不预压缩只影响文件集合，不影响正确性：归档数据仍全量可读。
    store.reopen_archive(&tbl, "001").await.unwrap();
    for index in 0..60u32 {
      let key = format!("k{index:03}");
      assert_eq!(
        store.get(&tbl, key.as_bytes()).await.unwrap(),
        Some(vec![b'v'; 30])
      );
    }
  });
}

/// sem-03 的 segment 侧镜像：大 value 多轮覆盖留下死段文件，data 侧 factor=0
/// 隔离后，`create_archive` 必须由 `needs_segment_maintain` 触发
/// `compact_segment_if_needed` + `file_gc`：死段文件被回收、归档 snapshot 的
/// segment mapping 指向搬迁后的段且全部存在、数据全量可读。
#[test]
fn archive_segment_amplify_compacts_and_gcs_segments() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 15).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let segment_shift = layout.segments_per_file_shift();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024)
      .with_file_amplify_factor(0)
      .with_segment_amplify_factor(1);
    // 常驻 key 的段留在 file 0，压缩才有可搬的 live 段。
    let keep = large_value_pattern(9000, 9);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"keep".to_vec(), keep.clone(), 1)],
      )
      .await
      .unwrap();
    let mut last = Vec::new();
    for round in 0..12u32 {
      last = large_value_pattern(9000 + round as usize, round as u8);
      store
        .put_large(
          &tbl,
          &[BatchEntry::upsert(
            b"big".to_vec(),
            last.clone(),
            u64::from(round) + 1,
          )],
        )
        .await
        .unwrap();
    }
    // 前置：只有 segment 侧超阈值（data factor=0 隔离），盘上确有死段文件。
    assert!(
      store.needs_segment_maintain(&tbl),
      "前置：多轮覆盖应让 segment 放大率超阈值"
    );
    assert!(
      !store.needs_maintain(&tbl),
      "前置：data 侧必须关闭（factor=0），否则测不到 segment 分支"
    );
    let live_file_before = live_segment_file_li(&store, &tbl, segment_shift);
    let file_before = store.page_io().list_segment_file_id_li(&tbl).await.unwrap();
    let dead_file_li: Vec<u64> = file_before
      .iter()
      .copied()
      .filter(|file_id| !live_file_before.contains(file_id))
      .collect();
    assert!(
      !dead_file_li.is_empty() && file_before.len() > live_file_before.len(),
      "前置：多轮覆盖应留下死段文件 files={file_before:?} live={live_file_before:?}"
    );
    let value_before = live_segment_value_li(&store, &tbl);

    let info = store.create_archive(&tbl, "001", 0).await.unwrap();
    assert!(info.bytes > 0);

    // 死段文件被回收：磁盘集合恰好等于 live mapping 引用的文件集合。
    let file_after = store.page_io().list_segment_file_id_li(&tbl).await.unwrap();
    assert!(
      file_after.len() < file_before.len(),
      "pre-compact must drop garbage segment files: before={file_before:?} after={file_after:?}"
    );
    for dead_file in &dead_file_li {
      assert!(
        !file_after.contains(dead_file),
        "死段文件 {dead_file} 未被 GC 回收: {file_after:?}"
      );
    }
    assert_eq!(
      file_after,
      live_segment_file_li(&store, &tbl, segment_shift),
      "归档后 segment 文件集合必须与活跃 mapping 引用一致"
    );
    // live 段被搬到文件尾：mapping 值变化，排除「只 GC 不搬」。
    assert_ne!(
      live_segment_value_li(&store, &tbl),
      value_before,
      "pre-compact 必须把 live 段搬到文件尾"
    );

    // 归档 snapshot 的 segment mapping 必须是预压缩后的映射，且引用的段都还在。
    let archive_bytes = store
      .page_io()
      .read_manifest_file(&tbl, &info.name)
      .await
      .unwrap()
      .unwrap();
    let archive_state = ReplayState::from_snapshot(&archive_bytes).unwrap();
    let snapshot_value_li = snapshot_segment_value_li(&archive_state);
    assert_eq!(
      snapshot_value_li,
      live_segment_value_li(&store, &tbl),
      "archive snapshot 必须继承预压缩后的 segment mapping"
    );
    assert_eq!(
      archive_state.max_segment_fp_id,
      store.segment_mapper(&tbl).unwrap().max_fp_id()
    );
    let mut snapshot_file_li: Vec<u64> = snapshot_value_li
      .iter()
      .map(|value| decode_id(*value) >> segment_shift)
      .collect();
    snapshot_file_li.sort_unstable();
    snapshot_file_li.dedup();
    assert_eq!(
      snapshot_file_li, file_after,
      "归档 snapshot 引用的 segment 文件必须全部存在且无死文件"
    );

    // 数据全量可读（活跃视图）。
    assert_eq!(
      store.get_large(&tbl, b"keep").await.unwrap().unwrap().value,
      keep
    );
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      last
    );

    // 归档快照独立装回：mapping 仍只引用现存段，数据完整。
    store.reopen_archive(&tbl, "001").await.unwrap();
    assert_eq!(
      live_segment_value_li(&store, &tbl),
      snapshot_value_li,
      "reopen_archive 必须以归档 snapshot 的 segment mapping 为准"
    );
    assert_eq!(
      store.page_io().list_segment_file_id_li(&tbl).await.unwrap(),
      file_after,
      "reopen_archive 不应改动（或指向已删的）segment 文件"
    );
    assert_eq!(
      store.get_large(&tbl, b"keep").await.unwrap().unwrap().value,
      keep
    );
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      last
    );
  });
}

/// C++ `create archive does not compact below threshold`
/// （`tests/segment_compact.cpp:613-663`）的镜像：干净大 value 批次
/// factor 1 < threshold 2，归档不得尾重写 segment、不得推进 `max_fp_id`，
/// segment 文件集合与 snapshot 的 segment mapping 逐项不变，数据可读。
#[test]
fn archive_below_segment_threshold_pins_segment_files() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 16).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024)
      .with_file_amplify_factor(0)
      .with_segment_amplify_factor(2);
    // 每个 value 恰好一个 segment：3 个 live 段 / 3 个分配槽 = factor 1 < 2。
    let value_li = [
      large_value_pattern(4096, 1),
      large_value_pattern(4096, 2),
      large_value_pattern(4096, 3),
    ];
    store
      .put_large(
        &tbl,
        &[
          BatchEntry::upsert(b"k1".to_vec(), value_li[0].clone(), 1),
          BatchEntry::upsert(b"k2".to_vec(), value_li[1].clone(), 1),
          BatchEntry::upsert(b"k3".to_vec(), value_li[2].clone(), 1),
        ],
      )
      .await
      .unwrap();
    assert_eq!(
      store.segment_mapper(&tbl).unwrap().mapping_count(),
      3,
      "前置：3 个大 value 应各占一个 live segment"
    );
    assert!(
      !store.needs_segment_maintain(&tbl),
      "前置：干净批次的 segment 放大率必须低于阈值"
    );
    let file_before = store.page_io().list_segment_file_id_li(&tbl).await.unwrap();
    let max_before = store.segment_mapper(&tbl).unwrap().max_fp_id();
    let value_before = live_segment_value_li(&store, &tbl);
    assert_eq!(
      file_before.len(),
      1,
      "3 段应落在同一个 segment 文件: {file_before:?}"
    );
    assert_eq!(max_before, 3);

    let info = store.create_archive(&tbl, "001", 0).await.unwrap();
    assert!(info.bytes > 0);

    // 对齐 C++ `after.max_id == before.max_id` / `after.count == before.count`。
    assert_eq!(
      store.page_io().list_segment_file_id_li(&tbl).await.unwrap(),
      file_before,
      "低于阈值时 create_archive 不得改动 segment 文件集合"
    );
    assert_eq!(
      store.segment_mapper(&tbl).unwrap().max_fp_id(),
      max_before,
      "低于阈值时 create_archive 不得推进 segment max_fp_id"
    );
    assert_eq!(live_segment_value_li(&store, &tbl), value_before);

    // 归档 snapshot 钉住同一批 segment（max / mapping 均不变）。
    let archive_bytes = store
      .page_io()
      .read_manifest_file(&tbl, &info.name)
      .await
      .unwrap()
      .unwrap();
    let archive_state = ReplayState::from_snapshot(&archive_bytes).unwrap();
    assert_eq!(archive_state.max_segment_fp_id, max_before);
    assert_eq!(snapshot_segment_value_li(&archive_state), value_before);

    for (key, value) in [
      (b"k1".as_slice(), &value_li[0]),
      (b"k2".as_slice(), &value_li[1]),
      (b"k3".as_slice(), &value_li[2]),
    ] {
      assert_eq!(
        store.get_large(&tbl, key).await.unwrap().unwrap().value,
        *value
      );
    }

    // 归档快照装回后 segment 文件 / 高水位仍与归档前逐项一致。
    store.reopen_archive(&tbl, "001").await.unwrap();
    assert_eq!(
      store.page_io().list_segment_file_id_li(&tbl).await.unwrap(),
      file_before
    );
    assert_eq!(store.segment_mapper(&tbl).unwrap().max_fp_id(), max_before);
    for (key, value) in [
      (b"k1".as_slice(), &value_li[0]),
      (b"k2".as_slice(), &value_li[1]),
      (b"k3".as_slice(), &value_li[2]),
    ] {
      assert_eq!(
        store.get_large(&tbl, key).await.unwrap().unwrap().value,
        *value
      );
    }
  });
}

/// truncate 全删 + `maintain`（file_gc + cleanup_empty_partition）后，归档必须
/// 完整幸存：归档 manifest、归档快照引用的 data 文件、分区目录都不许删；
/// 随后 `reopen_archive(tag)` 能把快照装回并读到归档前全部 entry，归档后的
/// 后继写入不可见。
///
/// 对齐 C++ `tests/gc.cpp` 的 "archive prevents data deletion after truncate"。
/// 与两个半边用例互补：`truncate_drop.rs::
/// compio_truncate_then_maintain_removes_partition_dir` 锁无归档时的收尾，
/// [`drop_keeps_archive_and_other_branch_readable`] 锁 Drop 路径。本用例锁
/// 「truncate + maintain + archive」组合——cleanup 若先清目录、或 truncate
/// 开始剪枝 archive manifest，这里必须变红。
#[compio::test]
async fn archive_survives_truncate_and_maintain() {
  let root = TempRoot::new("store_archive_truncate_maintain");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store
    .put(
      &tbl,
      &[
        BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
        BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
      ],
    )
    .await
    .unwrap();
  let part_dir = root.join("demo.1");
  let active_manifest = part_dir.join("manifest_main_0");
  assert!(active_manifest.exists());

  let archived = store.create_archive(&tbl, "v1", 0).await.unwrap();
  assert_eq!(archived.name, "manifest_main_0_v1");
  let archive_path = part_dir.join(&archived.name);
  assert!(archive_path.exists());
  let archived_file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
  assert!(
    !archived_file_li.is_empty(),
    "归档快照应引用至少一个 data 文件"
  );
  assert_eq!(
    store.list_archive_tags(&tbl).await.unwrap(),
    vec![ArchiveTagInfo {
      tag: "v1".to_owned(),
      term: 0,
    }]
  );

  // 归档后继续写：活跃状态领先 v1，这些 entry 不在归档快照里。
  store
    .put(
      &tbl,
      &[
        BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 2),
        BatchEntry::upsert(b"d".to_vec(), b"D".to_vec(), 2),
      ],
    )
    .await
    .unwrap();
  assert_eq!(store.get(&tbl, b"c").await.unwrap(), Some(b"C".to_vec()));

  store.truncate(&tbl).await.unwrap();
  assert_eq!(store.root_id(&tbl), Some(PageId::MAX));
  // truncate 只写空 snapshot：归档 manifest 与归档引用的 data 文件都不许动。
  assert!(part_dir.exists());
  assert!(active_manifest.exists());
  assert!(archive_path.exists());
  assert_eq!(
    store.page_io().list_file_id_li(&tbl).await.unwrap(),
    archived_file_li,
    "truncate 不得回收归档引用的 data 文件"
  );

  // maintain = compact + file_gc + cleanup_empty_partition。归档仍引用 data，
  // 逻辑空表判定不成立：目录 / 活跃 manifest / 归档 manifest / 归档数据文件
  // 全部幸存。
  store.maintain(&tbl).await.unwrap();
  assert!(
    part_dir.exists(),
    "存在 archive 时 maintain 不得移除分区目录"
  );
  assert!(
    active_manifest.exists(),
    "存在 archive 时不得按逻辑空表删除活跃 manifest"
  );
  assert!(archive_path.exists(), "maintain 不得剪掉 archive manifest");
  let file_li_after_first = store.page_io().list_file_id_li(&tbl).await.unwrap();
  for file_id in &archived_file_li {
    assert!(
      file_li_after_first.contains(file_id),
      "归档引用的 data 文件 {file_id} 被 file_gc 回收（retained set 漏了 archive manifest）"
    );
  }
  assert_eq!(
    store.list_archive_tags(&tbl).await.unwrap(),
    vec![ArchiveTagInfo {
      tag: "v1".to_owned(),
      term: 0,
    }]
  );

  // 重复 maintain 幂等：data 文件集合与 manifest 集合不变，归档仍幸存。
  let mut manifest_li = store
    .page_io()
    .list_manifest_file_name_li(&tbl)
    .await
    .unwrap();
  manifest_li.sort();
  store.maintain(&tbl).await.unwrap();
  assert!(part_dir.exists());
  assert!(archive_path.exists());
  assert_eq!(
    store.page_io().list_file_id_li(&tbl).await.unwrap(),
    file_li_after_first,
    "重复 maintain 不应再改动文件集合"
  );
  let mut manifest_li_after = store
    .page_io()
    .list_manifest_file_name_li(&tbl)
    .await
    .unwrap();
  manifest_li_after.sort();
  assert_eq!(
    manifest_li_after, manifest_li,
    "重复 maintain 不应再改动 manifest 集合"
  );

  // 冷启动按 tag 回滚：归档前 entry 全部读回原值；truncate 前的后继写入不可见。
  drop(store);
  let mut reopened = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  let info = reopened.reopen_archive(&tbl, "v1").await.unwrap();
  assert_eq!(info.name, "manifest_main_0_v1");
  assert_eq!(reopened.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(reopened.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
  assert_eq!(reopened.get(&tbl, b"c").await.unwrap(), None);
  assert_eq!(reopened.get(&tbl, b"d").await.unwrap(), None);
  assert_eq!(
    reopened.scan(&tbl, b"", b"", 0).await.unwrap().len(),
    2,
    "归档快照只包含归档前的 entry"
  );
}
