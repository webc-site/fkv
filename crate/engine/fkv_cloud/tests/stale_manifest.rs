//! rev-34：Cloud reopen 的权威来源是远端 manifest，不是本地 cache。
//!
//! 本地活跃 manifest 文件名与 term 无关（恒为 `manifest_main_0`），跨 term
//! 接管复用旧目录时会命中上一任写下的 stale 快照。`read_manifest` 本地命中
//! 也必须回源比较：否则 replay 出过期 root（静默丢读），后续写回还会把
//! stale 基线晋升成远端权威。

use bytes::Bytes;
use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_error::KvError;
use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_object::{MemObjectStore, ObjectStore};
use fkv_store::Store;
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
const SHIFT: u8 = 2;
const PREFIX: &str = "fkv";

type TestStore = Store<CloudStore<MemStore, MemObjectStore>>;

fn store(local: MemStore, remote: &MemObjectStore, term: u64) -> TestStore {
  Store::new(
    CloudStore::new(local, remote.clone(), PREFIX, SHIFT),
    PAGE_SIZE,
    16,
  )
  .with_pages_per_file_shift(SHIFT)
  .with_branch("main", term)
}

fn manifest_key(tbl: &TableIdent, term: u64) -> String {
  format!("{PREFIX}/{tbl}/manifest_main_{term}")
}

async fn local_with_manifest(bytes: &[u8], tbl: &TableIdent) -> MemStore {
  let local = MemStore::with_pages_per_file(PAGE_SIZE, SHIFT);
  local.switch_manifest(tbl, bytes).await.unwrap();
  local
}

/// 本地 stale manifest（term 1）+ 远端更高 term（term 2）→ reopen 必须采纳远端，
/// 且后续写回不得把 stale 基线晋升成权威。
#[test]
fn stale_local_manifest_is_replaced_by_remote_on_reopen() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();

    // term 1 写入并提交 "old"；保存其本地 manifest 作为后续的 stale cache。
    let mut first = store(MemStore::with_pages_per_file(PAGE_SIZE, SHIFT), &remote, 1);
    first
      .put(
        &tbl,
        &[BatchEntry::upsert(b"old".to_vec(), b"OLD".to_vec(), 1)],
      )
      .await
      .unwrap();
    let stale_local = first.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    assert!(
      remote.get(&manifest_key(&tbl, 1)).await.unwrap().is_some(),
      "term 1 manifest 必须已上传"
    );

    // term 2 接管：本地为空，reopen 回源 term 1 后提交 "new"。
    let mut second = store(MemStore::with_pages_per_file(PAGE_SIZE, SHIFT), &remote, 2);
    second.reopen(&tbl).await.unwrap();
    assert_eq!(
      second.get(&tbl, b"old").await.unwrap(),
      Some(b"OLD".to_vec())
    );
    second
      .put(
        &tbl,
        &[BatchEntry::upsert(b"new".to_vec(), b"NEW".to_vec(), 2)],
      )
      .await
      .unwrap();
    assert!(
      remote.get(&manifest_key(&tbl, 2)).await.unwrap().is_some(),
      "term 2 manifest 必须已上传"
    );

    // term 3 复用同一本地目录：本地 manifest 仍是 term 1 的 stale 快照。
    let mut third = store(local_with_manifest(&stale_local, &tbl).await, &remote, 3);
    third.reopen(&tbl).await.unwrap();
    assert_eq!(
      third.get(&tbl, b"old").await.unwrap(),
      Some(b"OLD".to_vec()),
      "reopen 后旧数据仍在"
    );
    assert_eq!(
      third.get(&tbl, b"new").await.unwrap(),
      Some(b"NEW".to_vec()),
      "reopen 必须以远端 term 2 manifest 为基线，而不是 stale 本地快照"
    );

    // 以采纳后的基线继续写：远端 term 3 manifest 应包含全部数据。
    third
      .put(
        &tbl,
        &[BatchEntry::upsert(b"third".to_vec(), b"THIRD".to_vec(), 3)],
      )
      .await
      .unwrap();
    let mut fourth = store(MemStore::with_pages_per_file(PAGE_SIZE, SHIFT), &remote, 4);
    fourth.reopen(&tbl).await.unwrap();
    for (key, value) in [
      (b"old".as_slice(), b"OLD".as_slice()),
      (b"new".as_slice(), b"NEW".as_slice()),
      (b"third".as_slice(), b"THIRD".as_slice()),
    ] {
      assert_eq!(
        fourth.get(&tbl, key).await.unwrap(),
        Some(value.to_vec()),
        "stale 基线不得在写回时覆盖远端已提交数据"
      );
    }
  });
}

