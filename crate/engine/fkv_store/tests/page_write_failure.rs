//! test-01：页写 IO 错误后「旧数据完好 + mapper 干净 + store 可续写」。
//!
//! 与 `oom_retry` 的 `OutOfMem`（可重试）不同，这里注入不可重试的 `IoFail`：
//! `Store::put_at` 必须把错误上抛，同时 `settle_mapper_batch` 回滚 mapping /
//! 分配器 / 脏页记录，不能污染下一次写批。
//!
//! SimStore 的 `write_page_li` 走 `PageIo` 默认实现（逐页 `write_page`），因此
//! `WritePage@2` 会先让第 1 页真正落盘、再在第 2 页失败，形成「部分页已写」，
//! 而不是整批前置拒绝。对照 C++ `tests/persist.cpp` 的
//! "failed index-page write surfaces error without aborting"。
//!
//! 注意：两条小 value 若塞进同一 data 页，第二批只会写 1 页，`@2` 永不命中；
//! 这里让 b / c 各占一页（400B value），使第二批至少写 2 个 data 页 + index 页，
//! 才能稳定命中「第 2 次 WritePage」并留下部分写。

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_failpoint::{reset, set_spec};
use fkv_io_sim::{OpKind, SimStore};
use fkv_store::Store;
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
const RESTART_INTERVAL: u16 = 16;

#[test]
fn page_write_error_keeps_prior_committed_state() {
  pollster::block_on(async {
    let io = SimStore::new(PAGE_SIZE);
    let mut store = Store::new(io, PAGE_SIZE, RESTART_INTERVAL);
    let tbl = TableIdent::new("page-write-fail", 1).unwrap();

    // 1. 先提交 a，记录回滚基线。
    store
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

    let root_before = store.root_id(&tbl);
    let (max_fp_before, mapping_count_before) = {
      let mapper = store.mapper(&tbl).expect("put 后 mapper 必须存在");
      assert!(mapper.dirty_page_li().is_empty(), "提交后不应残留脏页");
      assert!(
        mapper.changed_page_li().is_empty(),
        "提交后不应残留变更记录"
      );
      (mapper.max_fp_id(), mapper.mapping_count())
    };
    let writes_before = store.page_io().op_count(OpKind::WritePage);

    // 2. 第 2 次 WritePage 注入 IoFail：第 1 页已写、第 2 页失败。
    set_spec("WritePage@2!err=IoFail");
    let err = store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"b".to_vec(), b"B".repeat(400), 2),
          BatchEntry::upsert(b"c".to_vec(), b"C".repeat(400), 2),
        ],
      )
      .await
      .unwrap_err();
    assert_eq!(err, KvError::IoFail);

    // 确认故障发生在「部分页已写」之后，而不是整批前置拒绝。
    assert_eq!(
      store.page_io().op_count(OpKind::WritePage),
      writes_before + 1,
      "第 1 页应已落盘，第 2 页才注入失败"
    );

    // 3. 旧数据完好，新键不可见。
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(store.get(&tbl, b"b").await.unwrap(), None);
    assert_eq!(store.get(&tbl, b"c").await.unwrap(), None);

    // 4. mapper 回滚到批次前：dirty / changed / 分配器 / mapping 均无残留。
    {
      let mapper = store.mapper(&tbl).expect("失败后 mapper 仍应存在");
      assert!(mapper.dirty_page_li().is_empty(), "回滚后不应残留脏页");
      assert!(
        mapper.changed_page_li().is_empty(),
        "回滚后不应残留变更记录"
      );
      assert_eq!(mapper.max_fp_id(), max_fp_before, "分配器高水位必须回滚");
      assert_eq!(
        mapper.mapping_count(),
        mapping_count_before,
        "mapping 数量必须回滚"
      );
    }
    assert_eq!(store.root_id(&tbl), root_before, "root 必须保持失败前一致");
    if let Some(segment_mapper) = store.segment_mapper(&tbl) {
      assert!(
        segment_mapper.dirty_page_li().is_empty(),
        "segment mapper 不应残留脏页"
      );
      assert!(
        segment_mapper.changed_page_li().is_empty(),
        "segment mapper 不应残留变更记录"
      );
    }

    // 5. reset 后续写成功，且失败批次不污染新批次。
    reset();
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"b".to_vec(), b"B".repeat(400), 3),
          BatchEntry::upsert(b"c".to_vec(), b"C".repeat(400), 3),
        ],
      )
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(store.get(&tbl, b"b").await.unwrap(), Some(b"B".repeat(400)));
    assert_eq!(store.get(&tbl, b"c").await.unwrap(), Some(b"C".repeat(400)));
    {
      let mapper = store.mapper(&tbl).unwrap();
      assert!(mapper.dirty_page_li().is_empty());
      assert!(mapper.changed_page_li().is_empty());
    }

    // 6. 落盘后重放也能看到完整提交（部分写留下的孤儿页不被引用）。
    store.reopen(&tbl).await.unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(store.get(&tbl, b"b").await.unwrap(), Some(b"B".repeat(400)));
    assert_eq!(store.get(&tbl, b"c").await.unwrap(), Some(b"C".repeat(400)));
  });
}
