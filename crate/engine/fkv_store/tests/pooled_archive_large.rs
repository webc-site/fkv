//! pooled-12：归档重开只回滚可见状态，segment（大 value）追加水位不得回退。
//!
//! `reopen_archive` 把归档时刻的 manifest 安装为活跃状态。pooled-11 只保留了
//! Append 的 data 追加水位（`replay.max_fp_id`），segment 侧
//! `replay.max_segment_fp_id` 仍随归档回退。段 mapper **恒 Append**
//! （pooled-02），与 `alloc_mode` 无关：回滚后新 large value 从归档段水位重新
//! 分配，覆盖仍被「更晚归档 / 子 / 孙分支」manifest 引用的段页。MemStore 按
//! `(tbl, segment_fp_id)` 覆盖，Compio / Cloud 同 identity 段文件名覆盖，
//! `get_large` 返回另一份合法字节且无错误码；回滚后的
//! `write_snapshot_manifest` 还会把回退水位持久化，崩溃重开不自愈。
//!
//! 修复：安装归档前把 `replay.max_segment_fp_id` 抬到活跃状态当前段水位
//! （内存 mapper / 盘上活跃 manifest），两种分配模式都保留；活跃状态不存在
//! 而归档带段体系时拒绝回滚（`ResourceMissing`）。每条 MemStore 用例跑
//! Pooled / Append 对照，Compio 覆盖同 term、main 与 term 0→3。

use fkv_btree::BatchEntry;
use fkv_io::{PageIo, SegmentIo};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_store::{AllocMode, Store};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
const RESTART: u16 = 8;
/// 4 页 / 文件，预留块 1024 文件 = 4096 页（与 pooled-11 用例一致）。
const SHIFT: u8 = 2;
const SEG_SIZE: usize = 4096;
/// 4 段 / 段文件。
const SEG_SHIFT: u8 = 2;
const THRESHOLD: usize = 1024;
/// 3 段：同 identity 下一个段文件内即可触发覆盖。
const BIG_LEN: usize = 9000;
/// 8 段：跨 2 个段文件，term bump 的「一个文件错位」挡不住。
const LONG_LEN: usize = 30000;
const MODES: [AllocMode; 2] = [AllocMode::Pooled, AllocMode::Append];

fn layout() -> SegmentLayout {
  SegmentLayout::new(SEG_SIZE as u32, SEG_SHIFT).expect("segment layout")
}

fn mem_store(mode: AllocMode) -> Store<MemStore> {
  Store::new(
    MemStore::with_pages_per_file(PAGE_SIZE, SHIFT).with_segment_config(SEG_SIZE, SEG_SHIFT),
    PAGE_SIZE,
    RESTART,
  )
  .with_pages_per_file_shift(SHIFT)
  .with_large_value(layout(), THRESHOLD)
  .with_alloc_mode(mode)
}

fn mem_reopen(mode: AllocMode, io: MemStore) -> Store<MemStore> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(SHIFT)
    .with_large_value(layout(), THRESHOLD)
    .with_alloc_mode(mode)
}

fn compio_store(root: &TempRoot, mode: AllocMode) -> Store<CompioStore> {
  Store::new(
    CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT)
      .with_segment_config(SEG_SIZE, SEG_SHIFT),
    PAGE_SIZE,
    RESTART,
  )
  .with_pages_per_file_shift(SHIFT)
  .with_large_value(layout(), THRESHOLD)
  .with_alloc_mode(mode)
}

fn up(key: &[u8], value: &[u8], ts: u64) -> BatchEntry {
  BatchEntry::upsert(key.to_vec(), value.to_vec(), ts)
}

/// 带 seed 的确定性长 value：不同版本首字节不同，便于失败时定位覆盖来源。
fn pattern(len: usize, seed: u8) -> Vec<u8> {
  (0..len)
    .map(|index| seed.wrapping_add((index % 251) as u8))
    .collect()
}

async fn large_value<I>(store: &Store<I>, tbl: &TableIdent, key: &[u8]) -> Option<Vec<u8>>
where
  I: PageIo + SegmentIo,
{
  store
    .get_large(tbl, key)
    .await
    .unwrap()
    .map(|read| read.value)
}

