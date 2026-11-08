//! io-18：O_DIRECT 默认接线与「文件系统不支持时回退 buffered」。
//!
//! 生产装配（shard / SDK / cloud local cache）硬编码 `with_direct_io(true)`。
//! tmpfs / 部分 overlay / 网络文件系统上 open 会返回 `EINVAL` / `EOPNOTSUPP`；
//! 本文件验证：
//! 1. 生产 `ShardConfig` 等价的 builder 链（4 KiB 页 + 256 KiB 段）确实让
//!    data / segment 的 direct 生效（移除接线时断言失败）；
//! 2. 不支持 O_DIRECT 时按路径类别回退 buffered 并告警，读写仍正确；
//! 3. data / segment 的回退互不影响；已有 direct 句柄后不把开关置 false
//!    （避免对齐 fd 配非对齐缓冲再次 `EINVAL`）。

use std::path::Path;

use fkv_io::{FileIo, PageIo, SegmentIo};
use fkv_io_compio::CompioStore;
use fkv_page::{PageType, set_checksum, set_page_type};
use fkv_testkit::{TempRoot, capture_warn, direct_io_probe};
use fkv_types::{DEFAULT_SEGMENT_SIZE, DEFAULT_SEGMENTS_PER_FILE_SHIFT, TableIdent};

const PAGE_SIZE: usize = 4096;
const SHIFT: u8 = 2;

fn valid_page(fill: u8) -> Vec<u8> {
  let mut page = vec![0u8; PAGE_SIZE];
  set_page_type(&mut page, PageType::Data).unwrap();
  page[100] = fill;
  set_checksum(&mut page).unwrap();
  page
}

/// 与生产 `fkv_shard::backend::build_compio_store` 同步的 builder 链（该函数
/// `pub(crate)` 不可复用）：生产默认 page 4 KiB、segment 256 KiB、managed 读 +
/// O_DIRECT。改生产装配参数时必须同步本函数。
fn shard_default_store(root: &Path) -> CompioStore {
  CompioStore::new(root.to_path_buf(), PAGE_SIZE, SHIFT)
    .with_segment_config(DEFAULT_SEGMENT_SIZE, DEFAULT_SEGMENTS_PER_FILE_SHIFT)
    .with_managed_segment_read()
    .with_max_write_batch_pages(32)
    .with_max_inflight_write(32 * 1024)
    .with_fd_cache_limit(0)
    .with_direct_io(true)
}

/// 生产默认接线必须真的开启 direct（而不是被对齐门静默关掉）。
#[compio::test]
async fn shard_default_wiring_enables_direct_io() {
  let root = TempRoot::new("io18_wiring");
  let probe = direct_io_probe(root.path());
  if !probe.supported() {
    eprintln!("跳过：临时目录不支持 O_DIRECT，生效断言不适用");
    return;
  }
  let store = shard_default_store(root.path());
  assert!(store.direct_io_enabled(), "data 页默认接线未开启 O_DIRECT");
  assert!(
    store.direct_segment_enabled(),
    "segment 默认接线未开启 O_DIRECT"
  );
  assert!(
    !store.managed_segment_read_enabled(),
    "O_DIRECT segment 下 managed 读必须显式关闭（io-16）"
  );

  let tbl = TableIdent::new("demo", 1).unwrap();
  let page = valid_page(3);
  store.write_page(&tbl, 0, &page).await.unwrap();
  let mut dst = vec![0u8; PAGE_SIZE];
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, page);

  let seg = vec![7u8; 16 * 1024];
  store.write_segment(&tbl, 0, &seg).await.unwrap();
  let mut seg_dst = vec![0u8; seg.len()];
  store.read_segment(&tbl, 0, &mut seg_dst).await.unwrap();
  assert_eq!(seg_dst, seg);

  // `read_file` 的 direct 分块分支（io-17）也要在默认接线下往返正确。
  let bytes: Vec<u8> = (0..3 * PAGE_SIZE).map(|i| (i % 251) as u8).collect();
  store.write_file(&tbl, 0, &bytes).await.unwrap();
  assert_eq!(store.read_file(&tbl, 0).await.unwrap().unwrap(), bytes);
  assert_eq!(
    store.direct_fallback_num(),
    0,
    "当前文件系统支持 O_DIRECT 时不应发生回退"
  );
}

/// data 文件 open 返回 EINVAL：回退 buffered、告警、读写仍正确；
/// segment 的 direct 不受影响（按路径类别独立）。
#[compio::test]
async fn data_open_einval_falls_back_and_keeps_segment_direct() {
  let warn = capture_warn();
  let root = TempRoot::new("io18_data_fallback");
  let store = shard_default_store(root.path());
  store.inject_direct_open_einval(false);

  let tbl = TableIdent::new("demo", 1).unwrap();
  let page = valid_page(5);
  store.write_page(&tbl, 0, &page).await.unwrap();
  assert!(
    !store.direct_io_enabled(),
    "data 回退后 direct 标志应为 false"
  );
  assert!(
    store.direct_segment_enabled(),
    "data 回退不得连带关闭 segment O_DIRECT"
  );
  assert_eq!(store.direct_fallback_num(), 1);
  assert!(
    warn.warned_with(&["O_DIRECT 不可用", "io18_data_fallback"]),
    "回退必须打 warn 日志，不能静默降级"
  );

  let mut dst = vec![0u8; PAGE_SIZE];
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, page);

  // 整文件读走的也是 data 路径：回退后按 buffered 分支读回。
  let bytes = vec![9u8; 2 * PAGE_SIZE];
  store.write_file(&tbl, 0, &bytes).await.unwrap();
  assert_eq!(store.read_file(&tbl, 0).await.unwrap().unwrap(), bytes);

  // segment 仍按 O_DIRECT 路径打开（真实文件系统支持时）并读写正确。
  let seg = vec![1u8; 8192];
  store.write_segment(&tbl, 0, &seg).await.unwrap();
  let mut seg_dst = vec![0u8; seg.len()];
  store.read_segment(&tbl, 0, &mut seg_dst).await.unwrap();
  assert_eq!(seg_dst, seg);
}

