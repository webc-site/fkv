//! 内核 provided-buffer 段读（io-35）：池配置正确时走 managed read，
//! 池尺寸不足时回落自管读池且数据正确。
//!
//! io-16：O_DIRECT 与 managed 读互斥——`with_direct_io(true)` 会显式关闭
//! managed 开关并告警；fixed 注册读走 `READ_FIXED`，不依赖 runtime 缓冲池。

use std::fs::write;

use compio::{driver::ProactorBuilder, fs::File, io::AsyncReadManagedAt, runtime::Runtime};
use fkv_io::{FixedSegmentIo, SegmentIo};
use fkv_io_compio::CompioStore;
use fkv_testkit::{TempRoot, capture_warn};
use fkv_types::TableIdent;

/// 大于 runtime 池默认缓冲（8192 B），确保「池过小」用例真的短读。
const SEG: usize = 64 * 1024;

fn payload() -> Vec<u8> {
  (0..SEG).map(|i| (i % 251) as u8).collect()
}

#[test]
fn managed_segment_read_skips_self_pool() {
  let mut proactor = ProactorBuilder::new();
  proactor.buffer_pool_buffer_len(SEG);
  let runtime = Runtime::builder().with_proactor(proactor).build().unwrap();
  runtime.block_on(async {
    // `TempRoot` 析构时递归清理，无需手工 `remove_dir_all`。
    let root = TempRoot::new("managed_enabled");
    // provided-buffer 依赖内核能力：部分 arm64 内核对带 buffer select 的
    // read 返回 EINVAL。生产路径会回落自管池，这里先探测，不支持就走
    // fallback 用例（`managed_segment_read_falls_back_when_pool_too_small`）。
    let probe = root.path().join("managed_probe");
    write(&probe, [0u8; 4096]).unwrap();
    let probe_file = File::open(&probe).await.unwrap();
    if probe_file.read_managed_at(4096, 0).await.is_err() {
      eprintln!("跳过：当前 runtime 不支持 managed read");
      return;
    }
    drop(probe_file);
    let tbl = TableIdent::new("demo", 1).unwrap();
    let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
      .with_segment_config(SEG, 2)
      .with_managed_segment_read();
    assert!(store.managed_segment_read_enabled());
    let src = payload();
    store.write_segment(&tbl, 0, &src).await.unwrap();
    let alloc_before = store.buf_alloc_num();
    let mut dst = vec![0u8; SEG];
    store.read_segment(&tbl, 0, &mut dst).await.unwrap();
    assert_eq!(dst, src);
    // 走 managed read 不触碰自管读池，因此分配计数不变。
    assert_eq!(store.buf_alloc_num(), alloc_before);
  });
}

#[test]
fn managed_segment_read_falls_back_when_pool_too_small() {
  let runtime = Runtime::new().unwrap();
  runtime.block_on(async {
    let root = TempRoot::new("managed_fallback");
    let tbl = TableIdent::new("demo", 1).unwrap();
    let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
      .with_segment_config(SEG, 2)
      .with_managed_segment_read();
    assert!(store.managed_segment_read_enabled());
    let src = payload();
    store.write_segment(&tbl, 0, &src).await.unwrap();
    let alloc_before = store.buf_alloc_num();
    let mut dst = vec![0u8; SEG];
    store.read_segment(&tbl, 0, &mut dst).await.unwrap();
    assert_eq!(dst, src);
    // 默认池只有 8192 B，短读后回落自管池，必然新分配一次读缓冲。
    assert!(store.buf_alloc_num() > alloc_before);
  });
}

/// io-16：生产装配顺序（先 managed、后 O_DIRECT）必须显式关闭开关并告警，
/// 且关闭后 O_DIRECT 读路径数据正确。
#[compio::test]
async fn managed_read_requested_before_direct_warns_and_still_reads() {
  let warn = capture_warn();
  let root = TempRoot::new("managed_direct_late");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 4096, 2)
    .with_segment_config(SEG, 2)
    .with_managed_segment_read()
    .with_direct_io(true);
  assert!(!store.managed_segment_read_enabled());
  assert!(
    warn.warned_with(&["managed segment read"]),
    "O_DIRECT 关闭 managed 读必须告警"
  );

  let src = payload();
  store.write_segment(&tbl, 0, &src).await.unwrap();
  let mut dst = vec![0u8; SEG];
  store.read_segment(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, src);
}

/// io-16：反向顺序（先 O_DIRECT、后 managed）同样互斥，不静默保留开关。
#[test]
fn managed_read_requested_after_direct_is_rejected() {
  let warn = capture_warn();
  let root = TempRoot::new("managed_direct_first");
  let store = CompioStore::new(root.path().to_path_buf(), 4096, 2)
    .with_segment_config(SEG, 2)
    .with_direct_io(true)
    .with_managed_segment_read();
  assert!(!store.managed_segment_read_enabled());
  assert!(
    warn.warned_with(&["managed segment read"]),
    "O_DIRECT 关闭 managed 读必须告警"
  );
}

/// io-16：只有 data 页 O_DIRECT（page_size 页对齐）而 segment_size 未页对齐时，
/// segment 文件仍是 buffered，managed 读合法，`with_direct_io` 不得误关开关。
#[test]
fn managed_read_survives_direct_io_when_segment_stays_buffered() {
  let root = TempRoot::new("managed_direct_data_only");
  let store = CompioStore::new(root.path().to_path_buf(), 4096, 2)
    .with_segment_config(3000, 2)
    .with_managed_segment_read()
    .with_direct_io(true);
  assert!(
    store.managed_segment_read_enabled(),
    "segment 仍 buffered 时 managed 读不应被 data 页 O_DIRECT 关闭"
  );
}

/// io-16：fixed 注册读走 `READ_FIXED` + 注册缓冲，与 provided-buffer 池无关；
/// O_DIRECT 下仍能读回整段（非 Linux 回落 `read_segment` 的对齐缓冲分支）。
#[compio::test]
async fn fixed_segment_read_unchanged_under_direct_io() {
  let root = TempRoot::new("managed_direct_fixed");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 4096, 2)
    .with_segment_config(SEG, 2)
    .with_direct_io(true);
  assert!(!store.managed_segment_read_enabled());

  let src = payload();
  store.write_segment(&tbl, 0, &src).await.unwrap();
  let mut buf = store.register_fixed_buffer(SEG).unwrap();
  store
    .read_segment_fixed(&tbl, 0, &mut buf, 0, SEG)
    .await
    .unwrap();
  assert_eq!(buf.as_slice(), src.as_slice());
}
