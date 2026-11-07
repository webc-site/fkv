//! MT19937 / C++ key 生成器兼容测试。

use super::*;

/// MT19937 与 libstdc++ `std::mt19937(0)` 逐位一致（前 8 个输出取自 g++）。
#[test]
fn mt19937_matches_std_seed_zero() {
  let mut mt = Mt19937::new(0);
  let expect_li = [
    2_357_136_044u32,
    2_546_248_239,
    3_071_714_933,
    3_626_093_760,
    2_588_848_963,
    3_684_848_379,
    2_340_255_427,
    3_638_918_503,
  ];
  for expect in expect_li {
    assert_eq!(mt.next_u32(), expect);
  }
}

/// C++ 兼容 key 生成器：前 3 个 batch（每批 4 条）与 `std::mt19937(0)` 的
/// `Writer` 复刻脚本逐条一致（首批先编码、后续先步进）。
#[test]
fn cpp_key_gen_matches_writer_batches() {
  let mut key_gen = CppKeyGen::new(1_000_000);
  let expect_li: [&[(u64, bool)]; 3] = [
    &[(0, false), (1, true), (3, false), (7, false)],
    &[(13, false), (15, false), (16, false), (19, true)],
    &[(20, true), (23, false), (26, false), (30, false)],
  ];
  for expect in expect_li {
    key_gen.start_batch();
    for &want in expect {
      assert_eq!(key_gen.next_entry(), want);
    }
    key_gen.finish_batch();
  }
}

/// 回绕只在整批填完后执行，阈值是 `>`：`writing_key == max_key` 不归零，
/// 批内 key 可以超过 `max_key`（C++ 同款），越过之后下一批从头开始。
#[test]
fn cpp_key_gen_wraps_only_after_batch() {
  let mut key_gen = CppKeyGen::new(11);
  let keys_of = |key_gen: &mut CppKeyGen| {
    key_gen.start_batch();
    let key_li = (0..4).map(|_| key_gen.next_entry().0).collect::<Vec<_>>();
    key_gen.finish_batch();
    key_li
  };
  // 首批结束 writing_key = 11：`11 > 11` 为假，下一批继续从 11 前进。
  assert_eq!(keys_of(&mut key_gen), vec![0, 1, 3, 7]);
  assert_eq!(keys_of(&mut key_gen), vec![13, 15, 16, 19]);
  // 上一批结束 19 > 11 → 归零，这一批从 0 重新起步。
  assert_eq!(keys_of(&mut key_gen), vec![1, 4, 7, 11]);
}

/// `WriteLane::next_batch` 必须把 `start_batch` / `finish_batch` 括在批次两侧，
/// 否则首批的「先编码」语义会退化成和后续批次一样。
#[test]
fn cpp_write_lane_uses_writer_batch_semantics() {
  let mix = WriteMix {
    rng: RngMode::Cpp,
    ..WriteMix::default()
  };
  let mut lane = write_lane_li(1_000_000, 1, mix).remove(0);
  let batch_key_li: Vec<Vec<u64>> = (0..3)
    .map(|_| {
      lane
        .next_batch(4, mix, b"v")
        .iter()
        .map(|entry| decode_key(&entry.key).unwrap() as u64)
        .collect()
    })
    .collect();
  assert_eq!(
    batch_key_li,
    vec![vec![0, 1, 3, 7], vec![13, 15, 16, 19], vec![20, 23, 26, 30]]
  );
}

/// `--rng cpp` 端到端 smoke：读 / scan 不做网格存活断言也能跑完。
#[test]
fn cpp_rng_workload_smoke() {
  let mix = WriteMix {
    rng: RngMode::Cpp,
    ..WriteMix::default()
  };
  let metric_li = bench_workload(
    BenchArgs {
      rows: 64,
      value_size: 16,
      batch_size: 8,
      workload: Workload::WriteRead,
      mix,
      plan: ReadPlan {
        partitions: 1,
        read_per_part: 2,
      },
      test_secs: 0.0,
      write_batchs: 0,
      scan_bytes: 0,
      ttl_ms: 0,
    },
    None,
  );
  assert_latency_metric_li(&metric_li, "put");
  assert_latency_metric_li(&metric_li, "get");
  // 窗口指标齐全。
  for suffix in WINDOW_SUFFIX_LI {
    assert!(
      metric_li
        .iter()
        .any(|(name, ..)| name == &format!("put_{suffix}")),
      "缺少 put_{suffix}"
    );
    assert!(
      metric_li
        .iter()
        .any(|(name, ..)| name == &format!("get_{suffix}")),
      "缺少 get_{suffix}"
    );
  }
}
