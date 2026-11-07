//! 归档 manifest 与 branch 生命周期：列举 / 创建 / 重开 / 删除 / drop。

use fkv_error::{KvError, Result};
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_manifest::BranchManifestMetadata;
use fkv_mapping::{AllocMode, PageMapper};
use fkv_task_read::YieldBudget;
use fkv_types::{
  BranchFileRange, DEFAULT_SEGMENTS_PER_FILE_SHIFT, FileId, MAIN_BRANCH_NAME, TableIdent,
  archive_file_name, is_valid_archive_tag, manifest_file_name, normalize_branch_name,
  parse_manifest_file_name,
};
use gxhash::HashSet;

use crate::{
  ArchiveInfo, ArchiveTagInfo, BRANCH_FILE_RESERVE_FILE_NUM, BranchInfo, Store,
  manifest::align_branch_block,
};

mod watermark;

use watermark::accumulate_manifest_ceiling;

impl<I: PageIo + ManifestIo> Store<I> {
  /// 列举该表目录下全部归档 manifest 的 `(tag, term)`，按 `(tag, term)` 升序去重。
  ///
  /// 语义对齐 C++ `HandleGlobalListArchiveTagsRequest`（`src/eloq_store.cpp:2133`）：
  /// - **不按 branch 过滤**：归档清单是表级 inventory，跨 branch 的同名 tag 也要
  ///   出现在结果里（按 `(tag, term)` 全等去重，与 C++ `sort` + `unique` 一致）；
  /// - 只解析文件名，不读 manifest 内容，也不需要表已打开；
  /// - `Cloud` 后端下 `list_manifest_file_name_li` 会合并本地 cache 与远端对象
  ///   （只列举 manifest，不下载 data / segment）。
  ///
  /// 与 `prune_archive_li` 的「按 tag 倒序保留最近 N 份」不同：保留策略只决定删
  /// 谁，控制面列举给的是完整清单，因此两者排序方向不共享。
  pub async fn list_archive_tags(&self, tbl: &TableIdent) -> Result<Vec<ArchiveTagInfo>> {
    let mut entry_li = Vec::new();
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    for name in self.io.list_manifest_file_name_li(tbl).await? {
      yield_budget.maybe_yield().await;
      let Some((_, term, Some(tag))) = parse_manifest_file_name(&name) else {
        continue;
      };
      entry_li.push(ArchiveTagInfo {
        tag: tag.to_owned(),
        term,
      });
    }
    entry_li.sort();
    entry_li.dedup();
    Ok(entry_li)
  }

  /// [`Self::list_archive_tags`] 的 `_li` 命名别名（fkv 内部 Vec 后缀习惯）。
  pub async fn list_archive_tag_li(&self, tbl: &TableIdent) -> Result<Vec<ArchiveTagInfo>> {
    self.list_archive_tags(tbl).await
  }

