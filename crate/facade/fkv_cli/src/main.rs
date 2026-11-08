//! fkv 检查 CLI：Local 便捷路径与 Runtime 模式（Cloud / Standby / Mem）。
//!
//! `--mode auto`（默认）按 `KvOption::derive_mode()` 选择入口：
//! Local 且未配 `store_path` 走 SDK 的 [`KvStore`]（无需 attach 的便捷路径）；
//! Local 配了 `store_path`、以及 Cloud / Standby 走 facade 暴露的 [`Runtime`]。
//! `store_path` 是配置驱动的数据根（单根或多根），SDK 只装配单根且要求它等于
//! `--root`，因此数据面必须与 `archive` 等控制面命令一样走 Runtime（`RootedStore`）。
//! `--mode mem` 走纯内存 Runtime。所有配置来自 `KvOption`（`--config <ini>`），
//! `--mode` 只负责选择 / 校验入口，不会静默改写模式。
//!
//! Local 的数据面（SDK）与控制面（Runtime）共用同一 `--term`：data/segment
//! 盘面文件名带 `(branch, term)`，本地活跃 manifest 固定名 `manifest_main_0`
//! （sto-70），term 不进 manifest 名。两边 term 分叉时读侧可经 `(branch, term)`
//! 变体回退读到旧 term 数据，但写只按当前 `(branch, term)` 命名，会把同一
//! 逻辑盘面分裂成两个 term 文件族；归档名构造、term 文件 CAS（attach
//! fencing）与 GC 变体归属都要求两边共用同一 term。

use std::{path::Path, str::FromStr};

use clap_args::{
  ArgAction, arg,
  clap::{Arg, ArgMatches, Command},
};
use compio::runtime::Runtime as CompioRuntime;
use fkv::{
  BatchEntry, GlobalReply, KvError, KvOption, KvStore, MAIN_BRANCH_NAME, Result, Runtime,
  StoreMode, TableIdent, now_ms, start_mem_runtime, start_runtime,
};
use rusty_alloc_default as _;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
  Auto,
  Local,
  Cloud,
  Standby,
  StandbyMaster,
  StandbyReplica,
  Mem,
}

impl Mode {
  fn parse(value: &str) -> Result<Self> {
    match value {
      "auto" => Ok(Self::Auto),
      "local" => Ok(Self::Local),
      "cloud" => Ok(Self::Cloud),
      "standby" => Ok(Self::Standby),
      "standby-master" => Ok(Self::StandbyMaster),
      "standby-replica" => Ok(Self::StandbyReplica),
      "mem" => Ok(Self::Mem),
      _ => Err(KvError::InvalidArgs),
    }
  }

  fn is_standby(self) -> bool {
    matches!(
      self,
      Self::Standby | Self::StandbyMaster | Self::StandbyReplica
    )
  }
}

fn arg_str<'a>(arg_match: &'a ArgMatches, key: &str, default: &'a str) -> &'a str {
  arg_match
    .get_one::<String>(key)
    .map(String::as_str)
    .unwrap_or(default)
}

fn table_of(arg_match: &ArgMatches) -> Result<TableIdent> {
  TableIdent::from_str(arg_str(arg_match, "table", ""))
}

fn options_of(arg_match: &ArgMatches) -> Result<KvOption> {
  let mut option = KvOption::default();
  let config = arg_str(arg_match, "config", "");
  if !config.is_empty() {
    option.load_from_ini(config)?;
  }
  Ok(option)
}

fn term_of(arg_match: &ArgMatches) -> Result<u64> {
  let term = arg_str(arg_match, "term", "1")
    .parse::<u64>()
    .map_err(|_| KvError::InvalidArgs)?;
  if term == 0 {
    return Err(KvError::InvalidArgs);
  }
  Ok(term)
}

/// 把 `--mode` 折叠进 `KvOption`，并校验派生模式与显式选择一致。
///
/// 不一致直接拒绝：配置写了 Cloud 却传 `--mode local` 时，静默清空 cloud
/// 配置会让「以为在写云端、实际落到本地盘」成立，比报错糟得多。
fn apply_mode(mode: Mode, option: &mut KvOption, root: &Path) -> Result<()> {
  if mode.is_standby() {
    option.enable_local_standby = true;
    // standby 要求显式 store_path；CLI 的 --root 就是数据根。
    if option.store_path.is_empty() {
      option.store_path = vec![root.to_string_lossy().into_owned()];
    }
  }
  // 显式 master 与配置里的 master 地址互斥：清掉地址，validate 才会派生
  // StandbyMaster（否则会被静默当成 replica）。
  if mode == Mode::StandbyMaster {
    option.standby_master_addr.clear();
    option.standby_master_store_paths.clear();
    option.standby_master_store_path_weights.clear();
    option.standby_master_store_path_lut_li.clear();
  }
  option.validate()?;
  let derived = option.derive_mode();
  let matched = match mode {
    Mode::Auto => true,
    Mode::Local => derived == StoreMode::Local,
    Mode::Cloud => derived == StoreMode::Cloud,
    Mode::Standby => matches!(
      derived,
      StoreMode::StandbyMaster | StoreMode::StandbyReplica
    ),
    Mode::StandbyMaster => derived == StoreMode::StandbyMaster,
    Mode::StandbyReplica => derived == StoreMode::StandbyReplica,
    Mode::Mem => true,
  };
  if matched {
    Ok(())
  } else {
    Err(KvError::InvalidArgs)
  }
}

