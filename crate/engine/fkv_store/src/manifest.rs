//! manifest 读写、attach / reopen 与 replay 状态安装。

use std::cell::RefCell;

use fkv_codec::{append_varint32, append_varint64};
use fkv_compress::DictCompression;
use fkv_error::{KvError, Result};
use fkv_io::{ManifestIo, PageIo};
use fkv_manifest::{BranchManifestMetadata, ManifestBuilder};
use fkv_mapping::{AllocMode, BranchPageOwner, PageMapper};
use fkv_task_read::YieldBudget;
use fkv_types::{
  BranchFileRange, FileId, FilePageId, InstallOutcome, MAIN_BRANCH_NAME, NULL_PAGE, PageId,
  TableIdent, manifest_file_name, normalize_branch_name,
};

use crate::{BRANCH_FILE_RESERVE_FILE_NUM, Store};

/// 跳到下一个文件边界的首个页（对齐 C++ `Replayer::GetMapper` 的 term bump）。
fn next_file_boundary(max_fp_id: FilePageId, shift: u8) -> FilePageId {
  ((max_fp_id >> shift) + 1) << shift
}

/// 把「第一个未承诺的 file id」向上取整到 branch 预留块边界。
///
/// 块位分配（`create_branch`）、块基址派生（[`branch_block_base`]）与复用
/// 下界候选共用同一对齐规则，避免 archive / manifest / reuse_floor 各写一份。
pub(crate) fn align_branch_block(next_file_id: FileId) -> Option<FileId> {
  next_file_id
    .div_ceil(BRANCH_FILE_RESERVE_FILE_NUM)
    .checked_mul(BRANCH_FILE_RESERVE_FILE_NUM)
}

/// 高水位的**下一次分配**所在文件之后的下一个文件 id。
///
/// `max_fp_id` 是下一次分配的页 id，不是「最后一个已写页 + 1」：main 在
/// `create_branch` 里 `reserve_until` 之后、或恰好写满一个文件后，该页所在
/// 文件可能还没有任何页落盘，但下一次分配一定从它开始（sem-25）。按
/// `max_fp_id >> shift` 取文件 id 会把「祖先下一次写入的块」让给新分支，
/// 新分支的首批写立即覆盖祖先；必须再跳过这个文件。向上对齐到块边界后，
/// 高水位所在块整块留给该 manifest 的持有者继续写。
///
/// 高水位为 0 时取 1：区间链以 `max_file_id` 为**闭区间**上界，父区间至少占
/// 到 file 0 才不会派生出 file 0 的块基址。
pub(crate) fn next_file_after_hwm(max_fp_id: FilePageId, shift: u8) -> FileId {
  (max_fp_id >> shift).saturating_add(1)
}

/// 本 branch 预留块基址（file id）：`file_range_li` 中**首个**本 branch 区间
/// 的起点向上取整到 `BRANCH_FILE_RESERVE_FILE_NUM` 边界。
///
/// branch manifest 的区间链以父分支区间为前缀、本 branch 区间结尾；首个本
/// branch 区间的起点 = 父分支创建该 branch 时的高水位 + 1，创建后不再变化。
/// `max_fp_id` 是移动的分配水位：分支恰好写满本块时它等于块末尾，用
/// `max_fp_id >> shift` 反推会把上限多扩一块（pooled-09）。
///
/// `segment` 选择区间链的 data / segment 水位。区间链缺失或本 branch 是首段
/// （legacy manifest，无父前缀）时返回 `None`，由调用方回退旧口径并告警。
pub(crate) fn branch_block_base(
  metadata: &BranchManifestMetadata,
  branch: &str,
  segment: bool,
) -> Option<FileId> {
  let own_idx = metadata
    .file_range_li
    .iter()
    .position(|range| range.branch_name.as_ref() == branch)?;
  let parent = metadata.file_range_li.get(own_idx.checked_sub(1)?)?;
  let parent_max = if segment {
    parent.max_segment_file_id
  } else {
    parent.max_file_id
  };
  align_branch_block(parent_max.checked_add(1)?)
}

/// 本 branch 预留块末尾（file id，半开）：`block_base + RESERVE`。
///
/// 块末尾就是该分支对整个表承诺的分配区间上界；全局块位分配必须从所有
/// manifest 的块末尾之后继续（pooled-10）。按块末尾而不是 `max_fp_id` 计算，
/// 分支恰好写满 / 尚未开始写都不会漂移。
pub(crate) fn branch_block_end(
  metadata: &BranchManifestMetadata,
  branch: &str,
  segment: bool,
) -> Option<FileId> {
  branch_block_base(metadata, branch, segment)?.checked_add(BRANCH_FILE_RESERVE_FILE_NUM)
}

/// manifest 在 data 侧已承诺的文件上界（file id，半开）。
///
/// - branch manifest（本分支在区间链中有父前缀）：本分支预留块末尾；
/// - main / legacy manifest：高水位**下一次分配**所在文件的下一块末尾
///   （[`next_file_after_hwm`] 再向上对齐）。main 的分配水位可能已 reserve 到
///   子块末尾、也可能在 fork 后继续写过但 manifest 尚未刷新；无论哪种，下一次
///   分配都落在 `max_fp_id >> shift` 文件，这个块必须整块留给 main（sem-25）。
pub(crate) fn manifest_data_ceiling(
  metadata: &BranchManifestMetadata,
  max_fp_id: FilePageId,
  shift: u8,
) -> Option<FileId> {
  branch_block_end(metadata, metadata.branch_name.as_ref(), false)
    .or_else(|| align_branch_block(next_file_after_hwm(max_fp_id, shift)))
}

