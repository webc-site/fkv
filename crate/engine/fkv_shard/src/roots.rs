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

/// 按方法清单生成 `RootedStore` 的 `store_of(tbl)` 转发。
///
/// 清单第一参数固定写作 `tbl: &TableIdent`：分区按它选根，并原样透传给内层
/// `CompioStore`。非分区方法（身份 / 缓存 / term / fixed buffer 注册）保留手写，
/// 见各 impl 内注释；手写原因都是「不是按分区转发」。
///
/// `async fn` / `fn` 与 `&self` / `&mut self` 的展开规则同 `forward_io_enum!`。
/// 转发目标是 `store_of(tbl)` 的共享引用，所以内层方法必须是 `&self`；
/// 六个 IO trait 的方法全部满足（没有 `&mut self` 方法）。
macro_rules! forward_io_rooted {
  () => {};

  ($(#[$m:meta])* async fn $name:ident(&self, tbl: &TableIdent $(, $arg:ident: $ty:ty)* $(,)?) $(-> $ret:ty)?; $($rest:tt)*) => {
    $(#[$m])*
    #[inline]
    async fn $name(&self, tbl: &TableIdent $(, $arg: $ty)*) $(-> $ret)? {
      self.store_of(tbl).$name(tbl, $($arg),*).await
    }
    forward_io_rooted!($($rest)*);
  };

  ($(#[$m:meta])* async fn $name:ident(&mut self, tbl: &TableIdent $(, $arg:ident: $ty:ty)* $(,)?) $(-> $ret:ty)?; $($rest:tt)*) => {
    $(#[$m])*
    #[inline]
    async fn $name(&mut self, tbl: &TableIdent $(, $arg: $ty)*) $(-> $ret)? {
      self.store_of(tbl).$name(tbl, $($arg),*).await
    }
    forward_io_rooted!($($rest)*);
  };

  ($(#[$m:meta])* fn $name:ident(&self, tbl: &TableIdent $(, $arg:ident: $ty:ty)* $(,)?) $(-> $ret:ty)?; $($rest:tt)*) => {
    $(#[$m])*
    #[inline]
    fn $name(&self, tbl: &TableIdent $(, $arg: $ty)*) $(-> $ret)? {
      self.store_of(tbl).$name(tbl, $($arg),*)
    }
    forward_io_rooted!($($rest)*);
  };

  ($(#[$m:meta])* fn $name:ident(&mut self, tbl: &TableIdent $(, $arg:ident: $ty:ty)* $(,)?) $(-> $ret:ty)?; $($rest:tt)*) => {
    $(#[$m])*
    #[inline]
    fn $name(&mut self, tbl: &TableIdent $(, $arg: $ty)*) $(-> $ret)? {
      self.store_of(tbl).$name(tbl, $($arg),*)
    }
    forward_io_rooted!($($rest)*);
  };
}

impl PageIo for RootedStore {
  // 手写例外：身份是 store 级状态，每个根都要对齐，否则跨根分区的文件名 term 过期。
  fn set_file_identity(&self, branch: &str, term: u64) {
    for store in &self.store_li {
      store.set_file_identity(branch, term);
    }
  }

  forward_io_rooted! {
    fn set_file_range_li(&self, tbl: &TableIdent, range_li: &[BranchFileRange]);
    fn preload_path_li(&self, tbl: &TableIdent) -> usize;
    fn evict_table(&self, tbl: &TableIdent);
    async fn read_page(&self, tbl: &TableIdent, file_page_id: FilePageId, dst: &mut [u8]) -> Result<()>;
    async fn read_page_owned(&self, tbl: &TableIdent, file_page_id: FilePageId, buf: &mut Vec<u8>) -> Result<()>;
    async fn read_page_li(&self, tbl: &TableIdent, file_page_id_li: &[FilePageId], page_buf_li: &mut [Vec<u8>]) -> Result<()>;
    async fn read_page_span(&self, tbl: &TableIdent, file_page_id: FilePageId, page_buf_li: &mut [Vec<u8>]) -> Result<()>;
    async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()>;
    async fn write_page_li(&self, tbl: &TableIdent, page_li: &[(FilePageId, Vec<u8>)]) -> Result<()>;
    async fn sync_data(&self, tbl: &TableIdent) -> Result<()>;
  }

  // 手写例外：路径索引缓存是 store 级状态，每个根都要失效。
  fn invalidate_path_cache(&self) {
    for store in &self.store_li {
      store.invalidate_path_cache();
    }
  }

  // 手写例外：checksum 开关是 store 级状态，每个根都要切换。
  fn set_verify_checksum(&self, enabled: bool) {
    for store in &self.store_li {
      store.set_verify_checksum(enabled);
    }
  }
}

impl ManifestIo for RootedStore {
  forward_io_rooted! {
    async fn reset_manifest(&self, tbl: &TableIdent, size: u64) -> Result<()>;
    async fn reset_manifest_file(&self, tbl: &TableIdent, name: &str, size: u64) -> Result<()>;
    async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()>;
    async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()>;
    async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>>;
    async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()>;
    async fn refresh_manifest(&self, tbl: &TableIdent) -> Result<()>;
    async fn drop_local_manifest(&self, tbl: &TableIdent) -> Result<()>;
    async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()>;
    async fn append_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()>;
    async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>>;
    /// 旁路文件（segment 完成标记）随分区落在对应根，必须显式下探。
    async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>>;
    async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()>;
    async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>>;
    /// 晋升种子必须显式转发：默认走 `write_manifest_file` 会把 Cloud 的晋升
    /// 对象提前发布到远端（sem-60 写时晋升）。
    async fn seed_local_manifest_file(&self, tbl: &TableIdent, name: &str, bytes: &[u8]) -> Result<()>;
    async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()>;
    async fn prune_superseded_manifest_file_li(&self, tbl: &TableIdent, branch: &str, term: u64) -> Result<usize>;
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
  // 手写例外：段长是后端级配置，各根一致，取首个根即可。
  fn write_segment_len(&self, src_len: usize) -> u64 {
    self.store_li[0].write_segment_len(src_len)
  }

  forward_io_rooted! {
    async fn read_segment(&self, tbl: &TableIdent, segment_fp_id: FilePageId, dst: &mut [u8]) -> Result<()>;
    async fn write_segment(&self, tbl: &TableIdent, segment_fp_id: FilePageId, src: &[u8]) -> Result<()>;
  }
}

impl FileIo for RootedStore {
  forward_io_rooted! {
    async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>>;
    async fn list_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>>;
    async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>>;
    async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()>;
    async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>>;
    async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()>;
    async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>>;
    async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()>;
    async fn list_local_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>>;
    async fn remove_local_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()>;
    async fn list_local_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>>;
    async fn remove_local_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()>;
    async fn remove_file_variant(&self, tbl: &TableIdent, file_id: FileId, branch: &str, term: u64) -> Result<()>;
    async fn remove_segment_file_variant(&self, tbl: &TableIdent, file_id: FileId, branch: &str, term: u64) -> Result<()>;
    async fn list_segment_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>>;
    async fn try_remove_empty_partition_dir(&self, tbl: &TableIdent) -> Result<bool>;
  }
}

impl FixedSegmentIo for RootedStore {
  /// 多根时固定 buffer 无法跨 store 复用（每个 `CompioStore` 各持一套注册表），
  /// 返回 `InvalidArgs` 让 shard 回落到非 fixed 路径（`put_large` / `get_large`），
  /// 不把数据写到错误的根。单根保持现有注册行为。
  ///
  /// 手写例外：`is_routed()` 分支不属于纯转发。
  fn register_fixed_buffer(&self, len: usize) -> Result<FixedBuffer> {
    if self.is_routed() {
      return Err(KvError::InvalidArgs);
    }
    self.store_li[0].register_fixed_buffer(len)
  }

  forward_io_rooted! {
    async fn read_segment_fixed(&self, tbl: &TableIdent, segment_fp_id: FilePageId, buf: &mut FixedBuffer, offset: usize, len: usize) -> Result<()>;
    async fn write_segment_fixed(&self, tbl: &TableIdent, segment_fp_id: FilePageId, buf: &FixedBuffer, offset: usize, len: usize) -> Result<()>;
  }
}
