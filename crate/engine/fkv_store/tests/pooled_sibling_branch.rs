//! pooled-08：Pooled 下打开**较早创建的 branch** 时，兄弟分支的预留块不是
//! 本分支的复用下界。
//!
//! `rebuild_reuse_floor` 过去把所有非活跃 branch manifest 都当成「该分支的
//! 整个预留块不可复用」。子分支 A 打开时，**更晚创建**的兄弟 B 的块末尾远高于
//! A 自己的 `branch_file_limit`，下界被抬到 B 块末尾后同步抬高分配水位；随后
//! 每次写都在批次结算点判 `OutOfSpace`，而 `write_batch` 已把越界页 flush 到
//! 父表刚用过的 file page id 上（`MemStore` 的页 key 只有 `(tbl, fp)`），父表
//! 数据被静默覆盖。
//!
//! 用例覆盖：
//! 1. 两兄弟 branch + main 的完整矩阵（父写 → 建 A → 建 B → 打开 A 写 → 回
//!    main 读、A / B 交错打开写读）；
//! 2. 打开较早分支后 `compact_file` / `truncate` + reopen 不得覆盖父表；
//! 3. 每个 branch 的 `OutOfSpace` 边界：写满本块后返回 `OutOfSpace` 且不再
//!    flush 越界页；
//! 4. reopen 后 floor 重建不互相污染；嵌套分支的父 manifest 越过自身块后
//!    也不能被当成孙分支的下界。

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io_mem::MemStore;
use fkv_store::{AllocMode, Store};
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, TableIdent};

const PAGE_SIZE: usize = 512;
const RESTART: u16 = 8;
const MODES: [AllocMode; 2] = [AllocMode::Pooled, AllocMode::Append];

fn new_store(mode: AllocMode) -> Store<MemStore> {
  Store::new(
    MemStore::with_pages_per_file(PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT),
    PAGE_SIZE,
    RESTART,
  )
  .with_pages_per_file_shift(DEFAULT_PAGES_PER_FILE_SHIFT)
  .with_alloc_mode(mode)
}

fn new_store_shift(mode: AllocMode, shift: u8) -> Store<MemStore> {
  Store::new(
    MemStore::with_pages_per_file(PAGE_SIZE, shift),
    PAGE_SIZE,
    RESTART,
  )
  .with_pages_per_file_shift(shift)
  .with_alloc_mode(mode)
}

fn reopen_store_shift(mode: AllocMode, io: MemStore, shift: u8) -> Store<MemStore> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(shift)
    .with_alloc_mode(mode)
}

fn reopen_store(mode: AllocMode, io: MemStore) -> Store<MemStore> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(DEFAULT_PAGES_PER_FILE_SHIFT)
    .with_alloc_mode(mode)
}

fn up(key: &[u8], value: &[u8], ts: u64) -> BatchEntry {
  BatchEntry::upsert(key.to_vec(), value.to_vec(), ts)
}

/// main 写 → create A → create B → main 写：返回共享的 `MemStore`。
async fn setup_two_siblings(mode: AllocMode, tbl: &TableIdent) -> MemStore {
  let mut main = new_store(mode);
  main.put(tbl, &[up(b"a", b"A", 1)]).await.unwrap();
  main.create_branch(tbl, "feature").await.unwrap();
  main.create_branch(tbl, "other").await.unwrap();
  main.put(tbl, &[up(b"m", b"MAIN", 2)]).await.unwrap();
  main.into_io()
}

