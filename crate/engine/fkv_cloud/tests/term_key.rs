//! 云端对象 key 必须按 manifest 记录的 per-file `(branch, term)` 解析。

use bytes::Bytes;
use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_error::KvError;
use fkv_io::{FileIo, ManifestIo, PageIo};
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_object::{MemObjectStore, ObjectStore};
use fkv_store::Store;
use fkv_types::{BranchFileRange, InstallOutcome, TableIdent};

const SHIFT: u8 = 2;
const PAGE_SIZE: usize = 512;
const SEGMENT_SIZE: usize = 4096;
const SEGMENTS_PER_FILE_SHIFT: u8 = 1;

fn page(fill: u8) -> Vec<u8> {
  vec![fill; PAGE_SIZE]
}

fn range(branch: &str, term: u64, max_file_id: u64) -> BranchFileRange {
  BranchFileRange {
    branch_name: branch.into(),
    term,
    max_file_id,
    max_segment_file_id: 0,
  }
}

fn cloud_with_segment(remote: &MemObjectStore) -> CloudStore<MemStore, MemObjectStore> {
  CloudStore::new(
    MemStore::new(PAGE_SIZE).with_segment_config(SEGMENT_SIZE, SEGMENTS_PER_FILE_SHIFT),
    remote.clone(),
    "fkv",
    SHIFT,
  )
  .with_segment_config(SEGMENT_SIZE, SEGMENTS_PER_FILE_SHIFT)
}

fn cloud(remote: &MemObjectStore) -> CloudStore<MemStore, MemObjectStore> {
  CloudStore::new(
    MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
    remote.clone(),
    "fkv",
    SHIFT,
  )
}

fn store_with_term(
  remote: &MemObjectStore,
  term: u64,
) -> Store<CloudStore<MemStore, MemObjectStore>> {
  Store::new(cloud(remote), PAGE_SIZE, 16)
    .with_pages_per_file_shift(SHIFT)
    .with_branch("main", term)
}

async fn remote_key_li(remote: &MemObjectStore, tbl: &TableIdent) -> Vec<String> {
  remote
    .list(&format!("fkv/{tbl}/"))
    .await
    .unwrap()
    .into_iter()
    .map(|meta| meta.key)
    .collect()
}

/// 旧 term 的页按写时 key 读取；超出链尾的 file id 回退当前 identity。
#[test]
fn read_page_resolves_per_file_branch_term() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    remote
      .put(
        "fkv/demo.1/data_0_main_3",
        Bytes::from(page(7)),
        None,
        false,
      )
      .await
      .unwrap();
    remote
      .put(
        "fkv/demo.1/data_1_feature_4",
        Bytes::from(page(9)),
        None,
        false,
      )
      .await
      .unwrap();

    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
      remote,
      "fkv",
      SHIFT,
    );
    cloud.set_file_identity("feature", 4);
    cloud.set_file_range_li(&tbl, &[range("main", 3, 0)]);

    // file 0 属于旧 term，必须走 data_0_main_3。
    let mut dst = vec![0u8; PAGE_SIZE];
    cloud.read_page(&tbl, 0, &mut dst).await.unwrap();
    assert_eq!(dst[0], 7);

    // file 1 在链尾之外（当前 term 新分配），回退到当前 identity。
    cloud.read_page(&tbl, 1 << SHIFT, &mut dst).await.unwrap();
    assert_eq!(dst[0], 9);
  });
}

/// Store 安装 replay 状态后把 file_range_li 同步给后端：分支读父分支数据。
#[test]
fn branch_reopen_reads_parent_pages_via_parent_term_key() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut main = Store::new(
      CloudStore::new(
        MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
        remote.clone(),
        "fkv",
        SHIFT,
      ),
      PAGE_SIZE,
      16,
    );
    main
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    main.create_branch(&tbl, "feature").await.unwrap();
    drop(main);

    // 全新本地 cache：读父分支页只能按 manifest 的 (branch, term) 拼远端 key。
    let mut branch = Store::new(
      CloudStore::new(
        MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
        remote,
        "fkv",
        SHIFT,
      ),
      PAGE_SIZE,
      16,
    );
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(branch.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(branch.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
  });
}

/// 跨 term manifest 选择：同 branch 取最大 term 的无 tag manifest，
/// archive / 其它 branch 不参与；选中内容安装为本地活跃 manifest。
#[test]
fn read_manifest_selects_latest_term_for_branch() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 7).unwrap();
    let remote = MemObjectStore::new();
    for (name, body) in [
      ("manifest_main_1", b"old".as_slice()),
      ("manifest_main_4", b"new".as_slice()),
      ("manifest_main_9_snap", b"archive".as_slice()),
      ("manifest_feature_8", b"branch".as_slice()),
    ] {
      remote
        .put(
          &format!("fkv/{tbl}/{name}"),
          Bytes::copy_from_slice(body),
          None,
          false,
        )
        .await
        .unwrap();
    }
    let cloud = cloud(&remote);
    cloud.set_identity("main", 6);

    assert_eq!(
      ManifestIo::read_manifest(&cloud, &tbl)
        .await
        .unwrap()
        .unwrap(),
      b"new"
    );
    assert_eq!(
      cloud.local().read_manifest(&tbl).await.unwrap().unwrap(),
      b"new"
    );
  });
}