fn seg_max<I: PageIo>(store: &Store<I>, tbl: &TableIdent) -> u64 {
  store
    .segment_mapper(tbl)
    .unwrap_or_else(|| panic!("segment mapper 缺失"))
    .max_fp_id()
}

/// main 建 branch f1 后以 f1 身份重开，返回已写入 `pre` 的 branch Store。
async fn mem_branch_store(mode: AllocMode, name: &str) -> (TableIdent, Store<MemStore>) {
  let tbl = TableIdent::new(name, 1).unwrap();
  let mut main = mem_store(mode);
  main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
  main.create_branch(&tbl, "f1").await.unwrap();
  let mut f1 = mem_reopen(mode, main.into_io());
  f1.prepare_attach("f1", 0).unwrap();
  f1.reopen(&tbl).await.unwrap();
  f1.put(&tbl, &[up(b"pre", b"P", 2)]).await.unwrap();
  (tbl, f1)
}

/// A：归档 a1（早于大 value）→ big1 → 归档 a2 → 回滚 a1 → big2 → 重开 a2。
///
/// a2 仍引用 big1 的段页；回滚后新 big2 若从 a1 的段水位分配，就会用 big2 的
/// 字节覆盖它们。
#[test]
fn parent_archive_rollback_keeps_later_archive_large_value() {
  pollster::block_on(async {
    for mode in MODES {
      let (tbl, mut f1) = mem_branch_store(mode, &format!("p12_later_{mode:?}")).await;
      let v1 = pattern(BIG_LEN, 0x11);
      let v2 = pattern(BIG_LEN, 0x77);
      f1.create_archive(&tbl, "a1", 0).await.unwrap();
      f1.put_large(&tbl, &[up(b"big1", &v1, 3)]).await.unwrap();
      f1.create_archive(&tbl, "a2", 0).await.unwrap();
      let after_a2 = seg_max(&f1, &tbl);
      f1.reopen_archive(&tbl, "a1").await.unwrap();
      assert!(
        seg_max(&f1, &tbl) >= after_a2,
        "mode={mode:?}: 归档回滚把段追加水位从 {after_a2} 降到 {}",
        seg_max(&f1, &tbl)
      );
      f1.put_large(&tbl, &[up(b"big2", &v2, 4)]).await.unwrap();
      f1.reopen_archive(&tbl, "a2").await.unwrap();
      assert_eq!(
        large_value(&f1, &tbl, b"big1").await,
        Some(v1),
        "mode={mode:?}: a2 的 big1 被回滚后写入的 big2 覆盖"
      );
      // a2 早于 big2：回滚后写的 key 不在 a2 的可见映射里。
      assert_eq!(large_value(&f1, &tbl, b"big2").await, None, "mode={mode:?}");
    }
  });
}

/// B：归档 a1 → big1 → 建子分支 f2（继承 big1）→ f2 建孙 g1 → 父回滚 a1 →
/// big2 → 重开 f2 / g1。data 映射由 pooled-11 的子孙页下界保护，被覆盖的是
/// 子 / 孙 manifest 继承的段页。
#[test]
fn parent_archive_rollback_keeps_child_and_grandchild_large_value() {
  pollster::block_on(async {
    for mode in MODES {
      let (tbl, mut f1) = mem_branch_store(mode, &format!("p12_child_{mode:?}")).await;
      let v1 = pattern(BIG_LEN, 0x22);
      let v2 = pattern(BIG_LEN, 0x88);
      f1.create_archive(&tbl, "a1", 0).await.unwrap();
      f1.put_large(&tbl, &[up(b"big1", &v1, 3)]).await.unwrap();
      f1.create_branch(&tbl, "f2").await.unwrap();

      let mut f2 = mem_reopen(mode, f1.into_io());
      f2.prepare_attach("f2", 0).unwrap();
      f2.reopen(&tbl).await.unwrap();
      assert_eq!(
        large_value(&f2, &tbl, b"big1").await,
        Some(v1.clone()),
        "mode={mode:?}: 子分支创建时应继承 big1"
      );
      f2.create_branch(&tbl, "g1").await.unwrap();

      let mut g1 = mem_reopen(mode, f2.into_io());
      g1.prepare_attach("g1", 0).unwrap();
      g1.reopen(&tbl).await.unwrap();
      assert_eq!(
        large_value(&g1, &tbl, b"big1").await,
        Some(v1.clone()),
        "mode={mode:?}: 孙分支创建时应继承 big1"
      );

      let mut f1 = mem_reopen(mode, g1.into_io());
      f1.prepare_attach("f1", 0).unwrap();
      f1.reopen(&tbl).await.unwrap();
      f1.reopen_archive(&tbl, "a1").await.unwrap();
      f1.put_large(&tbl, &[up(b"big2", &v2, 4)]).await.unwrap();
      let io = f1.into_io();

      let mut f2 = mem_reopen(mode, io);
      f2.prepare_attach("f2", 0).unwrap();
      f2.reopen(&tbl).await.unwrap();
      assert_eq!(
        large_value(&f2, &tbl, b"big1").await,
        Some(v1.clone()),
        "mode={mode:?}: 父分支回滚后子分支 big1 被覆盖"
      );

      let mut g1 = mem_reopen(mode, f2.into_io());
      g1.prepare_attach("g1", 0).unwrap();
      g1.reopen(&tbl).await.unwrap();
      assert_eq!(
        large_value(&g1, &tbl, b"big1").await,
        Some(v1),
        "mode={mode:?}: 父分支回滚后孙分支 big1 被覆盖"
      );
    }
  });
}

