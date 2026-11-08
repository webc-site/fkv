use std::{path::Path, sync::Arc};

use bytes::Bytes;
use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_io::ManifestIo;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_metrics::ShardMetric;
use fkv_object::{MemObjectStore, ObjectStore};
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, TableIdent, manifest_file_name};

const PAGE_SIZE: usize = 512;
const GARBAGE: [u8; 64] = [0xAB; 64];

fn cloud_over(remote: &MemObjectStore) -> Store<CloudStore<MemStore, MemObjectStore>> {
  Store::new(
    CloudStore::new(
      MemStore::new(PAGE_SIZE),
      remote.clone(),
      "fkv",
      DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    PAGE_SIZE,
    16,
  )
}

/// 生产本地 cache 形态：CompioStore 持有本地目录，CloudStore 在其上做回源 /
/// 驱逐。本地文件名同样按 `(file_id, branch, term)` 组织，读路径不能只按
/// file id / term 降序推断内容归属。
fn cloud_over_compio(
  root: &Path,
  remote: &MemObjectStore,
) -> Store<CloudStore<CompioStore, MemObjectStore>> {
  Store::new(
    CloudStore::new(
      CompioStore::new(root.to_path_buf(), PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT),
      remote.clone(),
      "fkv",
      DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    PAGE_SIZE,
    16,
  )
}

/// 把远端 manifest 对象改写成「干净快照 + 垃圾尾」，返回干净字节。
async fn pollute_remote(remote: &MemObjectStore, key: &str) -> Bytes {
  let clean = remote.get(key).await.unwrap().unwrap();
  let mut polluted = clean.to_vec();
  polluted.extend_from_slice(&GARBAGE);
  remote
    .put(key, Bytes::from(polluted), None, false)
    .await
    .unwrap();
  clean
}

/// sto-51 残余：Cloud 具名 branch manifest 的撕裂 / 垃圾尾必须在 replay 后
/// 截断本地 cache，否则下一次 append 会把尾巴整份 `put` 回远端。
///
/// 触发态是**远端对象本身**带垃圾尾（旧版本把撕裂 cache 整份上传，或从撕裂
/// 快照恢复）。`read_manifest_file` 以远端为准，本地 cache 与远端一致时不会
/// 改写；replay 停在最后一条完整 record，`valid_len < 文件长度`。若
/// `CloudStore::reset_manifest_file` 保持默认 no-op，本地仍带垃圾尾，
/// `append_manifest_file` 会在垃圾之后追加并整文件上传，远端永久固化污染。
#[test]
fn branch_manifest_torn_tail_is_truncated_before_append() {
  pollster::block_on(async {
    let tbl = TableIdent::new("tornbranch", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut main = cloud_over(&remote);
    main
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    main.create_branch(&tbl, "feature").await.unwrap();
    drop(main);

    let name = manifest_file_name("feature", 0);
    let key = format!("fkv/{tbl}/{name}");
    let clean = pollute_remote(&remote, &key).await;
    let valid_len = clean.len();

    // 新进程：本地 cache 为空，读取时被远端（含垃圾尾）覆写。
    let mut branch = cloud_over(&remote);
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    let cached = branch
      .page_io()
      .local()
      .read_manifest_file(&tbl, &name)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(
      cached.len(),
      valid_len,
      "reopen 应把本地具名 manifest cache 截到最后一个有效 record"
    );

    // 截断后追加：整文件上传必须只含干净前缀 + 新 record。
    branch
      .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)])
      .await
      .unwrap();
    let uploaded = remote.get(&key).await.unwrap().unwrap();
    assert!(uploaded.len() > valid_len, "append 应产生新 record");
    assert_eq!(&uploaded[..valid_len], &clean[..], "上传前缀必须是干净快照");
    assert!(
      !uploaded
        .windows(GARBAGE.len())
        .any(|window| window == GARBAGE),
      "append 上传不得携带撕裂 / 垃圾尾"
    );
    assert_eq!(branch.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(branch.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
  });
}

/// 活跃 manifest 与具名 manifest 同契约：远端对象带垃圾尾时，reopen 的
/// `reset_manifest` 必须把本地 cache 截回 `valid_len`；否则下一次
/// `append_manifest` 会把垃圾尾重新上传成远端权威。
#[test]
fn active_manifest_torn_tail_is_truncated_before_append() {
  pollster::block_on(async {
    let tbl = TableIdent::new("tornactive", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut main = cloud_over(&remote);
    main
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    drop(main);

    let name = manifest_file_name("main", 0);
    let key = format!("fkv/{tbl}/{name}");
    let clean = pollute_remote(&remote, &key).await;
    let valid_len = clean.len();

    let mut reopened = cloud_over(&remote);
    reopened.reopen(&tbl).await.unwrap();
    let cached = reopened
      .page_io()
      .local()
      .read_manifest(&tbl)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(
      cached.len(),
      valid_len,
      "reopen 应把本地活跃 manifest cache 截到最后一个有效 record"
    );

    reopened
      .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)])
      .await
      .unwrap();
    let uploaded = remote.get(&key).await.unwrap().unwrap();
    assert!(uploaded.len() > valid_len, "append 应产生新 record");
    assert_eq!(&uploaded[..valid_len], &clean[..], "上传前缀必须是干净快照");
    assert!(
      !uploaded
        .windows(GARBAGE.len())
        .any(|window| window == GARBAGE),
      "append 上传不得携带撕裂 / 垃圾尾"
    );
    assert_eq!(reopened.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(reopened.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
  });
}

#[test]
fn cloud_archive_and_branch_manifest_files() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
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
    .with_branch("main", 2);
    store
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    let archive = store.create_archive(&tbl, "snap", 5).await.unwrap();
    assert_eq!(archive.name, "manifest_main_2_snap");
    assert!(
      remote
        .get("fkv/demo.1/manifest_main_2_snap")
        .await
        .unwrap()
        .is_some()
    );

    let branch = store.create_branch(&tbl, "feature").await.unwrap();
    assert_eq!(branch.name, "manifest_feature_0");
    assert!(
      remote
        .get("fkv/demo.1/manifest_feature_0")
        .await
        .unwrap()
        .is_some()
    );
    assert_eq!(store.delete_branch(&tbl, "feature").await.unwrap(), 1);
    assert!(
      remote
        .get("fkv/demo.1/manifest_feature_0")
        .await
        .unwrap()
        .is_none()
    );
    // active branch manifest 与数据不受影响。
    assert!(
      remote
        .get("fkv/demo.1/manifest_main_2")
        .await
        .unwrap()
        .is_some()
    );
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  });
}

