//! 页读 / 整文件读：缓冲选择、瞬时错误重试、identity 候选回退。

use std::{
  io::{self, ErrorKind},
  mem,
  path::Path,
  sync::atomic::Ordering,
};

use compio::{
  buf::{BufResult, IntoInner, IoBuf},
  fs::File,
  io::AsyncReadAt,
};
use fkv_error::{KvError, Result};
use fkv_page::validate_checksum;
use fkv_types::{FilePageId, PAGE_ALIGN, TableIdent};

use crate::{
  CandidateRead, CompioStore, DirectBuf, PageDst, READ_RETRY_TIMES, ReadChunk,
  SPAN_COALESCE_MIN_PAGES, WHOLE_READ_CHUNK, map_io_err,
};

impl CompioStore {
  /// 发起一次页读：按 `direct_io` 选普通池化缓冲或 O_DIRECT 对齐缓冲。
  pub(crate) async fn read_page_chunk(
    &self,
    file: &File,
    len: usize,
    offset: u64,
  ) -> (io::Result<usize>, ReadChunk) {
    let direct = self.direct_io.get();
    if self.take_injected_read_err() {
      // 注入路径不碰文件，但仍取同类型缓冲，保证归还池路径与真实读一致。
      let chunk = if direct {
        ReadChunk::Direct(DirectBuf::from_buf(self.take_direct_buf(len), 0))
      } else {
        ReadChunk::Plain(self.take_read_buf(len))
      };
      return (Err(io::Error::from(ErrorKind::WouldBlock)), chunk);
    }
    if direct {
      let BufResult(result, buf) = file
        .read_at(DirectBuf::from_buf(self.take_direct_buf(len), 0), offset)
        .await;
      (result, ReadChunk::Direct(buf))
    } else {
      let BufResult(result, buf) = file.read_at(self.take_read_buf(len), offset).await;
      (result, ReadChunk::Plain(buf))
    }
  }

  /// 取出一次注入的瞬时读错误；未配置返回 false。
  pub(crate) fn take_injected_read_err(&self) -> bool {
    let left = self.read_try_again_times.get();
    if left == 0 {
      return false;
    }
    self.read_try_again_times.set(left - 1);
    true
  }

  /// 归还页读缓冲。
  pub(crate) fn put_read_chunk(&self, len: usize, chunk: ReadChunk) {
    match chunk {
      ReadChunk::Plain(buf) => self.put_read_buf(len, buf),
      ReadChunk::Direct(buf) => self.put_direct_buf(buf.into_inner()),
    }
  }

  /// 合并读一段连续页：`page_buf_li[i]` 承接 `start + i`（必须在同一物理文件
  /// 内，由调用方 `read_page_span` 切段保证）。
  ///
  /// 返回 `false` 表示「这一段不能合并读」，调用方按页重读：候选路径不唯一
  /// （同 file id 有旧 term 变体）、打开失败、瞬时 / 真实 IO 错误、段尾短读
  /// （含 EOF）、任一页 checksum 不过。逐页路径才是错误语义的唯一来源，合并读
  /// 只在「整段一次下盘、且每页都能验过」时才生效，因此不改变任何页的内容
  /// 来源，也不改变错误码。
  ///
  /// 页号连续 + 单候选是最省的下盘形态：一次 `pread` 拿回整段，再逐页校验并
  /// 拷进调用方缓冲（对齐写路径 `write_page_li` 的连续页合并口径）。
  pub(crate) async fn read_page_run(
    &self,
    tbl: &TableIdent,
    start: FilePageId,
    page_buf_li: &mut [Vec<u8>],
  ) -> Result<bool> {
    let cnt = page_buf_li.len();
    let page_size = self.page_size;
    if cnt < SPAN_COALESCE_MIN_PAGES {
      return Ok(false);
    }
    // 同一物理文件内页号才连续：跨文件不能一次 pread。
    let shift = self.pages_per_file_shift;
    if start >> shift != (start + cnt as u64 - 1) >> shift {
      return Ok(false);
    }
    // 调用方缓冲不足整页：与逐页路径同码报错，不静默读短路。
    if page_buf_li.iter().any(|buf| buf.len() < page_size) {
      return Err(KvError::InvalidArgs);
    }
    // 同 file id 存在旧 term 变体时不合并：逐页路径按 term 降序换候选，合并读
    // 表达不了「每页各自换候选」。
    let Some(path) = self.single_data_path(tbl, start) else {
      return Ok(false);
    };
    // 测试门闩与逐页路径同一注入点：一次段读就是一次页 IO。
    fkv_failpoint::hold_point!(READ_PAGE_HOLD);
    let Ok(file) = self.open_read_file(path.as_path()).await else {
      return Ok(false);
    };
    let len = cnt * page_size;
    self.read_at_num.fetch_add(1, Ordering::Relaxed);
    let (result, chunk) = self
      .read_page_chunk(&file, len, self.page_offset(start))
      .await;
    let read_len = match result {
      Ok(read_len) => read_len,
      Err(_) => {
        self.put_read_chunk(len, chunk);
        return Ok(false);
      }
    };
    // 段尾短读（含 0 字节 EOF）不合并：逐页路径按页给出 Eof / Corrupted 归类。
    if read_len != len {
      self.put_read_chunk(len, chunk);
      return Ok(false);
    }
    let mut merged = true;
    {
      let bytes = chunk.as_slice();
      for (index, buf) in page_buf_li.iter_mut().enumerate() {
        let page = &bytes[index * page_size..(index + 1) * page_size];
        if !(self.skip_verify_checksum.get() || validate_checksum(page).unwrap_or(false)) {
          merged = false;
          break;
        }
        buf[..page_size].copy_from_slice(page);
      }
    }
    self.put_read_chunk(len, chunk);
    Ok(merged)
  }