/// manifest 在 segment 侧已承诺的文件上界；没有段体系时返回 `None`。
///
/// 段块与 data 块使用独立的 file id 空间与 `segment_shift`，但块位规则相同。
pub(crate) fn manifest_segment_ceiling(
  metadata: &BranchManifestMetadata,
  max_segment_fp_id: FilePageId,
  segment_shift: u8,
) -> Option<FileId> {
  let branch = metadata.branch_name.as_ref();
  let own_has_segment = metadata
    .file_range_li
    .iter()
    .find(|range| range.branch_name.as_ref() == branch)
    .is_some_and(|range| range.max_segment_file_id > 0);
  if !own_has_segment && max_segment_fp_id == 0 {
    return None;
  }
  if own_has_segment && let Some(end) = branch_block_end(metadata, branch, true) {
    return Some(end);
  }
  align_branch_block(next_file_after_hwm(max_segment_fp_id, segment_shift))
}

/// branch 预留块上限（页 id 半开区间上界）：`(block_base + RESERVE) << shift`。
///
/// 优先从区间链推导块基址；legacy manifest 推导不出时回退到按当前
/// `max_fp_id` 反推（旧口径，可能漂移）并告警。`shift` 对 data / segment
/// 分别取 `pages_per_file_shift` / `segment_shift`。
fn branch_block_limit(
  metadata: &BranchManifestMetadata,
  branch: &str,
  max_fp_id: FilePageId,
  shift: u8,
  segment: bool,
) -> Result<FilePageId> {
  let base_file = match branch_block_base(metadata, branch, segment) {
    Some(base) => base,
    None => {
      log::warn!(
        "branch manifest 缺少可推导的区间链，上限按高水位反推 branch={branch} \
         max_fp_id={max_fp_id} segment={segment}"
      );
      (max_fp_id >> shift)
        .div_euclid(BRANCH_FILE_RESERVE_FILE_NUM)
        .checked_mul(BRANCH_FILE_RESERVE_FILE_NUM)
        .ok_or(KvError::InvalidArgs)?
    }
  };
  base_file
    .checked_add(BRANCH_FILE_RESERVE_FILE_NUM)
    .and_then(|limit_file| limit_file.checked_shl(u32::from(shift)))
    .ok_or(KvError::InvalidArgs)
}

/// 当前 `(branch, term)` 在 `file_range_li` 区间链中的首个文件起点（页 id）。
///
/// 区间链按 `max_file_id` 单调：第 i 段覆盖 `(prev_max, max_i]`，首段从 file 0
/// 起。同一 branch 的旧 term 区间低于当前 term 首个区间的起点；这些文件按旧
/// `(branch, term)` 命名，Pooled 复用其中的空洞会在同一 file id 下造出「新 term
/// 文件 + 旧 term 文件」，读路径当前 identity 优先会遮蔽旧 term 活页（io-15）。
/// 返回 `None` 表示链中没有当前 `(branch, term)` 区间（term 刚 bump、尚未写入）。
fn term_range_start(
  range_li: &[BranchFileRange],
  branch: &str,
  term: u64,
  shift: u8,
) -> Option<FilePageId> {
  let mut prev_max = None;
  let mut start = None;
  for range in range_li {
    let range_start = prev_max.map_or(0, |max: FileId| max.saturating_add(1));
    prev_max = Some(range.max_file_id);
    if range.branch_name.as_ref() == branch && range.term == term {
      // 同一 term 可能出现多段（term 反复 attach）：取最后一段的起点，覆盖集合更大。
      if let Some(fp) = range_start.checked_shl(u32::from(shift)) {
        start = Some(fp);
      }
    }
  }
  start
}

