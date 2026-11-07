//! 分区收尾与文件 / 本地 GC。

use fkv_error::{KvError, Result};
use fkv_io::{FileIo, ManifestIo, PageIo};
use fkv_task_gc::GcScope;
use fkv_types::{NULL_PAGE, TableIdent};
use gxhash::HashSet;

use crate::Store;

impl<I: PageIo + FileIo + ManifestIo> Store<I> {
  /// 表逻辑为空时收尾分区（对齐 C++ `ExecuteLocalGC` 第 5 步 →
  /// `TryCleanupLocalPartitionDir`）：只尝试移除已无任何文件的分区目录，
  /// **不删除活跃 manifest**（sem-59）。
  ///
  /// C++ 的 GC 收尾实质是一次 `rmdir(2)`（`UnlinkAt(AT_REMOVEDIR)`，
  /// `async_io_manager.cpp:2774-2790`）：manifest 文件还在时返回 `-ENOTEMPTY`，
  /// 调用方把 `-ENOENT / -ENOTEMPTY` 都当成功吞掉（`async_io_manager.cpp:1470-1476`）
  /// ——manifest 对象（空快照 + 删光批 delta）永久保留，`manifest_size_ > 0`，
  /// 其后每个 no-op 批整写空快照（C++ `need_empty_snapshot`，sem-56），
  /// 首个真实写批走 append。全仓删 manifest 的唯一生产调用点是显式 Drop
  /// （`BatchWriteTask::Drop` → `DropManifest`，`batch_write_task.cpp:1896`），
  /// fkv 对应 `drop_table`；本地 GC 收尾（`file_gc.cpp:364-371`）与云端收尾
  /// （`ExecuteCloudGC` 只删远端 data / segment 对象 + 调度本地目录清理）都不删。
  /// 因此本函数保留 manifest 与 `manifest_started / manifest_size` 原样，云端
  /// 后端同样绝不触碰远端 manifest 对象。
  ///
  /// 只处理「GC 已删光 data / segment」的表：root 归零、两套 mapper 无活跃映射、
  /// 目录内 data / segment 文件列表为空，缺一不可。`try_remove_empty_partition_dir`
  /// 遇到还有 manifest / 临时文件的目录会原样保留（对齐 C++ `-ENOTEMPTY` 分支），
  /// 绝不会误删仍被引用的分区。
  ///
  /// 收尾失败语义同 [`Self::file_gc`]：目录 IO 瞬时错误只告警、由调用方吞掉
  /// （`Store::maintain` 即按此口径，不中断 retired 页回池）；本函数保持
  /// `Result<bool>` 把错误交还调用方裁决（io-31）。
  pub async fn cleanup_empty_partition(&mut self, tbl: &TableIdent) -> Result<bool> {
    if !self.table_is_logically_empty(tbl).await? {
      return Ok(false);
    }
    self.io.try_remove_empty_partition_dir(tbl).await
  }

  /// 逻辑空表判定：已打开、树根为空、两套映射无活跃项、目录无数据文件。
  async fn table_is_logically_empty(&self, tbl: &TableIdent) -> Result<bool> {
    let Some(state) = self.table_map.get(tbl) else {
      return Ok(false);
    };
    // `manifest_started == false` 表示从未写过 manifest（无 manifest 收尾面），
    // 无需重做；已清空且保留 manifest 的分区每次维护都会走到 rmdir 尝试，
    // 与 C++ 每轮 GC 的 `TryCleanupLocalPartitionDir` 同节奏。
    if !state.manifest_started || state.root != Some(NULL_PAGE) {
      return Ok(false);
    }
    if state
      .mapper
      .as_ref()
      .is_none_or(|mapper| mapper.mapping_count() != 0)
    {
      return Ok(false);
    }
    if state
      .segment_mapper
      .as_ref()
      .is_some_and(|mapper| mapper.mapping_count() != 0)
    {
      return Ok(false);
    }
    if !self.io.list_file_id_li(tbl).await?.is_empty()
      || !self.io.list_segment_file_id_li(tbl).await?.is_empty()
    {
      return Ok(false);
    }
    Ok(true)
  }

  /// GC 删掉整文件后同步分配器空间统计，供下一轮 `needs_maintain` /
  /// `needs_segment_maintain` 使用。全空映射直接跳界，避免下一轮再为
  /// 空空间触发扫描。
  pub(crate) fn refresh_space_stat(&mut self, tbl: &TableIdent) {
    let shift = self.pages_per_file_shift;
    if let Some(mapper) = self
      .table_map
      .get_mut(tbl)
      .and_then(|state| state.mapper.as_mut())
    {
      if mapper.mapping_count() == 0 {
        mapper.skip_empty_space(shift);
      } else {
        mapper.update_stat_by_scan(shift);
      }
    }
    let segment_shift = self.segment_shift();
    if let Some(mapper) = self
      .table_map
      .get_mut(tbl)
      .and_then(|state| state.segment_mapper.as_mut())
    {
      if mapper.mapping_count() == 0 {
        mapper.skip_empty_space(segment_shift);
      } else {
        mapper.update_stat_by_scan(segment_shift);
      }
    }
  }