/// 兄弟分支 + main：打开较早的 A 必须可写，且不污染 B / main。
#[compio::test]
async fn pooled_sibling_branch_stays_writable() {
  for mode in MODES {
    let tbl = TableIdent::new("p08_sibling_write", 1).unwrap();
    let io = setup_two_siblings(mode, &tbl).await;

    let mut feature = reopen_store(mode, io);
    feature.open_branch(&tbl, "feature", 0).await.unwrap();
    let limit = feature.branch_file_limit().expect("branch 必须有上限");
    let mapper = feature.mapper(&tbl).expect("mapper");
    if mode == AllocMode::Pooled {
      // 继承页下界 = 父表创建该分支时的高水位（file 0 -> 1 页文件），
      // 不能是兄弟 B（file 3072+）的块末尾。
      assert_eq!(
        mapper.reuse_floor(),
        1 << DEFAULT_PAGES_PER_FILE_SHIFT,
        "A 的复用下界被兄弟块抬高 mode={mode:?}"
      );
    }
    assert!(
      mapper.reuse_floor() < limit,
      "复用下界越过本分支上限 mode={mode:?} floor={} limit={limit}",
      mapper.reuse_floor()
    );
    assert!(mapper.max_fp_id() <= limit);
    feature.put(&tbl, &[up(b"f1", b"F1", 3)]).await.unwrap();
    feature.put(&tbl, &[up(b"f2", b"F2", 4)]).await.unwrap();
    assert_eq!(
      feature.get(&tbl, b"f2").await.unwrap(),
      Some(b"F2".to_vec())
    );
    assert!(
      feature.mapper(&tbl).unwrap().max_fp_id() <= limit,
      "A 的写越过自身上限 mode={mode:?}"
    );
    let io = feature.into_io();

    // 兄弟 B 同样可写；B 看不到 A 的写（B 的 manifest 早于 A 的写）。
    let mut other = reopen_store(mode, io);
    other.open_branch(&tbl, "other", 0).await.unwrap();
    let other_limit = other.branch_file_limit().expect("branch 必须有上限");
    assert!(other.mapper(&tbl).unwrap().reuse_floor() < other_limit);
    other.put(&tbl, &[up(b"o1", b"O1", 5)]).await.unwrap();
    assert_eq!(other.get(&tbl, b"o1").await.unwrap(), Some(b"O1".to_vec()));
    assert_eq!(other.get(&tbl, b"f2").await.unwrap(), None);
    let io = other.into_io();

    // 回 main：父表原值完好，且看不到分支写。
    let mut main = reopen_store(mode, io);
    main.prepare_attach("main", 0).unwrap();
    main.reopen(&tbl).await.unwrap();
    assert_eq!(
      main.get(&tbl, b"m").await.unwrap(),
      Some(b"MAIN".to_vec()),
      "打开兄弟分支后父表数据被覆盖 mode={mode:?}"
    );
    assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(main.get(&tbl, b"f2").await.unwrap(), None);
  }
}

