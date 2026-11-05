use fkv_btree::BatchEntry;
use fkv_codec::{get_fixed32, get_varint32, get_varint64};
use fkv_error::KvError;
use fkv_io::ManifestIo;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_recovery::ReplayState;
use fkv_store::{ArchiveTagInfo, BRANCH_FILE_RESERVE_FILE_NUM, Store};
use fkv_testkit::TempRoot;
use fkv_types::{PageId, TableIdent};

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
