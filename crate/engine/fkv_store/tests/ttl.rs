use fkv_btree::BatchEntry;
use fkv_codec::{get_varint32, get_varint64};
use fkv_error::KvError;
use fkv_io::{ManifestIo, PageIo};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_manifest::payload;
use fkv_store::{CleanExpiredStat, ScanOption, Store};
use fkv_task_write::{is_expired, parse_ttl_key, prepare_write_batch, ttl_key};
use fkv_testkit::TempRoot;
use fkv_types::{INVALID_VALUE, PageId, TableIdent, WriteOp, now_ms};

fn upsert_with_expire(key: &[u8], value: &[u8], ts: u64, expire_ts: u64) -> BatchEntry {
  let mut entry = BatchEntry::upsert(key.to_vec(), value.to_vec(), ts);
  entry.expire_ts = expire_ts;
  entry
}

/// 在 now=0 写入，避免测试里的历史时间戳被写时过滤掉。
async fn seed<I: PageIo + ManifestIo>(
  store: &mut Store<I>,
  tbl: &TableIdent,
  batch_li: &[BatchEntry],
) {
  store.put_at(tbl, batch_li, 0).await.unwrap();
}

/// 段 mapper 中仍指向物理段的逻辑页数（释放后槽位变空闲链编码）。
fn live_segment_num(store: &Store<MemStore>, tbl: &TableIdent) -> usize {
  store.segment_mapper(tbl).map_or(0, |mapper| {
    mapper
      .tbl()
      .iter_value_li()
      .filter(|value| fkv_types::is_file_page_id(*value))
      .count()
  })
}

/// manifest log record 的 mapping 段：按操作序解析 `(page_id, value)`。
fn log_mapping_entry_li(record: &[u8]) -> Vec<(u32, u64)> {
  let body = payload(record).expect("log payload");
  let len = u32::from_le_bytes(body[..4].try_into().expect("mapping_len")) as usize;
  let section = body.get(4..4 + len).expect("mapping 段越界");
  let mut pos = 0usize;
  let mut entry_li = Vec::new();
  while pos < section.len() {
    let (page_id, read) = get_varint32(&section[pos..]).expect("page id");
    pos += read;
    let (value, read) = get_varint64(&section[pos..]).expect("mapping value");
    pos += read;
    entry_li.push((page_id, value));
  }
  entry_li
}

#[test]
fn ttl_key_layout_and_batch() {
  let key = ttl_key(0x0102_0304_0506_0708, b"user");
  assert_eq!(&key[..8], &0x0102_0304_0506_0708u64.to_be_bytes());
  assert_eq!(&key[8..], b"user");
  assert_eq!(
    parse_ttl_key(&key).unwrap(),
    (0x0102_0304_0506_0708, &b"user"[..])
  );
  assert_eq!(parse_ttl_key(b"short").unwrap_err(), KvError::Corrupted);
  assert!(ttl_key(1, b"z") < ttl_key(2, b"a"));
  assert!(is_expired(100, 100));
  assert!(!is_expired(101, 100));
  assert!(!is_expired(0, u64::MAX));

  let mut deleted = BatchEntry::delete(b"gone".to_vec(), 1);
  deleted.expire_ts = 50;
  let batch = [
    upsert_with_expire(b"a", b"A", 1, 100),
    upsert_with_expire(b"b", b"B", 1, 50),
    upsert_with_expire(b"a", b"A2", 2, 100),
    BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 1),
    deleted,
  ];
  // sem-55：写前缀只负责同 key 折叠与「过期 upsert → delete」，不再预生成
  // TTL upsert（索引项由主树 merge 按基页 expire 门控生成）。
  let prepared = prepare_write_batch(&batch, 0);
  assert_eq!(prepared.expired_num, 0);
  let live_li = prepared.live_li;
  let key_li: Vec<&[u8]> = live_li.iter().map(|entry| entry.key.as_slice()).collect();
  assert_eq!(key_li, vec![&b"a"[..], &b"b"[..], &b"c"[..], &b"gone"[..]]);
  assert_eq!(live_li[0].expire_ts, 100, "同 key 只保留 ts=2 的胜者");
  assert_eq!(live_li[3].op, WriteOp::Delete);
}

#[test]
fn get_scan_filter_expired_and_clean_expired() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    seed(
      &mut store,
      &tbl,
      &[
        upsert_with_expire(b"a", b"A", 1, 1000),
        BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        upsert_with_expire(b"c", b"C", 1, 500),
      ],
    )
    .await;
    assert_ne!(store.ttl_root_id(&tbl).unwrap(), PageId::MAX);

    assert_eq!(
      store.get_at(&tbl, b"a", 999).await.unwrap(),
      Some(b"A".to_vec())
    );
    assert_eq!(store.get_at(&tbl, b"a", 1000).await.unwrap(), None);
    assert_eq!(store.get_at(&tbl, b"c", 500).await.unwrap(), None);
    assert_eq!(
      store.scan_at(&tbl, b"", b"", 0, 499).await.unwrap(),
      vec![
        (b"a".to_vec(), b"A".to_vec()),
        (b"b".to_vec(), b"B".to_vec()),
        (b"c".to_vec(), b"C".to_vec())
      ]
    );
    assert_eq!(
      store.scan_at(&tbl, b"", b"", 0, 500).await.unwrap(),
      vec![
        (b"a".to_vec(), b"A".to_vec()),
        (b"b".to_vec(), b"B".to_vec())
      ]
    );
    assert_eq!(
      store.scan_at(&tbl, b"", b"", 0, 1000).await.unwrap(),
      vec![(b"b".to_vec(), b"B".to_vec())]
    );

    let stats = store.clean_expired(&tbl, 400).await.unwrap();
    assert_eq!(stats.deleted, 0);
    assert_eq!(stats.ttl_deleted, 0);
    assert_eq!(stats.next_expire_ts, 500);

    let stats = store.clean_expired(&tbl, 500).await.unwrap();
    assert_eq!(stats.deleted, 1);
    assert_eq!(stats.ttl_deleted, 1);
    assert_eq!(stats.next_expire_ts, 1000);
    assert_eq!(
      store.scan_at(&tbl, b"", b"", 0, 500).await.unwrap(),
      vec![
        (b"a".to_vec(), b"A".to_vec()),
        (b"b".to_vec(), b"B".to_vec())
      ]
    );

    let stats = store.clean_expired(&tbl, 1000).await.unwrap();
    assert_eq!(stats.deleted, 1);
    assert_eq!(stats.ttl_deleted, 1);
    assert_eq!(stats.next_expire_ts, 0);
    assert_eq!(store.get_at(&tbl, b"a", 0).await.unwrap(), None);
    assert_eq!(
      store.scan_at(&tbl, b"", b"", 0, 0).await.unwrap(),
      vec![(b"b".to_vec(), b"B".to_vec())]
    );

    let stats = store.clean_expired(&tbl, 2000).await.unwrap();
    assert_eq!(stats, CleanExpiredStat::default());
  });
}