/// sem-04：回退选择命中**高于本 shard term** 的无 tag manifest 时，惰性读
/// 路径（对齐 C++ `GetManifest`）返回 `ExpiredTerm`，且在下载 / 采纳之前
/// 失败——本地 cache 不被写入；再次读取仍快速失败。
#[test]
fn read_manifest_fences_higher_term_manifest() {
  pollster::block_on(async {
    let tbl = TableIdent::new("fence", 1).unwrap();
    let remote = MemObjectStore::new();
    remote
      .put(
        &format!("fkv/{tbl}/manifest_main_7"),
        Bytes::from_static(b"future"),
        None,
        false,
      )
      .await
      .unwrap();
    let cloud = cloud(&remote);
    cloud.set_identity("main", 6);

    assert_eq!(
      ManifestIo::read_manifest(&cloud, &tbl).await.unwrap_err(),
      KvError::ExpiredTerm
    );
    assert!(
      cloud.local().read_manifest(&tbl).await.unwrap().is_none(),
      "栅栏必须在下载 / 采纳之前生效，本地 cache 不得写入未来 term 快照"
    );
    // 同一选择结果的后续读取仍快速失败。
    assert_eq!(
      ManifestIo::read_manifest(&cloud, &tbl).await.unwrap_err(),
      KvError::ExpiredTerm
    );
  });
}

/// sem-04 反向：`selected_term == self.term()` 仍按原行为采纳（更低 term 的
/// 跨 term 采纳由 `read_manifest_selects_latest_term_for_branch` 覆盖）。
#[test]
fn read_manifest_accepts_equal_term_manifest() {
  pollster::block_on(async {
    let tbl = TableIdent::new("equal", 1).unwrap();
    let remote = MemObjectStore::new();
    remote
      .put(
        &format!("fkv/{tbl}/manifest_main_6"),
        Bytes::from_static(b"same-term"),
        None,
        false,
      )
      .await
      .unwrap();
    let cloud = cloud(&remote);
    cloud.set_identity("main", 6);

    assert_eq!(
      ManifestIo::read_manifest(&cloud, &tbl)
        .await
        .unwrap()
        .unwrap(),
      b"same-term"
    );
    assert_eq!(
      cloud.local().read_manifest(&tbl).await.unwrap().unwrap(),
      b"same-term"
    );
  });
}

/// sem-04 端到端（Store reopen，惰性读 / `GetManifest` 等价路径）：远端最新
/// manifest 属于更高 term owner 时 `reopen` 返回 `ExpiredTerm`，本地 table
/// 状态被驱逐；再次 reopen 仍快速失败。
#[test]
fn reopen_fences_higher_term_manifest() {
  pollster::block_on(async {
    let tbl = TableIdent::new("fence_store", 2).unwrap();
    let remote = MemObjectStore::new();
    // term 5 owner 写出的有效 manifest。
    let mut writer = store_with_term(&remote, 5);
    writer
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"V".to_vec(), 1)])
      .await
      .unwrap();
    drop(writer);

    // term 4 的过期 owner：看到 term 5 的 manifest 必须报错，不能采纳。
    let mut stale = store_with_term(&remote, 4);
    assert_eq!(stale.reopen(&tbl).await.unwrap_err(), KvError::ExpiredTerm);
    assert_eq!(stale.reopen(&tbl).await.unwrap_err(), KvError::ExpiredTerm);
  });
}

/// term A 写入、term B reopen：选择 term A 的远端 manifest，数据按 manifest
/// 记录的旧 term key 读取；新写入落到 term B 的对象。
#[test]
fn cross_term_reopen_reads_old_term_data() {
  pollster::block_on(async {
    let tbl = TableIdent::new("cross", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut writer = store_with_term(&remote, 3);
    writer
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    let key_li = remote_key_li(&remote, &tbl).await;
    assert!(
      key_li
        .iter()
        .any(|key| key == &format!("fkv/{tbl}/manifest_main_3")),
      "{key_li:?}"
    );
    assert!(
      key_li
        .iter()
        .any(|key| key.contains("/data_") && key.ends_with("_main_3")),
      "{key_li:?}"
    );
    drop(writer);

    let mut reader = store_with_term(&remote, 5);
    reader.reopen(&tbl).await.unwrap();
    assert_eq!(reader.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

    // 新写入使用进程 term（B），旧 term 对象保持不动。
    reader
      .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)])
      .await
      .unwrap();
    let key_li = remote_key_li(&remote, &tbl).await;
    assert!(
      key_li
        .iter()
        .any(|key| key.contains("/data_") && key.ends_with("_main_5")),
      "term B 写入未落到新对象: {key_li:?}"
    );
    assert_eq!(reader.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(reader.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
  });
}

/// 外部换主（`install_external_snapshot`）同样按最大 term 选 manifest：
/// 远端已推进到 term 3，follower 本地仍停在 term 0 也能采纳（RefreshManifest
/// 路径无采纳栅栏，见 COMPAT_BASELINE「云 manifest 采纳」）。
#[test]
fn install_external_snapshot_selects_latest_term() {
  pollster::block_on(async {
    let tbl = TableIdent::new("adopt", 3).unwrap();
    let remote = MemObjectStore::new();
    let mut writer = store_with_term(&remote, 3);
    writer
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"V".to_vec(), 1)])
      .await
      .unwrap();
    drop(writer);

    let mut follower = store_with_term(&remote, 0);
    assert_eq!(
      follower.install_external_snapshot(&tbl).await.unwrap(),
      InstallOutcome::Installed
    );
    assert_eq!(follower.get(&tbl, b"k").await.unwrap(), Some(b"V".to_vec()));
  });
}

