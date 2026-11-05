//! 存储 IO 抽象。只定义 trait，具体后端在 `fkv_io_mem` / `fkv_io_compio`。

use core::{fmt, future::Future, ops::Deref, slice};
use std::sync::{
  Arc,
  atomic::{AtomicUsize, Ordering},
};

use fkv_buf::AlignedBuf;
use fkv_error::{KvError, Result};
use fkv_types::{BranchFileRange, FileId, FilePageId, TableIdent};

pub trait PageIo {
  /// 更新 data / segment 文件命名使用的 branch/term；不支持的后端忽略。
  fn set_file_identity(&self, _branch: &str, _term: u64) {}

  /// replay 后同步 per-file `(branch, term)` 归属链（对齐 C++ `SetBranchFileMapping`）。
  /// 远端对象后端据此为历史 file id 解析对象 key；本地后端按当前 identity 命名，忽略。
  fn set_file_range_li(&self, _tbl: &TableIdent, _range_li: &[BranchFileRange]) {}

  /// 预加载 file id -> 文件路径索引；返回索引到的文件数。默认 no-op。
  fn preload_path_li(&self, _tbl: &TableIdent) -> usize {
    0
  }

  /// 外部工具改动目录后主动失效路径索引。默认 no-op。
  fn invalidate_path_cache(&self) {}

  /// 运行期切换 page checksum 校验；不支持的后端忽略。默认 no-op。
  fn set_verify_checksum(&self, _enabled: bool) {}

  /// 驱逐某 table 的缓存状态（路径索引 / FD 缓存）；detach 时调用。默认 no-op。
  fn evict_table(&self, _tbl: &TableIdent) {}

  /// 读取一整页到 `dst` 前 `page_size` 字节（`dst` 其余区域不写）。
  ///
  /// 错误码契约（io-41）：
  /// - 资源不存在（文件 / 页从未落盘）→ `ResourceMissing`；
  /// - 后端能确定文件长度、请求越过 EOF（截断文件）→ `EndOfFile`；
  /// - 读到部分页或 checksum 校验失败 → `Corrupted`；
  /// - 底层 IO 错误 → `IoFail`。
  ///
  /// Mem / Sim 只记录已落盘页，没有文件长度概念，缺页统一报 `ResourceMissing`；
  /// Compio 对已存在但被截断的 data 文件报 `EndOfFile`（对齐 C++ `res == 0`）。
  fn read_page<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_page_id: FilePageId,
    dst: &'a mut [u8],
  ) -> impl Future<Output = Result<()>> + 'a;

  /// 批量读页，对齐 C++ `ReadPages`：`page_buf_li[i]` 接收
  /// `file_page_id_li[i]` 的整页数据（后端须整页覆写，不做部分保留）。
  /// 默认逐页串行；后端可一次提交多个 SQE 并发等待。
  fn read_page_li<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_page_id_li: &'a [FilePageId],
    page_buf_li: &'a mut [Vec<u8>],
  ) -> impl Future<Output = Result<()>> + 'a {
    async move {
      if file_page_id_li.len() != page_buf_li.len() {
        return Err(KvError::InvalidArgs);
      }
      for (file_page_id, buf) in file_page_id_li.iter().zip(page_buf_li.iter_mut()) {
        self.read_page(tbl, *file_page_id, buf).await?;
      }
      Ok(())
    }
  }

  fn write_page<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_page_id: FilePageId,
    src: &'a [u8],
  ) -> impl Future<Output = Result<()>> + 'a;

  /// 批量写页：默认逐页调用 `write_page`，后端可合并连续页为一次写。
  fn write_page_li<'a>(
    &'a self,
    tbl: &'a TableIdent,
    page_li: &'a [(FilePageId, Vec<u8>)],
  ) -> impl Future<Output = Result<()>> + 'a {
    async move {
      for (file_page_id, page) in page_li {
        self.write_page(tbl, *file_page_id, page).await?;
      }
      Ok(())
    }
  }

  fn sync_data<'a>(&'a self, tbl: &'a TableIdent) -> impl Future<Output = Result<()>> + 'a;
}