  /// 按 tag 重新打开归档快照。
  ///
  /// - `tag` 为空：等价 [`Self::reopen`]（C++ `ReopenRequest`：empty = latest
  ///   available snapshot），不做归档选择。
  /// - `tag` 非空：在当前分支里选该 tag 的归档 manifest，replay 后把该快照
  ///   **提升为活跃状态**（写回活跃 manifest），后续读写继续基于它。
  ///   多 term 时取最高 term —— C++ Cloud 用当前 process term 精确匹配
  ///   （`CloudStoreMgr::RefreshManifest`），但 fkv 的归档跨 term 保留且归档名
  ///   自带 term，取最高 term 可在重启 / term 推进后仍按 tag 恢复旧快照。
  ///
  /// `ResourceMissing` 表示该 tag 没有归档（或 manifest 已被并发删除），由
  /// shard 的 reopen 延迟队列决定是否重试。tag 只做「可落盘」校验
  /// （[`is_valid_archive_tag`]）：C++ 写下的中文 / `:` / 空格 / `_` tag 必须能
  /// 直接恢复，不能只因为 Rust 侧的 ASCII 白名单就报 `InvalidArgs`。
  pub async fn reopen_archive(&mut self, tbl: &TableIdent, tag: &str) -> Result<ArchiveInfo> {
    let branch = normalize_branch_name(&self.branch_name).ok_or(KvError::InvalidArgs)?;
    if tag.is_empty() {
      self.reopen(tbl).await?;
      let state = self.table_map.get(tbl).ok_or(KvError::ResourceMissing)?;
      let name = self
        .manifest_file
        .clone()
        .unwrap_or_else(|| manifest_file_name(MAIN_BRANCH_NAME, 0));
      return Ok(ArchiveInfo {
        name,
        bytes: state.manifest_size,
        removed_num: 0,
      });
    }
    if !is_valid_archive_tag(tag) {
      return Err(KvError::InvalidArgs);
    }
    let mut selected: Option<(u64, String)> = None;
    for name in self.io.list_manifest_file_name_li(tbl).await? {
      let Some((archive_branch, term, Some(archive_tag))) = parse_manifest_file_name(&name) else {
        continue;
      };
      if archive_branch != branch.as_ref() || archive_tag != tag {
        continue;
      }
      if selected
        .as_ref()
        .is_none_or(|(best_term, _)| term >= *best_term)
      {
        selected = Some((term, name));
      }
    }
    let Some((_, name)) = selected else {
      return Err(KvError::ResourceMissing);
    };
    let bytes = self
      .io
      .read_manifest_file(tbl, &name)
      .await?
      .ok_or(KvError::ResourceMissing)?;
    let (mut replay, size) = fkv_task_reopen::replay_manifest_bytes_with_len(&bytes)?;
    // segment 追加水位与 data 水位同口径：归档重开只回滚可见状态（mapping /
    // root），段水位必须留在活跃状态已写完的位置之后。段 mapper 恒 Append
    // （pooled-02），与 `alloc_mode` 无关，Pooled / Append 都要保留；否则后续
    // 大 value 从归档水位重新分配，覆盖更晚归档 / 子 / 孙分支 manifest 仍引用
    // 的段页（pooled-12；MemStore 按 `(tbl, fp)` 覆盖，Compio / Cloud 同
    // identity 段文件名覆盖，读回的是另一份合法字节，无错误码）。
    let archive_seg_max = replay.max_segment_fp_id;
    match self.active_segment_watermark(tbl).await? {
      Some(current_seg_max) => {
        if archive_seg_max < current_seg_max {
          log::warn!(
            "归档重开: 归档段水位低于活跃状态，保留活跃水位 tbl={tbl} tag={tag} \
             branch={branch} archive={archive_seg_max} active={current_seg_max}"
          );
        }
        replay.max_segment_fp_id = archive_seg_max.max(current_seg_max);
      }
      // 活跃状态不存在（drop 过活跃 manifest / branch manifest 已被
      // `delete_branch` 删除）：不能只看归档自报的 `max_segment_fp_id`——
      // 生产配置下每份 branch 归档都带「段块基址」预留水位，纯数据归档也是
      // 「空 mapping + 高水位 > 0」，直接拒绝会让 sem-28 的孤儿归档在默认
      // 配置下不可恢复（pooled-13）。改用存活 manifest 的段页证据重建保守
      // 水位：无证据时归档水位本身安全（块基址以下没有段页）；有本块内引用
      // 时抬到引用之上，既不覆盖更晚归档 / 子分支的段页，也不越过本分支
      // `branch_segment_limit`。
      None => {
        let evidence = self
          .cold_segment_watermark(tbl, &replay, branch.as_ref())
          .await?;
        if evidence > archive_seg_max {
          log::warn!(
            "归档重开: 冷态按存活 manifest 抬高归档段水位 tbl={tbl} tag={tag} \
             branch={branch} archive={archive_seg_max} evidence={evidence}"
          );
        }
        replay.max_segment_fp_id = archive_seg_max.max(evidence);
      }
    }
    // 归档 manifest 带 branch 区间链：安装前恢复本 branch 的预留上限。shard 的
    // `ReopenArchive` 是 `SKIPS_OPEN`（不会先走 `open_branch`），这里连同公开
    // `Store` API 是本路径唯一的上限来源；缺失时 `write.rs` 的越界检查整段
    // 跳过，branch 可写穿自己的预留块（pooled-09）。传保留后的段水位：归档
    // 没有段段而当前有段体系时，`max_segment_fp_id > 0` 才能恢复
    // `branch_segment_limit`（pooled-12）。
    self.apply_branch_limits(
      &replay.branch_metadata,
      replay.max_fp_id,
      replay.max_segment_fp_id,
    )?;
    // Append 的「旧页永不复用」不能因归档重开失效：归档只回滚可见状态
    // （mapping / root），追加水位必须留在盘上已写完的位置之后。否则后续写
    // 从归档水位 `A` 继续分配，直接覆盖 `[A, C)` 里仍被子孙 branch / archive
    // manifest 引用的继承页（pooled-11；MemStore 与同 identity 的 Compio 都是
    // 字节覆盖）。活跃 manifest 的水位是子分支创建后父分支继续写过的位置，
    // 恒不低于任何子孙的继承页。Pooled 由 `rebuild_reuse_floor` 的子孙实际
    // 引用页下界保护，这里只处理没有空闲池的 Append。
    if self.alloc_mode == AllocMode::Append {
      let current_max = match self
        .table_map
        .get(tbl)
        .and_then(|state| state.mapper.as_ref())
      {
        Some(mapper) => Some(mapper.max_fp_id()),
        // 刚 attach 的 Store 还没有表状态：从活跃 branch manifest 取水位。
        // 活跃名带 attach term（sem-60），走回退链才能读到首写发布前的旧 term
        // 采纳内容；main 的活跃 manifest 不在具名文件里，交给
        // `rebuild_reuse_floor` 的子孙块末尾 reserve（sem-25），这里不额外读取。
        None => {
          if self.manifest_file.is_some() {
            self
              .io
              .read_branch_manifest(tbl, &self.branch_name, self.term)
              .await?
              .map(|bytes| {
                fkv_task_reopen::replay_manifest_bytes_with_len(&bytes)
                  .map(|(replay, _)| replay.max_fp_id)
              })
              .transpose()?
          } else {
            None
          }
        }
      };
      match current_max {
        Some(current_max) => replay.max_fp_id = replay.max_fp_id.max(current_max),
        // 冷态（branch manifest 已被 `delete_branch` 删除）：与段侧同口径，
        // 从存活 manifest 重建 data 追加水位。否则回滚到更早归档后，新写入
        // 从归档水位覆盖更晚归档 / 子分支 manifest 仍引用的页——Append 没有
        // 空闲池，`rebuild_reuse_floor` 也不为受限分支建下界（pooled-13）。
        None => {
          let archive_max = replay.max_fp_id;
          if let Some(cold) = self
            .cold_data_watermark(tbl, &replay, branch.as_ref())
            .await?
          {
            if cold > archive_max {
              log::warn!(
                "归档重开: 冷态按存活 manifest 抬高归档 data 水位 tbl={tbl} tag={tag} \
                 branch={branch} archive={archive_max} evidence={cold}"
              );
            }
            replay.max_fp_id = archive_max.max(cold);
          }
        }
      }
    }
    self.install_replay_state(tbl, replay, size)?;
    // 归档快照来自旧 term 时，把活跃身份改写为当前 (branch, term)：file range
    // 历史链保留旧 term 的对象 key，新写入从当前 term 继续（install 已按 term
    // 变化抬过文件边界）。
    if let Some(metadata) = self
      .table_map
      .get_mut(tbl)
      .and_then(|state| state.branch_metadata.as_mut())
    {
      metadata.branch_name = branch.as_ref().into();
      metadata.term = self.term;
    }
    let (root, ttl_root) = {
      let state = self.table_map.get(tbl).ok_or(KvError::ResourceMissing)?;
      (state.root.ok_or(KvError::ResourceMissing)?, state.ttl_root)
    };
    if let Err(err) = self.write_snapshot_manifest(tbl, root, ttl_root).await {
      // 内存已切到归档快照，但活跃 manifest 发布失败；清空表状态，避免下一次
      // 写以归档 root 给旧 manifest 追加 log（或写 log-only manifest）。
      self.evict_table(tbl);
      return Err(err);
    }
    // 归档 manifest 仍留在盘上并引用快照页：把 Pooled 复用下界抬到归档或
    // 其它快照的高水位，后续覆盖写退役的旧页不得回池（pooled-07）。
    self.rebuild_reuse_floor(tbl).await;
    Ok(ArchiveInfo {
      name,
      bytes: bytes.len(),
      removed_num: 0,
    })
  }

