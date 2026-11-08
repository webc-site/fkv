//! FD 复用、短读 / checksum 校验、dirty 集合与 manifest 目录持久化。

use std::fs::{create_dir_all, read, write};

use fkv_error::KvError;
#[cfg(target_os = "linux")]
use fkv_io::FixedSegmentIo;
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_io_compio::CompioStore;
use fkv_page::{PageType, set_checksum, set_page_type};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

/// 构造一个 checksum 合法的 data page。
fn valid_page(page_size: usize, fill: u8) -> Vec<u8> {
  let mut page = vec![0u8; page_size];
  set_page_type(&mut page, PageType::Data).unwrap();
  page[100] = fill;
  set_checksum(&mut page).unwrap();
  page
}

#[compio::test]
async fn fd_cache_reuses_handle_for_repeated_page_io() {
  let root = TempRoot::new("hardening_fd");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2).with_segment_config(4096, 2);
  let page = valid_page(512, 1);
  store.write_page(&tbl, 0, &page).await.unwrap();
  let mut dst = vec![0u8; 512];
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, page);

  // 首次写 + 首次读各 open 一次（写句柄 / 只读句柄分开缓存）。
  let open_num = store.fd_open_num();
  assert_eq!(open_num, 2);
  assert_eq!(store.fd_cache_len(), 2);
  for _ in 0..64 {
    store.write_page(&tbl, 0, &page).await.unwrap();
    store.read_page(&tbl, 0, &mut dst).await.unwrap();
  }
  assert_eq!(store.fd_open_num(), open_num);
  assert_eq!(store.fd_cache_len(), 2);
}

#[compio::test]
async fn page_buffers_are_reused_across_io() {
  let root = TempRoot::new("hardening_buf");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2);
  let page = valid_page(512, 2);
  store.write_page(&tbl, 0, &page).await.unwrap();
  let mut dst = vec![0u8; 512];
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  let alloc_num = store.buf_alloc_num();
  for _ in 0..32 {
    store.write_page(&tbl, 0, &page).await.unwrap();
    store.read_page(&tbl, 0, &mut dst).await.unwrap();
  }
  // 写池 / 读池各一块缓冲，之后不再分配。
  assert_eq!(store.buf_alloc_num(), alloc_num);
  assert!(alloc_num <= 2, "alloc {alloc_num}");
}

#[compio::test]
async fn fd_cache_is_bounded() {
  let root = TempRoot::new("hardening_lru");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
    .with_segment_config(4096, 2)
    .with_checksum_verify(false)
    .with_fd_cache_limit(256);
  let page = vec![1u8; 512];
  let mut dst = vec![0u8; 512];
  for file_id in 0..300u64 {
    let file_page_id = file_id << 2;
    store.write_page(&tbl, file_page_id, &page).await.unwrap();
    store.read_page(&tbl, file_page_id, &mut dst).await.unwrap();
  }
  assert!(store.fd_cache_len() <= 256, "len {}", store.fd_cache_len());
}

#[compio::test]
async fn read_page_reuses_read_buffer_and_keeps_dst_tail() {
  let root = TempRoot::new("hardening_readbuf");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2).with_segment_config(4096, 2);
  let page = valid_page(512, 9);
  store.write_page(&tbl, 0, &page).await.unwrap();

  // dst 大于 page：只写前 page_size 字节，尾部保持原值（不越界写、不清零）。
  let mut dst = vec![0xABu8; 1024];
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(&dst[..512], &page[..]);
  assert!(
    dst[512..].iter().all(|byte| *byte == 0xAB),
    "page 之后的区域被改写"
  );

  // 读缓冲按容量复用且不预清零：重复读不再分配缓冲，只保留一处 copy。
  let alloc_num = store.buf_alloc_num();
  for _ in 0..16 {
    store.read_page(&tbl, 0, &mut dst).await.unwrap();
    assert_eq!(&dst[..512], &page[..]);
    assert!(
      dst[512..].iter().all(|byte| *byte == 0xAB),
      "page 之后的区域被改写"
    );
  }
  assert_eq!(store.buf_alloc_num(), alloc_num, "读路径不应再分配缓冲");
}

