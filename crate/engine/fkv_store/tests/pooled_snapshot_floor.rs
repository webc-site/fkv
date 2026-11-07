//! pooled-07：Pooled 下 branch / archive 快照引用的物理页不得被回收复用。
//!
//! branch / archive 是持久化的页集合快照，不在 `SnapshotRegistry` 的读快照
//! 代际水位内：父表覆盖快照前已存在的 key 后，旧物理页离开活跃映射且没有
//! 在途读者，代际判据会放行回池——下一次分配立即覆盖快照数据。
//!
//! 两个方向都覆盖，每条用例同时跑 Pooled 与 Append 对照：
//! 1. 父表覆盖 → 快照 / archive 仍读原值；
//! 2. 子分支覆盖继承页 → 父表重开仍读原值；
//! 3. 父表 reopen 后的空闲池回填不得把子分支块 / 归档引用页当空洞。

use fkv_btree::BatchEntry;
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

fn reopen_store(mode: AllocMode, io: MemStore) -> Store<MemStore> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(DEFAULT_PAGES_PER_FILE_SHIFT)
    .with_alloc_mode(mode)
}

fn up(key: &[u8], value: &[u8], ts: u64) -> BatchEntry {
  BatchEntry::upsert(key.to_vec(), value.to_vec(), ts)
}

/// 父表覆盖快照前 key 再写新 key：branch 快照必须仍读到创建时的值。
#[compio::test]
async fn pooled_branch_snapshot_survives_parent_overwrite() {
  for mode in MODES {
    let tbl = TableIdent::new("p07_branch", 1).unwrap();
    let mut store = new_store(mode);
    store
      .put(&tbl, &[up(b"a", b"A", 1), up(b"x", b"X", 1)])
      .await
      .unwrap();
    store.create_branch(&tbl, "feature").await.unwrap();
    // 覆盖快照前已存在的 key：旧物理页退役；随后的一次写驱动回收 + 分配。
    store.put(&tbl, &[up(b"a", b"B", 2)]).await.unwrap();
    store.put(&tbl, &[up(b"b", b"C", 3)]).await.unwrap();

    let mut branch = reopen_store(mode, store.into_io());
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(
      branch.get(&tbl, b"a").await.unwrap(),
      Some(b"A".to_vec()),
      "branch 快照被父表后续写覆盖 mode={mode:?}"
    );
    assert_eq!(
      branch.get(&tbl, b"x").await.unwrap(),
      Some(b"X".to_vec()),
      "未被覆盖的 key 仍可读 mode={mode:?}"
    );
  }
}

/// 归档后父表覆盖再写：`reopen_archive` 必须读到归档时刻的值。
#[compio::test]
async fn pooled_archive_snapshot_survives_parent_overwrite() {
  for mode in MODES {
    let tbl = TableIdent::new("p07_archive", 1).unwrap();
    let mut store = new_store(mode);
    store
      .put(&tbl, &[up(b"a", b"A", 1), up(b"x", b"X", 1)])
      .await
      .unwrap();
    store.create_archive(&tbl, "tag1", 4).await.unwrap();
    store.put(&tbl, &[up(b"a", b"B", 2)]).await.unwrap();
    store.put(&tbl, &[up(b"b", b"C", 3)]).await.unwrap();

    store.reopen_archive(&tbl, "tag1").await.unwrap();
    assert_eq!(
      store.get(&tbl, b"a").await.unwrap(),
      Some(b"A".to_vec()),
      "archive 快照被父表后续写覆盖 mode={mode:?}"
    );
    assert_eq!(
      store.get(&tbl, b"x").await.unwrap(),
      Some(b"X".to_vec()),
      "未被覆盖的 key 仍可读 mode={mode:?}"
    );
  }
}

