//! fkv 本地检查 CLI：put / get / del / scan / floor / truncate。

use std::{path::Path, str::FromStr};

use clap_args::{
  ArgAction, arg,
  clap::{Arg, ArgMatches, Command},
};
use compio::runtime::Runtime;
use fkv_config::KvOptions;
use fkv_error::{KvError, Result};
use fkv_sdk::KvStore;
use fkv_types::TableIdent;
use rusty_alloc_default as _;

fn arg_str<'a>(arg_match: &'a ArgMatches, key: &str, default: &'a str) -> &'a str {
  arg_match
    .get_one::<String>(key)
    .map(String::as_str)
    .unwrap_or(default)
}

fn table_of(arg_match: &ArgMatches) -> Result<TableIdent> {
  let table = arg_str(arg_match, "table", "");
  TableIdent::from_str(table)
}

fn options_of(arg_match: &ArgMatches) -> Result<KvOptions> {
  let mut options = KvOptions::default();
  let config = arg_str(arg_match, "config", "");
  if !config.is_empty() {
    options.load_from_ini(config)?;
  }
  Ok(options)
}

async fn reopen_if_exists(store: &mut KvStore, tbl: &TableIdent) -> Result<bool> {
  match store.reopen(tbl).await {
    Ok(()) => Ok(true),
    Err(KvError::ResourceMissing) => Ok(false),
    Err(err) => Err(err),
  }
}

fn print_entry(key: &[u8], value: &[u8]) {
  println!(
    "{}\t{}",
    String::from_utf8_lossy(key),
    String::from_utf8_lossy(value)
  );
}

fn main() -> Result<()> {
  let Some(arg_match) = clap_args::parse!(|cmd: Command| {
    cmd
      .arg(arg!(--root <ROOT> "store 根目录").default_value(".data/fkv"))
      .arg(arg!(--config <FILE> "kvoptions ini").action(ArgAction::Set))
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
  }) else {
    return Ok(());
  };

  let options = options_of(&arg_match)?;
  let root = Path::new(arg_str(&arg_match, "root", ".data/fkv"));
  let Some((name, sub_match)) = arg_match.subcommand() else {
    println!("请使用 put/get/del/scan/floor/truncate");
    return Ok(());
  };
  let tbl = table_of(sub_match)?;
  let runtime = Runtime::new().map_err(|_| KvError::IoFail)?;
  runtime.block_on(async move {
    let mut store = KvStore::new(root, &options)?;
    match name {
      "put" => {
        reopen_if_exists(&mut store, &tbl).await?;
        let key = arg_str(sub_match, "key", "").as_bytes().to_vec();
        let value = arg_str(sub_match, "value", "").as_bytes().to_vec();
        store.put_kv(&tbl, key, value).await?;
      }
      "get" => {
        if !reopen_if_exists(&mut store, &tbl).await? {
          return Err(KvError::NotFound);
        }
        let key = arg_str(sub_match, "key", "");
        let value = store.read(&tbl, key.as_bytes()).await?;
        println!("{}", String::from_utf8_lossy(&value));
      }
      "del" => {
        if !reopen_if_exists(&mut store, &tbl).await? {
          return Err(KvError::NotFound);
        }
        store
          .delete(&tbl, arg_str(sub_match, "key", "").as_bytes())
          .await?;
      }
      "scan" => {
        if !reopen_if_exists(&mut store, &tbl).await? {
          return Ok(());
        }
        let limit = arg_str(sub_match, "limit", "0")
          .parse::<usize>()
          .map_err(|_| KvError::InvalidArgs)?;
        let entry_li = store
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
      }
      "floor" => {
        if !reopen_if_exists(&mut store, &tbl).await? {
          return Err(KvError::NotFound);
        }
        let entry = store
          .floor(&tbl, arg_str(sub_match, "key", "").as_bytes())
          .await?;
        print_entry(&entry.key, &entry.value);
      }
      "truncate" => {
        if !reopen_if_exists(&mut store, &tbl).await? {
          return Ok(());
        }
        let position = arg_str(sub_match, "position", "");
        if position.is_empty() {
          store.truncate(&tbl).await?;
        } else {
          store.truncate_from(&tbl, position.as_bytes()).await?;
        }
      }
      _ => return Err(KvError::InvalidArgs),
    }
    Ok(())
  })
}