  /// 删除当前分支下指定 tag 的归档 manifest（全部 term），返回删除条数。
  ///
  /// 对齐 C++ `ArchiveRequest::Action::Delete` 的幂等删除语义（文件不存在不算
  /// 失败）；与 `delete_branch` 一致按 tag 扫全部 term，这样 term 推进后仍能删掉
  /// 旧归档（C++ 只按当前 process term 定位）。
  pub async fn delete_archive(&mut self, tbl: &TableIdent, tag: &str) -> Result<usize> {
    if !is_valid_archive_tag(tag) {
      return Err(KvError::InvalidArgs);
    }
    let branch = normalize_branch_name(&self.branch_name).ok_or(KvError::InvalidArgs)?;
    let mut removed = 0;
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    for name in self.io.list_manifest_file_name_li(tbl).await? {
      yield_budget.maybe_yield().await;
      let Some((archive_branch, _, Some(archive_tag))) = parse_manifest_file_name(&name) else {
        continue;
      };
      if archive_branch != branch.as_ref() || archive_tag != tag {
        continue;
      }
      self.io.remove_manifest_file(tbl, &name).await?;
      removed += 1;
    }
    Ok(removed)
  }

  /// 删除 branch 的所有 manifest / archive 文件（不允许 main 或 active branch）。
  pub async fn delete_branch(&mut self, tbl: &TableIdent, branch_name: &str) -> Result<usize> {
    let normalized = normalize_branch_name(branch_name).ok_or(KvError::InvalidArgs)?;
    let active = normalize_branch_name(&self.branch_name).ok_or(KvError::InvalidArgs)?;
    if normalized.as_ref() == MAIN_BRANCH_NAME || normalized == active {
      return Err(KvError::InvalidArgs);
    }
    let mut removed = 0;
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    for name in self.io.list_manifest_file_name_li(tbl).await? {
      yield_budget.maybe_yield().await;
      let Some((branch, ..)) = parse_manifest_file_name(&name) else {
        continue;
      };
      if branch == normalized.as_ref() {
        self.io.remove_manifest_file(tbl, &name).await?;
        removed += 1;
      }
    }
    Ok(removed)
  }

  /// 归档保留剪枝（有意差异，见 eloqstore2fkv/COMPAT_BASELINE「归档保留策略」）：
  /// C++ 基线从不自动删归档（`DeleteOldArchives` 无调用点死代码），fkv 实现
  /// 配置项文档宣称的语义——按 branch 以 `(tag, term)` 双降序保留最近
  /// `num_retained` 个**唯一 tag**，同 tag 只保留最高 term（与 `reopen_archive`
  /// 的选择一致，去重后不占额外名额）；`num_retained=0` 表示不剪。
  /// 只删带 tag 的归档 manifest，活跃 / branch manifest（无 tag）不在范围。
  async fn prune_archive_li(
    &self,
    tbl: &TableIdent,
    branch: &str,
    num_retained: u32,
  ) -> Result<usize> {
    if num_retained == 0 {
      return Ok(0);
    }
    let mut archive_li: Vec<(String, u64, String)> = self
      .io
      .list_manifest_file_name_li(tbl)
      .await?
      .into_iter()
      .filter_map(|name| {
        let (archive_branch, term, tag) = parse_manifest_file_name(&name)?;
        let tag = tag?.to_owned();
        if archive_branch == branch {
          Some((tag, term, name))
        } else {
          None
        }
      })
      .collect();
    // 同一 tag 可能有多个 term：保留最高 term（与 `reopen_archive` 选择一致），
    // 再按 tag 倒序保留最近 `num_retained` 个标签。
    archive_li.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    let mut removed = 0;
    let mut kept_tag_set = HashSet::default();
    for (tag, _, name) in archive_li {
      yield_budget.maybe_yield().await;
      let keep = if kept_tag_set.contains(&tag) {
        false
      } else if kept_tag_set.len() < num_retained as usize {
        kept_tag_set.insert(tag);
        true
      } else {
        false
      };
      if !keep {
        self.io.remove_manifest_file(tbl, &name).await?;
        removed += 1;
      }
    }
    Ok(removed)
  }
}

impl<I: PageIo + ManifestIo + FileIo + SegmentIo> Store<I> {
  /// 生成一份 archive manifest，并按 tag 倒序保留最近 `num_retained` 份。
  ///
  /// 落 snapshot 前先做一次按需 pre-compact（对齐 C++ `CreateArchive` 的
  /// needs_compact 分支）：放大率超阈值时归档继承稠密 mapping，无引用文件
  /// 随归档回收。
  ///
  /// tag 只要求「可落盘」（[`is_valid_archive_tag`]，对齐 C++ `BranchArchiveName`
  /// 不做字符集校验）。**空 tag 在这里非法**：C++ 的自动时间戳在
  /// `BackgroundWrite::CreateArchive` 生成，Rust 对应 `fkv_shard` 的
  /// `gated_body` 在调用本方法前生成（公共 `Runtime::create_archive` 空 tag 可用）。
  /// 以 `.tmp` 结尾的 tag 必须拒绝：`<name>.tmp` 是 manifest 整写中间态，列举 /
  /// 解析一律跳过，允许写入会产出自己读不到的永久孤儿。
  pub async fn create_archive(
    &mut self,
    tbl: &TableIdent,
    tag: &str,
    num_retained: u32,
  ) -> Result<ArchiveInfo> {
    if !is_valid_archive_tag(tag) {
      return Err(KvError::InvalidArgs);
    }
    // 表未打开 / 已被 drop（或 `open_empty` 的空表没有真实 manifest）：
    // C++ `CreateArchive` 对 `FindRoot == NotFound` 直接成功且不落文件；
    // 返回空回执，避免归档线程“复活”已删表，也不能写空归档。
    if !self
      .table_map
      .get(tbl)
      .is_some_and(|state| state.manifest_started)
    {
      return Ok(ArchiveInfo {
        name: String::new(),
        bytes: 0,
        removed_num: 0,
      });
    }
    self.compact_before_archive(tbl).await?;
    // compact 会推进 mapper 与 root：对齐 C++ `Compact()` 后 re-`FindRoot`，
    // 这里重取快照所需的 root / ttl_root。
    let (root_id, ttl_root_id) = {
      let state = self.table_map.get(tbl).ok_or(KvError::ResourceMissing)?;
      (state.root.ok_or(KvError::ResourceMissing)?, state.ttl_root)
    };
    let branch = normalize_branch_name(&self.branch_name).ok_or(KvError::InvalidArgs)?;
    self.refresh_branch_metadata(tbl);
    // 归档 manifest 引用此刻映射里的全部物理页；发布前把复用下界抬到同一
    // 高水位，之后父表覆盖写退役的旧页才不会被回池复用（pooled-07）。
    let archive_floor = self
      .table_map
      .get(tbl)
      .and_then(|state| state.mapper.as_ref())
      .map_or(0, PageMapper::max_fp_id);
    // 对齐 C++ `CreateArchive`：`segment_mapper_ == nullptr` 时传 nullptr，
    // snapshot 不写可选 segment 段；有活跃 segment mapping 时由
    // `snapshot_record` 统一写入。
    let record = self.snapshot_record(tbl, root_id, ttl_root_id, None, None, None)?;
    self.raise_reuse_floor(tbl, archive_floor);
    let name = archive_file_name(&branch, self.term, tag);
    self.io.write_manifest_file(tbl, &name, &record).await?;
    // 归档成功后立即剪枝是有意差异：C++ 基线的 `DeleteOldArchives` 是无调用点
    // 死代码、归档从不自动删除（`num_retained_archives` 只控制 crond 启动），
    // 但 C++ 架构文档本身宣称按该值剪枝；详见 eloqstore2fkv/COMPAT_BASELINE
    // 「归档保留策略」条目。
    let removed_num = self.prune_archive_li(tbl, &branch, num_retained).await?;
    Ok(ArchiveInfo {
      name,
      bytes: record.len(),
      removed_num,
    })
  }