/// 子分支覆盖继承 key 后再写一次：父表重开必须仍读到原值。
///
/// `MemStore` 的页 key 只有 `(tbl, file_page_id)`（不含 identity），子分支
/// 复用继承页会直接覆盖父表页；Compio / Cloud 的 `(branch, term)` 命名同样
/// 会被同一 file id 的兄弟写命中，因此下界必须覆盖整个继承区。
#[compio::test]
async fn pooled_branch_child_overwrite_keeps_parent() {
  for mode in MODES {
    let tbl = TableIdent::new("p07_child", 1).unwrap();
    let mut parent = new_store(mode);
    parent.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
    parent.create_branch(&tbl, "feature").await.unwrap();

    let mut child = reopen_store(mode, parent.into_io());
    child.open_branch(&tbl, "feature", 0).await.unwrap();
    child.put(&tbl, &[up(b"a", b"B", 2)]).await.unwrap();
    // 第二次写驱动 reclaim + allocate：继承页不得回池。
    child.put(&tbl, &[up(b"b", b"C", 3)]).await.unwrap();
    assert_eq!(
      child.get(&tbl, b"a").await.unwrap(),
      Some(b"B".to_vec()),
      "子分支自身写入必须可见 mode={mode:?}"
    );

    let mut parent = reopen_store(mode, child.into_io());
    parent.reopen(&tbl).await.unwrap();
    assert_eq!(
      parent.get(&tbl, b"a").await.unwrap(),
      Some(b"A".to_vec()),
      "父表数据被子分支复用页覆盖 mode={mode:?}"
    );
  }
}

/// 父表 truncate 后重写：整表释放会退役全部旧页，回收同样不得复用快照页。
#[compio::test]
async fn pooled_branch_survives_parent_truncate_rewrite() {
  for mode in MODES {
    let tbl = TableIdent::new("p07_truncate", 1).unwrap();
    let mut store = new_store(mode);
    store
      .put(&tbl, &[up(b"a", b"A", 1), up(b"x", b"X", 1)])
      .await
      .unwrap();
    store.create_branch(&tbl, "feature").await.unwrap();
    store.truncate(&tbl).await.unwrap();
    store.put(&tbl, &[up(b"b", b"C", 2)]).await.unwrap();
    store.put(&tbl, &[up(b"c", b"D", 3)]).await.unwrap();

    let mut branch = reopen_store(mode, store.into_io());
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(
      branch.get(&tbl, b"a").await.unwrap(),
      Some(b"A".to_vec()),
      "truncate / 重写覆盖了 branch 快照 mode={mode:?}"
    );
    assert_eq!(
      branch.get(&tbl, b"x").await.unwrap(),
      Some(b"X".to_vec()),
      "未被覆盖的 key 仍可读 mode={mode:?}"
    );
  }
}

/// 父表 reopen 后继续写：空闲池回填不得把子分支块当空洞复用。
#[compio::test]
async fn pooled_parent_reopen_keeps_branch_snapshot() {
  for mode in MODES {
    let tbl = TableIdent::new("p07_parent_reopen", 1).unwrap();
    let mut store = new_store(mode);
    store
      .put(&tbl, &[up(b"a", b"A", 1), up(b"x", b"X", 1)])
      .await
      .unwrap();
    store.create_branch(&tbl, "feature").await.unwrap();
    store.put(&tbl, &[up(b"a", b"B", 2)]).await.unwrap();

    let mut store = reopen_store(mode, store.into_io());
    store.reopen(&tbl).await.unwrap();
    if mode == AllocMode::Pooled {
      let floor = store.mapper(&tbl).expect("mapper").reuse_floor();
      assert!(floor > 0, "reopen 必须从 branch manifest 重建复用下界");
    }
    // reopen 后的写：Pooled 的空闲池从映射空洞重建，不得回填到子分支块。
    store.put(&tbl, &[up(b"b", b"C", 3)]).await.unwrap();
    store.put(&tbl, &[up(b"c", b"D", 4)]).await.unwrap();

    let mut branch = reopen_store(mode, store.into_io());
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(
      branch.get(&tbl, b"a").await.unwrap(),
      Some(b"A".to_vec()),
      "父表 reopen 后写入覆盖了 branch 快照 mode={mode:?}"
    );
    assert_eq!(
      branch.get(&tbl, b"x").await.unwrap(),
      Some(b"X".to_vec()),
      "未被覆盖的 key 仍可读 mode={mode:?}"
    );
  }
}

