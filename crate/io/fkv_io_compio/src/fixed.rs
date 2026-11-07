//! Linux io_uring fixed-buffer 支持。
//!
//! 缓冲区注册在 compio runtime 的 io_uring 上（通过 `io_uring_register`
//! syscall 的 sparse table），`READ_FIXED` / `WRITE_FIXED` 作为自定义
//! [`compio::driver::OpCode`] 提交到同一个 ring，因此完成事件直接进入
//! compio completion queue，由 runtime waker 驱动，不再轮询独立 ring。

use std::{
  ffi::c_void,
  fmt,
  fs::File,
  io,
  mem::size_of,
  os::fd::RawFd,
  ptr::null,
  sync::{Arc, Mutex, Weak},
  thread::{self, ThreadId},
};

use compio::{
  driver::{AsRawFd, DriverType, OpCode, OpEntry, SharedFd},
  runtime::Runtime,
};
use fkv_buf::AlignedBuf;
use fkv_error::{KvError, Result};
use fkv_io::BufLease;
use io_uring::{
  opcode::{ReadFixed, WriteFixed},
  types::Fd,
};

const MAX_FIXED_BUFFERS: u32 = 64;

/// op 持有的 fd guard。
///
/// 对齐 compio 内建 op 的生命周期模型：`SharedFd` 是引用计数句柄，op 在飞
/// （含外层 future 被 select / timeout / drop 取消后 SQE 仍在 ring 中）期间
/// 保持 fd 存活，避免 fd 关闭后被后续 open 复用、SQE 读写到无关文件。
///
/// 由 `compio::fs::File` 的 `to_shared_fd()` 取得；调用方只在构造 op 时接触
/// `File`，op 自身只持 guard。
type FdGuard = SharedFd<File>;

const IORING_REGISTER_BUFFERS: u32 = 0;
const IORING_UNREGISTER_BUFFERS: u32 = 1;
const IORING_REGISTER_BUFFERS2: u32 = 15;
const IORING_REGISTER_BUFFERS_UPDATE: u32 = 16;
const IORING_RSRC_REGISTER_SPARSE: u32 = 1;

/// `struct io_uring_rsrc_register`（BUFFERS2）。
#[repr(C)]
struct RsrcRegister {
  nr: u32,
  flags: u32,
  resv2: u64,
  data: u64,
  tags: u64,
}

/// `struct io_uring_rsrc_update2`（BUFFERS_UPDATE）。
#[repr(C)]
struct RsrcUpdate2 {
  offset: u32,
  resv: u32,
  data: u64,
  tags: u64,
  nr: u32,
  resv2: u32,
}

fn register_syscall(fd: RawFd, opcode: u32, arg: *const c_void, nr_args: u32) -> io::Result<()> {
  // SAFETY: arg 指向的 struct / iovec 在 syscall 期间有效；fd 是 runtime 的
  // io_uring fd，由 compio 持有。
  let result = unsafe { libc::syscall(libc::SYS_io_uring_register, fd, opcode, arg, nr_args) };
  if result < 0 {
    Err(io::Error::last_os_error())
  } else {
    Ok(())
  }
}

fn register_sparse(fd: RawFd, nr: u32) -> io::Result<()> {
  let arg = RsrcRegister {
    nr,
    flags: IORING_RSRC_REGISTER_SPARSE,
    resv2: 0,
    data: 0,
    tags: 0,
  };
  register_syscall(
    fd,
    IORING_REGISTER_BUFFERS2,
    &arg as *const RsrcRegister as *const c_void,
    size_of::<RsrcRegister>() as u32,
  )
}

fn register_buffer_li(fd: RawFd, iovec_li: &[libc::iovec]) -> io::Result<()> {
  register_syscall(
    fd,
    IORING_REGISTER_BUFFERS,
    iovec_li.as_ptr() as *const c_void,
    iovec_li.len() as u32,
  )
}

fn update_buffer(fd: RawFd, offset: u32, iovec_li: &[libc::iovec]) -> io::Result<()> {
  let arg = RsrcUpdate2 {
    offset,
    resv: 0,
    data: iovec_li.as_ptr() as u64,
    tags: 0,
    nr: iovec_li.len() as u32,
    resv2: 0,
  };
  register_syscall(
    fd,
    IORING_REGISTER_BUFFERS_UPDATE,
    &arg as *const RsrcUpdate2 as *const c_void,
    size_of::<RsrcUpdate2>() as u32,
  )
}

