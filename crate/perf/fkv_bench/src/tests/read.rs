//! 读流水测试。

use super::*;

/// 并发读链数 = `partitions * read_per_part`（对齐 C++ 读者数），
/// 每链读本分区全部 key，总读次数仍为 `rows * read_per_part`。
#[test]
fn read_lanes_match_cpp_reader_num() {
  let lane_li = read_lane_li(16, 2, 3);
  assert_eq!(lane_li.len(), 6);
  let total: usize = lane_li.iter().map(|lane| lane.count).sum();
  assert_eq!(total, 16 * 3);
  assert!(lane_li.iter().all(|lane| lane.step == 2));
  assert_eq!(lane_li.iter().filter(|lane| lane.part == 0).count(), 3);
  // 分区内随机下标必须落在该分区：`part + step * i < rows`。
  for lane in &lane_li {
    assert!(lane.part + lane.step * (lane.count - 1) < 16);
  }
  // 链种子互不相同，并发下可复现。
  let mut seed_li: Vec<u64> = lane_li.iter().map(|lane| lane.seed).collect();
  seed_li.sort_unstable();
  seed_li.dedup();
  assert_eq!(seed_li.len(), 6);
  assert_ne!(lane_seed(0), lane_seed(1));
  // 空分区不建链。
  assert_eq!(read_lane_li(1, 4, 2).len(), 2);
}
