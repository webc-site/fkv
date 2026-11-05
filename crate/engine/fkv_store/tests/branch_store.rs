use std::fs::{metadata, read, write};

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io::{FileIo, ManifestIo};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::{TableIdent, decode_id, is_file_page_id};

fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

#[test]
fn branch_store_reads_parent_and_writes_independently() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut main = Store::new(MemStore::new(512), 512, 16);
    main
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"main-a".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"main-b".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    main.create_branch(&tbl, "feature").await.unwrap();
    let io = main.into_io();

    let mut branch = Store::new(io, 512, 16);
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(
      branch.get(&tbl, b"a").await.unwrap(),
      Some(b"main-a".to_vec())
    );
    assert_eq!(
      branch.get(&tbl, b"b").await.unwrap(),
      Some(b"main-b".to_vec())
    );
    branch
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"branch-a".to_vec(), 2),
          BatchEntry::upsert(b"c".to_vec(), b"branch-c".to_vec(), 2),
        ],
      )
      .await
      .unwrap();
    assert_eq!(
      branch.get(&tbl, b"a").await.unwrap(),
      Some(b"branch-a".to_vec())
    );
    assert_eq!(
      branch.get(&tbl, b"c").await.unwrap(),
      Some(b"branch-c".to_vec())
    );

    // 重启后 branch 写仍然存在。
    let io = branch.into_io();
    let mut branch = Store::new(io, 512, 16);
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(
      branch.get(&tbl, b"a").await.unwrap(),
      Some(b"branch-a".to_vec())
    );
    assert_eq!(
      branch.get(&tbl, b"c").await.unwrap(),
      Some(b"branch-c".to_vec())
    );

    // main 分支保持原值，看不到 branch 的新 key。
    let io = branch.into_io();
    let mut main = Store::new(io, 512, 16);
    main.reopen(&tbl).await.unwrap();
    assert_eq!(
      main.get(&tbl, b"a").await.unwrap(),
      Some(b"main-a".to_vec())
    );
    assert_eq!(
      main.get(&tbl, b"b").await.unwrap(),
      Some(b"main-b".to_vec())
    );
    assert_eq!(main.get(&tbl, b"c").await.unwrap(), None);
  });
}

#[compio::test]
async fn compio_branch_store_with_large_values_and_missing_branch() {
  let root = TempRoot::new("store_branch_store_compio");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let mut main = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  let big = pattern(9000);
  main
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  main
    .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 1)])
    .await
    .unwrap();
  main.create_branch(&tbl, "feature").await.unwrap();
  drop(main);

  let mut branch = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  branch.open_branch(&tbl, "feature", 0).await.unwrap();
  assert_eq!(branch.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(
    branch.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big
  );
  let branch_big = pattern(5000);
  branch
    .put_large(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), branch_big.clone(), 2)],
    )
    .await
    .unwrap();
  assert_eq!(
    branch.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    branch_big
  );
  drop(branch);

  let mut branch = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  branch.open_branch(&tbl, "feature", 0).await.unwrap();
  assert_eq!(
    branch.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    branch_big
  );
  drop(branch);

  let mut main = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  main.reopen(&tbl).await.unwrap();
  assert_eq!(
    main.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big
  );

  assert_eq!(
    main.open_branch(&tbl, "missing", 0).await.unwrap_err(),
    KvError::ResourceMissing
  );
  assert_eq!(
    main.open_branch(&tbl, "bad name", 0).await.unwrap_err(),
    KvError::InvalidArgs
  );
}