/// runtime ring 上的 fixed buffer 表。
///
/// 槽位归还由完成事件驱动（io-37）：compio 的取消不可靠，外层 future 被
/// drop 后 SQE 仍可能在 ring 中执行。若此时把 index 放回 free 表并被
/// `BUFFERS_UPDATE` 覆盖，在飞 op 会按**新**注册表解析 buf_index，读写到
/// 新缓冲。因此 [`SlotState::inflight`] 记录在飞 op 数，用户侧
/// `FixedBuffer` 释放后仍需等最后一个 op（driver 收割 CQE 后 drop）离场
/// 才真正归还。
pub(crate) struct FixedBufferRegistry {
  ring_fd: RawFd,
  owner: ThreadId,
  next_index: u16,
  free_slot_li: Vec<u16>,
  sparse: bool,
  slot_li: Vec<SlotState>,
  /// 本表生命周期标识；`reset_fixed_ring` 重建后旧租约不再影响新表。
  token: Arc<()>,
}

/// 单槽位状态机：内核注册项 ↔ 用户 buffer ↔ 在飞 op。
#[derive(Default)]
struct SlotState {
  /// 内核注册指向的内存；`None` 表示槽位空闲。ring 关闭前用户 buffer
  /// 可能先 drop，这里保活。
  buf: Option<Arc<AlignedBuf>>,
  /// 已提交、CQE 未收割的 op 数。
  inflight: u32,
  /// 用户侧 `FixedBuffer` 已释放，等待 `inflight` 归零。
  released: bool,
}

/// 槽位租约：op 存活期间阻止该 index 被复用。
///
/// 由 op 持有，op 只在 driver 收割完成事件（含取消确认）后才 drop，
/// 因此 `Drop` 就是「完成确认」点。
pub(crate) struct SlotLease {
  shared: Weak<Mutex<Option<FixedBufferRegistry>>>,
  token: Arc<()>,
  index: u16,
}

impl Drop for SlotLease {
  fn drop(&mut self) {
    if let Some(shared) = self.shared.upgrade()
      && let Ok(mut slot) = shared.lock()
      && let Some(buffers) = slot.as_mut()
      && Arc::ptr_eq(&buffers.token, &self.token)
    {
      buffers.leave(self.index);
    }
  }
}

impl fmt::Debug for FixedBufferRegistry {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("FixedBufferRegistry")
      .field("ring_fd", &self.ring_fd)
      .field("registered", &self.next_index)
      .field("sparse", &self.sparse)
      .finish()
  }
}

impl Drop for FixedBufferRegistry {
  fn drop(&mut self) {
    if self.next_index == 0 {
      return;
    }
    // 只有当前 runtime 仍是同一 ring 时才注销，避免 fd 复用后误操作。
    if thread::current().id() != self.owner {
      return;
    }
    let current_fd = Runtime::try_with_current(|runtime| runtime.as_raw_fd()).ok();
    if current_fd == Some(self.ring_fd) {
      let _ = register_syscall(self.ring_fd, IORING_UNREGISTER_BUFFERS, null(), 0);
    }
  }
}

impl FixedBufferRegistry {
  /// 绑定当前 compio runtime 的 io_uring。
  pub(crate) fn current() -> Result<Self> {
    let (driver_type, ring_fd) =
      Runtime::try_with_current(|runtime| (runtime.driver_type(), runtime.as_raw_fd()))
        .map_err(|_| KvError::NotRunning)?;
    if driver_type != DriverType::IoUring {
      return Err(KvError::IoFail);
    }
    Ok(Self {
      ring_fd,
      owner: thread::current().id(),
      next_index: 0,
      free_slot_li: Vec::new(),
      sparse: false,
      slot_li: Vec::new(),
      token: Arc::new(()),
    })
  }

  pub(crate) fn register(&mut self, buf: Arc<AlignedBuf>) -> Result<u16> {
    if buf.is_empty() {
      return Err(KvError::InvalidArgs);
    }
    let iovec = libc::iovec {
      iov_base: buf.as_ptr() as *mut c_void,
      iov_len: buf.len(),
    };
    if !self.sparse {
      if self.next_index > 0 {
        return Err(KvError::OutOfMem);
      }
      match register_sparse(self.ring_fd, MAX_FIXED_BUFFERS) {
        Ok(()) => self.sparse = true,
        Err(_) => {
          // 内核 < 5.13 或不支持 sparse table：退化为经典单 buffer 注册。
          register_buffer_li(self.ring_fd, &[iovec]).map_err(|_| KvError::IoFail)?;
          self.next_index = 1;
          self.slot_li.push(SlotState {
            buf: Some(buf),
            ..SlotState::default()
          });
          return Ok(0);
        }
      }
    }
    let (index, fresh) = match self.free_slot_li.pop() {
      Some(index) => (index, false),
      None => {
        let index = self.next_index;
        if u32::from(index) >= MAX_FIXED_BUFFERS {
          return Err(KvError::OutOfMem);
        }
        self.next_index += 1;
        self.slot_li.push(SlotState::default());
        (index, true)
      }
    };
    if update_buffer(self.ring_fd, u32::from(index), &[iovec]).is_err() {
      // 失败回滚：新建槽位撤销 next_index 与表项，复用槽位推回 free 链。
      // 否则每次失败都永久泄漏一个 index / slot，64 次后 fixed IO 全退化。
      if fresh {
        self.next_index = index;
        self.slot_li.pop();
      } else {
        self.free_slot_li.push(index);
      }
      return Err(KvError::IoFail);
    }
    self.slot_li[u32::from(index) as usize] = SlotState {
      buf: Some(buf),
      ..SlotState::default()
    };
    Ok(index)
  }

