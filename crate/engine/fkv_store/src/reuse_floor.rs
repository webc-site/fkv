//! Pooled 页复用下界（pooled-07）：branch / archive 是**持久化**的页集合快照，
//! 不在 `SnapshotRegistry` 的读快照代际水位内。父表后续写把快照引用的物理页
//! COW 出活跃映射后，代际判据会放行回池——下一次分配立即覆盖快照数据。
//!
//! 本模块按 manifest 重建 sticky 的 `reuse_floor`：
//!
//! - `create_branch`：父表下界抬到子分支预留区间末尾（`branch_limit`）；
//! - 子分支 open / reopen：下界 = 父分支创建时高水位（继承页永不复用），
//!   从 branch manifest 的 `file_range_li` 链推导；
//! - `create_archive` / `reopen_archive`：下界 = 归档创建时 `max_fp_id`；
//! - 其它 branch manifest / 归档 manifest 的预留块：受限分支只接受**块起点
//!   不高于当前 `max_fp_id`** 的候选（祖先 / 更早创建的 branch，它们的块在
//!   本分支块之前或之下），兄弟 / 子孙块的起点都在本分支 `branch_file_limit`
//!   之外，不能当下界——否则高水位被抬过上限，写批次 flush 越界页后才判
//!   `OutOfSpace`，会静默覆盖父表 / 兄弟页（pooled-08）。main 能分配到全部
//!   子孙块，保持全量候选。候选块基址优先按 manifest 区间链派生（pooled-09），
//!   对方恰好写满块时不会把本分支的块算成对方的块。
//! - 其它 branch 的归档不能依赖该 branch manifest 仍在盘上：C++ Local
//!   `DeleteBranch` 只删无 tag manifest，Rust `delete_branch` 删除中途失败 /
//!   崩溃也会留下孤儿归档。归档 manifest 自带同一份区间链，按自身块末尾
//!   （`branch_block_base + RESERVE`）计入候选；legacy 无区间链时回退归档
//!   高水位。否则 main 的空闲池会把孤儿归档块整块回填，覆盖归档页后
//!   `reopen_archive` 仍成功但读不到数据（sem-28）。
//! - 受限分支有子 / 孙分支时，下界取**安装水位与子孙实际引用页的较大者**：
//!   父分支创建子分支之后仍会在自己块内写，后续 COW 退役的继承页不得回池，
//!   否则子分支快照被静默覆盖；取高水位（而不是子块末尾）保证分配水位不被
//!   推满（sem-24）。但归档回滚会把安装水位降回归档时刻，早于子分支创建
//!   水位的归档重开后，`[A, C)` 的继承页只被子孙 manifest 引用——下界必须
//!   由子孙 manifest **实际引用**的本分支块内页重建，不能随安装水位下降
//!   （pooled-11）。
//! - Append 的 main 没有复用下界，但同样在 reopen 时把分配水位抬到所有预留
//!   块末尾之后：否则 main 重新 attach 后继续写，越过间隙就会进入子孙块并
//!   覆盖其物理页（sem-25）。受限分支由 `branch_file_limit` 挡在块内，跳过。
//!   Append 的受限分支在归档重开时不回退追加水位（`reopen_archive` 保留活跃
//!   manifest 的水位），保证「旧页永不复用」在回滚路径上同样成立（pooled-11）。
//!   段（大 value）mapper 恒 Append，归档重开同样保留 segment 追加水位，两种
//!   `alloc_mode` 一致：段页没有空闲池，回退水位会覆盖更晚归档 / 子孙分支仍
//!   引用的段页（pooled-12）。
//!
//! 下界只升不降：归档 / 分支被删除后不回收空间，换取实现简单与不误伤快照。

use fkv_io::{ManifestIo, PageIo};
use fkv_manifest::BranchManifestMetadata;
use fkv_mapping::{AllocMode, MappingTbl, PageMapper};
use fkv_types::{
  FilePageId, MAIN_BRANCH_NAME, TableIdent, decode_id, is_file_page_id, manifest_file_name,
  parse_manifest_file_name,
};

