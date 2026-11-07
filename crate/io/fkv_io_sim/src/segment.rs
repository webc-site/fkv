//! `SegmentIo` / `FixedSegmentIo`：segment 读写与定长缓冲视图。

use fkv_error::{KvError, Result};
use fkv_failpoint::points::WRITE_SEGMENTS;
use fkv_io::{FixedBuffer, FixedSegmentIo, SegmentIo};
use fkv_types::{FilePageId, TableIdent};

use crate::{
  SimStore,
  op::Op,
  state::{log_op, log_op_payload, overlay_get},
};

impl SegmentIo for SimStore {
  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    if dst.len() > self.segment_size {
      return Err(KvError::InvalidArgs);
    }
    fkv_failpoint::fail_point!(READ_SEGMENTS);
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ReadSegment {
        tbl: tbl.clone(),
        segment_fp_id,
        len: dst.len(),
      },
    )?;
    // 空读是 no-op，不要求段存在（与 compio / mem 一致）。
    if dst.is_empty() {
      return Ok(());
    }
    let key = (tbl.clone(), segment_fp_id);
    let src = overlay_get(
      &state.durable.segment_map,
      &state.volatile.segment_map,
      &key,
    )
    .ok_or(KvError::ResourceMissing)?;
    let copy_len = dst.len().min(src.len());
    dst[..copy_len].copy_from_slice(&src[..copy_len]);
    dst[copy_len..].fill(0);
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
    fkv_failpoint::kill_point(WRITE_SEGMENTS);
    let mut state = self.lock();
    log_op_payload(
      &self.config,
      &mut state,
      Op::WriteSegment {
        tbl: tbl.clone(),
        segment_fp_id,
        len: src.len(),
      },
      Some(src),
    )?;
    state
      .volatile
      .segment_map
      .insert((tbl.clone(), segment_fp_id), Some(src.to_vec()));
    Ok(())
  }
}

impl FixedSegmentIo for SimStore {
  fn register_fixed_buffer(&self, len: usize) -> Result<FixedBuffer> {
    let mut state = self.lock();
    log_op(&self.config, &mut state, Op::RegisterFixedBuffer { len })?;
    Ok(FixedBuffer::new(len))
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
    if end > buf.len() {
      return Err(KvError::InvalidArgs);
    }
    {
      let mut state = self.lock();
      log_op(
        &self.config,
        &mut state,
        Op::ReadSegmentFixed {
          tbl: tbl.clone(),
          segment_fp_id,
          offset,
          len,
        },
      )?;
    }
    let dst = buf.as_mut_slice().ok_or(KvError::Busy)?;
    self
      .read_segment(tbl, segment_fp_id, &mut dst[offset..end])
      .await
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
    if end > buf.len() {
      return Err(KvError::InvalidArgs);
    }
    {
      let mut state = self.lock();
      log_op(
        &self.config,
        &mut state,
        Op::WriteSegmentFixed {
          tbl: tbl.clone(),
          segment_fp_id,
          offset,
          len,
        },
      )?;
    }
    self
      .write_segment(tbl, segment_fp_id, &buf.as_slice()[offset..end])
      .await
  }
}