  /// 用户侧归还槽位：保留内核 sparse 表项，下一次 `register` 用
  /// BUFFERS_UPDATE 覆盖；仍有在飞 op 时推迟到 `leave`。
  pub(crate) fn release(&mut self, index: u16) {
    // 经典单 buffer 注册不支持槽位复用。
    if !self.sparse {
      return;
    }
    let ready = {
      let Some(slot) = self.slot_li.get_mut(index as usize) else {
        return;
      };
      if slot.buf.is_none() {
        return;
      }
      slot.released = true;
      slot.inflight == 0
    };
    if ready {
      self.push_free(index);
    }
  }

  /// 登记一个在飞 op 并返回租约；租约 drop（CQE 已收割）后 `inflight` 递减。
  pub(crate) fn lease(
    &mut self,
    shared: Weak<Mutex<Option<FixedBufferRegistry>>>,
    index: u16,
  ) -> SlotLease {
    if let Some(slot) = self.slot_li.get_mut(index as usize) {
      slot.inflight += 1;
    }
    SlotLease {
      shared,
      token: Arc::clone(&self.token),
      index,
    }
  }

  /// op 完成确认：最后一个在飞 op 离场且用户侧已归还时，槽位入 free 表。
  fn leave(&mut self, index: u16) {
    let ready = {
      let Some(slot) = self.slot_li.get_mut(index as usize) else {
        return;
      };
      slot.inflight = slot.inflight.saturating_sub(1);
      slot.released && slot.inflight == 0
    };
    if ready {
      self.push_free(index);
    }
  }

  fn push_free(&mut self, index: u16) {
    let Some(slot) = self.slot_li.get_mut(index as usize) else {
      return;
    };
    slot.buf = None;
    slot.released = false;
    self.free_slot_li.push(index);
  }

  /// 当前线程 / runtime 是否仍是注册时的 ring。跨线程或 runtime 重建后
  /// fixed IO 必须回落普通路径，否则新 ring 的固定表里没有这些槽位。
  pub(crate) fn owns_current_ring(&self) -> bool {
    if thread::current().id() != self.owner {
      return false;
    }
    Runtime::try_with_current(|runtime| runtime.as_raw_fd()).ok() == Some(self.ring_fd)
  }

  /// 本表生命周期标识。用户侧 drop hook 必须携带并 `ptr_eq` 校验，
  /// 防止 ring 重建后旧 buffer 释放新表的同号槽位。
  pub(crate) fn token(&self) -> Arc<()> {
    self.token.clone()
  }
}

/// 提交到 compio runtime 的 `READ_FIXED`。
pub(crate) struct FixedRead {
  fd: FdGuard,
  ptr: *mut u8,
  len: u32,
  offset: u64,
  index: u16,
  _buf: BufLease,
  /// 完成确认前阻止槽位复用（io-37）；未注册槽位为 `None`。
  _slot: Option<SlotLease>,
}

impl FixedRead {
  pub(crate) fn new(
    fd: FdGuard,
    buf: BufLease,
    ptr_offset: usize,
    len: usize,
    offset: u64,
    index: u16,
  ) -> Result<Self> {
    let end = ptr_offset.checked_add(len).ok_or(KvError::InvalidArgs)?;
    if end > buf.len() {
      return Err(KvError::InvalidArgs);
    }
    let ptr = unsafe { buf.as_ptr().cast_mut().add(ptr_offset) };
    Ok(Self {
      fd,
      ptr,
      len: u32::try_from(len).map_err(|_| KvError::InvalidArgs)?,
      offset,
      index,
      _buf: buf,
      _slot: None,
    })
  }

