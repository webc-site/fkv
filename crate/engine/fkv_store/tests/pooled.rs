//! Pooled 分配器（对齐 C++ `data_append_mode=false`）在 Store 层的接线验收：
//! 退役页回收、reopen 后空闲池重建，以及 append 模式的对照。

use std::time::{Duration, Instant};

use fkv_btree::BatchEntry;
use fkv_io::{FileIo, ManifestIo};
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_recovery::ReplayState;
use fkv_store::{AllocMode, Store};
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, TableIdent};

const PAGE_SIZE: usize = 512;
const RESTART: u16 = 8;
/// 段大小与每文件段槽位：9000 B 的大 value 占 3 段。
const SEGMENT_SIZE: usize = 4096;
const SEGMENT_SHIFT: u8 = 2;
const LARGE_THRESHOLD: usize = 1024;

fn batch(tag: u8, count: u32, ts: u64) -> Vec<BatchEntry> {
  (0..count)
    .map(|i| {
      let mut value = vec![tag; 32];
      value[..4].copy_from_slice(&i.to_be_bytes());
      BatchEntry::upsert(format!("k{i:05}").into_bytes(), value, ts)
    })
    .collect()
}

fn max_fp_id(store: &Store<MemStore>, tbl: &TableIdent) -> u64 {
  store.mapper(tbl).expect("table mapper").max_fp_id()
}

fn new_store(mode: AllocMode) -> Store<MemStore> {
  Store::new(
    MemStore::with_pages_per_file(PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT),
    PAGE_SIZE,
    RESTART,
  )
  .with_pages_per_file_shift(DEFAULT_PAGES_PER_FILE_SHIFT)
  .with_alloc_mode(mode)
}

/// 启用大 value / segment 的 store：段 mapper 由 `put_large` 懒建。
fn new_large_store(mode: AllocMode) -> Store<MemStore> {
  Store::new(
    MemStore::with_pages_per_file(PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT)
      .with_segment_config(SEGMENT_SIZE, SEGMENT_SHIFT),
    PAGE_SIZE,
    RESTART,
  )
  .with_pages_per_file_shift(DEFAULT_PAGES_PER_FILE_SHIFT)
  .with_alloc_mode(mode)
  .with_large_value(
    SegmentLayout::new(SEGMENT_SIZE as u32, SEGMENT_SHIFT).unwrap(),
    LARGE_THRESHOLD,
  )
}

#[compio::test]
async fn pooled_reuses_retired_pages_append_does_not() {
  let tbl = TableIdent::new("pooled", 1).unwrap();
  let mut pooled = new_store(AllocMode::Pooled);
  let mut append = new_store(AllocMode::Append);

  pooled.put(&tbl, &batch(1, 300, 1)).await.unwrap();
  append.put(&tbl, &batch(1, 300, 1)).await.unwrap();
  let pooled_first = max_fp_id(&pooled, &tbl);
  let append_first = max_fp_id(&append, &tbl);
  assert_eq!(pooled_first, append_first);

  // 第一次覆盖写仍在消费 high-water（上一批的旧页尚未退役回收），
  // 第二次起应完全从空闲链取页，max_fp_id 不再增长。
  for round in 2..=12u64 {
    pooled
      .put(&tbl, &batch(round as u8, 300, round))
      .await
      .unwrap();
    append
      .put(&tbl, &batch(round as u8, 300, round))
      .await
      .unwrap();
  }
  let pooled_last = max_fp_id(&pooled, &tbl);
  let append_last = max_fp_id(&append, &tbl);

  assert!(
    pooled_last <= pooled_first * 2,
    "pooled should stop growing after one overwrite: first={pooled_first}, last={pooled_last}"
  );
  assert!(
    append_last > append_first * 10,
    "append should keep consuming high-water: first={append_first}, last={append_last}"
  );
  assert!(pooled_last * 3 < append_last);

  // 复用页后数据仍是最后一轮内容。
  for i in [0u32, 17, 299] {
    let value = pooled
      .get(&tbl, format!("k{i:05}").as_bytes())
      .await
      .unwrap()
      .expect("value");
    assert_eq!(&value[..4], &i.to_be_bytes());
    assert!(value[4..].iter().all(|b| *b == 12));
  }
}

