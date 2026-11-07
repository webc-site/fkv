//! 读 / 写缓冲池的字节预算：大整文件读失败不遗留整文件缓冲，
//! 稳态 256 KiB 段读仍在预算内满命中。

use std::fs::{create_dir_all, write};

use fkv_error::KvError;
use fkv_io::{FileIo, SegmentIo};
use fkv_io_compio::CompioStore;
use fkv_testkit::TempRoot;
use fkv_types::{DEFAULT_SEGMENT_SIZE, TableIdent};

/// 与 `buf.rs` 的 `BUF_POOL_BUDGET` 对齐的池字节上界。
const BUF_POOL_BUDGET: usize = 4 << 20;

/// 8 MiB（= 默认 manifest_limit）整文件读短读失败后，整文件缓冲不入读池。
#[compio::test]
async fn read_whole_short_read_does_not_pool_whole_file_buffer() {
  let root = TempRoot::new("buf_pool_short_whole");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 4096, 2);
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  let len = 8 << 20;
  write(dir.join("data_0_main_0"), vec![3u8; len]).unwrap();

  store.inject_short_read(len / 2);
  assert_eq!(store.read_file(&tbl, 0).await.unwrap_err(), KvError::IoFail);
  assert_eq!(store.read_buf_pooled_bytes(), 0, "整文件缓冲不应留在读池");
}

/// 多次整文件读失败：每个长度各占一个桶，累计字节不超过预算，超出即丢弃。
#[compio::test]
async fn read_whole_failures_respect_byte_budget() {
  let root = TempRoot::new("buf_pool_budget");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 4096, 2);
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  // 5 个不同长度、均不超过 1 MiB 单块上限的 data 文件。
  let len_li: Vec<usize> = (0..5).map(|index| (1 << 20) - index * 4096).collect();
  for (file_id, len) in len_li.iter().copied().enumerate() {
    write(dir.join(format!("data_{file_id}_main_0")), vec![1u8; len]).unwrap();
  }

  let mut pooled = 0;
  for (file_id, len) in len_li.iter().copied().enumerate() {
    store.inject_short_read(len / 2);
    assert_eq!(
      store.read_file(&tbl, file_id as u64).await.unwrap_err(),
      KvError::IoFail
    );
    // 前 4 块（合计小于 4 MiB）入池；第 5 块会顶破预算，直接丢弃。
    if file_id < 4 {
      pooled += len;
      assert_eq!(store.read_buf_pooled_bytes(), pooled);
    }
  }
  assert_eq!(store.read_buf_pooled_bytes(), pooled);
  assert!(pooled <= BUF_POOL_BUDGET);
}

/// 默认 256 KiB 段读：单块在预算内，8 块可全量入池，稳态重复读零分配。
#[compio::test]
async fn segment_reads_reuse_pool_within_budget() {
  let root = TempRoot::new("buf_pool_segment");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 4096, 2);
  let src = vec![7u8; DEFAULT_SEGMENT_SIZE];
  store.write_segment(&tbl, 0, &src).await.unwrap();
  assert_eq!(store.write_buf_pooled_bytes(), DEFAULT_SEGMENT_SIZE);

  let mut dst = vec![0u8; DEFAULT_SEGMENT_SIZE];
  store.read_segment(&tbl, 0, &mut dst).await.unwrap();
  assert_eq!(dst, src);
  assert_eq!(store.read_buf_pooled_bytes(), DEFAULT_SEGMENT_SIZE);

  let alloc_num = store.buf_alloc_num();
  let take_num = store.read_buf_take_num();
  for _ in 0..8 {
    store.read_segment(&tbl, 0, &mut dst).await.unwrap();
  }
  assert_eq!(store.buf_alloc_num(), alloc_num, "稳态段读不应再分配");
  assert_eq!(store.read_buf_take_num(), take_num + 8);
  assert_eq!(store.read_buf_pooled_bytes(), DEFAULT_SEGMENT_SIZE);
}