use crate::{BRANCH_FILE_RESERVE_FILE_NUM, Store, manifest::branch_block_base};

/// 分支预留块的首个 file page id：`floor(file_id / RESERVE) * RESERVE << shift`。
///
/// 块起点 <= 当前高水位的 branch manifest 一定是祖先 / 更早创建的 branch：
/// 兄弟与子孙的块从当前分支块的末尾之后开始（`create_branch` 只在 1024 文件
/// 边界上分配块），把它们的块末尾当本分支下界会越过 `branch_file_limit`。
fn branch_block_start(max_fp_id: FilePageId, shift: u8) -> Option<FilePageId> {
  let reserve = BRANCH_FILE_RESERVE_FILE_NUM;
  (max_fp_id >> shift)
    .div_euclid(reserve)
    .checked_mul(reserve)?
    .checked_shl(u32::from(shift))
}

/// 分支分配区间末尾（`branch_base + RESERVE` 个文件），即该分支的
/// `branch_file_limit`。父表 / 祖先的复用下界必须覆盖整个预留区间。
fn branch_block_limit(max_fp_id: FilePageId, shift: u8) -> Option<FilePageId> {
  let reserve = BRANCH_FILE_RESERVE_FILE_NUM;
  let base_file = (max_fp_id >> shift)
    .div_euclid(reserve)
    .checked_mul(reserve)?;
  base_file
    .checked_add(reserve)?
    .checked_shl(u32::from(shift))
}

/// 从 manifest 区间链派生自身预留块的页区间 `[start, end)`。
///
/// 块基址 = `align(父区间上界 + 1)`（[`branch_block_base`]）；manifest 恰好
/// 写满块时 `max_fp_id` 落在下一块基址，按高水位反推会把本分支的块算成
/// 对方的块（sem-24）。`None` 表示 legacy manifest 缺父前缀，调用方回退按
/// 高水位反推的旧口径。
fn chain_block_span(
  metadata: &BranchManifestMetadata,
  shift: u8,
) -> Option<(FilePageId, FilePageId)> {
  let base_file = branch_block_base(metadata, metadata.branch_name.as_ref(), false)?;
  let start = base_file.checked_shl(u32::from(shift))?;
  let end = base_file
    .checked_add(BRANCH_FILE_RESERVE_FILE_NUM)?
    .checked_shl(u32::from(shift))?;
  Some((start, end))
}

/// 当前 branch manifest 中「首个本 branch 区间」之前的父分支高水位。
///
/// branch manifest 以父分支的 `file_range_li` 链为前缀、本 branch 的区间结尾；
/// 前缀最后一段的 `max_file_id` 就是父表创建该分支时的高水位。继承页全部
/// 低于 `(max_file_id + 1) << shift`，是子分支复用下界的紧上界。
fn inherited_floor(
  metadata: &BranchManifestMetadata,
  branch: &str,
  shift: u8,
) -> Option<FilePageId> {
  let own_idx = metadata
    .file_range_li
    .iter()
    .position(|range| range.branch_name.as_ref() == branch)?;
  let parent = metadata.file_range_li[..own_idx].last()?;
  parent
    .max_file_id
    .checked_add(1)?
    .checked_shl(u32::from(shift))
}

/// 子孙 manifest 在 `[start, end)` 内引用的最大物理页 + 1。
///
/// 子孙 branch / archive manifest 的 mapping 含它从当前分支继承的父块活页。
/// 归档把当前分支回滚到旧水位后，这些页既不在安装的归档 mapping 里，也不
/// 在安装水位之下——旧实现用安装后的 `max_fp_id` 当下界，下界会随归档回滚
/// 一起降回 `A`，`[A, C)` 的继承页被当作空洞重新分配（pooled-11）。
///
/// 这里直接取子孙 manifest **实际引用**的本分支块内页，与安装水位无关：
/// 重开 / `reopen_archive` 后都能从盘面重建。只统计本分支预留块内的页，
/// 子孙自己的块不会把下界推过 `branch_file_limit`（pooled-08 的死锁形态）。
fn descendant_page_floor(mapping: &MappingTbl, start: FilePageId, end: FilePageId) -> FilePageId {
  mapping
    .iter_value_li()
    .filter(|value| is_file_page_id(*value))
    .map(decode_id)
    .filter(|page| (start..end).contains(page))
    .max()
    .map_or(0, |page| page + 1)
}

