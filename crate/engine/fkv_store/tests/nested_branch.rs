//! sem-24：在 branch 上再建 branch 后，父 branch 必须仍能在**自己的预留块内**
//! 继续写，而不是被 `create_branch` 的 reserve 推过 `branch_file_limit` 后恒
//! `OutOfSpace`。
//!
//! 缺陷链路：
//! - `create_branch` 计算子块 `[base, base + 1024)` 后，对当前（父）mapper
//!   `reserve_until(child_limit)`，把父分支高水位直接抬到子块末尾；
//! - 父分支的 `branch_file_limit` 仍是自己块的末尾（不随 reserve 更新），
//!   于是 `write.rs` 的上限检查（`mapper_max >= limit`）恒成立；
//! - 父分支 manifest 也按被抬高的 max 落盘，重开（pooled-09 按区间链派生
//!   limit）后仍然 `max > limit`，不会自愈。
//!
//! 修复后：受限父分支的分配水位与块上限都留在自己的块内（子块本来就在父块
//! 之外），父分支块写满才 `OutOfSpace`；兄弟子分支的块由盘上已有子分支
//! manifest 的区间链反推，各占一个 1024 文件块。复用下界抬到父分支当前高
//! 水位，保护子分支继承的活页（父表后续 COW 退役的继承页不回池）。

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_io_sim::SimStore;
use fkv_large::SegmentLayout;
use fkv_recovery::ReplayState;
use fkv_store::{AllocMode, BRANCH_FILE_RESERVE_FILE_NUM, Store};
use fkv_types::{
  DEFAULT_PAGES_PER_FILE_SHIFT, MAIN_BRANCH_NAME, TableIdent, parse_manifest_file_name,
};

const PAGE_SIZE: usize = 512;
const RESTART: u16 = 8;
const MODES: [AllocMode; 2] = [AllocMode::Pooled, AllocMode::Append];
const SEGMENT_SIZE: usize = 4096;
/// `SegmentLayout::new(..., 2)` 的段文件位移。
const SEGMENT_SHIFT: u8 = 2;
const LARGE_THRESHOLD: usize = 1024;

fn new_store(mode: AllocMode, shift: u8) -> Store<MemStore> {
  Store::new(
    MemStore::with_pages_per_file(PAGE_SIZE, shift),
    PAGE_SIZE,
    RESTART,
  )
  .with_pages_per_file_shift(shift)
  .with_alloc_mode(mode)
}

fn reopen_store(mode: AllocMode, io: MemStore, shift: u8) -> Store<MemStore> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(shift)
    .with_alloc_mode(mode)
}

fn large_store(io: MemStore, shift: u8) -> Store<MemStore> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(shift)
    .with_large_value(
      SegmentLayout::new(SEGMENT_SIZE as u32, 2).unwrap(),
      LARGE_THRESHOLD,
    )
    .with_alloc_mode(AllocMode::Pooled)
}

fn up(key: &[u8], value: &[u8], ts: u64) -> BatchEntry {
  BatchEntry::upsert(key.to_vec(), value.to_vec(), ts)
}

fn pattern(len: usize, seed: u8) -> Vec<u8> {
  (0..len)
    .map(|index| seed.wrapping_add((index % 251) as u8))
    .collect()
}

/// main → f1 → f2：父分支 f1 建完子分支后仍可在自己块内写；f2 在父块之外；
/// 三向写读、reopen 后仍正确。
#[compio::test]
async fn nested_branch_parent_stays_writable() {
  for mode in MODES {
    let tbl = TableIdent::new("s24_nested_write", 1).unwrap();
    let shift = DEFAULT_PAGES_PER_FILE_SHIFT;
    let mut main = new_store(mode, shift);
    main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
    main.create_branch(&tbl, "f1").await.unwrap();
    let io = main.into_io();

    // f1 打开后先写一笔，再建 f2。
    let mut f1 = reopen_store(mode, io, shift);
    f1.open_branch(&tbl, "f1", 0).await.unwrap();
    let f1_limit = f1.branch_file_limit().expect("f1 必须有上限");
    f1.put(&tbl, &[up(b"f1a", b"F1A", 2)]).await.unwrap();
    let max_before = f1.mapper(&tbl).unwrap().max_fp_id();
    assert!(max_before < f1_limit);
    f1.create_branch(&tbl, "f2").await.unwrap();
    let max_after = f1.mapper(&tbl).unwrap().max_fp_id();
    assert_eq!(
      max_before, max_after,
      "create_branch 不得移动受限父分支高水位 mode={mode:?}"
    );
    assert!(max_after < f1_limit);
    assert_eq!(f1.branch_file_limit(), Some(f1_limit));
    if mode == AllocMode::Pooled {
      assert_eq!(
        f1.mapper(&tbl).unwrap().reuse_floor(),
        max_after,
        "子分支继承页必须以父分支高水位为复用下界 mode={mode:?}"
      );
    }
    // 建完子分支后父分支继续写自己的块。
    f1.put(&tbl, &[up(b"f1b", b"F1B", 3)]).await.unwrap();
    assert_eq!(f1.get(&tbl, b"f1b").await.unwrap(), Some(b"F1B".to_vec()));
    assert_eq!(f1.get(&tbl, b"f1a").await.unwrap(), Some(b"F1A".to_vec()));
    assert!(f1.mapper(&tbl).unwrap().max_fp_id() <= f1_limit);
    let io = f1.into_io();

    // 子分支块在父块之外，可正常写读。
    let mut f2 = reopen_store(mode, io, shift);
    f2.open_branch(&tbl, "f2", 0).await.unwrap();
    let f2_limit = f2.branch_file_limit().expect("f2 必须有上限");
    assert!(
      f2.mapper(&tbl).unwrap().max_fp_id() >= f1_limit,
      "子分支块起点落在父块内 mode={mode:?}"
    );
    assert_eq!(
      f2_limit,
      f2.mapper(&tbl).unwrap().max_fp_id() + (BRANCH_FILE_RESERVE_FILE_NUM << shift),
      "子分支块大小应为一个预留块 mode={mode:?}"
    );
    f2.put(&tbl, &[up(b"f2a", b"F2A", 4)]).await.unwrap();
    assert_eq!(f2.get(&tbl, b"f2a").await.unwrap(), Some(b"F2A".to_vec()));
    assert_eq!(f2.get(&tbl, b"f1b").await.unwrap(), None);
    let io = f2.into_io();

    // 重开父分支：manifest 里的 max 未越界，父分支仍可写；子分支数据不受影响。
    let mut f1 = reopen_store(mode, io, shift);
    f1.open_branch(&tbl, "f1", 0).await.unwrap();
    assert_eq!(f1.branch_file_limit(), Some(f1_limit));
    assert!(f1.mapper(&tbl).unwrap().max_fp_id() < f1_limit);
    if mode == AllocMode::Pooled {
      assert_eq!(
        f1.mapper(&tbl).unwrap().reuse_floor(),
        f1.mapper(&tbl).unwrap().max_fp_id(),
        "重开后复用下界必须覆盖子分支继承页 mode={mode:?}"
      );
    }
    f1.put(&tbl, &[up(b"f1c", b"F1C", 5)]).await.unwrap();
    assert_eq!(f1.get(&tbl, b"f1c").await.unwrap(), Some(b"F1C".to_vec()));
    // 子分支的写不回流父分支。
    assert_eq!(f1.get(&tbl, b"f2a").await.unwrap(), None);
    let io = f1.into_io();

    // 重开子分支：继承数据与自己的写都在。
    let mut f2 = reopen_store(mode, io, shift);
    f2.open_branch(&tbl, "f2", 0).await.unwrap();
    assert_eq!(f2.get(&tbl, b"f2a").await.unwrap(), Some(b"F2A".to_vec()));
    assert_eq!(f2.get(&tbl, b"f1c").await.unwrap(), None);
    let io = f2.into_io();

    // 回 main：父表数据完好，看不到分支写。
    let mut main = reopen_store(mode, io, shift);
    main.prepare_attach("main", 0).unwrap();
    main.reopen(&tbl).await.unwrap();
    assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(main.get(&tbl, b"f1b").await.unwrap(), None);
    assert_eq!(main.get(&tbl, b"f2a").await.unwrap(), None);
  }
}

