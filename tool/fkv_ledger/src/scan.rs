use std::{
  cell::RefCell,
  collections::hash_map::DefaultHasher,
  fs::{read, read_dir},
  hash::{Hash, Hasher},
  path::{Path, PathBuf},
};

use gxhash::HashSet;
use motedb::{Database, types::Value};
use tree_sitter::{Node, Parser};

use crate::{
  error::{LedgerError, Result},
  util::{now_secs, text_of},
};

struct ScanCtx<'a> {
  source: &'a [u8],
  rel: &'a str,
  source_hash: &'a str,
  db: &'a Database,
  now: i64,
  existing: &'a RefCell<HashSet<String>>,
  include_seen: &'a RefCell<HashSet<String>>,
}

/// 递归扫描 C++ 头文件与源文件。
///
/// 粒度：
/// - 一个 `class` / `struct` 一个 unit，类内方法归到 class unit；
/// - 文件作用域的 `function_definition` 一个 unit；
/// - 类外定义的 `Foo::bar` 仍按 function unit 处理。
pub fn scan(root: &Path, db: &Database) -> Result<usize> {
  let mut parser = Parser::new();
  parser
    .set_language(&tree_sitter_cpp::LANGUAGE.into())
    .map_err(|e| LedgerError::TreeSitter(e.to_string()))?;

  let mut file_li = Vec::new();
  collect(&root.join("include"), &mut file_li)?;
  collect(&root.join("src"), &mut file_li)?;

  let existing = RefCell::new(load_existing(db)?);
  let include_seen = RefCell::new(HashSet::default());
  let tx = db.begin_transaction()?;
  db.execute("DELETE FROM include_edge")?.materialize()?;

  let now = now_secs();
  let mut unit_count = 0;
  for file in file_li {
    let source = read(&file)?;
    let source_hash = hash_bytes(&source);
    let rel = file
      .strip_prefix(root)
      .unwrap_or(&file)
      .to_string_lossy()
      .into_owned();
    let Some(tree) = parser.parse(&source, None) else {
      continue;
    };
    let ctx = ScanCtx {
      source: &source,
      rel: &rel,
      source_hash: &source_hash,
      db,
      now,
      existing: &existing,
      include_seen: &include_seen,
    };
    visit(tree.root_node(), &ctx, false, false, &mut unit_count)?;
  }
  db.commit_transaction(tx)?;
  Ok(unit_count)
}

fn visit(
  node: Node<'_>,
  ctx: &ScanCtx<'_>,
  in_class: bool,
  in_function: bool,
  unit_count: &mut usize,
) -> Result<()> {
  let kind = node.kind();
  if kind == "preproc_include" {
    insert_include(node, ctx)?;
  }
  let is_class = matches!(kind, "class_specifier" | "struct_specifier");
  let is_function = kind == "function_definition";

  let unit_kind = if is_class && node.child_by_field_name("body").is_some() {
    Some(kind)
  } else if is_function && !in_function {
    Some(if in_class { "method" } else { "function" })
  } else {
    None
  };
  if let Some(unit_kind) = unit_kind {
    insert_unit(node, ctx, unit_kind)?;
    *unit_count += 1;
  }

  let next_in_class = in_class || is_class;
  let next_in_function = in_function || is_function;
  let mut cursor = node.walk();
  for child in node.children(&mut cursor) {
    visit(child, ctx, next_in_class, next_in_function, unit_count)?;
  }
  Ok(())
}

fn load_existing(db: &Database) -> Result<HashSet<String>> {
  let mut set = HashSet::default();
  let result = db.execute("SELECT id FROM unit")?.materialize()?;
  if let Some((_, row_li)) = result.select_rows() {
    for row in row_li {
      if let Some(id) = row.first().and_then(text_of) {
        set.insert(id.to_owned());
      }
    }
  }
  Ok(set)
}

