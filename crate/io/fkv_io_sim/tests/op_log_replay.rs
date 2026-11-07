//! 引擎级 op log：文本回放、确定性 dump、逐 op 崩溃 fuzz、failpoint `!kill` 子进程。
//!
//! 覆盖 6 条 op log：纯写、写删混合、TTL、truncate、archive+branch、reopen 交错。
//! 每条 log 都在每个逻辑 op 索引处 crash + restart + reopen，并把结果与
//! 同前缀的干净运行逐字节比较：已确认写不丢、无半值、Snapshot 确定、无悬挂引用。

use std::{
  env::{current_exe, var},
  process::{Command, exit},
};

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io_sim::{
  Op, OpRecord, SimStore, Snapshot,
  oplog::{self, EngineOp, OpLogError, hex_encode},
};
use fkv_store::Store;
use fkv_types::{TableIdent, parse_manifest_file_name};

const PAGE_SIZE: usize = 512;
const RESTART_INTERVAL: u16 = 16;
/// 所有 TTL 断言与 dump 都锚定同一逻辑时刻，避免读墙钟。
const SCAN_NOW: u64 = 10_000;

type KvLi = Vec<(Vec<u8>, Vec<u8>)>;
type BranchSnapshotLi = Vec<(String, KvLi)>;
type ExpectedLi<'a> = Vec<(&'a str, Vec<(&'a [u8], &'a [u8])>)>;

const CHILD_ENV: &str = "FKV_SIM_KILL_CHILD";

fn tbl() -> TableIdent {
  TableIdent::new("log", 1).unwrap()
}

fn put(key: &[u8], value: &[u8], ts: u64) -> EngineOp {
  EngineOp::Put {
    tbl: tbl(),
    ts,
    key: key.to_vec(),
    value: value.to_vec(),
    expire_ts: 0,
  }
}

fn put_ttl(key: &[u8], value: &[u8], ts: u64, expire_ts: u64) -> EngineOp {
  EngineOp::Put {
    tbl: tbl(),
    ts,
    key: key.to_vec(),
    value: value.to_vec(),
    expire_ts,
  }
}

fn del(key: &[u8], ts: u64) -> EngineOp {
  EngineOp::Delete {
    tbl: tbl(),
    ts,
    key: key.to_vec(),
  }
}

/// 固定场景：同一份文本 op log 在两次运行中必须产生同一 dump。
fn scenario_li() -> Vec<(&'static str, u64, Vec<EngineOp>)> {
  let truncate = EngineOp::Truncate { tbl: tbl() };
  let archive_1 = EngineOp::Archive {
    tbl: tbl(),
    tag: "001".to_owned(),
    keep: 2,
  };
  let archive_2 = EngineOp::Archive {
    tbl: tbl(),
    tag: "002".to_owned(),
    keep: 2,
  };
  let branch = EngineOp::Branch {
    tbl: tbl(),
    name: "feature-1".to_owned(),
  };
  vec![
    (
      "pure_write",
      0x1001,
      vec![
        put(b"a", b"A1", 1),
        put(b"b", b"B1", 2),
        put(b"c", b"C1", 3),
        put(b"a", b"A2", 4),
        put(b"d", b"D1", 5),
      ],
    ),
    (
      "write_delete_mix",
      0x1002,
      vec![
        put(b"a", b"A", 1),
        put(b"b", b"B", 2),
        put(b"c", b"C", 3),
        del(b"b", 4),
        put(b"d", b"D", 5),
        del(b"a", 6),
        put(b"b", b"B2", 7),
      ],
    ),
    (
      "ttl",
      0x1003,
      vec![
        put_ttl(b"a", b"A", 1, 50_000),
        put_ttl(b"b", b"B", 2, 1_000),
        put_ttl(b"c", b"C", 3, 50_000),
        put_ttl(b"d", b"D", 4, 100),
        put(b"e", b"E", 5),
      ],
    ),
    (
      "truncate",
      0x1004,
      vec![
        put(b"a", b"A", 1),
        put(b"b", b"B", 2),
        put(b"c", b"C", 3),
        truncate,
        put(b"x", b"X", 4),
        put(b"y", b"Y", 5),
      ],
    ),
    (
      "archive_branch",
      0x1005,
      vec![
        put(b"a", b"A", 1),
        put(b"b", b"B", 2),
        put(b"c", b"C", 3),
        archive_1,
        put(b"d", b"D", 4),
        archive_2,
        branch,
        put(b"e", b"E", 5),
      ],
    ),
    (
      "reopen_interleave",
      0x1006,
      vec![
        put(b"a", b"A", 1),
        EngineOp::Reopen { tbl: tbl() },
        put(b"b", b"B", 2),
        del(b"a", 3),
        EngineOp::Reopen { tbl: tbl() },
        put(b"c", b"C", 4),
      ],
    ),
  ]
}

