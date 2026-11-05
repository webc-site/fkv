//! gap-10：intra-op 崩溃闭环（真实 compio 后端）。
//!
//! 与 `fkv_io_sim/tests/op_log_replay.rs` 的 op 边界 fuzz 互补。那边的 `crash()`
//! 只在**逻辑 op 之间**调用，且 sim 的 manifest 立即写 durable、注入点又全部位于
//! 状态变更之前（`fkv_io_sim/src/lib.rs` 的 15 处 `fail_point`/`kill_point`），
//! 所以它**结构上**表达不出撕裂写与 tmp→rename 两阶段窗口。这里改成在真实后端的
//! `kill_point` 处让进程 `abort()`，再由父进程重开目录校验不变量。
//!
//! 不变量：崩溃并重开后，可见状态必须等于**某个 op 前缀**的干净状态 —— 既不能出现
//! 半值 / 撕裂值，也不能凭空多出或丢掉状态；attach 路径还必须清掉残留 `*.tmp`。

use std::{
  env,
  fs::read_dir,
  path::{Path, PathBuf},
  process::{Command, exit},
};

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io::PageIo;
use fkv_io_compio::CompioStore;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, TMP_SUFFIX, TableIdent};

const PAGE_SIZE: usize = 512;
const RESTART_INTERVAL: u16 = 16;

/// 子进程模式开关（区别于「缺注入点时的静默跳过」：这是一次性的角色切换）。
const CHILD_ENV: &str = "FKV_CRASH_CHILD";
const ROOT_ENV: &str = "FKV_CRASH_ROOT";
const PREFIX_ENV: &str = "FKV_CRASH_PREFIX";
/// 必须与下方 `#[compio::test]` 的函数名一致（子进程用 `--exact` 只跑它）。
const CHILD_TEST: &str = "crash_child_applies_prefix";
/// 命中序号上界（默认）。实际命中次数通常超过它，枚举到上界即停 —— 目的是覆盖
/// **不同的注入点**而不是穷举每一次命中，避免用例变成几百次进程拉起。
/// 需要更大预算时用 `FKV_CRASH_HIT_MAX` 覆盖（夜间作业可调大）。
const MAX_HIT: u32 = 6;
const HIT_MAX_ENV: &str = "FKV_CRASH_HIT_MAX";
/// 子进程在**执行每个 op 之前**用该前缀把序号打到 stderr（`eprintln!` 不经缓冲），
/// 父进程据此报告崩溃时正在执行的 op —— `abort()` 后子进程无法再汇报任何信息。
const OP_MARKER: &str = "CRASH_OP=";

/// 固定 op log。非空 op 各触发一次 manifest 追加；第 4 条 `truncate` 强制
/// `switch_manifest`，使 `AtomicWriteFile:*` 也进入可达集合。
const OP_NUM: usize = 6;
const TRUNCATE_OP: usize = 4;

/// compio 侧**有** `kill_point` 的名字：子进程在这类点上会 `abort()`。
///
/// 名单与 `fkv_io_compio` 的注入点一一对应；改名或移位必须同步这里，否则
/// 下方 `aborted > 0` 会变红（gap-11 那类静默漂移的探测器）。
const KILL_NAMES: &[&str] = &[
  "WritePage",
  "AppendManifest:Write",
  "AppendManifest:Sync",
  "AtomicWriteFile:Sync",
  "AtomicWriteFile:Rename",
  "AtomicWriteFile:SyncDir",
  "SyncDir",
];

/// compio 侧**只有** `fail_point`（注入错误、不 abort）的名字。
///
/// 它们是反向哨兵：`fkv_failpoint::fail_point` 对 `Kill` 动作直接返回 `None`
/// （`fkv_failpoint/src/lib.rs:138-140`），所以给这些名字 arm `!kill` 会**静默无效**。
/// 用例断言它们确实不 abort —— 一旦有人把 `kill_point` 加进这些点，或把上面的
/// 集合写错，这里会变红，而不是安静地少测一半。
const FAIL_ONLY_NAMES: &[&str] = &["SyncData", "WriteSegments", "WriteFile:SyncFail"];

type Kv = Vec<(Vec<u8>, Vec<u8>)>;

fn tbl() -> TableIdent {
  TableIdent::new("crash", 1).unwrap()
}

