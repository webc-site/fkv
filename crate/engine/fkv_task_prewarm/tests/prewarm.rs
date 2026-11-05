use core::{cell::Cell, task::Poll};
use std::slice::from_ref;

use bytes::Bytes;
use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_error::KvError;
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_object::{MemObjectStore, ObjectStore};
use fkv_store::Store;
use fkv_task_prewarm::{
  CompletionReason, prewarm_table, prewarm_tables, prewarm_tables_with_concurrency,
};
use fkv_types::{BranchFileRange, TableIdent};
use futures::future::poll_fn;

fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

fn cloud(local: MemStore, remote: MemObjectStore) -> CloudStore<MemStore, MemObjectStore> {
  CloudStore::new(
    local,
    remote,
    "fkv",
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 1)
}

/// 用真实写路径在远端落一份可解析的 manifest（预热只认远端权威）。
///
/// `prewarm_table` 在 manifest 缺失时直接返回（第 7 项），因此列举 / 下载
/// 语义的用例都必须先有 manifest。
async fn seed_manifest(remote: &MemObjectStore, tbl: &TableIdent) {
  let mut store = Store::new(cloud(MemStore::new(512), remote.clone()), 512, 16);
  store
    .put(tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
    .await
    .unwrap();
}

/// 远端该表目录下的 data 对象数（按对象名统计，不依赖本地 cache）。
async fn remote_data_num(remote: &MemObjectStore, tbl: &TableIdent) -> usize {
  remote
    .list(&format!("fkv/{tbl}/"))
    .await
    .unwrap()
    .iter()
    .filter(|meta| {
      meta
        .key
        .rsplit('/')
        .next()
        .is_some_and(|name| name.starts_with("data_"))
    })
    .count()
}

#[test]
fn prewarm_pulls_manifest_data_and_segments() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 1).unwrap();
    let remote = MemObjectStore::new();
    let mut store = Store::new(
      cloud(
        MemStore::new(512).with_segment_config(4096, 1),
        remote.clone(),
      ),
      512,
      16,
    )
    .with_large_value(layout, 1024);
    store
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    let big = pattern(9000);
    store
      .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 2)])
      .await
      .unwrap();
    drop(store);

    let fresh = cloud(MemStore::new(512).with_segment_config(4096, 1), remote);
    assert!(
      fresh
        .local()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty()
    );
    let stats = prewarm_table(&fresh, &tbl).await.unwrap();
    assert!(stats.manifest_present);
    assert!(stats.data_file_num >= 1, "{stats:?}");
    assert!(stats.segment_file_num >= 2, "{stats:?}");
    assert_eq!(stats.missing_num, 0);
    assert_eq!(stats.files_skipped_missing, 0);
    assert_eq!(stats.files_failed, 0);
    assert_eq!(stats.completion_reason, CompletionReason::Success);
    assert_eq!(stats.total_files_pulled, stats.file_num());
    assert!(
      !fresh
        .local()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty()
    );
    assert!(
      !fresh
        .local()
        .list_segment_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty()
    );
    assert!(fresh.local().read_manifest(&tbl).await.unwrap().is_some());

    let mut store = Store::new(fresh, 512, 16).with_large_value(layout, 1024);
    store.reopen(&tbl).await.unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      big
    );
  });
}

#[test]
fn prewarm_missing_table_is_reported() {
  pollster::block_on(async {
    let tbl = TableIdent::new("missing", 1).unwrap();
    let fresh = cloud(MemStore::new(512), MemObjectStore::new());
    let stats = prewarm_table(&fresh, &tbl).await.unwrap();
    assert!(!stats.manifest_present);
    assert_eq!(stats.data_file_num, 0);
    assert_eq!(stats.segment_file_num, 0);
    assert_eq!(stats.missing_num, 1);

    let report = prewarm_tables(&fresh, from_ref(&tbl)).await.unwrap();
    assert_eq!(report.len(), 1);
    assert_eq!(report[0].0, tbl);
    assert_eq!(report[0].1.missing_num, 1);
  });
}