  /// 归档前的按需 pre-compact（对齐 C++ `CreateArchive` 的 needs_compact 分支，
  /// 谓词同写路径 `MapperExceedsAmplification`，即 [`Self::needs_maintain`] /
  /// [`Self::needs_segment_maintain`]）：data / segment 任一侧放大率超阈值时
  /// 先跑一轮 compact，再回收无引用文件——归档 manifest 因此继承稠密
  /// mapping（C++ `Compact()` 末尾的 `RunFileGc`）。
  ///
  /// 与 [`Self::compact_before_branch`] 的差异对齐 C++：`CreateBranch` 无条件
  /// `Compact()`，`CreateArchive` 先做 O(1) 谓词判断，未超阈值不付 compact /
  /// GC 的目录扫描与搬迁代价。
  async fn compact_before_archive(&mut self, tbl: &TableIdent) -> Result<()> {
    if !(self.needs_maintain(tbl) || self.needs_segment_maintain(tbl)) {
      return Ok(());
    }
    self.compact_if_needed(tbl).await?;
    self.compact_segment_if_needed(tbl).await?;
    self.file_gc(tbl, self.pages_per_file_shift).await?;
    self.refresh_space_stat(tbl);
    Ok(())
  }

  /// 创建 branch manifest：继承当前 mapping，max_fp_id 跳到下一个文件避免与父 branch 冲突。
  ///
  /// 对齐 C++ `BackgroundWrite::CreateBranch`：
  /// - 先做一次 pre-compact，branch 继承稠密 mapping，不携带父的碎片文件；
  /// - 分区尚未打开（无 root）或 mapper 缺失时视为空分区，直接成功且不落 branch manifest。
  ///
  /// 嵌套语义（sem-24）：受限父分支（`branch_file_limit.is_some()`）建子分支后
  /// 仍能在自己的预留块内继续写。父分支的分配水位**不** reserve 到子块末尾
  /// （否则越界后每次写都 `OutOfSpace`），子块只能落在父块之外；新子块从
  /// 盘上**全部** manifest 的已承诺块上界之后继续分配
  /// （[`Self::committed_block_ceiling`]），各占一个
  /// `BRANCH_FILE_RESERVE_FILE_NUM` 文件块。main / 无上限 Store 保持原语义：
  /// reserve 跳过整个子块。
  pub async fn create_branch(&mut self, tbl: &TableIdent, branch_name: &str) -> Result<BranchInfo> {
    let normalized = normalize_branch_name(branch_name).ok_or(KvError::InvalidArgs)?;
    let active = normalize_branch_name(&self.branch_name).ok_or(KvError::InvalidArgs)?;
    if normalized == active || normalized.as_ref() == MAIN_BRANCH_NAME {
      return Err(KvError::InvalidArgs);
    }
    let opened = self.table_map.get(tbl).is_some_and(|state| {
      state.manifest_started && state.root.is_some() && state.mapper.is_some()
    });
    if !opened {
      // 空分区 / 未打开表（含 Runtime `open_empty`）：没有可继承的 mapping，
      // 不创建 active / branch manifest（对齐 C++ NotFound 直接成功）。
      return Ok(BranchInfo {
        name: String::new(),
        bytes: 0,
      });
    }
    self.compact_before_branch(tbl).await?;
    // 先把父表区间链刷到当前高水位再克隆：branch manifest 的前缀区间是子分支
    // 推导「继承页下界」的唯一依据（pooled-07），陈旧前缀会让下界偏低。
    self.refresh_branch_metadata(tbl);
    // 受限父分支（非 main）：子块必须落在父块之外，但父分支的分配水位不能像
    // main 那样 reserve 到子块末尾——那会推过自己的 `branch_file_limit`，此后
    // 父分支每次写都在 `write.rs` 的上限检查处 `OutOfSpace`（sem-24）。父分支
    // 保持块内可写，兄弟子块的位置从盘上 manifest 反推（子块已被承诺则不重叠）。
    let limited_parent = self.branch_file_limit.is_some();
    let (root_id, ttl_root_id, parent_max_fp_id, use_segment, parent_segment_max_fp, mut metadata) = {
      let state = self.table_map.get(tbl).ok_or(KvError::ResourceMissing)?;
      (
        state.root.ok_or(KvError::ResourceMissing)?,
        state.ttl_root,
        state.mapper.as_ref().map_or(0, PageMapper::max_fp_id),
        // 是否启用 segment 侧：显式配置了大 value，或该表已有 segment mapping。
        // 两者都不成立时（plain store）不创建 segment mapper，branch snapshot
        // 与 C++ `BackgroundWrite::CreateBranch` 一样完全省略可选 segment 段。
        state.segment_mapper.is_some() || self.large_value_layout.is_some(),
        state
          .segment_mapper
          .as_ref()
          .map_or(0, PageMapper::max_fp_id),
        state
          .branch_metadata
          .clone()
          .unwrap_or_else(|| BranchManifestMetadata::new(normalized.clone(), 0)),
      )
    };
    // 块位必须**全局单调**：新区块接在「所有已落盘 manifest 的已承诺块上界」
    // 与父分支高水位之后。只避让直接子分支会让孙 / 侄块与其它子树的块落在
    // 同一个 1024 文件预留块上；main 完全不扫描则连它的直接子块都会撞块
    // （pooled-10）。main / branch / archive manifest 一律计入，父子、兄弟、
    // 叔侄、祖孙任意组合都不重叠。归档 manifest 也计入：子分支被删除但归档
    // 仍引用其块时，该块不能重新分配。
    let (data_ceiling_file, segment_ceiling_file) = self.committed_block_ceiling(tbl).await?;
    let next_file_id = (parent_max_fp_id >> self.pages_per_file_shift)
      .checked_add(1)
      .ok_or(KvError::InvalidArgs)?;
    let branch_base_file =
      align_branch_block(next_file_id.max(data_ceiling_file)).ok_or(KvError::InvalidArgs)?;
    let branch_limit_file = branch_base_file
      .checked_add(BRANCH_FILE_RESERVE_FILE_NUM)
      .ok_or(KvError::InvalidArgs)?;
    let new_max_fp_id = branch_base_file
      .checked_shl(u32::from(self.pages_per_file_shift))
      .ok_or(KvError::InvalidArgs)?;
    let branch_limit_fp_id = branch_limit_file
      .checked_shl(u32::from(self.pages_per_file_shift))
      .ok_or(KvError::InvalidArgs)?;
    let segment_shift = self
      .large_value_layout
      .map_or(DEFAULT_SEGMENTS_PER_FILE_SHIFT, |layout| {
        layout.segments_per_file_shift()
      });
    let next_segment_file_id = (parent_segment_max_fp >> segment_shift)
      .checked_add(1)
      .ok_or(KvError::InvalidArgs)?;
    let branch_segment_base_file =
      align_branch_block(next_segment_file_id.max(segment_ceiling_file.unwrap_or(0)))
        .ok_or(KvError::InvalidArgs)?;
    let branch_segment_limit_file = branch_segment_base_file
      .checked_add(BRANCH_FILE_RESERVE_FILE_NUM)
      .ok_or(KvError::InvalidArgs)?;
    let branch_segment_fp_id = branch_segment_base_file
      .checked_shl(u32::from(segment_shift))
      .ok_or(KvError::InvalidArgs)?;
    let branch_segment_limit_fp_id = branch_segment_limit_file
      .checked_shl(u32::from(segment_shift))
      .ok_or(KvError::InvalidArgs)?;
    // 子分支继承父分支的 file_range_li 链，再追加自己的区间。
    //
    // 子分支的块基址由 manifest 区间链里的**父区间上界**派生
    // （[`crate::manifest::branch_block_base`]）：`align(parent_max_file_id + 1)`。全局块位可
    // 能把新块推到父区间之后的更远处，这时必须把快照前缀里的父区间上界补到
    // `base - 1`，否则重开 / 归档时派生的块基址会偏小，`branch_file_limit`
    // 随之出错，分支会写进别人的预留块。只在派生值小于实际 base 时抬高，
    // 避免无谓增大子分支继承页的复用下界。父分支自身的 manifest / 区间链
    // 不修改，term bump 的区间起点不会被推过自己的块上限。
    if let Some(parent_range) = metadata.file_range_li.last_mut() {
      let derived = align_branch_block(parent_range.max_file_id.saturating_add(1));
      if derived < Some(branch_base_file) {
        parent_range.max_file_id = branch_base_file.saturating_sub(1);
      }
      if use_segment {
        let derived_segment =
          align_branch_block(parent_range.max_segment_file_id.saturating_add(1));
        if derived_segment < Some(branch_segment_base_file) {
          parent_range.max_segment_file_id = branch_segment_base_file.saturating_sub(1);
        }
      }
    }
    metadata.branch_name = normalized.clone();
    metadata.term = 0;
    metadata.file_range_li.push(BranchFileRange {
      branch_name: normalized.clone(),
      term: 0,
      max_file_id: new_max_fp_id >> self.pages_per_file_shift,
      // 无 segment 侧时填 0：与 C++ branch metadata 一致，避免 GC / cloud
      // range lookup 把这个区间误当成已存在的 segment 文件区间。
      max_segment_file_id: if use_segment {
        branch_segment_base_file
      } else {
        0
      },
    });
    // 父 branch 先跳过整个 segment 预留区间，再写 branch snapshot：
    // 只有 use_segment 时才需要；plain store 不凭空创建 segment mapper。
    // 受限父分支的段写入由自己的 `branch_segment_limit` 挡在块内，不能把段
    // 水位 reserve 到子块末尾（sem-24，同 data 侧）；main 仍需要跳过子块。
    // 预留先落盘即使 branch manifest 写失败也只是浪费空间，不会重叠。
    //
    // 受限父分支（有段上限）跳过 reserve：段写入由自己的
    // `branch_segment_limit` 挡在块内（sem-24）。无段上限的 legacy branch
    // （创建时未启用大 value，后来又配置了 layout）保持旧行为：创建段
    // mapper 并跳过子段块，至少保证子分支段块不被父分支占用。
    if use_segment && self.branch_segment_limit.is_none() {
      let source = self.generation_source.clone();
      let snapshot_source = self.snapshots.generation_source();
      // 段 mapper 恒用 `Append`，与 data 侧 `alloc_mode` 无关（pooled-02，
      // 对齐 C++ `EnsureSegmentMapper`）：预留区间只抬高水位，不产生 retired。
      self
        .table_map
        .entry(tbl.clone())
        .or_default()
        .segment_mapper
        .get_or_insert_with(|| {
          let mut mapper = PageMapper::new_with_source(source, AllocMode::Append, 0);
          mapper.set_snapshot_generation_source(snapshot_source);
          mapper
        })
        .reserve_until(branch_segment_limit_fp_id);
    }
    let record = self.snapshot_record(
      tbl,
      root_id,
      ttl_root_id,
      Some(new_max_fp_id),
      use_segment.then_some(branch_segment_fp_id),
      Some(&metadata),
    )?;
    let name = manifest_file_name(&normalized, 0);
    // 父 branch 跳过整个 data 预留区间，先把新 max 持久化；
    // 即使随后 branch manifest 写入失败，也只是浪费预留空间，不会重叠。
    if limited_parent {
      // 受限父分支：分配水位停在块内，子分支继承的活页全部低于当前高水位。
      // 把复用下界抬到高水位即可保护它们（此后父表 COW 退役的继承页不回池，
      // pooled-07），同时不移动水位，父分支仍能在自己块内继续写（sem-24）。
      let floor = self.mapper(tbl).map_or(0, PageMapper::max_fp_id);
      self.raise_reuse_floor(tbl, floor);
    } else {
      if let Some(mapper) = self
        .table_map
        .get_mut(tbl)
        .and_then(|state| state.mapper.as_mut())
      {
        mapper.reserve_until(branch_limit_fp_id);
        // 父表下界 = 子分支预留区间末尾：父表此后分配都在 `branch_limit` 之上，
        // 低于它的旧页全部属于子分支继承的持久快照，不得回池（pooled-07）。
        mapper.raise_reuse_floor(branch_limit_fp_id);
      }
    }
    self
      .write_snapshot_manifest(tbl, root_id, ttl_root_id)
      .await?;
    self.io.write_manifest_file(tbl, &name, &record).await?;
    // 防御：受限分支的分配水位必须留在自己的块内（sem-24）。正常路径不会
    // 触发；盘上遗留的越界 manifest 在重开后仍可能越界，这里只 fail-fast 到
    // debug，不改变线上行为。
    debug_assert!(
      self.branch_file_limit.is_none_or(|limit| self
        .mapper(tbl)
        .is_none_or(|mapper| mapper.max_fp_id() <= limit)),
      "create_branch 后父分支高水位越过自身上限"
    );
    Ok(BranchInfo {
      name,
      bytes: record.len(),
    })
  }

