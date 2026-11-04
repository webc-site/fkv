//! 存储 IO 抽象。只定义 trait，具体后端在 `fkv_io_mem` / `fkv_io_compio`。

use core::{future::Future, slice};
use std::sync::Arc;

use fkv_buf::AlignedBuf;
use fkv_error::Result;
use fkv_types::{FileId, FilePageId, TableIdent};

pub trait PageIo {
  /// 更新 data / segment 文件命名使用的 branch/term；不支持的后端忽略。
  fn set_file_identity(&self, _branch: &str, _term: u64) {}

  /// 预加载 file id -> 文件路径索引；返回索引到的文件数。默认 no-op。
  fn preload_paths(&self, _tbl: &TableIdent) -> usize {
    0
  }

  /// 外部工具改动目录后主动失效路径索引。默认 no-op。
  fn invalidate_path_cache(&self) {}

  fn read_page<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_page_id: FilePageId,
    dst: &'a mut [u8],
  ) -> impl Future<Output = Result<()>> + 'a;

  fn write_page<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_page_id: FilePageId,
    src: &'a [u8],
  ) -> impl Future<Output = Result<()>> + 'a;

  fn sync_data<'a>(&'a self, tbl: &'a TableIdent) -> impl Future<Output = Result<()>> + 'a;
}

pub trait ManifestIo {
  fn append_manifest<'a>(
    &'a self,
    tbl: &'a TableIdent,
    record: &'a [u8],
  ) -> impl Future<Output = Result<()>> + 'a;

  fn switch_manifest<'a>(
    &'a self,
    tbl: &'a TableIdent,
    record: &'a [u8],
  ) -> impl Future<Output = Result<()>> + 'a;

  fn read_manifest<'a>(
    &'a self,
    tbl: &'a TableIdent,
  ) -> impl Future<Output = Result<Option<Vec<u8>>>> + 'a;

  fn drop_manifest<'a>(&'a self, tbl: &'a TableIdent) -> impl Future<Output = Result<()>> + 'a;

  /// 写入非活跃 manifest 文件（branch manifest / archive）。
  fn write_manifest_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
    record: &'a [u8],
  ) -> impl Future<Output = Result<()>> + 'a;

  fn list_manifest_file_name_li<'a>(
    &'a self,
    tbl: &'a TableIdent,
  ) -> impl Future<Output = Result<Vec<String>>> + 'a;

  fn read_manifest_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
  ) -> impl Future<Output = Result<Option<Vec<u8>>>> + 'a;

  fn remove_manifest_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
  ) -> impl Future<Output = Result<()>> + 'a;
}

pub trait TermIo {
  fn read_term<'a>(
    &'a self,
    branch: &'a str,
    shard_id: u32,
  ) -> impl Future<Output = Result<Option<u64>>> + 'a;

  fn upsert_term<'a>(
    &'a self,
    branch: &'a str,
    shard_id: u32,
    term: u64,
  ) -> impl Future<Output = Result<u64>> + 'a;
}

pub const DEFAULT_SEGMENT_SIZE: usize = 256 * 1024;
pub const DEFAULT_SEGMENTS_PER_FILE_SHIFT: u8 = 7;

/// 已注册 / 可注册到 io_uring fixed buffer 表的内存。
#[derive(Debug)]
pub struct FixedBuffer {
  buf: Arc<AlignedBuf>,
  index: u16,
  registered: bool,
}

impl FixedBuffer {
  /// 未注册 buffer（MemStore / 非 Linux fallback）。
  pub fn new(len: usize) -> Self {
    Self::from_arc(Arc::new(AlignedBuf::new(len)), 0, false)
  }

  /// 由后端注册后构造。
  pub fn registered(buf: AlignedBuf, index: u16) -> Self {
    Self::from_arc(Arc::new(buf), index, true)
  }

  /// 由后端注册后构造（保留共享指针，供 in-flight IO 持有生命周期）。
  pub fn registered_arc(buf: Arc<AlignedBuf>, index: u16) -> Self {
    Self::from_arc(buf, index, true)
  }

  fn from_arc(buf: Arc<AlignedBuf>, index: u16, registered: bool) -> Self {
    Self {
      buf,
      index,
      registered,
    }
  }

  pub fn len(&self) -> usize {
    self.buf.len()
  }

  pub fn is_empty(&self) -> bool {
    self.buf.is_empty()
  }

  pub fn as_slice(&self) -> &[u8] {
    &self.buf
  }

  /// 共享底层内存给 in-flight fixed IO；调用方必须保证在 IO 完成前不并发写。
  pub fn share(&self) -> Arc<AlignedBuf> {
    self.buf.clone()
  }

  pub fn as_mut_slice(&mut self) -> &mut [u8] {
    // SAFETY: `&mut self` 保证没有其他 Rust 引用同时访问；fixed IO 期间调用方
    // 不会调用本方法（IO future 持有 `&mut FixedBuffer` 借用）。
    unsafe { slice::from_raw_parts_mut(self.buf.as_ptr().cast_mut(), self.buf.len()) }
  }

  pub fn is_registered(&self) -> bool {
    self.registered
  }

  pub fn index(&self) -> u16 {
    self.index
  }
}

pub trait FixedSegmentIo: SegmentIo {
  fn register_fixed_buffer(&self, len: usize) -> Result<FixedBuffer>;

  fn read_segment_fixed<'a>(
    &'a self,
    tbl: &'a TableIdent,
    segment_fp_id: FilePageId,
    buf: &'a mut FixedBuffer,
    offset: usize,
    len: usize,
  ) -> impl Future<Output = Result<()>> + 'a;

  fn write_segment_fixed<'a>(
    &'a self,
    tbl: &'a TableIdent,
    segment_fp_id: FilePageId,
    buf: &'a FixedBuffer,
    offset: usize,
    len: usize,
  ) -> impl Future<Output = Result<()>> + 'a;
}

pub trait SegmentIo {
  fn read_segment<'a>(
    &'a self,
    tbl: &'a TableIdent,
    segment_fp_id: FilePageId,
    dst: &'a mut [u8],
  ) -> impl Future<Output = Result<()>> + 'a;

  fn write_segment<'a>(
    &'a self,
    tbl: &'a TableIdent,
    segment_fp_id: FilePageId,
    src: &'a [u8],
  ) -> impl Future<Output = Result<()>> + 'a;
}

pub trait FileIo {
  fn list_file_id_li<'a>(
    &'a self,
    tbl: &'a TableIdent,
  ) -> impl Future<Output = Result<Vec<FileId>>> + 'a;

  /// 读取整个 data 文件（空洞补零）；文件不存在返回 `None`。
  fn read_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
  ) -> impl Future<Output = Result<Option<Vec<u8>>>> + 'a;

  /// 以整文件粒度写入 / 覆盖 data 文件。调用方保证长度是 page_size 的倍数。
  fn write_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
    bytes: &'a [u8],
  ) -> impl Future<Output = Result<()>> + 'a;

  /// 本地文件字节数；不存在返回 `None`。
  fn file_size<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
  ) -> impl Future<Output = Result<Option<u64>>> + 'a;

  fn remove_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
  ) -> impl Future<Output = Result<()>> + 'a;

  fn list_segment_file_id_li<'a>(
    &'a self,
    tbl: &'a TableIdent,
  ) -> impl Future<Output = Result<Vec<FileId>>> + 'a;

  fn remove_segment_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
  ) -> impl Future<Output = Result<()>> + 'a;
}
