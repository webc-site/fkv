//! SimStore 故障矩阵：表驱动覆盖 `ReadPage` / `SyncData` / `WriteSegment` /
//! `AppendManifest` / `SwitchManifest` 的 `at = 1` 注入。
//!
//! 现有 `sim.rs` 只注入 `WritePage@2` / `FlushPage@1` / `ReadPage@0`，且
//! 「命中那次不得改动状态」只在 `WritePage` 上断言过一次。这里把该不变量
//! 推广成三条通用断言：
//!
//! 1. 命中那次返回注入错误；
//! 2. 命中那次不改变 durable / volatile：
//!    - 命中前后 `snapshot()`（durable + volatile 合并视图）逐项相等，抓 volatile
//!      改动（如 `SyncData` 提前刷页、`WriteSegment` 提前写段）；
//!    - `crash()` 丢弃 volatile 后，盘面等于「同前缀干净运行 + `crash()`」的
//!      [`Snapshot`]，抓 durable 改动（如 manifest 先写后报错）；
//!    - `op_log_li()` 在命中处只追加一条记录：失败 op 不得展开子 op
//!      （如 `SyncData` 命中后不得出现 `FlushPage` / `FlushSegment`）；
//! 3. `clear_fault_li()`（对应 C++ `FailPoint::Disarm`）后重试同一逻辑 op 成功，
//!    最终 `crash()` 盘面等于「前缀 + 该 op 成功 + 收尾」的干净运行。
//!
//! 状态比较复用 `SimStore::snapshot()`；op 序列比较复用 `op_log_li()` 的前缀，
//! 不另建 replay。C++ 对照：`eloqstore/tests/persist.cpp` 的
//! `ArmOnce("SyncFiles")` 用例——首次失败、Disarm 后重试成功。
//!
//! [`Snapshot`]: fkv_io_sim::Snapshot

use fkv_error::KvError;
use fkv_io::{ManifestIo, PageIo, SegmentIo};
use fkv_io_sim::{Fault, Op, OpKind, OpRecord, SimStore};
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 8;
const SEGMENT_SIZE: usize = 16;
const INJECTED: KvError = KvError::IoFail;

fn table() -> TableIdent {
  TableIdent::new("fault_matrix", 1).unwrap()
}

fn new_store() -> SimStore {
  SimStore::new(PAGE_SIZE).with_segment_config(SEGMENT_SIZE, 2)
}

/// 矩阵中的一步逻辑操作。只包含建立前置状态所需的最小集合。
#[derive(Debug, Clone, Copy)]
enum Step {
  /// `expect = Some(byte)` 时校验读到的内容（用于重试成功后的语义校验）。
  ReadPage {
    fp: u64,
    expect: Option<u8>,
  },
  WritePage {
    fp: u64,
    byte: u8,
  },
  SyncData,
  WriteSegment {
    fp: u64,
    bytes: &'static [u8],
  },
  AppendManifest {
    record: &'static [u8],
  },
  SwitchManifest {
    record: &'static [u8],
  },
}

impl Step {
  /// 该步骤在成功路径上产生的第一条 op 记录；用于校验命中 / 重试的记录参数。
  fn op(self, table: &TableIdent) -> Op {
    match self {
      Self::ReadPage { fp, .. } => Op::ReadPage {
        tbl: table.clone(),
        file_page_id: fp,
      },
      Self::WritePage { fp, .. } => Op::WritePage {
        tbl: table.clone(),
        file_page_id: fp,
        len: PAGE_SIZE,
      },
      Self::SyncData => Op::SyncData { tbl: table.clone() },
      Self::WriteSegment { fp, bytes } => Op::WriteSegment {
        tbl: table.clone(),
        segment_fp_id: fp,
        len: bytes.len(),
      },
      Self::AppendManifest { record } => Op::AppendManifest {
        tbl: table.clone(),
        len: record.len(),
      },
      Self::SwitchManifest { record } => Op::SwitchManifest {
        tbl: table.clone(),
        len: record.len(),
      },
    }
  }
}