/// io-29：远端列举超过一页（1001 个 data 对象）时，逐页列举 + 下载的流水线
/// 仍要全部拉取，不重不漏。
#[test]
fn prewarm_streams_across_list_pages() {
  pollster::block_on(async {
    let tbl = TableIdent::new("paged", 1).unwrap();
    let remote = MemObjectStore::new();
    seed_manifest(&remote, &tbl).await;
    // 本地 cache 的 page_size = 512，整文件写入必须是页大小整数倍。
    let content = Bytes::from(vec![1u8; 512]);
    for file_id in 1..=1000 {
      let name = fkv_types::data_file_name(file_id, "main", 0);
      remote
        .put(&format!("fkv/{tbl}/{name}"), content.clone(), None, false)
        .await
        .unwrap();
    }
    let want = remote_data_num(&remote, &tbl).await;
    let fresh = cloud(MemStore::new(512), remote);
    let stats = prewarm_table(&fresh, &tbl).await.unwrap();
    assert_eq!(stats.data_file_num, want, "{stats:?}");
    assert!(want > 1000, "必须跨页：{want}");
    assert_eq!(stats.total_files_listed, want + 1);
    assert_eq!(stats.files_failed, 0);
    assert_eq!(stats.missing_num, 0);
    assert_eq!(stats.completion_reason, CompletionReason::Success);
  });
}

/// cld-47：下载前按 `cache_remained() < file_size` 预判磁盘余量，
/// 不足即中止本轮并置 `DiskFull`（对齐 C++ `prewarm_task.cpp:135`）。
#[test]
fn prewarm_stops_when_cache_budget_exhausted() {
  pollster::block_on(async {
    let tbl = TableIdent::new("disk_full", 1).unwrap();
    let remote = MemObjectStore::new();
    seed_manifest(&remote, &tbl).await;
    let content = Bytes::from(vec![1u8; 512]);
    // 再补两个 data 对象：manifest 里的首个文件 + 这两个，预算只够装一个。
    for file_id in 1..=2 {
      let name = fkv_types::data_file_name(file_id, "main", 0);
      remote
        .put(&format!("fkv/{tbl}/{name}"), content.clone(), None, false)
        .await
        .unwrap();
    }
    // 预算只够装 manifest 引用的首个文件（1024 B），后续文件即 DiskFull。
    let fresh = cloud(MemStore::new(512), remote).with_cache_budget(1024);
    let stats = prewarm_table(&fresh, &tbl).await.unwrap();
    assert_eq!(stats.completion_reason, CompletionReason::DiskFull);
    assert_eq!(stats.data_file_num, 1, "{stats:?}");
    assert_eq!(stats.segment_file_num, 0);
    assert_eq!(stats.files_failed, 0);
    assert_eq!(fresh.resident_bytes(), 1024);
    assert!(fresh.cache_remained() < 512);
  });
}

/// 表级并发可配（对齐 `KvOption::prewarm_task_count`）：`0` 也不能把流饿死，
/// 按 1 串行仍要把表全部拉完。
#[test]
fn prewarm_tables_with_zero_concurrency_still_pulls_all() {
  pollster::block_on(async {
    let remote = MemObjectStore::new();
    let tbl_li: Vec<TableIdent> = (1..=3).map(|p| TableIdent::new("t", p).unwrap()).collect();
    {
      let mut store = Store::new(cloud(MemStore::new(512), remote.clone()), 512, 16);
      for tbl in &tbl_li {
        store
          .put(tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)])
          .await
          .unwrap();
      }
    }
    let fresh = cloud(MemStore::new(512), remote);
    let report = prewarm_tables_with_concurrency(&fresh, &tbl_li, 0)
      .await
      .unwrap();
    assert_eq!(report.len(), 3);
    for (tbl, stats) in &report {
      assert!(stats.manifest_present, "{tbl} {stats:?}");
      assert!(stats.data_file_num >= 1, "{tbl} {stats:?}");
      assert_eq!(
        stats.completion_reason,
        CompletionReason::Success,
        "{tbl} {stats:?}"
      );
    }
  });
}

