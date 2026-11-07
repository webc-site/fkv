//! 进程 IO 资源指标测试。

use super::*;

/// `/proc/self/io` 解析：缺失字段返回 `None`，齐全时按字段取值。
#[test]
fn parse_proc_io_reads_bytes() {
  let text = "rchar: 1\nread_bytes: 4096\nwrite_bytes: 8192\n";
  assert_eq!(parse_proc_io(text), Some((4096, 8192)));
  assert_eq!(parse_proc_io("rchar: 1\n"), None);
}

/// store 体积按文件名前缀拆分，并递归子目录。
#[test]
fn dir_stats_splits_by_prefix_and_recurses() {
  let root = env::temp_dir().join(format!("fkv_bench_dir_stats_{}_{}", id(), now_secs()));
  let nested = root.join("table");
  fs::create_dir_all(&nested).unwrap();
  fs::write(root.join("data_1"), [0u8; 10]).unwrap();
  fs::write(nested.join("segment_1"), [0u8; 20]).unwrap();
  fs::write(nested.join("manifest_1"), [0u8; 30]).unwrap();
  fs::write(nested.join("other"), [0u8; 40]).unwrap();
  let stats = dir_stats(&root);
  assert_eq!(stats.total_bytes, 100);
  assert_eq!(stats.file_num, 4);
  assert_eq!((stats.data_bytes, stats.data_file_num), (10, 1));
  assert_eq!((stats.segment_bytes, stats.segment_file_num), (20, 1));
  assert_eq!((stats.manifest_bytes, stats.manifest_file_num), (30, 1));
  assert_eq!((stats.other_bytes, stats.other_file_num), (40, 1));
  let _ = fs::remove_dir_all(&root);
}