  /// 绑定注册槽位租约：op 完成（CQE 收割）后才允许该 index 复用。
  pub(crate) fn with_slot(mut self, slot: SlotLease) -> Self {
    self._slot = Some(slot);
    self
  }
}

// SAFETY: entry 指向的 buffer 由 `_buf` 持有、fd 由 `fd` guard 保活，直至操作完成。
unsafe impl OpCode for FixedRead {
  type Control = ();

  fn create_entry(&mut self, _: &mut Self::Control) -> OpEntry {
    OpEntry::Submission(
      ReadFixed::new(Fd(self.fd.as_raw_fd()), self.ptr, self.len, self.index)
        .offset(self.offset)
        .build(),
    )
  }

  fn call_blocking(&mut self, _: &mut Self::Control) -> io::Result<usize> {
    // 内核不支持 READ_FIXED 时由 compio 线程池回退；buffer 由 `_buf`、
    // fd 由 `fd` guard 保活，回退路径同样不暴露裸 fd。
    let fd = self.fd.as_raw_fd();
    let mut done = 0usize;
    let len = self.len as usize;
    while done < len {
      let result = unsafe {
        libc::pread(
          fd,
          self.ptr.cast::<c_void>().add(done),
          len - done,
          (self.offset as usize + done) as libc::off_t,
        )
      };
      if result < 0 {
        let err = io::Error::last_os_error();
        if err.kind() == io::ErrorKind::Interrupted {
          continue;
        }
        return Err(err);
      }
      if result == 0 {
        break;
      }
      done += result as usize;
    }
    Ok(done)
  }
}

/// 提交到 compio runtime 的 `WRITE_FIXED`。
pub(crate) struct FixedWrite {
  fd: FdGuard,
  ptr: *const u8,
  len: u32,
  offset: u64,
  index: u16,
  _buf: BufLease,
  /// 完成确认前阻止槽位复用（io-37）；未注册槽位为 `None`。
  _slot: Option<SlotLease>,
}

impl FixedWrite {
  pub(crate) fn new(
    fd: FdGuard,
    buf: BufLease,
    ptr_offset: usize,
    len: usize,
    offset: u64,
    index: u16,
  ) -> Result<Self> {
    let end = ptr_offset.checked_add(len).ok_or(KvError::InvalidArgs)?;
    if end > buf.len() {
      return Err(KvError::InvalidArgs);
    }
    let ptr = unsafe { buf.as_ptr().add(ptr_offset) };
    Ok(Self {
      fd,
      ptr,
      len: u32::try_from(len).map_err(|_| KvError::InvalidArgs)?,
      offset,
      index,
      _buf: buf,
      _slot: None,
    })
  }

  /// 绑定注册槽位租约：op 完成（CQE 收割）后才允许该 index 复用。
  pub(crate) fn with_slot(mut self, slot: SlotLease) -> Self {
    self._slot = Some(slot);
    self
  }
}

// SAFETY: entry 指向的 buffer 由 `_buf` 持有、fd 由 `fd` guard 保活，直至操作完成。
unsafe impl OpCode for FixedWrite {
  type Control = ();

  fn create_entry(&mut self, _: &mut Self::Control) -> OpEntry {
    OpEntry::Submission(
      WriteFixed::new(Fd(self.fd.as_raw_fd()), self.ptr, self.len, self.index)
        .offset(self.offset)
        .build(),
    )
  }

  fn call_blocking(&mut self, _: &mut Self::Control) -> io::Result<usize> {
    // 内核不支持 WRITE_FIXED 时由 compio 线程池回退；fd 由 `fd` guard 保活。
    let fd = self.fd.as_raw_fd();
    let mut done = 0usize;
    let len = self.len as usize;
    while done < len {
      let result = unsafe {
        libc::pwrite(
          fd,
          self.ptr.add(done).cast(),
          len - done,
          (self.offset as usize + done) as libc::off_t,
        )
      };
      if result < 0 {
        let err = io::Error::last_os_error();
        if err.kind() == io::ErrorKind::Interrupted {
          continue;
        }
        return Err(err);
      }
      if result == 0 {
        return Err(io::Error::from(io::ErrorKind::WriteZero));
      }
      done += result as usize;
    }
    Ok(done)
  }
}

#[cfg(test)]
mod tests {
  use std::{
    fs::{OpenOptions, read, remove_file, write},
    os::fd::AsRawFd,
    path::PathBuf,
  };

  use fkv_testkit::TempRoot;

  use super::*;

  fn temp_path(tag: &str) -> (TempRoot, PathBuf) {
    let root = TempRoot::new(&format!("fkv_fixed_unit_{tag}"));
    let path = root.join("file");
    (root, path)
  }