/// 远端候选列举按 `data_` / `segment_` 前缀解析并保留同 branch 的旧 term；
/// 其它 branch 的对象不混入，prewarm 的分页列举仍只取当前 identity。
#[test]
fn remote_listing_includes_previous_terms_for_gc() {
  pollster::block_on(async {
    let tbl = TableIdent::new("list", 1).unwrap();
    let remote = MemObjectStore::new();
    for name in [
      "data_0_main_3",
      "data_1_main_5",
      "data_7_feature_4",
      "segment_0_main_3",
      "segment_2_main_4",
      "segment_9_feature_4",
    ] {
      remote
        .put(
          &format!("fkv/{tbl}/{name}"),
          Bytes::from_static(b"x"),
          None,
          false,
        )
        .await
        .unwrap();
    }
    let cloud = cloud(&remote);
    cloud.set_identity("main", 5);

    assert_eq!(cloud.list_file_id_li(&tbl).await.unwrap(), vec![0, 1]);
    assert_eq!(
      cloud.list_segment_file_id_li(&tbl).await.unwrap(),
      vec![0, 2]
    );
    let page = cloud.remote_data_file_page(&tbl, None, 0).await.unwrap();
    let file_id_li: Vec<u64> = page.entry_li.iter().map(|entry| entry.file_id).collect();
    assert_eq!(file_id_li, vec![1]);
  });
}

/// GC 必须能回收被淘汰的旧 term data / segment：第一轮剪掉旧 manifest，
/// 第二轮删除旧对象。
#[test]
fn gc_reclaims_superseded_term_objects() {
  pollster::block_on(async {
    let tbl = TableIdent::new("gc", 1).unwrap();
    let layout = SegmentLayout::new(SEGMENT_SIZE as u32, SEGMENTS_PER_FILE_SHIFT).unwrap();
    let remote = MemObjectStore::new();
    let mut writer = Store::new(cloud_with_segment(&remote), PAGE_SIZE, 16)
      .with_pages_per_file_shift(SHIFT)
      .with_branch("main", 3)
      .with_large_value(layout, 1024);
    let big = vec![7u8; 9000];
    writer
      .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 1)])
      .await
      .unwrap();
    writer
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    let key_li = remote_key_li(&remote, &tbl).await;
    assert!(
      key_li
        .iter()
        .any(|key| key.contains("/segment_") && key.ends_with("_main_3")),
      "{key_li:?}"
    );
    drop(writer);

    let mut reader = Store::new(cloud_with_segment(&remote), PAGE_SIZE, 16)
      .with_pages_per_file_shift(SHIFT)
      .with_branch("main", 5)
      .with_large_value(layout, 1024);
    reader.reopen(&tbl).await.unwrap();
    assert_eq!(
      reader.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      big
    );
    assert_eq!(reader.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

    // 清空映射后，旧 manifest 仍被 retained 集合引用：第一轮只剪 manifest。
    reader.truncate(&tbl).await.unwrap();
    reader.file_gc(&tbl, SHIFT).await.unwrap();
    let key_li = remote_key_li(&remote, &tbl).await;
    assert!(
      !key_li
        .iter()
        .any(|key| key == &format!("fkv/{tbl}/manifest_main_3")),
      "旧 term manifest 未被剪枝: {key_li:?}"
    );

    // 第二轮旧 manifest 已不可见，旧 term data / segment 对象回收。
    reader.file_gc(&tbl, SHIFT).await.unwrap();
    let key_li = remote_key_li(&remote, &tbl).await;
    assert!(
      key_li.iter().all(|key| !key.ends_with("_main_3")),
      "旧 term 对象未回收: {key_li:?}"
    );
    assert!(
      key_li
        .iter()
        .any(|key| key == &format!("fkv/{tbl}/manifest_main_5")),
      "当前 term manifest 丢失: {key_li:?}"
    );
  });
}
