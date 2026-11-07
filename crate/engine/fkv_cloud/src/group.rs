//! 共享 cache 预算组与跨实例守卫（`CacheGroup` / owner 租约 / 在飞与发布登记）。

use core::{cmp, ops::Deref};
use std::sync::{
  Arc, Mutex, MutexGuard,
  atomic::{AtomicU64, Ordering},
};

use fkv_types::{FileId, TableIdent};
use gxhash::{HashMap, HashSet};

/// 标记「本地 manifest 已写、远端发布未完成」；prewarm 不得据此删除本地快照。
///
/// 登记落在共享组上：prewarm 与 shard 是不同实例，实例私有登记看不到对方的
/// 首写窗口，会删掉 shard 正在发布（本地有、远端还没有）的本地 manifest。
pub(crate) struct PublishGuard<'a> {
  pub(crate) group: &'a CacheGroup,
  pub(crate) tbl: TableIdent,
}

impl<'a> PublishGuard<'a> {
  pub(crate) fn new(group: &'a CacheGroup, tbl: &TableIdent) -> Self {
    group.publish_guard().insert(tbl.clone());
    Self {
      group,
      tbl: tbl.clone(),
    }
  }
}

impl Drop for PublishGuard<'_> {
  fn drop(&mut self) {
    self.group.publish_guard().remove(&self.tbl);
  }
}

/// 组级 scheme 回填守卫：同一 `CacheGroup` 内每表只允许一个回填者。
///
/// 实例私有的登记挡不住「两个 shard 实例同时回填同一张表」；登记落到组上，
/// `Drop` 释放，future 被取消时不会永久占位。
pub(crate) struct EnablingGuard<'a> {
  pub(crate) group: &'a CacheGroup,
  pub(crate) tbl: TableIdent,
}

impl Drop for EnablingGuard<'_> {
  fn drop(&mut self) {
    self.group.enabling_guard().remove(&self.tbl);
  }
}

/// 一次 segment 安装 / 写入的组内在飞登记。
///
/// 所有安装路径（读 / 写 / 预热）都从 [`CloudStore::begin_segment_install`] 入口
/// 登记，`Drop` 时注销：不再依赖 `single_flight`，预热直连安装也参与计数。
/// 按 owner 记深度：写路径的登记与其内部 install 的登记会嵌套，深度配对后
/// 内层 `Drop` 不会把外层仍在写的文件提前注销。
pub(crate) struct SegmentInflightGuard<'a> {
  pub(crate) group: &'a CacheGroup,
  pub(crate) owner: u64,
  pub(crate) tbl: TableIdent,
  pub(crate) file_id: FileId,
}

impl<'a> SegmentInflightGuard<'a> {
  /// 无条件登记一次在飞（写路径用：本实例正在写这个文件）。
  pub(crate) fn new(group: &'a CacheGroup, owner: u64, tbl: &TableIdent, file_id: FileId) -> Self {
    group.set_segment_inflight(owner, tbl, file_id, true);
    Self {
      group,
      owner,
      tbl: tbl.clone(),
      file_id,
    }
  }

  /// 抢占安装独占权：登记与「他人是否在飞 / 持脏」判定在同一把锁内完成。
  ///
  /// 返回 `None` 时必须让路：同组另一实例正在写同一文件（或持有未上传的脏
  /// 数据），覆盖会把它的写抹成远端旧值，随后它的 flush 会把旧值上传成权威。
  pub(crate) fn claim(
    group: &'a CacheGroup,
    owner: u64,
    tbl: &TableIdent,
    file_id: FileId,
  ) -> Option<Self> {
    group
      .claim_segment_inflight(owner, tbl, file_id)
      .then(|| Self {
        group,
        owner,
        tbl: tbl.clone(),
        file_id,
      })
  }
}

impl Drop for SegmentInflightGuard<'_> {
  fn drop(&mut self) {
    self
      .group
      .set_segment_inflight(self.owner, &self.tbl, self.file_id, false);
  }
}

/// `CloudStore` 在共享组内的 owner 租约。
///
/// 共享组通常比实例长寿：实例销毁时必须清掉本 owner 的脏登记，否则该
/// segment 文件在组内永久 busy，后续 scheme 回填永远跳过它。
#[derive(Debug)]
pub(crate) struct OwnerLease {
  pub(crate) group: Arc<CacheGroup>,
  pub(crate) id: u64,
}

