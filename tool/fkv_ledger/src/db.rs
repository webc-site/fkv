use std::{fs::create_dir_all, path::Path};

use motedb::{Database, types::Value};

use crate::error::Result;

/// 账本 schema。
///
/// 设计原则：
/// - `unit` 是迁移单元主表，id 稳定，可跨多次 scan 复用；
/// - `edge` 记录 include / call / format 依赖；
/// - `claim` / `evidence` / `run` 记录认领与验证证据；
/// - `meta` 记录 schema 版本与兼容基线。
const SCHEMA_SQL_LI: &[&str] = &[
  "CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v TEXT)",
  "CREATE TABLE IF NOT EXISTS unit (\
     id TEXT PRIMARY KEY, path TEXT, symbol TEXT, kind TEXT, \
     layer TEXT, crate TEXT, module TEXT, start_line INT, end_line INT, \
     priority INT, risk TEXT, status TEXT, owner TEXT, compat TEXT, \
     source_hash TEXT, created_at INT, updated_at INT)",
  "CREATE TABLE IF NOT EXISTS edge (\
     id TEXT PRIMARY KEY, from_id TEXT, to_id TEXT, kind TEXT)",
  "CREATE TABLE IF NOT EXISTS evidence (\
     id TEXT PRIMARY KEY, unit_id TEXT, kind TEXT, path TEXT, \
     result TEXT, commit_id TEXT, created_at INT)",
  "CREATE TABLE IF NOT EXISTS claim (\
     id TEXT PRIMARY KEY, unit_id TEXT, owner TEXT, status TEXT, \
     branch TEXT, pr TEXT, claimed_at INT, released_at INT)",
  "CREATE TABLE IF NOT EXISTS run (\
     id TEXT PRIMARY KEY, unit_id TEXT, suite TEXT, impl TEXT, \
     result TEXT, metrics TEXT, commit_id TEXT, created_at INT)",
  "CREATE TABLE IF NOT EXISTS note (\
     id TEXT PRIMARY KEY, unit_id TEXT, author TEXT, body TEXT, created_at INT)",
  "CREATE TABLE IF NOT EXISTS rule (\
     id TEXT PRIMARY KEY, prefix TEXT, crate TEXT, layer TEXT)",
  "CREATE TABLE IF NOT EXISTS include_edge (\
     id TEXT PRIMARY KEY, path TEXT, inc_path TEXT)",
];

/// 打开或创建 motedb 账本，并确保 schema 存在。
pub fn open(path: &Path) -> Result<Database> {
  if let Some(parent) = path.parent() {
    create_dir_all(parent)?;
  }
  let db = if path.exists() {
    Database::open(path)?
  } else {
    Database::create(path)?
  };
  for sql in SCHEMA_SQL_LI {
    db.execute(sql)?.materialize()?;
  }
  db.execute_prepared(
    "DELETE FROM meta WHERE k = ?",
    vec![Value::text("schema_version".to_string())],
  )?
  .materialize()?;
  db.execute_prepared(
    "INSERT INTO meta (k, v) VALUES (?, ?)",
    vec![
      Value::text("schema_version".to_string()),
      Value::text("3".to_string()),
    ],
  )?
  .materialize()?;
  Ok(db)
}