#[compio::test]
async fn pooled_reopen_rebuilds_free_list() {
  let tbl = TableIdent::new("pooled", 2).unwrap();
  let mut store = new_store(AllocMode::Pooled);
  store.put(&tbl, &batch(1, 200, 1)).await.unwrap();
  // 覆盖写把旧页退役，但尚未回收就关闭：reopen 必须从映射空洞重建空闲池。
  store.put(&tbl, &batch(2, 200, 2)).await.unwrap();
  let before = max_fp_id(&store, &tbl);

  let io = store.into_io();
  let mut reopened = Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(DEFAULT_PAGES_PER_FILE_SHIFT)
    .with_alloc_mode(AllocMode::Pooled);
  reopened.reopen(&tbl).await.unwrap();
  assert_eq!(max_fp_id(&reopened, &tbl), before);
  for i in [0u32, 100, 199] {
    let value = reopened
      .get(&tbl, format!("k{i:05}").as_bytes())
      .await
      .unwrap()
      .expect("value after reopen");
    assert_eq!(&value[..4], &i.to_be_bytes());
    assert!(value[4..].iter().all(|b| *b == 2));
  }

  // reopen 后的空闲池覆盖 `0..max_fp_id` 中所有未引用页，下一轮覆盖写不再抬
  // high-water。先做一轮触发回收，第二轮才稳定。
  reopened.put(&tbl, &batch(3, 200, 3)).await.unwrap();
  let after_first = max_fp_id(&reopened, &tbl);
  reopened.put(&tbl, &batch(4, 200, 4)).await.unwrap();
  let after_second = max_fp_id(&reopened, &tbl);
  assert!(
    after_second <= after_first.max(before),
    "reopen free list should cap growth: before={before}, first={after_first}, second={after_second}"
  );
}

/// pooled-02：段 mapper 恒为 `Append`（对齐 C++ `EnsureSegmentMapper`），与
/// data 侧 `alloc_mode` 无关。
///
/// 覆盖写大 value 时旧段页直接作废、**不进 retired 队列**（否则该队列在
/// 「覆盖/删大 value」负载下只增不减，是长跑内存泄漏）；段分配 id 与 `Append`
/// store 逐轮完全一致（磁盘 file id 行为不变，故池化对段侧本就是退化的）。
#[compio::test]
async fn pooled_segment_mapper_stays_append() {
  let tbl = TableIdent::new("pooled", 3).unwrap();
  let mut pooled = new_large_store(AllocMode::Pooled);
  let mut append = new_large_store(AllocMode::Append);
  let mut last_segment_max = 0;
  let mut data_max = Vec::new();
  for round in 1..=8u64 {
    let batch = vec![BatchEntry::upsert(
      b"big".to_vec(),
      vec![round as u8; 9000],
      round,
    )];
    pooled.put_large(&tbl, &batch).await.unwrap();
    append.put_large(&tbl, &batch).await.unwrap();

    let segment = pooled.segment_mapper(&tbl).expect("segment mapper");
    assert_eq!(
      segment.alloc_mode(),
      AllocMode::Append,
      "段 mapper 必须恒为 Append round={round}"
    );
    assert_eq!(
      segment.retired_len(),
      0,
      "Append 段 mapper 不得堆积 retired 页 round={round}"
    );
    assert!(
      segment.max_fp_id() >= last_segment_max,
      "段分配 id 不得回退 round={round}: {} -> {}",
      last_segment_max,
      segment.max_fp_id()
    );
    assert_eq!(
      segment.max_fp_id(),
      append.segment_mapper(&tbl).unwrap().max_fp_id(),
      "Pooled 与 Append 的段 file id 轨迹必须一致 round={round}"
    );
    last_segment_max = segment.max_fp_id();
    data_max.push(max_fp_id(&pooled, &tbl));
  }
  assert!(last_segment_max > 0, "段侧必须真的分配过物理页");
  // data 侧仍按 Pooled 复用：段侧（Append）线性增长，data 侧必须收敛。
  assert_eq!(
    pooled.mapper(&tbl).unwrap().alloc_mode(),
    AllocMode::Pooled,
    "data mapper 必须仍是 Pooled"
  );
  assert!(
    *data_max.last().unwrap() <= data_max[0] * 2,
    "Pooled data 侧不得随覆盖写线性增长: {data_max:?}"
  );
}

