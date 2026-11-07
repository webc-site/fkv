//! debug / test profile 下的注册表语义。release 构建注入被编译掉，见
//! `tests/release_noop.rs`。
#![cfg(debug_assertions)]

use fkv_error::KvError;
use fkv_failpoint::{enabled, fail_point, reset, set_spec};

#[test]
fn failpoint_registry_semantics() {
  reset();
  assert!(!enabled());

  set_spec("WritePage@3");
  assert!(enabled());
  assert_eq!(fail_point("WritePage"), None);
  assert_eq!(fail_point("WritePage"), None);
  assert_eq!(fail_point("WritePage"), Some(KvError::IoFail));
  // 只触发一次。
  assert_eq!(fail_point("WritePage"), None);
  assert_eq!(fail_point("Unknown"), None);

  set_spec("AppendManifest:Sync@1!err=Corrupted");
  assert_eq!(fail_point("AppendManifest:Sync"), Some(KvError::Corrupted));

  // kill 动作不产生错误返回，仅解析。
  set_spec("ReadPage@1!kill");
  assert_eq!(fail_point("ReadPage"), None);

  reset();
  assert!(!enabled());
  assert_eq!(fail_point("WritePage"), None);
}
