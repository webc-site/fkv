//! 进程内 warn 日志捕获夹具。
//!
//! `log` 的全局 logger 进程内只能安装一次；各集成测试二进制相互独立，因此
//! [`capture_warn`] 首次调用时安装捕获器，返回的句柄只观察创建之后产生的
//! 记录。并行用例仍应按各自独有的关键字（如临时根目录名）过滤，避免串味。

use std::sync::{Mutex, MutexGuard, Once};

use log::{Level, LevelFilter};

/// 捕获到的 warn 文本历史；只追加，句柄用起始下标切出本次窗口。
static WARNINGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static INIT: Once = Once::new();

struct WarningCapture;

impl log::Log for WarningCapture {
  fn enabled(&self, metadata: &log::Metadata) -> bool {
    metadata.level() <= Level::Warn
  }

  fn log(&self, record: &log::Record) {
    if self.enabled(record.metadata()) {
      lock().push(record.args().to_string());
    }
  }

  fn flush(&self) {}
}

fn lock() -> MutexGuard<'static, Vec<String>> {
  // 捕获器只做 push，不持有跨 panic 的临界区；中毒时取回内部数据继续用。
  WARNINGS.lock().unwrap_or_else(|err| err.into_inner())
}

fn install() {
  INIT.call_once(|| {
    let _ = log::set_boxed_logger(Box::new(WarningCapture));
    log::set_max_level(LevelFilter::Warn);
  });
}

/// 从调用时刻开始捕获 warn 日志。
pub fn capture_warn() -> WarnCapture {
  install();
  WarnCapture {
    start: lock().len(),
  }
}

/// warn 日志捕获句柄；日志本身保留到进程结束，句柄只负责划定观察窗口。
#[derive(Debug)]
pub struct WarnCapture {
  start: usize,
}

impl WarnCapture {
  /// 捕获期内是否出现同时包含 `needles` 全部子串的 warn。
  ///
  /// 空 `needles` 恒为 `false`：否则「任意一条日志」都会命中，断言失去意义。
  pub fn warned_with(&self, needles: &[&str]) -> bool {
    !needles.is_empty()
      && self
        .messages()
        .iter()
        .any(|message| needles.iter().all(|needle| message.contains(needle)))
  }

  /// 捕获期内的全部 warn 文本快照。
  pub fn messages(&self) -> Vec<String> {
    lock().get(self.start..).unwrap_or_default().to_vec()
  }
}
