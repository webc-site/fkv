//! test-06：Store 级 `put_large` 失败必须整批回滚到段批次开场前。
//!
//! `Store::put_large_impl` 的三个失败入口（段写失败 / `base_timestamp` 树读失败 /
//! `branch_segment_limit` 超限）共享同一个 `settle_mapper_batch` 结算点；任何一条
//! 路径漏掉回滚，失败批分配的 segment file id 就会永久泄漏（分配器高水位不回收、
//! branch 预留区间被占满），或留下「半可见」的大 value。
//!
//! 与既有覆盖的分工：
//! - `fkv_large/tests/batch_fail.rs` 只验证单条 value 的 `write_value` 段释放；
//! - `fkv_io_sim/tests/fault_matrix.rs` 直接在 IO 层调 `write_segment`；
//! - 这里走完整 Store 路径，验证「先提交 A、批内 B/C 失败」后 A 完整可读、
//!   B/C 不可见、segment mapper 无残留、store 仍可续写、reopen 一致。
//!
//! failpoint 注册表是进程级全局状态；本文件内用例并行执行时会互相污染，
//! 因此所有用例先拿同一把锁并清空注入点。

use std::{
  sync::atomic::{AtomicBool, Ordering},
  thread::yield_now,
};

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_failpoint::{reset, set_spec};
use fkv_io::FileIo;
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_io_sim::{Fault, OpKind, SimStore};
use fkv_large::SegmentLayout;
use fkv_store::{BRANCH_FILE_RESERVE_FILE_NUM, Store};
use fkv_testkit::TempRoot;
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, TableIdent, decode_id, is_file_page_id};

const PAGE_SIZE: usize = 512;
const SEGMENT_SIZE: usize = 4096;
const SEGMENT_SHIFT: u8 = 2;
const RESTART_INTERVAL: u16 = 16;
const LARGE_THRESHOLD: usize = 1024;
/// `MAX_SEGMENTS_BATCH` = 8：20 段正好跨 3 个段批（8 + 8 + 4）。
const VALUE_SEGMENT_NUM: usize = 20;

static FAILPOINT_BUSY: AtomicBool = AtomicBool::new(false);

/// 串行化进程级 failpoint 注入窗口。
///
/// `std::sync::Mutex` 的 guard 不能跨 `await`（clippy `await_holding_lock`），
/// 而本文件用例的注入窗口包含整段异步写路径，因此用测试专用的自旋标志：
/// 用例都跑在独立测试线程上，等待方 `yield_now` 不会阻塞持锁方推进。
struct FailpointGuard;

impl FailpointGuard {
  fn acquire() -> Self {
    while FAILPOINT_BUSY.swap(true, Ordering::Acquire) {
      yield_now();
    }
    reset();
    Self
  }
}

impl Drop for FailpointGuard {
  fn drop(&mut self) {
    reset();
    FAILPOINT_BUSY.store(false, Ordering::Release);
  }
}

fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

fn layout() -> SegmentLayout {
  SegmentLayout::new(SEGMENT_SIZE as u32, SEGMENT_SHIFT).unwrap()
}

