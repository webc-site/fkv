use std::slice::from_ref;

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io::{FileIo, FixedBuffer, FixedSegmentIo};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

#[test]
fn memstore_large_value_put_get_cow_and_reopen() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let big = pattern(9000);
    store
      .put_large(
        &tbl,
        &[
          BatchEntry::upsert(b"big".to_vec(), big.clone(), 1).with_metadata(b"block-descriptor"),
          BatchEntry::upsert(b"small".to_vec(), b"S".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    let read = store.get_large(&tbl, b"big").await.unwrap().unwrap();
    assert_eq!(read.value, big);
    assert_eq!(read.metadata, b"block-descriptor");
    assert_eq!(
      store
        .get_large(&tbl, b"small")
        .await
        .unwrap()
        .unwrap()
        .value,
      b"S"
    );
    assert_eq!(store.segment_mapper(&tbl).unwrap().tbl().len(), 3);

    // 小 key 写入触发 data page COW，大 value 的 0b11 flag 必须保留。
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"other".to_vec(), b"O".to_vec(), 2)],
      )
      .await
      .unwrap();
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      big
    );

    // 覆盖为另一个大 value。
    let big2 = pattern(5000);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), big2.clone(), 3)],
      )
      .await
      .unwrap();
    let read = store.get_large(&tbl, b"big").await.unwrap().unwrap();
    assert_eq!(read.value, big2);
    assert!(read.metadata.is_empty());
    assert_eq!(
      store
        .get_large(&tbl, b"small")
        .await
        .unwrap()
        .unwrap()
        .value,
      b"S"
    );

    let io = store.into_io();
    let mut reopened = Store::new(io, 512, 16).with_large_value(layout, 1024);
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(
      reopened
        .get_large(&tbl, b"big")
        .await
        .unwrap()
        .unwrap()
        .value,
      big2
    );
    // core-02：新 value 的 segment 先分配、旧 value 的 3 个 segment 在同批随后释放，
    // 所以活跃 segment 为 2、其余 3 个槽位全部回空闲链表（可被后续批复用）。
    let segment_mapper = reopened.segment_mapper(&tbl).unwrap();
    let live_num = segment_mapper
      .tbl()
      .iter_value_li()
      .filter(|value| fkv_types::is_file_page_id(*value))
      .count();
    assert_eq!(live_num, 2, "old large value segments must be released");
    assert_eq!(segment_mapper.tbl().len(), 5);
    assert_eq!(
      segment_mapper.free_page_cnt() as usize,
      segment_mapper.tbl().len() - live_num,
      "every non-live segment slot must be reusable"
    );
  });
}

#[test]
fn segment_mapping_replays_from_manifest_log() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    // 第一次写建 snapshot。
    store
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    // 第二次写走 manifest log，segment mapping 以 delta 形式追加。
    let big = pattern(4200);
    store
      .put_large(
        &tbl,
        &[
          BatchEntry::upsert(b"big".to_vec(), big.clone(), 2),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2),
        ],
      )
      .await
      .unwrap();
    let mapped = store.segment_mapper(&tbl).unwrap().tbl().len();
    assert_eq!(mapped, 2);

    let io = store.into_io();
    let mut reopened = Store::new(io, 512, 16).with_large_value(layout, 1024);
    reopened.reopen(&tbl).await.unwrap();
    assert_eq!(
      reopened
        .get_large(&tbl, b"big")
        .await
        .unwrap()
        .unwrap()
        .value,
      big
    );
    assert_eq!(reopened.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(reopened.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
    assert_eq!(reopened.segment_mapper(&tbl).unwrap().tbl().len(), mapped);
  });
}

/// core-02：反复覆盖大 value 时，segment 逻辑页表必须有界（释放的逻辑页被复用）。
#[test]
fn repeated_put_large_keeps_segment_tbl_bounded() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let mut len_li = Vec::new();
    for ts in 1..=12 {
      store
        .put_large(
          &tbl,
          &[BatchEntry::upsert(b"big".to_vec(), pattern(9000), ts)],
        )
        .await
        .unwrap();
      assert_eq!(
        store
          .get_large(&tbl, b"big")
          .await
          .unwrap()
          .unwrap()
          .value
          .len(),
        9000
      );
      len_li.push(store.segment_mapper(&tbl).unwrap().tbl().len());
    }
    // 每轮 3 个 segment：同批先分配后释放，长度稳定在 6 以内；不复用则单调到 36。
    assert!(
      len_li.iter().all(|len| *len <= 6),
      "segment tbl must stay bounded, got {len_li:?}"
    );
    assert!(store.segment_mapper(&tbl).unwrap().free_page_cnt() > 0);
  });
}

