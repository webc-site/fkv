//! fkv 迁移账本。
//!
//! 纯 CLI：sync / scan / baseline / map / next / list / claim / status / ignore /
//! evidence / edges / stats / db。
//! 数据库固定 `.data/motedb.mote`，已在 `.gitignore` 中。

use std::path::Path;

use clap_args::{
  arg,
  clap::{Arg, ArgAction, ArgMatches, Command},
};
// rusty_alloc_default 在被链接时安装为全局分配器。
use rusty_alloc_default as _;

mod baseline;
mod db;
mod error;
mod ledger;
mod map;
mod scan;
mod util;

fn arg_str<'a>(arg_match: &'a ArgMatches, key: &str, default: &'a str) -> &'a str {
  arg_match
    .get_one::<String>(key)
    .map(String::as_str)
    .unwrap_or(default)
}

fn db_path(arg_match: &ArgMatches) -> &str {
  arg_str(arg_match, "db", ".data/motedb.mote")
}

fn main() -> error::Result<()> {
  let Some(arg_match) = clap_args::parse!(|cmd: Command| {
    cmd
      .subcommand(
        Command::new("scan")
          .about("用 tree-sitter 扫描 eloqstore 并写入迁移账本")
          .arg(arg!(--root <ROOT> "eloqstore 根目录").default_value("../eloqstore"))
          .arg(arg!(--db <DB> "motedb 文件").default_value(".data/motedb.mote")),
      )
      .subcommand(
        Command::new("baseline")
          .about("读取 / 更新 COMPAT_BASELINE 到最新 main")
          .arg(arg!(--repo <REPO> "eloqstore 仓库").default_value("../eloqstore"))
          .arg(arg!(--ref <REF> "git ref").default_value("main"))
          .arg(arg!(--file <FILE> "基线文件").default_value("COMPAT_BASELINE"))
          .arg(arg!(--write "写入基线文件").action(ArgAction::SetTrue))
          .arg(arg!(--offline "不执行 git fetch").action(ArgAction::SetTrue)),
      )
      .subcommand(
        Command::new("sync")
          .about("更新基线 + scan + crate 映射")
          .arg(arg!(--repo <REPO> "eloqstore 仓库").default_value("../eloqstore"))
          .arg(arg!(--root <ROOT> "eloqstore 根目录").default_value("../eloqstore"))
          .arg(arg!(--ref <REF> "git ref").default_value("main"))
          .arg(arg!(--file <FILE> "基线文件").default_value("COMPAT_BASELINE"))
          .arg(arg!(--db <DB> "motedb 文件").default_value(".data/motedb.mote"))
          .arg(arg!(--offline "不执行 git fetch").action(ArgAction::SetTrue)),
      )
      .subcommand(
        Command::new("map")
          .about("按 path 规则映射 unit -> crate")
          .arg(arg!(--seed "重置并写入默认规则").action(ArgAction::SetTrue))
          .arg(arg!(--force "覆盖已有 crate").action(ArgAction::SetTrue))
          .arg(arg!(--db <DB> "motedb 文件").default_value(".data/motedb.mote")),
      )
      .subcommand(
        Command::new("next")
          .about("列出下一批迁移任务")
          .arg(arg!(--limit <LIMIT> "数量").default_value("20"))
          .arg(arg!(--crate <CRATE> "按 crate 过滤").action(ArgAction::Set))
          .arg(arg!(--db <DB> "motedb 文件").default_value(".data/motedb.mote")),
      )
      .subcommand(
        Command::new("list")
          .about("列出迁移任务")
          .arg(arg!(--status <STATUS> "todo / claimed / ...").action(ArgAction::Set))
          .arg(arg!(--crate <CRATE> "按 crate 过滤").action(ArgAction::Set))
          .arg(arg!(--db <DB> "motedb 文件").default_value(".data/motedb.mote")),
      )
      .subcommand(
        Command::new("claim")
          .about("认领任务")
          .arg(Arg::new("unit").required(true))
          .arg(
            arg!(--owner <OWNER> "认领人")
              .action(ArgAction::Set)
              .required(true),
          )
          .arg(arg!(--db <DB> "motedb 文件").default_value(".data/motedb.mote")),
      )
      .subcommand(
        Command::new("status")
          .about("修改 unit 状态")
          .arg(Arg::new("unit").required(true))
          .arg(Arg::new("status").required(true))
          .arg(arg!(--db <DB> "motedb 文件").default_value(".data/motedb.mote")),
      )
      .subcommand(
        Command::new("ignore")
          .about("忽略不需要迁移的 unit")
          .arg(Arg::new("unit").required(true))
          .arg(arg!(--reason <REASON> "忽略原因").action(ArgAction::Set))
          .arg(arg!(--db <DB> "motedb 文件").default_value(".data/motedb.mote")),
      )
      .subcommand(
        Command::new("evidence")
          .about("迁移证据")
          .subcommand(
            Command::new("add")
              .about("添加 evidence")
              .arg(Arg::new("unit").required(true))
              .arg(Arg::new("kind").required(true))
              .arg(arg!(--path <PATH> "证据路径").action(ArgAction::Set))
              .arg(arg!(--result <RESULT> "pass / fail / skip").action(ArgAction::Set))
              .arg(arg!(--db <DB> "motedb 文件").default_value(".data/motedb.mote")),
          )
          .subcommand(
            Command::new("list")
              .about("列出 evidence")
              .arg(Arg::new("unit").required(true))
              .arg(arg!(--db <DB> "motedb 文件").default_value(".data/motedb.mote")),
          ),
      )
      .subcommand(
        Command::new("edges")
          .about("查看 unit 所在文件的 include 依赖")
          .arg(Arg::new("unit").required(true))
          .arg(arg!(--db <DB> "motedb 文件").default_value(".data/motedb.mote")),
      )
      .subcommand(
        Command::new("stats")
          .about("按状态统计")
          .arg(arg!(--db <DB> "motedb 文件").default_value(".data/motedb.mote")),
      )
      .subcommand(
        Command::new("db")
          .about("打开 / 初始化迁移账本")
          .arg(arg!(--db <DB> "motedb 文件").default_value(".data/motedb.mote")),
      )
  }) else {
    return Ok(());
  };

  match arg_match.subcommand() {
    Some(("scan", m)) => {
      let root = arg_str(m, "root", "../eloqstore");
      let db_path = db_path(m);
      let db = db::open(Path::new(db_path))?;
      let unit_count = scan::scan(Path::new(root), &db)?;
      println!("scan {root}: {unit_count} unit 已写入 {db_path}");
    }
    Some(("baseline", m)) => {
      let repo = arg_str(m, "repo", "../eloqstore");
      let reference = arg_str(m, "ref", "main");
      let file = arg_str(m, "file", "COMPAT_BASELINE");
      let old = baseline::read(Path::new(file))?;
      let (commit, date) = baseline::latest(Path::new(repo), reference, !m.get_flag("offline"))?;
      let old_desc = old
        .as_ref()
        .map(|value| format!("{} @ {}", value.commit, value.date))
        .unwrap_or_else(|| "<none>".to_owned());
      println!("baseline: {old_desc} -> {commit} ({date})");
      if m.get_flag("write") {
        let level = old
          .as_ref()
          .map(|value| value.level.as_str())
          .unwrap_or("L1");
        baseline::write(Path::new(file), reference, &commit, &date, level)?;
        println!("written {file}");
      }
    }
    Some(("sync", m)) => {
      let repo = arg_str(m, "repo", "../eloqstore");
      let root = arg_str(m, "root", "../eloqstore");
      let reference = arg_str(m, "ref", "main");
      let file = arg_str(m, "file", "COMPAT_BASELINE");
      let db_path = db_path(m);
      let old = baseline::read(Path::new(file))?;
      let (commit, date) = baseline::latest(Path::new(repo), reference, !m.get_flag("offline"))?;
      let level = old
        .as_ref()
        .map(|value| value.level.as_str())
        .unwrap_or("L1");
      baseline::write(Path::new(file), reference, &commit, &date, level)?;
      let db = db::open(Path::new(db_path))?;
      let unit_count = scan::scan(Path::new(root), &db)?;
      map::seed(&db)?;
      let mapped = map::apply(&db, false)?;
      println!("sync {commit}: {unit_count} unit, {mapped} mapped -> {db_path}");
    }
    Some(("map", m)) => {
      let db_path = db_path(m);
      let db = db::open(Path::new(db_path))?;
      if m.get_flag("seed") {
        let rule_count = map::seed(&db)?;
        println!("seeded {rule_count} rule");
      }
      let mapped = map::apply(&db, m.get_flag("force"))?;
      println!("mapped {mapped} unit");
    }
    Some(("next", m)) => {
      let db_path = db_path(m);
      let db = db::open(Path::new(db_path))?;
      let limit = arg_str(m, "limit", "20").parse().unwrap_or(20);
      ledger::next(&db, limit, m.get_one::<String>("crate").map(String::as_str))?;
    }
    Some(("list", m)) => {
      let db_path = db_path(m);
      let db = db::open(Path::new(db_path))?;
      ledger::list(
        &db,
        m.get_one::<String>("status").map(String::as_str),
        m.get_one::<String>("crate").map(String::as_str),
      )?;
    }
    Some(("claim", m)) => {
      let db_path = db_path(m);
      let db = db::open(Path::new(db_path))?;
      ledger::claim(&db, arg_str(m, "unit", ""), arg_str(m, "owner", ""))?;
    }
    Some(("status", m)) => {
      let db_path = db_path(m);
      let db = db::open(Path::new(db_path))?;
      ledger::set_status(&db, arg_str(m, "unit", ""), arg_str(m, "status", ""))?;
    }
    Some(("ignore", m)) => {
      let db_path = db_path(m);
      let db = db::open(Path::new(db_path))?;
      ledger::ignore(&db, arg_str(m, "unit", ""), arg_str(m, "reason", ""))?;
    }
    Some(("evidence", m)) => match m.subcommand() {
      Some(("add", sub)) => {
        let db_path = db_path(sub);
        let db = db::open(Path::new(db_path))?;
        ledger::evidence_add(
          &db,
          arg_str(sub, "unit", ""),
          arg_str(sub, "kind", ""),
          arg_str(sub, "path", ""),
          arg_str(sub, "result", ""),
        )?;
      }
      Some(("list", sub)) => {
        let db_path = db_path(sub);
        let db = db::open(Path::new(db_path))?;
        ledger::evidence_list(&db, arg_str(sub, "unit", ""))?;
      }
      _ => eprintln!("use evidence add / list"),
    },
    Some(("edges", m)) => {
      let db_path = db_path(m);
      let db = db::open(Path::new(db_path))?;
      ledger::edges(&db, arg_str(m, "unit", ""))?;
    }
    Some(("stats", m)) => {
      let db_path = db_path(m);
      let db = db::open(Path::new(db_path))?;
      ledger::stats(&db)?;
    }
    Some(("db", m)) => {
      let db_path = db_path(m);
      let _db = db::open(Path::new(db_path))?;
      println!("motedb: {db_path}");
    }
    _ => eprintln!("use --help"),
  }
  Ok(())
}