impl<I> Store<I> {
  /// Pooled：把该表复用下界抬到 `floor`（sticky）。
  ///
  /// 受限 branch 的下界 / 分配水位不得越过自己的 `branch_file_limit`：上限
  /// 之外的页属于兄弟 / 子孙分支的预留块，本 branch 分配不到，也不得把
  /// 高水位推过去——`write_batch` 会先 flush 再判上限，越界页会覆盖别人的
  /// 页 id（pooled-08）。这里 clamp 是纵深防御，正常候选已按块起点过滤。
  pub(crate) fn raise_reuse_floor(&mut self, tbl: &TableIdent, floor: FilePageId) {
    if floor == 0 || self.alloc_mode != AllocMode::Pooled {
      return;
    }
    let floor = self
      .branch_file_limit
      .map_or(floor, |limit| floor.min(limit));
    if let Some(mapper) = self
      .table_map
      .get_mut(tbl)
      .and_then(|state| state.mapper.as_mut())
    {
      mapper.raise_reuse_floor(floor);
    }
  }
}

impl<I: PageIo + ManifestIo> Store<I> {
  /// 从本表 manifest 重建 Pooled 页复用下界；reopen / open_branch /
  /// reopen_archive 安装映射之后调用。
  ///
  /// 活跃 manifest 自身不在下界内：它的页是当前映射的活页或可安全复用的
  /// 空洞，由 mapping 与读快照水位覆盖。候选 manifest 读取 / 解析失败时保守
  /// 抬到当前 `max_fp_id`（宁可少复用，也不能复用快照引用页）；列举失败同样
  /// 按保守值处理，不影响 reopen 本身。
  ///
  /// **Append 的 main 也要跑本扫描**（sem-25）：Append 没有复用下界，但 main
  /// 重新 attach 后从 manifest 高水位继续分配，若不跳过子孙预留块，main 写满
  /// 两者的间隙后就会进入 `g` 的块并静默覆盖它（MemStore / SimStore 尤其）。
  /// 这里把 main 的分配水位抬到所有预留块末尾之后，与 Pooled 的 main 下界
  /// 走同一候选集。受限分支在 Append 下不受影响：它们由 `branch_file_limit`
  /// 挡在自己的块内。
  ///
  /// 代价：存在 branch / archive manifest 时需 replay 它们。这与 `file_gc`
  /// 的引用集合扫描同量级；Append 只有 main 支付，受限分支直接跳过。
  pub(crate) async fn rebuild_reuse_floor(&mut self, tbl: &TableIdent) {
    let Some(max_fp_id) = self.mapper(tbl).map(PageMapper::max_fp_id) else {
      return;
    };
    let shift = self.pages_per_file_shift;
    let branch = self.branch_name.clone();
    let is_main = branch.as_str() == MAIN_BRANCH_NAME;
    // Append：受限分支不复用旧页，也没有越过自己块的风险；只有 main 需要
    // 跳过子孙预留块（sem-25）。
    if self.alloc_mode != AllocMode::Pooled && !is_main {
      return;
    }
    // 当前是子分支：继承页低于父表创建时高水位，manifest 的区间链前缀可推导。
    // 推导不出（legacy / 空区间链）时按当前高水位保守处理。
    let mut floor = if is_main {
      0
    } else {
      self
        .table_map
        .get(tbl)
        .and_then(|state| state.branch_metadata.as_ref())
        .and_then(|metadata| inherited_floor(metadata, &branch, shift))
        .unwrap_or(max_fp_id)
    };
    let names = match self.io.list_manifest_file_name_li(tbl).await {
      Ok(names) => names,
      Err(err) => {
        log::warn!("reuse floor: 列举 manifest 失败 tbl={tbl} err={err}");
        self.raise_reuse_floor(tbl, max_fp_id);
        return;
      }
    };
    // 活跃 manifest 自己跳过。本地 main 活跃 manifest 固定 `manifest_main_0`；
    // branch 身份由 `manifest_file` 精确排除。Cloud 的带 term 活跃 manifest
    // 若未被排除，只会多算一档下界（更保守，不会少保护）。
    let active_file = self.manifest_file.clone();
    let main_active = manifest_file_name(MAIN_BRANCH_NAME, 0);
    // 本分支预留块 `[base, limit)`：只有落在这里的子孙页是「从本分支继承」
    // 的页。limit 已由 `apply_branch_limits` 按 manifest 区间链派生（块基址 +
    // 预留区间），块基址 = `limit - RESERVE << shift`。拿不到上限（main /
    // legacy）时退化为不扫描，仍按安装水位保护。
    let block_range = self.branch_file_limit.and_then(|limit| {
      let span = BRANCH_FILE_RESERVE_FILE_NUM.checked_shl(u32::from(shift))?;
      let block_start = limit.saturating_sub(span);
      // 窗口下界不能固定为派生块基址（sem-36）：C++ / legacy 区间链未按 1024
      // 文件对齐，归档 / 安装水位可能低于块基址，子分支继承的低地址页落在
      // `[安装水位, 块基址)`。只扫块内会让 `descendant_floor` 漏掉它们，
      // 下界停在安装水位，Pooled 续写把继承页当空洞回填。低于安装水位的页
      // 不会被分配（分配只前进），取 `min(块基址, 安装水位)` 即可；Rust 块
      // 对齐布局下二者相等，窗口不变。
      Some((block_start.min(max_fp_id), limit))
    });
    // 本分支是否有子 / 孙分支：任何其它 manifest 的区间链里出现本分支都算
    // （branch manifest 以祖先链为前缀；归档 manifest 保留同一前缀）。
    let mut has_descendant = false;
    // 子孙 manifest 引用的本分支块内最大页 + 1；与安装水位无关，归档回滚
    // 不会把它降低（pooled-11）。
    let mut descendant_floor = 0;
    for name in names {
      if active_file.as_deref() == Some(name.as_str()) || name == main_active {
        continue;
      }
      let Some((name_branch, _term, tag)) = parse_manifest_file_name(&name) else {
        continue;
      };
      // 当前 branch 的无 tag manifest 是活跃 manifest 的历史（Cloud 按 term 命名：
      // `manifest_main_<term>`），不是子分支预留块。把它们当 branch 处理会把下界
      // 抬到整个 1024 文件预留区间末尾（默认量纲下是 2^28 页），却并不提供额外
      // 保护：旧 term 页由 `install_replay_state` 的 term 区间下界覆盖（io-15）。
      if tag.is_none() && (name_branch == MAIN_BRANCH_NAME || name_branch == branch.as_str()) {
        continue;
      }
      let candidate = match self.io.read_manifest_file(tbl, &name).await {
        Ok(Some(bytes)) => match fkv_task_reopen::replay_manifest_bytes_with_len(&bytes) {
          Ok((state, _)) => {
            let is_descendant = !is_main
              && name_branch != branch.as_str()
              && state
                .branch_metadata
                .file_range_li
                .iter()
                .any(|range| range.branch_name.as_ref() == branch.as_str());
            if is_descendant {
              has_descendant = true;
              if let Some((start, end)) = block_range {
                descendant_floor =
                  descendant_floor.max(descendant_page_floor(&state.mapping_tbl, start, end));
              }
            }
            match tag {
              // 本 branch 的归档：创建时高水位即其引用页上界。
              Some(_) if name_branch == branch.as_str() => state.max_fp_id,
              // 其它 branch 的归档：不能假设该 branch manifest 还在盘上——
              // C++ Local `DeleteBranch` 只删无 tag manifest，Rust
              // `delete_branch` 删除中途失败 / 崩溃也会留下孤儿归档。归档
              // manifest 自带同一份区间链，按自身预留块末尾计入候选；受限
              // 分支沿用块起点过滤（pooled-08），避免兄弟 / 子孙块把水位推过
              // 自己的上限。legacy 无区间链时回退归档高水位（sem-28）。
              Some(_) => {
                let (start, end) =
                  chain_block_span(&state.branch_metadata, shift).unwrap_or_else(|| {
                    (
                      branch_block_start(state.max_fp_id, shift).unwrap_or(max_fp_id),
                      state.max_fp_id,
                    )
                  });
                if !is_main && start > max_fp_id {
                  continue;
                }
                end
              }
              // branch manifest：整个预留区间都不可复用。受限子分支只接受
              // 块起点不高于自身高水位的候选；兄弟 / 子孙（以及 sem-24 下
              // manifest max 被推过自身块的祖先）块起点都在本块之外，计入会把
              // 水位抬过 `branch_file_limit`（pooled-08）。
              //
              // 块基址优先从 manifest 的区间链派生（与 `open_branch` 的上限
              // 同一口径）：对方**恰好写满**自己的块时 `max_fp_id` 落在下一块
              // 基址上，按高水位反推会把本分支的块算成对方的预留块，下界被抬到
              // 自己块末（sem-24：父分支满块后子分支不可写）。legacy 无区间链
              // 时回退旧口径。
              None => {
                let (start, end) =
                  chain_block_span(&state.branch_metadata, shift).unwrap_or_else(|| {
                    (
                      branch_block_start(state.max_fp_id, shift).unwrap_or(max_fp_id),
                      branch_block_limit(state.max_fp_id, shift).unwrap_or(max_fp_id),
                    )
                  });
                if !is_main && start > max_fp_id {
                  continue;
                }
                end
              }
            }
          }
          Err(err) => {
            log::warn!("reuse floor: manifest 解析失败 name={name} err={err}");
            max_fp_id
          }
        },
        // 列举与读取之间被删除：已无引用需要保护。
        Ok(None) => continue,
        Err(err) => {
          log::warn!("reuse floor: manifest 读取失败 name={name} err={err}");
          max_fp_id
        }
      };
      floor = floor.max(candidate);
    }
    // 受限分支有子孙时，继承页必须整体保护：下界取安装水位与子孙实际引用
    // 页的较大者。安装水位覆盖「本进程内创建子分支后父分支继续写」的页；
    // 子孙 manifest 引用页覆盖归档回滚：安装水位被合法降回 `A` 后，`[A, C)`
    // 的继承页仍被子孙 branch / archive manifest 引用，下界不得随安装水位
    // 下降（pooled-11）。不能用子孙块末尾（会越过自己的块上限并把分配水位
    // 推满，pooled-08 的死锁形态）。
    if !is_main && has_descendant {
      floor = floor.max(max_fp_id).max(descendant_floor);
    }
    if self.alloc_mode != AllocMode::Pooled {
      // Append + main：把分配水位抬到所有已承诺预留块末尾之后。Append 没有
      // 空闲池回填，不需要维护 `reuse_floor`，只需跳过这段区间（sem-25）。
      if let Some(mapper) = self
        .table_map
        .get_mut(tbl)
        .and_then(|state| state.mapper.as_mut())
      {
        mapper.reserve_until(floor);
      }
      return;
    }
    if let Some(limit) = self.branch_file_limit
      && floor > limit
    {
      log::warn!(
        "reuse floor 越过 branch 上限 tbl={tbl} branch={branch} floor={floor} limit={limit}"
      );
      floor = limit;
    }
    self.raise_reuse_floor(tbl, floor);
  }
}
