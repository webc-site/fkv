//! fkv_failpoint：命名 fail point / kill point 注册表（崩溃一致性与故障注入测试用）。
//!
//! 默认无配置时全部是 no-op，不改变生产行为。通过环境变量 `FKV_FAILPOINT`
//! 或 `set_spec` 配置，格式（逗号分隔）：
//!
//! ```text
//! WritePage@1                  // 第 1 次 WritePage 触发默认错误（IoFail）
//! AppendManifest:Sync@1!kill   // 点名前可含 ':'；@count 指定第几次命中
//! ReadSegments@3!err=Corrupted
//! ReadSegmentsHold@1!hold      // 命中后挂起调用方，直到 release（测试门闩）
//! ```
//!
//! 语法：`name[@count][!kill|!hold|!err=ErrorName]`，逗号分隔。`!kill` /
//! `!err` 触发后该点失效；`!hold` 的 `@count` 表示前 `count` 次命中都停在
//! 门闩上，之后放行（门闩由 [`release`] 打开）。
//!
//! 埋点侧不写字符串：名字的唯一来源是 [`points`]，调用点用
//! [`fail_point!`](crate::fail_point!) / [`kill_point!`](crate::kill_point!) /
//! [`hold_point!`](crate::hold_point!) 引用常量标识符，
//! 因此漏写或拼错的点名编译不过。配置串（环境变量 / `set_spec`）仍是对外契约的字符串。
//!
//! # release 构建不注入（对齐 C++ `NDEBUG`）
//!
//! C++ 基线在 `NDEBUG` 下把 `TEST_FAIL_POINT_RETURN` / `TEST_KILL_POINT_WEIGHT`
//! 展开为空（`eloqstore/include/fail_point.h`、`include/kill_point.h`），release 里
//! 既没有 `FailPoint::ShouldFail` 调用也没有 `KillPoint::TestKillRandom`。本 crate
//! 用 `#[cfg(debug_assertions)]` 对齐该口径：`not(debug_assertions)`（即
//! `cargo build --release` / `cargo nextest run --release`）下**不读取** `FKV_FAILPOINT`、
//! 不建立注入表 —— [`set_spec`] / [`reset`] 为空操作，[`enabled`] 恒为 `false`，
//! [`fail_point`](crate::fail_point()) 恒返回 `None`，
//! [`kill_point`](crate::kill_point()) 不会 `abort`，
//! [`fail_point!`](crate::fail_point!) / [`kill_point!`](crate::kill_point!) /
//! [`hold_point!`](crate::hold_point!) 展开为空。
//! 依赖注入的测试因此只在 debug / test profile（`debug-assertions = true`）下有效。

