use std::{
  collections::HashMap,
  fs::{read, read_dir},
};

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_page::{CompressionType, DataPageIter, PageType, page_type};
use fkv_store::{PageOption, ScanOption, Store};
use fkv_testkit::TempRoot;
use fkv_types::{DATA_FILE_PREFIX, TableIdent, now_ms};

/// 高可压缩 value：500B 重复字节，远超 SKIP_COMPRESSION_THRESHOLD。
fn compressible(seed: u8) -> Vec<u8> {
  vec![seed; 500]
}

#[compio::test]
async fn mem_store_compression_roundtrip_read_and_scan() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(MemStore::new(4096), 4096, 16).with_compression_level(3);
  let a = compressible(b'a');
  let b = compressible(b'b');
  store
    .put(
      &tbl,
      &[
        BatchEntry::upsert(b"k1".to_vec(), a.clone(), 1),
        BatchEntry::upsert(b"k2".to_vec(), b.clone(), 1),
        // 短值不压缩，走 None 分支。
        BatchEntry::upsert(b"k3".to_vec(), b"tiny".to_vec(), 1),
      ],
    )
    .await
    .unwrap();

  assert_eq!(store.read(&tbl, b"k1").await.unwrap(), a);
  assert_eq!(store.read(&tbl, b"k2").await.unwrap(), b);
  assert_eq!(store.read(&tbl, b"k3").await.unwrap(), b"tiny");

  let entry_li = store.scan(&tbl, b"", b"z", 0).await.unwrap();
  let map: HashMap<_, _> = entry_li.into_iter().collect();
  assert_eq!(map.get(b"k1".as_slice()), Some(&a));
  assert_eq!(map.get(b"k2".as_slice()), Some(&b));
  assert_eq!(map.get(b"k3".as_slice()), Some(&b"tiny".to_vec()));

  // floor 也走解压路径。
  assert_eq!(store.floor(&tbl, b"k2").await.unwrap().value, b);
}

#[compio::test]
async fn compio_store_compression_survives_reopen() {
  let root = TempRoot::new("store_compress_reopen");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let value = compressible(b'z');
  {
    let io = CompioStore::new(
      root.path().to_path_buf(),
      4096,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 4096, 16).with_compression_level(3);
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"key".to_vec(), value.clone(), 7)],
      )
      .await
      .unwrap();
    assert_eq!(store.read(&tbl, b"key").await.unwrap(), value);
  }
  {
    let io = CompioStore::new(
      root.path().to_path_buf(),
      4096,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 4096, 16).with_compression_level(3);
    store.reopen(&tbl).await.unwrap();
    assert_eq!(store.read(&tbl, b"key").await.unwrap(), value);
  }
}

#[compio::test]
async fn compression_off_stores_raw_and_still_reads() {
  let root = TempRoot::new("store_compress_off");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let io = CompioStore::new(
    root.path().to_path_buf(),
    4096,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  let mut store = Store::new(io, 4096, 16);
  let value = compressible(b'q');
  store
    .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), value.clone(), 1)])
    .await
    .unwrap();
  assert_eq!(store.read(&tbl, b"k").await.unwrap(), value);
}

/// 字典压缩端到端：写入按字典压缩，manifest 持久化字典，reopen 自动恢复可读。
#[compio::test]
async fn dictionary_compression_end_to_end_reopen() {
  let root = TempRoot::new("store_compress_dict");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let dict = vec![b'd'; 1024];
  let value = vec![b'd'; 600];
  {
    let io = CompioStore::new(
      root.path().to_path_buf(),
      4096,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 4096, 16).with_compression_level(3);
    store.set_dictionary(&tbl, dict.clone()).unwrap();
    store
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), value.clone(), 1)])
      .await
      .unwrap();
    assert_eq!(store.read(&tbl, b"k").await.unwrap(), value);
  }
  {
    // 重开时不显式设置字典，靠 manifest 恢复。
    let io = CompioStore::new(
      root.path().to_path_buf(),
      4096,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 4096, 16);
    store.reopen(&tbl).await.unwrap();
    assert!(store.has_dictionary(&tbl));
    assert_eq!(store.read(&tbl, b"k").await.unwrap(), value);
  }
}

