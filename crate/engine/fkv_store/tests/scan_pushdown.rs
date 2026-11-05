//! perf-84：`scan*` 非分页入口的 `max_entries` 与 TTL 过滤必须下推到叶子迭代器，
//! 不得整区间物化后再截断。用 `MemStore::read_num` 观察实际读页数。

use fkv_btree::BatchEntry;
use fkv_io_mem::MemStore;
use fkv_store::{ScanOption, Store};
use fkv_types::TableIdent;

fn entry(key: impl Into<Vec<u8>>, value: &[u8], expire_ts: u64) -> BatchEntry {
  let mut entry = BatchEntry::upsert(key.into(), value.to_vec(), 1);
  entry.expire_ts = expire_ts;
  entry
}

fn key_of(index: u32) -> Vec<u8> {
  format!("k{index:04}").into_bytes()
}

/// 1000 条区间、`max_entries=10`、每 7 条夹一条过期项：
/// 只读必要页，结果与全量扫描截断一致。
#[test]
fn scan_max_entries_materializes_only_needed_pages() {
  pollster::block_on(async {
    const TOTAL: u32 = 1000;
    const LIMIT: usize = 10;
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let batch: Vec<BatchEntry> = (0..TOTAL)
      .map(|index| {
        let expire_ts = if index % 7 == 0 { 100 } else { 0 };
        entry(key_of(index), &[b'v'; 8], expire_ts)
      })
      .collect();
    // now=0 写入，避免历史时间戳被写时过滤。
    store.put_at(&tbl, &batch, 0).await.unwrap();

    let before = store.page_io().read_num();
    let full_li = store.scan_entry_at(&tbl, b"", b"", 0, 1000).await.unwrap();
    let full_reads = store.page_io().read_num() - before;
    assert_eq!(full_li.len(), TOTAL as usize - TOTAL.div_ceil(7) as usize);

    let before = store.page_io().read_num();
    let limited_li = store
      .scan_entry_at(&tbl, b"", b"", LIMIT, 1000)
      .await
      .unwrap();
    let limited_reads = store.page_io().read_num() - before;

    // 过期项不占配额：前 10 条 live 覆盖到 index 11。
    let expected: Vec<Vec<u8>> = [1, 2, 3, 4, 5, 6, 8, 9, 10, 11]
      .into_iter()
      .map(key_of)
      .collect();
    let actual: Vec<Vec<u8>> = limited_li.iter().map(|entry| entry.key.clone()).collect();
    assert_eq!(actual, expected);
    assert_eq!(limited_li, full_li[..LIMIT]);

    // 整区间物化会读完整条叶子链；下推后只读下降路径 + 当前页 + 1 页预取。
    assert!(
      limited_reads * 8 < full_reads,
      "limited={limited_reads}, full={full_reads}"
    );
    assert!(
      limited_reads <= 8,
      "limited scan should read few pages, got {limited_reads}"
    );
  });
}

/// 关闭 TTL 读过滤时行为不变：过期项照常返回，且仍按 `max_entries` 提前停止。
#[test]
fn scan_without_ttl_filter_keeps_expired_entries() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 2).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16).with_ttl_read_filter(false);
    let batch: Vec<BatchEntry> = (0..100u32)
      .map(|index| entry(key_of(index), b"v", 100))
      .collect();
    store.put_at(&tbl, &batch, 0).await.unwrap();

    let before = store.page_io().read_num();
    let li = store.scan_entry_at(&tbl, b"", b"", 10, 1000).await.unwrap();
    let reads = store.page_io().read_num() - before;

    assert_eq!(li.len(), 10);
    assert_eq!(li[0].expire_ts, 100);
    assert!(reads <= 8, "expected early stop, read {reads} pages");
  });
}

/// perf-85：`keys_only` 只物化 key，不读 overflow 链；value 为空。
#[test]
fn scan_keys_only_skips_overflow_chain() {
  pollster::block_on(async {
    const TOTAL: usize = 5;
    // value 2000B，单页可用 500B → 每条约 4 个 overflow 页。
    const OVERFLOW_PAGES_PER_ENTRY: u64 = 4;
    let tbl = TableIdent::new("demo", 3).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let batch: Vec<BatchEntry> = (0..TOTAL as u32)
      .map(|index| BatchEntry::upsert(key_of(index), vec![b'v'; 2000], 1))
      .collect();
    store.put(&tbl, &batch).await.unwrap();

    let before = store.page_io().read_num();
    let full_li = store.scan(&tbl, b"", b"", 0).await.unwrap();
    let full_reads = store.page_io().read_num() - before;
    assert_eq!(full_li.len(), TOTAL);
    assert_eq!(full_li[0].1.len(), 2000);

    let before = store.page_io().read_num();
    let key_li = store
      .scan_with_option(
        &tbl,
        b"",
        b"",
        0,
        ScanOption {
          keys_only: true,
          ..ScanOption::default()
        },
      )
      .await
      .unwrap();
    let key_reads = store.page_io().read_num() - before;
    assert!(key_li.iter().all(|(_, value)| value.is_empty()));

    let key_of_full: Vec<Vec<u8>> = full_li.into_iter().map(|(key, _)| key).collect();
    let key_of_only: Vec<Vec<u8>> = key_li.into_iter().map(|(key, _)| key).collect();
    assert_eq!(key_of_only, key_of_full);
    // keys_only 只读下降路径 + 叶子页；全量扫描还要读每条 overflow 链。
    assert!(key_reads <= 4, "keys_only read {key_reads} pages");
    assert!(
      full_reads >= key_reads + TOTAL as u64 * OVERFLOW_PAGES_PER_ENTRY,
      "keys_only must skip overflow chain reads: full={full_reads}, keys={key_reads}"
    );
  });
}
