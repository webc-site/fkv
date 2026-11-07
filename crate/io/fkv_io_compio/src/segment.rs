//! segment 读写：定长段、O_DIRECT 对齐、io_uring 注册缓冲读写。

#[cfg(target_os = "linux")]
use std::sync::Arc;

#[cfg(target_os = "linux")]
use compio::driver::ToSharedFd;
use compio::{
  buf::{BufResult, IoBuf, IoBufMut},
  io::{AsyncReadAt, AsyncReadManagedAt},
};
#[cfg(target_os = "linux")]
use fkv_buf::AlignedBuf;
use fkv_error::{KvError, Result};
use fkv_io::{FixedBuffer, FixedSegmentIo, FixedSegmentRead, SegmentIo, validate_fixed_segment_li};
use fkv_types::{FilePageId, PAGE_ALIGN, TableIdent};
#[cfg(target_os = "linux")]
use futures::future::try_join_all;

#[cfg(target_os = "linux")]
use crate::fixed;
use crate::{CompioStore, DirectBuf, WriteChunk, map_io_err};

impl SegmentIo for CompioStore {
  fn write_segment_len(&self, _src_len: usize) -> u64 {
    self.segment_size as u64
  }

  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    if <[u8]>::is_empty(dst) {
      return Ok(());
    }
    if dst.len() > self.segment_size {
      return Err(KvError::InvalidArgs);
    }
    fkv_failpoint::fail_point!(READ_SEGMENTS);
    // 测试门闩：错误注入先判（上面的宏命中即返回），因此 `!err` + `!hold`
    // 组合可以让首次读失败、重排出的下一次读停在这里，便于用完成顺序断言
    // 替代「睡一小会儿后仍不应完成」的墙钟假设。
    fkv_failpoint::hold_point!(READ_SEGMENTS_HOLD);
    let len = dst.len();
    let file = self.open_segment_file(tbl, segment_fp_id).await?;
    let offset = self.segment_offset(segment_fp_id);
    if self.direct_segment() {
      // O_DIRECT 要求长度 / 缓冲对齐：读入向上取整的对齐缓冲，再取有效前缀。
      // `segment_size` 是页大小整数倍，任意 `len <= segment_size` 取整后仍落在
      // 该段范围内，不会越界读到下一段。
      let aligned = len.next_multiple_of(PAGE_ALIGN);
      let BufResult(result, buf) = file
        .read_at(
          DirectBuf::from_buf(self.take_direct_buf(aligned), 0),
          offset,
        )
        .await;
      let read_len = match result {
        Ok(read_len) => read_len.min(len),
        Err(err) => {
          // 失败路径同样归还对齐缓冲，保持池命中率（io-41）。
          self.put_direct_buf(buf.into_inner());
          return Err(map_io_err(err));
        }
      };
      // 段定长落盘、尾部补零：短读按零填充处理，不当作错误。
      dst[..read_len].copy_from_slice(&buf.as_init()[..read_len]);
      dst[read_len..].fill(0);
      self.put_direct_buf(buf.into_inner());
      return Ok(());
    }
    // provided-buffer 读：缓冲由 runtime 池常驻，io_uring 下免每次读的
    // page pin/unpin；仍是一次 copy。池耗尽 / 未配置 / 短读时回落自管池。
    // segment 走 O_DIRECT 时该分支在函数开头已 return，且开关会被显式
    // 关闭（io-16）；仅 data 页 direct 时 segment 仍 buffered、此分支可用。
    if self.managed_segment_read_enabled()
      && let Ok(Some(buf)) = file.read_managed_at(len, offset).await
      && buf.len() >= len
    {
      dst.copy_from_slice(&buf[..len]);
      return Ok(());
    }
    let buf = self.take_read_buf(len);
    let BufResult(result, buf) = file.read_at(buf, offset).await;
    let read_len = match result {
      Ok(read_len) => read_len.min(len),
      Err(err) => {
        self.put_read_buf(len, buf);
        return Err(map_io_err(err));
      }
    };
    dst[..read_len].copy_from_slice(&buf[..read_len]);
    dst[read_len..].fill(0);
    self.put_read_buf(len, buf);
    Ok(())
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> Result<()> {
    if src.len() > self.segment_size {
      return Err(KvError::InvalidArgs);
    }
    fkv_failpoint::fail_point!(WRITE_SEGMENTS);
    self.ensure_dir(tbl).await?;
    let path = self.segment_path(tbl, segment_fp_id);
    let mut file = self.open_write_file(&path).await.map_err(map_io_err)?;
    // segment 按 segment_size 定长落盘，尾部补零。
    // O_DIRECT 下缓冲与长度都必须整页对齐：用对齐缓冲定长整段写。
    let chunk = if self.direct_segment() {
      let mut buf = DirectBuf::from_buf(self.take_direct_buf(self.segment_size), self.segment_size);
      buf.as_mut_slice()[..src.len()].copy_from_slice(src);
      buf.as_mut_slice()[src.len()..].fill(0);
      WriteChunk::Direct(buf)
    } else {
      let mut buf = self.take_write_buf(self.segment_size);
      debug_assert_eq!(buf.len(), self.segment_size);
      buf[..src.len()].copy_from_slice(src);
      buf[src.len()..].fill(0);
      WriteChunk::Plain(buf)
    };
    let write_len = self
      .write_chunk(&mut file, chunk, self.segment_offset(segment_fp_id))
      .await
      .map_err(map_io_err)?;
    if write_len != self.segment_size {
      return Err(KvError::IoFail);
    }
    self.preallocate_segment_file(&path, &file).await?;
    self.mark_dirty(tbl, path);
    Ok(())
  }
}

