use std::sync::{
  Arc, Mutex,
  atomic::{AtomicU64, Ordering},
};

use bytes::Bytes;
use fkv_cloud::CloudStore;
use fkv_error::{KvError, Result};
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo, TermIo};
use fkv_io_mem::MemStore;
use fkv_object::{MemObjectStore, ObjectMeta, ObjectStore};
use fkv_types::TableIdent;

/// 记录每次写请求的 key / 字节数，用来量化写放大与刷盘顺序。
#[derive(Clone, Default)]
struct ProbeStore {
  inner: MemObjectStore,
  put_num: Arc<AtomicU64>,
  put_bytes: Arc<AtomicU64>,
  fail_put_num: Arc<AtomicU64>,
  log: Arc<Mutex<Vec<String>>>,
}

impl ProbeStore {
  fn put_num(&self) -> u64 {
    self.put_num.load(Ordering::Relaxed)
  }

  fn put_bytes(&self) -> u64 {
    self.put_bytes.load(Ordering::Relaxed)
  }

  fn log(&self) -> Vec<String> {
    self.log.lock().unwrap().clone()
  }
}

impl ObjectStore for ProbeStore {
  fn supports_put_range(&self) -> bool {
    true
  }

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
    if self.fail_put_num.load(Ordering::Relaxed) > 0 {
      self.fail_put_num.fetch_sub(1, Ordering::Relaxed);
      return Err(KvError::TryAgain);
    }
    self.put_num.fetch_add(1, Ordering::Relaxed);
    self
      .put_bytes
      .fetch_add(value.len() as u64, Ordering::Relaxed);
    self.log.lock().unwrap().push(format!("put:{key}"));
    self.inner.put(key, value, if_match, if_none_match).await
  }

  async fn put_range(&self, key: &str, offset: u64, bytes: &[u8]) -> Result<()> {
    self.put_num.fetch_add(1, Ordering::Relaxed);
    self
      .put_bytes
      .fetch_add(bytes.len() as u64, Ordering::Relaxed);
    self.log.lock().unwrap().push(format!("put_range:{key}"));
    self.inner.put_range(key, offset, bytes).await
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

fn segment_cloud(remote: ProbeStore) -> CloudStore<MemStore, ProbeStore> {
  CloudStore::new(
    MemStore::new(512).with_segment_config(4096, 1),
    remote,
    "fkv",
    18,
  )
  .with_segment_config(4096, 1)
}

#[test]
fn segment_upload_is_deferred_and_assembled_once_per_file() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = ProbeStore::default();
    let cloud = segment_cloud(remote.clone());

    // 同一 segment 文件的两个槽位：写段阶段不应产生任何上传。
    cloud.write_segment(&tbl, 0, &[1u8; 4096]).await.unwrap();
    cloud.write_segment(&tbl, 1, &[2u8; 4096]).await.unwrap();
    assert_eq!(remote.put_num(), 0);
    assert!(remote.list("fkv/demo.1/").await.unwrap().is_empty());

    // sync 时每个脏 segment 文件只组装 / 上传一次（整文件 8 KiB）。
    cloud.sync_data(&tbl).await.unwrap();
    assert_eq!(remote.put_num(), 1);
    assert_eq!(remote.put_bytes(), 8192);
    let bytes = remote
      .get("fkv/demo.1/segment_0_main_0")
      .await
      .unwrap()
      .unwrap();
    assert_eq!(bytes.len(), 8192);
    assert_eq!(bytes[0], 1);
    assert_eq!(bytes[4096], 2);

    // 第二个文件只上传自己的 8 KiB，不会连带重传第一个文件。
    cloud.write_segment(&tbl, 2, &[3u8; 4096]).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    assert_eq!(remote.put_num(), 2);
    assert_eq!(remote.put_bytes(), 16384);
  });
}

#[test]
fn manifest_file_flushes_dirty_data_and_segments_first() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = ProbeStore::default();
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2).with_segment_config(4096, 1),
      remote.clone(),
      "fkv",
      2,
    )
    .with_segment_config(4096, 1);

    let page = |tag: u8| vec![tag; 512];
    cloud.write_page(&tbl, 0, &page(1)).await.unwrap();
    cloud.write_page(&tbl, 4, &page(2)).await.unwrap();
    cloud.write_segment(&tbl, 0, &[9u8; 4096]).await.unwrap();
    cloud
      .write_manifest_file(&tbl, "manifest_feature_0", b"snap")
      .await
      .unwrap();

    let log = remote.log();
    let data_pos = log
      .iter()
      .position(|entry| entry == "put_range:fkv/demo.1/data_0_main_0")
      .expect("data file 0 must be uploaded before manifest");
    let data1_pos = log
      .iter()
      .position(|entry| entry == "put_range:fkv/demo.1/data_1_main_0")
      .expect("data file 1 must be uploaded before manifest");
    let segment_pos = log
      .iter()
      .position(|entry| entry == "put:fkv/demo.1/segment_0_main_0")
      .expect("dirty segment must be uploaded before manifest");
    let manifest_pos = log
      .iter()
      .position(|entry| entry == "put:fkv/demo.1/manifest_feature_0")
      .expect("manifest must be uploaded");
    assert!(data_pos < manifest_pos);
    assert!(data1_pos < manifest_pos);
    assert!(segment_pos < manifest_pos);

    // append 路径同样先落数据。
    cloud.write_page(&tbl, 8, &page(3)).await.unwrap();
    cloud
      .append_manifest_file(&tbl, "manifest_feature_0", b"rec")
      .await
      .unwrap();
    let log = remote.log();
    let data_pos = log
      .iter()
      .position(|entry| entry == "put_range:fkv/demo.1/data_2_main_0")
      .expect("data file 2 must be uploaded before append");
    let manifest_pos = log
      .iter()
      .rposition(|entry| entry == "put:fkv/demo.1/manifest_feature_0")
      .expect("manifest must be appended");
    assert!(data_pos < manifest_pos);
  });
}