pub trait ManifestIo {
  /// reopen 后把追加偏移对齐到最后一条有效 record；默认 no-op。
  fn reset_manifest<'a>(
    &'a self,
    _tbl: &'a TableIdent,
    _size: u64,
  ) -> impl Future<Output = Result<()>> + 'a {
    async move { Ok(()) }
  }

  /// reopen 后把具名 manifest 文件（branch / archive）截断到最后一条有效 record；
  /// 默认 no-op，本地文件后端负责实现。
  fn reset_manifest_file<'a>(
    &'a self,
    _tbl: &'a TableIdent,
    _name: &'a str,
    _size: u64,
  ) -> impl Future<Output = Result<()>> + 'a {
    async move { Ok(()) }
  }

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

  /// 用远端当前 active manifest 覆盖本地缓存（采纳外部状态）。
  ///
  /// 默认 no-op：Local / Mem 没有远端来源。Cloud 后端必须覆写，
  /// 否则 `Store::install_external_snapshot` 会静默退回读本地旧 manifest。
  fn refresh_manifest<'a>(&'a self, _tbl: &'a TableIdent) -> impl Future<Output = Result<()>> + 'a {
    async move { Ok(()) }
  }

  /// 只删本地 manifest，不触碰远端对象（对齐 C++ `DropLocalManifest`）。
  ///
  /// 默认等同 [`Self::drop_manifest`]：Local / Mem 没有远端对象。
  /// Cloud 后端必须覆写，否则会误删远端 durability anchor。
  fn drop_local_manifest<'a>(
    &'a self,
    tbl: &'a TableIdent,
  ) -> impl Future<Output = Result<()>> + 'a {
    self.drop_manifest(tbl)
  }

  /// 写入非活跃 manifest 文件（branch manifest / archive）。
  fn write_manifest_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
    record: &'a [u8],
  ) -> impl Future<Output = Result<()>> + 'a;

  /// 向非活跃 manifest 文件（branch manifest）追加一条 log record。
  fn append_manifest_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
    record: &'a [u8],
  ) -> impl Future<Output = Result<()>> + 'a;

  fn list_manifest_file_name_li<'a>(
    &'a self,
    tbl: &'a TableIdent,
  ) -> impl Future<Output = Result<Vec<String>>> + 'a;

  /// 列举 partition 目录内**非 manifest** 的旁路文件名（升序、去重）。
  ///
  /// 供上层清理自己写入的旁路元数据（如云端 cache 的 segment 完成标记）：
  /// 这些名字不匹配 manifest 命名规则，`list_manifest_file_name_li` 不会返回。
  ///
  /// **无默认实现**：空实现会让包装层漏转发时静默失效（标记永久残留、分区
  /// 目录永不回收），每个后端与每个包装层都必须显式提供。
  fn list_sidecar_file_name_li<'a>(
    &'a self,
    tbl: &'a TableIdent,
  ) -> impl Future<Output = Result<Vec<String>>> + 'a;

  /// 删除 partition 目录内的一个旁路文件；不存在视为成功。
  ///
  /// 只触达本地（与 [`Self::remove_manifest_file`] 不同：云端后端不得转发到
  /// 远端对象）。完成标记这类旁路元数据从不随 manifest 上传，删本地即可。
  fn remove_sidecar_file_name<'a>(
    &'a self,
    tbl: &'a TableIdent,
    name: &'a str,
  ) -> impl Future<Output = Result<()>> + 'a;

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

  /// 剪枝被当前 term 取代的旧 term manifest（同一 active branch 的版本链）。
  ///
  /// 只删除同时满足以下条件的文件：无 tag、branch 等于 active branch、
  /// `term < 当前 term`；并且必须先看到当前 term 的 manifest 文件，否则不判定
  /// 旧文件已被取代。`main` 分支的活跃 manifest 文件名固定，**本地**不剪；
  /// 云端后端仍按远端最新 term 剪掉被取代的旧 term 对象
  /// （见 `CloudStore::prune_superseded_manifest_file_li`）。
  ///
  /// 默认实现只经 `list` / `remove_manifest_file` 操作本地文件。云端后端删除
  /// 会触达远端对象，覆写时必须先确认本实例 term 是远端最新 term，且只删更旧
  /// 的无 tag manifest（见 `CloudStore::prune_superseded_manifest_file_li`）。
  fn prune_superseded_manifest_file_li<'a>(
    &'a self,
    tbl: &'a TableIdent,
    branch: &'a str,
    term: u64,
  ) -> impl Future<Output = Result<usize>> + 'a {
    async move {
      // main 的活跃 manifest 固定为 `manifest_main_0`，文件名不随 term 变化；
      // term == 0 时也不存在更旧的 term。
      if branch == fkv_types::MAIN_BRANCH_NAME || term == 0 {
        return Ok(0);
      }
      let active = fkv_types::manifest_file_name(branch, term);
      let name_li = self.list_manifest_file_name_li(tbl).await?;
      if !name_li.iter().any(|name| name == &active) {
        return Ok(0);
      }
      let mut removed = 0;
      for name in name_li {
        if name == active {
          continue;
        }
        let Some((file_branch, file_term, tag)) = fkv_types::parse_manifest_file_name(&name) else {
          continue;
        };
        if tag.is_none() && file_branch == branch && file_term < term {
          self.remove_manifest_file(tbl, &name).await?;
          removed += 1;
        }
      }
      Ok(removed)
    }
  }
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

  /// 删除 branch 的 term 文件（对齐 C++ `DeleteBranchFiles`）。不存在视为成功。
  fn remove_term<'a>(
    &'a self,
    branch: &'a str,
    shard_id: u32,
  ) -> impl Future<Output = Result<()>> + 'a;
}