#[test]
fn expired_overflow_point_read_skips_overflow_chain() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 11).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    // 4096 字节 value 必然走 overflow 链；小 value 只占 data page 内联区。
    let big = vec![b'v'; 4096];
    seed(
      &mut store,
      &tbl,
      &[
        upsert_with_expire(b"live", b"x", 1, 0),
        upsert_with_expire(b"big", &big, 1, 100),
      ],
    )
    .await;

    // 预热一次点读，把 index page 放进缓存，后续只比较 data page 读取次数。
    assert_eq!(
      store.get_at(&tbl, b"live", 0).await.unwrap(),
      Some(b"x".to_vec())
    );

    let before = store.page_io().read_num();
    assert_eq!(store.get_at(&tbl, b"big", 100).await.unwrap(), None);
    let expired_num = store.page_io().read_num() - before;

    let before = store.page_io().read_num();
    assert_eq!(
      store.get_at(&tbl, b"live", 0).await.unwrap(),
      Some(b"x".to_vec())
    );
    let live_num = store.page_io().read_num() - before;

    // 过期项在 value 物化前返回：读页次数与纯内联小 value 相同，
    // 不会沿 overflow 链读页。
    assert_eq!(expired_num, live_num);

    // 未过期时仍能拼回整条 overflow 链。
    assert_eq!(
      store.get_at(&tbl, b"big", 99).await.unwrap(),
      Some(big.clone())
    );
  });
}

#[test]
fn ttl_read_filter_can_match_cpp_lazy_cleanup() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 10).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16).with_ttl_read_filter(false);
    seed(
      &mut store,
      &tbl,
      &[
        upsert_with_expire(b"a", b"A", 1, 100),
        BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
      ],
    )
    .await;

    // C++ Read/Scan 不判断 expire_ts：清理前仍返回过期值。
    assert_eq!(
      store.get_at(&tbl, b"a", 100).await.unwrap(),
      Some(b"A".to_vec())
    );
    assert_eq!(
      store.scan_at(&tbl, b"", b"", 0, 100).await.unwrap(),
      vec![
        (b"a".to_vec(), b"A".to_vec()),
        (b"b".to_vec(), b"B".to_vec())
      ]
    );

    let stats = store.clean_expired(&tbl, 100).await.unwrap();
    assert_eq!(stats.deleted, 1);
    assert_eq!(store.get_at(&tbl, b"a", 100).await.unwrap(), None);
  });
}

/// sem-50：写批重写 data 页时，`expire_ts <= now_ms` 的基页旧条目被内联丢弃
/// （对齐 C++ `add_to_page` 的过期分支），同一批删除 TTL 索引项。`ttl_read_filter
/// = false` 下写后即不可见，不再依赖随后的 `clean_expired`。
#[test]
fn write_path_drops_expired_base_entry_inline() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 20).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16)
      .with_deferred_ttl(true)
      .with_ttl_read_filter(false);
    seed(
      &mut store,
      &tbl,
      &[
        upsert_with_expire(b"a", b"A", 1, 100),
        BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        upsert_with_expire(b"c", b"C", 1, 1_000),
      ],
    )
    .await;
    assert_ne!(store.ttl_root_id(&tbl).unwrap(), PageId::MAX);
    // 页未被重写前，C++ 口径（不过滤）仍读到过期值。
    assert_eq!(
      store.get_at(&tbl, b"a", 200).await.unwrap(),
      Some(b"A".to_vec())
    );

    // now=200 写同页另一个 key：a 已过期，必须在本批内物理丢弃。
    store
      .put_at(
        &tbl,
        &[BatchEntry::upsert(b"d".to_vec(), b"D".to_vec(), 2)],
        200,
      )
      .await
      .unwrap();
    assert_eq!(
      store.get_at(&tbl, b"a", 200).await.unwrap(),
      None,
      "写批必须内联丢弃过期基页条目"
    );
    assert_eq!(
      store.get_at(&tbl, b"c", 200).await.unwrap(),
      Some(b"C".to_vec())
    );
    // 页内容（scan 直接读数据页、关闭 TTL 读过滤）：过期条目不在页上。
    assert_eq!(
      store.scan_at(&tbl, b"", b"", 0, 200).await.unwrap(),
      vec![
        (b"b".to_vec(), b"B".to_vec()),
        (b"c".to_vec(), b"C".to_vec()),
        (b"d".to_vec(), b"D".to_vec()),
      ]
    );
    // TTL 索引项已在同一写批删除：clean_expired 只剩 c 的索引。
    let stats = store.clean_expired(&tbl, 200).await.unwrap();
    assert_eq!(stats.deleted, 0);
    assert_eq!(stats.ttl_deleted, 0);
    assert_eq!(stats.next_expire_ts, 1_000);
  });
}