/// 只在 `data_` 对象 `get` 上失败的远端：用于验证「持久下载错误中止整轮」（cld-54）。
struct FailingDataStore {
  inner: MemObjectStore,
}

impl ObjectStore for FailingDataStore {
  async fn get(&self, key: &str) -> fkv_error::Result<Option<Bytes>> {
    if key.contains("/data_") {
      return Err(KvError::CloudErr);
    }
    self.inner.get(key).await
  }

  async fn put(
    &self,
    key: &str,
    value: Bytes,
    if_match: Option<&str>,
    if_none_match: bool,
  ) -> fkv_error::Result<String> {
    self.inner.put(key, value, if_match, if_none_match).await
  }

  async fn head(&self, key: &str) -> fkv_error::Result<Option<fkv_object::ObjectMeta>> {
    self.inner.head(key).await
  }

  async fn delete(&self, key: &str) -> fkv_error::Result<()> {
    self.inner.delete(key).await
  }

  async fn list(&self, prefix: &str) -> fkv_error::Result<Vec<fkv_object::ObjectMeta>> {
    self.inner.list(prefix).await
  }
}

/// cld-54：持久下载错误立即中止整轮（对齐 C++ `StopAllPrewarmTasks`），
/// 而不是把剩下的文件继续拉一遍。
#[test]
fn prewarm_aborts_on_persistent_download_error() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    seed_manifest(&remote, &tbl).await;
    // 直接在远端造 32 个 data 对象（绕开写路径，文件名走 `data_file_name`）。
    let content = Bytes::from(vec![7u8; 512]);
    for file_id in 1..=32 {
      let name = fkv_types::data_file_name(file_id, "main", 0);
      remote
        .put(&format!("fkv/{tbl}/{name}"), content.clone(), None, false)
        .await
        .unwrap();
    }

    let failing = CloudStore::new(
      MemStore::new(512),
      FailingDataStore {
        inner: remote.clone(),
      },
      "fkv",
      18,
    )
    .with_segment_config(4096, 1);
    let stats = prewarm_table(&failing, &tbl).await.unwrap();
    assert_eq!(stats.completion_reason, CompletionReason::DownloadError);
    assert_eq!(stats.data_file_num, 0, "失败时不该有拉取成功的文件");
    assert_eq!(stats.files_failed, 1, "应中止于首个持久错误：{stats:?}");
    assert!(
      stats.total_files_listed < 32,
      "中止后不该继续消费列举（listed={} / 共 32）",
      stats.total_files_listed
    );
  });
}

/// `Request::ListObject` 的底层路径：Cloud 后端把远端列举原样转给
/// `ObjectStore::list_page`。用 `MemObjectStore` 钉住分页 token 契约
/// （key 字节序升序、token 严格大于、末页 `next_token == None`）。
#[test]
fn cloud_remote_list_object_paginates_over_mem_object_store() {
  pollster::block_on(async {
    let remote = MemObjectStore::new();
    let content = Bytes::from(vec![3u8; 512]);
    let name_li = [
      "data_0_main_0",
      "data_1_main_0",
      "manifest_main_0",
      "segment_0_main_0",
    ];
    for name in name_li {
      remote
        .put(&format!("fkv/demo.1/{name}"), content.clone(), None, false)
        .await
        .unwrap();
    }
    // 同 bucket 下另一个分区：前缀必须是递归匹配，但不能串到别的分区。
    remote
      .put("fkv/demo.2/data_0_main_0", content.clone(), None, false)
      .await
      .unwrap();

    let cloud = cloud(MemStore::new(512), remote);
    let first = cloud
      .remote()
      .list_page("fkv/demo.1/", None, 2)
      .await
      .unwrap();
    assert_eq!(first.meta_li.len(), 2);
    assert_eq!(first.meta_li[0].key, "fkv/demo.1/data_0_main_0");
    assert_eq!(first.meta_li[1].key, "fkv/demo.1/data_1_main_0");
    assert_eq!(
      first.next_token.as_deref(),
      Some("fkv/demo.1/data_1_main_0")
    );

    let second = cloud
      .remote()
      .list_page("fkv/demo.1/", first.next_token.as_deref(), 2)
      .await
      .unwrap();
    assert_eq!(second.meta_li.len(), 2);
    assert_eq!(second.meta_li[0].key, "fkv/demo.1/manifest_main_0");
    assert_eq!(second.meta_li[1].key, "fkv/demo.1/segment_0_main_0");
    assert_eq!(second.next_token, None);

    // 空 prefix = 整个 bucket（C++ ListObject RemotePath 空串语义）。
    let all = cloud.remote().list_page("", None, 0).await.unwrap();
    assert_eq!(all.meta_li.len(), 5);
    assert_eq!(all.next_token, None);
  });
}