impl OwnerLease {
  pub(crate) fn new(group: Arc<CacheGroup>) -> Self {
    let id = group.alloc_instance_id();
    Self { group, id }
  }
}

impl Deref for OwnerLease {
  type Target = CacheGroup;

  fn deref(&self) -> &CacheGroup {
    &self.group
  }
}

impl Drop for OwnerLease {
  fn drop(&mut self) {
    self.group.release_owner(self.id);
  }
}

/// 跨实例共享的本地 cache 预算组。
///
/// 同一个 `CacheGroup` 下的所有 `CloudStore`（例如同一运行时的各 shard 与运行时
/// 侧预热线程）共用一个字节上限：`resident_bytes` 是整组记账，任一实例写入后
/// 按**全局**用量触发淘汰，因此 `local_space_limit` 是整机本地 cache 上界，
/// 不再是 `(实例数 × 预算)`。
///
/// 未共享（`with_cache_budget`）时每个实例自己一组，行为与旧版完全一致。
/// 淘汰仍只能摘本实例的 LRU：某个实例在全局超预算但自己没有可淘汰条目时
/// 无法替别的实例删文件；它自己新写入的 clean 条目会立即被摘回，磁盘上界
/// 只额外多出待删队列（`cache_budget / 8`，`drain_evictions` 后归零）。
///
/// 实例销毁不主动归还自己的记账：共享组通常与运行时同生命周期，若组比实例
/// 长寿，残留记账只会让后续实例更早淘汰（保守方向），不会突破预算。
#[derive(Debug)]
pub struct CacheGroup {
  pub(crate) limit: u64,
  pub(crate) resident_bytes: AtomicU64,
  /// 整组在飞 / 脏的 segment 文件登记：scheme 回填据此跳过**任一实例**正在
  /// 写的文件（见 [`CloudStore::segment_backfill_skip`]）。
  ///
  /// 覆盖边界：只覆盖共享同一 `CacheGroup` 的实例（同一运行时内的各 shard 与
  /// 预热实例）。跨进程共用同一本地 root 时观测不到对方的写现场，这种部署
  /// 限定只由该分区的写实例启用 scheme，预热 / 只读实例不得触发回填。
  ///
  /// 按表嵌套，查询只借 `&TableIdent`：`segment_backfill_skip` 在回填循环里
  /// 对每个文件调一次，扁平 key 每次都要 clone 两个 `TableIdent`。
  pub(crate) segment_busy: Mutex<HashMap<TableIdent, HashMap<FileId, SegmentBusy>>>,
  /// 整组正在回填 scheme 的表：实例私有的登记挡不住两个 shard 实例同时回填。
  pub(crate) enabling_tbl_set: Mutex<HashSet<TableIdent>>,
  /// 整组正在发布本地 manifest 的表：本地已写、远端 PUT 尚未确认。
  pub(crate) publish_tbl_set: Mutex<HashSet<TableIdent>>,
  /// 实例 id 分配器：脏登记按 owner 集合记，避免一个实例 `mark_clean` 抹掉
  /// 另一个实例仍在写的脏状态。
  pub(crate) next_instance_id: AtomicU64,
}

/// 单个 segment 文件的跨实例写现场。
#[derive(Debug, Default)]
pub(crate) struct SegmentBusy {
  /// 各实例的在飞安装 / 写入深度：同一实例的写路径与其内部 install 会嵌套
  /// 登记，用深度配对。只记计数（旧实现）无法区分 owner，一个实例的注销会
  /// 抹掉另一个实例仍在写的状态。
  pub(crate) inflight_owner: HashMap<u64, u32>,
  /// 持有未上传脏数据的实例 id。
  pub(crate) dirty_owner_set: HashSet<u64>,
}

impl SegmentBusy {
  pub(crate) fn busy(&self) -> bool {
    !self.inflight_owner.is_empty() || !self.dirty_owner_set.is_empty()
  }

  /// 除 `owner` 之外是否有实例正在安装或有未上传脏数据。
  pub(crate) fn foreign_busy(&self, owner: u64) -> bool {
    self.inflight_owner.keys().any(|id| *id != owner)
      || self.dirty_owner_set.iter().any(|id| *id != owner)
  }

