//! CLI 参数折算测试。

use super::*;

/// `--kv-size` 解析（rev-01）：整对 KV 宽度（含 8B key）口径。
#[test]
fn resolve_kv_size_pair_semantics() {
  // 缺省 128，对齐 C++ `DEFINE_uint32(kv_size, 128, "size of a pair of KV")`。
  assert_eq!(resolve_kv_size(None), Ok(DEFAULT_KV_SIZE));
  assert_eq!(resolve_kv_size(Some("128")), Ok(128));
  // 非法数值沿用本文件其余参数的口径：按缺省值处理。
  assert_eq!(resolve_kv_size(Some("abc")), Ok(DEFAULT_KV_SIZE));
  // `<= 8` 拒绝：对齐 C++ `CHECK(FLAGS_kv_size > sizeof(uint64_t))`。
  assert!(resolve_kv_size(Some("8")).is_err());
  assert!(resolve_kv_size(Some("0")).is_err());
  // value 长度推导：value = kv_size - 8（对齐 C++ `value.resize(FLAGS_kv_size - 8)`）。
  assert_eq!(value_len(DEFAULT_KV_SIZE), 120);
  assert_eq!(value_len(9), 1);
}

/// redb 口径：24B key + 174B KV → 150B value；按 key 宽度校验整对宽度。
#[test]
fn resolve_kv_size_honors_the_key_width() {
  assert_eq!(resolve_kv_size_with_key(Some("174"), 24), Ok(174));
  assert_eq!(value_len_with_key(174, 24), 150);
  // 整对宽度必须大于 key 宽度。
  assert!(resolve_kv_size_with_key(Some("24"), 24).is_err());
  assert!(resolve_kv_size_with_key(Some("10"), 24).is_err());
  // 8B key 的历史口径不变。
  assert_eq!(value_len_with_key(128, 8), value_len(128));
}
