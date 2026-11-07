//! `SegmentIo` / `FixedSegmentIo` 实现与 segment 表 flush。

use bytes::Bytes;
use fkv_error::{KvError, Result};
use fkv_io::{FileIo, FixedBuffer, FixedSegmentIo, ManifestIo, PageIo, SegmentIo};
use fkv_object::ObjectStore;
use fkv_types::{FilePageId, TableIdent};

use crate::{CloudStore, download::DownloadOutcome, group::SegmentInflightGuard};

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> CloudStore<I, O> {
  /// 整文件上传该 table 的 dirty segment 文件，每个脏文件只组装 / 上传一次。
  pub(crate) async fn flush_segment_tbl(&self, tbl: &TableIdent) -> Result<()> {
    let file_id_li = self.dirty_segment_li(tbl);
    if !file_id_li.is_empty() {
      // 组装的段来自本地页缓存：整表段数据先落盘，再逐个写完成标记。
      // 标记不得早于数据 durability（崩溃后标记在、数据零洞会被读零上传）。
      self.local.sync_data(tbl).await?;
    }
    for file_id in file_id_li {
      let bytes = Bytes::from(self.assemble_segment_file(tbl, file_id).await?);
      let len = bytes.len() as u64;
      self
        .remote
        .put(&self.write_segment_key(tbl, file_id), bytes, None, false)
        .await?;
      // 上传成功后立即写完成标记：本实例本地新建的段文件不走 install 回填，
      // 缺标记会被 `segment_state` 判半装 → 淘汰 / 重启后整文件重下，
      // `restore_table_cache` 不登记（磁盘占用穿透预算）。
      self.write_segment_marker(tbl, file_id).await?;
      self.mark_segment_clean(tbl, file_id, len);
    }
    self.trim_cache();
    self.drain_evictions().await
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> SegmentIo for CloudStore<I, O> {
  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    let file_id = self.segment_file_id(segment_fp_id);
    // io-12：本地 segment 文件的内容归属必须与 manifest 的 per-file 归属一致；
    // 跨 term 重开归档时同 file id 的旧副本不能命中。
    if self.local_ident_ok(tbl, file_id, true) {
      match self.local.read_segment(tbl, segment_fp_id, dst).await {
        Ok(()) => {
          self.touch_segment(tbl, file_id);
          self.record_cache_hit();
          return Ok(());
        }
        Err(KvError::ResourceMissing) => {
          self.record_cache_miss();
        }
        Err(err) => return Err(err),
      }
    } else {
      self.record_cache_miss();
    }
    if self.fetch_segment_file(tbl, file_id).await? == DownloadOutcome::Missing {
      return Err(KvError::ResourceMissing);
    }
    let result = self.local.read_segment(tbl, segment_fp_id, dst).await;
    self.trim_cache();
    self.drain_evictions_if_due().await;
    result
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> Result<()> {
    let file_id = self.segment_file_id(segment_fp_id);
    // 入口即登记组内在飞，覆盖整个「补齐 + 本地写 + 标脏」：只在写完后置
    // `dirty` 的话，写 await 期间表上仍无 scheme 时，同组回填会按旧尾槽规则
    // 给半装文件盖「完整」标记。
    let _inflight = SegmentInflightGuard::new(&self.owner.group, self.owner.id, tbl, file_id);
    self.ensure_segment_resident(tbl, file_id).await?;
    // 与 `write_page` 同契约：`ensure_segment_resident` 与本地写都可能 await，
    // 期间表被 detach / Drop 时不得继续登记（`table_slot` 会让已驱逐表复活）。
    let epoch = self.evict_epoch.get();
    self.local.write_segment(tbl, segment_fp_id, src).await?;
    if self.evict_epoch.get() != epoch {
      return Err(KvError::TryAgain);
    }
    // 只标脏：flush_segment_tbl 里每个 segment 文件整份组装一次、上传一次。
    self.upsert_segment_resident(tbl, file_id, self.segment_file_bytes()?, true, None);
    self.trim_cache();
    self.drain_evictions_if_due().await;
    Ok(())
  }
}

impl<I: ManifestIo + PageIo + FixedSegmentIo + FileIo, O: ObjectStore> FixedSegmentIo
  for CloudStore<I, O>
{
  fn register_fixed_buffer(&self, len: usize) -> Result<FixedBuffer> {
    self.local.register_fixed_buffer(len)
  }

  async fn read_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &mut FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    let file_id = self.segment_file_id(segment_fp_id);
    // io-12：语义同 `read_segment`，固定缓冲路径同样先校验本地内容归属。
    if self.local_ident_ok(tbl, file_id, true) {
      match self
        .local
        .read_segment_fixed(tbl, segment_fp_id, buf, offset, len)
        .await
      {
        Ok(()) => {
          self.touch_segment(tbl, file_id);
          self.record_cache_hit();
          return Ok(());
        }
        Err(KvError::ResourceMissing) => {
          self.record_cache_miss();
        }
        Err(err) => return Err(err),
      }
    } else {
      self.record_cache_miss();
    }
    if self.fetch_segment_file(tbl, file_id).await? == DownloadOutcome::Missing {
      return Err(KvError::ResourceMissing);
    }
    let result = self
      .local
      .read_segment_fixed(tbl, segment_fp_id, buf, offset, len)
      .await;
    self.trim_cache();
    self.drain_evictions_if_due().await;
    result
  }

  async fn write_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    let file_id = self.segment_file_id(segment_fp_id);
    // 同 `write_segment`：登记必须覆盖整个写，不能等写完后才标脏。
    let _inflight = SegmentInflightGuard::new(&self.owner.group, self.owner.id, tbl, file_id);
    self.ensure_segment_resident(tbl, file_id).await?;
    let epoch = self.evict_epoch.get();
    self
      .local
      .write_segment_fixed(tbl, segment_fp_id, buf, offset, len)
      .await?;
    if self.evict_epoch.get() != epoch {
      return Err(KvError::TryAgain);
    }
    self.upsert_segment_resident(tbl, file_id, self.segment_file_bytes()?, true, None);
    self.trim_cache();
    self.drain_evictions_if_due().await;
    Ok(())
  }
}