/// 控制面命令必须走 Runtime（SDK 没有全局 fanout）。
fn is_runtime_command(name: &str) -> bool {
  matches!(
    name,
    "archive" | "list-archive-tags" | "reopen-archive" | "create-branch" | "drop-table"
  )
}

fn print_entry(key: &[u8], value: &[u8]) {
  println!(
    "{}\t{}",
    String::from_utf8_lossy(key),
    String::from_utf8_lossy(value)
  );
}

fn report_global(action: &str, reply: &GlobalReply) -> Result<()> {
  println!(
    "{action}\tok={}\tfailed={}\ttag={}\tname={}",
    reply.ok_num,
    reply.failed_li.len(),
    reply.tag,
    reply.name
  );
  for (tbl, err) in &reply.failed_li {
    eprintln!("{action} failed table={tbl} error={err}");
  }
  match reply.failed_li.first() {
    Some((_, err)) => Err(*err),
    None => Ok(()),
  }
}

async fn reopen_if_exists(store: &mut KvStore, tbl: &TableIdent) -> Result<bool> {
  match store.reopen(tbl).await {
    Ok(()) => Ok(true),
    Err(KvError::ResourceMissing) => Ok(false),
    Err(err) => Err(err),
  }
}

/// Local 便捷路径：单文件 `KvStore`，无需 attach。
///
/// `term` 必须与本地控制面（`run_runtime` 的 `attach_shard`）一致。
/// 只在 `store_path` 为空时调用：非空时数据根由配置决定，走 Runtime。
fn run_local(
  root: &Path,
  option: &KvOption,
  term: u64,
  name: &str,
  sub_match: &ArgMatches,
  tbl: &TableIdent,
) -> Result<()> {
  let runtime = CompioRuntime::new().map_err(|_| KvError::IoFail)?;
  runtime.block_on(async move {
    let mut store = KvStore::new(root, option, term)?;
    match name {
      "put" => {
        reopen_if_exists(&mut store, tbl).await?;
        let key = arg_str(sub_match, "key", "").as_bytes().to_vec();
        let value = arg_str(sub_match, "value", "").as_bytes().to_vec();
        store.put_kv(tbl, key, value).await?;
      }
      "get" => {
        if !reopen_if_exists(&mut store, tbl).await? {
          return Err(KvError::NotFound);
        }
        let key = arg_str(sub_match, "key", "");
        let value = store.read(tbl, key.as_bytes()).await?;
        println!("{}", String::from_utf8_lossy(&value));
      }
      "del" => {
        if !reopen_if_exists(&mut store, tbl).await? {
          return Err(KvError::NotFound);
        }
        store
          .delete(tbl, arg_str(sub_match, "key", "").as_bytes())
          .await?;
      }
      "scan" => {
        if !reopen_if_exists(&mut store, tbl).await? {
          return Ok(());
        }
        let limit = arg_str(sub_match, "limit", "0")
          .parse::<usize>()
          .map_err(|_| KvError::InvalidArgs)?;
        let entry_li = store
          .scan(
            tbl,
            arg_str(sub_match, "begin", "").as_bytes(),
            arg_str(sub_match, "end", "").as_bytes(),
            limit,
          )
          .await?;
        for (key, value) in entry_li {
          print_entry(&key, &value);
        }
      }
      "floor" => {
        if !reopen_if_exists(&mut store, tbl).await? {
          return Err(KvError::NotFound);
        }
        let entry = store
          .floor(tbl, arg_str(sub_match, "key", "").as_bytes())
          .await?;
        print_entry(&entry.key, &entry.value);
      }
      "truncate" => {
        if !reopen_if_exists(&mut store, tbl).await? {
          return Ok(());
        }
        let position = arg_str(sub_match, "position", "");
        if position.is_empty() {
          store.truncate(tbl).await?;
        } else {
          store.truncate_from(tbl, position.as_bytes()).await?;
        }
      }
      _ => return Err(KvError::InvalidArgs),
    }
    Ok(())
  })
}

