//! 纯内存后端：page / manifest / term 都存在 HashMap。

use core::cell::RefCell;

use fkv_error::{KvError, Result};
use fkv_io::{
  DEFAULT_SEGMENT_SIZE, DEFAULT_SEGMENTS_PER_FILE_SHIFT, FileIo, FixedBuffer, FixedSegmentIo,
  ManifestIo, PageIo, SegmentIo, TermIo,
};
use fkv_types::{FileId, FilePageId, TableIdent};
use gxhash::{HashMap, HashSet};

#[derive(Debug)]
pub struct MemStore {
  page_size: usize,
  pages_per_file_shift: u8,
  segment_size: usize,
  segments_per_file_shift: u8,
  page_map: RefCell<HashMap<(TableIdent, FilePageId), Vec<u8>>>,
  segment_map: RefCell<HashMap<(TableIdent, FilePageId), Vec<u8>>>,
  manifest_map: RefCell<HashMap<TableIdent, Vec<u8>>>,
  manifest_file_map: RefCell<HashMap<(TableIdent, String), Vec<u8>>>,
  term_map: RefCell<HashMap<(String, u32), u64>>,
}

impl MemStore {
  pub fn new(page_size: usize) -> Self {
    Self {
      page_size,
      pages_per_file_shift: 18,
      segment_size: DEFAULT_SEGMENT_SIZE,
      segments_per_file_shift: DEFAULT_SEGMENTS_PER_FILE_SHIFT,
      page_map: RefCell::new(HashMap::default()),
      segment_map: RefCell::new(HashMap::default()),
      manifest_map: RefCell::new(HashMap::default()),
      manifest_file_map: RefCell::new(HashMap::default()),
      term_map: RefCell::new(HashMap::default()),
    }
  }

  pub fn with_pages_per_file(page_size: usize, pages_per_file_shift: u8) -> Self {
    Self {
      pages_per_file_shift,
      ..Self::new(page_size)
    }
  }

  pub fn with_segment_config(mut self, segment_size: usize, segments_per_file_shift: u8) -> Self {
    self.segment_size = segment_size;
    self.segments_per_file_shift = segments_per_file_shift;
    self
  }

  pub fn segment_size(&self) -> usize {
    self.segment_size
  }

  pub fn segments_per_file_shift(&self) -> u8 {
    self.segments_per_file_shift
  }

  pub fn page_size(&self) -> usize {
    self.page_size
  }

  pub fn page_count(&self) -> usize {
    self.page_map.borrow().len()
  }
}

impl PageIo for MemStore {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    let page_map = self.page_map.borrow();
    let src = page_map
      .get(&(tbl.clone(), file_page_id))
      .ok_or(KvError::ResourceMissing)?;
    if dst.len() < src.len() {
      return Err(KvError::InvalidArgs);
    }
    dst[..src.len()].copy_from_slice(src);
    Ok(())
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    if src.len() != self.page_size {
      return Err(KvError::InvalidArgs);
    }
    self
      .page_map
      .borrow_mut()
      .insert((tbl.clone(), file_page_id), src.to_vec());
    Ok(())
  }

  async fn sync_data(&self, _tbl: &TableIdent) -> Result<()> {
    Ok(())
  }
}

impl ManifestIo for MemStore {
  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self
      .manifest_map
      .borrow_mut()
      .entry(tbl.clone())
      .or_default()
      .extend_from_slice(record);
    Ok(())
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self
      .manifest_map
      .borrow_mut()
      .insert(tbl.clone(), record.to_vec());
    Ok(())
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    Ok(self.manifest_map.borrow().get(tbl).cloned())
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
    self.manifest_map.borrow_mut().remove(tbl);
    Ok(())
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self
      .manifest_file_map
      .borrow_mut()
      .insert((tbl.clone(), name.to_owned()), record.to_vec());
    Ok(())
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    let mut name_li: Vec<String> = self
      .manifest_file_map
      .borrow()
      .keys()
      .filter(|key| &key.0 == tbl)
      .map(|key| key.1.clone())
      .collect();
    name_li.sort();
    Ok(name_li)
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    Ok(
      self
        .manifest_file_map
        .borrow()
        .get(&(tbl.clone(), name.to_owned()))
        .cloned(),
    )
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self
      .manifest_file_map
      .borrow_mut()
      .remove(&(tbl.clone(), name.to_owned()));
    Ok(())
  }
}

impl TermIo for MemStore {
  async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>> {
    Ok(
      self
        .term_map
        .borrow()
        .get(&(branch.to_owned(), shard_id))
        .copied(),
    )
  }

