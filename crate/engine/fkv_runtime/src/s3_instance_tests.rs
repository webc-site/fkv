use crate::config::cloud_s3_instance_num;

/// 实例数只计真实构造点：shard 数个 + 全局列举 1 个 + 已启用的 crond。
#[test]
fn counts_shards_and_enabled_runtime_side_instances() {
  assert_eq!(cloud_s3_instance_num(1, false, false), 2);
  assert_eq!(cloud_s3_instance_num(1, true, false), 3);
  assert_eq!(cloud_s3_instance_num(1, false, true), 3);
  assert_eq!(cloud_s3_instance_num(4, true, true), 7);
}