/// pooled-04：`compact_file` 退役的物理页必须立即回池，不必等下一次
/// `maintain` / `put`。
///
/// 修复前唯一回收点在 `put`，纯维护负载（只 compact、不写）下 `max_fp_id`
/// 每轮翻倍；修复后每轮 `compact_file` 返回时 `retired_len()` 已归零且高位
/// 不再增长。
#[test]
fn pooled_compact_file_reclaims_retired_pages() {
  pollster::block_on(async {
    let tbl = TableIdent::new("pooled", 4).unwrap();
    let mut store = new_store(AllocMode::Pooled);
    store.put(&tbl, &batch(1, 400, 1)).await.unwrap();

    let mut baseline = 0u64;
    for round in 0..4u64 {
      // 找一个仍有 live 页的文件 compact（等价 issue 探针里的反复 compact_file）。
      let file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
      let mut moved = 0;
      for file_id in &file_li {
        moved = store
          .compact_file(&tbl, *file_id, DEFAULT_PAGES_PER_FILE_SHIFT)
          .await
          .unwrap();
        if moved > 0 {
          break;
        }
      }
      assert!(moved > 0, "round={round}: 必须找到可搬迁的文件");
      assert_eq!(
        store.mapper(&tbl).unwrap().retired_len(),
        0,
        "round={round}: compact_file 必须立即回收 retired 页"
      );
      let now = max_fp_id(&store, &tbl);
      if round == 0 {
        baseline = now;
      } else {
        assert!(
          now <= baseline,
          "round={round}: 回收后高位不得继续增长 now={now} baseline={baseline}"
        );
      }
    }
    let value = store
      .get(&tbl, b"k00000")
      .await
      .unwrap()
      .expect("live value");
    assert!(value[4..].iter().all(|b| *b == 1), "数据不得被搬迁破坏");
  });
}

/// pooled-04：在途快照仍引用旧页时，`compact_file` 不得把退役页回池；
/// 快照释放后由下一轮 `maintain` 回收。
///
/// 这是 [`Store::reclaim_retired_now`] 以 `min_live_generation()` 为水位
/// （pooled-01 协议）的回归护栏：回池过早会让旧快照读到被重写的物理页。
#[test]
fn pooled_maintain_waits_for_snapshot_before_reclaim() {
  pollster::block_on(async {
    let tbl = TableIdent::new("pooled", 6).unwrap();
    let mut store = new_store(AllocMode::Pooled);
    store.put(&tbl, &batch(1, 400, 1)).await.unwrap();

    let mut baseline = 0u64;
    for round in 0..4u64 {
      // 旧快照跨 compact 存活：它按旧 FilePageId 读，退役页必须留在池外。
      let snapshot = store.snapshot(&tbl).expect("snapshot");
      let file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
      let mut moved = 0;
      for file_id in &file_li {
        moved = store
          .compact_file(&tbl, *file_id, DEFAULT_PAGES_PER_FILE_SHIFT)
          .await
          .unwrap();
        if moved > 0 {
          break;
        }
      }
      assert!(moved > 0, "round={round}: 必须找到可搬迁的文件");
      assert!(
        store.mapper(&tbl).unwrap().retired_len() > 0,
        "round={round}: 在途快照必须阻止 compact 回收"
      );
      drop(snapshot);

      store.maintain(&tbl).await.unwrap();
      assert_eq!(
        store.mapper(&tbl).unwrap().retired_len(),
        0,
        "round={round}: 快照释放后 maintain 必须回收 retired 页"
      );
      let now = max_fp_id(&store, &tbl);
      if round == 0 {
        baseline = now;
      } else {
        assert!(
          now <= baseline,
          "round={round}: 回收后高位不得继续增长 now={now} baseline={baseline}"
        );
      }
    }
    let value = store
      .get(&tbl, b"k00000")
      .await
      .unwrap()
      .expect("live value");
    assert!(value[4..].iter().all(|b| *b == 1), "数据不得被搬迁破坏");
  });
}