impl<I: PageIo + ManifestIo> Store<I> {
  pub(crate) async fn append_mapping_log(
    &mut self,
    tbl: &TableIdent,
    root_id: PageId,
    ttl_root_id: PageId,
  ) -> Result<()> {
    self.refresh_branch_metadata(tbl);
    // 脏页可能覆盖整表（首次写 / truncate 后），manifest 编码是纯 CPU 段，
    // 按固定步长检查段预算，避免一次写请求长时间独占 shard。
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    let record = {
      let state = self.table_map.get(tbl).ok_or(KvError::ResourceMissing)?;
      let mapper = state.mapper.as_ref().ok_or(KvError::ResourceMissing)?;
      let mut builder = ManifestBuilder::new();
      // 脏页记录自带操作时的 log 值（`FilePageId` 或 `INVALID_VALUE`），不能在
      // 此重读 `tbl()` 的最终槽值：同批 free → realloc 的释放项会被写成新的
      // `FilePageId`，与 C++ `DeleteMapping + UpdateMapping` 的记录序列不符
      // （sem-45）。
      for (seq, &(page_id, log_value)) in mapper.dirty_page_li().iter().enumerate() {
        yield_budget.maybe_yield_every(seq).await;
        builder.update_mapping(page_id, log_value);
      }
      builder.append_branch_metadata(
        state
          .branch_metadata
          .as_ref()
          .ok_or(KvError::ResourceMissing)?,
      )?;
      let segment_entries = if let Some(segment_mapper) = state.segment_mapper.as_ref() {
        let mut bytes = Vec::new();
        for (seq, &(page_id, log_value)) in segment_mapper.dirty_page_li().iter().enumerate() {
          yield_budget.maybe_yield_every(seq).await;
          append_varint32(&mut bytes, page_id);
          append_varint64(&mut bytes, log_value);
        }
        bytes
      } else {
        Vec::new()
      };
      let mut segment_delta = Vec::with_capacity(4 + segment_entries.len());
      segment_delta.extend_from_slice(&(segment_entries.len() as u32).to_le_bytes());
      segment_delta.extend_from_slice(&segment_entries);
      builder.append_segment_mapping(&segment_delta);
      builder.finalize(root_id, ttl_root_id)?
    };
    let (current_size, dict_dirty) = self.table_map.get(tbl).map_or((0, false), |state| {
      (
        state.manifest_size,
        state
          .compression
          .as_ref()
          .is_some_and(DictCompression::dirty),
      )
    });
    // 字典刚训练出来时必须写 snapshot 持久化 dict_bytes，
    // 否则日志增量不含字典，reopen 后无法解压 Dictionary 页（对齐 C++ `dict_dirty`）。
    //
    // 追加条件是「当前大小 + 本批 record 不超上限」（l2-06）。`manifest_limit == 0`
    // 时右式恒真 ⇒ **恒写整份 snapshot**：C++ 同配置下 `manifest_size > 0` 使
    // `manifest_size + log_physical_size <= manifest_limit` 恒假，同样落 snapshot
    // 分支（`src/tasks/write_task.cpp:807`）。默认 8 MiB 时本式等价于旧的
    // `manifest_limit != 0 && ... >`（左项恒真），盘面字节不变。
    if dict_dirty || current_size.saturating_add(record.len()) > self.manifest_limit {
      let snapshot = self.snapshot_record(tbl, root_id, ttl_root_id, None, None, None)?;
      self.write_manifest_record(tbl, &snapshot).await?;
      if let Some(state) = self.table_map.get_mut(tbl) {
        state.manifest_size = snapshot.len();
      }
      self.clear_dict_dirty(tbl);
    } else {
      self.append_manifest_record(tbl, &record).await?;
      if let Some(state) = self.table_map.get_mut(tbl) {
        state.manifest_size = current_size.saturating_add(record.len());
      }
    }
    // 只有 manifest 成功落盘后才清脏页；失败时保留供上层重试。
    if let Some(state) = self.table_map.get_mut(tbl) {
      if let Some(mapper) = state.mapper.as_mut() {
        mapper.clear_dirty_page_li();
      }
      if let Some(segment_mapper) = state.segment_mapper.as_mut() {
        segment_mapper.clear_dirty_page_li();
      }
    }
    Ok(())
  }

  /// 切换到一个 branch manifest 并打开独立 Store 状态。
  /// Attach 前重置 Store：清空所有 table 的缓存状态，并设置 branch manifest 模式。
  pub fn prepare_attach(&mut self, branch_name: &str, term: u64) -> Result<()> {
    let normalized = normalize_branch_name(branch_name).ok_or(KvError::InvalidArgs)?;
    self.reset_table_li();
    self.adopt_attach(normalized.as_ref(), term)?;
    self.io.set_file_identity(&self.branch_name, term);
    Ok(())
  }

  /// 新分区 Store 采纳 shard 当前的 attach 身份。
  ///
  /// 与 [`Self::prepare_attach`] 的区别：不清理共享 IO 的路径 / fd 缓存。
  /// shard 的每分区 Store 共用同一后端，attach 时已由首个 Store 统一失效过；
  /// 每个新分区再清一次会让其它分区的缓存反复失效。字段语义与
  /// `prepare_attach` 保持一致（main 无具名 manifest，branch 用
  /// `manifest_<branch>_<attach term>`——对齐 C++ attach 后全部读写换到
  /// `(GetActiveBranch(), ProcessTerm())` 名下，sem-60；Local 模式 term 恒 0，
  /// 名字自然保持 `manifest_<branch>_0`）。
  pub fn adopt_attach(&mut self, branch_name: &str, term: u64) -> Result<()> {
    let normalized = normalize_branch_name(branch_name).ok_or(KvError::InvalidArgs)?;
    if normalized.as_ref() == MAIN_BRANCH_NAME {
      self.branch_name = MAIN_BRANCH_NAME.to_owned();
      self.manifest_file = None;
    } else {
      self.manifest_file = Some(manifest_file_name(&normalized, term));
      self.branch_name = normalized.into_string();
    }
    self.branch_file_limit = None;
    self.branch_segment_limit = None;
    self.term = term;
    Ok(())
  }

  fn reset_table_li(&mut self) {
    self.table_map.clear();
    self.root_meta_used = 0;
    self.table_order_li.clear();
    self.touch_seq.clear();
    self.touch_clock = 0;
    self.io.invalidate_path_cache();
  }

