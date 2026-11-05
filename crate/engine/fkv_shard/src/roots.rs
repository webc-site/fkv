//! 多根目录本地后端：分区按 `store_path_lut` 选根（对齐 C++ `FileManager`
//! 的 `store_path` / `store_path_lut` 放置规则）。
//!
//! 单根路径仍走同一实现（LUT = `[0]`），行为与单 `CompioStore` 一致；多根时
//! 每个分区（`tbl.partition_id`）按 LUT 槽位固定落一个根，读 / 写 / manifest /
//! GC 全部沿用该映射，不跨根迁移数据。

use std::path::PathBuf;

use fkv_error::{KvError, Result};
use fkv_io::{
  FileIo, FileVariant, FixedBuffer, FixedSegmentIo, ManifestIo, PageIo, SegmentIo, TermIo,
};
use fkv_io_compio::CompioStore;
use fkv_types::{BranchFileRange, FileId, FilePageId, TableIdent};

use super::{ShardConfig, backend::build_compio_store};

/// 多根目录装配参数：根列表 + 分区 -> 根序号 LUT。
///
/// LUT 由 `KvOption::store_path_lut_li` 生成（`fkv_config::compute_store_path_lut`），
/// 构造时校验非空且索引不越界，运行期选根不再做错误处理。
#[derive(Debug, Clone)]
pub struct RootedConfig {
  root_li: Vec<PathBuf>,
  lut_li: Vec<u32>,
}

impl RootedConfig {
  pub fn new(root_li: Vec<PathBuf>, lut_li: Vec<u32>) -> Result<Self> {
    if root_li.is_empty() || lut_li.is_empty() {
      return Err(KvError::InvalidArgs);
    }
    let root_num = root_li.len();
    if lut_li
      .iter()
      .any(|&idx| usize::try_from(idx).map_or(true, |idx| idx >= root_num))
    {
      return Err(KvError::InvalidArgs);
    }
    Ok(Self { root_li, lut_li })
  }

  /// 单根配置（LUT 恒 `[0]`）：保持现行为。
  pub fn single(root: PathBuf) -> Self {
    Self {
      root_li: vec![root],
      lut_li: vec![0],
    }
  }
}

/// 按 LUT 分发到多个 `CompioStore` 的本地后端。
pub struct RootedStore {
  store_li: Vec<CompioStore>,
  lut_li: Vec<u32>,
}

impl RootedStore {
  pub fn new(config: &ShardConfig, rooted: &RootedConfig) -> Self {
    Self {
      store_li: rooted
        .root_li
        .iter()
        .map(|root| build_compio_store(config, root.clone()))
        .collect(),
      lut_li: rooted.lut_li.clone(),
    }
  }

  /// 分区归属的 `CompioStore`（LUT 已校验，索引必然有效）。
  fn store_of(&self, tbl: &TableIdent) -> &CompioStore {
    let slot = u64::from(tbl.partition_id) % self.lut_li.len() as u64;
    &self.store_li[self.lut_li[slot as usize] as usize]
  }

  /// 首个根的写 IO 上限（各根装配一致，测试装配投影用）。
  pub fn max_inflight_write(&self) -> usize {
    self.store_li[0].max_inflight_write()
  }

  fn is_routed(&self) -> bool {
    self.store_li.len() > 1
  }
}

impl PageIo for RootedStore {
  fn set_file_identity(&self, branch: &str, term: u64) {
    // 身份是 store 级状态：每个根都要对齐，否则跨根分区的文件名 term 过期。
    for store in &self.store_li {
      store.set_file_identity(branch, term);
    }
  }

  fn set_file_range_li(&self, tbl: &TableIdent, range_li: &[BranchFileRange]) {
    self.store_of(tbl).set_file_range_li(tbl, range_li);
  }

  fn preload_path_li(&self, tbl: &TableIdent) -> usize {
    self.store_of(tbl).preload_path_li(tbl)
  }

  fn invalidate_path_cache(&self) {
    for store in &self.store_li {
      store.invalidate_path_cache();
    }
  }

  fn set_verify_checksum(&self, enabled: bool) {
    for store in &self.store_li {
      store.set_verify_checksum(enabled);
    }
  }

  fn evict_table(&self, tbl: &TableIdent) {
    self.store_of(tbl).evict_table(tbl);
  }

  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    self.store_of(tbl).read_page(tbl, file_page_id, dst).await
  }

  async fn read_page_li(
    &self,
    tbl: &TableIdent,
    file_page_id_li: &[FilePageId],
    page_buf_li: &mut [Vec<u8>],
  ) -> Result<()> {
    self
      .store_of(tbl)
      .read_page_li(tbl, file_page_id_li, page_buf_li)
      .await
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    self.store_of(tbl).write_page(tbl, file_page_id, src).await
  }

  async fn write_page_li(&self, tbl: &TableIdent, page_li: &[(FilePageId, Vec<u8>)]) -> Result<()> {
    self.store_of(tbl).write_page_li(tbl, page_li).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.store_of(tbl).sync_data(tbl).await
  }
}

impl ManifestIo for RootedStore {
  async fn reset_manifest(&self, tbl: &TableIdent, size: u64) -> Result<()> {
    self.store_of(tbl).reset_manifest(tbl, size).await
  }

