//! TTL 索引与删除时序测试。

use fkv_btree::TtlUpsertSink;
use fkv_task_write::ttl_key;
use fkv_types::WriteOp;

use super::*;

/// sto-65：更旧时间戳的 delete 会被 merge 丢弃（旧条目存活），此时既不能释放旧值、
/// 也不能清旧 TTL 索引——否则存活条目指向已释放的页、TTL 过期永不触发。
#[test]
fn older_delete_keeps_old_value_and_ttl_index() {
  let store = MemStore::new(512);
  let tbl = TableIdent::new("demo", 1).unwrap();
  pollster::block_on(async {
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let mut sink = Vec::new();
    let seed = [BatchEntry {
      expire_ts: 1_000,
      ..BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 100)
    }];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &seed,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"a", 512)
        .await
        .unwrap(),
      Some(b"A".to_vec())
    );

    // 更旧的 delete（ts=50 < 100）：merge 丢弃它，旧条目存活。
    let mut sink = Vec::new();
    let older = [BatchEntry::delete(b"a".to_vec(), 50)];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &older,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    assert!(
      sink.is_empty(),
      "更旧的 delete 不应清存活条目的 TTL 索引：{sink:?}"
    );
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"a", 512)
        .await
        .unwrap(),
      Some(b"A".to_vec()),
      "更旧的 delete 不应释放存活条目的值"
    );

    // 更新的 delete（ts=200 > 100）：真正覆盖 → 值消失、TTL 索引被清。
    let mut sink = Vec::new();
    let newer = [BatchEntry::delete(b"a".to_vec(), 200)];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &newer,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    assert_eq!(sink.len(), 1, "更新的 delete 必须清旧 TTL 索引：{sink:?}");
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"a", 512)
        .await
        .unwrap(),
      None
    );
  });
}

/// sto-66：相同 expire_ts 的覆盖不能把 TTL 索引项删掉，否则到期后 `clean_expired`
/// 扫不到该 key、主树条目（含 overflow 链）永久滞留。
#[test]
fn same_expire_overwrite_keeps_ttl_index_entry() {
  let store = MemStore::new(512);
  let tbl = TableIdent::new("demo", 1).unwrap();
  pollster::block_on(async {
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let mut sink = Vec::new();
    let seed = [BatchEntry {
      expire_ts: 1_000,
      ..BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 100)
    }];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &seed,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();

    // 相同 expire_ts 覆盖：TTL 索引项不变，不许推删除项，也不生成 upsert
    // （sem-55：对齐 C++ `base_expire == expire_ts` 不调用 `UpdateTTL`）。
    let mut sink = Vec::new();
    let upsert_sink = TtlUpsertSink::new();
    let mut layout = PageLayout::new(512, 16, 16, None, 16);
    layout.data.ttl_upsert_sink = Some(&upsert_sink);
    let same_expire = [BatchEntry {
      expire_ts: 1_000,
      ..BatchEntry::upsert(b"a".to_vec(), b"B".to_vec(), 200)
    }];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &same_expire,
        layout,
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    assert!(
      sink.is_empty(),
      "相同 expire_ts 的覆盖不应删旧 TTL 索引项：{sink:?}"
    );
    assert!(
      upsert_sink.take().is_empty(),
      "相同 expire_ts 的覆盖不应生成 TTL upsert"
    );
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"a", 512)
        .await
        .unwrap(),
      Some(b"B".to_vec())
    );

    // expire_ts 真变了才删旧 TTL 索引项。
    let mut sink = Vec::new();
    let new_expire = [BatchEntry {
      expire_ts: 2_000,
      ..BatchEntry::upsert(b"a".to_vec(), b"C".to_vec(), 300)
    }];
    write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &new_expire,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    assert_eq!(sink.len(), 1, "expire_ts 变了必须删旧 TTL 索引项：{sink:?}");
  });
}

