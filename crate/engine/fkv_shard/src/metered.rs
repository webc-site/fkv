//! Metered IO 装饰器：给任意后端包一层在飞计数与字节 / 次数累计。

use std::{future::Future, sync::Arc};

use fkv_error::Result;
use fkv_io::{
  FileIo, FileVariant, FixedBuffer, FixedSegmentIo, ManifestIo, PageIo, SegmentIo, TermIo,
};
use fkv_metrics::ShardMetric;
use fkv_types::{BranchFileRange, FileId, FilePageId, TableIdent};

/// 给任意 `I` 包一层计数，用于 shard 的本地 IO。
#[derive(Debug, Clone)]
pub struct MeteredIo<I> {
  inner: I,
  metrics: Arc<ShardMetric>,
}

impl<I> MeteredIo<I> {
  pub fn new(inner: I, metrics: Arc<ShardMetric>) -> Self {
    Self { inner, metrics }
  }

  /// 内层 IO 引用。
  ///
  /// 计数只覆盖通用 IO 六件套；Cloud 的远端列举 / 预热等后端专有能力不在
  /// trait 上，需要调用方经此下探到具体后端（shard 的 per-table 请求）。
  pub fn inner(&self) -> &I {
    &self.inner
  }

  /// 包一层「在飞 IO」计数：future 首次 poll 时 +1，完成 / 取消 / 展开时 -1。
  ///
  /// 用 RAII 覆盖所有退出路径；被调用方 drop 的 IO future 不会漏计。
  async fn track<T, F: Future<Output = Result<T>>>(&self, fut: F) -> Result<T> {
    let _flight = IoFlightGuard::new(&self.metrics);
    fut.await
  }
}

/// 在飞 IO 守卫（见 [`MeteredIo::track`]）。
struct IoFlightGuard<'a> {
  metrics: &'a ShardMetric,
}

impl<'a> IoFlightGuard<'a> {
  fn new(metrics: &'a ShardMetric) -> Self {
    metrics.inc_io_in_flight();
    Self { metrics }
  }
}

impl Drop for IoFlightGuard<'_> {
  fn drop(&mut self) {
    self.metrics.dec_io_in_flight();
  }
}

