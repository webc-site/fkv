//! manifest 发布回滚、重开与 term 清理测试。

use super::*;

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
      async fn seed_local_manifest_file(
        &self,
        tbl: &TableIdent,
        name: &str,
        bytes: &[u8],
      ) -> Result<()> {
        self.inner.seed_local_manifest_file(tbl, name, bytes).await
      }
      async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
        self.inner.remove_manifest_file(tbl, name).await
      }
    }
  };
}

impl_manifest_io_via_inner!(FlakyRemoveStore);
impl_manifest_io_via_inner!(NoSizeStore);

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
