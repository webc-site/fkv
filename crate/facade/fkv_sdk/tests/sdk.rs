use fkv_btree::BatchEntry;
use fkv_config::KvOption;
use fkv_error::KvError;
use fkv_sdk::{KvStore, PageOption};
use fkv_store::ScanOption;
use fkv_testkit::TempRoot;
#[cfg(target_os = "linux")]
use fkv_testkit::direct_io_probe;
use fkv_types::TableIdent;

/// Cloud / Standby 配置不能静默降级成本地盘：SDK 只装配 Local，必须显式报错。
#[test]
fn sdk_rejects_non_local_mode() {
  let root = TempRoot::new("sdk");
  let cloud = KvOption {
    cloud_store_path: "fkv-test/prefix".to_owned(),
    cloud_endpoint: "http://127.0.0.1:1".to_owned(),
    ..KvOption::default()
  };
  let err = match KvStore::new(root.path().to_path_buf(), &cloud, 0) {
    Ok(_) => panic!("Cloud 配置必须报错，不能静默按 Local"),
    Err(err) => err,
  };
  assert_eq!(err, KvError::InvalidArgs);

  let standby = KvOption {
    enable_local_standby: true,
    store_path: vec![root.path().to_string_lossy().into_owned()],
    standby_master_addr: "local".to_owned(),
    standby_master_store_paths: vec![root.path().to_string_lossy().into_owned()],
    ..KvOption::default()
  };
  let err = match KvStore::new(root.path().to_path_buf(), &standby, 0) {
    Ok(_) => panic!("Standby 配置必须报错，不能静默按 Local"),
    Err(err) => err,
  };
  assert_eq!(err, KvError::InvalidArgs);

  // 纯 Local 不受影响。
  assert!(KvStore::new(root.path().to_path_buf(), &KvOption::default(), 0).is_ok());
}

#[compio::test]
async fn sdk_put_get_delete_scan_floor_and_reopen() {
  let root = TempRoot::new("sdk");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let option = KvOption::default();
  let mut store = KvStore::new(root.path().to_path_buf(), &option, 0).unwrap();
  store
    .put_kv(&tbl, b"a".to_vec(), b"A".to_vec())
    .await
    .unwrap();
  store
    .put_kv(&tbl, b"b".to_vec(), b"B".to_vec())
    .await
    .unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(store.read(&tbl, b"b").await.unwrap(), b"B");
  assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 2);
  assert_eq!(store.floor(&tbl, b"c").await.unwrap().key, b"b");
  assert_eq!(
    store.floor(&tbl, b"0").await.unwrap_err(),
    KvError::NotFound
  );
  store.delete(&tbl, b"a").await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), None);
  assert_eq!(store.read(&tbl, b"a").await.unwrap_err(), KvError::NotFound);
  drop(store);

  let mut store = KvStore::new(root.path().to_path_buf(), &option, 0).unwrap();
  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
}

#[compio::test]
async fn sdk_large_value_roundtrip() {
  let root = TempRoot::new("sdk");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let option = KvOption::default();
  let mut store = KvStore::new(root.path().to_path_buf(), &option, 0).unwrap();
  let value: Vec<u8> = (0..30_000).map(|index| (index % 251) as u8).collect();
  store
    .put_large(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), value.clone(), 1).with_metadata(b"meta")],
    )
    .await
    .unwrap();
  let read = store.get_large(&tbl, b"big").await.unwrap().unwrap();
  assert_eq!(read.value, value);
  assert_eq!(read.metadata, b"meta");
  drop(store);

  let mut store = KvStore::new(root.path().to_path_buf(), &option, 0).unwrap();
  store.reopen(&tbl).await.unwrap();
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    value
  );
  assert_eq!(
    store.scan_kv(&tbl, b"", b"", 0).await.unwrap_err(),
    KvError::LargeValueUnsupported
  );
}