/// sem-50：写批重写 data 页时，`expire_ts <= now_ms` 的旧条目直接丢弃，
/// 不进入新页；同一批把 `ttl_key(old_expire, key)` 推入 TTL 删除出口
/// （对齐 C++ `add_to_page` 的过期分支）。
#[test]
fn expired_base_entry_dropped_on_page_rewrite() {
  let store = MemStore::new(512);
  let tbl = TableIdent::new("sem50", 1).unwrap();
  pollster::block_on(async {
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let seed = [
      BatchEntry {
        expire_ts: 50,
        ..BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)
      },
      BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
      BatchEntry {
        expire_ts: 200,
        ..BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 1)
      },
    ];
    let mut sink = Vec::new();
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &seed,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();

    // now=100 重写同页：a（expire 50）丢弃，c（expire 200）保留。
    let mut sink = Vec::new();
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &[BatchEntry::upsert(b"d".to_vec(), b"D".to_vec(), 2)],
        layout: PageLayout::new(512, 16, 16, None, 16).with_now_ms(100),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();

    let all = scan_range(
      &store,
      ScanArg {
        tbl: &tbl,
        snapshot: &snap(&tbl, &mapper, root),
        tree: ScanTree::Data,
        begin: b"",
        end: b"",
        page_size: 512,
        max_entries: 0,
        dict: None,
        ttl_filter_now_ms: None,
      },
    )
    .await
    .unwrap();
    let key_li: Vec<Vec<u8>> = all.into_iter().map(|(key, _)| key).collect();
    assert_eq!(
      key_li,
      vec![b"b".to_vec(), b"c".to_vec(), b"d".to_vec()],
      "过期旧条目不得进入新页"
    );
    assert_eq!(sink.len(), 1, "过期条目的 TTL 索引必须在本批删除: {sink:?}");
    assert_eq!(sink[0].key, ttl_key(50, b"a"));
    assert_eq!(sink[0].op, WriteOp::Delete);
    assert_eq!(
      sink[0].timestamp, 0,
      "TTL 索引项版本号须与 C++ UpdateTTL 一致"
    );
  });
}

/// sem-50：过期旧条目的 overflow 链在同一写批内整链释放，不留给清理批。
#[test]
fn expired_base_overflow_chain_released_on_page_rewrite() {
  let store = MemStore::new(512);
  let tbl = TableIdent::new("sem50", 2).unwrap();
  pollster::block_on(async {
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let mut big = BatchEntry::upsert(b"big".to_vec(), vec![b'v'; 4096], 1);
    big.expire_ts = 50;
    let mut sink = Vec::new();
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &[big],
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    // 页布局：overflow 链 + data 页 + leaf index root。
    let overflow_num = mapper.tbl().len() - 2;
    assert!(overflow_num > 0, "fixture 必须产生 overflow 链");
    assert_eq!(mapper.free_page_cnt(), 0);

    let mut sink = Vec::new();
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &[BatchEntry::upsert(b"k2".to_vec(), b"2".to_vec(), 2)],
        layout: PageLayout::new(512, 16, 16, None, 16).with_now_ms(100),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    assert_eq!(
      mapper.free_page_cnt() as usize,
      overflow_num,
      "过期条目的 overflow 链必须整链释放"
    );
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"big", 512)
        .await
        .unwrap(),
      None
    );
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"k2", 512)
        .await
        .unwrap(),
      Some(b"2".to_vec())
    );
    assert_eq!(sink.len(), 1);
    assert_eq!(sink[0].key, ttl_key(50, b"big"));
  });
}

/// sem-55：无基页的插入必须生成 TTL upsert（C++ `cmp > 0` 的 INSERT 分支）。
#[test]
fn insert_collects_ttl_upsert() {
  let store = MemStore::new(512);
  let tbl = TableIdent::new("sem55", 3).unwrap();
  pollster::block_on(async {
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let sink = TtlUpsertSink::new();
    let mut layout = PageLayout::new(512, 16, 16, None, 16);
    layout.data.ttl_upsert_sink = Some(&sink);
    let batch = [BatchEntry {
      expire_ts: 1_000,
      ..BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)
    }];
    write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &batch,
        layout,
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(sink.take(), vec![(b"a".to_vec(), 1_000)]);
  });
}

/// sem-55：expire 变化的覆盖 = 删旧索引 + 建新索引（C++ `base_expire !=
/// expire_ts` 的两个 `UpdateTTL`）。
#[test]
fn expire_change_overwrite_replaces_ttl_index() {
  let store = MemStore::new(512);
  let tbl = TableIdent::new("sem55", 4).unwrap();
  pollster::block_on(async {
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let seed = [BatchEntry {
      expire_ts: 1_000,
      ..BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 100)
    }];
    let mut sink = Vec::new();
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &seed,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();

    let mut sink = Vec::new();
    let upsert_sink = TtlUpsertSink::new();
    let mut layout = PageLayout::new(512, 16, 16, None, 16);
    layout.data.ttl_upsert_sink = Some(&upsert_sink);
    let batch = [BatchEntry {
      expire_ts: 2_000,
      ..BatchEntry::upsert(b"a".to_vec(), b"A2".to_vec(), 200)
    }];
    write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &batch,
        layout,
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    assert_eq!(upsert_sink.take(), vec![(b"a".to_vec(), 2_000)]);
    assert_eq!(sink.len(), 1);
    assert_eq!(sink[0].key, ttl_key(1_000, b"a"));
    assert_eq!(sink[0].op, WriteOp::Delete);
  });
}