#[test]
fn segment_cache_respects_budget_and_redownloads() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = ProbeStore::default();
    // 一个 segment 文件 8 KiB，预算只容得下一个文件。
    let cloud = segment_cloud(remote.clone()).with_cache_budget(8192);
    cloud.write_segment(&tbl, 0, &[1u8; 4096]).await.unwrap();
    cloud.write_segment(&tbl, 1, &[2u8; 4096]).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    cloud.write_segment(&tbl, 2, &[3u8; 4096]).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();

    // file 1 仍是脏 / 最新，file 0 被预算淘汰。
    let file_id_li = cloud.local().list_segment_file_id_li(&tbl).await.unwrap();
    assert_eq!(file_id_li, vec![1]);
    assert_eq!(cloud.resident_bytes(), 8192);

    // 被淘汰的段重新从远端整文件拉回。
    let mut buf = [0u8; 4096];
    cloud.read_segment(&tbl, 0, &mut buf).await.unwrap();
    assert_eq!(buf[0], 1);
    assert!(
      cloud
        .local()
        .list_segment_file_id_li(&tbl)
        .await
        .unwrap()
        .contains(&0)
    );
  });
}

#[test]
fn upsert_term_same_term_is_idempotent() {
  pollster::block_on(async {
    let remote = ProbeStore::default();
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    assert_eq!(cloud.upsert_term("main", 1, 3).await.unwrap(), 3);
    let put_num = remote.put_num();
    // current == term：直接返回，不再发冗余 PUT。
    assert_eq!(cloud.upsert_term("main", 1, 3).await.unwrap(), 3);
    assert_eq!(remote.put_num(), put_num);
    assert_eq!(cloud.read_term("main", 1).await.unwrap(), Some(3));
  });
}

/// 预热只安装本地不存在的文件：半截 segment 可能是另一实例正在写的段，
/// 不能删除或整文件覆盖，否则对方的 flush 会把缺段组装成零洞。
#[test]
fn prewarm_skips_incomplete_local_segment_file() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = ProbeStore::default();
    let local = MemStore::new(512).with_segment_config(4096, 1);
    // 只写首个槽位：尾槽缺失，文件长度不足名义 8 KiB。
    local.write_segment(&tbl, 0, &[7u8; 4096]).await.unwrap();
    let mut full = vec![0u8; 8192];
    full[..4096].fill(7);
    full[4096..].fill(9);
    remote
      .put(
        "fkv/demo.1/segment_0_main_0",
        Bytes::from(full),
        None,
        false,
      )
      .await
      .unwrap();
    let cloud = CloudStore::new(
      local,
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1);
    assert!(cloud.prewarm_segment_file(&tbl, 0).await.unwrap());
    // 预热跳过半截文件，不登记为 resident（旧行为会覆盖成完整文件并登记）。
    assert!(!cloud.is_file_resident(&tbl, 0, true));
    assert_eq!(cloud.resident_file_count(), 0);
    // 本地半截文件保持原样：首段仍是 7；后续读路径再按 miss 回源补整份。
    let mut dst = vec![0u8; 4096];
    cloud.read_segment(&tbl, 0, &mut dst).await.unwrap();
    assert_eq!(dst, vec![7u8; 4096]);
    cloud.read_segment(&tbl, 1, &mut dst).await.unwrap();
    assert_eq!(dst, vec![9u8; 4096]);
  });
}

/// 远端 segment 对象短于名义定长文件 → 按 Corrupted 拒绝，不能零补齐伪装合法。
#[test]
fn short_remote_segment_object_is_corrupted() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = ProbeStore::default();
    remote
      .put(
        "fkv/demo.1/segment_0_main_0",
        Bytes::from(vec![7u8; 4096]),
        None,
        false,
      )
      .await
      .unwrap();
    let cloud = segment_cloud(remote);
    let mut dst = vec![0u8; 4096];
    assert_eq!(
      cloud.read_segment(&tbl, 0, &mut dst).await.unwrap_err(),
      KvError::Corrupted
    );
  });
}

/// cld-50：manifest 上传遇到瞬态错误会重试到成功。
#[test]
fn manifest_upload_retries_transient_error() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = ProbeStore::default();
    remote.fail_put_num.store(2, Ordering::Relaxed);
    let cloud = segment_cloud(remote.clone());
    cloud
      .write_manifest_file(&tbl, "manifest_main_0", b"snap")
      .await
      .unwrap();
    assert_eq!(remote.put_num(), 1);
    assert_eq!(remote.fail_put_num.load(Ordering::Relaxed), 0);
  });
}
