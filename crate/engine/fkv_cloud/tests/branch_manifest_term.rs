//! sem-60：Cloud 模式 branch 活跃 manifest 对象名带 attach term。
//!
//! 对齐 C++ attach 后按 `(GetActiveBranch(), ProcessTerm())` 命名：
//! `CreateBranch` 落盘 `manifest_<branch>_0`；attach term T 打开时先试
//! `manifest_<branch>_T`，未命中按「同 branch 最高无 tag term」回退采纳，
//! 首写按 `_T` 名发布（写时晋升，不额外上传晋升对象）；`_0` 在 `_T` 成为
//! 远端最高名 term 后可被剪枝。更高 term 对象存在时过期 owner 被
//! `ExpiredTerm` 栅栏拒绝（对齐 C++ `CloudStoreMgr::GetManifest`）。

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

fn entry(key: &[u8], value: &[u8]) -> BatchEntry {
  BatchEntry::upsert(key.to_vec(), value.to_vec(), 1)
}

fn cloud(remote: &MemObjectStore) -> CloudStore<MemStore, MemObjectStore> {
  CloudStore::new(
    MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
    remote.clone(),
    PREFIX,
    SHIFT,
  )
}

fn store(remote: &MemObjectStore) -> Store<CloudStore<MemStore, MemObjectStore>> {
  Store::new(cloud(remote), PAGE_SIZE, 16).with_pages_per_file_shift(SHIFT)
}

async fn remote_has(remote: &MemObjectStore, key: &str) -> bool {
  remote.get(key).await.unwrap().is_some()
}

/// 在 main 上造出 branch（远端落 `manifest_feature_0`）并写入基线数据。
async fn setup_branch(remote: &MemObjectStore, tbl: &TableIdent) {
  let mut main = Store::new(
    CloudStore::new(
      MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
      remote.clone(),
      PREFIX,
      SHIFT,
    ),
    PAGE_SIZE,
    16,
  )
  .with_pages_per_file_shift(SHIFT)
  .with_branch("main", 2);
  main.put(tbl, &[entry(b"a", b"A")]).await.unwrap();
  let branch = main.create_branch(tbl, "feature").await.unwrap();
  assert_eq!(branch.name, "manifest_feature_0");
  assert!(remote_has(remote, "fkv/demo.1/manifest_feature_0").await);
}

/// ① attach term T 后写一批：远端对象 = `manifest_B_T`；冷启动 reopen 经当前
/// term 名找回；更高 term 冷启动经回退链选中 `_T` 并续写发布 `_T'`。
#[test]
fn branch_manifest_object_carries_attach_term() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    setup_branch(&remote, &tbl).await;

    // attach term 3：远端只有 `_0`，回退采纳后首写发布 `manifest_feature_3`。
    let mut attached = store(&remote);
    attached.open_branch(&tbl, "feature", 3).await.unwrap();
    assert_eq!(attached.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert!(
      !remote_has(&remote, "fkv/demo.1/manifest_feature_3").await,
      "写时晋升：打开本身不上传当前 term 对象"
    );
    attached.put(&tbl, &[entry(b"b", b"B")]).await.unwrap();
    assert!(remote_has(&remote, "fkv/demo.1/manifest_feature_3").await);

    // 冷启动（全新本地 cache）：经当前 term 名 `manifest_feature_3` 直读。
    let mut reopened = store(&remote);
    reopened.open_branch(&tbl, "feature", 3).await.unwrap();
    assert_eq!(reopened.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(reopened.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));

    // 更高 term 冷启动：当前名 `_5` 未命中，回退链选中 `_3` 采纳后续写。
    let mut newer = store(&remote);
    newer.open_branch(&tbl, "feature", 5).await.unwrap();
    assert_eq!(newer.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
    newer.put(&tbl, &[entry(b"c", b"C")]).await.unwrap();
    assert!(remote_has(&remote, "fkv/demo.1/manifest_feature_5").await);
    assert_eq!(newer.get(&tbl, b"c").await.unwrap(), Some(b"C".to_vec()));
  });
}