fn fnv1a64(bytes: &[u8]) -> u64 {
  bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
    (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
  })
}

fn push_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
  out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
  out.extend_from_slice(bytes);
}

/// Snapshot 的规范字节序：字段顺序固定、内部列表已排序，跨进程稳定。
fn snapshot_bytes(snapshot: &Snapshot) -> Vec<u8> {
  let mut out = Vec::new();
  for (tbl, file_page_id, bytes) in &snapshot.page_li {
    out.push(b'P');
    push_bytes(&mut out, tbl.tbl_name.as_bytes());
    out.extend_from_slice(&tbl.partition_id.to_le_bytes());
    out.extend_from_slice(&file_page_id.to_le_bytes());
    push_bytes(&mut out, bytes);
  }
  for (tbl, file_page_id, bytes) in &snapshot.segment_li {
    out.push(b'S');
    push_bytes(&mut out, tbl.tbl_name.as_bytes());
    out.extend_from_slice(&tbl.partition_id.to_le_bytes());
    out.extend_from_slice(&file_page_id.to_le_bytes());
    push_bytes(&mut out, bytes);
  }
  for (tbl, bytes) in &snapshot.manifest_li {
    out.push(b'M');
    push_bytes(&mut out, tbl.tbl_name.as_bytes());
    out.extend_from_slice(&tbl.partition_id.to_le_bytes());
    push_bytes(&mut out, bytes);
  }
  for (tbl, name, bytes) in &snapshot.manifest_file_li {
    out.push(b'F');
    push_bytes(&mut out, tbl.tbl_name.as_bytes());
    out.extend_from_slice(&tbl.partition_id.to_le_bytes());
    push_bytes(&mut out, name.as_bytes());
    push_bytes(&mut out, bytes);
  }
  for (branch, shard_id, term) in &snapshot.term_li {
    out.push(b'T');
    push_bytes(&mut out, branch.as_bytes());
    out.extend_from_slice(&shard_id.to_le_bytes());
    out.extend_from_slice(&term.to_le_bytes());
  }
  out
}

struct RunResult {
  dump: String,
  /// branch 创建瞬间的 (name, 键值)，用于崩溃后校验 branch 可读。
  branch_li: BranchSnapshotLi,
}

