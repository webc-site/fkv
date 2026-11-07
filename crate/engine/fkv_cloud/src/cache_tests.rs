//! cache_tests

use core::{
  pin::pin,
  task::{Context, Poll},
};

use fkv_error::KvError;
use fkv_io::{FileIo, PageIo};
use fkv_io_mem::MemStore;
use fkv_object::MemObjectStore;
use fkv_types::TableIdent;
use futures::task::noop_waker;

use super::{CloudStore, InflightDownload};
use crate::{
  download::DownloadOutcome, manifest::manifest_retryable, test_support::YieldWriteStore,
};

#[test]
fn manifest_retryable_extends_is_retryable_with_cloud_transient() {
  // 超集部分：云端瞬时错误可重试（`is_retryable` 判否，见 fkv_error tests）。
  // 共同子集 TryAgain / Busy / OpenFileLimit 由 fkv_error::retryable_matches_cpp 钉死。
  for err in [KvError::Timeout, KvError::CloudErr] {
    assert!(manifest_retryable(err), "{err:?}");
  }
  // 空间不足 / 数据损坏 / 不存在不可重试。
  for err in [
    KvError::OssInsufficientStorage,
    KvError::OutOfSpace,
    KvError::Corrupted,
    KvError::NotFound,
  ] {
    assert!(!manifest_retryable(err), "{err:?}");
  }
}

/// 第 4 项：连续三轮都只是当 waiter、期间一次回源都没做时必须返回
/// `TryAgain`，不能报成功（否则写路径中段留零洞）。
///
/// 回退即失败：循环结束直接 `Ok(())` 时写路径会继续落盘。
#[test]
fn ensure_resident_waiter_rounds_exhausted_returns_try_again() {
  let tbl = TableIdent::new("waiter", 1).unwrap();
  let cloud = CloudStore::new(
    MemStore::new(512),
    MemObjectStore::new(),
    "fkv",
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  let key = (tbl.clone(), false, 0);
  let waker = noop_waker();
  let mut cx = Context::from_waker(&waker);
  let mut write = pin!(cloud.write_page(&tbl, 0, &[1u8; 512]));
  for round in 0..3 {
    // 手动扮演一个「登记了但一次回源都没做」的 owner：本轮只能当 waiter。
    cloud.inflight_download.borrow_mut().insert(
      key.clone(),
      InflightDownload {
        generation: cloud.next_download_generation(),
        waiter_li: Vec::new(),
      },
    );
    assert!(
      write.as_mut().poll(&mut cx).is_pending(),
      "第 {round} 轮应挂起等待 owner"
    );
    let waiter_li = cloud
      .inflight_download
      .borrow_mut()
      .remove(&key)
      .expect("在飞登记")
      .waiter_li;
    assert_eq!(waiter_li.len(), 1, "第 {round} 轮应挂上一个等待者");
    for waiter in waiter_li {
      waiter.send(Ok(DownloadOutcome::Cached)).unwrap();
    }
  }
  match write.as_mut().poll(&mut cx) {
    Poll::Ready(Err(KvError::TryAgain)) => {}
    other => panic!("三轮全当 waiter 必须返回 TryAgain，实际 {other:?}"),
  }
  assert_eq!(cloud.resident_file_count(), 0);
}

/// 第 6 项：`write_file` 的本地写入 await 期间表被驱逐，不得再把结果登记
/// 回已驱逐的表。
///
/// 回退即失败：旧实现会在写完后无条件 `upsert_resident`，驱逐后的表重新
/// 出现在 resident 记账里（reopen 会把本地残留当权威）。
#[test]
fn write_file_after_evict_is_not_registered() {
  let tbl = TableIdent::new("epoch", 1).unwrap();
  let cloud = CloudStore::new(
    YieldWriteStore::new(),
    MemObjectStore::new(),
    "fkv",
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  );
  let waker = noop_waker();
  let mut cx = Context::from_waker(&waker);
  let mut write = pin!(cloud.write_file(&tbl, 0, &[1u8; 512]));
  assert!(write.as_mut().poll(&mut cx).is_pending(), "应停在本地写入");
  cloud.evict_table(&tbl);
  match write.as_mut().poll(&mut cx) {
    Poll::Ready(Err(KvError::TryAgain)) => {}
    other => panic!("已驱逐的表不得重新登记，应返回 TryAgain，实际 {other:?}"),
  }
  assert_eq!(cloud.resident_file_count(), 0);
}

/// 第 3 项：本地有前缀页、远端对象缺失时，`ensure_resident` 必须先取远端；
/// 远端 `Missing` 时保留本地前缀页，不能提前删除（否则调用方从 `start`
/// 落盘会在中段留零洞）。
///
/// 回退即失败：旧实现先删本地再回源，远端缺失时本地文件已被丢掉。
#[test]
fn ensure_resident_keeps_local_prefix_when_remote_missing() {
  pollster::block_on(async {
    let tbl = TableIdent::new("prefix", 1).unwrap();
    let cloud = CloudStore::new(
      MemStore::new(512),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    cloud.local.write_file(&tbl, 0, &[1u8; 512]).await.unwrap();
    // 起点 4096 远大于本地 512 字节前缀，且远端没有该对象。
    cloud.ensure_resident(&tbl, 0, 4096).await.unwrap();
    assert_eq!(
      cloud.local.file_size(&tbl, 0).await.unwrap(),
      Some(512),
      "远端缺失时本地前缀页必须保留"
    );
  });
}