  #[cfg(test)]
  pub(crate) fn inflight_num(&self) -> u32 {
    self.inflight_owner.values().sum()
  }
}

impl CacheGroup {
  /// `limit == 0` 表示不限制预算（与 [`CloudStore::with_cache_budget`] 一致）。
  pub fn new(limit: u64) -> Self {
    Self {
      limit,
      resident_bytes: AtomicU64::new(0),
      segment_busy: Mutex::new(HashMap::default()),
      enabling_tbl_set: Mutex::new(HashSet::default()),
      publish_tbl_set: Mutex::new(HashSet::default()),
      next_instance_id: AtomicU64::new(1),
    }
  }

  pub fn limit(&self) -> u64 {
    self.limit
  }

  /// 整组当前驻留字节（各实例 resident 记账之和）。
  ///
  /// 同一批文件被多个实例各自登记时会重复计数（偏保守，只会早淘汰，不会超限）。
  pub fn resident_bytes(&self) -> u64 {
    self.resident_bytes.load(Ordering::Relaxed)
  }

  /// 为一个 `CloudStore` 分配组内唯一 owner id。
  pub(crate) fn alloc_instance_id(&self) -> u64 {
    self.next_instance_id.fetch_add(1, Ordering::Relaxed)
  }

  /// 登记 / 注销一次在飞 segment 下载（按 owner 记深度）。
  pub(crate) fn set_segment_inflight(
    &self,
    owner: u64,
    tbl: &TableIdent,
    file_id: FileId,
    inflight: bool,
  ) {
    let mut map = self.busy_guard();
    let file_map = map.entry(tbl.clone()).or_default();
    let entry = file_map.entry(file_id).or_default();
    if inflight {
      *entry.inflight_owner.entry(owner).or_insert(0) += 1;
    } else {
      match entry.inflight_owner.get_mut(&owner) {
        Some(depth) if *depth > 1 => *depth -= 1,
        Some(_) => {
          entry.inflight_owner.remove(&owner);
        }
        // 未配对的注销：不能减别人的深度，也不能让记账下溢。
        None => {}
      }
    }
    if !entry.busy() {
      file_map.remove(&file_id);
    }
    if file_map.is_empty() {
      map.remove(tbl);
    }
  }

  /// 抢占该 segment 文件的安装独占权：登记本 owner 并判定他人是否在飞 / 持脏。
  ///
  /// 判定与登记在同一把锁内完成：返回 `true` 说明此刻没有别的实例在写该文件，
  /// 调用方的整文件覆盖不会抹掉对方未上传的写。返回 `false` 时不留下任何登记。
  pub(crate) fn claim_segment_inflight(
    &self,
    owner: u64,
    tbl: &TableIdent,
    file_id: FileId,
  ) -> bool {
    let mut map = self.busy_guard();
    let file_map = map.entry(tbl.clone()).or_default();
    let entry = file_map.entry(file_id).or_default();
    let foreign = entry.foreign_busy(owner);
    if !foreign {
      *entry.inflight_owner.entry(owner).or_insert(0) += 1;
    }
    if !entry.busy() {
      file_map.remove(&file_id);
    }
    if file_map.is_empty() {
      map.remove(tbl);
    }
    !foreign
  }

  /// 除 `owner` 之外是否有实例正在写该 segment 文件（在飞或持脏）。
  pub(crate) fn segment_foreign_busy(&self, owner: u64, tbl: &TableIdent, file_id: FileId) -> bool {
    self
      .busy_guard()
      .get(tbl)
      .and_then(|file_map| file_map.get(&file_id))
      .is_some_and(|entry| entry.foreign_busy(owner))
  }

  /// 批量注销某 owner 在一张表下的多份脏登记（一次加锁）。
  ///
  /// 表级清理（detach / `Drop` / `invalidate`）要清 N 份登记，逐个调用
  /// `set_segment_dirty` 会加 N 次锁并在每次里重建 / 删除外层表条目。
  pub(crate) fn clear_segment_dirty_li(&self, owner: u64, tbl: &TableIdent, file_id_li: &[FileId]) {
    if file_id_li.is_empty() {
      return;
    }
    let mut map = self.busy_guard();
    let Some(file_map) = map.get_mut(tbl) else {
      return;
    };
    for file_id in file_id_li {
      let Some(entry) = file_map.get_mut(file_id) else {
        continue;
      };
      entry.dirty_owner_set.remove(&owner);
      if !entry.busy() {
        file_map.remove(file_id);
      }
    }
    if file_map.is_empty() {
      map.remove(tbl);
    }
  }