  #[test]
  fn blocking_fallback_read_and_write() {
    let (_root, path) = temp_path("rw");
    write(&path, b"hello world").unwrap();
    let guard = SharedFd::new(
      OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap(),
    );

    let dst = Arc::new(AlignedBuf::new(16));
    let mut read_op =
      FixedRead::new(guard.clone(), BufLease::detached(dst.clone()), 0, 11, 0, 0).unwrap();
    let read_len = read_op.call_blocking(&mut ()).unwrap();
    assert_eq!(read_len, 11);
    assert_eq!(&dst[..read_len], b"hello world");

    let mut plain = AlignedBuf::new(16);
    plain[..5].copy_from_slice(b"HELLO");
    let src = Arc::new(plain);
    let mut write_op = FixedWrite::new(guard, BufLease::detached(src), 0, 5, 6, 0).unwrap();
    let write_len = write_op.call_blocking(&mut ()).unwrap();
    assert_eq!(write_len, 5);
    assert_eq!(&read(&path).unwrap(), b"hello HELLO");

    remove_file(path).unwrap();
  }

  /// 取消 / 超时语义回归：op 持有 fd guard。
  ///
  /// SQE 提交后外层 future 可能被 select / timeout / drop 取消，compio 只是
  /// 请求 `cancel`（不可靠），op 仍由 driver 持有并在 ring 中执行。因此原
  /// `File` 先 drop 时 fd 必须仍然有效；只有最后一个 guard 释放后 fd 才关闭。
  #[test]
  fn fd_guard_outlives_source_file_drop() {
    let (_root, path) = temp_path("guard");
    write(&path, b"guard").unwrap();
    let guard = SharedFd::new(OpenOptions::new().read(true).open(&path).unwrap());

    let buf = Arc::new(AlignedBuf::new(16));
    let mut op =
      FixedRead::new(guard.clone(), BufLease::detached(buf.clone()), 0, 5, 0, 0).unwrap();
    let raw = op.fd.as_raw_fd();

    // 模拟外层 future 被取消后源句柄先 drop：fd 仍由 op 的 guard 保活。
    drop(guard);
    assert_ne!(unsafe { libc::fcntl(raw, libc::F_GETFD) }, -1);
    assert_eq!(op.call_blocking(&mut ()).unwrap(), 5);
    assert_eq!(&buf[..5], b"guard");

    // guard 释放后 fd 关闭，后续 open 复用的 fd 号不可能再被在飞 SQE 命中。
    drop(op);
    let closed = unsafe { libc::fcntl(raw, libc::F_GETFD) } == -1
      && io::Error::last_os_error().raw_os_error() == Some(libc::EBADF);
    assert!(closed, "guard 释放后 fd 应关闭");

    remove_file(path).unwrap();
  }

  /// io-37：用户侧 buffer 已释放但 op 未完成时，槽位不得复用；
  /// 租约 drop（CQE 收割）后才回到 free 表。
  #[compio::test]
  async fn released_slot_waits_for_inflight_lease() {
    let shared = Arc::new(Mutex::new(Some(FixedBufferRegistry::current().unwrap())));
    {
      let mut guard = shared.lock().unwrap();
      let buffers = guard.as_mut().unwrap();
      assert_eq!(
        buffers.register(Arc::new(AlignedBuf::new(4096))).unwrap(),
        0
      );
      assert_eq!(
        buffers.register(Arc::new(AlignedBuf::new(4096))).unwrap(),
        1
      );
    }

    // 模拟一次已提交、CQE 未收割的 op。
    let lease = {
      let mut guard = shared.lock().unwrap();
      guard.as_mut().unwrap().lease(Arc::downgrade(&shared), 0)
    };
    {
      // 外层 future 被 drop：用户侧 buffer 归还，但 op 仍在飞。
      let mut guard = shared.lock().unwrap();
      guard.as_mut().unwrap().release(0);
    }
    let index = {
      let mut guard = shared.lock().unwrap();
      guard
        .as_mut()
        .unwrap()
        .register(Arc::new(AlignedBuf::new(4096)))
        .unwrap()
    };
    assert_eq!(index, 2, "在飞 op 未确认完成前槽位不得复用");

    // 完成确认：槽位才回到 free 表。
    drop(lease);
    let index = {
      let mut guard = shared.lock().unwrap();
      guard
        .as_mut()
        .unwrap()
        .register(Arc::new(AlignedBuf::new(4096)))
        .unwrap()
    };
    assert_eq!(index, 0);
  }
}