  /// 扫描本表**全部** manifest，返回已承诺的 data / segment 块上界（file id，
  /// 半开；segment 为 `None` 表示没有段体系）。见
  /// [`crate::manifest::manifest_data_ceiling`] /
  /// [`crate::manifest::manifest_segment_ceiling`]。
  ///
  /// 不按父子关系过滤：块位必须全局单调，只避让直接子分支会让孙 / 侄块与
  /// 其它子树的块重叠（pooled-10）。main 的活跃 manifest 同样计入，覆盖
  /// 「祖先在 fork 后继续写」的区间（sem-25）：它的 `max_fp_id` 是下一次分配
  /// 的起点，所在块必须整块留给 main。
  ///
  /// 活跃 main manifest 需要单独读取：`MemStore` / `SimStore` 把 `switch_manifest`
  /// 的活跃槽位放在 `manifest_map`，`list_manifest_file_name_li` 只列具名
  /// manifest 文件，不会返回它（本地文件后端会列 `manifest!main!0`）。用
  /// [`ManifestIo::read_local_manifest`] 读取：Cloud 不额外回源，也不触发
  /// term 采纳栅栏——块位分配是控制面计算，不应改变 attach 语义。
  ///
  /// 解析失败的 manifest 不能静默跳过（无法判断它占了哪些块），直接
  /// fail-fast 返回 `Corrupted`；legacy 无区间链时按高水位上取整并告警，
  /// 比现状多推进一个块，宁浪费不重叠。
  async fn committed_block_ceiling(&self, tbl: &TableIdent) -> Result<(FileId, Option<FileId>)> {
    let shift = self.pages_per_file_shift;
    let segment_shift = self.segment_shift();
    let mut data_ceiling = 0u64;
    let mut segment_ceiling: Option<u64> = None;
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    // 活跃 main manifest 不在 MemStore / SimStore 的列举口径内。名字已列出的
    // 后端（本地文件 / Cloud 远端清单）不重复 replay。
    let main_name = manifest_file_name(MAIN_BRANCH_NAME, 0);
    let mut saw_main = false;
    for name in self.io.list_manifest_file_name_li(tbl).await? {
      yield_budget.maybe_yield().await;
      saw_main |= name == main_name;
      // 列举与读取之间被删除：已无引用，不占块。
      if let Some(bytes) = self.io.read_manifest_file(tbl, &name).await? {
        accumulate_manifest_ceiling(
          &name,
          &bytes,
          shift,
          segment_shift,
          &mut data_ceiling,
          &mut segment_ceiling,
        )?;
      }
    }
    if !saw_main && let Some(bytes) = self.io.read_local_manifest(tbl).await? {
      accumulate_manifest_ceiling(
        &main_name,
        &bytes,
        shift,
        segment_shift,
        &mut data_ceiling,
        &mut segment_ceiling,
      )?;
    }
    Ok((data_ceiling, segment_ceiling))
  }