fn op_batch(index: usize) -> Vec<BatchEntry> {
  match index {
    0 => vec![BatchEntry::upsert(b"a".to_vec(), b"A1".to_vec(), 1)],
    1 => vec![BatchEntry::upsert(b"b".to_vec(), b"B1".to_vec(), 2)],
    2 => vec![
      BatchEntry::upsert(b"a".to_vec(), b"A2".to_vec(), 3),
      BatchEntry::upsert(b"c".to_vec(), b"C1".to_vec(), 3),
    ],
    3 => vec![BatchEntry::upsert(b"d".to_vec(), b"D1".to_vec(), 4)],
    TRUNCATE_OP => Vec::new(),
    _ => vec![BatchEntry::upsert(b"e".to_vec(), b"E1".to_vec(), 5)],
  }
}

async fn apply(store: &mut Store<CompioStore>, index: usize) -> Result<(), KvError> {
  match index {
    TRUNCATE_OP => store.truncate(&tbl()).await,
    _ => store.put(&tbl(), &op_batch(index)).await,
  }
}

fn store_over(dir: &Path) -> Store<CompioStore> {
  let io = CompioStore::new(dir.to_path_buf(), PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT);
  Store::new(io, PAGE_SIZE, RESTART_INTERVAL)
}

/// 干净跑 `prefix` 条 op 后的可见 KV；`prefix == 0` 时表尚未建立，记空。
async fn clean_prefix(prefix: usize) -> Kv {
  let root = TempRoot::new("crash_ref");
  let mut store = store_over(root.path());
  for index in 0..prefix {
    apply(&mut store, index)
      .await
      .unwrap_or_else(|err| panic!("干净前缀 op#{index} 失败: {err}"));
  }
  if prefix == 0 {
    return Vec::new();
  }
  store
    .scan(&tbl(), b"", b"", 0)
    .await
    .expect("干净前缀 scan 失败")
}

fn tmp_li(dir: &Path) -> Vec<PathBuf> {
  let part = dir.join(format!("{}.{}", tbl().tbl_name, tbl().partition_id));
  let Ok(entry_li) = read_dir(&part) else {
    return Vec::new();
  };
  entry_li
    .flatten()
    .map(|entry| entry.path())
    .filter(|path| {
      path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(TMP_SUFFIX))
    })
    .collect()
}

/// 崩溃后重开并扫描。表未建立（前缀为空）时返回空 KV。
async fn reopen_and_scan(dir: &Path) -> Kv {
  let mut store = store_over(dir);
  // 对齐 attach：`set_file_identity` 重置身份 / 缓存，`preload_path_li` 重建目录索引并
  // 清掉上次崩溃残留的 `*.tmp`（`fkv_io_compio/src/lib.rs:2301-2312`、`:2149-2152`）。
  store.page_io().set_file_identity("main", 0);
  store.page_io().preload_path_li(&tbl());
  let leftover = tmp_li(dir);
  assert!(leftover.is_empty(), "attach 后仍残留临时文件: {leftover:?}");

  match store.reopen(&tbl()).await {
    Ok(()) => {}
    Err(KvError::ResourceMissing) => return Vec::new(),
    Err(err) => panic!("崩溃后 reopen 失败: {err}"),
  }
  store
    .scan(&tbl(), b"", b"", 0)
    .await
    .unwrap_or_else(|err| panic!("崩溃后 scan 失败: {err}"))
}

/// 子进程体：在给定目录上施加固定前缀，注入点由 `FKV_FAILPOINT` 控制。
///
/// 正常跑测试体时 `FKV_CRASH_CHILD` 未设，直接返回。
#[compio::test]
async fn crash_child_applies_prefix() {
  if env::var(CHILD_ENV).as_deref() != Ok("1") {
    return;
  }
  let dir = PathBuf::from(env::var(ROOT_ENV).expect("缺少 FKV_CRASH_ROOT"));
  let prefix: usize = env::var(PREFIX_ENV)
    .expect("缺少 FKV_CRASH_PREFIX")
    .parse()
    .expect("FKV_CRASH_PREFIX 不是 usize");
  let mut store = store_over(&dir);
  for index in 0..prefix {
    // 崩溃前把「正在执行第几条 op」落到 stderr：`abort()` 之后子进程再无输出机会，
    // 这行是父进程唯一能拿到的崩溃位置。
    eprintln!("{OP_MARKER}{index}");
    if let Err(err) = apply(&mut store, index).await {
      // 注入错误（fail point）不是崩溃路径：退出码 2 与 abort 区分开。
      eprintln!("崩溃子进程：op#{index} 返回 {err}");
      exit(2);
    }
  }
  exit(0);
}

