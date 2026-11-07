//! 分区目录名 `<tbl_name>.<partition_id>` 的生成 / 解析规则。
//!
//! `dir_name` 与 `Display` 同一实现；`parse_dir_name` 在 `FromStr` 之上要求
//! 原样往返。目录名按**最后一个** `.` 切分，表名可含 `.`（`db.users`），
//! 与 C++ `TableIdent::FromString` 的 `find_last_of('.')` 同构。

use std::path::Path;

use fkv_error::KvError;
use fkv_types::TableIdent;

#[test]
fn dir_name_matches_display() {
  let tbl = TableIdent::new("demo", 7).unwrap();
  assert_eq!(tbl.dir_name(), "demo.7");
  assert_eq!(tbl.dir_name(), tbl.to_string());
}

#[test]
fn dir_name_roundtrip() {
  for (name, partition) in [("demo", 0u32), ("t", 1), ("table_1", 42), ("A-b", u32::MAX)] {
    let tbl = TableIdent::new(name, partition).unwrap();
    assert_eq!(TableIdent::parse_dir_name(&tbl.dir_name()), Some(tbl));
  }
  assert_eq!(
    TableIdent::parse_dir_name("demo.7"),
    Some(TableIdent::new("demo", 7).unwrap())
  );
}

#[test]
fn parse_dir_name_rejects_empty_parts() {
  // 空表名 / 空分区。
  assert_eq!(TableIdent::parse_dir_name(".1"), None);
  assert_eq!(TableIdent::parse_dir_name("a."), None);
  assert_eq!(TableIdent::parse_dir_name("."), None);
  // 缺少分隔符。
  assert_eq!(TableIdent::parse_dir_name("a"), None);
  assert_eq!(TableIdent::parse_dir_name(""), None);
}

#[test]
fn parse_dir_name_rejects_non_canonical_partition() {
  // 可被归一化的写法必须拒绝，避免同一分区有两个目录名。
  assert_eq!(TableIdent::parse_dir_name("a.01"), None);
  assert_eq!(TableIdent::parse_dir_name("a.+1"), None);
  assert_eq!(TableIdent::parse_dir_name("a. 1"), None);
  assert_eq!(TableIdent::parse_dir_name("a.1 "), None);
  assert_eq!(TableIdent::parse_dir_name("a.0x10"), None);
  assert_eq!(TableIdent::parse_dir_name("a.-1"), None);
  // u32 上界 + 1 溢出。
  assert_eq!(TableIdent::parse_dir_name("a.4294967296"), None);
}

#[test]
fn parse_dir_name_rejects_nul() {
  assert_eq!(TableIdent::parse_dir_name("a\0b.1"), None);
  assert_eq!(TableIdent::parse_dir_name("a.1\0"), None);
}

/// 表名会拼进 `root.join("{tbl_name}.{partition_id}")`，必须能作为单一路径
/// 分量：绝对路径会替换 root，`..` 段会上跳，`/` / `\` 会生成全局枚举看不到
/// 的多级目录（sem-20）。
#[test]
fn rejects_path_component_escape_names() {
  let bad = [
    "",
    "\0",
    "a\0b",
    "/tmp/evil",
    "/abs",
    "..",
    ".",
    "../evil",
    "a/../../b",
    "a//b",
    "a/",
    "/a",
    "a/..",
    "./x",
    "..\\evil",
    "a\\b",
    "\\abs",
  ];
  for name in bad {
    assert_eq!(
      TableIdent::new(name, 1).err(),
      Some(KvError::InvalidArgs),
      "new({name:?}) 必须拒绝"
    );
    // 目录名解析同口径：列举到的逃逸形态一律跳过。
    assert!(
      format!("{name}.1").parse::<TableIdent>().is_err(),
      "FromStr 必须拒绝 {name:?}"
    );
    // 字段是 pub：结构体字面量绕过 new 时，is_valid 仍须给出否定。
    let bypass = TableIdent {
      tbl_name: name.into(),
      partition_id: 1,
    };
    assert!(!bypass.is_valid(), "is_valid 必须拒绝 {name:?}");
  }
}

/// 含 `.` 的表名（sem-09）与含 `..` 子串但不是独立分量的名字仍合法；拼出的
/// 分区目录必须恰好是 root 的一级子目录。
#[test]
fn accepts_ordinary_dotted_and_dotdot_substring_names() {
  let root = Path::new("/srv/fkv");
  for name in [
    "demo",
    "db.users",
    "a.b",
    "a..b",
    "table_1",
    "A-b",
    ".hidden",
    "中文表",
  ] {
    let tbl = TableIdent::new(name, 3).unwrap_or_else(|_| panic!("new({name:?}) 应接受"));
    let dir = root.join(tbl.dir_name());
    assert_eq!(
      dir.parent(),
      Some(root),
      "表名 {name:?} 必须落在 root 下一级"
    );
    assert!(dir.starts_with(root), "表名 {name:?} 不得逃逸 root");
    assert_eq!(TableIdent::parse_dir_name(&tbl.dir_name()), Some(tbl));
  }
}

/// 本地 / Cloud / standby 的目录列举都委托 `parse_dir_name`：含路径分隔符的
/// 目录名不是合法表目录，必须跳过。
#[test]
fn parse_dir_name_skips_escaped_directory_names() {
  for name in [
    "/tmp/evil.1",
    "../evil.1",
    "a/../../b.1",
    "a//b.1",
    "a/b.1",
    "..\\evil.1",
    "a\\b.1",
    "..1",
    "...1",
  ] {
    assert_eq!(TableIdent::parse_dir_name(name), None, "{name:?}");
  }
}

/// 表名可含 `.`（sem-09）：目录名按最后一个 `.` 切分，分区号不含 `.`，
/// 所以 `(表名, 分区) -> 目录名` 单射，往返唯一；与 C++ `find_last_of('.')`
/// 同构。构造面只额外拒绝空串、NUL、路径分隔符与整名 `.` / `..`（sem-20），
/// 含 `.` 的表名不受影响。
#[test]
fn dotted_table_name_roundtrips() {
  let tbl = TableIdent::new("db.users", 0).unwrap();
  assert_eq!(tbl.dir_name(), "db.users.0");
  assert_eq!(TableIdent::parse_dir_name("db.users.0"), Some(tbl.clone()));
  assert_eq!("db.users.0".parse::<TableIdent>().unwrap(), tbl);

  // 最后一个 `.` 永远是分隔符：前缀整体是表名，不存在「表 a 的分区 1 续写」。
  assert_eq!(
    TableIdent::parse_dir_name("a.1"),
    Some(TableIdent::new("a", 1).unwrap())
  );
  assert_eq!(
    TableIdent::parse_dir_name("a.12"),
    Some(TableIdent::new("a", 12).unwrap())
  );
  assert_eq!(
    TableIdent::parse_dir_name("a.1.2"),
    Some(TableIdent::new("a.1", 2).unwrap())
  );
  assert_eq!(
    TableIdent::parse_dir_name("a.1.2.3"),
    Some(TableIdent::new("a.1.2", 3).unwrap())
  );
  // 非规范数字分区仍被往返校验拒绝。
  assert_eq!(TableIdent::parse_dir_name("a.1.02"), None);
}
