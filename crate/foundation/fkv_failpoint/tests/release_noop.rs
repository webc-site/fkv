//! release 构建（`not(debug_assertions)`）下注入必须被编译掉，对齐 C++ `NDEBUG`
//! 把 `TEST_FAIL_POINT_RETURN` / `TEST_KILL_POINT_WEIGHT` 展开为空。
#![cfg(not(debug_assertions))]

use std::env::set_var;

use fkv_failpoint::{enabled, fail_point, kill_point, reset, set_spec};

/// 环境变量与 `set_spec` 在 release 下都不生效：不读 env、不建注入表、不 abort。
///
/// 环境变量必须在任何 failpoint API 调用之前设置：debug 实现里 `registry()`
/// 是懒初始化，首次访问时才读 `FKV_FAILPOINT`。本文件只有一个测试，nextest 也
/// 按「一测试一进程」调度，因此不存在并发读写环境变量的竞争。
#[test]
fn release_ignores_failpoint_env_and_spec() {
  // SAFETY: 进程内没有其它线程并发读环境变量（唯一测试，见上）。
  unsafe { set_var("FKV_FAILPOINT", "WritePage@1!kill") };

  // 环境变量不生效：kill point 不 abort、fail point 不注入错误。
  assert_eq!(fail_point("WritePage"), None);
  kill_point("WritePage");
  assert!(!enabled());

  // `set_spec` / `reset` 在 release 下不建立注入表。
  set_spec("WritePage@1!kill,SyncData@1!err=Corrupted");
  assert!(!enabled());
  assert_eq!(fail_point("WritePage"), None);
  assert_eq!(fail_point("SyncData"), None);
  kill_point("WritePage");

  reset();
  assert!(!enabled());
  assert_eq!(fail_point("WritePage"), None);
  kill_point("WritePage");
}
