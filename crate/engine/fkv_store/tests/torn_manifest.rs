//! gap-10：撕裂 manifest 尾的物化与恢复（真实 compio 后端）。
//!
//! `fkv_task_reopen::split_manifest_with_len` 的 torn 分支此前**没有任何测试经过
//! 真实 store 走到**：`fkv_io_sim` 的 manifest 是原子整体写入，永远产生不出撕裂字节，
//! 而该函数的两处 `break`（`fkv_task_reopen/src/lib.rs:34` 的 torn body、
//! `:51` 的「body 完好但 padding 未落盘」）正是 `sto-51` 那一类缺陷的防线。
//!
//! 这里直接在盘上制造撕裂 / 垃圾尾 / 中间损坏，断言：
//! 1. 撕裂尾恢复到最后一个**完整** record，且等于某个干净前缀；
//! 2. 追加基准被拉回有效长度（`reset_manifest` 截断），之后写入不落在垃圾之后；
//! 3. 「损坏后又出现完好 record」必须报 `Corrupted`，不得静默接受。
//!
//! 反向验证：去掉上述两处 torn `break` 后，本文件的用例必须失败。

use std::{
  fs,
  path::{Path, PathBuf},
};

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io_compio::CompioStore;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, MAIN_BRANCH_NAME, TableIdent, manifest_file_name};

const PAGE_SIZE: usize = 512;
const RESTART_INTERVAL: u16 = 16;
/// 写够多条以形成 snapshot + 至少 2 条 log record。
const OPS: usize = 5;

type Kv = Vec<(Vec<u8>, Vec<u8>)>;

fn tbl() -> TableIdent {
  TableIdent::new("torn", 1).unwrap()
}

fn manifest_path(dir: &Path) -> PathBuf {
  dir
    .join(format!("{}.{}", tbl().tbl_name, tbl().partition_id))
    .join(manifest_file_name(MAIN_BRANCH_NAME, 0))
}

fn batch(index: usize) -> Vec<BatchEntry> {
  vec![BatchEntry::upsert(
    format!("k{index}").into_bytes(),
    format!("v{index}").into_bytes(),
    index as u64 + 1,
  )]
}

fn store_over(dir: &Path) -> Store<CompioStore> {
  let io = CompioStore::new(dir.to_path_buf(), PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT);
  Store::new(io, PAGE_SIZE, RESTART_INTERVAL)
}

async fn seed(root: &TempRoot) {
  let mut store = store_over(root.path());
  for index in 0..OPS {
    store
      .put(&tbl(), &batch(index))
      .await
      .unwrap_or_else(|err| panic!("seed op#{index} 失败: {err}"));
  }
}

async fn reopen_dir(dir: &Path) -> Result<Kv, KvError> {
  let mut store = store_over(dir);
  store.reopen(&tbl()).await?;
  store.scan(&tbl(), b"", b"", 0).await
}

/// 把 `src` 的分区目录整体复制到 `dst`。
///
/// 数据页必须一起带上：前缀 manifest 引用的页少于全量 manifest，但都必须真实存在，
/// 否则 scan 会以 `ResourceMissing` 暴露悬挂引用。
fn copy_partition(src: &Path, dst: &Path) {
  let name = format!("{}.{}", tbl().tbl_name, tbl().partition_id);
  let from = src.join(&name);
  let to = dst.join(&name);
  fs::create_dir_all(&to).unwrap();
  for entry in fs::read_dir(&from).unwrap().flatten() {
    let path = entry.path();
    if path.is_file() {
      fs::copy(&path, to.join(path.file_name().unwrap())).unwrap();
    }
  }
}

/// 复制一份分区目录、把 manifest 覆盖为给定字节并重开，作为「前缀状态」的参考。
async fn kv_of_manifest(seeded: &Path, dst: &TempRoot, bytes: &[u8]) -> Kv {
  copy_partition(seeded, dst.path());
  fs::write(manifest_path(dst.path()), bytes).unwrap();
  reopen_dir(dst.path())
    .await
    .unwrap_or_else(|err| panic!("参考 manifest 应可重开: {err}"))
}

/// 干净 manifest 的字节与切分结果（含 record 片段的偏移）。
fn read_manifest(path: &Path) -> (Vec<u8>, Vec<usize>, usize) {
  let bytes = fs::read(path).unwrap();
  let (rec_li, valid_len) = fkv_task_reopen::split_manifest_with_len(&bytes)
    .unwrap_or_else(|err| panic!("干净 manifest 应可切分: {err}"));
  let offsets = rec_li
    .iter()
    .map(|record| record.as_ptr() as usize - bytes.as_ptr() as usize)
    .collect();
  (bytes, offsets, valid_len)
}

