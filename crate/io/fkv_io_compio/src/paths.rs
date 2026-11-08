//! table 分区路径与目录索引查询：路径推导、候选回退、偏移计算。

use std::{
  cmp::Reverse,
  path::PathBuf,
  sync::{Arc, atomic::Ordering},
};

use fkv_error::{KvError, Result};
use fkv_types::{
  FileId, FilePageId, MAIN_BRANCH_NAME, TableIdent, data_file_name, manifest_file_name,
  segment_file_name,
};

use crate::{
  CompioStore, DirIndex, DirScan, FileKind,
  fd_cache::{dir_stamp_of, scan_dir, spawn_dir_io},
};

impl CompioStore {
  pub(crate) fn part_dir(&self, tbl: &TableIdent) -> PathBuf {
    self.root.join(tbl.dir_name())
  }

  /// 同步扫描并重建索引（attach / reopen 阶段使用）。
  /// `force=false` 时仅在目录签名变化时扫描；返回是否真的扫描了。
  pub(crate) fn refresh_dir_index(&self, tbl: &TableIdent, force: bool) -> bool {
    let dir = self.part_dir(tbl);
    if !force {
      let stamp = dir_stamp_of(&dir);
      let index_map = self.dir_index_map.borrow();
      if let Some(index) = index_map.get(tbl)
        && index.scanned
        && index.stamp == stamp
      {
        return false;
      }
    }
    let (branch, term) = {
      let identity = self.identity.borrow();
      (identity.branch.clone(), identity.term)
    };
    let scan = scan_dir(&dir, &branch, term);
    self.apply_scan(tbl, scan)
  }

  /// 异步扫描：`metadata` + `read_dir` 都在阻塞线程池执行，不阻塞 reactor。
  pub(crate) async fn refresh_dir_index_async(&self, tbl: &TableIdent, force: bool) -> bool {
    let dir = self.part_dir(tbl);
    let cached = if force {
      None
    } else {
      self
        .dir_index_map
        .borrow()
        .get(tbl)
        .filter(|index| index.scanned)
        .map(|index| index.stamp.clone())
    };
    let (branch, term) = {
      let identity = self.identity.borrow();
      (identity.branch.clone(), identity.term)
    };
    // 闭包只捕获 PathBuf / identity 与目录签名，不把 &self / RefCell 送进阻塞线程。
    let scan = spawn_dir_io(move || {
      // 先做一次 stat，签名未变则直接跳过 getdents。
      if let Some(stamp) = &cached
        && dir_stamp_of(&dir) == *stamp
      {
        return None;
      }
      let scan = scan_dir(&dir, &branch, term);
      match &cached {
        Some(stamp) if *stamp == scan.stamp => None,
        _ => Some(scan),
      }
    })
    .await
    .unwrap_or(None);
    match scan {
      Some(scan) => self.apply_scan(tbl, scan),
      None => false,
    }
  }

  /// 用扫描结果替换索引；扫描计数在此累计。
  pub(crate) fn apply_scan(&self, tbl: &TableIdent, scan: DirScan) -> bool {
    self.data_scan_num.fetch_add(1, Ordering::Relaxed);
    self.segment_scan_num.fetch_add(1, Ordering::Relaxed);
    self.dir_index_map.borrow_mut().insert(
      tbl.clone(),
      DirIndex {
        stamp: scan.stamp,
        scanned: true,
        data: scan.data,
        segment: scan.segment,
        data_other: scan.data_other,
        segment_other: scan.segment_other,
      },
    );
    true
  }

  /// 读路径：优先当前 identity，否则回退到其它 identity 中 term 最大的文件。
  ///
  /// 返回 `Arc` 句柄而不是 `PathBuf`：调用方要把路径带过 `.await`，命中时
  /// refcount 自增即可，不需要克隆路径（perf-03）。
  pub(crate) fn index_path(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    kind: FileKind,
  ) -> Option<Arc<PathBuf>> {
    self.dir_index_map.borrow().get(tbl).and_then(|index| {
      index.primary(kind).get(&file_id).cloned().or_else(|| {
        index
          .other(kind)
          .get(&file_id)
          .and_then(|li| li.iter().max_by_key(|variant| variant.term))
          .map(|variant| Arc::new(variant.path.clone()))
      })
    })
  }

