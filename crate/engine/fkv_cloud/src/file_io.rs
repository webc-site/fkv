//! `FileIo` 实现、data 文件回源与表级 flush。

use bytes::Bytes;
use fkv_error::{KvError, Result};
use fkv_io::{FileIo, FileVariant, ManifestIo, PageIo, SegmentIo};
use fkv_object::ObjectStore;
use fkv_types::{FileId, TableIdent};
use gxhash::HashSet;

use crate::{CloudStore, download::DownloadOutcome};

impl<I: FileIo + ManifestIo, O: ObjectStore> CloudStore<I, O> {
  /// 写页前保证整文件已在本地：优先信任本地 cache，否则从远端拉整文件。
  ///
  /// `start` 是本次待写页的**起点**偏移。本地文件短于起点说明写到文件内部
  /// 而本地副本缺前段（崩溃残留的半截文件）：直接写入会在中段留下零洞，必须
  /// 删掉后回源重拉。`start <= size` 都放行：起点之后是正常扩展（含追加到
  /// EOF），远端本来也没有这些页，回源只会白拉整个文件。
  ///
  /// 快路径必须比较**记账长度**而不是只 `contains`：冷启动
  /// [`Self::restore_table_cache`] 会把崩溃残留的短文件按实际长度登记为
  /// resident，只判存在会把短文件当完整副本，越界写留零洞；prewarm 也因
  /// 「本地已 resident」不会修。
  ///
  /// 单飞 + 起点复核：等到别的 owner 的 `Cached` 时只保证覆盖**它自己的**
  /// 起点（快路径「本地文件已存在即返回」甚至可能一次回源都没做）。
  /// 因此不能按轮次决定是否回源：只有**本任务作为 owner 且执行了回源**之后，
  /// 才允许「远端确实短于起点 → 按扩展写放行」；否则自己再发起一轮回源。
  pub(crate) async fn ensure_resident(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    start: u64,
  ) -> Result<()> {
    let key = (tbl.clone(), false, file_id);
    // 是否已经由本任务完成过一次真正的回源（短文件删除 + 远端整文件覆盖）。
    let mut refetched = false;
    // 最多三轮：等待者轮 / 本方回源轮 / 远端确认短于起点后的放行轮。
    for _ in 0..3 {
      if self.resident_covers(tbl, file_id, start) {
        return Ok(());
      }
      // 未完成强制回源前，每一轮都以回源语义执行；完成后才允许短文件放行。
      let refetch = !refetched;
      let (outcome, owner) = self
        .single_flight(key.clone(), || async {
          let epoch = self.evict_epoch.get();
          // 本地短于写起点：记下「需要先删本地」，但**推迟到远端确认存在之后**。
          let mut short_local = false;
          match self.local.file_size(tbl, file_id).await? {
            Some(size) if size >= start => {
              // `file_size` await 期间表可能已被驱逐：已驱逐的表不得重新登记，
              // 否则 reopen 会把本地残留当权威。
              if self.evict_epoch.get() != epoch {
                return Err(KvError::TryAgain);
              }
              self.upsert_resident(
                tbl,
                file_id,
                size,
                false,
                Some(self.attr_ident(tbl, file_id, false)),
              );
              return Ok(DownloadOutcome::Cached);
            }
            Some(size) => {
              if self.is_dirty(tbl, file_id) {
                // 脏文件只有本地一份，回源会丢掉未上传的写；短于本次写起点时
                // 只能保留文件交给写路径扩展（远端没有可补齐的版本）。
                log::warn!(
                  "脏文件短于写入起点，跳过回源 tbl={tbl} file_id={file_id} size={size} start={start}"
                );
                return Ok(DownloadOutcome::Cached);
              }
              if !refetch {
                // 远端对象本身短于写起点：按扩展写放行，不再重复整文件下载。
                return Ok(DownloadOutcome::Cached);
              }
              short_local = true;
            }
            None => {
              // 记账存在但本地文件已不在：清掉旧长度，避免快路径按过期值放行。
              if self.contains(tbl, file_id) {
                self.forget(tbl, file_id);
                self.sync_cache_bytes();
              }
              if !refetch {
                return Ok(DownloadOutcome::Missing);
              }
            }
          }
          // 先取远端再删本地：远端 `Missing` 时本地前缀页是唯一副本，提前删除
          // 会让调用方从 `start` 落盘，`[size, start)` 留零洞。
          let Some(bytes) = self.remote.get(&self.write_file_key(tbl, file_id)).await? else {
            return Ok(DownloadOutcome::Missing);
          };
          if short_local {
            // 远端已有完整副本：本地短文件是崩溃残留，删掉后整文件覆盖，
            // 否则整写同样会在中段留下零洞。
            self.local.remove_file(tbl, file_id).await?;
            self.forget(tbl, file_id);
            self.sync_cache_bytes();
          }
          self
            .cache_local_file(tbl, file_id, &bytes, epoch, self.current_ident())
            .await?;
          Ok(DownloadOutcome::Cached)
        })
        .await?;
      // 脏文件无法从远端补齐（会丢未上传写），第二轮没有意义。
      if outcome == DownloadOutcome::Missing || self.is_dirty(tbl, file_id) {
        return Ok(());
      }
      // 等待者的结果不作数：本方还没回源，下一轮继续强制回源。
      refetched |= owner && refetch;
    }
    // 三轮都只是当 waiter、期间没有任何一次真正的回源：不能报成功，否则写路径
    // 会在中段留下零洞。远端确认短于起点（`refetched`）时按扩展写放行。
    if !refetched && !self.resident_covers(tbl, file_id, start) {
      return Err(KvError::TryAgain);
    }
    Ok(())
  }
}