/// 子分支 manifest 名恒为 `manifest_<branch>_0`：换 term 后预热仍要选中它，
/// 并按 branch metadata 的 `file_range_li` 把父分支继承的数据一起拉下来。
#[test]
fn prewarm_branch_manifest_installs_and_pulls_parent_ranges() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut store = Store::new(
      cloud(
        MemStore::new(512).with_segment_config(4096, 1),
        remote.clone(),
      ),
      512,
      16,
    )
    .with_branch("main", 2);
    store
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    let branch = store.create_branch(&tbl, "feature").await.unwrap();
    assert_eq!(branch.name, "manifest_feature_0");
    drop(store);

    let fresh = cloud(MemStore::new(512).with_segment_config(4096, 1), remote);
    // attach term 与 branch manifest 名里的 term 无关（分支名恒 `_0`）。
    fresh.set_identity("feature", 2);
    let stats = prewarm_table(&fresh, &tbl).await.unwrap();
    assert!(stats.manifest_present, "{stats:?}");
    assert_eq!(
      stats.completion_reason,
      CompletionReason::Success,
      "{stats:?}"
    );
    assert!(stats.data_file_num >= 1, "{stats:?}");
    assert_eq!(stats.missing_num, 0);
    assert!(
      fresh
        .local()
        .read_manifest_file(&tbl, "manifest_feature_0")
        .await
        .unwrap()
        .is_some(),
      "branch manifest 应按本地具名文件安装"
    );
    assert!(
      !fresh
        .local()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty(),
      "父分支继承的 data 文件应被预热到本地"
    );
  });
}

/// 第 7 项：远端 manifest 缺失（表已被 Drop）时，预热不得按目录把残留
/// data / segment 整表拉回，并且必须作废旧 manifest 的 per-file 归属链。
///
/// 回退修复即失败：旧实现会继续列举下载（`data_file_num > 0`），且残留的
/// `file_range_li` 会让旧 term 对象继续被采纳。
#[test]
fn prewarm_skips_files_and_clears_range_when_manifest_missing() {
  pollster::block_on(async {
    let tbl = TableIdent::new("dropped", 1).unwrap();
    let remote = MemObjectStore::new();
    // 残留对象：当前 term 的 segment，以及旧 term（term 7）的 data —— 后者只有
    // 在 manifest 区间仍引用时才可见，正好用来验证归属链被清。
    remote
      .put(
        &format!("fkv/{tbl}/{}", fkv_types::data_file_name(0, "main", 7)),
        Bytes::from(vec![2u8; 512]),
        None,
        false,
      )
      .await
      .unwrap();
    remote
      .put(
        &format!("fkv/{tbl}/segment_0_main_0"),
        Bytes::from(vec![0u8; 4096]),
        None,
        false,
      )
      .await
      .unwrap();

    let fresh = cloud(MemStore::new(512), remote);
    // 旧 manifest 的归属链：file 0 属于 main/term7。
    fresh.set_file_range(
      &tbl,
      &[BranchFileRange {
        branch_name: "main".into(),
        term: 7,
        max_file_id: 0,
        max_segment_file_id: 0,
      }],
    );
    let page = fresh.remote_data_file_page(&tbl, None, 10).await.unwrap();
    assert!(
      page.entry_li.iter().any(|entry| entry.file_id == 0),
      "前置：有归属链时旧 term 对象会被采纳"
    );

    let stats = prewarm_table(&fresh, &tbl).await.unwrap();
    assert!(!stats.manifest_present);
    assert_eq!(stats.data_file_num, 0, "{stats:?}");
    assert_eq!(stats.segment_file_num, 0, "{stats:?}");
    assert_eq!(stats.total_files_listed, 0, "{stats:?}");
    assert!(
      fresh
        .local()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty(),
      "manifest 缺失时不得下载残留 data"
    );

    let page = fresh.remote_data_file_page(&tbl, None, 10).await.unwrap();
    assert!(
      page.entry_li.is_empty(),
      "manifest 缺失必须清掉旧 file_range，旧 term 对象不得再被采纳: {:?}",
      page.entry_li
    );
  });
}