#[compio::test]
async fn compio_large_value_segment_file_reopen_and_truncate() {
  let root = TempRoot::new("store_large_value_compio");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  let big = pattern(9000);
  store
    .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 1)])
    .await
    .unwrap();
  assert!(root.join("demo.1").join("segment_0_main_0").exists());
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big
  );
  drop(store);

  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  store.reopen(&tbl).await.unwrap();
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big
  );

  store.truncate(&tbl).await.unwrap();
  assert_eq!(store.get_large(&tbl, b"big").await.unwrap(), None);
  assert!(
    store
      .segment_mapper(&tbl)
      .unwrap()
      .tbl()
      .iter_value_li()
      .all(|value| !fkv_types::is_file_page_id(value))
  );

  let big2 = pattern(3000);
  store
    .put_large(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), big2.clone(), 2)],
    )
    .await
    .unwrap();
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big2
  );
}

#[test]
fn put_large_requires_layout() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    assert_eq!(
      store
        .put_large(
          &tbl,
          &[BatchEntry::upsert(b"a".to_vec(), vec![1u8; 4096], 1)]
        )
        .await
        .unwrap_err(),
      KvError::InvalidArgs
    );
  });
}

#[compio::test]
async fn segment_gc_reclaims_overwritten_and_deleted_values() {
  let root = TempRoot::new("store_large_value_segment_gc");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 1).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  let big = pattern(9000);
  store
    .put_large(&tbl, &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 1)])
    .await
    .unwrap();
  assert_eq!(
    store.page_io().list_segment_file_id_li(&tbl).await.unwrap(),
    vec![0, 1]
  );

  // 9000B / 4096 = 3 segments；覆盖为 3000B 只占 1 个新 segment（file 1）。
  let big2 = pattern(3000);
  store
    .put_large(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), big2.clone(), 2)],
    )
    .await
    .unwrap();
  let removed = store
    .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .await
    .unwrap();
  assert!(removed >= 1);
  assert_eq!(
    store.page_io().list_segment_file_id_li(&tbl).await.unwrap(),
    vec![1]
  );
  assert!(!root.join("demo.1").join("segment_0_main_0").exists());
  assert!(root.join("demo.1").join("segment_1_main_0").exists());
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big2
  );

  drop(store);
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1),
    512,
    16,
  )
  .with_large_value(layout, 1024);
  store.reopen(&tbl).await.unwrap();
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    big2
  );
  assert_eq!(
    store.page_io().list_segment_file_id_li(&tbl).await.unwrap(),
    vec![1]
  );

  // 删除旧大 value 后，最后一个 segment 文件也应被 GC 回收。
  store
    .put_large(&tbl, &[BatchEntry::delete(b"big".to_vec(), 3)])
    .await
    .unwrap();
  assert_eq!(store.get_large(&tbl, b"big").await.unwrap(), None);
  store
    .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
    .await
    .unwrap();
  assert!(
    store
      .page_io()
      .list_segment_file_id_li(&tbl)
      .await
      .unwrap()
      .is_empty()
  );
  assert!(!root.join("demo.1").join("segment_1_main_0").exists());
}