/// sem-50：过期基页条目的 overflow 链在同一写批内释放（不再留给清理批）。
#[test]
fn write_path_releases_expired_base_overflow_chain() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 21).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16).with_deferred_ttl(true);
    let big = vec![b'v'; 4096];
    seed(
      &mut store,
      &tbl,
      &[upsert_with_expire(b"big", &big, 1, 100)],
    )
    .await;
    assert_eq!(
      store.get_at(&tbl, b"big", 0).await.unwrap(),
      Some(big.clone())
    );
    // 页布局：overflow 链 + data 页 + leaf index root + TTL 树（data + index）。
    let pages_before = store.mapper(&tbl).unwrap().tbl().len();
    let overflow_num = pages_before - 4;
    assert!(overflow_num > 0, "fixture 必须产生 overflow 链");
    let free_before = store.mapper(&tbl).unwrap().free_page_cnt();

    store
      .put_at(
        &tbl,
        &[BatchEntry::upsert(b"k2".to_vec(), b"2".to_vec(), 2)],
        200,
      )
      .await
      .unwrap();
    let mapper = store.mapper(&tbl).unwrap();
    // overflow 链 + 已空的 TTL 树（data + leaf index）一并释放。
    assert_eq!(
      mapper.free_page_cnt() - free_before,
      u32::try_from(overflow_num + 2).unwrap(),
      "过期条目的 overflow 链与 TTL 树页必须释放"
    );
    assert_eq!(store.get_at(&tbl, b"big", 200).await.unwrap(), None);
    assert_eq!(
      store.get_at(&tbl, b"k2", 200).await.unwrap(),
      Some(b"2".to_vec())
    );
  });
}

/// sem-50：过期基页条目的 large value 段在同一写批内释放。
#[test]
fn write_path_releases_expired_base_large_segment() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 22).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store = Store::new(MemStore::new(512).with_segment_config(4096, 2), 512, 16)
      .with_large_value(layout, 1024)
      .with_deferred_ttl(true);
    // `put_large` 用墙钟判定过期：expire 取未来，随后用注入时间越过它。
    let expire_ts = now_ms() + 60_000;
    let mut big = BatchEntry::upsert(b"big".to_vec(), vec![b'v'; 9000], 1);
    big.expire_ts = expire_ts;
    store.put_large(&tbl, &[big]).await.unwrap();
    assert!(live_segment_num(&store, &tbl) > 0);

    store
      .put_at(
        &tbl,
        &[BatchEntry::upsert(b"k2".to_vec(), b"2".to_vec(), 2)],
        expire_ts + 1,
      )
      .await
      .unwrap();
    assert_eq!(
      live_segment_num(&store, &tbl),
      0,
      "过期大 value 的段必须随写批释放"
    );
    assert_eq!(store.get_large_at(&tbl, b"big", 0).await.unwrap(), None);
    assert_eq!(
      store.get_at(&tbl, b"k2", 0).await.unwrap(),
      Some(b"2".to_vec())
    );
  });
}

/// sem-50：过期旧条目的 overflow 释放记录必须落在合并后 key 顺序的位置——
/// a（过期）的链释放先于其后新键 b 的链分配，对齐 C++ `ApplyOnePage` 的
/// 逐 key free→alloc 相位。
#[test]
fn write_path_expired_release_precedes_later_key_alloc() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 23).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16).with_deferred_ttl(true);
    let old_big = vec![b'a'; 4096];
    seed(
      &mut store,
      &tbl,
      &[
        upsert_with_expire(b"a", &old_big, 1, 100),
        BatchEntry::upsert(b"m".to_vec(), b"M".to_vec(), 1),
      ],
    )
    .await;
    // 页布局：a 的 overflow 链 + data 页 + leaf index root + TTL 树（data + index）。
    let overflow_num = store.mapper(&tbl).unwrap().tbl().len() - 4;
    assert!(overflow_num > 0, "fixture 必须产生 overflow 链");
    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();

    // b 位于 a 与 m 之间且自带新 overflow 链：先释放 a 的链，再分配 b 的链。
    store
      .put_at(
        &tbl,
        &[BatchEntry::upsert(b"b".to_vec(), vec![b'b'; 4096], 2)],
        200,
      )
      .await
      .unwrap();

    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let record = after.get(before.len()..).expect("增量 record");
    let delta = log_mapping_entry_li(record);
    // 链页按写入顺序从逻辑页 0 起分配：前 overflow_num 条必须是 a 的前向释放。
    let free_li: Vec<u32> = delta
      .iter()
      .take(overflow_num)
      .map(|(page_id, _)| *page_id)
      .collect();
    assert_eq!(
      free_li,
      (0..u32::try_from(overflow_num).unwrap()).collect::<Vec<_>>(),
      "a 的 overflow 链必须按前向顺序先释放: {delta:?}"
    );
    assert!(
      delta
        .iter()
        .take(overflow_num)
        .all(|(_, value)| *value == INVALID_VALUE),
      "链释放记录必须写 InvalidValue: {delta:?}"
    );
    // 释放之后才出现 b 的新链 / data 页分配记录。
    let first_alloc = delta
      .iter()
      .position(|(_, value)| *value != INVALID_VALUE)
      .expect("b 的新 overflow 链应被分配");
    assert!(
      first_alloc >= overflow_num,
      "a 的链释放必须全部先于 b 的分配: {delta:?}"
    );
  });
}

#[test]
fn scan_pagination_does_not_count_expired_entries() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 9).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let mut batch = Vec::new();
    for index in 0..10u32 {
      let key = format!("k{index:02}").into_bytes();
      if index % 2 == 0 {
        batch.push(upsert_with_expire(&key, b"v", 1, 100));
      } else {
        batch.push(BatchEntry::upsert(key, b"v".to_vec(), 1));
      }
    }
    seed(&mut store, &tbl, &batch).await;

    let mut begin = b"".to_vec();
    let mut begin_inclusive = true;
    let mut page_li = Vec::new();
    loop {
      let page = store
        .scan_page_at(
          &tbl,
          &begin,
          b"",
          100,
          fkv_store::ScanOption {
            begin_inclusive,
            ..ScanOption::default()
          },
          fkv_task_scan::PageOption {
            page_entries: 3,
            page_size: 0,
          },
        )
        .await
        .unwrap();
      // 过期项不占 entry 配额，因此每页尽可能给满 3 条 live entry。
      assert!(page.entry_li.len() <= 3);
      if let Some((key, _)) = page.entry_li.last() {
        begin = key.clone();
        begin_inclusive = false;
      }
      let has_remaining = page.has_remaining;
      page_li.push(page.entry_li);
      if !has_remaining {
        break;
      }
    }
    assert_eq!(page_li.len(), 2);
    assert_eq!(page_li[0].len(), 3);
    assert_eq!(page_li[1].len(), 2);
    let key_li: Vec<Vec<u8>> = page_li.into_iter().flatten().map(|(key, _)| key).collect();
    assert_eq!(
      key_li,
      vec![
        b"k01".to_vec(),
        b"k03".to_vec(),
        b"k05".to_vec(),
        b"k07".to_vec(),
        b"k09".to_vec()
      ]
    );
  });
}