/// 首次 `list_segment_file_id_li` 挂起一次的本地后端。
///
/// scheme 回填在持有组级回填权时列段文件，这里让它停在 await 上，逼出
/// 「其余并发安装抢不到回填权」的真实竞争（内存后端全部 Ready，不会交错）。
struct YieldListStore {
  inner: MemStore,
  yielded: Cell<bool>,
}

impl YieldListStore {
  fn new() -> Self {
    Self {
      inner: MemStore::new(512).with_segment_config(4096, 1),
      yielded: Cell::new(true),
    }
  }

  /// 本轮首次 poll 主动让出执行权。
  async fn yield_once(&self) {
    poll_fn(|cx| {
      if self.yielded.replace(false) {
        cx.waker().wake_by_ref();
        Poll::Pending
      } else {
        Poll::Ready(())
      }
    })
    .await
  }
}

impl FileIo for YieldListStore {
  async fn list_file_id_li(&self, tbl: &TableIdent) -> fkv_error::Result<Vec<fkv_types::FileId>> {
    self.inner.list_file_id_li(tbl).await
  }

  async fn read_file(
    &self,
    tbl: &TableIdent,
    file_id: fkv_types::FileId,
  ) -> fkv_error::Result<Option<Vec<u8>>> {
    self.inner.read_file(tbl, file_id).await
  }

  async fn write_file(
    &self,
    tbl: &TableIdent,
    file_id: fkv_types::FileId,
    bytes: &[u8],
  ) -> fkv_error::Result<()> {
    self.inner.write_file(tbl, file_id, bytes).await
  }

  async fn file_size(
    &self,
    tbl: &TableIdent,
    file_id: fkv_types::FileId,
  ) -> fkv_error::Result<Option<u64>> {
    self.inner.file_size(tbl, file_id).await
  }

  async fn remove_file(
    &self,
    tbl: &TableIdent,
    file_id: fkv_types::FileId,
  ) -> fkv_error::Result<()> {
    self.inner.remove_file(tbl, file_id).await
  }

  async fn list_segment_file_id_li(
    &self,
    tbl: &TableIdent,
  ) -> fkv_error::Result<Vec<fkv_types::FileId>> {
    self.yield_once().await;
    self.inner.list_segment_file_id_li(tbl).await
  }

  async fn remove_segment_file(
    &self,
    tbl: &TableIdent,
    file_id: fkv_types::FileId,
  ) -> fkv_error::Result<()> {
    self.inner.remove_segment_file(tbl, file_id).await
  }
}

impl SegmentIo for YieldListStore {
  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: fkv_types::FilePageId,
    dst: &mut [u8],
  ) -> fkv_error::Result<()> {
    self.inner.read_segment(tbl, segment_fp_id, dst).await
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: fkv_types::FilePageId,
    src: &[u8],
  ) -> fkv_error::Result<()> {
    self.inner.write_segment(tbl, segment_fp_id, src).await
  }
}

