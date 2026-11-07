//! `FileIo`：整文件读写 / 列举 / 删除，按 `pages_per_file_shift` 映射到页。

use fkv_error::{KvError, Result};
use fkv_failpoint::points::WRITE_FILE_RENAME;
use fkv_io::FileIo;
use fkv_types::{FileId, TableIdent};
use gxhash::HashSet as GxHashSet;

use crate::{
  SimStore,
  op::Op,
  state::{
    PageKey, existing_page_key_li, live_page_key_li, live_segment_key_li, log_op, log_op_payload,
    overlay_get,
  },
};

impl FileIo for SimStore {
  async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ReadFile {
        tbl: tbl.clone(),
        file_id,
      },
    )?;
    let mask = (1u64 << self.pages_per_file_shift) - 1;
    let keys = live_page_key_li(&state.durable, &state.volatile, tbl);
    let mut max_index: Option<u64> = None;
    for key in &keys {
      if key.1 >> self.pages_per_file_shift != file_id {
        continue;
      }
      let index = key.1 & mask;
      max_index = Some(max_index.map_or(index, |current| current.max(index)));
    }
    let Some(max_index) = max_index else {
      return Ok(None);
    };
    let mut bytes = vec![0u8; (max_index as usize + 1) * self.page_size];
    for key in &keys {
      if key.1 >> self.pages_per_file_shift != file_id {
        continue;
      }
      let Some(src) = overlay_get(&state.durable.page_map, &state.volatile.page_map, key) else {
        continue;
      };
      let offset = (key.1 & mask) as usize * self.page_size;
      bytes[offset..offset + src.len()].copy_from_slice(&src);
    }
    Ok(Some(bytes))
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()> {
    if !bytes.len().is_multiple_of(self.page_size) {
      return Err(KvError::InvalidArgs);
    }
    fkv_failpoint::fail_point!(WRITE_FILE_SYNC_FAIL);
    fkv_failpoint::kill_point(WRITE_FILE_RENAME);
    let mut state = self.lock();
    log_op_payload(
      &self.config,
      &mut state,
      Op::WriteFile {
        tbl: tbl.clone(),
        file_id,
        len: bytes.len(),
      },
      Some(bytes),
    )?;
    let min_fp = file_id << self.pages_per_file_shift;
    let max_fp = (file_id + 1) << self.pages_per_file_shift;
    let targets: Vec<PageKey> = existing_page_key_li(&state.durable, &state.volatile, tbl)
      .into_iter()
      .filter(|key| key.1 >= min_fp && key.1 < max_fp)
      .collect();
    for key in targets {
      state.volatile.page_map.insert(key, None);
    }
    for (index, chunk) in bytes.chunks(self.page_size).enumerate() {
      // 全零块是空洞，不占内存；真实 page 必含非零 checksum / header。
      if chunk.iter().all(|byte| *byte == 0) {
        continue;
      }
      state
        .volatile
        .page_map
        .insert((tbl.clone(), min_fp + index as u64), Some(chunk.to_vec()));
    }
    Ok(())
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::FileSize {
        tbl: tbl.clone(),
        file_id,
      },
    )?;
    let mask = (1u64 << self.pages_per_file_shift) - 1;
    let mut max_index: Option<u64> = None;
    for key in live_page_key_li(&state.durable, &state.volatile, tbl) {
      if key.1 >> self.pages_per_file_shift != file_id {
        continue;
      }
      let index = key.1 & mask;
      max_index = Some(max_index.map_or(index, |current| current.max(index)));
    }
    Ok(max_index.map(|index| (index + 1) * self.page_size as u64))
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::RemoveFile {
        tbl: tbl.clone(),
        file_id,
      },
    )?;
    let min_fp = file_id << self.pages_per_file_shift;
    let max_fp = (file_id + 1) << self.pages_per_file_shift;
    let targets: Vec<PageKey> = existing_page_key_li(&state.durable, &state.volatile, tbl)
      .into_iter()
      .filter(|key| key.1 >= min_fp && key.1 < max_fp)
      .collect();
    for key in targets {
      state.volatile.page_map.insert(key, None);
    }
    Ok(())
  }

  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ListFileIdLi { tbl: tbl.clone() },
    )?;
    let mut set = GxHashSet::default();
    for key in live_page_key_li(&state.durable, &state.volatile, tbl) {
      set.insert(key.1 >> self.pages_per_file_shift);
    }
    let mut file_id_li: Vec<FileId> = set.into_iter().collect();
    file_id_li.sort_unstable();
    Ok(file_id_li)
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ListSegmentFileIdLi { tbl: tbl.clone() },
    )?;
    let mut set = GxHashSet::default();
    for key in live_segment_key_li(&state.durable, &state.volatile, tbl) {
      set.insert(key.1 >> self.segments_per_file_shift);
    }
    let mut file_id_li: Vec<FileId> = set.into_iter().collect();
    file_id_li.sort_unstable();
    Ok(file_id_li)
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::RemoveSegmentFile {
        tbl: tbl.clone(),
        file_id,
      },
    )?;
    let min_fp = file_id << self.segments_per_file_shift;
    let max_fp = (file_id + 1) << self.segments_per_file_shift;
    let targets: Vec<PageKey> = state
      .durable
      .segment_map
      .keys()
      .chain(state.volatile.segment_map.keys())
      .filter(|key| &key.0 == tbl && key.1 >= min_fp && key.1 < max_fp)
      .map(PageKey::clone)
      .collect();
    for key in targets {
      state.volatile.segment_map.insert(key, None);
    }
    Ok(())
  }
}