#[test]
fn overwrite_and_delete_drop_ttl_index_without_touching_value() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    seed(&mut store, &tbl, &[upsert_with_expire(b"k", b"v1", 1, 100)]).await;
    // 覆盖为无 TTL 的新值；旧 TTL 索引条目成为 stale。
    seed(
      &mut store,
      &tbl,
      &[BatchEntry::upsert(b"k".to_vec(), b"v2".to_vec(), 2)],
    )
    .await;
    let stats = store.clean_expired(&tbl, 200).await.unwrap();
    assert_eq!(stats.deleted, 0);
    // eng-50：覆盖时旧 TTL 索引已被删除，clean_expired 不应再看到它。
    assert_eq!(stats.ttl_deleted, 0);
    assert_eq!(
      store.get_at(&tbl, b"k", 200).await.unwrap(),
      Some(b"v2".to_vec())
    );

    // 两次带 TTL 覆盖：旧 ts 的索引项不应删除新值。
    seed(&mut store, &tbl, &[upsert_with_expire(b"k", b"v3", 3, 300)]).await;
    seed(&mut store, &tbl, &[upsert_with_expire(b"k", b"v4", 4, 400)]).await;
    let stats = store.clean_expired(&tbl, 300).await.unwrap();
    assert_eq!(stats.deleted, 0);
    assert_eq!(stats.ttl_deleted, 0);
    assert_eq!(
      store.get_at(&tbl, b"k", 300).await.unwrap(),
      Some(b"v4".to_vec())
    );

    let stats = store.clean_expired(&tbl, 400).await.unwrap();
    assert_eq!(stats.deleted, 1);
    assert_eq!(stats.ttl_deleted, 1);
    assert_eq!(store.get_at(&tbl, b"k", 400).await.unwrap(), None);

    // 删除带 TTL 的 key 同样移除 TTL 索引，不会复活数据。
    seed(&mut store, &tbl, &[upsert_with_expire(b"d", b"D", 5, 500)]).await;
    store
      .put_at(&tbl, &[BatchEntry::delete(b"d".to_vec(), 6)], 6)
      .await
      .unwrap();
    let stats = store.clean_expired(&tbl, 500).await.unwrap();
    assert_eq!(stats.deleted, 0);
    assert_eq!(stats.ttl_deleted, 0);
    assert_eq!(store.get_at(&tbl, b"d", 500).await.unwrap(), None);
  });
}

#[test]
fn already_expired_upsert_is_skipped_at_write() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 11).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    // now=0 时写入，100 到期。
    seed(&mut store, &tbl, &[upsert_with_expire(b"k", b"v1", 1, 100)]).await;

    // now=100 时写入一个已过期的新值：不落主树，只删除对应 TTL 索引；
    // 同批折叠后它是该 key 的最新写，因此主树要删除旧值（R3 P2）。
    store
      .put_at(&tbl, &[upsert_with_expire(b"k", b"v2", 2, 100)], 100)
      .await
      .unwrap();
    assert_eq!(store.get_at(&tbl, b"k", 0).await.unwrap(), None);
    let stats = store.clean_expired(&tbl, 100).await.unwrap();
    // TTL 索引已在写路径删除，clean 看不到待清理项。
    assert_eq!(stats.deleted, 0);
    assert_eq!(stats.ttl_deleted, 0);

    // 全新的已过期 key 完全不产生主树 entry。
    store
      .put_at(&tbl, &[upsert_with_expire(b"gone", b"G", 3, 50)], 100)
      .await
      .unwrap();
    assert_eq!(store.get_at(&tbl, b"gone", 0).await.unwrap(), None);
    // 旧值已被最新（已过期）写覆盖删除。
    assert_eq!(
      store.scan_at(&tbl, b"", b"", 0, 0).await.unwrap(),
      Vec::new()
    );
  });
}

#[test]
fn write_after_expiry_triggers_cleanup() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 12).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    // 过期项放首张 data 页；足够多的 live key 让表跨多页，后续写只碰最后一页。
    // 这样删除只能来自写后 TriggerTTL——写路径内联清理只作用于被重写的页
    // （sem-50）。
    let mut batch = vec![upsert_with_expire(b"a", b"A", 1, 100)];
    for index in 0..300u32 {
      batch.push(BatchEntry::upsert(
        format!("k{index:03}").into_bytes(),
        b"v".to_vec(),
        1,
      ));
    }
    seed(&mut store, &tbl, &batch).await;

    // 未到期：普通写入不触发清理。
    store
      .put_at(
        &tbl,
        &[BatchEntry::upsert(b"zzzz".to_vec(), b"Z".to_vec(), 2)],
        99,
      )
      .await
      .unwrap();
    assert_eq!(
      store.get_at(&tbl, b"a", 99).await.unwrap(),
      Some(b"A".to_vec())
    );

    // now=100 的写入触发 TriggerTTL，过期项在写入后被物理删除。
    store
      .put_at(
        &tbl,
        &[BatchEntry::upsert(b"zzzz".to_vec(), b"Z2".to_vec(), 3)],
        100,
      )
      .await
      .unwrap();
    assert_eq!(store.get_at(&tbl, b"a", 100).await.unwrap(), None);
    assert_eq!(
      store.get_at(&tbl, b"k000", 100).await.unwrap(),
      Some(b"v".to_vec())
    );
    assert_eq!(
      store.get_at(&tbl, b"zzzz", 100).await.unwrap(),
      Some(b"Z2".to_vec())
    );
  });
}

#[compio::test]
async fn expired_only_write_reopens_without_data() {
  let root = TempRoot::new("store_ttl_expired_only");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store
    .put_at(&tbl, &[upsert_with_expire(b"gone", b"G", 1, 10)], 100)
    .await
    .unwrap();
  assert_eq!(store.get_at(&tbl, b"gone", 0).await.unwrap(), None);
  // sem-57：无 manifest 分区上的全过期批折叠成无效果 delete，与 C++
  // `manifest_size_ == 0` 一致，不创建 manifest 对象。
  assert_eq!(store.page_io().read_manifest(&tbl).await.unwrap(), None);
  drop(store);

  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  // 无 manifest 分区的 raw reopen 按 ResourceMissing 拒绝；SDK / shard 懒开
  // （ResourceMissing → `open_empty`）下用户可见语义不变：空分区、无数据。
  assert!(matches!(
    store.reopen(&tbl).await,
    Err(KvError::ResourceMissing)
  ));
  store.open_empty(&tbl);
  assert_eq!(store.get_at(&tbl, b"gone", 0).await.unwrap(), None);
  assert!(
    store
      .scan_at(&tbl, b"", b"", 0, 0)
      .await
      .unwrap()
      .is_empty()
  );
}