/// 已注册 / 可注册到 io_uring fixed buffer 表的内存。
pub struct FixedBuffer {
  buf: Arc<AlignedBuf>,
  /// 在飞 fixed op 数：租约随 op 存活，op 完成（含取消后 CQE 确认）才递减。
  inflight: Arc<AtomicUsize>,
  index: u16,
  registered: bool,
  on_drop: Option<Arc<dyn Fn(u16) + Send + Sync>>,
}

impl fmt::Debug for FixedBuffer {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("FixedBuffer")
      .field("len", &self.buf.len())
      .field("inflight", &self.inflight.load(Ordering::Relaxed))
      .field("index", &self.index)
      .field("registered", &self.registered)
      .finish()
  }
}

impl FixedBuffer {
  /// 未注册 buffer（MemStore / 非 Linux fallback）。
  pub fn new(len: usize) -> Self {
    Self::from_arc(Arc::new(AlignedBuf::new(len)), 0, false, None)
  }

  /// 注册并携带槽位归还回调；`Drop` 时把 index 还给后端。
  pub fn registered_arc_with_hook(
    buf: Arc<AlignedBuf>,
    index: u16,
    on_drop: Arc<dyn Fn(u16) + Send + Sync>,
  ) -> Self {
    Self::from_arc(buf, index, true, Some(on_drop))
  }

  fn from_arc(
    buf: Arc<AlignedBuf>,
    index: u16,
    registered: bool,
    on_drop: Option<Arc<dyn Fn(u16) + Send + Sync>>,
  ) -> Self {
    Self {
      buf,
      inflight: Arc::new(AtomicUsize::new(0)),
      index,
      registered,
      on_drop,
    }
  }

  pub fn len(&self) -> usize {
    self.buf.len()
  }

  pub fn is_empty(&self) -> bool {
    self.buf.is_empty()
  }

  /// 只读视图。调用方须确保没有在飞 fixed op（租约已随 CQE 收割释放），
  /// 否则内核可能在并发改写同一缓冲，读到撕裂数据。
  pub fn as_slice(&self) -> &[u8] {
    debug_assert_eq!(
      self.inflight.load(Ordering::Acquire),
      0,
      "仍有在飞 fixed op 时读取缓冲"
    );
    &self.buf
  }

  /// 独占借出底层内存给一次 fixed IO；返回的租约随 op 存活，op 完成（CQE 收割）
  /// 后才释放，期间 [`Self::as_mut_slice`] / 再次 `share` 都拒绝。
  ///
  /// compio 取消不可靠：外层 future 被 drop 后 SQE 仍在 ring 中，此时重试同一
  /// 缓冲会与未确认完成的 op 竞争，故在飞时返回 `None`，调用方按 `Busy` 重试。
  pub fn share(&self) -> Option<BufLease> {
    BufLease::try_new(self.buf.clone(), Arc::clone(&self.inflight))
  }

