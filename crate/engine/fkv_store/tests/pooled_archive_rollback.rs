//! pooled-11：父 branch 归档回滚不得把复用下界 / 追加水位降到归档时刻。
//!
//! `rebuild_reuse_floor` 的 `has_descendant` 分支原先取**安装后的**
//! `max_fp_id`。普通 reopen 时该水位是盘上最新值（≥ 子分支创建时的父分支
//! 水位 `C`），但 `reopen_archive` 会把归档时刻的水位 `A` 安装为活跃状态：
//! `A < C` 时下界停在 `A`，父分支随后的写从 `A` 起复用空闲页，直接覆盖子 /
//! 孙分支 manifest 仍引用的继承页 `[A, C)`（静默丢数据）。
//!
//! 修复后下界由子孙 manifest **实际引用**的本分支块内页重建（与安装水位
//! 无关）；Append 没有空闲池，归档重开只回滚可见状态，追加水位保留在活跃
//! manifest 的水位（旧页永不复用）。每条用例都跑 Pooled / Append 对照。

use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_store::{AllocMode, Store};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
const RESTART: u16 = 8;
/// 4 页 / 文件，预留块 1024 文件 = 4096 页：80 条写足以跨过归档水位。
const SHIFT: u8 = 2;
const K_NUM: u32 = 80;
const REWRITE_NUM: u32 = 200;
const MODES: [AllocMode; 2] = [AllocMode::Pooled, AllocMode::Append];

fn mem_store(mode: AllocMode) -> Store<MemStore> {
  Store::new(
    MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
    PAGE_SIZE,
    RESTART,
  )
  .with_pages_per_file_shift(SHIFT)
  .with_alloc_mode(mode)
}

fn mem_reopen(mode: AllocMode, io: MemStore) -> Store<MemStore> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(SHIFT)
    .with_alloc_mode(mode)
}

fn compio_store(root: &TempRoot) -> Store<CompioStore> {
  Store::new(
    CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT),
    PAGE_SIZE,
    RESTART,
  )
  .with_pages_per_file_shift(SHIFT)
  .with_alloc_mode(AllocMode::Pooled)
}

fn up(key: &[u8], value: &[u8], ts: u64) -> BatchEntry {
  BatchEntry::upsert(key.to_vec(), value.to_vec(), ts)
}

fn payload(seed: u8) -> Vec<u8> {
  vec![seed; 120]
}

fn k_key(index: u32) -> Vec<u8> {
  format!("k{index:03}").into_bytes()
}

fn n_key(index: u32) -> Vec<u8> {
  format!("n{index:03}").into_bytes()
}

async fn count_missing<I>(store: &Store<I>, tbl: &TableIdent, n: u32) -> u32
where
  I: fkv_io::PageIo,
{
  let mut missing = 0;
  for index in 0..n {
    if store.get(tbl, &k_key(index)).await.unwrap().is_none() {
      missing += 1;
    }
  }
  missing
}

async fn put_many<I>(store: &mut Store<I>, tbl: &TableIdent, count: u32)
where
  I: fkv_io::PageIo + fkv_io::ManifestIo,
{
  for index in 0..count {
    store
      .put(
        tbl,
        &[up(&n_key(index), &payload(b'N'), 1000 + u64::from(index))],
      )
      .await
      .unwrap();
  }
}