/// 父分支块写满：块内写成功，到 `max == branch_file_limit` 后下一次写
/// `OutOfSpace` 且不移动水位；reopen 后仍是满块状态；子分支照常可写。
#[compio::test]
async fn nested_branch_parent_block_full_out_of_space() {
  let tbl = TableIdent::new("s24_nested_full", 1).unwrap();
  let shift = 0u8;
  let mode = AllocMode::Pooled;
  let mut main = new_store(mode, shift);
  main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
  main.create_branch(&tbl, "f1").await.unwrap();
  let io = main.into_io();

  let mut f1 = reopen_store(mode, io, shift);
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  let limit = f1.branch_file_limit().expect("f1 上限");
  f1.put(&tbl, &[up(b"f1a", b"F1A", 2)]).await.unwrap();
  f1.create_branch(&tbl, "f2").await.unwrap();
  assert_eq!(f1.branch_file_limit(), Some(limit));
  assert!(f1.mapper(&tbl).unwrap().max_fp_id() < limit);

  // 块内继续写，直到本块写满（`shift=0` 时块 = 1024 页）。
  let mut reached = false;
  for i in 0..8000u32 {
    if f1.mapper(&tbl).unwrap().max_fp_id() >= limit {
      reached = true;
      break;
    }
    let key = format!("k{i:05}").into_bytes();
    match f1
      .put(&tbl, &[BatchEntry::upsert(key, vec![b'V'; 200], 3)])
      .await
    {
      Ok(()) => assert!(f1.mapper(&tbl).unwrap().max_fp_id() <= limit),
      Err(KvError::OutOfSpace) => {
        reached = true;
        break;
      }
      Err(err) => panic!("意外的写入错误: {err:?}"),
    }
  }
  assert!(reached, "父分支块内写没能写满（布局变化？）");
  assert_eq!(f1.mapper(&tbl).unwrap().max_fp_id(), limit);

  let before = f1.mapper(&tbl).unwrap().max_fp_id();
  assert_eq!(
    f1.put(&tbl, &[up(b"zz", b"Z", 4)]).await.unwrap_err(),
    KvError::OutOfSpace
  );
  assert_eq!(
    f1.mapper(&tbl).unwrap().max_fp_id(),
    before,
    "满块后的失败批不得再分配越界页"
  );
  let io = f1.into_io();

  // 重开父分支：limit 仍由区间链派生为同一块末尾，写继续被拒绝（不自愈成越界）。
  let mut f1 = reopen_store(mode, io, shift);
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  assert_eq!(f1.branch_file_limit(), Some(limit));
  assert_eq!(f1.mapper(&tbl).unwrap().max_fp_id(), limit);
  assert_eq!(
    f1.put(&tbl, &[up(b"zz2", b"Z", 5)]).await.unwrap_err(),
    KvError::OutOfSpace
  );
  let io = f1.into_io();

  // 子分支不受父分支满块影响。
  let mut f2 = reopen_store(mode, io, shift);
  f2.open_branch(&tbl, "f2", 0).await.unwrap();
  f2.put(&tbl, &[up(b"f2a", b"F2A", 6)]).await.unwrap();
  assert_eq!(f2.get(&tbl, b"f2a").await.unwrap(), Some(b"F2A".to_vec()));
  let io = f2.into_io();

  // 父分支满块期间写出的块内数据在重开后完整。
  let mut f1 = reopen_store(mode, io, shift);
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  assert_eq!(f1.get(&tbl, b"f1a").await.unwrap(), Some(b"F1A".to_vec()));
  assert_eq!(
    f1.get(&tbl, b"k00000").await.unwrap(),
    Some(vec![b'V'; 200])
  );
  let io = f1.into_io();

  let mut main = reopen_store(mode, io, shift);
  main.prepare_attach("main", 0).unwrap();
  main.reopen(&tbl).await.unwrap();
  assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
}