/// 归档创建后重启（plain reopen）再覆盖写：归档快照必须仍然完好。
#[compio::test]
async fn pooled_archive_survives_parent_reopen() {
  for mode in MODES {
    let tbl = TableIdent::new("p07_archive_reopen", 1).unwrap();
    let mut store = new_store(mode);
    store
      .put(&tbl, &[up(b"a", b"A", 1), up(b"x", b"X", 1)])
      .await
      .unwrap();
    store.create_archive(&tbl, "tag1", 4).await.unwrap();
    store.put(&tbl, &[up(b"a", b"B", 2)]).await.unwrap();

    let mut store = reopen_store(mode, store.into_io());
    store.reopen(&tbl).await.unwrap();
    if mode == AllocMode::Pooled {
      let floor = store.mapper(&tbl).expect("mapper").reuse_floor();
      assert!(floor > 0, "reopen 必须从 archive manifest 重建复用下界");
    }
    store.put(&tbl, &[up(b"b", b"C", 3)]).await.unwrap();
    store.put(&tbl, &[up(b"a", b"D", 4)]).await.unwrap();
    store.put(&tbl, &[up(b"c", b"E", 5)]).await.unwrap();

    store.reopen_archive(&tbl, "tag1").await.unwrap();
    assert_eq!(
      store.get(&tbl, b"a").await.unwrap(),
      Some(b"A".to_vec()),
      "archive 快照被 reopen 后的覆盖写破坏 mode={mode:?}"
    );
    assert_eq!(
      store.get(&tbl, b"x").await.unwrap(),
      Some(b"X".to_vec()),
      "未被覆盖的 key 仍可读 mode={mode:?}"
    );
  }
}

/// 行为版（shift=0，每文件 1 页）：父表 reopen 后写满整段空洞，分配不得落进
/// branch 的继承页。branch 预留块只有 1024 页，空闲池从高水位向下分配约
/// 2048 页即可回到 page 0，能确定性地触发旧实现的覆盖。
#[compio::test]
async fn pooled_parent_reopen_write_does_not_reach_branch_pages() {
  for mode in MODES {
    let tbl = TableIdent::new("p07_parent_reopen_write", 1).unwrap();
    let mut store = Store::new(
      MemStore::with_pages_per_file(PAGE_SIZE, 0),
      PAGE_SIZE,
      RESTART,
    )
    .with_pages_per_file_shift(0)
    .with_alloc_mode(mode);
    store
      .put(&tbl, &[up(b"a", b"A", 1), up(b"x", b"X", 1)])
      .await
      .unwrap();
    store.create_branch(&tbl, "feature").await.unwrap();
    // 覆盖 a：旧页离开父表映射，reopen 后成为可回填空洞。
    store.put(&tbl, &[up(b"a", b"B", 2)]).await.unwrap();

    let mut store = reopen_store(mode, store.into_io());
    store.reopen(&tbl).await.unwrap();
    let batch: Vec<BatchEntry> = (0..3000u32)
      .map(|i| BatchEntry::upsert(format!("n{i:05}").into_bytes(), vec![7u8; 300], 3))
      .collect();
    store.put(&tbl, &batch).await.unwrap();

    let mut branch = Store::new(store.into_io(), PAGE_SIZE, RESTART)
      .with_pages_per_file_shift(0)
      .with_alloc_mode(mode);
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(
      branch.get(&tbl, b"a").await.unwrap(),
      Some(b"A".to_vec()),
      "父表 reopen 后写入覆盖了 branch 继承页 mode={mode:?}"
    );
  }
}