/// ② 远端只有 `manifest_B_0`（CreateBranch 产物）时：回退选中 `_0`，本地种子
/// 萅当前名（不发布），首写整份上传为 `manifest_B_T`。
#[test]
fn fallback_adoption_seeds_current_term_name_locally() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    setup_branch(&remote, &tbl).await;

    let backend = cloud(&remote);
    backend.set_identity("feature", 4);
    let bytes = backend
      .read_branch_manifest(&tbl, "feature", 4)
      .await
      .unwrap()
      .expect("回退应选中 manifest_feature_0");
    assert!(!bytes.is_empty());
    // 写时晋升：远端不因读取出现 `_4`，本地 cache 已按当前名装种子。
    assert!(!remote_has(&remote, "fkv/demo.1/manifest_feature_4").await);
    assert!(
      backend
        .local()
        .read_manifest_file(&tbl, "manifest_feature_4")
        .await
        .unwrap()
        .is_some(),
      "晋升种子应装在当前 term 名下"
    );

    // 首写发布当前名；发布前 append 的回源也走回退链（种子被逐出的窗口）。
    let mut store = Store::new(backend, PAGE_SIZE, 16).with_pages_per_file_shift(SHIFT);
    store.open_branch(&tbl, "feature", 4).await.unwrap();
    store.put(&tbl, &[entry(b"b", b"B")]).await.unwrap();
    assert!(remote_has(&remote, "fkv/demo.1/manifest_feature_4").await);
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(store.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
  });
}

/// ③ 剪枝：`manifest_B_T` 成为远端最高名 term 后 `manifest_B_0` 可被回收；
/// 发布前（最高名 term 仍为 0）守卫不满足，不删任何对象。
#[test]
fn prune_reclaims_create_branch_manifest_after_promotion() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    setup_branch(&remote, &tbl).await;

    let backend = cloud(&remote);
    backend.set_identity("feature", 3);
    // 发布前：远端最高名 term 为 0 ≠ 当前 term 3，整轮跳过。
    assert_eq!(
      backend
        .prune_superseded_manifest_file_li(&tbl, "feature", 3)
        .await
        .unwrap(),
      0
    );
    assert!(remote_has(&remote, "fkv/demo.1/manifest_feature_0").await);

    // 首写发布 `_3` 后：`_0` 被 `_3` 取代，可回收。
    backend
      .write_manifest_file(&tbl, "manifest_feature_3", b"record")
      .await
      .unwrap();
    assert_eq!(
      backend
        .prune_superseded_manifest_file_li(&tbl, "feature", 3)
        .await
        .unwrap(),
      1
    );
    assert!(!remote_has(&remote, "fkv/demo.1/manifest_feature_0").await);
    assert!(remote_has(&remote, "fkv/demo.1/manifest_feature_3").await);
  });
}

/// ④ 反向对照：Local 模式（term 0）manifest 名仍为 `manifest_B_0`，不出现
/// 其它 term 名。
#[test]
fn local_branch_manifest_name_stays_term_zero() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut main = Store::new(
      MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
      PAGE_SIZE,
      16,
    )
    .with_pages_per_file_shift(SHIFT);
    main.put(&tbl, &[entry(b"a", b"A")]).await.unwrap();
    main.create_branch(&tbl, "feature").await.unwrap();
    let io = main.into_io();

    let mut branch = Store::new(io, PAGE_SIZE, 16).with_pages_per_file_shift(SHIFT);
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    branch.put(&tbl, &[entry(b"b", b"B")]).await.unwrap();

    let name_li = branch
      .page_io()
      .list_manifest_file_name_li(&tbl)
      .await
      .unwrap();
    let feature_li: Vec<&str> = name_li
      .iter()
      .map(String::as_str)
      .filter(|name| name.starts_with("manifest_feature_"))
      .collect();
    assert_eq!(feature_li, vec!["manifest_feature_0"]);
    assert_eq!(branch.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
  });
}

/// 回退选中 term 高于 attach term：更高 term 的 owner 已接管，`ExpiredTerm`
/// 栅栏拒绝采纳（对齐 C++ `GetManifest` 的 selected_term > process_term）。
#[test]
fn fallback_fences_stale_owner_with_expired_term() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    setup_branch(&remote, &tbl).await;
    // 新 owner 已在 term 9 发布。
    let backend = cloud(&remote);
    backend.set_identity("feature", 9);
    backend
      .write_manifest_file(&tbl, "manifest_feature_9", b"record")
      .await
      .unwrap();

    // 旧 term 5 的打开：当前名 miss、回退发现 term 9 > 5 → ExpiredTerm。
    let mut stale = store(&remote);
    assert_eq!(
      stale.open_branch(&tbl, "feature", 5).await.unwrap_err(),
      KvError::ExpiredTerm
    );
  });
}