  pub(crate) async fn cached_data_path(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
  ) -> Option<Arc<PathBuf>> {
    let file_id = file_page_id >> self.pages_per_file_shift;
    if let Some(path) = self.index_path(tbl, file_id, FileKind::Data) {
      return Some(path);
    }
    self.refresh_dir_index_async(tbl, false).await;
    self.index_path(tbl, file_id, FileKind::Data)
  }

  /// 热路径单候选：primary 命中，且该 file id 没有其它 identity 变体。
  ///
  /// 返回 `Arc` 句柄（只做一次 refcount 自增，无堆分配）：`RefCell` 借用
  /// 在函数返回时结束，句柄可以安全跨 `.await` 持有（perf-03）。
  /// 返回 `None` 时调用方走 `data_path_li` 的通用路径（多候选 / 未命中 /
  /// 索引未扫描），语义与旧实现一致。
  pub(crate) fn single_data_path(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
  ) -> Option<Arc<PathBuf>> {
    let file_id = file_page_id >> self.pages_per_file_shift;
    let index_map = self.dir_index_map.borrow();
    let index = index_map.get(tbl)?;
    if index.data_other.contains_key(&file_id) {
      // 同 file id 还有旧 term 变体：保留 primary + term 降序回退顺序。
      return None;
    }
    index.data.get(&file_id).cloned()
  }

  /// 同一 file id 的全部候选路径，当前 identity 优先，其余按 term 降序。
  /// 新 term 会在同 file id 下另建文件，旧 term 页必须能回退读取。
  ///
  /// 只在多候选 / 索引未命中时使用；单候选热路径走 `single_data_path`。
  /// 候选天然互异，不再做 `dedup`：primary 与 other 按 `(branch, term)`
  /// 互斥，文件名编码了 identity，同一 file id 下不可能出现同路径两次
  /// （旧实现的 `dedup()` 在未整体排序时不保证生效，反而掩盖这一不变量）。
  pub(crate) fn collect_data_path_li(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
  ) -> Vec<PathBuf> {
    let file_id = file_page_id >> self.pages_per_file_shift;
    let mut path_li = Vec::new();
    if let Some(index) = self.dir_index_map.borrow().get(tbl) {
      if let Some(path) = index.data.get(&file_id) {
        path_li.push(path.as_ref().clone());
      }
      if let Some(other_li) = index.data_other.get(&file_id) {
        let mut sorted = other_li.clone();
        sorted.sort_by_key(|variant| Reverse(variant.term));
        path_li.extend(sorted.into_iter().map(|variant| variant.path));
      }
    }
    debug_assert!(
      path_li
        .iter()
        .enumerate()
        .all(|(i, path)| !path_li[..i].contains(path)),
      "候选路径按构造互异：{path_li:?}"
    );
    path_li
  }

  pub(crate) async fn data_path_li(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
  ) -> Vec<PathBuf> {
    let mut path_li = self.collect_data_path_li(tbl, file_page_id);
    if path_li.is_empty() {
      // 索引未扫描 / 文件被外部新增或删除：重扫一次再定位。
      self.refresh_dir_index_async(tbl, false).await;
      path_li = self.collect_data_path_li(tbl, file_page_id);
    }
    path_li
  }

  /// 由当前 `(branch, term)` 确定性生成路径。
  pub(crate) fn current_path(&self, tbl: &TableIdent, file_id: FileId, kind: FileKind) -> PathBuf {
    let identity = self.identity.borrow();
    let name = match kind {
      FileKind::Data => data_file_name(file_id, &identity.branch, identity.term),
      FileKind::Segment => segment_file_name(file_id, &identity.branch, identity.term),
    };
    self.part_dir(tbl).join(name)
  }