  /// 文件 GC（对齐 C++ `FileGcRequest` → `WriteTask::RunFileGc`）：按 IO 后端的
  /// 语义删除无引用文件 —— 本地 / 内存后端删本地文件，云端后端删远端对象并清
  /// 对应本地 cache。data 与 segment 在同一轮处理。
  ///
  /// retained set 以所有 manifest（active + branch + archive）为准，再并上当前
  /// 内存 mapping（含未 flush 的 page）与**所有活跃 mapper 快照**，并按 per-branch
  /// 高水位保护「已写数据页、manifest 尚未提交」的文件。
  ///
  /// 快照并集保证：读请求跨 `await` 持旧映射时，旧快照独占的 file page 不会被
  /// 删除。无活跃快照时该分支只是一次空表遍历，不产生额外分配。快照释放后
  /// 旧页从**下一轮** GC 起可回收，GC 不会因一次 drop 当场重跑。
  ///
  /// term 熔断（sem-26）：存在 term 高于本实例的 manifest 时整轮不删任何文件；
  /// **同 active branch** 更高 term（本 owner 被接管）返回 `ExpiredTerm`，其它
  /// branch 的更高 term 只返回 `Ok(0)`（本轮跳过，不改变生命周期）。
  ///
  /// 收尾（空分区目录移除）失败只告警不上抛：已完成的删除数照常返回（io-31，
  /// 对齐 C++ `file_gc.cpp:364-371` 仅 `LOG(WARNING)` + `NoError`）。
  pub async fn file_gc(&self, tbl: &TableIdent, pages_per_file_shift: u8) -> Result<usize> {
    self
      .gc_with_scope(tbl, pages_per_file_shift, GcScope::File)
      .await
  }

  /// 本地 GC（对齐 C++ `LocalGcRequest` → `WriteTask::RunLocalFileGc`）：引用集
  /// 判定同 [`Self::file_gc`]，但只列举 / 删除本地文件与本地 cache，**绝不删除
  /// 远端对象**（C++ `ExecuteLocalGC` 只走本地 directory + IouringMgr）。
  ///
  /// 云端后端不剪枝远端 manifest：旧 term manifest 的回收仍由 `FileGc` 的
  /// `prune_superseded_manifest_file_li` 负责，本地 GC 只处理 data / segment。
  /// term 熔断口径与 [`Self::file_gc`] 相同（按 active branch 过滤，sem-26）。
  pub async fn local_gc(&self, tbl: &TableIdent, pages_per_file_shift: u8) -> Result<usize> {
    self
      .gc_with_scope(tbl, pages_per_file_shift, GcScope::Local)
      .await
  }

