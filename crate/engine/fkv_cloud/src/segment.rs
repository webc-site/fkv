//! segment 文件 scheme、完成标记、完整性判定与安装路径。

use fkv_error::{KvError, Result};
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_object::ObjectStore;
use fkv_types::{FileId, TableIdent};

use crate::{CloudStore, download::DownloadOutcome, group::SegmentInflightGuard};

/// segment 文件完成标记的文件名前缀（本地 cache 侧旁路元数据）。
///
/// 名字不以 `manifest_` 开头，本地后端的 manifest 列举与
/// `parse_manifest_file_name` 都不会把它当成 manifest；它只是复用
/// `ManifestIo::write_manifest_file` 的原子整写能力落一个旁路文件。
/// 标记是**可选**的：旧目录没有它时继续按尾槽规则判定，on-disk 兼容。
pub(crate) const SEGMENT_MARKER_PREFIX: &str = "segcomplete";
/// 完成标记内容：固定魔数，便于识别脏标记与将来演进。
pub(crate) const SEGMENT_MARKER_MAGIC: &[u8] = b"fkv-segcomplete-v1";

/// 某 segment 文件的完成标记名。
pub(crate) fn segment_marker_name(file_id: FileId) -> String {
  format!("{SEGMENT_MARKER_PREFIX}_{file_id}")
}

/// 是否为 segment 完成标记（表级 scheme 标记或单文件标记）。
///
/// 这些名字不是 manifest，`list_manifest_file_name_li` 合并本地 cache 时要滤掉。
pub(crate) fn is_segment_marker_name(name: &str) -> bool {
  name.starts_with(SEGMENT_MARKER_PREFIX)
}

/// 本地 segment 文件的完整状态（只读尾槽 + 标记，O(1)）。
///
/// `Missing` 与 `Incomplete` 必须分开：前者直接回源，后者要先删掉崩溃残留，
/// 否则整文件覆盖会在中段留零洞；混成一个 `false` 会多一次无谓的删目录索引
/// 刷新（compio `remove_segment_file` 会走 `variant_path_li`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalSegmentState {
  Missing,
  Incomplete,
  Complete,
}

impl<I: SegmentIo, O> CloudStore<I, O> {
  /// 每个 segment 文件的定长槽位数。
  pub(crate) fn segment_slot_num(&self) -> Result<usize> {
    if self.segment_size == 0 || self.segments_per_file_shift >= 63 {
      return Err(KvError::InvalidArgs);
    }
    1usize
      .checked_shl(u32::from(self.segments_per_file_shift))
      .ok_or(KvError::InvalidArgs)
  }
}

impl<I: SegmentIo, O> CloudStore<I, O> {
  /// segment 文件名义字节数（按定长槽位计，与本地后端 preallocate 一致）。
  pub(crate) fn segment_file_bytes(&self) -> Result<u64> {
    Ok(self.segment_slot_num()? as u64 * self.segment_size as u64)
  }
}

impl<I: SegmentIo, O> CloudStore<I, O> {
  pub(crate) async fn assemble_segment_file(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
  ) -> Result<Vec<u8>> {
    let slot_num = self.segment_slot_num()?;
    let size = self.segment_size;
    let mut bytes = vec![0u8; slot_num * size];
    let base = file_id << self.segments_per_file_shift;
    // 直接读进目标切片：缺槽保持零，不再每槽分配一份临时缓冲（原实现
    // slot_num 次分配 + 2x 峰值内存）。
    for (index, chunk) in bytes.chunks_exact_mut(size).enumerate() {
      match self
        .local
        .read_segment(tbl, base + index as u64, chunk)
        .await
      {
        Ok(()) => {}
        Err(KvError::ResourceMissing) => chunk.fill(0),
        Err(err) => return Err(err),
      }
    }
    Ok(bytes)
  }
}

impl<I: ManifestIo, O> CloudStore<I, O> {
  /// 本表是否已启用显式完成标记；「存在」按表缓存一次（见 `segment_scheme_set`）。
  pub(crate) async fn segment_scheme_enabled(&self, tbl: &TableIdent) -> Result<bool> {
    if self.segment_scheme_set.borrow().contains(tbl) {
      return Ok(true);
    }
    let present = self
      .segment_marker_present(tbl, SEGMENT_MARKER_PREFIX)
      .await?;
    if present {
      self.segment_scheme_set.borrow_mut().insert(tbl.clone());
    }
    Ok(present)
  }
}

