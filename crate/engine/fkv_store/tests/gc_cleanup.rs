//! sto-55：GC 收尾的旧 term manifest 剪枝与空分区目录移除。
//!
//! 删除必须保守：非空目录原样保留；旧 manifest 只有在同 branch 的当前 term
//! manifest 已存在时才剪枝。

use std::{
  fs::{create_dir_all, write},
  path::Path,
};

use fkv_btree::BatchEntry;
use fkv_io::ManifestIo;
use fkv_io_compio::CompioStore;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn write_name_li(dir: &Path, name_li: &[&str]) {
  for name in name_li {
    write(dir.join(name), b"x").unwrap();
  }
}

async fn manifest_name_li(io: &CompioStore, tbl: &TableIdent) -> Vec<String> {
  io.list_manifest_file_name_li(tbl).await.unwrap()
}

/// 只删同 branch、无 tag、term 更小的旧 manifest；当前 term / 其它 branch /
/// archive / main 全部保留。
#[compio::test]
async fn prune_removes_only_superseded_active_branch_manifests() {
  let root = TempRoot::new("store_gc_cleanup_prune");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  write_name_li(
    &dir,
    &[
      "manifest_feature_2",
      "manifest_feature_0",
      "manifest_feature_1",
      "manifest_other_0",
      "manifest_feature_0_snap",
      "manifest_main_0",
    ],
  );
  let io = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );

  let removed = io
    .prune_superseded_manifest_file_li(&tbl, "feature", 2)
    .await
    .unwrap();
  assert_eq!(removed, 2);
  assert_eq!(
    manifest_name_li(&io, &tbl).await,
    vec![
      "manifest_feature_0_snap",
      "manifest_feature_2",
      "manifest_main_0",
      "manifest_other_0",
    ]
  );

  // main 的活跃 manifest 文件名固定，任何 term 下都不剪枝。
  let removed = io
    .prune_superseded_manifest_file_li(&tbl, "main", 9)
    .await
    .unwrap();
  assert_eq!(removed, 0);
  assert!(dir.join("manifest_main_0").exists());
}

/// 没看到当前 term manifest 时不判定旧文件被取代，一个都不删。
#[compio::test]
async fn prune_requires_current_term_manifest() {
  let root = TempRoot::new("store_gc_cleanup_no_current");
  let tbl = TableIdent::new("demo", 2).unwrap();
  let dir = root.join("demo.2");
  create_dir_all(&dir).unwrap();
  write_name_li(&dir, &["manifest_feature_0", "manifest_feature_1"]);
  let io = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );

  let removed = io
    .prune_superseded_manifest_file_li(&tbl, "feature", 2)
    .await
    .unwrap();
  assert_eq!(removed, 0);
  assert_eq!(
    manifest_name_li(&io, &tbl).await,
    vec!["manifest_feature_0", "manifest_feature_1"]
  );
}

/// 完全空的分区目录在 GC 收尾时移除（drop_table 只删文件，留下空壳目录）。
#[compio::test]
async fn gc_removes_truly_empty_partition_dir() {
  let root = TempRoot::new("store_gc_cleanup_empty");
  let tbl = TableIdent::new("demo", 3).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  let dir = root.join("demo.3");
  assert!(dir.exists());

  store.drop_table(&tbl).await.unwrap();
  assert!(dir.exists(), "drop_table 不应递归删目录");
  // Drop 只做逻辑删除（对齐 C++ `DropManifest`），data 文件留给 GC 按
  // retained set 回收；这里验证随后的 FileGc 才会删文件与空目录。

  store
    .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .await
    .unwrap();
  assert!(!dir.exists(), "空分区目录应被 GC 收尾移除");
}

/// 目录里还有 active manifest（truncate 后）或未知残留时不得删除。
#[compio::test]
async fn gc_keeps_non_empty_partition_dir() {
  let root = TempRoot::new("store_gc_cleanup_keep");
  let tbl = TableIdent::new("demo", 4).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store
    .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
    .await
    .unwrap();
  let dir = root.join("demo.4");
  store.truncate(&tbl).await.unwrap();
  store
    .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .await
    .unwrap();
  assert!(dir.exists(), "manifest 仍在时不得删除分区目录");
  assert!(dir.join("manifest_main_0").exists());

  // 未知残留文件同样保守保留。
  let stray = TableIdent::new("demo", 5).unwrap();
  let stray_dir = root.join("demo.5");
  create_dir_all(&stray_dir).unwrap();
  write(stray_dir.join("note.txt"), b"keep").unwrap();
  store
    .file_gc(&stray, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .await
    .unwrap();
  assert!(stray_dir.exists(), "未知残留文件存在时不得删除分区目录");
}