/// sem-67 声明 1 主缺陷：**manifest 已建**的表注入字典必须置脏持久化。
/// 与上面的端到端用例互补：那里 `set_dictionary` 发生在首批写之前，首个
/// manifest 本来就是全量 snapshot（`first_manifest`），dict_bytes 无条件包含，
/// 恰好绕过缺陷窗口；本用例先 put 建首 manifest（无字典），此后注入的字典
/// 只能靠 `dict_dirty` 触发的全量 snapshot 进 manifest——去掉置脏即红
/// （reopen 后 `has_dictionary == false`、Dictionary 压缩页 read `Corrupted`）。
#[compio::test]
async fn dictionary_injected_after_first_manifest_survives_reopen() {
  let root = TempRoot::new("store_compress_dict_after_manifest");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let dict = vec![b'd'; 1024];
  let value = vec![b'd'; 600];
  {
    let io = CompioStore::new(
      root.path().to_path_buf(),
      4096,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 4096, 16).with_compression_level(3);
    // 先写后注入：首个 manifest 落盘时无字典，此后的 flush 不再走
    // first_manifest 分支。
    let first = compressible(b'f');
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"first".to_vec(), first.clone(), 1)],
      )
      .await
      .unwrap();
    assert!(!store.has_dictionary(&tbl));

    store.set_dictionary(&tbl, dict).unwrap();
    store
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), value.clone(), 2)])
      .await
      .unwrap();
    assert_eq!(store.read(&tbl, b"k").await.unwrap(), value);
  }
  {
    // 不显式设置字典，靠 manifest 恢复：注入必须已随 snapshot 持久化。
    let io = CompioStore::new(
      root.path().to_path_buf(),
      4096,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 4096, 16);
    store.reopen(&tbl).await.unwrap();
    assert!(
      store.has_dictionary(&tbl),
      "注入的字典必须随 manifest 持久化"
    );
    assert_eq!(store.read(&tbl, b"k").await.unwrap(), value);
    assert_eq!(
      store.read(&tbl, b"first").await.unwrap(),
      compressible(b'f')
    );
  }
}

/// sem-67 声明 2：已有字典的表再 `set_dictionary` 必须拒绝（对齐 C++ 封闭
/// 生命周期）——zstd 帧不内嵌字典、不校验字典身份，静默替换会让旧
/// Dictionary 压缩页按新字典解出**错值**（无任何错误信号）。去掉守卫即红
/// （第二次注入返回 `Ok`、旧值被换字典解码）。
#[compio::test]
async fn set_dictionary_rejects_replacing_live_dictionary() {
  let root = TempRoot::new("store_compress_dict_no_replace");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let value = vec![b'd'; 600];
  {
    let io = CompioStore::new(
      root.path().to_path_buf(),
      4096,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 4096, 16).with_compression_level(3);
    store.set_dictionary(&tbl, vec![b'd'; 1024]).unwrap();
    store
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), value.clone(), 1)])
      .await
      .unwrap();

    let err = store.set_dictionary(&tbl, vec![b'x'; 1024]);
    assert_eq!(err, Err(KvError::InvalidArgs), "已有字典时必须拒绝替换");
    assert!(store.has_dictionary(&tbl));
    // 拒绝后原字典仍完整服务读路径。
    assert_eq!(store.read(&tbl, b"k").await.unwrap(), value);
  }
  {
    // reopen 后仍是注入时的原字典语义，旧值可读。
    let io = CompioStore::new(
      root.path().to_path_buf(),
      4096,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 4096, 16);
    store.reopen(&tbl).await.unwrap();
    assert!(store.has_dictionary(&tbl));
    assert_eq!(store.read(&tbl, b"k").await.unwrap(), value);
  }
}

