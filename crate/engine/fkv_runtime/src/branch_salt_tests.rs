use crate::archive::branch_salt;

/// 2³² µs 回绕前后的时间戳必须得到不同 salt：旧实现取 `as u32`，两个值
/// 完全相同，同一逻辑名会落到已存在的 branch 上。
#[test]
fn salt_does_not_repeat_after_low_32_bit_wrap() {
  let base = 1_700_000_000_000_000u64;
  for k in 0..8u64 {
    let a = base + k * (1u64 << 32);
    let b = a + (1u64 << 32);
    assert_eq!(a as u32, b as u32, "构造必须落在同一个 u32 回绕点");
    assert_ne!(branch_salt(a), branch_salt(b), "回绕周期后 salt 重名: {a}");
    assert_ne!(branch_salt(a), 0, "salt 不能为 0");
  }
}