#[cfg(debug_assertions)]
use std::{
  collections::HashMap,
  env::var,
  future::Future,
  mem::take,
  pin::Pin,
  process::abort,
  sync::{
    Arc, Mutex, OnceLock, RwLock,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
  task::{Context, Poll, Waker},
};

use fkv_error::KvError;

/// 注入点名字的唯一来源。
///
/// 埋点与测试都从这里引用；[`fail_point!`](crate::fail_point!) /
/// [`kill_point!`](crate::kill_point!) 只接受本模块的常量标识符，因此**写不出**
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
  /// 段读取（`SegmentIo::read_segment` 的 plain 路径）。fixed（READ_FIXED）
  /// 路径另有缓冲约束，不经此点。
  pub const READ_SEGMENTS: &str = "ReadSegments";
  /// 段读取的测试门闩（`!hold`）：命中后挂起调用方，直到 [`release`](crate::release) 打开。
  ///
  /// 与 [`READ_SEGMENTS`] 同一调用点，但错误注入先判：首次读可注入
  /// `OutOfMem`，OOM 重排出的下一次读再停在门闩上，测试因此不必按墙钟猜
  /// 「重试还没完成」。
  pub const READ_SEGMENTS_HOLD: &str = "ReadSegmentsHold";
  /// 页读取的测试门闩（`!hold`）：命中后挂起调用方，直到 [`release`](crate::release) 打开。
  ///
  /// 用于把一条普通读确定性地钉在页 IO 上（例如占住读窗口），不依赖 sleep
  /// 之后的状态推断。
  pub const READ_PAGE_HOLD: &str = "ReadPageHold";
  /// `FileIo::write_file` 的 tmp fdatasync 失败（注入错误，不回滚语义之外的崩溃）。
  pub const WRITE_FILE_SYNC_FAIL: &str = "WriteFile:SyncFail";
  /// `FileIo::write_file` 的 rename 之后、目录项 fsync 之前。
  pub const WRITE_FILE_RENAME: &str = "WriteFile:Rename";
  /// `FileIo::try_remove_empty_partition_dir` 的目录收尾瞬时失败：注入点在
  /// 函数入口（read_dir 之前，目录非空的调用也可命中），等价模拟 read_dir
  /// EMFILE / EACCES 或 read_dir 见空后 rmdir 的 ENOTEMPTY 竞态 / EACCES，
  /// 用于验证 GC 收尾的 warning-only 语义（io-31）。
  pub const TRY_REMOVE_EMPTY_DIR_FAIL: &str = "TryRemoveEmptyDir:Fail";

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
  /// `CloudStore::list_manifest_file_name_li` 入口失败注入：等价模拟云端
  /// ListObjects 网络错误，用于验证 `Store::drop_table`（branch 模式）列举
  /// 失败时的零删除 + evict 语义（sem-63）。
  pub const CLOUD_LIST_MANIFEST_FAIL: &str = "CloudManifest:ListFail";

  /// `fkv_io_compio` 埋点的全部名字。
  pub const COMPIO: &[&str] = &[
    WRITE_PAGE,
    SYNC_DATA,
    SYNC_DIR,
    WRITE_SEGMENTS,
    READ_SEGMENTS,
    READ_SEGMENTS_HOLD,
    READ_PAGE_HOLD,
    WRITE_FILE_SYNC_FAIL,
    WRITE_FILE_RENAME,
    TRY_REMOVE_EMPTY_DIR_FAIL,
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
    READ_SEGMENTS,
    WRITE_FILE_SYNC_FAIL,
    WRITE_FILE_RENAME,
    APPEND_WRITE,
    APPEND_SYNC,
    ATOMIC_SYNC,
    ATOMIC_RENAME,
  ];

  /// `fkv_cloud` 埋点的全部名字。
  pub const CLOUD: &[&str] = &[CLOUD_MANIFEST_PUT, CLOUD_LIST_MANIFEST_FAIL];
}

/// 触发动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
  Fail(KvError),
  Kill,
  /// 命中后挂起调用方（异步门闩），直到 [`release`] 打开。
  Hold,
}

/// 可等待门闩：闭合时挂起调用方，[`Latch::release`] 打开并唤醒。
#[cfg(debug_assertions)]
#[derive(Debug, Default)]
struct Latch {
  open: AtomicBool,
  waker_li: Mutex<Vec<Waker>>,
}

#[cfg(debug_assertions)]
impl Latch {
  fn release(&self) {
    self.open.store(true, Ordering::Release);
    let waker_li = take(&mut *self.waker_li.lock().unwrap_or_else(|err| err.into_inner()));
    for waker in waker_li {
      waker.wake();
    }
  }

  fn is_open(&self) -> bool {
    self.open.load(Ordering::Acquire)
  }
}

/// `!hold` 注入点返回的等待句柄：`.await` 到 [`release`] 打开对应门闩。
///
/// release 构建下该类型不存在（[`hold_point!`](crate::hold_point!) 展开为空）。
#[cfg(debug_assertions)]
pub struct Hold {
  latch: Arc<Latch>,
}

#[cfg(debug_assertions)]
impl Future for Hold {
  type Output = ();

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
    if self.latch.is_open() {
      return Poll::Ready(());
    }
    self
      .latch
      .waker_li
      .lock()
      .unwrap_or_else(|err| err.into_inner())
      .push(cx.waker().clone());
    // 存 waker 后再查一次：release 可能刚好落在两次检查之间。
    if self.latch.is_open() {
      Poll::Ready(())
    } else {
      Poll::Pending
    }
  }
}

#[cfg(debug_assertions)]
#[derive(Debug)]
struct Entry {
  /// 距离触发还剩多少次命中。
  countdown: AtomicUsize,
  action: Action,
  fired: AtomicBool,
  /// `!hold` 点的门闩：同一注入点（含同名历史配置）共享一次 release。
  latch: Option<Arc<Latch>>,
}