/// drop 不复活：采纳后未发布（远端只有 `_0`）时 drop 必须连同 `_0` 一起删，
/// 否则 reopen 按回退链重新选中已 Drop 的表。
#[test]
fn drop_after_adoption_does_not_resurrect_branch() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    setup_branch(&remote, &tbl).await;

    let mut attached = store(&remote);
    attached.open_branch(&tbl, "feature", 3).await.unwrap();
    attached.drop_table(&tbl).await.unwrap();
    assert!(!remote_has(&remote, "fkv/demo.1/manifest_feature_0").await);
    assert!(!remote_has(&remote, "fkv/demo.1/manifest_feature_3").await);

    let mut reopened = store(&remote);
    assert_eq!(
      reopened.open_branch(&tbl, "feature", 3).await.unwrap_err(),
      KvError::ResourceMissing,
      "已 Drop 的 branch 不得经回退链复活"
    );
  });
}

/// ⑦ branch drop 遇列举失败（sem-63）：drop 返回 Err、远端对象一个未少
/// （零删除）、表状态仍被 evict（失败也走统一收尾）；移除注入后重试 drop
/// 完成全 term 清扫，reopen `ResourceMissing`。重试成立的前提是 evict 不
/// 丢弃 branch 身份：`manifest_file` 等是 Store 级状态，drop 不依赖表已加载。
#[test]
fn branch_drop_list_failure_deletes_nothing_evicts_then_retry_completes() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    setup_branch(&remote, &tbl).await;

    let mut attached = store(&remote);
    attached.open_branch(&tbl, "feature", 3).await.unwrap();
    // 首写发布 manifest_feature_3：drop 前远端同时存在 `_0`（CreateBranch）
    // 与 `_3`（写时晋升）两个对象。
    attached.put(&tbl, &[entry(b"b", b"B")]).await.unwrap();
    let remote_key_li = |remote: &MemObjectStore| {
      let remote = remote.clone();
      async move {
        let mut li: Vec<String> = remote
          .list("fkv/demo.1/")
          .await
          .unwrap()
          .into_iter()
          .map(|meta| meta.key)
          .collect();
        li.sort();
        li
      }
    };
    let before_li = remote_key_li(&remote).await;
    assert!(
      before_li.contains(&"fkv/demo.1/manifest_feature_0".to_owned())
        && before_li.contains(&"fkv/demo.1/manifest_feature_3".to_owned()),
      "前提：drop 前远端有两个 term 的 manifest 对象: {before_li:?}"
    );

    // 注入列举失败（等价模拟云端 ListObjects 网络错误）。
    fkv_failpoint::set_spec("CloudManifest:ListFail@1");
    let err = attached.drop_table(&tbl).await.unwrap_err();
    fkv_failpoint::reset();
    assert!(
      matches!(err, KvError::IoFail),
      "列举失败应经统一收尾上抛: {err:?}"
    );
    // 远端对象一个未少：列举失败必须跳过整个删除阶段（含 own 幂等删）——
    // 没有列举就无法区分「仅剩 own」与「还有旧 term」，仅删 own 会让漏删
    // 的旧 term 对象把已 Drop 的表经回退链复活（⑥ 锁定）。
    let after_li = remote_key_li(&remote).await;
    assert_eq!(after_li, before_li, "列举失败时零删除，远端对象一个未少");
    assert!(!attached.is_open(&tbl), "失败也 evict：不得留下脏的表状态");

    // 移除注入后重试 drop：同一 Store（branch 身份未被 evict 丢弃）重新
    // 列举并完成全 term 清扫——即无注入的正常 drop 全删对照。data 文件与
    // main 的 manifest 不在 branch drop 的删除面（由 FileGc 按 retained
    // set 回收），只断言 branch manifest 全删。
    attached.drop_table(&tbl).await.unwrap();
    assert!(
      !remote_key_li(&remote)
        .await
        .iter()
        .any(|key| key.contains("manifest_feature")),
      "重试 drop 必须删尽同 branch 全部 term 的 manifest 对象"
    );

    let mut reopened = store(&remote);
    assert_eq!(
      reopened.open_branch(&tbl, "feature", 3).await.unwrap_err(),
      KvError::ResourceMissing,
      "重试 drop 完成清扫后 reopen 不得复活"
    );
  });
}
