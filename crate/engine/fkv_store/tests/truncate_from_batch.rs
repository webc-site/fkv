//! `truncate_from` 分批扫描 / 删除的正确性（R2-1）。
//!
//! 表规模超过单轮上限 [`fkv_store::TRUNCATE_FROM_BATCH`] 时必须多轮衔接：
//! 已删除区间不再出现，剩余前缀完整，重开后可复现（内存上界由实现保证）。

use fkv_btree::BatchEntry;
use fkv_io_mem::MemStore;
use fkv_store::{Store, TRUNCATE_FROM_BATCH};
use fkv_types::TableIdent;

const KEY_NUM: usize = TRUNCATE_FROM_BATCH + 404;
const KEEP_NUM: usize = TRUNCATE_FROM_BATCH + 100;

fn key(index: usize) -> Vec<u8> {
  format!("k{index:05}").into_bytes()
}

#[test]
fn truncate_from_spans_multiple_batches() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::new(256), 256, 16);
    let batch: Vec<BatchEntry> = (0..KEY_NUM)
      .map(|index| BatchEntry::upsert(key(index), vec![b'v'; 32], 1))
      .collect();
    store.put(&tbl, &batch).await.unwrap();
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), KEY_NUM);

    let position = key(KEEP_NUM);
    assert_eq!(
      store.truncate_from(&tbl, &position).await.unwrap(),
      KEY_NUM - KEEP_NUM
    );

    let entry_li = store.scan(&tbl, b"", b"", 0).await.unwrap();
    assert_eq!(entry_li.len(), KEEP_NUM);
    assert_eq!(entry_li.first().unwrap().0, key(0));
    assert_eq!(entry_li.last().unwrap().0, key(KEEP_NUM - 1));
    assert!(store.get(&tbl, &position).await.unwrap().is_none());

    // 幂等：位置之后已无 key。
    assert_eq!(store.truncate_from(&tbl, &position).await.unwrap(), 0);

    // 重开后前缀与删除结果一致。
    let mut reopened = Store::new(store.into_io(), 256, 16);
    reopened.reopen(&tbl).await.unwrap();
    let entry_li = reopened.scan(&tbl, b"", b"", 0).await.unwrap();
    assert_eq!(entry_li.len(), KEEP_NUM);
    assert_eq!(entry_li.last().unwrap().0, key(KEEP_NUM - 1));
  });
}