  /// 按区间借出底层内存：允许多个 fixed op 同时挂在同一缓冲上，调用方须保证
  /// 各 op 访问互不重叠的字节区间（批量段写即此场景）。
  ///
  /// 租约仍会抬高在飞计数，因此期间 [`Self::as_mut_slice`] 依旧拒绝改写。
  pub fn share_disjoint(&self) -> BufLease {
    BufLease::new(self.buf.clone(), Arc::clone(&self.inflight))
  }

  /// 可写切片；仍有未确认完成的 fixed op 租约时返回 `None`。
  ///
  /// compio 取消不可靠：外层 future 被 drop 后 SQE 仍在 ring 中，直接改写同一
  /// 缓冲会与在飞 IO 竞争。调用方应把 `None` 当作可重试（`Busy`）。
  pub fn as_mut_slice(&mut self) -> Option<&mut [u8]> {
    if self.inflight.load(Ordering::Acquire) != 0 {
      return None;
    }
    // SAFETY: `&mut self` 保证没有其他 Rust 引用同时访问；在飞 op 已归零，
    // 且新 op 需要重新调用 `share`（需 `&self`，与 `&mut` 借用互斥）。
    Some(unsafe { slice::from_raw_parts_mut(self.buf.as_ptr().cast_mut(), self.buf.len()) })
  }

  pub fn is_registered(&self) -> bool {
    self.registered
  }

  pub fn index(&self) -> u16 {
    self.index
  }
}

/// fixed op 持有的缓冲租约。
///
/// 与 [`FixedBuffer`] 共享在飞计数，op 只在驱动收割完成事件（含取消确认）
/// 后 drop，因此 `Drop` 即「缓冲可安全改写」的确认点。
pub struct BufLease {
  buf: Arc<AlignedBuf>,
  /// `None` 表示独立缓冲（未关联 `FixedBuffer`，不做在飞计数）。
  inflight: Option<Arc<AtomicUsize>>,
}

impl BufLease {
  /// 在飞计数为 0 时占位成功；已有在飞 op 时返回 `None`（调用方按 `Busy` 重试）。
  fn try_new(buf: Arc<AlignedBuf>, inflight: Arc<AtomicUsize>) -> Option<Self> {
    inflight
      .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
      .ok()?;
    Some(Self {
      buf,
      inflight: Some(inflight),
    })
  }

  /// 并发租约：只抬高在飞计数，不做独占判定。
  fn new(buf: Arc<AlignedBuf>, inflight: Arc<AtomicUsize>) -> Self {
    inflight.fetch_add(1, Ordering::AcqRel);
    Self {
      buf,
      inflight: Some(inflight),
    }
  }

  /// 独立缓冲租约（单测 / 无 owner 场景），不做在飞计数。
  pub fn detached(buf: Arc<AlignedBuf>) -> Self {
    Self {
      buf,
      inflight: None,
    }
  }
}

impl Deref for BufLease {
  type Target = [u8];

  fn deref(&self) -> &Self::Target {
    &self.buf
  }
}

impl Drop for BufLease {
  fn drop(&mut self) {
    if let Some(inflight) = &self.inflight {
      inflight.fetch_sub(1, Ordering::AcqRel);
    }
  }
}

impl Drop for FixedBuffer {
  fn drop(&mut self) {
    if self.registered
      && let Some(hook) = &self.on_drop
    {
      hook(self.index);
    }
  }
}

/// 批量 fixed 读计划项：`(段物理 id, 缓冲偏移, 长度)`。
///
/// 各项读入同一 [`FixedBuffer`] 的 `[offset, offset + len)` 区间；区间必须
/// 互不重叠（与 [`FixedBuffer::share_disjoint`] 同一借用契约，由
/// [`validate_fixed_segment_li`] 强制）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedSegmentRead {
  /// 段所在 segment 文件的物理页 id。
  pub segment_fp_id: FilePageId,
  /// 写入注册缓冲的起始偏移。
  pub offset: usize,
  /// 读取字节数；尾读零填充语义与 [`FixedSegmentIo::read_segment_fixed`] 一致。
  pub len: usize,
}

