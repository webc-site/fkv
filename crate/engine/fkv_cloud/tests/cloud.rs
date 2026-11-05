use std::{
  future::{Future, poll_fn},
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
  },
  task::{Context, Poll},
  thread::scope,
};

use bytes::Bytes;
use fkv_btree::BatchEntry;
use fkv_cloud::{CacheGroup, CloudStore, EVICT_PENDING_FILE_MAX};
use fkv_error::{KvError, Result};
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo, TermIo};
use fkv_io_mem::MemStore;
use fkv_metrics::ShardMetric;
use fkv_object::{MemObjectStore, ObjectMeta, ObjectStore};
use fkv_store::Store;
use fkv_types::{BranchFileRange, FileId, FilePageId, InstallOutcome, TableIdent};
use futures::{future::join_all, task::noop_waker};

#[test]
fn cloud_overlay_cache_miss_and_reopen() {
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
    );
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    let key_li = remote.list("fkv/demo.1/").await.unwrap();
    assert!(
      key_li
        .iter()
        .any(|meta| meta.key.starts_with("fkv/demo.1/data_"))
    );
    assert!(
      key_li
        .iter()
        .all(|meta| !meta.key.starts_with("fkv/demo.1/page/"))
    );
    drop(store);

    let mut cached = Store::new(
      CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    cached.reopen(&tbl).await.unwrap();
    assert_eq!(cached.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
    // 缓存未命中时按整文件下载：index + data 至少两个 page 同时驻留。
    assert!(cached.page_io().local().page_count() >= 2);
    assert_eq!(cached.scan(&tbl, b"", b"", 0).await.unwrap().len(), 2);
  });
}

#[test]
fn term_cas() {
  pollster::block_on(async {
    let remote = MemObjectStore::new();
    let store = CloudStore::new(
      MemStore::new(512),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    store.upsert_term("main", 1, 1).await.unwrap();
    store.upsert_term("main", 1, 2).await.unwrap();
    assert_eq!(store.read_term("main", 1).await.unwrap(), Some(2));
    assert_eq!(
      store.upsert_term("main", 1, 1).await.unwrap_err(),
      KvError::ExpiredTerm
    );
  });
}

#[test]
fn store_rejects_expired_term() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut first = Store::new(
      CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    first.claim_term("main", 1, 1).await.unwrap();
    first
      .put_with_term(
        &tbl,
        &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
        "main",
        1,
        1,
      )
      .await
      .unwrap();

    let second = Store::new(
      CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    second.claim_term("main", 1, 2).await.unwrap();
    assert_eq!(
      first
        .put_with_term(
          &tbl,
          &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)],
          "main",
          1,
          1,
        )
        .await
        .unwrap_err(),
      KvError::ExpiredTerm
    );
  });
}

#[test]
fn whole_file_upload_is_deferred_until_sync() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    );
    for file_page_id in 0..3u64 {
      let mut page = vec![0u8; 512];
      page[0] = file_page_id as u8 + 1;
      cloud.write_page(&tbl, file_page_id, &page).await.unwrap();
    }
    assert!(remote.list("fkv/demo.1/").await.unwrap().is_empty());
    cloud.sync_data(&tbl).await.unwrap();
    let meta_li = remote.list("fkv/demo.1/").await.unwrap();
    assert_eq!(meta_li.len(), 1);
    assert_eq!(meta_li[0].key, "fkv/demo.1/data_0_main_0");
    assert_eq!(meta_li[0].size, 3 * 512);
    assert!(remote.list("fkv/demo.1/page/").await.unwrap().is_empty());
  });
}

#[test]
fn cache_budget_evicts_and_redownloads_whole_files() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    // 4 pages / file = 2048 B；预算 3072 B，只容得下一个整文件。
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_budget(3072);
    for file_page_id in 0..8u64 {
      let mut page = vec![0u8; 512];
      page[0] = file_page_id as u8 + 1;
      cloud.write_page(&tbl, file_page_id, &page).await.unwrap();
    }
    cloud.sync_data(&tbl).await.unwrap();
    assert_eq!(remote.list("fkv/demo.1/").await.unwrap().len(), 2);
    assert_eq!(cloud.resident_file_count(), 1);
    assert_eq!(cloud.resident_bytes(), 2048);
    // 最久未使用的 file 0 被淘汰。
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_none());
    assert!(cloud.local().file_size(&tbl, 1).await.unwrap().is_some());

    let mut page = vec![0u8; 512];
    cloud.read_page(&tbl, 0, &mut page).await.unwrap();
    assert_eq!(page[0], 1);
    // 重新下载 file 0 后预算仍然生效：file 1 被淘汰。
    assert_eq!(cloud.resident_file_count(), 1);
    assert_eq!(cloud.resident_bytes(), 2048);
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_some());
    assert!(cloud.local().file_size(&tbl, 1).await.unwrap().is_none());
  });
}

/// 写 `file_num` 个整文件（每文件 4 页 × 512B = 2048B）并 flush，制造可淘汰的
/// clean cache。dirty 文件不可淘汰，必须先 `sync_data` 才有统一预算语义。
async fn fill_file_li(
  cloud: &CloudStore<MemStore, MemObjectStore>,
  tbl: &TableIdent,
  file_num: u64,
) {
  for file_id in 0..file_num {
    for index in 0..4u64 {
      let fp_id = file_id * 4 + index;
      let mut page = vec![0u8; 512];
      page[0] = (fp_id % 255) as u8 + 1;
      cloud.write_page(tbl, fp_id, &page).await.unwrap();
    }
  }
  cloud.sync_data(tbl).await.unwrap();
}

/// 本地后端实际驻留的 data 字节（MemStore 上等价于「占用磁盘」）。
async fn local_bytes(cloud: &CloudStore<MemStore, MemObjectStore>, tbl: &TableIdent) -> u64 {
  let mut total = 0u64;
  for file_id in cloud.local().list_file_id_li(tbl).await.unwrap() {
    total += cloud
      .local()
      .file_size(tbl, file_id)
      .await
      .unwrap()
      .unwrap_or(0);
  }
  total
}

/// 12d：两个实例共享一个 `CacheGroup` 时，整组驻留与本地实际占用都不超过
/// **单个** `local_space_limit`；对照组（各持一份预算）同样写入会翻倍，
/// 证明本测试能区分「共享记账」与「平行预算」。
#[test]
fn cache_group_bounds_parallel_instances_to_single_budget() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let group = Arc::new(CacheGroup::new(3072));
    // shard 侧与运行时侧预热：同一 root / 同一批文件，共享一份预算。
    let shard = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_group(group.clone());
    let prewarm = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_group(group.clone());

    // shard 写 2 个整文件 = 4096B > 3072B：flush 后淘汰最旧的 file 0。
    fill_file_li(&shard, &tbl, 2).await;
    assert_eq!(shard.resident_bytes(), 2048);
    assert_eq!(group.resident_bytes(), 2048);
    // 预热实例看到的余量必须是整组余量，不是自己那份 3072。
    assert_eq!(prewarm.cache_remained(), 1024);

    // 预热再把 file 0 拉进来：整组已装不下两份 2048B，实例只能摘自己的新条目。
    let mut page = vec![0u8; 512];
    prewarm.read_page(&tbl, 0, &mut page).await.unwrap();
    assert_eq!(page[0], 1);
    prewarm.drain_evictions().await.unwrap();
    shard.drain_evictions().await.unwrap();
    assert_eq!(prewarm.resident_bytes(), 0);
    assert_eq!(group.resident_bytes(), 2048);
    assert!(group.resident_bytes() <= 3072, "{group:?}");
    // 实际落盘字节（两个本地后端之和）同样不超过单个预算。
    let disk = local_bytes(&shard, &tbl).await + local_bytes(&prewarm, &tbl).await;
    assert_eq!(disk, 2048);

    // 只列举（归档 crond / global_* 的运行时侧实例）不写 cache、不占预算。
    let partition_page = prewarm
      .remote_partition_page(None, None, 100)
      .await
      .unwrap();
    assert_eq!(partition_page.table_li, vec![tbl.clone()]);
    assert_eq!(group.resident_bytes(), 2048);

    // 对照组：不共享预算时两个实例各容 2048B，合计 4096B 超过单个上界。
    let first = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_budget(3072);
    let second = CloudStore::new(MemStore::with_pages_per_file(512, 2), remote, "fkv", 2)
      .with_cache_budget(3072);
    fill_file_li(&first, &tbl, 2).await;
    fill_file_li(&second, &tbl, 2).await;
    let unshared = local_bytes(&first, &tbl).await + local_bytes(&second, &tbl).await;
    assert_eq!(unshared, 4096, "对照组应复现平行预算导致的 2× 上界");
  });
}