impl<I: FileIo + ManifestIo, O: ObjectStore> CloudStore<I, O> {
  /// 远端整文件回填本地 cache：后端负责 tmp + rename 原子落盘，失败时删掉
  /// 可能留下的短文件。
  ///
  /// CompioStore 的整写走 tmp + rename，崩溃只会留下 `.tmp`；其它后端若
  /// 非原子，失败 / 崩溃可能留下 `file_size().is_some()` 的短文件，这里的
  /// 长度复核与 [`Self::ensure_resident`] 的起点判定会把它当作 miss。
  ///
  /// `epoch` 是下载开始前的 [`Self::evict_epoch`]：表在下载期间被驱逐时，
  /// 落盘结果与记账都必须撤销，否则已 detach / Drop 的表会复活。
  ///
  /// `ident` 是这份字节对应的远端对象写时 `(branch, term)`：读路径按
  /// manifest 的 per-file 归属回源，写前补齐按当前 identity。它随记账保存，
  /// 读命中前据此校验，跨 term 重开归档时另一 term 的同 id 副本不能命中
  /// （io-12）。
  pub(crate) async fn cache_local_file(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    bytes: &[u8],
    epoch: u64,
    ident: (String, u64),
  ) -> Result<()> {
    if self.evict_epoch.get() != epoch {
      return Err(KvError::TryAgain);
    }
    if let Err(err) = self.local.write_file(tbl, file_id, bytes).await {
      let _ = self.local.remove_file(tbl, file_id).await;
      self.forget(tbl, file_id);
      self.sync_cache_bytes();
      return Err(err);
    }
    let len = bytes.len() as u64;
    // 后端若未实现原子写，至少拒绝短写 / 未落盘：`file_size` 为 `None`
    // （写成功但文件不存在）同样按失败处理，绝不能按 `bytes.len()` 登记为
    // 完整 resident —— 否则后续读写不再回源，直到进程重启。
    let size = self.local.file_size(tbl, file_id).await?;
    let err = if self.evict_epoch.get() != epoch {
      Some(KvError::TryAgain)
    } else if size == Some(len) {
      None
    } else {
      Some(KvError::IoFail)
    };
    if let Some(err) = err {
      let _ = self.local.remove_file(tbl, file_id).await;
      self.forget(tbl, file_id);
      self.sync_cache_bytes();
      return Err(err);
    }
    // 本地字节刚被整文件覆盖：条目已存在时 upsert 保留原归属，必须显式改写。
    self.upsert_resident(tbl, file_id, len, false, None);
    self.set_ident(tbl, file_id, false, ident);
    Ok(())
  }
}