/// sem-34：`ReadRequest` 的公开输出 `value_` / `ts_` / `expire_ts_` 在 SDK 入口
/// 可见；large value 走 metadata-only（对齐 C++ 默认 monostate），完整 value
/// 仍由 `get_large` 提供。
#[compio::test]
async fn sdk_get_entry_matches_cpp_read_request_outputs() {
  let root = TempRoot::new("sdk");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let option = KvOption::default();
  let mut store = KvStore::new(root.path().to_path_buf(), &option, 0).unwrap();

  store
    .put_kv_at(&tbl, b"small".to_vec(), b"S".to_vec(), 100)
    .await
    .unwrap();
  let value: Vec<u8> = (0..30_000).map(|index| (index % 251) as u8).collect();
  let mut large = BatchEntry::upsert(b"big".to_vec(), value.clone(), 7).with_metadata(b"meta");
  large.expire_ts = u64::MAX / 2;
  store.put_large(&tbl, &[large]).await.unwrap();

  let small = store.get_entry(&tbl, b"small").await.unwrap().unwrap();
  assert!(!small.is_large_value);
  assert_eq!(small.value, b"S");
  assert!(small.metadata.is_empty());
  assert_eq!(small.timestamp, 100);
  assert_eq!(small.expire_ts, 0);

  let big = store.get_entry(&tbl, b"big").await.unwrap().unwrap();
  assert!(big.is_large_value);
  assert!(big.value.is_empty());
  assert_eq!(big.metadata, b"meta");
  assert_eq!(big.timestamp, 7);
  assert_eq!(big.expire_ts, u64::MAX / 2);

  // 完整 value 仍由 `get_large` 提供，并携带同样的 entry 元数据。
  let full = store.get_large(&tbl, b"big").await.unwrap().unwrap();
  assert_eq!(full.value, value);
  assert_eq!(full.metadata, b"meta");
  assert_eq!(full.timestamp, 7);
  assert_eq!(full.expire_ts, u64::MAX / 2);

  // 既有分层不变。
  assert_eq!(
    store.get(&tbl, b"big").await.unwrap_err(),
    KvError::LargeValueUnsupported
  );
}

/// TTL 未过期的 entry 原样返回 `expire_ts`，已过期的被读过滤剔除。
#[compio::test]
async fn sdk_scan_kv_returns_expire_ts_and_filters_expired() {
  let root = TempRoot::new("sdk");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let option = KvOption::default();
  let mut store = KvStore::new(root.path().to_path_buf(), &option, 0).unwrap();

  let now = fkv_types::now_ms();
  let mut live = BatchEntry::upsert(b"live".to_vec(), b"L".to_vec(), 1);
  live.expire_ts = now + 60_000;
  let mut dead = BatchEntry::upsert(b"dead".to_vec(), b"D".to_vec(), 1);
  dead.expire_ts = 1;
  let plain = BatchEntry::upsert(b"plain".to_vec(), b"P".to_vec(), 1);
  // now=0 写入：过期时间尚未到达，三条 entry 都落主树。
  store.put_at(&tbl, &[dead, live, plain], 0).await.unwrap();

  let entry_li = store.scan_kv(&tbl, b"", b"", 0).await.unwrap();
  assert_eq!(entry_li.len(), 2);
  assert_eq!(entry_li[0].key, b"live");
  assert_eq!(entry_li[0].value, b"L");
  assert_eq!(entry_li[0].timestamp, 1);
  assert_eq!(entry_li[0].expire_ts, now + 60_000);
  assert_eq!(entry_li[1].key, b"plain");
  assert_eq!(entry_li[1].timestamp, 1);
  assert_eq!(entry_li[1].expire_ts, 0);
  assert_eq!(store.scan_kv(&tbl, b"", b"", 0).await.unwrap(), entry_li);

  // 分页路径同样保留 expire_ts。
  let page = store
    .scan_kv_page(
      &tbl,
      b"",
      b"",
      ScanOption::default(),
      PageOption {
        page_entries: 1,
        page_size: 0,
      },
    )
    .await
    .unwrap();
  assert_eq!(page.entry_li.len(), 1);
  assert_eq!(page.entry_li[0].timestamp, 1);
  assert_eq!(page.entry_li[0].expire_ts, now + 60_000);
  assert!(page.has_remaining);
}