fn compio_store(root: &TempRoot) -> Store<CompioStore> {
  let io = CompioStore::new(
    root.path().to_path_buf(),
    PAGE_SIZE,
    DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(SEGMENT_SIZE, SEGMENT_SHIFT);
  Store::new(io, PAGE_SIZE, RESTART_INTERVAL).with_large_value(layout(), LARGE_THRESHOLD)
}

/// 段写入口失败：B 的整条段链已落盘、C 的段链写了一半时注入 `IoFail`。
///
/// 失败点 `WriteSegments@29` = B 的 20 段（3 批）+ C 的第一批 8 段之后，
/// 即 C 的第二个段批首段；此时 data 树尚未写，store 必须整体回滚 segment
/// MapperBatch，而不是只释放 C 自己分配的那部分段。
#[compio::test]
async fn put_large_segment_write_failure_rolls_back_and_keeps_store_writable() {
  let _guard = FailpointGuard::acquire();
  let root = TempRoot::new("store_large_segment_write_failure");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = compio_store(&root);

  // 1. 先提交 A（20 段 = 3 个段批），记录回滚基线。
  let value_a = pattern(SEGMENT_SIZE * VALUE_SEGMENT_NUM);
  let entry_a = BatchEntry::upsert(b"a".to_vec(), value_a.clone(), 1).with_metadata(b"meta-a");
  store.put_large(&tbl, &[entry_a]).await.unwrap();
  let max_fp_before = {
    let mapper = store
      .segment_mapper(&tbl)
      .expect("put_large 成功后必须有 segment mapper");
    assert_eq!(mapper.max_fp_id(), VALUE_SEGMENT_NUM as u64);
    assert!(
      mapper.changed_page_li().is_empty(),
      "提交后不应残留变更记录"
    );
    assert!(mapper.dirty_page_li().is_empty(), "提交后不应残留脏页");
    mapper.max_fp_id()
  };
  let mapping_before = store.segment_mapper(&tbl).unwrap().mapping_count();
  let root_before = store.root_id(&tbl);
  let files_before = store.page_io().list_segment_file_id_li(&tbl).await.unwrap();
  assert_eq!(
    files_before,
    vec![0, 1, 2, 3, 4],
    "A 的 20 段应跨 5 个 segment 文件"
  );

  // 2. B 的 20 段全部写完，C 的第一批 8 段写完，第 29 次段写失败。
  let value_b = pattern(SEGMENT_SIZE * VALUE_SEGMENT_NUM);
  let value_c = pattern(SEGMENT_SIZE * VALUE_SEGMENT_NUM);
  set_spec("WriteSegments@29!err=IoFail");
  let err = store
    .put_large(
      &tbl,
      &[
        BatchEntry::upsert(b"b".to_vec(), value_b.clone(), 2).with_metadata(b"meta-b"),
        BatchEntry::upsert(b"c".to_vec(), value_c.clone(), 2).with_metadata(b"meta-c"),
      ],
    )
    .await
    .unwrap_err();
  assert_eq!(err, KvError::IoFail, "段写失败必须从 Store::put_large 上抛");

  // 3. 失败阶段证据：B 的 5 个段文件与 C 的 2 个半截段文件确实已落盘，
  //    证明错误发生在「部分段已写」之后，而不是整批前置拒绝。
  let files_during = store.page_io().list_segment_file_id_li(&tbl).await.unwrap();
  assert_eq!(
    files_during,
    (0..=11).collect::<Vec<_>>(),
    "失败前 B 的整链与 C 的前 8 段必须已落盘"
  );

  // 4. A 完整可读（value + metadata），B/C 不可见。
  let read_a = store
    .get_large(&tbl, b"a")
    .await
    .unwrap()
    .expect("已提交的 A 必须仍可读");
  assert_eq!(read_a.value, value_a);
  assert_eq!(read_a.metadata, b"meta-a");
  assert_eq!(
    store.get_large(&tbl, b"b").await.unwrap(),
    None,
    "B 不得可见"
  );
  assert_eq!(
    store.get_large(&tbl, b"c").await.unwrap(),
    None,
    "C 不得可见"
  );
  assert_eq!(
    store.get(&tbl, b"b").await.unwrap(),
    None,
    "B 不得出现在主树"
  );

  // 5. 段 mapper 回滚到批次前：无 changed/dirty、高水位回退、无半截段引用。
  {
    let mapper = store
      .segment_mapper(&tbl)
      .expect("失败后 segment mapper 仍应存在");
    assert!(
      mapper.changed_page_li().is_empty(),
      "回滚后不应残留变更记录"
    );
    assert!(mapper.dirty_page_li().is_empty(), "回滚后不应残留脏页");
    assert_eq!(mapper.max_fp_id(), max_fp_before, "分配器高水位必须回滚");
    assert_eq!(
      mapper.mapping_count(),
      mapping_before,
      "活跃段映射数必须回滚"
    );
    assert_eq!(
      mapper.tbl().len(),
      mapping_before,
      "不得残留失败批的逻辑槽位"
    );
    assert_eq!(mapper.free_page_cnt(), 0, "失败批释放的段不得留在空闲链表");
    let live_fp_li: Vec<u64> = mapper
      .tbl()
      .iter_value_li()
      .filter(|value| is_file_page_id(*value))
      .map(decode_id)
      .collect();
    assert_eq!(live_fp_li.len(), mapping_before);
    assert!(
      live_fp_li.iter().all(|fp| *fp < max_fp_before),
      "活跃映射不得指向失败批的半截段: {live_fp_li:?}"
    );
  }
  assert_eq!(
    store.root_id(&tbl),
    root_before,
    "失败批不得推进 data 树 root"
  );

  // 6. reset 后重试同一批成功：A/B/C 全部可读，孤儿段槽位被原地复用。
  reset();
  store
    .put_large(
      &tbl,
      &[
        BatchEntry::upsert(b"b".to_vec(), value_b.clone(), 3).with_metadata(b"meta-b"),
        BatchEntry::upsert(b"c".to_vec(), value_c.clone(), 3).with_metadata(b"meta-c"),
      ],
    )
    .await
    .unwrap();
  assert_eq!(
    store.segment_mapper(&tbl).unwrap().max_fp_id(),
    3 * VALUE_SEGMENT_NUM as u64
  );
  assert_eq!(
    store.page_io().list_segment_file_id_li(&tbl).await.unwrap(),
    (0..=14).collect::<Vec<_>>(),
    "重试必须复用孤儿段槽位，不得继续扩张文件"
  );
  assert_eq!(
    store.get_large(&tbl, b"a").await.unwrap().unwrap().value,
    value_a
  );
  let read_b = store.get_large(&tbl, b"b").await.unwrap().unwrap();
  assert_eq!(read_b.value, value_b);
  assert_eq!(read_b.metadata, b"meta-b");
  let read_c = store.get_large(&tbl, b"c").await.unwrap().unwrap();
  assert_eq!(read_c.value, value_c);
  assert_eq!(read_c.metadata, b"meta-c");

  // 7. reopen 后一致：失败批的孤儿段不被任何 mapping 引用。
  drop(store);
  let mut reopened = compio_store(&root);
  reopened.reopen(&tbl).await.unwrap();
  let mapper = reopened.segment_mapper(&tbl).unwrap();
  assert_eq!(mapper.max_fp_id(), 3 * VALUE_SEGMENT_NUM as u64);
  assert!(mapper.changed_page_li().is_empty());
  assert!(mapper.dirty_page_li().is_empty());
  assert_eq!(
    reopened.get_large(&tbl, b"a").await.unwrap().unwrap().value,
    value_a
  );
  assert_eq!(
    reopened.get_large(&tbl, b"b").await.unwrap().unwrap().value,
    value_b
  );
  assert_eq!(
    reopened.get_large(&tbl, b"c").await.unwrap().unwrap().value,
    value_c
  );
}

/// `branch_segment_limit` 超限（第三条失败入口）。
///
/// branch 的 segment 分配器从预留块下界起步，上界为 `base + reserve` 个 segment。
/// 写一条恰好越过上界的 value：段已全部落盘后才在统一结算点返回 `OutOfSpace`，
/// 必须把本批分配整体回滚，否则预留区间会被一次性占满、后续写永久 OutOfSpace。
#[test]
fn put_large_branch_segment_limit_rolls_back() {
  pollster::block_on(async {
    let _guard = FailpointGuard::acquire();
    let tbl = TableIdent::new("demo", 2).unwrap();
    // shift = 0：预留块 = 1024 个 segment（1 段/文件），超限只需 1025 段。
    let branch_layout = SegmentLayout::new(SEGMENT_SIZE as u32, 0).unwrap();
    let mut parent = Store::new(
      MemStore::new(PAGE_SIZE).with_segment_config(SEGMENT_SIZE, 0),
      PAGE_SIZE,
      RESTART_INTERVAL,
    )
    .with_large_value(branch_layout, LARGE_THRESHOLD);
    parent
      .put(
        &tbl,
        &[BatchEntry::upsert(b"seed".to_vec(), b"seed".to_vec(), 1)],
      )
      .await
      .unwrap();
    let info = parent.create_branch(&tbl, "feature").await.unwrap();
    assert!(info.bytes > 0, "branch manifest 必须落盘");

    let mut branch = Store::new(parent.into_io(), PAGE_SIZE, RESTART_INTERVAL)
      .with_large_value(branch_layout, LARGE_THRESHOLD);
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    let base_fp = branch
      .segment_mapper(&tbl)
      .expect("branch 必须带 segment mapper")
      .max_fp_id();
    let limit_fp = branch
      .branch_segment_limit()
      .expect("branch 必须携带 segment 预留上界");
    assert_eq!(base_fp, BRANCH_FILE_RESERVE_FILE_NUM);
    assert_eq!(limit_fp, base_fp + BRANCH_FILE_RESERVE_FILE_NUM);

    // 1025 段 > 预留区间容量 1024 段。
    let oversize = pattern(SEGMENT_SIZE * (BRANCH_FILE_RESERVE_FILE_NUM as usize + 1));
    let err = branch
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"too-big".to_vec(), oversize, 2)],
      )
      .await
      .unwrap_err();
    assert_eq!(
      err,
      KvError::OutOfSpace,
      "branch 预留区间之外的段分配必须报 OutOfSpace"
    );

    {
      let mapper = branch
        .segment_mapper(&tbl)
        .expect("失败后 segment mapper 仍应存在");
      assert!(
        mapper.changed_page_li().is_empty(),
        "超限后不应残留变更记录"
      );
      assert!(mapper.dirty_page_li().is_empty(), "超限后不应残留脏页");
      assert_eq!(mapper.max_fp_id(), base_fp, "超限批次的段分配器必须回滚");
      assert_eq!(mapper.mapping_count(), 0, "超限批次的段映射不得残留");
      assert_eq!(mapper.tbl().len(), 0, "不得残留超限段的逻辑槽位");
      assert_eq!(mapper.free_page_cnt(), 0, "超限批释放的段不得留在空闲链表");
    }
    assert_eq!(branch.get_large(&tbl, b"too-big").await.unwrap(), None);
    assert_eq!(
      branch.get(&tbl, b"seed").await.unwrap(),
      Some(b"seed".to_vec()),
      "失败批不得影响已提交数据"
    );

    // 预留区间未被占用：后续小批大 value 仍可写，且只占实际段数。
    let value = pattern(SEGMENT_SIZE * 3);
    branch
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"small-big".to_vec(), value.clone(), 3).with_metadata(b"meta")],
      )
      .await
      .unwrap();
    assert_eq!(
      branch.segment_mapper(&tbl).unwrap().max_fp_id(),
      base_fp + 3,
      "重试只占用实际写入的 3 段"
    );
    let read = branch.get_large(&tbl, b"small-big").await.unwrap().unwrap();
    assert_eq!(read.value, value);
    assert_eq!(read.metadata, b"meta");

    // 重开 branch：预留上界与有效数据一致，超限批的段不被引用。
    let mut reopened = Store::new(branch.into_io(), PAGE_SIZE, RESTART_INTERVAL)
      .with_large_value(branch_layout, LARGE_THRESHOLD);
    reopened.open_branch(&tbl, "feature", 0).await.unwrap();
    assert_eq!(
      reopened.branch_segment_limit(),
      Some(limit_fp),
      "重开后预留上界不变"
    );
    assert_eq!(
      reopened.segment_mapper(&tbl).unwrap().max_fp_id(),
      base_fp + 3
    );
    assert_eq!(
      reopened
        .get_large(&tbl, b"small-big")
        .await
        .unwrap()
        .unwrap()
        .value,
      value
    );
    assert_eq!(
      reopened.get(&tbl, b"seed").await.unwrap(),
      Some(b"seed".to_vec())
    );
  });
}