  /// 登记 / 注销某实例持有的脏 segment 文件。
  pub(crate) fn set_segment_dirty(
    &self,
    owner: u64,
    tbl: &TableIdent,
    file_id: FileId,
    dirty: bool,
  ) {
    let mut map = self.busy_guard();
    let file_map = map.entry(tbl.clone()).or_default();
    let entry = file_map.entry(file_id).or_default();
    if dirty {
      entry.dirty_owner_set.insert(owner);
    } else {
      entry.dirty_owner_set.remove(&owner);
    }
    if !entry.busy() {
      file_map.remove(&file_id);
    }
    if file_map.is_empty() {
      map.remove(tbl);
    }
  }

  /// 实例销毁时清掉它名下的全部脏登记。
  ///
  /// 共享组比实例长寿时，残留 owner 会让该文件在组内永久 busy，scheme 回填
  /// 永远跳过它。
  pub(crate) fn release_owner(&self, owner: u64) {
    let mut map = self.busy_guard();
    map.retain(|_, file_map| {
      file_map.retain(|_, entry| {
        entry.dirty_owner_set.remove(&owner);
        entry.busy()
      });
      !file_map.is_empty()
    });
  }

  /// 该 segment 文件是否有任一实例正在安装或有未上传脏数据。
  pub(crate) fn segment_busy(&self, tbl: &TableIdent, file_id: FileId) -> bool {
    self
      .busy_guard()
      .get(tbl)
      .and_then(|file_map| file_map.get(&file_id))
      .is_some_and(SegmentBusy::busy)
  }

  /// 尝试取得组级回填权；已有实例在回填同一表时返回 `None`。
  pub(crate) fn try_begin_enable(&self, tbl: &TableIdent) -> Option<EnablingGuard<'_>> {
    let mut set = self.enabling_guard();
    set.insert(tbl.clone()).then(|| EnablingGuard {
      group: self,
      tbl: tbl.clone(),
    })
  }

  /// 锁中毒时继续使用数据：登记表只做保守判定，不参与内存安全。
  pub(crate) fn busy_guard(
    &self,
  ) -> MutexGuard<'_, HashMap<TableIdent, HashMap<FileId, SegmentBusy>>> {
    self
      .segment_busy
      .lock()
      .unwrap_or_else(|poison| poison.into_inner())
  }

  pub(crate) fn enabling_guard(&self) -> MutexGuard<'_, HashSet<TableIdent>> {
    self
      .enabling_tbl_set
      .lock()
      .unwrap_or_else(|poison| poison.into_inner())
  }

  pub(crate) fn publish_guard(&self) -> MutexGuard<'_, HashSet<TableIdent>> {
    self
      .publish_tbl_set
      .lock()
      .unwrap_or_else(|poison| poison.into_inner())
  }

  /// 该表是否有任一实例正在发布本地 manifest。
  pub(crate) fn is_publishing(&self, tbl: &TableIdent) -> bool {
    self.publish_guard().contains(tbl)
  }

  pub(crate) fn reserve(&self, bytes: u64) {
    if bytes > 0 {
      self.resident_bytes.fetch_add(bytes, Ordering::Relaxed);
    }
  }

  pub(crate) fn release(&self, bytes: u64) {
    if bytes == 0 {
      return;
    }
    // 饱和减法一次完成（`try_update` 自 1.95 stable，即本仓 MSRV）。记账对称时
    // 不会下溢；实例销毁未归还等异常路径按 0 饱和，避免整组记账回绕成超大值
    // 后把 cache 全部淘汰。
    let _ = self
      .resident_bytes
      .try_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
        Some(used.saturating_sub(bytes))
      });
  }

  /// 记账从 `old` 变为 `new`（新文件 / 文件增长 / 文件缩小）。
  pub(crate) fn adjust(&self, old: u64, new: u64) {
    match new.cmp(&old) {
      cmp::Ordering::Greater => self.reserve(new - old),
      cmp::Ordering::Less => self.release(old - new),
      cmp::Ordering::Equal => {}
    }
  }
}
