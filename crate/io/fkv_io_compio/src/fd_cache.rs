//! FD 缓存与分区目录索引：按 (路径, 访问模式) 复用句柄、目录签名扫描。
//!
//! 索引类型本身定义在 crate 根（`DirIndex` / `DirScan`），本模块只放实现与
//! 扫描辅助函数；拆自原 `lib.rs`，仅做位置调整。

use std::{
  fs,
  fs::{metadata, read_dir},
  path::{Path, PathBuf},
  sync::Arc,
  thread,
};

use compio::{fs::File, runtime::spawn_blocking};
use fkv_config::DEFAULT_FD_LIMIT;
use fkv_error::{KvError, Result};
use fkv_types::{FILE_NAME_DATA, FILE_NAME_SEGMENT, FileId, TMP_SUFFIX, parse_data_file_name};
use gxhash::HashMap;

use crate::{Access, DirIndex, DirScan, DirStamp, FdCache, FdNode, FileKind, OtherVariant};

impl OtherVariant {
  /// 变体身份三元组的 `(branch, term)` 部分。
  pub(crate) fn ident(&self) -> (&str, u64) {
    (self.branch.as_ref(), self.term)
  }
}

impl DirIndex {
  /// 当前 `(branch, term)` 的 file id → 路径。
  pub(crate) fn primary(&self, kind: FileKind) -> &HashMap<FileId, Arc<PathBuf>> {
    match kind {
      FileKind::Data => &self.data,
      FileKind::Segment => &self.segment,
    }
  }

  pub(crate) fn primary_mut(&mut self, kind: FileKind) -> &mut HashMap<FileId, Arc<PathBuf>> {
    match kind {
      FileKind::Data => &mut self.data,
      FileKind::Segment => &mut self.segment,
    }
  }

  /// 其它 `(branch, term)` 的同 file id 文件。
  pub(crate) fn other(&self, kind: FileKind) -> &HashMap<FileId, Vec<OtherVariant>> {
    match kind {
      FileKind::Data => &self.data_other,
      FileKind::Segment => &self.segment_other,
    }
  }

  pub(crate) fn other_mut(&mut self, kind: FileKind) -> &mut HashMap<FileId, Vec<OtherVariant>> {
    match kind {
      FileKind::Data => &mut self.data_other,
      FileKind::Segment => &mut self.segment_other,
    }
  }
}

impl Default for FdCache {
  fn default() -> Self {
    Self {
      node_li: Vec::new(),
      free_li: Vec::new(),
      read: HashMap::default(),
      write: HashMap::default(),
      head: None,
      tail: None,
      limit: DEFAULT_FD_LIMIT as usize,
      owner: None,
    }
  }
}

// SAFETY: compio `File` 内部是线程本地的引用计数，本身 `!Send`。这里显式实现 `Send`
// 依赖 `bind_thread`：句柄只在打开它的线程内使用，`CompioStore` 被整体 move 到别的线程时
// 首次访问即清空缓存，不存在两个线程同时持有同一句柄的情况。
unsafe impl Send for FdCache {}

impl FdCache {
  /// 绑定当前线程；线程变化时丢弃旧句柄（compio thread-per-core 契约）。
  fn bind_thread(&mut self) {
    let current = thread::current().id();
    if self.owner != Some(current) {
      self.clear();
      self.owner = Some(current);
    }
  }

  fn map(&self, access: Access) -> &HashMap<PathBuf, usize> {
    match access {
      Access::Read => &self.read,
      Access::Write => &self.write,
    }
  }

  fn map_mut(&mut self, access: Access) -> &mut HashMap<PathBuf, usize> {
    match access {
      Access::Read => &mut self.read,
      Access::Write => &mut self.write,
    }
  }

  /// 从链表摘除节点；索引无效时空操作。
  fn unlink(&mut self, index: usize) {
    let Some(node) = self.node_li[index].as_mut() else {
      return;
    };
    let (prev, next) = (node.prev.take(), node.next.take());
    match prev {
      Some(prev) => {
        if let Some(prev_node) = self.node_li[prev].as_mut() {
          prev_node.next = next;
        }
      }
      None => self.head = next,
    }
    match next {
      Some(next) => {
        if let Some(next_node) = self.node_li[next].as_mut() {
          next_node.prev = prev;
        }
      }
      None => self.tail = prev,
    }
  }

  /// 头插：标记为最近使用。
  fn push_front(&mut self, index: usize) {
    let old_head = self.head;
    if let Some(node) = self.node_li[index].as_mut() {
      node.prev = None;
      node.next = old_head;
    }
    match old_head {
      Some(head) => {
        if let Some(head_node) = self.node_li[head].as_mut() {
          head_node.prev = Some(index);
        }
      }
      None => self.tail = Some(index),
    }
    self.head = Some(index);
  }

  /// 取一个槽位：优先复用空闲索引，否则追加。
  fn alloc(&mut self, node: FdNode) -> usize {
    match self.free_li.pop() {
      Some(index) => {
        self.node_li[index] = Some(node);
        index
      }
      None => {
        self.node_li.push(Some(node));
        self.node_li.len() - 1
      }
    }
  }

  /// 摘除并回收槽位（同时清索引）；文件句柄随节点 drop 关闭。
  fn release(&mut self, index: usize) {
    let Some(node) = self.node_li[index].take() else {
      return;
    };
    self.map_mut(node.access).remove(&node.path);
    self.free_li.push(index);
  }