async fn apply(store: &SimStore, step: Step, context: &str) -> Result<(), KvError> {
  let table = table();
  match step {
    Step::ReadPage { fp, expect } => {
      let mut dst = [0u8; PAGE_SIZE];
      let result = store.read_page(&table, fp, &mut dst).await;
      if let (Ok(()), Some(byte)) = (&result, expect) {
        assert_eq!(
          dst, [byte; PAGE_SIZE],
          "{context}: read_page(fp={fp}) 内容与期望不符"
        );
      }
      result
    }
    Step::WritePage { fp, byte } => {
      let bytes = [byte; PAGE_SIZE];
      store.write_page(&table, fp, &bytes).await
    }
    Step::SyncData => store.sync_data(&table).await,
    Step::WriteSegment { fp, bytes } => store.write_segment(&table, fp, bytes).await,
    Step::AppendManifest { record } => store.append_manifest(&table, record).await,
    Step::SwitchManifest { record } => store.switch_manifest(&table, record).await,
  }
}

async fn run_step_li(store: &SimStore, step_li: &[Step], case: &str, phase: &str) {
  let context = format!("{case} [{phase}]");
  for step in step_li {
    apply(store, *step, &context)
      .await
      .unwrap_or_else(|err| panic!("{context}: 步骤 {step:?} 失败: {err}"));
  }
}

/// 复用 `op_log_li()` 做前缀比较：`actual` 必须以 `expected` 开头。
fn assert_log_prefix(expected: &[OpRecord], actual: &[OpRecord], case: &str, phase: &str) {
  assert!(
    actual.len() >= expected.len(),
    "{case} [{phase}]: op log 长度 {} 小于期望前缀 {}",
    actual.len(),
    expected.len()
  );
  assert_eq!(
    &actual[..expected.len()],
    expected,
    "{case} [{phase}]: op log 前缀不一致"
  );
}

struct Case {
  name: &'static str,
  fault_op: OpKind,
  /// 前置状态。不得包含 `fault_op`，否则 `at = 1` 会在前缀里提前命中。
  prefix: &'static [Step],
  /// 被注入故障的那次逻辑操作（该 op kind 的第一次出现）。
  victim: Step,
  /// Disarm 后重试同一逻辑操作。
  retry: Step,
  /// 重试成功后的收尾步骤（如数据类写入需要 `sync_data` 才抗掉电）。
  after_retry: &'static [Step],
}

fn case_li() -> Vec<Case> {
  vec![
    Case {
      name: "ReadPage@1",
      fault_op: OpKind::ReadPage,
      // 先造两页 durable 状态：读失败不得动它们。
      prefix: &[
        Step::WritePage { fp: 0, byte: 0xA1 },
        Step::WritePage { fp: 1, byte: 0xB2 },
        Step::SyncData,
      ],
      victim: Step::ReadPage {
        fp: 0,
        expect: None,
      },
      retry: Step::ReadPage {
        fp: 0,
        expect: Some(0xA1),
      },
      after_retry: &[],
    },
    Case {
      name: "SyncData@1",
      fault_op: OpKind::SyncData,
      // 两页都在 volatile：命中时一页都不得被提前刷进 durable。
      prefix: &[
        Step::WritePage { fp: 0, byte: 0xA1 },
        Step::WritePage { fp: 1, byte: 0xB2 },
      ],
      victim: Step::SyncData,
      retry: Step::SyncData,
      after_retry: &[],
    },
    Case {
      name: "WriteSegment@1",
      fault_op: OpKind::WriteSegment,
      // 先落 durable 页；段写失败不得动它，也不得留下 volatile 段。
      prefix: &[Step::WritePage { fp: 0, byte: 0xA1 }, Step::SyncData],
      victim: Step::WriteSegment {
        fp: 7,
        bytes: b"segment",
      },
      retry: Step::WriteSegment {
        fp: 7,
        bytes: b"segment",
      },
      // 段是数据类写入，重试后需 sync_data 才抗掉电；干净运行走同一序列。
      after_retry: &[Step::SyncData],
    },
    Case {
      name: "AppendManifest@1",
      fault_op: OpKind::AppendManifest,
      // 用 SwitchManifest 造旧 manifest：prefix 不得含 AppendManifest，否则提前命中。
      prefix: &[
        Step::WritePage { fp: 0, byte: 0xA1 },
        Step::SyncData,
        Step::SwitchManifest { record: b"head" },
      ],
      victim: Step::AppendManifest { record: b"body" },
      retry: Step::AppendManifest { record: b"body" },
      after_retry: &[],
    },
    Case {
      name: "SwitchManifest@1",
      fault_op: OpKind::SwitchManifest,
      // 用 AppendManifest 造旧 manifest：prefix 不得含 SwitchManifest，否则提前命中。
      prefix: &[
        Step::WritePage { fp: 0, byte: 0xA1 },
        Step::SyncData,
        Step::AppendManifest { record: b"old" },
      ],
      victim: Step::SwitchManifest { record: b"new" },
      retry: Step::SwitchManifest { record: b"new" },
      after_retry: &[],
    },
  ]
}

