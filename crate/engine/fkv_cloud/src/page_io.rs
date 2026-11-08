//! `PageIo` 实现：页读写、驱逐与 identity 转发。

use fkv_error::{KvError, Result};
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_object::ObjectStore;
use fkv_types::{BranchFileRange, FileId, FilePageId, TableIdent};
use gxhash::HashMap;

use crate::{
  CloudStore,
  download::{DownloadKey, DownloadOutcome},
  object_key::FileIdentity,
};

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> PageIo for CloudStore<I, O> {
  fn set_file_range_li(&self, tbl: &TableIdent, range_li: &[BranchFileRange]) {
    // reopen 会重复同步同一份链，内容不变时不重建。
    self.set_file_range(tbl, range_li);
    // 冷启动扫盘登记的副本归属未知：按刚下发的归属链补记（见
    // `adopt_unknown_ident`），避免正常重启后整份 cache 被误判 miss。
    self.adopt_unknown_ident(tbl);
  }

  async fn write_page_li(&self, tbl: &TableIdent, page_li: &[(FilePageId, Vec<u8>)]) -> Result<()> {
    if page_li.is_empty() {
      return Ok(());
    }
    // 逐页确保文件常驻，再把整批交给本地 `write_page_li` 合并写。
    // 同一文件取末端最大的那页的起点：它决定本批写入是否落在本地已有内容之后。
    let mut start_map = HashMap::<FileId, (u64, u64)>::default();
    for (file_page_id, page) in page_li {
      let len = page.len() as u64;
      let end = self.page_size_of(*file_page_id, page.len());
      let span = start_map.entry(self.file_id_of(*file_page_id)).or_default();
      if end > span.1 {
        *span = (end - len, end);
      }
    }
    for (file_id, (start, _)) in start_map {
      self.ensure_resident(tbl, file_id, start).await?;
    }
    // 落盘前取值，落盘后复核：本批写入 await 期间表可能已被驱逐，已驱逐的表
    // 不得重新登记（否则 reopen 会把本地残留当权威）。
    let epoch = self.evict_epoch.get();
    self.local.write_page_li(tbl, page_li).await?;
    if self.evict_epoch.get() != epoch {
      return Err(KvError::TryAgain);
    }
    for (file_page_id, page) in page_li {
      let file_id = self.file_id_of(*file_page_id);
      let size = self.page_size_of(*file_page_id, page.len());
      self.upsert_resident(tbl, file_id, size, true, None);
      let index = *file_page_id & ((1u64 << self.pages_per_file_shift) - 1);
      self.mark_dirty_range(tbl, file_id, index * page.len() as u64, page.len() as u64);
    }
    self.trim_cache();
    self.drain_evictions_if_due().await;
    Ok(())
  }

  fn set_file_identity(&self, branch: &str, term: u64) {
    *self.identity.borrow_mut() = FileIdentity {
      branch: branch.to_owned(),
      term,
    };
    self.local.set_file_identity(branch, term);
  }

  fn preload_path_li(&self, tbl: &TableIdent) -> usize {
    self.local.preload_path_li(tbl)
  }

  fn invalidate_path_cache(&self) {
    self.local.invalidate_path_cache();
  }

  fn set_verify_checksum(&self, enabled: bool) {
    self.local.set_verify_checksum(enabled);
  }

  fn evict_table(&self, tbl: &TableIdent) {
    self
      .foreign_file_set
      .borrow_mut()
      .retain(|(file_tbl, ..)| file_tbl != tbl);
    // 先推进代数：在飞 owner 落盘 / 登记前会复核，撤销已驱逐表的结果。
    // 全局递增，见 `evict_epoch` 注释。
    self.evict_epoch.set(self.evict_epoch.get().wrapping_add(1));
    // 摘掉该表所有在飞下载并唤醒等待者：owner 完成时不能再把已 detach 的表
    // `upsert_resident`（否则 reopen 会把本地残留当权威）。
    let mut inflight = self.inflight_download.borrow_mut();
    let key_li: Vec<DownloadKey> = inflight
      .keys()
      .filter(|(key_tbl, ..)| key_tbl == tbl)
      .cloned()
      .collect();
    for key in key_li {
      if let Some(entry) = inflight.remove(&key) {
        for waiter in entry.waiter_li {
          let _ = waiter.send(Err(KvError::TryAgain));
        }
      }
    }
    drop(inflight);
    self.forget_table_cache(tbl);
    self.local.evict_table(tbl);
    // 表状态已驱逐，per-file 归属链随 reopen 重新同步，避免残留旧 manifest 视图。
    self.file_range_map.borrow_mut().remove(tbl);
    // 下次打开必须重新扫描本地文件并登记：只清记账不摘「已扫描」标记会让
    // 盘上文件既不计预算也不参与 LRU（预算与驱逐脱钩）。
    self.restored_table_set.borrow_mut().remove(tbl);
  }

  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    let file_id = self.file_id_of(file_page_id);
    // io-12：本地副本必须属于 manifest 为该 file id 解析出的 (branch, term)。
    // 跨 term 重开归档时同一 file id 指向另一个 term 的远端对象，本地残留的
    // 新 term 内容不能命中，直接按 cache miss 回源。
    if self.local_ident_ok(tbl, file_id, false) {
      match self.local.read_page(tbl, file_page_id, dst).await {
        Ok(()) => {
          self.touch(tbl, file_id);
          self.record_cache_hit();
          self.trim_cache();
          self.drain_evictions_if_due().await;
          return Ok(());
        }
        // 本地缺失 / 短读 / checksum 失败都按 cache miss 处理：回源一次。
        Err(err @ (KvError::ResourceMissing | KvError::EndOfFile | KvError::Corrupted)) => {
          self.record_cache_miss();
          // 脏文件含未上传数据：远端没有对应版本，回源会丢写。
          if self.is_dirty(tbl, file_id) {
            return Err(err);
          }
        }
        Err(err) => return Err(err),
      }
    } else {
      // 记账缺失 / 内容归属与本轮 manifest 不一致：按 cache miss 回源覆盖。
      self.record_cache_miss();
    }
    let key = (tbl.clone(), false, file_id);
    let (outcome, _) = self
      .single_flight(key, || async {
        let epoch = self.evict_epoch.get();
        // 只有 owner 清理半截 / 损坏 cache，等待者不碰 owner 正在写的文件。
        self.local.remove_file(tbl, file_id).await?;
        self.forget(tbl, file_id);
        self.sync_cache_bytes();
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
      .await?;
    if outcome == DownloadOutcome::Missing {
      return Err(KvError::ResourceMissing);
    }
    let result = self.local.read_page(tbl, file_page_id, dst).await;
    // dst 已拿到数据后再淘汰，预算可以严格执行。
    self.trim_cache();
    self.drain_evictions_if_due().await;
    result
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    let file_id = self.file_id_of(file_page_id);
    let size = self.page_size_of(file_page_id, src.len());
    let index = file_page_id & ((1u64 << self.pages_per_file_shift) - 1);
    self
      .ensure_resident(tbl, file_id, index * src.len() as u64)
      .await?;
    let epoch = self.evict_epoch.get();
    self.local.write_page(tbl, file_page_id, src).await?;
    if self.evict_epoch.get() != epoch {
      return Err(KvError::TryAgain);
    }
    self.upsert_resident(tbl, file_id, size, true, None);
    self.mark_dirty_range(tbl, file_id, index * src.len() as u64, src.len() as u64);
    self.trim_cache();
    self.drain_evictions_if_due().await;
    Ok(())
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.flush_all_tbl(tbl).await?;
    self.local.sync_data(tbl).await
  }
}