#[compio::test]
async fn store_fixed_buffer_put_and_get() {
  let root = TempRoot::new("store_large_value_fixed");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let io = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 2);
  // 注册缓冲依赖内核 / memlock 限额；runner 不提供时跳过这条能力用例。
  let Ok(mut buf) = io.register_fixed_buffer(64 * 1024) else {
    eprintln!("skip: register_fixed_buffer unavailable");
    return;
  };
  assert_eq!(buf.is_registered(), cfg!(target_os = "linux"));
  let mut store = Store::new(io, 512, 16).with_large_value(layout, 1024);
  let value = pattern(9000);
  let mut entry = BatchEntry::upsert(b"big".to_vec(), value.clone(), 1);
  entry.metadata = b"meta".to_vec();
  store
    .put_large_fixed(&tbl, &[entry], &mut buf)
    .await
    .unwrap();

  let into = store
    .get_large_into(&tbl, b"big", &mut buf)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(into.value_len, value.len());
  assert_eq!(&buf.as_slice()[..into.value_len], value);
  assert_eq!(into.metadata, b"meta");
  assert_eq!(
    store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
    value
  );
}

/// issue 15：`get` / `read` 不能把 large value 的内部编码（segment 指针+metadata）
/// 当作数据返回；必须让调用方走 `get_large*`。
#[test]
fn get_and_read_reject_large_value() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let big = pattern(9000);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), big.clone(), 1).with_metadata(b"meta")],
      )
      .await
      .unwrap();
    assert_eq!(
      store.get(&tbl, b"big").await.unwrap_err(),
      KvError::LargeValueUnsupported
    );
    assert_eq!(
      store.read(&tbl, b"big").await.unwrap_err(),
      KvError::LargeValueUnsupported
    );
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      big
    );

    // 同表的普通 value 不受影响。
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"small".to_vec(), b"S".to_vec(), 2)],
      )
      .await
      .unwrap();
    assert_eq!(
      store.get(&tbl, b"small").await.unwrap(),
      Some(b"S".to_vec())
    );
  });
}

fn live_segment_num(store: &Store<MemStore>) -> usize {
  store
    .segment_mapper(&TableIdent::new("demo", 1).unwrap())
    .map_or(0, |mapper| {
      mapper
        .tbl()
        .iter_value_li()
        .filter(|value| fkv_types::is_file_page_id(*value))
        .count()
    })
}

/// eng-51：delete / 小值覆盖大 value 时必须释放 segment 映射，不能永久泄漏。
#[test]
fn delete_and_overwrite_free_large_value_segments() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), pattern(9000), 1)],
      )
      .await
      .unwrap();
    assert_eq!(live_segment_num(&store), 3);

    // 普通小值覆盖大 value。
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"big".to_vec(), b"tiny".to_vec(), 2)],
      )
      .await
      .unwrap();
    assert_eq!(live_segment_num(&store), 0, "覆盖后旧段必须释放");
    assert_eq!(
      store.get(&tbl, b"big").await.unwrap(),
      Some(b"tiny".to_vec())
    );

    // 重新写大 value 后 delete。
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big2".to_vec(), pattern(9000), 3)],
      )
      .await
      .unwrap();
    assert_eq!(live_segment_num(&store), 3);
    store.delete(&tbl, b"big2", 4).await.unwrap();
    assert_eq!(live_segment_num(&store), 0, "delete 后段必须释放");

    // 覆盖成新的大 value：旧段释放、新段保留。
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"big2".to_vec(), pattern(5000), 5)],
      )
      .await
      .unwrap();
    assert_eq!(live_segment_num(&store), 2);
  });
}

/// eng-75：plain / fixed 两条写路径与 owned / fixed 两条读路径逐字节等价。
#[test]
fn large_value_paths_are_byte_equivalent() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut plain = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let mut fixed = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let mut buf = FixedBuffer::new(64 * 1024);

    // 阈值以下、恰在阈值、段边界两侧、多段。
    for (seq, len) in [1usize, 1024, 4096, 4097, 9000, 16384]
      .into_iter()
      .enumerate()
    {
      let key = format!("k{len}").into_bytes();
      let value = pattern(len);
      let mut entry = BatchEntry::upsert(key.clone(), value.clone(), seq as u64);
      if len >= 1024 {
        entry.metadata = format!("m{len}").into_bytes();
      }
      plain.put_large(&tbl, from_ref(&entry)).await.unwrap();
      fixed
        .put_large_fixed(&tbl, from_ref(&entry), &mut buf)
        .await
        .unwrap();

      let owned = plain.get_large(&tbl, &key).await.unwrap().unwrap();
      assert_eq!(owned, fixed.get_large(&tbl, &key).await.unwrap().unwrap());
      assert_eq!(owned.value, value, "len={len}");
      assert_eq!(owned.metadata, entry.metadata, "len={len}");

      for store in [&plain, &fixed] {
        let into = store
          .get_large_into(&tbl, &key, &mut buf)
          .await
          .unwrap()
          .unwrap();
        assert_eq!(into.value_len, value.len(), "len={len}");
        assert_eq!(
          &buf.as_slice()[..into.value_len],
          value.as_slice(),
          "len={len}"
        );
        assert_eq!(into.metadata, owned.metadata, "len={len}");
      }

      let floor = plain.floor(&tbl, &key).await.unwrap();
      assert_eq!(floor.key, key);
      assert_eq!(floor.value, value);
      assert_eq!(floor.metadata, owned.metadata);
    }
  });
}