  /// branch 创建前的全量 pre-compact（对齐 C++ `BackgroundWrite::Compact`）：
  /// data / segment 两侧各自按放大率把 live page 搬到文件尾，搬空的旧文件
  /// 随后由 GC 删除，branch 继承的 mapping 因此是稠密的。
  ///
  /// data 侧只在 `Append` 模式参与（[`Self::amplify_compact_enabled`]，对齐
  /// C++ `Compact()` 的 `if (opts->data_append_mode) DoCompactDataFile(...)`）；
  /// Pooled 下 branch 直接继承当前 mapping，末尾 `file_gc` 仍照常回收零引用
  /// 文件。
  ///
  /// 正在追加的尾文件未写满，按容量算放大率会误判，跳过（同 `compact_if_needed`）。
  /// 两个放大率阈值都为 0（或 data 侧被 gate 关闭）表示关闭自动 compact。
  async fn compact_before_branch(&mut self, tbl: &TableIdent) -> Result<()> {
    let data_factor = if self.amplify_compact_enabled() {
      self.file_amplify_factor
    } else {
      0
    };
    let segment_factor = self.segment_file_amplify_factor;
    if data_factor == 0 && segment_factor == 0 {
      return Ok(());
    }
    if data_factor != 0
      && let Some(mapper) = self
        .table_map
        .get(tbl)
        .and_then(|state| state.mapper.as_ref())
    {
      let plan_li = {
        let live_map = fkv_gc::file_live_count(mapper.tbl(), self.pages_per_file_shift);
        let candidate_li = self.io.list_file_id_li(tbl).await?;
        let pages_per_file = 1u64 << self.pages_per_file_shift;
        // 尾文件按分配器高水位算：盘上最大 id 可能因空文件被 GC 删除而更小。
        fkv_gc::compact_plan(
          &live_map,
          &candidate_li,
          pages_per_file,
          data_factor,
          mapper.max_fp_id() >> self.pages_per_file_shift,
        )
      };
      if !plan_li.is_empty() {
        self
          .compact_file_li(tbl, &plan_li, self.pages_per_file_shift)
          .await?;
      }
    }
    if segment_factor != 0 {
      let plan_li = self.segment_compact_plan(tbl).await?;
      if !plan_li.is_empty() {
        self.compact_segment_file_li(tbl, &plan_li).await?;
      }
    }
    // C++ `Compact()` 末尾无条件 `RunFileGc`：即使没有文件达到 compact 阈值，
    // 也回收无引用的旧文件，保证 branch 继承的文件集合与 C++ 一致。
    self.file_gc(tbl, self.pages_per_file_shift).await?;
    self.refresh_space_stat(tbl);
    Ok(())
  }