/// `base_timestamp` 读失败（`write.rs` 注释专门强调的防御性路径）。
///
/// `fkv_failpoint` 目前只注册了写入侧的注入点（`WritePage` / `WriteSegments` 等），
/// 没有 `ReadPage`，因此这里用 SimStore 自带的「第 N 次逻辑 op」故障注入等价命中
/// 树下降读。setup 阶段不得消耗 ReadPage，所以第 1 次 ReadPage 必然是失败批
/// `base_timestamp` 的树下降：段批次已开场、尚未分配任何段，必须走到统一结算点
/// 回滚，而不是 `?` 提前返回把 segment MapperBatch 留在打开状态。
#[test]
fn put_large_base_timestamp_read_failure_rolls_back() {
  pollster::block_on(async {
    let _guard = FailpointGuard::acquire();
    let tbl = TableIdent::new("demo", 3).unwrap();
    let io = SimStore::new(PAGE_SIZE)
      .with_segment_config(SEGMENT_SIZE, SEGMENT_SHIFT)
      .with_fault(Fault {
        op: OpKind::ReadPage,
        at: 1,
        err: KvError::IoFail,
      });
    let mut store =
      Store::new(io, PAGE_SIZE, RESTART_INTERVAL).with_large_value(layout(), LARGE_THRESHOLD);

    let value_a = pattern(SEGMENT_SIZE * 3);
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"a".to_vec(), value_a.clone(), 1).with_metadata(b"meta-a")],
      )
      .await
      .unwrap();
    assert_eq!(
      store.page_io().op_count(OpKind::ReadPage),
      0,
      "setup 阶段不得消耗第 1 次 ReadPage 故障点"
    );
    let segment_writes_before = store.page_io().op_count(OpKind::WriteSegment);
    let max_fp_before = store.segment_mapper(&tbl).unwrap().max_fp_id();
    let mapping_before = store.segment_mapper(&tbl).unwrap().mapping_count();
    let root_before = store.root_id(&tbl);

    let value_b = pattern(SEGMENT_SIZE * 3);
    let err = store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"b".to_vec(), value_b.clone(), 2)],
      )
      .await
      .unwrap_err();
    assert_eq!(err, KvError::IoFail);

    // 阶段证据：失败恰好是第 1 次 ReadPage，且段写次数没有增加。
    assert_eq!(
      store.page_io().op_count(OpKind::ReadPage),
      1,
      "失败必须落在 base_timestamp 的树下降第一页"
    );
    assert_eq!(
      store.page_io().op_count(OpKind::WriteSegment),
      segment_writes_before,
      "base_timestamp 失败时不得落任何段"
    );
    assert_eq!(
      store.root_id(&tbl),
      root_before,
      "失败批不得推进 data 树 root"
    );
    {
      let mapper = store.segment_mapper(&tbl).unwrap();
      assert!(
        mapper.changed_page_li().is_empty(),
        "回滚后不应残留变更记录"
      );
      assert!(mapper.dirty_page_li().is_empty(), "回滚后不应残留脏页");
      assert_eq!(mapper.max_fp_id(), max_fp_before);
      assert_eq!(mapper.mapping_count(), mapping_before);
    }

    // 旧数据完好、失败批不可见，store 仍可续写。
    let read_a = store.get_large(&tbl, b"a").await.unwrap().unwrap();
    assert_eq!(read_a.value, value_a);
    assert_eq!(read_a.metadata, b"meta-a");
    assert_eq!(store.get_large(&tbl, b"b").await.unwrap(), None);

    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"b".to_vec(), value_b.clone(), 3).with_metadata(b"meta-b")],
      )
      .await
      .unwrap();
    let read_b = store.get_large(&tbl, b"b").await.unwrap().unwrap();
    assert_eq!(read_b.value, value_b);
    assert_eq!(read_b.metadata, b"meta-b");

    store.reopen(&tbl).await.unwrap();
    assert_eq!(
      store.get_large(&tbl, b"a").await.unwrap().unwrap().value,
      value_a
    );
    assert_eq!(
      store.get_large(&tbl, b"b").await.unwrap().unwrap().value,
      value_b
    );
  });
}
