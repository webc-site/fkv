//! 内核 provided-buffer 段读（io-35）：池配置正确时走 managed read，
//! 池尺寸不足时回落自管读池且数据正确。

use compio::{driver::ProactorBuilder, fs::File, io::AsyncReadManagedAt, runtime::Runtime};
use fkv_io::SegmentIo;
use fkv_io_compio::CompioStore;
use fkv_testkit::TempRoot;
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
    std::fs::write(&probe, [0u8; 4096]).unwrap();
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