  /// 命中后移到队首（O(1)），返回句柄。
  fn touch(&mut self, access: Access, path: &Path) -> Option<File> {
    self.bind_thread();
    let index = *self.map(access).get(path)?;
    self.unlink(index);
    self.push_front(index);
    let node = self.node_li[index].as_ref()?;
    Some(node.file.clone())
  }

  /// 取只读句柄。
  pub(crate) fn get_read(&mut self, path: &Path) -> Option<File> {
    self.touch(Access::Read, path)
  }

  /// 取写句柄。
  pub(crate) fn get_write(&mut self, path: &Path) -> Option<File> {
    self.touch(Access::Write, path)
  }

  pub(crate) fn insert(&mut self, path: PathBuf, access: Access, file: File) {
    self.bind_thread();
    // 同 (路径, 模式) 已缓存：原位替换句柄并移到队首，不增长条数。
    if let Some(&index) = self.map(access).get(&path) {
      if let Some(node) = self.node_li[index].as_mut() {
        node.file = file;
      }
      self.unlink(index);
      self.push_front(index);
      return;
    }
    let index = self.alloc(FdNode {
      path: path.clone(),
      access,
      file,
      prev: None,
      next: None,
    });
    self.map_mut(access).insert(path, index);
    self.push_front(index);
    // 超限从队尾逐个淘汰，单次 O(1)；`limit == 0` 表示不限制。
    if self.limit != 0 {
      while self.len() > self.limit {
        let Some(tail) = self.tail else {
          break;
        };
        self.unlink(tail);
        self.release(tail);
      }
    }
  }

  /// 丢弃某目录下所有句柄（detach 驱逐 table）。
  pub(crate) fn forget_under(&mut self, dir: &Path) {
    let index_li: Vec<usize> = self
      .node_li
      .iter()
      .enumerate()
      .filter_map(|(index, node)| {
        node
          .as_ref()
          .filter(|node| node.path.starts_with(dir))
          .map(|_| index)
      })
      .collect();
    for index in index_li {
      self.unlink(index);
      self.release(index);
    }
  }

  /// 失效一条路径的两种模式句柄。
  pub(crate) fn remove(&mut self, path: &Path) {
    for access in [Access::Read, Access::Write] {
      if let Some(&index) = self.map(access).get(path) {
        self.unlink(index);
        self.release(index);
      }
    }
  }

  pub(crate) fn len(&self) -> usize {
    self.read.len() + self.write.len()
  }

  pub(crate) fn clear(&mut self) {
    self.node_li.clear();
    self.free_li.clear();
    self.read.clear();
    self.write.clear();
    self.head = None;
    self.tail = None;
  }
}

/// 删除分区目录里残留的 `*.tmp`（原子写崩溃半成品）。只用于 attach 前。
pub(crate) fn remove_stale_tmp(dir: &Path) {
  let Ok(entry_li) = read_dir(dir) else {
    return;
  };
  for entry in entry_li.flatten() {
    let path = entry.path();
    let is_tmp = path
      .file_name()
      .and_then(|name| name.to_str())
      .is_some_and(|name| name.ends_with(TMP_SUFFIX));
    if is_tmp {
      let _ = fs::remove_file(&path);
    }
  }
}

/// 分区目录扫描：目录签名 + data / segment 路径索引（纯同步，供阻塞线程调用）。
pub(crate) fn scan_dir(dir: &Path, branch: &str, term: u64) -> DirScan {
  let mut scan = DirScan {
    stamp: dir_stamp_of(dir),
    ..DirScan::default()
  };
  let Ok(entry_li) = read_dir(dir) else {
    return scan;
  };
  for entry in entry_li.flatten() {
    let name = entry.file_name();
    let Some(name) = name.to_str() else {
      continue;
    };
    let is_data = name.starts_with(FILE_NAME_DATA);
    let is_segment = name.starts_with(FILE_NAME_SEGMENT);
    if !is_data && !is_segment {
      continue;
    }
    if let Some((file_id, file_branch, file_term)) = parse_data_file_name(name) {
      let path = entry.path();
      if file_branch == branch && file_term == term {
        if is_data {
          scan.data.insert(file_id, Arc::new(path));
        } else {
          scan.segment.insert(file_id, Arc::new(path));
        }
      } else {
        // 保留全部非当前 identity 的变体（branch + term）：GC 按变体身份
        // 精确判删（rev-39），缺 branch 无法区分跨 branch 的同 id 文件。
        let other = if is_data {
          &mut scan.data_other
        } else {
          &mut scan.segment_other
        };
        other.entry(file_id).or_default().push(OtherVariant {
          branch: file_branch.into(),
          term: file_term,
          path,
        });
      }
    }
  }
  scan
}

/// 目录签名。`metadata` 是阻塞 syscall，只允许在阻塞线程或 attach 阶段调用。
pub(crate) fn dir_stamp_of(dir: &Path) -> Option<DirStamp> {
  let meta = metadata(dir).ok()?;
  Some(DirStamp {
    modified: meta.modified().ok(),
    len: meta.len(),
  })
}

/// 在阻塞线程池执行同步目录 IO。阻塞线程 panic 时按 `IoFail` 处理。
pub(crate) async fn spawn_dir_io<T: Send + 'static>(
  f: impl FnOnce() -> T + Send + 'static,
) -> Result<T> {
  spawn_blocking(f).await.map_err(|_join| KvError::IoFail)
}