/// sto-51a：branch manifest 尾部垃圾在 open_branch 时截断，
/// 后续写从有效边界追加，再次 open_branch 不再 Corrupted / 丢写。
#[compio::test]
async fn compio_branch_reopen_truncates_corrupted_tail() {
  let root = TempRoot::new("store_branch_store_tail");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut main = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  main
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  main.create_branch(&tbl, "feature").await.unwrap();
  drop(main);

  let manifest = root.join("demo.1").join("manifest_feature_0");
  let mut bytes = read(&manifest).unwrap();
  let valid_len = bytes.len();
  bytes.extend_from_slice(&[0xff; 64]);
  write(&manifest, &bytes).unwrap();

  let mut branch = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  branch.open_branch(&tbl, "feature", 0).await.unwrap();
  assert_eq!(metadata(&manifest).unwrap().len() as usize, valid_len);
  branch
    .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)])
    .await
    .unwrap();
  drop(branch);

  let mut branch = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  branch.open_branch(&tbl, "feature", 0).await.unwrap();
  assert_eq!(branch.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(branch.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
  drop(branch);
}

/// issue 23：branch 每次 put 必须追加 log record，而不是整份 snapshot 重写。
#[test]
fn branch_puts_append_log_not_full_snapshot() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 9).unwrap();
    let mut main = Store::new(MemStore::new(512), 512, 16);
    let batch: Vec<BatchEntry> = (0..20u32)
      .map(|index| BatchEntry::upsert(format!("k{index:02}").into_bytes(), b"base".to_vec(), 1))
      .collect();
    main.put(&tbl, &batch).await.unwrap();
    main.create_branch(&tbl, "feature").await.unwrap();
    let io = main.into_io();

    let mut branch = Store::new(io, 512, 16);
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    let append_before = branch.page_io().manifest_append_num();
    let switch_before = branch.page_io().manifest_switch_num();
    for index in 0..5u32 {
      branch
        .put(
          &tbl,
          &[BatchEntry::upsert(
            format!("n{index}").into_bytes(),
            b"value".to_vec(),
            2,
          )],
        )
        .await
        .unwrap();
    }
    assert!(
      branch.page_io().manifest_append_num() >= append_before + 5,
      "branch puts must append log records"
    );
    assert_eq!(
      branch.page_io().manifest_switch_num(),
      switch_before,
      "branch puts must not rewrite full snapshot"
    );
    for index in 0..5u32 {
      let key = format!("n{index}");
      assert_eq!(
        branch.get(&tbl, key.as_bytes()).await.unwrap(),
        Some(b"value".to_vec())
      );
    }
  });
}

/// eng-60：未打开 / 空分区不创建 branch manifest（对齐 C++ `CreateBranch` 的
/// `NotFound` / mapper 缺失分支）。
#[test]
fn create_branch_skips_unopened_partition() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 7).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let info = store.create_branch(&tbl, "feature").await.unwrap();
    assert_eq!(info.name, "");
    assert_eq!(info.bytes, 0);
    assert!(
      store
        .page_io()
        .list_manifest_file_name_li(&tbl)
        .await
        .unwrap()
        .is_empty(),
      "empty partition must not write a branch manifest"
    );
  });
}

/// eng-60：`create_branch` 先 pre-compact，branch 继承稠密 mapping；搬空的旧文件
/// 在 GC 后删除，磁盘文件集合等于父 mapping 实际引用的文件集合。
#[test]
fn create_branch_precompacts_fragmented_parent() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 8).unwrap();
    let mut store = Store::new(MemStore::with_pages_per_file(128, 1), 128, 8)
      .with_pages_per_file_shift(1)
      .with_file_amplify_factor(1);
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
      store.put(&tbl, &batch).await.unwrap();
    }
    let file_before = store.page_io().list_file_id_li(&tbl).await.unwrap().len();
    let info = store.create_branch(&tbl, "feature").await.unwrap();
    assert!(info.bytes > 0);
    let live_file_li: Vec<u64> = store
      .mapper(&tbl)
      .unwrap()
      .tbl()
      .iter_value_li()
      .filter(|value| is_file_page_id(*value))
      .map(|value| decode_id(value) >> 1)
      .collect();
    let file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
    assert!(
      file_li.len() < file_before,
      "pre-compact must drop garbage files: before={file_before} after={}",
      file_li.len()
    );
    assert!(
      file_li.iter().all(|file_id| live_file_li.contains(file_id)),
      "no dead file may survive pre-compact + gc: files={file_li:?} live={live_file_li:?}"
    );

    let io = store.into_io();
    let mut branch = Store::new(io, 128, 8).with_pages_per_file_shift(1);
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    for index in 0..60u32 {
      let key = format!("k{index:03}");
      assert_eq!(
        branch.get(&tbl, key.as_bytes()).await.unwrap(),
        Some(vec![b'v'; 30])
      );
    }
  });
}

