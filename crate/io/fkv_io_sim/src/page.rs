//! `PageIo`：page 读写与 `sync_data`（volatile → durable）。

use fastrand::Rng;
use fkv_error::{KvError, Result};
use fkv_failpoint::points::{SYNC_DATA, WRITE_PAGE};
use fkv_io::PageIo;
use fkv_types::{FilePageId, TableIdent};

use crate::{
  SimStore,
  op::Op,
  state::{PageKey, log_op, log_op_payload, overlay_get, sort_page_key_li},
};

impl PageIo for SimStore {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ReadPage {
        tbl: tbl.clone(),
        file_page_id,
      },
    )?;
    let key = (tbl.clone(), file_page_id);
    let src = overlay_get(&state.durable.page_map, &state.volatile.page_map, &key)
      .ok_or(KvError::ResourceMissing)?;
    if dst.len() < src.len() {
      return Err(KvError::InvalidArgs);
    }
    dst[..src.len()].copy_from_slice(&src);
    Ok(())
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    if src.len() != self.page_size {
      return Err(KvError::InvalidArgs);
    }
    fkv_failpoint::fail_point!(WRITE_PAGE);
    fkv_failpoint::kill_point(WRITE_PAGE);
    let mut state = self.lock();
    log_op_payload(
      &self.config,
      &mut state,
      Op::WritePage {
        tbl: tbl.clone(),
        file_page_id,
        len: src.len(),
      },
      Some(src),
    )?;
    state
      .volatile
      .page_map
      .insert((tbl.clone(), file_page_id), Some(src.to_vec()));
    Ok(())
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    fkv_failpoint::fail_point!(SYNC_DATA);
    fkv_failpoint::kill_point(SYNC_DATA);
    let mut state = self.lock();
    log_op(&self.config, &mut state, Op::SyncData { tbl: tbl.clone() })?;

    let mut page_keys: Vec<PageKey> = state
      .volatile
      .page_map
      .keys()
      .filter(|key| &key.0 == tbl)
      .map(PageKey::clone)
      .collect();
    sort_page_key_li(&mut page_keys);
    if let Some(seed) = self.config.reorder_seed {
      Rng::with_seed(seed).shuffle(&mut page_keys);
    }
    for key in page_keys {
      // 故障判定前置到状态变更之前：命中时该次 FlushPage 不得改动任何状态。
      log_op(
        &self.config,
        &mut state,
        Op::FlushPage {
          tbl: key.0.clone(),
          file_page_id: key.1,
        },
      )?;
      if let Some(entry) = state.volatile.page_map.remove(&key) {
        match entry {
          Some(bytes) => {
            state.durable.page_map.insert(key.clone(), bytes);
          }
          None => {
            state.durable.page_map.remove(&key);
          }
        }
      }
    }

    let mut segment_keys: Vec<PageKey> = state
      .volatile
      .segment_map
      .keys()
      .filter(|key| &key.0 == tbl)
      .map(PageKey::clone)
      .collect();
    sort_page_key_li(&mut segment_keys);
    for key in segment_keys {
      // 同页刷写：先判故障，再迁移 volatile → durable。
      log_op(
        &self.config,
        &mut state,
        Op::FlushSegment {
          tbl: key.0.clone(),
          segment_fp_id: key.1,
        },
      )?;
      if let Some(entry) = state.volatile.segment_map.remove(&key) {
        match entry {
          Some(bytes) => {
            state.durable.segment_map.insert(key.clone(), bytes);
          }
          None => {
            state.durable.segment_map.remove(&key);
          }
        }
      }
    }

    // manifest / 旁路文件不在这里刷：真实后端的 manifest 写路径自己 fsync，
    // 写成功即 durable；由 `sync_data` 顺带提升会掩盖「写路径漏 fsync」的缺陷。
    Ok(())
  }
}