/// 按方法清单生成 `MeteredIo` 的转发：同步方法直接下探内层；
/// 异步方法包一层 `MeteredIo::track`（在飞 IO 计数），不加其它逻辑。
///
/// 带指标累计（页 / 段 / 文件字节、fsync、manifest 读写）的方法保留手写，
/// 原因见各 impl 内的「手写例外」注释：成功分支还要额外 `add_*`。
///
/// `async fn` / `fn` 与 `&self` / `&mut self` 的展开规则同 shard 的转发宏；
/// 返回类型可省略（即 `()`）。六个 IO trait 没有泛型参数 / `where` 子句。
macro_rules! forward_io_metered {
  () => {};

  ($(#[$m:meta])* async fn $name:ident(&self $(, $arg:ident: $ty:ty)* $(,)?) $(-> $ret:ty)?; $($rest:tt)*) => {
    $(#[$m])*
    #[inline]
    async fn $name(&self $(, $arg: $ty)*) $(-> $ret)? {
      self.track(self.inner.$name($($arg),*)).await
    }
    forward_io_metered!($($rest)*);
  };

  ($(#[$m:meta])* async fn $name:ident(&mut self $(, $arg:ident: $ty:ty)* $(,)?) $(-> $ret:ty)?; $($rest:tt)*) => {
    $(#[$m])*
    #[inline]
    async fn $name(&mut self $(, $arg: $ty)*) $(-> $ret)? {
      self.track(self.inner.$name($($arg),*)).await
    }
    forward_io_metered!($($rest)*);
  };

  ($(#[$m:meta])* fn $name:ident(&self $(, $arg:ident: $ty:ty)* $(,)?) $(-> $ret:ty)?; $($rest:tt)*) => {
    $(#[$m])*
    #[inline]
    fn $name(&self $(, $arg: $ty)*) $(-> $ret)? {
      self.inner.$name($($arg),*)
    }
    forward_io_metered!($($rest)*);
  };

  ($(#[$m:meta])* fn $name:ident(&mut self $(, $arg:ident: $ty:ty)* $(,)?) $(-> $ret:ty)?; $($rest:tt)*) => {
    $(#[$m])*
    #[inline]
    fn $name(&mut self $(, $arg: $ty)*) $(-> $ret)? {
      self.inner.$name($($arg),*)
    }
    forward_io_metered!($($rest)*);
  };
}

impl<I: PageIo> PageIo for MeteredIo<I> {
  forward_io_metered! {
    fn set_file_identity(&self, branch: &str, term: u64);
    /// 必须下探：Cloud 的 per-file term 范围决定跨 term 读的对象 key，
    /// 默认 no-op 会让包裹后的后端按当前 identity 选错 key。
    fn set_file_range_li(&self, tbl: &TableIdent, range_li: &[BranchFileRange]);
    fn preload_path_li(&self, tbl: &TableIdent) -> usize;
    fn invalidate_path_cache(&self);
    fn set_verify_checksum(&self, enabled: bool);
    fn evict_table(&self, tbl: &TableIdent);
  }

  // 手写例外：成功路径要累计页读写字节 / fsync 次数。

  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    let result = self
      .track(self.inner.read_page(tbl, file_page_id, dst))
      .await;
    if result.is_ok() {
      self.metrics.add_page_read(dst.len() as u64);
    }
    result
  }

  async fn read_page_owned(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    buf: &mut Vec<u8>,
  ) -> Result<()> {
    // 转发而不是走默认实现：内层 Local 可内核直读调用方 Vec。
    let result = self
      .track(self.inner.read_page_owned(tbl, file_page_id, buf))
      .await;
    if result.is_ok() {
      self.metrics.add_page_read(buf.len() as u64);
    }
    result
  }

  async fn read_page_li(
    &self,
    tbl: &TableIdent,
    file_page_id_li: &[FilePageId],
    page_buf_li: &mut [Vec<u8>],
  ) -> Result<()> {
    // 转发而不是走默认逐页实现：内层可能一次提交多个 SQE。
    let result = self
      .track(self.inner.read_page_li(tbl, file_page_id_li, page_buf_li))
      .await;
    if result.is_ok() {
      let bytes: u64 = page_buf_li.iter().map(|page| page.len() as u64).sum();
      self.metrics.add_page_read(bytes);
    }
    result
  }

  async fn read_page_span(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    page_buf_li: &mut [Vec<u8>],
  ) -> Result<()> {
    // 转发而不是走默认逐页实现：内层能把连续页合并成一次 pread。
    let result = self
      .track(self.inner.read_page_span(tbl, file_page_id, page_buf_li))
      .await;
    if result.is_ok() {
      let bytes: u64 = page_buf_li.iter().map(|page| page.len() as u64).sum();
      self.metrics.add_page_read(bytes);
    }
    result
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    let result = self
      .track(self.inner.write_page(tbl, file_page_id, src))
      .await;
    if result.is_ok() {
      self.metrics.add_page_write(src.len() as u64);
    }
    result
  }

  async fn write_page_li(&self, tbl: &TableIdent, page_li: &[(FilePageId, Vec<u8>)]) -> Result<()> {
    let result = self.track(self.inner.write_page_li(tbl, page_li)).await;
    if result.is_ok() {
      let bytes: u64 = page_li.iter().map(|(_, page)| page.len() as u64).sum();
      self.metrics.add_page_write(bytes);
    }
    result
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.track(self.inner.sync_data(tbl)).await?;
    self.metrics.add_fsync();
    Ok(())
  }
}

impl<I: ManifestIo> ManifestIo for MeteredIo<I> {
  forward_io_metered! {
    async fn reset_manifest(&self, tbl: &TableIdent, size: u64) -> Result<()>;
    async fn reset_manifest_file(&self, tbl: &TableIdent, name: &str, size: u64) -> Result<()>;
    async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()>;
    /// 外部换主后的本地回放：必须下探到 Cloud 的 `read_local_manifest`，
    /// 不能走默认实现（默认会调 `read_manifest` 重新回源并触发采纳栅栏）。
    async fn read_local_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>>;
    /// 不额外读一次做指标：Cloud 内部已经下载并写入本地 cache。
    async fn refresh_manifest(&self, tbl: &TableIdent) -> Result<()>;
    async fn drop_local_manifest(&self, tbl: &TableIdent) -> Result<()>;
    async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>>;
    /// 旁路文件列举必须下探：默认实现已删除，漏转发会编译失败。
    async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>>;
    async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()>;
    async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()>;
    /// 晋升种子必须下探：默认走 `write_manifest_file` 会把 Cloud 的晋升对象
    /// 提前发布到远端（sem-60 写时晋升）。
    async fn seed_local_manifest_file(&self, tbl: &TableIdent, name: &str, bytes: &[u8]) -> Result<()>;
    /// 默认实现只走本地 list / remove，会绕过 Cloud 的远端剪枝。
    async fn prune_superseded_manifest_file_li(&self, tbl: &TableIdent, branch: &str, term: u64) -> Result<usize>;
  }

  // 手写例外：成功路径要累计 manifest 写入字节 / fsync / dir fsync 或读取字节。

  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    let result = self.track(self.inner.append_manifest(tbl, record)).await;
    if result.is_ok() {
      self.metrics.add_manifest_write(record.len() as u64);
      self.metrics.add_fsync();
    }
    result
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    let result = self.track(self.inner.switch_manifest(tbl, record)).await;
    if result.is_ok() {
      self.metrics.add_manifest_write(record.len() as u64);
      self.metrics.add_fsync();
      self.metrics.add_dir_fsync();
    }
    result
  }

  async fn append_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    let result = self
      .track(self.inner.append_manifest_file(tbl, name, record))
      .await;
    if result.is_ok() {
      self.metrics.add_manifest_write(record.len() as u64);
      self.metrics.add_fsync();
    }
    result
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    let result = self.track(self.inner.read_manifest(tbl)).await?;
    if let Some(bytes) = &result {
      self.metrics.add_manifest_read(bytes.len() as u64);
    }
    Ok(result)
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    let result = self
      .track(self.inner.write_manifest_file(tbl, name, record))
      .await;
    if result.is_ok() {
      self.metrics.add_manifest_write(record.len() as u64);
      self.metrics.add_fsync();
      self.metrics.add_dir_fsync();
    }
    result
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    let result = self.track(self.inner.read_manifest_file(tbl, name)).await?;
    if let Some(bytes) = &result {
      self.metrics.add_manifest_read(bytes.len() as u64);
    }
    Ok(result)
  }
}

impl<I: FileIo> FileIo for MeteredIo<I> {
  forward_io_metered! {
    async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>>;
    /// 默认实现会把 identity 全记为 None，丢掉文件名里的 (branch, term)。
    async fn list_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>>;
    /// LocalGc 必须落到后端的“只看本地”实现；透传到默认 list_file_variants
    /// 会让 Cloud 列举远端对象并在删除时触达远端。
    async fn list_local_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>>;
    async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>>;
    async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>>;
    async fn list_local_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>>;
    async fn list_local_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>>;
    async fn list_segment_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>>;
    async fn list_local_segment_file_variants(&self, tbl: &TableIdent) -> Result<Vec<FileVariant>>;
    async fn try_remove_empty_partition_dir(&self, tbl: &TableIdent) -> Result<bool>;
  }

  // 手写例外：成功路径要累计文件读写字节 / 删除次数。

  async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>> {
    let result = self.track(self.inner.read_file(tbl, file_id)).await?;
    if let Some(bytes) = &result {
      self.metrics.add_file_read(bytes.len() as u64);
    }
    Ok(result)
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()> {
    let result = self.track(self.inner.write_file(tbl, file_id, bytes)).await;
    if result.is_ok() {
      self.metrics.add_file_write(bytes.len() as u64);
    }
    result
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.track(self.inner.remove_file(tbl, file_id)).await?;
    self.metrics.add_file_remove();
    Ok(())
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self
      .track(self.inner.remove_segment_file(tbl, file_id))
      .await?;
    self.metrics.add_file_remove();
    Ok(())
  }

  async fn remove_local_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self
      .track(self.inner.remove_local_file(tbl, file_id))
      .await?;
    self.metrics.add_file_remove();
    Ok(())
  }

  async fn remove_local_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self
      .track(self.inner.remove_local_segment_file(tbl, file_id))
      .await?;
    self.metrics.add_file_remove();
    Ok(())
  }

  async fn remove_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .track(self.inner.remove_file_variant(tbl, file_id, branch, term))
      .await?;
    self.metrics.add_file_remove();
    Ok(())
  }

  async fn remove_local_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .track(
        self
          .inner
          .remove_local_file_variant(tbl, file_id, branch, term),
      )
      .await?;
    self.metrics.add_file_remove();
    Ok(())
  }

  async fn remove_segment_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .track(
        self
          .inner
          .remove_segment_file_variant(tbl, file_id, branch, term),
      )
      .await?;
    self.metrics.add_file_remove();
    Ok(())
  }

  async fn remove_local_segment_file_variant(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    branch: &str,
    term: u64,
  ) -> Result<()> {
    self
      .track(
        self
          .inner
          .remove_local_segment_file_variant(tbl, file_id, branch, term),
      )
      .await?;
    self.metrics.add_file_remove();
    Ok(())
  }
}

impl<I: SegmentIo> SegmentIo for MeteredIo<I> {
  forward_io_metered! {
    /// 定长段后端会补零到整段；不透传会把落盘字节数记少。
    fn write_segment_len(&self, src_len: usize) -> u64;
  }

  // 手写例外：成功路径要累计段读写字节。

  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    let result = self
      .track(self.inner.read_segment(tbl, segment_fp_id, dst))
      .await;
    if result.is_ok() {
      self.metrics.add_segment_read(dst.len() as u64);
    }
    result
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> Result<()> {
    let result = self
      .track(self.inner.write_segment(tbl, segment_fp_id, src))
      .await;
    if result.is_ok() {
      self
        .metrics
        .add_segment_write(self.inner.write_segment_len(src.len()));
    }
    result
  }
}

impl<I: FixedSegmentIo> FixedSegmentIo for MeteredIo<I> {
  forward_io_metered! {
    fn register_fixed_buffer(&self, len: usize) -> Result<FixedBuffer>;
  }

  // 手写例外：成功路径要累计 fixed 段读写字节。

  async fn read_segment_fixed(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    buf: &mut FixedBuffer,
    offset: usize,
    len: usize,
  ) -> Result<()> {
    let result = self
      .track(
        self
          .inner
          .read_segment_fixed(tbl, segment_fp_id, buf, offset, len),
      )
      .await;
    if result.is_ok() {
      self.metrics.add_segment_read(len as u64);
    }
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
    let result = self
      .track(
        self
          .inner
          .write_segment_fixed(tbl, segment_fp_id, buf, offset, len),
      )
      .await;
    if result.is_ok() {
      self
        .metrics
        .add_segment_write(self.inner.write_segment_len(len));
    }
    result
  }
}

impl<I: TermIo> TermIo for MeteredIo<I> {
  forward_io_metered! {
    async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>>;
    async fn upsert_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<u64>;
    async fn remove_term(&self, branch: &str, shard_id: u32) -> Result<()>;
  }
}