/// Runtime 数据面命令。
///
/// 不显式 `reopen` 目标表：Runtime 的读路径会按模式处理「表尚未打开」——
/// Local / Mem 懒创建空表，Cloud / StandbyReplica 由 shard 的 `auto_reopen`
/// 等 manifest 可见。显式 `Reopen` 在 manifest 缺失时会被挂进延迟重试队列，
/// 一次 CLI 调用可能空等 `auto_reopen_retry_times × auto_reopen_pending_time`。
async fn dispatch_runtime(
  runtime: &Runtime,
  name: &str,
  sub_match: &ArgMatches,
  retain: u32,
) -> Result<()> {
  match name {
    "put" => {
      let tbl = table_of(sub_match)?;
      let key = arg_str(sub_match, "key", "").as_bytes().to_vec();
      let value = arg_str(sub_match, "value", "").as_bytes().to_vec();
      runtime
        .put(&tbl, vec![BatchEntry::upsert(key, value, now_ms())])
        .await
    }
    "get" => {
      let tbl = table_of(sub_match)?;
      let value = runtime
        .read(&tbl, arg_str(sub_match, "key", "").as_bytes())
        .await?;
      println!("{}", String::from_utf8_lossy(&value));
      Ok(())
    }
    "del" => {
      let tbl = table_of(sub_match)?;
      runtime
        .put(
          &tbl,
          vec![BatchEntry::delete(
            arg_str(sub_match, "key", "").as_bytes().to_vec(),
            now_ms(),
          )],
        )
        .await
    }
    "scan" => {
      let tbl = table_of(sub_match)?;
      let limit = arg_str(sub_match, "limit", "0")
        .parse::<usize>()
        .map_err(|_| KvError::InvalidArgs)?;
      let entry_li = runtime
        .scan(
          &tbl,
          arg_str(sub_match, "begin", "").as_bytes(),
          arg_str(sub_match, "end", "").as_bytes(),
          limit,
        )
        .await?;
      for (key, value) in entry_li {
        print_entry(&key, &value);
      }
      Ok(())
    }
    "floor" => {
      let tbl = table_of(sub_match)?;
      let entry = runtime
        .floor(&tbl, arg_str(sub_match, "key", "").as_bytes())
        .await?;
      print_entry(&entry.key, &entry.value);
      Ok(())
    }
    "truncate" => {
      let tbl = table_of(sub_match)?;
      let position = arg_str(sub_match, "position", "");
      if position.is_empty() {
        runtime.truncate(&tbl).await
      } else {
        runtime.truncate_from(&tbl, position.as_bytes()).await
      }
    }
    "archive" => {
      let reply = runtime
        .global_archive(arg_str(sub_match, "tag", ""), retain)
        .await?;
      report_global("archive", &reply)
    }
    "list-archive-tags" => {
      let entry_li = runtime
        .global_list_archive_tags(arg_str(sub_match, "prefix", ""))
        .await?;
      for entry in entry_li {
        println!("{}\t{}", entry.tag, entry.term);
      }
      Ok(())
    }
    "reopen-archive" => {
      let reply = runtime.global_reopen(arg_str(sub_match, "tag", "")).await?;
      report_global("reopen-archive", &reply)
    }
    "create-branch" => {
      let reply = runtime
        .global_create_branch(arg_str(sub_match, "branch", ""))
        .await?;
      report_global("create-branch", &reply)
    }
    "drop-table" => {
      let reply = runtime
        .global_drop_table(arg_str(sub_match, "name", ""))
        .await?;
      report_global("drop-table", &reply)
    }
    _ => Err(KvError::InvalidArgs),
  }
}

/// Runtime 路径：先 attach 全部 shard，再执行命令，最后确定性 shutdown。
fn run_runtime(
  runtime: Runtime,
  term: u64,
  retain: u32,
  name: &str,
  sub_match: &ArgMatches,
) -> Result<()> {
  let driver = CompioRuntime::new().map_err(|_| KvError::IoFail)?;
  driver.block_on(async move {
    let mut result = Ok(());
    for shard_id in 0..u32::from(runtime.num_threads()) {
      // `Busy` = attach 被更晚的控制操作接管（superseded），请求本身没失败；
      // 有界重放即可恢复视图，不让 CLI 直接退出。
      let attach =
        fkv_cli::attach_with_retry(|| runtime.attach_shard(shard_id, MAIN_BRANCH_NAME, term)).await;
      if let Err(err) = attach {
        result = Err(err);
        break;
      }
    }
    if result.is_ok() {
      result = dispatch_runtime(&runtime, name, sub_match, retain).await;
    }
    let shutdown = runtime.shutdown().await;
    result.and(shutdown)
  })
}

