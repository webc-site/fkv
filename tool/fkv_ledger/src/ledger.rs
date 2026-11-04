use motedb::{Database, types::Value};

use crate::{
  error::{LedgerError, Result},
  util::{cell_text, now_secs},
};

const STATUS_LI: &[&str] = &[
  "todo",
  "claimed",
  "in_progress",
  "review",
  "done",
  "verified",
  "blocked",
  "ignored",
];

pub fn list(db: &Database, status: Option<&str>, crate_name: Option<&str>) -> Result<()> {
  let mut sql = String::from("SELECT id, symbol, kind, status, owner FROM unit");
  let mut cond_li = Vec::new();
  let mut param_li = Vec::new();
  if let Some(status) = status {
    cond_li.push("status = ?");
    param_li.push(Value::text(status.to_owned()));
  }
  if let Some(crate_name) = crate_name {
    cond_li.push("crate = ?");
    param_li.push(Value::text(crate_name.to_owned()));
  }
  if !cond_li.is_empty() {
    sql.push_str(" WHERE ");
    sql.push_str(&cond_li.join(" AND "));
  }
  sql.push_str(" ORDER BY path, start_line LIMIT 200");

  let result = db.execute_prepared(&sql, param_li)?.materialize()?;
  if let Some((column_li, row_li)) = result.select_rows() {
    println!("{}", column_li.join(" | "));
    for row in row_li {
      let cell_li: Vec<String> = row.iter().map(cell_text).collect();
      println!("{}", cell_li.join(" | "));
    }
  }
  Ok(())
}

pub fn claim(db: &Database, unit_id: &str, owner: &str) -> Result<()> {
  let result = db
    .execute_prepared(
      "UPDATE unit SET owner = ?, status = 'claimed', updated_at = ? WHERE id = ?",
      vec![
        Value::text(owner.to_owned()),
        Value::Integer(now_secs()),
        Value::text(unit_id.to_owned()),
      ],
    )?
    .materialize()?;
  println!("claimed {} unit", result.affected_rows());
  Ok(())
}

pub fn set_status(db: &Database, unit_id: &str, status: &str) -> Result<()> {
  if !STATUS_LI.contains(&status) {
    return Err(LedgerError::Invalid(format!(
      "unknown status {status}, expected one of {STATUS_LI:?}"
    )));
  }
  let result = db
    .execute_prepared(
      "UPDATE unit SET status = ?, updated_at = ? WHERE id = ?",
      vec![
        Value::text(status.to_owned()),
        Value::Integer(now_secs()),
        Value::text(unit_id.to_owned()),
      ],
    )?
    .materialize()?;
  println!("updated {} unit -> {status}", result.affected_rows());
  Ok(())
}

pub fn ignore(db: &Database, unit_id: &str, reason: &str) -> Result<()> {
  set_status(db, unit_id, "ignored")?;
  if !reason.is_empty() {
    let note_id = format!("{unit_id}#ignore#{}", now_secs());
    db.execute_prepared(
      "INSERT INTO note (id, unit_id, author, body, created_at) VALUES (?, ?, ?, ?, ?)",
      vec![
        Value::text(note_id),
        Value::text(unit_id.to_owned()),
        Value::text("cli".to_owned()),
        Value::text(reason.to_owned()),
        Value::Integer(now_secs()),
      ],
    )?
    .materialize()?;
  }
  Ok(())
}

pub fn stats(db: &Database) -> Result<()> {
  let mut total = 0;
  for status in STATUS_LI {
    let result = db
      .execute_prepared(
        "SELECT COUNT(*) FROM unit WHERE status = ?",
        vec![Value::text((*status).to_owned())],
      )?
      .materialize()?;
    if let Some((_, row_li)) = result.select_rows()
      && let Some(row) = row_li.first()
      && let Some(Value::Integer(count)) = row.first()
    {
      total += *count;
      println!("{status}: {count}");
    }
  }
  let result = db.execute("SELECT COUNT(*) FROM unit")?.materialize()?;
  if let Some((_, row_li)) = result.select_rows()
    && let Some(row) = row_li.first()
    && let Some(Value::Integer(count)) = row.first()
  {
    println!("total: {count}");
  }
  println!("status_sum: {total}");
  Ok(())
}

pub fn next(db: &Database, limit: usize, crate_name: Option<&str>) -> Result<()> {
  let mut sql = String::from(
    "SELECT id, path, symbol, kind FROM unit WHERE (status = 'todo' OR status = 'claimed')",
  );
  let mut param_li = Vec::new();
  if let Some(crate_name) = crate_name {
    sql.push_str(" AND crate = ?");
    param_li.push(Value::text(crate_name.to_owned()));
  }
  sql.push_str(&format!(
    " ORDER BY priority, path, start_line LIMIT {limit}"
  ));

  let result = db.execute_prepared(&sql, param_li)?.materialize()?;
  if let Some((column_li, row_li)) = result.select_rows() {
    println!("{}", column_li.join(" | "));
    for row in row_li {
      let cell_li: Vec<String> = row.iter().map(cell_text).collect();
      println!("{}", cell_li.join(" | "));
    }
  }
  Ok(())
}

pub fn evidence_add(
  db: &Database,
  unit_id: &str,
  kind: &str,
  path: &str,
  result: &str,
) -> Result<()> {
  let evidence_id = format!("{unit_id}#{kind}#{}", now_secs());
  db.execute_prepared(
    "INSERT INTO evidence (id, unit_id, kind, path, result, commit_id, created_at) \
     VALUES (?, ?, ?, ?, ?, ?, ?)",
    vec![
      Value::text(evidence_id),
      Value::text(unit_id.to_owned()),
      Value::text(kind.to_owned()),
      Value::text(path.to_owned()),
      Value::text(result.to_owned()),
      Value::text("".to_owned()),
      Value::Integer(now_secs()),
    ],
  )?
  .materialize()?;
  println!("evidence added for {unit_id}: {kind} {result}");
  Ok(())
}

pub fn evidence_list(db: &Database, unit_id: &str) -> Result<()> {
  let result = db
    .execute_prepared(
      "SELECT kind, path, result, created_at FROM evidence WHERE unit_id = ?",
      vec![Value::text(unit_id.to_owned())],
    )?
    .materialize()?;
  if let Some((column_li, row_li)) = result.select_rows() {
    println!("{}", column_li.join(" | "));
    for row in row_li {
      let cell_li: Vec<String> = row.iter().map(cell_text).collect();
      println!("{}", cell_li.join(" | "));
    }
  }
  Ok(())
}

pub fn edges(db: &Database, unit_id: &str) -> Result<()> {
  let path = unit_id.split('#').next().unwrap_or(unit_id);
  let result = db
    .execute_prepared(
      "SELECT inc_path FROM include_edge WHERE path = ?",
      vec![Value::text(path.to_owned())],
    )?
    .materialize()?;
  if let Some((_, row_li)) = result.select_rows() {
    for row in row_li {
      if let Some(value) = row.first() {
        println!("{}", cell_text(value));
      }
    }
  }
  Ok(())
}