impl<I: FileIo + ManifestIo, O: ObjectStore> CloudStore<I, O> {
  /// 按读路径 key（`file_key`，含区间归属的旧 term）单飞取整文件。
  pub(crate) async fn fetch_data_file(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
  ) -> Result<DownloadOutcome> {
    let key = (tbl.clone(), false, file_id);
    self
      .single_flight(key, || async {
        let epoch = self.evict_epoch.get();
        // owner 可能已被前一个等待者取代；先看本地是否已装入。
        // 本地副本只有在归属与 manifest 一致时才算「已缓存」：跨 term 重开
        // 归档时同 file id 的旧副本必须重新回源覆盖（io-12）。
        if self.local_ident_ok(tbl, file_id, false)
          && self.local.file_size(tbl, file_id).await?.is_some()
        {
          return Ok(DownloadOutcome::Cached);
        }
        let Some(bytes) = self.remote.get(&self.file_key(tbl, file_id)).await? else {
          return Ok(DownloadOutcome::Missing);
        };
        self
          .cache_local_file(
            tbl,
            file_id,
            &bytes,
            epoch,
            self.attr_ident(tbl, file_id, false),
          )
          .await?;
        Ok(DownloadOutcome::Cached)
      })
      .await
      .map(|(outcome, _)| outcome)
  }
}

impl<I: FileIo + ManifestIo, O: ObjectStore> CloudStore<I, O> {
  /// 把某个 table 的 dirty data 文件整文件上传。
  pub(crate) async fn flush_tbl(&self, tbl: &TableIdent) -> Result<()> {
    for file_id in self.dirty_file_li(tbl) {
      let Some(bytes) = self.local.read_file(tbl, file_id).await? else {
        // 本地文件已不在：脏标记永远无法完成上传，继续循环会卡死整表 flush。
        // 丢弃该条目并告警，其余表 / 条目照常处理。
        log::warn!("flush 跳过本地缺失的脏文件 tbl={tbl} file_id={file_id}");
        self.forget(tbl, file_id);
        self.sync_cache_bytes();
        continue;
      };
      // `Vec<u8> -> Bytes` 零拷贝；后续整对象上传 / 区间切片都只借用同一块内存。
      let bytes = Bytes::from(bytes);
      let len = bytes.len() as u64;
      let key = self.write_file_key(tbl, file_id);
      let ident = {
        let id = self.identity();
        (id.branch.clone(), id.term)
      };
      let range_li = self.take_dirty_range_li(tbl, file_id, &ident);
      if range_li.as_ref().is_none_or(|range_li| range_li.is_empty())
        || !self.remote.supports_put_range()
      {
        // 无区间信息 / identity 变更 / 后端不支持区间写：整对象一次上传
        // （对齐 C++ 每文件一次 PUT）。
        if let Err(err) = self.remote.put(&key, bytes, None, false).await {
          // 区间已从记账摘除，失败必须归还，否则后续 flush 以为无脏数据。
          self.restore_dirty_range_li(tbl, file_id, range_li);
          return Err(err);
        }
      } else {
        let range_li = range_li.unwrap_or_default();
        let mut covered = 0usize;
        let mut uncovered_range_li = Vec::new();
        for (index, (start, range_len)) in range_li.iter().enumerate() {
          let start_usize = *start as usize;
          let end = start_usize
            .saturating_add(*range_len as usize)
            .min(bytes.len());
          if start_usize >= end {
            uncovered_range_li.push((*start, *range_len));
            continue;
          }
          if let Err(err) = self
            .remote
            .put_range(&key, start_usize as u64, &bytes[start_usize..end])
            .await
          {
            uncovered_range_li.push((*start, *range_len));
            uncovered_range_li.extend(range_li[index + 1..].iter().copied());
            self.restore_dirty_range_li(tbl, file_id, Some(uncovered_range_li));
            return Err(err);
          }
          covered += end - start_usize;
        }
        if covered == 0 {
          // 区间全落在文件实际长度之外：整对象上传已覆盖全部数据。
          if let Err(err) = self.remote.put(&key, bytes, None, false).await {
            self.restore_dirty_range_li(tbl, file_id, None);
            return Err(err);
          }
        } else if !uncovered_range_li.is_empty() {
          // 有区间没上传成功（越界）：不能标 clean，归还后下轮整写兜底。
          self.restore_dirty_range_li(tbl, file_id, Some(uncovered_range_li));
          continue;
        }
      }
      self.mark_clean(tbl, file_id, len);
    }
    self.trim_cache();
    self.drain_evictions().await
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 先 data 后 segment；两者都落 S3 之后才允许发布 manifest。
  pub(crate) async fn flush_all_tbl(&self, tbl: &TableIdent) -> Result<()> {
    self.flush_tbl(tbl).await?;
    self.flush_segment_tbl(tbl).await
  }
}

impl<I: ManifestIo + FileIo, O: ObjectStore> FileIo for CloudStore<I, O> {
  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    let mut file_id_set = HashSet::default();
    file_id_set.extend(self.local.list_file_id_li(tbl).await?);
    file_id_set.extend(self.remote_file_id_li_of(tbl, false).await?);
    // 各后端 `list_*` 统一升序（io-41），GC / compact 的遍历因此可复现。
    let mut file_id_li: Vec<FileId> = file_id_set.into_iter().collect();
    file_id_li.sort_unstable();
    Ok(file_id_li)
  }