#[compio::test]
async fn truncated_page_fails_and_short_segment_zero_fills() {
  let root = TempRoot::new("hardening_short");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
    .with_segment_config(4096, 2)
    .with_checksum_verify(false);
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  // 只落半页：同一 offset 的短读重试仍返回 256 字节，候选耗尽后按 Corrupted
  // 上报；0 字节才是确定性 EOF。两种情况都不能把零填充当数据返回。
  write(dir.join("data_0_main_0"), vec![7u8; 256]).unwrap();
  let mut dst = vec![0u8; 512];
  assert_eq!(
    store.read_page(&tbl, 0, &mut dst).await.unwrap_err(),
    KvError::Corrupted
  );
  // segment 是定长段：文件短于请求时读取可用前缀并零填充，不报错（io-41）。
  write(dir.join("segment_0_main_0"), vec![8u8; 128]).unwrap();
  let mut segment = vec![0xFFu8; 4096];
  store.read_segment(&tbl, 0, &mut segment).await.unwrap();
  assert_eq!(&segment[..128], &[8u8; 128][..]);
  assert!(segment[128..].iter().all(|byte| *byte == 0));
}

#[compio::test]
async fn corrupted_page_is_detected_by_checksum() {
  let root = TempRoot::new("hardening_sum");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2);
  let page = valid_page(512, 3);
  store.write_page(&tbl, 0, &page).await.unwrap();
  let mut dst = vec![0u8; 512];
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, page);

  // 外部破坏页内容（checksum 前缀不动）。
  let path = root.join("demo.1").join("data_0_main_0");
  let mut bytes = read(&path).unwrap();
  bytes[100] ^= 0xFF;
  write(&path, &bytes).unwrap();
  assert_eq!(
    store.read_page(&tbl, 0, &mut dst).await.unwrap_err(),
    KvError::Corrupted
  );

  // 运行期关闭校验（skip_verify_checksum）后仍可读；重新打开后再次报错。
  store.set_skip_verify_checksum(true);
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst[100], 3 ^ 0xFF);
  store.set_skip_verify_checksum(false);
  assert_eq!(
    store.read_page(&tbl, 0, &mut dst).await.unwrap_err(),
    KvError::Corrupted
  );
}

#[compio::test]
async fn sync_data_flushes_dirty_files_and_clears_set() {
  let root = TempRoot::new("hardening_dirty");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2).with_segment_config(4096, 2);
  let page = valid_page(512, 4);
  store.write_page(&tbl, 0, &page).await.unwrap();
  store.write_page(&tbl, 4, &page).await.unwrap();
  store.write_segment(&tbl, 0, &[5u8; 64]).await.unwrap();
  assert_eq!(store.dirty_file_num(&tbl), 3);

  store.sync_data(&tbl).await.unwrap();
  assert_eq!(store.dirty_file_num(&tbl), 0);
  // 重复 sync 是空操作。
  store.sync_data(&tbl).await.unwrap();
  assert_eq!(store.dirty_file_num(&tbl), 0);

  // 再次写重新标脏；删除文件后不再计入。
  store.write_page(&tbl, 0, &page).await.unwrap();
  assert_eq!(store.dirty_file_num(&tbl), 1);
  store.remove_file(&tbl, 0).await.unwrap();
  assert_eq!(store.dirty_file_num(&tbl), 0);
}

#[compio::test]
async fn switch_manifest_invalidates_cached_handle() {
  let root = TempRoot::new("hardening_manifest");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2);
  store.append_manifest(&tbl, b"one").await.unwrap();
  store.switch_manifest(&tbl, b"two").await.unwrap();
  assert_eq!(store.read_manifest(&tbl).await.unwrap().unwrap(), b"two");
  // rename 之后旧句柄失效：继续 append 必须落到新 inode。
  store.append_manifest(&tbl, b"!").await.unwrap();
  assert_eq!(store.read_manifest(&tbl).await.unwrap().unwrap(), b"two!");

  // 非活跃 manifest 文件同样 write -> rename -> 目录 fsync。
  store
    .write_manifest_file(&tbl, "manifest_extra_0", b"x")
    .await
    .unwrap();
  assert_eq!(
    store
      .read_manifest_file(&tbl, "manifest_extra_0")
      .await
      .unwrap()
      .unwrap(),
    b"x"
  );
  let name_li = store.list_manifest_file_name_li(&tbl).await.unwrap();
  assert!(name_li.iter().any(|name| name == "manifest_extra_0"));
  assert!(!root.join("demo.1").join("manifest_main_0.tmp").exists());
}