/// io-12：同一实例先后重开不同 term 的归档，本地 cache 不得按 file id 复用
/// 另一 term 的数据文件。
///
/// 场景（term 3 与 term 5 的归档都引用 file 0，两代内容不同）：
/// 1. 冷启动实例重开 `v1`（term 5 快照）→ 本地 cache 的 file 0 是 term 5 内容；
/// 2. 同一实例再重开 `only3`（term 3 快照）→ 必须回源读 term 3 对象，读到
///    `old`，不能命中本地残留的 term 5 页（`new`）；
/// 3. 再切回 `v1` → 必须重新回源 term 5 对象，双向都不能被反向污染。
#[test]
fn reopen_archive_same_instance_validates_local_cache_identity() {
  pollster::block_on(async {
    let tbl = TableIdent::new("archalias", 1).unwrap();
    let remote = MemObjectStore::new();

    // term 3：写 a=old，落 only3 归档。
    let mut t3 = cloud_over(&remote).with_branch("main", 3);
    t3.put(
      &tbl,
      &[BatchEntry::upsert(b"a".to_vec(), b"old".to_vec(), 1)],
    )
    .await
    .unwrap();
    t3.create_archive(&tbl, "only3", 0).await.unwrap();
    drop(t3);

    // term 5：同 file id 写入 a=new，落 v1 归档。
    let mut t5 = cloud_over(&remote).with_branch("main", 5);
    t5.put(
      &tbl,
      &[BatchEntry::upsert(b"a".to_vec(), b"new".to_vec(), 2)],
    )
    .await
    .unwrap();
    t5.create_archive(&tbl, "v1", 0).await.unwrap();
    drop(t5);

    // 同一实例：先 v1（term 5）后 only3（term 3），再切回 v1。
    let mut store = cloud_over(&remote).with_branch("main", 6);
    store.reopen_archive(&tbl, "v1").await.unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"new".to_vec()));

    store.reopen_archive(&tbl, "only3").await.unwrap();
    assert_eq!(
      store.get(&tbl, b"a").await.unwrap(),
      Some(b"old".to_vec()),
      "重开旧 term 归档必须读到该 term 的数据，不能命中本地 cache 里新 term 的同 id 文件"
    );

    store.reopen_archive(&tbl, "v1").await.unwrap();
    assert_eq!(
      store.get(&tbl, b"a").await.unwrap(),
      Some(b"new".to_vec()),
      "切回新 term 归档同样必须重新校验本地 cache 身份"
    );
  });
}