/// A / B 交错打开写读：各自的数据在下次打开自己的分支时仍在，main 不变。
#[compio::test]
async fn pooled_sibling_branches_interleaved_read_write() {
  for mode in MODES {
    let tbl = TableIdent::new("p08_interleave", 1).unwrap();
    let io = setup_two_siblings(mode, &tbl).await;

    let mut feature = reopen_store(mode, io);
    feature.open_branch(&tbl, "feature", 0).await.unwrap();
    feature.put(&tbl, &[up(b"fa", b"FA", 3)]).await.unwrap();
    let io = feature.into_io();

    let mut other = reopen_store(mode, io);
    other.open_branch(&tbl, "other", 0).await.unwrap();
    other.put(&tbl, &[up(b"ob", b"OB", 4)]).await.unwrap();
    assert_eq!(other.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    let io = other.into_io();

    let mut feature = reopen_store(mode, io);
    feature.open_branch(&tbl, "feature", 0).await.unwrap();
    feature.put(&tbl, &[up(b"fc", b"FC", 5)]).await.unwrap();
    assert_eq!(
      feature.get(&tbl, b"fa").await.unwrap(),
      Some(b"FA".to_vec()),
      "A 重开丢失自身写 mode={mode:?}"
    );
    assert_eq!(
      feature.get(&tbl, b"fc").await.unwrap(),
      Some(b"FC".to_vec())
    );
    let io = feature.into_io();

    let mut other = reopen_store(mode, io);
    other.open_branch(&tbl, "other", 0).await.unwrap();
    assert_eq!(
      other.get(&tbl, b"ob").await.unwrap(),
      Some(b"OB".to_vec()),
      "B 重开丢失自身写 mode={mode:?}"
    );
    let io = other.into_io();

    let mut main = reopen_store(mode, io);
    main.prepare_attach("main", 0).unwrap();
    main.reopen(&tbl).await.unwrap();
    assert_eq!(main.get(&tbl, b"m").await.unwrap(), Some(b"MAIN".to_vec()));
    assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  }
}

/// 打开较早分支后 compact：搬移页只能分配在本块内，父表原值仍可读。
#[compio::test]
async fn pooled_sibling_branch_compact_keeps_parent() {
  for mode in MODES {
    let tbl = TableIdent::new("p08_compact", 1).unwrap();
    let io = setup_two_siblings(mode, &tbl).await;

    let mut feature = reopen_store(mode, io);
    feature.open_branch(&tbl, "feature", 0).await.unwrap();
    let moved = feature
      .compact_file(&tbl, 0, DEFAULT_PAGES_PER_FILE_SHIFT)
      .await
      .unwrap();
    assert!(moved > 0, "compaction 应搬移继承页 mode={mode:?}");
    assert!(
      feature.mapper(&tbl).unwrap().max_fp_id() <= feature.branch_file_limit().expect("limit"),
      "compact 把分配推过 branch 上限 mode={mode:?}"
    );
    let io = feature.into_io();

    let mut main = reopen_store(mode, io);
    main.prepare_attach("main", 0).unwrap();
    main.reopen(&tbl).await.unwrap();
    assert_eq!(
      main.get(&tbl, b"m").await.unwrap(),
      Some(b"MAIN".to_vec()),
      "compact 覆盖了父表数据 mode={mode:?}"
    );
    assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  }
}

/// 打开较早分支 → truncate → 重开该分支写：截断快照的 max 必须在本块内，
/// 重开后仍可写且不覆盖父表。
#[compio::test]
async fn pooled_sibling_branch_truncate_reopen_keeps_parent() {
  for mode in MODES {
    let tbl = TableIdent::new("p08_truncate", 1).unwrap();
    let io = setup_two_siblings(mode, &tbl).await;

    let mut feature = reopen_store(mode, io);
    feature.open_branch(&tbl, "feature", 0).await.unwrap();
    let limit = feature.branch_file_limit().expect("limit");
    feature.truncate(&tbl).await.unwrap();
    assert!(
      feature.mapper(&tbl).unwrap().max_fp_id() <= limit,
      "truncate 持久化了越界高水位 mode={mode:?}"
    );
    let io = feature.into_io();

    let mut feature = reopen_store(mode, io);
    feature.open_branch(&tbl, "feature", 0).await.unwrap();
    assert!(feature.mapper(&tbl).unwrap().reuse_floor() < feature.branch_file_limit().unwrap());
    feature.put(&tbl, &[up(b"f", b"F", 3)]).await.unwrap();
    assert_eq!(feature.get(&tbl, b"f").await.unwrap(), Some(b"F".to_vec()));
    let io = feature.into_io();

    let mut main = reopen_store(mode, io);
    main.prepare_attach("main", 0).unwrap();
    main.reopen(&tbl).await.unwrap();
    assert_eq!(
      main.get(&tbl, b"m").await.unwrap(),
      Some(b"MAIN".to_vec()),
      "truncate / reopen 后的写覆盖了父表 mode={mode:?}"
    );
    assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  }
}

/// 写满 branch 自己的块：先按块内高水位正常写入，到 `max == branch_file_limit`
/// 后下一次 `put` 返回 `OutOfSpace`，且不分配、不覆盖父表。
///
/// `pages_per_file_shift = 0` 把预留块缩到 1024 页，用单页写的块内分配把
/// 高水位推满；`MemStore` 的页 key 不含 identity，任何越界 flush 都会直接
/// 命中父表刚用过的页 id。
#[compio::test]
async fn pooled_branch_limit_failure_does_not_touch_parent_pages() {
  let tbl = TableIdent::new("p08_limit", 1).unwrap();
  let mode = AllocMode::Pooled;
  let mut main = new_store_shift(mode, 0);
  main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
  main.create_branch(&tbl, "feature").await.unwrap();
  main.create_branch(&tbl, "other").await.unwrap();
  main.put(&tbl, &[up(b"m", b"MAIN", 2)]).await.unwrap();
  let io = main.into_io();

  let mut feature = reopen_store_shift(mode, io, 0);
  feature.open_branch(&tbl, "feature", 0).await.unwrap();
  let limit = feature.branch_file_limit().expect("limit");
  assert!(feature.mapper(&tbl).unwrap().max_fp_id() < limit);

  // 单条写：池化 COW 会先复用继承块里的空闲页，直到本块真正写满。
  let mut reached = false;
  for i in 0..6000u32 {
    if feature.mapper(&tbl).unwrap().max_fp_id() >= limit {
      reached = true;
      break;
    }
    let key = format!("k{i:05}").into_bytes();
    match feature
      .put(&tbl, &[BatchEntry::upsert(key, vec![b'V'; 200], 3)])
      .await
    {
      Ok(()) => assert!(feature.mapper(&tbl).unwrap().max_fp_id() <= limit),
      Err(KvError::OutOfSpace) => break,
      Err(err) => panic!("意外的写入错误: {err:?}"),
    }
  }
  assert!(reached, "单条写未能把 branch 写到块末尾（布局变化？）");
  assert_eq!(feature.mapper(&tbl).unwrap().max_fp_id(), limit);

  // 高水位已到上限：必须在 `write_batch` 之前拒绝（不分配、不 flush）。
  let before = feature.mapper(&tbl).unwrap().max_fp_id();
  let err = feature
    .put(&tbl, &[up(b"zz", b"Z", 4)])
    .await
    .expect_err("块已满必须 OutOfSpace");
  assert_eq!(err, KvError::OutOfSpace);
  assert_eq!(
    feature.mapper(&tbl).unwrap().max_fp_id(),
    before,
    "失败批不得再分配越界页"
  );
  let io = feature.into_io();

  let mut main = Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(0)
    .with_alloc_mode(mode);
  main.prepare_attach("main", 0).unwrap();
  main.reopen(&tbl).await.unwrap();
  assert_eq!(
    main.get(&tbl, b"m").await.unwrap(),
    Some(b"MAIN".to_vec()),
    "块满失败批覆盖了父表数据"
  );
  assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
}

/// reopen 后 floor 从 manifest 重建：较早分支的下界仍只由祖先链推导，
/// 不会被兄弟分支污染；重开后写读都正常。
#[compio::test]
async fn pooled_sibling_branch_reopen_floor_is_stable() {
  for mode in MODES {
    let tbl = TableIdent::new("p08_reopen_floor", 1).unwrap();
    let io = setup_two_siblings(mode, &tbl).await;

    let mut feature = reopen_store(mode, io);
    feature.open_branch(&tbl, "feature", 0).await.unwrap();
    feature.put(&tbl, &[up(b"f1", b"F1", 3)]).await.unwrap();
    let io = feature.into_io();

    let mut feature = reopen_store(mode, io);
    feature.open_branch(&tbl, "feature", 0).await.unwrap();
    let limit = feature.branch_file_limit().expect("limit");
    if mode == AllocMode::Pooled {
      assert_eq!(
        feature.mapper(&tbl).unwrap().reuse_floor(),
        1 << DEFAULT_PAGES_PER_FILE_SHIFT,
        "reopen 后 floor 被兄弟块抬高 mode={mode:?}"
      );
    }
    feature.put(&tbl, &[up(b"f2", b"F2", 4)]).await.unwrap();
    assert_eq!(
      feature.get(&tbl, b"f1").await.unwrap(),
      Some(b"F1".to_vec())
    );
    assert_eq!(
      feature.get(&tbl, b"f2").await.unwrap(),
      Some(b"F2".to_vec())
    );
    assert!(feature.mapper(&tbl).unwrap().max_fp_id() <= limit);
    let io = feature.into_io();

    let mut main = reopen_store(mode, io);
    main.prepare_attach("main", 0).unwrap();
    main.reopen(&tbl).await.unwrap();
    assert_eq!(main.get(&tbl, b"m").await.unwrap(), Some(b"MAIN".to_vec()));
    assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  }
}

/// 嵌套 branch（sem-24 同族）：孙分支打开时，父 / 祖父分支的 manifest 已把
/// 自身 max 推到孙分支块之后（sem-24 的 reserve），该 manifest 不能当孙分支
/// 的下界；孙分支在自己块内仍可写，父表数据不受影响。
///
/// 父分支 f1 在 `create_branch(f2)` 之后的写仍受 sem-24 影响（`branch_file_limit`
/// 不随 reserve 更新），本条只验证 pooled-08 的 floor 方向，不改变该语义。
#[compio::test]
async fn pooled_nested_branch_floor_stays_within_limit() {
  for mode in MODES {
    let tbl = TableIdent::new("p08_nested", 1).unwrap();
    let mut main = new_store(mode);
    main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
    main.create_branch(&tbl, "f1").await.unwrap();
    let io = main.into_io();

    let mut f1 = reopen_store(mode, io);
    f1.open_branch(&tbl, "f1", 0).await.unwrap();
    f1.put(&tbl, &[up(b"f1a", b"F1A", 2)]).await.unwrap();
    f1.create_branch(&tbl, "f2").await.unwrap();
    let io = f1.into_io();

    // 孙分支：父 manifest 的 max 已被 reserve 推到 f2 块末尾之后，若按
    // `branch_block_limit(parent.max)` 取下界会越过 f2 自己的上限。
    let mut f2 = reopen_store(mode, io);
    f2.open_branch(&tbl, "f2", 0).await.unwrap();
    let f2_limit = f2.branch_file_limit().expect("limit");
    assert!(
      f2.mapper(&tbl).unwrap().reuse_floor() <= f2_limit,
      "孙分支下界越过自身上限 mode={mode:?}"
    );
    assert!(f2.mapper(&tbl).unwrap().max_fp_id() <= f2_limit);
    f2.put(&tbl, &[up(b"f2a", b"F2A", 4)]).await.unwrap();
    assert_eq!(f2.get(&tbl, b"f2a").await.unwrap(), Some(b"F2A".to_vec()));
    let io = f2.into_io();

    let mut main = reopen_store(mode, io);
    main.prepare_attach("main", 0).unwrap();
    main.reopen(&tbl).await.unwrap();
    assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  }
}