#[compio::test]
async fn ttl_tree_survives_reopen_and_log_replay() {
  let root = TempRoot::new("store_ttl_reopen");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  seed(&mut store, &tbl, &[upsert_with_expire(b"a", b"A", 1, 1000)]).await;
  // 第二次 put 走 manifest log，TTL 映射也必须被 replay。
  seed(&mut store, &tbl, &[upsert_with_expire(b"b", b"B", 2, 2000)]).await;
  let ttl_root = store.ttl_root_id(&tbl).unwrap();
  assert_ne!(ttl_root, PageId::MAX);
  drop(store);

  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.ttl_root_id(&tbl), Some(ttl_root));
  assert_eq!(
    store.get_at(&tbl, b"a", 999).await.unwrap(),
    Some(b"A".to_vec())
  );
  assert_eq!(store.get_at(&tbl, b"a", 1000).await.unwrap(), None);
  assert_eq!(store.get_at(&tbl, b"b", 2000).await.unwrap(), None);

  let stats = store.clean_expired(&tbl, 1000).await.unwrap();
  assert_eq!(stats.deleted, 1);
  assert_eq!(stats.next_expire_ts, 2000);
  let stats = store.clean_expired(&tbl, 2000).await.unwrap();
  assert_eq!(stats.deleted, 1);
  assert_eq!(stats.next_expire_ts, 0);
  drop(store);

  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  );
  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.get_at(&tbl, b"a", 0).await.unwrap(), None);
  assert_eq!(store.get_at(&tbl, b"b", 0).await.unwrap(), None);
  assert_eq!(store.scan_at(&tbl, b"", b"", 0, 0).await.unwrap(), vec![]);
}

/// issue 14：CleanExpired 只扫 `[空, ttl_key(now+1))`，并正确维护下一条到期时间。
#[compio::test]
async fn clean_expired_scans_bounded_range_and_tracks_next() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(MemStore::new(512), 512, 16);
  // 200 条未来才过期 + 5 条已过期。
  let mut batch = Vec::new();
  for i in 0..200u32 {
    batch.push(upsert_with_expire(
      format!("future{i:04}").as_bytes(),
      b"v",
      1,
      10_000 + u64::from(i),
    ));
  }
  for i in 0..5u32 {
    batch.push(upsert_with_expire(
      format!("expired{i:04}").as_bytes(),
      b"v",
      1,
      100,
    ));
  }
  seed(&mut store, &tbl, &batch).await;

  let stats = store.clean_expired(&tbl, 1_000).await.unwrap();
  assert_eq!(stats.deleted, 5);
  assert_eq!(stats.ttl_deleted, 5);
  // 最早未过期 = 10000。
  assert_eq!(stats.next_expire_ts, 10_000);

  for i in 0..5u32 {
    let key = format!("expired{i:04}");
    assert_eq!(
      store.get_at(&tbl, key.as_bytes(), 1_000).await.unwrap(),
      None
    );
  }
  for i in 0..200u32 {
    let key = format!("future{i:04}");
    assert_eq!(
      store.get_at(&tbl, key.as_bytes(), 1_000).await.unwrap(),
      Some(b"v".to_vec())
    );
  }

  // 再清理一次：无到期项，next 仍为 10000。
  let stats = store.clean_expired(&tbl, 1_001).await.unwrap();
  assert_eq!(stats.deleted, 0);
  assert_eq!(stats.next_expire_ts, 10_000);
}

/// eng-45：开启延迟清理后，写路径只标记 pending，不在 put 内联扫 TTL；
/// `drain_pending_ttl` 才真正删除。
#[compio::test]
async fn deferred_ttl_cleanup_requires_drain() {
  let tbl = TableIdent::new("demo", 2).unwrap();
  let mut store = Store::new(MemStore::new(512), 512, 16)
    .with_deferred_ttl(true)
    .with_ttl_read_filter(false);
  // 过期项放首张 data 页；300 条 live key 撑出多页，now=200 的写只改最后一页，
  // 过期项因此保留到 drain（写路径内联清理只作用于被重写的页，sem-50）。
  let mut batch = vec![upsert_with_expire(b"k000", b"v", 1, 100)];
  for index in 1..300u32 {
    batch.push(BatchEntry::upsert(
      format!("k{index:03}").into_bytes(),
      b"v".to_vec(),
      1,
    ));
  }
  store.put_at(&tbl, &batch, 0).await.unwrap();
  // now=200 的新写只标记 pending，不内联清理。
  store
    .put_at(
      &tbl,
      &[BatchEntry::upsert(b"zzzz".to_vec(), b"Z".to_vec(), 2)],
      200,
    )
    .await
    .unwrap();
  assert!(store.pending_ttl_len() >= 1);
  assert_eq!(
    store.get_at(&tbl, b"k000", 200).await.unwrap(),
    Some(b"v".to_vec()),
    "deferred mode must not clean inline"
  );

  let cleaned = store.drain_pending_ttl_at(200).await.unwrap();
  assert!(cleaned >= 1);
  assert_eq!(store.pending_ttl_len(), 0);
  assert_eq!(store.get_at(&tbl, b"k000", 200).await.unwrap(), None);
  assert_eq!(
    store.get_at(&tbl, b"zzzz", 200).await.unwrap(),
    Some(b"Z".to_vec())
  );
}