/// `prepare_attach` + `reopen` 安装路径（非 `open_branch`）同样保留嵌套语义。
#[compio::test]
async fn nested_branch_reopen_via_manifest_keeps_parent_writable() {
  let tbl = TableIdent::new("s24_nested_reopen", 1).unwrap();
  let shift = DEFAULT_PAGES_PER_FILE_SHIFT;
  let mode = AllocMode::Pooled;
  let mut main = new_store(mode, shift);
  main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
  main.create_branch(&tbl, "f1").await.unwrap();
  let io = main.into_io();

  let mut f1 = reopen_store(mode, io, shift);
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  let f1_limit = f1.branch_file_limit().unwrap();
  f1.put(&tbl, &[up(b"f1a", b"F1A", 2)]).await.unwrap();
  f1.create_branch(&tbl, "f2").await.unwrap();
  f1.put(&tbl, &[up(b"f1b", b"F1B", 3)]).await.unwrap();
  let io = f1.into_io();

  let mut f1 = reopen_store(mode, io, shift);
  f1.prepare_attach("f1", 0).unwrap();
  f1.reopen(&tbl).await.unwrap();
  assert_eq!(f1.branch_file_limit(), Some(f1_limit));
  assert!(f1.mapper(&tbl).unwrap().max_fp_id() < f1_limit);
  f1.put(&tbl, &[up(b"f1c", b"F1C", 4)]).await.unwrap();
  assert_eq!(f1.get(&tbl, b"f1b").await.unwrap(), Some(b"F1B".to_vec()));
  let io = f1.into_io();

  let mut f2 = reopen_store(mode, io, shift);
  f2.prepare_attach("f2", 0).unwrap();
  f2.reopen(&tbl).await.unwrap();
  f2.put(&tbl, &[up(b"f2a", b"F2A", 5)]).await.unwrap();
  // f2 继承创建时的父快照：f1a 可见；创建之后的 f1b / f1c 不可见。
  assert_eq!(f2.get(&tbl, b"f1a").await.unwrap(), Some(b"F1A".to_vec()));
  assert!(f2.get(&tbl, b"f1b").await.unwrap().is_none());
  assert!(f2.get(&tbl, b"f1c").await.unwrap().is_none());
  let io = f2.into_io();

  let mut f2 = reopen_store(mode, io, shift);
  f2.prepare_attach("f2", 0).unwrap();
  f2.reopen(&tbl).await.unwrap();
  assert_eq!(f2.get(&tbl, b"f2a").await.unwrap(), Some(b"F2A".to_vec()));
}

/// 同一受限父分支下的两个子分支必须落在不同预留块，写读互不覆盖。
#[compio::test]
async fn nested_sibling_children_use_distinct_blocks() {
  for mode in MODES {
    let tbl = TableIdent::new("s24_nested_siblings", 1).unwrap();
    let shift = DEFAULT_PAGES_PER_FILE_SHIFT;
    let mut main = new_store(mode, shift);
    main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
    main.create_branch(&tbl, "f1").await.unwrap();
    let io = main.into_io();

    let mut f1 = reopen_store(mode, io, shift);
    f1.open_branch(&tbl, "f1", 0).await.unwrap();
    f1.put(&tbl, &[up(b"f1a", b"F1A", 2)]).await.unwrap();
    f1.create_branch(&tbl, "f2").await.unwrap();
    assert!(f1.mapper(&tbl).unwrap().max_fp_id() < f1.branch_file_limit().unwrap());
    let io = f1.into_io();

    // 重开父分支后再建第二个子分支：已承诺子块从盘上 manifest 的区间链恢复。
    let mut f1 = reopen_store(mode, io, shift);
    f1.open_branch(&tbl, "f1", 0).await.unwrap();
    f1.create_branch(&tbl, "f2b").await.unwrap();
    // 两次 create_branch 都不移动父分支水位。
    assert!(f1.mapper(&tbl).unwrap().max_fp_id() < f1.branch_file_limit().unwrap());
    f1.put(&tbl, &[up(b"f1b", b"F1B", 3)]).await.unwrap();
    let io = f1.into_io();

    let mut f2 = reopen_store(mode, io, shift);
    f2.open_branch(&tbl, "f2", 0).await.unwrap();
    let f2_base = f2.mapper(&tbl).unwrap().max_fp_id();
    let f2_limit = f2.branch_file_limit().unwrap();
    f2.put(&tbl, &[up(b"f2a", b"F2A", 4)]).await.unwrap();
    let io = f2.into_io();

    let mut f2b = reopen_store(mode, io, shift);
    f2b.open_branch(&tbl, "f2b", 0).await.unwrap();
    let f2b_base = f2b.mapper(&tbl).unwrap().max_fp_id();
    let f2b_limit = f2b.branch_file_limit().unwrap();
    assert!(
      f2b_base >= f2_limit,
      "兄弟子分支块重叠 mode={mode:?} f2_base={f2_base} f2_limit={f2_limit} \
       f2b_base={f2b_base}"
    );
    f2b.put(&tbl, &[up(b"f2b", b"F2B", 5)]).await.unwrap();
    let io = f2b.into_io();

    // 两个兄弟各自的数据在重开后都还在，父表不受影响。
    let mut f2 = reopen_store(mode, io, shift);
    f2.open_branch(&tbl, "f2", 0).await.unwrap();
    assert_eq!(f2.branch_file_limit(), Some(f2_limit));
    assert_eq!(f2.get(&tbl, b"f2a").await.unwrap(), Some(b"F2A".to_vec()));
    assert_eq!(f2.get(&tbl, b"f2b").await.unwrap(), None);
    let io = f2.into_io();

    let mut f2b = reopen_store(mode, io, shift);
    f2b.open_branch(&tbl, "f2b", 0).await.unwrap();
    assert_eq!(f2b.branch_file_limit(), Some(f2b_limit));
    assert_eq!(f2b.get(&tbl, b"f2b").await.unwrap(), Some(b"F2B".to_vec()));
    let io = f2b.into_io();

    let mut f1 = reopen_store(mode, io, shift);
    f1.open_branch(&tbl, "f1", 0).await.unwrap();
    assert_eq!(f1.get(&tbl, b"f1b").await.unwrap(), Some(b"F1B".to_vec()));
    let io = f1.into_io();

    let mut main = reopen_store(mode, io, shift);
    main.prepare_attach("main", 0).unwrap();
    main.reopen(&tbl).await.unwrap();
    assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(main.get(&tbl, b"f1b").await.unwrap(), None);
    assert_eq!(main.get(&tbl, b"f2a").await.unwrap(), None);
  }
}

