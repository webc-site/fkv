//! sto-55b：云端旧 term manifest 远端对象剪枝。
//!
//! 判定必须保守：只在远端同 branch 的最高 term 就是本实例 term 时，删除更旧
//! 的无 tag manifest；当前 term、其它 branch、tagged archive 一律保留。
//! `main` 在云端同样按 term 命名，存在更高 term 时参与剪枝。

use bytes::Bytes;
use fkv_cloud::CloudStore;
use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_object::{MemObjectStore, ObjectStore};
use fkv_types::TableIdent;

const PREFIX: &str = "fkv";

fn cloud(remote: &MemObjectStore) -> CloudStore<MemStore, MemObjectStore> {
  CloudStore::new(
    MemStore::new(512),
    remote.clone(),
    PREFIX,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
}

async fn remote_manifest_name_li(remote: &MemObjectStore, tbl: &TableIdent) -> Vec<String> {
  let mut name_li: Vec<String> = remote
    .list(&format!("{PREFIX}/{tbl}/"))
    .await
    .unwrap()
    .into_iter()
    .filter_map(|meta| {
      let name = meta.key.rsplit('/').next()?;
      name.starts_with("manifest_").then(|| name.to_owned())
    })
    .collect();
  name_li.sort();
  name_li
}

/// 只删已被当前 term 取代的旧 manifest；当前 term / 其它 branch / archive /
/// main 全部保留，本地 cache 同步清理。
#[test]
fn prune_removes_only_superseded_remote_manifests() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = cloud(&remote);
    cloud.set_identity("feature", 2);
    for name in [
      "manifest_feature_0",
      "manifest_feature_1",
      "manifest_feature_2",
      "manifest_feature_0_snap",
      "manifest_other_0",
      "manifest_main_0",
    ] {
      cloud
        .write_manifest_file(&tbl, name, b"record")
        .await
        .unwrap();
    }

    let removed = cloud
      .prune_superseded_manifest_file_li(&tbl, "feature", 2)
      .await
      .unwrap();
    assert_eq!(removed, 2);
    assert_eq!(
      remote_manifest_name_li(&remote, &tbl).await,
      vec![
        "manifest_feature_0_snap",
        "manifest_feature_2",
        "manifest_main_0",
        "manifest_other_0",
      ]
    );
    // 本地 cache 不留旧名字，避免下一轮 replay 到已删对象。
    let local_name_li = cloud.list_manifest_file_name_li(&tbl).await.unwrap();
    assert!(
      !local_name_li
        .iter()
        .any(|name| name == "manifest_feature_0")
    );
    assert!(
      !local_name_li
        .iter()
        .any(|name| name == "manifest_feature_1")
    );
  });
}

/// 远端出现更高 term（另一实例已推进）时整轮放弃，旧 term 一个都不删。
#[test]
fn prune_skips_when_higher_term_exists() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 2).unwrap();
    let remote = MemObjectStore::new();
    let cloud = cloud(&remote);
    cloud.set_identity("feature", 2);
    for name in [
      "manifest_feature_1",
      "manifest_feature_2",
      "manifest_feature_3",
    ] {
      cloud
        .write_manifest_file(&tbl, name, b"record")
        .await
        .unwrap();
    }

    let removed = cloud
      .prune_superseded_manifest_file_li(&tbl, "feature", 2)
      .await
      .unwrap();
    assert_eq!(removed, 0);
    assert_eq!(
      remote_manifest_name_li(&remote, &tbl).await,
      vec![
        "manifest_feature_1",
        "manifest_feature_2",
        "manifest_feature_3",
      ]
    );
  });
}

/// identity 与调用参数不一致时不做判定，避免替别的 branch / term 剪枝。
#[test]
fn prune_skips_on_identity_mismatch() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 3).unwrap();
    let remote = MemObjectStore::new();
    let cloud = cloud(&remote);
    cloud.set_identity("feature", 1);
    for name in ["manifest_feature_1", "manifest_feature_2"] {
      remote
        .put(
          &format!("{PREFIX}/{tbl}/{name}"),
          Bytes::from_static(b"record"),
          None,
          false,
        )
        .await
        .unwrap();
    }

    let removed = cloud
      .prune_superseded_manifest_file_li(&tbl, "feature", 2)
      .await
      .unwrap();
    assert_eq!(removed, 0);
    assert_eq!(
      remote_manifest_name_li(&remote, &tbl).await,
      vec!["manifest_feature_1", "manifest_feature_2"]
    );
  });
}

/// main 只在远端已有本实例 term 的 manifest 时才剪旧 term；这里远端最高
/// term 仍是 0（与实例 term 9 不一致），保守跳过。
#[test]
fn prune_keeps_main_when_newer_term_missing() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 4).unwrap();
    let remote = MemObjectStore::new();
    let cloud = cloud(&remote);
    cloud.set_identity("main", 9);
    remote
      .put(
        &format!("{PREFIX}/{tbl}/manifest_main_0"),
        Bytes::from_static(b"record"),
        None,
        false,
      )
      .await
      .unwrap();

    let removed = cloud
      .prune_superseded_manifest_file_li(&tbl, "main", 9)
      .await
      .unwrap();
    assert_eq!(removed, 0);
    assert_eq!(
      remote_manifest_name_li(&remote, &tbl).await,
      vec!["manifest_main_0"]
    );
  });
}

/// main 的远端活跃 manifest 名同样带 term：本实例 term 已是远端最高时，
/// 剪掉更旧的无 tag main manifest；tagged archive 保留。
#[test]
fn prune_main_removes_superseded_terms() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 5).unwrap();
    let remote = MemObjectStore::new();
    let cloud = cloud(&remote);
    cloud.set_identity("main", 5);
    for name in ["manifest_main_3", "manifest_main_5", "manifest_main_4_snap"] {
      cloud
        .write_manifest_file(&tbl, name, b"record")
        .await
        .unwrap();
    }

    let removed = cloud
      .prune_superseded_manifest_file_li(&tbl, "main", 5)
      .await
      .unwrap();
    assert_eq!(removed, 1);
    assert_eq!(
      remote_manifest_name_li(&remote, &tbl).await,
      vec!["manifest_main_4_snap", "manifest_main_5"]
    );
  });
}