#[test]
fn write_to_evicted_file_restores_before_modifying() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_budget(3072);
    for file_page_id in 0..8u64 {
      let mut page = vec![0u8; 512];
      page[0] = file_page_id as u8 + 1;
      cloud.write_page(&tbl, file_page_id, &page).await.unwrap();
    }
    cloud.sync_data(&tbl).await.unwrap();
    // file 0 已被淘汰，再写其中一页必须先恢复整文件。
    let page = vec![7u8; 512];
    cloud.write_page(&tbl, 1, &page).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    let Some(bytes) = remote.get("fkv/demo.1/data_0_main_0").await.unwrap() else {
      panic!("file 0 must be uploaded");
    };
    assert_eq!(bytes[0], 1);
    assert_eq!(bytes[512], 7);
    assert_eq!(bytes[2 * 512], 3);
  });
}

#[test]
fn drop_table_removes_local_and_remote_objects() {
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
    );
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    assert!(!remote.list("fkv/demo.1/").await.unwrap().is_empty());

    store.drop_table(&tbl).await.unwrap();
    // Drop 只删活跃 manifest；data/segment 对象由 FileGc 按 retained set 回收
    // （对齐 C++ `DropManifest` + `TryAddFileGc`）。
    assert!(
      !remote
        .list("fkv/demo.1/")
        .await
        .unwrap()
        .iter()
        .any(|meta| meta.key.ends_with("manifest_main_0"))
    );
    store
      .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
      .await
      .unwrap();
    assert!(remote.list("fkv/demo.1/").await.unwrap().is_empty());
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), None);

    store
      .put(&tbl, &[BatchEntry::upsert(b"x".to_vec(), b"X".to_vec(), 2)])
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, b"x").await.unwrap(), Some(b"X".to_vec()));
    assert!(!remote.list("fkv/demo.1/").await.unwrap().is_empty());
  });
}

#[test]
fn cache_metrics_record_hit_miss_and_eviction() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let metrics = Arc::new(ShardMetric::new(0));
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_budget(3072)
    .with_metrics(metrics.clone());
    for file_page_id in 0..8u64 {
      let mut page = vec![0u8; 512];
      page[0] = file_page_id as u8 + 1;
      cloud.write_page(&tbl, file_page_id, &page).await.unwrap();
    }
    cloud.sync_data(&tbl).await.unwrap();
    assert!(metrics.snapshot().cache_evict >= 1);

    let mut page = vec![0u8; 512];
    cloud.read_page(&tbl, 0, &mut page).await.unwrap();
    assert_eq!(page[0], 1);
    cloud.read_page(&tbl, 0, &mut page).await.unwrap();
    let snapshot = metrics.snapshot();
    assert!(snapshot.cache_miss >= 1);
    assert!(snapshot.cache_hit >= 1);
    assert!(snapshot.cache_bytes > 0);
    assert_eq!(snapshot.cache_bytes, cloud.resident_bytes());
  });
}

/// 对齐 C++ `AsyncHttpManager::ComposeKey`：
/// `<prefix>/<table>.<partition>/<filename>`，filename 带 branch/term，
/// 没有 `data/`、`segment/`、`manifest/` 子目录；同表不同 branch 不冲突。
#[test]
fn cloud_object_keys_match_cpp_compose_key_layout() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    );
    let mut page = vec![0u8; 512];
    page[0] = 1;
    cloud.set_file_identity("feature", 7);
    cloud.write_page(&tbl, 0, &page).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    let key_li: Vec<String> = remote
      .list("fkv/demo.1/")
      .await
      .unwrap()
      .into_iter()
      .map(|meta| meta.key)
      .collect();
    assert_eq!(key_li, vec!["fkv/demo.1/data_0_feature_7".to_owned()]);

    // 切回 main/0 写同一 file_id：对象 key 不同，互不覆盖。
    cloud.set_file_identity("main", 0);
    cloud.write_page(&tbl, 0, &page).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    let key_li = remote.list("fkv/demo.1/").await.unwrap();
    assert!(
      key_li
        .iter()
        .any(|meta| meta.key == "fkv/demo.1/data_0_feature_7")
    );
    assert!(
      key_li
        .iter()
        .any(|meta| meta.key == "fkv/demo.1/data_0_main_0")
    );
    assert!(key_li.iter().all(|meta| !meta.key.contains("/data/")));
  });
}

#[test]
fn incremental_upload_only_sends_dirty_range_li() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    );
    let page = |tag: u8| {
      let mut page = vec![0u8; 512];
      page[0] = tag;
      page
    };
    cloud.write_page(&tbl, 0, &page(1)).await.unwrap();
    cloud.write_page(&tbl, 1, &page(2)).await.unwrap();
    cloud.write_page(&tbl, 2, &page(3)).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    let baseline = remote.range_put_bytes();
    assert_eq!(baseline, 3 * 512);

    // 只重写中间页：上传字节应等于一页，而非整文件。
    cloud.write_page(&tbl, 1, &page(9)).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    assert_eq!(remote.range_put_bytes() - baseline, 512);

    let bytes = remote
      .get("fkv/demo.1/data_0_main_0")
      .await
      .unwrap()
      .unwrap();
    assert_eq!(bytes.len(), 3 * 512);
    assert_eq!(bytes[0], 1);
    assert_eq!(bytes[512], 9);
    assert_eq!(bytes[1024], 3);
  });
}

/// cld-41 读取探针：统计 term CAS 路径发出的读取次数，并可在首次读之后
/// 注入一次“别的 writer 推进 term”的写入。
struct TermProbe {
  /// 读到 term 文件后立即写入的 term（模拟 GET 与 CAS 之间的并发更新）。
  race: Option<u64>,
  raced: AtomicBool,
  get: AtomicUsize,
  head: AtomicUsize,
  versioned: AtomicUsize,
}

impl TermProbe {
  fn new(race: Option<u64>) -> Self {
    Self {
      race,
      raced: AtomicBool::new(false),
      get: AtomicUsize::new(0),
      head: AtomicUsize::new(0),
      versioned: AtomicUsize::new(0),
    }
  }

  /// `(get, head, get_versioned)` 调用次数。
  fn counts(&self) -> (usize, usize, usize) {
    (
      self.get.load(Ordering::SeqCst),
      self.head.load(Ordering::SeqCst),
      self.versioned.load(Ordering::SeqCst),
    )
  }

  /// 只注入一次：写入发生在本次读取返回之后、调用方 CAS 之前。
  async fn race_after_read(&self, inner: &MemObjectStore, key: &str) {
    let Some(term) = self.race else {
      return;
    };
    if self.raced.swap(true, Ordering::SeqCst) {
      return;
    }
    inner
      .put(key, Bytes::from(term.to_string()), None, false)
      .await
      .unwrap();
  }
}

/// 包装 `MemObjectStore`，在 term CAS 的读取路径上做计数与并发注入。
struct TermProbeStore {
  inner: MemObjectStore,
  probe: Arc<TermProbe>,
}

impl ObjectStore for TermProbeStore {
  async fn get(&self, key: &str) -> Result<Option<Bytes>> {
    self.probe.get.fetch_add(1, Ordering::SeqCst);
    let value = self.inner.get(key).await?;
    self.probe.race_after_read(&self.inner, key).await;
    Ok(value)
  }

  async fn get_versioned(&self, key: &str) -> Result<Option<(Bytes, String)>> {
    self.probe.versioned.fetch_add(1, Ordering::SeqCst);
    let pair = self.inner.get_versioned(key).await?;
    self.probe.race_after_read(&self.inner, key).await;
    Ok(pair)
  }

  async fn put(
    &self,
    key: &str,
    value: Bytes,
    if_match: Option<&str>,
    if_none_match: bool,
  ) -> Result<String> {
    self.inner.put(key, value, if_match, if_none_match).await
  }

  async fn head(&self, key: &str) -> Result<Option<ObjectMeta>> {
    self.probe.head.fetch_add(1, Ordering::SeqCst);
    self.inner.head(key).await
  }

  async fn delete(&self, key: &str) -> Result<()> {
    self.inner.delete(key).await
  }

  async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>> {
    self.inner.list(prefix).await
  }
}

/// cld-41：term CAS 必须一次读取同时拿到 value 与 etag（对齐 C++ `ReadTermFile`
/// 的单次 GET）。旧实现先 `get` 再 `head`，两个计数都会非零。
#[test]
fn term_cas_reads_value_and_etag_in_one_request() {
  pollster::block_on(async {
    let probe = Arc::new(TermProbe::new(None));
    let store = CloudStore::new(
      MemStore::new(512),
      TermProbeStore {
        inner: MemObjectStore::new(),
        probe: Arc::clone(&probe),
      },
      "fkv",
      18,
    );
    assert_eq!(store.upsert_term("main", 1, 5).await.unwrap(), 5);
    assert_eq!(store.upsert_term("main", 1, 7).await.unwrap(), 7);
    let (get, head, versioned) = probe.counts();
    assert_eq!(head, 0, "term CAS 不应再单独发 HEAD 取 etag");
    assert_eq!(get, 0, "term CAS 不应再单独发 GET 取 value");
    assert_eq!(versioned, 2, "两次 upsert 各一次同源读取");
    assert_eq!(store.read_term("main", 1).await.unwrap(), Some(7));
  });
}