/// io-26：FD 缓存上限可配置，超出后按 LRU 淘汰。
#[compio::test]
async fn fd_cache_limit_is_configurable() {
  let root = TempRoot::new("hardening_fd_limit");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
    .with_segment_config(4096, 2)
    .with_fd_cache_limit(2);
  assert_eq!(store.fd_cache_limit(), 2);
  let page = valid_page(512, 9);
  // shift=2：每个 file id 4 页，写 0/4/8/12/16 产生 5 个写句柄。
  for file_page_id in [0u64, 4, 8, 12, 16] {
    store.write_page(&tbl, file_page_id, &page).await.unwrap();
  }
  assert!(store.fd_cache_len() <= 2);
}

/// io-26：limit=0 表示不限制，所有打开句柄都留在缓存。
#[compio::test]
async fn fd_cache_limit_zero_is_unlimited() {
  let root = TempRoot::new("hardening_fd_unlimited");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
    .with_segment_config(4096, 2)
    .with_fd_cache_limit(0);
  assert_eq!(store.fd_cache_limit(), usize::MAX);
  let page = valid_page(512, 9);
  for file_page_id in [0u64, 4, 8, 12, 16] {
    store.write_page(&tbl, file_page_id, &page).await.unwrap();
  }
  assert_eq!(store.fd_cache_len(), 5);
}

/// eng-79：超过上限的多文件写入按 LRU 淘汰，重复命中不再产生 open。
#[compio::test]
async fn fd_cache_evicts_and_reuses_under_limit() {
  let root = TempRoot::new("hardening_fd_lru");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
    .with_segment_config(4096, 2)
    .with_fd_cache_limit(4);
  let page = valid_page(512, 5);
  // shift=2：每个 file id 4 页；file_page_id 取 4 的倍数即一个独立文件。
  let file_page_li: Vec<u64> = (0..12).map(|index| index * 4).collect();
  for &file_page_id in &file_page_li {
    store.write_page(&tbl, file_page_id, &page).await.unwrap();
  }
  assert!(store.fd_cache_len() <= 4);

  // 缓存内保留最后 4 个文件（最近使用）；重复写不新增 open。
  let open_num = store.fd_open_num();
  for &file_page_id in &file_page_li[8..] {
    store.write_page(&tbl, file_page_id, &page).await.unwrap();
  }
  assert_eq!(store.fd_open_num(), open_num);
  assert!(store.fd_cache_len() <= 4);
}

/// io-21：Linux 下数据页 O_DIRECT 读写必须走 4 KiB 对齐缓冲。
#[cfg(target_os = "linux")]
#[compio::test]
async fn direct_io_page_roundtrip() {
  let root = TempRoot::new("hardening_direct");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 4096, 2)
    .with_segment_config(4096, 2)
    .with_direct_io(true);
  let page = valid_page(4096, 7);
  store.write_page(&tbl, 0, &page).await.unwrap();
  let mut dst = vec![0u8; 4096];
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, page);
}

