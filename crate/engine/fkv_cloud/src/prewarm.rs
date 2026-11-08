//! 预热与本地 cache 恢复：manifest / data / segment 安装及失效。

use bytes::Bytes;
use fkv_error::{KvError, Result};
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_object::ObjectStore;
use fkv_types::{FileId, MAIN_BRANCH_NAME, TableIdent, manifest_file_name};

use crate::{CloudStore, segment::LocalSegmentState};

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 按表懒重建本地 cache 记账（对齐 C++ `RestoreLocalCacheState`）。
  ///
  /// 冷启动后旧进程留下的 data / segment 仍在盘上：不重建记账会让它们既不计预算
  /// 也不参与 LRU，预热还会因为内存态 miss 而重复下载。`allow_reuse_local_caches
  /// == false` 时反向清理该表的本地 data / segment，只从远端重建。
  ///
  /// 同一实例内每表只扫描一次；新写入走正常路径，不重复登记。
  pub async fn restore_table_cache(&self, tbl: &TableIdent) -> Result<()> {
    if self.restored_table_set.borrow().contains(tbl) {
      return Ok(());
    }
    if !self.reuse_local_caches {
      self.invalidate_local_file_li(tbl).await?;
      self.sync_cache_bytes();
      self.restored_table_set.borrow_mut().insert(tbl.clone());
      return Ok(());
    }
    // 冷启动扫盘时还没 replay，拿不到 manifest 的 per-file 归属链：内容归属
    // 记 `None`（未知），由随后 `set_file_range_li` 的 `adopt_unknown_ident`
    // 按归属链补记。直接记当前 identity 会让 term 抬升后的旧文件全部误判
    // miss（io-12）。
    for file_id in self.local.list_file_id_li(tbl).await? {
      if let Some(size) = self.local.file_size(tbl, file_id).await? {
        self.upsert_resident(tbl, file_id, size, false, None);
      }
    }
    for file_id in self.local.list_segment_file_id_li(tbl).await? {
      if self.segment_file_complete(tbl, file_id).await? {
        self.upsert_segment_resident(tbl, file_id, self.segment_file_bytes()?, false, None);
      }
    }
    self.trim_cache();
    self.sync_cache_bytes();
    self.restored_table_set.borrow_mut().insert(tbl.clone());
    Ok(())
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 预热的磁盘余量判定前，把本地**物理已存在**的文件登记进记账。
  ///
  /// 与 [`Self::restore_table_cache`] 的区别：不做整表扫描、不删任何文件；
  /// 只认调用方已经列举到的文件，用于覆盖「预热期间 shard 新落盘」的窗口。
  /// data 文件按本地实际长度登记（短文件也占磁盘）；segment 只登记完整文件。
  pub async fn adopt_local_file(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    segment: bool,
    expected_size: u64,
  ) -> Result<bool> {
    if self.is_file_resident(tbl, file_id, segment) {
      return Ok(true);
    }
    if segment {
      // 只做单文件完整性判定（读尾槽，O(1)）：预热对每个 segment 文件都会调
      // 本函数，再列一次整表 segment id 会让大批 segment 表退化成 O(N²)
      // 目录扫描。
      if !self.segment_file_complete(tbl, file_id).await? {
        return Ok(false);
      }
      // 文件由另一实例（shard）写好：只借用记账，驱逐时不能 unlink。
      self
        .foreign_file_set
        .borrow_mut()
        .insert((tbl.clone(), true, file_id));
      self.upsert_segment_resident(
        tbl,
        file_id,
        expected_size.max(self.segment_file_bytes()?),
        false,
        Some(self.attr_ident(tbl, file_id, true)),
      );
      return Ok(true);
    }
    let Some(size) = self.local.file_size(tbl, file_id).await? else {
      return Ok(false);
    };
    // 借用 shard 已写文件：驱逐只注销记账，不删共享路径。
    self
      .foreign_file_set
      .borrow_mut()
      .insert((tbl.clone(), false, file_id));
    self.upsert_resident(
      tbl,
      file_id,
      size,
      false,
      Some(self.attr_ident(tbl, file_id, false)),
    );
    // 短文件只登记实际占用，不当作预热命中（是否回源由调用方决定，预热不覆写）。
    Ok(size >= expected_size)
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 从 manifest 内容采纳 branch metadata 的 per-file 归属链。
  ///
  /// 预热必须先有这条链，才能把父 branch / 旧 term 引用的 data / segment
  /// 也算进目标集合（见 [`Self::remote_file_entry_page`]）。
  pub(crate) fn adopt_manifest_range_li(&self, tbl: &TableIdent, bytes: &[u8]) -> Result<()> {
    let (state, _) =
      fkv_task_reopen::replay_manifest_bytes_with_len(bytes).map_err(|_| KvError::Corrupted)?;
    self.set_file_range(tbl, &state.branch_metadata.file_range_li);
    Ok(())
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 确保当前 attach identity 的 manifest 已在本地；返回是否存在。
  ///
  /// 选择口径与 [`Self::latest_remote_manifest_bytes`] 一致：同 branch 最新无 tag
  /// manifest（不按 identity term 精确匹配——远端权威可能仍在旧 term 名下）。
  /// main 装成本地活跃 manifest，branch 装成本地具名文件
  /// `manifest_<branch>_<identity term>`（sem-60：branch 活跃 manifest 名带
  /// attach term，与 `Store::open_branch` 的读取 / 首写发布名一致）。装完立即
  /// 解析 branch metadata 并同步 `file_range_li`，后续分页预热才能覆盖父
  /// branch / 旧 term 文件。
  ///
  /// 本地已有时**仍要取远端最新 term 比对内容**：重启后本地 manifest 可能是
  /// 旧 term 留下的，只按它采纳 `file_range_li` 会漏预热新 term 引用的文件。
  /// 内容不同则以远端为准重装本地（只写本地，不触发上传 / flush）。
  pub async fn prewarm_manifest(&self, tbl: &TableIdent) -> Result<bool> {
    // `Ref` 不跨 await 持有。
    let (branch, term) = {
      let id = self.identity();
      (id.branch.clone(), id.term)
    };
    let branch_manifest = (branch != MAIN_BRANCH_NAME).then(|| manifest_file_name(&branch, term));
    let local_bytes = match &branch_manifest {
      Some(name) => self.local.read_manifest_file(tbl, name).await?,
      None => self.local.read_manifest(tbl).await?,
    }
    .map(Bytes::from);
    // 远端最新无 tag manifest（`latest_remote_manifest` 已按当前 branch 过滤，
    // 不再重复 filter）。
    let remote_bytes = self
      .latest_remote_manifest(tbl)
      .await?
      .map(|(_, bytes)| bytes);
    let bytes = match (local_bytes, remote_bytes) {
      (Some(local), Some(remote)) => {
        if self.owner.is_publishing(tbl) {
          // shard 正持有本地新快照发布中：远端还是旧内容，覆盖本地会让发布
          // 成功后远端被“旧快照 + log”替换。本轮预热跳过。
          return Ok(false);
        }
        let local_slice: &[u8] = local.as_ref();
        let remote_slice: &[u8] = remote.as_ref();
        if local_slice.len() > remote_slice.len() && local_slice.starts_with(remote_slice) {
          // 本地是远端的前缀扩展：shard 刚 append 成功、manifest 上传尚在
          // 重试窗口。prewarm 与 shard 共享 local_root，此时用远端覆盖会把
          // 已 ack 的 record 抹掉；保留本地，等上传完成 / 失败回滚。
          local
        } else if local_slice != remote_slice {
          // 旧 term / 内容漂移：以远端最新为准重装，再按新内容采纳区间。
          self
            .install_local_manifest(tbl, branch_manifest.as_deref(), &remote)
            .await?;
          remote
        } else {
          local
        }
      }
      // 远端没有 manifest：发布进行中时本地新快照还没到远端，不能删；否则
      // 本地只是 stale cache，清掉避免 Drop 后预热把整表重新拉回。
      (Some(_), None) => {
        if self.owner.is_publishing(tbl) {
          return Ok(false);
        }
        match &branch_manifest {
          Some(name) => self.local.remove_manifest_file(tbl, name).await?,
          None => self.local.drop_manifest(tbl).await?,
        }
        // 旧 manifest 的 per-file 归属链同样作废：留着会让后续分页预热按旧区间
        // 把已 Drop 的文件重新拉回。
        self.file_range_map.borrow_mut().remove(tbl);
        return Ok(false);
      }
      (None, Some(remote)) => {
        self
          .install_local_manifest(tbl, branch_manifest.as_deref(), &remote)
          .await?;
        remote
      }
      (None, None) => {
        self.file_range_map.borrow_mut().remove(tbl);
        return Ok(false);
      }
    };
    self.adopt_manifest_range_li(tbl, &bytes)?;
    Ok(true)
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 只写本地 manifest（不 flush / 不上传）：main 落活跃名，branch 落
  /// `manifest_<branch>_<identity term>`。预热实例与 shard 共用 `local_root`，
  /// 这里刻意不走 [`ManifestIo::switch_manifest`] 的 flush + 远端发布路径。
  pub(crate) async fn install_local_manifest(
    &self,
    tbl: &TableIdent,
    branch_manifest: Option<&str>,
    bytes: &[u8],
  ) -> Result<()> {
    match branch_manifest {
      Some(name) => self.local.write_manifest_file(tbl, name, bytes).await,
      None => self.local.switch_manifest(tbl, bytes).await,
    }
  }
}

impl<I: ManifestIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 作废该表的本地 data / segment 缓存与 resident 记账（不触碰远端）。
  pub(crate) async fn invalidate_local_file_li(&self, tbl: &TableIdent) -> Result<()> {
    self.forget_table_cache(tbl);
    self
      .foreign_file_set
      .borrow_mut()
      .retain(|(file_tbl, ..)| file_tbl != tbl);
    self.file_range_map.borrow_mut().remove(tbl);
    // 本地文件已作废（或被外部状态替换）：允许下次打开重新扫描登记。
    self.restored_table_set.borrow_mut().remove(tbl);
    for file_id in self.local.list_file_id_li(tbl).await? {
      self.local.remove_file(tbl, file_id).await?;
    }
    for file_id in self.local.list_segment_file_id_li(tbl).await? {
      self.local.remove_segment_file(tbl, file_id).await?;
    }
    // segment 文件已全部删除，旁路标记必须同步清掉：否则 file_id 复用时
    // stale 标记会把新的半装文件判完整，分区目录也回收不掉。
    self.clear_segment_marker_li(tbl).await?;
    Ok(())
  }
}

impl<I: ManifestIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 确保 data 文件已在本地 cache；返回文件是否存在。
  ///
  /// [`Self::read_file`] 本地命中即直接返回，不会用远端对象覆盖本地文件：
  /// 预热实例与 shard 实例共用 `local_root` 时，shard 正在写的脏页不会被
  /// 预热的整文件 tmp+rename 冲掉。
  pub async fn prewarm_data_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<bool> {
    if self.contains(tbl, file_id) {
      return Ok(true);
    }
    Ok(self.read_file(tbl, file_id).await?.is_some())
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 确保 segment 文件已在本地 cache；返回文件是否存在。
  ///
  /// 只安装本地**不存在**的文件。本地已有文件时绝不删除或覆写：
  /// - 完整文件：直接登记 resident；
  /// - 半截文件：可能是另一实例（shard）正在写的段文件，删掉会让它的
  ///   `flush_segment_tbl` 组装出零洞。预热只跳过，修复留给该实例的读路径
  ///   （`ensure_segment_resident` / `read_segment`）。
  ///
  /// 跨实例共享 `local_root` 时没有写锁，因此安装前必须先在共享组里抢占
  /// 独占权（[`Self::begin_segment_install`]）：同组另一实例正在写该文件时
  /// 返回 `TryAgain`，预热的有界重试会在对方写完后再来，绝不覆盖它的脏段。
  pub async fn prewarm_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<bool> {
    if self.segment_contains(tbl, file_id) {
      return Ok(true);
    }
    // 下载前先抢占独占权：回填权（scheme 启用）与在飞登记都在 `remote.get`
    // 之前完成，抢不到时不会白下载整个对象。
    let claim = self.begin_segment_install(tbl, file_id).await?;
    // 单文件尾槽判定，O(1)；本地已有（含半截）文件一律跳过，不抢 shard 的写。
    // 判定在独占权保护下重做一次：上面的 `segment_contains` 与判定之间对方
    // 可能已经写完，旧判定结果不能再用。
    match self.segment_state(tbl, file_id).await? {
      LocalSegmentState::Complete => {
        // 本地已有：可能是 shard 正在写的文件，标记为借用避免本实例驱逐 unlink。
        self
          .foreign_file_set
          .borrow_mut()
          .insert((tbl.clone(), true, file_id));
        self.upsert_segment_resident(
          tbl,
          file_id,
          self.segment_file_bytes()?,
          false,
          Some(self.attr_ident(tbl, file_id, true)),
        );
        return Ok(true);
      }
      LocalSegmentState::Incomplete => return Ok(true),
      LocalSegmentState::Missing => {}
    }
    // epoch 必须在 `remote.get` **之前**取：get 期间表可能被 `evict_table`
    // 驱逐，若 get 之后才取值，install 会把已 detach 的表重新登记（复活）。
    let epoch = self.evict_epoch.get();
    let Some(bytes) = self.remote.get(&self.segment_key(tbl, file_id)).await? else {
      return Ok(false);
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
    self.trim_cache();
    self.drain_evictions_if_due().await;
    Ok(true)
  }
}