/// 远端没有 manifest 时，本地 cache 不是权威：reopen 返回 `ResourceMissing`
/// 交给重试队列，不得以 stale 快照懒创建；远端状态恢复后下一次 reopen 成功。
#[test]
fn stale_local_manifest_without_remote_is_not_trusted() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 2).unwrap();
    let remote = MemObjectStore::new();

    let mut writer = store(MemStore::with_pages_per_file(PAGE_SIZE, SHIFT), &remote, 1);
    writer
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    let stale_local = writer.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    drop(writer);

    // 只删远端 manifest（数据对象仍在，模拟 manifest 暂不可见 / 被外部清理）。
    for meta in remote.list(&format!("{PREFIX}/{tbl}/")).await.unwrap() {
      if meta
        .key
        .rsplit('/')
        .next()
        .is_some_and(|name| name.starts_with("manifest_"))
      {
        remote.delete(&meta.key).await.unwrap();
      }
    }

    let mut reopened = store(local_with_manifest(&stale_local, &tbl).await, &remote, 2);
    assert_eq!(
      reopened.reopen(&tbl).await.unwrap_err(),
      KvError::ResourceMissing,
      "远端无 manifest 时不能以 stale 本地快照起跑"
    );

    // 远端状态恢复：回源成功，数据可读。
    remote
      .put(
        &manifest_key(&tbl, 1),
        Bytes::from(stale_local.clone()),
        None,
        false,
      )
      .await
      .unwrap();
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(reopened.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  });
}

/// attach 抬高 term 后本 term 尚未写过该表：远端权威 manifest 仍在旧 term。
/// `drop_table` 必须删掉这个旧 term 对象，否则 reopen 会按它复活整表。
#[test]
fn drop_table_removes_old_term_manifest_after_attach_bump() {
  pollster::block_on(async {
    let tbl = TableIdent::new("dropbump", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut writer = store(MemStore::with_pages_per_file(PAGE_SIZE, SHIFT), &remote, 3);
    writer
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    assert!(
      remote.get(&manifest_key(&tbl, 3)).await.unwrap().is_some(),
      "term 3 的活跃 manifest 必须已上传"
    );
    drop(writer);

    // term 4 接管（本地 cache 复用同一目录），本 term 不写该表。
    let mut taker = store(MemStore::with_pages_per_file(PAGE_SIZE, SHIFT), &remote, 4);
    taker.reopen(&tbl).await.unwrap();
    taker.drop_table(&tbl).await.unwrap();
    assert!(
      remote
        .list(&format!("{PREFIX}/{tbl}/"))
        .await
        .unwrap()
        .iter()
        .all(|meta| !meta.key.contains("/manifest_")),
      "Drop 必须同时清掉被接管的旧 term manifest"
    );

    let mut probe = store(MemStore::with_pages_per_file(PAGE_SIZE, SHIFT), &remote, 5);
    assert_eq!(
      probe.reopen(&tbl).await.unwrap_err(),
      KvError::ResourceMissing,
      "Drop 后 reopen 不得按旧 term manifest 复活"
    );
  });
}

/// 远端无 manifest 时预热不得把本地旧副本当权威：与 `read_manifest` 的
/// `DropLocalManifest` 语义一致，返回 `false` 并清掉本地残留。
#[test]
fn prewarm_manifest_does_not_trust_local_without_remote() {
  pollster::block_on(async {
    let tbl = TableIdent::new("nowarmlocal", 1).unwrap();
    let local = MemStore::with_pages_per_file(PAGE_SIZE, SHIFT);
    local.switch_manifest(&tbl, b"stale").await.unwrap();
    let cloud = CloudStore::new(local, MemObjectStore::new(), PREFIX, SHIFT);
    assert!(!cloud.prewarm_manifest(&tbl).await.unwrap());
    assert!(
      cloud.local().read_manifest(&tbl).await.unwrap().is_none(),
      "本地 stale manifest 必须被清掉"
    );
  });
}