/// 父分支建子分支后继续覆盖写：父表 COW 退役的继承页不得回池，子分支快照
/// 必须保持创建时刻的值（复用下界 = 父分支当前高水位）。
#[compio::test]
async fn nested_parent_cow_keeps_child_snapshot() {
  for mode in MODES {
    let tbl = TableIdent::new("s24_nested_cow", 1).unwrap();
    let shift = DEFAULT_PAGES_PER_FILE_SHIFT;
    let mut main = new_store(mode, shift);
    main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
    main.create_branch(&tbl, "f1").await.unwrap();
    let io = main.into_io();

    let mut f1 = reopen_store(mode, io, shift);
    f1.open_branch(&tbl, "f1", 0).await.unwrap();
    f1.put(&tbl, &[up(b"k", b"V1", 2)]).await.unwrap();
    f1.create_branch(&tbl, "f2").await.unwrap();
    // 覆盖写 k：旧页被 COW 退役；后续写入会触发 retired 回收，若下界没有
    // 覆盖到创建时高水位，旧页会回池并被新页覆盖，f2 的继承值变成 V2。
    f1.put(&tbl, &[up(b"k", b"V2", 3)]).await.unwrap();
    for i in 0..64u32 {
      let key = format!("p{i:04}").into_bytes();
      f1.put(&tbl, &[up(&key, b"pad", 4 + u64::from(i))])
        .await
        .unwrap();
    }
    assert_eq!(f1.get(&tbl, b"k").await.unwrap(), Some(b"V2".to_vec()));
    let io = f1.into_io();

    let mut f2 = reopen_store(mode, io, shift);
    f2.open_branch(&tbl, "f2", 0).await.unwrap();
    assert_eq!(
      f2.get(&tbl, b"k").await.unwrap(),
      Some(b"V1".to_vec()),
      "父分支 COW 退役页回池覆盖了子分支快照 mode={mode:?}"
    );
  }
}

/// segment 侧同构：父分支建完子分支后仍能写自己的段块，子分支段块在父块之外。
#[compio::test]
async fn nested_branch_segment_stays_in_own_block() {
  let tbl = TableIdent::new("s24_nested_segment", 1).unwrap();
  let shift = DEFAULT_PAGES_PER_FILE_SHIFT;
  let mut main = large_store(MemStore::with_pages_per_file(PAGE_SIZE, shift), shift);
  let big_a = pattern(SEGMENT_SIZE * 2, 1);
  main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
  main
    .put_large(&tbl, &[up(b"big-main", &big_a, 2)])
    .await
    .unwrap();
  main.create_branch(&tbl, "f1").await.unwrap();
  let io = main.into_io();

  let mut f1 = reopen_store_with_large(io, shift);
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  let f1_seg_limit = f1.branch_segment_limit().expect("f1 段上限");
  let big = pattern(SEGMENT_SIZE * 3, 2);
  f1.put_large(&tbl, &[up(b"big-f1", &big, 3)]).await.unwrap();
  let seg_before = f1.segment_mapper(&tbl).unwrap().max_fp_id();
  assert!(seg_before < f1_seg_limit);
  f1.create_branch(&tbl, "f2").await.unwrap();
  assert_eq!(
    f1.segment_mapper(&tbl).unwrap().max_fp_id(),
    seg_before,
    "create_branch 不得移动父分支段水位"
  );
  assert_eq!(f1.branch_segment_limit(), Some(f1_seg_limit));
  let big2 = pattern(SEGMENT_SIZE * 2 + 7, 3);
  f1.put_large(&tbl, &[up(b"big-f1b", &big2, 4)])
    .await
    .unwrap();
  assert_eq!(
    f1.get_large(&tbl, b"big-f1b").await.unwrap().unwrap().value,
    big2
  );
  assert_eq!(
    f1.get_large(&tbl, b"big-f1").await.unwrap().unwrap().value,
    big
  );
  let io = f1.into_io();

  let mut f2 = reopen_store_with_large(io, shift);
  f2.open_branch(&tbl, "f2", 0).await.unwrap();
  let f2_seg_limit = f2.branch_segment_limit().expect("f2 段上限");
  assert!(
    f2.segment_mapper(&tbl).unwrap().max_fp_id() >= f1_seg_limit,
    "子分支段块落在父段块内"
  );
  assert_eq!(
    f2_seg_limit,
    f2.segment_mapper(&tbl).unwrap().max_fp_id() + (BRANCH_FILE_RESERVE_FILE_NUM << 2)
  );
  let big_f2 = pattern(SEGMENT_SIZE * 2, 4);
  f2.put_large(&tbl, &[up(b"big-f2", &big_f2, 5)])
    .await
    .unwrap();
  assert_eq!(
    f2.get_large(&tbl, b"big-f2").await.unwrap().unwrap().value,
    big_f2
  );
  let io = f2.into_io();

  // 重开父分支：段块内继续可写，继承的段数据可读。
  let mut f1 = reopen_store_with_large(io, shift);
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  assert_eq!(f1.branch_segment_limit(), Some(f1_seg_limit));
  assert_eq!(
    f1.get_large(&tbl, b"big-f1").await.unwrap().unwrap().value,
    big
  );
  let big3 = pattern(SEGMENT_SIZE + 11, 5);
  f1.put_large(&tbl, &[up(b"big-f1c", &big3, 6)])
    .await
    .unwrap();
  assert_eq!(
    f1.get_large(&tbl, b"big-f1c").await.unwrap().unwrap().value,
    big3
  );
  let io = f1.into_io();

  let mut main = reopen_store_with_large(io, shift);
  main.prepare_attach("main", 0).unwrap();
  main.reopen(&tbl).await.unwrap();
  assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(
    main
      .get_large(&tbl, b"big-main")
      .await
      .unwrap()
      .unwrap()
      .value,
    big_a
  );
}

fn reopen_store_with_large(io: MemStore, shift: u8) -> Store<MemStore> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(shift)
    .with_large_value(
      SegmentLayout::new(SEGMENT_SIZE as u32, 2).unwrap(),
      LARGE_THRESHOLD,
    )
    .with_alloc_mode(AllocMode::Pooled)
}

// pooled-10：块位全局单调

