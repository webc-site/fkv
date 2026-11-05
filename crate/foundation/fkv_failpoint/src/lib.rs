//! fkv_failpoint：命名 fail point / kill point 注册表（崩溃一致性与故障注入测试用）。
//!
//! 默认无配置时全部是 no-op，不改变生产行为。通过环境变量 `FKV_FAILPOINT`
//! 或 `set_spec` 配置，格式（逗号分隔）：
//!
//! ```text
//! WritePage@1                  // 第 1 次 WritePage 触发默认错误（IoFail）
//! AppendManifest:Sync@1!kill   // 点名前可含 ':'；@count 指定第几次命中
//! ReadPage@3!err=Corrupted
//! ```
//!
//! 语法：`name[@count][!kill|!err=ErrorName]`，逗号分隔。触发后该点失效。
//!
//! 埋点侧不写字符串：名字的唯一来源是 [`points`]，调用点用
//! [`fail_point!`](crate::fail_point) / [`kill_point!`](crate::kill_point) 引用常量标识符，
//! 因此漏写或拼错的点名编译不过。配置串（环境变量 / `set_spec`）仍是对外契约的字符串。

use std::{
  collections::HashMap,
  env::var,
  process::abort,
  sync::{
    OnceLock, RwLock,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
};

use fkv_error::KvError;

/// 注入点名字的唯一来源。
///
/// 埋点与测试都从这里引用；[`fail_point!`](crate::fail_point) /
/// [`kill_point!`](crate::kill_point) 只接受本模块的常量标识符，因此**写不出**
/// 只存在于某一个后端的散落字面量 —— 名字漂移在编译期消失，不必靠测试去比对
/// 两边的字符串集合。
///
/// 各组自带 `*:COMPIO` / `*:SIM` / `*:CLOUD` 子集清单，供枚举式 fuzz 驱动使用。
pub mod points {
  /// `PageIo::write_page`。
  pub const WRITE_PAGE: &str = "WritePage";
  /// `PageIo::sync_data`。
  pub const SYNC_DATA: &str = "SyncData";
  /// 目录项 fsync（`CompioStore::sync_dir_entry`）。sim 无目录概念，故无对应点。
  pub const SYNC_DIR: &str = "SyncDir";
  /// 段写入（`SegmentIo::write_segment`）。
  pub const WRITE_SEGMENTS: &str = "WriteSegments";
  /// `FileIo::write_file` 的 tmp fdatasync 失败（注入错误，不回滚语义之外的崩溃）。
  pub const WRITE_FILE_SYNC_FAIL: &str = "WriteFile:SyncFail";
  /// `FileIo::write_file` 的 rename 之后、目录项 fsync 之前。
  pub const WRITE_FILE_RENAME: &str = "WriteFile:Rename";

  /// `ManifestIo::append_manifest` 的 `write_at` 之后、`fdatasync` 之前（撕裂 record 窗口）。
  pub const APPEND_WRITE: &str = "AppendManifest:Write";
  /// `append_manifest` 的 `fdatasync` 之后、目录项 fsync 之前。
  pub const APPEND_SYNC: &str = "AppendManifest:Sync";
  /// `append_manifest` 的 sync 失败注入。sim 的 manifest 写是原子的，无此失败模式。
  pub const APPEND_SYNC_FAIL: &str = "AppendManifest:SyncFail";

  /// `atomic_write_file` 的 tmp 已耐久、尚未 rename。
  pub const ATOMIC_SYNC: &str = "AtomicWriteFile:Sync";
  /// `atomic_write_file` 已 rename、父目录项尚未 fsync。
  pub const ATOMIC_RENAME: &str = "AtomicWriteFile:Rename";
  /// `atomic_write_file` 的目录项已 fsync（提交完成）。sim 无目录项持久化。
  pub const ATOMIC_SYNC_DIR: &str = "AtomicWriteFile:SyncDir";

  /// 云后端 manifest 上传。
  pub const CLOUD_MANIFEST_PUT: &str = "CloudManifest:Put";

  /// `fkv_io_compio` 埋点的全部名字。
  pub const COMPIO: &[&str] = &[
    WRITE_PAGE,
    SYNC_DATA,
    SYNC_DIR,
    WRITE_SEGMENTS,
    WRITE_FILE_SYNC_FAIL,
    WRITE_FILE_RENAME,
    APPEND_WRITE,
    APPEND_SYNC,
    APPEND_SYNC_FAIL,
    ATOMIC_SYNC,
    ATOMIC_RENAME,
    ATOMIC_SYNC_DIR,
  ];

  /// `fkv_io_sim` 埋点的全部名字。
  ///
  /// 是 [`COMPIO`] 的**真子集**：`SYNC_DIR` / `ATOMIC_SYNC_DIR` 依赖目录项持久化、
  /// `APPEND_SYNC_FAIL` 依赖「manifest 写可失败」，sim 的掉电模型里都没有对应概念，
  /// 属有意缺失而非漂移。
  pub const SIM: &[&str] = &[
    WRITE_PAGE,
    SYNC_DATA,
    WRITE_SEGMENTS,
    WRITE_FILE_SYNC_FAIL,
    WRITE_FILE_RENAME,
    APPEND_WRITE,
    APPEND_SYNC,
    ATOMIC_SYNC,
    ATOMIC_RENAME,
  ];

  /// `fkv_cloud` 埋点的全部名字。
  pub const CLOUD: &[&str] = &[CLOUD_MANIFEST_PUT];
}

/// 触发动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
  Fail(KvError),
  Kill,
}

