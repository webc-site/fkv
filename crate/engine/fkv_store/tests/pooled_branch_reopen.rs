//! pooled-05：branch reopen 的空闲池必须只回填自身 file 区间（C++ `FileIdInBranch`）。
//!
//! 夹具把父 branch 的高水位精确停在 file 1023（`pages_per_file_shift = 0`）：
//! `create_branch` 因此给出 `max_fp_id = 1024` 的分支 manifest，父分支的 1023 个
//! 活页全部落在 file 0..=1022。修复前分支重开会把空映射页 1023（父 branch
//! 名下）回填空闲池并立刻分配；修复后分支只能从 file 1024（自身区间）开始分配。

use fkv_btree::BatchEntry;
use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_recovery::ReplayState;
use fkv_store::{AllocMode, Store};
use fkv_types::{TableIdent, decode_id, is_file_page_id};

const PAGE_SIZE: usize = 512;
const RESTART: u16 = 8;
/// 父 branch 的活页数：page/file 位移为 0 时，正好把高水位停在 file 1023。
const TARGET_FP: u64 = 1023;

fn fresh_store() -> Store<MemStore> {
  Store::new(
    MemStore::with_pages_per_file(PAGE_SIZE, 0),
    PAGE_SIZE,
    RESTART,
  )
  .with_pages_per_file_shift(0)
  .with_alloc_mode(AllocMode::Pooled)
}

fn used_fp_li(store: &Store<MemStore>, tbl: &TableIdent) -> Vec<u64> {
  let mut li: Vec<u64> = store
    .mapper(tbl)
    .expect("mapper")
    .tbl()
    .iter_value_li()
    .filter(|value| is_file_page_id(*value))
    .map(decode_id)
    .collect();
  li.sort_unstable();
  li.dedup();
  li
}

/// 单 key 大 value 的页数随 value 长度单调增长；标定出高水位恰为 [`TARGET_FP`] 的长度。
async fn calibrate_value_len(tbl: &TableIdent) -> usize {
  async fn hw_for_len(tbl: &TableIdent, len: usize) -> u64 {
    let mut store = fresh_store();
    store
      .put(tbl, &[BatchEntry::upsert(b"a".to_vec(), vec![9u8; len], 1)])
      .await
      .unwrap();
    store.mapper(tbl).expect("mapper").max_fp_id()
  }

  // 常规格式下 506_000 字节正好 1023 页；格式变动时退回二分标定。
  const KNOWN_LEN: usize = 506_000;
  if hw_for_len(tbl, KNOWN_LEN).await == TARGET_FP {
    return KNOWN_LEN;
  }
  let (mut lo, mut hi) = (400_000usize, 700_000usize);
  assert!(hw_for_len(tbl, lo).await < TARGET_FP, "标定下界过高");
  assert!(hw_for_len(tbl, hi).await > TARGET_FP, "标定上界过低");
  while lo + 1 < hi {
    let mid = (lo + hi) / 2;
    if hw_for_len(tbl, mid).await >= TARGET_FP {
      hi = mid;
    } else {
      lo = mid;
    }
  }
  for len in (hi..hi.saturating_add(4096)).step_by(64) {
    let hw = hw_for_len(tbl, len).await;
    if hw == TARGET_FP {
      return len;
    }
    if hw > TARGET_FP {
      break;
    }
  }
  panic!("无法标定高水位 {TARGET_FP} 的 value 长度（最近候选 {hi}）");
}

#[test]
fn branch_reopen_allocates_only_inside_own_file_range() {
  pollster::block_on(async {
    let tbl = TableIdent::new("pooled_branch", 1).unwrap();
    let value_len = calibrate_value_len(&tbl).await;
    let mut parent = fresh_store();
    parent
      .put(
        &tbl,
        &[BatchEntry::upsert(b"a".to_vec(), vec![9u8; value_len], 1)],
      )
      .await
      .unwrap();
    assert_eq!(
      parent.mapper(&tbl).unwrap().max_fp_id(),
      TARGET_FP,
      "夹具：父高水位必须停在 file 1023"
    );
    let info = parent.create_branch(&tbl, "feature").await.unwrap();
    // 分支 manifest：继承父映射（0..=1022 已用），max 指向自身区间起点 1024。
    let bytes = parent
      .page_io()
      .read_manifest_file(&tbl, &info.name)
      .await
      .unwrap()
      .unwrap();
    let branch_state = ReplayState::from_snapshot(&bytes).unwrap();
    assert_eq!(branch_state.max_fp_id, 1024);
    assert_eq!(
      branch_state.branch_metadata.file_range_li[0].max_file_id, 1023,
      "父 branch 区间到 file 1023"
    );
    let own = branch_state
      .branch_metadata
      .file_range_li
      .last()
      .expect("own range");
    assert_eq!(own.branch_name.as_ref(), "feature");
    assert_eq!(own.max_file_id, 1024, "分支自身区间从 file 1024 起");

    let io = parent.into_io();
    let mut branch = Store::new(io, PAGE_SIZE, RESTART)
      .with_pages_per_file_shift(0)
      .with_alloc_mode(AllocMode::Pooled);
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    let before = used_fp_li(&branch, &tbl);
    assert_eq!(before.len(), TARGET_FP as usize);
    branch
      .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)])
      .await
      .unwrap();
    let after = used_fp_li(&branch, &tbl);
    let new_li: Vec<u64> = after
      .iter()
      .copied()
      .filter(|id| before.binary_search(id).is_err())
      .collect();
    assert!(!new_li.is_empty(), "分支写入必须分配新页");
    assert!(
      new_li.iter().all(|id| *id > 1023),
      "branch 分配越界到父 branch 区间（file<=1023）: {new_li:?}"
    );
    assert_eq!(branch.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));

    // 重开 branch：继承数据与自身新写都必须可读。
    let io = branch.into_io();
    let mut branch = Store::new(io, PAGE_SIZE, RESTART)
      .with_pages_per_file_shift(0)
      .with_alloc_mode(AllocMode::Pooled);
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    let inherited = branch
      .get(&tbl, b"a")
      .await
      .unwrap()
      .expect("inherited value");
    assert_eq!(inherited.len(), value_len);
    assert!(inherited.iter().all(|byte| *byte == 9));
    assert_eq!(branch.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
  });
}