/// sem-55：新值为无 TTL 覆盖带 TTL 的旧值：只删旧索引，不建新索引
/// （C++ `UpdateTTL(0, Upsert)` 是 no-op）。
#[test]
fn clearing_expire_deletes_old_ttl_index_without_upsert() {
  let store = MemStore::new(512);
  let tbl = TableIdent::new("sem55", 5).unwrap();
  pollster::block_on(async {
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let seed = [BatchEntry {
      expire_ts: 1_000,
      ..BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 100)
    }];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &seed,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();

    let mut sink = Vec::new();
    let upsert_sink = TtlUpsertSink::new();
    let mut layout = PageLayout::new(512, 16, 16, None, 16);
    layout.data.ttl_upsert_sink = Some(&upsert_sink);
    let batch = [BatchEntry::upsert(b"a".to_vec(), b"A2".to_vec(), 200)];
    write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &batch,
        layout,
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    assert!(
      upsert_sink.take().is_empty(),
      "expire=0 的新值不得生成 TTL 索引项"
    );
    assert_eq!(sink.len(), 1);
    assert_eq!(sink[0].key, ttl_key(1_000, b"a"));
  });
}

/// sem-55 B：落后 timestamp 的覆盖被 merge 丢弃时，不得生成新 expire 的
/// 幽灵 TTL 索引项，也不得删存活条目的旧索引（C++ `change_ts < base_ts`
/// 完全不调用 `UpdateTTL`）。
#[test]
fn losing_upsert_collects_no_ttl_change() {
  let store = MemStore::new(512);
  let tbl = TableIdent::new("sem55", 6).unwrap();
  pollster::block_on(async {
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let seed = [BatchEntry {
      expire_ts: 2_000,
      ..BatchEntry::upsert(b"a".to_vec(), b"OLD".to_vec(), 10)
    }];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &seed,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();

    let mut sink = Vec::new();
    let upsert_sink = TtlUpsertSink::new();
    let mut layout = PageLayout::new(512, 16, 16, None, 16);
    layout.data.ttl_upsert_sink = Some(&upsert_sink);
    let batch = [BatchEntry {
      expire_ts: 1_500,
      ..BatchEntry::upsert(b"a".to_vec(), b"NEW".to_vec(), 5)
    }];
    write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &batch,
        layout,
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    assert!(
      upsert_sink.take().is_empty(),
      "被丢弃的落后写入不得生成幽灵 TTL 索引项"
    );
    assert!(sink.is_empty(), "存活条目的旧 TTL 索引必须保留：{sink:?}");
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"a", 512)
        .await
        .unwrap(),
      Some(b"OLD".to_vec())
    );
  });
}

/// sem-55：delete 胜出只删旧 TTL 索引，不生成 upsert；更旧的 delete 被丢弃
/// 时两者都不产生。
#[test]
fn delete_winner_collects_only_ttl_delete() {
  let store = MemStore::new(512);
  let tbl = TableIdent::new("sem55", 7).unwrap();
  pollster::block_on(async {
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let seed = [BatchEntry {
      expire_ts: 1_000,
      ..BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 100)
    }];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch_li: &seed,
        layout: PageLayout::new(512, 16, 16, None, 16),
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: None,
      },
    )
    .await
    .unwrap();

    // 更旧的 delete：被 merge 丢弃，TTL 不变。
    let mut sink = Vec::new();
    let upsert_sink = TtlUpsertSink::new();
    let mut layout = PageLayout::new(512, 16, 16, None, 16);
    layout.data.ttl_upsert_sink = Some(&upsert_sink);
    let older = [BatchEntry::delete(b"a".to_vec(), 50)];
    let root = write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &older,
        layout,
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    assert!(sink.is_empty());
    assert!(upsert_sink.take().is_empty());

    // 更新的 delete：删旧索引，无 upsert。
    let mut sink = Vec::new();
    let upsert_sink = TtlUpsertSink::new();
    let mut layout = PageLayout::new(512, 16, 16, None, 16);
    layout.data.ttl_upsert_sink = Some(&upsert_sink);
    let newer = [BatchEntry::delete(b"a".to_vec(), 200)];
    write_batch(
      &store,
      WriteArg {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch_li: &newer,
        layout,
        segment_mapper: None,
        segment_size: 0,
        ttl_delete_sink: Some(&mut sink),
      },
    )
    .await
    .unwrap();
    assert!(upsert_sink.take().is_empty());
    assert_eq!(sink.len(), 1);
    assert_eq!(sink[0].key, ttl_key(1_000, b"a"));
    assert_eq!(sink[0].op, WriteOp::Delete);
  });
}