  /// 读单个候选文件。
  ///
  /// 瞬时错误（`TryAgain` / `Busy` / `OpenFileLimit`）与短读只在同一路径内
  /// 重试；短读重试耗尽按 `Corrupted` 终局上报，不换旧 term 候选。EOF 与
  /// checksum 失败返回 `Eof` / `Miss`，由调用方决定是否回退。
  ///
  /// 读目标二选一（io-06）：`Owned` 且非 O_DIRECT 时调用方 Vec 直接作为读
  /// 目标——compio 按 `slice(..page_size)` 界定读长（容量大于一页也不会越过
  /// 页边界过读），内核 DMA 落页即最终缓冲，省一次整页拷贝与一次读池槽
  /// 周转；`advance_to` 只增不缩，Vec 长度在任何返回点都与进入时一致。
  /// 其余组合（借用切片无所有权 / O_DIRECT 需页对齐缓冲）沿用
  /// 「池缓冲直读 + 一次性拷贝」。
  pub(crate) async fn read_page_candidate(
    &self,
    path: &Path,
    page_size: usize,
    offset: u64,
    dst: &mut PageDst<'_>,
  ) -> Result<CandidateRead> {
    let file = match self.open_read_file(path).await {
      Ok(file) => file,
      // 索引里的路径被外部删除：该候选不存在，允许尝试其它候选。
      Err(err) if err.kind() == ErrorKind::NotFound => {
        return Ok(CandidateRead::Miss);
      }
      Err(err) => {
        return Err(map_io_err(err));
      }
    };
    let direct = self.direct_io.get();
    for attempt in 0..READ_RETRY_TIMES {
      self.read_at_num.fetch_add(1, Ordering::Relaxed);
      // `Some(chunk)`：池缓冲路径，页数据在 chunk 内（每个非返回出口都要
      // 归还池）；`None`：owned 直读，页数据已落进调用方 Vec。
      let (result, chunk) = match dst {
        PageDst::Owned(vec) if !direct => {
          if self.take_injected_read_err() {
            // 注入路径不碰文件；owned 目标无池缓冲，不存在归还路径。
            (Err(io::Error::from(ErrorKind::WouldBlock)), None)
          } else {
            let buf = mem::take(&mut **vec);
            let BufResult(result, sliced) = file.read_at(buf.slice(..page_size), offset).await;
            // 读完成即写回：此后任何出口调用方缓冲都在位（内容为本次读入）。
            // 外层 future 在 await 点被取消时写回不会执行：该次分配随 op 由
            // 运行时释放，调用方 Vec 剩空壳——只损失一次分配，不损坏数据
            // （池缓冲路径取消时同样损失池缓冲）。
            **vec = sliced.into_inner();
            (result, None)
          }
        }
        _ => {
          let (result, chunk) = self.read_page_chunk(&file, page_size, offset).await;
          (result, Some(chunk))
        }
      };
      let read_len = match result {
        Ok(read_len) => read_len,
        Err(err) => {
          if let Some(chunk) = chunk {
            self.put_read_chunk(page_size, chunk);
          }
          // `is_retryable()` 覆盖 TryAgain / Busy / OpenFileLimit，均为瞬时错误。
          let err = map_io_err(err);
          if err.is_retryable() && attempt + 1 < READ_RETRY_TIMES {
            // 只在同一路径重试，耗尽后上抛，绝不切换旧 term 候选。
            continue;
          }
          return Err(err);
        }
      };
      // `res == 0` 是确定性 EOF：本候选不重试，交给上一层尝试更旧的 term
      // 候选，否则新 term 的稀疏文件会遮蔽旧 term 已落盘的尾部页。
      if read_len == 0 {
        if let Some(chunk) = chunk {
          self.put_read_chunk(page_size, chunk);
        }
        return Ok(CandidateRead::Eof);
      }
      if read_len != page_size {
        // 短读不是 EOF：torn 写 / 截断属于当前文件自身损坏。同路径有限
        // 重试，耗尽即终局 Corrupted，不换旧 term 候选读回陈旧页。
        if let Some(chunk) = chunk {
          self.put_read_chunk(page_size, chunk);
        }
        continue;
      }
      // 整页读到：owned 路径数据已就位，只校验；池缓冲路径校验后整页拷入
      // dst 再归还（校验与拷贝用的是同一份字节，先后无语义差别）。
      let page_ok = if let Some(chunk) = &chunk {
        let ok =
          self.skip_verify_checksum.get() || validate_checksum(chunk.as_slice()).unwrap_or(false);
        dst.as_mut_slice()[..page_size].copy_from_slice(chunk.as_slice());
        ok
      } else {
        self.skip_verify_checksum.get()
          || validate_checksum(&dst.as_slice()[..page_size]).unwrap_or(false)
      };
      if let Some(chunk) = chunk {
        self.put_read_chunk(page_size, chunk);
      }
      if page_ok {
        return Ok(CandidateRead::Hit);
      }
      // 整页可读但 checksum 不通过：稀疏洞（全零页）或损坏，试更旧 term。
      return Ok(CandidateRead::Miss);
    }
    // 只有短读会耗尽循环（瞬时错误在最后一次尝试时已上抛）。
    Err(KvError::Corrupted)
  }
}

