//! 慢读分段探针：只读路径的 opt-in 诊断。
//!
//! 默认完全关闭（[`enabled`] 只读一次缓存的阈值）。设置
//! `FKV_SLOW_READ_US=<微秒>` 后，每个读请求在 [`scope`] 里分配一个独立采样槽，
//! 路径上的关键段用 [`record`] 显式累加自己的耗时；请求结束（守卫析构）时
//! 从槽表取走该槽并释放，总耗时超过阈值就向 stderr 打一行分段明细。
//!
//! 为什么不是「每线程单槽」：shard 的每条只读请求都是独立 compio task，在同一
//! 线程上交错推进。单槽会被后来的请求覆盖起点、清空已记录的段：先结束的请求
//! 反而结算了别人的起点与分段，后结束的请求则一行都没有。这里改为
//! **按请求标识的槽表**：`scope` 的每次 poll 期间把本请求的槽号装进线程本地
//! `CURRENT`，[`record`] 只写入当前正在 poll 的请求；两条读交错完成时各自的
//! 段仍归各自。没有采样槽时（如直接调用 `fkv_task_read` 的宿主）`record`
//! 照旧静默丢弃，不会误记到别的请求上。
//!
//! 设计取舍：
//! - **显式区间**而不是「相邻 mark 相减」：`read_page_li` 内部并发等待多个
//!   页读，单线程上各 future 交错推进，相减法会把耗时记到错误的段上。
//! - 关闭时每个埋点只有一次 `enabled()` 判断（原子读 + 分支），不取时钟、
//!   不分配、不碰 TLS；[`scope`] 也直接转交内层 future。
//! - 输出走 stderr：探针要在没有 logger 的 bench / 压测进程里也能直接可见；
//!   每条完成的请求另留一份有界报告到线程本地队列（[`take_reports`]），
//!   供测试 / 嵌入宿主断言，不依赖捕获 stderr。

use std::{
  cell::{Cell, RefCell},
  env,
  future::{Future, poll_fn},
  mem,
  pin::pin,
  sync::{
    LazyLock,
    atomic::{AtomicU64, Ordering},
  },
  time::{Duration, Instant},
};

use itoa::Buffer;

/// 触发输出的总耗时阈值（微秒）；`0`（默认）= 关闭。
pub const SLOW_READ_ENV: &str = "FKV_SLOW_READ_US";
/// 最多输出多少行，避免压测时刷屏（默认 2000，`0` = 不限）。
pub const SLOW_READ_MAX_ENV: &str = "FKV_SLOW_READ_MAX";
/// 默认输出行数上限。
const DEFAULT_MAX_LINE: u64 = 2000;
/// 阈值尚未初始化：首次 [`enabled`] 时从环境变量读取并缓存。
const THRESHOLD_UNINIT: u64 = u64::MAX;
/// 每线程保留的最近报告条数：开启探针时也不无界增长。
const REPORT_KEEP: usize = 128;

/// 解析 `<name>` 为正整数；缺省 / 非法按 `default`。
fn env_num(name: &str, default: u64) -> u64 {
  env::var(name)
    .ok()
    .and_then(|value| value.parse().ok())
    .unwrap_or(default)
}

static THRESHOLD_US: AtomicU64 = AtomicU64::new(THRESHOLD_UNINIT);
static MAX_LINE: LazyLock<u64> = LazyLock::new(|| env_num(SLOW_READ_MAX_ENV, DEFAULT_MAX_LINE));
static PRINTED: AtomicU64 = AtomicU64::new(0);

/// 阈值微秒数；首次调用从 `FKV_SLOW_READ_US` 读取并缓存（之后只读原子）。
#[inline]
fn threshold_us() -> u64 {
  let us = THRESHOLD_US.load(Ordering::Relaxed);
  if us != THRESHOLD_UNINIT {
    return us;
  }
  let us = env_num(SLOW_READ_ENV, 0);
  THRESHOLD_US.store(us, Ordering::Relaxed);
  us
}

/// 是否开启慢读探针；关闭时所有埋点都应该用 `if slow_read::enabled()` 短路。
#[inline]
pub fn enabled() -> bool {
  threshold_us() != 0
}

/// 编程设置阈值（微秒；`0` = 关闭），优先于 `FKV_SLOW_READ_US`。
///
/// 供测试 / 嵌入宿主在不改环境变量的情况下开关探针。环境变量只作为首次
/// [`enabled`] 的默认值，进程内之后以本设置为准。
pub fn set_threshold_us(us: u64) {
  THRESHOLD_US.store(us, Ordering::Relaxed);
}