/// cld-41：读到旧值后、CAS 之前别的 writer 把 term 推进到 9。
/// 旧实现会拿 HEAD 读到的 E9 去 If-Match，把 9 回退成 7；新实现 CAS 失败后重读，
/// 认到 `current > term` 返回 `ExpiredTerm`，term 停在 9。
#[test]
fn term_cas_does_not_roll_back_advanced_term() {
  pollster::block_on(async {
    let inner = MemObjectStore::new();
    let seed = CloudStore::new(
      MemStore::new(512),
      TermProbeStore {
        inner: inner.clone(),
        probe: Arc::new(TermProbe::new(None)),
      },
      "fkv",
      18,
    );
    seed.upsert_term("main", 1, 5).await.unwrap();

    let store = CloudStore::new(
      MemStore::new(512),
      TermProbeStore {
        inner: inner.clone(),
        probe: Arc::new(TermProbe::new(Some(9))),
      },
      "fkv",
      18,
    );
    assert_eq!(
      store.upsert_term("main", 1, 7).await.unwrap_err(),
      KvError::ExpiredTerm
    );
    assert_eq!(store.read_term("main", 1).await.unwrap(), Some(9));
  });
}

/// cld-41：两个 writer 交替 upsert 更高 term，观察到的 CURRENT_TERM 不得回退。
#[test]
fn concurrent_writers_never_roll_back_term() {
  const ROUNDS: u64 = 200;
  let inner = MemObjectStore::new();
  let observed = Arc::new(AtomicU64::new(0));
  scope(|scope| {
    for offset in 0..2u64 {
      let inner = inner.clone();
      let observed = Arc::clone(&observed);
      scope.spawn(move || {
        pollster::block_on(async {
          let store = CloudStore::new(
            MemStore::new(512),
            inner,
            "fkv",
            fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
          );
          // 只比较本线程前后两次读到的 term：另一线程的读可能发生在本线程读之前、
          // 却在本线程 `fetch_max` 之后更新共享 max，用共享 max 比较会误报回退。
          let mut last = 0u64;
          for round in 0..ROUNDS {
            let term = 2 * round + 1 + offset;
            loop {
              match store.upsert_term("main", 1, term).await {
                Ok(_) | Err(KvError::ExpiredTerm) => break,
                // 重试风暴下允许外层重来，但绝不允许回退。
                Err(KvError::TryAgain) => continue,
                Err(err) => panic!("upsert_term(term={term}) 意外错误: {err:?}"),
              }
            }
            let current = store.read_term("main", 1).await.unwrap().unwrap_or(0);
            assert!(current >= last, "term 回退：{last} -> {current}");
            last = current;
            observed.fetch_max(current, Ordering::SeqCst);
          }
        });
      });
    }
  });
  assert_eq!(observed.load(Ordering::SeqCst), 2 * ROUNDS);
}

/// io-29：分页列举按 token 衔接，跨页不重不漏；条目带对象大小。
#[test]
fn remote_file_id_page_paginates() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    for file_id in 0..5 {
      let name = fkv_types::data_file_name(file_id, "main", 0);
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
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut token: Option<String> = None;
    let mut file_id_li = Vec::new();
    loop {
      let page = cloud
        .remote_data_file_page(&tbl, token.as_deref(), 2)
        .await
        .unwrap();
      assert!(page.entry_li.len() <= 2);
      for entry in page.entry_li {
        assert_eq!(entry.size, 1);
        file_id_li.push(entry.file_id);
      }
      match page.next_token {
        Some(next) => token = Some(next),
        None => break,
      }
    }
    assert_eq!(file_id_li, (0..5).collect::<Vec<_>>());
  });
}

/// 远端分区列举：跨页用 `after`/`last` 去重，只产出 `{tbl}.{partition}`，
/// 无分区目录的 key（term 文件等）与 prefix 之外的 key 都跳过。
#[test]
fn remote_partition_page_dedups_across_pages() {
  pollster::block_on(async {
    let remote = MemObjectStore::new();
    let tbl_li = [
      TableIdent::new("demo", 1).unwrap(),
      TableIdent::new("demo", 3).unwrap(),
      TableIdent::new("other", 2).unwrap(),
    ];
    // 每个分区放 3 个对象：同一分区的 key 在字节序里连续，跨页只该产出一次。
    for tbl in &tbl_li {
      for file_id in 0..3 {
        let name = fkv_types::data_file_name(file_id, "main", 0);
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
    }
    // term 文件没有 `/`，不是分区目录。
    remote
      .put(
        "fkv/CURRENT_TERM_main_0",
        Bytes::from_static(b"0"),
        None,
        false,
      )
      .await
      .unwrap();
    // prefix 之外的 key 不参与列举。
    remote
      .put(
        "other-prefix/demo.9/data_0_main_0",
        Bytes::from_static(b"x"),
        None,
        false,
      )
      .await
      .unwrap();

    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut seen_li = Vec::new();
    let mut after: Option<TableIdent> = None;
    let mut token: Option<String> = None;
    loop {
      let page = cloud
        .remote_partition_page(after.as_ref(), token.as_deref(), 2)
        .await
        .unwrap();
      seen_li.extend(page.table_li);
      after = page.last;
      token = page.next_token;
      if token.is_none() {
        break;
      }
    }
    assert_eq!(seen_li, tbl_li);
    // max_keys=0（不限）一次取完应与翻页结果一致。
    let page = cloud.remote_partition_page(None, None, 0).await.unwrap();
    assert_eq!(page.table_li, tbl_li);

    // 换 prefix：原对象不可见。
    let other = CloudStore::new(
      MemStore::new(512),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let page = other.remote_partition_page(None, None, 0).await.unwrap();
    assert!(page.table_li.is_empty());
  });
}

/// 缓存按表嵌套后，detach 只删目标表的条目，`resident_bytes` 精确归零该表占用。
#[test]
fn evict_table_drops_only_that_table_cache() {
  pollster::block_on(async {
    let tbl_a = TableIdent::new("demo", 1).unwrap();
    let tbl_b = TableIdent::new("demo", 2).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(MemStore::with_pages_per_file(512, 2), remote, "fkv", 2);
    for (tbl, base) in [(&tbl_a, 0u8), (&tbl_b, 10u8)] {
      for file_page_id in 0..8u64 {
        let mut page = vec![0u8; 512];
        page[0] = base + file_page_id as u8 + 1;
        cloud.write_page(tbl, file_page_id, &page).await.unwrap();
      }
      cloud.sync_data(tbl).await.unwrap();
    }
    // shift=2 → 4 页/file（2048 B），每表 file 0/1 两个。
    assert_eq!(cloud.resident_file_count(), 4);
    assert_eq!(cloud.resident_bytes(), 8192);

    cloud.evict_table(&tbl_a);

    // 只掉了 tbl_a 的条目：计数与字节数各减一半；local 后端是否真删文件由后端自理
    // （`FileIo::evict_table` 默认 no-op）。
    assert_eq!(cloud.resident_file_count(), 2);
    assert_eq!(cloud.resident_bytes(), 4096);
    // tbl_b 不受影响：仍命中缓存，记账不变。
    let mut page = vec![0u8; 512];
    cloud.read_page(&tbl_b, 0, &mut page).await.unwrap();
    assert_eq!(page[0], 11);
    assert_eq!(cloud.resident_bytes(), 4096);
  });
}

/// 分区级 evict 后再次打开必须重新扫描本地文件并登记：只清记账不摘
/// `restored_table_set` 会让盘上文件永久脱钩预算 / LRU。
#[test]
fn evict_table_then_reopen_rescans_local_cache() {
  pollster::block_on(async {
    let tbl = TableIdent::new("rescan", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(MemStore::with_pages_per_file(512, 2), remote, "fkv", 2)
      .with_reuse_local_caches(true);
    // shift=2 → 4 页/file；8 个页 id 覆盖 file 0 / 1。
    for file_page_id in 0..8u64 {
      cloud
        .write_page(&tbl, file_page_id, &[file_page_id as u8 + 1; 512])
        .await
        .unwrap();
    }
    cloud.sync_data(&tbl).await.unwrap();
    assert_eq!(cloud.resident_file_count(), 2);

    cloud.evict_table(&tbl);
    assert_eq!(cloud.resident_file_count(), 0);
    assert_eq!(cloud.resident_bytes(), 0);

    // 重新打开：restore_table_cache 必须重扫盘上仍存在的 data 文件。
    cloud.read_manifest(&tbl).await.unwrap();
    assert_eq!(
      cloud.resident_file_count(),
      2,
      "evict 后 reopen 必须重新登记本地 cache"
    );
    assert_eq!(cloud.resident_bytes(), 4096);
  });
}

/// 全局侵入式 LRU：跨表按最近使用顺序淘汰，命中会把文件挪到最新端。
#[test]
fn cache_budget_evicts_globally_oldest_across_tables() {
  pollster::block_on(async {
    let tbl_a = TableIdent::new("a", 1).unwrap();
    let tbl_b = TableIdent::new("b", 1).unwrap();
    let tbl_c = TableIdent::new("c", 1).unwrap();
    let remote = MemObjectStore::new();
    // shift=2 → 4 页/file，写第 3 页即 2048 B；预算 2 个整文件。
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_budget(4096);
    let page = vec![7u8; 512];

    cloud.write_page(&tbl_a, 3, &page).await.unwrap();
    cloud.sync_data(&tbl_a).await.unwrap();
    cloud.write_page(&tbl_b, 3, &page).await.unwrap();
    cloud.sync_data(&tbl_b).await.unwrap();
    assert_eq!(cloud.resident_file_count(), 2);

    // 命中 a：a 变最新，LRU 旧端轮到 b。
    let mut dst = vec![0u8; 512];
    cloud.read_page(&tbl_a, 3, &mut dst).await.unwrap();
    assert_eq!(dst, page);

    // c 入 cache 触发淘汰：全局最久未用的 b 被踢，a 保留。
    cloud.write_page(&tbl_c, 3, &page).await.unwrap();
    assert!(cloud.local().file_size(&tbl_b, 0).await.unwrap().is_none());
    assert!(cloud.local().file_size(&tbl_a, 0).await.unwrap().is_some());
    assert!(cloud.local().file_size(&tbl_c, 0).await.unwrap().is_some());
    assert_eq!(cloud.resident_bytes(), 4096);
  });
}

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

/// 空 prefix 时 term 对象 key 不能多出前导 `/`：与 `tbl_prefix` 同规则，
/// 对齐 C++ `ComposeKeyFromRemote`（空组件直接跳过），否则跨实现互看不见 term 文件。
#[test]
fn term_key_has_no_leading_slash_with_empty_prefix() {
  pollster::block_on(async {
    for (prefix, want) in [
      ("", "CURRENT_TERM_main_0"),
      ("pfx", "pfx/CURRENT_TERM_main_0"),
      ("pfx/", "pfx/CURRENT_TERM_main_0"),
    ] {
      let remote = MemObjectStore::new();
      let cloud = CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        prefix,
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      );
      assert_eq!(cloud.upsert_term("main", 0, 7).await.unwrap(), 7);
      let key_li: Vec<String> = remote
        .list("")
        .await
        .unwrap()
        .into_iter()
        .map(|meta| meta.key)
        .collect();
      assert!(
        key_li.iter().any(|key| key == want),
        "prefix={prefix:?} 未见 {want}，实际 {key_li:?}"
      );
      assert!(
        !key_li.iter().any(|key| key.starts_with('/')),
        "prefix={prefix:?} 出现前导斜杠 key：{key_li:?}"
      );
      assert_eq!(cloud.read_term("main", 0).await.unwrap(), Some(7));
    }
  });
}

/// 本地 cache 只覆盖文件前缀（崩溃残留短文件）时，读越界页应按 miss 回源，
/// 而不是因为 `file_size().is_some()` 直接拒绝回源。
#[test]
fn read_page_refetches_after_local_short_file() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    {
      let cloud = CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      );
      cloud.write_page(&tbl, 0, &[1u8; 512]).await.unwrap();
      cloud.write_page(&tbl, 1, &[2u8; 512]).await.unwrap();
      cloud.sync_data(&tbl).await.unwrap();
    }
    // 新进程本地只有第 0 页，第 1 页落在文件长度之外。
    let local = MemStore::new(512);
    local.write_page(&tbl, 0, &[1u8; 512]).await.unwrap();
    let cloud = CloudStore::new(
      local,
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut dst = [0u8; 512];
    cloud.read_page(&tbl, 1, &mut dst).await.unwrap();
    assert_eq!(dst, [2u8; 512]);
  });
}