fn main() -> Result<()> {
  let Some(arg_match) = clap_args::parse!(|cmd: Command| {
    cmd
      .arg(
        arg!(--root <ROOT> "store 根目录（配置了 store_path 时以配置为准）")
          .default_value(".data/fkv"),
      )
      .arg(arg!(--config <FILE> "kvoptions ini").action(ArgAction::Set))
      .arg(
        arg!(--mode <MODE> "运行模式: auto/local/cloud/standby/standby-master/standby-replica/mem")
          .default_value("auto"),
      )
      .arg(arg!(--term <TERM> "attach 使用的 term（>=1）").default_value("1"))
      .subcommand(
        Command::new("put")
          .about("写入一个 key")
          .arg(Arg::new("table").long("table").required(true))
          .arg(Arg::new("key").long("key").required(true))
          .arg(Arg::new("value").long("value").required(true)),
      )
      .subcommand(
        Command::new("get")
          .about("读取一个 key")
          .arg(Arg::new("table").long("table").required(true))
          .arg(Arg::new("key").long("key").required(true)),
      )
      .subcommand(
        Command::new("del")
          .about("删除一个 key")
          .arg(Arg::new("table").long("table").required(true))
          .arg(Arg::new("key").long("key").required(true)),
      )
      .subcommand(
        Command::new("scan")
          .about("范围扫描")
          .arg(Arg::new("table").long("table").required(true))
          .arg(arg!(--begin <KEY> "起始 key").default_value(""))
          .arg(arg!(--end <KEY> "结束 key（exclusive）").default_value(""))
          .arg(arg!(--limit <N> "最多条数，0 不限").default_value("0")),
      )
      .subcommand(
        Command::new("floor")
          .about("最大且 <= key 的 entry")
          .arg(Arg::new("table").long("table").required(true))
          .arg(Arg::new("key").long("key").required(true)),
      )
      .subcommand(
        Command::new("truncate")
          .about("删除 >= position 的 key；position 为空时清空整表")
          .arg(Arg::new("table").long("table").required(true))
          .arg(arg!(--position <KEY> "截断位置").default_value("")),
      )
      .subcommand(
        Command::new("archive")
          .about("全局归档（Runtime::global_archive）")
          .arg(arg!(--tag <TAG> "归档 tag，空则由运行时生成").default_value("")),
      )
      .subcommand(
        Command::new("list-archive-tags")
          .about("列出归档 tag（Runtime::global_list_archive_tags）")
          .arg(arg!(--prefix <TAG> "tag 前缀").default_value("")),
      )
      .subcommand(
        Command::new("reopen-archive")
          .about("按 tag 重开归档（Runtime::global_reopen）")
          .arg(Arg::new("tag").long("tag").required(true)),
      )
      .subcommand(
        Command::new("create-branch")
          .about("创建分支（Runtime::global_create_branch）")
          .arg(Arg::new("branch").long("branch").required(true)),
      )
      .subcommand(
        Command::new("drop-table")
          .about("删除整表（Runtime::global_drop_table）")
          .arg(Arg::new("name").long("name").required(true)),
      )
  }) else {
    return Ok(());
  };

  let root = Path::new(arg_str(&arg_match, "root", ".data/fkv"));
  let Some((name, sub_match)) = arg_match.subcommand() else {
    println!(
      "请使用 put/get/del/scan/floor/truncate/archive/list-archive-tags/reopen-archive/create-branch/drop-table"
    );
    return Ok(());
  };
  let mode = Mode::parse(arg_str(&arg_match, "mode", "auto"))?;
  let term = term_of(&arg_match)?;
  if mode == Mode::Mem {
    // 内存模式没有 `KvOption` 装载路径：显式 `--config` 无法生效，直接拒绝，
    // 不能静默丢掉调用方以为生效的配置。
    if !arg_str(&arg_match, "config", "").is_empty() {
      eprintln!("--mode mem 不支持 --config（内存模式无 KvOption 装载路径）");
      return Err(KvError::InvalidArgs);
    }
    let runtime = start_mem_runtime(root)?;
    let retain = KvOption::default().num_retained_archives;
    return run_runtime(runtime, term, retain, name, sub_match);
  }
  let mut option = options_of(&arg_match)?;
  let retain = option.num_retained_archives;
  apply_mode(mode, &mut option, root)?;
  // `store_path` 非空（单根或多根）时不能走 SDK：SDK 只装配单根且要求
  // `store_path[0] == --root`，否则 `InvalidArgs`（默认 `--root=.data/fkv`
  // 与生产 ini 的 `store_path` 必然不等）。与 `archive` 等控制面命令统一走
  // Runtime，同一份配置不再有两套行为。
  if option.derive_mode() == StoreMode::Local
    && option.store_path.is_empty()
    && !is_runtime_command(name)
  {
    let tbl = table_of(sub_match)?;
    return run_local(root, &option, term, name, sub_match, &tbl);
  }
  let runtime = start_runtime(root, &option)?;
  run_runtime(runtime, term, retain, name, sub_match)
}
