//! object_key_tests：对象 key 的目录段与分区目录名同构解析。

use fkv_io_mem::MemStore;
use fkv_object::MemObjectStore;
use fkv_types::TableIdent;

use super::CloudStore;

/// 含 `.` 的表名：对象 key `<prefix>/<tbl>.<partition>/<name>` 按最后一个
/// `.` 切分（与本地目录名同构）。
///
/// 回退即失败：若拒绝含 `.` 的目录段，Cloud 模式下该表的整个对象前缀都
/// 不可枚举（归档 crond / prewarm / `global_*` 看不到远端分区）。
#[test]
fn partition_of_key_accepts_dotted_table_name() {
  let cloud = CloudStore::new(
    MemStore::new(512),
    MemObjectStore::new(),
    "fkv",
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  assert_eq!(
    cloud.partition_of_key("fkv/db.users.0/manifest_main_0"),
    Some(TableIdent::new("db.users", 0).unwrap())
  );
  // 最后一个 `.` 是分隔符：`a.1.2` 只能是表 `a.1` 的分区 2。
  assert_eq!(
    cloud.partition_of_key("fkv/a.1.2/data_0_main_1"),
    Some(TableIdent::new("a.1", 2).unwrap())
  );
  // 非分区对象 / 不在本 prefix 下的 key 仍然跳过。
  assert_eq!(cloud.partition_of_key("fkv/term_main_0"), None);
  assert_eq!(
    cloud.partition_of_key("other/db.users.0/manifest_main_0"),
    None
  );
}