/// 从盘上全部活跃 branch manifest 反推预留块 `(branch, base, end)`（页 id）。
///
/// 块基址从区间链里的父区间上界派生（与 `branch_block_base` 同口径），不读
/// 移动的 `max_fp_id`：分支写满自己的块是合法终态。
async fn manifest_block_li(
  io: &MemStore,
  tbl: &TableIdent,
  shift: u8,
  segment: bool,
) -> Vec<(String, u64, u64)> {
  let mut block_li = Vec::new();
  for name in io.list_manifest_file_name_li(tbl).await.unwrap() {
    let Some((name_branch, _term, tag)) = parse_manifest_file_name(&name) else {
      continue;
    };
    // 只取 active branch manifest；归档与 branch manifest 的块位相同。
    if tag.is_some() || name_branch == MAIN_BRANCH_NAME {
      continue;
    }
    let bytes = io.read_manifest_file(tbl, &name).await.unwrap().unwrap();
    let state = ReplayState::from_snapshot(&bytes).unwrap();
    let metadata = &state.branch_metadata;
    let own_idx = metadata
      .file_range_li
      .iter()
      .position(|range| range.branch_name.as_ref() == name_branch)
      .unwrap_or_else(|| panic!("branch manifest 缺少本分支区间 name={name}"));
    assert!(own_idx > 0, "branch manifest 缺少父区间前缀 name={name}");
    let own = &metadata.file_range_li[own_idx];
    if segment && own.max_segment_file_id == 0 {
      continue;
    }
    let block_shift = if segment { SEGMENT_SHIFT } else { shift };
    let parent = &metadata.file_range_li[own_idx - 1];
    let parent_max = if segment {
      parent.max_segment_file_id
    } else {
      parent.max_file_id
    };
    let base_file =
      (parent_max + 1).div_ceil(BRANCH_FILE_RESERVE_FILE_NUM) * BRANCH_FILE_RESERVE_FILE_NUM;
    block_li.push((
      name_branch.to_owned(),
      base_file << block_shift,
      (base_file + BRANCH_FILE_RESERVE_FILE_NUM) << block_shift,
    ));
  }
  block_li.sort();
  block_li
}

/// 块基址两两不同、半开区间两两不相交。
fn assert_blocks_disjoint(block_li: &[(String, u64, u64)]) {
  let mut base_li: Vec<u64> = block_li.iter().map(|(_, base, _)| *base).collect();
  let base_cnt = base_li.len();
  base_li.sort_unstable();
  base_li.dedup();
  assert_eq!(base_cnt, base_li.len(), "块基址重复: {block_li:?}");
  for (index, left) in block_li.iter().enumerate() {
    for right in &block_li[index + 1..] {
      assert!(
        left.2 <= right.1 || right.2 <= left.1,
        "块区间重叠: {} [{},{}) vs {} [{},{})",
        left.0,
        left.1,
        left.2,
        right.0,
        right.1,
        right.2
      );
    }
  }
}

fn block_of<'a>(block_li: &'a [(String, u64, u64)], branch: &str) -> &'a (String, u64, u64) {
  block_li
    .iter()
    .find(|(name, ..)| name == branch)
    .unwrap_or_else(|| panic!("缺少 branch 块 branch={branch} blocks={block_li:?}"))
}

async fn put_pad(store: &mut Store<MemStore>, tbl: &TableIdent, prefix: &str, value: u8, ts: u64) {
  for index in 0..80u32 {
    let key = format!("{prefix}{index:03}").into_bytes();
    store
      .put(tbl, &[up(&key, &[value; 120], ts + u64::from(index))])
      .await
      .unwrap();
  }
}

async fn assert_pad(
  store: &Store<MemStore>,
  tbl: &TableIdent,
  prefix: &str,
  value: u8,
  mode: AllocMode,
) {
  for index in 0..80u32 {
    let key = format!("{prefix}{index:03}").into_bytes();
    assert_eq!(
      store.get(tbl, &key).await.unwrap(),
      Some(vec![value; 120]),
      "数据丢失 prefix={prefix} key={key:?} mode={mode:?}"
    );
  }
}

/// pooled-10 主复现：main → f1；f1 → f2 / f3；f2 → g1。
///
/// 修复前 f3 与 g1 的块基址相同：Pooled 下两个分支 `floor == limit`，
/// `create_branch` 返回 Ok 但首批写 `OutOfSpace`；Append 下 g1 覆盖 f3 的
/// 物理页（80/80 丢失）。修复后块位全局单调，f3 / g1 都可写且数据完整。
#[compio::test]
async fn nested_sibling_and_grandchild_blocks_disjoint() {
  for mode in MODES {
    let tbl = TableIdent::new("p10_sibling_grandchild", 1).unwrap();
    let shift = DEFAULT_PAGES_PER_FILE_SHIFT;
    let mut main = new_store(mode, shift);
    main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
    main.create_branch(&tbl, "f1").await.unwrap();
    let io = main.into_io();

    let mut f1 = reopen_store(mode, io, shift);
    f1.open_branch(&tbl, "f1", 0).await.unwrap();
    f1.put(&tbl, &[up(b"f1a", b"F1A", 2)]).await.unwrap();
    f1.create_branch(&tbl, "f2").await.unwrap();
    let io = f1.into_io();

    // 重开父分支后再建 f2 的直接兄弟 f3。
    let mut f1 = reopen_store(mode, io, shift);
    f1.open_branch(&tbl, "f1", 0).await.unwrap();
    f1.create_branch(&tbl, "f3").await.unwrap();
    f1.put(&tbl, &[up(b"f1b", b"F1B", 3)]).await.unwrap();
    let io = f1.into_io();

    // f3 先写 80 条：Append 下这段数据会被后建的 g1 覆盖。
    let mut f3 = reopen_store(mode, io, shift);
    f3.open_branch(&tbl, "f3", 0).await.unwrap();
    put_pad(&mut f3, &tbl, "s", b'S', 10).await;
    let io = f3.into_io();

    // f2 在 f3 之后建孙分支 g1。
    let mut f2 = reopen_store(mode, io, shift);
    f2.open_branch(&tbl, "f2", 0).await.unwrap();
    f2.create_branch(&tbl, "g1").await.unwrap();
    let io = f2.into_io();

    let mut g1 = reopen_store(mode, io, shift);
    g1.open_branch(&tbl, "g1", 0).await.unwrap();
    let g1_base = g1.mapper(&tbl).unwrap().max_fp_id();
    let g1_limit = g1.branch_file_limit().unwrap();
    put_pad(&mut g1, &tbl, "g", b'G', 100).await;
    let io = g1.into_io();

    let block_li = manifest_block_li(&io, &tbl, shift, false).await;
    assert_blocks_disjoint(&block_li);
    let f2_block = block_of(&block_li, "f2");
    let f3_block = block_of(&block_li, "f3");
    let g1_block = block_of(&block_li, "g1");
    assert!(
      f3_block.1 >= f2_block.2,
      "f2 / f3 兄弟块重叠 mode={mode:?} blocks={block_li:?}"
    );
    assert!(
      g1_block.1 >= f3_block.2,
      "g1 块落在 f3 块内 mode={mode:?} blocks={block_li:?}"
    );
    assert_eq!(g1_base, g1_block.1);
    assert_eq!(g1_limit, g1_block.2);

    // f3 重开：80 条仍在（Append 不丢），且仍可写（Pooled 不是满块）。
    let mut f3 = reopen_store(mode, io, shift);
    f3.open_branch(&tbl, "f3", 0).await.unwrap();
    let f3_limit = f3.branch_file_limit().unwrap();
    assert_eq!(f3_limit, f3_block.2);
    assert!(f3.mapper(&tbl).unwrap().max_fp_id() <= f3_limit);
    if mode == AllocMode::Pooled {
      assert!(
        f3.mapper(&tbl).unwrap().reuse_floor() < f3_limit,
        "f3 被 g1 推成满块 mode={mode:?}"
      );
    }
    assert_pad(&f3, &tbl, "s", b'S', mode).await;
    f3.put(&tbl, &[up(b"f3x", b"X", 999)]).await.unwrap();
    let io = f3.into_io();

    // g1 自己的 80 条也在。
    let mut g1 = reopen_store(mode, io, shift);
    g1.open_branch(&tbl, "g1", 0).await.unwrap();
    assert_pad(&g1, &tbl, "g", b'G', mode).await;
    g1.put(&tbl, &[up(b"g1x", b"X", 1000)]).await.unwrap();
  }
}