/// eng-50：覆盖 / 删除带 TTL 的 key 必须移除旧 TTL 索引项。
#[test]
fn overwrite_and_delete_remove_stale_ttl_entries() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    seed(
      &mut store,
      &tbl,
      &[upsert_with_expire(b"a", b"A1", 1, 5000)],
    )
    .await;
    // 覆盖成新的 expire：旧 TTL(5000,a) 必须被删除。
    seed(
      &mut store,
      &tbl,
      &[upsert_with_expire(b"a", b"A2", 2, 9000)],
    )
    .await;
    let stats = store.clean_expired(&tbl, 6000).await.unwrap();
    assert_eq!(stats.ttl_deleted, 0, "旧 TTL 项不应残留");
    assert_eq!(stats.deleted, 0);
    assert_eq!(
      store.get_at(&tbl, b"a", 6000).await.unwrap(),
      Some(b"A2".to_vec())
    );

    // 删除带 TTL 的 key：TTL 项同样要移除。
    seed(&mut store, &tbl, &[upsert_with_expire(b"b", b"B", 3, 5000)]).await;
    seed(&mut store, &tbl, &[BatchEntry::delete(b"b".to_vec(), 4)]).await;
    let stats = store.clean_expired(&tbl, 7000).await.unwrap();
    assert_eq!(stats.ttl_deleted, 0, "删除后 TTL 项不应残留");
    assert_eq!(store.get(&tbl, b"b").await.unwrap(), None);
    // 新的 expire 项仍应保留到过期。
    let stats = store.clean_expired(&tbl, 9500).await.unwrap();
    assert_eq!(stats.ttl_deleted, 1);
    assert_eq!(stats.deleted, 1);
  });
}

/// core-14：reopen 后 TTL 树非空时，下一次写入必须触发一次过期清理，
/// 不需要等到新的 TTL 写入。
#[compio::test]
async fn reopen_forces_ttl_check_on_next_write() {
  let root = TempRoot::new("store_ttl_reopen_trigger");
  let tbl = TableIdent::new("demo", 13).unwrap();
  {
    let mut store = Store::new(
      CompioStore::new(root.path().to_path_buf(), 512, 16),
      512,
      16,
    );
    // 过期项放首张 data 页，live key 撑出多页：后续写不重写该页，删除只能由
    // reopen 置位的 next_expire 触发（sem-50 的写路径内联清理不参与）。
    let mut batch = vec![upsert_with_expire(b"a", b"A", 1, 100)];
    for index in 0..300u32 {
      batch.push(BatchEntry::upsert(
        format!("k{index:03}").into_bytes(),
        b"v".to_vec(),
        1,
      ));
    }
    seed(&mut store, &tbl, &batch).await;
  }

  let mut store = Store::new(
    CompioStore::new(root.path().to_path_buf(), 512, 16),
    512,
    16,
  );
  store.reopen(&tbl).await.unwrap();
  // 未到期：普通写入不清理。
  store
    .put_at(
      &tbl,
      &[BatchEntry::upsert(b"zzzz".to_vec(), b"Z".to_vec(), 2)],
      99,
    )
    .await
    .unwrap();
  assert_eq!(
    store.get_at(&tbl, b"a", 99).await.unwrap(),
    Some(b"A".to_vec())
  );

  // 已到期：写入本身不带 TTL，也必须由 reopen 置位的 next_expire 触发清理。
  store
    .put_at(
      &tbl,
      &[BatchEntry::upsert(b"zzzz".to_vec(), b"Z2".to_vec(), 3)],
      100,
    )
    .await
    .unwrap();
  assert_eq!(store.get_at(&tbl, b"a", 100).await.unwrap(), None);
  assert_eq!(
    store.get_at(&tbl, b"zzzz", 100).await.unwrap(),
    Some(b"Z2".to_vec())
  );
}

/// R4-03：`install_replay_state` 整体替换表状态时必须清掉延迟 TTL 清理标记。
///
/// 否则 reopen 后 `pending_ttl_len` 虚高，下一轮 drain 会做无谓清理。
#[compio::test]
async fn reopen_resets_pending_ttl_flag() {
  let root = TempRoot::new("store_ttl_pending_reopen");
  let tbl = TableIdent::new("demo", 9).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      512,
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    512,
    16,
  )
  .with_deferred_ttl(true);
  store
    .put_at(&tbl, &[upsert_with_expire(b"a", b"A", 1, 10)], 0)
    .await
    .unwrap();
  // now 已越过最早过期时间：defer 模式下只登记 pending，不内联清理。
  store
    .put_at(
      &tbl,
      &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)],
      200,
    )
    .await
    .unwrap();
  assert_eq!(store.pending_ttl_len(), 1);

  store.reopen(&tbl).await.unwrap();
  assert_eq!(store.pending_ttl_len(), 0);
}

/// R6：主树删除时间戳必须压过条目自身的 ts。
///
/// 用户 ts 可能是 µs / ns 量级（远大于毫秒 `now_ms`）。若 delete 用毫秒
/// 时间戳，折叠时 delete 落败，过期条目永不物理删除、空间无界增长。
#[compio::test]
async fn clean_expired_removes_entries_with_ns_timestamp() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(MemStore::new(512), 512, 16);
  let ns_ts = 1_760_000_000_000_000_000u64;
  seed(
    &mut store,
    &tbl,
    &[upsert_with_expire(b"ns", b"V", ns_ts, 500)],
  )
  .await;
  // 过期前可读（now=0 不过滤）。
  assert_eq!(
    store.get_at(&tbl, b"ns", 0).await.unwrap(),
    Some(b"V".to_vec())
  );

  let stats = store.clean_expired(&tbl, 1_000).await.unwrap();
  assert_eq!(stats.deleted, 1);
  assert_eq!(stats.ttl_deleted, 1);
  assert_eq!(stats.next_expire_ts, 0);
  // 物理删除：主树里不再有该条目（不以 TTL 过滤掩盖）。
  assert_eq!(store.get_at(&tbl, b"ns", 0).await.unwrap(), None);
  assert_eq!(store.scan_at(&tbl, b"", b"", 0, 0).await.unwrap(), vec![]);
}