#[cfg(debug_assertions)]
#[derive(Debug, Default)]
struct Registry {
  entries: HashMap<String, Entry>,
  /// 历史上创建过的门闩（含被 `set_spec` 替换掉的旧配置）：被挂起的任务
  /// 持有 `Arc<Latch>`，`release(name)` / `reset` 仍要能找到它们。
  latches: Vec<(String, Arc<Latch>)>,
}

#[cfg(debug_assertions)]
static REGISTRY: OnceLock<RwLock<Registry>> = OnceLock::new();

#[cfg(debug_assertions)]
fn registry() -> &'static RwLock<Registry> {
  REGISTRY.get_or_init(|| {
    let mut registry = Registry::default();
    if let Ok(spec) = var("FKV_FAILPOINT") {
      apply_spec(&mut registry, &spec);
    }
    RwLock::new(registry)
  })
}

#[cfg(debug_assertions)]
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

#[cfg(debug_assertions)]
fn parse_action(text: &str) -> Action {
  if text == "kill" {
    Action::Kill
  } else if text == "hold" {
    Action::Hold
  } else if let Some(name) = text.strip_prefix("err=") {
    Action::Fail(error_by_name(name))
  } else {
    Action::Fail(KvError::IoFail)
  }
}

#[cfg(debug_assertions)]
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
    // `!hold` 每次配置（含同名重配）都新建门闩：被挂起的任务持有旧的
    // `Arc<Latch>`，重配不能影响它；`release(name)` 负责打开同名全部门闩。
    let latch = (action == Action::Hold).then(|| Arc::new(Latch::default()));
    if let Some(latch) = &latch {
      registry.latches.push((name.to_owned(), latch.clone()));
    }
    registry.entries.insert(
      name.to_owned(),
      Entry {
        countdown: AtomicUsize::new(count),
        action,
        fired: AtomicBool::new(false),
        latch,
      },
    );
  }
}

/// 替换当前注册表（测试/工具用）。空字符串清空。
#[cfg(debug_assertions)]
pub fn set_spec(spec: &str) {
  let mut guard = registry().write().unwrap_or_else(|err| err.into_inner());
  guard.entries.clear();
  apply_spec(&mut guard, spec);
}

/// 替换当前注册表（测试/工具用）。release 构建下为空操作。
#[cfg(not(debug_assertions))]
pub fn set_spec(_spec: &str) {}

/// 清空所有注入点，并打开全部门闩（含历史配置），避免挂起的读任务拖住
/// 用例收尾。release 构建下为空操作。
pub fn reset() {
  #[cfg(debug_assertions)]
  {
    let guard = registry().read().unwrap_or_else(|err| err.into_inner());
    for (_, latch) in &guard.latches {
      latch.release();
    }
  }
  set_spec("");
}

/// 是否配置了任意注入点。
#[cfg(debug_assertions)]
pub fn enabled() -> bool {
  !registry()
    .read()
    .unwrap_or_else(|err| err.into_inner())
    .entries
    .is_empty()
}

/// 是否配置了任意注入点。release 构建下恒为 `false`。
#[cfg(not(debug_assertions))]
pub fn enabled() -> bool {
  false
}

/// 指定注入点是否已经触发过（次数用尽）。
///
/// 测试用它同步「注入已落到被测路径」再断言后续调度行为，避免按墙钟猜
/// 时序。未配置 / release 构建下恒为 `false`（注入不会发生）。
#[cfg(debug_assertions)]
pub fn fired(name: &str) -> bool {
  registry()
    .read()
    .unwrap_or_else(|err| err.into_inner())
    .entries
    .get(name)
    .is_some_and(|entry| entry.fired.load(Ordering::Acquire))
}

/// 指定注入点是否已经触发过。release 构建下恒为 `false`。
#[cfg(not(debug_assertions))]
pub fn fired(_name: &str) -> bool {
  false
}

/// 命中 fail point：到点返回注入错误，否则 `None`。
#[cfg(debug_assertions)]
pub fn fail_point(name: &str) -> Option<KvError> {
  let guard = registry().read().unwrap_or_else(|err| err.into_inner());
  let entry = guard.entries.get(name)?;
  let Action::Fail(err) = entry.action else {
    return None;
  };
  hit(entry).then_some(err)
}

