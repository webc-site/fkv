//! 写路径：注册写槽位、合并写、脏文件登记。

#[cfg(target_os = "linux")]
use std::sync::Arc;
use std::{
  io,
  path::{Path, PathBuf},
};

#[cfg(target_os = "linux")]
use compio::driver::ToSharedFd;
use compio::{
  buf::{BufResult, IoBufMut},
  fs::File,
  io::AsyncWriteAt,
};
#[cfg(target_os = "linux")]
use fkv_error::KvError;
use fkv_error::Result;
#[cfg(target_os = "linux")]
use fkv_io::{FixedBuffer, FixedSegmentIo};
use fkv_types::TableIdent;

#[cfg(target_os = "linux")]
use crate::FIXED_WRITE_SLOT_LIMIT;
#[cfg(target_os = "linux")]
use crate::fixed;
use crate::{CompioStore, DirectBuf, WriteChunk, map_io_err};

/// 借出的注册写槽位守卫：取消安全由 `Drop` 保证，不靠调用点纪律（io-07）。
///
/// 写组 future 在 `WRITE_FIXED` await 点被 drop（任务 panic 被捕获 / 外部
/// 取消）时，compio 的取消不可靠，在飞 op 由 driver 持有直到 CQE 收割。
/// 守卫按 [`FixedBuffer::as_mut_slice`] 区分两条归还路径：
/// - 在飞 op 归零（含常规完成）→ 缓冲回空闲池复用；
/// - 取消未确认 → 丢弃槽位（内核 index 交给注册表延迟回收，池配额扣减）。
///
/// 无论哪条路径 `FixedWritePool::total` 都回到借出前的值，避免配额单向
/// 棘轮：否则累计 [`crate::FIXED_WRITE_SLOT_LIMIT`] 次取消后 WRITE_FIXED
/// 静默永久退化为普通写。
#[cfg(target_os = "linux")]
pub(crate) struct FixedSlotGuard<'a> {
  store: &'a CompioStore,
  /// `None` = 已通过 [`Self::submit_done`] 归还，Drop 不再重复处理。
  buf: Option<FixedBuffer>,
}

#[cfg(target_os = "linux")]
impl FixedSlotGuard<'_> {
  /// 可写切片；仍有取消未确认的在飞 op 租约时返回 `None`（按 `Busy` 处理）。
  pub(crate) fn as_mut_slice(&mut self) -> Option<&mut [u8]> {
    self.buf.as_mut()?.as_mut_slice()
  }

  /// 提交一次 `WRITE_FIXED`，返回后缓冲回空闲池。
  ///
  /// 仅在 [`Self::as_mut_slice`] 返回 `Some` 后调用（缓冲无在飞租约）；
  /// 违反契约只得到 `Busy`，不 panic。真实 IO 错误同样归还缓冲后上抛。
  pub(crate) async fn submit_done(
    &mut self,
    file: &File,
    len: usize,
    offset: u64,
  ) -> Result<usize> {
    let result = match self.buf.as_ref() {
      Some(buf) => self.store.submit_fixed_write(file, buf, len, offset).await,
      None => Err(KvError::Busy),
    };
    if let Some(buf) = self.buf.take() {
      self.store.put_fixed_slot(buf);
    }
    result
  }
}

#[cfg(target_os = "linux")]
impl Drop for FixedSlotGuard<'_> {
  fn drop(&mut self) {
    let Some(mut buf) = self.buf.take() else {
      return;
    };
    // 在飞 op 已归零：缓冲内容稳定，回空闲池照常复用。
    if buf.as_mut_slice().is_some() {
      self.store.put_fixed_slot(buf);
      return;
    }
    // 取消未确认：丢弃该槽位（Drop 归还内核 index，池配额扣减）。
    self.store.discard_fixed_slot(buf);
  }
}

