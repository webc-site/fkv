//! `PageIo` 实现：页读 / 批读 / 页写 / 批量写 / sync_data。

use std::{
  io::{self, ErrorKind},
  path::PathBuf,
  sync::atomic::Ordering,
};

use compio::fs::File;
use fkv_error::{KvError, Result};
use fkv_failpoint::points::WRITE_PAGE;
use fkv_io::PageIo;
use fkv_types::{FilePageId, TableIdent, slow_read};
use futures::{StreamExt, future::try_join_all, stream};

use crate::{CandidateRead, CompioStore, DirectBuf, FileIdentity, PageDst, WriteChunk, map_io_err};

impl PageIo for CompioStore {
  fn set_file_identity(&self, branch: &str, term: u64) {
    self.dir_index_map.borrow_mut().clear();
    self.fd_cache.borrow_mut().clear();
    *self.identity.borrow_mut() = FileIdentity {
      branch: branch.to_owned(),
      term,
    };
  }

  fn preload_path_li(&self, tbl: &TableIdent) -> usize {
    self.preload(tbl)
  }

  fn invalidate_path_cache(&self) {
    self.clear_caches();
  }

  fn set_verify_checksum(&self, enabled: bool) {
    self.skip_verify_checksum.set(!enabled);
  }

  fn evict_table(&self, tbl: &TableIdent) {
    self.forget_table_state(tbl);
  }

  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    self
      .read_page_dst(tbl, file_page_id, PageDst::Slice(dst))
      .await
  }

  /// perf-10：单页 owned 直读——非 O_DIRECT 时调用方 `Vec` 直接作为读目标
  /// （与 [`Self::read_page_li`] 共用 `PageDst::Owned` 路径），省每页一次整页
  /// 拷贝与一次读池槽周转；O_DIRECT 需页对齐缓冲，退回池缓冲 + 拷贝。
  async fn read_page_owned(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    buf: &mut Vec<u8>,
  ) -> Result<()> {
    self
      .read_page_dst(tbl, file_page_id, PageDst::Owned(buf))
      .await
  }

  /// 批内每页各压一个 SQE 后统一等待（对齐 C++ `ReadPages`）；批大小由
  /// overflow 链每层指针数决定（≤ `overflow_pointers`），天然有界。
  ///
  /// io-06：批读目标本就是调用方拥有的 Vec——非 O_DIRECT 时 `mem::take`
  /// 交给内核直读（详见 [`Self::read_page_candidate`]），省每页一次整页
  /// 拷贝与一次读池槽周转。
  async fn read_page_li(
    &self,
    tbl: &TableIdent,
    file_page_id_li: &[FilePageId],
    page_buf_li: &mut [Vec<u8>],
  ) -> Result<()> {
    if file_page_id_li.len() != page_buf_li.len() {
      return Err(KvError::InvalidArgs);
    }
    let batch_start = slow_read::start();
    try_join_all(
      file_page_id_li
        .iter()
        .zip(page_buf_li.iter_mut())
        .map(|(file_page_id, buf)| self.read_page_dst(tbl, *file_page_id, PageDst::Owned(buf))),
    )
    .await?;
    slow_read::stop("batch_wait", batch_start);
    Ok(())
  }

  /// perf-11：连续页合并读（见 [`fkv_io::PageIo::read_page_span`]）。
  ///
  /// 按物理文件切段（文件内页号才连续），每段先试一次合并 `pread`；段内任何
  /// 异常（跨文件 / 候选不唯一 / EOF / 短读 / 校验不过）都退回逐页读，因此
  /// 成功值与错误码与逐页调用完全一致。叶子链窗口按页号连续取数，链上实测
  /// 99% 的相邻页物理页号 +1（同段分配），合并把「每页一次页 IO」压成「每段
  /// 一次页 IO」：请求内页读次数 = 链页数 → ceil(链页数 / 窗口深度)。
  async fn read_page_span(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    page_buf_li: &mut [Vec<u8>],
  ) -> Result<()> {
    let span_start = slow_read::start();
    let shift = self.pages_per_file_shift;
    let mask = (1u64 << shift) - 1;
    let mut offset = 0usize;
    while offset < page_buf_li.len() {
      let start = file_page_id + offset as u64;
      // 本段必须落在同一物理文件内：页号到文件末尾的剩余页数即段长上界。
      let file_left = (1u64 << shift) - (start & mask);
      let run_end = usize::try_from(file_left)
        .unwrap_or(usize::MAX)
        .min(page_buf_li.len() - offset)
        .max(1);
      let end = offset + run_end;
      let dst = &mut page_buf_li[offset..end];
      if !self.read_page_run(tbl, start, dst).await? {
        for (index, buf) in dst.iter_mut().enumerate() {
          self.read_page_owned(tbl, start + index as u64, buf).await?;
        }
      }
      offset = end;
    }
    slow_read::stop("batch_wait", span_start);
    Ok(())
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    if src.len() != self.page_size {
      return Err(KvError::InvalidArgs);
    }
    fkv_failpoint::fail_point!(WRITE_PAGE);
    fkv_failpoint::kill_point(WRITE_PAGE);
    self.ensure_dir(tbl).await?;
    let path = self.data_path(tbl, file_page_id);
    let mut file = self.open_write_file(&path).await.map_err(map_io_err)?;
    let chunk = if self.direct_io.get() {
      WriteChunk::Direct(DirectBuf::from_slice(self.take_direct_buf(src.len()), src))
    } else {
      let mut buf = self.take_write_buf(src.len());
      debug_assert_eq!(buf.len(), src.len());
      buf.copy_from_slice(src);
      WriteChunk::Plain(buf)
    };
    self.write_at_num.fetch_add(1, Ordering::Relaxed);
    let write_len = self
      .write_chunk(&mut file, chunk, self.page_offset(file_page_id))
      .await
      .map_err(map_io_err)?;
    if write_len != src.len() {
      return Err(KvError::IoFail);
    }
    self.mark_dirty(tbl, path);
    Ok(())
  }

  async fn write_page_li(&self, tbl: &TableIdent, page_li: &[(FilePageId, Vec<u8>)]) -> Result<()> {
    for (_, page) in page_li {
      if page.len() != self.page_size {
        return Err(KvError::InvalidArgs);
      }
      fkv_failpoint::fail_point!(WRITE_PAGE);
      fkv_failpoint::kill_point(WRITE_PAGE);
    }
    self.ensure_dir(tbl).await?;
    // 按 (file_id, page_index) 排序，连续页合并为一次 write_at。
    let mut sorted: Vec<(u64, u64, &Vec<u8>)> = page_li
      .iter()
      .map(|(file_page_id, page)| {
        let file_id = file_page_id >> self.pages_per_file_shift;
        let page_index = file_page_id & ((1u64 << self.pages_per_file_shift) - 1);
        (file_id, page_index, page)
      })
      .collect();
    sorted.sort_unstable_by_key(|(file_id, page_index, _)| (*file_id, *page_index));
    // 按连续段切组，页切片只借用不拷贝；缓冲在组 future 内按需借出，
    // 避免先把整批页拷进临时大 Vec。组内仍是一次连续写，页数受
    // `max_write_batch_pages` 约束。
    let max_pages = self.max_write_batch_pages;
    let mut group_li: Vec<(PathBuf, u64, Vec<&[u8]>, usize)> = Vec::new();
    let mut start = 0usize;
    while start < sorted.len() {
      let (file_id, first_index, _) = sorted[start];
      let mut end = start + 1;
      while end < sorted.len()
        && end - start < max_pages
        && sorted[end].0 == file_id
        && sorted[end].1 == sorted[end - 1].1 + 1
      {
        end += 1;
      }
      let page_li: Vec<&[u8]> = sorted[start..end]
        .iter()
        .map(|(_, _, page)| page.as_slice())
        .collect();
      let bytes = page_li.len() * self.page_size;
      let file_offset = first_index * self.page_size as u64;
      let path = self.data_path(tbl, file_id << self.pages_per_file_shift);
      group_li.push((path, file_offset, page_li, bytes));
      start = end;
    }
    let write_li = group_li
      .into_iter()
      .map(|(path, file_offset, page_li, bytes)| async move {
        // IO 层闸门：限制所有 write_page_li 调用叠加后的在飞写组数。
        let _permit = self.write_gate.acquire().await;
        let mut file = self.open_write_file(&path).await.map_err(map_io_err)?;
        self.write_at_num.fetch_add(1, Ordering::Relaxed);
        let write_len = self
          .write_page_group(&mut file, &page_li, bytes, file_offset)
          .await?;
        if write_len != bytes {
          return Err(KvError::IoFail);
        }
        Ok::<PathBuf, KvError>(path)
      });
    // 提交窗口与写闸门取同一容量：闸门是唯一的全局在飞上限，
    // `buffer_unordered` 只是不让本轮把超过总额的组 future 一起推进。
    let inflight_limit = self.max_inflight_write;
    let result_li: Vec<Result<PathBuf>> = stream::iter(write_li)
      .buffer_unordered(inflight_limit)
      .collect()
      .await;
    // 成功组先记脏再返回首个错误，避免失败组跳过其它已落盘文件的 sync。
    let mut first_err = None;
    for result in result_li {
      match result {
        Ok(path) => self.mark_dirty(tbl, path),
        Err(err) if first_err.is_none() => first_err = Some(err),
        Err(_) => {}
      }
    }
    first_err.map_or(Ok(()), Err)
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.data_sync_num.fetch_add(1, Ordering::Relaxed);
    fkv_failpoint::fail_point!(SYNC_DATA);
    // 快照路径集合，避免持有 RefCell 借用跨 await。
    let path_li: Vec<PathBuf> = self
      .dirty
      .borrow()
      .get(tbl)
      .map_or_else(Vec::new, |set| set.iter().cloned().collect());
    let mut file_li: Vec<(PathBuf, File)> = Vec::with_capacity(path_li.len());
    for path in path_li {
      match self.open_for_sync(&path).await {
        Ok(file) => file_li.push((path, file)),
        Err(err) if err.kind() == ErrorKind::NotFound => {
          // 文件已删除，无需落盘。
          self.forget_dirty(&path);
        }
        Err(_) => return Err(KvError::IoFail),
      }
    }
    // 多个脏文件并发 fdatasync（对齐 C++ `FdatasyncFiles` 批量提交）：
    // 并发度跟随本轮脏文件数，并受 `max_inflight_write` 约束。
    let inflight = &self.sync_inflight;
    let sync_li = file_li.iter().map(|(path, file)| {
      let path = path.clone();
      async move {
        let _guard = inflight.enter();
        (path, file.sync_data().await)
      }
    });
    let result_li: Vec<(PathBuf, io::Result<()>)> = stream::iter(sync_li)
      .buffer_unordered(self.sync_inflight_limit(file_li.len()))
      .collect()
      .await;
    for (path, result) in result_li {
      result.map_err(map_io_err)?;
      self.data_fsync_num.fetch_add(1, Ordering::Relaxed);
      if let Some(set) = self.dirty.borrow_mut().get_mut(tbl) {
        set.remove(&path);
      }
    }
    // 新建 / 删除过文件名：先文件后目录，保证 manifest 引用的文件目录项已落盘。
    // 只有目录 fsync 成功后才清 dirty_dir：失败时保留条目，下一次 sync_data
    // 仍会重试目录同步；先清后同步会让失败后的重试静默跳过。
    if self.dirty_dir.borrow().contains(tbl) {
      let dir = self.part_dir(tbl);
      self.sync_dir(&dir).await?;
      self.dirty_dir.borrow_mut().remove(tbl);
    }
    Ok(())
  }
}