/// 命中 fail point：release 构建下恒为 `None`（注入被编译掉）。
#[cfg(not(debug_assertions))]
pub fn fail_point(_name: &str) -> Option<KvError> {
  None
}

/// 命中 kill point：到点直接终止进程（模拟崩溃）。
#[cfg(debug_assertions)]
pub fn kill_point(name: &str) {
  let guard = registry().read().unwrap_or_else(|err| err.into_inner());
  let Some(entry) = guard.entries.get(name) else {
    return;
  };
  if entry.action == Action::Kill && hit(entry) {
    abort();
  }
}

/// 命中 kill point：release 构建下不会终止进程（注入被编译掉）。
#[cfg(not(debug_assertions))]
pub fn kill_point(_name: &str) {}

/// 命中 `!hold` 注入点：返回等待句柄，调用方 `.await` 到 [`release`] 打开门闩。
///
/// `@count` 控制挂起次数：前 `count` 次命中都返回句柄（停在门闩上），用尽后
/// 放行；[`fired`] 在最后一次命中时置位，测试据此确认「调用方已停在门闩上」。
///
/// 未配置 / 次数用尽返回 `None`。release 构建下本函数不存在
/// （[`hold_point!`](crate::hold_point!) 展开为空）。
#[cfg(debug_assertions)]
pub fn hold_point(name: &str) -> Option<Hold> {
  let guard = registry().read().unwrap_or_else(|err| err.into_inner());
  let entry = guard.entries.get(name)?;
  if entry.action != Action::Hold {
    return None;
  }
  let latch = entry.latch.clone()?;
  hold_hit(entry).then_some(Hold { latch })
}

/// 打开指定注入点的全部门闩（含该名字历史上被 `set_spec` 替换掉的配置）。
///
/// release 构建下为空操作。
#[cfg(debug_assertions)]
pub fn release(name: &str) {
  let guard = registry().read().unwrap_or_else(|err| err.into_inner());
  for (latch_name, latch) in &guard.latches {
    if latch_name == name {
      latch.release();
    }
  }
}

/// 打开指定注入点的全部门闩。release 构建下为空操作。
#[cfg(not(debug_assertions))]
pub fn release(_name: &str) {}

#[cfg(debug_assertions)]
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

/// `!hold` 的命中判定：`count` 次以内每次都要挂起，最后一次置 `fired`。
#[cfg(debug_assertions)]
fn hold_hit(entry: &Entry) -> bool {
  if entry.fired.load(Ordering::Acquire) {
    return false;
  }
  // `checked_sub` 避免次数用尽后的竞争把计数减回 `usize::MAX`。
  let Ok(previous) = entry
    .countdown
    .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
      count.checked_sub(1)
    })
  else {
    return false;
  };
  if previous == 1 {
    entry.fired.store(true, Ordering::Release);
  }
  true
}

/// 便捷封装：`fail_point(name)?` 的宏形式，少写样板。
///
/// 只接受 [`points`](crate::points) 里的常量标识符（`fail_point!(WRITE_PAGE)`），
/// 传字面量或未定义标识符都会编译失败 —— 名字集合因此无法悄悄发散。
///
/// release 构建下展开为空（`()`），调用点不会残留注册表查询。
#[macro_export]
macro_rules! fail_point {
  ($name:ident) => {{
    #[cfg(debug_assertions)]
    {
      if let Some(err) = $crate::fail_point($crate::points::$name) {
        return Err(err);
      }
    }
  }};
}

/// 同 [`fail_point!`](crate::fail_point!)，用于 kill point（命中即 `abort`）。
///
/// release 构建下展开为空（`()`），不会 `abort`。
#[macro_export]
macro_rules! kill_point {
  ($name:ident) => {{
    #[cfg(debug_assertions)]
    $crate::kill_point($crate::points::$name);
  }};
}

/// 命中 `!hold` 注入点后挂起，直到 [`release`](crate::release) 打开门闩。
///
/// 语句级宏（内部已 `.await`），调用点写作 `hold_point!(READ_PAGE_HOLD);`。
/// release 构建下展开为空，不会产生任何 await / 分配。
#[macro_export]
macro_rules! hold_point {
  ($name:ident) => {{
    #[cfg(debug_assertions)]
    if let Some(hold) = $crate::hold_point($crate::points::$name) {
      hold.await;
    }
  }};
}