/// 父分支（f1）建旧归档 → 写 k → 建子分支（f2）→ 回滚 f1 的旧归档 → f1 续写。
///
/// 返回 f2 重开后丢失的 k 条数；同时断言 f2 仍可继续写读。
async fn child_rollback_missing(mode: AllocMode, term: u64, name: &str) -> u32 {
  let tbl = TableIdent::new(name, 1).unwrap();
  let mut main = mem_store(mode);
  main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
  main.create_branch(&tbl, "f1").await.unwrap();

  let mut f1 = mem_reopen(mode, main.into_io());
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  f1.put(&tbl, &[up(b"pre", b"PRE", 2)]).await.unwrap();
  f1.create_archive(&tbl, "old", 0).await.unwrap();
  let a_max = f1.mapper(&tbl).unwrap().max_fp_id();
  for index in 0..K_NUM {
    f1.put(
      &tbl,
      &[up(&k_key(index), &payload(b'K'), 10 + u64::from(index))],
    )
    .await
    .unwrap();
  }
  let c_max = f1.mapper(&tbl).unwrap().max_fp_id();
  assert!(c_max > a_max, "k 写必须推进父分支水位 mode={mode:?}");
  f1.create_branch(&tbl, "f2").await.unwrap();
  let f1_limit = f1.branch_file_limit().expect("f1 必须有预留上限");

  let mut f2 = mem_reopen(mode, f1.into_io());
  f2.open_branch(&tbl, "f2", 0).await.unwrap();
  assert_eq!(
    count_missing(&f2, &tbl, K_NUM).await,
    0,
    "子分支首次打开应看到全部继承页 mode={mode:?}"
  );

  // 回滚到早于 k 写、早于 f2 创建的归档。
  let mut f1 = mem_reopen(mode, f2.into_io());
  f1.prepare_attach("f1", term).unwrap();
  f1.reopen_archive(&tbl, "old").await.unwrap();
  let mapper = f1.mapper(&tbl).unwrap();
  assert!(
    mapper.max_fp_id() > a_max,
    "归档回滚后分配水位必须高于归档水位 mode={mode:?} term={term}"
  );
  if mode == AllocMode::Pooled {
    assert!(
      mapper.reuse_floor() > a_max,
      "归档回滚不得把复用下界降回归档水位 mode={mode:?} term={term}"
    );
  }
  assert!(
    mapper.max_fp_id() <= f1_limit,
    "下界不得把父分支水位推过自己的块上限 mode={mode:?} term={term}"
  );

  put_many(&mut f1, &tbl, REWRITE_NUM).await;

  let mut f2 = mem_reopen(mode, f1.into_io());
  f2.open_branch(&tbl, "f2", 0).await.unwrap();
  let missing = count_missing(&f2, &tbl, K_NUM).await;
  f2.put(&tbl, &[up(b"after", b"AFTER", 5000)]).await.unwrap();
  assert_eq!(
    f2.get(&tbl, b"after").await.unwrap(),
    Some(b"AFTER".to_vec()),
    "子分支在父归档回滚后仍必须可写 mode={mode:?} term={term}"
  );
  missing
}

/// 父 → 子 → 孙：孙分支通过子分支的继承链引用同一批父块页。
async fn grandchild_rollback_missing(mode: AllocMode, name: &str) -> u32 {
  let tbl = TableIdent::new(name, 1).unwrap();
  let mut main = mem_store(mode);
  main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
  main.create_branch(&tbl, "f1").await.unwrap();

  let mut f1 = mem_reopen(mode, main.into_io());
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  f1.put(&tbl, &[up(b"pre", b"PRE", 2)]).await.unwrap();
  f1.create_archive(&tbl, "old", 0).await.unwrap();
  let a_max = f1.mapper(&tbl).unwrap().max_fp_id();
  for index in 0..K_NUM {
    f1.put(
      &tbl,
      &[up(&k_key(index), &payload(b'K'), 10 + u64::from(index))],
    )
    .await
    .unwrap();
  }
  let c_max = f1.mapper(&tbl).unwrap().max_fp_id();
  assert!(c_max > a_max);
  f1.create_branch(&tbl, "f2").await.unwrap();

  let mut f2 = mem_reopen(mode, f1.into_io());
  f2.open_branch(&tbl, "f2", 0).await.unwrap();
  f2.create_branch(&tbl, "g1").await.unwrap();

  let mut g1 = mem_reopen(mode, f2.into_io());
  g1.open_branch(&tbl, "g1", 0).await.unwrap();
  assert_eq!(
    count_missing(&g1, &tbl, K_NUM).await,
    0,
    "孙分支首次打开应看到父块继承页 mode={mode:?}"
  );

  let mut f1 = mem_reopen(mode, g1.into_io());
  f1.prepare_attach("f1", 0).unwrap();
  f1.reopen_archive(&tbl, "old").await.unwrap();
  assert!(
    f1.mapper(&tbl).unwrap().max_fp_id() > a_max,
    "归档回滚后分配水位必须高于归档水位 mode={mode:?}"
  );
  put_many(&mut f1, &tbl, REWRITE_NUM).await;

  let mut g1 = mem_reopen(mode, f1.into_io());
  g1.open_branch(&tbl, "g1", 0).await.unwrap();
  count_missing(&g1, &tbl, K_NUM).await
}

/// 控制组：归档晚于子分支创建，回滚水位 `A' >= C`，修复前后都不得丢数据。
async fn archive_after_child_missing(mode: AllocMode, name: &str) -> u32 {
  let tbl = TableIdent::new(name, 1).unwrap();
  let mut main = mem_store(mode);
  main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
  main.create_branch(&tbl, "f1").await.unwrap();

  let mut f1 = mem_reopen(mode, main.into_io());
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  f1.put(&tbl, &[up(b"pre", b"PRE", 2)]).await.unwrap();
  for index in 0..40 {
    f1.put(
      &tbl,
      &[up(&k_key(index), &payload(b'K'), 10 + u64::from(index))],
    )
    .await
    .unwrap();
  }
  f1.create_branch(&tbl, "f2").await.unwrap();

  let mut f2 = mem_reopen(mode, f1.into_io());
  f2.open_branch(&tbl, "f2", 0).await.unwrap();
  assert_eq!(count_missing(&f2, &tbl, 40).await, 0);
  let mut f1 = mem_reopen(mode, f2.into_io());
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  // 归档创建于子分支之后：水位已覆盖继承页。
  f1.create_archive(&tbl, "new", 0).await.unwrap();
  let mut f1 = mem_reopen(mode, f1.into_io());
  f1.prepare_attach("f1", 0).unwrap();
  f1.reopen_archive(&tbl, "new").await.unwrap();
  put_many(&mut f1, &tbl, REWRITE_NUM).await;

  let mut f2 = mem_reopen(mode, f1.into_io());
  f2.open_branch(&tbl, "f2", 0).await.unwrap();
  count_missing(&f2, &tbl, 40).await
}