#[compio::test]
async fn torn_manifest_tail_recovers_last_complete_record() {
  let root = TempRoot::new("torn_seed");
  seed(&root).await;
  let path = manifest_path(root.path());
  let (bytes, offsets, valid_len) = read_manifest(&path);
  assert!(
    offsets.len() >= 3,
    "需要 snapshot + 至少 2 条 log record 才能切在中间，实际 {}",
    offsets.len()
  );
  assert_eq!(valid_len, bytes.len(), "干净 manifest 不应有撕裂尾");

  // 截掉最后一个 record 的 padding 字节 → 该 record 视为未提交。
  let torn = valid_len - 1;
  let (_, prefix_len) = fkv_task_reopen::split_manifest_with_len(&bytes[..torn]).unwrap();
  assert!(
    prefix_len < torn,
    "padding 未落盘的尾部 record 必须被丢弃：有效长度应 < {torn}，实际 {prefix_len}。\n\
     此处失败意味着 `split_manifest_with_len` 缺少「body 完好但 padding 未落盘」的丢弃分支\
     （`fkv_task_reopen/src/lib.rs` 的 `if next > bytes.len() {{ break; }}`）：\
     有效长度会超出文件长度，`reset_manifest` 随之把文件撑长，下一次 append 落进零洞，\
     再 reopen 即 `Corrupted`。"
  );

  let ref_root = TempRoot::new("torn_ref");
  let expected = kv_of_manifest(root.path(), &ref_root, &bytes[..prefix_len]).await;

  fs::write(&path, &bytes[..torn]).unwrap();
  let actual = reopen_dir(root.path())
    .await
    .unwrap_or_else(|err| panic!("撕裂尾必须仍可重开: {err}"));
  assert_eq!(actual, expected, "撕裂尾应恢复到最后一个完整 record");
  assert_eq!(
    fs::metadata(&path).unwrap().len(),
    prefix_len as u64,
    "reopen 应把追加基准拉回有效长度（reset_manifest 截断）"
  );

  // 截断后再写一轮：必须落在有效长度之后，而不是垃圾之后。
  let mut store = store_over(root.path());
  store.reopen(&tbl()).await.unwrap();
  store.put(&tbl(), &batch(OPS)).await.unwrap();
  let mut want = expected;
  want.push((
    format!("k{OPS}").into_bytes(),
    format!("v{OPS}").into_bytes(),
  ));
  want.sort();
  assert_eq!(
    reopen_dir(root.path()).await.unwrap(),
    want,
    "截断后追加的写必须可读回"
  );
}

#[compio::test]
async fn garbage_manifest_tail_is_ignored_and_truncated() {
  let root = TempRoot::new("torn_garbage");
  seed(&root).await;
  let path = manifest_path(root.path());
  let (bytes, _, valid_len) = read_manifest(&path);

  let ref_root = TempRoot::new("torn_garbage_ref");
  let expected = kv_of_manifest(root.path(), &ref_root, &bytes[..valid_len]).await;

  let mut polluted = bytes;
  polluted.extend_from_slice(&[0xAB; 64]);
  fs::write(&path, &polluted).unwrap();

  let actual = reopen_dir(root.path())
    .await
    .unwrap_or_else(|err| panic!("垃圾尾必须可重开: {err}"));
  assert_eq!(actual, expected, "垃圾尾不得影响可见状态");
  assert_eq!(
    fs::metadata(&path).unwrap().len(),
    valid_len as u64,
    "垃圾尾应被截掉，追加基准回到有效长度"
  );
}

#[compio::test]
async fn corrupted_record_then_valid_record_is_rejected() {
  let root = TempRoot::new("torn_corrupt");
  seed(&root).await;
  let path = manifest_path(root.path());
  let (mut bytes, offsets, _) = read_manifest(&path);
  assert!(
    offsets.len() >= 3,
    "需要至少 3 条 record：损坏中间一条、其后再留一条完好，实际 {}",
    offsets.len()
  );

  // 破坏第 2 条 record 的 body（首 8 字节是 checksum，content 从 HEADER_BYTES 起）。
  let target = offsets[1] + fkv_manifest::HEADER_BYTES + 1;
  bytes[target] ^= 0xff;
  fs::write(&path, &bytes).unwrap();

  let mut store = store_over(root.path());
  assert_eq!(
    store.reopen(&tbl()).await,
    Err(KvError::Corrupted),
    "「损坏后又出现完好 record」必须报 Corrupted，不得静默接受"
  );
}