impl<I: ManifestIo, O> CloudStore<I, O> {
  /// 某旁路标记文件是否存在且内容为约定魔数（脏 / 截断标记一律不算数）。
  pub(crate) async fn segment_marker_present(&self, tbl: &TableIdent, name: &str) -> Result<bool> {
    Ok(
      self
        .local
        .read_manifest_file(tbl, name)
        .await?
        .is_some_and(|bytes| bytes.as_slice() == SEGMENT_MARKER_MAGIC),
    )
  }
}

impl<I: ManifestIo, O> CloudStore<I, O> {
  /// 写某 segment 文件的完成标记（槽位全部落盘后调用）。
  pub(crate) async fn write_segment_marker(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self
      .local
      .write_manifest_file(tbl, &segment_marker_name(file_id), SEGMENT_MARKER_MAGIC)
      .await
  }
}

impl<I: ManifestIo, O> CloudStore<I, O> {
  /// 删某 segment 文件的完成标记（删文件 / 变体时连带调用，幂等）。
  ///
  /// 标记只落本地（从不随 manifest 上传），走 `local` 的旁路删除接口，
  /// 不触达远端对象。
  pub(crate) async fn remove_segment_marker(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
  ) -> Result<()> {
    self
      .local
      .remove_sidecar_file_name(tbl, &segment_marker_name(file_id))
      .await
  }
}