/// main 级同构：main → f1、f0；f1 → f2 → g1。修复前孙分支 g1 与叔分支 f0
/// 同块；修复后两者不相交且都能写。
#[compio::test]
async fn main_child_and_grandchild_blocks_disjoint() {
  for mode in MODES {
    let tbl = TableIdent::new("p10_uncle_grandchild", 1).unwrap();
    let shift = DEFAULT_PAGES_PER_FILE_SHIFT;
    let mut main = new_store(mode, shift);
    main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
    main.create_branch(&tbl, "f1").await.unwrap();
    main.create_branch(&tbl, "f0").await.unwrap();
    let io = main.into_io();

    let mut f0 = reopen_store(mode, io, shift);
    f0.open_branch(&tbl, "f0", 0).await.unwrap();
    put_pad(&mut f0, &tbl, "u", b'U', 10).await;
    let io = f0.into_io();

    let mut f1 = reopen_store(mode, io, shift);
    f1.open_branch(&tbl, "f1", 0).await.unwrap();
    f1.put(&tbl, &[up(b"f1a", b"F1A", 2)]).await.unwrap();
    f1.create_branch(&tbl, "f2").await.unwrap();
    let io = f1.into_io();

    let mut f2 = reopen_store(mode, io, shift);
    f2.open_branch(&tbl, "f2", 0).await.unwrap();
    f2.put(&tbl, &[up(b"f2a", b"F2A", 3)]).await.unwrap();
    f2.create_branch(&tbl, "g1").await.unwrap();
    let io = f2.into_io();

    let mut g1 = reopen_store(mode, io, shift);
    g1.open_branch(&tbl, "g1", 0).await.unwrap();
    let g1_base = g1.mapper(&tbl).unwrap().max_fp_id();
    let g1_limit = g1.branch_file_limit().unwrap();
    put_pad(&mut g1, &tbl, "g", b'G', 100).await;
    let io = g1.into_io();

    let block_li = manifest_block_li(&io, &tbl, shift, false).await;
    assert_blocks_disjoint(&block_li);
    let f0_block = block_of(&block_li, "f0");
    let g1_block = block_of(&block_li, "g1");
    assert!(
      g1_block.1 >= f0_block.2 || f0_block.1 >= g1_block.2,
      "g1 与 f0 块重叠 blocks={block_li:?}"
    );
    assert_eq!(g1_base, g1_block.1);
    assert_eq!(g1_limit, g1_block.2);

    // f0 重开：80 条仍在且可写。
    let mut f0 = reopen_store(mode, io, shift);
    f0.open_branch(&tbl, "f0", 0).await.unwrap();
    assert_pad(&f0, &tbl, "u", b'U', mode).await;
    f0.put(&tbl, &[up(b"f0x", b"X", 999)]).await.unwrap();
    let io = f0.into_io();

    // g1 重开：80 条仍在且可写。
    let mut g1 = reopen_store(mode, io, shift);
    g1.open_branch(&tbl, "g1", 0).await.unwrap();
    assert_pad(&g1, &tbl, "g", b'G', mode).await;
    g1.put(&tbl, &[up(b"g1x", b"X", 1000)]).await.unwrap();
    let io = g1.into_io();

    let mut main = reopen_store(mode, io, shift);
    main.prepare_attach("main", 0).unwrap();
    main.reopen(&tbl).await.unwrap();
    assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  }
}

/// 同一父分支连续建多个兄弟：每次 create 之间是否重开父分支不影响块位。
async fn create_sibling_chain(
  reopen_between: bool,
  tbl_name: &str,
) -> (MemStore, Vec<(String, u64, u64)>) {
  let tbl = TableIdent::new(tbl_name, 1).unwrap();
  let shift = DEFAULT_PAGES_PER_FILE_SHIFT;
  let mut main = new_store(AllocMode::Pooled, shift);
  main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
  main.create_branch(&tbl, "f1").await.unwrap();
  let io = main.into_io();

  let mut f1 = reopen_store(AllocMode::Pooled, io, shift);
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  f1.put(&tbl, &[up(b"f1a", b"F1A", 2)]).await.unwrap();
  for (index, name) in ["f2", "f3", "f4"].into_iter().enumerate() {
    if reopen_between && index > 0 {
      let io = f1.into_io();
      f1 = reopen_store(AllocMode::Pooled, io, shift);
      f1.open_branch(&tbl, "f1", 0).await.unwrap();
    }
    f1.create_branch(&tbl, name).await.unwrap();
  }
  let io = f1.into_io();
  let block_li = manifest_block_li(&io, &tbl, shift, false).await;
  (io, block_li)
}