/// 矩阵必须覆盖 issue 要求的 5 个 op；表被误删 / 缩减时直接失败。
#[test]
fn fault_matrix_covers_at_least_five_ops() {
  let kind_li: Vec<OpKind> = case_li().iter().map(|case| case.fault_op).collect();
  assert_eq!(
    kind_li,
    vec![
      OpKind::ReadPage,
      OpKind::SyncData,
      OpKind::WriteSegment,
      OpKind::AppendManifest,
      OpKind::SwitchManifest,
    ],
    "故障矩阵必须覆盖这 5 个 op"
  );
}

/// 断言 1 + 2：命中那次返回注入错误，且不改变 durable / volatile 状态。
#[test]
fn fault_matrix_leaves_state_unmodified() {
  pollster::block_on(async {
    for case in case_li() {
      let table = table();
      let fault = Fault {
        op: case.fault_op,
        at: 1,
        err: INJECTED,
      };

      // 参照运行：同前缀、无故障，记录命中前的可见 / crash 后盘面与 op log。
      let clean = new_store();
      run_step_li(&clean, case.prefix, case.name, "clean-prefix").await;
      assert_eq!(
        clean.op_count(case.fault_op),
        0,
        "{}: 干净前缀不得包含目标 op（否则 at=1 提前命中）",
        case.name
      );
      let clean_prefix_log = clean.op_log_li();
      let clean_prefix_clock = clean.clock();
      let clean_visible_snapshot = clean.snapshot();
      clean.crash();
      let clean_crash_snapshot = clean.snapshot();

      // 故障运行：同前缀 + at=1 注入。
      let store = new_store().with_fault(fault);
      run_step_li(&store, case.prefix, case.name, "prefix").await;
      assert_eq!(
        store.op_count(case.fault_op),
        0,
        "{}: 前缀不得消耗 at=1",
        case.name
      );
      let before_snapshot = store.snapshot();
      let before_log = store.op_log_li();
      assert_eq!(
        before_log, clean_prefix_log,
        "{}: 前缀 op log 与干净运行不一致",
        case.name
      );
      assert_eq!(
        before_snapshot, clean_visible_snapshot,
        "{}: 前缀可见盘面与干净运行不一致",
        case.name
      );

      // 1. 命中那次返回注入错误。
      let err = apply(&store, case.victim, &format!("{} [hit]", case.name)).await;
      assert_eq!(
        err,
        Err(INJECTED),
        "{} [hit]: 命中那次必须返回注入错误",
        case.name
      );
      assert_eq!(
        store.op_count(case.fault_op),
        1,
        "{} [hit]: at=1 应恰好命中第 1 次",
        case.name
      );

      // 2a. 可见状态（durable + volatile 合并视图）不变。
      assert_eq!(
        store.snapshot(),
        before_snapshot,
        "{} [hit]: 命中那次不得改动 durable/volatile 可见状态",
        case.name
      );

      // 2b. op log 只追加命中记录，失败 op 不得展开任何子 op。
      let hit_log = store.op_log_li();
      assert_eq!(
        hit_log.len(),
        before_log.len() + 1,
        "{} [hit]: 失败 op 不得展开子 op\nbefore={before_log:?}\nafter={hit_log:?}",
        case.name
      );
      assert_log_prefix(&before_log, &hit_log, case.name, "hit");
      let hit_record = hit_log.last().expect("命中后至少有一条记录");
      assert_eq!(
        hit_record.op,
        case.victim.op(&table),
        "{} [hit]: 命中记录参数不符",
        case.name
      );
      assert_eq!(
        hit_record.clock,
        clean_prefix_clock + 1,
        "{} [hit]: 命中记录时钟不符",
        case.name
      );

      // 2c. crash 丢弃 volatile 后，durable 必须等于该 op 之前的干净前缀。
      store.crash();
      assert_eq!(
        store.snapshot(),
        clean_crash_snapshot,
        "{} [crash]: 命中那次 crash 后盘面必须等于该 op 之前的干净前缀",
        case.name
      );
    }
  });
}