/// 在远端造 `file_num` 个 512B（单页）data 对象。
async fn seed_remote_data(remote: &MemObjectStore, tbl: &TableIdent, file_num: u64) {
  for file_id in 0..file_num {
    let name = fkv_types::data_file_name(file_id, "main", 0);
    // 非零内容：MemStore 把全零块当空洞，全零文件不会落本地。
    remote
      .put(
        &format!("fkv/{tbl}/{name}"),
        Bytes::from(vec![(file_id % 255 + 1) as u8; 512]),
        None,
        false,
      )
      .await
      .unwrap();
  }
}

/// 淘汰只摘链，删除按 `cache_budget / 8` 阈值批量执行；淘汰后又被写入的
/// 文件必须取消删除，不能误删刚回填的 cache。
#[test]
fn cache_evict_batches_deletes_and_keeps_refilled_file() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    seed_remote_data(&remote, &tbl, 18).await;
    // 预算 8192 = 16 个 512B 文件；待删阈值 8192 / 8 = 1024 = 2 个文件。
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_cache_budget(8192);
    for file_id in 0..17u64 {
      assert!(cloud.read_file(&tbl, file_id).await.unwrap().is_some());
    }
    // 第 17 个文件只淘汰 file 0，待删 512B 未达阈值：本地删除延迟。
    assert_eq!(cloud.resident_bytes(), 8192);
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_some());

    // 淘汰后重新写入 file 0：待删项在 drain 时必须跳过它，只删别的 victim。
    cloud.write_file(&tbl, 0, &[9u8; 512]).await.unwrap();
    assert_eq!(cloud.resident_bytes(), 8192);
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_some());
    assert!(cloud.local().file_size(&tbl, 1).await.unwrap().is_none());
  });
}

/// 待删队列未到阈值时，显式 `drain_evictions` 必须立即删除并清空队列。
#[test]
fn drain_evictions_flushes_pending_deletes() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    seed_remote_data(&remote, &tbl, 17).await;
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_cache_budget(8192);
    for file_id in 0..17u64 {
      assert!(cloud.read_file(&tbl, file_id).await.unwrap().is_some());
    }
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_some());
    cloud.drain_evictions().await.unwrap();
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_none());
    assert_eq!(cloud.resident_bytes(), 8192);
  });
}

/// 删除失败时项回队、记账不丢：热路径只告警不失败，显式 drain 重试成功。
struct FlakyRemoveStore {
  inner: MemStore,
  fail_num: AtomicUsize,
}

impl FileIo for FlakyRemoveStore {
  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<u64>> {
    self.inner.list_file_id_li(tbl).await
  }

  async fn read_file(&self, tbl: &TableIdent, file_id: u64) -> Result<Option<Vec<u8>>> {
    self.inner.read_file(tbl, file_id).await
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: u64, bytes: &[u8]) -> Result<()> {
    self.inner.write_file(tbl, file_id, bytes).await
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: u64) -> Result<Option<u64>> {
    self.inner.file_size(tbl, file_id).await
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: u64) -> Result<()> {
    if self.fail_num.load(Ordering::Relaxed) > 0 {
      self.fail_num.fetch_sub(1, Ordering::Relaxed);
      return Err(KvError::IoFail);
    }
    self.inner.remove_file(tbl, file_id).await
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<u64>> {
    self.inner.list_segment_file_id_li(tbl).await
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: u64) -> Result<()> {
    self.inner.remove_segment_file(tbl, file_id).await
  }
}

#[test]
fn drain_evictions_requeues_failed_delete() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    seed_remote_data(&remote, &tbl, 2).await;
    // 预算 512：第二个文件一入 cache 就淘汰 file 0，阈值 64 立即触发删除。
    let cloud = CloudStore::new(
      FlakyRemoveStore {
        inner: MemStore::new(512),
        fail_num: AtomicUsize::new(1),
      },
      remote,
      "fkv",
      18,
    )
    .with_cache_budget(512);
    assert!(cloud.read_file(&tbl, 0).await.unwrap().is_some());
    // 热路径删除失败不回传错误，victim 回队等待重试。
    assert!(cloud.read_file(&tbl, 1).await.unwrap().is_some());
    assert_eq!(cloud.resident_bytes(), 512);
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_some());

    cloud.drain_evictions().await.unwrap();
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_none());
    assert!(cloud.local().file_size(&tbl, 1).await.unwrap().is_some());
  });
}