#[compio::test]
async fn nested_later_sibling_after_reopen_disjoint() {
  let (mut io, reopen_li) = create_sibling_chain(true, "p10_reopen_yes").await;
  let (_plain_io, plain_li) = create_sibling_chain(false, "p10_reopen_no").await;
  assert_blocks_disjoint(&reopen_li);
  assert_blocks_disjoint(&plain_li);
  assert_eq!(reopen_li, plain_li, "重开父分支改变了兄弟块位");

  // 每个兄弟都能打开、写入自己的块。
  let shift = DEFAULT_PAGES_PER_FILE_SHIFT;
  let tbl = TableIdent::new("p10_reopen_yes", 1).unwrap();
  for (index, name) in ["f2", "f3", "f4"].into_iter().enumerate() {
    let mut branch = reopen_store(AllocMode::Pooled, io, shift);
    branch.open_branch(&tbl, name, 0).await.unwrap();
    let key = format!("{name}x").into_bytes();
    branch
      .put(&tbl, &[up(&key, b"X", 10 + index as u64)])
      .await
      .unwrap();
    assert_eq!(branch.get(&tbl, &key).await.unwrap(), Some(b"X".to_vec()));
    io = branch.into_io();
  }
}

/// 三层链 main → f1 → g1：孙块接在父块之后，父 / 孙各自块内可写。
#[compio::test]
async fn nested_three_level_blocks_disjoint() {
  for mode in MODES {
    let tbl = TableIdent::new("p10_three_level", 1).unwrap();
    let shift = DEFAULT_PAGES_PER_FILE_SHIFT;
    let mut main = new_store(mode, shift);
    main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
    main.create_branch(&tbl, "f1").await.unwrap();
    let io = main.into_io();

    let mut f1 = reopen_store(mode, io, shift);
    f1.open_branch(&tbl, "f1", 0).await.unwrap();
    f1.put(&tbl, &[up(b"f1a", b"F1A", 2)]).await.unwrap();
    f1.create_branch(&tbl, "g1").await.unwrap();
    let io = f1.into_io();

    let mut g1 = reopen_store(mode, io, shift);
    g1.open_branch(&tbl, "g1", 0).await.unwrap();
    g1.put(&tbl, &[up(b"g1a", b"G1A", 3)]).await.unwrap();
    let io = g1.into_io();

    let block_li = manifest_block_li(&io, &tbl, shift, false).await;
    assert_blocks_disjoint(&block_li);
    let f1_block = block_of(&block_li, "f1");
    let g1_block = block_of(&block_li, "g1");
    assert!(
      g1_block.1 >= f1_block.2,
      "g1 块落在 f1 块内 blocks={block_li:?}"
    );

    // 父 / 孙重开后都能继续写。
    let mut f1 = reopen_store(mode, io, shift);
    f1.open_branch(&tbl, "f1", 0).await.unwrap();
    assert_eq!(f1.get(&tbl, b"f1a").await.unwrap(), Some(b"F1A".to_vec()));
    f1.put(&tbl, &[up(b"f1b", b"F1B", 4)]).await.unwrap();
    let io = f1.into_io();

    let mut g1 = reopen_store(mode, io, shift);
    g1.open_branch(&tbl, "g1", 0).await.unwrap();
    assert_eq!(g1.get(&tbl, b"g1a").await.unwrap(), Some(b"G1A".to_vec()));
    g1.put(&tbl, &[up(b"g1b", b"G1B", 5)]).await.unwrap();
  }
}

/// segment 侧块位同样全局单调：main → f1；f1 → f2 / f3；f2 → g1。
#[compio::test]
async fn nested_segment_blocks_globally_disjoint() {
  let tbl = TableIdent::new("p10_segment_blocks", 1).unwrap();
  let shift = DEFAULT_PAGES_PER_FILE_SHIFT;
  let mut main = large_store(MemStore::with_pages_per_file(PAGE_SIZE, shift), shift);
  main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
  main.create_branch(&tbl, "f1").await.unwrap();
  let io = main.into_io();

  let mut f1 = reopen_store_with_large(io, shift);
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  f1.create_branch(&tbl, "f2").await.unwrap();
  let io = f1.into_io();

  let mut f2 = reopen_store_with_large(io, shift);
  f2.open_branch(&tbl, "f2", 0).await.unwrap();
  f2.create_branch(&tbl, "g1").await.unwrap();
  let io = f2.into_io();

  let mut f1 = reopen_store_with_large(io, shift);
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  f1.create_branch(&tbl, "f3").await.unwrap();
  let mut io = f1.into_io();

  let segment_li = manifest_block_li(&io, &tbl, shift, true).await;
  assert_blocks_disjoint(&segment_li);
  assert_eq!(segment_li.len(), 4, "每个 branch 都应有段预留块");
  for (branch, base, end) in &segment_li {
    let mut store = reopen_store_with_large(io, shift);
    store.open_branch(&tbl, branch, 0).await.unwrap();
    assert_eq!(store.branch_segment_limit(), Some(*end));
    assert_eq!(store.segment_mapper(&tbl).unwrap().max_fp_id(), *base);
    io = store.into_io();
  }
}

// sem-25：祖先 post-fork 写区间不得被孙分支块复用