  async fn gc_with_scope(
    &self,
    tbl: &TableIdent,
    pages_per_file_shift: u8,
    scope: GcScope,
  ) -> Result<usize> {
    let segment_shift = self.segment_shift();
    let (mut data_set, mut segment_set, mut guard) =
      fkv_task_gc::manifest_retained_file_page_set_budget(
        &self.io,
        tbl,
        pages_per_file_shift,
        segment_shift,
        self.yield_budget_us,
      )
      .await?;
    if let Some(mapper) = self
      .table_map
      .get(tbl)
      .and_then(|state| state.mapper.as_ref())
    {
      // 内存 mapping 的引用不带 (branch, term)：并入 retained 的同时登记 pin，
      // 变体级 GC 对同 id 全部变体整体保留（rev-39）。
      guard.add_pinned_data_mapping(mapper.tbl(), &mut data_set, pages_per_file_shift);
      guard.seed(
        &self.branch_name,
        self.term,
        mapper.max_fp_id(),
        pages_per_file_shift,
      );
    }
    if let Some(segment_mapper) = self
      .table_map
      .get(tbl)
      .and_then(|state| state.segment_mapper.as_ref())
    {
      guard.add_pinned_segment_mapping(segment_mapper.tbl(), &mut segment_set, segment_shift);
      guard.seed_segment(
        &self.branch_name,
        self.term,
        segment_mapper.max_fp_id(),
        segment_shift,
      );
    }
    guard.set_process_term(self.term);
    // 对齐 C++ `file_gc.cpp:1553-1560`：看到任一新 owner 的 manifest term
    // 高于本进程 term，整轮 GC 绝不删除它的对象。这是**删除安全**的全局闸门
    // （manifest 只记录已提交文件，新 owner 未提交的 in-flight 文件没有任何
    // 引用，只有整轮跳过才不会误删）。
    if guard.term_above(self.term) {
      // attach term 是 per-branch：只有**当前 active branch** 出现更高 term 才
      // 说明本 owner 被同 branch 接管，返回 `ExpiredTerm` 交给 shard 收口；
      // 其它 branch（含祖先 main）的更高 term 是同一分区里并存的 owner，不等于
      // 本 branch 过期（sem-26）。此路径同样不删除任何文件，只跳过本轮 GC，
      // 避免把「别的 branch 有更高 term」误判成生命周期过期（sem-22 残差）。
      if !guard.term_above_branch(&self.branch_name, self.term) {
        log::debug!(
          "GC 跳过：存在更高 term 的 manifest，但都不属于 active branch {}（term {}）",
          self.branch_name,
          self.term
        );
        return Ok(0);
      }
      return Err(KvError::ExpiredTerm);
    }
    // 旧快照独占的页不在当前 mapper / 最新 manifest 里，必须显式并集；
    // 快照读者可能按旧变体读取，同 id 全部变体一并 pin（rev-39）。
    let mut snapshot_data_set = HashSet::default();
    self
      .snapshots
      .data_retained_into(tbl, &mut snapshot_data_set);
    if !snapshot_data_set.is_empty() {
      guard.pin_data_page_set(&snapshot_data_set, pages_per_file_shift);
      data_set.extend(snapshot_data_set);
    }
    let mut snapshot_segment_set = HashSet::default();
    self
      .snapshots
      .segment_retained_into(tbl, &mut snapshot_segment_set);
    if !snapshot_segment_set.is_empty() {
      guard.pin_segment_page_set(&snapshot_segment_set, segment_shift);
      segment_set.extend(snapshot_segment_set);
    }
    // 有 manifest 损坏 / 不可读时引用集合不完整：本轮不删文件、不剪枝，
    // 只记录，等坏文件恢复或修复后再回收，绝不误删仍被引用的数据。
    if guard.is_damaged() {
      log::warn!(
        "GC 跳过删除：存在损坏/不可读 manifest {:?}",
        guard.damaged_li()
      );
      return Ok(0);
    }
    let mut removed = match scope {
      GcScope::File => {
        fkv_task_gc::gc_files_with_retained_budget(
          &self.io,
          tbl,
          &data_set,
          &guard,
          pages_per_file_shift,
          self.yield_budget_us,
        )
        .await?
      }
      GcScope::Local => {
        fkv_task_gc::gc_local_files_with_retained_budget(
          &self.io,
          tbl,
          &data_set,
          &guard,
          pages_per_file_shift,
          self.yield_budget_us,
        )
        .await?
      }
    };
    if let Some(layout) = self.large_value_layout {
      let segment_shift = layout.segments_per_file_shift();
      removed += match scope {
        GcScope::File => {
          fkv_task_gc::gc_segment_files_with_retained_budget(
            &self.io,
            tbl,
            &segment_set,
            &guard,
            segment_shift,
            self.yield_budget_us,
          )
          .await?
        }
        GcScope::Local => {
          fkv_task_gc::gc_local_segment_files_with_retained_budget(
            &self.io,
            tbl,
            &segment_set,
            &guard,
            segment_shift,
            self.yield_budget_us,
          )
          .await?
        }
      };
    }
    // 剪枝放在数据文件删除之后：本轮的 retained set 仍包含旧 manifest 的引用，
    // 只被旧版本引用的文件要等下一轮 GC 才回收，避免「剪 manifest → 还有读者
    // 按旧版本判定」的窗口。云端后端只在远端最新 term 就是本实例 term 时
    // 才删更旧的无 tag manifest（见 `CloudStore::prune_superseded_manifest_file_li`）。
    // `LocalGc` 跳过：该实现会删远端对象，而本地 GC 只处理本地文件
    // （C++ `ExecuteLocalGC` 也不剪 manifest）。
    if scope == GcScope::File {
      self
        .io
        .prune_superseded_manifest_file_li(tbl, &self.branch_name, self.term)
        .await?;
    }
    // 收尾对齐 C++ `TryCleanupLocalPartitionDir`：目录内已无任何条目才移除；
    // 还有 manifest / 临时文件 / 未知残留时原样保留（绝不递归删除）。
    // 收尾失败只告警不上抛（io-31）：对齐 C++ `ExecuteLocalGC` 对
    // `TryCleanupLocalPartitionDir` 失败仅 `LOG(WARNING)`、函数仍返回
    // `NoError`（file_gc.cpp:364-371）——文件删除此时已经成功完成，目录
    // 收尾的瞬时错误（read_dir EMFILE、跨进程写入触发的 ENOTEMPTY、EACCES
    // 等，compio 实现已把 ENOENT / ENOTEMPTY 按「未移除」吞掉）不应推翻
    // GC 结果或丢掉删除数 `removed`；云模式 `ExecuteCloudGC` 的本地目录
    // 清理更是异步调度（file_gc.cpp:1613-1618），失败无从影响回执。
    if let Err(err) = self.io.try_remove_empty_partition_dir(tbl).await {
      log::warn!("GC 收尾空目录移除失败 tbl={tbl} err={err}");
    }
    Ok(removed)
  }
}
