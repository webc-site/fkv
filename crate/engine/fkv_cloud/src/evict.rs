//! 待删队列的批量删除（显式 drain 与热路径按阈值 drain）。

use std::mem::take;

use fkv_error::Result;
use fkv_io::{FileIo, ManifestIo};
use fkv_object::ObjectStore;

use crate::{CloudStore, cache::EVICT_PENDING_FILE_MAX, segment::segment_marker_name};

impl<I: FileIo + ManifestIo, O: ObjectStore> CloudStore<I, O> {
  /// 批量删除待删队列里的本地文件；失败项回队，记账不丢。
  ///
  /// 删除前复核驻留状态：淘汰后同一文件可能已被重新下载 / 写入，此时必须
  /// 取消删除，否则会误删刚回填的 cache。显式调用（flush / 运维）返回错误；
  /// 热路径走 `drain_evictions_if_due` 只告警，不把清理失败升级成数据面
  /// 读写失败。
  pub async fn drain_evictions(&self) -> Result<()> {
    let (batch, attempted_seq) = {
      let mut state = self.state.borrow_mut();
      state.pending_delete_bytes = 0;
      (take(&mut state.pending_delete_li), state.evict_seq)
    };
    if batch.is_empty() {
      return Ok(());
    }
    let mut failed_li = Vec::new();
    let mut first_err = None;
    for victim in batch {
      if self.is_resident(&victim) {
        continue;
      }
      let result = if victim.segment {
        // 先删段文件，再删完成标记：文件删除失败时标记必须保留（文件仍完整，
        // 下一轮重试）。只删文件不删标记会按淘汰量线性泄漏小文件，分区目录
        // 永远无法回收。
        match self
          .local
          .remove_segment_file(&victim.tbl, victim.file_id)
          .await
        {
          Ok(()) => {
            self
              .local
              .remove_sidecar_file_name(&victim.tbl, &segment_marker_name(victim.file_id))
              .await
          }
          Err(err) => Err(err),
        }
      } else {
        self.local.remove_file(&victim.tbl, victim.file_id).await
      };
      if let Err(err) = result {
        failed_li.push(victim);
        if first_err.is_none() {
          first_err = Some(err);
        }
      }
    }
    {
      let mut state = self.state.borrow_mut();
      // 只把本次快照标记为已尝试；期间新入队的仍会在下一轮触发。
      state.drain_seq = attempted_seq;
      if !failed_li.is_empty() {
        state.drain_fail_streak = state.drain_fail_streak.saturating_add(1);
        // 回队同样受上限约束：坏盘持续失败时丢弃溢出的失败项，只保留有界重试窗口。
        let mut kept_bytes = 0u64;
        for victim in failed_li {
          if state.pending_delete_li.len() >= EVICT_PENDING_FILE_MAX {
            break;
          }
          kept_bytes += victim.bytes;
          state.pending_delete_li.push(victim);
        }
        state.pending_delete_bytes += kept_bytes;
      } else {
        state.drain_fail_streak = 0;
      }
    }
    match first_err {
      Some(err) => Err(err),
      None => Ok(()),
    }
  }
}

impl<I: FileIo + ManifestIo, O: ObjectStore> CloudStore<I, O> {
  /// 热路径的批量删除：达到阈值才执行，失败只告警并回队。
  pub(crate) async fn drain_evictions_if_due(&self) {
    if !self.evict_drain_due() {
      return;
    }
    if let Err(err) = self.drain_evictions().await {
      log::warn!("本地 cache 淘汰删除失败，已回队等待重试: {err}");
    }
  }
}