impl CompioStore {
  /// 借一个长度 `len` 的注册写槽位守卫：优先复用空闲槽位，池未满时新注册。
  /// 注册失败返回 `None`，调用方回落普通写路径。
  ///
  /// 守卫的 `Drop` 覆盖所有离场路径（含 future 中途取消），`total` 必归位。
  #[cfg(target_os = "linux")]
  pub(crate) fn take_fixed_slot(&self, len: usize) -> Option<FixedSlotGuard<'_>> {
    let buf = self.take_fixed_slot_buf(len)?;
    Some(FixedSlotGuard {
      store: self,
      buf: Some(buf),
    })
  }

  #[cfg(target_os = "linux")]
  fn take_fixed_slot_buf(&self, len: usize) -> Option<FixedBuffer> {
    // 槽位缓存上限跟随在飞上限收紧，但不超过注册缓冲的常驻上限。
    let slot_limit = self.max_inflight_write.min(FIXED_WRITE_SLOT_LIMIT);
    {
      let mut pool = self.fixed_write.borrow_mut();
      if let Some(buf) = pool.free.get_mut(&len).and_then(Vec::pop) {
        return Some(buf);
      }
      // 批次页数配置变化后旧长度槽位不再复用，先释放再为新长度让位。
      if pool.total >= slot_limit {
        let other_li: Vec<usize> = pool
          .free
          .keys()
          .copied()
          .filter(|other| *other != len)
          .collect();
        for other in other_li {
          if let Some(buf_li) = pool.free.remove(&other) {
            pool.total -= buf_li.len();
          }
        }
      }
      if pool.total >= slot_limit {
        return None;
      }
      pool.total += 1;
    }
    // 注册要拿 `fixed_io` 锁，必须放在池借用之外。
    match self.register_fixed_buffer(len) {
      Ok(buf) => {
        self.fixed_write.borrow_mut().alloc_num += 1;
        Some(buf)
      }
      Err(_) => {
        self.fixed_write.borrow_mut().total -= 1;
        None
      }
    }
  }

  #[cfg(target_os = "linux")]
  pub(crate) fn put_fixed_slot(&self, buf: FixedBuffer) {
    let len = buf.len();
    self
      .fixed_write
      .borrow_mut()
      .free
      .entry(len)
      .or_default()
      .push(buf);
  }

  /// 丢弃一个注册槽位：`Drop` 把 index 还给后端，池配额同步扣减。
  ///
  /// 与 [`Self::put_fixed_slot`] 是两条归还路径，都必须让 `total` 回到
  /// `take_fixed_slot` 借出前的值，否则配额永久泄漏、后续注册被拒。
  #[cfg(target_os = "linux")]
  pub(crate) fn discard_fixed_slot(&self, buf: FixedBuffer) {
    drop(buf);
    let mut pool = self.fixed_write.borrow_mut();
    pool.total -= 1;
    pool.discard_num += 1;
    drop(pool);
    log::warn!(
      target: "fkv_io_compio",
      "fixed write slot discarded: write group future cancelled with unconfirmed op"
    );
  }

  /// 注册槽位是否属于当前线程 / runtime 的 ring。跨线程或 runtime 重建后
  /// fixed IO 必须回落普通路径。
  #[cfg(target_os = "linux")]
  pub(crate) fn owns_fixed_ring(&self) -> bool {
    self.fixed_io.lock().ok().is_some_and(|slot| {
      slot
        .as_ref()
        .is_some_and(fixed::FixedBufferRegistry::owns_current_ring)
    })
  }

  /// runtime 重建后丢弃旧 ring 的槽位池，下一次 `take_fixed_slot` 重新注册。
  #[cfg(target_os = "linux")]
  pub(crate) fn reset_fixed_ring(&self) {
    if let Ok(mut slot) = self.fixed_io.lock() {
      *slot = None;
    }
    let mut pool = self.fixed_write.borrow_mut();
    pool.free.clear();
    pool.total = 0;
  }

  /// 为一次在飞 fixed op 登记槽位租约（io-37）。
  ///
  /// 租约随 op 生命周期存活：compio 取消不可靠，外层 future 被 drop 后
  /// SQE 仍在 ring 中；只有 driver 收割 CQE 并释放 op 后，槽位才可复用。
  #[cfg(target_os = "linux")]
  pub(crate) fn lease_fixed_slot(&self, index: u16) -> Result<fixed::SlotLease> {
    let mut slot = self.fixed_io.lock().map_err(|_| KvError::IoFail)?;
    let buffers = slot.as_mut().ok_or(KvError::IoFail)?;
    if !buffers.owns_current_ring() {
      return Err(KvError::IoFail);
    }
    Ok(buffers.lease(Arc::downgrade(&self.fixed_io), index))
  }

  /// 用注册槽位提交一次 `WRITE_FIXED`。
  #[cfg(target_os = "linux")]
  pub(crate) async fn submit_fixed_write(
    &self,
    file: &File,
    buf: &FixedBuffer,
    len: usize,
    offset: u64,
  ) -> Result<usize> {
    let lease = buf.share().ok_or(KvError::Busy)?;
    let slot = self.lease_fixed_slot(buf.index())?;
    let op = fixed::FixedWrite::new(file.to_shared_fd(), lease, 0, len, offset, buf.index())?
      .with_slot(slot);
    let BufResult(result, _) = compio::runtime::submit(op).await;
    result.map_err(map_io_err)
  }

  /// 提交一次写并归还缓冲：direct 缓冲回对齐池，普通缓冲回写池。
  pub(crate) async fn write_chunk(
    &self,
    file: &mut File,
    chunk: WriteChunk,
    offset: u64,
  ) -> io::Result<usize> {
    match chunk {
      WriteChunk::Plain(buf) => {
        let len = buf.len();
        let BufResult(result, buf) = file.write_at(buf, offset).await;
        self.put_write_buf(len, buf);
        result
      }
      WriteChunk::Direct(buf) => {
        let BufResult(result, buf) = file.write_at(buf, offset).await;
        self.put_direct_buf(buf.into_inner());
        result
      }
    }
  }

  /// 提交一组连续页写。
  ///
  /// Linux 优先借注册写槽位走 `WRITE_FIXED`（槽位复用，无每次分配）；
  /// 注册不可用或运行时已重建时回落到本地池化 / O_DIRECT 对齐缓冲。
  /// 取消安全由 [`FixedSlotGuard`] 的 `Drop` 保证：future 在 await 点被丢弃
  /// 时槽位配额仍归还，不依赖调用点纪律。
  pub(crate) async fn write_page_group(
    &self,
    file: &mut File,
    page_li: &[&[u8]],
    bytes: usize,
    offset: u64,
  ) -> Result<usize> {
    #[cfg(target_os = "linux")]
    {
      let slot_len = self.max_write_batch_pages * self.page_size;
      if let Some(mut slot) = self.take_fixed_slot(slot_len) {
        if self.owns_fixed_ring() {
          if let Some(dst) = slot.as_mut_slice() {
            for (index, page) in page_li.iter().enumerate() {
              let start = index * self.page_size;
              dst[start..start + page.len()].copy_from_slice(page);
            }
            return slot.submit_done(file, bytes, offset).await;
          }
          // 槽位缓冲仍有取消未确认的在飞 op：守卫 Drop 丢弃该槽位
          // （归还 index 与池配额），本次回落普通写路径。
        } else {
          // 运行时已重建：丢弃旧 ring 的槽位池，下一次重新注册。
          // 守卫 Drop 先归还池内缓冲，`reset_fixed_ring` 再整体清空。
          drop(slot);
          self.reset_fixed_ring();
        }
      }
    }
    let result = if self.direct_io.get() {
      let mut buf = DirectBuf::from_buf(self.take_direct_buf(bytes), bytes);
      for (index, page) in page_li.iter().enumerate() {
        let start = index * self.page_size;
        buf.as_mut_slice()[start..start + page.len()].copy_from_slice(page);
      }
      self
        .write_chunk(file, WriteChunk::Direct(buf), offset)
        .await
    } else {
      let mut buf = self.take_write_buf(bytes);
      for (index, page) in page_li.iter().enumerate() {
        let start = index * self.page_size;
        buf[start..start + page.len()].copy_from_slice(page);
      }
      self.write_chunk(file, WriteChunk::Plain(buf), offset).await
    };
    result.map_err(map_io_err)
  }

  /// 记录本表未落盘的 data / segment 文件。
  pub(crate) fn mark_dirty(&self, tbl: &TableIdent, path: PathBuf) {
    let mut dirty = self.dirty.borrow_mut();
    if let Some(set) = dirty.get_mut(tbl) {
      set.insert(path);
      return;
    }
    dirty.entry(tbl.clone()).or_default().insert(path);
  }

  /// 文件删除后从 dirty 集合剔除。
  pub(crate) fn forget_dirty(&self, path: &Path) {
    for set in self.dirty.borrow_mut().values_mut() {
      set.remove(path);
    }
  }
}
