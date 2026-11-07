//! 写流水 / 分区 / key 编码测试。

use super::*;

/// 写请求按**分区内序列位**编码；delete 目标与 upsert 同属一个分区。
#[test]
fn write_entries_stay_in_partition() {
  let mix = WriteMix {
    key_interval: 1,
    del_ratio: 4,
    ..WriteMix::default()
  };
  // 分区 1/4：序列位 3 是 delete 位，目标是序列位 0 → 全局 key 1。
  let del = write_entry(1, 4, 3, mix, b"v");
  assert_eq!(del.key, key_bytes(1).to_vec());
  assert!(del.op.is_delete());
  // 序列位 2 → 全局 key 1 + 4 * 2 = 9。
  let upsert = write_entry(1, 4, 2, mix, b"v");
  assert_eq!(upsert.key, key_bytes(9).to_vec());
  assert_eq!(upsert.value, b"v".to_vec());
  assert!(!upsert.op.is_delete());
  // 稀疏 key：序列位 2、interval 3 → key (1 + 4 * 2) * 3。
  let sparse = write_entry(
    1,
    4,
    2,
    WriteMix {
      key_interval: 3,
      del_ratio: 4,
      ..WriteMix::default()
    },
    b"v",
  );
  assert_eq!(sparse.key, key_bytes(27).to_vec());
}

/// `--rng cpp` 写的是 `[0, rows)` 原始 key，读 / scan 不能把
/// `key_interval` 当成 key 步长，否则会去查从未写过的 key。
#[test]
fn probe_key_uses_grid_interval_only_for_grid_rng() {
  let cpp = WriteMix {
    rng: RngMode::Cpp,
    key_interval: 4,
    del_ratio: 4,
    ..WriteMix::default()
  };
  assert_eq!(probe_key_bytes(0, cpp), key_bytes(0));
  assert_eq!(probe_key_bytes(999_999, cpp), key_bytes(999_999));

  let grid = WriteMix {
    rng: RngMode::Grid,
    key_interval: 4,
    ..WriteMix::default()
  };
  assert_eq!(probe_key_bytes(7, grid), key_bytes(28));
}

/// `--batch-size` 是单分区单请求条数：`partitions 4 / batch 8192` 下每个
/// 分区请求恰好 8192 条（对齐 C++ `Writer` 每请求 `batch_size` 条）。
#[test]
fn write_batch_is_per_partition_entries() {
  let mix = WriteMix::default();
  let mut lane = WriteLane {
    part: 2,
    step: 4,
    count: 8192,
    pos: 0,
    key_gen: None,
  };
  let batch = lane.next_batch(8192, mix, b"v");
  assert_eq!(batch.len(), 8192);
  assert_eq!(lane.pos, 0, "写满一遍后游标回到 0");
  // 所有 key 都落在本分区：全局下标 % 4 == 2。
  for (i, entry) in batch.iter().enumerate() {
    assert_eq!(decode_key(&entry.key), Some(2 + 4 * i));
  }
  // 写一遍的分区批数 = ceil(count / batch_size)。
  assert_eq!(once_batch_num(8192, 8192), 1);
  assert_eq!(once_batch_num(8193, 8192), 2);
  assert_eq!(once_batch_num(0, 8192), 0);
}

/// 覆盖写游标越过分区末尾回绕，且每个请求仍是 `batch_size` 条。
#[test]
fn write_lane_wraps_within_partition() {
  let mix = WriteMix::default();
  let mut lane = WriteLane {
    part: 0,
    step: 1,
    count: 5,
    pos: 3,
    key_gen: None,
  };
  let batch = lane.next_batch(7, mix, b"v");
  assert_eq!(batch.len(), 7);
  let index_li: Vec<usize> = batch
    .iter()
    .map(|entry| decode_key(&entry.key).unwrap())
    .collect();
  // 3,4 越界回绕到 0..5；同一 batch 内允许重复 key（C++ 回绕同款）。
  assert_eq!(index_li, vec![3, 4, 0, 1, 2, 3, 4]);
}

