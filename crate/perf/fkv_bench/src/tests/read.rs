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

/// redb 口径：总读次数固定为 `--read-count`，跨全部读链均分（余数给前几条）。
#[test]
fn read_count_is_shared_across_lanes() {
  let lane_li = read_lane_li_with_count(1_000_000, 1, 8, 1_000_000);
  assert_eq!(lane_li.len(), 8);
  let total: u64 = lane_li.iter().filter_map(|lane| lane.reads).sum();
  assert_eq!(total, 1_000_000);
  assert_eq!(lane_li[0].reads, Some(125_000));

  // 余数按链顺序分配；None 保持历史语义（每链读一遍）。
  let lane_li = read_lane_li_with_count(1000, 1, 3, 10);
  assert_eq!(
    lane_li.iter().map(|lane| lane.reads).collect::<Vec<_>>(),
    vec![Some(4), Some(3), Some(3)]
  );
  assert!(
    read_lane_li(1000, 1, 3)
      .iter()
      .all(|lane| lane.reads.is_none())
  );
}
