//! 归档保留策略行为锁定（sem-02）：`create_archive` 成功后按唯一 tag 倒序保留
//! `num_retained` 份、同 tag 只保留最高 term，活跃 manifest（无 tag）不受剪枝
//! 影响。C++ 基线（82b774c）的 `DeleteOldArchives` 是无调用点死代码、归档从不
//! 自动删除；该剪枝属有意差异（实现两侧文档共同宣称的保留语义），判定与差分
//! 口径见 eloqstore2fkv/COMPAT_BASELINE「归档保留策略」条目。

use fkv_btree::BatchEntry;
use fkv_io::ManifestIo;
use fkv_io_compio::CompioStore;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn entry(key: &[u8], value: &[u8]) -> BatchEntry {
  BatchEntry::upsert(key.to_vec(), value.to_vec(), 1)
}

fn new_store(root: &TempRoot, term: u64) -> Store<CompioStore> {
  Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  )
  .with_branch("main", term)
}

/// 跨 term 重开后，`prune_archive_li` 的计数与选择口径：
/// - 按**唯一 tag** 计数（同 tag 多 term 去重后只占一个名额）；
/// - 同 tag 只保留最高 term（与 `reopen_archive` 的选择一致）；
/// - 超出 `num_retained` 个唯一 tag 时，按 tag 倒序剪掉最旧 tag 的全部 term；
/// - 活跃 manifest（无 tag，固定 `manifest_main_0`）永不入剪枝列表。
///
/// term 4 的归档同时扮演「接管前已存在」的旧归档：Rust 首次归档即触发对它的
/// 清理（COMPAT_BASELINE 记录的混合场景语义）。
#[compio::test]
async fn prune_keeps_newest_unique_tags_and_highest_term() {
  let root = TempRoot::new("store_archive_retention");
  let tbl = TableIdent::new("demo", 1).unwrap();

  // term 4：写下第一份归档 tag "a"（模拟 C++ / 旧 term 遗留归档）。
  let mut store = new_store(&root, 4);
  store.put(&tbl, &[entry(b"a", b"A")]).await.unwrap();
  let first = store.create_archive(&tbl, "a", 2).await.unwrap();
  assert_eq!(first.name, "manifest_main_4_a");
  assert_eq!(first.removed_num, 0);
  drop(store);

  // term 5 重开：同 tag "a" 追加更高 term，旧 term 的同名归档被剪，
  // 且 "a" 去重后仍只占 1 个唯一 tag 名额。
  let mut store = new_store(&root, 5);
  store.reopen(&tbl).await.unwrap();
  let second = store.create_archive(&tbl, "a", 2).await.unwrap();
  assert_eq!(second.name, "manifest_main_5_a");
  assert_eq!(second.removed_num, 1, "同 tag 只保留最高 term");

  // 唯一 tag 数 1 < 2：tag "b" 仍可进入保留集（若按 C++ 死代码的文件计数，
  // 此时两个文件已占满名额，"b" 会被误剪——口径必须按唯一 tag）。
  let third = store.create_archive(&tbl, "b", 2).await.unwrap();
  assert_eq!(third.removed_num, 0);

  // 第 3 个唯一 tag "c"：tag 倒序保留 "c" / "b"，最旧 tag "a" 的全部 term 被剪。
  let fourth = store.create_archive(&tbl, "c", 2).await.unwrap();
  assert_eq!(fourth.removed_num, 1, "超出唯一 tag 名额时剪最旧 tag");

  // 剪枝只碰带 tag 的归档：活跃 manifest（无 tag）必须原样保留。
  let name_li = store
    .page_io()
    .list_manifest_file_name_li(&tbl)
    .await
    .unwrap();
  assert_eq!(
    name_li,
    vec![
      "manifest_main_0".to_owned(),
      "manifest_main_5_b".to_owned(),
      "manifest_main_5_c".to_owned(),
    ],
    "只剩最新 2 个 tag 的归档，活跃 manifest 不受剪枝影响"
  );

  // 数据面不受影响：接管前的数据仍可读，继续写正常落盘。
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  store.put(&tbl, &[entry(b"z", b"Z")]).await.unwrap();
  assert_eq!(store.get(&tbl, b"z").await.unwrap(), Some(b"Z".to_vec()));
}