  /// 安装 branch manifest 时派生 data / segment 写入上限（pooled-09）。
  ///
  /// `adopt_attach` 会先把两条上限清成 `None`，而 `write.rs` / `put_large`
  /// 的越界检查只在上限存在时生效。因此**所有**安装 branch manifest 的入口
  /// ——`open_branch`、`reopen`（具名 branch manifest）与 `reopen_archive`
  /// （含 shard 的 `SKIPS_OPEN` 路径）——都必须在 `install_replay_state`
  /// 之前调用本方法，否则 branch 可写穿自己的 1024 文件预留块。
  ///
  /// 块基址从 manifest 的 `file_range_li` 推导（见 [`branch_block_base`]），
  /// 不用移动的 `max_fp_id` 反推：分支恰好写满本块是合法终态，按高水位反推
  /// 会多扩一块。main 无预留块，两条上限保持 `None`。
  pub(crate) fn apply_branch_limits(
    &mut self,
    metadata: &BranchManifestMetadata,
    max_fp_id: FilePageId,
    max_segment_fp_id: FilePageId,
  ) -> Result<()> {
    if self.branch_name == MAIN_BRANCH_NAME {
      self.branch_file_limit = None;
      self.branch_segment_limit = None;
      return Ok(());
    }
    let shift = self.pages_per_file_shift;
    let segment_shift = self.segment_shift();
    let branch = self.branch_name.as_str();
    let file_limit = branch_block_limit(metadata, branch, max_fp_id, shift, false)?;
    // 没有 segment 高水位 = 该 manifest 没有 segment 体系（plain store）；
    // 与旧口径一致，不凭空设置段上限。
    let segment_limit = if max_segment_fp_id > 0 {
      Some(branch_block_limit(
        metadata,
        branch,
        max_segment_fp_id,
        segment_shift,
        true,
      )?)
    } else {
      None
    };
    self.branch_file_limit = Some(file_limit);
    self.branch_segment_limit = segment_limit;
    Ok(())
  }

  /// 打开具名 branch manifest；`term` 是 attach 后的活跃 term。
  ///
  /// manifest 名带 attach term（`manifest_<branch>_<term>`，sem-60，对齐 C++
  /// attach 后按 `(GetActiveBranch(), ProcessTerm())` 命名）：`create_branch`
  /// 落盘的是 `_0`，attach term T 打开时先试 `_T`，未命中按「同 branch 最高
  /// 无 tag term」回退选中 `_0` 等旧对象（`read_branch_manifest`，含 ExpiredTerm
  /// 栅栏），首写再按 `_T` 名发布（写时晋升）。attach term 只决定 manifest 名、
  /// 后续写对象与 `file_range_li` 区间归属。必须在 `install_replay_state` 之前
  /// 采纳 attach term：install 依据 term 变化抬升文件边界，并把新区间记到当前
  /// identity；若先用 0 回放，term bump 永不触发，区间永远记 0，远端读 `_0`
  /// 找不到 attach 后写出的 `_T` 对象。
  pub async fn open_branch(
    &mut self,
    tbl: &TableIdent,
    branch_name: &str,
    term: u64,
  ) -> Result<()> {
    let normalized = normalize_branch_name(branch_name).ok_or(KvError::InvalidArgs)?;
    let manifest_file = manifest_file_name(&normalized, term);
    let bytes = self
      .io
      .read_branch_manifest(tbl, normalized.as_ref(), term)
      .await?
      .ok_or(KvError::ResourceMissing)?;
    // branch manifest 同样以逻辑 replay size 为追加基准：撕裂 / 损坏尾部不计入，
    // 否则下一次写会落在垃圾之后，reopen 见「损坏 + 完好」直接 Corrupted。
    let (state, size) = fkv_task_reopen::replay_manifest_bytes_with_len(&bytes)?;
    self.branch_name = normalized.clone().into_string();
    self.term = term;
    self.io.set_file_identity(normalized.as_ref(), term);
    self.manifest_file = Some(manifest_file);
    self.apply_branch_limits(
      &state.branch_metadata,
      state.max_fp_id,
      state.max_segment_fp_id,
    )?;
    self.install_replay_state(tbl, state, size)?;
    // branch / archive 快照引用的旧物理页不在读快照水位内：按 manifest 重建
    // Pooled 复用下界，之后退役的继承页才不会被回池复用（pooled-07）。
    self.rebuild_reuse_floor(tbl).await;
    let name = self
      .manifest_file
      .as_deref()
      .ok_or(KvError::ResourceMissing)?;
    // 同活跃 manifest：追加基准取 `valid_len`，截断撕裂尾后再 append；
    // 不采用 C++ padding 缺失分支「file_size_ 越过被丢弃 record」的口径（sem-14）。
    self.io.reset_manifest_file(tbl, name, size as u64).await?;
    Ok(())
  }

  /// 写整份 snapshot：branch 模式写具名文件，main 模式 switch 活跃 manifest。
  pub(crate) async fn write_manifest_record(
    &mut self,
    tbl: &TableIdent,
    record: &[u8],
  ) -> Result<()> {
    match self.manifest_file.clone() {
      Some(name) => self.io.write_manifest_file(tbl, &name, record).await,
      None => self.io.switch_manifest(tbl, record).await,
    }
  }

  /// 追加 log record：branch 模式追加到具名文件，main 模式追加活跃 manifest。
  async fn append_manifest_record(&mut self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    match self.manifest_file.clone() {
      Some(name) => self.io.append_manifest_file(tbl, &name, record).await,
      None => self.io.append_manifest(tbl, record).await,
    }
  }