async fn scan(
  store: &Store<SimStore>,
  tbl: &TableIdent,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, String> {
  store
    .scan_at(tbl, b"", b"", 0, SCAN_NOW)
    .await
    .map_err(|err| format!("scan 失败: {err}"))
}

/// 规范化 dump：键值遍历 + archive tag + branch 清单 + Snapshot 摘要。
async fn dump(store: &Store<SimStore>, tbl: &TableIdent) -> Result<String, String> {
  let mut out = String::new();
  out.push_str("kv\n");
  for (key, value) in scan(store, tbl).await? {
    out.push_str(&hex_encode(&key));
    out.push('=');
    out.push_str(&hex_encode(&value));
    out.push('\n');
  }

  let tag_li = store
    .list_archive_tags(tbl)
    .await
    .map_err(|err| format!("list_archive_tags 失败: {err}"))?;
  out.push_str("archive\n");
  for info in tag_li {
    out.push_str(&info.tag);
    out.push('\n');
  }

  let snapshot = store.page_io().snapshot();
  let mut branch_name_li: Vec<&str> = snapshot
    .manifest_file_li
    .iter()
    .filter_map(|(_, name, _)| {
      let (branch, _, tag) = parse_manifest_file_name(name)?;
      (tag.is_none() && branch != "main").then_some(branch)
    })
    .collect();
  branch_name_li.sort_unstable();
  branch_name_li.dedup();
  out.push_str("branch\n");
  for name in branch_name_li {
    out.push_str(name);
    out.push('\n');
  }

  let canonical = snapshot_bytes(&snapshot);
  out.push_str(&format!(
    "snapshot={:016x} pages={} segments={} manifests={} manifest_files={} terms={}\n",
    fnv1a64(&canonical),
    snapshot.page_li.len(),
    snapshot.segment_li.len(),
    snapshot.manifest_li.len(),
    snapshot.manifest_file_li.len(),
    snapshot.term_li.len()
  ));
  Ok(out)
}

async fn apply(
  store: &mut Store<SimStore>,
  op: &EngineOp,
  branch_li: &mut BranchSnapshotLi,
) -> Result<(), String> {
  match op {
    EngineOp::Put {
      tbl,
      ts,
      key,
      value,
      expire_ts,
    } => {
      let mut entry = BatchEntry::upsert(key.clone(), value.clone(), *ts);
      entry.expire_ts = *expire_ts;
      store
        .put_at(tbl, &[entry], *ts)
        .await
        .map_err(|err| format!("put 失败: {err}"))
    }
    EngineOp::Delete { tbl, ts, key } => store
      .delete(tbl, key, *ts)
      .await
      .map_err(|err| format!("delete 失败: {err}")),
    EngineOp::Truncate { tbl } => store
      .truncate(tbl)
      .await
      .map_err(|err| format!("truncate 失败: {err}")),
    EngineOp::Archive { tbl, tag, keep } => store
      .create_archive(tbl, tag, *keep)
      .await
      .map(|_| ())
      .map_err(|err| format!("archive 失败: {err}")),
    EngineOp::Branch { tbl, name } => {
      store
        .create_branch(tbl, name)
        .await
        .map_err(|err| format!("branch 失败: {err}"))?;
      let kv_li = scan(store, tbl).await?;
      branch_li.push((name.clone(), kv_li));
      Ok(())
    }
    EngineOp::Reopen { tbl } => store
      .reopen(tbl)
      .await
      .map_err(|err| format!("reopen 失败: {err}")),
  }
}

fn run_log(op_li: &[EngineOp], seed: u64) -> Result<RunResult, String> {
  pollster::block_on(async {
    let io = SimStore::new(PAGE_SIZE)
      .with_reorder_seed(seed)
      .with_payload_capture();
    let mut store = Store::new(io, PAGE_SIZE, RESTART_INTERVAL);
    let mut branch_li = Vec::new();
    for (index, op) in op_li.iter().enumerate() {
      apply(&mut store, op, &mut branch_li)
        .await
        .map_err(|err| format!("op#{index} {}: {err}", op.to_text()))?;
    }
    let dump = dump(&store, &tbl()).await?;
    Ok(RunResult { dump, branch_li })
  })
}

/// 崩溃后重新打开：`reopen` 读 manifest，再逐键 scan 触发页读取，
/// 任何悬挂引用都会以 `ResourceMissing` / `Corrupted` 暴露。
fn crash_reopen(prefix_len: usize, seed: u64, op_li: &[EngineOp]) -> Result<String, String> {
  pollster::block_on(async {
    let io = SimStore::new(PAGE_SIZE)
      .with_reorder_seed(seed)
      .with_payload_capture();
    let mut store = Store::new(io, PAGE_SIZE, RESTART_INTERVAL);
    let mut branch_li = Vec::new();
    for (index, op) in op_li.iter().enumerate() {
      apply(&mut store, op, &mut branch_li)
        .await
        .map_err(|err| format!("op#{index} {}: {err}", op.to_text()))?;
    }
    store.page_io().crash();
    store.page_io().restart();
    let io = store.into_io();
    let mut store = Store::new(io, PAGE_SIZE, RESTART_INTERVAL);
    match store.reopen(&tbl()).await {
      Ok(()) => {}
      // 前缀为空：盘上还没有 manifest，空表是唯一合法状态。
      Err(KvError::ResourceMissing) if prefix_len == 0 => {}
      Err(err) => return Err(format!("崩溃后 reopen 失败: {err}")),
    }
    dump(&store, &tbl()).await
  })
}

/// 同一 op log 两次运行 dump 逐字节一致。
#[test]
fn op_log_replay_is_deterministic() {
  for (name, seed, op_li) in scenario_li() {
    let text = oplog::render_engine_log(&op_li);
    assert_eq!(
      oplog::render_engine_log(&oplog::parse_engine_log(&text).unwrap()),
      text,
      "{name}: 文本 op log 往返不一致"
    );

    let first = run_log(&op_li, seed).unwrap_or_else(|err| panic!("{name} 首次运行失败: {err}"));
    let second = run_log(&op_li, seed).unwrap_or_else(|err| panic!("{name} 二次运行失败: {err}"));
    assert_eq!(first.dump, second.dump, "{name}: 两次 dump 不一致");
  }
}

/// 读 / 列举类底层变体不进入可回放文本格式：序列化与解析都显式报错。
#[test]
fn unsupported_io_variant_errors_explicitly() {
  let record = OpRecord {
    op: Op::ReadPage {
      tbl: tbl(),
      file_page_id: 0,
    },
    clock: 1,
    delay: 0,
    payload: None,
  };
  assert!(matches!(
    record.to_text(1),
    Err(OpLogError::UnsupportedOp { .. })
  ));
  assert!(matches!(
    oplog::parse_record(1, "clock=1 delay=0 op=read_page tbl=6c6f67@1 fp=0"),
    Err(OpLogError::UnsupportedOp { .. })
  ));
  assert!(matches!(
    oplog::parse_record(1, "clock=1 delay=0 op=no_such_op"),
    Err(OpLogError::UnknownOp { .. })
  ));
  assert!(matches!(
    oplog::parse_engine_log("bogus tbl=6c6f67@1"),
    Err(OpLogError::UnknownOp { .. })
  ));
}

/// 语义抽样：确保每条 op log 真的改变了预期状态，而不是空跑。
#[test]
fn op_log_replay_semantics() {
  let expected_li: ExpectedLi<'_> = vec![
    (
      "pure_write",
      vec![(b"a", b"A2"), (b"b", b"B1"), (b"c", b"C1"), (b"d", b"D1")],
    ),
    (
      "write_delete_mix",
      vec![(b"b", b"B2"), (b"c", b"C"), (b"d", b"D")],
    ),
    // now=10000：b(exp=1000) / d(exp=100) 已过期，a / c / e 仍可见。
    ("ttl", vec![(b"a", b"A"), (b"c", b"C"), (b"e", b"E")]),
    ("truncate", vec![(b"x", b"X"), (b"y", b"Y")]),
    (
      "archive_branch",
      vec![
        (b"a", b"A"),
        (b"b", b"B"),
        (b"c", b"C"),
        (b"d", b"D"),
        (b"e", b"E"),
      ],
    ),
    ("reopen_interleave", vec![(b"b", b"B"), (b"c", b"C")]),
  ];
  let scenario_li = scenario_li();
  for (name, expect_li) in expected_li {
    let (_, seed, op_li) = scenario_li
      .iter()
      .find(|(scenario, ..)| *scenario == name)
      .unwrap_or_else(|| panic!("缺少场景 {name}"));
    pollster::block_on(async {
      let io = SimStore::new(PAGE_SIZE).with_reorder_seed(*seed);
      let mut store = Store::new(io, PAGE_SIZE, RESTART_INTERVAL);
      let mut branch_li = Vec::new();
      for (index, op) in op_li.iter().enumerate() {
        apply(&mut store, op, &mut branch_li)
          .await
          .unwrap_or_else(|err| panic!("{name} op#{index}: {err}"));
      }
      let kv_li = scan(&store, &tbl()).await.unwrap();
      let expect_li: Vec<(Vec<u8>, Vec<u8>)> = expect_li
        .iter()
        .map(|(key, value)| (key.to_vec(), value.to_vec()))
        .collect();
      assert_eq!(kv_li, expect_li, "{name}: 最终键值不符");
    });
  }
}