/// perf-12：长快照存活期间多批覆盖写的退役页在队列里持续堆积；快照释放后
/// 一次 `reclaim`（`maintain` 末尾）必须把整个队列弹空，而不是逐批全量扫描。
#[test]
fn pooled_reclaims_whole_queue_after_long_snapshot_drop() {
  pollster::block_on(async {
    let tbl = TableIdent::new("pooled", 9).unwrap();
    let mut store = new_store(AllocMode::Pooled);
    store.put(&tbl, &batch(1, 200, 1)).await.unwrap();

    let snapshot = store.snapshot(&tbl).expect("snapshot");
    store.put(&tbl, &batch(2, 200, 2)).await.unwrap();
    let one_batch = store.mapper(&tbl).unwrap().retired_len();
    assert!(one_batch > 0, "快照存活时覆盖写必须留下退役页");

    // 后续每批覆盖继续往队列尾部追加，队列长度随批数增长（旧实现的扫描量）。
    let mut scanned_per_put = 0u64;
    for round in 3..=6u64 {
      let before = store.mapper(&tbl).unwrap().retired_scan_cnt();
      store
        .put(&tbl, &batch(round as u8, 200, round))
        .await
        .unwrap();
      scanned_per_put = store.mapper(&tbl).unwrap().retired_scan_cnt() - before;
    }
    let queued = store.mapper(&tbl).unwrap().retired_len();
    assert!(
      queued >= one_batch * 4,
      "长快照期间退役页必须随批次累积: one_batch={one_batch} queued={queued}"
    );
    // perf-12 的写路径复现：开批前的 reclaim 只检查队头一条就停，扫描量不随
    // 队列长度增长；旧实现 `retain` 每批会扫完 queued 条。
    assert!(
      scanned_per_put <= 1,
      "快照存活时每批回收只检查队头一条: scanned={scanned_per_put} queued={queued}"
    );
    drop(snapshot);

    // maintain 末尾的 reclaim 在批次之外执行：快照已释放，一次调用回收整个队列。
    store.maintain(&tbl).await.unwrap();
    assert_eq!(
      store.mapper(&tbl).unwrap().retired_len(),
      0,
      "快照释放后一次 reclaim 必须回收整个队列"
    );
    let value = store
      .get(&tbl, b"k00000")
      .await
      .unwrap()
      .expect("live value");
    assert!(value[4..].iter().all(|b| *b == 6), "数据不得被回收破坏");
  });
}

/// pooled-04：`truncate` 释放整表物理页后必须立即回池，下一次写入复用原
/// 物理页而不是继续抬 `max_fp_id`。
#[test]
fn pooled_truncate_reclaims_retired_pages() {
  pollster::block_on(async {
    let tbl = TableIdent::new("pooled", 7).unwrap();
    let mut store = new_store(AllocMode::Pooled);
    store.put(&tbl, &batch(1, 400, 1)).await.unwrap();
    let before = max_fp_id(&store, &tbl);
    assert!(before > 0, "首轮写入必须分配物理页");

    store.truncate(&tbl).await.unwrap();
    assert_eq!(
      store.mapper(&tbl).unwrap().retired_len(),
      0,
      "truncate 必须把退役页还回空闲池"
    );
    assert_eq!(max_fp_id(&store, &tbl), before, "回收不改变分配高位");

    // 截断后重写同样规模的数据：页全部来自空闲链 / 空闲池，高位不增长。
    store.put(&tbl, &batch(2, 400, 2)).await.unwrap();
    assert_eq!(
      max_fp_id(&store, &tbl),
      before,
      "truncate 回池后写入必须复用原物理页"
    );
    let value = store
      .get(&tbl, b"k00000")
      .await
      .unwrap()
      .expect("value after rewrite");
    assert!(value[4..].iter().all(|b| *b == 2));
  });
}

/// pooled-04：`truncate` 遇到在途快照时同样不得回池；快照释放后下一批次
/// （`put` 开批前）回收。
#[test]
fn pooled_truncate_waits_for_snapshot_before_reclaim() {
  pollster::block_on(async {
    let tbl = TableIdent::new("pooled", 8).unwrap();
    let mut store = new_store(AllocMode::Pooled);
    store.put(&tbl, &batch(1, 50, 1)).await.unwrap();

    let snapshot = store.snapshot(&tbl).expect("snapshot");
    store.truncate(&tbl).await.unwrap();
    assert!(
      store.mapper(&tbl).unwrap().retired_len() > 0,
      "在途快照引用旧页，truncate 不得回收"
    );
    drop(snapshot);

    store.put(&tbl, &batch(2, 1, 2)).await.unwrap();
    assert_eq!(
      store.mapper(&tbl).unwrap().retired_len(),
      0,
      "快照释放后下一次写入必须回收"
    );
  });
}