/// 写流水按分区切分：各分区 key 数之和 == rows，Once 模式批数按分区累加。
#[test]
fn write_lanes_cover_rows() {
  let lane_li = write_lane_li(10, 3, WriteMix::default());
  assert_eq!(lane_li.len(), 3);
  let total: usize = lane_li.iter().map(|lane| lane.count).sum();
  assert_eq!(total, 10);
  assert_eq!(lane_li[0].step, 3);
  let batch_num: usize = lane_li
    .iter()
    .map(|lane| once_batch_num(lane.count, 4))
    .sum();
  // 4 / 3 / 3 个 key，batch=4 → 1 + 1 + 1。
  assert_eq!(batch_num, 3);
  // rows < partitions 的空分区仍保留流水（runner 跳过）。
  assert_eq!(write_lane_li(1, 4, WriteMix::default()).len(), 4);
  assert_eq!(write_lane_li(1, 4, WriteMix::default())[3].count, 0);
}

/// `--write-batchs` 是全系统预算：并发流水合计认领恰好 `batch_num` 个 batch。
#[test]
fn claim_batch_budget_is_global() {
  let budget = AtomicU64::new(0);
  assert!(claim_batch(&budget, 3));
  assert!(claim_batch(&budget, 3));
  assert!(claim_batch(&budget, 3));
  assert!(!claim_batch(&budget, 3));
  assert!(!claim_batch(&budget, 0));
  assert_eq!(budget.load(Ordering::Relaxed), 5);
}

/// 分区 key 数按 `index % partitions` 精确划分，且各分区之和 == rows。
#[test]
fn partition_key_num_covers_rows() {
  let total: usize = (0..3).map(|part| partition_key_num(10, 3, part)).sum();
  assert_eq!(total, 10);
  assert_eq!(partition_key_num(10, 3, 0), 4);
  assert_eq!(partition_key_num(10, 3, 1), 3);
  assert_eq!(partition_key_num(10, 3, 2), 3);
  // key 数少于分区数时，空分区计 0。
  assert_eq!(partition_key_num(1, 4, 0), 1);
  assert_eq!(partition_key_num(1, 4, 3), 0);
}

#[test]
fn key_bytes_are_big_endian_and_ordered() {
  // 大端编码保证字典序与数值序一致（小端会在 255/256 处反转）。
  assert_eq!(
    key_bytes(0x0102_0304).to_vec(),
    vec![0, 0, 0, 0, 1, 2, 3, 4]
  );
  assert!(key_bytes(1) < key_bytes(2));
  assert!(key_bytes(255) < key_bytes(256));
}

/// `delete-only` 的计量序列只发 delete，key 与 `Once` 的网格序列一一对应，
/// 越过分区末尾按同一回绕规则回到 0。
#[test]
fn delete_batch_is_delete_only_and_wraps_like_once() {
  let mix = WriteMix::default();
  let mut lane = WriteLane {
    part: 1,
    step: 4,
    count: 3,
    pos: 0,
    key_gen: None,
  };
  let batch = lane.next_delete_batch(5, mix);
  assert_eq!(batch.len(), 5);
  assert!(batch.iter().all(|entry| entry.op.is_delete()));
  let index_li: Vec<usize> = batch
    .iter()
    .map(|entry| decode_key(&entry.key).unwrap())
    .collect();
  // 分区 1/4 的序列位 0,1,2 → 全局 key 1,5,9；随后回绕到 1,5。
  assert_eq!(index_li, vec![1, 5, 9, 1, 5]);
  assert_eq!(lane.pos, 2);
}

/// 全系统 delete 预算 = 各分区 `Once` 批数之和；`--write-batchs` 可覆盖。
#[test]
fn delete_batch_num_covers_once_per_partition() {
  assert_eq!(delete_batch_num(10, 3, 4), 3);
  assert_eq!(delete_batch_num(64, 1, 8), 8);
  assert_eq!(delete_batch_num(64, 2, 8), 8);
  assert_eq!(delete_batch_num(1, 4, 8), 1);
}