/// 一次崩溃子进程的结果。
struct ChildOutcome {
  /// 进程非正常退出（即被 kill point `abort`）。
  aborted: bool,
  /// 崩溃时正在执行的 op 序号；在第一条 op 就死则为 `None`。
  op_index: Option<usize>,
}

/// 在 `dir` 上跑固定前缀并 arm `spec`，返回子进程结果。
fn spawn_child(dir: &Path, prefix: usize, spec: &str) -> ChildOutcome {
  let exe = env::current_exe().expect("current_exe");
  let output = Command::new(&exe)
    .args(["--exact", CHILD_TEST, "--nocapture"])
    .env(CHILD_ENV, "1")
    .env(ROOT_ENV, dir)
    .env(PREFIX_ENV, prefix.to_string())
    .env("FKV_FAILPOINT", spec)
    .output()
    .expect("启动崩溃子进程");
  let stderr = String::from_utf8_lossy(&output.stderr);
  let op_index = stderr
    .lines()
    .filter_map(|line| line.trim().strip_prefix(OP_MARKER))
    .filter_map(|text| text.trim().parse::<usize>().ok())
    .last();
  ChildOutcome {
    aborted: !output.status.success(),
    op_index,
  }
}

/// 单行 op 摘要，让失败信息不必再回读源码才看得懂。
fn op_desc(index: usize) -> String {
  match index {
    TRUNCATE_OP => "truncate".to_owned(),
    _ => format!("put x{}", op_batch(index).len()),
  }
}

/// 一键复现命令：把注入点、目录与前缀拼成可直接粘贴的一行。
fn replay_command(dir: &Path, prefix: usize, spec: &str) -> String {
  let exe = env::current_exe().map_or_else(|_| "<test-binary>".to_owned(), |p| {
    p.display().to_string()
  });
  format!(
    "FKV_CRASH_CHILD=1 FKV_CRASH_ROOT={} FKV_CRASH_PREFIX={prefix} FKV_FAILPOINT='{spec}' \
     {exe} --exact {CHILD_TEST} --nocapture",
    dir.display()
  )
}

/// 逐 kill 名字 × 命中序号在 op 内部中止进程，重开后状态必须等于某个 op 前缀。
#[compio::test]
async fn crash_at_failpoint_leaves_some_clean_prefix() {
  let mut prefix_kv_li: Vec<Kv> = Vec::new();
  for prefix in 0..=OP_NUM {
    prefix_kv_li.push(clean_prefix(prefix).await);
  }
  let op_text = (0..OP_NUM)
    .map(|index| format!("op#{index}={}", op_desc(index)))
    .collect::<Vec<_>>()
    .join(", ");

  let hit_max = env::var(HIT_MAX_ENV)
    .ok()
    .and_then(|text| text.parse().ok())
    .unwrap_or(MAX_HIT);

  for name in KILL_NAMES {
    let mut aborted = 0;
    for hit in 1..=hit_max {
      let spec = format!("{name}@{hit}!kill");
      let root = TempRoot::new("crash_fp");
      let outcome = spawn_child(root.path(), OP_NUM, &spec);
      if !outcome.aborted {
        // 该名字第 hit 次已不再命中 → 该点的命中次数枚举完毕。
        break;
      }
      aborted += 1;
      let kv = reopen_and_scan(root.path()).await;
      assert!(
        prefix_kv_li.contains(&kv),
        "崩溃后状态不等于任何干净前缀\n\
         注入点:  {spec}\n\
         崩溃时:  op#{}\n\
         op log:  {op_text}\n\
         复现:    {}\n\
         崩溃后:  {kv:?}\n\
         前缀集:  {prefix_kv_li:?}",
        outcome
          .op_index
          .map_or_else(|| "<首条 op 之前>".to_owned(), |index| index.to_string()),
        replay_command(root.path(), OP_NUM, &spec),
      );
    }
    assert!(
      aborted > 0,
      "{name} 在 {OP_NUM} 条 op 内从未 abort —— kill 名字集合已漂移\n\
       复现单个注入点: {}",
      replay_command(Path::new("<临时目录>"), OP_NUM, &format!("{name}@1!kill")),
    );
  }

  for name in FAIL_ONLY_NAMES {
    let spec = format!("{name}@1!kill");
    let root = TempRoot::new("crash_fp_rev");
    let outcome = spawn_child(root.path(), OP_NUM, &spec);
    assert!(
      !outcome.aborted,
      "{name} 只有 fail_point，arm !kill 却让进程非正常退出：fail_point 对 Kill 动作应静默无效\n\
       复现: {}",
      replay_command(root.path(), OP_NUM, &spec),
    );
  }
}