/// scan / floor 必须同样用表字典还原 Dictionary entry。
#[compio::test]
async fn dictionary_compression_scan_and_floor() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(MemStore::new(4096), 4096, 16).with_compression_level(3);
  store.set_dictionary(&tbl, vec![b'd'; 1024]).unwrap();
  let value = vec![b'd'; 600];
  store
    .put(
      &tbl,
      &[
        BatchEntry::upsert(b"k1".to_vec(), value.clone(), 1),
        BatchEntry::upsert(b"k2".to_vec(), value.clone(), 1),
      ],
    )
    .await
    .unwrap();

  let entry_li = store.scan(&tbl, b"", b"z", 0).await.unwrap();
  let map: HashMap<_, _> = entry_li.into_iter().collect();
  assert_eq!(map.get(b"k1".as_slice()), Some(&value));
  assert_eq!(map.get(b"k2".as_slice()), Some(&value));
  assert_eq!(store.floor(&tbl, b"k2").await.unwrap().value, value);
}

/// 主树已有字典压缩 entry 时，大 value 覆盖写的 `capture_old_large` 必须带字典回读。
#[compio::test]
async fn dictionary_entry_survives_large_value_overwrite() {
  let root = TempRoot::new("store_compress_dict_large");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let io = CompioStore::new(
    root.path().to_path_buf(),
    4096,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let mut store = Store::new(io, 4096, 16)
    .with_compression_level(3)
    .with_large_value(layout, 1024);
  store.set_dictionary(&tbl, vec![b'd'; 1024]).unwrap();

  // 600B < 1024：小值走字典压缩落在主树。
  let small = vec![b'd'; 600];
  store
    .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), small, 1)])
    .await
    .unwrap();

  // 同 key 换大值：会先读回旧 entry（字典压缩）再写 segment。
  let big = vec![b'e'; 3000];
  store
    .put_large(&tbl, &[BatchEntry::upsert(b"k".to_vec(), big.clone(), 2)])
    .await
    .unwrap();
  assert_eq!(
    store.get_large(&tbl, b"k").await.unwrap().unwrap().value,
    big
  );
}

/// `clean_expired` 要读回主树 entry 比对 expire_ts，字典压缩 entry 不能报错。
#[compio::test]
async fn clean_expired_reads_dictionary_compressed_entry() {
  let root = TempRoot::new("store_compress_dict_ttl");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let io = CompioStore::new(
    root.path().to_path_buf(),
    4096,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  let mut store = Store::new(io, 4096, 16).with_compression_level(3);
  store.set_dictionary(&tbl, vec![b'd'; 1024]).unwrap();

  let value = vec![b'd'; 600];
  let mut entry = BatchEntry::upsert(b"k".to_vec(), value.clone(), 1);
  entry.expire_ts = 1_000;
  store.put_at(&tbl, &[entry], 0).await.unwrap();
  assert_eq!(store.read_at(&tbl, b"k", 0).await.unwrap(), value);

  let stats = store.clean_expired(&tbl, 1_000).await.unwrap();
  assert_eq!(stats.deleted, 1);
  assert!(store.get_at(&tbl, b"k", 1_000).await.unwrap().is_none());
}

/// issue 08：只设置压缩级别、不手工塞字典，写入足够样本后应自动训练字典，
/// 且字典随 snapshot 持久化，reopen 后自动恢复（否则 Dictionary 页不可解压）。
#[compio::test]
async fn auto_dictionary_training_persists_and_reopens() {
  let root = TempRoot::new("store_compress_auto_dict");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut expected = HashMap::new();
  {
    let io = CompioStore::new(
      root.path().to_path_buf(),
      4096,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 4096, 16).with_compression_level(3);
    // 1.6 MB 样本（= SAMPLE_TARGET_BYTES），足以触发训练。
    for chunk in 0..40u32 {
      let mut batch = Vec::new();
      for index in 0..40u32 {
        let id = chunk * 40 + index;
        let key = format!("k{id:05}").into_bytes();
        let mut value = vec![(id % 251) as u8; 1024];
        value[..4].copy_from_slice(&id.to_le_bytes());
        expected.insert(key.clone(), value.clone());
        batch.push(BatchEntry::upsert(key, value, 1));
      }
      store.put(&tbl, &batch).await.unwrap();
    }
    assert!(store.has_dictionary(&tbl), "样本足够后应自动训练字典");
  }
  {
    // 不显式设置字典，靠 manifest snapshot 恢复。
    let io = CompioStore::new(
      root.path().to_path_buf(),
      4096,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut store = Store::new(io, 4096, 16);
    store.reopen(&tbl).await.unwrap();
    assert!(store.has_dictionary(&tbl), "reopen 应恢复字典");
    for (key, value) in &expected {
      assert_eq!(&store.read(&tbl, key).await.unwrap(), value);
    }
  }
}

/// 切到无字典的 branch 时必须清空旧字典：否则分支页会按未持久化字典压缩，
/// 重开后无字典可解。
#[test]
fn open_branch_clears_stale_dictionary() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut main = Store::new(MemStore::new(512), 512, 16);
    main
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    main.create_branch(&tbl, "feature").await.unwrap();
    let io = main.into_io();

    let mut branch = Store::new(io, 512, 16).with_compression_level(3);
    branch.set_dictionary(&tbl, vec![b'd'; 1024]).unwrap();
    assert!(branch.has_dictionary(&tbl));
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    assert!(
      !branch.has_dictionary(&tbl),
      "branch 无字典时不得残留旧字典"
    );

    // 分支新写入的页不带字典，重开必须可读。
    let value = compressible(b'z');
    branch
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), value.clone(), 2)])
      .await
      .unwrap();
    let io = branch.into_io();
    let mut reopened = Store::new(io, 512, 16);
    reopened.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(reopened.read(&tbl, b"k").await.unwrap(), value);
  });
}