/// C：回滚 + big2 后崩溃 / Store 重开，活跃 manifest 不得持久化回退的段水位。
#[test]
fn parent_archive_rollback_large_value_survives_crash() {
  pollster::block_on(async {
    for mode in MODES {
      let (tbl, mut f1) = mem_branch_store(mode, &format!("p12_crash_{mode:?}")).await;
      let v1 = pattern(BIG_LEN, 0x33);
      let v2 = pattern(BIG_LEN, 0x99);
      f1.create_archive(&tbl, "a1", 0).await.unwrap();
      f1.put_large(&tbl, &[up(b"big1", &v1, 3)]).await.unwrap();
      f1.create_archive(&tbl, "a2", 0).await.unwrap();
      let after_a2 = seg_max(&f1, &tbl);
      f1.reopen_archive(&tbl, "a1").await.unwrap();
      f1.put_large(&tbl, &[up(b"big2", &v2, 4)]).await.unwrap();
      let after_big2 = seg_max(&f1, &tbl);

      // 模拟崩溃：只留下 MemStore，新 Store 从盘上 replay。
      let io = f1.into_io();
      let mut f1 = mem_reopen(mode, io);
      f1.prepare_attach("f1", 0).unwrap();
      f1.reopen(&tbl).await.unwrap();
      assert!(
        seg_max(&f1, &tbl) >= after_big2,
        "mode={mode:?}: 重开后段水位回退（盘上 {} < 崩溃前 {after_big2}）",
        seg_max(&f1, &tbl)
      );
      assert!(
        after_big2 > after_a2,
        "mode={mode:?}: 回滚后 big2 必须分配在 a2 段水位之后"
      );
      f1.reopen_archive(&tbl, "a2").await.unwrap();
      assert_eq!(
        large_value(&f1, &tbl, b"big1").await,
        Some(v1),
        "mode={mode:?}: 崩溃重开后重开 a2 仍读到被覆盖的 big1"
      );
    }
  });
}

/// D：term 0→3 回滚同一归档。9000B（3 段）可能被一个段文件的错位侥幸避开，
/// 这里用 30000B（8 段，跨 2 个段文件）压住「term bump 不是安全边界」。
#[test]
fn parent_archive_rollback_term_bump_keeps_large_value() {
  pollster::block_on(async {
    for mode in MODES {
      let (tbl, mut f1) = mem_branch_store(mode, &format!("p12_term_{mode:?}")).await;
      let v1 = pattern(LONG_LEN, 0x44);
      let v2 = pattern(LONG_LEN, 0xAA);
      f1.create_archive(&tbl, "a1", 0).await.unwrap();
      f1.put_large(&tbl, &[up(b"big1", &v1, 3)]).await.unwrap();
      f1.create_archive(&tbl, "a2", 0).await.unwrap();
      let after_a2 = seg_max(&f1, &tbl);
      f1.prepare_attach("f1", 3).unwrap();
      f1.reopen_archive(&tbl, "a1").await.unwrap();
      assert!(
        seg_max(&f1, &tbl) >= after_a2,
        "mode={mode:?}: term bump 回滚后段水位 {} < {after_a2}",
        seg_max(&f1, &tbl)
      );
      f1.put_large(&tbl, &[up(b"big2", &v2, 4)]).await.unwrap();
      f1.reopen_archive(&tbl, "a2").await.unwrap();
      assert_eq!(
        large_value(&f1, &tbl, b"big1").await,
        Some(v1),
        "mode={mode:?}: term bump 不能阻止跨段文件的 big1 被覆盖"
      );
    }
  });
}