  async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>> {
    // io-12：整文件读同样要校验本地副本的内容归属（GC / compact 会用它搬页）。
    if self.local_ident_ok(tbl, file_id, false)
      && let Some(bytes) = self.local.read_file(tbl, file_id).await?
    {
      self.touch(tbl, file_id);
      self.record_cache_hit();
      self.trim_cache();
      self.drain_evictions_if_due().await;
      return Ok(Some(bytes));
    }
    self.record_cache_miss();
    if self.fetch_data_file(tbl, file_id).await? == DownloadOutcome::Missing {
      return Ok(None);
    }
    let bytes = self.local.read_file(tbl, file_id).await?;
    self.trim_cache();
    self.drain_evictions_if_due().await;
    Ok(bytes)
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()> {
    let epoch = self.evict_epoch.get();
    self.local.write_file(tbl, file_id, bytes).await?;
    // 写入 await 期间表可能已被驱逐：已驱逐的表不得重新登记。
    if self.evict_epoch.get() != epoch {
      return Err(KvError::TryAgain);
    }
    self.upsert_resident(tbl, file_id, bytes.len() as u64, true, None);
    self.trim_cache();
    self.drain_evictions_if_due().await;
    Ok(())
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>> {
    self.local.file_size(tbl, file_id).await
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.local.remove_file(tbl, file_id).await?;
    self.forget(tbl, file_id);
    self.sync_cache_bytes();
    self.remote.delete(&self.file_key(tbl, file_id)).await?;
    Ok(())
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    let mut file_id_set = HashSet::default();
    file_id_set.extend(self.local.list_segment_file_id_li(tbl).await?);
    file_id_set.extend(self.remote_file_id_li_of(tbl, true).await?);
    let mut file_id_li: Vec<FileId> = file_id_set.into_iter().collect();
    file_id_li.sort_unstable();
    Ok(file_id_li)
  }

  /// 全部 data 文件变体（rev-39）：本地 cache 的变体（compio 解析文件名）并上
  /// 远端对象变体（按当前 identity branch 过滤）。同 triple 去重——远端对象的
  /// 本地 cache 副本与远端条目是同一份。
  async fn list_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    let mut variant_set = HashSet::default();
    variant_set.extend(self.local.list_file_variants(tbl).await?);
    variant_set.extend(self.remote_file_variant_li(tbl, false).await?);
    let mut variant_li: Vec<FileVariant> = variant_set.into_iter().collect();
    variant_li.sort_unstable();
    Ok(variant_li)
  }

  async fn list_segment_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    let mut variant_set = HashSet::default();
    variant_set.extend(self.local.list_segment_file_variants(tbl).await?);
    variant_set.extend(self.remote_file_variant_li(tbl, true).await?);
    let mut variant_li: Vec<FileVariant> = variant_set.into_iter().collect();
    variant_li.sort_unstable();
    Ok(variant_li)
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.local.remove_segment_file(tbl, file_id).await?;
    // 文件已删，完成标记必须同步摘除：file_id 复用时 stale 标记会把半装
    // 文件判成完整，分区目录也会因残留标记无法回收。
    self.remove_segment_marker(tbl, file_id).await?;
    self.forget_segment(tbl, file_id);
    self.sync_cache_bytes();
    self.remote.delete(&self.segment_key(tbl, file_id)).await?;
    Ok(())
  }

