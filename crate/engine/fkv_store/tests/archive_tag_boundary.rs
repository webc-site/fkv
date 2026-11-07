//! sem-10：归档 tag 的字符集边界。
//!
//! C++ `BranchArchiveName` 只把 tag 原样拼进 `manifest_<branch>_<term>_<tag>`，
//! 不做字符集校验；Rust 旧实现只允许 `[A-Za-z0-9._-]`，于是 C++ 写下的中文 /
//! `:` / 空格 tag 会被 Rust 列出却无法 reopen / delete（`InvalidArgs`）。
//! 文件名格式本身没有歧义（branch 段不允许 `_`，term 段必须是数字，tag 是其余
//! 整段），因此可表示性只取决于「能否落成同目录下可解析的文件名」。这里锁定
//! 放宽后的契约，并钉住 `.tmp` 后缀的写前拒绝——`<name>.tmp` 是整写中间态，
//! 允许写入会产出自己列举不到的永久孤儿。

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io::ManifestIo;
use fkv_io_compio::CompioStore;
use fkv_store::{ArchiveTagInfo, Store};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

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

fn entry(key: &[u8], value: &[u8]) -> BatchEntry {
  BatchEntry::upsert(key.to_vec(), value.to_vec(), 1)
}

#[compio::test]
async fn cpp_style_tags_are_listable_reopenable_and_deletable() {
  let root = TempRoot::new("store_archive_tag_charset");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = new_store(&root, 3);
  store.put(&tbl, &[entry(b"a", b"A")]).await.unwrap();

  let tag_li = ["备份", "tag:prod", "with space", "a_b.c-d"];
  for tag in tag_li {
    let archive = store.create_archive(&tbl, tag, 0).await.unwrap();
    assert_eq!(archive.name, format!("manifest_main_3_{tag}"));
    assert!(
      root.join("demo.1").join(&archive.name).exists(),
      "tag {tag:?} 未落盘"
    );
  }

  let mut expect_li: Vec<ArchiveTagInfo> = tag_li
    .iter()
    .map(|tag| ArchiveTagInfo {
      tag: (*tag).to_owned(),
      term: 3,
    })
    .collect();
  expect_li.sort();
  assert_eq!(store.list_archive_tags(&tbl).await.unwrap(), expect_li);

  // 恢复：中文 tag 的归档能提升为活跃状态（旧白名单在这里返回 InvalidArgs）。
  let info = store.reopen_archive(&tbl, "备份").await.unwrap();
  assert_eq!(info.name, "manifest_main_3_备份");
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

  // 删除：`:` / 空格 tag 均命中；删完从清单消失，重复删除幂等。
  assert_eq!(store.delete_archive(&tbl, "tag:prod").await.unwrap(), 1);
  assert_eq!(store.delete_archive(&tbl, "with space").await.unwrap(), 1);
  assert_eq!(store.delete_archive(&tbl, "tag:prod").await.unwrap(), 0);
  let remain_li: Vec<String> = store
    .list_archive_tags(&tbl)
    .await
    .unwrap()
    .into_iter()
    .map(|entry| entry.tag)
    .collect();
  assert_eq!(remain_li, vec!["a_b.c-d".to_owned(), "备份".to_owned()]);
  assert_eq!(store.delete_archive(&tbl, "a_b.c-d").await.unwrap(), 1);
  assert_eq!(store.delete_archive(&tbl, "备份").await.unwrap(), 1);
  assert!(store.list_archive_tags(&tbl).await.unwrap().is_empty());
}

/// 不可表示 tag 在写前拒绝：`.tmp` 后缀（整写中间态保留名）、路径分隔符、NUL、
/// 空串。空串在 `Store` 层非法；公共 `Runtime::create_archive` 入口会在
/// `fkv_shard` 里先生成时间戳 tag，与 C++ 单表 Archive 空 tag 行为一致。
#[compio::test]
async fn unrepresentable_tags_are_rejected_before_writing() {
  let root = TempRoot::new("store_archive_tag_reject");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = new_store(&root, 3);
  store.put(&tbl, &[entry(b"a", b"A")]).await.unwrap();
  let dir = root.join("demo.1");

  for tag in ["x.tmp", "bad/tag", "bad\0tag"] {
    assert_eq!(
      store.create_archive(&tbl, tag, 0).await.unwrap_err(),
      KvError::InvalidArgs,
      "{tag:?}"
    );
    assert_eq!(
      store.reopen_archive(&tbl, tag).await.unwrap_err(),
      KvError::InvalidArgs,
      "{tag:?}"
    );
    assert_eq!(
      store.delete_archive(&tbl, tag).await.unwrap_err(),
      KvError::InvalidArgs,
      "{tag:?}"
    );
  }
  assert_eq!(
    store.create_archive(&tbl, "", 0).await.unwrap_err(),
    KvError::InvalidArgs
  );
  assert_eq!(
    store.delete_archive(&tbl, "").await.unwrap_err(),
    KvError::InvalidArgs
  );
  // 拒绝发生在落盘前：`.tmp` 名字与它的整写中间态都不存在。
  assert!(!dir.join("manifest_main_3_x.tmp").exists());
  assert!(!dir.join("manifest_main_3_x.tmp.tmp").exists());

  // 外部（C++）写下的 `.tmp` 结尾归档：该后缀是两侧共享的整写中间态标记，
  // Rust 列举一律跳过，读面永远看不到。用一份合法归档记录复制成同名形态，
  // 确认清单不会多出这个 tag。
  let keep = store.create_archive(&tbl, "keep", 0).await.unwrap();
  let record = store
    .page_io()
    .read_manifest_file(&tbl, &keep.name)
    .await
    .unwrap()
    .unwrap();
  store
    .page_io()
    .write_manifest_file(&tbl, "manifest_main_3_foreign.tmp", &record)
    .await
    .unwrap();
  let tag_li: Vec<String> = store
    .list_archive_tags(&tbl)
    .await
    .unwrap()
    .into_iter()
    .map(|entry| entry.tag)
    .collect();
  assert_eq!(tag_li, vec!["keep".to_owned()]);
}