fn insert_unit(node: Node<'_>, ctx: &ScanCtx<'_>, unit_kind: &str) -> Result<()> {
  let local = symbol_of(node, ctx.source)
    .unwrap_or_else(|| format!("{}@{}", node.kind(), node.start_position().row + 1));
  let symbol = match qualified_name(node, ctx.source) {
    Some(prefix) if !prefix.ends_with(&format!("::{local}")) && prefix != local => {
      format!("{prefix}::{local}")
    }
    Some(prefix) => prefix,
    None => local,
  };
  let id = format!("{}#{unit_kind}:{symbol}", ctx.rel);
  let start = node.start_position().row as i64 + 1;
  let end = node.end_position().row as i64 + 1;

  let exists = ctx.existing.borrow().contains(&id);

  if exists {
    ctx
      .db
      .execute_prepared(
        "UPDATE unit SET path = ?, symbol = ?, start_line = ?, end_line = ?, \
           source_hash = ?, updated_at = ? WHERE id = ?",
        vec![
          Value::text(ctx.rel.to_owned()),
          Value::text(symbol),
          Value::Integer(start),
          Value::Integer(end),
          Value::text(ctx.source_hash.to_owned()),
          Value::Integer(ctx.now),
          Value::text(id),
        ],
      )?
      .materialize()?;
  } else {
    ctx
      .db
      .execute_prepared(
        "INSERT INTO unit (\
           id, path, symbol, kind, start_line, end_line, priority, risk, \
           status, owner, compat, source_hash, created_at, updated_at\
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        vec![
          Value::text(id.clone()),
          Value::text(ctx.rel.to_owned()),
          Value::text(symbol),
          Value::text(unit_kind.to_owned()),
          Value::Integer(start),
          Value::Integer(end),
          Value::Integer(0),
          Value::text("medium".to_owned()),
          Value::text("todo".to_owned()),
          Value::Null,
          Value::text("L0".to_owned()),
          Value::text(ctx.source_hash.to_owned()),
          Value::Integer(ctx.now),
          Value::Integer(ctx.now),
        ],
      )?
      .materialize()?;
    ctx.existing.borrow_mut().insert(id);
  }
  Ok(())
}

fn insert_include(node: Node<'_>, ctx: &ScanCtx<'_>) -> Result<()> {
  let Some(path_node) = node.child_by_field_name("path") else {
    return Ok(());
  };
  let Ok(raw) = path_node.utf8_text(ctx.source) else {
    return Ok(());
  };
  let inc_path = raw.trim_matches(|c| c == '"' || c == '<' || c == '>');
  if inc_path.is_empty() {
    return Ok(());
  }
  let id = format!("{}->{inc_path}", ctx.rel);
  if !ctx.include_seen.borrow_mut().insert(id.clone()) {
    return Ok(());
  }
  ctx
    .db
    .execute_prepared(
      "INSERT INTO include_edge (id, path, inc_path) VALUES (?, ?, ?)",
      vec![
        Value::text(id),
        Value::text(ctx.rel.to_owned()),
        Value::text(inc_path.to_owned()),
      ],
    )?
    .materialize()?;
  Ok(())
}

fn qualified_name(node: Node<'_>, source: &[u8]) -> Option<String> {
  let mut part_li = Vec::new();
  let mut cur = Some(node);
  while let Some(n) = cur {
    if matches!(
      n.kind(),
      "class_specifier" | "struct_specifier" | "namespace_definition"
    ) && let Some(name) = n.child_by_field_name("name")
      && let Ok(text) = name.utf8_text(source)
    {
      part_li.push(text.to_owned());
    }
    cur = n.parent();
  }
  if part_li.is_empty() {
    return None;
  }
  part_li.reverse();
  Some(part_li.join("::"))
}

fn symbol_of(node: Node<'_>, source: &[u8]) -> Option<String> {
  if let Some(name) = node.child_by_field_name("name") {
    return name.utf8_text(source).ok().map(str::to_owned);
  }
  let mut cur = node.child_by_field_name("declarator")?;
  loop {
    if matches!(
      cur.kind(),
      "identifier" | "field_identifier" | "qualified_identifier" | "destructor_name"
    ) {
      return cur.utf8_text(source).ok().map(str::to_owned);
    }
    let Some(next) = cur.child_by_field_name("declarator") else {
      let mut node_stack = vec![cur];
      while let Some(n) = node_stack.pop() {
        if matches!(
          n.kind(),
          "identifier" | "field_identifier" | "qualified_identifier"
        ) {
          return n.utf8_text(source).ok().map(str::to_owned);
        }
        let mut cursor = n.walk();
        node_stack.extend(n.children(&mut cursor));
      }
      return None;
    };
    cur = next;
  }
}

fn hash_bytes(bytes: &[u8]) -> String {
  let mut hasher = DefaultHasher::new();
  bytes.hash(&mut hasher);
  format!("{:016x}", hasher.finish())
}

fn collect(dir: &Path, out_li: &mut Vec<PathBuf>) -> Result<()> {
  if !dir.is_dir() {
    return Ok(());
  }
  for entry in read_dir(dir)? {
    let path = entry?.path();
    if path.is_dir() {
      collect(&path, out_li)?;
    } else if matches!(
      path.extension().and_then(|e| e.to_str()),
      Some("h" | "hpp" | "hh" | "cc" | "cpp" | "cxx")
    ) {
      out_li.push(path);
    }
  }
  Ok(())
}
