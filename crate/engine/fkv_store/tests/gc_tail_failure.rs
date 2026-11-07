//! io-31：GC 收尾（空分区目录移除）失败不得让整个 FileGc / maintain 回错误。
//!
//! 对齐 C++ `ExecuteLocalGC` 对 `TryCleanupLocalPartitionDir` 失败仅
//! `LOG(WARNING)` 并返回 `NoError`（`file_gc.cpp:364-371`）：文件删除已经
//! 成功完成，目录收尾的瞬时错误（read_dir EMFILE、跨进程写入触发的
//! ENOTEMPTY、EACCES）不应推翻 GC 结果、丢掉删除数或中断 maintain 尾部步骤。

use fkv_btree::BatchEntry;
use fkv_failpoint::set_spec;
use fkv_io_compio::CompioStore;
use fkv_store::{AllocMode, Store};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn batch(prefix: &str, count: u32) -> Vec<BatchEntry> {
  (0..count)
    .map(|i| {
      BatchEntry::upsert(
        format!("{prefix}k{i:02}").into_bytes(),
        vec![i as u8 + 1; 64],
        1,
      )
    })
    .collect()
}

fn compio(root: &TempRoot) -> CompioStore {
  CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
}

/// put → drop_table → 注入 rmdir 失败：data 文件已删，`file_gc` 必须回
/// `Ok(removed >= 1)` 且目录留在原地；无注入对照仍移除目录。
#[compio::test]
async fn gc_tail_rmdir_failure_keeps_removed_count() {
  let root = TempRoot::new("store_gc_tail_rmdir_fail");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(compio(&root), 512, 16);
  store.put(&tbl, &batch("", 4)).await.unwrap();
  let dir = root.join("demo.1");
  assert!(dir.exists());

  store.drop_table(&tbl).await.unwrap();
  assert!(dir.exists(), "drop_table 不应递归删目录");

  // 注入一次目录收尾瞬时失败（read_dir EMFILE / rmdir ENOTEMPTY 竞态 /
  // EACCES 等价类）：文件删除阶段已成功完成，收尾失败只告警。
  set_spec("TryRemoveEmptyDir:Fail@1");
  let removed = store
    .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .await
    .unwrap_or_else(|err| panic!("收尾失败不应让 FileGc 回错误: {err}"));
  set_spec("");
  assert!(
    removed >= 1,
    "已完成的文件删除数不得丢弃: removed={removed}"
  );
  assert!(dir.exists(), "注入的 rmdir 失败应保留目录");

  // 无注入对照：下一轮 GC 收尾移除空目录。
  let removed_again = store
    .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .await
    .unwrap();
  assert_eq!(removed_again, 0, "文件已删光，第二轮不应再删文件");
  assert!(!dir.exists(), "无注入时空分区目录应被收尾移除");
}

/// `Store::maintain` 在 GC 收尾失败时仍完成尾部步骤：pooled 覆盖写退役的
/// 物理页必须当轮回池（reclaim_retired_now），不得被目录收尾错误推迟。
#[compio::test]
async fn maintain_tail_steps_survive_rmdir_failure() {
  let root = TempRoot::new("store_maintain_tail_rmdir_fail");
  let tbl = TableIdent::new("demo", 2).unwrap();
  let mut store = Store::new(compio(&root), 512, 16)
    .with_pages_per_file_shift(2)
    .with_alloc_mode(AllocMode::Pooled);
  store.put(&tbl, &batch("", 4)).await.unwrap();
  // 覆盖同批 key：pooled COW 把旧物理页退役进 retired 队列。
  store.put(&tbl, &batch("", 4)).await.unwrap();
  let retired = store.mapper(&tbl).expect("mapper").retired_len();
  assert!(retired > 0, "覆盖写应退役旧物理页: retired={retired}");

  // 注入命中 maintain 内 file_gc 的收尾目录 IO（表非逻辑空，
  // cleanup_empty_partition 随后的 rmdir 尝试因 manifest 在场只会 Ok(false)，
  // 唯一可达的失败类是 read_dir 侧瞬时错误）。
  set_spec("TryRemoveEmptyDir:Fail@1");
  store
    .maintain(&tbl)
    .await
    .unwrap_or_else(|err| panic!("收尾失败不应中断 maintain: {err}"));
  set_spec("");
  assert_eq!(
    store.mapper(&tbl).expect("mapper").retired_len(),
    0,
    "reclaim_retired_now 必须仍被执行"
  );
}

/// put → truncate 清空（sem-59：manifest 保留、root 归零）→ maintain 内
/// `cleanup_empty_partition` 的 rmdir 尝试被注入失败：maintain 仍回 Ok。
/// 注入用 @2：第一次命中是同轮 file_gc 收尾的目录检查（manifest 在场，
/// Ok(false)），第二次才是 data 文件删光后的真正收尾尝试。
#[compio::test]
async fn maintain_survives_cleanup_empty_partition_failure() {
  let root = TempRoot::new("store_maintain_cleanup_fail");
  let tbl = TableIdent::new("demo", 3).unwrap();
  let mut store = Store::new(compio(&root), 512, 16);
  store.put(&tbl, &batch("", 4)).await.unwrap();
  store.truncate(&tbl).await.unwrap();

  set_spec("TryRemoveEmptyDir:Fail@2");
  store
    .maintain(&tbl)
    .await
    .unwrap_or_else(|err| panic!("空分区收尾失败不应中断 maintain: {err}"));
  set_spec("");

  // 目录因 manifest 保留（sem-59）；下一轮无注入时收尾幂等 Ok(false)。
  store.maintain(&tbl).await.unwrap();
  assert!(root.join("demo.3").exists(), "manifest 在场时分区目录保留");
}
