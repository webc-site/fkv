//! pooled-13：生产配置（分区恒配置 large value）下孤儿归档的冷重开。
//!
//! 生产 shard / SDK 无条件 `with_large_value`，`create_branch` 的
//! `use_segment` 因此恒为 true：branch manifest / 归档都带「子分支段块基址」
//! 作为 `max_segment_fp_id`，**纯数据**分支的归档也是「空 segment mapping +
//! 高水位 > 0」。branch manifest 被 `delete_branch` 删除后（sem-28 孤儿归档），
//! 冷 Store 的 `reopen_archive` 两处活跃段水位都不存在，旧判据
//! `archive_seg_max > 0` 会把它误判成「归档引用了段页」而返回
//! `ResourceMissing`（pooled-13）。
//!
//! Cloud 的「冷进程」= 本地 cache 为空的新 `CloudStore`（同一远端对象存储），
//! 与 shard `ReopenArchive` 的 `SKIPS_OPEN` 路径等价。修复后按存活 manifest
//! 的段页证据重建保守水位：纯数据归档在段块基址之上继续分配大 value，
//! 更晚归档 / 子分支仍引用的段页不被覆盖。

use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_io::{ManifestIo, PageIo};
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_object::MemObjectStore;
use fkv_store::{AllocMode, BRANCH_FILE_RESERVE_FILE_NUM, Store};
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
const RESTART: u16 = 8;
/// 4 页 / 文件；预留块 1024 文件 = 4096 页。
const SHIFT: u8 = 2;
const SEG_SIZE: usize = 4096;
/// 4 段 / 段文件。
const SEG_SHIFT: u8 = 2;
const THRESHOLD: usize = 1024;
const BLOCK_FP: u64 = BRANCH_FILE_RESERVE_FILE_NUM << SHIFT;
const BIG_LEN: usize = 9000;
const MODES: [AllocMode; 2] = [AllocMode::Pooled, AllocMode::Append];

fn layout() -> SegmentLayout {
  SegmentLayout::new(SEG_SIZE as u32, SEG_SHIFT).expect("segment layout")
}