/// 崩溃残留短文件在扩展写入前必须回源补齐：否则中段页会变成零洞。
#[test]
fn write_page_refetches_short_local_file_before_extending() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    {
      let cloud = CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      );
      for (page, tag) in [(0u64, 1u8), (1, 2), (2, 3)] {
        cloud.write_page(&tbl, page, &[tag; 512]).await.unwrap();
      }
      cloud.sync_data(&tbl).await.unwrap();
    }
    // 新进程本地只剩第 0 页（内容还是错的），写第 2 页前必须整文件回源。
    let local = MemStore::new(512);
    local.write_page(&tbl, 0, &[9u8; 512]).await.unwrap();
    let cloud = CloudStore::new(
      local,
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    cloud.write_page(&tbl, 2, &[4u8; 512]).await.unwrap();
    let mut dst = [0u8; 512];
    cloud.read_page(&tbl, 1, &mut dst).await.unwrap();
    assert_eq!(dst, [2u8; 512]);
    cloud.read_page(&tbl, 0, &mut dst).await.unwrap();
    assert_eq!(dst, [1u8; 512]);
  });
}

/// 换 term 后旧 term 的 dirty ranges 不能带到新 key：必须整对象上传，
/// 否则 `put_range` 会用零前缀把新对象首页冲成空洞。
#[test]
fn identity_change_discards_ranges_and_uploads_whole_object() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    cloud.write_page(&tbl, 0, &[1u8; 512]).await.unwrap();
    cloud.write_page(&tbl, 1, &[2u8; 512]).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    // term 0 下再改一页，ranges 尚未上传。
    cloud.write_page(&tbl, 1, &[3u8; 512]).await.unwrap();
    cloud.set_file_identity("main", 1);
    cloud.sync_data(&tbl).await.unwrap();
    let bytes = remote
      .get("fkv/demo.1/data_0_main_1")
      .await
      .unwrap()
      .unwrap();
    assert_eq!(bytes.len(), 1024);
    assert_eq!(bytes[0], 1);
    assert_eq!(bytes[512], 3);
  });
}

/// identity 变更后**首次** `mark_dirty_range` 也必须整对象上传：只记新区间
/// 会让新 term 对象前段成为零洞，旧脏页永久丢失。
#[test]
fn identity_change_then_dirty_range_uploads_whole_object() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    // term 0：只写第二页，保持脏且未上传。
    cloud.write_page(&tbl, 1, &[2u8; 512]).await.unwrap();
    // 切到 term 1 后写第一页：这是 identity 变更后的首个脏区间。
    cloud.set_file_identity("main", 1);
    cloud.write_page(&tbl, 0, &[1u8; 512]).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    let bytes = remote
      .get("fkv/demo.1/data_0_main_1")
      .await
      .unwrap()
      .unwrap();
    assert_eq!(bytes.len(), 1024);
    assert_eq!(bytes[0], 1);
    assert_eq!(bytes[512], 2);
  });
}

/// 预热不得用远端旧对象覆盖本地脏页：本地命中直接返回，由 shard 自己的
/// flush 上传最新内容。
#[test]
fn prewarm_data_file_keeps_local_dirty_pages() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    remote
      .put(
        "fkv/demo.1/data_0_main_0",
        Bytes::from(vec![9u8; 1024]),
        None,
        false,
      )
      .await
      .unwrap();
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    cloud.write_page(&tbl, 0, &[1u8; 512]).await.unwrap();
    assert!(cloud.prewarm_data_file(&tbl, 0).await.unwrap());
    cloud.sync_data(&tbl).await.unwrap();
    let bytes = remote
      .get("fkv/demo.1/data_0_main_0")
      .await
      .unwrap()
      .unwrap();
    assert_eq!(bytes[0], 1);
  });
}

/// 坏盘持续删除失败时，回队不能无界增长。
#[test]
fn evict_requeue_queue_is_bounded() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let file_num = EVICT_PENDING_FILE_MAX as u64 + 8;
    seed_remote_data(&remote, &tbl, file_num).await;
    let cloud = CloudStore::new(
      FlakyRemoveStore {
        inner: MemStore::new(512),
        fail_num: AtomicUsize::new(usize::MAX),
      },
      remote,
      "fkv",
      18,
    )
    .with_cache_budget(512);
    for file_id in 0..file_num {
      cloud.read_file(&tbl, file_id).await.unwrap();
    }
    assert!(cloud.pending_delete_count() <= EVICT_PENDING_FILE_MAX);
    assert!(cloud.pending_delete_count() > 0);
  });
}

/// 冷启动后必须按本地物理文件重建记账：否则旧文件不计预算、不参与淘汰，
/// 预热还会把已落盘文件当成 miss。
#[test]
fn restore_local_cache_accounting_after_restart() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    PageIo::set_file_identity(&cloud, "main", 1);
    cloud.write_page(&tbl, 0, &[7u8; 512]).await.unwrap();
    assert_eq!(cloud.resident_bytes(), 512);

    let (local, _) = cloud.into_parts();
    let restarted = CloudStore::new(
      local,
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    restarted.restore_table_cache(&tbl).await.unwrap();
    assert!(restarted.is_file_resident(&tbl, 0, false));
    assert_eq!(restarted.resident_bytes(), 512);

    // `allow_reuse_local_caches=false`：拒绝复用，清空本地 data / segment。
    let (local, _) = restarted.into_parts();
    let cleared = CloudStore::new(
      local,
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_reuse_local_caches(false);
    cleared.restore_table_cache(&tbl).await.unwrap();
    assert_eq!(cleared.resident_bytes(), 0);
    assert!(
      cleared
        .local()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty()
    );
  });
}

/// 子分支继承父分支数据：预热列举必须放行 manifest 区间引用的父 branch /
/// 旧 term 对象，同时仍拒绝无关 branch 与未被引用的旧 term。
#[test]
fn remote_file_page_includes_parent_branch_ranges() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    for name in [
      "data_0_main_1",
      "data_1_main_1",
      "data_2_feature_2",
      "data_3_feature_1",
      "data_4_other_1",
    ] {
      remote
        .put(
          &format!("fkv/demo.1/{name}"),
          Bytes::from_static(b"x"),
          None,
          false,
        )
        .await
        .unwrap();
    }
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    PageIo::set_file_identity(&cloud, "feature", 2);
    cloud.set_file_range(
      &tbl,
      &[
        BranchFileRange {
          branch_name: "main".into(),
          term: 1,
          max_file_id: 2,
          max_segment_file_id: 2,
        },
        BranchFileRange {
          branch_name: "feature".into(),
          term: 2,
          max_file_id: 4,
          max_segment_file_id: 4,
        },
      ],
    );
    let page = cloud.remote_data_file_page(&tbl, None, 1000).await.unwrap();
    let mut id_li: Vec<_> = page.entry_li.iter().map(|entry| entry.file_id).collect();
    id_li.sort_unstable();
    // 0/1 由父 branch 区间放行，2 走当前 term 快路径。
    assert_eq!(id_li, vec![0, 1, 2]);
  });
}

/// 冷启动 `restore_table_cache` 把崩溃残留的短文件按实际长度登记 resident 后，
/// 越界写仍必须回源补齐：只 `contains` 会放行半截文件，中段留零洞。
#[test]
fn write_page_refetches_after_restore_short_file() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    {
      let cloud = CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      );
      for (page, tag) in [(0u64, 1u8), (1, 2), (2, 3)] {
        cloud.write_page(&tbl, page, &[tag; 512]).await.unwrap();
      }
      cloud.sync_data(&tbl).await.unwrap();
    }
    // 新进程本地只剩第 0 页（内容还是错的），restore 按 512 登记 resident。
    let local = MemStore::new(512);
    local.write_page(&tbl, 0, &[9u8; 512]).await.unwrap();
    let cloud = CloudStore::new(
      local,
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    cloud.restore_table_cache(&tbl).await.unwrap();
    assert!(cloud.is_file_resident(&tbl, 0, false));
    cloud.write_page(&tbl, 2, &[4u8; 512]).await.unwrap();
    let mut dst = [0u8; 512];
    cloud.read_page(&tbl, 1, &mut dst).await.unwrap();
    assert_eq!(dst, [2u8; 512], "restore 后的短文件必须先回源再越界写");
    cloud.read_page(&tbl, 0, &mut dst).await.unwrap();
    assert_eq!(dst, [1u8; 512], "回源必须覆盖崩溃残留的旧内容");
  });
}

/// 正常追加（写起点 == 当前 EOF）不得回源：判据是写起点而不是写末端，
/// 否则每次追加都会删本地副本 + 整文件 GET。
#[test]
fn append_after_sync_does_not_refetch_whole_file() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = CountingStore::new(false);
    let probe = remote.clone();
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    cloud.write_page(&tbl, 0, &[1u8; 512]).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    let before = probe.get_count();
    // 页 1 的起点 == 页 0 的末端 == 本地/远端当前 EOF：纯追加。
    cloud.write_page(&tbl, 1, &[2u8; 512]).await.unwrap();
    assert_eq!(
      probe.get_count(),
      before,
      "追加写不得触发整文件回源（写起点判据）"
    );
    let mut dst = [0u8; 512];
    cloud.read_page(&tbl, 1, &mut dst).await.unwrap();
    assert_eq!(dst, [2u8; 512]);
  });
}