impl CompioStore {
  /// 读取整个文件。短读视为 `IoFail`。
  ///
  /// `direct` 为目标文件是否按 O_DIRECT 打开：读目标必须是页对齐缓冲，而
  /// 返回值必须是拥有所有权的 `Vec<u8>`（Cloud `flush_tbl` 直接把它交给
  /// `Bytes::from` 做零拷贝上传），`Vec` 的普通分配不保证页对齐。因此
  /// `direct` 走分块读：每块（≤ [`WHOLE_READ_CHUNK`]）读进可复用的页对齐
  /// 暂存缓冲，再追加到输出 `Vec`。峰值 = `len` + 1 块，而不是「`len` 对齐
  /// 缓冲 + `len` 输出 Vec」的 2×；清零只发生在块缓冲首次分配时。
  ///
  /// Cloud `flush_tbl` 每条 sync 都会对每个脏 data 文件走这里，大表批量
  /// 导入时文件可达数百 MiB（默认 4 KiB × 2^18 = 1 GiB），所以这是热路径，
  /// 不能继续按「整文件读是冷路径、允许一次额外拷贝」的假设设计。
  pub(crate) async fn read_whole(&self, file: &File, direct: bool) -> Result<Vec<u8>> {
    let len = usize::try_from(file.metadata().await.map_err(map_io_err)?.len())
      .map_err(|_| KvError::InvalidArgs)?;
    if len == 0 {
      return Ok(Vec::new());
    }
    if direct {
      return self.read_whole_direct(file, len).await;
    }
    // 走读缓冲池：容量直读，不预先清零。短读注入一次性生效。
    let short_read = self.short_read_bytes.take();
    let BufResult(result, buf) = file.read_at(self.take_read_buf(len), 0).await;
    let read_len = match result {
      Ok(read_len) => read_len,
      Err(err) => {
        self.put_read_buf(len, buf);
        return Err(map_io_err(err));
      }
    };
    let read_len = short_read.map_or(read_len, |limit| read_len.min(limit));
    if read_len != len {
      self.put_read_buf(len, buf);
      return Err(KvError::IoFail);
    }
    Ok(buf)
  }

  /// O_DIRECT 整文件读：固定块暂存 + 追加到输出 `Vec`（io-17）。
  ///
  /// 每块请求长度向上取整到 `PAGE_ALIGN`（文件尾可能不足一页，内核按 EOF
  /// 返回短读），偏移始终是块长的整数倍，天然页对齐。暂存缓冲从 direct 池
  /// 取用，读完整块即归还，因此一个 store 读任意大小文件只新增一块 ≤
  /// `WHOLE_READ_CHUNK` 的对齐缓冲，后续整文件读全部命中池。
  ///
  /// 短读一律 `IoFail`：`metadata` 之后文件被截断属于本次读失败；向上取整的
  /// 尾块若因文件变长而过读，同样不是本次 flush 能安全上传的内容，与旧的
  /// 一次性整文件读语义保持一致。
  async fn read_whole_direct(&self, file: &File, len: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
      let offset = out.len();
      let want = (len - offset).min(WHOLE_READ_CHUNK);
      let req = want.next_multiple_of(PAGE_ALIGN);
      self.read_whole_chunk_num.fetch_add(1, Ordering::Relaxed);
      let BufResult(result, buf) = file
        .read_at(
          DirectBuf::from_buf(self.take_direct_buf(req), 0),
          offset as u64,
        )
        .await;
      let read_len = match result {
        Ok(read_len) => read_len,
        Err(err) => {
          self.put_direct_buf(buf.into_inner());
          return Err(map_io_err(err));
        }
      };
      if read_len != want {
        self.put_direct_buf(buf.into_inner());
        return Err(KvError::IoFail);
      }
      // compio 已把缓冲长度推进到本次读入字节数，等同 `&buf[..read_len]`。
      out.extend_from_slice(buf.as_init());
      self.put_direct_buf(buf.into_inner());
    }
    Ok(out)
  }
}