/// 校验批量 fixed 读计划：区间不越界、互不重叠。
///
/// 并发后端对同一注册缓冲同时提交多个在飞 op，重叠区间是内核并发写同一
/// 字节的真数据竞争，必须在提交前整批拒绝。`len` 超过后端段长的拒绝由
/// 各后端自行补充（默认实现经逐段 `read_segment_fixed` 兜住）。
pub fn validate_fixed_segment_li(buf_len: usize, item_li: &[FixedSegmentRead]) -> Result<()> {
  let mut range_li: Vec<(usize, usize)> = Vec::with_capacity(item_li.len());
  for item in item_li {
    let end = item
      .offset
      .checked_add(item.len)
      .ok_or(KvError::InvalidArgs)?;
    if end > buf_len {
      return Err(KvError::InvalidArgs);
    }
    // 空区间不占字节，不参与重叠判定。
    if item.len > 0 {
      range_li.push((item.offset, end));
    }
  }
  // 按起点排序后相邻比较即可覆盖全部重叠对：若 i < j 重叠，则二者之间的
  // 相邻对必有一对重叠（起点单调，end_i > start_j >= start_{i+1}）。
  range_li.sort_unstable_by_key(|&(start, _)| start);
  if range_li.windows(2).any(|pair| pair[0].1 > pair[1].0) {
    return Err(KvError::InvalidArgs);
  }
  Ok(())
}

pub trait FixedSegmentIo: SegmentIo {
  fn register_fixed_buffer(&self, len: usize) -> Result<FixedBuffer>;

  /// 读 `buf[offset..offset + len]`，尾读语义与 [`SegmentIo::read_segment`] 相同：
  /// 段文件短于请求时读取可用前缀，剩余部分零填充并返回 `Ok(())`。
  ///
  /// `offset + len > buf.len()` 或 `len > segment_size` → `InvalidArgs`；
  /// 缓冲区仍有未确认完成的 fixed op（compio）→ `Busy`；
  /// 段文件不存在 → `ResourceMissing`；底层 IO 错误 → `IoFail`。
  fn read_segment_fixed<'a>(
    &'a self,
    tbl: &'a TableIdent,
    segment_fp_id: FilePageId,
    buf: &'a mut FixedBuffer,
    offset: usize,
    len: usize,
  ) -> impl Future<Output = Result<()>> + 'a;

  /// 批量 fixed 读：`item_li[i]` 读入 `buf` 的 `[offset, offset + len)` 区间，
  /// 对齐 C++ `ReadSegments`。
  ///
  /// 各区间必须互不重叠（[`validate_fixed_segment_li`] 整批强制）：已注册
  /// 缓冲上多个 READ_FIXED 以 `share_disjoint` 区间租约同时提交，重叠区间
  /// 是内核并发写同一字节的真数据竞争。尾读零填充语义逐条与
  /// [`Self::read_segment_fixed`] 相同；任一项失败即中止剩余项。
  ///
  /// 默认逐段串行回退；后端可对已注册缓冲一次提交多个 READ_FIXED 并发
  /// 等待（批大小由调用方约束，同 [`PageIo::read_page_li`] 的约定）。
  fn read_segment_fixed_li<'a>(
    &'a self,
    tbl: &'a TableIdent,
    item_li: &'a [FixedSegmentRead],
    buf: &'a mut FixedBuffer,
  ) -> impl Future<Output = Result<()>> + 'a {
    async move {
      validate_fixed_segment_li(buf.len(), item_li)?;
      for item in item_li {
        self
          .read_segment_fixed(tbl, item.segment_fp_id, buf, item.offset, item.len)
          .await?;
      }
      Ok(())
    }
  }

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
  /// 单段写入实际落盘的字节数：定长段后端会补零到整段（io-28）。
  fn write_segment_len(&self, src_len: usize) -> u64 {
    src_len as u64
  }

  /// 读一个定长段的 `dst.len()` 字节。
  ///
  /// 段按 `segment_size` 定长落盘、尾部补零，因此「文件短于请求」不是数据损坏：
  /// 后端读取实际可用前缀，把 `dst` 剩余部分填 0 并返回 `Ok(())`（plain 与
  /// [`FixedSegmentIo::read_segment_fixed`] 一致）。
  ///
  /// 错误码契约（io-41）：
  /// - 段文件不存在 → `ResourceMissing`；
  /// - `dst.len() > segment_size` → `InvalidArgs`；
  /// - `dst` 为空 → `Ok(())`（不要求段存在）；
  /// - 底层 IO 错误 → `IoFail`。
  ///
  /// 短读不返回 `EndOfFile` / `Corrupted`。
  fn read_segment<'a>(
    &'a self,
    tbl: &'a TableIdent,
    segment_fp_id: FilePageId,
    dst: &'a mut [u8],
  ) -> impl Future<Output = Result<()>> + 'a;

  /// 写一个段；定长段后端把尾部补零到 `segment_size`（见 [`Self::write_segment_len`]）。
  fn write_segment<'a>(
    &'a self,
    tbl: &'a TableIdent,
    segment_fp_id: FilePageId,
    src: &'a [u8],
  ) -> impl Future<Output = Result<()>> + 'a;
}