/// MemStore + Pooled/Append：归档早于子分支，父回滚后子快照不得丢。
#[compio::test]
async fn parent_archive_rollback_before_child_keeps_child_snapshot() {
  for mode in MODES {
    assert_eq!(
      child_rollback_missing(mode, 0, "p11_child").await,
      0,
      "mode={mode:?}"
    );
  }
}

/// MemStore + Pooled/Append：换 term 重开同一归档，继承页同样不得丢。
#[compio::test]
async fn parent_archive_rollback_term7_keeps_child_snapshot() {
  for mode in MODES {
    assert_eq!(
      child_rollback_missing(mode, 7, "p11_child_term7").await,
      0,
      "mode={mode:?} term=7"
    );
  }
}

/// MemStore + Pooled/Append：孙分支继承链上的父块页不得被父回滚覆盖。
#[compio::test]
async fn parent_archive_rollback_keeps_grandchild_snapshot() {
  for mode in MODES {
    assert_eq!(
      grandchild_rollback_missing(mode, "p11_grandchild").await,
      0,
      "mode={mode:?}"
    );
  }
}

/// 控制组：归档在子分支之后创建，回滚不丢数据（防止修复误伤正常路径）。
#[compio::test]
async fn parent_archive_after_child_rollback_no_loss() {
  for mode in MODES {
    assert_eq!(
      archive_after_child_missing(mode, "p11_after").await,
      0,
      "mode={mode:?}"
    );
  }
}

/// CompioStore + Pooled：同 term 与换 term 各跑一次（默认本地后端同口径）。
#[compio::test]
async fn compio_parent_archive_rollback_keeps_child_snapshot() {
  for term in [0u64, 7] {
    let root = TempRoot::new(&format!("p11_compio_child_{term}"));
    let tbl = TableIdent::new("p11_compio_child", 1).unwrap();
    let mut main = compio_store(&root);
    main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
    main.create_branch(&tbl, "f1").await.unwrap();
    drop(main);

    let mut f1 = compio_store(&root);
    f1.open_branch(&tbl, "f1", 0).await.unwrap();
    f1.put(&tbl, &[up(b"pre", b"PRE", 2)]).await.unwrap();
    f1.create_archive(&tbl, "old", 0).await.unwrap();
    let a_max = f1.mapper(&tbl).unwrap().max_fp_id();
    for index in 0..K_NUM {
      f1.put(
        &tbl,
        &[up(&k_key(index), &payload(b'K'), 10 + u64::from(index))],
      )
      .await
      .unwrap();
    }
    let c_max = f1.mapper(&tbl).unwrap().max_fp_id();
    assert!(c_max > a_max);
    f1.create_branch(&tbl, "f2").await.unwrap();
    let f1_limit = f1.branch_file_limit().expect("f1 必须有预留上限");
    drop(f1);

    let mut f2 = compio_store(&root);
    f2.open_branch(&tbl, "f2", 0).await.unwrap();
    assert_eq!(
      count_missing(&f2, &tbl, K_NUM).await,
      0,
      "子分支首次打开应看到全部继承页 term={term}"
    );
    drop(f2);

    let mut f1 = compio_store(&root);
    f1.prepare_attach("f1", term).unwrap();
    f1.reopen_archive(&tbl, "old").await.unwrap();
    let mapper = f1.mapper(&tbl).unwrap();
    assert!(
      mapper.max_fp_id() > a_max,
      "归档回滚后复用下界不得停在归档水位 term={term}"
    );
    assert!(mapper.reuse_floor() > a_max, "term={term}");
    assert!(mapper.max_fp_id() <= f1_limit, "term={term}");
    put_many(&mut f1, &tbl, REWRITE_NUM).await;
    drop(f1);

    let mut f2 = compio_store(&root);
    f2.open_branch(&tbl, "f2", 0).await.unwrap();
    assert_eq!(
      count_missing(&f2, &tbl, K_NUM).await,
      0,
      "父归档回滚后子分支快照丢失 term={term}"
    );
    f2.put(&tbl, &[up(b"after", b"AFTER", 5000)]).await.unwrap();
    assert_eq!(
      f2.get(&tbl, b"after").await.unwrap(),
      Some(b"AFTER".to_vec())
    );
  }
}