fn cloud_store(
  remote: &MemObjectStore,
  mode: AllocMode,
) -> Store<CloudStore<MemStore, MemObjectStore>> {
  Store::new(
    CloudStore::new(
      MemStore::with_pages_per_file(PAGE_SIZE, SHIFT).with_segment_config(SEG_SIZE, SEG_SHIFT),
      remote.clone(),
      "fkv",
      SHIFT,
    )
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

fn key(prefix: &str, index: u32) -> Vec<u8> {
  format!("{prefix}{index}").into_bytes()
}

fn val(prefix: &str, index: u32) -> Vec<u8> {
  format!("{}{index}", prefix.to_uppercase()).into_bytes()
}

fn seg_max<I: PageIo>(store: &Store<I>, tbl: &TableIdent) -> u64 {
  store
    .segment_mapper(tbl)
    .map_or(0, |mapper| mapper.max_fp_id())
}

/// main 写 a0..a7 → 建 f1 → f1 写 b0..b7 → 归档 keep → 删除 branch manifest。
async fn build_orphan(
  remote: &MemObjectStore,
  mode: AllocMode,
  tbl: &TableIdent,
) -> Store<CloudStore<MemStore, MemObjectStore>> {
  let mut main = cloud_store(remote, mode);
  for index in 0..8 {
    main
      .put(tbl, &[up(&key("a", index), &val("a", index), 1)])
      .await
      .unwrap();
  }
  main.create_branch(tbl, "f1").await.unwrap();
  drop(main);

  // 冷进程（空本地 cache）attach 到 f1 并写归档。
  let mut f1 = cloud_store(remote, mode);
  f1.prepare_attach("f1", 0).unwrap();
  f1.reopen(tbl).await.unwrap();
  for index in 0..8 {
    f1.put(tbl, &[up(&key("b", index), &val("b", index), 2)])
      .await
      .unwrap();
  }
  f1.create_archive(tbl, "keep", 0).await.unwrap();
  f1.page_io()
    .remove_manifest_file(tbl, "manifest_f1_0")
    .await
    .unwrap();
  f1
}

/// main 冷进程洪泛：孤儿归档块必须参与复用下界（sem-28），不得覆盖归档页。
async fn flood_main<I>(store: &mut Store<I>, tbl: &TableIdent)
where
  I: PageIo + ManifestIo,
{
  store.put(tbl, &[up(b"a0", b"A0-new", 3)]).await.unwrap();
  let payload = vec![b'X'; 300];
  for batch in 0..10u32 {
    let entries: Vec<_> = (0..100u32)
      .map(|i| {
        let n = batch * 100 + i;
        BatchEntry::upsert(
          format!("c{n:05}").into_bytes(),
          payload.clone(),
          4 + u64::from(n),
        )
      })
      .collect();
    store.put(tbl, &entries).await.unwrap();
  }
}

/// 孤儿归档冷重开：`term == 0` 走 main 洪泛 + f1 冷重开；`term == 3` 走 term
/// bump 变体（bump 后的水位仍须落在自己段块内，后续大 value 可写）。
async fn run_cold_reopen(remote: &MemObjectStore, mode: AllocMode, term: u64) {
  let tbl = TableIdent::new(format!("p13_cloud_cold_{mode:?}_{term}"), 1).unwrap();
  let f1 = build_orphan(remote, mode, &tbl).await;
  drop(f1);

  if term == 0 {
    let mut main = cloud_store(remote, mode);
    main.prepare_attach("main", 0).unwrap();
    main.reopen(&tbl).await.unwrap();
    let main_mapper = main.mapper(&tbl).unwrap();
    // Pooled 用复用下界保护孤儿块；Append 没有空闲池，main 的分配水位跳过
    // 整个孤儿块（sem-25 / sem-28）。
    let allocated_at = if mode == AllocMode::Pooled {
      main_mapper.reuse_floor()
    } else {
      main_mapper.max_fp_id()
    };
    assert!(
      allocated_at >= 2 * BLOCK_FP,
      "mode={mode:?}: 孤儿归档块未计入 main 的复用下界 / 分配水位 {allocated_at}"
    );
    flood_main(&mut main, &tbl).await;
  }

  let mut cold = cloud_store(remote, mode);
  assert!(
    cold.segment_mapper(&tbl).is_none() && cold.mapper(&tbl).is_none(),
    "冷 Store 不应已有表状态"
  );
  cold.prepare_attach("f1", term).unwrap();
  cold
    .reopen_archive(&tbl, "keep")
    .await
    .unwrap_or_else(|err| {
      panic!("mode={mode:?} term={term}: 生产配置下孤儿归档冷重开失败: {err:?}")
    });
  let watermark = seg_max(&cold, &tbl);
  assert!(
    (BLOCK_FP..2 * BLOCK_FP).contains(&watermark),
    "mode={mode:?} term={term}: 重建段水位越界 {watermark}"
  );
  let big = vec![b'L'; BIG_LEN];
  cold
    .put_large(&tbl, &[up(b"after-reopen", &big, 9)])
    .await
    .unwrap_or_else(|err| panic!("mode={mode:?} term={term}: 冷重开后大 value 不可写: {err:?}"));
  assert_eq!(
    cold
      .get_large(&tbl, b"after-reopen")
      .await
      .unwrap()
      .map(|read| read.value),
    Some(big),
    "mode={mode:?} term={term}: 重建水位后大 value 读回不一致"
  );
  for index in 0..8 {
    assert_eq!(
      cold.get(&tbl, &key("a", index)).await.unwrap(),
      Some(val("a", index)),
      "mode={mode:?} term={term}: 归档继承页 a{index} 丢失"
    );
    assert_eq!(
      cold.get(&tbl, &key("b", index)).await.unwrap(),
      Some(val("b", index)),
      "mode={mode:?} term={term}: 归档自身页 b{index} 丢失"
    );
  }
}

#[test]
fn cloud_orphan_data_archive_cold_reopen() {
  pollster::block_on(async {
    for mode in MODES {
      let remote = MemObjectStore::new();
      run_cold_reopen(&remote, mode, 0).await;
    }
  });
}

#[test]
fn cloud_orphan_data_archive_cold_reopen_term_bump() {
  pollster::block_on(async {
    for mode in MODES {
      let remote = MemObjectStore::new();
      run_cold_reopen(&remote, mode, 3).await;
    }
  });
}

/// main → f1 → 归档 a1 → big1 → 建子分支 f2（继承 big1 引用）→ 删 f1 的
/// branch manifest → 冷态重开 a1 → big2 → 以 f2 身份读 big1 必须完好。
///
/// 子分支 manifest 里的继承 data / 段页 id 是 f1 块内仍被引用的页；冷态重建
/// 必须扫描全部存活 manifest，不能只看本分支的归档（a1 早于 big1）。
#[test]
fn cloud_orphan_archive_child_inherited_large() {
  pollster::block_on(async {
    for mode in MODES {
      let remote = MemObjectStore::new();
      let tbl = TableIdent::new(format!("p13_cloud_child_{mode:?}"), 1).unwrap();
      let mut main = cloud_store(&remote, mode);
      main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
      main.create_branch(&tbl, "f1").await.unwrap();
      drop(main);

      let mut f1 = cloud_store(&remote, mode);
      f1.prepare_attach("f1", 0).unwrap();
      f1.reopen(&tbl).await.unwrap();
      f1.put(&tbl, &[up(b"pre", b"PRE", 2)]).await.unwrap();
      f1.create_archive(&tbl, "a1", 0).await.unwrap();
      let big1 = vec![b'1'; BIG_LEN];
      f1.put_large(&tbl, &[up(b"big1", &big1, 3)]).await.unwrap();
      f1.create_branch(&tbl, "f2").await.unwrap();
      let f1_after_big1 = f1.mapper(&tbl).unwrap().max_fp_id();
      f1.page_io()
        .remove_manifest_file(&tbl, "manifest_f1_0")
        .await
        .unwrap();
      drop(f1);

      let mut cold = cloud_store(&remote, mode);
      cold.prepare_attach("f1", 0).unwrap();
      cold.reopen_archive(&tbl, "a1").await.unwrap();
      assert!(
        cold.mapper(&tbl).unwrap().max_fp_id() >= f1_after_big1,
        "mode={mode:?}: 冷态 data 水位低于子分支继承的引用"
      );
      assert!(
        seg_max(&cold, &tbl) > BLOCK_FP,
        "mode={mode:?}: 冷态段水位没有覆盖子分支继承的段页"
      );
      let big2 = vec![b'2'; BIG_LEN];
      cold
        .put_large(&tbl, &[up(b"big2", &big2, 4)])
        .await
        .unwrap();

      let mut f2 = cloud_store(&remote, mode);
      f2.prepare_attach("f2", 0).unwrap();
      f2.reopen(&tbl).await.unwrap();
      assert_eq!(
        f2.get_large(&tbl, b"big1")
          .await
          .unwrap()
          .map(|read| read.value),
        Some(big1),
        "mode={mode:?}: 冷态重开 f1 的 a1 后覆盖了子分支 f2 继承的段页"
      );
    }
  });
}

/// main → f1 → 归档 a1 → big1 → 归档 a2 → 删 branch manifest → 冷态重开 a1：
/// 重建的 data / segment 水位都不得低于 a2，后续 big2 不得覆盖 a2 的 big1。
#[test]
fn cloud_orphan_archive_keeps_later_archive_large() {
  pollster::block_on(async {
    for mode in MODES {
      let remote = MemObjectStore::new();
      let tbl = TableIdent::new(format!("p13_cloud_later_{mode:?}"), 1).unwrap();
      let mut main = cloud_store(&remote, mode);
      main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
      main.create_branch(&tbl, "f1").await.unwrap();
      drop(main);

      let mut f1 = cloud_store(&remote, mode);
      f1.prepare_attach("f1", 0).unwrap();
      f1.reopen(&tbl).await.unwrap();
      f1.put(&tbl, &[up(b"pre", b"PRE", 2)]).await.unwrap();
      f1.create_archive(&tbl, "a1", 0).await.unwrap();
      let big1 = vec![b'1'; BIG_LEN];
      f1.put_large(&tbl, &[up(b"big1", &big1, 3)]).await.unwrap();
      f1.create_archive(&tbl, "a2", 0).await.unwrap();
      let a2_seg = seg_max(&f1, &tbl);
      let a2_max = f1.mapper(&tbl).unwrap().max_fp_id();
      f1.page_io()
        .remove_manifest_file(&tbl, "manifest_f1_0")
        .await
        .unwrap();
      drop(f1);

      let mut cold = cloud_store(&remote, mode);
      cold.prepare_attach("f1", 0).unwrap();
      cold.reopen_archive(&tbl, "a1").await.unwrap();
      assert!(
        seg_max(&cold, &tbl) >= a2_seg,
        "mode={mode:?}: 冷态重建段水位低于 a2 {a2_seg}"
      );
      assert!(
        cold.mapper(&tbl).unwrap().max_fp_id() >= a2_max,
        "mode={mode:?}: 冷态重建 data 水位低于 a2 {a2_max}"
      );
      let big2 = vec![b'2'; BIG_LEN];
      cold
        .put_large(&tbl, &[up(b"big2", &big2, 4)])
        .await
        .unwrap();
      cold.reopen_archive(&tbl, "a2").await.unwrap();
      assert_eq!(
        cold
          .get_large(&tbl, b"big1")
          .await
          .unwrap()
          .map(|read| read.value),
        Some(big1),
        "mode={mode:?}: 冷态重开 a1 后续写覆盖了 a2 的 big1"
      );
    }
  });
}
