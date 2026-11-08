//! io-21：O_DIRECT 回退与在飞 open 的竞态。
//!
//! io-18 的类别回退只看 `direct_opened`（已成功 open 的类别位），不包含在飞
//! open。交错时序：
//!
//! 1. T1 以 O_DIRECT 打开 data 文件 A，OPENAT 在飞；
//! 2. T2 打开 data 文件 B 返回 `EINVAL`，此时 `direct_opened & Data == 0`
//!    → 关闭 `direct_io`；
//! 3. T1 的 OPENAT 成功返回，把 **O_DIRECT 句柄**插入 fd 缓存，而开关已是
//!    false。
//!
//! 此后所有走该句柄的读写都按 buffered 选缓冲（普通 `Vec` / `WriteChunk::
//! Plain`），在 O_DIRECT fd 上因地址 / 长度不页对齐持续 `EINVAL`。
//!
//! 本文件用单线程 `join` 构造该交错：A 的 open 在第一次 poll 时提交后挂起，
//! 随后轮询 B——B 命中路径级 `EINVAL` 注入，同步关闭类别开关；A 的 open
//! 完成后必须复查开关、丢弃 direct 句柄并按 buffered 重开（方案 A）。

use std::path::Path;

use fkv_io::{FileIo, PageIo};
use fkv_io_compio::CompioStore;
use fkv_page::{PageType, set_checksum, set_page_type};
use fkv_testkit::{TempRoot, direct_io_probe};
use fkv_types::{
  DEFAULT_SEGMENT_SIZE, DEFAULT_SEGMENTS_PER_FILE_SHIFT, MAIN_BRANCH_NAME, TableIdent,
  data_file_name,
};
use futures::future::join;

const PAGE_SIZE: usize = 4096;
const SHIFT: u8 = 2;

fn valid_page(fill: u8) -> Vec<u8> {
  let mut page = vec![0u8; PAGE_SIZE];
  set_page_type(&mut page, PageType::Data).unwrap();
  page[100] = fill;
  set_checksum(&mut page).unwrap();
  page
}

/// 与 `fkv_shard::build_compio_store` 相同的 builder 链：4 KiB 页、
/// 256 KiB 段、managed 读 + O_DIRECT（managed 会被 io-16 显式关闭）。
fn shard_default_store(root: &Path) -> CompioStore {
  CompioStore::new(root.to_path_buf(), PAGE_SIZE, SHIFT)
    .with_segment_config(DEFAULT_SEGMENT_SIZE, DEFAULT_SEGMENTS_PER_FILE_SHIFT)
    .with_managed_segment_read()
    .with_max_write_batch_pages(32)
    .with_max_inflight_write(32 * 1024)
    .with_fd_cache_limit(0)
    .with_direct_io(true)
}

/// 失败 open 先返回并关闭类别开关，随后完成的在飞 direct open 必须被丢弃、
/// 不置 `direct_opened`、不进入 fd 缓存，并按 buffered 重开。
#[compio::test]
async fn inflight_direct_open_discarded_after_concurrent_fallback() {
  let root = TempRoot::new("io21_race");
  let probe = direct_io_probe(root.path());
  if !probe.supported() {
    eprintln!("跳过：临时目录不支持 O_DIRECT，失败 open 与成功 open 无法交错");
    return;
  }
  let store = shard_default_store(root.path());
  let tbl = TableIdent::new("demo", 1).unwrap();
  // 预热分区目录（`dir_ready`）：两条 write_page 第一次 poll 就能走到 open，
  // 从而保证 join 的固定轮询顺序——先 A 提交 direct open，再 B 失败回退。
  store.write_file(&tbl, 2, &valid_page(13)).await.unwrap();

  // B 的路径：file_page_id 4 → file_id 1（pages_per_file_shift = 2）。
  let path_b = root
    .path()
    .join(tbl.dir_name())
    .join(data_file_name(1, MAIN_BRANCH_NAME, 0));
  store.inject_direct_open_einval_path(path_b);

  let page_a = valid_page(21);
  let page_b = valid_page(22);
  // join 先 poll A：direct open 提交后挂起；再 poll B：路径级 EINVAL 注入
  // 同步关闭 data 开关，并用 buffered 打开 B。
  let (result_a, result_b) = join(
    store.write_page(&tbl, 0, &page_a),
    store.write_page(&tbl, 4, &page_b),
  )
  .await;
  result_a.expect("在飞 direct open 应丢弃并按 buffered 重开，写不得 EINVAL");
  result_b.expect("命中 EINVAL 注入的 open 应按 buffered 回退成功");

  assert!(
    !store.direct_io_enabled(),
    "一次 EINVAL 应关闭 data 类别开关"
  );
  assert_eq!(store.direct_fallback_num(), 1, "只有 B 的注入失败计入回退");
  assert_eq!(
    store.direct_discard_num(),
    1,
    "开关关闭后完成的在飞 direct open 必须被丢弃重开"
  );
  // 类别独立（io-18）：data 回退不得连带关闭 segment。
  assert!(store.direct_segment_enabled());

  let mut dst = vec![0u8; PAGE_SIZE];
  store.read_page(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, page_a);
  store.read_page(&tbl, 4, &mut dst).await.unwrap();
  assert_eq!(dst, page_b);
}
