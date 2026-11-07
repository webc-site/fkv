//! io-15：Pooled 跨 term 复用旧 file id 的回归护栏。
//!
//! term bump 时 allocator 抬到下一个文件边界（`install_replay_state`），但旧 term
//! 文件的空洞仍会从 mapping 重建回空闲池：新 term 的第一笔写落进旧文件 id 的高位
//! 空洞，CompioStore 下同一 file id 出现「新 term 文件（稀疏）+ 旧 term 文件」。
//! 读路径「当前 identity 优先」会先命中新 term 文件里的整页零洞；
//! `skip_verify_checksum=true` 时零页被当成命中，旧 term 活页不再回退读取。
//! CloudStore 则是写入键（当前 identity）与读取键（区间链解析的写时 identity）
//! 分叉。修复后 term bump 把 Pooled 复用下界抬到新文件边界，旧 term 文件只读不复用。

use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_store::{AllocMode, Store};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
/// 每文件 4 页：一次小批量写就能落满 file 0，term bump 的边界容易构造。
const SHIFT: u8 = 2;
const RESTART: u16 = 16;

fn batch() -> Vec<BatchEntry> {
  (0..4u32)
    .map(|i| BatchEntry::upsert(format!("k{i}").into_bytes(), vec![i as u8 + 1; 64], 1))
    .collect()
}

fn new_compio(root: &TempRoot, term: u64, mode: AllocMode) -> Store<CompioStore> {
  Store::new(
    CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT),
    PAGE_SIZE,
    RESTART,
  )
  .with_pages_per_file_shift(SHIFT)
  .with_alloc_mode(mode)
  .with_branch("main", term)
}

fn new_mem(io: MemStore, term: u64, mode: AllocMode) -> Store<MemStore> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(SHIFT)
    .with_alloc_mode(mode)
    .with_branch("main", term)
}

/// Local（CompioStore）：term 3 写 → term 5 重开 → 新写不得复用旧 term 文件的
/// 空洞；旧 term 的活页（含 root / 叶页）在 `skip_verify_checksum=true` 下仍要
/// 读回原值。修复前新写吃掉旧文件的高位空洞，读旧页命中新文件整页零洞。
#[compio::test]
async fn pooled_term_bump_does_not_reuse_old_term_holes() {
  let root = TempRoot::new("pooled_term_reuse");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let entries = batch();

  let mut term3 = new_compio(&root, 3, AllocMode::Pooled);
  term3.put(&tbl, &entries).await.unwrap();
  let old_max = term3.mapper(&tbl).unwrap().max_fp_id();
  let old_root = term3.root_id(&tbl).unwrap();
  let old_root_fp = term3.mapper(&tbl).unwrap().file_page_id(old_root).unwrap();
  assert_eq!(
    old_root_fp >> SHIFT,
    0,
    "小批量应把 root 放在 file 0（old_max={old_max}, root_fp={old_root_fp}）"
  );
  drop(term3);

  let mut term5 = new_compio(&root, 5, AllocMode::Pooled).with_verify_checksum(false);
  term5.reopen(&tbl).await.unwrap();
  let bumped = ((old_max >> SHIFT) + 1) << SHIFT;
  assert_eq!(
    term5.mapper(&tbl).unwrap().max_fp_id(),
    bumped,
    "term 3 → 5 必须按文件边界抬分配水位"
  );

  term5
    .put(
      &tbl,
      &[BatchEntry::upsert(b"new".to_vec(), b"N".to_vec(), 2)],
    )
    .await
    .unwrap();
  for entry in &entries {
    let value = term5
      .get(&tbl, &entry.key)
      .await
      .unwrap_or_else(|err| panic!("read old key {:?} failed: {err}", entry.key));
    assert_eq!(
      value,
      Some(entry.value.clone()),
      "skip_verify_checksum 下旧 term 页被新文件零洞遮蔽: key={:?}",
      String::from_utf8_lossy(&entry.key)
    );
  }
  let new_max = term5.mapper(&tbl).unwrap().max_fp_id();
  assert!(
    new_max > bumped,
    "term bump 后的新写不得复用旧 term 文件的空洞: max={new_max} bumped={bumped}"
  );
  assert_eq!(
    term5.get(&tbl, b"new").await.unwrap(),
    Some(b"N".to_vec()),
    "新 term 写入必须可读"
  );

  // 旧 term 文件在新 term 下仍然存在且按旧 identity 只读。
  let dir = root.join("demo.1");
  assert!(
    dir.join("data_0_main_3").exists(),
    "旧 term 数据文件必须保留"
  );
  assert!(
    !dir.join("data_0_main_5").exists(),
    "修复后新 term 不得向旧 file id 写文件"
  );
}

/// 对照：checksum 开启时，旧实现靠「零页校验失败 → 回退旧 term」也能读对；
/// 修复后不再需要回退。
#[compio::test]
async fn pooled_term_bump_checksum_on_still_reads_old_pages() {
  let root = TempRoot::new("pooled_term_reuse_checksum");
  let tbl = TableIdent::new("demo", 2).unwrap();
  let entries = batch();

  let mut term3 = new_compio(&root, 3, AllocMode::Pooled);
  term3.put(&tbl, &entries).await.unwrap();
  drop(term3);

  let mut term5 = new_compio(&root, 5, AllocMode::Pooled);
  term5.reopen(&tbl).await.unwrap();
  term5
    .put(
      &tbl,
      &[BatchEntry::upsert(b"new".to_vec(), b"N".to_vec(), 2)],
    )
    .await
    .unwrap();

  for entry in &entries {
    assert_eq!(
      term5.get(&tbl, &entry.key).await.unwrap(),
      Some(entry.value.clone())
    );
  }
  assert_eq!(term5.get(&tbl, b"new").await.unwrap(), Some(b"N".to_vec()));
}