/// segment 文件 open 返回 EINVAL：只回退 segment，data 的 direct 保持生效。
#[compio::test]
async fn segment_open_einval_falls_back_without_disabling_data() {
  let warn = capture_warn();
  let root = TempRoot::new("io18_segment_fallback");
  let probe = direct_io_probe(root.path());
  if !probe.supported() {
    eprintln!("跳过：临时目录不支持 O_DIRECT，无法验证「data 已 direct」");
    return;
  }
  let store = shard_default_store(root.path());

  let tbl = TableIdent::new("demo", 1).unwrap();
  // 先真实打开一个 data 文件，证明该类 direct 可用（direct_opened）。
  let page = valid_page(6);
  store.write_page(&tbl, 0, &page).await.unwrap();
  assert!(store.direct_io_enabled());

  store.inject_direct_open_einval(true);
  let seg = vec![2u8; 8192];
  store.write_segment(&tbl, 0, &seg).await.unwrap();
  assert!(!store.direct_segment_enabled(), "segment 回退后应为 false");
  assert!(
    store.direct_io_enabled(),
    "segment 回退不得连带关闭 data O_DIRECT"
  );
  assert_eq!(store.direct_fallback_num(), 1);
  assert!(
    warn.warned_with(&["O_DIRECT 不可用", "io18_segment_fallback"]),
    "segment 回退必须打 warn 日志"
  );

  let mut seg_dst = vec![0u8; seg.len()];
  store.read_segment(&tbl, 0, &mut seg_dst).await.unwrap();
  assert_eq!(seg_dst, seg);
  let mut dst = vec![0u8; PAGE_SIZE];
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, page);
}

/// 已有 direct 句柄后再次 EINVAL：只让本次 open 走 buffered，类别开关保持
/// true。否则后续写会用非对齐缓冲写已打开的 O_DIRECT fd，再次 EINVAL。
#[compio::test]
async fn einval_after_direct_open_keeps_category_enabled() {
  let root = TempRoot::new("io18_proven_keep");
  let probe = direct_io_probe(root.path());
  if !probe.supported() {
    eprintln!("跳过：临时目录不支持 O_DIRECT，无法验证「已有 direct 句柄」");
    return;
  }
  let store = shard_default_store(root.path());
  let tbl = TableIdent::new("demo", 1).unwrap();

  // file_page_id 0 与 4 分属两个 data 文件（shift=2）。
  let first = valid_page(8);
  store.write_page(&tbl, 0, &first).await.unwrap();
  assert!(store.direct_io_enabled());

  store.inject_direct_open_einval(false);
  let second = valid_page(9);
  store.write_page(&tbl, 4, &second).await.unwrap();
  assert!(
    store.direct_io_enabled(),
    "已有 direct 句柄时不得把类别开关置 false"
  );
  assert_eq!(store.direct_fallback_num(), 1);

  let mut dst = vec![0u8; PAGE_SIZE];
  store.read_page(&tbl, 4, &mut dst).await.unwrap();
  assert_eq!(dst, second);
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, first);
}

/// Linux tmpfs（`/dev/shm`）真实环境：不支持 O_DIRECT 时必须回退后正常读写，
/// 而不是让首次写就 `IoFail`。
#[cfg(target_os = "linux")]
#[compio::test]
async fn tmpfs_falls_back_to_buffered() {
  let shm = Path::new("/dev/shm");
  if !shm.is_dir() {
    eprintln!("跳过：当前环境没有 /dev/shm");
    return;
  }
  let root = TempRoot::with_base(shm, "io18_tmpfs");
  // 少数环境把 /dev/shm 挂成支持 O_DIRECT 的文件系统；那就不属于本用例。
  if direct_io_probe(root.path()).supported() {
    eprintln!("跳过：/dev/shm 支持 O_DIRECT，回退路径不适用");
    return;
  }

  let store = shard_default_store(root.path());
  let tbl = TableIdent::new("demo", 1).unwrap();
  let page = valid_page(4);
  store.write_page(&tbl, 0, &page).await.unwrap();
  assert!(
    !store.direct_io_enabled(),
    "tmpfs 上 data 必须回退 buffered"
  );
  let mut dst = vec![0u8; PAGE_SIZE];
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, page);

  let seg = vec![3u8; 8192];
  store.write_segment(&tbl, 0, &seg).await.unwrap();
  assert!(
    !store.direct_segment_enabled(),
    "tmpfs 上 segment 必须回退 buffered"
  );
  let mut seg_dst = vec![0u8; seg.len()];
  store.read_segment(&tbl, 0, &mut seg_dst).await.unwrap();
  assert_eq!(seg_dst, seg);
}