  async fn reset_manifest_file(&self, tbl: &TableIdent, name: &str, size: u64) -> Result<()> {
    self
      .store_of(tbl)
      .reset_manifest_file(tbl, name, size)
      .await
  }

  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.store_of(tbl).append_manifest(tbl, record).await
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.store_of(tbl).switch_manifest(tbl, record).await
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    self.store_of(tbl).read_manifest(tbl).await
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
    self.store_of(tbl).drop_manifest(tbl).await
  }

  async fn refresh_manifest(&self, tbl: &TableIdent) -> Result<()> {
    self.store_of(tbl).refresh_manifest(tbl).await
  }

  async fn drop_local_manifest(&self, tbl: &TableIdent) -> Result<()> {
    self.store_of(tbl).drop_local_manifest(tbl).await
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self
      .store_of(tbl)
      .write_manifest_file(tbl, name, record)
      .await
  }

  async fn append_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self
      .store_of(tbl)
      .append_manifest_file(tbl, name, record)
      .await
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    self.store_of(tbl).list_manifest_file_name_li(tbl).await
  }

  /// 旁路文件（segment 完成标记）随分区落在对应根，必须显式下探。
  async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    self.store_of(tbl).list_sidecar_file_name_li(tbl).await
  }

  async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.store_of(tbl).remove_sidecar_file_name(tbl, name).await
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    self.store_of(tbl).read_manifest_file(tbl, name).await
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.store_of(tbl).remove_manifest_file(tbl, name).await
  }

  async fn prune_superseded_manifest_file_li(
    &self,
    tbl: &TableIdent,
    branch: &str,
    term: u64,
  ) -> Result<usize> {
    self
      .store_of(tbl)
      .prune_superseded_manifest_file_li(tbl, branch, term)
      .await
  }
}

impl TermIo for RootedStore {
  /// term 是 shard 级（只按 branch / shard_id 命名，不带分区），固定放首个根，
  /// 避免同一 shard 的 term 在多根之间分叉。C++ 的 term 文件同样按 shard 唯一。
  async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>> {
    self.store_li[0].read_term(branch, shard_id).await
  }

  async fn upsert_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<u64> {
    self.store_li[0].upsert_term(branch, shard_id, term).await
  }

  async fn remove_term(&self, branch: &str, shard_id: u32) -> Result<()> {
    self.store_li[0].remove_term(branch, shard_id).await
  }
}

impl SegmentIo for RootedStore {
  fn write_segment_len(&self, src_len: usize) -> u64 {
    self.store_li[0].write_segment_len(src_len)
  }

  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    self
      .store_of(tbl)
      .read_segment(tbl, segment_fp_id, dst)
      .await
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> Result<()> {
    self
      .store_of(tbl)
      .write_segment(tbl, segment_fp_id, src)
      .await
  }
}

impl FileIo for RootedStore {
  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.store_of(tbl).list_file_id_li(tbl).await
  }

  async fn list_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    self.store_of(tbl).list_file_variants(tbl).await
  }

  async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>> {
    self.store_of(tbl).read_file(tbl, file_id).await
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()> {
    self.store_of(tbl).write_file(tbl, file_id, bytes).await
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>> {
    self.store_of(tbl).file_size(tbl, file_id).await
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.store_of(tbl).remove_file(tbl, file_id).await
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.store_of(tbl).list_segment_file_id_li(tbl).await
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.store_of(tbl).remove_segment_file(tbl, file_id).await
  }

  async fn list_local_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.store_of(tbl).list_local_file_id_li(tbl).await
  }

  async fn remove_local_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.store_of(tbl).remove_local_file(tbl, file_id).await
  }

  async fn list_local_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.store_of(tbl).list_local_segment_file_id_li(tbl).await
  }

  async fn remove_local_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self
      .store_of(tbl)
      .remove_local_segment_file(tbl, file_id)
      .await
  }

  async fn remove_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .store_of(tbl)
      .remove_file_variant(tbl, file_id, branch, term)
      .await
  }

  async fn remove_segment_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .store_of(tbl)
      .remove_segment_file_variant(tbl, file_id, branch, term)
      .await
  }

  async fn list_segment_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    self.store_of(tbl).list_segment_file_variants(tbl).await
  }

  async fn try_remove_empty_partition_dir(&self, tbl: &TableIdent) -> Result<bool> {
    self.store_of(tbl).try_remove_empty_partition_dir(tbl).await
  }
}

impl FixedSegmentIo for RootedStore {
  /// 多根时固定 buffer 无法跨 store 复用（每个 `CompioStore` 各持一套注册表），
  /// 返回 `InvalidArgs` 让 shard 回落到非 fixed 路径（`put_large` / `get_large`），
  /// 不把数据写到错误的根。单根保持现有注册行为。
  fn register_fixed_buffer(&self, len: usize) -> Result<FixedBuffer> {
    if self.is_routed() {
      return Err(KvError::InvalidArgs);
    }
    self.store_li[0].register_fixed_buffer(len)
  }

  async fn read_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &mut FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    self
      .store_of(tbl)
      .read_segment_fixed(tbl, segment_fp_id, buf, offset, len)
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
    self
      .store_of(tbl)
      .write_segment_fixed(tbl, segment_fp_id, buf, offset, len)
      .await
  }
}
