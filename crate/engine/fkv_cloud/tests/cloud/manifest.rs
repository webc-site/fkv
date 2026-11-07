//! manifest 安装 / 删除与 prewarm 测试。

use super::*;

/// 外部换主后本地 cache 仍是旧 manifest：`install_external_snapshot` 必须强制
/// 回源远端，采纳新状态。
#[test]
fn install_external_snapshot_refreshes_stale_local_manifest() {
  pollster::block_on(async {
    let tbl = TableIdent::new("adopt", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut writer = Store::new(
      CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    writer
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    let mut follower = Store::new(
      CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    assert_eq!(
      follower.install_external_snapshot(&tbl).await.unwrap(),
      InstallOutcome::Installed
    );
    assert_eq!(follower.get(&tbl, b"k").await.unwrap(), Some(b"A".to_vec()));

    writer
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"B".to_vec(), 2)])
      .await
      .unwrap();
    // follower 本地 manifest 仍是 A；install 必须覆盖成远端新状态。
    assert_eq!(
      follower.install_external_snapshot(&tbl).await.unwrap(),
      InstallOutcome::Installed
    );
    assert_eq!(follower.get(&tbl, b"k").await.unwrap(), Some(b"B".to_vec()));
  });
}

/// 远端无 manifest：只清本地缓存与内存状态，返回「等待远端」。
#[test]
fn install_external_snapshot_waits_when_remote_missing() {
  pollster::block_on(async {
    let tbl = TableIdent::new("wait", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut writer = Store::new(
      CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    writer
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    let mut follower = Store::new(
      CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    assert_eq!(
      follower.install_external_snapshot(&tbl).await.unwrap(),
      InstallOutcome::Installed
    );
    remote.delete("fkv/wait.1/manifest_main_0").await.unwrap();

    assert_eq!(
      follower.install_external_snapshot(&tbl).await.unwrap(),
      InstallOutcome::WaitingRemote
    );
    assert!(!follower.is_open(&tbl));
    assert!(
      follower
        .page_io()
        .local()
        .read_manifest(&tbl)
        .await
        .unwrap()
        .is_none()
    );
  });
}

/// `drop_local_manifest` 绝不删远端 durability anchor（对齐 C++ `DropLocalManifest`）。
#[test]
fn drop_local_manifest_keeps_remote_object() {
  pollster::block_on(async {
    let tbl = TableIdent::new("keep", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(cloud, 512, 16);
    store
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    store.page_io().drop_local_manifest(&tbl).await.unwrap();
    let cloud = store.page_io();
    assert!(cloud.local().read_manifest(&tbl).await.unwrap().is_none());
    assert!(
      remote
        .get("fkv/keep.1/manifest_main_0")
        .await
        .unwrap()
        .is_some()
    );
  });
}

/// `drop_manifest` 必须删**当前 identity term** 的活跃 manifest：
/// 固定删 `manifest_<branch>_0` 时，term>0 的 Drop 后 reopen 会把旧快照复活。
#[test]
fn drop_manifest_removes_current_term_anchor() {
  pollster::block_on(async {
    let tbl = TableIdent::new("dropterm", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut store = Store::new(
      CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    )
    .with_pages_per_file_shift(fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .with_branch("main", 3);
    store
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    assert!(
      remote
        .get("fkv/dropterm.1/manifest_main_3")
        .await
        .unwrap()
        .is_some(),
      "term>0 的活跃 manifest 必须按 identity term 命名"
    );

    store.drop_table(&tbl).await.unwrap();
    assert!(
      remote
        .get("fkv/dropterm.1/manifest_main_3")
        .await
        .unwrap()
        .is_none(),
      "Drop 必须删当前 term 的 manifest"
    );
    assert!(
      remote
        .list("fkv/dropterm.1/")
        .await
        .unwrap()
        .iter()
        .all(|meta| !meta.key.contains("/manifest_")),
      "不得残留任何 term 的 manifest"
    );
  });
}

/// 本地 manifest 存在但落后于远端最新 term 时，prewarm 必须按远端内容重装本地
/// 并采纳新区间；只信本地副本会沿用旧 `file_range_li`，漏预热新 term 文件。
#[test]
fn prewarm_manifest_refreshes_stale_local_manifest() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let other = TableIdent::new("other", 1).unwrap();
    // 用另一张表的有效 manifest 伪造「重启后本地仍是旧 term 内容」：两者
    // 都通过 replay 校验，但内容必然不同。
    let mut other_store = Store::new(
      CloudStore::new(
        MemStore::new(512),
        MemObjectStore::new(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    // 多写几条，保证 manifest 的根 / 文件布局与目标表不同（manifest 只含
    // 布局元数据，不含 key；单条写入的两张表字节会完全一样）。
    let entry_li: Vec<_> = (0..16u8)
      .map(|i| BatchEntry::upsert(vec![i], vec![i; 64], 1))
      .collect();
    other_store.put(&other, &entry_li).await.unwrap();
    let (other_local, _) = other_store.into_io().into_parts();
    let stale = ManifestIo::read_manifest(&other_local, &other)
      .await
      .unwrap()
      .unwrap();

    let remote = MemObjectStore::new();
    let mut target_store = Store::new(
      CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    target_store
      .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1)])
      .await
      .unwrap();
    let (local, remote) = target_store.into_io().into_parts();
    let latest = remote
      .get("fkv/demo.1/manifest_main_0")
      .await
      .unwrap()
      .unwrap();
    assert_ne!(latest.as_ref(), stale.as_slice());
    // 模拟重启后本地仍是旧 manifest：远端已是新内容。
    ManifestIo::switch_manifest(&local, &tbl, &stale)
      .await
      .unwrap();

    let cloud = CloudStore::new(
      local,
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    assert!(cloud.prewarm_manifest(&tbl).await.unwrap());
    let refreshed = ManifestIo::read_manifest(cloud.local(), &tbl)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(
      refreshed,
      latest.as_ref(),
      "prewarm 必须用远端最新 manifest 覆盖本地旧副本"
    );
  });
}