impl FixedSegmentIo for CompioStore {
  fn register_fixed_buffer(&self, len: usize) -> Result<FixedBuffer> {
    #[cfg(target_os = "linux")]
    {
      if len == 0 {
        return Err(KvError::InvalidArgs);
      }
      let buf = Arc::new(AlignedBuf::new(len));
      let mut slot = self.fixed_io.lock().map_err(|_| KvError::IoFail)?;
      // 旧 ring 的注册表不能继续 register：ring_fd 可能已被新 runtime 复用，
      // BUFFERS_UPDATE 会写进兄弟 ring 的 sparse 表。换代即重建注册表。
      if slot
        .as_ref()
        .is_some_and(|buffers| !buffers.owns_current_ring())
      {
        *slot = None;
      }
      if slot.is_none() {
        *slot = Some(fixed::FixedBufferRegistry::current()?);
      }
      let buffers = slot.as_mut().ok_or(KvError::IoFail)?;
      let token = buffers.token();
      let index = buffers.register(buf.clone())?;
      drop(slot);
      let weak = Arc::downgrade(&self.fixed_io);
      let hook = Arc::new(move |index: u16| {
        if let Some(shared) = weak.upgrade()
          && let Ok(mut slot) = shared.lock()
          && let Some(buffers) = slot.as_mut()
          && Arc::ptr_eq(&buffers.token(), &token)
        {
          buffers.release(index);
        }
      });
      Ok(FixedBuffer::registered_arc_with_hook(buf, index, hook))
    }
    #[cfg(not(target_os = "linux"))]
    {
      Ok(FixedBuffer::new(len))
    }
  }