#[derive(Debug)]
struct Entry {
  /// 距离触发还剩多少次命中。
  countdown: AtomicUsize,
  action: Action,
  fired: AtomicBool,
}

#[derive(Debug, Default)]
struct Registry {
  entries: HashMap<String, Entry>,
}

static REGISTRY: OnceLock<RwLock<Registry>> = OnceLock::new();

fn registry() -> &'static RwLock<Registry> {
  REGISTRY.get_or_init(|| {
    let mut registry = Registry::default();
    if let Ok(spec) = var("FKV_FAILPOINT") {
      apply_spec(&mut registry, &spec);
    }
    RwLock::new(registry)
  })
}

fn error_by_name(name: &str) -> KvError {
  match name {
    "Corrupted" | "corrupted" => KvError::Corrupted,
    "OutOfSpace" | "out_of_space" => KvError::OutOfSpace,
    "OutOfMem" | "out_of_mem" => KvError::OutOfMem,
    "NotRunning" | "not_running" => KvError::NotRunning,
    "ResourceMissing" | "resource_missing" => KvError::ResourceMissing,
    "InvalidArgs" | "invalid_args" => KvError::InvalidArgs,
    "EndOfFile" | "end_of_file" => KvError::EndOfFile,
    _ => KvError::IoFail,
  }
}

fn parse_action(text: &str) -> Action {
  if text == "kill" {
    Action::Kill
  } else if let Some(name) = text.strip_prefix("err=") {
    Action::Fail(error_by_name(name))
  } else {
    Action::Fail(KvError::IoFail)
  }
}

fn apply_spec(registry: &mut Registry, spec: &str) {
  for item in spec.split(',') {
    let item = item.trim();
    if item.is_empty() {
      continue;
    }
    // 点名前可包含 ':'；动作用 '!' 分隔，次数用 '@' 分隔。
    let (body, action) = match item.find('!') {
      Some(index) => (&item[..index], parse_action(&item[index + 1..])),
      None => (item, Action::Fail(KvError::IoFail)),
    };
    let (name, count) = match body.rfind('@') {
      Some(index) => (
        &body[..index],
        body[index + 1..].parse::<usize>().unwrap_or(1).max(1),
      ),
      None => (body, 1),
    };
    if name.is_empty() {
      continue;
    }
    registry.entries.insert(
      name.to_owned(),
      Entry {
        countdown: AtomicUsize::new(count),
        action,
        fired: AtomicBool::new(false),
      },
    );
  }
}

/// 替换当前注册表（测试/工具用）。空字符串清空。
pub fn set_spec(spec: &str) {
  let mut guard = registry().write().unwrap_or_else(|err| err.into_inner());
  guard.entries.clear();
  apply_spec(&mut guard, spec);
}

/// 清空所有注入点。
pub fn reset() {
  set_spec("");
}

/// 是否配置了任意注入点。
pub fn enabled() -> bool {
  !registry()
    .read()
    .unwrap_or_else(|err| err.into_inner())
    .entries
    .is_empty()
}

/// 命中 fail point：到点返回注入错误，否则 `None`。
pub fn fail_point(name: &str) -> Option<KvError> {
  let guard = registry().read().unwrap_or_else(|err| err.into_inner());
  let entry = guard.entries.get(name)?;
  let Action::Fail(err) = entry.action else {
    return None;
  };
  hit(entry).then_some(err)
}

/// 命中 kill point：到点直接终止进程（模拟崩溃）。
pub fn kill_point(name: &str) {
  let guard = registry().read().unwrap_or_else(|err| err.into_inner());
  let Some(entry) = guard.entries.get(name) else {
    return;
  };
  if entry.action == Action::Kill && hit(entry) {
    abort();
  }
}

fn hit(entry: &Entry) -> bool {
  if entry.fired.load(Ordering::Acquire) {
    return false;
  }
  let previous = entry.countdown.fetch_sub(1, Ordering::AcqRel);
  if previous == 1 {
    entry.fired.store(true, Ordering::Release);
    return true;
  }
  false
}

/// 便捷封装：`fail_point(name)?` 的宏形式，少写样板。
///
/// 只接受 [`points`](crate::points) 里的常量标识符（`fail_point!(WRITE_PAGE)`），
/// 传字面量或未定义标识符都会编译失败 —— 名字集合因此无法悄悄发散。
#[macro_export]
macro_rules! fail_point {
  ($name:ident) => {
    if let Some(err) = $crate::fail_point($crate::points::$name) {
      return Err(err);
    }
  };
}

/// 同 [`fail_point!`](crate::fail_point)，用于 kill point（命中即 `abort`）。
#[macro_export]
macro_rules! kill_point {
  ($name:ident) => {
    $crate::kill_point($crate::points::$name)
  };
}