/// 一条请求的采样结果：总耗时与按名合并后的分段耗时（微秒）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
  pub total_us: u64,
  pub phase_li: Vec<(&'static str, u64)>,
}

/// 单条请求的采样槽。
struct Slot {
  /// 本请求起始时刻。
  start: Instant,
  /// 各段显式累加耗时（微秒），同名段在结算时合并。
  phase_li: Vec<(&'static str, u64)>,
}

/// 每线程槽表：槽号即下标，释放后进 `free_li` 复用（O(1) 分配 / 释放）。
///
/// 只由本线程的请求 future 在 poll 内访问；条数上限即同线程并发读请求数
/// （shard 侧为 `MAX_READ_IN_FLIGHT`），不会随历史请求数增长。
#[derive(Default)]
struct SlotTable {
  li: Vec<Option<Slot>>,
  /// 已释放的槽号，下一次 [`begin`] 优先复用。
  free_li: Vec<usize>,
}

thread_local! {
  static SLOTS: RefCell<SlotTable> = const {
    RefCell::new(SlotTable {
      li: Vec::new(),
      free_li: Vec::new(),
    })
  };
  /// 当前正在 poll 的请求槽号；由 `scope` 的 poll 包装在 poll 期间设置。
  static CURRENT: Cell<Option<usize>> = const { Cell::new(None) };
  /// 最近完成的报告（有界），供 [`take_reports`] 取用。
  static REPORT_LI: RefCell<Vec<Report>> = const { RefCell::new(Vec::new()) };
}

/// 采样槽守卫：`Drop` 时结算并释放槽位。
///
/// 只由 [`scope`] 创建。关闭探针时 `id` 为 `None`，析构不碰 TLS。
struct Guard {
  id: Option<usize>,
}

/// poll 期间的「当前请求」守卫：析构时恢复外层槽号（支持嵌套作用域）。
struct CurrentGuard {
  prev: Option<usize>,
}

impl Drop for CurrentGuard {
  fn drop(&mut self) {
    let _ = CURRENT.try_with(|current| current.set(self.prev));
  }
}

impl Guard {
  fn is_on(&self) -> bool {
    self.id.is_some()
  }