/// sem-25 主复现：`main` 建 `f1` 后继续写，`f1` 再建 `g`。
///
/// 修复前 `g` 的块基址取「main 高水位所在文件」向上对齐（高水位正好是
/// `f1` 块末尾时仍得到同一个 1024 文件块），`g` 的首批写覆盖 `main` 在
/// fork 后写入的页：Pooled / Append 下 `main.get(m) == None`。
///
/// 修复后 `g` 的块从「main 下一次分配所在文件之后」的块边界开始；`main`
/// 重开（含 term bump）时分配水位被抬到所有预留块末尾之后，后续写也不会
/// 进入 `g` 的块。
#[compio::test]
async fn ancestor_post_fork_write_block_not_reused() {
  let shift = 2u8;
  let reserve_pages = BRANCH_FILE_RESERVE_FILE_NUM << shift;
  for mode in MODES {
    for term in [0u64, 3] {
      let name = format!("s25_post_fork_{mode:?}_{term}");
      let tbl = TableIdent::new(name.as_str(), 1).unwrap();
      let mut main = new_store(mode, shift);
      main.prepare_attach("main", term).unwrap();
      main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
      main.create_branch(&tbl, "f1").await.unwrap();
      // main 的分配水位被 reserve 到 f1 块末尾（文件边界）。
      let reserved = main.mapper(&tbl).unwrap().max_fp_id();
      assert_eq!(reserved % (1 << shift), 0, "f1 块末尾必须是文件边界");
      main.put(&tbl, &[up(b"m", b"M", 2)]).await.unwrap();
      let main_max = main.mapper(&tbl).unwrap().max_fp_id();
      assert_eq!(
        main_max,
        reserved + 1,
        "fork 后写必须从预留水位开始 mode={mode:?}"
      );
      let io = main.into_io();

      let mut f1 = reopen_store(mode, io, shift);
      f1.open_branch(&tbl, "f1", term).await.unwrap();
      f1.create_branch(&tbl, "g").await.unwrap();
      let io = f1.into_io();

      let mut g = reopen_store(mode, io, shift);
      g.open_branch(&tbl, "g", term + 1).await.unwrap();
      let g_limit = g.branch_file_limit().expect("g 必须有上限");
      let g_block_base = g_limit - reserve_pages;
      // g 的块必须从「main 下一次分配所在文件」之后开始：main 下一次写
      // （页 main_max）落在 file main_max >> shift，该块整块属于 main。
      let main_next_file = main_max >> shift;
      let expect_base_file =
        (main_next_file + 1).div_ceil(BRANCH_FILE_RESERVE_FILE_NUM) * BRANCH_FILE_RESERVE_FILE_NUM;
      assert_eq!(
        g_block_base >> shift,
        expect_base_file,
        "g 块基址没有避开 main 的 post-fork 写区间 mode={mode:?} term={term}"
      );
      assert!(g_block_base > main_max);
      assert_eq!(g_limit, g_block_base + reserve_pages);
      g.put(&tbl, &[up(b"g", b"G", 3)]).await.unwrap();
      let io = g.into_io();

      // main 重开（带 term bump）：fork 前 / 后的数据都在，且分配水位已越过
      // g 的整个预留块，后续写不会进入 g 的块。
      let mut main = reopen_store(mode, io, shift);
      main.prepare_attach("main", term + 2).unwrap();
      main.reopen(&tbl).await.unwrap();
      assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
      assert_eq!(
        main.get(&tbl, b"m").await.unwrap(),
        Some(b"M".to_vec()),
        "main 的 post-fork 写被 g 覆盖 mode={mode:?} term={term}"
      );
      let main_reopen_max = main.mapper(&tbl).unwrap().max_fp_id();
      assert!(
        main_reopen_max >= g_limit,
        "main 重开后分配水位仍可能进入 g 的块 max={main_reopen_max} g_limit={g_limit} \
         mode={mode:?} term={term}"
      );
      main.put(&tbl, &[up(b"m2", b"M2", 4)]).await.unwrap();
      let io = main.into_io();

      // g 的数据完好且仍可写。
      let mut g = reopen_store(mode, io, shift);
      g.open_branch(&tbl, "g", term + 3).await.unwrap();
      assert_eq!(g.get(&tbl, b"g").await.unwrap(), Some(b"G".to_vec()));
      g.put(&tbl, &[up(b"g2", b"G2", 5)]).await.unwrap();
      let io = g.into_io();

      let mut main = reopen_store(mode, io, shift);
      main.prepare_attach("main", term + 4).unwrap();
      main.reopen(&tbl).await.unwrap();
      assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
      assert_eq!(main.get(&tbl, b"m").await.unwrap(), Some(b"M".to_vec()));
      assert_eq!(main.get(&tbl, b"m2").await.unwrap(), Some(b"M2".to_vec()));
      assert_eq!(main.get(&tbl, b"g").await.unwrap(), None);
    }
  }
}

/// sem-25 的 SimStore 对照：SimStore 与 MemStore 一样把活跃 main manifest
/// 放在独立槽位、不参与 `list_manifest_file_name_li`；块位扫描必须显式读取它，
/// 否则孙块仍会落在 main 的 post-fork 写区间上。
#[compio::test]
async fn ancestor_post_fork_write_block_not_reused_sim_store() {
  let shift = 2u8;
  let tbl = TableIdent::new("s25_sim_post_fork", 1).unwrap();
  let new_sim = || {
    Store::new(
      SimStore::new(PAGE_SIZE).with_pages_per_file(PAGE_SIZE, shift),
      PAGE_SIZE,
      RESTART,
    )
    .with_pages_per_file_shift(shift)
    .with_alloc_mode(AllocMode::Pooled)
  };
  let mut main = new_sim();
  main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
  main.create_branch(&tbl, "f1").await.unwrap();
  main.put(&tbl, &[up(b"m", b"M", 2)]).await.unwrap();
  let main_max = main.mapper(&tbl).unwrap().max_fp_id();
  let io = main.into_io();

  let mut f1 = Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(shift)
    .with_alloc_mode(AllocMode::Pooled);
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  f1.create_branch(&tbl, "g").await.unwrap();
  let io = f1.into_io();

  let mut g = Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(shift)
    .with_alloc_mode(AllocMode::Pooled);
  g.open_branch(&tbl, "g", 0).await.unwrap();
  assert!(
    g.mapper(&tbl).unwrap().max_fp_id() > main_max,
    "g 块落在 main 的 post-fork 写区间上"
  );
  g.put(&tbl, &[up(b"g", b"G", 3)]).await.unwrap();
  let io = g.into_io();

  let mut main = Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(shift)
    .with_alloc_mode(AllocMode::Pooled);
  main.prepare_attach("main", 0).unwrap();
  main.reopen(&tbl).await.unwrap();
  assert_eq!(main.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(
    main.get(&tbl, b"m").await.unwrap(),
    Some(b"M".to_vec()),
    "SimStore 上 main 的 post-fork 写被 g 覆盖"
  );
}