/// pooled-04：`clean_expired` 退役的物理页必须在同一次调用内回池。
///
/// TTL 清理是纯维护负载（无 `put` 兜底回收），循环 N 轮后 `retired_len()`
/// 必须归零且 `max_fp_id()` 不再增长。
#[test]
fn pooled_clean_expired_reclaims_retired_pages() {
  pollster::block_on(async {
    let tbl = TableIdent::new("pooled", 5).unwrap();
    let mut store = new_store(AllocMode::Pooled);
    // 常驻条目保证 data / TTL 树根非空，`clean_expired` 不会走空表短路。
    store.put_at(&tbl, &batch(0, 1, 0), 0).await.unwrap();

    let mut baseline = 0u64;
    for round in 1..=4u64 {
      let expiring: Vec<BatchEntry> = (0..200u32)
        .map(|i| {
          let mut entry =
            BatchEntry::upsert(format!("e{i:05}").into_bytes(), vec![b'e'; 32], round);
          entry.expire_ts = 10;
          entry
        })
        .collect();
      store.put_at(&tbl, &expiring, round).await.unwrap();

      let stat = store.clean_expired(&tbl, 100).await.unwrap();
      assert_eq!(stat.deleted, 200, "round={round}: 过期项必须被物理删除");
      assert_eq!(
        store.mapper(&tbl).unwrap().retired_len(),
        0,
        "round={round}: clean_expired 必须回收 retired 页"
      );
      let now = max_fp_id(&store, &tbl);
      if round == 1 {
        baseline = now;
      } else {
        assert!(
          now <= baseline,
          "round={round}: TTL 清理不得持续抬高位 now={now} baseline={baseline}"
        );
      }
    }
  });
}

/// pooled-06：branch reopen 不得按 `max_fp_id` 逐页播种空闲池。
///
/// 父 branch 只写一条记录，`create_branch` 会把 branch manifest 的 `max_fp_id`
/// 抬到 1024 个文件（默认 2^18 页/文件 = 2.68 亿个 page 位）。旧实现为每个
/// 未引用页位压一条空闲链目（GB 级内存 / 数十秒）；区间记账后逻辑空闲页数
/// 不变，但条目数只与真实空洞段数同阶。
#[test]
fn pooled_branch_reopen_keeps_free_pool_lazy() {
  pollster::block_on(async {
    let tbl = TableIdent::new("pooled", 9).unwrap();
    let mut parent = new_store(AllocMode::Pooled);
    parent.put(&tbl, &batch(1, 1, 1)).await.unwrap();
    let info = parent.create_branch(&tbl, "feature").await.unwrap();
    let branch_max_fp = 1024u64 << DEFAULT_PAGES_PER_FILE_SHIFT;
    let bytes = parent
      .page_io()
      .read_manifest_file(&tbl, &info.name)
      .await
      .unwrap()
      .expect("branch manifest");
    assert_eq!(
      ReplayState::from_snapshot(&bytes).unwrap().max_fp_id,
      branch_max_fp,
      "夹具：branch manifest 高水位必须落在预留块起点"
    );

    let io = parent.into_io();
    let start = Instant::now();
    let mut branch = Store::new(io, PAGE_SIZE, RESTART)
      .with_pages_per_file_shift(DEFAULT_PAGES_PER_FILE_SHIFT)
      .with_alloc_mode(AllocMode::Pooled);
    branch.open_branch(&tbl, "feature", 0).await.unwrap();
    let elapsed = start.elapsed();

    let mapper = branch.mapper(&tbl).expect("mapper");
    assert_eq!(mapper.max_fp_id(), branch_max_fp);
    // feature 拥有 file 1..=1024，file 0 是父 branch 的空间：空闲页只差一个
    // 文件（父文件）的页位。
    assert!(
      mapper.pooled_free_len() >= branch_max_fp - (1 << DEFAULT_PAGES_PER_FILE_SHIFT),
      "逻辑空闲页数必须覆盖当前 branch 的全部未引用页位: {}",
      mapper.pooled_free_len()
    );
    // 单一连续区间 + 父分支文件裁剪：绝不随 2.68 亿页位展开。
    assert!(
      mapper.pooled_free_range_cnt() <= 4,
      "reopen 空闲池不得逐页物化: ranges={}",
      mapper.pooled_free_range_cnt()
    );
    assert!(
      elapsed < Duration::from_secs(5),
      "branch reopen 应为毫秒级，实际 {elapsed:?}"
    );

    // 重建后的池仍可正常分配 / 读写。
    branch
      .put(&tbl, &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)])
      .await
      .unwrap();
    assert_eq!(branch.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
    let inherited = branch
      .get(&tbl, b"k00000")
      .await
      .unwrap()
      .expect("inherited value");
    assert_eq!(inherited.len(), 32);
    assert!(
      inherited[4..].iter().all(|byte| *byte == 1),
      "继承的父数据仍可读"
    );
  });
}