  /// 写路径：只按当前 `(branch, term)` 生成，绝不回退旧 identity 文件
  /// （否则新 term 会覆写旧 term 文件），也不触发目录扫描。
  pub(crate) fn write_path(&self, tbl: &TableIdent, file_id: FileId, kind: FileKind) -> PathBuf {
    let path = self.current_path(tbl, file_id, kind);
    let mut index_map = self.dir_index_map.borrow_mut();
    let index = index_map.entry(tbl.clone()).or_default();
    // 同一 file id 的路径由 identity 确定性推导，索引里已有相同值时不重新
    // 分配 Arc（页写每页都走这里，避免每页一次 PathBuf 克隆）。
    let stored_same = index
      .primary(kind)
      .get(&file_id)
      .is_some_and(|stored| stored.as_path() == path.as_path());
    if !stored_same {
      index
        .primary_mut(kind)
        .insert(file_id, Arc::new(path.clone()));
    }
    drop(index_map);
    if !stored_same {
      // 新文件名首次出现：目录项需要在下一次 durability barrier 落盘。
      self.dirty_dir.borrow_mut().insert(tbl.clone());
    }
    path
  }

  pub(crate) fn data_path(&self, tbl: &TableIdent, file_page_id: FilePageId) -> PathBuf {
    self.write_path(
      tbl,
      file_page_id >> self.pages_per_file_shift,
      FileKind::Data,
    )
  }

  pub(crate) async fn cached_segment_path(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
  ) -> Option<Arc<PathBuf>> {
    let file_id = segment_fp_id >> self.segments_per_file_shift;
    if let Some(path) = self.index_path(tbl, file_id, FileKind::Segment) {
      return Some(path);
    }
    self.refresh_dir_index_async(tbl, false).await;
    self.index_path(tbl, file_id, FileKind::Segment)
  }

  pub(crate) fn segment_path(&self, tbl: &TableIdent, segment_fp_id: FilePageId) -> PathBuf {
    self.write_path(
      tbl,
      segment_fp_id >> self.segments_per_file_shift,
      FileKind::Segment,
    )
  }

  /// 某 file id 在目录中出现的全部路径：当前 identity 与所有其它 identity 的变体。
  /// 末尾补当前 identity 的推导路径，覆盖索引尚未扫描到的情况。
  pub(crate) fn variant_path_li(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    kind: FileKind,
  ) -> Vec<PathBuf> {
    let mut path_li = Vec::new();
    if let Some(index) = self.dir_index_map.borrow().get(tbl) {
      if let Some(path) = index.primary(kind).get(&file_id) {
        path_li.push(path.as_ref().clone());
      }
      if let Some(li) = index.other(kind).get(&file_id) {
        path_li.extend(li.iter().map(|variant| variant.path.clone()));
      }
    }
    path_li.push(self.current_path(tbl, file_id, kind));
    path_li.sort_unstable();
    path_li.dedup();
    path_li
  }

  /// 某 `(file_id, branch, term)` 变体的候选路径（rev-39 精确删除）。
  ///
  /// identity 命中时取索引路径并补推导路径（索引可能未扫描）；非 identity
  /// 变体只取 other 表中匹配 `(branch, term)` 的条目。
  pub(crate) fn variant_paths_of(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    kind: FileKind,
    branch: &str,
    term: u64,
  ) -> Vec<PathBuf> {
    let identity_match = {
      let identity = self.identity.borrow();
      identity.branch == branch && identity.term == term
    };
    let mut path_li = Vec::new();
    if let Some(index) = self.dir_index_map.borrow().get(tbl) {
      if identity_match && let Some(path) = index.primary(kind).get(&file_id) {
        path_li.push(path.as_ref().clone());
      }
      if !identity_match && let Some(li) = index.other(kind).get(&file_id) {
        path_li.extend(
          li.iter()
            .filter(|variant| variant.ident() == (branch, term))
            .map(|variant| variant.path.clone()),
        );
      }
    }
    if identity_match {
      path_li.push(self.current_path(tbl, file_id, kind));
    }
    path_li.sort_unstable();
    path_li.dedup();
    path_li
  }

