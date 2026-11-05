//! fkv_testkit：测试夹具。
//!
//! 当前提供 `TempRoot`：进程内唯一、析构时递归删除的临时根目录。
//! 后续夹具（对齐 buffer、确定性时钟、crash harness）按需在此扩展。

use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all},
  io,
  mem::forget,
  path::{Path, PathBuf},
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

/// 进程内自增序号，保证同一纳秒内多次创建也不撞名。
static NEXT_SEQ: AtomicU64 = AtomicU64::new(0);

/// 进程内唯一的临时根目录；`Drop` 时递归删除（忽略删除失败）。
///
/// ```no_run
/// let root = fkv_testkit::TempRoot::new("store");
/// std::fs::write(root.join("db"), b"x").unwrap();
/// ```
#[derive(Debug)]
pub struct TempRoot {
  path: PathBuf,
}

impl TempRoot {
  /// 以 `tag` 为前缀创建临时目录；创建失败直接 panic（夹具无需恢复路径）。
  pub fn new(tag: &str) -> Self {
    Self::try_new(tag).unwrap_or_else(|err| panic!("fkv_testkit: 创建临时目录失败: {err}"))
  }

  /// 同 `new`，创建失败返回 `io::Error`。
  pub fn try_new(tag: &str) -> io::Result<Self> {
    let seq = NEXT_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map_or(0, |elapsed| elapsed.as_nanos());
    let path = temp_dir().join(format!("fkv_{}_{}_{}_{}", sanitize(tag), id(), nanos, seq));
    create_dir_all(&path)?;
    Ok(Self { path })
  }

  /// 根目录路径。
  pub fn path(&self) -> &Path {
    &self.path
  }

  /// 拼接根目录下的子路径；不检查存在性。
  pub fn join(&self, name: impl AsRef<Path>) -> PathBuf {
    self.path.join(name)
  }

  /// 取出路径并放弃自动清理，删除责任交给调用方。
  pub fn into_path(self) -> PathBuf {
    let path = self.path.clone();
    forget(self);
    path
  }
}

impl Drop for TempRoot {
  fn drop(&mut self) {
    let _ = remove_dir_all(&self.path);
  }
}

/// 目录名只保留 ASCII 字母数字、`_`、`-`，其余字符替换为 `_`。
fn sanitize(tag: &str) -> String {
  tag
    .chars()
    .map(|ch| {
      if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
        ch
      } else {
        '_'
      }
    })
    .collect()
}