/// 控制组：归档晚于 big1 时回滚不丢（修复前也通过，防止修复误拒正常路径）。
#[test]
fn parent_archive_after_large_value_rollback_no_loss() {
  pollster::block_on(async {
    for mode in MODES {
      let (tbl, mut f1) = mem_branch_store(mode, &format!("p12_ctrl_{mode:?}")).await;
      let v1 = pattern(BIG_LEN, 0x55);
      let v2 = pattern(BIG_LEN, 0xBB);
      f1.put_large(&tbl, &[up(b"big1", &v1, 3)]).await.unwrap();
      f1.create_archive(&tbl, "a1", 0).await.unwrap();
      f1.reopen_archive(&tbl, "a1").await.unwrap();
      f1.put_large(&tbl, &[up(b"big2", &v2, 4)]).await.unwrap();
      assert_eq!(
        large_value(&f1, &tbl, b"big1").await,
        Some(v1),
        "mode={mode:?}: 归档已包含 big1，回滚后不得被 big2 覆盖"
      );
    }
  });
}

/// main 冷进程（等价 shard `ReopenArchive` 的 `SKIPS_OPEN`）：新 Store 没有表
/// 状态，直接回滚 a1 再写 big2，段水位只能从盘上活跃 main manifest 读取。
#[test]
fn main_archive_rollback_large_value_survives_cold_reopen() {
  pollster::block_on(async {
    for mode in MODES {
      let tbl = TableIdent::new(format!("p12_main_{mode:?}"), 1).unwrap();
      let v1 = pattern(BIG_LEN, 0x66);
      let v2 = pattern(BIG_LEN, 0xCC);
      let mut main = mem_store(mode);
      main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
      main.create_archive(&tbl, "a1", 0).await.unwrap();
      main.put_large(&tbl, &[up(b"big1", &v1, 2)]).await.unwrap();
      main.create_archive(&tbl, "a2", 0).await.unwrap();
      let io = main.into_io();

      let mut cold = mem_reopen(mode, io);
      assert!(
        cold.segment_mapper(&tbl).is_none(),
        "mode={mode:?}: 冷 Store 不应已有表状态"
      );
      cold.reopen_archive(&tbl, "a1").await.unwrap();
      cold.put_large(&tbl, &[up(b"big2", &v2, 3)]).await.unwrap();
      cold.reopen_archive(&tbl, "a2").await.unwrap();
      assert_eq!(
        large_value(&cold, &tbl, b"big1").await,
        Some(v1),
        "mode={mode:?}: main 冷进程回滚未从盘上活跃 manifest 保留段水位"
      );
    }
  });
}