  /// 删除前确保目录索引已建立：GC / drop_table 会先 `list_*`，
  /// 直接调用时索引可能为空，不扫描会漏删同 id 的其它 identity 变体。
  pub(crate) async fn ensure_dir_index(&self, tbl: &TableIdent) {
    let scanned = self
      .dir_index_map
      .borrow()
      .get(tbl)
      .is_some_and(|index| index.scanned);
    if !scanned {
      self.refresh_dir_index_async(tbl, false).await;
    }
  }

  /// 丢弃一条路径缓存并强制下次 lookup 重扫目录（文件被外部替换时使用）。
  pub(crate) fn forget_path(&self, tbl: &TableIdent, file_id: FileId, kind: FileKind) {
    if let Some(index) = self.dir_index_map.borrow_mut().get_mut(tbl) {
      index.primary_mut(kind).remove(&file_id);
      index.other_mut(kind).remove(&file_id);
      index.stamp = None;
    }
  }

  /// 变体删除后清对应索引条目：identity 命中清 primary，否则只清 other 表里
  /// 匹配 `(branch, term)` 的条目（同 id 其它变体的读路由不受影响）。
  pub(crate) fn forget_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    kind: FileKind,
    branch: &str,
    term: u64,
  ) {
    let mut index_map = self.dir_index_map.borrow_mut();
    let Some(index) = index_map.get_mut(tbl) else {
      return;
    };
    let identity_match = {
      let identity = self.identity.borrow();
      identity.branch == branch && identity.term == term
    };
    if identity_match {
      index.primary_mut(kind).remove(&file_id);
    }
    if let Some(li) = index.other_mut(kind).get_mut(&file_id) {
      li.retain(|variant| variant.ident() != (branch, term));
      if li.is_empty() {
        index.other_mut(kind).remove(&file_id);
      }
    }
    index.stamp = None;
  }
}

impl CompioStore {
  pub(crate) fn page_offset(&self, file_page_id: FilePageId) -> u64 {
    let mask = (1u64 << self.pages_per_file_shift) - 1;
    (file_page_id & mask) * self.page_size as u64
  }

  pub(crate) fn segment_offset(&self, segment_fp_id: FilePageId) -> u64 {
    let mask = (1u64 << self.segments_per_file_shift) - 1;
    (segment_fp_id & mask) * self.segment_size as u64
  }

  pub(crate) fn sequence_file_size(&self) -> Result<u64> {
    (self.segment_size as u64)
      .checked_shl(u32::from(self.segments_per_file_shift))
      .ok_or(KvError::InvalidArgs)
  }

  pub(crate) fn manifest_path(&self, tbl: &TableIdent) -> PathBuf {
    // 本地活跃 manifest 固定为 `manifest_main_0`（sto-70 决策）：term 只影响
    // data/segment 对象名，standby 安装也统一落这个名字。branch manifest 走
    // `write_manifest_file` / `reset_manifest_file` 的显式名字，不经过这里。
    self
      .part_dir(tbl)
      .join(manifest_file_name(MAIN_BRANCH_NAME, 0))
  }

  /// 单次 `sync_data` 的 fdatasync 并发度：脏文件数与 `max_inflight_write`
  /// 取小。C++ `FdatasyncFiles` 一次把全部脏 fd 压进 ring（只受 ring 容量
  /// 约束），这里用配置的会话级在飞 IO 额度代替 ring 容量，脏文件少时不做
  /// 无谓的波次切分。
  ///
  /// 空集合返回 1：`buffer_unordered(0)` 不会推进非空流。
  pub(crate) fn sync_inflight_limit(&self, file_num: usize) -> usize {
    file_num.min(self.max_inflight_write).max(1)
  }
}