  pub(crate) async fn write_snapshot_manifest(
    &mut self,
    tbl: &TableIdent,
    root_id: PageId,
    ttl_root_id: PageId,
  ) -> Result<()> {
    self.refresh_branch_metadata(tbl);
    let record = self.snapshot_record(tbl, root_id, ttl_root_id, None, None, None)?;
    self.write_manifest_record(tbl, &record).await?;
    if let Some(state) = self.table_map.get_mut(tbl) {
      state.manifest_size = record.len();
      state.manifest_started = true;
      if let Some(mapper) = state.mapper.as_mut() {
        mapper.clear_dirty_page_li();
      }
      if let Some(segment_mapper) = state.segment_mapper.as_mut() {
        segment_mapper.clear_dirty_page_li();
      }
    }
    self.clear_dict_dirty(tbl);
    Ok(())
  }

  /// 读取并安装本地 manifest；失败时清空该表内存状态，避免后续写把 log
  /// record 写成 manifest 第一条（远端 manifest 缺失 / 损坏后重试路径）。
  pub async fn reopen(&mut self, tbl: &TableIdent) -> Result<()> {
    self.reopen_with(tbl, false).await
  }

  /// 只回放本地缓存的 active manifest；失败同样清空该表内存状态。
  ///
  /// 只给 [`Self::install_external_snapshot`] 用：`refresh_manifest` 已经把外部
  /// 快照写进本地缓存，普通 [`Self::reopen`] 会再次回源选择远端最新 term，
  /// 在本 shard term 较低时触发 `GetManifest` 侧的采纳栅栏（sem-04）。C++
  /// `RefreshManifest` 路径明确不做该栅栏，replay 的是本轮下载的内容。
  async fn reopen_local(&mut self, tbl: &TableIdent) -> Result<()> {
    self.reopen_with(tbl, true).await
  }

  async fn reopen_with(&mut self, tbl: &TableIdent, local_only: bool) -> Result<()> {
    let result = self.reopen_inner(tbl, local_only).await;
    if result.is_err() {
      self.evict_table(tbl);
    }
    result
  }

  /// `local_only`：main branch 只回放本地 cache，不按远端最新 term 重选。
  /// 具名 branch manifest 两种模式都按 `(branch, term)` 名 + 回退链读取
  /// （`read_branch_manifest`）——attach 身份已定，名字与 `open_branch` 一致。
  async fn reopen_inner(&mut self, tbl: &TableIdent, local_only: bool) -> Result<()> {
    let (state, size) = if self.manifest_file.is_some() {
      let bytes = self
        .io
        .read_branch_manifest(tbl, &self.branch_name, self.term)
        .await?
        .ok_or(KvError::ResourceMissing)?;
      fkv_task_reopen::replay_manifest_bytes_with_len(&bytes)?
    } else if local_only {
      fkv_task_reopen::reopen_local_with_size(&self.io, tbl).await?
    } else {
      fkv_task_reopen::reopen_with_size(&self.io, tbl).await?
    };
    // 具名 branch manifest：与 `open_branch` 同口径恢复预留上限。公开 Store
    // API（`prepare_attach` + `reopen`）与 shard 的 branch reopen / 外部快照
    // 安装都从这里装状态，缺上限会让 `write.rs` 的越界检查整段跳过（pooled-09）。
    if self.manifest_file.is_some() {
      self.apply_branch_limits(
        &state.branch_metadata,
        state.max_fp_id,
        state.max_segment_fp_id,
      )?;
    }
    self.install_replay_state(tbl, state, size)?;
    // Pooled：分支 / 归档快照的持久页引用下界（pooled-07）。main 的活跃
    // manifest 无快照时只是一次目录列举，开销可忽略。
    self.rebuild_reuse_floor(tbl).await;
    // 追加基准取 replay 的 `valid_len`（最后一条完整 record 的对齐末尾），
    // 不是 C++ 的 `file_size_`：C++ 在「body 完好 + padding 缺失」时把
    // `file_size_` 推进到被丢弃 record 的对齐末尾，下一次 append 在 EOF 后
    // 留零洞、再 reopen 会复活该 record（`eloqstore/src/replayer.cpp:124-131`、
    // `tests/manifest.cpp:632-643`）。Rust 有意截断到 `valid_len`，下一次 append
    // 覆盖撕裂 record，保持恢复幂等（sem-14，见 `fkv_task_reopen` 模块文档）。
    match self.manifest_file.clone() {
      Some(name) => self.io.reset_manifest_file(tbl, &name, size as u64).await?,
      None => self.io.reset_manifest(tbl, size as u64).await?,
    }
    self.touch_table(tbl);
    Ok(())
  }

  /// 采纳外部（远端）当前快照：强制刷新 manifest → replay → 原子安装。
  ///
  /// 对齐 C++ `PageManager::InstallExternalSnapshot`：
  /// - 远端有 manifest：`refresh_manifest` 把快照写入本地缓存后，用
  ///   [`Self::reopen_local`] 回放刚写入的内容（对齐 C++ `RefreshManifest`：
  ///   该路径无 `ExpiredTerm` 栅栏，低 term follower / standby 采纳更高 term
  ///   新 owner 的快照是合法交接；不能再走会回源重选的 [`Self::reopen`]）；
  /// - 远端无 manifest：只删**本地** manifest 并清空内存表状态，返回
  ///   [`InstallOutcome::WaitingRemote`]，绝不以本地空状态起跑、绝不删远端对象。
  pub async fn install_external_snapshot(&mut self, tbl: &TableIdent) -> Result<InstallOutcome> {
    match self.io.refresh_manifest(tbl).await {
      Ok(()) => {}
      Err(KvError::NotFound | KvError::ResourceMissing) => {
        return self.clear_local_snapshot(tbl).await;
      }
      Err(err) => return Err(err),
    }
    match self.reopen_local(tbl).await {
      Ok(()) => Ok(InstallOutcome::Installed),
      // refresh 与 replay 之间本地 manifest 消失：同样只清本地。
      Err(KvError::ResourceMissing) => self.clear_local_snapshot(tbl).await,
      Err(err) => Err(err),
    }
  }