/// 逐逻辑 op 崩溃 fuzz：crash + restart + reopen 后必须与同前缀干净运行逐字节一致。
#[test]
fn crash_fuzz_matches_clean_prefix() {
  for (name, seed, op_li) in scenario_li() {
    let text_li: Vec<String> = op_li.iter().map(EngineOp::to_text).collect();
    for kill_point in 0..=op_li.len() {
      let op_index = kill_point;
      let expected = run_log(&op_li[..kill_point], seed).unwrap_or_else(|err| {
        panic!(
          "{name} seed={seed} kill_point={kill_point} op_index={op_index}: 参考运行失败: {err}"
        )
      });
      let actual = crash_reopen(kill_point, seed, &op_li[..kill_point]).unwrap_or_else(|err| {
        panic!("{name} seed={seed} kill_point={kill_point} op_index={op_index}: {err}")
      });
      assert_eq!(
        actual,
        expected.dump,
        "{name} seed={seed} kill_point={kill_point} op_index={op_index} \
         crash 前后状态不一致\nprefix:\n{}\n",
        text_li[..kill_point].join("\n")
      );
    }
  }
}

/// branch 在崩溃后仍可 open_branch 读出完整快照；branch 内容与创建时一致。
#[test]
fn branch_survives_crash_and_reopens() {
  let scenario_li = scenario_li();
  let (name, seed, op_li) = scenario_li
    .iter()
    .find(|(scenario, ..)| *scenario == "archive_branch")
    .expect("缺少 archive_branch 场景");
  let branch_index = op_li
    .iter()
    .position(|op| matches!(op, EngineOp::Branch { .. }))
    .expect("场景应包含 branch op");
  // 在 branch 之后每个索引都 crash：branch manifest 与父页都必须可读。
  for kill_point in branch_index + 1..=op_li.len() {
    let expected = run_log(&op_li[..kill_point], *seed).unwrap();
    let (branch_name, expected_kv) = expected.branch_li.first().expect("应记录 branch 快照");
    pollster::block_on(async {
      let io = SimStore::new(PAGE_SIZE)
        .with_reorder_seed(*seed)
        .with_payload_capture();
      let mut store = Store::new(io, PAGE_SIZE, RESTART_INTERVAL);
      let mut branch_li = Vec::new();
      for op in &op_li[..kill_point] {
        apply(&mut store, op, &mut branch_li).await.unwrap();
      }
      store.page_io().crash();
      let io = store.into_io();
      let mut store = Store::new(io, PAGE_SIZE, RESTART_INTERVAL);
      let table = tbl();
      store
        .reopen(&table)
        .await
        .unwrap_or_else(|err| panic!("{name} kill_point={kill_point}: reopen 失败: {err}"));
      let io = store.into_io();
      let mut branch = Store::new(io, PAGE_SIZE, RESTART_INTERVAL);
      branch
        .open_branch(&table, branch_name, 0)
        .await
        .unwrap_or_else(|err| {
          panic!("{name} kill_point={kill_point}: open_branch({branch_name}) 失败: {err}")
        });
      let kv_li = scan(&branch, &table).await.unwrap();
      assert_eq!(
        &kv_li, expected_kv,
        "{name} kill_point={kill_point}: branch 内容与创建时不一致"
      );
    });
  }
}