  /// 进入本请求的 poll：把槽号装入线程本地，返回时恢复外层。
  fn enter(&self) -> CurrentGuard {
    let prev = CURRENT
      .try_with(|current| current.replace(self.id))
      .unwrap_or(None);
    CurrentGuard { prev }
  }
}

impl Drop for Guard {
  fn drop(&mut self) {
    let Some(id) = self.id else {
      return;
    };
    let Some(slot) = finish(id) else {
      return;
    };
    publish(Report {
      total_us: u64::try_from(slot.start.elapsed().as_micros()).unwrap_or(u64::MAX),
      phase_li: merge(slot.phase_li),
    });
  }
}

/// 在当前线程分配一个采样槽；探针关闭时返回空守卫。
fn begin() -> Guard {
  if !enabled() {
    return Guard { id: None };
  }
  let id = SLOTS
    .try_with(|slots| {
      let mut slots = slots.borrow_mut();
      let id = slots.free_li.pop().unwrap_or_else(|| {
        slots.li.push(None);
        slots.li.len() - 1
      });
      slots.li[id] = Some(Slot {
        start: Instant::now(),
        phase_li: Vec::new(),
      });
      id
    })
    .ok();
  Guard { id }
}

/// 取走并释放一个采样槽；线程本地已销毁时返回 `None`（静默丢弃）。
fn finish(id: usize) -> Option<Slot> {
  SLOTS
    .try_with(|slots| {
      let mut slots = slots.borrow_mut();
      let slot = slots.li.get_mut(id).and_then(Option::take)?;
      slots.free_li.push(id);
      Some(slot)
    })
    .ok()
    .flatten()
}

/// 在一个读请求的 future 上建立采样上下文。
///
/// 开启时每次 poll 期间把本请求的槽号装进线程本地，嵌套路径（`read_entry` /
/// `descend_step` / `PageIo` / 快照登记）上的 [`record`] 因此总是归到正在
/// poll 的那条请求；交错推进的并发读各写各槽。关闭时直接转交内层 future，
/// 不碰 TLS、不分配、不取时钟。
///
/// 槽号存在首次 poll 所在线程；读请求不跨线程迁移（`Store` 含 `RefCell`，
/// 读 future 非 `Send`，shard / bench 均在单线程运行时内推进），故无需跨线程
/// 合并槽表。
pub async fn scope<F: Future>(fut: F) -> F::Output {
  let guard = begin();
  if !guard.is_on() {
    return fut.await;
  }
  let mut fut = pin!(fut);
  poll_fn(|cx| {
    let _current = guard.enter();
    fut.as_mut().poll(cx)
  })
  .await
}

/// 记录一段耗时（显式区间，可并发交错调用）。
///
/// 只写入当前 poll 中请求的槽；没有采样槽时静默丢弃。关闭时只剩一次
/// `enabled()` 判断；调用方若已在 `enabled()` 分支内，可直接传 `Some(t)`
/// 给 [`stop`]。
pub fn record(name: &'static str, dur: Duration) {
  if !enabled() {
    return;
  }
  let id = CURRENT.try_with(Cell::get).ok().flatten();
  let Some(id) = id else {
    return;
  };
  let us = u64::try_from(dur.as_micros()).unwrap_or(u64::MAX);
  let _ = SLOTS.try_with(|slots| {
    if let Ok(mut slots) = slots.try_borrow_mut()
      && let Some(Some(slot)) = slots.li.get_mut(id)
    {
      slot.phase_li.push((name, us));
    }
  });
}

/// 段起点：关闭或当前不在任何请求的 poll 中时返回 `None`，调用方据此跳过计时。
#[inline]
pub fn start() -> Option<Instant> {
  if !enabled() {
    return None;
  }
  CURRENT.try_with(Cell::get).ok().flatten()?;
  Some(Instant::now())
}

/// 记录从 `start` 起的耗时；`None` 时空操作。
#[inline]
pub fn stop(name: &'static str, start: Option<Instant>) {
  if let Some(start) = start {
    record(name, start.elapsed());
  }
}

/// 按段名线性合并（段数很少，≤ 10，不建表）。
fn merge(phase_li: Vec<(&'static str, u64)>) -> Vec<(&'static str, u64)> {
  let mut merged: Vec<(&'static str, u64)> = Vec::with_capacity(phase_li.len());
  for (name, us) in phase_li {
    match merged.iter_mut().find(|(exist, _)| *exist == name) {
      Some((_, sum)) => *sum += us,
      None => merged.push((name, us)),
    }
  }
  merged
}

/// 结算一条请求：报告入有界队列；总耗时超阈值时打印一行。
fn publish(report: Report) {
  if report.total_us >= threshold_us() {
    print(&report);
  }
  let _ = REPORT_LI.try_with(|report_li| {
    let mut report_li = report_li.borrow_mut();
    if report_li.len() == REPORT_KEEP {
      report_li.remove(0);
    }
    report_li.push(report);
  });
}

/// 打印一行分段明细（受 `FKV_SLOW_READ_MAX` 行数上限约束）。
fn print(report: &Report) {
  if *MAX_LINE != 0 {
    let printed = PRINTED.fetch_add(1, Ordering::Relaxed);
    if printed >= *MAX_LINE {
      return;
    }
  }
  let mut line = String::with_capacity(128);
  line.push_str("slow_read total_us=");
  line.push_str(Buffer::new().format(report.total_us));
  for (name, us) in &report.phase_li {
    line.push(' ');
    line.push_str(name);
    line.push('=');
    line.push_str(Buffer::new().format(*us));
  }
  eprintln!("{line}");
}

/// 取走当前线程最近完成的采样报告（测试 / 嵌入侧断言用）。
///
/// 报告只在探针开启时产生；每线程最多保留 [`REPORT_KEEP`] 条，超出丢弃最旧。
pub fn take_reports() -> Vec<Report> {
  REPORT_LI
    .try_with(|report_li| mem::take(&mut *report_li.borrow_mut()))
    .unwrap_or_default()
}

/// 当前线程仍被占用的采样槽数。
///
/// 测试用：探针关闭时读路径不应分配任何槽（恒为 0）。
#[doc(hidden)]
pub fn slot_num() -> usize {
  SLOTS
    .try_with(|slots| {
      slots
        .borrow()
        .li
        .iter()
        .filter(|slot| slot.is_some())
        .count()
    })
    .unwrap_or(0)
}
