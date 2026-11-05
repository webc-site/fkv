//! SimStore 接进引擎：open / put / get / reopen 最小组装（gap-03）。

use fkv_btree::BatchEntry;
use fkv_io_sim::{SimStore, oplog};
use fkv_store::Store;
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
const RESTART_INTERVAL: u16 = 16;

fn tbl() -> TableIdent {
  TableIdent::new("sim", 1).unwrap()
}

#[test]
fn sim_store_open_put_get_reopen() {
  pollster::block_on(async {
    let io = SimStore::new(PAGE_SIZE).with_payload_capture();
    let mut store = Store::new(io, PAGE_SIZE, RESTART_INTERVAL);
    let tbl = tbl();

    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

    // reopen 从 manifest 重放，不依赖内存里的 root 缓存。
    store.reopen(&tbl).await.unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(store.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 2);

    // 写入路径自己 fsync（sync_data 先于 manifest 追加），掉电后已确认写不丢。
    store.page_io().crash();
    store.reopen(&tbl).await.unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(store.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
  });
}

/// 底层写 op 的文本格式必须带 payload，且能逐行解析回来。
#[test]
fn sim_store_op_log_text_roundtrip() {
  pollster::block_on(async {
    let io = SimStore::new(PAGE_SIZE).with_payload_capture();
    let mut store = Store::new(io, PAGE_SIZE, RESTART_INTERVAL);
    let tbl = tbl();
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"k".to_vec(), b"value".to_vec(), 1)],
      )
      .await
      .unwrap();

    let record_li = store.page_io().op_log_li();
    assert!(
      record_li.iter().any(|record| record
        .payload
        .as_deref()
        .is_some_and(|payload| payload.windows(5).any(|window| window == b"value"))),
      "payload 捕获应包含 value 字节"
    );
    // 观察型（读 / 列举）变体显式报错，写类变体逐行往返一致。
    for (index, record) in record_li.iter().enumerate() {
      match record.to_text(index + 1) {
        Ok(text) => {
          let parsed = oplog::parse_record(index + 1, &text).unwrap();
          assert_eq!(&parsed, record, "第 {} 条 op 文本往返不一致", index + 1);
          assert!(text.contains("clock=") && text.contains("op="));
        }
        Err(oplog::OpLogError::UnsupportedOp { .. }) => {}
        Err(err) => panic!("第 {} 条 op 序列化失败: {err}", index + 1),
      }
    }
  });
}