/// r9b-02 / eng-60b 遗留缺口：create_branch 的 segment 预压缩 exact 路径直测。
///
/// 「fork 点落在 segment 中部」场景：父表多轮覆盖大 value 制造碎片 segment
/// 文件（含全死文件），`segment_file_amplify_factor != 0` 时
/// `compact_before_branch` 的 segment 臂必须把非尾文件的 live 段搬到文件尾，
/// 搬空的死文件由末尾 file_gc 回收；随后断言 fork 预留区间
/// （`branch_segment_limit`）不与搬移结果重叠、branch 侧大 value 全量可读。
#[test]
fn create_branch_precompacts_fragmented_segment_parent() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 11).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let shift = layout.segments_per_file_shift();
    let mut main = Store::new(MemStore::new(512).with_segment_config(4096, shift), 512, 16)
      .with_large_value(layout, 1024)
      .with_segment_amplify_factor(1);
    // 常驻 key 的段落在低号文件（fork 点之前），预压缩才有可搬的 live 段。
    let keep = pattern(9000);
    main
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"keep".to_vec(), keep.clone(), 1)],
      )
      .await
      .unwrap();
    // 多轮覆盖 big：每轮旧段全灭、新段追加到文件尾，制造碎片与全死文件。
    let mut big = pattern(9000);
    main
      .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 1)])
      .await
      .unwrap();
    for round in 0..12u32 {
      big = pattern(9000 + round as usize);
      main
        .put_large(
          &tbl,
          &[BatchEntry::upsert(
            b"big".to_vec(),
            big.clone(),
            u64::from(round) + 2,
          )],
        )
        .await
        .unwrap();
    }

    // 碎片前置条件：磁盘 segment 文件数多于 live 文件数，存在全死文件，
    // 且存在 live 段落在 fork 尾文件之前的低号文件。
    let pre_tail_file = main.segment_mapper(&tbl).unwrap().max_fp_id() >> shift;
    let live_file_before = live_segment_file_li(main.segment_mapper(&tbl).unwrap(), shift);
    let disk_before = main.page_io().list_segment_file_id_li(&tbl).await.unwrap();
    assert!(
      disk_before.len() > live_file_before.len(),
      "前置：应有全死 segment 文件 disk={disk_before:?} live={live_file_before:?}"
    );
    assert!(
      disk_before
        .iter()
        .any(|file_id| !live_file_before.contains(file_id)),
      "前置：至少一个纯死文件 disk={disk_before:?} live={live_file_before:?}"
    );
    assert!(
      live_file_before
        .iter()
        .any(|file_id| *file_id < pre_tail_file),
      "前置：live 段应散布在尾文件之前 live={live_file_before:?} tail={pre_tail_file}"
    );

    let info = main.create_branch(&tbl, "feature").await.unwrap();
    assert!(info.bytes > 0);

    // fork 预留边界：占用远不足 reserve 个文件，branch 的 segment 起点必然
    // 落在第一个预留块 [reserve, 2*reserve)，父分配器跳到 2*reserve 之后。
    let reserve = fkv_store::BRANCH_FILE_RESERVE_FILE_NUM;
    assert!(
      pre_tail_file + u64::try_from(live_file_before.len()).unwrap() < reserve,
      "前置：预压缩足迹必须落在第一个预留块内"
    );
    let base_file = reserve;
    let base_fp = base_file << shift;
    let limit_fp = (base_file + reserve) << shift;

    // 预压缩生效：搬移后所有 live 段都在旧尾文件之后、预留块之前。
    let live_file_after = live_segment_file_li(main.segment_mapper(&tbl).unwrap(), shift);
    assert!(
      live_file_after
        .iter()
        .all(|file_id| *file_id >= pre_tail_file),
      "非尾文件的 live 段必须被搬到文件尾 live={live_file_after:?} tail={pre_tail_file}"
    );
    assert!(
      live_file_after.iter().all(|file_id| *file_id < base_file),
      "搬移结果不得伸入 fork 预留区间 live={live_file_after:?} base_file={base_file}"
    );
    // 磁盘文件集合 == live 文件集合：死文件全部被 file_gc 回收。
    let disk_after = main.page_io().list_segment_file_id_li(&tbl).await.unwrap();
    assert!(
      disk_after.len() < disk_before.len(),
      "预压缩 + GC 必须减少 segment 文件数: {disk_before:?} -> {disk_after:?}"
    );
    assert_eq!(
      disk_after, live_file_after,
      "磁盘文件集合必须等于父 mapping 实际引用的 segment 文件集合"
    );
    // 父分配器跳过整个预留区间。
    assert_eq!(
      main.segment_mapper(&tbl).unwrap().max_fp_id(),
      limit_fp,
      "父 segment 分配器必须预留到 branch 区间上界"
    );
    // 搬移后父表读路径完整（keep 的段已被搬迁）。
    assert_eq!(
      main.get_large(&tbl, b"keep").await.unwrap().unwrap().value,
      keep
    );
    assert_eq!(
      main.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      big
    );

    let io = main.into_io();
    let mut branch = Store::new(io, 512, 16).with_large_value(layout, 1024);
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    // branch 从预留块下界起步，上界与父的预留一致：区间不重叠。
    assert_eq!(branch.branch_segment_limit(), Some(limit_fp));
    assert_eq!(
      branch.segment_mapper(&tbl).unwrap().max_fp_id(),
      base_fp,
      "branch segment 分配器必须从预留块下界起步"
    );
    assert_eq!(
      branch
        .get_large(&tbl, b"keep")
        .await
        .unwrap()
        .unwrap()
        .value,
      keep
    );
    assert_eq!(
      branch.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      big
    );

    // branch 写新大 value：段落进自己的预留块首文件，不碰父的搬移文件。
    let extra = pattern(9000);
    branch
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"extra".to_vec(), extra.clone(), 2)],
      )
      .await
      .unwrap();
    let branch_max = branch.segment_mapper(&tbl).unwrap().max_fp_id();
    assert!(
      branch_max >= base_fp && branch_max < limit_fp,
      "branch 新段必须落在自己的预留区间内: {branch_max}"
    );
    let branch_disk = branch
      .page_io()
      .list_segment_file_id_li(&tbl)
      .await
      .unwrap();
    assert!(
      branch_disk.contains(&base_file),
      "branch 新段应落在预留块首文件: {branch_disk:?}"
    );
    assert_eq!(
      branch
        .get_large(&tbl, b"extra")
        .await
        .unwrap()
        .unwrap()
        .value,
      extra
    );

    // 重启后 branch 的 segment 映射与数据一致。
    let io = branch.into_io();
    let mut branch = Store::new(io, 512, 16).with_large_value(layout, 1024);
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(
      branch
        .get_large(&tbl, b"keep")
        .await
        .unwrap()
        .unwrap()
        .value,
      keep
    );
    assert_eq!(
      branch.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      big
    );
    assert_eq!(
      branch
        .get_large(&tbl, b"extra")
        .await
        .unwrap()
        .unwrap()
        .value,
      extra
    );
  });
}