/// 同 term 再次 reopen（首次 bump 之后）：manifest 区间链已记录当前 term 区间，
/// 旧 term 文件的空洞仍不得回池。修复前下界只在 bump 那一次生效，第二次 reopen
/// 从映射空洞重建空闲池，又会把旧 term 文件的高位空洞发出去。
#[compio::test]
async fn pooled_same_term_reopen_keeps_old_term_files_out_of_pool() {
  let root = TempRoot::new("pooled_term_reuse_same_term");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let entries = batch();

  let mut term3 = new_compio(&root, 3, AllocMode::Pooled);
  term3.put(&tbl, &entries).await.unwrap();
  let old_max = term3.mapper(&tbl).unwrap().max_fp_id();
  drop(term3);

  let mut term5 = new_compio(&root, 5, AllocMode::Pooled).with_verify_checksum(false);
  term5.reopen(&tbl).await.unwrap();
  let bumped = ((old_max >> SHIFT) + 1) << SHIFT;
  term5
    .put(
      &tbl,
      &[BatchEntry::upsert(b"new".to_vec(), b"N".to_vec(), 2)],
    )
    .await
    .unwrap();
  let written_max = term5.mapper(&tbl).unwrap().max_fp_id();
  assert!(written_max > bumped);
  drop(term5);

  let mut again = new_compio(&root, 5, AllocMode::Pooled).with_verify_checksum(false);
  again.reopen(&tbl).await.unwrap();
  assert_eq!(
    again.mapper(&tbl).unwrap().max_fp_id(),
    written_max,
    "同 term 重开不得抬水位（只在旧 term 区间以下有额外空洞）"
  );
  again
    .put(
      &tbl,
      &[BatchEntry::upsert(b"new2".to_vec(), b"M".to_vec(), 3)],
    )
    .await
    .unwrap();
  assert!(
    again.mapper(&tbl).unwrap().max_fp_id() > written_max,
    "同 term 重开后的新写不得复用旧 term 文件的空洞"
  );
  for entry in &entries {
    assert_eq!(
      again.get(&tbl, &entry.key).await.unwrap(),
      Some(entry.value.clone()),
      "同 term 重开后旧 term 页被复用遮蔽"
    );
  }
  assert_eq!(again.get(&tbl, b"new").await.unwrap(), Some(b"N".to_vec()));
  let dir = root.join("demo.1");
  assert!(dir.join("data_0_main_3").exists());
  assert!(
    !dir.join("data_0_main_5").exists(),
    "当前 term 不得向旧 term 文件 id 写文件"
  );
}

/// 对照：Append 分配永不复用旧页，term bump 后同样能读回旧值。
#[compio::test]
async fn append_term_bump_keeps_old_pages() {
  let root = TempRoot::new("append_term_reuse");
  let tbl = TableIdent::new("demo", 3).unwrap();
  let entries = batch();

  let mut term3 = new_compio(&root, 3, AllocMode::Append);
  term3.put(&tbl, &entries).await.unwrap();
  let old_max = term3.mapper(&tbl).unwrap().max_fp_id();
  drop(term3);

  let mut term5 = new_compio(&root, 5, AllocMode::Append).with_verify_checksum(false);
  term5.reopen(&tbl).await.unwrap();
  let bumped = ((old_max >> SHIFT) + 1) << SHIFT;
  assert_eq!(term5.mapper(&tbl).unwrap().max_fp_id(), bumped);
  term5
    .put(
      &tbl,
      &[BatchEntry::upsert(b"new".to_vec(), b"N".to_vec(), 2)],
    )
    .await
    .unwrap();
  assert!(term5.mapper(&tbl).unwrap().max_fp_id() > bumped);
  for entry in &entries {
    assert_eq!(
      term5.get(&tbl, &entry.key).await.unwrap(),
      Some(entry.value.clone())
    );
  }
}

/// MemStore 把物理页按 `(tbl, file_page_id)` 记账（不含 identity）：旧实现里
/// 复用旧 term 空洞会直接覆盖旧页内容，与 checksum 无关。修复后旧值必须完好。
#[test]
fn pooled_term_bump_keeps_old_pages_mem_store() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 4).unwrap();
    let entries = batch();
    let mut term3 = new_mem(
      MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
      3,
      AllocMode::Pooled,
    );
    term3.put(&tbl, &entries).await.unwrap();
    let old_max = term3.mapper(&tbl).unwrap().max_fp_id();
    let io = term3.into_io();

    let mut term5 = new_mem(io, 5, AllocMode::Pooled).with_verify_checksum(false);
    term5.reopen(&tbl).await.unwrap();
    let bumped = ((old_max >> SHIFT) + 1) << SHIFT;
    assert_eq!(term5.mapper(&tbl).unwrap().max_fp_id(), bumped);
    term5
      .put(
        &tbl,
        &[BatchEntry::upsert(b"new".to_vec(), b"N".to_vec(), 2)],
      )
      .await
      .unwrap();
    assert!(term5.mapper(&tbl).unwrap().max_fp_id() > bumped);

    for entry in &entries {
      assert_eq!(
        term5.get(&tbl, &entry.key).await.unwrap(),
        Some(entry.value.clone()),
        "key 被跨 term 复用覆盖"
      );
    }
  });
}