/// 直接改写 TTL 树的唯一 data 页（测试构造坏 key / 手工布局用）。
///
/// `entry_li` 必须按 key 升序。TTL 根是 leaf index 时数据页在 leftmost 指针处。
async fn rewrite_ttl_page(store: &Store<MemStore>, tbl: &TableIdent, entry_li: &[(&[u8], &[u8])]) {
  use fkv_page::{
    CompressionType, DataPageBuilder, EntrySpec, IndexPage, PageType, page_type, set_next_page_id,
    set_prev_page_id,
  };
  use fkv_types::NULL_PAGE;

  let ttl_root = store.ttl_root_id(tbl).unwrap();
  let mapper = store.mapper(tbl).unwrap();
  let root_fp = mapper.file_page_id(ttl_root).unwrap();
  let mut buf = vec![0u8; 512];
  store
    .page_io()
    .read_page(tbl, root_fp, &mut buf)
    .await
    .unwrap();
  let data_page_id = match page_type(&buf).unwrap() {
    PageType::LeafIndex => IndexPage::new(&buf).unwrap().leftmost_ptr().unwrap(),
    PageType::Data => ttl_root,
    other => panic!("TTL 根页类型异常: {other:?}"),
  };
  let data_fp = mapper.file_page_id(data_page_id).unwrap();
  let mut builder = DataPageBuilder::new(512, 16).unwrap();
  for (key, value) in entry_li {
    assert!(
      builder
        .add(EntrySpec {
          key,
          value,
          ts: 0,
          expire_ts: 0,
          overflow: false,
          compression: CompressionType::None,
          large_value: false,
        })
        .unwrap(),
      "测试页容量不足"
    );
  }
  let mut page = builder.finish().unwrap();
  set_prev_page_id(&mut page, NULL_PAGE).unwrap();
  set_next_page_id(&mut page, NULL_PAGE).unwrap();
  store
    .page_io()
    .write_page(tbl, data_fp, &page)
    .await
    .unwrap();
  // 旁路写：直写 `page_io()` 绕过了写路径的页缓存（同一 `FilePageId` 的盘上
  // 字节已变而缓存不知），必须显式失效，否则后续 `clean_expired` 的写批会
  // 从缓存读到改写前的整页。
  store.invalidate_write_cache();
}

/// 坏 TTL key（长度不足 8 字节）不得让 `next_expire_ts` 归零、清理停摆，
/// 并且必须被真正回收，否则出界坏 key 永久占用探测成本（r7a-core-05）。
///
/// 构造「坏 key 在 expired_end 之后、合法 TTL key 在坏 key 之后」的布局：
/// 探测到坏 key 时跳过它继续向后，返回后面合法条目的到期时间，同时把坏 key
/// 收进本轮删除批次。
#[compio::test]
async fn clean_expired_skips_corrupt_ttl_key_probe() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(MemStore::new(512), 512, 16);
  seed(
    &mut store,
    &tbl,
    &[upsert_with_expire(b"seed", b"S", 1, 1_000)],
  )
  .await;

  // 坏 key = 6 字节（缺 8 字节 ts 前缀的后两位），排在 expired_end 之后、
  // 合法项之前；合法项 ts = 65536，是 now+1=5 之后的第一条可解析条目。
  let corrupt = [0u8, 0, 0, 0, 0, 1];
  let good = ttl_key(65_536, b"k");
  assert!(corrupt.as_slice() < good.as_slice());
  rewrite_ttl_page(
    &store,
    &tbl,
    &[(corrupt.as_slice(), b""), (good.as_slice(), b"")],
  )
  .await;

  let stats = store.clean_expired(&tbl, 4).await.unwrap();
  // 修复前：坏 key 只跳过不删除，且被当成 expire_ts = 0 → next 归零、停摆。
  assert_eq!(stats.next_expire_ts, 65_536);
  assert_eq!(stats.deleted, 0);
  assert_eq!(stats.ttl_deleted, 1, "探测跳过的坏 key 必须被回收");

  // 第二轮：坏 key 已不存在，探测不再为它多读页。
  let stats = store.clean_expired(&tbl, 4).await.unwrap();
  assert_eq!(stats.ttl_deleted, 0, "坏 key 已回收，不得重复删除");
  assert_eq!(stats.next_expire_ts, 65_536);
}

/// 坏 key 长尾按上界分批回收：单轮最多跳过
/// `TTL_BAD_KEY_SKIP_MAX` 条，命中上界时 `next_expire_ts` 压回当前时刻，
/// 下一轮继续，直到长尾清空（r7a-core-05）。
#[compio::test]
async fn clean_expired_reclaims_corrupt_tail_in_bounded_rounds() {
  // 与 store 侧常量保持一致：本轮上界即单轮最多跳过的坏 key 数。
  const SKIP_MAX: usize = 16;
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(MemStore::new(512), 512, 16);
  seed(
    &mut store,
    &tbl,
    &[upsert_with_expire(b"seed", b"S", 1, 1_000)],
  )
  .await;

  // 20 条坏 key 全部排在 expired_end 之后（第 6 字节 > 0），无合法项。
  let bad_li: Vec<[u8; 6]> = (1..=SKIP_MAX + 4)
    .map(|idx| [0, 0, 0, 0, 0, idx as u8])
    .collect();
  let empty: &[u8] = b"";
  let page_li: Vec<(&[u8], &[u8])> = bad_li.iter().map(|key| (key.as_slice(), empty)).collect();
  rewrite_ttl_page(&store, &tbl, &page_li).await;

  let stats = store.clean_expired(&tbl, 4).await.unwrap();
  assert_eq!(stats.ttl_deleted, SKIP_MAX, "单轮按上限回收坏 key");
  assert_eq!(stats.next_expire_ts, 4, "命中上界必须驱动下一轮");

  let stats = store.clean_expired(&tbl, 4).await.unwrap();
  assert_eq!(stats.ttl_deleted, 4, "剩余坏 key 在下一轮清空");
  assert_eq!(stats.next_expire_ts, 0, "长尾清空后不再重试");

  let stats = store.clean_expired(&tbl, 4).await.unwrap();
  assert_eq!(stats.ttl_deleted, 0);
  assert_eq!(stats.next_expire_ts, 0);
}