/// r9b-02 对照组：`segment_file_amplify_factor = 0`（data 侧同为 0）时
/// `compact_before_branch` 整体短路，同场景下死 segment 文件原样保留、
/// live 段不搬移——与上一条 factor>0 的断言互为镜像，锁死「预压缩由
/// segment 臂触发」这一因果。
#[test]
fn create_branch_without_segment_factor_keeps_fragmented_segment_files() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 12).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let shift = layout.segments_per_file_shift();
    let mut main = Store::new(MemStore::new(512).with_segment_config(4096, shift), 512, 16)
      .with_large_value(layout, 1024);
    let keep = pattern(9000);
    main
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"keep".to_vec(), keep.clone(), 1)],
      )
      .await
      .unwrap();
    let mut big = pattern(9000);
    main
      .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 1)])
      .await
      .unwrap();
    for round in 0..12u32 {
      big = pattern(9000 + round as usize);
      main
        .put_large(
          &tbl,
          &[BatchEntry::upsert(
            b"big".to_vec(),
            big.clone(),
            u64::from(round) + 2,
          )],
        )
        .await
        .unwrap();
    }

    let pre_tail_file = main.segment_mapper(&tbl).unwrap().max_fp_id() >> shift;
    let live_file_before = live_segment_file_li(main.segment_mapper(&tbl).unwrap(), shift);
    let disk_before = main.page_io().list_segment_file_id_li(&tbl).await.unwrap();
    assert!(
      disk_before.len() > live_file_before.len(),
      "前置：应有全死 segment 文件 disk={disk_before:?} live={live_file_before:?}"
    );

    main.create_branch(&tbl, "feature").await.unwrap();

    // factor=0：磁盘文件集合原样保留（不回收死文件），live 段不搬移。
    let disk_after = main.page_io().list_segment_file_id_li(&tbl).await.unwrap();
    assert_eq!(
      disk_after, disk_before,
      "factor=0 时 create_branch 不得回收 / 搬移 segment 文件"
    );
    let live_file_after = live_segment_file_li(main.segment_mapper(&tbl).unwrap(), shift);
    assert_eq!(
      live_file_after, live_file_before,
      "factor=0 时 live 段位置不得变化"
    );
    assert!(
      live_file_after
        .iter()
        .any(|file_id| *file_id < pre_tail_file),
      "factor=0 时低号文件的 live 段保留原位 live={live_file_after:?} tail={pre_tail_file}"
    );

    // 不预压缩只影响文件集合，不影响正确性：branch 数据仍全量可读。
    let io = main.into_io();
    let mut branch = Store::new(io, 512, 16).with_large_value(layout, 1024);
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(
      branch
        .get_large(&tbl, b"keep")
        .await
        .unwrap()
        .unwrap()
        .value,
      keep
    );
    assert_eq!(
      branch.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      big
    );
  });
}

/// segment mapper 里 live 段所在的文件 id（升序去重）。
fn live_segment_file_li(mapper: &fkv_mapping::PageMapper, shift: u8) -> Vec<u64> {
  let mut file_li: Vec<u64> = mapper
    .tbl()
    .iter_value_li()
    .filter(|value| is_file_page_id(*value))
    .map(|value| decode_id(value) >> shift)
    .collect();
  file_li.sort_unstable();
  file_li.dedup();
  file_li
}

/// branch 没有 segment 段时必须清空旧 segment mapper，不能让大 value 写入
/// 落到上一身份的物理段空间。
#[test]
fn open_branch_clears_stale_segment_mapper() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut main = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16);
    main
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    main.create_branch(&tbl, "feature").await.unwrap();
    let io = main.into_io();

    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut branch = Store::new(io, 512, 16).with_large_value(layout, 1024);
    branch
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), pattern(9000), 2).with_metadata(b"meta")],
      )
      .await
      .unwrap();
    assert!(branch.segment_mapper(&tbl).is_some());

    // branch manifest 没有 segment 段：安装后必须整体替换为 None。
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    assert!(
      branch.segment_mapper(&tbl).is_none(),
      "branch 无 segment 映射时不得残留旧 mapper"
    );
  });
}