/// io-21：Linux 下 segment O_DIRECT 整段写、非对齐长度读往返一致。
#[cfg(target_os = "linux")]
#[compio::test]
async fn direct_io_segment_roundtrip() {
  let root = TempRoot::new("hardening_direct_segment");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let segment_size = 16 * 1024;
  let store = CompioStore::new(root.path().to_path_buf(), 4096, 2)
    .with_segment_config(segment_size, 2)
    .with_direct_io(true);
  let src: Vec<u8> = (0..5000).map(|i| (i % 251) as u8).collect();
  store.write_segment(&tbl, 0, &src).await.unwrap();
  // 段尾补零：整段可读，但只比较已写入前缀。
  let mut dst = vec![0u8; 4099];
  store.read_segment(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, src[..4099]);
  // 跨段偏移仍按段对齐读写。
  let src2 = [9u8; 33];
  store.write_segment(&tbl, 3, &src2).await.unwrap();
  let mut dst2 = vec![0u8; 33];
  store.read_segment(&tbl, 3, &mut dst2).await.unwrap();
  assert_eq!(dst2, src2);
}

/// io-07：fixed buffer 槽位在 `FixedBuffer` drop 后回收，不能累计 64 次后 OutOfMem。
#[cfg(target_os = "linux")]
#[compio::test]
async fn fixed_buffer_slots_are_recycled() {
  let root = TempRoot::new("hardening_fixed_recycle");
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2).with_segment_config(4096, 2);
  for _ in 0..100 {
    let buf = store.register_fixed_buffer(4096).unwrap();
    assert!(buf.is_registered());
    drop(buf);
  }
}

#[compio::test]
async fn empty_page_file_returns_end_of_file() {
  let root = TempRoot::new("hardening_eof");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
    .with_segment_config(4096, 2)
    .with_checksum_verify(false);
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  // 0 字节 = 确定性 EOF；对齐 C++ `res == 0`，不能报 Corrupted 或重试。
  write(dir.join("data_0_main_0"), Vec::new()).unwrap();
  let mut dst = vec![0u8; 512];
  assert_eq!(
    store.read_page(&tbl, 0, &mut dst).await.unwrap_err(),
    KvError::EndOfFile
  );
}

/// io-31：新 term 文件被截断成非页对齐长度（torn write）时，短读只在同一
/// 路径重试，耗尽后返回 `Corrupted`；不得落回旧 term 候选返回陈旧页。
#[compio::test]
async fn short_read_exhaustion_is_terminal_not_old_term_fallback() {
  const RETRY_NUM: u64 = 3;
  let root = TempRoot::new("hardening_io31_short");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2);
  let old_page = valid_page(512, 0xA1);
  store.write_page(&tbl, 0, &old_page).await.unwrap();

  // 当前 identity 切到 term 1，直接落半页（绕过写路径）模拟 torn write。
  store.set_file_identity("main", 1);
  write(root.join("demo.1").join("data_0_main_1"), [0xB2u8; 256]).unwrap();

  let mut dst = vec![0xEEu8; 512];
  let before = store.read_at_num();
  assert_eq!(
    store.read_page(&tbl, 0, &mut dst).await.unwrap_err(),
    KvError::Corrupted
  );
  // 3 次尝试全部落在新 term 文件上；若回退旧 term 会多出一次读。
  assert_eq!(store.read_at_num() - before, RETRY_NUM);
  assert_eq!(store.stale_read_num(), 0);
  assert!(dst.iter().all(|byte| *byte == 0xEE), "dst 被旧 term 页覆盖");
}

/// io-31：稀疏洞（新 term 该页全零、checksum 失败）仍回退旧 term 数据页，
/// 迁移机制不被短读终局化误杀；回退命中计数可见。
#[compio::test]
async fn checksum_hole_falls_back_to_old_term_page() {
  let root = TempRoot::new("hardening_io31_hole");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2);
  let old_page = valid_page(512, 0xA3);
  store.write_page(&tbl, 0, &old_page).await.unwrap();

  store.set_file_identity("main", 1);
  // 新 term 文件该页是洞（零页），checksum 必然不通过。
  write(root.join("demo.1").join("data_0_main_1"), [0u8; 512]).unwrap();

  let mut dst = vec![0u8; 512];
  let before = store.read_at_num();
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, old_page);
  // 每个候选各读一次：新 term 校验失败 → 旧 term 命中。
  assert_eq!(store.read_at_num() - before, 2);
  assert_eq!(store.stale_read_num(), 1);
}

