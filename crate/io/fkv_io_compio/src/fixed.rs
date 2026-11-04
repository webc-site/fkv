//! Linux io_uring fixed-buffer 支持。
//!
//! 缓冲区注册在 compio runtime 的 io_uring 上（通过 `io_uring_register`
//! syscall 的 sparse table），`READ_FIXED` / `WRITE_FIXED` 作为自定义
//! [`compio::driver::OpCode`] 提交到同一个 ring，因此完成事件直接进入
//! compio completion queue，由 runtime waker 驱动，不再轮询独立 ring。

use std::{ffi::c_void, fmt, io, mem::size_of, os::fd::RawFd, ptr::null, sync::Arc};

use compio::{
  driver::{AsRawFd, DriverType, OpCode, OpEntry},
  runtime::Runtime,
};
use fkv_buf::AlignedBuf;
use fkv_error::{KvError, Result};
use io_uring::{
  opcode::{ReadFixed, WriteFixed},
  types::Fd,
};

const MAX_FIXED_BUFFERS: u32 = 64;

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

fn register_buffers(fd: RawFd, iovec_li: &[libc::iovec]) -> io::Result<()> {
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
pub(crate) struct FixedBuffers {
  ring_fd: RawFd,
  next_index: u16,
  sparse: bool,
  /// 保持注册内存存活；ring 关闭前用户 buffer 可能先 drop。
  guard_li: Vec<Arc<AlignedBuf>>,
}

impl fmt::Debug for FixedBuffers {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("FixedBuffers")
      .field("ring_fd", &self.ring_fd)
      .field("registered", &self.next_index)
      .field("sparse", &self.sparse)
      .finish()
  }
}

impl Drop for FixedBuffers {
  fn drop(&mut self) {
    if self.next_index == 0 {
      return;
    }
    // 只有当前 runtime 仍是同一 ring 时才注销，避免 fd 复用后误操作。
    let current_fd = Runtime::try_with_current(|runtime| runtime.as_raw_fd()).ok();
    if current_fd == Some(self.ring_fd) {
      let _ = register_syscall(self.ring_fd, IORING_UNREGISTER_BUFFERS, null(), 0);
    }
  }
}

impl FixedBuffers {
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
      next_index: 0,
      sparse: false,
      guard_li: Vec::new(),
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
          register_buffers(self.ring_fd, &[iovec]).map_err(|_| KvError::IoFail)?;
          self.next_index = 1;
          self.guard_li.push(buf);
          return Ok(0);
        }
      }
    }
    let index = self.next_index;
    if u32::from(index) >= MAX_FIXED_BUFFERS {
      return Err(KvError::OutOfMem);
    }
    update_buffer(self.ring_fd, u32::from(index), &[iovec]).map_err(|_| KvError::IoFail)?;
    self.next_index += 1;
    self.guard_li.push(buf);
    Ok(index)
  }
}

/// 提交到 compio runtime 的 `READ_FIXED`。
pub(crate) struct FixedRead {
  fd: RawFd,
  ptr: *mut u8,
  len: u32,
  offset: u64,
  index: u16,
  _buf: Arc<AlignedBuf>,
}

impl FixedRead {
  pub(crate) fn new(
    fd: RawFd,
    buf: Arc<AlignedBuf>,
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
    })
  }
}

// SAFETY: entry 指向的 buffer 由 `_buf` 持有，直至操作完成。
unsafe impl OpCode for FixedRead {
  type Control = ();

  fn create_entry(&mut self, _: &mut Self::Control) -> OpEntry {
    OpEntry::Submission(
      ReadFixed::new(Fd(self.fd), self.ptr, self.len, self.index)
        .offset(self.offset)
        .build(),
    )
  }

  fn call_blocking(&mut self, _: &mut Self::Control) -> io::Result<usize> {
    // 内核不支持 READ_FIXED 时由 compio 线程池回退；buffer 仍由 `_buf` 持有。
    let result = unsafe {
      libc::pread(
        self.fd,
        self.ptr.cast(),
        self.len as usize,
        self.offset as libc::off_t,
      )
    };
    if result < 0 {
      Err(io::Error::last_os_error())
    } else {
      Ok(result as usize)
    }
  }
}

/// 提交到 compio runtime 的 `WRITE_FIXED`。
pub(crate) struct FixedWrite {
  fd: RawFd,
  ptr: *const u8,
  len: u32,
  offset: u64,
  index: u16,
  _buf: Arc<AlignedBuf>,
}

impl FixedWrite {
  pub(crate) fn new(
    fd: RawFd,
    buf: Arc<AlignedBuf>,
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
    })
  }
}

// SAFETY: entry 指向的 buffer 由 `_buf` 持有，直至操作完成。
unsafe impl OpCode for FixedWrite {
  type Control = ();

  fn create_entry(&mut self, _: &mut Self::Control) -> OpEntry {
    OpEntry::Submission(
      WriteFixed::new(Fd(self.fd), self.ptr, self.len, self.index)
        .offset(self.offset)
        .build(),
    )
  }

  fn call_blocking(&mut self, _: &mut Self::Control) -> io::Result<usize> {
    // 内核不支持 WRITE_FIXED 时由 compio 线程池回退。
    let result = unsafe {
      libc::pwrite(
        self.fd,
        self.ptr.cast(),
        self.len as usize,
        self.offset as libc::off_t,
      )
    };
    if result < 0 {
      Err(io::Error::last_os_error())
    } else {
      Ok(result as usize)
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{
    env::temp_dir,
    fs::{OpenOptions, read, remove_file, write},
    os::fd::AsRawFd,
    path::PathBuf,
    process::id,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
  };

  use super::*;

  fn temp_path(tag: &str) -> PathBuf {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    temp_dir().join(format!(
      "fkv_fixed_unit_{tag}_{}_{}_{}",
      id(),
      SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos(),
      seq
    ))
  }

  #[test]
  fn blocking_fallback_read_and_write() {
    let path = temp_path("rw");
    write(&path, b"hello world").unwrap();
    let file = OpenOptions::new()
      .read(true)
      .write(true)
      .open(&path)
      .unwrap();

    let dst = Arc::new(AlignedBuf::new(16));
    let mut read_op = FixedRead::new(file.as_raw_fd(), dst.clone(), 0, 11, 0, 0).unwrap();
    let read_len = read_op.call_blocking(&mut ()).unwrap();
    assert_eq!(read_len, 11);
    assert_eq!(&dst[..read_len], b"hello world");

    let mut plain = AlignedBuf::new(16);
    plain[..5].copy_from_slice(b"HELLO");
    let src = Arc::new(plain);
    let mut write_op = FixedWrite::new(file.as_raw_fd(), src, 0, 5, 6, 0).unwrap();
    let write_len = write_op.call_blocking(&mut ()).unwrap();
    assert_eq!(write_len, 5);
    assert_eq!(&read(&path).unwrap(), b"hello HELLO");

    remove_file(path).unwrap();
  }
}