impl PageIo for YieldListStore {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: fkv_types::FilePageId,
    dst: &mut [u8],
  ) -> fkv_error::Result<()> {
    self.inner.read_page(tbl, file_page_id, dst).await
  }

  async fn write_page(
    &self,
    tbl: &TableIdent,
    file_page_id: fkv_types::FilePageId,
    src: &[u8],
  ) -> fkv_error::Result<()> {
    self.inner.write_page(tbl, file_page_id, src).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> fkv_error::Result<()> {
    self.inner.sync_data(tbl).await
  }
}

impl ManifestIo for YieldListStore {
  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> fkv_error::Result<()> {
    self.inner.append_manifest(tbl, record).await
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> fkv_error::Result<()> {
    self.inner.switch_manifest(tbl, record).await
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> fkv_error::Result<Option<Vec<u8>>> {
    self.inner.read_manifest(tbl).await
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> fkv_error::Result<()> {
    self.inner.drop_manifest(tbl).await
  }

  async fn write_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
    record: &[u8],
  ) -> fkv_error::Result<()> {
    self.inner.write_manifest_file(tbl, name, record).await
  }

  async fn append_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
    record: &[u8],
  ) -> fkv_error::Result<()> {
    self.inner.append_manifest_file(tbl, name, record).await
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> fkv_error::Result<Vec<String>> {
    self.inner.list_manifest_file_name_li(tbl).await
  }

  async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> fkv_error::Result<Vec<String>> {
    self.inner.list_sidecar_file_name_li(tbl).await
  }

  async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> fkv_error::Result<()> {
    self.inner.remove_sidecar_file_name(tbl, name).await
  }

  async fn read_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
  ) -> fkv_error::Result<Option<Vec<u8>>> {
    self.inner.read_manifest_file(tbl, name).await
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> fkv_error::Result<()> {
    self.inner.remove_manifest_file(tbl, name).await
  }
}

/// 8 路同表冷预热：只有 1 路拿到 scheme 回填权，其余必须靠有界重试等到
/// scheme 落盘，最终 8 个 segment 文件全部完成。
///
/// 回退即失败：把 `TryAgain` 当持久错误时，首个失败就 break 整轮
/// （`DownloadError`，只拉到 1 个段文件）。
#[test]
fn prewarm_eight_way_cold_segment_install_completes() {
  pollster::block_on(async {
    let tbl = TableIdent::new("cold8", 1).unwrap();
    let remote = MemObjectStore::new();
    seed_manifest(&remote, &tbl).await;
    // 2 槽 / 文件（segment_size 4096, shift 1），远端 8 个段对象。
    for file_id in 1..=8u64 {
      let name = fkv_types::segment_file_name(file_id, "main", 0);
      remote
        .put(
          &format!("fkv/{tbl}/{name}"),
          Bytes::from(vec![file_id as u8; 8192]),
          None,
          false,
        )
        .await
        .unwrap();
    }

    let fresh = CloudStore::new(
      YieldListStore::new(),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1);
    let stats = prewarm_table(&fresh, &tbl).await.unwrap();
    assert_eq!(
      stats.completion_reason,
      CompletionReason::Success,
      "{stats:?}"
    );
    assert_eq!(stats.files_failed, 0, "{stats:?}");
    assert_eq!(stats.segment_file_num, 8, "{stats:?}");
    assert_eq!(stats.missing_num, 0, "{stats:?}");

    let mut file_id_li = fresh.local().list_segment_file_id_li(&tbl).await.unwrap();
    file_id_li.sort_unstable();
    assert_eq!(
      file_id_li,
      (1..=8).collect::<Vec<_>>(),
      "8 个段必须全部落地"
    );
    for file_id in 1..=8u64 {
      let mut dst = [0u8; 4096];
      fresh
        .local()
        .read_segment(&tbl, file_id * 2, &mut dst)
        .await
        .unwrap();
      assert_eq!(
        dst, [file_id as u8; 4096],
        "段 {file_id} 内容必须是远端副本"
      );
    }
  });
}