/// 子进程用例：真正命中 `WritePage@1!kill`，进程应 abort。
///
/// 只有父用例通过环境变量拉起时才执行；正常跑测试体时直接返回
/// （这是子进程模式开关，不是缺 failpoint 时的静默跳过）。
#[test]
fn kill_point_child_aborts() {
  if var(CHILD_ENV).as_deref() != Ok("1") {
    return;
  }
  fkv_failpoint::reset();
  fkv_failpoint::set_spec("WritePage@1!kill");
  pollster::block_on(async {
    let mut store = Store::new(SimStore::new(PAGE_SIZE), PAGE_SIZE, RESTART_INTERVAL);
    let _ = store
      .put(
        &tbl(),
        &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      )
      .await;
  });
  // kill point 未生效才会走到这里。
  println!("kill point 未触发 abort");
  exit(0);
}

#[test]
fn kill_point_aborts_child_process() {
  let exe = current_exe().expect("current_exe");
  let output = Command::new(exe)
    .args(["--exact", "kill_point_child_aborts", "--nocapture"])
    .env(CHILD_ENV, "1")
    .env_remove("FKV_FAILPOINT")
    .output()
    .expect("启动子进程");
  let stderr = String::from_utf8_lossy(&output.stderr);
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(
    !output.status.success(),
    "kill point 子进程应非 0 退出，实际 {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
    output.status
  );
  assert!(
    output.status.code().is_none_or(|code| code != 0),
    "kill point 子进程退出码应为信号 / 非 0，实际 {:?}",
    output.status
  );
}