  /// 只删一个 `(file_id, branch, term)` 物理变体（rev-39）：本地半按文件名精确
  /// 匹配，远端半直接拼对象 key——不再走 `file_key` 的区间解析，同 id 的其它
  /// `(branch, term)` 变体不受影响。
  async fn remove_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .local
      .remove_file_variant(tbl, file_id, branch, term)
      .await?;
    self.forget(tbl, file_id);
    self.sync_cache_bytes();
    let name = fkv_types::data_file_name(file_id, branch, term);
    self.remote.delete(&self.object_key(tbl, &name)).await?;
    Ok(())
  }

  async fn remove_segment_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .local
      .remove_segment_file_variant(tbl, file_id, branch, term)
      .await?;
    // 标记按 file_id 记，不区分变体：保守摘除，剩余的其它变体会按半装重装一次。
    self.remove_segment_marker(tbl, file_id).await?;
    self.forget_segment(tbl, file_id);
    self.sync_cache_bytes();
    let name = fkv_types::segment_file_name(file_id, branch, term);
    self.remote.delete(&self.object_key(tbl, &name)).await?;
    Ok(())
  }

  async fn list_local_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.local.list_file_id_li(tbl).await
  }

  /// 只删本地 cache 的 data 文件：`LocalGc` 不得删除远端对象（对齐 C++
  /// `ExecuteLocalGC`），远端副本留给后续读按需回源。
  async fn remove_local_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.local.remove_file(tbl, file_id).await?;
    self.forget(tbl, file_id);
    self.sync_cache_bytes();
    Ok(())
  }

  async fn list_local_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.local.list_segment_file_id_li(tbl).await
  }

  async fn remove_local_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.local.remove_segment_file(tbl, file_id).await?;
    // LocalGc 删文件必须带上标记：否则空分区目录永远回收不掉。
    self.remove_segment_marker(tbl, file_id).await?;
    self.forget_segment(tbl, file_id);
    self.sync_cache_bytes();
    Ok(())
  }

  /// 本地 cache 的 data 文件变体（不含远端对象）。
  async fn list_local_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    self.local.list_file_variants(tbl).await
  }

  /// 只删本地 cache 的一个物理变体，远端对象不动（对齐 `remove_local_file`）。
  async fn remove_local_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .local
      .remove_file_variant(tbl, file_id, branch, term)
      .await?;
    self.forget(tbl, file_id);
    self.sync_cache_bytes();
    Ok(())
  }

  async fn list_local_segment_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    self.local.list_segment_file_variants(tbl).await
  }

  async fn remove_local_segment_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .local
      .remove_segment_file_variant(tbl, file_id, branch, term)
      .await?;
    self.remove_segment_marker(tbl, file_id).await?;
    self.forget_segment(tbl, file_id);
    self.sync_cache_bytes();
    Ok(())
  }

  /// 转发本地后端的空分区目录回收。
  ///
  /// 默认实现是 no-op，`fkv_shard` 的 `Backend::Cloud` 分支会因此永远删不掉
  /// 本地 cache 目录。回收前由本地后端自行判定目录是否为空（有残留就保留），
  /// 云端不额外删任何文件。
  async fn try_remove_empty_partition_dir(&self, tbl: &TableIdent) -> Result<bool> {
    self.local.try_remove_empty_partition_dir(tbl).await
  }
}