/// 按给定压缩开关在同一目录重建 `Store`；`None` 等价于不调用
/// `with_compression_level`（关闭压缩）。
fn open_compio_store(root: &TempRoot, compression_level: Option<i32>) -> Store<CompioStore> {
  let io = CompioStore::new(
    root.path().to_path_buf(),
    4096,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  let store = Store::new(io, 4096, 16);
  match compression_level {
    Some(level) => store.with_compression_level(level),
    None => store,
  }
}

/// 四重校验：逐 key 点读、全量 `scan`、`scan_entry_page`（含 `expire_ts`）、
/// `floor`；全部走同一套页级解码路径。
async fn assert_all_entries_readable(
  store: &Store<CompioStore>,
  tbl: &TableIdent,
  expected: &HashMap<Vec<u8>, (Vec<u8>, u64)>,
) {
  for (key, (value, _)) in expected {
    assert_eq!(
      &store.read(tbl, key).await.unwrap(),
      value,
      "点读不一致: {}",
      String::from_utf8_lossy(key)
    );
  }

  let scanned: HashMap<Vec<u8>, Vec<u8>> = store
    .scan(tbl, b"", b"", 0)
    .await
    .unwrap()
    .into_iter()
    .collect();
  assert_eq!(scanned.len(), expected.len(), "scan 返回条数不一致");
  for (key, (value, _)) in expected {
    assert_eq!(
      scanned.get(key),
      Some(value),
      "scan 值不一致: {}",
      String::from_utf8_lossy(key)
    );
  }

  let page = store
    .scan_entry_page(tbl, b"", b"", ScanOption::default(), PageOption::default())
    .await
    .unwrap();
  assert_eq!(page.entry_li.len(), expected.len(), "分页 scan 条数不一致");
  for entry in &page.entry_li {
    let key = String::from_utf8_lossy(&entry.key).into_owned();
    let (value, expire_ts) = expected
      .get(&entry.key)
      .unwrap_or_else(|| panic!("分页 scan 返回期望外的 key: {key}"));
    assert_eq!(&entry.value, value, "分页 scan 值不一致: {key}");
    assert_eq!(entry.expire_ts, *expire_ts, "expire_ts 不一致: {key}");
  }

  // floor 同样走页解码路径：最大 key 必须精确命中。
  let (max_key, (max_value, max_expire)) = expected
    .iter()
    .max_by(|left, right| left.0.cmp(right.0))
    .expect("期望集不应为空");
  let floor = store.floor(tbl, max_key).await.unwrap();
  assert_eq!(&floor.key, max_key, "floor 命中 key 不一致");
  assert_eq!(&floor.value, max_value, "floor 值不一致");
  assert_eq!(floor.expire_ts, *max_expire, "floor expire_ts 不一致");
}

/// 扫描表目录下所有 data 页，汇总 `key → 页内 compression flag`。
///
/// store 的公共读 API 只返回解压后的值，无法断言写路径落盘形态；这里按
/// `fkv_page` 的页格式直接迭代盘面（data 页定长 `page_size`，index /
/// overflow 页跳过）。本用例中目标 key 各只写入一次，盘面无同 key 旧版本。
fn data_page_compression(
  root: &TempRoot,
  tbl: &TableIdent,
  page_size: usize,
) -> HashMap<Vec<u8>, CompressionType> {
  let dir = root.join(tbl.dir_name());
  let mut compression_map = HashMap::new();
  for entry in read_dir(dir).unwrap() {
    let path = entry.unwrap().path();
    let is_data_file = path
      .file_name()
      .is_some_and(|name| name.to_string_lossy().starts_with(DATA_FILE_PREFIX));
    if !is_data_file {
      continue;
    }
    let bytes = read(&path).unwrap();
    for page in bytes.chunks_exact(page_size) {
      if page_type(page).unwrap() != PageType::Data {
        continue;
      }
      let mut iter = DataPageIter::new(page).unwrap();
      while iter.advance().unwrap() {
        compression_map.insert(iter.key().to_vec(), iter.compression());
      }
    }
  }
  compression_map
}

/// 压缩开关只影响此后新写入的页：raw 页与字典压缩页共存于同一 store，
/// 跨重启反复翻转设置后旧数据仍全部可读、scan 全量一致。对齐 C++
/// `persist.cpp` 的 "append mode survives compression toggles across restarts"。
///
/// 断言刻意覆盖两类回归：
/// - 解码按「当前进程配置」而非页级 compression flag 判定：阶段 2 在装载字典
///   后读阶段 1 的 raw 页，阶段 3 在关闭压缩时读阶段 2 的字典压缩页；
/// - 字典 restore 与翻转后新写页的交互：阶段 3 关闭压缩重启仍须恢复旧字典，
///   但新写入不得复用该字典（对齐 C++ `enable_compression` 门控，落 raw 页）；
///   阶段 4 再以 level 3 重启后所有写代都必须可解。
#[compio::test]
async fn compression_toggle_across_reopen_keeps_all_data_readable() {
  let root = TempRoot::new("store_compress_toggle");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let ttl_key = b"p3:ttl:00".to_vec();
  let ttl_value = compressible(b'T');
  let ttl_expire = now_ms() + 24 * 60 * 60 * 1_000;
  let mut expected: HashMap<Vec<u8>, (Vec<u8>, u64)> = HashMap::new();

  // 阶段 1：关闭压缩。500B 可压缩值此时也落 raw 页（无字典不走压缩），
  // 短值恒 raw；另写一条 TTL entry 验证 TTL 索引跨重启。
  {
    let mut store = open_compio_store(&root, None);
    let mut batch = Vec::new();
    for i in 0..12u8 {
      let key = format!("p1:big:{i:02}").into_bytes();
      let value = compressible(b'a' + i);
      expected.insert(key.clone(), (value.clone(), 0));
      batch.push(BatchEntry::upsert(key, value, 1));
    }
    for i in 0..12u32 {
      let key = format!("p1:tiny:{i:02}").into_bytes();
      let value = format!("tiny-value-{i:02}").into_bytes();
      expected.insert(key.clone(), (value.clone(), 0));
      batch.push(BatchEntry::upsert(key, value, 1));
    }
    let mut ttl_entry = BatchEntry::upsert(ttl_key.clone(), ttl_value.clone(), 1);
    ttl_entry.expire_ts = ttl_expire;
    expected.insert(ttl_key.clone(), (ttl_value.clone(), ttl_expire));
    batch.push(ttl_entry);
    store.put_at(&tbl, &batch, 0).await.unwrap();

    assert!(!store.has_dictionary(&tbl), "关闭压缩时不应建立字典");
    assert_all_entries_readable(&store, &tbl, &expected).await;
  }

  // 阶段 2：以 level 3 重启。先读阶段 1 的 raw 页（此时仍无字典），再写足
  // 1.6MB 样本触发自动训练，随后的 500B 批落字典压缩页。
  {
    let mut store = open_compio_store(&root, Some(3));
    store.reopen(&tbl).await.unwrap();
    assert_all_entries_readable(&store, &tbl, &expected).await;

    for chunk in 0..40u32 {
      let mut batch = Vec::with_capacity(40);
      for index in 0..40u32 {
        let id = chunk * 40 + index;
        let key = format!("p2:train:{id:05}").into_bytes();
        let mut value = vec![(id % 251) as u8; 1024];
        value[..4].copy_from_slice(&id.to_le_bytes());
        expected.insert(key.clone(), (value.clone(), 0));
        batch.push(BatchEntry::upsert(key, value, 2));
      }
      store.put(&tbl, &batch).await.unwrap();
    }
    assert!(store.has_dictionary(&tbl), "样本达到目标后应自动训练字典");

    let mut batch = Vec::new();
    for i in 0..16u8 {
      let key = format!("p2:big:{i:02}").into_bytes();
      let value = compressible(b'A' + i);
      expected.insert(key.clone(), (value.clone(), 0));
      batch.push(BatchEntry::upsert(key, value, 3));
    }
    store.put(&tbl, &batch).await.unwrap();

    // 同一 store 内 raw 页（阶段 1）与压缩页（阶段 2）共存，全部可读。
    assert_all_entries_readable(&store, &tbl, &expected).await;
  }

  // 阶段 3：关闭压缩重启。旧字典压缩页必须靠 manifest 恢复的字典 + 页级 flag
  // 解码；即使字典已恢复，新写入也不得再走字典压缩（对齐 C++ `enable_compression`
  // 关闭时不 `Prepare`），应落 raw 页。
  {
    let mut store = open_compio_store(&root, None);
    store.reopen(&tbl).await.unwrap();
    assert!(
      store.has_dictionary(&tbl),
      "关闭压缩重启仍须恢复旧盘面字典，否则旧压缩页不可解"
    );
    assert_all_entries_readable(&store, &tbl, &expected).await;

    let mut batch = Vec::new();
    for i in 0..8u8 {
      let key = format!("p3:big:{i:02}").into_bytes();
      let value = compressible(b'0' + i);
      expected.insert(key.clone(), (value.clone(), 0));
      batch.push(BatchEntry::upsert(key, value, 4));
    }
    store.put(&tbl, &batch).await.unwrap();
    assert_all_entries_readable(&store, &tbl, &expected).await;
  }

  // 盘面断言写入门控：关闭压缩阶段新写的 500B 值必须是 raw；阶段 2 的字典
  // 压缩 entry 仍保留原 flag（旧页可读，不被按当前配置重编码）。
  let compression_map = data_page_compression(&root, &tbl, 4096);
  for i in 0..8u8 {
    let key = format!("p3:big:{i:02}").into_bytes();
    assert_eq!(
      compression_map.get(&key),
      Some(&CompressionType::None),
      "关闭压缩阶段的新写入必须落 raw 页: {}",
      String::from_utf8_lossy(&key)
    );
  }
  for i in 0..16u8 {
    let key = format!("p2:big:{i:02}").into_bytes();
    assert_eq!(
      compression_map.get(&key),
      Some(&CompressionType::Dictionary),
      "旧字典压缩 entry 应保留 Dictionary flag: {}",
      String::from_utf8_lossy(&key)
    );
  }

  // 阶段 4：再以 level 3 重启，全量验证所有写代；TTL 索引在多次翻转后
  // 仍可定位并清理过期项。
  {
    let mut store = open_compio_store(&root, Some(3));
    store.reopen(&tbl).await.unwrap();
    assert!(store.has_dictionary(&tbl));
    assert_all_entries_readable(&store, &tbl, &expected).await;

    let stats = store.clean_expired(&tbl, ttl_expire).await.unwrap();
    assert_eq!(stats.deleted, 1, "TTL 索引跨压缩开关翻转后仍应可清理");
    expected.remove(&ttl_key);
    assert_all_entries_readable(&store, &tbl, &expected).await;
  }
}