/// 同 io-12，但本地 cache 换成生产形态的 [`CompioStore`]（文件名自带
/// `(file_id, branch, term)`）。
///
/// 这里的关键差异：下载远端 term 5 对象时，本地后端按**当前** identity
/// （冷启动的 term 6）落文件名，内容归属只有 CloudStore 的记账知道。切到
/// term 3 归档时若只按「当前 identity 优先、其它按 term 降序」推断，同 id 的
/// 本地文件仍会命中错误 term 的内容。
#[compio::test]
async fn reopen_archive_same_instance_validates_local_cache_identity_compio() {
  let tbl = TableIdent::new("archaliasc", 1).unwrap();
  let remote = MemObjectStore::new();
  // 三个生命周期各自的本地 cache 目录（生产上不同进程的 cache 根不同）。
  let root3 = TempRoot::new("archalias_t3");
  let root5 = TempRoot::new("archalias_t5");
  let root6 = TempRoot::new("archalias_t6");

  let mut t3 = cloud_over_compio(root3.path(), &remote).with_branch("main", 3);
  t3.put(
    &tbl,
    &[BatchEntry::upsert(b"a".to_vec(), b"old".to_vec(), 1)],
  )
  .await
  .unwrap();
  t3.create_archive(&tbl, "only3", 0).await.unwrap();
  drop(t3);

  let mut t5 = cloud_over_compio(root5.path(), &remote).with_branch("main", 5);
  t5.put(
    &tbl,
    &[BatchEntry::upsert(b"a".to_vec(), b"new".to_vec(), 2)],
  )
  .await
  .unwrap();
  t5.create_archive(&tbl, "v1", 0).await.unwrap();
  drop(t5);

  let mut store = cloud_over_compio(root6.path(), &remote).with_branch("main", 6);
  store.reopen_archive(&tbl, "v1").await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"new".to_vec()));

  store.reopen_archive(&tbl, "only3").await.unwrap();
  assert_eq!(
    store.get(&tbl, b"a").await.unwrap(),
    Some(b"old".to_vec()),
    "CompioStore 本地 cache 同样必须按 manifest 的 per-file 归属校验"
  );

  store.reopen_archive(&tbl, "v1").await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"new".to_vec()));
}

/// io-12（性能面）：冷启动扫盘登记的本地副本在 replay 后按 per-file 归属补记
/// identity，正常重启（term 抬升）仍命中本地 cache，不因新增校验整份回源。
#[compio::test]
async fn restart_after_term_bump_keeps_local_cache_hits() {
  let tbl = TableIdent::new("archwarm", 1).unwrap();
  let remote = MemObjectStore::new();
  let root = TempRoot::new("archwarm");

  let mut t5 = cloud_over_compio(root.path(), &remote).with_branch("main", 5);
  t5.put(
    &tbl,
    &[BatchEntry::upsert(b"a".to_vec(), b"old".to_vec(), 1)],
  )
  .await
  .unwrap();
  t5.create_archive(&tbl, "v1", 0).await.unwrap();
  drop(t5);

  // term 6 冷启动：同一本地 cache 目录，reopen 后读已缓存页必须命中。
  let metrics = Arc::new(ShardMetric::new(0));
  let cloud = CloudStore::new(
    CompioStore::new(
      root.path().to_path_buf(),
      PAGE_SIZE,
      DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    remote,
    "fkv",
    DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_metrics(metrics.clone());
  let mut store = Store::new(cloud, PAGE_SIZE, 16).with_branch("main", 6);
  store.reopen(&tbl).await.unwrap();
  let before = metrics.snapshot();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"old".to_vec()));
  let after = metrics.snapshot();
  assert_eq!(
    after.cache_miss, before.cache_miss,
    "重启后本地副本归属应被归属链补记，读页不得回源"
  );
  assert!(after.cache_hit > before.cache_hit);
}