/// Compio 同 term（生产默认本地后端）：回滚写回同一
/// `segment_<file>_<branch>_<term>` 文件，按同一对象键覆盖。
#[compio::test]
async fn compio_branch_same_term_keeps_large_value() {
  for mode in MODES {
    let root = TempRoot::new(&format!("p12_compio_branch_{mode:?}"));
    let tbl = TableIdent::new(format!("p12cb_{mode:?}"), 1).unwrap();
    let v1 = pattern(BIG_LEN, 0x11);
    let v2 = pattern(BIG_LEN, 0x77);
    let mut main = compio_store(&root, mode);
    main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
    main.create_branch(&tbl, "f1").await.unwrap();
    drop(main);

    let mut f1 = compio_store(&root, mode);
    f1.prepare_attach("f1", 0).unwrap();
    f1.reopen(&tbl).await.unwrap();
    f1.put(&tbl, &[up(b"pre", b"P", 2)]).await.unwrap();
    f1.create_archive(&tbl, "a1", 0).await.unwrap();
    f1.put_large(&tbl, &[up(b"big1", &v1, 3)]).await.unwrap();
    f1.create_archive(&tbl, "a2", 0).await.unwrap();
    let after_a2 = seg_max(&f1, &tbl);
    f1.reopen_archive(&tbl, "a1").await.unwrap();
    assert!(
      seg_max(&f1, &tbl) >= after_a2,
      "mode={mode:?}: Compio 回滚后段水位 {} < {after_a2}",
      seg_max(&f1, &tbl)
    );
    f1.put_large(&tbl, &[up(b"big2", &v2, 4)]).await.unwrap();
    f1.reopen_archive(&tbl, "a2").await.unwrap();
    assert_eq!(
      large_value(&f1, &tbl, b"big1").await,
      Some(v1),
      "mode={mode:?}: Compio 同 term 段文件被 big2 覆盖"
    );
  }
}

/// Compio main 身份：活跃 main manifest 不在具名文件里，段水位从本地
/// active manifest 读取（同一 Store 走内存 mapper）。
#[compio::test]
async fn compio_main_same_term_keeps_large_value() {
  for mode in MODES {
    let root = TempRoot::new(&format!("p12_compio_main_{mode:?}"));
    let tbl = TableIdent::new(format!("p12cm_{mode:?}"), 1).unwrap();
    let v1 = pattern(BIG_LEN, 0x21);
    let v2 = pattern(BIG_LEN, 0x66);
    let mut main = compio_store(&root, mode);
    main.prepare_attach("main", 0).unwrap();
    main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
    main.create_archive(&tbl, "a1", 0).await.unwrap();
    main.put_large(&tbl, &[up(b"big1", &v1, 2)]).await.unwrap();
    main.create_archive(&tbl, "a2", 0).await.unwrap();
    let after_a2 = seg_max(&main, &tbl);
    main.reopen_archive(&tbl, "a1").await.unwrap();
    assert!(
      seg_max(&main, &tbl) >= after_a2,
      "mode={mode:?}: Compio main 回滚后段水位 {} < {after_a2}",
      seg_max(&main, &tbl)
    );
    main.put_large(&tbl, &[up(b"big2", &v2, 3)]).await.unwrap();
    main.reopen_archive(&tbl, "a2").await.unwrap();
    assert_eq!(
      large_value(&main, &tbl, b"big1").await,
      Some(v1),
      "mode={mode:?}: Compio main 段文件被 big2 覆盖"
    );
  }
}

/// Compio term 0→3 + 30000B：一个段文件的错位不足以避开跨文件的大 value。
#[compio::test]
async fn compio_term_bump_keeps_large_value() {
  for mode in MODES {
    let root = TempRoot::new(&format!("p12_compio_term_{mode:?}"));
    let tbl = TableIdent::new(format!("p12ct_{mode:?}"), 1).unwrap();
    let v1 = pattern(LONG_LEN, 0x44);
    let v2 = pattern(LONG_LEN, 0xAA);
    let mut main = compio_store(&root, mode);
    main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
    main.create_branch(&tbl, "f1").await.unwrap();
    drop(main);

    let mut f1 = compio_store(&root, mode);
    f1.prepare_attach("f1", 0).unwrap();
    f1.reopen(&tbl).await.unwrap();
    f1.put(&tbl, &[up(b"pre", b"P", 2)]).await.unwrap();
    f1.create_archive(&tbl, "a1", 0).await.unwrap();
    f1.put_large(&tbl, &[up(b"big1", &v1, 3)]).await.unwrap();
    f1.create_archive(&tbl, "a2", 0).await.unwrap();
    f1.prepare_attach("f1", 3).unwrap();
    f1.reopen_archive(&tbl, "a1").await.unwrap();
    f1.put_large(&tbl, &[up(b"big2", &v2, 4)]).await.unwrap();
    f1.reopen_archive(&tbl, "a2").await.unwrap();
    assert_eq!(
      large_value(&f1, &tbl, b"big1").await,
      Some(v1),
      "mode={mode:?}: Compio term bump 后跨段文件的 big1 被覆盖"
    );
  }
}