impl<I: ManifestIo, O> CloudStore<I, O> {
  /// 清掉本表全部 segment 旁路标记（表级 scheme + 单文件标记）。
  ///
  /// `drop` / `invalidate` 必须调用：残留标记会让同 `file_id` 复用时半装文件
  /// 被 stale 标记判成完整；分区目录也会因标记文件永远无法回收。
  pub(crate) async fn clear_segment_marker_li(&self, tbl: &TableIdent) -> Result<()> {
    self.segment_scheme_set.borrow_mut().remove(tbl);
    // 标记不匹配 manifest 命名规则，`list_manifest_file_name_li` 看不到，
    // 必须走旁路文件列举（compio 按 manifest 前缀过滤了 manifest 名单）。
    // 标记只在本地：走 `local` 的旁路接口，不触达远端对象。
    for name in self.local.list_sidecar_file_name_li(tbl).await? {
      if is_segment_marker_name(&name) {
        self.local.remove_sidecar_file_name(tbl, &name).await?;
      }
    }
    Ok(())
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 按读路径 key（`segment_key`）单飞取整文件。
  pub(crate) async fn fetch_segment_file(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
  ) -> Result<DownloadOutcome> {
    let key = (tbl.clone(), true, file_id);
    self
      .single_flight(key, || async {
        // 回填权 / 组内在飞登记前移到 **下载之前**：抢不到时立刻失败，
        // 不会先白下载整个对象再丢弃。
        let claim = self.begin_segment_install(tbl, file_id).await?;
        let epoch = self.evict_epoch.get();
        let Some(bytes) = self.remote.get(&self.segment_key(tbl, file_id)).await? else {
          return Ok(DownloadOutcome::Missing);
        };
        self
          .install_claimed(
            tbl,
            file_id,
            &bytes,
            epoch,
            self.attr_ident(tbl, file_id, true),
          )
          .await?;
        drop(claim);
        Ok(DownloadOutcome::Cached)
      })
      .await
      .map(|(outcome, _)| outcome)
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 安装前的组内独占权：登记在飞 + 判定他人 busy + 确保表级 scheme 已启用。
  ///
  /// 顺序要求：**先**抢回填权 / 登记，**再**下载整对象。回填权落到
  /// `remote.get` 之后才争取的话，抢不到时已经付过一次整文件下载的代价
  /// （压缩的 8 路批量会因此整批 abort）。
  ///
  /// 登记与「他人是否在飞 / 持脏」判定在同一把锁内完成：同组另一实例正在写
  /// 该文件时返回 `TryAgain`，绝不把对方未上传的写覆盖成远端旧值。
  ///
  /// 只读实例（[`Self::with_read_only`]）不落 scheme：跨进程共用本地 root 时
  /// 看不到写实例的现场，回填会把对方半装文件盖成完整。
  pub(crate) async fn begin_segment_install(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
  ) -> Result<SegmentInflightGuard<'_>> {
    let Some(claim) = SegmentInflightGuard::claim(&self.owner.group, self.owner.id, tbl, file_id)
    else {
      return Err(KvError::TryAgain);
    };
    if !self.read_only && !self.segment_scheme_enabled(tbl).await? {
      self.enable_segment_marker(tbl).await?;
    }
    Ok(claim)
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 「取独占权 + 安装」的合并入口；生产路径把两步拆开、把独占权放到下载
  /// 之前，仅测试直接调用本函数。
  #[cfg(test)]
  pub(crate) async fn install_segment_file(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    bytes: &[u8],
    epoch: u64,
  ) -> Result<()> {
    let claim = self.begin_segment_install(tbl, file_id).await?;
    self
      .install_claimed(
        tbl,
        file_id,
        bytes,
        epoch,
        self.attr_ident(tbl, file_id, true),
      )
      .await?;
    drop(claim);
    Ok(())
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 在已持有 [`SegmentInflightGuard`] 的前提下写槽位并落完成标记。
  ///
  /// 调用方必须已经通过 [`Self::begin_segment_install`] 取得独占权：回填权
  /// 与组内在飞登记都已就位，这里只做复核与落盘。`ident` 是 `bytes` 对应的
  /// 远端对象写时 `(branch, term)`，随记账保存供读命中校验（io-12）。
  pub(crate) async fn install_claimed(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    bytes: &[u8],
    epoch: u64,
    ident: (String, u64),
  ) -> Result<()> {
    let slot_num = self.segment_slot_num()?;
    let file_bytes = slot_num * self.segment_size;
    // 短对象不能零补齐伪装成合法文件：缺槽说明上传端 / 远端对象损坏。
    if bytes.len() != file_bytes {
      return Err(KvError::Corrupted);
    }
    if self.evict_epoch.get() != epoch {
      return Err(KvError::TryAgain);
    }
    // 下载期间同组另一实例可能才开始写同一文件：**写槽前复核**，绝不把它
    // 正在写的脏段覆盖成远端旧值（否则它 flush 后会把旧值上传成权威）。
    if self.owner.segment_foreign_busy(self.owner.id, tbl, file_id) {
      return Err(KvError::TryAgain);
    }
    let base = file_id << self.segments_per_file_shift;
    // 重新装同一文件前先摘掉旧标记：半装期间不得留着上一轮的成功标记。
    self
      .local
      .remove_sidecar_file_name(tbl, &segment_marker_name(file_id))
      .await?;
    // 逐段写不是原子的：中途失败 / 崩溃会留下「前半段有数据、尾段空洞」的
    // 文件。因此失败时删文件，且调用方必须用 `segment_file_complete` 复核
    // 完成标记，不能只看存在性 / 长度。
    for index in 0..slot_num {
      let start = index * self.segment_size;
      if let Err(err) = self
        .local
        .write_segment(
          tbl,
          base + index as u64,
          &bytes[start..start + self.segment_size],
        )
        .await
      {
        let _ = self.local.remove_segment_file(tbl, file_id).await;
        self.forget_segment(tbl, file_id);
        self.sync_cache_bytes();
        return Err(err);
      }
    }
    // 下载期间表被驱逐：落盘的分段必须撤掉，不能登记进已 detach 的表。
    if self.evict_epoch.get() != epoch {
      let _ = self.local.remove_segment_file(tbl, file_id).await;
      self.forget_segment(tbl, file_id);
      self.sync_cache_bytes();
      return Err(KvError::TryAgain);
    }
    // 槽位写只进页缓存：完成标记是「数据已 durable」的承诺，必须先把本表
    // 段数据 fdatasync 落盘再写标记。否则崩溃后标记在、段数据未落盘（定长
    // 后端尾槽可读零），`restore_table_cache` 按标记判完整并登记 clean，读
    // 本地零洞，后续 flush 再把零上传成云端权威。
    self.local.sync_data(tbl).await?;
    // 全部槽位落盘且表未被驱逐：最后一步写完成标记（原子整写）。
    self.write_segment_marker(tbl, file_id).await?;
    // 槽位已被整文件覆盖：条目已存在时 upsert 保留原归属，必须显式改写，
    // 否则读路径仍按旧 identity 命中 / 误判 miss（io-12）。
    self.upsert_segment_resident(tbl, file_id, file_bytes as u64, false, None);
    self.set_ident(tbl, file_id, true, ident);
    Ok(())
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 标记本表使用显式 segment 完成标记（表级 scheme）。
  ///
  /// 只有出现过新格式写入的表，缺失单文件标记才表示「半装」；旧目录没有这个
  /// scheme 标记，继续按尾槽规则判定，保持 on-disk 兼容。
  ///
  /// 首次启用时**先给盘上已完整的旧段回填单文件标记，再落 scheme**：
  /// 顺序反过来的话，旧段在新 scheme 下缺标记会被当半装 —— `restore_table_cache`
  /// 不记账（预算低估、预热误报 `DiskFull`），首次写还要整包重下。回填用旧规则
  /// （尾槽可读即完整），与升级前判定一致，不改变半装文件的既有归类。
  /// 先回填后落 scheme：中途崩溃时盘上仍无 scheme，标记被忽略，行为回到旧目录。
  ///
  /// 并发安全：同一表同一时刻只允许一个任务回填（表级守卫），且回填**跳过**
  /// 任一共享 `CacheGroup` 实例在飞下载 / 脏的 file id——它们正是另一半装
  /// 文件的现场，按旧尾槽规则（定长段后端首写即 `set_len`，尾槽可读零）会把
  /// 半装文件盖成完整，崩溃后留下「新头 + 旧尾」被当成完整读。
  ///
  /// 跨进程边界：`CacheGroup` 只覆盖同一运行时内的实例。多个进程共用同一本地
  /// root 时看不到对方的写现场，这种部署要求**只由该分区的写实例**启用 scheme。
  /// [`CloudStore::with_read_only`] 置位的实例直接跳过回填（只写自己的单文件
  /// 标记），由写实例稍后的回填补齐 scheme。
  pub(crate) async fn enable_segment_marker(&self, tbl: &TableIdent) -> Result<()> {
    if self.read_only {
      return Ok(());
    }
    // 已确认 scheme 存在：直接返回。
    if self.segment_scheme_set.borrow().contains(tbl) {
      return Ok(());
    }
    // 回填权重落组上：同一 `CacheGroup` 内不同实例（shard / 预热）也只允许
    // 一个回填者；否则两边会各自给对方在写的文件盖「完整」标记。
    //
    // 抢不到回填权时必须 `TryAgain`，不能报成功：调用方 `begin_segment_install`
    // 紧接着会删旧标记、写槽位，此刻盘上还没有 scheme，写入变成「无 scheme
    // 的半装文件」，重启后按旧尾槽规则被当完整；同时与持权者的回填互相盖标记。
    let Some(_guard) = self.owner.try_begin_enable(tbl) else {
      return Err(KvError::TryAgain);
    };
    for file_id in self.local.list_segment_file_id_li(tbl).await? {
      if self.segment_backfill_skip(tbl, file_id) {
        continue;
      }
      // 此刻盘上还没有 scheme，回填采用旧规则（首尾槽都可读）。
      if !self.legacy_segment_complete(tbl, file_id).await? {
        continue;
      }
      // 判定期间另一实例可能刚开始 install（登记在入口）：写标记前复核。
      if self.segment_backfill_skip(tbl, file_id) {
        continue;
      }
      self.write_segment_marker(tbl, file_id).await?;
    }
    self
      .local
      .write_manifest_file(tbl, SEGMENT_MARKER_PREFIX, SEGMENT_MARKER_MAGIC)
      .await?;
    self.segment_scheme_set.borrow_mut().insert(tbl.clone());
    Ok(())
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 回填是否跳过该 file id：在飞下载（另一半装现场）或有未上传脏数据。
  ///
  /// 跳过不是漏标记：在飞 install 结束时自己写标记，脏数据由 flush 成功后写。
  pub(crate) fn segment_backfill_skip(&self, tbl: &TableIdent, file_id: FileId) -> bool {
    // 组内登记覆盖同运行时的全部实例（shard / 预热），不只本实例。
    if self.owner.segment_busy(tbl, file_id) {
      return true;
    }
    let inflight = self.inflight_download.borrow();
    if inflight
      .keys()
      .any(|(key_tbl, segment, id)| *segment && *id == file_id && key_tbl == tbl)
    {
      return true;
    }
    drop(inflight);
    self
      .state
      .borrow()
      .table_map
      .get(tbl)
      .and_then(|cache| cache.segment.get(&file_id))
      .is_some_and(|entry| entry.dirty)
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 本地 segment 文件状态（只读尾槽一次，不列举目录）。
  ///
  /// 长度不可判：定长段后端首写即 `set_len` 整个文件（尾部补零），半装文件的
  /// 尾槽同样「可读」，读到的零与真实零数据无法区分 —— 只看尾槽会把半装文件
  /// 永久判成完整，之后读零、flush 再把零上传成云端权威。因此以显式完成标记
  /// 为准：
  ///
  /// 1. 首尾槽都不可读 → 文件缺失 `Missing`；
  /// 2. 尾槽不可读但首槽可读 → 截断 `Incomplete`；
  /// 3. 本表未启用标记（旧目录）→ 尾槽可读即 `Complete`，保持兼容；
  /// 4. 本表已启用标记：有单文件标记 → `Complete`，缺标记 → `Incomplete`。
  pub(crate) async fn segment_state(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
  ) -> Result<LocalSegmentState> {
    let slot_num = self.segment_slot_num()?;
    // 尾槽可读即「文件存在且按旧规则完整」：旧目录的判定口径必须保持不变，
    // 只写了尾槽的稀疏文件在升级前就是完整，不能因本次改动变成半装。
    if self
      .segment_slot_readable(tbl, file_id, slot_num - 1)
      .await?
    {
      if !self.segment_scheme_enabled(tbl).await? {
        return Ok(LocalSegmentState::Complete);
      }
      return Ok(
        if self
          .segment_marker_present(tbl, &segment_marker_name(file_id))
          .await?
        {
          LocalSegmentState::Complete
        } else {
          LocalSegmentState::Incomplete
        },
      );
    }
    // 尾槽不可读：首槽可读说明文件存在但被截断（崩溃残留），要删掉重装；
    // 两个槽都不可读按缺失处理，直接回源。
    if slot_num > 1 && self.segment_slot_readable(tbl, file_id, 0).await? {
      return Ok(LocalSegmentState::Incomplete);
    }
    Ok(LocalSegmentState::Missing)
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 旧规则下的完整判定：尾槽可读即完整（未启用 scheme 时使用）。
  pub(crate) async fn legacy_segment_complete(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
  ) -> Result<bool> {
    let slot_num = self.segment_slot_num()?;
    self.segment_slot_readable(tbl, file_id, slot_num - 1).await
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 某段槽位是否可读；只读一个段，不列举目录。
  pub(crate) async fn segment_slot_readable(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    index: usize,
  ) -> Result<bool> {
    let mut buf = vec![0u8; self.segment_size];
    let segment_fp_id = (file_id << self.segments_per_file_shift) + index as u64;
    match self.local.read_segment(tbl, segment_fp_id, &mut buf).await {
      Ok(()) => Ok(true),
      Err(KvError::ResourceMissing | KvError::IoFail | KvError::EndOfFile) => Ok(false),
      Err(err) => Err(err),
    }
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 本地 segment 文件是否完整。
  pub(crate) async fn segment_file_complete(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
  ) -> Result<bool> {
    Ok(self.segment_state(tbl, file_id).await? == LocalSegmentState::Complete)
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 本地 segment 文件不完整时删掉（连同完成标记），交给调用方按 cache miss 回源。
  pub(crate) async fn drop_incomplete_segment(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
  ) -> Result<()> {
    self.local.remove_segment_file(tbl, file_id).await?;
    self.remove_segment_marker(tbl, file_id).await?;
    self.forget_segment(tbl, file_id);
    self.sync_cache_bytes();
    Ok(())
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 写 segment 前保证整文件在本地：否则只写新段会把同文件旧段冲成空洞。
  pub(crate) async fn ensure_segment_resident(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
  ) -> Result<()> {
    if self.segment_contains(tbl, file_id) {
      return Ok(());
    }
    let key = (tbl.clone(), true, file_id);
    self
      .single_flight(key, || async {
        // 回填权与组内在飞登记在 **下载 / 状态判定之前** 取得：判定本身有
        // await，之后再抢权会留下「判完即被对方开写」的覆盖窗口。
        let claim = self.begin_segment_install(tbl, file_id).await?;
        let epoch = self.evict_epoch.get();
        if self.segment_contains(tbl, file_id) {
          return Ok(DownloadOutcome::Cached);
        }
        // 单文件尾槽判定，O(1)：每次新段都列整表 segment id 会让压缩 / 新写
        // 退化成 O(N²)。
        match self.segment_state(tbl, file_id).await? {
          LocalSegmentState::Complete => {
            // 判定 await 期间表可能已被驱逐：已驱逐的表不得重新登记。
            if self.evict_epoch.get() != epoch {
              return Err(KvError::TryAgain);
            }
            self.upsert_segment_resident(
              tbl,
              file_id,
              self.segment_file_bytes()?,
              false,
              Some(self.attr_ident(tbl, file_id, true)),
            );
            return Ok(DownloadOutcome::Cached);
          }
          // 崩溃残留的半截文件：删掉后按 cache miss 重新拉整文件。
          LocalSegmentState::Incomplete => self.drop_incomplete_segment(tbl, file_id).await?,
          LocalSegmentState::Missing => {}
        }
        let Some(bytes) = self
          .remote
          .get(&self.write_segment_key(tbl, file_id))
          .await?
        else {
          return Ok(DownloadOutcome::Missing);
        };
        self
          .install_claimed(tbl, file_id, &bytes, epoch, self.current_ident())
          .await?;
        drop(claim);
        Ok(DownloadOutcome::Cached)
      })
      .await
      .map(|_| ())
  }
}
