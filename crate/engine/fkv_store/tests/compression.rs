use std::collections::HashMap;

use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

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