#[compio::test]
async fn sdk_exists_matches_get_and_covers_large_value() {
  let root = TempRoot::new("sdk");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let option = KvOption::default();
  let mut store = KvStore::new(root.path().to_path_buf(), &option, 0).unwrap();

  assert!(!store.exists(&tbl, b"a").await.unwrap());
  store
    .put_kv(&tbl, b"a".to_vec(), b"A".to_vec())
    .await
    .unwrap();
  assert!(store.exists(&tbl, b"a").await.unwrap());
  assert!(store.exists(&tbl, b"a").await.unwrap());

  let value: Vec<u8> = (0..30_000).map(|index| (index % 251) as u8).collect();
  store
    .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), value, 1)])
    .await
    .unwrap();
  // large value 的 get 报 LargeValueUnsupported，exists 仍需返回 true。
  assert_eq!(
    store.get(&tbl, b"big").await.unwrap_err(),
    KvError::LargeValueUnsupported
  );
  assert!(store.exists(&tbl, b"big").await.unwrap());

  store.delete(&tbl, b"a").await.unwrap();
  assert!(!store.exists(&tbl, b"a").await.unwrap());
}

#[compio::test]
async fn sdk_explicit_timestamp_put_and_delete() {
  let root = TempRoot::new("sdk");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let option = KvOption::default();
  let mut store = KvStore::new(root.path().to_path_buf(), &option, 0).unwrap();

  store
    .put_kv_at(&tbl, b"k".to_vec(), b"v1".to_vec(), 100)
    .await
    .unwrap();
  assert_eq!(store.get(&tbl, b"k").await.unwrap(), Some(b"v1".to_vec()));
  store
    .put_kv_at(&tbl, b"k".to_vec(), b"v2".to_vec(), 200)
    .await
    .unwrap();
  assert_eq!(store.get(&tbl, b"k").await.unwrap(), Some(b"v2".to_vec()));

  store.delete_at(&tbl, b"k", 300).await.unwrap();
  assert!(!store.exists(&tbl, b"k").await.unwrap());
}

/// sto-62c：钉住 `Drop`（按分区删整表）与 `Truncate`（清内容保留表）的语义差异，
/// 并验证单 key 删除只影响目标 key、重启后仍是 tombstone。
///
/// 请求侧对应：单 key 删除 = `Request::Put` 携带 `BatchEntry::delete`；
/// `Request::Drop` = 本测试里的 `drop_table`；`Request::Truncate` = `truncate`。
#[compio::test]
async fn sdk_delete_keeps_siblings_drop_and_truncate_semantics() {
  let root = TempRoot::new("sdk");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let other = TableIdent::new("demo", 2).unwrap();
  let option = KvOption::default();
  let mut store = KvStore::new(root.path().to_path_buf(), &option, 0).unwrap();

  // 单 key 删除：只影响该 key，同表其它 key 不受影响，重启后 tombstone 仍生效。
  for (key, value) in [(b"a", b"A"), (b"b", b"B"), (b"c", b"C")] {
    store
      .put_kv(&tbl, key.to_vec(), value.to_vec())
      .await
      .unwrap();
  }
  store
    .put_kv(&other, b"a".to_vec(), b"OTHER".to_vec())
    .await
    .unwrap();
  store.delete(&tbl, b"b").await.unwrap();
  assert_eq!(store.get(&tbl, b"b").await.unwrap(), None);
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(store.get(&tbl, b"c").await.unwrap(), Some(b"C".to_vec()));
  drop(store);

  let mut store = KvStore::new(root.path().to_path_buf(), &option, 0).unwrap();
  store.reopen(&tbl).await.unwrap();
  store.reopen(&other).await.unwrap();
  assert_eq!(store.get(&tbl, b"b").await.unwrap(), None);
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

  // Drop 按分区删整表：目标分区的键全部消失，兄弟分区不受影响。
  store.drop_table(&tbl).await.unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), None);
  assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 0);
  assert_eq!(
    store.get(&other, b"a").await.unwrap(),
    Some(b"OTHER".to_vec())
  );

  // Truncate 只清内容、保留表：清空后同一张表仍可继续读写。
  store.truncate(&other).await.unwrap();
  assert_eq!(store.scan(&other, b"", b"", 0).await.unwrap().len(), 0);
  store
    .put_kv(&other, b"z".to_vec(), b"Z".to_vec())
    .await
    .unwrap();
  assert_eq!(store.get(&other, b"z").await.unwrap(), Some(b"Z".to_vec()));
}