/// io-31：新 term 文件长度不及该页 offset 时读到 0 字节（EOF），仍回退旧
/// term 数据页；io-39 既有语义不变。
#[compio::test]
async fn eof_candidate_falls_back_to_old_term_page() {
  let root = TempRoot::new("hardening_io31_eof");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2);
  let old_page = valid_page(512, 0xA4);
  store.write_page(&tbl, 0, &old_page).await.unwrap();

  store.set_file_identity("main", 1);
  write(root.join("demo.1").join("data_0_main_1"), Vec::new()).unwrap();

  let mut dst = vec![0u8; 512];
  let before = store.read_at_num();
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, old_page);
  assert_eq!(store.read_at_num() - before, 2);
  assert_eq!(store.stale_read_num(), 1);
}

/// io-31：瞬时 `TryAgain` 只重试同一路径。重试成功读到新 term 页；耗尽后
/// 上抛 `TryAgain`，绝不切换旧 term 候选。
#[compio::test]
async fn transient_read_error_retries_same_path_only() {
  let root = TempRoot::new("hardening_io31_retry");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2);
  let old_page = valid_page(512, 0xA5);
  store.write_page(&tbl, 0, &old_page).await.unwrap();
  let new_page = valid_page(512, 0xB5);
  store.set_file_identity("main", 1);
  store.write_page(&tbl, 0, &new_page).await.unwrap();

  let mut dst = vec![0u8; 512];
  // 前 2 次注入 TryAgain，第 3 次成功：3 次都读同一路径，不碰旧 term。
  store.inject_read_try_again(2);
  let before = store.read_at_num();
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, new_page);
  assert_eq!(store.read_at_num() - before, 3);
  assert_eq!(store.stale_read_num(), 0);

  // 3 次全部注入：重试耗尽后上抛 TryAgain，dst 不被旧 term 页覆盖。
  dst.fill(0xEE);
  store.inject_read_try_again(3);
  let before = store.read_at_num();
  assert_eq!(
    store.read_page(&tbl, 0, &mut dst).await.unwrap_err(),
    KvError::TryAgain
  );
  assert_eq!(store.read_at_num() - before, 3);
  assert_eq!(store.stale_read_num(), 0);
  assert!(dst.iter().all(|byte| *byte == 0xEE), "dst 被旧 term 页覆盖");
}

/// 打开路径的非 NotFound errno 不能被折叠成 ResourceMissing / 恒 IoFail：
/// 把期望路径替换成同名目录（EISDIR）时，data / segment 都应回 `IoFail`。
#[compio::test]
async fn open_errors_keep_errno_classification() {
  use std::fs::create_dir_all;

  let root = TempRoot::new("hardening_open_errno");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let dir = root.join("demo.1");
  create_dir_all(dir.join("data_0_main_0")).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 1)
    .with_segment_config(4096, 1)
    .with_checksum_verify(false);
  let mut dst = vec![0u8; 512];
  assert_eq!(
    store.read_page(&tbl, 0, &mut dst).await.unwrap_err(),
    KvError::IoFail
  );

  create_dir_all(dir.join("segment_0_main_0")).unwrap();
  let mut segment = vec![0u8; 4096];
  assert_eq!(
    store.read_segment(&tbl, 0, &mut segment).await.unwrap_err(),
    KvError::IoFail
  );
}

/// write_file 的 tmp fdatasync 失败必须清理 tmp 并保留旧文件。
#[compio::test]
async fn write_file_sync_failure_keeps_old_file() {
  let root = TempRoot::new("hardening_write_file_sync");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 1);
  store.write_file(&tbl, 0, &[1u8; 512]).await.unwrap();
  fkv_failpoint::set_spec("WriteFile:SyncFail@1!err=IoFail");
  assert_eq!(
    store.write_file(&tbl, 0, &[2u8; 512]).await.unwrap_err(),
    KvError::IoFail
  );
  fkv_failpoint::reset();
  assert_eq!(
    store.read_file(&tbl, 0).await.unwrap().unwrap(),
    vec![1u8; 512]
  );
  assert!(!root.join("demo.1").join("data_0_main_0.tmp").exists());
}