impl CompioStore {
  /// `read_page` / `read_page_owned` / `read_page_li` 的公共主体：候选路径
  /// 解析、单候选热路径、旧 term 候选回退与 EOF / Corrupted 归类；读目标由
  /// `dst` 决定（借用切片 → 池缓冲 + 拷贝；调用方 Vec → 内核直读，io-06）。
  ///
  /// 同一 file id 可能同时存在旧 term / 新 term 文件：只有「该候选确定没有
  /// 这一页」（EOF / checksum 失败）才换下一个候选；短读耗尽、非可重试错误
  /// 属于当前文件自身的终局故障，直接上抛——否则新 term 的 torn 页会静默
  /// 落回旧 term 读回陈旧页。
  async fn read_page_dst(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    mut dst: PageDst<'_>,
  ) -> Result<()> {
    if dst.as_slice().len() < self.page_size {
      return Err(KvError::InvalidArgs);
    }
    // 测试门闩：把一条普通读确定性地钉在页 IO 上（例如占住读窗口）。
    fkv_failpoint::hold_point!(READ_PAGE_HOLD);
    let page_size = self.page_size;
    let offset = self.page_offset(file_page_id);
    // 热路径：primary 单候选（无旧 term 变体）。`Arc<PathBuf>` 句柄只做一次
    // refcount 自增，路径解析不分配；`RefCell` 借用不会跨 await（perf-03）。
    // 语义与通用路径的单候选分支一致：EOF → EndOfFile，其它未命中 → Corrupted。
    if let Some(path) = self.single_data_path(tbl, file_page_id) {
      return match self
        .read_page_candidate(path.as_path(), page_size, offset, &mut dst)
        .await?
      {
        CandidateRead::Hit => Ok(()),
        CandidateRead::Eof => Err(KvError::EndOfFile),
        CandidateRead::Miss => Err(KvError::Corrupted),
      };
    }
    let path_li = self.data_path_li(tbl, file_page_id).await;
    if path_li.is_empty() {
      return Err(KvError::ResourceMissing);
    }
    let mut saw_eof = false;
    for (index, path) in path_li.into_iter().enumerate() {
      let outcome = self
        .read_page_candidate(&path, page_size, offset, &mut dst)
        .await?;
      match outcome {
        CandidateRead::Hit => {
          if index > 0 {
            // 更新的候选不可用后从旧 term 读到完整页：记一次回退命中。
            self.stale_read_num.fetch_add(1, Ordering::Relaxed);
          }
          return Ok(());
        }
        CandidateRead::Eof => saw_eof = true,
        CandidateRead::Miss => {}
      }
    }
    if saw_eof {
      Err(KvError::EndOfFile)
    } else {
      Err(KvError::Corrupted)
    }
  }
}