  async fn upsert_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<u64> {
    let key = (branch.to_owned(), shard_id);
    if let Some(current) = self.term_map.borrow().get(&key).copied()
      && current > term
    {
      return Err(KvError::ExpiredTerm);
    }
    self.term_map.borrow_mut().insert(key, term);
    Ok(term)
  }
}

impl SegmentIo for MemStore {
  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    if dst.len() > self.segment_size {
      return Err(KvError::InvalidArgs);
    }
    let segment_map = self.segment_map.borrow();
    let src = segment_map
      .get(&(tbl.clone(), segment_fp_id))
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
    self
      .segment_map
      .borrow_mut()
      .insert((tbl.clone(), segment_fp_id), src.to_vec());
    Ok(())
  }
}

impl FixedSegmentIo for MemStore {
  fn register_fixed_buffer(&self, len: usize) -> Result<FixedBuffer> {
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
    self
      .read_segment(tbl, segment_fp_id, &mut buf.as_mut_slice()[offset..end])
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
    self
      .write_segment(tbl, segment_fp_id, &buf.as_slice()[offset..end])
      .await
  }
}

impl FileIo for MemStore {
  async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>> {
    let page_map = self.page_map.borrow();
    let mut max_index = None;
    for (tbl_key, file_page_id) in page_map.keys() {
      if tbl_key != tbl || file_page_id >> self.pages_per_file_shift != file_id {
        continue;
      }
      let index = file_page_id & ((1u64 << self.pages_per_file_shift) - 1);
      max_index = Some(max_index.map_or(index, |current: u64| current.max(index)));
    }
    let Some(max_index) = max_index else {
      return Ok(None);
    };
    let mut bytes = vec![0u8; (max_index as usize + 1) * self.page_size];
    let base = file_id << self.pages_per_file_shift;
    for ((tbl_key, file_page_id), src) in page_map.iter() {
      if tbl_key != tbl || file_page_id >> self.pages_per_file_shift != file_id {
        continue;
      }
      let offset = ((file_page_id - base) as usize) * self.page_size;
      bytes[offset..offset + src.len()].copy_from_slice(src);
    }
    Ok(Some(bytes))
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()> {
    if !bytes.len().is_multiple_of(self.page_size) {
      return Err(KvError::InvalidArgs);
    }
    let min_fp = file_id << self.pages_per_file_shift;
    let max_fp = (file_id + 1) << self.pages_per_file_shift;
    let mut page_map = self.page_map.borrow_mut();
    page_map.retain(|key, _| !(&key.0 == tbl && key.1 >= min_fp && key.1 < max_fp));
    for (index, chunk) in bytes.chunks(self.page_size).enumerate() {
      // 全零块是空洞，不占本地内存；真实 page 必含非零 checksum / header。
      if chunk.iter().all(|byte| *byte == 0) {
        continue;
      }
      page_map.insert((tbl.clone(), min_fp + index as u64), chunk.to_vec());
    }
    Ok(())
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>> {
    let page_map = self.page_map.borrow();
    let mut max_index = None;
    for (tbl_key, file_page_id) in page_map.keys() {
      if tbl_key != tbl || file_page_id >> self.pages_per_file_shift != file_id {
        continue;
      }
      let index = file_page_id & ((1u64 << self.pages_per_file_shift) - 1);
      max_index = Some(max_index.map_or(index, |current: u64| current.max(index)));
    }
    Ok(max_index.map(|index| (index + 1) * self.page_size as u64))
  }

  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    let mut set = HashSet::default();
    for key in self.page_map.borrow().keys() {
      if &key.0 == tbl {
        set.insert(key.1 >> self.pages_per_file_shift);
      }
    }
    Ok(set.into_iter().collect())
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    let min_fp = file_id << self.pages_per_file_shift;
    let max_fp = (file_id + 1) << self.pages_per_file_shift;
    self
      .page_map
      .borrow_mut()
      .retain(|key, _| !(&key.0 == tbl && key.1 >= min_fp && key.1 < max_fp));
    Ok(())
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    let mut set = HashSet::default();
    for key in self.segment_map.borrow().keys() {
      if &key.0 == tbl {
        set.insert(key.1 >> self.segments_per_file_shift);
      }
    }
    let mut file_id_li: Vec<FileId> = set.into_iter().collect();
    file_id_li.sort_unstable();
    Ok(file_id_li)
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    let min_fp = file_id << self.segments_per_file_shift;
    let max_fp = (file_id + 1) << self.segments_per_file_shift;
    self
      .segment_map
      .borrow_mut()
      .retain(|key, _| !(&key.0 == tbl && key.1 >= min_fp && key.1 < max_fp));
    Ok(())
  }
}