  /// 删除整表：只删当前 identity 的活跃 manifest，随后清空内存状态。
  ///
  /// 对齐 C++ `BatchWriteTask::Drop` → `DropManifest`：manifest 名是
  /// `manifest_<active_branch>_<process_term>`，删本 shard 当前身份这一份。
  /// branch 模式（sem-60 写时晋升）下只删当前名会让旧 term 对象残留，reopen
  /// 按回退链重新选中已 Drop 的表（复活）；因此与 Cloud main 面
  /// `drop_manifest` 同口径，删**同 branch、无 tag、term 不高于当前 identity
  /// 的全部**具名 manifest——更高 term 属于新 owner，保留其 durability
  /// anchor。data / segment 文件不在这里硬删——其它 branch / archive manifest
  /// 可能仍引用它们，直接删会让那些快照变成悬空引用。无引用的文件由调用方
  /// 发起的 `FileGc` 按 retained set 回收（与 C++ Drop 末尾 `TryAddFileGc`
  /// 一致）。
  ///
  /// 失败也先 `evict_table` 再返回 `Err`（含 branch 面的 manifest 列举
  /// 失败）：避免失败后留下 `manifest_started=true` 的脏状态，下一次写把
  /// log record 写成 manifest 第一条记录（恢复侧会按 snapshot 解析而
  /// 损坏）。失败分两种形态：
  /// - 删除阶段失败（branch 面单条 remove、main 面 io 失败）：尽力删完全部
  ///   目标后收集首错返回——own 名留在盘上会让 reopen 复活，不能中断；
  /// - branch 面列举失败：删除阶段整体跳过、零删除——没有列举就无法区分
  ///   「仅剩 own」与「还有旧 term」，仅删 own 会让漏删的旧 term 对象把已
  ///   Drop 的表经回退链复活（sem-60 锁定）。盘面保持完整，由调用方重试
  ///   完成清扫；evict 不影响重试：branch 身份是 Store 级状态，drop 不依赖
  ///   表已加载。
  pub async fn drop_table(&mut self, tbl: &TableIdent) -> Result<()> {
    // branch attach 后活跃 manifest 是显式名字（`manifest_<branch>_<term>`），
    // 不走 main 的固定路径；main / 未指定 branch 时才走 `drop_manifest`。
    let drop_result = if self.manifest_file.is_some() {
      let branch = self.branch_name.clone();
      let term = self.term;
      let own = manifest_file_name(&branch, term);
      let mut first_err = None;
      // 列举失败不 `?` 提前返回：那会绕过下方的 `evict_table` 统一收尾，
      // 留下 manifest_started 脏状态。失败时删除阶段（含 own 幂等删）整体
      // 跳过，保持远端对象零删除，错误与删除失败同路收尾。
      match self.io.list_manifest_file_name_li(tbl).await {
        Ok(name_li) => {
          // 本 term 可能从未发布过该表：own 名不在列举里也要幂等删一次。
          let mut target_li: Vec<(u64, String)> = name_li
            .into_iter()
            .filter_map(|name| {
              let (file_branch, file_term, tag) = fkv_types::parse_manifest_file_name(&name)?;
              (tag.is_none() && file_branch == branch && file_term <= term)
                .then_some((file_term, name))
            })
            .collect();
          if !target_li.iter().any(|(_, name)| name == &own) {
            target_li.push((term, own));
          }
          // 旧 term 先删、own（本 identity 的最高 term）最后：先删 own 的话并发
          // 读者会在两次删除之间按回退链选中旧 term 对象并采纳（复活）。
          target_li.sort_unstable_by_key(|(file_term, _)| *file_term);
          for (_, name) in &target_li {
            if let Err(err) = self.io.remove_manifest_file(tbl, name).await {
              // 单条失败不中断：own 留在盘上会让 reopen 复活；收集首错继续删。
              first_err.get_or_insert(err);
            }
          }
        }
        Err(err) => {
          first_err.get_or_insert(err);
        }
      }
      match first_err {
        Some(err) => Err(err),
        None => Ok(()),
      }
    } else {
      self.io.drop_manifest(tbl).await
    };
    self.evict_table(tbl);
    drop_result?;
    // 物理文件不在这里同步删除：C++ `Drop` 只 `DropManifest` 并触发后台
    // `FileGc`，由 GC 依据全部 manifest 的 retained set 决定哪些文件无引用。
    // 调用方需要立即回收时显式发起 `FileGc` / `maintain`。
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use fkv_btree::BatchEntry;
  use fkv_error::KvError;
  use fkv_io::ManifestIo;
  use fkv_io_mem::MemStore;
  use fkv_types::TableIdent;

  use crate::{ArchiveTagInfo, Store};

  /// sem-27：`delete_branch` 删除该 branch 的全部 manifest（含带 tag 归档），
  /// 且不波及同表其它 branch 的同名 tag 归档。
  ///
  /// 对齐 C++ **Cloud** `CloudStoreMgr::DeleteBranchFiles`（按
  /// `manifest_<branch>_` 前缀全删）；C++ Local `IouringMgr::DeleteBranchFiles`
  /// 只删无 tag manifest + CURRENT_TERM、归档保留，属有意差异（Local 上
  /// C++ 显式 `Reopen` 本身返回 `InvalidArgs`，归档不经 Cloud / Standby 无法
  /// 按 tag 恢复）。Local 与 Cloud 共用本泛型实现（`Store<I>`），盘面删除
  /// 语义一致。判定与差分口径见 eloqstore2fkv/COMPAT_BASELINE「`DeleteBranch`
  /// 的归档删除面」及 issue/reviewed/sem-27-delete-branch-archive-divergence.md。
  #[test]
  fn delete_branch_removes_branch_archives() {
    pollster::block_on(async {
      let tbl = TableIdent::new("deletebranch", 1).unwrap();
      let mut store = Store::new(MemStore::new(512), 512, 16).with_branch("main", 7);
      store
        .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
        .await
        .unwrap();
      // main 与 feature 故意共用 tag "v1"：删除只能按 branch 过滤，不能按 tag 全删。
      let kept = store.create_archive(&tbl, "v1", 0).await.unwrap();
      assert_eq!(kept.name, "manifest_main_7_v1");
      let branch = store.create_branch(&tbl, "feature").await.unwrap();
      assert_eq!(branch.name, "manifest_feature_0");

      // 在 feature（term 1）上落两份归档：v1 与 main 同名，drop 仅属 feature。
      store.prepare_attach("feature", 1).unwrap();
      store.reopen(&tbl).await.unwrap();
      let shared = store.create_archive(&tbl, "v1", 0).await.unwrap();
      assert_eq!(shared.name, "manifest_feature_1_v1");
      let only_feature = store.create_archive(&tbl, "drop", 0).await.unwrap();
      assert_eq!(only_feature.name, "manifest_feature_1_drop");

      // 回到 main 才能删除 feature（不允许删 active branch）。
      store.prepare_attach("main", 7).unwrap();
      store.reopen(&tbl).await.unwrap();
      assert_eq!(
        store.list_archive_tags(&tbl).await.unwrap(),
        vec![
          ArchiveTagInfo {
            tag: "drop".to_owned(),
            term: 1,
          },
          ArchiveTagInfo {
            tag: "v1".to_owned(),
            term: 1,
          },
          ArchiveTagInfo {
            tag: "v1".to_owned(),
            term: 7,
          },
        ]
      );

      // manifest_feature_0（创建）+ manifest_feature_1（sem-60：term 1 reopen
      // 回退采纳 `_0` 后的晋升种子）+ 两份归档一起删除。
      assert_eq!(store.delete_branch(&tbl, "feature").await.unwrap(), 4);
      let name_li = store
        .page_io()
        .list_manifest_file_name_li(&tbl)
        .await
        .unwrap();
      assert!(
        name_li
          .iter()
          .all(|name| !name.starts_with("manifest_feature_")),
        "branch 的全部 manifest（含归档）应被删除: {name_li:?}"
      );
      assert!(name_li.contains(&"manifest_main_7_v1".to_owned()));
      // 表级归档清单只剩 main 的 v1：feature 的 drop / v1 都不再可见。
      assert_eq!(
        store.list_archive_tags(&tbl).await.unwrap(),
        vec![ArchiveTagInfo {
          tag: "v1".to_owned(),
          term: 7,
        }]
      );
      assert_eq!(
        store.reopen_archive(&tbl, "drop").await.unwrap_err(),
        KvError::ResourceMissing
      );
      // main 的同名归档不受影响，仍可按 tag 恢复且数据可读。
      store.reopen_archive(&tbl, "v1").await.unwrap();
      assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    });
  }

  /// sem-29：`create_branch` 对 main / 当前 active 返回 `InvalidArgs`，且活跃
  /// manifest 字节与父分支分配水位都不变。
  ///
  /// C++ 基线 `BackgroundWrite::CreateBranch` 只校验 branch 名非空，Local 下
  /// `ProcessTerm()` 恒为 0，`manifest_<branch>_0` 正是活跃 manifest：
  /// `CreateBranch("main")` / `CreateBranch(<active>)` 会把活跃 manifest 整份
  /// 覆盖（main 的 `max_fp_id` 被抬到下一文件边界、active 的 term / 区间链被
  /// 重置）并返回 `NoError`。Rust 在入口拒绝，属**有意差异**（单侧安全加固），
  /// 差分测试不得把 C++ 的 `NoError` 当 oracle；判定与口径见
  /// eloqstore2fkv/COMPAT_BASELINE「`CreateBranch` 目标为 main / active」及
  /// issue/done/sem-29-create-branch-main-active-divergence.md。
  #[test]
  fn create_branch_rejects_main_and_active() {
    pollster::block_on(async {
      let tbl = TableIdent::new("createbranch", 1).unwrap();
      let mut store = Store::new(MemStore::new(512), 512, 16).with_branch("main", 0);
      // 校验在 opened 判断之前：空分区同样拒绝（C++ 空分区直接 NoError 且不落
      // 文件，同属未定义输入，不作为对齐目标）。
      assert_eq!(
        store.create_branch(&tbl, "main").await.unwrap_err(),
        KvError::InvalidArgs
      );

      store
        .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
        .await
        .unwrap();
      let main_bytes = store
        .page_io()
        .read_manifest(&tbl)
        .await
        .unwrap()
        .expect("main 活跃 manifest 应已落盘");
      let main_max_fp = store.mapper(&tbl).unwrap().max_fp_id();
      assert_eq!(
        store.create_branch(&tbl, "main").await.unwrap_err(),
        KvError::InvalidArgs
      );
      assert_eq!(
        store.page_io().read_manifest(&tbl).await.unwrap().unwrap(),
        main_bytes,
        "拒绝 create_branch(main) 不得覆盖 main 活跃 manifest"
      );
      assert_eq!(store.mapper(&tbl).unwrap().max_fp_id(), main_max_fp);

      // 合法新分支仍可创建；随后在该 branch 上以 active 名再建应被拒绝。
      let branch = store.create_branch(&tbl, "f1").await.unwrap();
      assert_eq!(branch.name, "manifest_f1_0");
      store.prepare_attach("f1", 0).unwrap();
      store.reopen(&tbl).await.unwrap();
      let active_name = "manifest_f1_0";
      let active_bytes = store
        .page_io()
        .read_manifest_file(&tbl, active_name)
        .await
        .unwrap()
        .expect("f1 活跃 manifest 应已落盘");
      let active_max_fp = store.mapper(&tbl).unwrap().max_fp_id();
      assert_eq!(
        store.create_branch(&tbl, "f1").await.unwrap_err(),
        KvError::InvalidArgs
      );
      assert_eq!(
        store
          .page_io()
          .read_manifest_file(&tbl, active_name)
          .await
          .unwrap()
          .unwrap(),
        active_bytes,
        "拒绝 create_branch(<active>) 不得覆盖活跃 branch manifest"
      );
      assert_eq!(store.mapper(&tbl).unwrap().max_fp_id(), active_max_fp);

      // 对照：非 main / 非 active 的新名字照常可用。
      let f2 = store.create_branch(&tbl, "f2").await.unwrap();
      assert_eq!(f2.name, "manifest_f2_0");
    });
  }
}