/// 包装 `MemObjectStore`：统计 `get` 次数，并让 `get` 主动让出一次执行权，
/// 保证并发冷读能在 owner 完成前全部挂到 single-flight 上。
#[derive(Clone)]
struct CountingStore {
  inner: MemObjectStore,
  get_count: Arc<AtomicUsize>,
  yield_on_get: bool,
}

impl CountingStore {
  fn new(yield_on_get: bool) -> Self {
    Self {
      inner: MemObjectStore::new(),
      get_count: Arc::new(AtomicUsize::new(0)),
      yield_on_get,
    }
  }

  fn get_count(&self) -> usize {
    self.get_count.load(Ordering::SeqCst)
  }
}

impl ObjectStore for CountingStore {
  async fn get(&self, key: &str) -> Result<Option<Bytes>> {
    self.get_count.fetch_add(1, Ordering::SeqCst);
    if self.yield_on_get {
      yield_once().await;
    }
    self.inner.get(key).await
  }

  async fn put(
    &self,
    key: &str,
    value: Bytes,
    if_match: Option<&str>,
    if_none_match: bool,
  ) -> Result<String> {
    self.inner.put(key, value, if_match, if_none_match).await
  }

  async fn head(&self, key: &str) -> Result<Option<ObjectMeta>> {
    self.inner.head(key).await
  }

  async fn delete(&self, key: &str) -> Result<()> {
    self.inner.delete(key).await
  }

  async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>> {
    self.inner.list(prefix).await
  }
}

/// 让出一次执行权：同线程上的其它 ready future 得以继续 poll。
async fn yield_once() {
  let mut yielded = false;
  poll_fn(move |cx| {
    if yielded {
      Poll::Ready(())
    } else {
      yielded = true;
      cx.waker().wake_by_ref();
      Poll::Pending
    }
  })
  .await
}

/// 同一文件的 N 个并发冷读只允许一次远端 GET。
#[test]
fn single_flight_concurrent_cold_reads_fetch_once() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = CountingStore::new(true);
    let name = fkv_types::data_file_name(0, "main", 0);
    remote
      .inner
      .put(
        &format!("fkv/{tbl}/{name}"),
        Bytes::from(vec![7u8; 512]),
        None,
        false,
      )
      .await
      .unwrap();
    let probe = remote.clone();
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let fut_li: Vec<_> = (0..8).map(|_| cloud.read_file(&tbl, 0)).collect();
    let result_li = join_all(fut_li).await;
    for result in result_li {
      assert_eq!(result.unwrap().unwrap(), vec![7u8; 512]);
    }
    assert_eq!(probe.get_count(), 1, "并发冷读必须单飞，只能回源一次");
  });
}

/// owner 被取消（future drop）后，等待者必须收到 `TryAgain` 而不是永久挂起。
#[test]
fn single_flight_waiter_returns_try_again_when_owner_dropped() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = CountingStore::new(true);
    let name = fkv_types::data_file_name(0, "main", 0);
    remote
      .inner
      .put(
        &format!("fkv/{tbl}/{name}"),
        Bytes::from(vec![7u8; 512]),
        None,
        false,
      )
      .await
      .unwrap();
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);

    // owner 首次 poll 停在远端 get（yield_once），此时单飞登记已建立。
    let mut owner = Box::pin(cloud.read_file(&tbl, 0));
    assert!(owner.as_mut().poll(&mut cx).is_pending());
    // 后到者挂到同一 key 上等待。
    let mut waiter = Box::pin(cloud.read_file(&tbl, 0));
    assert!(waiter.as_mut().poll(&mut cx).is_pending());

    // 模拟 owner 被取消：DownloadGuard drop 必须唤醒等待者并下发 TryAgain。
    drop(owner);
    assert_eq!(waiter.await.unwrap_err(), KvError::TryAgain);
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

/// 本地后端：不实现整表列举（`list_segment_file_id_li` 直接报错）。
///
/// `adopt_local_file` 若仍逐文件列目录，本后端会让预热失败；改成单文件
/// 完整性判定后，只读尾槽即可登记。
struct NoListStore {
  inner: MemStore,
}

impl PageIo for NoListStore {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> fkv_error::Result<()> {
    self.inner.read_page(tbl, file_page_id, dst).await
  }

  async fn write_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    src: &[u8],
  ) -> fkv_error::Result<()> {
    self.inner.write_page(tbl, file_page_id, src).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> fkv_error::Result<()> {
    self.inner.sync_data(tbl).await
  }
}

impl SegmentIo for NoListStore {
  async fn read_segment<'a>(
    &'a self,
    tbl: &'a TableIdent,
    segment_fp_id: FilePageId,
    dst: &'a mut [u8],
  ) -> Result<()> {
    self.inner.read_segment(tbl, segment_fp_id, dst).await
  }

  async fn write_segment<'a>(
    &'a self,
    tbl: &'a TableIdent,
    segment_fp_id: FilePageId,
    src: &'a [u8],
  ) -> Result<()> {
    self.inner.write_segment(tbl, segment_fp_id, src).await
  }
}

impl FileIo for NoListStore {
  async fn list_file_id_li<'a>(&'a self, tbl: &'a TableIdent) -> Result<Vec<FileId>> {
    self.inner.list_file_id_li(tbl).await
  }

  async fn read_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
  ) -> Result<Option<Vec<u8>>> {
    self.inner.read_file(tbl, file_id).await
  }

  async fn write_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
    bytes: &'a [u8],
  ) -> Result<()> {
    self.inner.write_file(tbl, file_id, bytes).await
  }

  async fn file_size<'a>(&'a self, tbl: &'a TableIdent, file_id: FileId) -> Result<Option<u64>> {
    self.inner.file_size(tbl, file_id).await
  }

  async fn remove_file<'a>(&'a self, tbl: &'a TableIdent, file_id: FileId) -> Result<()> {
    self.inner.remove_file(tbl, file_id).await
  }

  async fn list_segment_file_id_li<'a>(&'a self, _tbl: &'a TableIdent) -> Result<Vec<FileId>> {
    Err(KvError::InvalidArgs)
  }

  async fn remove_segment_file<'a>(&'a self, tbl: &'a TableIdent, file_id: FileId) -> Result<()> {
    self.inner.remove_segment_file(tbl, file_id).await
  }
}

/// 完成标记走 manifest 旁路文件，本后端只需透传（不涉及整表列举）。
impl ManifestIo for NoListStore {
  async fn append_manifest<'a>(&'a self, tbl: &'a TableIdent, record: &'a [u8]) -> Result<()> {
    self.inner.append_manifest(tbl, record).await
  }

  async fn switch_manifest<'a>(&'a self, tbl: &'a TableIdent, record: &'a [u8]) -> Result<()> {
    self.inner.switch_manifest(tbl, record).await
  }

  async fn read_manifest<'a>(&'a self, tbl: &'a TableIdent) -> Result<Option<Vec<u8>>> {
    self.inner.read_manifest(tbl).await
  }

  async fn drop_manifest<'a>(&'a self, tbl: &'a TableIdent) -> Result<()> {
    self.inner.drop_manifest(tbl).await
  }

  async fn write_manifest_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
    record: &'a [u8],
  ) -> Result<()> {
    self.inner.write_manifest_file(tbl, name, record).await
  }

  async fn append_manifest_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
    record: &'a [u8],
  ) -> Result<()> {
    self.inner.append_manifest_file(tbl, name, record).await
  }

  async fn list_manifest_file_name_li<'a>(&'a self, tbl: &'a TableIdent) -> Result<Vec<String>> {
    self.inner.list_manifest_file_name_li(tbl).await
  }

  async fn list_sidecar_file_name_li<'a>(&'a self, tbl: &'a TableIdent) -> Result<Vec<String>> {
    self.inner.list_sidecar_file_name_li(tbl).await
  }

  async fn remove_sidecar_file_name<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
  ) -> Result<()> {
    self.inner.remove_sidecar_file_name(tbl, name).await
  }

  async fn read_manifest_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
  ) -> Result<Option<Vec<u8>>> {
    self.inner.read_manifest_file(tbl, name).await
  }

  async fn remove_manifest_file<'a>(&'a self, tbl: &'a TableIdent, name: &'a str) -> Result<()> {
    self.inner.remove_manifest_file(tbl, name).await
  }
}