  /// 清空本地 manifest 与内存表状态，等待远端状态重新出现。
  async fn clear_local_snapshot(&mut self, tbl: &TableIdent) -> Result<InstallOutcome> {
    self.io.drop_local_manifest(tbl).await?;
    self.evict_table(tbl);
    Ok(InstallOutcome::WaitingRemote)
  }

  pub(crate) fn install_replay_state(
    &mut self,
    tbl: &TableIdent,
    replay: fkv_recovery::ReplayState,
    size: usize,
  ) -> Result<()> {
    // term 变化 = 新文件边界起步（对齐 C++ `Replayer::GetMapper`）：旧 term 的页
    // 按 (branch, term) 寻址，新数据不得复用同一 file id 的旧对象 key 空间。
    //
    // 条件等价性（l2-05a）：C++ 判据是 `DeriveStoreMode(*opts_) != Local &&
    // expect_term != 0 && manifest_term != expect_term`
    // （`eloqstore/src/replayer.cpp:356-357`），但 Local 下 shard term 被强制 0
    // （`eloqstore/src/eloq_store.cpp:930-932`），且 `options_` 构造后不可变 ⇒
    // `expect_term != 0` 单独已蕴含「非 Local」，`self.term != 0` 就是它的 fkv
    // 对应项；`expect_term` 的来源有且只有 ProcessTerm()（各调用点
    // `page_manager.cpp:185,192,605,629`），即宿主 attach 的 term。
    // 真正的差异在 **Local 是否采纳宿主 term**：C++ 丢弃（恒 0），fkv 采纳
    // （CLI `--term` 默认 1 且拒绝 0），因此 Local 盘面文件名带 `_<term>`、
    // 跨 term 重开也 bump。见 COMPAT_BASELINE「term bump 条件与 Local 模式
    // term 口径」条：不得为「对齐 C++」删掉 `self.term != 0`，也不得在 Local
    // 下绕开 bump（会与 fkv 自己的 `(branch, term)` 命名 + 复用下界冲突）。
    let bump = self.term != 0 && replay.branch_metadata.term != self.term;
    // term bump 若会撞到 branch 预留区间上限，就保留当前高水位：文件名带
    // term，不会与旧 term 对象冲突；否则最后一次 bump 会占满区间（水位正好
    // 落在 `limit` 上，`write.rs` 的 `max_fp_id >= limit` 前置检查即拒写），
    // 后续写永久 OutOfSpace。C++ `CreateBranch` 不预留块、replayer 也无上限
    // 概念，故这是 **Rust 单侧加固**（l2-05a，COMPAT_BASELINE 已登记），
    // 回归用例 `fkv_store/tests/reopen_watermark.rs` 的
    // `term_bump_at_branch_limit_keeps_high_water`。
    let bumped_fp_id = next_file_boundary(replay.max_fp_id, self.pages_per_file_shift);
    let max_fp_id = if bump
      && self
        .branch_file_limit
        .is_none_or(|limit| bumped_fp_id < limit)
    {
      bumped_fp_id
    } else {
      replay.max_fp_id
    };
    let bumped_segment_fp_id = next_file_boundary(replay.max_segment_fp_id, self.segment_shift());
    let max_segment_fp_id = if bump
      && self
        .branch_segment_limit
        .is_none_or(|limit| bumped_segment_fp_id < limit)
    {
      bumped_segment_fp_id
    } else {
      replay.max_segment_fp_id
    };
    // 重建 mapper 继续消费本 Store 的代际来源：与旧化身 / 其它表共用序列，
    // 旧快照的代际必然早于新化身，缓存 baseline 不会与在途快照重叠（perf-06）。
    let generation_source = self.generation_source.clone();
    let snapshot_source = self.snapshots.generation_source();
    let alloc_mode = self.alloc_mode;
    // 空闲池回填只收当前 manifest branch 名下的页（C++ `FileIdInBranch`）：
    // 映射空洞可能是父 / 兄弟 branch 的活空闲页，复用会按 file id 写到对方的
    // (branch, term) 命名空间，静默覆盖其数据。用 manifest 自带 branch 名，
    // 与 C++ `branch_metadata_.branch_name` 同口径；区间链为空时回退全量回填。
    let data_owner = BranchPageOwner {
      branch_name: replay.branch_metadata.branch_name.as_ref(),
      range_li: &replay.branch_metadata.file_range_li,
      segment: false,
      shift: self.pages_per_file_shift,
    };
    let mut mapper = PageMapper::with_tbl_and_source_in_branch(
      replay.mapping_tbl,
      generation_source.clone(),
      alloc_mode,
      max_fp_id,
      data_owner,
    );
    mapper.set_snapshot_generation_source(snapshot_source.clone());
    // Pooled 跨 term 复用下界（io-15）：旧 term 文件按旧 `(branch, term)` 命名，
    // 复用其中的空洞会在同一 file id 下造出「新 term 文件 + 旧 term 文件」：
    // 读路径当前 identity 优先，`skip_verify_checksum` 会把新文件的整页零洞判成
    // 命中、不再回退旧 term 活页；Cloud 的写键（当前 identity）与读键（区间链
    // 解析的旧 identity）也分叉。下界取「当前 term 区间起点」：
    // - term 刚 bump（manifest 里还没有当前 term 区间）：下界 = 新文件边界起点；
    // - 同 term 重开（链里已有当前 term 区间）：下界 = 该区间起点，旧 term
    //   区间整段不可复用，当前 term 的区间仍按 Pooled 复用。legacy 空链在
    //   bump 时补一条合成旧 term 区间（见下），保证这里同样取到 bump 边界。
    // 与 pooled-07 的 sticky 下界同机制：只升不降，旧页永不回池。
    let term_floor = if bump {
      max_fp_id
    } else {
      term_range_start(
        &replay.branch_metadata.file_range_li,
        &self.branch_name,
        self.term,
        self.pages_per_file_shift,
      )
      .unwrap_or(0)
    };
    mapper.raise_reuse_floor(term_floor);
    // 由活跃映射反推 `min_file_id` / `empty_file_cnt`，对齐 C++ `Replayer::GetMapper`：
    // 否则 `space_size` 在 reopen 后按 0 起算，`needs_maintain` 口径失真。
    mapper.update_stat_by_scan(self.pages_per_file_shift);
    // 旧缓存能否复用（eng-67）：只有同一 (branch, term) 才可能是同一物理页体系；
    // 跨分支 / 跨 term 的 file id 空间不同，且 term 变化本身已按新文件边界起步。
    let same_identity = !bump
      && self.table_map.get(tbl).is_some_and(|state| {
        state.cache_branch.as_deref() == Some(self.branch_name.as_str())
          && state.cache_term == self.term
      });
    // 恢复字典：即便当前未启用压缩，也要能读回旧盘面里字典压缩的页。
    // 先于任何状态改写执行：字典损坏必须 fail-fast，且不留下半套新状态。
    let dict = if replay.dict_bytes.is_empty() {
      None
    } else {
      let level = self.compression_level.unwrap_or_default();
      let mut dict = DictCompression::new(level);
      // 字典损坏必须 fail-fast：静默丢字典会让后续 snapshot 写出无字典 manifest，
      // 旧压缩页永久不可解压。
      dict
        .load_dictionary(replay.dict_bytes.clone())
        .map_err(|_| KvError::Corrupted)?;
      Some(dict)
    };
    // 写路径页缓存不跨 mapper 化身复用：重建后的分配器空闲池按 `reuse_floor`
    // 与快照水位重算，旧条目的物理页可能已不属于活跃映射。重建代价只是第一批
    // 按未命中回填；与 index 缓存逐条校验（`retain_mapped`）的复用策略不同，
    // 这里选整体丢弃，不为此再维护一套「物理页仍有效」的判据。
    self.clear_page_cache(tbl);
    let old_cache = self
      .table_map
      .get_mut(tbl)
      .and_then(|state| state.index_cache.take());
    let mut cache = match old_cache {
      Some(old) if same_identity => old.into_inner(),
      _ => self.new_index_cache(),
    };
    if same_identity {
      // 逐条按新映射校验：逻辑页仍指向条目记录的物理页才保留，等价 C++ 的
      // swizzle 复用。append-only 分配下物理页内容写后不变，因此物理页相同
      // 即缓存页仍有效；映射缺失 / 换页 / 非 baseline 代条目一律丢弃。
      cache.retain_mapped(|page_id| mapper.file_page_id(page_id).ok());
    }
    // 重建后的 mapper 取全新代际（本 Store 内单调，不复用旧值）：baseline 一律
    // 对齐到该代。复用路径靠它让保留条目继续命中（27b）；新建路径靠它挡住
    // 旧化身在途快照的回填（baseline 留在 0 等于接受所有代际，r7a-core-02）。
    cache.retag(mapper.generation());
    let branch = self.branch_name.clone();
    let term = self.term;
    let mut metadata = replay.branch_metadata;
    // legacy 空区间链的 term bump（pooled-14）：链里没有旧 term 区间，bump 后的
    // 第一笔写由 `refresh_branch_metadata` 往空链追加 `(branch, new_term)` 区间，
    // 而区间链的隐式起点是 file 0 —— 旧 term 文件整段被标成新 term。同 term 重启
    // 后 `term_range_start` 返回 0，io-15 的复用下界失效，旧 term 文件的空洞重新
    // 进池。这里在覆盖 `metadata.term` 之前补一条合成旧 term 区间，使新 term 区间
    // 起点 = bump 边界（`refresh_branch_metadata` 追加的区间从下一段开始）。
    //
    // 只在空链上补：带父前缀的 legacy branch manifest 依赖「首个本 branch 区间」
    // 推导 `branch_block_base`，在链首插区间会把 parent 变成 None，改变预留上限
    // 口径；branch 名与 attach 身份不一致的 manifest 同样跳过（链首区间会被当成
    // 当前 branch 的父前缀）。data 侧 bump 撞 branch 上限而未抬升时不补：同一
    // file 内旧 / 新 term 页并存，file 粒度区间链无法表达（l2-05a 已登记的边界），
    // 保持原行为。segment 侧 bump 独立判定，被挡回时上界只到「最后一个完全属于
    // 旧 term 的文件」，共享段文件留给当前 identity（io-28 / io-29，见字段注释）。
    if bump
      && max_fp_id == bumped_fp_id
      && replay.max_fp_id > 0
      && metadata.file_range_li.is_empty()
      && metadata.branch_name.as_ref() == self.branch_name
    {
      metadata.file_range_li.push(BranchFileRange {
        branch_name: metadata.branch_name.clone(),
        term: metadata.term,
        // 旧 term 可能写到的最后一个文件（新 term 从下一文件边界起步）。
        max_file_id: (bumped_fp_id >> self.pages_per_file_shift) - 1,
        // segment 侧同理，但 data 与 segment 的 bump 独立判定（io-28 / io-29）：
        // - bump 真的抬升：旧 term 的最后一段仍在 `旧水位 >> shift` 文件内，
        //   而新 term 从下一文件边界起步，整个文件完全属于旧 term，按旧水位填；
        // - 被 branch 预留上限挡回：旧水位恰好是**新 term 下一次段分配所在的
        //   file id**（水位 = 下一次分配），该文件新旧段混存、无法表达
        //   （l2-05a）。只声称它之前完全属于旧 term 的文件——即
        //   `(旧水位 >> shift) - 1`；填 0 会把 file 1..水位文件-1 这些旧 term
        //   独占且可读的文件也让给当前 identity，读旧值 `ResourceMissing`
        //   （io-29）。共享文件留给当前 identity，新 term 段页照旧可读（io-28）；
        //   没有段体系（`max_segment_fp_id == 0`）时保持 0，避免把 plain store
        //   误判成有 segment 体系（`manifest_segment_ceiling` 的
        //   `own_has_segment`）。
        max_segment_file_id: if replay.max_segment_fp_id > 0 {
          if max_segment_fp_id == bumped_segment_fp_id {
            replay.max_segment_fp_id >> self.segment_shift()
          } else {
            (replay.max_segment_fp_id >> self.segment_shift()).saturating_sub(1)
          }
        } else {
          0
        },
      });
    }
    // 把父分支 / 旧 term 的 per-file 归属交给后端：远端对象 key 必须按写时解析。
    // 放在字典校验之后：字典损坏时 fail-fast，后端与内存状态都不留半套新值。
    // 传补过合成区间的链：Cloud 读旧 file id 才能解析到旧 term 对象（pooled-14）。
    self.io.set_file_range_li(tbl, &metadata.file_range_li);
    let state = self.table_state(tbl);
    state.mapper = Some(mapper);
    state.index_cache = Some(RefCell::new(cache));
    state.cache_branch = Some(branch.clone());
    state.cache_term = term;
    // 新状态整体替换：replay 没有字典就必须清空，不能沿用上一分支 / 归档的
    // 字典，否则新写入的页按未持久化字典压缩，重开无法解压。
    state.compression = dict;
    // segment 侧同理：空 mapping + 高水位 0 表示该状态没有 segment 体系，
    // 残留的旧 mapper 会让后续大 value 写入落到上一身份的分段空间。
    state.segment_mapper = (!replay.segment_mapping.is_empty() || replay.max_segment_fp_id > 0)
      .then(|| {
        // 段 mapper 恒 `Append`（pooled-02，对齐 C++ `EnsureSegmentMapper`）：
        // 与 data 侧的 `alloc_mode` 无关，reopen 也不重建空闲池。
        let mut mapper = PageMapper::with_tbl_and_source(
          replay.segment_mapping,
          generation_source,
          AllocMode::Append,
          max_segment_fp_id,
        );
        mapper.set_snapshot_generation_source(snapshot_source);
        mapper
      });
    // 活跃身份以当前安装上下文为准：term 不同步会让下次 reopen 每次判定
    // 「term 变化」而反复抬文件边界；branch 不同步会把新区间挂到旧分支。
    metadata.branch_name = branch.into();
    metadata.term = term;
    state.branch_metadata = Some(metadata);
    state.root = Some(replay.root_id);
    state.ttl_root = replay.ttl_root_id;
    state.manifest_started = true;
    state.manifest_size = size;
    // 整体替换状态：旧表残留的延迟 TTL 清理标记必须清掉，否则 pending_ttl_len
    // 虚高，下一轮 drain 会做无谓清理。
    state.ttl_pending = false;
    // 对齐 C++ `page_manager.cpp` reopen：TTL 树非空时 next_expire_ts=1，
    // 强制重启后的第一次写触发一次过期清理（而不是等新的 TTL 写入）。
    state.next_expire = u64::from(replay.ttl_root_id != NULL_PAGE);
    // 只读打开（`--read-only-existing`）的 store 从不走写路径，页缓存实例必须
    // 在这里建好，否则读路径只能直连 IO。旧实例已在上面整体清空，这里复用。
    self.ensure_page_cache(tbl);
    // 挂载的缓存改变了实例数：均分额度同步收缩，不能等下一次写才回收。
    self.rebalance_cache_budget();
    // open_branch 不走 `touch_table`：安装后立即补记账，避免 attach 表漏计预算。
    self.refresh_table_bytes(tbl);
    Ok(())
  }
}