/// data / segment 文件的物理变体身份（rev-39）。
///
/// 文件名 / 对象名自带 `<prefix>_<file_id>_<branch>_<term>`：branch reserve
/// 桶内 main 与 branch 从 fork 点起各自分配，同一 `file_id` 可分属不同
/// `(branch, term)` 的多个物理文件。GC 按变体粒度判删，必须能枚举每个物理
/// 变体的三元组。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileVariant {
  pub file_id: FileId,
  /// 文件名自带的 `(branch, term)`。后端没有变体概念时（纯内存 / 未透传的
  /// 包装层）为 `None`：GC 回退到区间归属判定，行为与 rev-39 之前一致。
  pub identity: Option<(Box<str>, u64)>,
}

impl FileVariant {
  /// 无身份变体：单变体后端的占位（每个 file id 至多一个物理文件）。
  pub fn unknown(file_id: FileId) -> Self {
    Self {
      file_id,
      identity: None,
    }
  }
}

pub trait FileIo {
  /// 列出该 table 已存在的 data file id：**升序、去重**。
  ///
  /// 依赖顺序的 GC 遍历 / 确定性测试可假定该顺序（io-41）。
  fn list_file_id_li<'a>(
    &'a self,
    tbl: &'a TableIdent,
  ) -> impl Future<Output = Result<Vec<FileId>>> + 'a;

  /// 列出该 table 全部 data 文件变体：按 `(file_id, branch, term)` **升序、去重**。
  ///
  /// 同 `file_id` 的跨 `(branch, term)` 变体各占一条（rev-39）。默认实现没有
  /// 文件名身份（纯内存 / 未透传的包装层），回退 [`Self::list_file_id_li`] 并把
  /// identity 记为 `None`；有文件名 / 对象名的后端必须覆写。
  fn list_file_variants<'a>(
    &'a self,
    tbl: &'a TableIdent,
  ) -> impl Future<Output = Result<Vec<FileVariant>>> + 'a {
    async move {
      Ok(
        self
          .list_file_id_li(tbl)
          .await?
          .into_iter()
          .map(FileVariant::unknown)
          .collect(),
      )
    }
  }

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

  /// 列出该 table 已存在的 segment file id：**升序、去重**（io-41）。
  fn list_segment_file_id_li<'a>(
    &'a self,
    tbl: &'a TableIdent,
  ) -> impl Future<Output = Result<Vec<FileId>>> + 'a;

  fn remove_segment_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
  ) -> impl Future<Output = Result<()>> + 'a;

  /// 列出**本地**已存在的 data file id：**升序、去重**（io-41）。
  ///
  /// 「本地」的语义随后端：本地后端就是盘上文件，云端后端是本地 cache 目录，
  /// **不含**远端对象。`LocalGc` 只按本方法列举 / [`Self::remove_local_file`]
  /// 删除，保证不触达远端（对齐 C++ `ExecuteLocalGC`）。
  ///
  /// 默认实现同 [`Self::list_file_id_li`]：本地 / 内存后端没有远端副本。
  fn list_local_file_id_li<'a>(
    &'a self,
    tbl: &'a TableIdent,
  ) -> impl Future<Output = Result<Vec<FileId>>> + 'a {
    self.list_file_id_li(tbl)
  }

  /// 只删除本地 data 文件；云端后端不得删除远端对象。
  ///
  /// 默认实现同 [`Self::remove_file`]。
  fn remove_local_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
  ) -> impl Future<Output = Result<()>> + 'a {
    self.remove_file(tbl, file_id)
  }

  /// 列出**本地**已存在的 segment file id：**升序、去重**（io-41）。
  ///
  /// 语义同 [`Self::list_local_file_id_li`]。默认实现同
  /// [`Self::list_segment_file_id_li`]。
  fn list_local_segment_file_id_li<'a>(
    &'a self,
    tbl: &'a TableIdent,
  ) -> impl Future<Output = Result<Vec<FileId>>> + 'a {
    self.list_segment_file_id_li(tbl)
  }

  /// 只删除本地 segment 文件；云端后端不得删除远端对象。
  ///
  /// 默认实现同 [`Self::remove_segment_file`]。
  fn remove_local_segment_file<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
  ) -> impl Future<Output = Result<()>> + 'a {
    self.remove_segment_file(tbl, file_id)
  }

  /// 只删除**一个**物理变体：`(file_id, branch, term)` 精确匹配文件名（rev-39）。
  ///
  /// 同 id 的其它 `(branch, term)` 变体不受影响——跨 branch 同号 file id 各自
  /// 独立判删。默认实现没有变体概念，退化为 [`Self::remove_file`]（整 id 删除，
  /// 与 rev-39 之前一致）。
  fn remove_file_variant<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
    _branch: &'a str,
    _term: u64,
  ) -> impl Future<Output = Result<()>> + 'a {
    self.remove_file(tbl, file_id)
  }

  /// 只删除**一个** segment 物理变体；语义同 [`Self::remove_file_variant`]。
  fn remove_segment_file_variant<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
    _branch: &'a str,
    _term: u64,
  ) -> impl Future<Output = Result<()>> + 'a {
    self.remove_segment_file(tbl, file_id)
  }

  /// 列出该 table 全部 segment 文件变体；语义同 [`Self::list_file_variants`]。
  ///
  /// 默认实现回退 [`Self::list_segment_file_id_li`]，identity 记为 `None`。
  fn list_segment_file_variants<'a>(
    &'a self,
    tbl: &'a TableIdent,
  ) -> impl Future<Output = Result<Vec<FileVariant>>> + 'a {
    async move {
      Ok(
        self
          .list_segment_file_id_li(tbl)
          .await?
          .into_iter()
          .map(FileVariant::unknown)
          .collect(),
      )
    }
  }

  /// 列出**本地** data 文件变体；语义同 [`Self::list_local_file_id_li`]。
  ///
  /// 默认实现同 [`Self::list_file_variants`]：本地 / 内存后端没有远端副本。
  fn list_local_file_variants<'a>(
    &'a self,
    tbl: &'a TableIdent,
  ) -> impl Future<Output = Result<Vec<FileVariant>>> + 'a {
    self.list_file_variants(tbl)
  }

  /// 只删除**本地**的一个物理变体；云端后端不得删除远端对象。
  ///
  /// 默认实现同 [`Self::remove_file_variant`]。
  fn remove_local_file_variant<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
    branch: &'a str,
    term: u64,
  ) -> impl Future<Output = Result<()>> + 'a {
    self.remove_file_variant(tbl, file_id, branch, term)
  }

  /// 列出**本地** segment 文件变体；语义同 [`Self::list_local_file_variants`]。
  fn list_local_segment_file_variants<'a>(
    &'a self,
    tbl: &'a TableIdent,
  ) -> impl Future<Output = Result<Vec<FileVariant>>> + 'a {
    self.list_segment_file_variants(tbl)
  }

  /// 只删除**本地**的一个 segment 物理变体；语义同 [`Self::remove_local_file_variant`]。
  fn remove_local_segment_file_variant<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_id: FileId,
    branch: &'a str,
    term: u64,
  ) -> impl Future<Output = Result<()>> + 'a {
    self.remove_segment_file_variant(tbl, file_id, branch, term)
  }

  /// 分区目录已无任何残留（data / segment / manifest / 临时文件）时移除该目录。
  ///
  /// 对齐 C++ `TryCleanupLocalPartitionDir`：目录非空一律保留，返回是否删除。
  /// 默认 no-op（内存后端没有目录；云端本地缓存目录由云端统一管理）。
  fn try_remove_empty_partition_dir<'a>(
    &'a self,
    _tbl: &'a TableIdent,
  ) -> impl Future<Output = Result<bool>> + 'a {
    async move { Ok(false) }
  }
}