/// `adopt_local_file` 的 segment 分支必须只做单文件完整性判定：
/// 逐文件列整表 segment 目录会让预热退化成 O(N²)。
#[test]
fn adopt_local_segment_file_does_not_list_table_dir() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let local = MemStore::new(512).with_segment_config(4096, 1);
    // slot_num = 2，尾槽 = file_id * 2 + 1；写满即完整。
    local.write_segment(&tbl, 1, &[7u8; 4096]).await.unwrap();
    let cloud = CloudStore::new(
      NoListStore { inner: local },
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1);
    assert!(
      cloud.adopt_local_file(&tbl, 0, true, 4096).await.unwrap(),
      "完整 segment 必须被登记"
    );
    assert!(cloud.is_file_resident(&tbl, 0, true));
    assert!(
      !cloud.adopt_local_file(&tbl, 1, true, 4096).await.unwrap(),
      "缺失的 segment 文件不能登记"
    );
  });
}

/// 本地后端：`write_file` 成功但 `file_size` 恒为 `None`（写后不落盘）。
struct NoSizeStore {
  inner: MemStore,
}

impl FileIo for NoSizeStore {
  async fn list_file_id_li<'a>(&'a self, tbl: &'a TableIdent) -> Result<Vec<FileId>> {
    self.inner.list_file_id_li(tbl).await
  }

  async fn read_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
  ) -> Result<Option<Vec<u8>>> {
    self.inner.read_file(tbl, file_id).await
  }

  async fn write_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
    bytes: &'a [u8],
  ) -> Result<()> {
    self.inner.write_file(tbl, file_id, bytes).await
  }

  async fn file_size<'a>(&'a self, _tbl: &'a TableIdent, _file_id: FileId) -> Result<Option<u64>> {
    Ok(None)
  }

  async fn remove_file<'a>(&'a self, tbl: &'a TableIdent, file_id: FileId) -> Result<()> {
    self.inner.remove_file(tbl, file_id).await
  }

  async fn list_segment_file_id_li<'a>(&'a self, tbl: &'a TableIdent) -> Result<Vec<FileId>> {
    self.inner.list_segment_file_id_li(tbl).await
  }

  async fn remove_segment_file<'a>(&'a self, tbl: &'a TableIdent, file_id: FileId) -> Result<()> {
    self.inner.remove_segment_file(tbl, file_id).await
  }
}

/// `cache_local_file` 必须把「写后文件不存在」当失败：按 `bytes.len()` 登记
/// resident 会让后续读写不再回源，直到进程重启。
#[test]
fn cache_local_file_rejects_missing_file_after_write() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let name = fkv_types::data_file_name(0, "main", 0);
    remote
      .put(
        &format!("fkv/{tbl}/{name}"),
        Bytes::from(vec![7u8; 512]),
        None,
        false,
      )
      .await
      .unwrap();
    let cloud = CloudStore::new(
      NoSizeStore {
        inner: MemStore::new(512),
      },
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    assert_eq!(cloud.read_file(&tbl, 0).await.unwrap_err(), KvError::IoFail);
    assert_eq!(cloud.resident_file_count(), 0, "不能登记未落盘的文件");
  });
}

/// 驱逐表时必须摘掉该表的在飞下载：等待者收到 `TryAgain`，owner 完成后也
/// 不能把已 detach 的表重新落盘 / `upsert_resident`（reopen 会当权威）。
#[test]
fn evict_table_cancels_inflight_download_and_rejects_stale_install() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = CountingStore::new(true);
    let name = fkv_types::data_file_name(0, "main", 0);
    remote
      .inner
      .put(
        &format!("fkv/{tbl}/{name}"),
        Bytes::from(vec![7u8; 512]),
        None,
        false,
      )
      .await
      .unwrap();
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut owner = Box::pin(cloud.read_file(&tbl, 0));
    assert!(owner.as_mut().poll(&mut cx).is_pending());
    let mut waiter = Box::pin(cloud.read_file(&tbl, 0));
    assert!(waiter.as_mut().poll(&mut cx).is_pending());

    cloud.evict_table(&tbl);
    assert_eq!(
      waiter.await.unwrap_err(),
      KvError::TryAgain,
      "驱逐后等待者不能被挂死"
    );
    assert_eq!(
      owner.await.unwrap_err(),
      KvError::TryAgain,
      "owner 的下载结果必须作废"
    );
    assert!(
      cloud.local().file_size(&tbl, 0).await.unwrap().is_none(),
      "已驱逐的表不得留下落盘文件"
    );
    assert_eq!(cloud.resident_file_count(), 0);
  });
}

/// 远端对象：首次 GET 让出执行权并返回短对象，之后返回完整对象。
///
/// 用于制造「等待者 round 0 拿到别人的 `Cached`（只覆盖别人起点）」的场景：
/// 等待者必须自己再回源一次，不能按短文件放行越界写。
#[derive(Clone)]
struct GrowingStore {
  inner: MemObjectStore,
  get_count: Arc<AtomicUsize>,
}

impl GrowingStore {
  fn new() -> Self {
    Self {
      inner: MemObjectStore::new(),
      get_count: Arc::new(AtomicUsize::new(0)),
    }
  }
}

impl ObjectStore for GrowingStore {
  async fn get(&self, _key: &str) -> Result<Option<Bytes>> {
    let count = self.get_count.fetch_add(1, Ordering::SeqCst);
    if count == 0 {
      yield_once().await;
      return Ok(Some(Bytes::from(vec![1u8; 1024])));
    }
    Ok(Some(Bytes::from(vec![2u8; 2048])))
  }

  async fn put(
    &self,
    key: &str,
    value: Bytes,
    if_match: Option<&str>,
    if_none_match: bool,
  ) -> Result<String> {
    self.inner.put(key, value, if_match, if_none_match).await
  }

  async fn head(&self, key: &str) -> Result<Option<ObjectMeta>> {
    self.inner.head(key).await
  }

  async fn delete(&self, key: &str) -> Result<()> {
    self.inner.delete(key).await
  }

  async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>> {
    self.inner.list(prefix).await
  }
}

/// `ensure_resident` 的等待者路径必须强制回源：round 0 收到别人的 `Cached`
/// 不代表本地文件覆盖自己的写起点，按轮次关掉回源会留下零洞。
#[test]
fn ensure_resident_waiter_forces_own_refetch() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = GrowingStore::new();
    let probe = remote.clone();
    // pages_per_file_shift = 2：file 0 有 4 页，写第 3 页的起点是 1536。
    let cloud = CloudStore::new(MemStore::new(512), remote, "fkv", 2);
    let mut page = [0u8; 512];
    let mut reader = Box::pin(cloud.read_page(&tbl, 0, &mut page));
    let write_page = [3u8; 512];
    let mut writer = Box::pin(cloud.write_page(&tbl, 3, &write_page));
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    // reader 成为 owner 并停在远端 GET；writer 挂到同一 key 上。
    assert!(reader.as_mut().poll(&mut cx).is_pending());
    assert!(writer.as_mut().poll(&mut cx).is_pending());
    let (read_result, write_result) = futures::join!(reader, writer);
    read_result.unwrap();
    write_result.unwrap();

    assert_eq!(
      probe.get_count.load(Ordering::SeqCst),
      2,
      "等待者必须自己再回源一次"
    );
    // 回源后的完整文件覆盖 page 2；按短文件放行时这里会是零洞。
    let mut dst = [0u8; 512];
    cloud.local().read_page(&tbl, 2, &mut dst).await.unwrap();
    assert_eq!(dst, [2u8; 512]);
  });
}

/// 远端 manifest 发布失败后，本地追加必须回滚；下一次成功写不能把已 abort
/// 的 record 一起发布（否则 allocator 复用 FilePageId 后 reopen 页错位）。
#[test]
fn failed_manifest_publish_rolls_back_local_append() {
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
    );
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"base".to_vec(), b"B".to_vec(), 1)],
      )
      .await
      .unwrap();

    fkv_failpoint::set_spec("CloudManifest:Put@1!err=IoFail");
    let failed = store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"bad".to_vec(), b"X".to_vec(), 2)],
      )
      .await;
    fkv_failpoint::reset();
    assert!(failed.is_err());

    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"good".to_vec(), b"G".to_vec(), 3)],
      )
      .await
      .unwrap();
    drop(store);

    let mut reopened = Store::new(
      CloudStore::new(
        MemStore::new(512),
        remote,
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(
      reopened.get(&tbl, b"base").await.unwrap(),
      Some(b"B".to_vec())
    );
    assert_eq!(reopened.get(&tbl, b"bad").await.unwrap(), None);
    assert_eq!(
      reopened.get(&tbl, b"good").await.unwrap(),
      Some(b"G".to_vec())
    );
  });
}

/// 具名 branch / archive manifest 读取以远端为权威：本地 cache 旧内容必须被
/// 远端覆盖，否则跨 term reopen 会读到 stale 前缀并在下一次 append 覆盖远端。
#[test]
fn named_manifest_read_prefers_remote_over_stale_local_cache() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let local = MemStore::new(512);
    let cloud = CloudStore::new(local, remote.clone(), "fkv", 1);
    let name = "manifest_feature_0";
    cloud
      .write_manifest_file(&tbl, name, b"stale-local")
      .await
      .unwrap();
    remote
      .put(
        "fkv/demo.1/manifest_feature_0",
        Bytes::from_static(b"fresh-remote"),
        None,
        false,
      )
      .await
      .unwrap();
    assert_eq!(
      cloud.read_manifest_file(&tbl, name).await.unwrap().unwrap(),
      b"fresh-remote"
    );
  });
}

