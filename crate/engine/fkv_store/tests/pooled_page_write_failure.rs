//! test-11：Pooled（生产默认）下的页写 IoFail 回滚与续写。
//!
//! 对照 `page_write_failure.rs`（Append）：失败批次**覆盖已提交的 key**，
//! 强制 COW `remap_page` 先把旧物理页放进 retired 队列，再在第二页注入
//! `IoFail`。`abort_batch` 必须同时恢复 mapping / 分配器 / dirty / changed
//! 与 **retired 队列**；否则回滚后重新活跃的旧页会被下一次 reclaim 回池复用，
//! 覆盖 `a` 的已提交值。
//!
//! 这里只加 Pooled 特有的断言：失败后 `retired_len` 必须回到批次前（Append
//! 模式 retire 本身是 no-op，测不出这条回滚）。

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_failpoint::{reset, set_spec};
use fkv_io_sim::{OpKind, SimStore};
use fkv_store::{AllocMode, Store};
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
const RESTART_INTERVAL: u16 = 16;

fn value(tag: u8) -> Vec<u8> {
  vec![tag; 400]
}

#[test]
fn pooled_page_write_error_rolls_back_retired_and_keeps_prior_state() {
  pollster::block_on(async {
    let io = SimStore::new(PAGE_SIZE);
    let mut store = Store::new(io, PAGE_SIZE, RESTART_INTERVAL).with_alloc_mode(AllocMode::Pooled);
    let tbl = TableIdent::new("pooled-page-write-fail", 1).unwrap();

    // 1. 先提交 a，记录回滚基线。
    store
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

    let root_before = store.root_id(&tbl);
    let (max_fp_before, mapping_count_before, retired_before) = {
      let mapper = store.mapper(&tbl).expect("put 后 mapper 必须存在");
      assert_eq!(mapper.alloc_mode(), AllocMode::Pooled);
      (
        mapper.max_fp_id(),
        mapper.mapping_count(),
        mapper.retired_len(),
      )
    };
    let writes_before = store.page_io().op_count(OpKind::WritePage);

    // 2. 失败批次同时**覆盖 a** 并新增 b / c：覆盖会 retire a 的旧物理页，
    //    第二页写失败后必须撤销这次 retire。
    set_spec("WritePage@2!err=IoFail");
    let err = store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), value(b'A'), 2),
          BatchEntry::upsert(b"b".to_vec(), value(b'B'), 2),
          BatchEntry::upsert(b"c".to_vec(), value(b'C'), 2),
        ],
      )
      .await
      .unwrap_err();
    assert_eq!(err, KvError::IoFail);

    // 故障确实发生在「部分页已写」之后。
    assert_eq!(
      store.page_io().op_count(OpKind::WritePage),
      writes_before + 1,
      "第 1 页应已落盘，第 2 页才注入失败"
    );

    // 3. 旧数据完好，失败批次的新值 / 新键不可见。
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(store.get(&tbl, b"b").await.unwrap(), None);
    assert_eq!(store.get(&tbl, b"c").await.unwrap(), None);

    // 4. mapper 完整回滚：分配器 / mapping / retired 队列都不得留下失败批次的痕迹。
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
      assert_eq!(
        mapper.retired_len(),
        retired_before,
        "批次内 retire 的旧页必须在 abort 时撤销，否则会被回池复用"
      );
    }
    assert_eq!(store.root_id(&tbl), root_before, "root 必须保持失败前一致");

    // 5. reset 后续写成功，失败批次不污染新批次。
    reset();
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), value(b'A'), 3),
          BatchEntry::upsert(b"b".to_vec(), value(b'B'), 3),
          BatchEntry::upsert(b"c".to_vec(), value(b'C'), 3),
        ],
      )
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(value(b'A')));
    assert_eq!(store.get(&tbl, b"b").await.unwrap(), Some(value(b'B')));
    assert_eq!(store.get(&tbl, b"c").await.unwrap(), Some(value(b'C')));

    // 6. 再覆盖一轮：回收 / 复用旧页后数据仍必须是最后一轮。
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), value(b'D'), 4),
          BatchEntry::upsert(b"b".to_vec(), value(b'D'), 4),
        ],
      )
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(value(b'D')));
    assert_eq!(store.get(&tbl, b"b").await.unwrap(), Some(value(b'D')));

    // 7. 重开重放：失败批次的孤儿页不得被 manifest 引用，已提交状态完整。
    store.reopen(&tbl).await.unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(value(b'D')));
    assert_eq!(store.get(&tbl, b"b").await.unwrap(), Some(value(b'D')));
    assert_eq!(store.get(&tbl, b"c").await.unwrap(), Some(value(b'C')));
  });
}