/// 断言 3：`clear_fault_li()`（Disarm）后重试同一逻辑操作成功，并达到干净运行的最终盘面。
#[test]
fn fault_matrix_retry_completes_after_reset() {
  pollster::block_on(async {
    for case in case_li() {
      let table = table();
      let fault = Fault {
        op: case.fault_op,
        at: 1,
        err: INJECTED,
      };

      // 干净完整运行：prefix + 目标 op 成功 + 收尾，作为重试后的目标盘面。
      let clean = new_store();
      run_step_li(&clean, case.prefix, case.name, "clean-prefix").await;
      let clean_prefix_log = clean.op_log_li();
      let prefix_len = clean_prefix_log.len();
      apply(&clean, case.victim, &format!("{} [clean]", case.name))
        .await
        .unwrap_or_else(|err| panic!("{} [clean]: 干净运行的目标 op 失败: {err}", case.name));
      let clean_hit_log = clean.op_log_li();
      run_step_li(&clean, case.after_retry, case.name, "clean-after").await;
      clean.crash();
      let clean_crash_snapshot = clean.snapshot();

      let mut store = new_store().with_fault(fault);
      run_step_li(&store, case.prefix, case.name, "prefix").await;
      assert_eq!(
        store.op_log_li(),
        clean_prefix_log,
        "{}: 前缀 op log 与干净运行不一致",
        case.name
      );

      assert_eq!(
        apply(&store, case.victim, &format!("{} [hit]", case.name)).await,
        Err(INJECTED),
        "{} [hit]: 首次必须返回注入错误",
        case.name
      );
      let hit_log = store.op_log_li();
      assert_eq!(
        hit_log.len(),
        prefix_len + 1,
        "{} [hit]: 失败 op 不得展开子 op",
        case.name
      );
      assert_log_prefix(&clean_prefix_log, &hit_log, case.name, "hit");
      assert_eq!(
        hit_log[prefix_len], clean_hit_log[prefix_len],
        "{} [hit]: 命中记录必须与干净运行的首条目标 op 记录一致",
        case.name
      );

      // Disarm：对应 C++ `FailPoint::Disarm`，之后同一逻辑 op 必须可完成。
      store.clear_fault_li();
      apply(&store, case.retry, &format!("{} [retry]", case.name))
        .await
        .unwrap_or_else(|err| {
          panic!(
            "{} [retry]: reset 后重试 {:?} 失败: {err}",
            case.name, case.retry
          )
        });
      assert_eq!(
        store.op_count(case.fault_op),
        2,
        "{} [retry]: 重试应是该 op 的第 2 次",
        case.name
      );

      let retry_log = store.op_log_li();
      assert!(
        retry_log.len() > prefix_len + 1,
        "{} [retry]: 重试必须留下 op 记录",
        case.name
      );
      assert_eq!(
        retry_log[prefix_len + 1].op,
        case.retry.op(&table),
        "{} [retry]: 重试记录参数不符",
        case.name
      );

      run_step_li(&store, case.after_retry, case.name, "after-retry").await;
      store.crash();
      assert_eq!(
        store.snapshot(),
        clean_crash_snapshot,
        "{} [final]: reset 后重试未完成同一逻辑操作",
        case.name
      );
    }
  });
}