/// eng-75：fixed buffer 装不下 value 时与 plain 路径同样报 `InvalidArgs`，
/// 且整批回滚（不落 segment、不写同批其他 entry）。
#[test]
fn put_large_fixed_rejects_value_larger_than_buffer() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let mut buf = FixedBuffer::new(1024);
    let batch = [
      BatchEntry::upsert(b"small".to_vec(), b"S".to_vec(), 1),
      BatchEntry::upsert(b"big".to_vec(), pattern(4096), 2),
    ];
    assert_eq!(
      store
        .put_large_fixed(&tbl, &batch, &mut buf)
        .await
        .unwrap_err(),
      KvError::InvalidArgs
    );
    assert_eq!(store.get_large(&tbl, b"big").await.unwrap(), None);
    assert_eq!(store.get(&tbl, b"small").await.unwrap(), None);
    assert_eq!(live_segment_num(&store), 0);
  });
}

/// perf-85：`truncate_from` 不物化 value，但删除仍须释放大 value segment。
#[test]
fn truncate_from_frees_large_value_segments() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    store
      .put_large(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"tiny".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), pattern(9000), 1),
          BatchEntry::upsert(b"c".to_vec(), pattern(9000), 1),
        ],
      )
      .await
      .unwrap();
    assert_eq!(live_segment_num(&store), 6);

    assert_eq!(store.truncate_from(&tbl, b"b").await.unwrap(), 2);
    assert_eq!(live_segment_num(&store), 0, "截断必须释放旧 segment");
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"tiny".to_vec()));
    assert_eq!(store.get(&tbl, b"b").await.unwrap(), None);
    assert_eq!(store.floor(&tbl, b"z").await.unwrap().key, b"a");
  });
}

/// R4-05：同批同 key 多版本必须先按 timestamp 折叠，只为胜者写 segment。
///
/// 先逐条落段再交给 `put` 折叠时，败者的段链既无树引用也不会释放，
/// 映射与空间会一直泄漏到下一次段压缩。
#[test]
fn put_large_folds_duplicate_keys_before_writing_segments() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let new = pattern(9000);
    store
      .put_large(
        &tbl,
        &[
          BatchEntry::upsert(b"big".to_vec(), pattern(20_000), 1),
          BatchEntry::upsert(b"big".to_vec(), new.clone(), 2),
        ],
      )
      .await
      .unwrap();
    // 胜者 9000B 占 3 个 segment；败者 20000B（5 个）不得落盘。
    assert_eq!(live_segment_num(&store), 3, "败者段链泄漏");
    assert_eq!(
      store.get_large(&tbl, b"big").await.unwrap().unwrap().value,
      new
    );
  });
}

/// 同批被过期 upsert 覆盖时不落任何 segment：胜者最终折叠为 delete。
#[test]
fn put_large_skips_segments_for_expired_upsert() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024);
    let mut expired = BatchEntry::upsert(b"big".to_vec(), pattern(20_000), 2);
    expired.expire_ts = 1;
    store
      .put_large(
        &tbl,
        &[
          BatchEntry::upsert(b"big".to_vec(), pattern(9000), 1),
          expired,
        ],
      )
      .await
      .unwrap();
    assert_eq!(live_segment_num(&store), 0, "过期 upsert 不得落段");
    assert_eq!(store.get(&tbl, b"big").await.unwrap(), None);
  });
}