/// `drop_manifest` 必须删掉同 branch、无 tag、`file_term <= identity term`
/// 的**全部** manifest。
///
/// 回退即失败：只删最新一条时 `manifest_main_1` 会残留，reopen 按
/// `latest_remote_manifest` 选中它，已 Drop 的表复活。
#[test]
fn drop_manifest_removes_all_superseded_term_manifests() {
  pollster::block_on(async {
    let tbl = TableIdent::new("dropall", 1).unwrap();
    let remote = MemObjectStore::new();
    for term in [1u64, 3] {
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
      .with_branch("main", term);
      store
        .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"A".to_vec(), 1)])
        .await
        .unwrap();
    }
    let key = |term: u64| format!("fkv/{tbl}/manifest_main_{term}");
    assert!(remote.get(&key(1)).await.unwrap().is_some());
    assert!(remote.get(&key(3)).await.unwrap().is_some());

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
    .with_branch("main", 3);
    store.drop_table(&tbl).await.unwrap();
    for term in [1u64, 3] {
      assert!(
        remote.get(&key(term)).await.unwrap().is_none(),
        "term {term} 的 manifest 必须被删净"
      );
    }
  });
}

/// 记录 `delete` 顺序的远端：用于钉住 manifest 删除顺序。
#[derive(Clone)]
struct DeleteRecorder {
  inner: MemObjectStore,
  delete_li: Arc<Mutex<Vec<String>>>,
}

impl ObjectStore for DeleteRecorder {
  async fn get(&self, key: &str) -> Result<Option<Bytes>> {
    self.inner.get(key).await
  }

  async fn put(
    &self,
    key: &str,
    value: Bytes,
    if_match: Option<&str>,
    if_none_match: bool,
  ) -> Result<String> {
    self.inner.put(key, value, if_match, if_none_match).await
  }

  async fn head(&self, key: &str) -> Result<Option<ObjectMeta>> {
    self.inner.head(key).await
  }

  async fn delete(&self, key: &str) -> Result<()> {
    self.delete_li.lock().unwrap().push(key.to_owned());
    self.inner.delete(key).await
  }

  async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>> {
    self.inner.list(prefix).await
  }
}

/// 删除顺序：旧 term 先删、本 identity（最高 term）最后。
///
/// 回退即失败：先删 own 时并发读者会在两次删除之间只看到旧 manifest，
/// 采纳后回写把它晋升成新权威。
#[test]
fn drop_manifest_deletes_old_terms_before_own() {
  pollster::block_on(async {
    let tbl = TableIdent::new("droporder", 1).unwrap();
    let remote = DeleteRecorder {
      inner: MemObjectStore::new(),
      delete_li: Arc::new(Mutex::new(Vec::new())),
    };
    for term in [1u64, 3] {
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
      .with_branch("main", term);
      store
        .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"A".to_vec(), 1)])
        .await
        .unwrap();
    }
    remote.delete_li.lock().unwrap().clear();

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
    .with_branch("main", 3);
    store.drop_table(&tbl).await.unwrap();

    let log = remote.delete_li.lock().unwrap().clone();
    let index_of = |name: &str| {
      log
        .iter()
        .position(|key| key.ends_with(name))
        .unwrap_or_else(|| panic!("未删除 {name}: {log:?}"))
    };
    assert!(
      index_of("manifest_main_1") < index_of("manifest_main_3"),
      "旧 term 必须先删、own 最后: {log:?}"
    );
  });
}

/// `evict_table` 摘掉在飞登记后，新一代 owner 复用同一 key；旧 owner 完成时
/// 不得摘掉新登记、也不得把旧结果发给新登记的等待者。
///
/// 回退即失败：无世代号时旧 owner 的 `finish_download` 会删掉新入口，
/// 新等待者收到旧 outcome（这里是 `TryAgain`）。
#[test]
fn stale_owner_finish_keeps_new_generation_inflight() {
  pollster::block_on(async {
    let tbl = TableIdent::new("flight", 1).unwrap();
    let remote = CountingStore::new(true);
    let name = fkv_types::data_file_name(0, "main", 0);
    remote
      .inner
      .put(
        &format!("fkv/{tbl}/{name}"),
        Bytes::from(vec![7u8; 512]),
        None,
        false,
      )
      .await
      .unwrap();
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);

    // 第一代 owner：停在远端 get。
    let mut owner1 = Box::pin(cloud.read_file(&tbl, 0));
    assert!(owner1.as_mut().poll(&mut cx).is_pending());
    // 驱逐摘掉第一代登记。
    cloud.evict_table(&tbl);
    // 第二代 owner 复用同一 key，并挂一个等待者。
    let mut owner2 = Box::pin(cloud.read_file(&tbl, 0));
    assert!(owner2.as_mut().poll(&mut cx).is_pending());
    let mut waiter = Box::pin(cloud.read_file(&tbl, 0));
    assert!(waiter.as_mut().poll(&mut cx).is_pending());

    // 旧 owner 取消：只允许摘自己那代登记。
    drop(owner1);

    // 先驱动第二代 owner 完成，再收等待者结果（等待者挂在它的登记上）。
    assert_eq!(owner2.await.unwrap().unwrap(), vec![7u8; 512]);
    assert!(
      waiter.await.is_ok(),
      "新等待者必须拿到第二代 owner 的结果，而不是旧 owner 的 TryAgain"
    );
  });
}

/// switch_manifest 发布失败必须把本地快照切回旧内容，否则下一次写会把失败
/// 的 truncate / archive 提升成远端权威。
#[test]
fn failed_switch_manifest_publish_restores_local_snapshot() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(MemStore::new(512), remote, "fkv", 1);
    let old = b"old-snapshot";
    cloud.switch_manifest(&tbl, old).await.unwrap();
    fkv_failpoint::set_spec("CloudManifest:Put@1!err=IoFail");
    assert_eq!(
      cloud
        .switch_manifest(&tbl, b"new-snapshot")
        .await
        .unwrap_err(),
      KvError::IoFail
    );
    fkv_failpoint::reset();
    assert_eq!(
      cloud.read_manifest(&tbl).await.unwrap().unwrap(),
      old.to_vec()
    );
  });
}

/// reopen 失败（远端 manifest 消失）后必须清空内存表状态；否则后续 put 会
/// 把 log record 写成新 manifest 的第一条，reopen 直接 EndOfFile。
#[test]
fn failed_reopen_clears_memory_before_next_put() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut store = Store::new(
      CloudStore::new(MemStore::new(512), remote.clone(), "fkv", 1),
      512,
      16,
    );
    store
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    remote.delete("fkv/demo.1/manifest_main_0").await.unwrap();
    assert_eq!(
      store.reopen(&tbl).await.unwrap_err(),
      KvError::ResourceMissing
    );
    store
      .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)])
      .await
      .unwrap();
    drop(store);

    let mut reopened = Store::new(
      CloudStore::new(MemStore::new(512), remote, "fkv", 1),
      512,
      16,
    );
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(reopened.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
  });
}

/// 测试包装 store 只实现 FileIo，但 CloudStore 的 FileIo 删 segment 时要清
/// 本地完成标记（ManifestIo）；统一委托给内层 MemStore。
macro_rules! impl_manifest_io_via_inner {
  ($ty:ty) => {
    impl ManifestIo for $ty {
      async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
        self.inner.append_manifest(tbl, record).await
      }
      async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
        self.inner.switch_manifest(tbl, record).await
      }
      async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
        self.inner.read_manifest(tbl).await
      }
      async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
        self.inner.drop_manifest(tbl).await
      }
      async fn write_manifest_file(
        &self,
        tbl: &TableIdent,
        name: &str,
        record: &[u8],
      ) -> Result<()> {
        self.inner.write_manifest_file(tbl, name, record).await
      }
      async fn append_manifest_file(
        &self,
        tbl: &TableIdent,
        name: &str,
        record: &[u8],
      ) -> Result<()> {
        self.inner.append_manifest_file(tbl, name, record).await
      }
      async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
        self.inner.list_manifest_file_name_li(tbl).await
      }
      async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
        self.inner.list_sidecar_file_name_li(tbl).await
      }
      async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()> {
        self.inner.remove_sidecar_file_name(tbl, name).await
      }
      async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
        self.inner.read_manifest_file(tbl, name).await
      }
      async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
        self.inner.remove_manifest_file(tbl, name).await
      }
    }
  };
}

impl_manifest_io_via_inner!(FlakyRemoveStore);
impl_manifest_io_via_inner!(NoSizeStore);