/// 更旧 timestamp 的 upsert 被 merge 丢弃时（旧值存活），旧 TTL 索引也不能被删；
/// 否则存活条目到期后永远不会被 `CleanExpired` 物理清理（C++ `change_ts <
/// base_ts` 不走 `UpdateTTL`）。
///
/// sem-50 补充：若写批的 `now_ms` 已越过旧条目自身的 `expire_ts`，C++
/// `add_to_page` 无论 batch 是否覆盖都会丢弃该基页条目；此时主树条目与 TTL
/// 索引项都在同一写批消失，`clean_expired` 不再有工作。
#[test]
fn older_expired_upsert_keeps_old_ttl_index_until_expiry() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16).with_deferred_ttl(true);
    store
      .put_at(&tbl, &[upsert_with_expire(b"k", b"OLD", 10, 2000)], 1000)
      .await
      .unwrap();
    // ts=5 < 旧值 ts=10，且写批 now=1500 未到期：merge 保留旧值，TTL 索引保留。
    store
      .put_at(&tbl, &[upsert_with_expire(b"k", b"NEW", 5, 2000)], 1500)
      .await
      .unwrap();
    assert_eq!(
      store.get_at(&tbl, b"k", 1500).await.unwrap(),
      Some(b"OLD".to_vec())
    );
    let stats = store.clean_expired(&tbl, 3000).await.unwrap();
    assert!(stats.deleted >= 1, "{stats:?}");
    assert!(stats.ttl_deleted >= 1, "{stats:?}");
    assert_eq!(store.get_at(&tbl, b"k", 0).await.unwrap(), None);

    // 同样的旧 timestamp 批，但写批 now=3000 已越过旧条目 expire：旧条目按
    // sem-50 在写批内丢弃，TTL 索引同时删除（对齐 C++ add_to_page）。
    store
      .put_at(&tbl, &[upsert_with_expire(b"k", b"OLD", 10, 2000)], 1000)
      .await
      .unwrap();
    store
      .put_at(&tbl, &[upsert_with_expire(b"k", b"NEW", 5, 2000)], 3000)
      .await
      .unwrap();
    assert_eq!(store.get_at(&tbl, b"k", 1000).await.unwrap(), None);
    let stats = store.clean_expired(&tbl, 3000).await.unwrap();
    assert_eq!(stats.deleted, 0, "写批已丢弃过期条目: {stats:?}");
    assert_eq!(stats.ttl_deleted, 0, "TTL 索引随写批删除: {stats:?}");
  });
}

/// sem-55 A：相同 expire 的覆盖不调用 `UpdateTTL`（C++ `base_expire ==
/// expire_ts` 分支），`ttl_batch_` 为空时 `ApplyTTLBatch` 不写 TTL 树。
///
/// 修复前该负载的 manifest 增量为 `[(data 页, _), (TTL data 页, _)]`：TTL 数据页
/// 内容不变仍被 COW 重写；修复后只剩主树 data 页的 COW。
#[test]
fn sem55_same_expire_overwrite_skips_ttl_page_write() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem55", 8).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    store
      .put_at(&tbl, &[upsert_with_expire(b"k", b"v1", 1, 2_000)], 1_000)
      .await
      .unwrap();
    let ttl_root = store.ttl_root_id(&tbl);
    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    store
      .put_at(&tbl, &[upsert_with_expire(b"k", b"v2", 2, 2_000)], 1_000)
      .await
      .unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let delta = log_mapping_entry_li(after.get(before.len()..).expect("增量 record"));
    assert_eq!(
      delta.len(),
      1,
      "相同 expire 覆盖只应写主树 data 页，不得重写 TTL 页：{delta:?}"
    );
    assert_eq!(delta[0].0, 0, "增量必须只有 data 页 0 的 COW：{delta:?}");
    assert_eq!(store.ttl_root_id(&tbl), ttl_root, "TTL 树不得被重写");
    assert_eq!(
      store.get_at(&tbl, b"k", 1_000).await.unwrap(),
      Some(b"v2".to_vec())
    );
    // 相同 expire 覆盖不得丢索引项：到期后主树值与 TTL 索引一并清理（sto-66）。
    let stats = store.clean_expired(&tbl, 2_000).await.unwrap();
    assert_eq!(
      (stats.deleted, stats.ttl_deleted),
      (1, 1),
      "相同 expire 覆盖后索引项必须仍可被 clean_expired 命中: {stats:?}"
    );
    assert_eq!(store.get_at(&tbl, b"k", 0).await.unwrap(), None);
  });
}

/// sem-55 B：落后 timestamp 的覆盖（expire 不同）被 merge 丢弃
/// （`kept.timestamp < iter.timestamp()`），C++ 完全不调用 `UpdateTTL`。
///
/// 修复前预生成的 `ttl_key(1500, k)` 仍被写入 TTL 树：manifest 多一条 TTL 页
/// 记录、`note_ttl_batch` 把 `next_expire` 拉低到 1500，`clean_expired(1500)`
/// 返回 `ttl_deleted=1`。修复后该轮必须无删除，`next_expire` 保持 2000。
#[test]
fn sem55_losing_upsert_leaves_no_phantom_ttl_index() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem55", 9).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16).with_deferred_ttl(true);
    store
      .put_at(&tbl, &[upsert_with_expire(b"k", b"v1", 10, 2_000)], 1_000)
      .await
      .unwrap();
    let before = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    store
      .put_at(&tbl, &[upsert_with_expire(b"k", b"v2", 5, 1_500)], 1_000)
      .await
      .unwrap();
    let after = store.page_io().read_manifest(&tbl).await.unwrap().unwrap();
    let delta = log_mapping_entry_li(after.get(before.len()..).expect("增量 record"));
    assert_eq!(
      delta.len(),
      1,
      "被丢弃的落后写入只重写主树 data 页，不得写 TTL 页：{delta:?}"
    );
    assert_eq!(delta[0].0, 0, "{delta:?}");
    assert_eq!(
      store.get_at(&tbl, b"k", 1_000).await.unwrap(),
      Some(b"v1".to_vec()),
      "落后写入不得覆盖主树值"
    );
    let stats = store.clean_expired(&tbl, 1_500).await.unwrap();
    assert_eq!(
      (stats.deleted, stats.ttl_deleted),
      (0, 0),
      "TTL 树不得存在 1500 的幽灵索引项: {stats:?}"
    );
    assert_eq!(
      stats.next_expire_ts, 2_000,
      "next_expire 不得被落后写入拉低: {stats:?}"
    );
    // 原索引项仍在，真正到期时正常清理。
    let stats = store.clean_expired(&tbl, 2_000).await.unwrap();
    assert_eq!(
      (stats.deleted, stats.ttl_deleted),
      (1, 1),
      "存活条目的索引项必须保留至到期: {stats:?}"
    );
  });
}