/// `page_cache_bytes` 必须透传到 SDK 装配的 Store：
/// 预算压到 512 字节后，2000 个键的写入 + 读取必须触发逐出。
#[compio::test]
async fn sdk_page_cache_bytes_reaches_store() {
  let root = TempRoot::new("sdk_cache");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let option = KvOption {
    page_cache_bytes: 512,
    ..KvOption::default()
  };
  let mut store = KvStore::new(root.path().to_path_buf(), &option, 0).unwrap();
  let batch: Vec<BatchEntry> = (0..2000)
    .map(|idx| BatchEntry::upsert(format!("k{idx:04}").into_bytes(), vec![b'v'; 8], 1))
    .collect();
  store.put(&tbl, &batch).await.unwrap();
  for idx in (0..2000).step_by(7) {
    assert_eq!(
      store
        .get(&tbl, format!("k{idx:04}").as_bytes())
        .await
        .unwrap(),
      Some(vec![b'v'; 8])
    );
  }
  let stat = store.inner().page_cache_stat();
  assert!(stat.evict > 0, "页缓存预算必须触发逐出: {stat:?}");
}

/// `store_path` 是配置驱动的数据根：SDK 只有一个 `Store`，与传入 root 冲突
/// （或要求多根）时必须拒绝，不能静默写进 root。
#[test]
fn sdk_rejects_conflicting_store_path() {
  let root = TempRoot::new("sdk_path");
  let other = TempRoot::new("sdk_path_other");
  let root_path = root.path().to_path_buf();
  let root_str = root_path.to_string_lossy().into_owned();
  let other_str = other.path().to_string_lossy().into_owned();

  for store_path in [
    vec![other_str.clone()],
    vec![root_str.clone(), other_str.clone()],
  ] {
    let option = KvOption {
      store_path,
      ..KvOption::default()
    };
    let err = match KvStore::new(root_path.clone(), &option, 0) {
      Ok(_) => panic!("store_path 与 root 冲突必须报错"),
      Err(err) => err,
    };
    assert_eq!(err, KvError::InvalidArgs);
  }

  // 单根且与 root 一致：正常装配。
  let option = KvOption {
    store_path: vec![root_str],
    store_path_weights: vec![1],
    ..KvOption::default()
  };
  assert!(KvStore::new(root_path, &option, 0).is_ok());
}

/// 重启后不显式 reopen 也不能丢数据：首次 get/put 自动加载已有 manifest。
#[compio::test]
async fn sdk_lazily_reopens_existing_table() {
  let root = TempRoot::new("sdk_auto_reopen");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let option = KvOption::default();
  {
    let mut store = KvStore::new(root.path().to_path_buf(), &option, 0).unwrap();
    store
      .put_kv(&tbl, b"a".to_vec(), b"A".to_vec())
      .await
      .unwrap();
  }
  // 新进程/新 Store：不调用 reopen，直接读 + 写。
  let mut store = KvStore::new(root.path().to_path_buf(), &option, 0).unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  store
    .put_kv(&tbl, b"b".to_vec(), b"B".to_vec())
    .await
    .unwrap();
  assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
  assert_eq!(store.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
}

/// io-18：SDK 默认配置（4 KiB 页 + 256 KiB 段）必须真的按 O_DIRECT 打开 data
/// 文件；`lib.rs` 里 `.with_direct_io(true)` 被删掉或改成 false 时本用例失败。
#[cfg(target_os = "linux")]
#[compio::test]
async fn sdk_default_wiring_opens_direct_data_file() {
  let root = TempRoot::new("sdk_direct");
  let probe = direct_io_probe(root.path());
  if !probe.supported() {
    eprintln!("跳过：临时目录不支持 O_DIRECT");
    return;
  }
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = KvStore::new(root.path().to_path_buf(), &KvOption::default(), 0).unwrap();
  store
    .put_kv(&tbl, b"k".to_vec(), vec![7u8; 8 * 1024])
    .await
    .unwrap();
  assert_eq!(
    store.get(&tbl, b"k").await.unwrap(),
    Some(vec![7u8; 8 * 1024])
  );
  // 8 KiB value 由 `put_kv`（普通 Put）写入 data 文件 overflow 链，不落
  // segment；按类别断言 data 句柄，避免「任意 direct fd ≥ 1」被 manifest /
  // segment 单独满足。
  let (data, segment) = probe.fd_kind_num();
  assert!(
    data >= 1,
    "SDK 默认配置没有按 O_DIRECT 打开 data 文件：data={data} segment={segment}"
  );
}