  async fn read_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &mut FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    let end = offset.checked_add(len).ok_or(KvError::InvalidArgs)?;
    if end > buf.len() || len > self.segment_size {
      return Err(KvError::InvalidArgs);
    }
    // O_DIRECT 注册读要求缓冲地址与长度都按页对齐；段内偏移非对齐时回落
    // `read_segment`（内部用对齐缓冲承接）。buffered 句柄不受此限制。
    #[cfg(target_os = "linux")]
    let direct_ok = !self.direct_segment()
      || (offset.is_multiple_of(PAGE_ALIGN) && len.is_multiple_of(PAGE_ALIGN));
    #[cfg(target_os = "linux")]
    if direct_ok
      && buf.is_registered()
      && self.fixed_io.lock().ok().is_some_and(|slot| {
        slot
          .as_ref()
          .is_some_and(fixed::FixedBufferRegistry::owns_current_ring)
      })
    {
      let file = self.open_segment_file(tbl, segment_fp_id).await?;
      let slot = self.lease_fixed_slot(buf.index())?;
      let op = fixed::FixedRead::new(
        file.to_shared_fd(),
        buf.share().ok_or(KvError::Busy)?,
        offset,
        len,
        self.segment_offset(segment_fp_id),
        buf.index(),
      )?
      .with_slot(slot);
      let BufResult(result, _) = compio::runtime::submit(op).await;
      let read_len = result.map_err(map_io_err)?;
      // 段尾未写满部分补零。
      if read_len < len {
        buf.as_mut_slice().ok_or(KvError::Busy)?[offset + read_len..end].fill(0);
      }
      return Ok(());
    }
    let dst = buf.as_mut_slice().ok_or(KvError::Busy)?;
    self
      .read_segment(tbl, segment_fp_id, &mut dst[offset..end])
      .await
  }

  /// 批量 fixed 读：已注册且当前 ring 拥有槽位表时，整段
  /// 对齐项一次并发提交多个 READ_FIXED（对齐 C++ `ReadSegments` 批内并发）；
  /// O_DIRECT 下非页对齐的项（值尾段）与未注册 / 跨 ring 场景逐段串行回退。
  ///
  /// 并发安全：各区间互不重叠由 [`validate_fixed_segment_li`] 在提交前整批
  /// 强制；同槽位多个在飞 op 由 `SlotLease` 在飞计数与 `share_disjoint`
  /// 区间租约共同看护（io-37），future 被取消时租约随 op 存活到 CQE 收割。
  async fn read_segment_fixed_li(
    &self,
    tbl: &TableIdent,
    item_li: &[FixedSegmentRead],
    buf: &mut FixedBuffer,
  ) -> Result<()> {
    validate_fixed_segment_li(buf.len(), item_li)?;
    #[cfg(target_os = "linux")]
    if buf.is_registered() && self.owns_fixed_ring() {
      if item_li.iter().any(|item| item.len > self.segment_size) {
        return Err(KvError::InvalidArgs);
      }
      // O_DIRECT 下 READ_FIXED 要求缓冲偏移与长度页对齐；值尾段常不满足，
      // 留给串行回退用 `read_segment` 的对齐暂存缓冲承接。
      let (fixed_li, plain_li): (Vec<FixedSegmentRead>, Vec<FixedSegmentRead>) =
        item_li.iter().copied().partition(|item| {
          !self.direct_segment()
            || (item.offset.is_multiple_of(PAGE_ALIGN) && item.len.is_multiple_of(PAGE_ALIGN))
        });
      let read_len_li = {
        // 批内共享借用：各 READ_FIXED future 以互不重叠的区间租约并发持有
        // 同一注册缓冲，全部 CQE 收割后借用结束、在飞计数归零。
        let buf = &*buf;
        try_join_all(fixed_li.iter().map(|item| async move {
          let file = self.open_segment_file(tbl, item.segment_fp_id).await?;
          let slot = self.lease_fixed_slot(buf.index())?;
          let op = fixed::FixedRead::new(
            file.to_shared_fd(),
            buf.share_disjoint(),
            item.offset,
            item.len,
            self.segment_offset(item.segment_fp_id),
            buf.index(),
          )?
          .with_slot(slot);
          let BufResult(result, _) = compio::runtime::submit(op).await;
          result.map_err(map_io_err)
        }))
        .await?
      };
      // 短读（段文件被截断）补零尾部，语义与单段读一致。
      let dst = buf.as_mut_slice().ok_or(KvError::Busy)?;
      for (item, read_len) in fixed_li.iter().zip(read_len_li) {
        if read_len < item.len {
          dst[item.offset + read_len..item.offset + item.len].fill(0);
        }
      }
      for item in plain_li {
        self
          .read_segment_fixed(tbl, item.segment_fp_id, buf, item.offset, item.len)
          .await?;
      }
      return Ok(());
    }
    // 未注册 / 非 Linux / 跨 ring：逐段串行（同默认实现语义）。
    for item in item_li {
      self
        .read_segment_fixed(tbl, item.segment_fp_id, buf, item.offset, item.len)
        .await?;
    }
    Ok(())
  }

  async fn write_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    let end = offset.checked_add(len).ok_or(KvError::InvalidArgs)?;
    if end > buf.len() || len > self.segment_size {
      return Err(KvError::InvalidArgs);
    }
    // 批量段写允许同一缓冲上多个互不重叠区间的 op 并发（fkv_large 的
    // `run_batch`），故用区间租约；`as_mut_slice` 仍被在飞计数挡住。
    let lease = buf.share_disjoint();
    #[cfg(target_os = "linux")]
    if buf.is_registered()
      && self.fixed_io.lock().ok().is_some_and(|slot| {
        slot
          .as_ref()
          .is_some_and(fixed::FixedBufferRegistry::owns_current_ring)
      })
    {
      // 尾段不足一整段时走普通路径：需要补零到 `segment_size`，否则复用旧段会
      // 残留旧字节；`write_segment` 用对齐缓冲按段起始偏移提交，O_DIRECT 下安全。
      if len != self.segment_size {
        return self
          .write_segment(tbl, segment_fp_id, &lease[offset..end])
          .await;
      }
      // 整段 WRITE_FIXED 在 O_DIRECT 下要求用户缓冲地址、长度与文件偏移都按页
      // 对齐：缓冲基址由注册时的 `AlignedBuf` 保证，文件偏移是 `segment_offset`
      // 加 `offset`。段内偏移非对齐时内核回 EINVAL，会被 `map_io_err` 摊平成
      // `IoFail` 硬错误，掩盖调用方的错位参数。此处提交前拒绝，返回
      // `InvalidArgs`，不静默落回 buffered（与 read 的回落语义不同：写有副作用，
      // 静默换路径会掩盖上层越界/错位 bug）。
      if self.direct_segment()
        && !(offset.is_multiple_of(PAGE_ALIGN) && len.is_multiple_of(PAGE_ALIGN))
      {
        return Err(KvError::InvalidArgs);
      }
      self.ensure_dir(tbl).await?;
      let path = self.segment_path(tbl, segment_fp_id);
      let file = self.open_write_file(&path).await.map_err(map_io_err)?;
      let slot = self.lease_fixed_slot(buf.index())?;
      let op = fixed::FixedWrite::new(
        file.to_shared_fd(),
        lease,
        offset,
        len,
        self.segment_offset(segment_fp_id),
        buf.index(),
      )?
      .with_slot(slot);
      let BufResult(result, _) = compio::runtime::submit(op).await;
      let write_len = result.map_err(map_io_err)?;
      if write_len != len {
        return Err(KvError::IoFail);
      }
      self.preallocate_segment_file(&path, &file).await?;
      self.mark_dirty(tbl, path);
      return Ok(());
    }
    self
      .write_segment(tbl, segment_fp_id, &lease[offset..end])
      .await
  }
}