/// sem-12：归档按 tag 的 term 选择与删除范围（有意差异，登记见
/// eloqstore2fkv/COMPAT_BASELINE「缺失快照的重开语义」）：
///
/// - C++ `CloudStoreMgr::RefreshManifest(tbl, tag)` 只按 `process_term` 精确
///   匹配 `manifest_<branch>_<process_term>_<tag>`，下载失败直接报错；
///   `DeleteArchive` 也只 unlink 当前 process term 一份。
/// - fkv 的归档跨 term 保留（`term` 是嵌入式单 attach 的 attach term，不是 C++
///   的 process term）：`reopen_archive` 取**最高 term**，`delete_archive` 删该
///   tag 的**全部 term**。
///
/// 本用例把两侧差异锁在行为上：只有旧 term 归档时仍可重开（C++ process_term
/// 匹配会 `NotFound`），同 tag 多 term 时选最高 term，删除覆盖全部 term。
#[test]
fn reopen_archive_selects_highest_term_and_delete_removes_all_terms() {
  pollster::block_on(async {
    let tbl = TableIdent::new("archterm", 1).unwrap();
    let remote = MemObjectStore::new();
    let key = |name: &str| format!("fkv/{tbl}/{name}");

    // term 3：写 a=old，落 v1 / only3 两份归档。
    let mut t3 = cloud_over(&remote).with_branch("main", 3);
    t3.put(
      &tbl,
      &[BatchEntry::upsert(b"a".to_vec(), b"old".to_vec(), 1)],
    )
    .await
    .unwrap();
    assert_eq!(
      t3.create_archive(&tbl, "v1", 0).await.unwrap().name,
      "manifest_main_3_v1"
    );
    assert_eq!(
      t3.create_archive(&tbl, "only3", 0).await.unwrap().name,
      "manifest_main_3_only3"
    );
    drop(t3);

    // term 5：同 tag 再落一份归档（a=new）。
    let mut t5 = cloud_over(&remote).with_branch("main", 5);
    t5.put(
      &tbl,
      &[BatchEntry::upsert(b"a".to_vec(), b"new".to_vec(), 2)],
    )
    .await
    .unwrap();
    assert_eq!(
      t5.create_archive(&tbl, "v1", 0).await.unwrap().name,
      "manifest_main_5_v1"
    );
    drop(t5);

    // 冷启动 term 6（C++ 只认 process_term=6，三份归档都不可达）：
    // fkv 选最高 term 的 v1 → term 5 快照。
    let mut cold = cloud_over(&remote).with_branch("main", 6);
    cold.reopen_archive(&tbl, "v1").await.unwrap();
    assert_eq!(cold.get(&tbl, b"a").await.unwrap(), Some(b"new".to_vec()));

    // 另一个冷启动实例重开只有 term 3 的归档：同样成功（跨 term 更宽容）。
    // 用独立实例避免本地 cache 的 file id 复用干扰（见 issue
    // `io-12-cloud-archive-reopen-stale-term-cache`）。
    let mut cold_old = cloud_over(&remote).with_branch("main", 6);
    cold_old.reopen_archive(&tbl, "only3").await.unwrap();
    assert_eq!(
      cold_old.get(&tbl, b"a").await.unwrap(),
      Some(b"old".to_vec())
    );

    // 删除按 tag 扫全部 term：v1 的两份都消失（C++ 只删当前 term 一份）。
    assert_eq!(cold.delete_archive(&tbl, "v1").await.unwrap(), 2);
    assert!(
      remote
        .get(&key("manifest_main_3_v1"))
        .await
        .unwrap()
        .is_none()
    );
    assert!(
      remote
        .get(&key("manifest_main_5_v1"))
        .await
        .unwrap()
        .is_none()
    );
    // 其它 tag 不受影响；重复删除幂等。
    assert!(
      remote
        .get(&key("manifest_main_3_only3"))
        .await
        .unwrap()
        .is_some()
    );
    assert_eq!(cold.delete_archive(&tbl, "v1").await.unwrap(), 0);
  });
}
